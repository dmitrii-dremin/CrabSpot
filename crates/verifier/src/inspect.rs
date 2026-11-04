use crate::{
    Body, Checked, CodeValidationErrorKind as E, CodeValidationLocation as L,
    CodeValidationOptions, CodeValidationResource as R, CodeValidationUsage,
    scan::{self, Name, Role, Summary},
};
use crabspot_bytecode::{DecodedCode, Instruction, Operands, decode};
use crabspot_classfile::{Attribute, AttributeBody, ClassFile, Code, Constant, Member};
use std::collections::HashMap;

pub(crate) struct Context<'a> {
    pub class: &'a ClassFile,
    pub method_index: usize,
    options: CodeValidationOptions,
    pub usage: CodeValidationUsage,
    pub location: L,
    cache: HashMap<(u16, Role), Summary>,
}
impl<'a> Context<'a> {
    pub fn new(class: &'a ClassFile, method_index: usize, options: CodeValidationOptions) -> Self {
        Self {
            class,
            method_index,
            options,
            usage: CodeValidationUsage::default(),
            location: L::Method,
            cache: HashMap::new(),
        }
    }
    pub fn amount(value: usize, resource: R) -> Checked<u64> {
        u64::try_from(value).map_err(|_| E::AccountingOverflow { resource })
    }
    pub fn charge(&mut self, code: u64, metadata: u64, utf16: u64, work: u64) -> Checked<()> {
        let additions = [
            (
                self.usage.code_bytes,
                code,
                self.options.max_code_bytes,
                R::CodeBytes,
            ),
            (
                self.usage.metadata_entries,
                metadata,
                self.options.max_metadata_entries,
                R::MetadataEntries,
            ),
            (
                self.usage.utf16_units,
                utf16,
                self.options.max_utf16_units,
                R::Utf16Units,
            ),
            (
                self.usage.work_units,
                work,
                self.options.max_work_units,
                R::WorkUnits,
            ),
        ];
        let mut totals = [0; 4];
        for (target, (current, amount, limit, resource)) in totals.iter_mut().zip(additions) {
            *target = current
                .checked_add(amount)
                .ok_or(E::AccountingOverflow { resource })?;
            if *target > limit {
                return Err(E::ResourceLimit {
                    resource,
                    limit,
                    required: *target,
                });
            }
        }
        self.usage = CodeValidationUsage {
            code_bytes: totals[0],
            metadata_entries: totals[1],
            utf16_units: totals[2],
            work_units: totals[3],
        };
        Ok(())
    }
    pub fn step(&mut self) -> Checked<()> {
        self.charge(0, 0, 0, 1)
    }
    pub fn metadata(&mut self, length: usize) -> Checked<()> {
        let count = Self::amount(length, R::MetadataEntries)?;
        self.charge(0, count, 0, count)
    }
    pub fn count(length: usize) -> Checked<()> {
        if length > usize::from(u16::MAX) {
            Err(E::InvalidMetadata(
                "vector count exceeds u16 representation",
            ))
        } else {
            Ok(())
        }
    }
    pub fn cp(&mut self, index: u16) -> Checked<&'a Constant> {
        self.step()?;
        self.class
            .constant_pool
            .get(index)
            .map_err(|_| E::InvalidConstant {
                index,
                expected: "usable constant",
            })
    }
    pub fn utf(
        &mut self,
        index: u16,
        role: Role,
        locals: Option<&mut Vec<u8>>,
    ) -> Checked<Summary> {
        let Constant::Utf8(value) = self.cp(index)? else {
            return Err(E::InvalidConstant {
                index,
                expected: "Utf8",
            });
        };
        if let Some(summary) = self.cache.get(&(index, role)) {
            return Ok(*summary);
        }
        let length = Self::amount(value.units().len(), R::Utf16Units)?;
        self.charge(0, 0, length, length)?;
        let summary = scan::inspect(value.units(), index, role, locals)?;
        self.reserve_cache(1)?;
        self.cache.insert((index, role), summary);
        Ok(summary)
    }
    pub(super) fn reserve_cache(&mut self, additional: usize) -> Checked<()> {
        self.cache
            .try_reserve(additional)
            .map_err(|_| E::AllocationFailure {
                resource: "UTF-16 summary cache",
            })
    }
    pub fn class_name(&mut self, index: u16) -> Checked<Summary> {
        let Constant::Class { name_index } = self.cp(index)? else {
            return Err(E::InvalidConstant {
                index,
                expected: "Class",
            });
        };
        self.utf(*name_index, Role::Class, None)
    }
    fn nat(&mut self, index: u16) -> Checked<(u16, u16)> {
        match self.cp(index)? {
            Constant::NameAndType {
                name_index,
                descriptor_index,
            } => Ok((*name_index, *descriptor_index)),
            _ => Err(E::InvalidConstant {
                index,
                expected: "NameAndType",
            }),
        }
    }
    fn field_nat(&mut self, index: u16) -> Checked<Summary> {
        let (name, descriptor) = self.nat(index)?;
        self.utf(name, Role::FieldName, None)?;
        self.utf(descriptor, Role::Field, None)
    }
    fn method_nat(&mut self, index: u16) -> Checked<(Name, Summary)> {
        let (name, descriptor) = self.nat(index)?;
        let name = self.utf(name, Role::MethodName, None)?.name;
        let descriptor = self.utf(descriptor, Role::Method, None)?;
        Ok((name, descriptor))
    }
    fn field_ref(&mut self, index: u16) -> Checked<Summary> {
        let Constant::Fieldref {
            class_index,
            name_and_type_index,
        } = self.cp(index)?
        else {
            return Err(E::InvalidConstant {
                index,
                expected: "Fieldref",
            });
        };
        self.class_name(*class_index)?;
        self.field_nat(*name_and_type_index)
    }
    // The caller's tag/version policy is checked before following any link.
    fn method_ref(&mut self, index: u16, tag: u8) -> Checked<(Name, Summary, bool)> {
        let (owner, nat, interface) = match self.cp(index)? {
            Constant::Methodref {
                class_index,
                name_and_type_index,
            } if tag != 11 => (*class_index, *name_and_type_index, false),
            Constant::InterfaceMethodref {
                class_index,
                name_and_type_index,
            } if tag != 10 => (*class_index, *name_and_type_index, true),
            _ => {
                return Err(E::InvalidConstant {
                    index,
                    expected: if tag == 10 {
                        "Methodref"
                    } else if tag == 11 {
                        "InterfaceMethodref"
                    } else {
                        "method reference"
                    },
                });
            }
        };
        self.class_name(owner)?;
        let (name, descriptor) = self.method_nat(nat)?;
        Ok((name, descriptor, interface))
    }
    fn capacity(summary: Summary, receiver: bool) -> Checked<()> {
        if summary
            .slots
            .checked_add(u16::from(receiver))
            .is_none_or(|slots| slots > 255)
        {
            Err(E::InvalidOperand(
                "parameter slots including receiver exceed 255",
            ))
        } else {
            Ok(())
        }
    }
    fn loadable(&mut self, index: u16, wide: bool) -> Checked<()> {
        let version = self.class.version.major;
        let constant = self.cp(index)?;
        // For fixed-category tags the opcode category is known before any
        // linked Utf8/member lookup. Dynamic alone needs its descriptor first.
        if (wide
            && !matches!(
                constant,
                Constant::Long(_) | Constant::Double(_) | Constant::Dynamic { .. }
            ))
            || (!wide && matches!(constant, Constant::Long(_) | Constant::Double(_)))
        {
            return Err(E::InvalidConstant {
                index,
                expected: if wide {
                    "category-two loadable constant"
                } else {
                    "category-one loadable constant"
                },
            });
        }
        let width = match constant {
            Constant::Integer(_) | Constant::Float(_) => 1,
            Constant::Long(_) | Constant::Double(_) => {
                self.step()?;
                let next = usize::from(index)
                    .checked_add(1)
                    .ok_or(E::InvalidConstant {
                        index,
                        expected: "wide continuation hole",
                    })?;
                if !matches!(
                    self.class.constant_pool.entries().get(next),
                    Some(Constant::Unusable)
                ) {
                    return Err(E::InvalidConstant {
                        index,
                        expected: "wide continuation hole",
                    });
                }
                2
            }
            Constant::String { string_index } => {
                if !matches!(self.cp(*string_index)?, Constant::Utf8(_)) {
                    return Err(E::InvalidConstant {
                        index: *string_index,
                        expected: "Utf8 string literal",
                    });
                }
                1
            }
            Constant::Class { name_index } if version >= 49 => {
                self.utf(*name_index, Role::Class, None)?;
                1
            }
            Constant::MethodType { descriptor_index } if version >= 51 => {
                self.utf(*descriptor_index, Role::Method, None)?;
                1
            }
            Constant::MethodHandle {
                reference_kind,
                reference_index,
            } if version >= 51 => {
                let kind = *reference_kind;
                let target = *reference_index;
                match kind {
                    1..=4 => {
                        self.field_ref(target)?;
                    }
                    5..=9 => {
                        let tag = match kind {
                            5 | 8 => 10,
                            6 | 7 if version >= 52 => 0,
                            6 | 7 => 10,
                            9 => 11,
                            _ => 0,
                        };
                        let (name, summary, _) = self.method_ref(target, tag)?;
                        if (kind == 8 && (name != Name::Init || !summary.void))
                            || (kind != 8 && name != Name::Ordinary)
                        {
                            return Err(E::InvalidOperand(
                                "method-handle initializer name/kind mismatch",
                            ));
                        }
                        Self::capacity(summary, kind != 6)?;
                    }
                    _ => return Err(E::InvalidOperand("invalid method-handle reference kind")),
                }
                1
            }
            Constant::Dynamic {
                name_and_type_index,
                ..
            } if version >= 55 => self.field_nat(*name_and_type_index)?.width,
            _ => {
                return Err(E::InvalidConstant {
                    index,
                    expected: "version-eligible loadable constant",
                });
            }
        };
        if (width == 2) != wide {
            return Err(E::InvalidOperand(
                "ldc constant category does not match opcode",
            ));
        }
        Ok(())
    }
    fn invocation(&mut self, index: u16, opcode: u8, count: Option<u8>) -> Checked<()> {
        if opcode == 0xba {
            let Constant::InvokeDynamic {
                name_and_type_index,
                ..
            } = self.cp(index)?
            else {
                return Err(E::InvalidConstant {
                    index,
                    expected: "InvokeDynamic",
                });
            };
            let (name, summary) = self.method_nat(*name_and_type_index)?;
            if name != Name::Ordinary {
                return Err(E::InvalidOperand(
                    "invokedynamic cannot name an initializer",
                ));
            }
            return Self::capacity(summary, false);
        }
        let tag = if opcode == 0xb9 {
            11
        } else if matches!(opcode, 0xb7 | 0xb8) && self.class.version.major >= 52 {
            0
        } else {
            10
        };
        let (name, summary, interface) = self.method_ref(index, tag)?;
        if name == Name::Clinit
            || (name == Name::Init && (opcode != 0xb7 || interface || !summary.void))
        {
            return Err(E::InvalidOperand(
                "invocation initializer name, tag or result is invalid",
            ));
        }
        Self::capacity(summary, opcode != 0xb8)?;
        if let Some(count) = count {
            if u16::from(count) != summary.slots + 1 {
                return Err(E::InvalidOperand(
                    "invokeinterface count differs from receiver plus parameter slots",
                ));
            }
        }
        Ok(())
    }
    fn local(index: u16, width: u8, max_locals: u16) -> Checked<()> {
        if index
            .checked_add(u16::from(width))
            .is_none_or(|end| end > max_locals)
        {
            Err(E::LocalOutOfBounds {
                index,
                width,
                max_locals,
            })
        } else {
            Ok(())
        }
    }
    fn instruction(&mut self, instruction: &Instruction, code: &Code) -> Checked<()> {
        self.location = L::Instruction(instruction.offset);
        self.step()?;
        let opcode = instruction.opcode;
        let width = if matches!(opcode, 0x16 | 0x18 | 0x37 | 0x39) {
            2
        } else {
            1
        };
        match &instruction.operands {
            Operands::Local(index) | Operands::Iinc { index, .. } => {
                Self::local(*index, width, code.max_locals)?
            }
            Operands::Constant(index) => match opcode {
                0x12..=0x14 => self.loadable(*index, opcode == 0x14)?,
                0xb2..=0xb5 => {
                    self.field_ref(*index)?;
                }
                0xb6..=0xb8 => self.invocation(*index, opcode, None)?,
                0xbb | 0xbd | 0xc0 | 0xc1 => {
                    let name = self.class_name(*index)?;
                    if (opcode == 0xbb && name.rank > 0) || (opcode == 0xbd && name.rank == 255) {
                        return Err(E::InvalidOperand("invalid allocation class/array rank"));
                    }
                }
                _ => {
                    return Err(E::InvalidMetadata(
                        "decoded constant operand has unexpected opcode",
                    ));
                }
            },
            Operands::InvokeInterface { index, count } => {
                self.invocation(*index, opcode, Some(*count))?
            }
            Operands::InvokeDynamic { index } => self.invocation(*index, opcode, None)?,
            Operands::MultiANewArray { index, dimensions } => {
                let summary = self.class_name(*index)?;
                if *dimensions == 0 || *dimensions > summary.rank {
                    return Err(E::InvalidOperand(
                        "multianewarray dimensions exceed array rank",
                    ));
                }
            }
            _ => {}
        }
        if (0x1a..=0x2d).contains(&opcode) {
            let group = (opcode - 0x1a) / 4;
            Self::local(
                u16::from((opcode - 0x1a) % 4),
                if matches!(group, 1 | 3) { 2 } else { 1 },
                code.max_locals,
            )?;
        }
        if (0x3b..=0x4e).contains(&opcode) {
            let group = (opcode - 0x3b) / 4;
            Self::local(
                u16::from((opcode - 0x3b) % 4),
                if matches!(group, 1 | 3) { 2 } else { 1 },
                code.max_locals,
            )?;
        }
        Ok(())
    }
    pub fn boundary<'b>(
        &mut self,
        decoded: &'b DecodedCode,
        offset: u32,
    ) -> Checked<&'b Instruction> {
        self.step()?;
        decoded.instruction_at(offset).ok_or(E::InvalidBoundary {
            offset,
            expected: "instruction start",
        })
    }
    pub fn attribute(&mut self, attribute: &Attribute, method: bool) -> Checked<Name> {
        self.metadata(1)?;
        let name = self.utf(attribute.name_index, Role::Attribute, None)?.name;
        match &attribute.body {
            AttributeBody::Code(_) if !method || name != Name::Code => {
                return Err(E::InvalidMetadata(
                    "typed Code body has wrong name/location",
                ));
            }
            AttributeBody::StackMapTable(_)
                if method || self.class.version.major < 50 || name != Name::StackMap =>
            {
                return Err(E::InvalidMetadata(
                    "typed StackMapTable body has wrong name/location/version",
                ));
            }
            _ => {}
        }
        if method && name == Name::Code && !matches!(attribute.body, AttributeBody::Code(_)) {
            return Err(E::InvalidMetadata(
                "recognized Code name requires typed Code body",
            ));
        }
        if !method
            && self.class.version.major >= 50
            && name == Name::StackMap
            && !matches!(attribute.body, AttributeBody::StackMapTable(_))
        {
            return Err(E::InvalidMetadata(
                "recognized StackMapTable name requires typed body",
            ));
        }
        Ok(name)
    }
    pub fn run(&mut self) -> Checked<(&'a Member, Body<'a>)> {
        let version = self.class.version;
        if !(45..=65).contains(&version.major)
            || (version.major >= 56
                && version.minor != 0
                && !(version.major == 65 && version.minor == u16::MAX))
        {
            return Err(E::UnsupportedVersion {
                major: version.major,
                minor: version.minor,
            });
        }
        if version.major == 65 && version.minor == u16::MAX && !self.options.allow_preview {
            return Err(E::PreviewDisabled);
        }
        let method =
            self.class
                .methods
                .get(self.method_index)
                .ok_or(E::MethodIndexOutOfBounds {
                    index: self.method_index,
                    count: self.class.methods.len(),
                })?;
        let name = self.utf(method.name_index, Role::MethodName, None)?.name;
        let clinit = name == Name::Clinit;
        let constructor = name == Name::Init;
        let receiver = !clinit && method.access_flags & 8 == 0;
        let mut locals = Vec::new();
        if receiver {
            scan::reserve(&mut locals, 1, "initial local widths")?;
            locals.push(1);
        }
        let signature = self.utf(method.descriptor_index, Role::Method, Some(&mut locals))?;
        if signature.slots + u16::from(receiver) > 255 {
            return Err(E::InvalidDescriptor {
                index: method.descriptor_index,
            });
        }
        if constructor
            && (!signature.void
                || method.access_flags & (8 | 0x100 | 0x400) != 0
                || self.class.access_flags & 0x200 != 0)
        {
            return Err(E::InvalidMetadata(
                "constructor must be class-owned, instance, concrete and void",
            ));
        }
        if clinit {
            if !signature.void {
                return Err(E::InvalidMetadata("class initializer must return void"));
            }
            if signature.slots != 0 {
                return Err(if version.major < 51 {
                    E::UnsupportedHistoricalInitializer
                } else {
                    E::InvalidMetadata("modern class initializer must be ()V")
                });
            }
            if version.major >= 51 && method.access_flags & 8 == 0 {
                return Err(E::InvalidMetadata(
                    "modern class initializer must be static",
                ));
            }
        }
        Self::count(method.attributes.len())?;
        let mut selected = None;
        for (index, attribute) in method.attributes.iter().enumerate() {
            self.location = L::MethodAttribute(index);
            self.attribute(attribute, true)?;
            if let AttributeBody::Code(code) = &attribute.body {
                if selected.replace((index, code)).is_some() {
                    return Err(E::InvalidMetadata("multiple Code bodies"));
                }
            }
        }
        let bodyless = !clinit && method.access_flags & (0x100 | 0x400) != 0;
        let Some((code_index, code)) = selected else {
            if bodyless {
                return Ok((method, Body::Absent));
            }
            return Err(E::InvalidMetadata("concrete method has no Code"));
        };
        if bodyless {
            return Err(E::InvalidMetadata("abstract/native method has Code"));
        }
        self.location = L::MethodAttribute(code_index);
        if code.code.is_empty() || code.code.len() > 65_535 {
            return Err(E::InvalidMetadata("Code length must be 1..65535"));
        }
        if signature.slots + u16::from(receiver) > code.max_locals {
            return Err(E::InvalidMetadata(
                "parameters including receiver exceed max_locals",
            ));
        }
        let length = Self::amount(code.code.len(), R::CodeBytes)?;
        self.charge(length, 0, 0, length)?;
        let decoded = decode(&code.code, version.major).map_err(|error| {
            self.location = L::Instruction(error.offset);
            E::Decode(error)
        })?;
        for instruction in decoded.instructions() {
            self.instruction(instruction, code)?;
        }
        self.location = L::ExceptionHandler(0);
        Self::count(code.exception_table.len())?;
        self.metadata(code.exception_table.len())?;
        for (index, handler) in code.exception_table.iter().enumerate() {
            self.location = L::ExceptionHandler(index);
            if handler.start_pc >= handler.end_pc {
                return Err(E::InvalidMetadata("handler start must precede end"));
            }
            self.boundary(&decoded, u32::from(handler.start_pc))?;
            // End-of-Code still pays for its boundary query.
            self.step()?;
            if u32::from(handler.end_pc) != decoded.code_len()
                && decoded.instruction_at(u32::from(handler.end_pc)).is_none()
            {
                return Err(E::InvalidBoundary {
                    offset: u32::from(handler.end_pc),
                    expected: "instruction start or Code end",
                });
            }
            self.boundary(&decoded, u32::from(handler.handler_pc))?;
            if handler.catch_type != 0 && self.class_name(handler.catch_type)?.rank != 0 {
                return Err(E::InvalidOperand("catch type must be a nonarray Class"));
            }
        }
        self.location = L::CodeAttribute(0);
        Self::count(code.attributes.len())?;
        let mut stack_map = false;
        for (index, attribute) in code.attributes.iter().enumerate() {
            self.location = L::CodeAttribute(index);
            self.attribute(attribute, false)?;
            if let AttributeBody::StackMapTable(frames) = &attribute.body {
                if stack_map {
                    return Err(E::InvalidMetadata("multiple StackMapTable attributes"));
                }
                stack_map = true;
                self.stack_maps(
                    &decoded,
                    code,
                    index,
                    frames,
                    &mut locals,
                    signature.slots + u16::from(receiver),
                    constructor,
                )?;
            }
        }
        Ok((method, Body::Present { code, decoded }))
    }
}

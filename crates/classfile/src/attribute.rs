use crate::{
    Annotation, Constant, ConstantPool, ElementValue, ErrorKind, Module, ParseError,
    TypeAnnotation,
    annotation::{self, AnnotationBudget},
    descriptor,
    reader::Reader,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub offset: usize,
    pub name_index: u16,
    pub body: AttributeBody,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttributeBody {
    ConstantValue(u16),
    Code(Code),
    StackMapTable(Vec<StackMapFrame>),
    Exceptions(Vec<u16>),
    InnerClasses(Vec<InnerClass>),
    EnclosingMethod {
        class_index: u16,
        method_index: u16,
    },
    Synthetic,
    Signature(u16),
    SourceFile(u16),
    SourceDebugExtension(Vec<u8>),
    LineNumberTable(Vec<LineNumber>),
    LocalVariableTable(Vec<LocalVariable>),
    LocalVariableTypeTable(Vec<LocalVariable>),
    Deprecated,
    RuntimeVisibleAnnotations(Vec<Annotation>),
    RuntimeInvisibleAnnotations(Vec<Annotation>),
    RuntimeVisibleParameterAnnotations(Vec<Vec<Annotation>>),
    RuntimeInvisibleParameterAnnotations(Vec<Vec<Annotation>>),
    RuntimeVisibleTypeAnnotations(Vec<TypeAnnotation>),
    RuntimeInvisibleTypeAnnotations(Vec<TypeAnnotation>),
    AnnotationDefault(ElementValue),
    BootstrapMethods(Vec<BootstrapMethod>),
    MethodParameters(Vec<MethodParameter>),
    Module(Module),
    ModulePackages(Vec<u16>),
    ModuleMainClass(u16),
    NestHost(u16),
    NestMembers(Vec<u16>),
    Record(Vec<RecordComponent>),
    PermittedSubclasses(Vec<u16>),
    /// `standard` distinguishes unknown extensions from unimplemented JVMS attributes.
    /// No validation of the contents of opaque attributes has been performed.
    Opaque {
        bytes: Vec<u8>,
        standard: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Code {
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<u8>,
    pub exception_table: Vec<ExceptionHandler>,
    pub attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExceptionHandler {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapMethod {
    pub bootstrap_method_ref: u16,
    pub arguments: Vec<u16>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InnerClass {
    pub inner_class_info_index: u16,
    pub outer_class_info_index: u16,
    pub inner_name_index: u16,
    pub access_flags: u16,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineNumber {
    pub start_pc: u16,
    pub line_number: u16,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalVariable {
    pub start_pc: u16,
    pub length: u16,
    pub name_index: u16,
    pub descriptor_or_signature_index: u16,
    pub index: u16,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodParameter {
    pub name_index: u16,
    pub access_flags: u16,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecordComponent {
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationType {
    Top,
    Integer,
    Float,
    Double,
    Long,
    Null,
    UninitializedThis,
    Object(u16),
    Uninitialized(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackMapFrame {
    pub offset_delta: u16,
    pub bytecode_offset: u32,
    pub kind: StackMapFrameKind,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StackMapFrameKind {
    Same,
    SameLocalsOneStack(VerificationType),
    Chop(u8),
    Append(Vec<VerificationType>),
    Full {
        locals: Vec<VerificationType>,
        stack: Vec<VerificationType>,
    },
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Location {
    Class,
    Field,
    Method,
    Code,
    RecordComponent,
}

#[derive(Clone, Copy)]
pub(crate) struct CodeContext {
    pub length: usize,
    pub max_locals: u16,
    pub exception_count: usize,
}

fn class_index(pool: &ConstantPool, index: u16) -> Result<(), ParseError> {
    pool.class_name(index).map(|_| ())
}
fn indices(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    expected: &'static str,
    predicate: impl Fn(&Constant) -> bool,
) -> Result<Vec<u16>, ParseError> {
    let count = reader.u2()?;
    let mut result = Vec::new();
    for _ in 0..count {
        let index = reader.u2()?;
        pool.require(index, expected, &predicate)?;
        result.push(index);
    }
    Ok(result)
}

fn verification(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    code: CodeContext,
) -> Result<VerificationType, ParseError> {
    Ok(match reader.u1()? {
        0 => VerificationType::Top,
        1 => VerificationType::Integer,
        2 => VerificationType::Float,
        3 => VerificationType::Double,
        4 => VerificationType::Long,
        5 => VerificationType::Null,
        6 => VerificationType::UninitializedThis,
        7 => {
            let index = reader.u2()?;
            class_index(pool, index)?;
            VerificationType::Object(index)
        }
        8 => {
            let offset = reader.u2()?;
            if usize::from(offset) >= code.length {
                return Err(reader.invalid("uninitialized variable offset is outside Code"));
            }
            VerificationType::Uninitialized(offset)
        }
        _ => return Err(reader.invalid("invalid verification_type_info tag")),
    })
}

fn stack_map(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    code: CodeContext,
) -> Result<Vec<StackMapFrame>, ParseError> {
    let count = reader.u2()?;
    let mut result = Vec::new();
    let mut previous: Option<u32> = None;
    for _ in 0..count {
        let tag = reader.u1()?;
        let (offset_delta, kind) = match tag {
            0..=63 => (u16::from(tag), StackMapFrameKind::Same),
            64..=127 => (
                u16::from(tag - 64),
                StackMapFrameKind::SameLocalsOneStack(verification(reader, pool, code)?),
            ),
            247 => (
                reader.u2()?,
                StackMapFrameKind::SameLocalsOneStack(verification(reader, pool, code)?),
            ),
            248..=250 => (reader.u2()?, StackMapFrameKind::Chop(251 - tag)),
            251 => (reader.u2()?, StackMapFrameKind::Same),
            252..=254 => {
                let offset = reader.u2()?;
                let mut locals = Vec::new();
                for _ in 0..(tag - 251) {
                    locals.push(verification(reader, pool, code)?);
                }
                (offset, StackMapFrameKind::Append(locals))
            }
            255 => {
                let offset = reader.u2()?;
                let mut locals = Vec::new();
                for _ in 0..reader.u2()? {
                    locals.push(verification(reader, pool, code)?);
                }
                let mut stack = Vec::new();
                for _ in 0..reader.u2()? {
                    stack.push(verification(reader, pool, code)?);
                }
                (offset, StackMapFrameKind::Full { locals, stack })
            }
            _ => return Err(reader.invalid("reserved StackMapTable frame type")),
        };
        let bytecode_offset = previous.map_or(u32::from(offset_delta), |previous| {
            previous + u32::from(offset_delta) + 1
        });
        if bytecode_offset >= code.length as u32 {
            return Err(reader.invalid("stack-map frame offset is outside Code"));
        }
        previous = Some(bytecode_offset);
        result.push(StackMapFrame {
            offset_delta,
            bytecode_offset,
            kind,
        });
    }
    Ok(result)
}

fn standard_info(name: &str) -> Option<(u16, &'static [Location], bool)> {
    use Location::*;
    // Version, allowed locations, at-most-one rule (JVMS 4.7).
    Some(match name {
        "ConstantValue" => (45, &[Field], true),
        "Code" => (45, &[Method], true),
        "StackMapTable" => (50, &[Code], true),
        "Exceptions" => (45, &[Method], true),
        "InnerClasses" | "SourceFile" => (45, &[Class], true),
        "Synthetic" | "Deprecated" => (45, &[Class, Field, Method], false),
        "Signature" => (49, &[Class, Field, Method, RecordComponent], true),
        "EnclosingMethod" | "SourceDebugExtension" => (49, &[Class], true),
        "LineNumberTable" | "LocalVariableTable" => (45, &[Code], false),
        "LocalVariableTypeTable" => (49, &[Code], false),
        "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
            (49, &[Class, Field, Method, RecordComponent], true)
        }
        "RuntimeVisibleParameterAnnotations"
        | "RuntimeInvisibleParameterAnnotations"
        | "AnnotationDefault" => (49, &[Method], true),
        "RuntimeVisibleTypeAnnotations" | "RuntimeInvisibleTypeAnnotations" => {
            (52, &[Class, Field, Method, Code, RecordComponent], true)
        }
        "BootstrapMethods" => (51, &[Class], true),
        "MethodParameters" => (52, &[Method], true),
        "Module" | "ModulePackages" | "ModuleMainClass" => (53, &[Class], true),
        "NestHost" | "NestMembers" => (55, &[Class], true),
        "Record" => (60, &[Class], true),
        "PermittedSubclasses" => (61, &[Class], true),
        _ => return None,
    })
}

pub(crate) fn parse_attributes(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    major: u16,
    location: Location,
    annotation_budget: &mut AnnotationBudget,
) -> Result<Vec<Attribute>, ParseError> {
    attributes(reader, pool, major, location, None, annotation_budget)
}

fn attributes(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    major: u16,
    location: Location,
    code: Option<CodeContext>,
    annotation_budget: &mut AnnotationBudget,
) -> Result<Vec<Attribute>, ParseError> {
    let count = reader.u2()?;
    let mut result: Vec<Attribute> = Vec::new();
    let mut singleton_names = std::collections::BTreeSet::new();
    for _ in 0..count {
        let offset = reader.offset();
        let name_index = reader.u2()?;
        let java_name = pool.utf8(name_index)?;
        let name = java_name.to_string_lossy();
        // Predefined names are reserved only in the versions and locations
        // where JVMS 4.7 defines them. Elsewhere they are extension attributes.
        let info = standard_info(&name)
            .filter(|(introduced, allowed, _)| major >= *introduced && allowed.contains(&location));
        if let Some((_, _, singleton)) = info {
            if singleton && !singleton_names.insert(name.clone()) {
                return Err(reader.invalid("duplicate standard attribute"));
            }
        }
        let length = reader.u4()? as usize;
        let start = reader.offset();
        let bytes = reader.take(length)?;
        let mut body_reader = Reader::new(bytes, start, "attribute body");
        let r = &mut body_reader;
        let body = if info.is_none() {
            AttributeBody::Opaque {
                bytes: r.take(r.remaining())?.to_vec(),
                standard: false,
            }
        } else {
            match name.as_str() {
                "ConstantValue" => {
                    let index = r.u2()?;
                    pool.require(
                        index,
                        "Integer, Float, Long, Double or String",
                        |constant| {
                            matches!(
                                constant,
                                Constant::Integer(_)
                                    | Constant::Float(_)
                                    | Constant::Long(_)
                                    | Constant::Double(_)
                                    | Constant::String { .. }
                            )
                        },
                    )?;
                    AttributeBody::ConstantValue(index)
                }
                "Code" => AttributeBody::Code(parse_code(r, pool, major, annotation_budget)?),
                "RuntimeVisibleAnnotations" => AttributeBody::RuntimeVisibleAnnotations(
                    annotation::annotations(r, pool, annotation_budget)?,
                ),
                "RuntimeInvisibleAnnotations" => AttributeBody::RuntimeInvisibleAnnotations(
                    annotation::annotations(r, pool, annotation_budget)?,
                ),
                "RuntimeVisibleParameterAnnotations" => {
                    AttributeBody::RuntimeVisibleParameterAnnotations(
                        annotation::parameter_annotations(r, pool, annotation_budget)?,
                    )
                }
                "RuntimeInvisibleParameterAnnotations" => {
                    AttributeBody::RuntimeInvisibleParameterAnnotations(
                        annotation::parameter_annotations(r, pool, annotation_budget)?,
                    )
                }
                "RuntimeVisibleTypeAnnotations" => AttributeBody::RuntimeVisibleTypeAnnotations(
                    annotation::type_annotations(r, pool, location, code, annotation_budget)?,
                ),
                "RuntimeInvisibleTypeAnnotations" => {
                    AttributeBody::RuntimeInvisibleTypeAnnotations(annotation::type_annotations(
                        r,
                        pool,
                        location,
                        code,
                        annotation_budget,
                    )?)
                }
                "AnnotationDefault" => AttributeBody::AnnotationDefault(annotation::element_value(
                    r,
                    pool,
                    annotation_budget,
                    1,
                )?),
                "Module" => AttributeBody::Module(crate::module::parse_module(r, pool, major)?),
                "StackMapTable" => AttributeBody::StackMapTable(stack_map(
                    r,
                    pool,
                    code.ok_or_else(|| r.invalid("StackMapTable requires Code context"))?,
                )?),
                "Exceptions" => AttributeBody::Exceptions(indices(r, pool, "Class", |c| {
                    matches!(c, Constant::Class { .. })
                })?),
                "InnerClasses" => {
                    let mut classes = Vec::new();
                    let mut inner_names = std::collections::BTreeSet::new();
                    for _ in 0..r.u2()? {
                        let inner_class_info_index = r.u2()?;
                        let outer_class_info_index = r.u2()?;
                        let inner_name_index = r.u2()?;
                        let access_flags = r.u2()?;
                        class_index(pool, inner_class_info_index)?;
                        if !inner_names.insert(pool.class_name(inner_class_info_index)?.units()) {
                            return Err(
                                r.invalid("duplicate InnerClasses entry for the same class")
                            );
                        }
                        if outer_class_info_index != 0 {
                            class_index(pool, outer_class_info_index)?;
                            if pool.class_name(inner_class_info_index)?
                                == pool.class_name(outer_class_info_index)?
                            {
                                return Err(r.invalid(
                                    "an InnerClasses entry cannot be its own outer class",
                                ));
                            }
                        }
                        if inner_name_index != 0 {
                            if !descriptor::valid_unqualified_name(
                                pool.utf8(inner_name_index)?.units(),
                                false,
                            ) {
                                return Err(r.invalid("invalid inner class name"));
                            }
                        } else if major >= 51 && outer_class_info_index != 0 {
                            return Err(
                                r.invalid("anonymous classes cannot have an outer class entry")
                            );
                        }
                        classes.push(InnerClass {
                            inner_class_info_index,
                            outer_class_info_index,
                            inner_name_index,
                            access_flags,
                        });
                    }
                    AttributeBody::InnerClasses(classes)
                }
                "EnclosingMethod" => {
                    let class_index = r.u2()?;
                    let method_index = r.u2()?;
                    pool.class_name(class_index)?;
                    if method_index != 0 {
                        pool.name_and_type(method_index, true)?;
                    }
                    AttributeBody::EnclosingMethod {
                        class_index,
                        method_index,
                    }
                }
                "Synthetic" => AttributeBody::Synthetic,
                "Deprecated" => AttributeBody::Deprecated,
                "Signature" => {
                    let index = r.u2()?;
                    pool.utf8(index)?;
                    AttributeBody::Signature(index)
                }
                "SourceFile" => {
                    let index = r.u2()?;
                    pool.utf8(index)?;
                    AttributeBody::SourceFile(index)
                }
                "SourceDebugExtension" => {
                    let offset = r.offset();
                    let bytes = r.take(r.remaining())?;
                    crate::constant_pool::decode_modified_utf8(bytes, offset).map_err(
                        |mut error| {
                            error.context = "SourceDebugExtension";
                            error
                        },
                    )?;
                    AttributeBody::SourceDebugExtension(bytes.to_vec())
                }
                "LineNumberTable" => {
                    let code =
                        code.ok_or_else(|| r.invalid("LineNumberTable requires Code context"))?;
                    let mut entries = Vec::new();
                    for _ in 0..r.u2()? {
                        let start_pc = r.u2()?;
                        let line_number = r.u2()?;
                        if usize::from(start_pc) >= code.length {
                            return Err(r.invalid("line-number start_pc is outside Code"));
                        }
                        entries.push(LineNumber {
                            start_pc,
                            line_number,
                        });
                    }
                    AttributeBody::LineNumberTable(entries)
                }
                "LocalVariableTable" | "LocalVariableTypeTable" => {
                    let code = code
                        .ok_or_else(|| r.invalid("local-variable table requires Code context"))?;
                    let mut entries = Vec::new();
                    for _ in 0..r.u2()? {
                        let start_pc = r.u2()?;
                        let length = r.u2()?;
                        let name_index = r.u2()?;
                        let descriptor_or_signature_index = r.u2()?;
                        let index = r.u2()?;
                        if !descriptor::valid_unqualified_name(
                            pool.utf8(name_index)?.units(),
                            false,
                        ) {
                            return Err(r.invalid("invalid local variable name"));
                        }
                        let desc = pool.utf8(descriptor_or_signature_index)?;
                        let slots = if name == "LocalVariableTable" {
                            descriptor::parse_field_descriptor(desc.units())
                                .map_err(|_| r.error(ErrorKind::InvalidDescriptor))?
                                .slots()
                        } else {
                            1
                        };
                        if usize::from(start_pc) >= code.length
                            || usize::from(start_pc) + usize::from(length) > code.length
                        {
                            return Err(r.invalid("local-variable range is outside Code"));
                        }
                        if u32::from(index) + u32::from(slots) > u32::from(code.max_locals) {
                            return Err(r.invalid("local-variable slot is outside max_locals"));
                        }
                        entries.push(LocalVariable {
                            start_pc,
                            length,
                            name_index,
                            descriptor_or_signature_index,
                            index,
                        });
                    }
                    if name == "LocalVariableTable" {
                        AttributeBody::LocalVariableTable(entries)
                    } else {
                        AttributeBody::LocalVariableTypeTable(entries)
                    }
                }
                "BootstrapMethods" => {
                    let mut methods = Vec::new();
                    for _ in 0..r.u2()? {
                        let bootstrap_method_ref = r.u2()?;
                        pool.require(bootstrap_method_ref, "MethodHandle", |c| {
                            matches!(c, Constant::MethodHandle { .. })
                        })?;
                        let arguments = indices(r, pool, "a loadable constant", |c| {
                            matches!(
                                c,
                                Constant::Integer(_)
                                    | Constant::Float(_)
                                    | Constant::Long(_)
                                    | Constant::Double(_)
                                    | Constant::Class { .. }
                                    | Constant::String { .. }
                                    | Constant::MethodHandle { .. }
                                    | Constant::MethodType { .. }
                                    | Constant::Dynamic { .. }
                            )
                        })?;
                        methods.push(BootstrapMethod {
                            bootstrap_method_ref,
                            arguments,
                        });
                    }
                    AttributeBody::BootstrapMethods(methods)
                }
                "MethodParameters" => {
                    let mut parameters = Vec::new();
                    for _ in 0..r.u1()? {
                        let name_index = r.u2()?;
                        if name_index != 0
                            && !descriptor::valid_unqualified_name(
                                pool.utf8(name_index)?.units(),
                                false,
                            )
                        {
                            return Err(r.invalid("invalid method parameter name"));
                        }
                        let access_flags = r.u2()?;
                        parameters.push(MethodParameter {
                            name_index,
                            access_flags,
                        });
                    }
                    AttributeBody::MethodParameters(parameters)
                }
                "ModulePackages" => {
                    AttributeBody::ModulePackages(indices(r, pool, "Package", |c| {
                        matches!(c, Constant::Package { .. })
                    })?)
                }
                "ModuleMainClass" => {
                    let index = r.u2()?;
                    class_index(pool, index)?;
                    AttributeBody::ModuleMainClass(index)
                }
                "NestHost" => {
                    let index = r.u2()?;
                    class_index(pool, index)?;
                    AttributeBody::NestHost(index)
                }
                "NestMembers" => AttributeBody::NestMembers(indices(r, pool, "Class", |c| {
                    matches!(c, Constant::Class { .. })
                })?),
                "PermittedSubclasses" => {
                    AttributeBody::PermittedSubclasses(indices(r, pool, "Class", |c| {
                        matches!(c, Constant::Class { .. })
                    })?)
                }
                "Record" => {
                    let mut components = Vec::new();
                    for _ in 0..r.u2()? {
                        let name_index = r.u2()?;
                        let descriptor_index = r.u2()?;
                        if !descriptor::valid_unqualified_name(
                            pool.utf8(name_index)?.units(),
                            false,
                        ) {
                            return Err(r.invalid("invalid record-component name"));
                        }
                        descriptor::parse_field_descriptor(pool.utf8(descriptor_index)?.units())
                            .map_err(|_| r.error(ErrorKind::InvalidDescriptor))?;
                        let attributes = attributes(
                            r,
                            pool,
                            major,
                            Location::RecordComponent,
                            None,
                            annotation_budget,
                        )?;
                        components.push(RecordComponent {
                            name_index,
                            descriptor_index,
                            attributes,
                        });
                    }
                    AttributeBody::Record(components)
                }
                _ => AttributeBody::Opaque {
                    bytes: r.take(r.remaining())?.to_vec(),
                    standard: info.is_some(),
                },
            }
        };
        r.finish()?;
        result.push(Attribute {
            offset,
            name_index,
            body,
        });
    }
    Ok(result)
}

fn parse_code(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    major: u16,
    annotation_budget: &mut AnnotationBudget,
) -> Result<Code, ParseError> {
    let max_stack = reader.u2()?;
    let max_locals = reader.u2()?;
    let length = reader.u4()? as usize;
    if length == 0 || length >= 65536 {
        return Err(reader.invalid("code_length must be in 1..=65535"));
    }
    let code = reader.take(length)?.to_vec();
    let mut exception_table = Vec::new();
    for _ in 0..reader.u2()? {
        let start_pc = reader.u2()?;
        let end_pc = reader.u2()?;
        let handler_pc = reader.u2()?;
        let catch_type = reader.u2()?;
        if start_pc >= end_pc || usize::from(end_pc) > length || usize::from(handler_pc) >= length {
            return Err(reader.invalid("invalid Code exception-table range"));
        }
        if catch_type != 0 {
            class_index(pool, catch_type)?;
        }
        exception_table.push(ExceptionHandler {
            start_pc,
            end_pc,
            handler_pc,
            catch_type,
        });
    }
    let attributes = attributes(
        reader,
        pool,
        major,
        Location::Code,
        Some(CodeContext {
            length,
            max_locals,
            exception_count: exception_table.len(),
        }),
        annotation_budget,
    )?;
    validate_local_variable_tables(&attributes, pool, reader)?;
    Ok(Code {
        max_stack,
        max_locals,
        code,
        exception_table,
        attributes,
    })
}

fn validate_local_variable_tables(
    attributes: &[Attribute],
    pool: &ConstantPool,
    reader: &Reader<'_>,
) -> Result<(), ParseError> {
    let mut variables = std::collections::BTreeSet::new();
    let mut generic_variables = std::collections::BTreeSet::new();
    for attribute in attributes {
        let (entries, seen) = match &attribute.body {
            AttributeBody::LocalVariableTable(entries) => (entries, &mut variables),
            AttributeBody::LocalVariableTypeTable(entries) => (entries, &mut generic_variables),
            _ => continue,
        };
        // The restriction is on separate attributes for the same variable,
        // not on the number of table attributes. Use semantic UTF-16 text so
        // duplicate constants cannot bypass the cross-attribute check.
        let mut in_attribute = std::collections::BTreeSet::new();
        for entry in entries {
            in_attribute.insert((
                entry.start_pc,
                entry.length,
                entry.index,
                pool.utf8(entry.name_index)?.units(),
                pool.utf8(entry.descriptor_or_signature_index)?.units(),
            ));
        }
        for key in in_attribute {
            if !seen.insert(key) {
                return Err(reader.invalid(
                    "a local variable appears in more than one table attribute of the same kind",
                ));
            }
        }
    }
    Ok(())
}

use super::{
    frames::{Frame, Locals},
    symbols::{Leaf, Primitive, Proof, Ty},
    *,
};
use crabspot_bytecode::{DecodedCode, Instruction, Operands};
use crabspot_classfile::{Code, Constant};
use std::collections::HashSet;

struct MemberSymbol<'a> {
    owner: Ty,
    name: &'a [u16],
    special: u8,
    descriptor: &'a [u16],
    descriptor_index: u16,
}
enum Edit {
    None,
    Store(usize, Ty),
    New(u32),
    Constructor(Ty, Ty),
}
struct Action {
    terminal: bool,
    edit: Edit,
}
struct Handler {
    start: u32,
    end: u32,
    map: usize,
    stack: [Ty; 1],
}
impl Action {
    fn next() -> Self {
        Self {
            terminal: false,
            edit: Edit::None,
        }
    }
}
fn primitive_type(opcode: u8) -> Ty {
    match opcode {
        1 => Ty::Long,
        2 => Ty::Float,
        3 => Ty::Double,
        _ => Ty::Int,
    }
}

impl<'e, 'a> Proof<'e, 'a> {
    pub fn check(&mut self, validation: &MethodCodeValidation<'a>) -> T<()> {
        if validation.class().version.major < 51 {
            return Err(TypeVerificationErrorKind::UnsupportedHistory {
                major: validation.class().version.major,
            });
        }
        let member = validation.method();
        let signature = self.signature(self.class, member.descriptor_index)?;
        let (_, special) = self.name(self.class, member.name_index)?;
        self.final_override(member)?;
        let (Some(code), Some(decoded)) = (validation.code(), validation.decoded()) else {
            return Ok(());
        };
        let current = self.definition_ty(self.class)?;
        let constructor = special == 1;
        let pending = constructor && self.class != self.env.core.object;
        let receiver = special != 2 && member.access_flags & 8 == 0;
        let n = self
            .signatures
            .get(signature)
            .ok_or(bad("missing selected signature"))?
            .parameters
            .len();
        let mut slots = usize::from(receiver);
        for index in 0..n {
            self.work(1)?;
            slots = slots
                .checked_add(self.parameter(signature, index)?.width())
                .ok_or(bad("initial locals overflow"))?;
        }
        self.cells(slots)?;
        let mut values = Vec::new();
        reserve(&mut values, slots, "initial type locals")?;
        if receiver {
            values.push(if pending { Ty::This } else { current });
        }
        for index in 0..n {
            let ty = self.parameter(signature, index)?;
            values.push(ty);
            if ty.width() == 2 {
                values.push(Ty::WideTail);
            }
        }
        let initial = self.publish_prefix(values)?;
        self.prepare_maps(code, initial)?;
        let mut handlers = Vec::new();
        for (index, handler) in code.exception_table.iter().enumerate() {
            self.location = CodeValidationLocation::ExceptionHandler(index);
            self.edge()?;
            let map = self.map_index(u32::from(handler.handler_pc))?;
            let catch = if handler.catch_type == 0 {
                self.core_ty(VerificationCoreRole::Throwable)?
            } else {
                self.class_ty(self.class, handler.catch_type)?
            };
            let throwable = self.core_ty(VerificationCoreRole::Throwable)?;
            self.require_assign(catch, throwable)?;
            let target = self.maps.get(map).ok_or(bad("missing handler map"))?;
            if target.stack.len() != 1 {
                return Err(bad("handler map must contain one slot"));
            }
            let target = *target.stack.first().ok_or(bad("missing handler stack"))?;
            self.require_assign(catch, target)?;
            self.cells(1)?;
            reserve(&mut handlers, 1, "handler type facts")?;
            handlers.push(Handler {
                start: u32::from(handler.start_pc),
                end: u32::from(handler.end_pc),
                map,
                stack: [catch],
            });
        }
        let mut frame = Some(Frame {
            locals: Locals::Shared(initial),
            stack: Vec::new(),
            pending,
        });
        let mut map_cursor = 0;
        for instruction in decoded.instructions() {
            if self
                .maps
                .get(map_cursor)
                .is_some_and(|m| m.offset == instruction.offset)
            {
                self.location = self
                    .maps
                    .get(map_cursor)
                    .ok_or(bad("map disappeared"))?
                    .location;
                if let Some(source) = &frame {
                    self.edge()?;
                    self.compare_map(source, map_cursor, None)?;
                }
                frame = Some(self.map_frame(map_cursor)?);
                map_cursor += 1;
            }
            self.location = CodeValidationLocation::Instruction(instruction.offset);
            self.work(1)?;
            let state = frame.as_mut().ok_or(bad(
                "instruction after terminal transfer requires a declared map",
            ))?;
            let action = self.instruction(
                instruction,
                state,
                code,
                decoded,
                signature,
                current,
                constructor,
            )?;
            for (index, handler) in handlers.iter().enumerate() {
                self.location = CodeValidationLocation::ExceptionHandler(index);
                self.edge()?;
                if instruction.offset >= handler.start && instruction.offset < handler.end {
                    self.edge()?;
                    self.compare_parts(
                        &state.locals,
                        &handler.stack,
                        state.pending,
                        handler.map,
                        None,
                    )?;
                    if let Edit::Constructor(old, new) = action.edit {
                        self.edge()?;
                        self.compare_parts(
                            &state.locals,
                            &handler.stack,
                            state.pending,
                            handler.map,
                            Some((old, new)),
                        )?;
                    }
                }
            }
            self.location = CodeValidationLocation::Instruction(instruction.offset);
            match action.edit {
                Edit::None => {}
                Edit::Store(index, ty) => self.store(state, index, ty, code.max_locals)?,
                Edit::New(offset) => self.substitute_locals(state, Ty::Uninit(offset), Ty::Top)?,
                Edit::Constructor(old, new) => {
                    self.substitute(state, old, new)?;
                    if old == Ty::This {
                        state.pending = false;
                    }
                }
            }
            if action.terminal {
                frame = None;
            }
        }
        if frame.is_some() {
            return Err(bad("execution can fall off Code end"));
        }
        Ok(())
    }
    fn parameter(&self, signature: usize, index: usize) -> T<Ty> {
        self.signatures
            .get(signature)
            .and_then(|s| s.parameters.get(index))
            .copied()
            .ok_or(bad("signature parameter index invalid"))
    }
    fn result(&self, signature: usize) -> T<Option<Ty>> {
        self.signatures
            .get(signature)
            .map(|s| s.result)
            .ok_or(bad("signature result index invalid"))
    }
    fn member(&mut self, index: u16) -> T<MemberSymbol<'a>> {
        let (owner, nat) = match self.cp(self.class, index)? {
            Constant::Fieldref {
                class_index,
                name_and_type_index,
            }
            | Constant::Methodref {
                class_index,
                name_and_type_index,
            }
            | Constant::InterfaceMethodref {
                class_index,
                name_and_type_index,
            } => (*class_index, *name_and_type_index),
            _ => return Err(bad("invalid member-reference operand")),
        };
        let owner = self.class_ty(self.class, owner)?;
        let Constant::NameAndType {
            name_index,
            descriptor_index,
        } = self.cp(self.class, nat)?
        else {
            return Err(bad("invalid member name-and-type"));
        };
        let (name_index, descriptor_index) = (*name_index, *descriptor_index);
        let (name, special) = self.name(self.class, name_index)?;
        let descriptor = self.utf(self.class, descriptor_index)?;
        Ok(MemberSymbol {
            owner,
            name,
            special,
            descriptor,
            descriptor_index,
        })
    }
    fn fact(
        &mut self,
        owner: crabspot_runtime::ClassId,
        name: &[u16],
        descriptor: &[u16],
        field: bool,
    ) -> T<Option<(crabspot_runtime::ClassId, u16)>> {
        let mut work = Vec::new();
        reserve(&mut work, 1, "protected fact traversal")?;
        work.push(owner);
        let mut seen = HashSet::new();
        while let Some(class) = work.pop() {
            self.work(1)?;
            if seen.contains(&class) {
                continue;
            }
            seen.try_reserve(1)
                .map_err(|_| TypeVerificationErrorKind::AllocationFailure {
                    resource: "protected visited classes",
                })?;
            seen.insert(class);
            let node = self
                .env
                .nodes
                .get(&class)
                .ok_or(TypeVerificationErrorKind::UnretainedClass { class })?;
            let members = if field {
                &node.metadata.fields
            } else {
                &node.metadata.methods
            };
            for member in members {
                if self.member_equal(class, member, name, descriptor)? {
                    return Ok(Some((class, member.access_flags)));
                }
            }
            if let Some(parent) = node
                .layout
                .direct_superclass()
                .filter(|_| !field || node.metadata.access_flags & 0x200 == 0)
            {
                self.work(1)?;
                reserve(&mut work, 1, "protected ancestry")?;
                work.push(parent);
            }
            if field {
                for interface in node.layout.direct_interfaces().iter().rev() {
                    self.work(1)?;
                    reserve(&mut work, 1, "protected interfaces")?;
                    work.push(*interface);
                }
            }
        }
        Ok(None)
    }
    fn protected(
        &mut self,
        symbol: &MemberSymbol<'a>,
        receiver: Ty,
        field: bool,
        current: Ty,
    ) -> T<()> {
        let Ty::Class(id) = symbol.owner else {
            return Ok(());
        };
        let name = self.symbol_name(id)?;
        let mut ancestor = self
            .env
            .nodes
            .get(&self.class)
            .ok_or(TypeVerificationErrorKind::UnretainedClass { class: self.class })?
            .layout
            .direct_superclass();
        if ancestor.is_some() {
            self.work(1)?;
        }
        let mut relevant = false;
        for _ in 0..self.env.nodes.len() {
            let Some(class) = ancestor else {
                break;
            };
            self.work(1)?;
            let node = self
                .env
                .nodes
                .get(&class)
                .ok_or(TypeVerificationErrorKind::UnretainedClass { class })?;
            self.utf_work(name.len())?;
            self.utf_work(node.name.len())?;
            if name == node.name {
                relevant = true;
                break;
            }
            ancestor = node.layout.direct_superclass();
            if ancestor.is_some() {
                self.work(1)?;
            }
        }
        if !relevant {
            return Ok(());
        }
        let owner = self.resolve(id)?;
        let Some((declaring, flags)) = self.fact(owner, symbol.name, symbol.descriptor, field)?
        else {
            return Ok(());
        };
        if flags & 4 == 0 || flags & 8 != 0 {
            return Ok(());
        }
        let actual = self
            .env
            .nodes
            .get(&declaring)
            .ok_or(TypeVerificationErrorKind::UnretainedClass { class: declaring })?;
        let here = self
            .env
            .nodes
            .get(&self.class)
            .ok_or(TypeVerificationErrorKind::UnretainedClass { class: self.class })?;
        if actual.package == here.package {
            return Ok(());
        }
        let clone = symbol
            .name
            .iter()
            .copied()
            .eq("clone".bytes().map(u16::from));
        let clone_descriptor = symbol
            .descriptor
            .iter()
            .copied()
            .eq("()Ljava/lang/Object;".bytes().map(u16::from));
        if !field
            && matches!(receiver, Ty::Array(..))
            && owner == self.env.core.object
            && clone
            && clone_descriptor
        {
            return Ok(());
        }
        self.require_assign(receiver, current)
    }
    fn target(&mut self, frame: &Frame, offset: u32, displacement: i32) -> T<()> {
        self.edge()?;
        let target = u32::try_from(i64::from(offset) + i64::from(displacement))
            .map_err(|_| bad("invalid signed control target"))?;
        let map = self.map_index(target)?;
        self.compare_map(frame, map, None)
    }
    fn allocated(&mut self, decoded: &DecodedCode, offset: u32) -> T<Ty> {
        self.work(16)?;
        let instruction = decoded
            .instruction_at(offset)
            .ok_or(bad("uninitialized allocation instruction missing"))?;
        if instruction.opcode != 0xbb {
            return Err(bad("uninitialized site is not new"));
        }
        let Operands::Constant(index) = instruction.operands else {
            return Err(bad("new has invalid operands"));
        };
        self.class_ty(self.class, index)
    }
    fn pop_assign(&mut self, frame: &mut Frame, to: Ty) -> T<Ty> {
        let from = self.pop(frame)?;
        if !from.ordinary() && to.ordinary() {
            return Err(bad(
                "unfinished reference cannot escape as an ordinary value",
            ));
        }
        self.require_assign(from, to)?;
        Ok(from)
    }
    fn array_component(&self, ty: Ty) -> T<Ty> {
        match ty {
            Ty::Null => Ok(Ty::Null),
            Ty::Array(rank, leaf) if rank > 1 => Ok(Ty::Array(rank - 1, leaf)),
            Ty::Array(1, Leaf::Class(id)) => Ok(Ty::Class(id)),
            _ => Err(bad("reference array required")),
        }
    }
    fn array_primitive(&self, ty: Ty, kind: Primitive) -> T<()> {
        if ty == Ty::Null
            || matches!(ty,Ty::Array(1,Leaf::Primitive(p)) if p==kind||(kind==Primitive::Byte&&p==Primitive::Boolean))
        {
            Ok(())
        } else {
            Err(bad("wrong primitive array type"))
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn instruction(
        &mut self,
        i: &Instruction,
        f: &mut Frame,
        code: &Code,
        decoded: &DecodedCode,
        signature: usize,
        current: Ty,
        constructor: bool,
    ) -> T<Action> {
        let op = i.opcode;
        let max = code.max_stack;
        let mut action = Action::next();
        match op {
            0x00 => {}
            0x01 => self.push(f, Ty::Null, max)?,
            0x02..=0x08 | 0x10 | 0x11 => self.push(f, Ty::Int, max)?,
            0x09..=0x0a => self.push(f, Ty::Long, max)?,
            0x0b..=0x0d => self.push(f, Ty::Float, max)?,
            0x0e..=0x0f => self.push(f, Ty::Double, max)?,
            0x12..=0x14 => {
                let Operands::Constant(index) = i.operands else {
                    return Err(bad("ldc operand missing"));
                };
                let ty = match self.cp(self.class, index)? {
                    Constant::Integer(_) => Ty::Int,
                    Constant::Float(_) => Ty::Float,
                    Constant::Long(_) => Ty::Long,
                    Constant::Double(_) => Ty::Double,
                    Constant::String { .. } => self.core_ty(VerificationCoreRole::String)?,
                    Constant::Class { .. } => self.core_ty(VerificationCoreRole::Class)?,
                    Constant::MethodType { .. } => {
                        self.core_ty(VerificationCoreRole::MethodType)?
                    }
                    Constant::MethodHandle { .. } => {
                        self.core_ty(VerificationCoreRole::MethodHandle)?
                    }
                    Constant::Dynamic {
                        name_and_type_index,
                        ..
                    } => {
                        let nat = *name_and_type_index;
                        let Constant::NameAndType {
                            descriptor_index, ..
                        } = self.cp(self.class, nat)?
                        else {
                            return Err(bad("dynamic name-and-type missing"));
                        };
                        self.field_type(self.class, *descriptor_index)?
                    }
                    _ => return Err(bad("invalid ldc type")),
                };
                self.push(f, ty, max)?;
            }
            0x15..=0x2d => {
                let (index, group) = if op <= 0x19 {
                    let Operands::Local(index) = i.operands else {
                        return Err(bad("load local missing"));
                    };
                    (usize::from(index), op - 0x15)
                } else {
                    (usize::from((op - 0x1a) % 4), (op - 0x1a) / 4)
                };
                let ty = self.load(
                    f,
                    index,
                    if group == 4 {
                        None
                    } else {
                        Some(primitive_type(group))
                    },
                )?;
                self.push(f, ty, max)?;
            }
            0x2e..=0x35 => {
                self.pop_exact(f, Ty::Int)?;
                let array = self.pop_reference(f, true)?;
                let ty = match op {
                    0x32 => self.array_component(array)?,
                    0x2e => {
                        self.array_primitive(array, Primitive::Int)?;
                        Ty::Int
                    }
                    0x2f => {
                        self.array_primitive(array, Primitive::Long)?;
                        Ty::Long
                    }
                    0x30 => {
                        self.array_primitive(array, Primitive::Float)?;
                        Ty::Float
                    }
                    0x31 => {
                        self.array_primitive(array, Primitive::Double)?;
                        Ty::Double
                    }
                    0x33 => {
                        self.array_primitive(array, Primitive::Byte)?;
                        Ty::Int
                    }
                    0x34 => {
                        self.array_primitive(array, Primitive::Char)?;
                        Ty::Int
                    }
                    _ => {
                        self.array_primitive(array, Primitive::Short)?;
                        Ty::Int
                    }
                };
                self.push(f, ty, max)?;
            }
            0x36..=0x4e => {
                let (index, group) = if op <= 0x3a {
                    let Operands::Local(index) = i.operands else {
                        return Err(bad("store local missing"));
                    };
                    (usize::from(index), op - 0x36)
                } else {
                    (usize::from((op - 0x3b) % 4), (op - 0x3b) / 4)
                };
                let value = if group == 4 {
                    self.pop_reference(f, false)?
                } else {
                    self.pop_exact(f, primitive_type(group))?
                };
                action.edit = Edit::Store(index, value);
            }
            0x4f..=0x56 => {
                let value = match op {
                    0x50 => self.pop_exact(f, Ty::Long)?,
                    0x51 => self.pop_exact(f, Ty::Float)?,
                    0x52 => self.pop_exact(f, Ty::Double)?,
                    0x53 => self.pop_reference(f, true)?,
                    _ => self.pop_exact(f, Ty::Int)?,
                };
                self.pop_exact(f, Ty::Int)?;
                let array = self.pop_reference(f, true)?;
                if op == 0x53 {
                    self.array_component(array)?;
                } else {
                    let kind = match op {
                        0x4f => Primitive::Int,
                        0x50 => Primitive::Long,
                        0x51 => Primitive::Float,
                        0x52 => Primitive::Double,
                        0x54 => Primitive::Byte,
                        0x55 => Primitive::Char,
                        _ => Primitive::Short,
                    };
                    self.array_primitive(array, kind)?;
                }
                let _ = value;
            }
            0x57..=0x5f => self.stack_operation(f, op, max)?,
            0x60..=0x73 => {
                let ty = primitive_type((op - 0x60) % 4);
                self.pop_exact(f, ty)?;
                self.pop_exact(f, ty)?;
                self.push(f, ty, max)?;
            }
            0x74..=0x77 => {
                let ty = primitive_type(op - 0x74);
                self.pop_exact(f, ty)?;
                self.push(f, ty, max)?;
            }
            0x78..=0x7d => {
                let ty = if op % 2 == 0 { Ty::Int } else { Ty::Long };
                self.pop_exact(f, Ty::Int)?;
                self.pop_exact(f, ty)?;
                self.push(f, ty, max)?;
            }
            0x7e..=0x83 => {
                let ty = if op % 2 == 0 { Ty::Int } else { Ty::Long };
                self.pop_exact(f, ty)?;
                self.pop_exact(f, ty)?;
                self.push(f, ty, max)?;
            }
            0x84 => {
                let Operands::Iinc { index, .. } = i.operands else {
                    return Err(bad("iinc index missing"));
                };
                self.load(f, usize::from(index), Some(Ty::Int))?;
            }
            0x85..=0x93 => {
                let (from, to) = match op {
                    0x85 => (Ty::Int, Ty::Long),
                    0x86 => (Ty::Int, Ty::Float),
                    0x87 => (Ty::Int, Ty::Double),
                    0x88 => (Ty::Long, Ty::Int),
                    0x89 => (Ty::Long, Ty::Float),
                    0x8a => (Ty::Long, Ty::Double),
                    0x8b => (Ty::Float, Ty::Int),
                    0x8c => (Ty::Float, Ty::Long),
                    0x8d => (Ty::Float, Ty::Double),
                    0x8e => (Ty::Double, Ty::Int),
                    0x8f => (Ty::Double, Ty::Long),
                    0x90 => (Ty::Double, Ty::Float),
                    _ => (Ty::Int, Ty::Int),
                };
                self.pop_exact(f, from)?;
                self.push(f, to, max)?;
            }
            0x94..=0x98 => {
                let ty = if op == 0x94 {
                    Ty::Long
                } else if op <= 0x96 {
                    Ty::Float
                } else {
                    Ty::Double
                };
                self.pop_exact(f, ty)?;
                self.pop_exact(f, ty)?;
                self.push(f, Ty::Int, max)?;
            }
            0x99..=0xa6 | 0xc6 | 0xc7 => {
                if (0x99..=0x9e).contains(&op) {
                    self.pop_exact(f, Ty::Int)?;
                } else if (0x9f..=0xa4).contains(&op) {
                    self.pop_exact(f, Ty::Int)?;
                    self.pop_exact(f, Ty::Int)?;
                } else {
                    self.pop_reference(f, false)?;
                    if op <= 0xa6 {
                        self.pop_reference(f, false)?;
                    }
                }
                let Operands::Branch(d) = i.operands else {
                    return Err(bad("branch displacement missing"));
                };
                self.target(f, i.offset, d)?;
            }
            0xa7 | 0xc8 => {
                let Operands::Branch(d) = i.operands else {
                    return Err(bad("goto displacement missing"));
                };
                self.target(f, i.offset, d)?;
                action.terminal = true;
            }
            0xaa | 0xab => {
                self.pop_exact(f, Ty::Int)?;
                match &i.operands {
                    Operands::TableSwitch {
                        default, offsets, ..
                    } => {
                        self.target(f, i.offset, *default)?;
                        for d in offsets {
                            self.target(f, i.offset, *d)?;
                        }
                    }
                    Operands::LookupSwitch { default, pairs } => {
                        self.target(f, i.offset, *default)?;
                        for (_, d) in pairs {
                            self.target(f, i.offset, *d)?;
                        }
                    }
                    _ => return Err(bad("switch operands missing")),
                };
                action.terminal = true;
            }
            0xac..=0xb1 => {
                let result = self.result(signature)?;
                if op == 0xb1 {
                    if result.is_some() {
                        return Err(bad("void return in value method"));
                    }
                } else {
                    let expected = match op {
                        0xac => Ty::Int,
                        0xad => Ty::Long,
                        0xae => Ty::Float,
                        0xaf => Ty::Double,
                        _ => result.ok_or(bad("reference return in void method"))?,
                    };
                    if op == 0xb0 {
                        if !expected.ordinary() {
                            return Err(bad("reference return in primitive method"));
                        }
                        self.pop_assign(f, expected)?;
                    } else {
                        if result != Some(expected) {
                            return Err(bad("return opcode differs from descriptor"));
                        }
                        self.pop_exact(f, expected)?;
                    }
                }
                if constructor && f.pending {
                    return Err(bad("constructor returns before chaining"));
                }
                action.terminal = true;
            }
            0xb2..=0xb5 => {
                let Operands::Constant(index) = i.operands else {
                    return Err(bad("field operand missing"));
                };
                let symbol = self.member(index)?;
                let field = self.field_type(self.class, symbol.descriptor_index)?;
                if op == 0xb3 || op == 0xb5 {
                    self.pop_assign(f, field)?;
                }
                if op == 0xb4 || op == 0xb5 {
                    let receiver = self.pop_reference(f, false)?;
                    if receiver == Ty::This && op == 0xb5 && constructor {
                        if symbol.owner != current {
                            return Err(bad("early putfield must name current class"));
                        }
                        let metadata = self.env.registry.class(self.class)?;
                        let mut found = false;
                        for member in &metadata.fields {
                            if self.member_equal(
                                self.class,
                                member,
                                symbol.name,
                                symbol.descriptor,
                            )? {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            return Err(bad("early putfield must declare own field"));
                        }
                    } else {
                        if !receiver.ordinary() {
                            return Err(bad("field receiver is unfinished"));
                        }
                        self.require_assign(receiver, symbol.owner)?;
                        self.protected(&symbol, receiver, true, current)?;
                    }
                }
                if op == 0xb2 || op == 0xb4 {
                    self.push(f, field, max)?;
                }
            }
            0xb6..=0xba => {
                let index = match i.operands {
                    Operands::Constant(index)
                    | Operands::InvokeInterface { index, .. }
                    | Operands::InvokeDynamic { index } => index,
                    _ => return Err(bad("invocation operand missing")),
                };
                let (symbol, signature) = if op == 0xba {
                    let Constant::InvokeDynamic {
                        name_and_type_index,
                        ..
                    } = self.cp(self.class, index)?
                    else {
                        return Err(bad("invokedynamic constant missing"));
                    };
                    let Constant::NameAndType {
                        descriptor_index, ..
                    } = self.cp(self.class, *name_and_type_index)?
                    else {
                        return Err(bad("dynamic name-and-type missing"));
                    };
                    (None, self.signature(self.class, *descriptor_index)?)
                } else {
                    let symbol = self.member(index)?;
                    let signature = self.signature(self.class, symbol.descriptor_index)?;
                    (Some(symbol), signature)
                };
                let count = self
                    .signatures
                    .get(signature)
                    .ok_or(bad("signature disappeared"))?
                    .parameters
                    .len();
                for p in (0..count).rev() {
                    self.work(1)?;
                    let expected = self.parameter(signature, p)?;
                    self.pop_assign(f, expected)?;
                }
                if op != 0xb8 && op != 0xba {
                    let symbol = symbol.as_ref().ok_or(bad("instance symbol missing"))?;
                    let receiver = self.pop_reference(f, false)?;
                    if op == 0xb7 && symbol.special == 1 {
                        let new = match receiver {
                            Ty::This => {
                                if !constructor {
                                    return Err(bad("unfinished this outside constructor"));
                                }
                                let owner = match symbol.owner {
                                    Ty::Class(id) => self.resolve(id)?,
                                    _ => return Err(bad("constructor owner is array")),
                                };
                                let parent = self
                                    .env
                                    .nodes
                                    .get(&self.class)
                                    .ok_or(TypeVerificationErrorKind::UnretainedClass {
                                        class: self.class,
                                    })?
                                    .layout
                                    .direct_superclass();
                                if owner != self.class && parent != Some(owner) {
                                    return Err(bad(
                                        "this constructor must name current or direct superclass",
                                    ));
                                }
                                current
                            }
                            Ty::Uninit(offset) => {
                                let allocated = self.allocated(decoded, offset)?;
                                if allocated != symbol.owner {
                                    return Err(bad("constructor owner differs from allocation"));
                                }
                                self.protected(symbol, allocated, false, current)?;
                                allocated
                            }
                            _ => return Err(bad("constructor receiver is initialized")),
                        };
                        action.edit = Edit::Constructor(receiver, new);
                    } else {
                        if !receiver.ordinary() {
                            return Err(bad("ordinary method receiver is unfinished"));
                        }
                        self.require_assign(receiver, symbol.owner)?;
                        if op == 0xb7 {
                            self.require_assign(receiver, current)?;
                            let owner = match symbol.owner {
                                Ty::Class(id) => self.resolve(id)?,
                                _ => return Err(bad("special owner is array")),
                            };
                            let node = self.env.nodes.get(&self.class).ok_or(
                                TypeVerificationErrorKind::UnretainedClass { class: self.class },
                            )?;
                            let mut direct = false;
                            for interface in node.layout.direct_interfaces() {
                                self.work(1)?;
                                if *interface == owner {
                                    direct = true;
                                    break;
                                }
                            }
                            if owner != self.class
                                && !direct
                                && !self.subclass(self.class, owner)?
                            {
                                return Err(bad("illegal ordinary special owner"));
                            }
                        } else if op == 0xb6 {
                            self.protected(symbol, receiver, false, current)?;
                        }
                    }
                }
                if let Some(result) = self.result(signature)? {
                    self.push(f, result, max)?;
                }
            }
            0xbb => {
                let Operands::Constant(index) = i.operands else {
                    return Err(bad("new class missing"));
                };
                let _ = self.class_ty(self.class, index)?;
                self.work(f.stack.len())?;
                if f.stack.contains(&Ty::Uninit(i.offset)) {
                    return Err(bad("new site already live on stack"));
                }
                self.push(f, Ty::Uninit(i.offset), max)?;
                action.edit = Edit::New(i.offset);
            }
            0xbc => {
                self.pop_exact(f, Ty::Int)?;
                let Operands::NewArray(kind) = i.operands else {
                    return Err(bad("newarray type missing"));
                };
                let kind = match kind {
                    4 => Primitive::Boolean,
                    5 => Primitive::Char,
                    6 => Primitive::Float,
                    7 => Primitive::Double,
                    8 => Primitive::Byte,
                    9 => Primitive::Short,
                    10 => Primitive::Int,
                    11 => Primitive::Long,
                    _ => return Err(bad("invalid primitive array type")),
                };
                self.push(f, Ty::Array(1, Leaf::Primitive(kind)), max)?;
            }
            0xbd => {
                self.pop_exact(f, Ty::Int)?;
                let Operands::Constant(index) = i.operands else {
                    return Err(bad("anewarray type missing"));
                };
                let component = self.class_ty(self.class, index)?;
                let array = match component {
                    Ty::Class(id) => Ty::Array(1, Leaf::Class(id)),
                    Ty::Array(rank, leaf) => {
                        Ty::Array(rank.checked_add(1).ok_or(bad("array rank overflow"))?, leaf)
                    }
                    _ => return Err(bad("invalid array component")),
                };
                self.push(f, array, max)?;
            }
            0xbe => {
                let ty = self.pop_reference(f, true)?;
                if !matches!(ty, Ty::Array(..) | Ty::Null) {
                    return Err(bad("arraylength requires array"));
                }
                self.push(f, Ty::Int, max)?;
            }
            0xbf => {
                let value = self.pop_reference(f, true)?;
                let throwable = self.core_ty(VerificationCoreRole::Throwable)?;
                self.require_assign(value, throwable)?;
                action.terminal = true;
            }
            0xc0 | 0xc1 => {
                self.pop_reference(f, true)?;
                let Operands::Constant(index) = i.operands else {
                    return Err(bad("cast type missing"));
                };
                let ty = self.class_ty(self.class, index)?;
                self.push(f, if op == 0xc0 { ty } else { Ty::Int }, max)?;
            }
            0xc2 | 0xc3 => {
                self.pop_reference(f, false)?;
            }
            0xc5 => {
                let Operands::MultiANewArray { index, dimensions } = i.operands else {
                    return Err(bad("multianewarray operand missing"));
                };
                for _ in 0..dimensions {
                    self.pop_exact(f, Ty::Int)?;
                }
                let ty = self.class_ty(self.class, index)?;
                self.push(f, ty, max)?;
            }
            _ => return Err(bad("opcode lacks a modern verification rule")),
        }
        Ok(action)
    }
    fn stack_operation(&mut self, f: &mut Frame, op: u8, max: u16) -> T<()> {
        let a = self.pop(f)?;
        let one = |ty: Ty| {
            if ty.width() == 1 {
                Ok(())
            } else {
                Err(bad("stack form requires category one"))
            }
        };
        match op {
            0x57 => one(a)?,
            0x58 => {
                if a.width() == 1 {
                    one(self.pop(f)?)?;
                }
            }
            0x59 => {
                one(a)?;
                self.push(f, a, max)?;
                self.push(f, a, max)?;
            }
            0x5a => {
                one(a)?;
                let b = self.pop(f)?;
                one(b)?;
                for v in [a, b, a] {
                    self.push(f, v, max)?;
                }
            }
            0x5b => {
                one(a)?;
                let b = self.pop(f)?;
                if b.width() == 2 {
                    for v in [a, b, a] {
                        self.push(f, v, max)?;
                    }
                } else {
                    let c = self.pop(f)?;
                    one(c)?;
                    for v in [a, c, b, a] {
                        self.push(f, v, max)?;
                    }
                }
            }
            0x5c => {
                if a.width() == 2 {
                    self.push(f, a, max)?;
                    self.push(f, a, max)?;
                } else {
                    let b = self.pop(f)?;
                    one(b)?;
                    for v in [b, a, b, a] {
                        self.push(f, v, max)?;
                    }
                }
            }
            0x5d => {
                let b = self.pop(f)?;
                one(b)?;
                if a.width() == 2 {
                    for v in [a, b, a] {
                        self.push(f, v, max)?;
                    }
                } else {
                    let c = self.pop(f)?;
                    one(c)?;
                    for v in [b, a, c, b, a] {
                        self.push(f, v, max)?;
                    }
                }
            }
            0x5e => {
                let b = self.pop(f)?;
                if a.width() == 2 {
                    if b.width() == 2 {
                        for v in [a, b, a] {
                            self.push(f, v, max)?;
                        }
                    } else {
                        let c = self.pop(f)?;
                        one(c)?;
                        for v in [a, c, b, a] {
                            self.push(f, v, max)?;
                        }
                    }
                } else {
                    one(b)?;
                    let c = self.pop(f)?;
                    if c.width() == 2 {
                        for v in [b, a, c, b, a] {
                            self.push(f, v, max)?;
                        }
                    } else {
                        let d = self.pop(f)?;
                        one(d)?;
                        for v in [b, a, d, c, b, a] {
                            self.push(f, v, max)?;
                        }
                    }
                }
            }
            0x5f => {
                one(a)?;
                let b = self.pop(f)?;
                one(b)?;
                self.push(f, a, max)?;
                self.push(f, b, max)?;
            }
            _ => return Err(bad("unknown stack manipulation")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Spec, World};
    use super::*;

    #[test]
    fn field_lookup_never_follows_an_interfaces_structural_object_edge() {
        let world = World::new(
            &[
                Spec {
                    name: "q/Base",
                    parent: Some("java/lang/Object"),
                    interfaces: &[],
                    flags: 0x21,
                    fields: &[("f", "I", 4)],
                    methods: &[],
                },
                Spec {
                    name: "q/I",
                    parent: Some("java/lang/Object"),
                    interfaces: &[],
                    flags: 0x601,
                    fields: &[],
                    methods: &[],
                },
                Spec {
                    name: "q/Mid",
                    parent: Some("q/Base"),
                    interfaces: &["q/I"],
                    flags: 0x21,
                    fields: &[],
                    methods: &[],
                },
                Spec {
                    name: "p/Child",
                    parent: Some("q/Mid"),
                    interfaces: &[],
                    flags: 0x21,
                    fields: &[],
                    methods: &[],
                },
            ],
            &[("f", "I", 1)],
        );
        let env = world.environment();
        let child = world.extra[3];
        let mut p = Proof::new(
            &env,
            child,
            TypeVerificationOptions::default(),
            CodeValidationUsage::default(),
        );
        let name: Vec<_> = "f".encode_utf16().collect();
        let descriptor: Vec<_> = "I".encode_utf16().collect();
        assert_eq!(
            p.fact(world.extra[2], &name, &descriptor, true).unwrap(),
            Some((world.extra[0], 4))
        );
        let owner = p.definition_ty(world.extra[2]).unwrap();
        let receiver = p.definition_ty(world.extra[0]).unwrap();
        let current = p.definition_ty(child).unwrap();
        let symbol = MemberSymbol {
            owner,
            name: &name,
            special: 0,
            descriptor: &descriptor,
            descriptor_index: 0,
        };
        assert!(matches!(
            p.protected(&symbol, receiver, true, current),
            Err(TypeVerificationErrorKind::TypeRule { .. })
        ));
    }
    #[test]
    fn protected_initial_superclass_edge_is_charged_before_traversal() {
        let world = World::new(
            &[Spec {
                name: "p/C",
                parent: Some("java/lang/Object"),
                interfaces: &[],
                flags: 0x21,
                fields: &[],
                methods: &[],
            }],
            &[],
        );
        let env = world.environment();
        let mut p = Proof::new(
            &env,
            world.extra[0],
            TypeVerificationOptions::default(),
            CodeValidationUsage::default(),
        );
        let current = p.definition_ty(world.extra[0]).unwrap();
        let owner = p.definition_ty(world.core.object).unwrap();
        let name: Vec<_> = "f".encode_utf16().collect();
        let descriptor: Vec<_> = "I".encode_utf16().collect();
        let symbol = MemberSymbol {
            owner,
            name: &name,
            special: 0,
            descriptor: &descriptor,
            descriptor_index: 0,
        };
        p.options.max_work_units = p.usage.work_units;
        let before = p.usage;
        assert!(
            matches!(p.protected(&symbol,current,true,current),Err(TypeVerificationErrorKind::ResourceLimit{resource:TypeVerificationResource::WorkUnits,required,..}) if required==before.work_units+1)
        );
        assert_eq!(p.usage, before);
    }
}

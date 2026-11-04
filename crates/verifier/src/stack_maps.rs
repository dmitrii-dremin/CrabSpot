use crate::{
    Checked, CodeValidationErrorKind as E, CodeValidationLocation as L, inspect::Context, scan,
};
use crabspot_bytecode::DecodedCode;
use crabspot_classfile::{Code, StackMapFrame, StackMapFrameKind as Frame, VerificationType as V};

impl Context<'_> {
    fn verification_type(
        &mut self,
        decoded: &DecodedCode,
        value: &V,
        constructor: bool,
    ) -> Checked<u8> {
        match value {
            V::Long | V::Double => Ok(2),
            V::Object(index) => {
                self.class_name(*index)?;
                Ok(1)
            }
            V::Uninitialized(offset) => {
                if self.boundary(decoded, u32::from(*offset))?.opcode != 0xbb {
                    return Err(E::InvalidStackMap("uninitialized offset does not name new"));
                }
                Ok(1)
            }
            V::UninitializedThis if !constructor => Err(E::InvalidStackMap(
                "UninitializedThis requires a selected constructor",
            )),
            _ => Ok(1),
        }
    }
    fn list(&mut self, length: usize) -> Checked<()> {
        Self::count(length)?;
        self.metadata(length)
    }
    fn stack_slots(
        &mut self,
        decoded: &DecodedCode,
        stack: &[V],
        constructor: bool,
        max_stack: u16,
    ) -> Checked<()> {
        self.list(stack.len())?;
        let mut slots = 0_u32;
        for value in stack {
            slots = slots
                .checked_add(u32::from(self.verification_type(
                    decoded,
                    value,
                    constructor,
                )?))
                .ok_or(E::InvalidStackMap("stack width overflow"))?;
            if slots > u32::from(max_stack) {
                return Err(E::InvalidStackMap("frame stack exceeds max_stack"));
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn stack_maps(
        &mut self,
        decoded: &DecodedCode,
        code: &Code,
        attribute: usize,
        frames: &[StackMapFrame],
        locals: &mut Vec<u8>,
        initial_slots: u16,
        constructor: bool,
    ) -> Checked<()> {
        Self::count(frames.len())?;
        self.metadata(frames.len())?;
        let mut previous: Option<u32> = None;
        let mut local_slots = u32::from(initial_slots);
        for (index, frame) in frames.iter().enumerate() {
            self.location = L::StackMapFrame {
                attribute,
                frame: index,
            };
            let offset = match previous {
                Some(previous) => previous
                    .checked_add(u32::from(frame.offset_delta))
                    .and_then(|value| value.checked_add(1)),
                None => Some(u32::from(frame.offset_delta)),
            }
            .ok_or(E::InvalidStackMap("frame offset arithmetic overflow"))?;
            if offset != frame.bytecode_offset {
                return Err(E::InvalidStackMap(
                    "stored frame offset differs from offset_delta",
                ));
            }
            self.boundary(decoded, offset)?;
            previous = Some(offset);
            match &frame.kind {
                Frame::Same => {}
                Frame::SameLocalsOneStack(value) => self.stack_slots(
                    decoded,
                    std::slice::from_ref(value),
                    constructor,
                    code.max_stack,
                )?,
                Frame::Chop(count) => {
                    if !(1..=3).contains(count) || usize::from(*count) > locals.len() {
                        return Err(E::InvalidStackMap("invalid or underflowing Chop frame"));
                    }
                    for _ in 0..*count {
                        let width = locals
                            .pop()
                            .ok_or(E::InvalidStackMap("Chop locals disappeared"))?;
                        local_slots = local_slots
                            .checked_sub(u32::from(width))
                            .ok_or(E::InvalidStackMap("Chop slot underflow"))?;
                    }
                }
                Frame::Append(values) => {
                    self.list(values.len())?;
                    if !(1..=3).contains(&values.len()) {
                        return Err(E::InvalidStackMap(
                            "Append must have one through three entries",
                        ));
                    }
                    scan::reserve(locals, values.len(), "current local widths")?;
                    for value in values {
                        let width = self.verification_type(decoded, value, constructor)?;
                        local_slots = local_slots
                            .checked_add(u32::from(width))
                            .ok_or(E::InvalidStackMap("local width overflow"))?;
                        if local_slots > u32::from(code.max_locals) {
                            return Err(E::InvalidStackMap("frame locals exceed max_locals"));
                        }
                        locals.push(width);
                    }
                }
                Frame::Full {
                    locals: values,
                    stack,
                } => {
                    self.list(values.len())?;
                    locals.clear();
                    local_slots = 0;
                    scan::reserve(locals, values.len(), "current local widths")?;
                    for value in values {
                        let width = self.verification_type(decoded, value, constructor)?;
                        local_slots = local_slots
                            .checked_add(u32::from(width))
                            .ok_or(E::InvalidStackMap("local width overflow"))?;
                        if local_slots > u32::from(code.max_locals) {
                            return Err(E::InvalidStackMap("frame locals exceed max_locals"));
                        }
                        locals.push(width);
                    }
                    self.stack_slots(decoded, stack, constructor, code.max_stack)?;
                }
            }
        }
        Ok(())
    }
}

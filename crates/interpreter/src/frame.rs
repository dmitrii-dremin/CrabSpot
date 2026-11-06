use crate::{ExecutionError, ExecutionErrorKind as E, Value, ValueKind};

use crabspot_runtime::{MethodId, Reference};
pub(crate) fn protocol(offset: u32, reason: &'static str) -> ExecutionError {
    ExecutionError::new(Some(offset), E::ConstructionProtocol { reason })
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AliasKey {
    Allocation { method: MethodId, offset: u32 },
    This(u64),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Uninitialized {
    pub reference: Reference,
    pub key: AliasKey,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameValue {
    Ordinary(Value),
    Uninitialized(Uninitialized),
}
impl FrameValue {
    fn slots(self) -> usize {
        match self {
            Self::Ordinary(v) => v.slots(),
            Self::Uninitialized(_) => 1,
        }
    }
    fn kind(self) -> ValueKind {
        match self {
            Self::Ordinary(v) => v.kind(),
            Self::Uninitialized(_) => ValueKind::Reference,
        }
    }
    fn ordinary(self, pc: u32) -> Result<Value, ExecutionError> {
        match self {
            Self::Ordinary(v) => Ok(v),
            _ => Err(protocol(
                pc,
                "unfinished reference used as an ordinary value",
            )),
        }
    }
    pub fn reference(self, pc: u32) -> Result<Reference, ExecutionError> {
        match self {
            Self::Ordinary(Value::Reference(r)) => Ok(r),
            Self::Uninitialized(v) if !v.reference.is_null() => Ok(v.reference),
            Self::Uninitialized(_) => Err(protocol(pc, "unfinished token has a null identity")),
            Self::Ordinary(v) => Err(ExecutionError::new(
                Some(pc),
                E::WrongValueType {
                    expected: ValueKind::Reference,
                    actual: v.kind(),
                },
            )),
        }
    }
    fn site(self, method: MethodId, pc: u32) -> bool {
        matches!(self,Self::Uninitialized(Uninitialized { key: AliasKey::Allocation { method: site,offset }, .. }) if method==site && offset==pc)
    }
}
#[cfg(test)]
impl PartialEq<Value> for FrameValue {
    fn eq(&self, v: &Value) -> bool {
        matches!(self,Self::Ordinary(value) if value==v)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalSlot {
    Empty,
    Value(FrameValue),
    Continuation { head: usize },
}

pub(crate) struct Locals {
    slots: Vec<LocalSlot>,
}
impl Locals {
    pub fn new(capacity: u16) -> Self {
        Self {
            slots: vec![LocalSlot::Empty; usize::from(capacity)],
        }
    }
    fn bounds(&self, index: usize, width: usize, offset: u32) -> Result<(), ExecutionError> {
        if index
            .checked_add(width)
            .is_some_and(|end| end <= self.slots.len())
        {
            Ok(())
        } else {
            Err(ExecutionError::new(
                Some(offset),
                E::LocalOutOfBounds {
                    index,
                    width,
                    capacity: self.slots.len(),
                },
            ))
        }
    }
    fn invalidate(&mut self, index: usize) {
        match self.slots.get(index).copied() {
            Some(LocalSlot::Value(value)) if value.slots() == 2 => {
                if let Some(slot) = self.slots.get_mut(index + 1) {
                    *slot = LocalSlot::Empty;
                }
            }
            Some(LocalSlot::Continuation { head }) => {
                if let Some(slot) = self.slots.get_mut(head) {
                    *slot = LocalSlot::Empty;
                }
            }
            _ => {}
        }
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = LocalSlot::Empty;
        }
    }
    pub fn store(&mut self, index: usize, value: Value, offset: u32) -> Result<(), ExecutionError> {
        self.store_frame(index, FrameValue::Ordinary(value), offset)
    }
    pub fn store_frame(
        &mut self,
        index: usize,
        value: FrameValue,
        offset: u32,
    ) -> Result<(), ExecutionError> {
        let width = value.slots();
        self.bounds(index, width, offset)?;
        self.invalidate(index);
        if width == 2 {
            self.invalidate(index + 1);
        }
        self.slots[index] = LocalSlot::Value(value);
        if width == 2 {
            self.slots[index + 1] = LocalSlot::Continuation { head: index };
        }
        Ok(())
    }
    pub fn load(
        &self,
        index: usize,
        expected: ValueKind,
        offset: u32,
    ) -> Result<Value, ExecutionError> {
        self.load_frame(index, expected, offset)?.ordinary(offset)
    }
    pub fn load_reference(&self, index: usize, offset: u32) -> Result<FrameValue, ExecutionError> {
        self.load_frame(index, ValueKind::Reference, offset)
    }
    fn load_frame(
        &self,
        index: usize,
        expected: ValueKind,
        offset: u32,
    ) -> Result<FrameValue, ExecutionError> {
        self.bounds(index, expected.slots(), offset)?;
        let Some(LocalSlot::Value(value)) = self.slots.get(index).copied() else {
            return Err(ExecutionError::new(
                Some(offset),
                E::LocalNotValue { index },
            ));
        };
        if value.kind() != expected {
            return Err(ExecutionError::new(
                Some(offset),
                E::WrongValueType {
                    expected,
                    actual: value.kind(),
                },
            ));
        }
        if value.slots() == 2
            && self.slots.get(index + 1) != Some(&LocalSlot::Continuation { head: index })
        {
            return Err(ExecutionError::new(
                Some(offset),
                E::LocalNotValue { index },
            ));
        }
        Ok(value)
    }
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn rewrite_alias(&mut self, alias: Uninitialized, promote: bool) {
        for slot in &mut self.slots {
            if *slot == LocalSlot::Value(FrameValue::Uninitialized(alias)) {
                *slot = if promote {
                    LocalSlot::Value(FrameValue::Ordinary(Value::Reference(alias.reference)))
                } else {
                    LocalSlot::Empty
                };
            }
        }
    }
    pub fn invalidate_site(&mut self, method: MethodId, pc: u32) {
        for slot in &mut self.slots {
            if matches!(slot,LocalSlot::Value(v) if v.site(method,pc)) {
                *slot = LocalSlot::Empty;
            }
        }
    }
}

pub(crate) struct Stack {
    values: Vec<FrameValue>,
    slots: usize,
    capacity: usize,
}
impl Stack {
    pub fn new(capacity: u16) -> Self {
        Self {
            values: Vec::new(),
            slots: 0,
            capacity: usize::from(capacity),
        }
    }
    pub fn push(&mut self, value: Value, offset: u32) -> Result<(), ExecutionError> {
        self.push_frame(FrameValue::Ordinary(value), offset)
    }
    pub fn push_frame(&mut self, value: FrameValue, offset: u32) -> Result<(), ExecutionError> {
        let required = self.slots + value.slots();
        if required > self.capacity {
            return Err(ExecutionError::new(
                Some(offset),
                E::StackOverflow {
                    required,
                    capacity: self.capacity,
                },
            ));
        }
        self.values.push(value);
        self.slots = required;
        Ok(())
    }
    pub fn install_exception(
        &mut self,
        reference: crabspot_runtime::Reference,
        offset: u32,
    ) -> Result<(), ExecutionError> {
        if self.capacity < 1 {
            return Err(ExecutionError::new(
                Some(offset),
                E::StackOverflow {
                    required: 1,
                    capacity: self.capacity,
                },
            ));
        }
        if self.values.capacity() < 1 {
            self.values.try_reserve_exact(1).map_err(|_| {
                ExecutionError::new(
                    Some(offset),
                    E::Heap(crabspot_runtime::HeapError::AllocationFailure {
                        resource: "exception handler operand stack",
                    }),
                )
            })?;
        }
        self.values.clear();
        self.values
            .push(FrameValue::Ordinary(Value::Reference(reference)));
        self.slots = 1;
        Ok(())
    }
    fn peek(&self, depth: usize, offset: u32) -> Result<FrameValue, ExecutionError> {
        self.values
            .len()
            .checked_sub(depth + 1)
            .and_then(|index| self.values.get(index))
            .copied()
            .ok_or_else(|| ExecutionError::new(Some(offset), E::StackUnderflow))
    }
    pub fn pop_any(&mut self, offset: u32) -> Result<Value, ExecutionError> {
        let value = self.peek(0, offset)?.ordinary(offset)?;
        self.values.pop();
        self.slots -= value.slots();
        Ok(value)
    }
    pub fn pop(&mut self, expected: ValueKind, offset: u32) -> Result<Value, ExecutionError> {
        let value = self.peek(0, offset)?;
        if value.kind() != expected {
            return Err(ExecutionError::new(
                Some(offset),
                E::WrongValueType {
                    expected,
                    actual: value.kind(),
                },
            ));
        }
        let ordinary = value.ordinary(offset)?;
        self.values.pop();
        self.slots -= value.slots();
        Ok(ordinary)
    }
    pub fn int(&mut self, offset: u32) -> Result<i32, ExecutionError> {
        match self.pop(ValueKind::Int, offset)? {
            Value::Int(value) => Ok(value),
            _ => Err(ExecutionError::new(Some(offset), E::InvalidOperands)),
        }
    }
    pub fn long(&mut self, offset: u32) -> Result<i64, ExecutionError> {
        match self.pop(ValueKind::Long, offset)? {
            Value::Long(value) => Ok(value),
            _ => Err(ExecutionError::new(Some(offset), E::InvalidOperands)),
        }
    }
    pub fn float(&mut self, offset: u32) -> Result<f32, ExecutionError> {
        match self.pop(ValueKind::Float, offset)? {
            Value::Float(bits) => Ok(f32::from_bits(bits)),
            _ => Err(ExecutionError::new(Some(offset), E::InvalidOperands)),
        }
    }
    pub fn double(&mut self, offset: u32) -> Result<f64, ExecutionError> {
        match self.pop(ValueKind::Double, offset)? {
            Value::Double(bits) => Ok(f64::from_bits(bits)),
            _ => Err(ExecutionError::new(Some(offset), E::InvalidOperands)),
        }
    }
    pub fn reference(
        &mut self,
        offset: u32,
    ) -> Result<crabspot_runtime::Reference, ExecutionError> {
        match self.pop(ValueKind::Reference, offset)? {
            Value::Reference(value) => Ok(value),
            _ => Err(ExecutionError::new(Some(offset), E::InvalidOperands)),
        }
    }

    pub fn pop_reference(&mut self, pc: u32) -> Result<FrameValue, ExecutionError> {
        let v = self.peek(0, pc)?;
        if v.kind() != ValueKind::Reference {
            return Err(ExecutionError::new(
                Some(pc),
                E::WrongValueType {
                    expected: ValueKind::Reference,
                    actual: v.kind(),
                },
            ));
        }
        self.values.pop();
        self.slots -= 1;
        Ok(v)
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn reserve_one(&mut self, pc: u32) -> Result<(), ExecutionError> {
        let required = self
            .slots
            .checked_add(1)
            .ok_or_else(|| protocol(pc, "operand slot arithmetic overflow"))?;
        if required > self.capacity {
            return Err(ExecutionError::new(
                Some(pc),
                E::StackOverflow {
                    required,
                    capacity: self.capacity,
                },
            ));
        }
        self.values.try_reserve(1).map_err(|_| {
            ExecutionError::new(
                Some(pc),
                E::Heap(crabspot_runtime::HeapError::AllocationFailure {
                    resource: "construction operand stack",
                }),
            )
        })
    }
    pub fn publish(&mut self, v: Uninitialized) {
        self.values.push(FrameValue::Uninitialized(v));
        self.slots += 1;
    }
    pub fn slot_count(&self) -> usize {
        self.slots
    }
    pub fn check_reserved_one(&self, pc: u32) -> Result<(), ExecutionError> {
        let required = self
            .slots
            .checked_add(1)
            .ok_or_else(|| protocol(pc, "native operand slot overflow"))?;
        if required > self.capacity || self.values.len() >= self.values.capacity() {
            return Err(protocol(pc, "native result reservation was lost"));
        }
        Ok(())
    }
    /// Private commit after reserve_one on the unchanged suspended caller stack.
    pub fn publish_reserved_int(&mut self, value: i32) {
        self.values.push(FrameValue::Ordinary(Value::Int(value)));
        self.slots += 1;
    }
    pub fn publish_reserved_reference(&mut self, value: Reference) {
        self.values
            .push(FrameValue::Ordinary(Value::Reference(value)));
        self.slots += 1;
    }
    pub fn contains_site(&self, method: MethodId, pc: u32) -> bool {
        self.values.iter().any(|v| v.site(method, pc))
    }
    pub fn rewrite_alias(&mut self, alias: Uninitialized, promote: bool) {
        if promote {
            for v in &mut self.values {
                if *v == FrameValue::Uninitialized(alias) {
                    *v = FrameValue::Ordinary(Value::Reference(alias.reference));
                }
            }
        } else {
            let mut removed = 0;
            self.values.retain(|value| {
                if *value == FrameValue::Uninitialized(alias) {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
            self.slots -= removed;
        }
    }
    fn replace(
        &mut self,
        count: usize,
        values: &[FrameValue],
        offset: u32,
    ) -> Result<(), ExecutionError> {
        let Some(start) = self.values.len().checked_sub(count) else {
            return Err(ExecutionError::new(Some(offset), E::StackUnderflow));
        };
        let removed: usize = self.values[start..].iter().map(|value| value.slots()).sum();
        let required =
            self.slots - removed + values.iter().map(|value| value.slots()).sum::<usize>();
        if required > self.capacity {
            return Err(ExecutionError::new(
                Some(offset),
                E::StackOverflow {
                    required,
                    capacity: self.capacity,
                },
            ));
        }
        self.values.truncate(start);
        self.values.extend_from_slice(values);
        self.slots = required;
        Ok(())
    }
    pub fn manipulate(&mut self, opcode: u8, offset: u32) -> Result<(), ExecutionError> {
        let bad = || ExecutionError::new(Some(offset), E::InvalidStackCategory { opcode });
        let v1 = self.peek(0, offset)?;
        match opcode {
            0x57 => {
                if v1.slots() != 1 {
                    return Err(bad());
                }
                self.replace(1, &[], offset)
            }
            0x58 => {
                if v1.slots() == 2 {
                    self.replace(1, &[], offset)
                } else {
                    let v2 = self.peek(1, offset)?;
                    if v2.slots() != 1 {
                        return Err(bad());
                    }
                    self.replace(2, &[], offset)
                }
            }
            0x59 => {
                if v1.slots() != 1 {
                    return Err(bad());
                }
                self.replace(1, &[v1, v1], offset)
            }
            0x5a => {
                let v2 = self.peek(1, offset)?;
                if v1.slots() != 1 || v2.slots() != 1 {
                    return Err(bad());
                }
                self.replace(2, &[v1, v2, v1], offset)
            }
            0x5b => {
                if v1.slots() != 1 {
                    return Err(bad());
                }
                let v2 = self.peek(1, offset)?;
                if v2.slots() == 2 {
                    self.replace(2, &[v1, v2, v1], offset)
                } else {
                    let v3 = self.peek(2, offset)?;
                    if v3.slots() != 1 {
                        return Err(bad());
                    }
                    self.replace(3, &[v1, v3, v2, v1], offset)
                }
            }
            0x5c => {
                if v1.slots() == 2 {
                    self.replace(1, &[v1, v1], offset)
                } else {
                    let v2 = self.peek(1, offset)?;
                    if v2.slots() != 1 {
                        return Err(bad());
                    }
                    self.replace(2, &[v2, v1, v2, v1], offset)
                }
            }
            0x5d => {
                let v2 = self.peek(1, offset)?;
                if v1.slots() == 2 {
                    if v2.slots() != 1 {
                        return Err(bad());
                    }
                    self.replace(2, &[v1, v2, v1], offset)
                } else {
                    let v3 = self.peek(2, offset)?;
                    if v2.slots() != 1 || v3.slots() != 1 {
                        return Err(bad());
                    }
                    self.replace(3, &[v2, v1, v3, v2, v1], offset)
                }
            }
            0x5e => {
                let v2 = self.peek(1, offset)?;
                if v1.slots() == 2 {
                    if v2.slots() == 2 {
                        self.replace(2, &[v1, v2, v1], offset)
                    } else {
                        let v3 = self.peek(2, offset)?;
                        if v3.slots() != 1 {
                            return Err(bad());
                        }
                        self.replace(3, &[v1, v3, v2, v1], offset)
                    }
                } else {
                    if v2.slots() != 1 {
                        return Err(bad());
                    }
                    let v3 = self.peek(2, offset)?;
                    if v3.slots() == 2 {
                        self.replace(3, &[v2, v1, v3, v2, v1], offset)
                    } else {
                        let v4 = self.peek(3, offset)?;
                        if v4.slots() != 1 {
                            return Err(bad());
                        }
                        self.replace(4, &[v2, v1, v4, v3, v2, v1], offset)
                    }
                }
            }
            0x5f => {
                let v2 = self.peek(1, offset)?;
                if v1.slots() != 1 || v2.slots() != 1 {
                    return Err(bad());
                }
                self.replace(2, &[v1, v2], offset)
            }
            _ => Err(ExecutionError::new(
                Some(offset),
                E::UnsupportedInstruction(opcode),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_slot_reference_data_preserves_generic_frame_width_and_overlap_rules() {
        let reference = Value::Reference(crabspot_runtime::Reference::null());
        for wide in [Value::Long(1), Value::Double(0x7ff8_0000_0000_0042)] {
            for overwritten in [0, 1] {
                let mut locals = Locals::new(2);
                locals.store(0, wide, 0).unwrap();
                locals.store(overwritten, reference, 0).unwrap();
                assert!(locals.load(0, wide.kind(), 0).is_err());
                assert_eq!(
                    locals.load(overwritten, ValueKind::Reference, 0).unwrap(),
                    reference
                );
                assert!(
                    locals
                        .load(1 - overwritten, ValueKind::Reference, 0)
                        .is_err()
                );
            }
        }
        let mut stack = Stack::new(2);
        stack.push(reference, 0).unwrap();
        stack.manipulate(0x59, 0).unwrap();
        assert_eq!(stack.slots, 2);
        assert_eq!(stack.pop(ValueKind::Reference, 0).unwrap(), reference);
        assert_eq!(stack.pop(ValueKind::Reference, 0).unwrap(), reference);
    }
    #[test]
    fn double_local_overlap_invalidates_mixed_category_two_pairs() {
        let mut locals = Locals::new(5);
        locals
            .store(0, Value::Double(0x7ff8_0000_0000_0042), 0)
            .unwrap();
        locals.store(1, Value::Float(0x7fc1_2345), 0).unwrap();
        assert!(locals.load(0, ValueKind::Double, 0).is_err());
        locals.store(0, Value::Double(1), 0).unwrap();
        locals.store(0, Value::Int(1), 0).unwrap();
        assert!(locals.load(1, ValueKind::Float, 0).is_err());
        locals.store(0, Value::Double(1), 0).unwrap();
        locals.store(2, Value::Long(2), 0).unwrap();
        locals.store(1, Value::Double(3), 0).unwrap();
        assert!(locals.load(0, ValueKind::Int, 0).is_err());
        assert!(locals.load(3, ValueKind::Float, 0).is_err());
        assert_eq!(
            locals.load(1, ValueKind::Double, 0).unwrap(),
            Value::Double(3)
        );
        let mut stack = Stack::new(4);
        stack.push(Value::Double(0x7ff8_0000_0000_0042), 0).unwrap();
        stack.manipulate(0x5c, 0).unwrap();
        assert_eq!(stack.values, vec![Value::Double(0x7ff8_0000_0000_0042); 2]);
    }
    #[test]
    fn locals_overwrite_either_half_and_overlapping_pairs() {
        let mut locals = Locals::new(5);
        locals.store(0, Value::Long(5), 0).unwrap();
        locals.store(1, Value::Int(7), 0).unwrap();
        assert!(matches!(
            locals.load(0, ValueKind::Long, 0).unwrap_err().kind,
            E::LocalNotValue { .. }
        ));
        assert_eq!(locals.load(1, ValueKind::Int, 0).unwrap(), Value::Int(7));
        locals.store(0, Value::Long(8), 0).unwrap();
        locals.store(0, Value::Int(9), 0).unwrap();
        assert!(matches!(
            locals.load(1, ValueKind::Int, 0).unwrap_err().kind,
            E::LocalNotValue { .. }
        ));
        locals.store(0, Value::Long(1), 0).unwrap();
        locals.store(2, Value::Long(2), 0).unwrap();
        locals.store(1, Value::Long(3), 0).unwrap();
        assert!(locals.load(0, ValueKind::Int, 0).is_err());
        assert!(locals.load(3, ValueKind::Int, 0).is_err());
        assert_eq!(locals.load(1, ValueKind::Long, 0).unwrap(), Value::Long(3));
    }
    #[test]
    fn locals_bounds_empty_tail_and_type_checks() {
        let mut locals = Locals::new(2);
        assert!(locals.load(0, ValueKind::Int, 0).is_err());
        assert!(locals.store(1, Value::Long(0), 0).is_err());
        assert!(locals.store(usize::MAX, Value::Int(0), 0).is_err());
        locals.store(0, Value::Long(0), 0).unwrap();
        assert!(locals.load(1, ValueKind::Int, 0).is_err());
        assert!(matches!(
            locals.load(0, ValueKind::Int, 0).unwrap_err().kind,
            E::WrongValueType { .. }
        ));
    }
    #[test]
    fn stack_slots_and_type_errors_are_checked() {
        let mut stack = Stack::new(2);
        assert!(stack.pop(ValueKind::Int, 7).is_err());
        stack.push(Value::Long(1), 0).unwrap();
        assert!(stack.push(Value::Int(1), 0).is_err());
        assert_eq!(stack.slots, 2);
        assert!(stack.pop(ValueKind::Int, 0).is_err());
        assert_eq!(stack.long(0).unwrap(), 1);
        assert_eq!(stack.slots, 0);
    }
    #[test]
    fn every_legal_stack_manipulation_form_preserves_value_order() {
        use Value::{Int as I, Long as L};
        let cases: Vec<(u8, Vec<Value>, Vec<Value>)> = vec![
            (0x57, vec![I(1)], vec![]),
            (0x58, vec![I(1), I(2)], vec![]),
            (0x58, vec![L(1)], vec![]),
            (0x59, vec![I(1)], vec![I(1), I(1)]),
            (0x5a, vec![I(2), I(1)], vec![I(1), I(2), I(1)]),
            (0x5b, vec![I(3), I(2), I(1)], vec![I(1), I(3), I(2), I(1)]),
            (0x5b, vec![L(2), I(1)], vec![I(1), L(2), I(1)]),
            (0x5c, vec![I(2), I(1)], vec![I(2), I(1), I(2), I(1)]),
            (0x5c, vec![L(1)], vec![L(1), L(1)]),
            (
                0x5d,
                vec![I(3), I(2), I(1)],
                vec![I(2), I(1), I(3), I(2), I(1)],
            ),
            (0x5d, vec![I(2), L(1)], vec![L(1), I(2), L(1)]),
            (
                0x5e,
                vec![I(4), I(3), I(2), I(1)],
                vec![I(2), I(1), I(4), I(3), I(2), I(1)],
            ),
            (
                0x5e,
                vec![L(3), I(2), I(1)],
                vec![I(2), I(1), L(3), I(2), I(1)],
            ),
            (0x5e, vec![I(3), I(2), L(1)], vec![L(1), I(3), I(2), L(1)]),
            (0x5e, vec![L(2), L(1)], vec![L(1), L(2), L(1)]),
            (0x5f, vec![I(2), I(1)], vec![I(1), I(2)]),
        ];
        for (opcode, before, after) in cases {
            let token = |value: Value| match value {
                Value::Int(value) => FrameValue::Uninitialized(Uninitialized {
                    reference: Reference::null(),
                    key: AliasKey::This(value as u64),
                }),
                value => FrameValue::Ordinary(value),
            };
            let mut private = Stack::new(16);
            for value in before.iter().copied() {
                private.push_frame(token(value), 0).unwrap();
            }
            private.manipulate(opcode, 0).unwrap();
            assert_eq!(
                private.values,
                after.iter().copied().map(token).collect::<Vec<_>>(),
                "private {opcode:x}"
            );
            let mut stack = Stack::new(16);
            for value in before {
                stack.push(value, 0).unwrap();
            }
            stack.manipulate(opcode, 0).unwrap();
            assert_eq!(stack.values, after, "{opcode:x}");
            assert_eq!(
                stack.slots,
                stack.values.iter().map(|value| value.slots()).sum()
            );
        }
    }
    #[test]
    fn stack_manipulation_rejects_invalid_categories_underflow_and_capacity() {
        for opcode in 0x57..=0x5f {
            let mut stack = Stack::new(2);
            assert!(stack.manipulate(opcode, 0).is_err());
        }
        for (opcode, values) in [
            (0x57, vec![Value::Long(0)]),
            (0x59, vec![Value::Long(0)]),
            (0x5a, vec![Value::Long(0), Value::Int(0)]),
            (0x5c, vec![Value::Long(0), Value::Int(0)]),
            (0x5f, vec![Value::Int(0), Value::Long(0)]),
        ] {
            let mut stack = Stack::new(8);
            for value in values {
                stack.push(value, 0).unwrap();
            }
            assert!(matches!(
                stack.manipulate(opcode, 0).unwrap_err().kind,
                E::InvalidStackCategory { .. }
            ));
        }
        let mut stack = Stack::new(1);
        stack.push(Value::Int(0), 0).unwrap();
        assert!(matches!(
            stack.manipulate(0x59, 0).unwrap_err().kind,
            E::StackOverflow { .. }
        ));
    }
    #[test]
    fn exception_installation_resets_slots_and_preserves_stack_before_capacity_failure() {
        let mut stack = Stack::new(2);
        stack.push(Value::Long(19), 0).unwrap();
        stack.capacity = 0;
        assert_eq!(
            stack
                .install_exception(crabspot_runtime::Reference::null(), 7)
                .unwrap_err()
                .kind,
            E::StackOverflow {
                required: 1,
                capacity: 0
            }
        );
        assert_eq!(stack.values, vec![Value::Long(19)]);
        assert_eq!(stack.slots, 2);
        stack.capacity = 2;
        stack
            .install_exception(crabspot_runtime::Reference::null(), 7)
            .unwrap();
        assert_eq!(
            stack.values,
            vec![Value::Reference(crabspot_runtime::Reference::null())]
        );
        assert_eq!(stack.slots, 1);
    }
    #[test]
    fn failed_alias_rewrite_keeps_mixed_category_two_survivor_slots_in_one_pass() {
        let alias = Uninitialized {
            reference: Reference::null(),
            key: AliasKey::This(1),
        };
        let other = Uninitialized {
            reference: Reference::null(),
            key: AliasKey::This(2),
        };
        let mut stack = Stack::new(8);
        for value in [
            FrameValue::Ordinary(Value::Long(7)),
            FrameValue::Uninitialized(alias),
            FrameValue::Ordinary(Value::Double(9)),
            FrameValue::Uninitialized(other),
            FrameValue::Uninitialized(alias),
        ] {
            stack.push_frame(value, 0).unwrap();
        }
        assert_eq!(stack.slots, 7);
        stack.rewrite_alias(alias, false);
        assert_eq!(stack.slots, 5);
        assert_eq!(
            stack.values,
            vec![
                FrameValue::Ordinary(Value::Long(7)),
                FrameValue::Ordinary(Value::Double(9)),
                FrameValue::Uninitialized(other)
            ]
        );
        stack.rewrite_alias(other, true);
        assert_eq!(stack.slots, 5);
        assert_eq!(
            stack.pop(ValueKind::Reference, 0).unwrap(),
            Value::Reference(Reference::null())
        );
        assert_eq!(stack.slots, 4);
    }
}

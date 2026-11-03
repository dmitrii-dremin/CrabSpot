use crate::{ExecutionError, ExecutionErrorKind as E, Value, ValueKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalSlot {
    Empty,
    Value(Value),
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
}

pub(crate) struct Stack {
    values: Vec<Value>,
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
    fn peek(&self, depth: usize, offset: u32) -> Result<Value, ExecutionError> {
        self.values
            .len()
            .checked_sub(depth + 1)
            .and_then(|index| self.values.get(index))
            .copied()
            .ok_or_else(|| ExecutionError::new(Some(offset), E::StackUnderflow))
    }
    pub fn pop_any(&mut self, offset: u32) -> Result<Value, ExecutionError> {
        let value = self.peek(0, offset)?;
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
        self.values.pop();
        self.slots -= value.slots();
        Ok(value)
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
    fn replace(
        &mut self,
        count: usize,
        values: &[Value],
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
}

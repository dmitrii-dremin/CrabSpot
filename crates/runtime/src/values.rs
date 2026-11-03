use crate::Reference;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Int(i32),
    Long(i64),
    /// Raw IEEE binary32 bits. Equality compares representations, not Java numbers.
    Float(u32),
    /// Raw IEEE binary64 bits. Equality compares representations, not Java numbers.
    Double(u64),
    Reference(Reference),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueKind {
    Int,
    Long,
    Float,
    Double,
    Reference,
}
impl ValueKind {
    /// JVM execution-frame width. Typed storage locations count values instead.
    pub fn slots(self) -> usize {
        if matches!(self, Self::Long | Self::Double) {
            2
        } else {
            1
        }
    }
}
impl Value {
    pub fn slots(self) -> usize {
        self.kind().slots()
    }
    pub fn kind(self) -> ValueKind {
        match self {
            Self::Int(_) => ValueKind::Int,
            Self::Long(_) => ValueKind::Long,
            Self::Float(_) => ValueKind::Float,
            Self::Double(_) => ValueKind::Double,
            Self::Reference(_) => ValueKind::Reference,
        }
    }
    pub fn from_f32(value: f32) -> Self {
        Self::Float(value.to_bits())
    }
    pub fn from_f64(value: f64) -> Self {
        Self::Double(value.to_bits())
    }
    pub fn as_f32(self) -> Option<f32> {
        if let Self::Float(bits) = self {
            Some(f32::from_bits(bits))
        } else {
            None
        }
    }
    pub fn as_f64(self) -> Option<f64> {
        if let Self::Double(bits) = self {
            Some(f64::from_bits(bits))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_is_a_distinct_one_slot_value_without_a_heap_identity() {
        let reference = Reference::null();
        assert_eq!(reference, Reference::default());
        assert!(reference.is_null());
        assert_eq!(reference.heap_ref(), None);
        let value = Value::Reference(reference);
        assert_eq!(value.kind(), ValueKind::Reference);
        assert_eq!(value.slots(), 1);
        assert_eq!(ValueKind::Reference.slots(), 1);
        assert!(value.as_f32().is_none());
        assert!(value.as_f64().is_none());
        assert_ne!(value, Value::Int(0));
    }
}

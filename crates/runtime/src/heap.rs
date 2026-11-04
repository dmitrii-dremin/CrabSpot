//! A bounded, noncollecting arena of primitive/reference arrays and ordinary instances. Handles survive arena
//! relocation; this service does not issue Java exception objects or array mirrors.
use crate::instance::{InstanceOptions, InstancePayload, InstanceState};
use crate::{HeapRef, Reference, Value, ValueKind};
use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "multidimensional.rs"]
mod multidimensional;

static NEXT_HEAP: AtomicU64 = AtomicU64::new(1);
pub(crate) const LIVE_GENERATION: u64 = 1;
fn allocate_identity(allocator: &AtomicU64) -> Result<u64, HeapError> {
    let mut current = allocator.load(Ordering::Relaxed);
    loop {
        let next = if current == 0 {
            None
        } else {
            current.checked_add(1)
        }
        .ok_or(HeapError::IdentityExhausted)?;
        match allocator.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(current),
            Err(observed) => current = observed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum PrimitiveArrayKind {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
}
impl PrimitiveArrayKind {
    pub fn value_kind(self) -> ValueKind {
        match self {
            Self::Boolean | Self::Byte | Self::Char | Self::Short | Self::Int => ValueKind::Int,
            Self::Long => ValueKind::Long,
            Self::Float => ValueKind::Float,
            Self::Double => ValueKind::Double,
        }
    }
    pub fn element_bytes(self) -> usize {
        match self {
            Self::Boolean | Self::Byte => 1,
            Self::Char | Self::Short => 2,
            Self::Int | Self::Float => 4,
            Self::Long | Self::Double => 8,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeapOptions {
    pub max_arrays: usize,
    pub max_array_length: usize,
    pub max_total_elements: usize,
    pub max_payload_bytes: usize,
}
impl Default for HeapOptions {
    fn default() -> Self {
        Self {
            max_arrays: 4096,
            max_array_length: 1_048_576,
            max_total_elements: 4_194_304,
            max_payload_bytes: 33_554_432,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapResource {
    Arrays,
    ArrayLength,
    TotalElements,
    PayloadBytes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapEntryKind {
    PrimitiveArray,
    ReferenceArray,
    /// Expected kind for generic length; never an actual entry payload kind.
    Array,
    Instance,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HeapError {
    NullReference,
    WrongEntryKind {
        expected: HeapEntryKind,
        actual: HeapEntryKind,
    },
    ForeignHeap,
    MissingEntry {
        index: usize,
    },
    StaleGeneration {
        expected: u64,
        actual: u64,
    },
    NegativeLength {
        length: i32,
    },
    IndexOutOfBounds {
        index: i32,
        length: i32,
    },
    ValueKindMismatch {
        expected: ValueKind,
        actual: ValueKind,
    },
    ResourceLimit {
        resource: HeapResource,
        limit: usize,
        required: usize,
    },
    IdentityExhausted,
    AllocationFailure {
        resource: &'static str,
    },
    InvalidState(&'static str),
}
impl fmt::Display for HeapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NullReference => f.write_str("null reference does not identify a heap entry"),
            Self::WrongEntryKind { expected, actual } => {
                write!(f, "expected {expected:?}, found {actual:?}")
            }
            Self::ForeignHeap => f.write_str("reference belongs to another heap"),
            Self::MissingEntry { index } => write!(f, "heap entry {index} does not exist"),
            Self::StaleGeneration { expected, actual } => write!(
                f,
                "heap generation mismatch: expected {expected}, found {actual}"
            ),
            Self::NegativeLength { length } => write!(f, "negative array length {length}"),
            Self::IndexOutOfBounds { index, length } => {
                write!(f, "array index {index} is outside length {length}")
            }
            Self::ValueKindMismatch { expected, actual } => {
                write!(f, "store requires {expected:?}, received {actual:?}")
            }
            Self::ResourceLimit {
                resource,
                limit,
                required,
            } => write!(
                f,
                "heap {resource:?} limit {limit} exceeded (required {required})"
            ),
            Self::IdentityExhausted => f.write_str("heap identity namespace exhausted"),
            Self::AllocationFailure { resource } => {
                write!(f, "host allocation failed for {resource}")
            }
            Self::InvalidState(reason) => write!(f, "invalid heap state: {reason}"),
        }
    }
}
impl std::error::Error for HeapError {}

#[derive(Debug)]
pub(crate) enum Payload {
    Boolean(Vec<u8>),
    Byte(Vec<i8>),
    Char(Vec<u16>),
    Short(Vec<i16>),
    Int(Vec<i32>),
    Long(Vec<i64>),
    Float(Vec<u32>),
    Double(Vec<u64>),
}
fn zeroed<T: Default>(length: usize) -> Result<Vec<T>, HeapError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| HeapError::AllocationFailure {
            resource: "primitive-array payload",
        })?;
    values.resize_with(length, T::default);
    Ok(values)
}
impl Payload {
    fn new(kind: PrimitiveArrayKind, length: usize) -> Result<Self, HeapError> {
        Ok(match kind {
            PrimitiveArrayKind::Boolean => Self::Boolean(zeroed(length)?),
            PrimitiveArrayKind::Byte => Self::Byte(zeroed(length)?),
            PrimitiveArrayKind::Char => Self::Char(zeroed(length)?),
            PrimitiveArrayKind::Short => Self::Short(zeroed(length)?),
            PrimitiveArrayKind::Int => Self::Int(zeroed(length)?),
            PrimitiveArrayKind::Long => Self::Long(zeroed(length)?),
            PrimitiveArrayKind::Float => Self::Float(zeroed(length)?),
            PrimitiveArrayKind::Double => Self::Double(zeroed(length)?),
        })
    }
    fn kind(&self) -> PrimitiveArrayKind {
        match self {
            Self::Boolean(_) => PrimitiveArrayKind::Boolean,
            Self::Byte(_) => PrimitiveArrayKind::Byte,
            Self::Char(_) => PrimitiveArrayKind::Char,
            Self::Short(_) => PrimitiveArrayKind::Short,
            Self::Int(_) => PrimitiveArrayKind::Int,
            Self::Long(_) => PrimitiveArrayKind::Long,
            Self::Float(_) => PrimitiveArrayKind::Float,
            Self::Double(_) => PrimitiveArrayKind::Double,
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::Boolean(values) => values.len(),
            Self::Byte(values) => values.len(),
            Self::Char(values) => values.len(),
            Self::Short(values) => values.len(),
            Self::Int(values) => values.len(),
            Self::Long(values) => values.len(),
            Self::Float(values) => values.len(),
            Self::Double(values) => values.len(),
        }
    }
    fn length(&self) -> Result<i32, HeapError> {
        i32::try_from(self.len())
            .map_err(|_| HeapError::InvalidState("array length exceeds i32 representation"))
    }
    fn index(&self, index: i32) -> Result<usize, HeapError> {
        usize::try_from(index)
            .ok()
            .filter(|index| *index < self.len())
            .ok_or(HeapError::IndexOutOfBounds {
                index,
                length: self.length()?,
            })
    }
    fn load(&self, index: usize) -> Result<Value, HeapError> {
        let value = match self {
            Self::Boolean(values) => values.get(index).map(|value| Value::Int(i32::from(*value))),
            Self::Byte(values) => values.get(index).map(|value| Value::Int(i32::from(*value))),
            Self::Char(values) => values.get(index).map(|value| Value::Int(i32::from(*value))),
            Self::Short(values) => values.get(index).map(|value| Value::Int(i32::from(*value))),
            Self::Int(values) => values.get(index).copied().map(Value::Int),
            Self::Long(values) => values.get(index).copied().map(Value::Long),
            Self::Float(values) => values.get(index).copied().map(Value::Float),
            Self::Double(values) => values.get(index).copied().map(Value::Double),
        };
        value.ok_or(HeapError::InvalidState(
            "validated array index is missing from payload",
        ))
    }
    fn store(&mut self, index: usize, value: Value) -> Result<(), HeapError> {
        let expected = self.kind().value_kind();
        if value.kind() != expected {
            return Err(HeapError::ValueKindMismatch {
                expected,
                actual: value.kind(),
            });
        }
        let missing = HeapError::InvalidState("validated array index is missing from payload");
        match (self, value) {
            (Self::Boolean(values), Value::Int(value)) => {
                *values.get_mut(index).ok_or(missing)? = (value & 1) as u8
            }
            (Self::Byte(values), Value::Int(value)) => {
                *values.get_mut(index).ok_or(missing)? = value as i8
            }
            (Self::Char(values), Value::Int(value)) => {
                *values.get_mut(index).ok_or(missing)? = value as u16
            }
            (Self::Short(values), Value::Int(value)) => {
                *values.get_mut(index).ok_or(missing)? = value as i16
            }
            (Self::Int(values), Value::Int(value)) => {
                *values.get_mut(index).ok_or(missing)? = value
            }
            (Self::Long(values), Value::Long(value)) => {
                *values.get_mut(index).ok_or(missing)? = value
            }
            (Self::Float(values), Value::Float(value)) => {
                *values.get_mut(index).ok_or(missing)? = value
            }
            (Self::Double(values), Value::Double(value)) => {
                *values.get_mut(index).ok_or(missing)? = value
            }
            _ => {
                return Err(HeapError::InvalidState(
                    "validated value kind does not match primitive payload",
                ));
            }
        }
        Ok(())
    }
}
#[derive(Debug)]
pub(crate) enum EntryPayload {
    Array(Payload),
    ReferenceArray(crate::reference_array::ReferenceArrayPayload),
    Instance(InstancePayload),
}
impl EntryPayload {
    fn array(&self) -> Result<&Payload, HeapError> {
        match self {
            Self::Array(payload) => Ok(payload),
            Self::ReferenceArray(_) => Err(HeapError::WrongEntryKind {
                expected: HeapEntryKind::PrimitiveArray,
                actual: HeapEntryKind::ReferenceArray,
            }),
            Self::Instance(_) => Err(HeapError::WrongEntryKind {
                expected: HeapEntryKind::PrimitiveArray,
                actual: HeapEntryKind::Instance,
            }),
        }
    }
    fn array_mut(&mut self) -> Result<&mut Payload, HeapError> {
        match self {
            Self::Array(payload) => Ok(payload),
            Self::ReferenceArray(_) => Err(HeapError::WrongEntryKind {
                expected: HeapEntryKind::PrimitiveArray,
                actual: HeapEntryKind::ReferenceArray,
            }),
            Self::Instance(_) => Err(HeapError::WrongEntryKind {
                expected: HeapEntryKind::PrimitiveArray,
                actual: HeapEntryKind::Instance,
            }),
        }
    }
}
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) generation: u64,
    pub(crate) payload: EntryPayload,
}
pub(crate) fn reserve_entries(
    entries: &mut Vec<Entry>,
    additional: usize,
) -> Result<(), HeapError> {
    entries
        .try_reserve(additional)
        .map_err(|_| HeapError::AllocationFailure {
            resource: "managed heap arena",
        })
}
fn increased(
    current: usize,
    amount: usize,
    limit: usize,
    resource: HeapResource,
) -> Result<usize, HeapError> {
    let required = current
        .checked_add(amount)
        .ok_or(HeapError::ResourceLimit {
            resource,
            limit,
            required: usize::MAX,
        })?;
    if required > limit {
        Err(HeapError::ResourceLimit {
            resource,
            limit,
            required,
        })
    } else {
        Ok(required)
    }
}
fn payload_size(length: usize, kind: PrimitiveArrayKind, limit: usize) -> Result<usize, HeapError> {
    length
        .checked_mul(kind.element_bytes())
        .ok_or(HeapError::ResourceLimit {
            resource: HeapResource::PayloadBytes,
            limit,
            required: usize::MAX,
        })
}

/// A noncollecting arena. There is no entry free/reuse API and no caller root set.
#[derive(Debug)]
pub struct ManagedHeap {
    pub(crate) identity: u64,
    options: HeapOptions,
    pub(crate) entries: Vec<Entry>,
    pub(crate) instances: Option<InstanceState>,
    pub(crate) reference_universe: Option<crate::ReferenceTypeUniverse>,
    arrays: usize,
    elements: usize,
    bytes: usize,
}
impl ManagedHeap {
    pub fn new(options: HeapOptions) -> Result<Self, HeapError> {
        Ok(Self {
            identity: allocate_identity(&NEXT_HEAP)?,
            options,
            entries: Vec::new(),
            instances: None,
            reference_universe: None,
            arrays: 0,
            elements: 0,
            bytes: 0,
        })
    }
    pub fn new_with_instances(
        options: HeapOptions,
        instance_options: InstanceOptions,
    ) -> Result<Self, HeapError> {
        let mut heap = Self::new(options)?;
        heap.instances = Some(InstanceState::new(instance_options));
        Ok(heap)
    }
    pub fn allocated_arrays(&self) -> usize {
        self.arrays
    }
    pub fn total_elements(&self) -> usize {
        self.elements
    }
    /// Typed element contents only; excludes arena and allocator overhead.
    pub fn payload_bytes(&self) -> usize {
        self.bytes
    }
    pub fn allocate_primitive_array(
        &mut self,
        kind: PrimitiveArrayKind,
        length: i32,
    ) -> Result<Reference, HeapError> {
        let size = usize::try_from(length).map_err(|_| HeapError::NegativeLength { length })?;
        let arrays = increased(
            self.arrays,
            1,
            self.options.max_arrays,
            HeapResource::Arrays,
        )?;
        increased(
            0,
            size,
            self.options.max_array_length,
            HeapResource::ArrayLength,
        )?;
        let elements = increased(
            self.elements,
            size,
            self.options.max_total_elements,
            HeapResource::TotalElements,
        )?;
        let payload_bytes = payload_size(size, kind, self.options.max_payload_bytes)?;
        let bytes = increased(
            self.bytes,
            payload_bytes,
            self.options.max_payload_bytes,
            HeapResource::PayloadBytes,
        )?;
        let payload = Payload::new(kind, size)?;
        reserve_entries(&mut self.entries, 1)?;
        let index = self.entries.len();
        self.entries.push(Entry {
            generation: LIVE_GENERATION,
            payload: EntryPayload::Array(payload),
        });
        self.arrays = arrays;
        self.elements = elements;
        self.bytes = bytes;
        // No fallible operation remains after entry publication; only the heap
        // issuer can construct this opaque non-null reference.
        Ok(Reference::issued(self.identity, index, LIVE_GENERATION))
    }
    pub(crate) fn checked_handle(&self, handle: HeapRef) -> Result<usize, HeapError> {
        let (heap, index, generation) = handle.components();
        if heap != self.identity {
            return Err(HeapError::ForeignHeap);
        }
        let entry = self
            .entries
            .get(index)
            .ok_or(HeapError::MissingEntry { index })?;
        if generation != entry.generation {
            return Err(HeapError::StaleGeneration {
                expected: entry.generation,
                actual: generation,
            });
        }
        Ok(index)
    }
    pub(crate) fn allocate_reference_array_payload(
        &mut self,
        array_type: crate::ReferenceType,
        length: i32,
    ) -> Result<Reference, HeapError> {
        use crate::reference_array::{ReferenceArrayPayload, reference_payload_size};
        ReferenceArrayPayload::validate_type(array_type)?;
        let size = usize::try_from(length).map_err(|_| HeapError::NegativeLength { length })?;
        let arrays = increased(
            self.arrays,
            1,
            self.options.max_arrays,
            HeapResource::Arrays,
        )?;
        increased(
            0,
            size,
            self.options.max_array_length,
            HeapResource::ArrayLength,
        )?;
        let elements = increased(
            self.elements,
            size,
            self.options.max_total_elements,
            HeapResource::TotalElements,
        )?;
        let payload_bytes = reference_payload_size(size, self.options.max_payload_bytes)?;
        let bytes = increased(
            self.bytes,
            payload_bytes,
            self.options.max_payload_bytes,
            HeapResource::PayloadBytes,
        )?;
        let payload = ReferenceArrayPayload::new(array_type, size)?;
        reserve_entries(&mut self.entries, 1)?;
        let index = self.entries.len();
        self.entries.push(Entry {
            generation: LIVE_GENERATION,
            payload: EntryPayload::ReferenceArray(payload),
        });
        self.arrays = arrays;
        self.elements = elements;
        self.bytes = bytes;
        Ok(Reference::issued(self.identity, index, LIVE_GENERATION))
    }
    pub(crate) fn entry(&self, reference: Reference) -> Result<&Entry, HeapError> {
        let handle = reference.heap_ref().ok_or(HeapError::NullReference)?;
        let index = self.checked_handle(handle)?;
        self.entries
            .get(index)
            .ok_or(HeapError::MissingEntry { index })
    }
    pub fn kind(&self, reference: Reference) -> Result<PrimitiveArrayKind, HeapError> {
        Ok(self.entry(reference)?.payload.array()?.kind())
    }
    pub fn length(&self, reference: Reference) -> Result<i32, HeapError> {
        match &self.entry(reference)?.payload {
            EntryPayload::Array(payload) => payload.length(),
            EntryPayload::ReferenceArray(payload) => payload.length(),
            EntryPayload::Instance(_) => Err(HeapError::WrongEntryKind {
                expected: HeapEntryKind::Array,
                actual: HeapEntryKind::Instance,
            }),
        }
    }
    pub fn load(&self, reference: Reference, index: i32) -> Result<Value, HeapError> {
        let payload = self.entry(reference)?.payload.array()?;
        payload.load(payload.index(index)?)
    }
    pub fn store(
        &mut self,
        reference: Reference,
        index: i32,
        value: Value,
    ) -> Result<(), HeapError> {
        let handle = reference.heap_ref().ok_or(HeapError::NullReference)?;
        let entry_index = self.checked_handle(handle)?;
        let entry = self
            .entries
            .get_mut(entry_index)
            .ok_or(HeapError::MissingEntry { index: entry_index })?;
        let payload = entry.payload.array_mut()?;
        let index = payload.index(index)?;
        payload.store(index, value)
    }
    /// Validate identity/bounds/generation only. This neither marks nor retains
    /// an allocation and does not substitute for execution/static/native roots.
    pub fn validate_root(&self, handle: HeapRef) -> Result<(), HeapError> {
        self.checked_handle(handle).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forged_handles_check_owner_index_and_generation_before_access() {
        let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        let reference = heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
            .unwrap();
        heap.store(reference, 0, Value::Int(42)).unwrap();
        let (owner, index, generation) = reference.heap_ref().unwrap().components();
        assert_ne!(generation, 0);
        for bad_index in [1, usize::MAX] {
            let forged = Reference::issued(owner, bad_index, generation);
            assert_eq!(
                heap.load(forged, 0).unwrap_err(),
                HeapError::MissingEntry { index: bad_index }
            );
            assert!(heap.validate_root(forged.heap_ref().unwrap()).is_err());
        }
        for actual in [0, 2, u64::MAX] {
            let forged = Reference::issued(owner, index, actual);
            assert_eq!(
                heap.store(forged, 0, Value::Int(1)).unwrap_err(),
                HeapError::StaleGeneration {
                    expected: 1,
                    actual
                }
            );
            assert!(heap.validate_root(forged.heap_ref().unwrap()).is_err());
        }
        assert_eq!(
            heap.kind(Reference::issued(0, index, generation))
                .unwrap_err(),
            HeapError::ForeignHeap
        );
        assert_eq!(heap.load(reference, 0).unwrap(), Value::Int(42));
    }
    #[test]
    fn heap_namespace_and_accounting_do_not_wrap_or_publish_on_overflow() {
        let allocator = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate_identity(&allocator).unwrap(), u64::MAX - 1);
        assert_eq!(
            allocate_identity(&allocator).unwrap_err(),
            HeapError::IdentityExhausted
        );
        assert_eq!(allocator.load(Ordering::Relaxed), u64::MAX);
        assert!(allocate_identity(&AtomicU64::new(0)).is_err());
        for resource in [
            HeapResource::Arrays,
            HeapResource::ArrayLength,
            HeapResource::TotalElements,
            HeapResource::PayloadBytes,
        ] {
            assert_eq!(
                increased(usize::MAX, 1, usize::MAX, resource).unwrap_err(),
                HeapError::ResourceLimit {
                    resource,
                    limit: usize::MAX,
                    required: usize::MAX
                }
            );
            assert_eq!(
                increased(usize::MAX, 0, usize::MAX, resource).unwrap(),
                usize::MAX
            );
        }
        assert_eq!(
            payload_size(usize::MAX, PrimitiveArrayKind::Double, usize::MAX).unwrap_err(),
            HeapError::ResourceLimit {
                resource: HeapResource::PayloadBytes,
                limit: usize::MAX,
                required: usize::MAX
            }
        );
        let mut heap = ManagedHeap::new(HeapOptions {
            max_total_elements: usize::MAX,
            max_payload_bytes: usize::MAX,
            ..Default::default()
        })
        .unwrap();
        heap.elements = usize::MAX;
        assert!(matches!(
            heap.allocate_primitive_array(PrimitiveArrayKind::Int, 1),
            Err(HeapError::ResourceLimit {
                resource: HeapResource::TotalElements,
                ..
            })
        ));
        assert_eq!(heap.allocated_arrays(), 0);
        assert_eq!(heap.elements, usize::MAX);
        assert_eq!(heap.bytes, 0);
        heap.elements = 0;
        heap.bytes = usize::MAX;
        assert!(matches!(
            heap.allocate_primitive_array(PrimitiveArrayKind::Long, 1),
            Err(HeapError::ResourceLimit {
                resource: HeapResource::PayloadBytes,
                ..
            })
        ));
        assert_eq!(heap.allocated_arrays(), 0);
        assert_eq!(heap.elements, 0);
        assert_eq!(heap.bytes, usize::MAX);
    }
    #[test]
    fn fallible_capacity_reservations_leave_live_data_and_logical_accounting_unchanged() {
        assert!(matches!(
            zeroed::<u64>(usize::MAX),
            Err(HeapError::AllocationFailure {
                resource: "primitive-array payload"
            })
        ));
        let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        let reference = heap
            .allocate_primitive_array(PrimitiveArrayKind::Long, 1)
            .unwrap();
        heap.store(reference, 0, Value::Long(123)).unwrap();
        assert_eq!(
            reserve_entries(&mut heap.entries, usize::MAX).unwrap_err(),
            HeapError::AllocationFailure {
                resource: "managed heap arena"
            }
        );
        assert_eq!(heap.allocated_arrays(), 1);
        assert_eq!(heap.total_elements(), 1);
        assert_eq!(heap.payload_bytes(), 8);
        assert_eq!(heap.load(reference, 0).unwrap(), Value::Long(123));
    }
    #[test]
    fn checked_heap_identity_allocator_is_unique_under_contention() {
        use std::{collections::HashSet, sync::Arc};
        let allocator = Arc::new(AtomicU64::new(1));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let allocator = allocator.clone();
                std::thread::spawn(move || {
                    (0..128)
                        .map(|_| allocate_identity(&allocator).unwrap())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let tokens: HashSet<_> = threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(tokens.len(), 1024);
        assert!(!tokens.contains(&0));
        assert_eq!(allocator.load(Ordering::Relaxed), 1025);
    }
}
#[test]
fn reference_array_checked_counter_overflows_and_arena_reservation_preserve_publication() {
    let array_type = crate::ReferenceType::Array {
        dimensions: 1,
        element: crate::ArrayElement::Class(crate::ClassId {
            registry: 1,
            index: 0,
        }),
        defining_loader: crate::LoaderId {
            registry: 1,
            index: 0,
        },
    };
    for resource in [
        HeapResource::Arrays,
        HeapResource::TotalElements,
        HeapResource::PayloadBytes,
    ] {
        let mut heap = ManagedHeap::new(HeapOptions {
            max_arrays: usize::MAX,
            max_array_length: usize::MAX,
            max_total_elements: usize::MAX,
            max_payload_bytes: usize::MAX,
        })
        .unwrap();
        let reference = heap
            .allocate_reference_array_payload(array_type, 1)
            .unwrap();
        match resource {
            HeapResource::Arrays => heap.arrays = usize::MAX,
            HeapResource::TotalElements => heap.elements = usize::MAX,
            HeapResource::PayloadBytes => heap.bytes = usize::MAX,
            _ => unreachable!(),
        }
        let before = (heap.entries.len(), heap.arrays, heap.elements, heap.bytes);
        assert_eq!(
            heap.allocate_reference_array_payload(array_type, 1),
            Err(HeapError::ResourceLimit {
                resource,
                limit: usize::MAX,
                required: usize::MAX
            })
        );
        assert_eq!(
            before,
            (heap.entries.len(), heap.arrays, heap.elements, heap.bytes)
        );
        assert_eq!(heap.length(reference), Ok(1));
        assert_eq!(heap.load_reference(reference, 0), Ok(Reference::null()));
        assert!(matches!(
            reserve_entries(&mut heap.entries, usize::MAX),
            Err(HeapError::AllocationFailure { .. })
        ));
        assert_eq!(
            before,
            (heap.entries.len(), heap.arrays, heap.elements, heap.bytes)
        );
        assert_eq!(heap.reference_array_type(reference), Ok(array_type));
    }
}

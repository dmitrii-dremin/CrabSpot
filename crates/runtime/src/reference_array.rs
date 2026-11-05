//! Typed, nonrecursive reference-array storage with context-owned assignment checks.
use crate::heap::EntryPayload;
use crate::{
    ArrayElement, FieldResolutionContext, HeapEntryKind, HeapError, ManagedHeap, Reference,
    ReferenceType, TypeAccessError, TypeJavaError,
};
use std::fmt;

/// Actual shallow Vec element representation; excludes headers/capacity/arena metadata.
pub const REFERENCE_ARRAY_ELEMENT_BYTES: usize = core::mem::size_of::<Reference>();
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArrayJavaError {
    NullPointer,
    IndexOutOfBounds,
    NegativeArraySize,
    ArrayStore,
    IllegalAccess,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArrayAccessError {
    Java(ArrayJavaError),
    Type(Box<TypeAccessError>),
    Heap(HeapError),
}
impl fmt::Display for ArrayAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Java(error) => write!(f, "reference array operation failed: {error:?}"),
            Self::Type(error) => write!(f, "reference array type validation failed: {error}"),
            Self::Heap(error) => write!(f, "reference array heap operation failed: {error}"),
        }
    }
}
impl std::error::Error for ArrayAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Type(error) => Some(error.as_ref()),
            Self::Heap(error) => Some(error),
            _ => None,
        }
    }
}
impl From<TypeAccessError> for ArrayAccessError {
    fn from(error: TypeAccessError) -> Self {
        Self::Type(Box::new(error))
    }
}
impl From<HeapError> for ArrayAccessError {
    fn from(error: HeapError) -> Self {
        Self::Heap(error)
    }
}
fn destination_error(error: HeapError) -> ArrayAccessError {
    match error {
        HeapError::NullReference => ArrayAccessError::Java(ArrayJavaError::NullPointer),
        HeapError::IndexOutOfBounds { .. } => {
            ArrayAccessError::Java(ArrayJavaError::IndexOutOfBounds)
        }
        error => ArrayAccessError::Heap(error),
    }
}
fn reserve_values(values: &mut Vec<Reference>, amount: usize) -> Result<(), HeapError> {
    values
        .try_reserve_exact(amount)
        .map_err(|_| HeapError::AllocationFailure {
            resource: "reference-array elements",
        })
}
pub(crate) fn reference_payload_size(length: usize, limit: usize) -> Result<usize, HeapError> {
    length
        .checked_mul(REFERENCE_ARRAY_ELEMENT_BYTES)
        .ok_or(HeapError::ResourceLimit {
            resource: crate::HeapResource::PayloadBytes,
            limit,
            required: usize::MAX,
        })
}
#[derive(Debug)]
pub(crate) struct ReferenceArrayPayload {
    pub(crate) array_type: ReferenceType,
    pub(crate) values: Vec<Reference>,
}
impl ReferenceArrayPayload {
    pub(crate) fn validate_type(array_type: ReferenceType) -> Result<(), HeapError> {
        match array_type {
            ReferenceType::Array {
                dimensions,
                element,
                ..
            } if dimensions > 0
                && (dimensions > 1 || matches!(element, ArrayElement::Class(_))) =>
            {
                Ok(())
            }
            _ => Err(HeapError::InvalidState(
                "reference-array payload requires a nonprimitive reference component",
            )),
        }
    }
    pub(crate) fn new(array_type: ReferenceType, length: usize) -> Result<Self, HeapError> {
        Self::validate_type(array_type)?;
        let mut values = Vec::new();
        reserve_values(&mut values, length)?;
        values.resize(length, Reference::null());
        Ok(Self { array_type, values })
    }
    pub(crate) fn length(&self) -> Result<i32, HeapError> {
        Self::validate_type(self.array_type)?;
        i32::try_from(self.values.len()).map_err(|_| {
            HeapError::InvalidState("reference-array payload exceeds signed JVM length")
        })
    }
    fn index(&self, index: i32) -> Result<usize, HeapError> {
        let length = self.length()?;
        if index < 0 || index >= length {
            return Err(HeapError::IndexOutOfBounds { index, length });
        }
        usize::try_from(index)
            .map_err(|_| HeapError::InvalidState("validated reference-array index is negative"))
    }
}
fn reference_payload(payload: &EntryPayload) -> Result<&ReferenceArrayPayload, HeapError> {
    match payload {
        EntryPayload::ReferenceArray(payload) => Ok(payload),
        EntryPayload::Array(_) => Err(HeapError::WrongEntryKind {
            expected: HeapEntryKind::ReferenceArray,
            actual: HeapEntryKind::PrimitiveArray,
        }),
        EntryPayload::Instance(_) | EntryPayload::Mirror(_) => Err(HeapError::WrongEntryKind {
            expected: HeapEntryKind::ReferenceArray,
            actual: HeapEntryKind::Instance,
        }),
    }
}
impl ManagedHeap {
    pub fn reference_array_type(&self, array: Reference) -> Result<ReferenceType, HeapError> {
        let payload = reference_payload(&self.entry(array)?.payload)?;
        payload.length()?;
        Ok(payload.array_type)
    }
    pub(crate) fn reference_array_index(
        &self,
        array: Reference,
        index: i32,
    ) -> Result<(usize, usize), HeapError> {
        let handle = array.heap_ref().ok_or(HeapError::NullReference)?;
        let entry_index = self.checked_handle(handle)?;
        let entry = self
            .entries
            .get(entry_index)
            .ok_or(HeapError::MissingEntry { index: entry_index })?;
        let slot = reference_payload(&entry.payload)?.index(index)?;
        Ok((entry_index, slot))
    }
    pub fn load_reference(&self, array: Reference, index: i32) -> Result<Reference, HeapError> {
        let (entry_index, slot) = self.reference_array_index(array, index)?;
        let payload = reference_payload(
            &self
                .entries
                .get(entry_index)
                .ok_or(HeapError::MissingEntry { index: entry_index })?
                .payload,
        )?;
        let value = *payload
            .values
            .get(slot)
            .ok_or(HeapError::InvalidState("reference array slot is absent"))?;
        if !value.is_null() {
            self.entry(value)?;
        }
        Ok(value)
    }
    // Context caller owns assignment proof. Revalidate all local identities and
    // bounds before one mutation; there is no public raw setter or allocation.
    pub(crate) fn commit_reference_array(
        &mut self,
        array: Reference,
        index: i32,
        value: Reference,
    ) -> Result<(), HeapError> {
        let (entry_index, slot) = self.reference_array_index(array, index)?;
        if !value.is_null() {
            self.entry(value)?;
        }
        let EntryPayload::ReferenceArray(payload) = &mut self
            .entries
            .get_mut(entry_index)
            .ok_or(HeapError::MissingEntry { index: entry_index })?
            .payload
        else {
            return Err(HeapError::InvalidState(
                "validated reference-array payload changed kind",
            ));
        };
        *payload
            .values
            .get_mut(slot)
            .ok_or(HeapError::InvalidState("reference array slot is absent"))? = value;
        Ok(())
    }
}
impl FieldResolutionContext<'_> {
    pub fn allocate_reference_array(
        &mut self,
        component: ReferenceType,
        length: i32,
    ) -> Result<Reference, ArrayAccessError> {
        self.unchanged().map_err(TypeAccessError::from)?;
        let universe = self.universe()?;
        self.validate_reference_type(component, universe)?;
        let array_type = match component {
            ReferenceType::Class(class) => ReferenceType::Array {
                dimensions: 1,
                element: ArrayElement::Class(class),
                defining_loader: self
                    .registry()
                    .defining_loader(class)
                    .map_err(TypeAccessError::from)?,
            },
            ReferenceType::Array {
                dimensions,
                element,
                defining_loader,
            } => ReferenceType::Array {
                dimensions: dimensions
                    .checked_add(1)
                    .ok_or(TypeAccessError::InvalidType(
                        "reference-array component rank must not exceed 254",
                    ))?,
                element,
                defining_loader,
            },
        };
        self.heap_mut()
            .allocate_reference_array_payload(array_type, length)
            .map_err(|error| match error {
                HeapError::NegativeLength { .. } => {
                    ArrayAccessError::Java(ArrayJavaError::NegativeArraySize)
                }
                error => ArrayAccessError::Heap(error),
            })
    }
    pub fn new_reference_array(
        &mut self,
        origin: crate::ClassId,
        index: u16,
        length: i32,
    ) -> Result<Reference, ArrayAccessError> {
        let component =
            self.resolve_reference_type(origin, index)
                .map_err(|error| match error {
                    TypeAccessError::Java(TypeJavaError::IllegalAccess) => {
                        ArrayAccessError::Java(ArrayJavaError::IllegalAccess)
                    }
                    error => error.into(),
                })?;
        self.allocate_reference_array(component, length)
    }
    pub fn read_reference_array(
        &mut self,
        array: Reference,
        index: i32,
    ) -> Result<Reference, ArrayAccessError> {
        self.unchanged().map_err(TypeAccessError::from)?;
        self.heap()
            .reference_array_index(array, index)
            .map_err(destination_error)?;
        // Only the preceding destination check maps null/bounds. A corrupt stored
        // handle discovered by the load remains an opaque host failure.
        Ok(self.heap().load_reference(array, index)?)
    }
    pub fn write_reference_array(
        &mut self,
        array: Reference,
        index: i32,
        value: Reference,
    ) -> Result<(), ArrayAccessError> {
        self.unchanged().map_err(TypeAccessError::from)?;
        self.heap()
            .reference_array_index(array, index)
            .map_err(destination_error)?;
        if !value.is_null() {
            let source = self
                .reference_type(value)?
                .ok_or(TypeAccessError::InvalidType(
                    "nonnull array value has no runtime type",
                ))?;
            let actual = self
                .reference_type(array)?
                .ok_or(TypeAccessError::InvalidType(
                    "nonnull array destination has no runtime type",
                ))?;
            let ReferenceType::Array {
                dimensions,
                element,
                defining_loader,
            } = actual
            else {
                return Err(TypeAccessError::InvalidType(
                    "reference-array destination is not an array type",
                )
                .into());
            };
            let component = crate::types::component(dimensions, element, defining_loader)?;
            if !self.is_assignable(source, component)? {
                return Err(ArrayAccessError::Java(ArrayJavaError::ArrayStore));
            }
        }
        Ok(self
            .heap_mut()
            .commit_reference_array(array, index, value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ClassId, DefinitionRegistry, FieldResolutionOptions, HeapOptions, HierarchyOptions,
        InstanceOptions, LoaderId, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver,
        build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    use std::error::Error;
    fn fixture() -> (DefinitionRegistry, ManagedHeap, ReferenceTypeUniverse) {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut classes = Vec::new();
        for (name, flags, parent) in [
            ("java/lang/Object", 0x21_u16, 0_u16),
            ("java/lang/Cloneable", 0x601, 4),
            ("java/io/Serializable", 0x601, 4),
        ] {
            let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 5];
            for (index, text) in [(1, name), (3, "java/lang/Object")] {
                bytes.push(1);
                bytes.extend((text.len() as u16).to_be_bytes());
                bytes.extend(text.bytes());
                bytes.extend([7, 0, index]);
            }
            for word in [flags, 2, parent, 0, 0, 0, 0] {
                bytes.extend(word.to_be_bytes());
            }
            classes.push(
                registry
                    .define(
                        loader,
                        &name.encode_utf16().collect::<Vec<_>>(),
                        &bytes,
                        ParseOptions::default(),
                    )
                    .unwrap(),
            );
        }
        let u = ReferenceTypeUniverse {
            bootstrap_loader: loader,
            object: classes[0],
            cloneable: classes[1],
            serializable: classes[2],
        };
        let plans: Vec<_> = classes
            .iter()
            .map(|class| {
                build_hierarchy(
                    &registry,
                    *class,
                    &mut SameLoaderResolver,
                    HierarchyOptions::default(),
                )
                .unwrap()
            })
            .collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(&registry, u).unwrap();
        (registry, heap, u)
    }
    #[test]
    fn reservation_and_multiplication_overflow_preserve_live_values() {
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        let loader = LoaderId {
            registry: 1,
            index: 0,
        };
        let actual = ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Class(class),
            defining_loader: loader,
        };
        let mut values = vec![Reference::null()];
        assert!(matches!(
            reserve_values(&mut values, usize::MAX),
            Err(HeapError::AllocationFailure { .. })
        ));
        assert_eq!(values, [Reference::null()]);
        assert!(matches!(
            ReferenceArrayPayload::new(actual, usize::MAX),
            Err(HeapError::AllocationFailure { .. })
        ));
        assert_eq!(
            reference_payload_size(usize::MAX, usize::MAX),
            Err(HeapError::ResourceLimit {
                resource: crate::HeapResource::PayloadBytes,
                limit: usize::MAX,
                required: usize::MAX
            })
        );
    }
    #[test]
    fn private_commit_forged_handles_bounds_and_corrupt_types_never_mutate() {
        let (registry, mut heap, u) = fixture();
        let object = heap.allocate_instance(u.object).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let a = ctx
            .allocate_reference_array(ReferenceType::Class(u.object), 1)
            .unwrap();
        ctx.write_reference_array(a, 0, object).unwrap();
        drop(ctx);
        let (id, index, generation) = a.heap_ref().unwrap().components();
        let old = heap.load_reference(a, 0).unwrap();
        let primitive = heap
            .allocate_primitive_array(crate::PrimitiveArrayKind::Int, 0)
            .unwrap();
        let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
        let foreign_ref = foreign
            .allocate_primitive_array(crate::PrimitiveArrayKind::Int, 0)
            .unwrap();
        for (destination, slot, value) in [
            (Reference::null(), 0, object),
            (a, -1, foreign_ref),
            (a, 1, foreign_ref),
            (primitive, 0, object),
            (object, 0, object),
            (foreign_ref, 0, object),
            (Reference::issued(id, usize::MAX, generation), 0, object),
            (Reference::issued(id, index, generation + 1), 0, object),
            (a, 0, foreign_ref),
            (a, 0, Reference::issued(id, usize::MAX, generation)),
        ] {
            assert!(
                heap.commit_reference_array(destination, slot, value)
                    .is_err()
            );
            assert_eq!(heap.load_reference(a, 0), Ok(old));
        }
        let actual = heap.reference_array_type(a).unwrap();
        for invalid in [
            ReferenceType::Class(u.object),
            ReferenceType::Array {
                dimensions: 0,
                element: ArrayElement::Class(u.object),
                defining_loader: u.bootstrap_loader,
            },
            ReferenceType::Array {
                dimensions: 1,
                element: ArrayElement::Primitive(crate::PrimitiveArrayKind::Int),
                defining_loader: u.bootstrap_loader,
            },
        ] {
            let EntryPayload::ReferenceArray(payload) = &mut heap.entries[index].payload else {
                panic!()
            };
            payload.array_type = invalid;
            assert!(matches!(
                heap.reference_array_type(a),
                Err(HeapError::InvalidState(_))
            ));
            assert!(matches!(
                heap.commit_reference_array(a, 0, object),
                Err(HeapError::InvalidState(_))
            ));
            let EntryPayload::ReferenceArray(payload) = &mut heap.entries[index].payload else {
                panic!()
            };
            assert_eq!(payload.values, [old]);
            payload.array_type = actual;
        }
    }
    #[test]
    fn corrupt_stored_foreign_and_stale_handles_are_host_read_errors_not_destination_java() {
        let (registry, mut heap, u) = fixture();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let a = ctx
            .allocate_reference_array(ReferenceType::Class(u.object), 1)
            .unwrap();
        drop(ctx);
        let (id, index, generation) = a.heap_ref().unwrap().components();
        for stored in [
            Reference::issued(id + 1, index, generation),
            Reference::issued(id, index, generation + 1),
            Reference::issued(id, usize::MAX, generation),
        ] {
            let EntryPayload::ReferenceArray(payload) = &mut heap.entries[index].payload else {
                panic!()
            };
            payload.values[0] = stored;
            assert!(heap.load_reference(a, 0).is_err());
            let mut ctx = FieldResolutionContext::new_unnamed_modules(
                &registry,
                &mut heap,
                &mut resolver,
                FieldResolutionOptions::default(),
            )
            .unwrap();
            assert!(matches!(
                ctx.read_reference_array(a, 0),
                Err(ArrayAccessError::Heap(_))
            ));
            ctx.write_reference_array(a, 0, Reference::null()).unwrap();
            assert_eq!(ctx.read_reference_array(a, 0), Ok(Reference::null()));
        }
        let wrapped = ArrayAccessError::Type(Box::new(TypeAccessError::Java(
            TypeJavaError::IllegalAccess,
        )));
        assert!(wrapped.source().is_some());
        assert!(matches!(wrapped, ArrayAccessError::Type(_)));
        assert!(
            ArrayAccessError::Java(ArrayJavaError::ArrayStore)
                .source()
                .is_none()
        );
        assert!(wrapped.to_string().contains("type validation"));
    }
}

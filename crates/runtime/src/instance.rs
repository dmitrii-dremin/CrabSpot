//! Bounded ordinary-instance shapes and field storage. This is a memory service,
//! not constructor execution, Java linkage, access checking or initialization.
use crate::heap::{Entry, EntryPayload, LIVE_GENERATION, reserve_entries};
use crate::{
    ClassId, ClassLayout, DefinitionRegistry, FieldId, HeapEntryKind, HeapError, HeapRef,
    HierarchyPlan, ManagedHeap, Reference, RegistryError, Value,
};
use crabspot_classfile::{ErrorKind, ParseError};
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstanceOptions {
    pub max_layout_classes: usize,
    pub max_layout_edges: usize,
    pub max_layout_fields: usize,
    pub max_instances: usize,
    pub max_instance_slots: usize,
    pub max_total_instance_slots: usize,
}
impl Default for InstanceOptions {
    fn default() -> Self {
        Self {
            max_layout_classes: 1024,
            max_layout_edges: 8192,
            max_layout_fields: 65_536,
            max_instances: 4096,
            max_instance_slots: 65_536,
            max_total_instance_slots: 1_048_576,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceResource {
    LayoutClasses,
    LayoutEdges,
    LayoutFields,
    Instances,
    InstanceSlots,
    TotalInstanceSlots,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceClassKind {
    Interface,
    Abstract,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstanceError {
    ReadOnlyThrowableState {
        field: FieldId,
    },
    Disabled,
    ReservedClassObject {
        class: ClassId,
    },
    OpaqueClassObject {
        class: ClassId,
    },
    MirrorLayoutSealed,
    ReservedStringObject {
        class: ClassId,
    },
    OpaqueStringObject {
        class: ClassId,
    },
    Registry(RegistryError),
    Heap(HeapError),
    ForeignRegistry,
    LayoutMissing {
        class: ClassId,
    },
    LayoutConflict {
        class: ClassId,
    },
    ReferenceUniverseMismatch {
        class: ClassId,
        expected_object: ClassId,
    },
    NonInstantiable {
        class: ClassId,
        kind: InstanceClassKind,
    },
    MissingField {
        class: ClassId,
        field: FieldId,
    },
    FinalField {
        field: FieldId,
    },
    UnsupportedReferenceStore {
        field: FieldId,
    },
    InvalidDescriptor {
        field: FieldId,
        error: ParseError,
    },
    InvalidMetadata(&'static str),
    ResourceLimit {
        resource: InstanceResource,
        limit: usize,
        required: usize,
    },
}
impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReservedClassObject { class } => {
                write!(f, "ordinary allocation of VM Class {class:?} is sealed")
            }
            Self::OpaqueClassObject { class } => write!(f, "Class {class:?} has opaque VM state"),
            Self::ReservedStringObject { class } => {
                write!(f, "ordinary allocation of VM String {class:?} is sealed")
            }
            Self::OpaqueStringObject { class } => write!(f, "String {class:?} has opaque VM state"),
            Self::MirrorLayoutSealed => {
                f.write_str("mirror binding seals the admitted layout graph")
            }
            Self::ReadOnlyThrowableState { field } => {
                write!(f, "native-owned Throwable field {field:?} is read-only")
            }
            Self::Disabled => f.write_str("instance storage is disabled"),
            Self::Registry(e) => write!(f, "instance metadata lookup failed: {e}"),
            Self::Heap(e) => write!(f, "instance heap operation failed: {e}"),
            Self::ForeignRegistry => f.write_str("instance identity belongs to another registry"),
            Self::LayoutMissing { class } => write!(f, "no instance layout retained for {class:?}"),
            Self::LayoutConflict { class } => {
                write!(f, "conflicting instance layouts for {class:?}")
            }
            Self::ReferenceUniverseMismatch {
                class,
                expected_object,
            } => write!(
                f,
                "layout root {class:?} differs from the bound Object {expected_object:?}"
            ),
            Self::NonInstantiable { class, kind } => {
                write!(f, "cannot allocate {kind:?} {class:?}")
            }
            Self::MissingField { class, field } => {
                write!(f, "{field:?} is not an instance field of {class:?}")
            }
            Self::FinalField { field } => write!(f, "cannot write final field {field:?}"),
            Self::UnsupportedReferenceStore { field } => {
                write!(f, "non-null reference store to {field:?} is unsupported")
            }
            Self::InvalidDescriptor { field, error } => {
                write!(f, "invalid descriptor for {field:?}: {error}")
            }
            Self::InvalidMetadata(reason) => write!(f, "invalid instance metadata: {reason}"),
            Self::ResourceLimit {
                resource,
                limit,
                required,
            } => write!(
                f,
                "instance {resource:?} limit {limit} exceeded (required {required})"
            ),
        }
    }
}
impl std::error::Error for InstanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e),
            Self::Heap(e) => Some(e),
            Self::InvalidDescriptor { error, .. } => Some(error),
            _ => None,
        }
    }
}
impl From<HeapError> for InstanceError {
    fn from(e: HeapError) -> Self {
        Self::Heap(e)
    }
}
impl From<RegistryError> for InstanceError {
    fn from(e: RegistryError) -> Self {
        Self::Registry(e)
    }
}
fn allocation(resource: &'static str) -> InstanceError {
    HeapError::AllocationFailure { resource }.into()
}
fn reserve<T>(values: &mut Vec<T>, additional: usize) -> Result<(), InstanceError> {
    values
        .try_reserve_exact(additional)
        .map_err(|_| allocation("instance metadata/payload"))
}
fn reserve_map<K: Eq + std::hash::Hash, V>(
    map: &mut HashMap<K, V>,
    additional: usize,
) -> Result<(), InstanceError> {
    map.try_reserve(additional)
        .map_err(|_| allocation("instance layout map"))
}
fn increased(
    current: usize,
    amount: usize,
    limit: usize,
    resource: InstanceResource,
) -> Result<usize, InstanceError> {
    let required = current
        .checked_add(amount)
        .ok_or(InstanceError::ResourceLimit {
            resource,
            limit,
            required: usize::MAX,
        })?;
    if required > limit {
        Err(InstanceError::ResourceLimit {
            resource,
            limit,
            required,
        })
    } else {
        Ok(required)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FieldKind {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Reference,
}
impl FieldKind {
    pub(crate) fn default_value(self) -> Value {
        match self {
            Self::Long => Value::Long(0),
            Self::Float => Value::Float(0),
            Self::Double => Value::Double(0),
            Self::Reference => Value::Reference(Reference::null()),
            _ => Value::Int(0),
        }
    }
    pub(crate) fn store(self, field: FieldId, value: Value) -> Result<Value, InstanceError> {
        let expected = self.default_value().kind();
        if value.kind() != expected {
            return Err(HeapError::ValueKindMismatch {
                expected,
                actual: value.kind(),
            }
            .into());
        }
        Ok(match (self, value) {
            (Self::Boolean, Value::Int(v)) => Value::Int(v & 1),
            (Self::Byte, Value::Int(v)) => Value::Int(i32::from(v as i8)),
            (Self::Short, Value::Int(v)) => Value::Int(i32::from(v as i16)),
            (Self::Char, Value::Int(v)) => Value::Int(i32::from(v as u16)),
            (Self::Reference, Value::Reference(r)) if !r.is_null() => {
                return Err(InstanceError::UnsupportedReferenceStore { field });
            }
            (_, value) => value,
        })
    }
}
// Allocation-free descriptor validation avoids copying arbitrarily long object
// names. Metadata remains immutable; the scalar CP key is classified once.
pub(crate) fn classify(field: FieldId, units: &[u16]) -> Result<FieldKind, InstanceError> {
    let invalid = || InstanceError::InvalidDescriptor {
        field,
        error: ParseError {
            offset: 0,
            context: "instance field descriptor",
            kind: ErrorKind::InvalidDescriptor,
        },
    };
    let mut index = 0;
    while units.get(index) == Some(&91) {
        index += 1;
        if index > 255 {
            return Err(invalid());
        }
    }
    let array = index != 0;
    let tag = *units.get(index).ok_or_else(invalid)?;
    index += 1;
    let kind = match tag {
        90 => FieldKind::Boolean,
        66 => FieldKind::Byte,
        67 => FieldKind::Char,
        83 => FieldKind::Short,
        73 => FieldKind::Int,
        74 => FieldKind::Long,
        70 => FieldKind::Float,
        68 => FieldKind::Double,
        76 => {
            let name = units
                .get(index..units.len().saturating_sub(1))
                .ok_or_else(invalid)?;
            if units.last() != Some(&59)
                || name.is_empty()
                || !name
                    .split(|u| *u == 47)
                    .all(|p| !p.is_empty() && !p.iter().any(|u| matches!(*u, 46 | 59 | 91)))
            {
                return Err(invalid());
            }
            index = units.len();
            FieldKind::Reference
        }
        _ => return Err(invalid()),
    };
    if index != units.len() {
        return Err(invalid());
    }
    Ok(if array { FieldKind::Reference } else { kind })
}
#[derive(Debug, Eq, PartialEq)]
struct OwnField {
    id: FieldId,
    kind: FieldKind,
    is_final: bool,
}
#[derive(Debug)]
struct Shape {
    superclass: Option<ClassId>,
    interfaces: Vec<ClassId>,
    flags: u16,
    fields: Vec<OwnField>,
    inherited: usize,
    slots: usize,
}
/// Borrowed relationship data from an admitted shape. No field list is copied.
pub(crate) struct InstanceGraphNode<'a> {
    pub superclass: Option<ClassId>,
    pub interfaces: &'a [ClassId],
    pub flags: u16,
}
impl Shape {
    fn matches(&self, layout: &ClassLayout, flags: u16) -> bool {
        self.superclass == layout.direct_superclass()
            && self.interfaces == layout.direct_interfaces()
            && self.flags == flags
            && self.inherited == layout.inherited_instance_slots()
            && self.slots == layout.instance_slots()
            && self
                .fields
                .iter()
                .map(|f| f.id)
                .eq(layout.declared_instance_fields().iter().copied())
    }
}
#[derive(Debug)]
pub(crate) struct InstanceState {
    pub(crate) options: InstanceOptions,
    registry: Option<u64>,
    shapes: HashMap<ClassId, Shape>,
    edges: usize,
    fields: usize,
    pub(crate) count: usize,
    slots: usize,
}
impl InstanceState {
    pub(crate) fn new(options: InstanceOptions) -> Self {
        Self {
            options,
            registry: None,
            shapes: HashMap::new(),
            edges: 0,
            fields: 0,
            count: 0,
            slots: 0,
        }
    }
    fn check_registry(&self, class: ClassId) -> Result<(), InstanceError> {
        if self.registry.is_some_and(|r| r != class.registry) {
            Err(InstanceError::ForeignRegistry)
        } else {
            Ok(())
        }
    }
    fn shape(&self, class: ClassId) -> Result<&Shape, InstanceError> {
        self.check_registry(class)?;
        self.shapes
            .get(&class)
            .ok_or(InstanceError::LayoutMissing { class })
    }
    fn field(&self, class: ClassId, field: FieldId) -> Result<(usize, &OwnField), InstanceError> {
        self.check_registry(field.owner())?;
        let missing = || InstanceError::MissingField { class, field };
        let mut current = class;
        for _ in 0..self.shapes.len() {
            let shape = self
                .shapes
                .get(&current)
                .ok_or(InstanceError::InvalidMetadata(
                    "receiver chain has a missing shape",
                ))?;
            if current == field.owner() {
                let (index, own) = shape
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, own)| own.id == field)
                    .ok_or_else(missing)?;
                let slot = shape
                    .inherited
                    .checked_add(index)
                    .ok_or(InstanceError::InvalidMetadata("field slot overflow"))?;
                if slot >= shape.slots {
                    return Err(InstanceError::InvalidMetadata(
                        "field lies outside declaring shape",
                    ));
                }
                return Ok((slot, own));
            }
            current = shape.superclass.ok_or_else(missing)?;
        }
        Err(InstanceError::InvalidMetadata(
            "receiver superclass chain is cyclic",
        ))
    }
}
#[derive(Debug)]
pub(crate) struct InstancePayload {
    class: ClassId,
    values: Vec<Value>,
}
// Exclusive, short-lived publication proof. Only the checked construction
// service can supply an opaque constructor authority; no public final bypass.
pub(crate) struct ConstructorWrite {
    heap: u64,
    receiver: Reference,
    constructor: crate::MethodId,
    constructor_key: (u16, u16, u16),
    field: FieldId,
    field_key: (u16, u16, u16),
    slot: usize,
    kind: FieldKind,
    is_final: bool,
}
fn instance(payload: &EntryPayload) -> Result<&InstancePayload, InstanceError> {
    match payload {
        EntryPayload::Instance(value) => Ok(value),
        EntryPayload::Mirror(_) | EntryPayload::String(_) | EntryPayload::Backtrace(_) => Err(
            InstanceError::InvalidMetadata("opaque mirror passed to unguarded instance helper"),
        ),
        EntryPayload::Array(_) => Err(HeapError::WrongEntryKind {
            expected: HeapEntryKind::Instance,
            actual: HeapEntryKind::PrimitiveArray,
        }
        .into()),
        EntryPayload::ReferenceArray(_) => Err(HeapError::WrongEntryKind {
            expected: HeapEntryKind::Instance,
            actual: HeapEntryKind::ReferenceArray,
        }
        .into()),
    }
}
fn same_layout(left: &ClassLayout, right: &ClassLayout) -> bool {
    left.direct_superclass() == right.direct_superclass()
        && left.direct_interfaces() == right.direct_interfaces()
        && left.declared_instance_fields() == right.declared_instance_fields()
        && left.inherited_instance_slots() == right.inherited_instance_slots()
        && left.instance_slots() == right.instance_slots()
}

#[cfg(test)]
mod construction_guards {
    use super::*;
    mod rt {
        pub use crate::*;
    }
    #[allow(dead_code)]
    mod support {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/construction.rs"
        ));
    }
    use crate::{ConstructorReceiverKind, SameLoaderResolver};
    use support::*;

    #[test]
    fn damaged_constructor_publication_proofs_and_sources_leave_nonempty_fields_unchanged() {
        let mut w = world();
        let own = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[
                Field::new("x", "I", 17, None),
                Field::new("r", "LC;", 17, None),
            ],
            &[
                Method::new("<init>", "()V", 1),
                Method::new("helper", "()V", 1),
            ],
            &[("C", "<init>", "()V", 10)],
            &[],
            65,
        );
        let mut resolver = SameLoaderResolver;
        let mut heap = heap(&w, &[own.class, w.error], &mut resolver);
        let receiver = heap.allocate_instance(own.class).unwrap();
        let source = heap.allocate_instance(own.class).unwrap();
        let mut foreign = ManagedHeap::new_with_instances(
            crate::HeapOptions::default(),
            InstanceOptions::default(),
        )
        .unwrap();
        let foreign_source = foreign
            .allocate_primitive_array(crate::PrimitiveArrayKind::Int, 0)
            .unwrap();
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        let ctor = ctx
            .resolve_constructor(
                &claim,
                own.methods[0],
                own.refs[0],
                receiver,
                ConstructorReceiverKind::Allocation,
            )
            .unwrap();
        drop(ctx);
        let x = field(&w, own.class, "x", "I");
        let r = field(&w, own.class, "r", "LC;");
        let proof = heap
            .constructor_field_proof(&w.registry, &ctor, receiver, x)
            .unwrap();
        heap.commit_constructor_field(&w.registry, proof, Value::Int(41))
            .unwrap();
        let proof = heap
            .constructor_field_proof(&w.registry, &ctor, receiver, r)
            .unwrap();
        heap.commit_constructor_field(&w.registry, proof, Value::Reference(source))
            .unwrap();
        let counts = (heap.allocated_instances(), heap.total_instance_slots());
        for variant in 0..8 {
            let mut proof = heap
                .constructor_field_proof(&w.registry, &ctor, receiver, x)
                .unwrap();
            match variant {
                0 => proof.heap = 0,
                1 => proof.slot = usize::MAX,
                2 => proof.kind = FieldKind::Long,
                3 => proof.is_final = false,
                4 => proof.field_key.1 = u16::MAX,
                5 => {
                    proof.constructor = own.methods[1];
                    let m = w.registry.method(proof.constructor).unwrap();
                    proof.constructor_key = (m.name_index, m.descriptor_index, m.access_flags);
                }
                6 => proof.constructor.index = usize::MAX,
                _ => proof.receiver = Reference::null(),
            }
            assert!(
                heap.commit_constructor_field(&w.registry, proof, Value::Int(99))
                    .is_err()
            );
            assert_eq!(heap.get_field(receiver, x), Ok(Value::Int(41)));
            assert_eq!(
                heap.children(receiver).unwrap().collect::<Vec<_>>(),
                [source.heap_ref().unwrap()]
            );
        }
        let (_, index, generation) = source.heap_ref().unwrap().components();
        let stale = Reference::issued(heap.identity, index, generation + 1);
        for value in [
            Value::Int(3),
            Value::Reference(foreign_source),
            Value::Reference(stale),
        ] {
            let proof = heap
                .constructor_field_proof(&w.registry, &ctor, receiver, r)
                .unwrap();
            assert!(
                heap.commit_constructor_field(&w.registry, proof, value)
                    .is_err()
            );
            assert_eq!(heap.get_field(receiver, r), Ok(Value::Reference(source)));
            assert_eq!(
                heap.children(receiver).unwrap().collect::<Vec<_>>(),
                [source.heap_ref().unwrap()]
            );
        }
        let proof = heap
            .constructor_field_proof(&w.registry, &ctor, receiver, r)
            .unwrap();
        let handle = receiver.heap_ref().unwrap();
        let index = heap.checked_handle(handle).unwrap();
        let EntryPayload::Instance(payload) = &mut heap.entries.get_mut(index).unwrap().payload
        else {
            panic!("instance")
        };
        payload.values.truncate(1);
        let before = payload.values.clone();
        assert!(
            heap.commit_constructor_field(&w.registry, proof, Value::Reference(source))
                .is_err()
        );
        let EntryPayload::Instance(payload) = &heap.entries.get(index).unwrap().payload else {
            panic!("instance")
        };
        assert_eq!(payload.values, before);
        assert_eq!(
            (heap.allocated_instances(), heap.total_instance_slots()),
            counts
        );
    }
}
fn parse_error(error: ParseError) -> InstanceError {
    RegistryError::Parse(error).into()
}

impl ManagedHeap {
    pub(crate) fn check_reserved_class(&self, class: ClassId) -> Result<(), InstanceError> {
        if self.strings.as_ref().is_some_and(|s| s.class == class) {
            return Err(InstanceError::ReservedStringObject { class });
        }
        if self.mirrors.as_ref().is_some_and(|m| m.class == class) {
            return Err(InstanceError::ReservedClassObject { class });
        }
        Ok(())
    }
    pub(crate) fn reject_opaque(&self, reference: Reference) -> Result<(), InstanceError> {
        if matches!(self.entry(reference)?.payload, EntryPayload::Backtrace(_)) {
            return Err(InstanceError::InvalidMetadata(
                "Backtrace has opaque VM state",
            ));
        }
        if matches!(self.entry(reference)?.payload, EntryPayload::String(_)) {
            let class = self
                .strings
                .as_ref()
                .ok_or(InstanceError::InvalidMetadata("String without binding"))?
                .class;
            return Err(InstanceError::OpaqueStringObject { class });
        }
        if matches!(self.entry(reference)?.payload, EntryPayload::Mirror(_)) {
            let class = self
                .mirrors
                .as_ref()
                .ok_or(InstanceError::InvalidMetadata("mirror without binding"))?
                .class;
            return Err(InstanceError::OpaqueClassObject { class });
        }
        Ok(())
    }
    fn ordinary_instance(&self, reference: Reference) -> Result<&InstancePayload, InstanceError> {
        self.reject_opaque(reference)?;
        instance(&self.entry(reference)?.payload)
    }
    pub(crate) fn instance_graph_registry(&self) -> Result<Option<u64>, InstanceError> {
        Ok(self
            .instances
            .as_ref()
            .ok_or(InstanceError::Disabled)?
            .registry)
    }
    pub(crate) fn instance_graph_classes(
        &self,
    ) -> Result<impl Iterator<Item = ClassId> + '_, InstanceError> {
        Ok(self
            .instances
            .as_ref()
            .ok_or(InstanceError::Disabled)?
            .shapes
            .keys()
            .copied())
    }
    pub(crate) fn instance_graph_node(
        &self,
        class: ClassId,
    ) -> Result<InstanceGraphNode<'_>, InstanceError> {
        let shape = self
            .instances
            .as_ref()
            .ok_or(InstanceError::Disabled)?
            .shape(class)?;
        Ok(InstanceGraphNode {
            superclass: shape.superclass,
            interfaces: &shape.interfaces,
            flags: shape.flags,
        })
    }
    pub(crate) fn instance_graph_contains(&self, class: ClassId) -> Result<bool, InstanceError> {
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        if state.registry != Some(class.registry) {
            return Err(InstanceError::ForeignRegistry);
        }
        Ok(state.shapes.contains_key(&class))
    }
    pub fn allocated_instances(&self) -> usize {
        self.instances.as_ref().map_or(0, |s| s.count)
    }
    pub fn total_instance_slots(&self) -> usize {
        self.instances.as_ref().map_or(0, |s| s.slots)
    }
    pub fn retained_layout_classes(&self) -> usize {
        self.instances.as_ref().map_or(0, |s| s.shapes.len())
    }
    pub fn retained_layout_edges(&self) -> usize {
        self.instances.as_ref().map_or(0, |s| s.edges)
    }
    pub fn retained_layout_fields(&self) -> usize {
        self.instances.as_ref().map_or(0, |s| s.fields)
    }
    pub fn admit_instance_layouts(
        &mut self,
        registry: &DefinitionRegistry,
        plans: &[&HierarchyPlan],
    ) -> Result<(), InstanceError> {
        let universe = self.reference_universe;
        let state = self.instances.as_mut().ok_or(InstanceError::Disabled)?;
        if plans.is_empty() {
            return Ok(());
        }
        registry.class(plans[0].root())?;
        let registry_id = plans[0].root().registry;
        if state.registry.is_some_and(|r| r != registry_id) {
            return Err(InstanceError::ForeignRegistry);
        }
        let mut candidates: HashMap<ClassId, &ClassLayout> = HashMap::new();
        let mut edges = state.edges;
        let mut fields = state.fields;
        for plan in plans {
            registry.class(plan.root())?;
            if !plan.classes().contains(&plan.root()) {
                return Err(InstanceError::InvalidMetadata("plan root is not retained"));
            }
            for &class in plan.classes() {
                let metadata = registry.class(class)?;
                let layout = plan
                    .layout(class)
                    .ok_or(InstanceError::InvalidMetadata("plan class has no layout"))?;
                if layout.class_id() != class {
                    return Err(InstanceError::InvalidMetadata(
                        "layout identity does not match plan class",
                    ));
                }
                if let Some(universe) = universe {
                    if layout.direct_superclass().is_none() && class != universe.object {
                        return Err(InstanceError::ReferenceUniverseMismatch {
                            class,
                            expected_object: universe.object,
                        });
                    }
                }
                // Check all consumed field handles, including statics which are not retained.
                let mut own = layout.declared_instance_fields().iter();
                let mut statics = layout.declared_static_fields().iter();
                for (index, member) in metadata.fields.iter().enumerate() {
                    let expected = FieldId {
                        owner: class,
                        index,
                    };
                    let retained = if member.access_flags & 8 == 0 {
                        own.next()
                    } else {
                        statics.next()
                    };
                    if let Some(field) = retained {
                        registry.field(*field)?;
                    }
                    if retained != Some(&expected) {
                        return Err(InstanceError::InvalidMetadata(
                            "field handles differ from declaration order/category",
                        ));
                    }
                }
                if own.next().is_some() || statics.next().is_some() {
                    return Err(InstanceError::InvalidMetadata(
                        "layout retains extra fields",
                    ));
                }
                if let Some(shape) = state.shapes.get(&class) {
                    if !shape.matches(layout, metadata.access_flags) {
                        return Err(InstanceError::LayoutConflict { class });
                    }
                } else if let Some(previous) = candidates.get(&class) {
                    if !same_layout(previous, layout) {
                        return Err(InstanceError::LayoutConflict { class });
                    }
                } else {
                    if self.mirrors.is_some() {
                        return Err(InstanceError::MirrorLayoutSealed);
                    }
                    increased(
                        state.shapes.len(),
                        candidates
                            .len()
                            .checked_add(1)
                            .ok_or(InstanceError::InvalidMetadata("candidate count overflow"))?,
                        state.options.max_layout_classes,
                        InstanceResource::LayoutClasses,
                    )?;
                    edges = increased(
                        edges,
                        layout.direct_interfaces().len(),
                        state.options.max_layout_edges,
                        InstanceResource::LayoutEdges,
                    )?;
                    edges = increased(
                        edges,
                        usize::from(layout.direct_superclass().is_some()),
                        state.options.max_layout_edges,
                        InstanceResource::LayoutEdges,
                    )?;
                    fields = increased(
                        fields,
                        layout.declared_instance_fields().len(),
                        state.options.max_layout_fields,
                        InstanceResource::LayoutFields,
                    )?;
                    reserve_map(&mut candidates, 1)?;
                    candidates.insert(class, layout);
                }
            }
        }
        let slots_of = |class: ClassId| -> Result<usize, InstanceError> {
            candidates
                .get(&class)
                .map(|l| l.instance_slots())
                .or_else(|| state.shapes.get(&class).map(|s| s.slots))
                .ok_or(InstanceError::InvalidMetadata(
                    "dependency shape is absent from admission union",
                ))
        };
        for (&class, layout) in &candidates {
            let metadata = registry.class(class)?;
            let superclass = layout.direct_superclass();
            if (metadata.super_class == 0) != superclass.is_none()
                || metadata.interfaces.len() != layout.direct_interfaces().len()
            {
                return Err(InstanceError::InvalidMetadata(
                    "layout relationships differ from class metadata",
                ));
            }
            if let Some(parent) = superclass {
                slots_of(parent)?;
                let parent_metadata = registry.class(parent)?;
                if parent_metadata.access_flags & (0x0200 | 0x0010) != 0 {
                    return Err(InstanceError::InvalidMetadata(
                        "invalid superclass kind/finality",
                    ));
                }
                if metadata
                    .constant_pool
                    .class_name(metadata.super_class)
                    .map_err(parse_error)?
                    .units()
                    != parent_metadata.name().map_err(parse_error)?.units()
                {
                    return Err(InstanceError::InvalidMetadata(
                        "resolved superclass name differs from declaration",
                    ));
                }
            }
            for (&cp, &interface) in metadata.interfaces.iter().zip(layout.direct_interfaces()) {
                slots_of(interface)?;
                let target = registry.class(interface)?;
                if target.access_flags & 0x0200 == 0
                    || metadata
                        .constant_pool
                        .class_name(cp)
                        .map_err(parse_error)?
                        .units()
                        != target.name().map_err(parse_error)?.units()
                {
                    return Err(InstanceError::InvalidMetadata(
                        "resolved interface differs from declaration",
                    ));
                }
            }
            let inherited = if metadata.access_flags & 0x0200 != 0 {
                0
            } else {
                superclass.map(slots_of).transpose()?.unwrap_or(0)
            };
            let total = inherited
                .checked_add(layout.declared_instance_fields().len())
                .ok_or(InstanceError::InvalidMetadata(
                    "layout slot arithmetic overflow",
                ))?;
            if inherited != layout.inherited_instance_slots() || total != layout.instance_slots() {
                return Err(InstanceError::InvalidMetadata(
                    "layout slot counts do not match dependencies and own fields",
                ));
            }
        }
        validate_acyclic(&candidates)?;
        let mut staged = HashMap::new();
        reserve_map(&mut staged, candidates.len())?;
        let mut kinds = HashMap::new();
        for (&class, layout) in &candidates {
            let metadata = registry.class(class)?;
            let mut own = Vec::new();
            reserve(&mut own, layout.declared_instance_fields().len())?;
            for &field in layout.declared_instance_fields() {
                let member = registry.field(field)?;
                let key = (class, member.descriptor_index);
                let kind = if let Some(kind) = kinds.get(&key) {
                    *kind
                } else {
                    let descriptor = metadata
                        .constant_pool
                        .utf8(member.descriptor_index)
                        .map_err(parse_error)?
                        .units();
                    let kind = classify(field, descriptor)?;
                    reserve_map(&mut kinds, 1)?;
                    kinds.insert(key, kind);
                    kind
                };
                own.push(OwnField {
                    id: field,
                    kind,
                    is_final: member.access_flags & 0x10 != 0,
                });
            }
            let mut interfaces = Vec::new();
            reserve(&mut interfaces, layout.direct_interfaces().len())?;
            interfaces.extend_from_slice(layout.direct_interfaces());
            staged.insert(
                class,
                Shape {
                    superclass: layout.direct_superclass(),
                    interfaces,
                    flags: metadata.access_flags,
                    fields: own,
                    inherited: layout.inherited_instance_slots(),
                    slots: layout.instance_slots(),
                },
            );
        }
        reserve_map(&mut state.shapes, staged.len())?;
        // Destination capacity and all metadata are ready. Publication is infallible.
        state.shapes.extend(staged);
        state.registry = Some(registry_id);
        state.edges = edges;
        state.fields = fields;
        Ok(())
    }
    pub(crate) fn instance_allocation_shape(
        &self,
        class: ClassId,
    ) -> Result<(Option<ClassId>, usize), InstanceError> {
        let shape = self
            .instances
            .as_ref()
            .ok_or(InstanceError::Disabled)?
            .shape(class)?;
        Ok((shape.superclass, shape.fields.len()))
    }
    pub fn allocate_instance(&mut self, class: ClassId) -> Result<Reference, InstanceError> {
        self.check_reserved_class(class)?;
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let shape = state.shape(class)?;
        let kind = if shape.flags & 0x200 != 0 {
            Some(InstanceClassKind::Interface)
        } else if shape.flags & 0x400 != 0 {
            Some(InstanceClassKind::Abstract)
        } else {
            None
        };
        if let Some(kind) = kind {
            return Err(InstanceError::NonInstantiable { class, kind });
        }
        let count = increased(
            state.count,
            1,
            state.options.max_instances,
            InstanceResource::Instances,
        )?;
        increased(
            0,
            shape.slots,
            state.options.max_instance_slots,
            InstanceResource::InstanceSlots,
        )?;
        let slots = increased(
            state.slots,
            shape.slots,
            state.options.max_total_instance_slots,
            InstanceResource::TotalInstanceSlots,
        )?;
        let mut values = Vec::new();
        reserve(&mut values, shape.slots)?;
        values.resize(shape.slots, Value::Int(0));
        let mut current = Some(class);
        let mut remaining = state.shapes.len();
        while let Some(id) = current {
            if remaining == 0 {
                return Err(InstanceError::InvalidMetadata(
                    "allocation superclass chain is cyclic",
                ));
            }
            remaining -= 1;
            let shape = state.shapes.get(&id).ok_or(InstanceError::InvalidMetadata(
                "allocation superclass shape is missing",
            ))?;
            if shape.flags & 0x200 != 0 {
                return Err(InstanceError::InvalidMetadata(
                    "allocation chain contains interface",
                ));
            }
            let expected_prefix = match shape.superclass {
                Some(parent) => {
                    state
                        .shapes
                        .get(&parent)
                        .ok_or(InstanceError::InvalidMetadata(
                            "allocation superclass shape is missing",
                        ))?
                        .slots
                }
                None => 0,
            };
            if shape.inherited != expected_prefix {
                return Err(InstanceError::InvalidMetadata(
                    "allocation inherited prefix differs from parent",
                ));
            }
            let end = shape.inherited.checked_add(shape.fields.len()).ok_or(
                InstanceError::InvalidMetadata("allocation slot arithmetic overflow"),
            )?;
            if end != shape.slots {
                return Err(InstanceError::InvalidMetadata(
                    "allocation shape has inconsistent counts",
                ));
            }
            let own =
                values
                    .get_mut(shape.inherited..end)
                    .ok_or(InstanceError::InvalidMetadata(
                        "allocation shape exceeds receiver slots",
                    ))?;
            for (value, field) in own.iter_mut().zip(&shape.fields) {
                *value = field.kind.default_value();
            }
            current = shape.superclass;
        }
        reserve_entries(&mut self.entries, 1)?;
        let index = self.entries.len();
        let state = self.instances.as_mut().ok_or(InstanceError::Disabled)?;
        self.entries.push(Entry {
            generation: LIVE_GENERATION,
            string_backing: None,
            payload: EntryPayload::Instance(InstancePayload { class, values }),
        });
        state.count = count;
        state.slots = slots;
        Ok(Reference::issued(self.identity, index, LIVE_GENERATION))
    }
    pub fn instance_class(&self, reference: Reference) -> Result<ClassId, InstanceError> {
        self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let class = match &self.entry(reference)?.payload {
            EntryPayload::Backtrace(_) => {
                self.throwables
                    .as_ref()
                    .ok_or(InstanceError::InvalidMetadata("Backtrace without binding"))?
                    .plan
                    .object
            }
            EntryPayload::String(_) => {
                self.strings
                    .as_ref()
                    .ok_or(InstanceError::InvalidMetadata("String without binding"))?
                    .class
            }
            EntryPayload::Mirror(_) => {
                self.mirrors
                    .as_ref()
                    .ok_or(InstanceError::InvalidMetadata("mirror without binding"))?
                    .class
            }
            payload => instance(payload)?.class,
        };
        self.instances
            .as_ref()
            .ok_or(InstanceError::Disabled)?
            .shape(class)?;
        Ok(class)
    }
    pub fn get_field(&self, reference: Reference, field: FieldId) -> Result<Value, InstanceError> {
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let object = self.ordinary_instance(reference)?;
        let (slot, _) = state.field(object.class, field)?;
        object
            .values
            .get(slot)
            .copied()
            .ok_or(InstanceError::InvalidMetadata(
                "field slot is outside instance payload",
            ))
    }
    pub fn put_field(
        &mut self,
        reference: Reference,
        field: FieldId,
        value: Value,
    ) -> Result<(), InstanceError> {
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let handle = reference.heap_ref().ok_or(HeapError::NullReference)?;
        let index = self.checked_handle(handle)?;
        self.reject_opaque(reference)?;
        let object = instance(
            &self
                .entries
                .get(index)
                .ok_or(HeapError::MissingEntry { index })?
                .payload,
        )?;
        let (slot, own) = state.field(object.class, field)?;
        if own.is_final {
            return Err(InstanceError::FinalField { field });
        }
        let value = own.kind.store(field, value)?;
        let native_owned = self.throwable_field_is_native(field);
        let EntryPayload::Instance(object) = &mut self
            .entries
            .get_mut(index)
            .ok_or(HeapError::MissingEntry { index })?
            .payload
        else {
            return Err(InstanceError::InvalidMetadata(
                "validated instance changed kind",
            ));
        };
        let destination = object
            .values
            .get_mut(slot)
            .ok_or(InstanceError::InvalidMetadata(
                "field slot is outside instance payload",
            ))?;
        if native_owned {
            return Err(InstanceError::ReadOnlyThrowableState { field });
        }
        *destination = value;
        Ok(())
    }
    // The field context owns the descriptor/assignment proof. There are no
    // external callbacks between that proof and this exclusive heap mutation.
    // Public put_field deliberately never routes a non-null value here.
    pub(crate) fn commit_reference_field(
        &mut self,
        receiver: Reference,
        field: FieldId,
        value: Reference,
    ) -> Result<(), InstanceError> {
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let handle = receiver.heap_ref().ok_or(HeapError::NullReference)?;
        let index = self.checked_handle(handle)?;
        self.reject_opaque(receiver)?;
        let object = instance(
            &self
                .entries
                .get(index)
                .ok_or(HeapError::MissingEntry { index })?
                .payload,
        )?;
        let (slot, own) = state.field(object.class, field)?;
        if own.is_final {
            return Err(InstanceError::FinalField { field });
        }
        if own.kind != FieldKind::Reference {
            return Err(HeapError::ValueKindMismatch {
                expected: own.kind.default_value().kind(),
                actual: crate::ValueKind::Reference,
            }
            .into());
        }
        if !value.is_null() {
            self.entry(value)?;
        }
        let native_owned = self.throwable_field_is_native(field);
        let EntryPayload::Instance(object) = &mut self
            .entries
            .get_mut(index)
            .ok_or(HeapError::MissingEntry { index })?
            .payload
        else {
            return Err(InstanceError::InvalidMetadata(
                "validated reference receiver changed kind",
            ));
        };
        let destination = object
            .values
            .get_mut(slot)
            .ok_or(InstanceError::InvalidMetadata(
                "reference field slot is outside instance payload",
            ))?;
        if native_owned {
            return Err(InstanceError::ReadOnlyThrowableState { field });
        }
        *destination = Value::Reference(value);
        Ok(())
    }
    pub(crate) fn constructor_field_proof(
        &self,
        registry: &DefinitionRegistry,
        authority: &crate::ResolvedConstructor,
        receiver: Reference,
        field: FieldId,
    ) -> Result<ConstructorWrite, InstanceError> {
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let object = self.ordinary_instance(receiver)?;
        let (slot, own) = state.field(object.class, field)?;
        let member = registry.field(field)?;
        let constructor = registry.method(authority.method())?;
        let definition = registry.class(authority.method().owner())?;
        if constructor.access_flags & 8 != 0
            || !definition
                .constant_pool
                .utf8(constructor.name_index)
                .map_err(parse_error)?
                .equals_ascii("<init>")
        {
            return Err(InstanceError::InvalidMetadata(
                "constructor proof requires an instance initializer",
            ));
        }
        if own.is_final != (member.access_flags & 16 != 0) || member.access_flags & 8 != 0 {
            return Err(InstanceError::InvalidMetadata(
                "constructor field metadata differs from layout",
            ));
        }
        if own.is_final && authority.method().owner() != field.owner() {
            return Err(InstanceError::FinalField { field });
        }
        Ok(ConstructorWrite {
            heap: self.identity,
            receiver,
            constructor: authority.method(),
            field,
            constructor_key: (
                constructor.name_index,
                constructor.descriptor_index,
                constructor.access_flags,
            ),
            field_key: (
                member.name_index,
                member.descriptor_index,
                member.access_flags,
            ),
            slot,
            kind: own.kind,
            is_final: own.is_final,
        })
    }
    pub(crate) fn commit_constructor_field(
        &mut self,
        registry: &DefinitionRegistry,
        proof: ConstructorWrite,
        value: Value,
    ) -> Result<(), InstanceError> {
        if proof.heap != self.identity {
            return Err(InstanceError::InvalidMetadata(
                "constructor proof belongs to another heap",
            ));
        }
        let handle = proof.receiver.heap_ref().ok_or(HeapError::NullReference)?;
        let index = self.checked_handle(handle)?;
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let object = self.ordinary_instance(proof.receiver)?;
        let (slot, own) = state.field(object.class, proof.field)?;
        let member = registry.field(proof.field)?;
        let constructor = registry.method(proof.constructor)?;
        let definition = registry.class(proof.constructor.owner())?;
        if constructor.access_flags & 8 != 0
            || !definition
                .constant_pool
                .utf8(constructor.name_index)
                .map_err(parse_error)?
                .equals_ascii("<init>")
            || member.access_flags & 8 != 0
            || own.is_final != (member.access_flags & 16 != 0)
        {
            return Err(InstanceError::InvalidMetadata(
                "constructor proof has invalid role or final metadata",
            ));
        }
        if (
            member.name_index,
            member.descriptor_index,
            member.access_flags,
        ) != proof.field_key
            || (
                constructor.name_index,
                constructor.descriptor_index,
                constructor.access_flags,
            ) != proof.constructor_key
        {
            return Err(InstanceError::InvalidMetadata(
                "constructor proof metadata fingerprint differs",
            ));
        }
        if slot != proof.slot || own.kind != proof.kind || own.is_final != proof.is_final {
            return Err(InstanceError::InvalidMetadata(
                "constructor field proof no longer matches layout",
            ));
        }
        if own.is_final && proof.constructor.owner() != proof.field.owner() {
            return Err(InstanceError::FinalField { field: proof.field });
        }
        let value = match (own.kind, value) {
            (FieldKind::Reference, Value::Reference(reference)) if !reference.is_null() => {
                self.entry(reference)?;
                value
            }
            _ => own.kind.store(proof.field, value)?,
        };
        let native_owned = self.throwable_field_is_native(proof.field);
        // All checks, including source handle and destination bounds, precede
        // this single write; neither proof creation nor commit invokes callbacks.
        let EntryPayload::Instance(object) = &mut self
            .entries
            .get_mut(index)
            .ok_or(HeapError::MissingEntry { index })?
            .payload
        else {
            return Err(InstanceError::InvalidMetadata(
                "constructor receiver changed kind",
            ));
        };
        let destination = object
            .values
            .get_mut(slot)
            .ok_or(InstanceError::InvalidMetadata(
                "constructor field slot is outside instance payload",
            ))?;
        if native_owned {
            return Err(InstanceError::ReadOnlyThrowableState { field: proof.field });
        }
        *destination = value;
        Ok(())
    }
    pub fn children(
        &self,
        reference: Reference,
    ) -> Result<impl Iterator<Item = HeapRef> + '_, HeapError> {
        let (values, references, backing): (&[Value], &[Reference], Option<HeapRef>) =
            match &self.entry(reference)?.payload {
                EntryPayload::Array(_) | EntryPayload::Mirror(_) => (&[], &[], None),
                EntryPayload::Backtrace(p) => (&[], &[], Some(p.owner)),
                EntryPayload::String(p) => (&[], &[], p.backing.map(|(h, _)| h)),
                EntryPayload::ReferenceArray(a) => (&[], a.values.as_slice(), None),
                EntryPayload::Instance(o) => (o.values.as_slice(), &[], None),
            };
        Ok(values
            .iter()
            .filter_map(|value| match value {
                Value::Reference(r) => r.heap_ref(),
                _ => None,
            })
            .chain(
                references
                    .iter()
                    .filter_map(|reference| reference.heap_ref()),
            )
            .chain(backing))
    }
}
fn validate_acyclic(candidates: &HashMap<ClassId, &ClassLayout>) -> Result<(), InstanceError> {
    let mut marks = HashMap::new();
    reserve_map(&mut marks, candidates.len())?;
    let mut pending = Vec::new();
    reserve(&mut pending, candidates.len())?;
    for &root in candidates.keys() {
        if marks.contains_key(&root) {
            continue;
        }
        marks.insert(root, false);
        pending.push((root, 0usize));
        while let Some((class, next)) = pending.last_mut() {
            let layout = candidates
                .get(class)
                .ok_or(InstanceError::InvalidMetadata("traversal shape missing"))?;
            let dependency = if *next == 0 {
                layout.direct_superclass()
            } else {
                layout.direct_interfaces().get(*next - 1).copied()
            };
            // Index zero is reserved for the optional superclass even when absent.
            if *next == 0 && dependency.is_none() {
                *next = 1;
                continue;
            }
            *next = next.checked_add(1).ok_or(InstanceError::InvalidMetadata(
                "dependency traversal overflow",
            ))?;
            match dependency {
                Some(target) if candidates.contains_key(&target) => match marks.get(&target) {
                    Some(false) => {
                        return Err(InstanceError::InvalidMetadata(
                            "admitted dependency graph is cyclic",
                        ));
                    }
                    Some(true) => {}
                    None => {
                        marks.insert(target, false);
                        pending.push((target, 0));
                    }
                },
                Some(_) => {}
                None => {
                    let (class, _) = pending
                        .pop()
                        .ok_or(InstanceError::InvalidMetadata("traversal stack empty"))?;
                    marks.insert(class, true);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HeapOptions, HierarchyOptions, PrimitiveArrayKind, RegistryOptions, SameLoaderResolver,
        build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn definition(name: &str, parent: bool, fields: &[(&str, &str, u16)]) -> Vec<u8> {
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        bytes.extend((5 + fields.len() as u16 * 2).to_be_bytes());
        fn utf8(bytes: &mut Vec<u8>, text: &str) {
            bytes.push(1);
            bytes.extend((text.len() as u16).to_be_bytes());
            bytes.extend(text.bytes());
        }
        utf8(&mut bytes, name);
        bytes.extend([7, 0, 1]);
        utf8(&mut bytes, "java/lang/Object");
        bytes.extend([7, 0, 3]);
        for (name, descriptor, _) in fields {
            utf8(&mut bytes, name);
            utf8(&mut bytes, descriptor);
        }
        for word in [0x21, 2, if parent { 4 } else { 0 }, 0, fields.len() as u16] {
            bytes.extend(word.to_be_bytes());
        }
        for (index, (_, _, flags)) in fields.iter().enumerate() {
            for word in [*flags, 5 + index as u16 * 2, 6 + index as u16 * 2, 0] {
                bytes.extend(word.to_be_bytes());
            }
        }
        bytes.extend([0, 0, 0, 0]);
        bytes
    }
    fn setup() -> (ManagedHeap, ClassId, Vec<FieldId>) {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        registry
            .define(
                loader,
                &units("java/lang/Object"),
                &definition("java/lang/Object", false, &[]),
                ParseOptions::default(),
            )
            .unwrap();
        let fields = [
            ("i", "I", 1),
            ("r", "Ljava/lang/Object;", 1),
            ("f", "J", 0x11),
        ];
        let class = registry
            .define(
                loader,
                &units("T"),
                &definition("T", true, &fields),
                ParseOptions::default(),
            )
            .unwrap();
        let plan = build_hierarchy(
            &registry,
            class,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        (
            heap,
            class,
            (0..3)
                .map(|index| FieldId {
                    owner: class,
                    index,
                })
                .collect(),
        )
    }
    #[test]
    fn checked_reference_commit_revalidates_guards_and_never_mutates_on_failure() {
        let (mut heap, class, fields) = setup();
        let receiver = heap.allocate_instance(class).unwrap();
        let value = heap.allocate_instance(class).unwrap();
        heap.commit_reference_field(receiver, fields[1], value)
            .unwrap();
        let old = heap.get_field(receiver, fields[1]).unwrap();
        let edges: Vec<_> = heap.children(receiver).unwrap().collect();
        let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
        let foreign_ref = foreign
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
        let array = heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
        let (id, index, generation) = value.heap_ref().unwrap().components();
        let foreign_field = FieldId {
            owner: ClassId {
                registry: class.registry + 1,
                index: class.index,
            },
            index: 1,
        };
        let unrelated = FieldId {
            owner: ClassId {
                registry: class.registry,
                index: usize::MAX,
            },
            index: 1,
        };
        for (parent, field, source) in [
            (receiver, fields[0], value),
            (receiver, fields[2], value),
            (receiver, foreign_field, value),
            (receiver, unrelated, value),
            (receiver, fields[1], foreign_ref),
            (foreign_ref, fields[1], value),
            (array, fields[1], value),
            (Reference::null(), fields[1], value),
            (
                receiver,
                fields[1],
                Reference::issued(id, usize::MAX, generation),
            ),
            (
                receiver,
                fields[1],
                Reference::issued(id, index, generation + 1),
            ),
        ] {
            assert!(heap.commit_reference_field(parent, field, source).is_err());
            assert_eq!(heap.get_field(receiver, fields[1]).unwrap(), old);
            assert_eq!(heap.children(receiver).unwrap().collect::<Vec<_>>(), edges);
        }
        let receiver_index = receiver.heap_ref().unwrap().components().1;
        let EntryPayload::Instance(object) = &mut heap.entries[receiver_index].payload else {
            panic!()
        };
        object.values.truncate(1);
        let before = object.values.clone();
        assert!(matches!(
            heap.commit_reference_field(receiver, fields[1], value),
            Err(InstanceError::InvalidMetadata(_))
        ));
        let EntryPayload::Instance(object) = &heap.entries[receiver_index].payload else {
            panic!()
        };
        assert_eq!(object.values, before);
        assert!(heap.children(receiver).unwrap().next().is_none());
    }
    #[test]
    fn allocation_free_descriptor_classifier_matches_valid_kinds_and_rejects_malformed_names() {
        let field = FieldId {
            owner: ClassId {
                registry: 1,
                index: 0,
            },
            index: 0,
        };
        for (descriptor, kind) in [
            ("Z", FieldKind::Boolean),
            ("B", FieldKind::Byte),
            ("C", FieldKind::Char),
            ("S", FieldKind::Short),
            ("I", FieldKind::Int),
            ("J", FieldKind::Long),
            ("F", FieldKind::Float),
            ("D", FieldKind::Double),
            ("Ljava/lang/Object;", FieldKind::Reference),
            ("[[J", FieldKind::Reference),
        ] {
            assert_eq!(classify(field, &units(descriptor)).unwrap(), kind);
        }
        for descriptor in [
            "",
            "V",
            "()V",
            "[V",
            "L;",
            "L/foo;",
            "Lfoo/;",
            "Lfoo//bar;",
            "Lfoo.bar;",
            "Lfoo[bar;",
            "Lfoo;bar;",
            "Ljava/lang/Object",
            "II",
            "[Iextra",
        ] {
            assert!(
                matches!(
                    classify(field, &units(descriptor)),
                    Err(InstanceError::InvalidDescriptor { .. })
                ),
                "{descriptor}"
            );
        }
        assert_eq!(
            classify(field, &[76, 0xd800, 59]).unwrap(),
            FieldKind::Reference
        );
        let mut descriptor = vec![91; 255];
        descriptor.push(73);
        assert!(classify(field, &descriptor).is_ok());
        descriptor.insert(0, 91);
        assert!(classify(field, &descriptor).is_err());
    }
    #[test]
    fn policy_arithmetic_and_capacity_failures_do_not_wrap_or_destroy_live_objects() {
        for resource in [
            InstanceResource::LayoutClasses,
            InstanceResource::LayoutEdges,
            InstanceResource::LayoutFields,
            InstanceResource::Instances,
            InstanceResource::InstanceSlots,
            InstanceResource::TotalInstanceSlots,
        ] {
            assert_eq!(
                increased(usize::MAX, 1, usize::MAX, resource).unwrap_err(),
                InstanceError::ResourceLimit {
                    resource,
                    limit: usize::MAX,
                    required: usize::MAX
                }
            );
            assert_eq!(
                increased(usize::MAX, 0, usize::MAX, resource).unwrap(),
                usize::MAX
            );
            assert!(increased(0, 1, 0, resource).is_err());
        }
        let (mut heap, class, fields) = setup();
        let object = heap.allocate_instance(class).unwrap();
        heap.put_field(object, fields[0], Value::Int(42)).unwrap();
        let state = heap.instances.as_mut().unwrap();
        assert!(matches!(
            reserve_map(&mut state.shapes, usize::MAX),
            Err(InstanceError::Heap(HeapError::AllocationFailure { .. }))
        ));
        let EntryPayload::Instance(payload) = &mut heap.entries[0].payload else {
            panic!()
        };
        assert!(matches!(
            reserve(&mut payload.values, usize::MAX),
            Err(InstanceError::Heap(HeapError::AllocationFailure { .. }))
        ));
        assert_eq!(heap.get_field(object, fields[0]).unwrap(), Value::Int(42));
        assert_eq!(heap.allocated_instances(), 1);
        assert_eq!(heap.total_instance_slots(), 3);
        assert_eq!(heap.retained_layout_classes(), 2);
        assert_eq!(heap.allocated_arrays(), 0);
    }
    #[test]
    fn instance_identity_and_field_checks_reject_private_forgeries_before_writes() {
        let (mut heap, class, fields) = setup();
        let object = heap.allocate_instance(class).unwrap();
        let (owner, index, generation) = object.heap_ref().unwrap().components();
        for reference in [
            Reference::null(),
            Reference::issued(owner, usize::MAX, generation),
            Reference::issued(owner, index, 0),
            Reference::issued(0, index, generation),
        ] {
            assert!(matches!(
                heap.get_field(reference, fields[0]),
                Err(InstanceError::Heap(_))
            ));
            assert!(matches!(
                heap.put_field(reference, fields[0], Value::Int(7)),
                Err(InstanceError::Heap(_))
            ));
        }
        assert_eq!(
            heap.get_field(
                object,
                FieldId {
                    owner: ClassId {
                        registry: class.registry + 1,
                        index: 0
                    },
                    index: 0
                }
            )
            .unwrap_err(),
            InstanceError::ForeignRegistry
        );
        assert!(matches!(
            heap.get_field(
                object,
                FieldId {
                    owner: class,
                    index: usize::MAX
                }
            ),
            Err(InstanceError::MissingField { .. })
        ));
        assert!(matches!(
            heap.allocate_instance(ClassId {
                registry: class.registry,
                index: usize::MAX
            }),
            Err(InstanceError::LayoutMissing { .. })
        ));
        assert_eq!(heap.get_field(object, fields[0]).unwrap(), Value::Int(0));
        assert_eq!(
            heap.put_field(object, fields[2], Value::Reference(Reference::null()))
                .unwrap_err(),
            InstanceError::FinalField { field: fields[2] }
        );
        let array = heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
            .unwrap();
        assert!(matches!(
            heap.get_field(array, fields[0]),
            Err(InstanceError::Heap(HeapError::WrongEntryKind {
                expected: HeapEntryKind::Instance,
                actual: HeapEntryKind::PrimitiveArray
            }))
        ));
        for result in [
            heap.kind(object).map(|_| ()),
            heap.load(object, 0).map(|_| ()),
            heap.store(object, 0, Value::Int(1)),
        ] {
            assert_eq!(
                result.unwrap_err(),
                HeapError::WrongEntryKind {
                    expected: HeapEntryKind::PrimitiveArray,
                    actual: HeapEntryKind::Instance
                }
            );
        }
        assert_eq!(
            heap.length(object),
            Err(HeapError::WrongEntryKind {
                expected: HeapEntryKind::Array,
                actual: HeapEntryKind::Instance,
            })
        );
    }
    #[test]
    fn damaged_shapes_and_payloads_fail_with_bounded_walks_and_no_publication() {
        let (mut heap, class, fields) = setup();
        let original_parent = heap.instances.as_ref().unwrap().shapes[&class].superclass;
        heap.instances
            .as_mut()
            .unwrap()
            .shapes
            .get_mut(&class)
            .unwrap()
            .inherited = 1;
        assert!(matches!(
            heap.allocate_instance(class),
            Err(InstanceError::InvalidMetadata(_))
        ));
        assert_eq!(heap.allocated_instances(), 0);
        heap.instances
            .as_mut()
            .unwrap()
            .shapes
            .get_mut(&class)
            .unwrap()
            .inherited = 0;
        let object = heap.allocate_instance(class).unwrap();
        heap.instances
            .as_mut()
            .unwrap()
            .shapes
            .get_mut(&class)
            .unwrap()
            .superclass = Some(class);
        let unknown = FieldId {
            owner: original_parent.unwrap(),
            index: 0,
        };
        assert!(matches!(
            heap.get_field(object, unknown),
            Err(InstanceError::InvalidMetadata(_))
        ));
        assert!(matches!(
            heap.allocate_instance(class),
            Err(InstanceError::InvalidMetadata(_))
        ));
        heap.instances
            .as_mut()
            .unwrap()
            .shapes
            .get_mut(&class)
            .unwrap()
            .superclass = original_parent;
        heap.instances
            .as_mut()
            .unwrap()
            .shapes
            .remove(&original_parent.unwrap());
        assert!(matches!(
            heap.allocate_instance(class),
            Err(InstanceError::InvalidMetadata(_))
        ));
        let EntryPayload::Instance(payload) = &mut heap.entries[0].payload else {
            panic!()
        };
        payload.values.clear();
        assert!(matches!(
            heap.get_field(object, fields[0]),
            Err(InstanceError::InvalidMetadata(_))
        ));
        assert!(matches!(
            heap.put_field(object, fields[0], Value::Int(3)),
            Err(InstanceError::InvalidMetadata(_))
        ));
    }
    #[test]
    fn quota_overflow_and_payload_reservation_failure_preserve_counts_and_references() {
        let (mut heap, class, _) = setup();
        let state = heap.instances.as_mut().unwrap();
        state.options.max_instances = usize::MAX;
        state.count = usize::MAX;
        assert!(matches!(
            heap.allocate_instance(class),
            Err(InstanceError::ResourceLimit {
                resource: InstanceResource::Instances,
                required: usize::MAX,
                ..
            })
        ));
        assert!(heap.entries.is_empty());
        let state = heap.instances.as_mut().unwrap();
        state.count = 0;
        state.slots = usize::MAX;
        state.options.max_total_instance_slots = usize::MAX;
        assert!(matches!(
            heap.allocate_instance(class),
            Err(InstanceError::ResourceLimit {
                resource: InstanceResource::TotalInstanceSlots,
                required: usize::MAX,
                ..
            })
        ));
        assert!(heap.entries.is_empty());
        let state = heap.instances.as_mut().unwrap();
        state.slots = 0;
        state.options.max_instance_slots = usize::MAX;
        state.shapes.get_mut(&class).unwrap().slots = usize::MAX;
        assert!(matches!(
            heap.allocate_instance(class),
            Err(InstanceError::Heap(HeapError::AllocationFailure { .. }))
        ));
        assert!(heap.entries.is_empty());
        assert_eq!(heap.allocated_instances(), 0);
        assert_eq!(heap.total_instance_slots(), 0);
    }
    #[test]
    fn children_borrow_only_nonnull_reference_values_without_using_array_or_instance_quotas() {
        let (mut heap, class, fields) = setup();
        let object = heap.allocate_instance(class).unwrap();
        let array = heap
            .allocate_primitive_array(PrimitiveArrayKind::Long, 2)
            .unwrap();
        assert_eq!(heap.children(object).unwrap().count(), 0);
        assert_eq!(heap.children(array).unwrap().count(), 0);
        let EntryPayload::Instance(payload) = &mut heap.entries[0].payload else {
            panic!()
        };
        payload.values[1] = Value::Reference(array);
        assert_eq!(
            heap.children(object).unwrap().collect::<Vec<_>>(),
            vec![array.heap_ref().unwrap()]
        );
        assert_eq!(
            heap.put_field(object, fields[1], Value::Reference(object))
                .unwrap_err(),
            InstanceError::UnsupportedReferenceStore { field: fields[1] }
        );
        assert_eq!(
            heap.get_field(object, fields[1]).unwrap(),
            Value::Reference(array)
        );
        heap.validate_root(object.heap_ref().unwrap()).unwrap();
        heap.validate_root(array.heap_ref().unwrap()).unwrap();
        assert_eq!(heap.allocated_arrays(), 1);
        assert_eq!(heap.allocated_instances(), 1);
        assert_eq!(heap.payload_bytes(), 16);
        assert_eq!(heap.total_instance_slots(), 3);
    }

    #[test]
    fn field_context_rejects_forged_shape_flags_and_missing_or_extra_dependencies() {
        use crate::{FieldAccessError, FieldResolutionContext, FieldResolutionOptions};
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let object = registry
            .define(
                loader,
                &units("java/lang/Object"),
                &definition("java/lang/Object", false, &[]),
                ParseOptions::default(),
            )
            .unwrap();
        let class = registry
            .define(
                loader,
                &units("T"),
                &definition("T", true, &[]),
                ParseOptions::default(),
            )
            .unwrap();
        let plan = build_hierarchy(
            &registry,
            class,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        for corruption in 0..4 {
            let mut heap =
                ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                    .unwrap();
            heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
            let state = heap.instances.as_mut().unwrap();
            if corruption == 3 {
                state.shapes.remove(&object);
            } else {
                let shape = state.shapes.get_mut(&class).unwrap();
                match corruption {
                    0 => shape.flags ^= 0x200,
                    1 => shape.superclass = None,
                    _ => shape.interfaces.push(object),
                }
            }
            let mut resolver = SameLoaderResolver;
            let result = FieldResolutionContext::new_unnamed_modules(
                &registry,
                &mut heap,
                &mut resolver,
                FieldResolutionOptions::default(),
            );
            assert!(if corruption < 3 {
                matches!(result, Err(FieldAccessError::InvalidState(_)))
            } else {
                matches!(result, Err(FieldAccessError::Instance(InstanceError::LayoutMissing { class: missing })) if missing == object)
            });
        }
    }
}
// Fixed native field publication; the only writer of the bound pair.
impl ManagedHeap {
    pub(crate) fn throwable_binding_slots(
        &self,
        fields: [FieldId; 2],
        ordinals: [usize; 2],
    ) -> crate::throwable::R<[usize; 2]> {
        use crate::throwable::{ThrowableErrorKind as E, error};
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let shape = state.shape(fields[0].owner())?;
        let mut slots = [0; 2];
        for j in 0..2 {
            let own = shape
                .fields
                .get(ordinals[j])
                .ok_or_else(|| error(E::InvalidBinding("native field ordinal missing")))?;
            if own.id != fields[j]
                || own.is_final
                || own.kind
                    != if j == 0 {
                        FieldKind::Reference
                    } else {
                        FieldKind::Int
                    }
            {
                return Err(error(E::InvalidBinding("native field layout differs")));
            }
            slots[j] = shape
                .inherited
                .checked_add(ordinals[j])
                .filter(|slot| *slot < shape.slots)
                .ok_or_else(|| error(E::InvalidBinding("native field slot exceeds shape")))?
        }
        if slots[0] == slots[1] {
            return Err(error(E::InvalidBinding("native field slots alias")));
        }
        Ok(slots)
    }
    pub(crate) fn throwable_pair_slots(
        &self,
        receiver: Reference,
        fields: [FieldId; 2],
        steps: &mut usize,
        options: crate::FieldResolutionOptions,
    ) -> crate::throwable::R<(ClassId, [usize; 2])> {
        use crate::throwable::{ThrowableErrorKind as E, error};
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let EntryPayload::Instance(object) = &self.entry(receiver)?.payload else {
            return Err(error(E::InvalidReceiver));
        };
        let mut current = Some(object.class);
        for _ in 0..state.shapes.len() {
            let Some(id) = current else {
                return Err(error(E::InvalidReceiver));
            };
            crate::field::step(steps, options)?;
            let shape = state.shape(id)?;
            if id == fields[0].owner() {
                let mut slots = [None; 2];
                for (index, field) in shape.fields.iter().enumerate() {
                    crate::field::step(steps, options)?;
                    for j in 0..2 {
                        if field.id == fields[j] {
                            if field.is_final
                                || field.kind
                                    != if j == 0 {
                                        FieldKind::Reference
                                    } else {
                                        FieldKind::Int
                                    }
                            {
                                return Err(error(E::InvalidState("native field layout damaged")));
                            }
                            slots[j] = shape
                                .inherited
                                .checked_add(index)
                                .filter(|slot| *slot < shape.slots)
                        }
                    }
                    if slots.iter().all(Option::is_some) {
                        break;
                    }
                }
                let slots = [
                    slots[0]
                        .ok_or_else(|| error(E::InvalidState("native backtrace slot absent")))?,
                    slots[1].ok_or_else(|| error(E::InvalidState("native depth slot absent")))?,
                ];
                if slots[0] == slots[1] {
                    return Err(error(E::InvalidState("native slots alias")));
                }
                return Ok((object.class, slots));
            }
            current = shape.superclass;
        }
        Err(error(E::InvalidState("native receiver ancestry cyclic")))
    }
    pub(crate) fn throwable_pair(
        &self,
        receiver: Reference,
        actual: ClassId,
        slots: [usize; 2],
    ) -> crate::throwable::R<(Reference, i32)> {
        use crate::throwable::{ThrowableErrorKind as E, error};
        let EntryPayload::Instance(object) = &self.entry(receiver)?.payload else {
            return Err(error(E::InvalidReceiver));
        };
        if object.class != actual || slots[0] == slots[1] {
            return Err(error(E::InvalidCapture));
        }
        let Some(Value::Reference(token)) = object.values.get(slots[0]) else {
            return Err(error(E::InvalidState(
                "native backtrace slot kind or bounds differ",
            )));
        };
        let Some(Value::Int(depth)) = object.values.get(slots[1]) else {
            return Err(error(E::InvalidState(
                "native depth slot kind or bounds differ",
            )));
        };
        Ok((*token, *depth))
    }
    pub(crate) fn publish_backtrace(
        &mut self,
        receiver: Reference,
        actual: ClassId,
        slots: [usize; 2],
        payload: crate::throwable::Backtrace,
    ) -> crate::throwable::R<()> {
        use crate::throwable::{ThrowableErrorKind as E, ThrowableResource as R, error};
        let index = self.checked_handle(receiver.heap_ref().ok_or(HeapError::NullReference)?)?;
        self.throwable_pair(receiver, actual, slots)?;
        let trace_state = self.throwables.as_ref().ok_or_else(|| error(E::Disabled))?;
        if slots != trace_state.slots
            || payload.fingerprint != trace_state.fingerprint
            || payload.owner != receiver.heap_ref().ok_or(HeapError::NullReference)?
            || payload.actual != actual
        {
            return Err(error(E::InvalidCapture));
        }
        let state = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let count = increased(
            state.count,
            1,
            state.options.max_instances,
            InstanceResource::Instances,
        )?;
        let bytes = payload
            .frames
            .len()
            .checked_mul(std::mem::size_of::<crate::TraceFrame>())
            .ok_or_else(|| {
                error(E::Overflow {
                    resource: R::PayloadBytes,
                })
            })?;
        let total = crate::heap::increased(
            self.bytes,
            bytes,
            self.options.max_payload_bytes,
            crate::HeapResource::PayloadBytes,
        )?;
        let depth = i32::try_from(payload.frames.len()).map_err(|_| {
            error(E::UnrepresentableDepth {
                required: payload.frames.len() as u64,
            })
        })?;
        let traces = self.throwables.as_mut().ok_or_else(|| error(E::Disabled))?;
        let backtraces = traces
            .usage
            .backtraces
            .checked_add(1)
            .filter(|n| *n <= traces.usage.snapshots)
            .ok_or_else(|| error(E::InvalidState("published captures exceed snapshots")))?;
        traces.reserving()?;
        reserve_entries(&mut self.entries, 1)?;
        self.throwables
            .as_mut()
            .ok_or_else(|| error(E::Disabled))?
            .reserving()?;
        // Obtain every mutable destination before the first write. Arena capacity is
        // already reserved; publication performs only scalar stores and one push.
        let token = Reference::issued(self.identity, self.entries.len(), LIVE_GENERATION);
        let state = self.instances.as_mut().ok_or(InstanceError::Disabled)?;
        let traces = self.throwables.as_mut().ok_or_else(|| error(E::Disabled))?;
        let EntryPayload::Instance(object) = &mut self
            .entries
            .get_mut(index)
            .ok_or(HeapError::MissingEntry { index })?
            .payload
        else {
            return Err(error(E::InvalidReceiver));
        };
        let (low, high, reverse) = if slots[0] < slots[1] {
            (slots[0], slots[1], false)
        } else {
            (slots[1], slots[0], true)
        };
        let (before, after) = object
            .values
            .split_at_mut_checked(high)
            .ok_or_else(|| error(E::InvalidCapture))?;
        let a = before
            .get_mut(low)
            .ok_or_else(|| error(E::InvalidCapture))?;
        let b = after.first_mut().ok_or_else(|| error(E::InvalidCapture))?;
        if reverse {
            *a = Value::Int(depth);
            *b = Value::Reference(token)
        } else {
            *a = Value::Reference(token);
            *b = Value::Int(depth)
        };
        state.count = count;
        traces.usage.backtraces = backtraces;
        self.bytes = total;
        self.entries.push(Entry {
            generation: LIVE_GENERATION,
            string_backing: None,
            payload: EntryPayload::Backtrace(payload),
        });
        Ok(())
    }
}

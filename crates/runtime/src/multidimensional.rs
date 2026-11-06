//! Private geometry, staging and atomic publication for a single rooted array tree.
//! This heap child module shares the existing quota/payload implementation without
//! exposing raw allocation, plans or future handles outside the runtime.
use super::{
    Entry, EntryPayload, HeapError, HeapOptions, HeapResource, LIVE_GENERATION, ManagedHeap,
    Payload, increased, reserve_entries,
};
use crate::reference_array::{REFERENCE_ARRAY_ELEMENT_BYTES, ReferenceArrayPayload};
use crate::{
    ArrayAccessError, ArrayElement, ArrayJavaError, FieldResolutionContext, Reference,
    ReferenceType, TypeAccessError, TypeJavaError,
};
use crate::{ArrayOperandFault, OperandOutcome};

fn invalid(reason: &'static str) -> HeapError {
    HeapError::InvalidState(reason)
}
fn arena_failure() -> HeapError {
    HeapError::AllocationFailure {
        resource: "multidimensional arena indices",
    }
}
fn overflow(resource: HeapResource, limit: usize) -> HeapError {
    HeapError::ResourceLimit {
        resource,
        limit,
        required: usize::MAX,
    }
}
fn multiply(
    left: usize,
    right: usize,
    resource: HeapResource,
    limit: usize,
) -> Result<usize, HeapError> {
    left.checked_mul(right)
        .ok_or_else(|| overflow(resource, limit))
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    identity: u64,
    base: usize,
    arrays: usize,
    elements: usize,
    bytes: usize,
    options: HeapOptions,
}
impl Snapshot {
    fn capture(heap: &ManagedHeap) -> Self {
        Self {
            identity: heap.identity,
            base: heap.entries.len(),
            arrays: heap.arrays,
            elements: heap.elements,
            bytes: heap.bytes,
            options: heap.options,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Level {
    nodes: usize,
    elements: usize,
    start: usize,
    array_type: ReferenceType,
    length: i32,
}
#[derive(Debug, Eq, PartialEq)]
struct Geometry {
    before: Snapshot,
    requested: ReferenceType,
    counts: [i32; 255],
    dimensions: usize,
    levels: [Option<Level>; 255],
    depth: usize,
    nodes: usize,
    links: usize,
    arrays: usize,
    elements: usize,
    bytes: usize,
    end: usize,
}
impl Geometry {
    fn level(&self, index: usize) -> Result<Level, HeapError> {
        self.levels
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| invalid("multidimensional level is absent"))
    }
    fn build(
        before: Snapshot,
        requested: ReferenceType,
        counts: &[i32],
        mut step: impl FnMut() -> Result<(), ArrayAccessError>,
    ) -> Result<Self, ArrayAccessError> {
        let ReferenceType::Array {
            dimensions: rank,
            element,
            defining_loader,
        } = requested
        else {
            return Err(invalid("multidimensional target is not an array").into());
        };
        if rank == 0 || counts.is_empty() || counts.len() > usize::from(rank) || counts.len() > 255
        {
            return Err(invalid("invalid multidimensional dimension shape").into());
        }
        let mut geometry = Self {
            before,
            requested,
            counts: [0; 255],
            dimensions: counts.len(),
            levels: [None; 255],
            depth: 0,
            nodes: 0,
            links: 0,
            arrays: before.arrays,
            elements: before.elements,
            bytes: before.bytes,
            end: before.base,
        };
        geometry
            .counts
            .get_mut(..counts.len())
            .ok_or_else(|| invalid("dimension snapshot is too short"))?
            .copy_from_slice(counts);
        let mut nodes = 1usize;
        for (depth, &length) in counts.iter().enumerate() {
            step()?;
            let size = usize::try_from(length).map_err(|_| invalid("negative preflight length"))?;
            geometry.arrays = increased(
                geometry.arrays,
                nodes,
                before.options.max_arrays,
                HeapResource::Arrays,
            )?;
            increased(
                0,
                size,
                before.options.max_array_length,
                HeapResource::ArrayLength,
            )?;
            let elements = multiply(
                nodes,
                size,
                HeapResource::TotalElements,
                before.options.max_total_elements,
            )?;
            geometry.elements = increased(
                geometry.elements,
                elements,
                before.options.max_total_elements,
                HeapResource::TotalElements,
            )?;
            let actual_rank = rank
                .checked_sub(
                    u8::try_from(depth).map_err(|_| invalid("preflight depth exceeds rank"))?,
                )
                .filter(|rank| *rank > 0)
                .ok_or_else(|| invalid("preflight rank underflow"))?;
            let array_type = ReferenceType::Array {
                dimensions: actual_rank,
                element,
                defining_loader,
            };
            let width = match (actual_rank, element) {
                (1, ArrayElement::Primitive(kind)) => kind.element_bytes(),
                _ => REFERENCE_ARRAY_ELEMENT_BYTES,
            };
            let bytes = multiply(
                elements,
                width,
                HeapResource::PayloadBytes,
                before.options.max_payload_bytes,
            )?;
            geometry.bytes = increased(
                geometry.bytes,
                bytes,
                before.options.max_payload_bytes,
                HeapResource::PayloadBytes,
            )?;
            let level = Level {
                nodes,
                elements,
                start: geometry.nodes,
                array_type,
                length,
            };
            *geometry
                .levels
                .get_mut(depth)
                .ok_or_else(|| invalid("too many allocation levels"))? = Some(level);
            geometry.nodes = geometry
                .nodes
                .checked_add(nodes)
                .ok_or_else(|| overflow(HeapResource::Arrays, before.options.max_arrays))?;
            geometry.depth = depth
                .checked_add(1)
                .ok_or_else(|| invalid("allocation depth overflow"))?;
            if size == 0 || depth + 1 == counts.len() {
                break;
            }
            geometry.links = geometry
                .links
                .checked_add(elements)
                .ok_or_else(|| invalid("internal link count overflow"))?;
            nodes = elements;
        }
        if geometry.nodes.checked_sub(1) != Some(geometry.links) {
            return Err(invalid("allocation is not one rooted tree").into());
        }
        geometry.end = before
            .base
            .checked_add(geometry.nodes)
            .ok_or_else(arena_failure)?;
        Ok(geometry)
    }
    fn expected_child(
        &self,
        depth: usize,
        parent: usize,
        slot: usize,
    ) -> Result<Reference, HeapError> {
        if depth
            .checked_add(1)
            .ok_or_else(|| invalid("child depth overflow"))?
            >= self.depth
        {
            return Ok(Reference::null());
        }
        let level = self.level(depth)?;
        let next = self.level(depth + 1)?;
        let length =
            usize::try_from(level.length).map_err(|_| invalid("negative stored level length"))?;
        if parent >= level.nodes || slot >= length || next.nodes != level.elements {
            return Err(invalid("invalid parent/child geometry"));
        }
        let ordinal = parent
            .checked_mul(length)
            .and_then(|start| start.checked_add(slot))
            .ok_or_else(|| invalid("child ordinal overflow"))?;
        if ordinal >= next.nodes {
            return Err(invalid("child ordinal outside next level"));
        }
        let staged = next.start.checked_add(ordinal).ok_or_else(arena_failure)?;
        let index = self
            .before
            .base
            .checked_add(staged)
            .filter(|index| *index < self.end)
            .ok_or_else(arena_failure)?;
        Ok(Reference::issued(
            self.before.identity,
            index,
            LIVE_GENERATION,
        ))
    }
}
fn payload(array_type: ReferenceType, length: usize) -> Result<EntryPayload, HeapError> {
    match array_type {
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Primitive(kind),
            ..
        } => Ok(EntryPayload::Array(Payload::new(kind, length)?)),
        ReferenceType::Array { .. } => Ok(EntryPayload::ReferenceArray(
            ReferenceArrayPayload::new(array_type, length)?,
        )),
        _ => Err(invalid("staged node type is not an array")),
    }
}
struct Staged {
    geometry: Geometry,
    entries: Vec<Entry>,
}
impl Staged {
    fn build(
        geometry: Geometry,
        mut make: impl FnMut(ReferenceType, usize) -> Result<EntryPayload, HeapError>,
    ) -> Result<Self, HeapError> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(geometry.nodes)
            .map_err(|_| HeapError::AllocationFailure {
                resource: "multidimensional staging metadata",
            })?;
        for depth in 0..geometry.depth {
            let level = geometry.level(depth)?;
            let length = usize::try_from(level.length)
                .map_err(|_| invalid("negative staged node length"))?;
            for _ in 0..level.nodes {
                entries.push(Entry {
                    generation: LIVE_GENERATION,
                    string_backing: None,
                    payload: make(level.array_type, length)?,
                });
            }
        }
        for depth in 0..geometry.depth.saturating_sub(1) {
            let level = geometry.level(depth)?;
            for parent in 0..level.nodes {
                let staged = level.start.checked_add(parent).ok_or_else(arena_failure)?;
                let EntryPayload::ReferenceArray(node) = &mut entries
                    .get_mut(staged)
                    .ok_or_else(|| invalid("staged parent is missing"))?
                    .payload
                else {
                    return Err(invalid("primitive parent has allocated children"));
                };
                for (slot, value) in node.values.iter_mut().enumerate() {
                    *value = geometry.expected_child(depth, parent, slot)?;
                }
            }
        }
        Ok(Self { geometry, entries })
    }
    fn validate(&self, heap: &ManagedHeap) -> Result<(), ArrayAccessError> {
        let geometry = &self.geometry;
        if Snapshot::capture(heap) != geometry.before {
            return Err(invalid("heap changed before multidimensional publication").into());
        }
        let counts = geometry
            .counts
            .get(..geometry.dimensions)
            .ok_or_else(|| invalid("invalid retained dimension count"))?;
        if counts.iter().any(|count| *count < 0) {
            return Err(invalid("negative retained dimension before publication").into());
        }
        let expected = Geometry::build(geometry.before, geometry.requested, counts, || Ok(()))?;
        if expected != *geometry || self.entries.len() != geometry.nodes {
            return Err(invalid("staged geometry differs from checked allocation").into());
        }
        for depth in 0..geometry.depth {
            let level = geometry.level(depth)?;
            for ordinal in 0..level.nodes {
                let staged = level.start.checked_add(ordinal).ok_or_else(arena_failure)?;
                let entry = self
                    .entries
                    .get(staged)
                    .ok_or_else(|| invalid("staged node is missing"))?;
                if entry.string_backing.is_some() {
                    return Err(invalid("staged array cannot be protected String storage").into());
                }
                if entry.generation != LIVE_GENERATION {
                    return Err(invalid("staged generation is invalid").into());
                }
                match (&entry.payload, level.array_type) {
                    (
                        EntryPayload::Array(actual),
                        ReferenceType::Array {
                            dimensions: 1,
                            element: ArrayElement::Primitive(kind),
                            ..
                        },
                    ) if actual.kind() == kind && actual.length()? == level.length => {}
                    (EntryPayload::ReferenceArray(actual), _)
                        if actual.array_type == level.array_type
                            && actual.length()? == level.length =>
                    {
                        for (slot, &value) in actual.values.iter().enumerate() {
                            if value != geometry.expected_child(depth, ordinal, slot)? {
                                return Err(invalid(
                                    "staged reference is not its exact expected child or null",
                                )
                                .into());
                            }
                        }
                    }
                    _ => {
                        return Err(invalid(
                            "staged payload type or length differs from its level",
                        )
                        .into());
                    }
                }
            }
        }
        Ok(())
    }
    fn publish(self, heap: &mut ManagedHeap) -> Result<Reference, ArrayAccessError> {
        self.publish_after(heap, || Ok(()))
    }
    // The only production caller supplies a no-op. Private tests inject a failure
    // after reservation without exposing staging, future handles or a public option.
    fn publish_after(
        mut self,
        heap: &mut ManagedHeap,
        before_publish: impl FnOnce() -> Result<(), HeapError>,
    ) -> Result<Reference, ArrayAccessError> {
        self.validate(heap)?;
        reserve_entries(&mut heap.entries, self.geometry.nodes)?;
        before_publish()?;
        let geometry = self.geometry;
        heap.entries.append(&mut self.entries);
        heap.arrays = geometry.arrays;
        heap.elements = geometry.elements;
        heap.bytes = geometry.bytes;
        Ok(Reference::issued(
            heap.identity,
            geometry.before.base,
            LIVE_GENERATION,
        ))
    }
}
impl FieldResolutionContext<'_> {
    pub fn allocate_multidimensional_array(
        &mut self,
        array_type: ReferenceType,
        counts: &[i32],
    ) -> Result<Reference, ArrayAccessError> {
        crate::reference_array::project_array(
            self.allocate_multidimensional_array_detailed(array_type, counts)?,
        )
    }
    pub fn allocate_multidimensional_array_detailed(
        &mut self,
        array_type: ReferenceType,
        counts: &[i32],
    ) -> Result<OperandOutcome<Reference, ArrayOperandFault>, ArrayAccessError> {
        self.unchanged().map_err(TypeAccessError::from)?;
        let universe = self.universe()?;
        self.validate_reference_type(array_type, universe)?;
        let ReferenceType::Array {
            dimensions: rank, ..
        } = array_type
        else {
            return Err(
                TypeAccessError::InvalidType("multidimensional target must be an array").into(),
            );
        };
        if counts.is_empty() || counts.len() > usize::from(rank) {
            return Err(TypeAccessError::InvalidType(
                "requested dimension count must be 1 through array rank",
            )
            .into());
        }
        for (dimension, &count) in counts.iter().enumerate() {
            self.step().map_err(TypeAccessError::from)?;
            if count < 0 {
                return Ok(OperandOutcome::Fault(ArrayOperandFault::NegativeSize {
                    length: count,
                    dimension: u8::try_from(dimension).map_err(|_| {
                        TypeAccessError::InvalidType("negative dimension exceeds representation")
                    })?,
                }));
            }
        }
        let before = Snapshot::capture(self.heap());
        let geometry = Geometry::build(before, array_type, counts, || {
            self.step()
                .map_err(TypeAccessError::from)
                .map_err(ArrayAccessError::from)
        })?;
        self.charge_bulk_work(geometry.nodes, geometry.links)
            .map_err(TypeAccessError::from)?;
        Staged::build(geometry, payload)?
            .publish(self.heap_mut())
            .map(OperandOutcome::Value)
    }
    pub fn new_multidimensional_array(
        &mut self,
        origin: crate::ClassId,
        index: u16,
        counts: &[i32],
    ) -> Result<Reference, ArrayAccessError> {
        crate::reference_array::project_array(
            self.new_multidimensional_array_detailed(origin, index, counts)?,
        )
    }
    pub fn new_multidimensional_array_detailed(
        &mut self,
        origin: crate::ClassId,
        index: u16,
        counts: &[i32],
    ) -> Result<OperandOutcome<Reference, ArrayOperandFault>, ArrayAccessError> {
        let array_type =
            self.resolve_reference_type(origin, index)
                .map_err(|error| match error {
                    TypeAccessError::Java(TypeJavaError::IllegalAccess) => {
                        ArrayAccessError::Java(ArrayJavaError::IllegalAccess)
                    }
                    error => error.into(),
                })?;
        self.allocate_multidimensional_array_detailed(array_type, counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(rank: u8) -> ReferenceType {
        ReferenceType::Array {
            dimensions: rank,
            element: ArrayElement::Primitive(crate::PrimitiveArrayKind::Int),
            defining_loader: crate::LoaderId {
                registry: 1,
                index: 0,
            },
        }
    }
    fn heap() -> (ManagedHeap, Reference) {
        let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        let old = heap
            .allocate_primitive_array(crate::PrimitiveArrayKind::Long, 1)
            .unwrap();
        heap.store(old, 0, crate::Value::Long(77)).unwrap();
        (heap, old)
    }
    fn geometry(heap: &ManagedHeap, rank: u8, counts: &[i32]) -> Geometry {
        Geometry::build(Snapshot::capture(heap), target(rank), counts, || Ok(())).unwrap()
    }
    #[test]
    fn future_handles_remain_uncommitted_until_one_validated_publication() {
        let (mut heap, old) = heap();
        let staged = Staged::build(geometry(&heap, 2, &[2, 3]), payload).unwrap();
        let EntryPayload::ReferenceArray(root) = &staged.entries[0].payload else {
            panic!()
        };
        let future = root.values[0];
        assert!(matches!(
            heap.validate_root(future.heap_ref().unwrap()),
            Err(HeapError::MissingEntry { .. })
        ));
        assert_eq!(heap.entries.len(), 1);
        let root = staged.publish(&mut heap).unwrap();
        assert_eq!(heap.entries.len(), 4);
        let child = heap.load_reference(root, 0).unwrap();
        assert_eq!(heap.length(child), Ok(3));
        assert_eq!(heap.load(child, 0), Ok(crate::Value::Int(0)));
        assert_eq!(heap.load(old, 0), Ok(crate::Value::Long(77)));
        assert_eq!(
            (heap.arrays, heap.elements, heap.bytes),
            (4, 9, 8 + 2 * REFERENCE_ARRAY_ELEMENT_BYTES + 24)
        );
    }
    #[test]
    fn corrupt_geometry_generations_types_lengths_and_exact_links_cannot_publish() {
        for case in 0..25 {
            let (mut heap, old) = heap();
            let mut before = Snapshot::capture(&heap);
            let mut staged = Staged::build(geometry(&heap, 2, &[2, 3]), payload).unwrap();
            match case {
                0 => {
                    staged.entries.pop();
                }
                1 => staged.entries.push(Entry {
                    generation: LIVE_GENERATION,
                    string_backing: None,
                    payload: payload(target(1), 3).unwrap(),
                }),
                2 => staged.entries[0].generation = LIVE_GENERATION + 1,
                3 => {
                    let EntryPayload::ReferenceArray(root) = &mut staged.entries[0].payload else {
                        panic!()
                    };
                    root.array_type = target(3);
                }
                4 | 5 | 6 | 14 => {
                    let EntryPayload::ReferenceArray(root) = &mut staged.entries[0].payload else {
                        panic!()
                    };
                    root.values[0] = match case {
                        4 => Reference::issued(heap.identity + 1, 2, LIVE_GENERATION),
                        5 => Reference::issued(heap.identity, usize::MAX, LIVE_GENERATION),
                        6 => old,
                        _ => Reference::null(),
                    };
                }
                7 => {
                    staged.entries[1].payload = EntryPayload::Array(
                        Payload::new(crate::PrimitiveArrayKind::Long, 3).unwrap(),
                    )
                }
                8 => staged.geometry.levels[1].as_mut().unwrap().start = usize::MAX,
                9 => staged.geometry.before.base = usize::MAX,
                10 => staged.geometry.before.identity += 1,
                11 => staged.geometry.arrays += 1,
                12 => staged.geometry.counts[0] += 1,
                13 => staged.geometry.counts[1] = -1,
                15 => staged.geometry.requested = target(0),
                16 => staged.geometry.dimensions = 256,
                17 => staged.geometry.nodes = usize::MAX,
                18 => {
                    let EntryPayload::ReferenceArray(root) = &mut staged.entries[0].payload else {
                        panic!()
                    };
                    root.values.truncate(1);
                }
                19 => staged.geometry.before.arrays += 1,
                20 => {
                    heap.arrays += 1;
                    before = Snapshot::capture(&heap);
                }
                21 => {
                    heap.allocate_primitive_array(crate::PrimitiveArrayKind::Byte, 1)
                        .unwrap();
                    before = Snapshot::capture(&heap);
                }
                22 => staged.entries[1].generation += 1,
                23 => {
                    let EntryPayload::ReferenceArray(root) = &mut staged.entries[0].payload else {
                        panic!()
                    };
                    let child = root.values[0].heap_ref().unwrap().components();
                    root.values[0] = Reference::issued(child.0, child.1, child.2 + 1);
                }
                24 => {
                    let EntryPayload::ReferenceArray(root) = &mut staged.entries[0].payload else {
                        panic!()
                    };
                    root.values[0] = root.values[1];
                }
                _ => unreachable!(),
            }
            assert!(staged.publish(&mut heap).is_err(), "corruption {case}");
            assert_eq!(Snapshot::capture(&heap), before);
            assert_eq!(heap.load(old, 0), Ok(crate::Value::Long(77)));
        }
        let (mut heap, old) = heap();
        let before = Snapshot::capture(&heap);
        let mut staged = Staged::build(geometry(&heap, 3, &[2]), payload).unwrap();
        let EntryPayload::ReferenceArray(root) = &mut staged.entries[0].payload else {
            panic!()
        };
        root.values[0] = old;
        assert!(staged.publish(&mut heap).is_err());
        assert_eq!(Snapshot::capture(&heap), before);
        let mut staged = Staged::build(geometry(&heap, 3, &[0, 1]), payload).unwrap();
        staged.geometry.counts[1] = -1;
        assert!(staged.publish(&mut heap).is_err());
        assert_eq!(Snapshot::capture(&heap), before);
    }
    #[test]
    fn injected_payload_and_prepublication_failures_drop_only_temporary_staging() {
        let (mut heap, old) = heap();
        let old_parent = heap
            .allocate_reference_array_payload(
                ReferenceType::Array {
                    dimensions: 2,
                    element: ArrayElement::Primitive(crate::PrimitiveArrayKind::Long),
                    defining_loader: crate::LoaderId {
                        registry: 1,
                        index: 0,
                    },
                },
                1,
            )
            .unwrap();
        heap.commit_reference_array(old_parent, 0, old).unwrap();
        let old_edges = heap.children(old_parent).unwrap().collect::<Vec<_>>();
        let before = Snapshot::capture(&heap);
        for fail_at in 1..=3 {
            let mut call = 0;
            let staged = Staged::build(geometry(&heap, 2, &[2, 3]), |ty, length| {
                call += 1;
                if call == fail_at {
                    Err(HeapError::AllocationFailure {
                        resource: "injected staged payload",
                    })
                } else {
                    payload(ty, length)
                }
            });
            assert!(staged.is_err());
            assert_eq!(Snapshot::capture(&heap), before);
            assert_eq!(heap.load(old, 0), Ok(crate::Value::Long(77)));
            assert_eq!(
                heap.children(old_parent).unwrap().collect::<Vec<_>>(),
                old_edges
            );
        }
        let staged = Staged::build(geometry(&heap, 2, &[2, 3]), payload).unwrap();
        assert_eq!(
            staged.publish_after(&mut heap, || Err(HeapError::AllocationFailure {
                resource: "injected before publication"
            })),
            Err(ArrayAccessError::Heap(HeapError::AllocationFailure {
                resource: "injected before publication"
            }))
        );
        assert_eq!(Snapshot::capture(&heap), before);
        assert_eq!(heap.load(old, 0), Ok(crate::Value::Long(77)));
        assert_eq!(
            heap.children(old_parent).unwrap().collect::<Vec<_>>(),
            old_edges
        );
    }
    #[test]
    fn arithmetic_and_future_index_overflow_guards_are_structured() {
        for resource in [HeapResource::TotalElements, HeapResource::PayloadBytes] {
            assert_eq!(
                multiply(usize::MAX, 2, resource, usize::MAX),
                Err(overflow(resource, usize::MAX))
            );
        }
        let (heap, _) = heap();
        for resource in [
            HeapResource::Arrays,
            HeapResource::TotalElements,
            HeapResource::PayloadBytes,
        ] {
            let mut before = Snapshot::capture(&heap);
            before.options = HeapOptions {
                max_arrays: usize::MAX,
                max_array_length: usize::MAX,
                max_total_elements: usize::MAX,
                max_payload_bytes: usize::MAX,
            };
            match resource {
                HeapResource::Arrays => before.arrays = usize::MAX,
                HeapResource::TotalElements => before.elements = usize::MAX,
                HeapResource::PayloadBytes => before.bytes = usize::MAX,
                _ => unreachable!(),
            };
            assert_eq!(
                Geometry::build(before, target(1), &[1], || Ok(())).unwrap_err(),
                ArrayAccessError::Heap(overflow(resource, usize::MAX))
            );
        }
        let mut before = Snapshot::capture(&heap);
        before.base = usize::MAX;
        assert_eq!(
            Geometry::build(before, target(1), &[1], || Ok(())).unwrap_err(),
            ArrayAccessError::Heap(arena_failure())
        );
        let mut geometry = geometry(&heap, 2, &[2, 3]);
        assert!(geometry.expected_child(0, usize::MAX, 0).is_err());
        assert!(geometry.expected_child(0, 0, usize::MAX).is_err());
        geometry.levels[1].as_mut().unwrap().start = usize::MAX;
        assert!(geometry.expected_child(0, 0, 0).is_err());
    }
}

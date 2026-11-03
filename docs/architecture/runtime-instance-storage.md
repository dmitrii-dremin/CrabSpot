# Ordinary-instance storage and field service

Status: implemented and approved in `runtime-instance-storage-review.md` on 2026-11-03. This contract supersedes the draft in `runtime-instance-storage-next.md`.

## Scope and placement

Extend the runtime heap to retain ordinary class instances alongside primitive arrays in the same managed-reference namespace. This is a memory service consuming registered metadata and checked hierarchy plans. Allocation supplies default field storage; it does not run a constructor, initialize a class, verify bytecode or prove complete Java linking. Do not expose this operation as execution of `new`, add a final-field privilege flag, or extend the interpreter in this increment.

Keep `HeapOptions` and `ManagedHeap::new(HeapOptions)` unchanged. That constructor leaves the instance service disabled. Add `ManagedHeap::new_with_instances(array_options: HeapOptions, instance_options: InstanceOptions) -> Result<Self, HeapError>`. Policies are fixed for the heap's lifetime. Existing primitive-array quotas, counters and signatures retain their meaning, even when object entries share the arena. `allocated_arrays()` must count arrays rather than all arena entries; `payload_bytes()` continues to mean primitive-array payload bytes.

Use `runtime::instance` for layout/field types and implementation helpers if useful, with public reexports from runtime. Keep reference issuance private to the runtime and exclusively behind successful heap entry publication. Do not create an empty crate or a second reference namespace for objects.

## Concrete API and limits

`InstanceOptions` has the following usize fields and defaults:

| Field | Default | Meaning |
| --- | ---: | --- |
| `max_layout_classes` | 1,024 | Unique retained class/interface shapes |
| `max_layout_edges` | 8,192 | Retained direct superclass and interface edges |
| `max_layout_fields` | 65,536 | Retained own instance-field descriptors/handles |
| `max_instances` | 4,096 | Allocated ordinary-instance entries |
| `max_instance_slots` | 65,536 | Logical field locations per allocated instance |
| `max_total_instance_slots` | 1,048,576 | Aggregate allocated logical instance locations |

Add these ManagedHeap methods:

- `admit_instance_layouts(&mut self, registry: &DefinitionRegistry, plans: &[&HierarchyPlan]) -> Result<(), InstanceError>`
- `allocate_instance(&mut self, class: ClassId) -> Result<Reference, InstanceError>`
- `instance_class(&self, reference: Reference) -> Result<ClassId, InstanceError>`
- `get_field(&self, reference: Reference, field: FieldId) -> Result<Value, InstanceError>`
- `put_field(&mut self, reference: Reference, field: FieldId, value: Value) -> Result<(), InstanceError>`
- `allocated_instances() -> usize`, `total_instance_slots() -> usize`
- `retained_layout_classes() -> usize`, `retained_layout_edges() -> usize`, `retained_layout_fields() -> usize`
- `children(&self, reference: Reference) -> Result<impl Iterator<Item = HeapRef> + '_, HeapError>`

Instance/layout counters are zero when disabled. Instance operations, including empty admission, return `InstanceError::Disabled` when disabled. With the service enabled, empty admission is an unconditional no-op and does not bind a registry. The children iterator is a general heap operation and also works on arrays from the original constructor.

Limits count specified logical resources, not exact host memory. Long/Double consume one instance location. Count before allocation with checked arithmetic, and reserve vectors/maps fallibly. A large allowance must not trigger eager allocation. No instance limit consumes array quotas, and no array consumes instance quotas. Shared arena growth still uses checked, fallible reservation.

## Atomic multi-plan admission

One configured heap admits shapes from one definition registry. The first successful nonempty admission binds that registry; a failed admission must not bind it. All ClassId/FieldId values consumed from supplied plans must be checked against the supplied registry. Later nonempty admissions must belong to the same registry.

Process the union of all plans in one call as a single transaction. Deduplicate by ClassId across both the batch and previously retained shapes. An identical shape is a no-op and consumes no additional quota. The same ClassId with a different resolved direct superclass, ordered interface identities, flags, own instance fields, inherited count or total slot count is a conflict. Reject the entire batch, including when two new plans conflict with each other. Distinct defining-loader identities remain distinct even if names match.

Compare each plan's relationships against existing/batch relationships, not merely its root or total field count. Two same-named delegated parents with equal slot counts still represent different shapes. All dependency shapes must be present in the admitted union. Validate retained own field handles, declaration order, owner, nonstatic category, descriptors and layout arithmetic against immutable registry metadata and the dependency shapes. Missing/corrupt internal relationships are errors, never zero-field fallbacks.

Retain each class's own instance fields once, plus parent links, direct interfaces and prefix/total counts. Static fields contribute no instance locations or retained-field quota. Interface Object dependencies remain graph edges but contribute no inherited instance storage. Do not retain whole cloned plans or flattened ancestor field metadata per class. Temporary descriptor classification should use scalar `(ClassId, constant-pool index)` keys rather than repeatedly hashing/copying shared long descriptor text.

Check all proposed unique-class, edge and own-field totals and conflicts before commit. Stage new metadata, reserve destination capacity, then publish without remaining fallible work. Failure preserves registry binding, all logical counters, existing shapes and object locations. Allocator capacity changes during failed preparation are not logical publication and need not be rolled back. Sequential and batched admission must yield the same field interpretation; re-admitting a shape must never reinterpret live objects.

## Allocation and field access

Allocation requires a retained concrete class. Interface and abstract definitions may be retained as dependencies but cannot be allocated directly. Reject a missing/foreign class explicitly. Traverse superclass links iteratively and create a value vector covering inherited fields followed by own fields in the agreed logical slot order. Do not recurse during admission, allocation, query or destruction; deep layouts must remain safe. Per-instance and aggregate slot limits are checked before payload allocation. A zero-field instance still consumes a distinct instance entry.

Defaults are Int(0) for Z/B/C/S/I, Long(0), Float(0), Double(0), and Reference(null) for object/array fields. Floating zeros are positive. Ignore ConstantValue, including instance ConstantValue metadata, and run no `<init>`/`<clinit>` code. Publish counters and a validated non-null reference only after field payload and arena preparation succeed.

Field lookup follows the receiver's retained superclass chain to the FieldId's declaring owner and uses that owner's prefix plus own-field position. Hidden fields remain distinct. Foreign, static, unretained or unrelated fields fail; a matching name is never sufficient. Validate reference ownership/index/generation and require an instance entry before accessing fields. Field-service methods do not resolve symbolic Fieldref entries or implement Java accessibility rules; the future execution bridge owns those responsibilities.

All final-field writes fail, even a write of the existing default. For nonfinal fields require the exact computational Value kind. Z stores mask with one; B/S stores truncate and sign extend; C stores truncate and zero extend; I/J preserve their width; F/D preserve raw bits. Reference fields accept null only. Any non-null reference value, whether local or foreign, yields UnsupportedReferenceStore without dereferencing it; assignability and non-null field mutation are not implemented. Numeric fields given Reference still yield ValueKindMismatch. Failed writes leave the prior value unchanged.

The subword rule is an explicit service contract with JDK 21 differential acceptance. [JVMS SE 21 putfield](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.putfield) describes computational kinds and Boolean masking; the [SE 22 wording](https://docs.oracle.com/javase/specs/jvms/se22/html/jvms-6.html#jvms-6.5.putfield) explicitly describes B/C/S truncation. The later wording is a semantic clarification to cross-check against raw-store JDK 21 tests, not a declaration of Java 22 support.

## Errors and child references

Add `HeapEntryKind::{PrimitiveArray, Instance}` and `HeapError::WrongEntryKind { expected: HeapEntryKind, actual: HeapEntryKind }`. Existing array kind/length/load/store methods reject an instance through this error. Identity validation and `validate_root` work for either entry kind. Generalize array-specific null/value-mismatch diagnostic wording where needed without changing error categories.

Define `InstanceResource::{LayoutClasses, LayoutEdges, LayoutFields, Instances, InstanceSlots, TotalInstanceSlots}` and `InstanceClassKind::{Interface, Abstract}`. `InstanceError` has these variants:

- `Disabled`
- `Registry(RegistryError)` and `Heap(HeapError)`
- `ForeignRegistry`
- `LayoutMissing { class: ClassId }` and `LayoutConflict { class: ClassId }`
- `NonInstantiable { class: ClassId, kind: InstanceClassKind }` (interface classification takes precedence)
- `MissingField { class: ClassId, field: FieldId }`
- `FinalField { field: FieldId }` and `UnsupportedReferenceStore { field: FieldId }`
- `InvalidDescriptor { field: FieldId, error: ParseError }`
- `InvalidMetadata(&'static str)`
- `ResourceLimit { resource: InstanceResource, limit: usize, required: usize }`

Wrap allocation failures and ValueKindMismatch in `InstanceError::Heap`; retain source errors in Display/Error implementations. An overflow reports the relevant resource with required usize::MAX, following existing policy errors. Handles from a foreign registry supplied to instance operations return ForeignRegistry; missing same-registry shapes/fields use the corresponding missing variant. Invalid supplied-plan handles discovered through registry lookup may return the wrapped Registry error. These are host-service errors, not Java linkage/instantiation/access exceptions.

For field operations, check enabled state, receiver identity/kind, field registry/relationship, then finality (writes), value kind and non-null restriction in that order. No ordering among independent admission defects is promised. The children iterator validates the entry and borrows non-null reference-field values without allocation. Primitive arrays yield no children. Public instance children remain empty because non-null stores are unsupported; private tests may seed a valid issued reference to check filtering. Children are neither caller roots nor a collector, retention policy or safepoint.

## Acceptance and next gate

Public tests cover multiple unrelated roots, shared ancestors, batch and sequential deduplication, same-ClassId/different-delegated-parent conflicts (including equal slot counts), interface-edge conflicts, late batch failure atomicity and subsequent recovery. Check all quotas at zero/exact/cumulative/maximum allowances using small real allocations, deep parent chains, concrete/abstract/interface behavior, foreign identities, arrays/instances sharing one arena, empty instances and old-reference stability after layout admission.

Cover every field descriptor/default, inherited hiding, all numeric kinds/subword conversions/raw floating bits, ignored ConstantValue, static/unrelated/final rejection, null-only reference writes and child iteration. Private tests cover forged handles, damaged shape invariants, arithmetic overflow and deterministic reservation failures without host exhaustion.

Use genuine compiled metadata and an independently generated JDK 21 oracle for field conversions, including synthetic raw B/C/S/Z putfield instructions that javac may otherwise precede with conversions. This tests memory-service results, not CrabSpot field-bytecode execution. Preserve all 8,285 existing execution outcomes, 151 array-service round trips, fixture reproduction and old constructor behavior. Require formatting, strict workspace/all-target Clippy, full debug tests, optimized runtime/interpreter tests and technical-director source review before accepting this increment or authorizing a field execution bridge.

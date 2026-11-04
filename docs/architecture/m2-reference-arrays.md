# Reference-array storage and execution

Status: bounded implementation approved by the technical director on 2026-11-04 after frozen review and the passing integrated gate recorded in `m2-reference-arrays-review.md`. Implementation used separate runtime/interpreter worktrees based on f8a872a. Fresh hosted validation remains pending publication. Full M2 remains open. No NanoSpec.

## Scope and representation

Add actual reference-array storage and runtime-aware anewarray (bd), aaload (32) and aastore (53). Retain the existing engine, explicit universe, admitted graph, loader identities, binding cache, work limits, numeric/void method signatures and host-selected instance entry point. Runtime-aware coverage becomes 189 effective opcodes. multianewarray (c5), reference parameters/results, constructors, static writes, initialization, mirrors, handlers, verification and collection remain outside this increment.

Use a distinct heap entry payload containing an immutable complete ReferenceType::Array and Vec<Reference>, initialized entirely to null. Retain the actual array type, not merely the component ClassId, a caller's field type or a descriptor spelling. An ordinary nominal component produces rank-one ArrayElement::Class with that class's defining loader. An array component produces an array with one additional rank, preserving its leaf and defining loader. A primitive component itself cannot be expressed by ReferenceType; rank-one primitive arrays continue to use the old payload. A rank-two primitive-leaf array is a reference array whose elements may hold rank-one primitive arrays.

All supplied types must pass existing universe, registry, admitted nominal identity, nonzero rank and leaf-loader validation before shortcuts. Array-component rank 254 can create rank 255. Rank 255 as a component is rejected with TypeAccessError::InvalidType before length checking or allocation, including length zero or negative. Never wrap rank to zero, synthesize a ClassId, resolve an array by name-only core identity or recursively allocate components.

Reference arrays use the existing heap namespace, entry index and generation. Allocating one does not change the admitted graph or bound universe and does not invalidate an existing context. A heap replacement, admission change or universe change retains the previous invalidation behavior. Existing primitive arrays allocated before binding retain their identities and behavior.

## Public API

Export the following types, with Debug, Clone, Eq and PartialEq; ArrayJavaError is also Copy:

```rust
pub enum ArrayJavaError {
    NullPointer,
    IndexOutOfBounds,
    NegativeArraySize,
    ArrayStore,
    IllegalAccess,
}
pub enum ArrayAccessError {
    Java(ArrayJavaError),
    Type(Box<TypeAccessError>),
    Heap(HeapError),
}
```

Provide Display and Error::source for wrapped errors. Context failures can be wrapped as Type(TypeAccessError::Context(...)); no reverse recursive error variant is needed. Add ExecutionErrorKind::ArrayAccess(Box<ArrayAccessError>) and the symbolic Java outcome ArrayStoreException. Only explicit ArrayAccessError::Java maps to Java outcomes; never recursively infer a Java outcome from Type or Heap wrappers.

Add these methods to FieldResolutionContext, retaining its existing lifetime/generic parameters:

```rust
pub fn allocate_reference_array(
    &mut self, component: ReferenceType, length: i32,
) -> Result<Reference, ArrayAccessError>;
pub fn new_reference_array(
    &mut self, origin: ClassId, index: u16, length: i32,
) -> Result<Reference, ArrayAccessError>;
pub fn read_reference_array(
    &mut self, array: Reference, index: i32,
) -> Result<Reference, ArrayAccessError>;
pub fn write_reference_array(
    &mut self, array: Reference, index: i32, value: Reference,
) -> Result<(), ArrayAccessError>;
```

The typed allocation method is for callers with an already resolved component identity. It validates that identity but has no initiating caller or new access check. The CP-based method is the bytecode service and resolves/access-checks the component using the current declaring origin. It shares the typed allocation implementation after resolution, without duplicating name scans or allocating a descriptor tree.

Add read-only heap inspection methods:

```rust
pub fn reference_array_type(&self, array: Reference)
    -> Result<ReferenceType, HeapError>;
pub fn load_reference(&self, array: Reference, index: i32)
    -> Result<Reference, HeapError>;
```

These require an actual reference-array payload. load_reference checks the destination handle/kind and index, then validates any stored non-null handle before returning it; it does not run assignment or descriptor resolution again. reference_array_type returns the immutable retained identity, with local structural validation against corrupt rank/payload combinations; full registry validation belongs to the context. Neither method mutates or supplies a writable location.

Do not add public raw reference-array allocation or any raw reference-array setter, including a null-only setter. Existing ManagedHeap::store remains primitive-only. Implement narrow crate-private allocation and commit helpers; only the checked context may reach them. The commit rechecks destination ownership/generation/kind/index and non-null source ownership/generation before one-slot mutation. Its context caller owns the assignment proof, holds exclusive heap access, and makes no intervening external callback. No public token or mutable Vec/slice may bypass this path.

## Shared quota accounting and atomicity

Do not add HeapOptions fields. Both array payload kinds share max_arrays, max_array_length, max_total_elements and max_payload_bytes, and the corresponding public totals. Export `pub const REFERENCE_ARRAY_ELEMENT_BYTES: usize = core::mem::size_of::<Reference>();`. Each reference array charges checked length * REFERENCE_ARRAY_ELEMENT_BYTES to payload bytes. Store a Vec<Reference> so this accounts for the actual typed element representation rather than pretending each opaque handle is an eight-byte address. It excludes Vec capacity slack, headers, retained type metadata, arena/allocator overhead and instance storage; it is not total process memory. Tests use the exported constant, not an assumed host size.

After identity/rank validation and the negative-length check, retain primitive allocation's quota precedence: array count, per-array length, aggregate elements, payload multiplication and aggregate payload bytes. Arithmetic overflow produces HeapError::ResourceLimit with the existing saturation policy; quotas and host reservation failures remain host errors, not fabricated OutOfMemoryError. Use fallible reservations, initialize the entire payload and reserve the arena before publishing a handle and updating counters. Failure changes neither entries nor public counters; temporary capacity changes are not observable allocations. Zero-length arrays still have distinct identities and consume one array count. Their element and payload charges are zero.

Instance quotas remain independent. Stores and reads change no allocation counters. Allocation/store failures preserve prior elements, instance fields and child enumeration; consumed work and successful resolver bindings may remain.

## Type, length and edge integration

Add HeapEntryKind::ReferenceArray. Add HeapEntryKind::Array as an expected-kind diagnostic for generic length only; actual entry kinds remain PrimitiveArray, ReferenceArray or Instance. ManagedHeap::length accepts either array payload, and rejects an instance with expected Array/actual Instance. This is the one intentional diagnostic generalization of the former primitive-only length method; update its exact-error tests explicitly. Primitive kind/load/store keep their signatures and original behavior on old inputs, and reject ReferenceArray with expected PrimitiveArray before inspecting an index/value. Instance field/receiver operations reject reference arrays as noninstances.

Context reference_type recognizes the retained actual reference-array identity and validates it with the existing full type validator. The shared is_assignable relation remains the single covariance implementation. Casts, instanceof and checked instance fields must consequently accept/reject actual reference arrays using existing identity rules without their own alternate relation. Interface-to-Object assignment must still not widen protected subclass checks. Preserve all old type-operation budget tests; extra validation for the new payload must not charge old primitive-array or instance paths differently.

ManagedHeap::children keeps its existing public return signature and allocation-free borrowed iterator. For reference arrays, emit one HeapRef for every non-null element in ascending index order. Preserve duplicates, self-edges and cycles; omit nulls. Instance ordering remains logical field-slot order, and primitive arrays remain childless. Use an internal iterator enum or chained borrowed iterators if needed; no heap-allocated boxed iterator, recursive traversal, deduplication or root/GC claim. Arena relocation and dropping deep/cyclic graphs remain nonrecursive because elements contain opaque handles.

## Exact operation ordering

The governing instruction rules are: anewarray resolves its component before a negative-length outcome; aaload checks null then bounds; aastore checks null, bounds and incompatible component assignment in that order. Successful allocation supplies null elements and successful loads preserve reference identity. See [JVMS anewarray](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.anewarray), [aaload](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.aaload), [aastore](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.aastore) and [arraylength](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.arraylength). The additional malformed-input and policy ordering below is this unverified kernel's explicit host contract.

For new_reference_array:

1. Use resolve_reference_type(origin, index), preserving its context/origin/CP-shape checks, universe requirement, complete name work charge, shared binding and target accessibility. Missing/unadmitted targets stay host errors. Only a direct TypeAccessError::Java(IllegalAccess) from this component-resolution call is converted to ArrayAccessError::Java(IllegalAccess); all other type errors stay boxed.
2. Validate the resolved public component identity using the typed allocation path and derive the new array type with checked rank increment. Do not skip resolution for zero/negative length or inaccessible components, and do not apply the field-store identity-only access policy here.
3. Negative length gives Java NegativeArraySize; otherwise apply quotas/reservations/publication above.

For allocate_reference_array, check context validity, require the universe and fully validate the supplied component first, then derive the rank and perform step 3. It performs no resolver callback or name-access test and accepts abstract/interface components because no component instance is allocated.

For read_reference_array:

1. Check context validity without requiring new name resolution or a universe lookup.
2. Null array gives Java NullPointer. A non-null invalid/foreign/stale handle or wrong payload kind is a host Heap error.
3. Check the signed index against actual length; failure gives Java IndexOutOfBounds.
4. Return the stored reference unchanged after the heap read's non-null handle check. Do not rerun assignment or type traversal on reads.

For write_reference_array:

1. Check context validity, array null/handle/kind and signed bounds in the same order as the read service. Null/bounds precede any value-handle, universe or assignment work.
2. A null value goes straight to the guarded null commit: no type relation, resolver, symbol insertion or type-work charge.
3. For non-null values, use reference_type(value) first, then reference_type(array) to validate the retained actual destination type and derive its reference component. These operations preserve the explicit universe and handle checks. Use is_assignable(source, component); false gives Java ArrayStore. An exhausted policy or malformed host identity stays a host error.
4. Commit the exact reference only after success. No caller accessibility test, field loading-constraint rule or new nominal name resolution applies to an already retained actual array component. Distinct loaders are handled by actual ClassIds and structural type identity, not descriptor spelling.

Heap-error mapping is operation-local: map NullReference and IndexOutOfBounds only at the destination read/write checks, and NegativeLength only at allocation. A later malformed stored/source handle remains host failure even if a nested error resembles a Java category. Do not reinterpret wrappers transitively. An incompatible array store is a real ArrayStoreException; incompatible instance-field stores retain their existing host diagnostic.

## Budgets and interpreter

Use the existing cumulative context policy. CP allocation uses the existing full UTF-16 name charge once and the same pre-callback symbol/unit checks. The typed allocation path calls the existing full public type validator once; array-type construction is constant-sized checked arithmetic with no additional lookup-step charge. Non-null stores charge source type validation, destination type validation and the unchanged assignment relation exactly in that order. Reads and null stores add no lookup-step charge beyond context validity. No new resolution cache, graph clone, successful-assignment cache or alternative type parser is introduced. Traversal allocations remain fallible, iterative and charged before node/edge work as already contracted.

The interpreter enables only 32/53/bd through Services::Fields. Older numeric and primitive-heap entry points continue rejecting them; c5 remains UnsupportedInstruction in every mode. Existing arraylength uses the newly generic heap length service, including a reference array obtained in runtime-aware mode. Primitive load/store opcodes still require primitive payloads before their existing kind/value rules.

Typed stack extraction precedes runtime service calls: anewarray pops Int; aaload pops Int index then Reference array; aastore pops Reference value, Int index, Reference array. Do not prevalidate handles during typed extraction or move source validation before null/bounds. Supply current-frame owner only for bd's CP resolution. Reads push the exact Reference, allocation pushes the issued Reference, stores push nothing. Use the existing one-instruction fuel charge, stack capacity checks, MethodId frames/cache and transition-site provenance. A completed allocation/store is not rolled back because a later push, instruction or budget fails.

## Ownership and acceptance

The runtime worker owns payload/type/context integration, public service tests and private corruption/reservation guards. A new runtime module such as reference_array.rs is appropriate; keep the independent classfile parser untouched. The interpreter worker owns the three instruction branches, boxed error/Java outcome integration, synthetic execution tests and its README. The coordinator owns genuine Java fixtures, independently generated outputs, integration tests and final evidence. Worktrees and output paths remain separate.

Runtime tests must cover all nominal/interface/core assignments, rank-one object arrays, rank-two arrays for all eight primitive leaves, higher/mixed-rank covariance, actual loader-separated names, invalid public identities, rank 254/255, abstract/interface components, defaults/empty identity, aliases/overwrite/clearing/self-links/multiobject cycles and exact child order. Prove generic length and primitive-only rejection, array values in checked fields and casts, shared primitive/reference quotas at zero/exact/cumulative/MAX boundaries, checked multiplication/addition and deterministic reservation failures. Failed allocation/store must preserve previous contents and counters. Private guards must reject forged handles and corrupt retained type/payload/index states without unchecked indexing or mutation.

Pin null/bounds/value precedence, resolution/access before negative length, current-origin loader/access, inaccessible component versus legal assignment to an already allocated array, exact binding/work boundaries, no-work reads/null stores, failed relation atomicity, context invalidation and deep iterative assignment/drop. Do not silently remove older exact-budget tests.

Interpreter tests must cover each added opcode, wrong computational kinds, malformed CP, old-mode/c5 rejection, exact fuel/stack/frame/code limits, current-origin resolution across call kinds, preserved identity through locals/fields/casts/calls and all symbolic/host outcomes with persistent earlier effects. Genuine numeric wrappers should exercise object/interface arrays, nested primitive-array components, covariant alias stores with ArrayStoreException, null/bounds/negative failures and default elements. Avoid constructor, reference-signature or multianewarray claims; independently inspect genuine Code to establish the intended bytecodes rather than assuming javac emits them.

The gate preserves all 13,085 accepted execution comparisons, separate 151-case array/field service sets and the 80-class corpus. Require frozen review, formatting, strict workspace/all-target Clippy, all debug tests, optimized runtime/interpreter tests, genuine fixture/reference reproduction and expanded parser smoke. Record new hosted Linux/Windows validation after publication separately. This bounded approval will not complete M2 or certify JVM conformance/performance superiority.

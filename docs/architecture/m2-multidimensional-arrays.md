# Bounded multidimensional allocation

Status: bounded implementation approved by the technical director on 2026-11-04 after frozen review and the passing integrated gate recorded in `m2-multidimensional-arrays-review.md`. Implementation used separate runtime/interpreter worktrees based on bd71ae0. Fresh hosted validation remains pending publication. Full M2 remains open. No NanoSpec.

## Scope and public API

Add multianewarray (c5) to the existing runtime-aware body engine, bringing that profile to 190 effective opcodes. Reuse actual structural array types, the explicit universe, admitted identities, resolver/cache, both array payload kinds and shared array quotas. Earlier numeric and primitive-heap entry points keep rejecting c5. Do not widen method signatures or add constructors, initialization, static writes, mirrors, handlers, verification, collection or another execution engine.

Add only these public methods to FieldResolutionContext:

```rust
pub fn allocate_multidimensional_array(
    &mut self,
    array_type: ReferenceType,
    counts: &[i32],
) -> Result<Reference, ArrayAccessError>;

pub fn new_multidimensional_array(
    &mut self,
    origin: ClassId,
    index: u16,
    counts: &[i32],
) -> Result<Reference, ArrayAccessError>;
```

The type is the complete resulting array type, unlike allocate_reference_array's component argument. counts is ordered outermost to innermost. The typed method validates the supplied identity without caller accessibility; the CP method resolves/access-checks the target using the current declaring origin, then shares the typed implementation. No public raw heap allocator, transaction, future-handle issuer, mutable payload or plan constructor is exposed. Keep HeapOptions and existing primitive/reference allocation APIs unchanged.

Reuse ArrayAccessError and ArrayJavaError. A direct IllegalAccess from CP target resolution becomes explicit ArrayJavaError::IllegalAccess; a detected negative count becomes explicit NegativeArraySize. Other type/context/heap failures stay their existing wrapped host categories. No recursive interpretation of nested Java-like errors, new Java verifier outcome or fabricated OutOfMemoryError is introduced.

## Identity, rank and failure order

The normative requirements are that the target resolves to an array type with sufficient rank, all requested dimensions are nonnegative, and only the requested prefix is allocated. A zero prevents deeper allocation but does not exempt remaining counts from the negative test. Leaf defaults and partially allocated reference components retain their usual defaults. See [JVMS multianewarray](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.multianewarray). The precise malformed-input and policy ordering below is this unverified kernel's host contract.

For new_multidimensional_array:

1. Call resolve_reference_type(origin, index), preserving context/origin/CP shape, universe requirement, complete UTF-16 name work, shared binding and caller access. Do not skip resolution for empty, zero or negative counts. Missing/unadmitted targets remain host errors.
2. Enter the typed path below. This deliberately retains the same full type-validation charge as a separate typed allocation call after resolution. Do not rescan or copy the target spelling.

For allocate_multidimensional_array:

1. Check context validity, require the explicit universe and fully validate the public type using the existing type validator.
2. Require ReferenceType::Array and 1 <= counts.len() <= array rank. Rank is already in 1..=255; reject empty, oversized, non-array or excessive-dimension input as TypeAccessError::InvalidType before inspecting individual counts. In particular, a huge host slice is rejected by its length without scanning it. Rank 255 with 255 requested dimensions is supported; no rank increment occurs.
3. Scan counts in outer-to-inner order. Charge one LookupSteps unit before inspecting each count. The first negative produces explicit Java NegativeArraySize with no heap mutation. Do not stop the scan at a zero. If all counts are nonnegative, continue to complete geometry preflight.
4. Preflight all reachable allocation levels and shared quotas, precharge staging work, stage/reserve everything, then publish atomically as specified below.

Thus resolution/access/type/dimension-shape failures precede negative-size outcomes; a policy failure can stop a dimension scan before an otherwise later negative. No array is allocated during validation. Successful bindings and consumed work may remain after any failure.

At depth d (outer depth zero), the actual node type is the requested type with rank reduced by d and the same primitive/nominal leaf and defining loader. A node is a primitive payload only when its actual rank is one and its leaf is primitive. Every other node is a reference payload retaining its full actual type. Interface/abstract nominal leaves are legal because no component instance is constructed. Array nominal identity/access continues to use the leaf's defining loader; primitive-leaf arrays use the designated bootstrap loader. No ClassId or core identity is inferred from spelling alone.

If fewer dimensions than rank are requested, the last allocated reference arrays contain nulls. If allocation reaches a primitive leaf, the existing typed zero representation supplies its defaults, including positive floating zero. If a reached count is zero, allocate all zero-length nodes at that level and no deeper nodes. Examples: counts [2,0,7] allocate one length-two outer array and two empty children; counts [0,7,9] allocate only one empty outer array; [0,-1] still fails before allocation. counts [2,3] for int[][][] allocate an outer rank-three array and two rank-two arrays with three null elements each, without primitive leaves.

## Complete geometry and quota policy

Do not repeatedly call the public single-array allocators and publish a partial graph. Preflight the entire reachable geometry first. Keep level metadata bounded by 255 entries; fixed arrays or fallibly reserved bounded vectors are suitable. No per-node queue is necessary.

Let N_d be the number of nodes at depth d, initially N_0 = 1. Let L_d = counts[d]. The number of element locations at that level is E_d = N_d * L_d. For a next requested level, N_(d+1) = E_d. Stop after a reached zero-length level; do not evaluate deeper positive sizes for allocation quotas. Every count has already passed the negative scan.

Process reachable levels in increasing depth. Charge one LookupSteps unit before each level's arithmetic. At each level, apply these checks in this exact order, against the original heap totals plus all preceding levels:

1. Checked cumulative new array count plus N_d against max_arrays, including existing allocated_arrays.
2. L_d against max_array_length. This check applies only to reached levels.
3. Checked N_d * L_d and cumulative new element total against max_total_elements, including existing total_elements. A multiplication overflow is a TotalElements resource error, even if the product would also be the next level's array count.
4. Checked E_d * element width and cumulative new payload bytes against max_payload_bytes, including existing payload_bytes. Width is the primitive kind's element_bytes only for primitive node payloads, otherwise REFERENCE_ARRAY_ELEMENT_BYTES.

Use existing ResourceLimit categories and required = usize::MAX on arithmetic overflow. The ordering is depth-first in the list of levels, not a promise that every global count error precedes every length error. For example, an outer length failure precedes an inner array-count failure. Pin this policy with conflicting-limit tests. Per-instance quotas are unaffected. Zero-length nodes consume array count and entry metadata, but zero elements/payload bytes. Counts deeper than a reached zero do not trigger positive-size limits or product overflow.

Record checked per-level node counts, element counts, starting staged indices, node types and lengths while preflighting; calculate total staged nodes N and internal parent-child links K. This is one rooted tree, so K = N - 1, including links to empty children. Preserve this as a checked invariant, not unchecked subtraction. Validate the future arena index end with checked entries.len() + N before reservation; overflow is HeapError::AllocationFailure for the arena rather than a forged/wrapped index.

## Work budget

Keep earlier instruction/type-operation accounting unchanged. This new operation charges:

- CP target resolution only for the CP method, using its existing name/binding work;
- one existing full public type validation in the typed path;
- one step per inspected count, including zeros and the failing negative count;
- one step per reachable preflight level, before arithmetic;
- N + K steps reserved before staging, one for every new node and internal link.

Add a narrow crate-private checked bulk work charge if useful. Its policy is atomic: checked current steps + amount must fit max_lookup_steps; otherwise return FieldAccessError::ResourceLimit(LookupSteps) with that complete required value (usize::MAX on overflow), leaving the counter at its pre-charge value. On success add the whole amount once. This policy is specific to the bulk reservation; do not alter existing single-step error counts. The N + K sum must itself be checked and fail as LookupSteps overflow before allocating staging storage. Wrap this failure through TypeAccessError::Context in ArrayAccessError::Type.

The reserved node/link units cover the bounded local payload/identity checks and link creation; do not call resolver or general is_assignable per link, or charge again while iterating the staged graph. Its rank-derived construction proves each child type is the parent's component. Actual memory initialization is bounded separately by the successful element/payload quotas. There is no successful-allocation cache or retained geometry cache.

## Staging, memory and publication

Stage a Vec<Entry> or equivalent owned temporary containing exactly N future entries in breadth-first level order. Each payload is allocated exactly once and later moved into the arena. Reserve staging metadata fallibly only after complete quota and work preflight. All primitive/reference payload constructors and arena reservations remain fallible; do not use unchecked reserve/collect driven by input sizes. Store at most O(255) level metadata and O(N) entry descriptors in addition to the eventual payload contents. Do not allocate a second reference payload, descriptor tree, per-link queue or hash map. Arena capacity may coexist with staged descriptors; this shallow O(N) metadata duplication is explicit and is bounded by max_arrays, not counted as payload bytes. Capacity slack and allocator overhead remain outside the public payload metric.

Null/zero initialize each staged payload, then connect parent elements to the correct staged children. For parent ordinal p at level d, the first child ordinal is p * L_d in level d+1. Use checked multiplication/addition for its level offset, future arena index and child range. Links refer to the same heap identity, LIVE_GENERATION and future index base + staged_index. These uncommitted handles must remain entirely inside private staging; they cannot pass the public heap's checked_handle until publication and must never escape through callbacks, errors or return values.

Encapsulate staging invariants behind private constructors/helpers. Before commit, validate current arena base/heap identity and original counters, expected staged length, entry generations, per-level type/length and link ranges/identities against the checked geometry. Every internal reference slot must identify its exact expected child; reference slots without an allocated child must remain null. Primitive payload defaults come from the existing checked zero constructors. A private corrupted-plan/staging test must not be able to publish an out-of-range, cross-heap or wrong-rank link. This local validation can be iterative and uses the already reserved node/link work; it is not another nominal relation traversal. Source data and the context's exclusive mutable heap borrow prevent external graph changes between preflight and commit.

Reserve arena capacity for all N entries and perform every fallible check before appending the first entry. After that point, append/move the staged entries without further fallible allocation, install the precomputed array/element/payload totals, and return the outer handle. No references to old heap objects are changed by allocation. Existing generations/handles, instance slots, registry bindings and heap identity stay intact. The commit helper must not expose a public transaction or ownership bypass.

On any validation, work/quota, arithmetic, reservation or staging failure, drop only the temporary staged payloads and leave live arena length, counters, old payloads and child enumeration unchanged. Dropping staged/live deep graphs is iterative because payloads contain handles, not recursive owned children. A successful arena capacity reserve may remain after a later failure; this is not an observable allocation under the public counters. A host allocation failure remains a host error; this atomic policy does not claim Java OutOfMemoryError/GC semantics. Failure injection for tests may be private and deterministic, never a production public option or a real host-exhaustion experiment.

## Interpreter behavior

Keep decoder semantics: c5 encodes a nonzero u8 dimension count and a constant index; unreachable malformed encodings remain eagerly rejected. The execution layer requires the existing Operands::MultiANewArray form. Earlier entry points return UnsupportedInstruction(c5) before extracting operands; runtime-aware execution uses the shared frame loop.

Use bounded temporary count storage, such as [i32; 255]. Pop dimensions Int values from the stack in reverse index order, so the slice passed to the service is count1 through countD. Typed underflow/wrong-kind errors precede CP resolution. Do not validate individual signs in the interpreter or return early for zero; the service owns target/access/sign ordering. Pass the current frame's declaring ClassId and CP index to new_multidimensional_array, then push the returned outer Reference unchanged.

Reuse the explicit-only array failure mapper and existing method/offset/transition-site diagnostics. c5 consumes one interpreter fuel unit; its bounded internal work is covered by the context and heap policies above. No extra frame/cache is created. If allocation commits and a later push/instruction/fuel check fails, the completed allocation remains, just as earlier heap instructions preserve their effects. Ordinary arraylength, loads/stores, children, fields, casts and receiver uses operate on the resulting actual node payloads without special cases or synthetic component initialization.

## Ownership and gate

The runtime worker owns the new context methods, private geometry/staging/commit machinery, runtime synthetic/private tests and README. Keep field/type changes narrow; reuse their validators and add only the private bulk charge needed by this contract. The interpreter worker owns c5 integration, synthetic tests, deliberate replacement of old runtime-aware c5 rejection assertions and README. Root owns genuine Java/JDK tooling, state assertions, integration and final evidence. Separate worktrees/output directories are required; no new crate is necessary.

Required runtime evidence includes complete and partial rank for all eight primitive leaves and nominal/interface leaves; one requested dimension; rank 255; zero at each level; negative after zero; positive enormous unreachable dimensions after zero; exact unique identities and child order; retained actual types/defaults; mixed prior primitive/reference allocations; loader/access and typed inaccessible-leaf cases; malformed count lengths/types; all quota precedence/overflow boundaries; exact single/bulk work limits and bulk failure counters; unchanged prior state after every failure. Exercise deterministic failures during payload staging and before arena publication, and corrupted private geometry/link/base/generation/type guards. Deep 255-level allocation/drop must be nonrecursive, and wide trees must stay within the admitted metadata/payload bounds.

Interpreter evidence must pin dimension extraction order with asymmetric sizes, wrong stack kinds/underflow, malformed CP/target rank, target access before negatives, all-negative scanning after zero, partial allocation, old-mode rejection and current-origin behavior across call forms. Test shared fuel/stack/frame/code/work quotas, exact symbolic versus wrapped host mapping and retained earlier effects. Genuine wrappers should emit c5 (verified by decoder), read/write nested primitive/reference leaves through accepted instructions, and compare JDK numeric outcomes/failures plus explicitly source-derived reachable graph/counter assertions.

Preserve all 17,033 accepted execution comparisons, separate 151-case array/field service sets and the 86-class corpus. Acceptance requires frozen technical review, formatting, strict workspace/all-target Clippy, all debug tests, optimized runtime/interpreter tests, genuine fixture/reference reproduction and expanded parser smoke. Fresh hosted Linux/Windows evidence is recorded after publication separately. Full M2, complete JVM conformance and performance superiority remain open.

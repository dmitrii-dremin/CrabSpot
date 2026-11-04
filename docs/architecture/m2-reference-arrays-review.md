# Reference-array review

Status: approved by the technical director on 2026-11-04 for the bounded reference-array storage/execution increment. Frozen technical review and the coordinator's integrated gate satisfy `m2-reference-arrays.md`. Full M2 remains open.

## Reviewed behavior

Reference arrays retain a complete immutable structural type and null-initialized Vec<Reference>. Typed allocation validates the component identity; bytecode allocation first resolves it with the current declaring origin and caller accessibility. Rank 254 components can produce rank 255; rank 255 components fail before length checks. Abstract/interface components are permitted without allocating component instances. Array creation does not introduce mirrors, constructor execution or initialization.

Allocation shares the existing primitive-array count, length, aggregate-element and payload quotas. Reference payload bytes use the actual size_of::<Reference>() representation through the exported constant. This accounts for shallow typed elements, not total host memory. Checked arithmetic, fallible payload/arena reservation and complete initialization precede publication and counter updates. Primitive and instance storage retain independent kinds and their existing semantics; generic length accepts both array payloads and deliberately generalizes the non-array diagnostic to expected Array.

Destination null, handle/kind and bounds checks precede source type work. Null stores and reads add no lookup work. Non-null stores validate the source and retained actual array type before applying the shared assignment relation and committing one exact reference. Incompatible components produce explicit ArrayStoreException; nested type/heap failures stay host diagnostics. The read path validates any stored non-null handle without turning corrupt child failures into destination Java outcomes. Raw inspection is read-only, and there is no public allocation/setter bypass.

Child enumeration chains borrowed slices without allocating, retaining duplicate aliases, self-links and cycles in index order. Actual arrays participate in casts and checked instance fields through the same type relation. No protected-access predicate, resolver identity rule or old type-operation budget was widened.

The interpreter adds bd/32/53 to the existing runtime-aware loop, with typed stack extraction before service calls and current-frame origin for component resolution. Only explicit array-service Java variants become symbolic Java outcomes. Earlier modes reject the new instructions, c5 remains unsupported, and runtime-aware coverage becomes 189 effective opcodes. Completed allocations and writes survive later instruction, Java, host or policy failures.

No blocking semantic defect or contract relaxation was required during review. Requested private regressions pin corrupt stored-handle reads and nested error mapping. The payload multiplication calculation was extracted into a shared helper to test deterministic overflow; its accounting semantics are unchanged. Existing exact-error assertions for generic length were deliberately updated to the contract's Array diagnostic.

## Coverage

Eighteen new public runtime tests cover nominal/interface/abstract components, all eight primitive-leaf outer arrays, rank boundaries, unequal-rank covariance, exact loader identities, inaccessible typed components versus CP accessibility, null/default/empty arrays, aliasing/clearing/cycles, primitive-only rejection, shared quotas and failure atomicity. Inclusive cumulative work boundaries are 6 for typed allocation, 13 after an object store and 21 after a self-array store in the pinned fixture; preceding limits fail without mutation. Binding cache hits still charge complete names. A 10,000-class assignment graph and 10,000 cyclic array links exercise iterative traversal and drop.

Four new private runtime tests cover forged destination/source handles, malformed retained payload types, stored foreign/stale/missing handles, multiplication/counter overflow and deterministic reservation failure. Existing contents and public accounting remain intact. Checked field and cast integration uses actual retained array types rather than a widened declared alias.

The interpreter's twenty public cases cover the three instructions, all primitive nested components, actual-component covariance, rank 255, malformed stack/CP states, resolution/access before negative length, generic length and primitive rejection, exact fuel/frame/slot/code and quota limits, no-work reads/null stores, late relation exhaustion, current declaring origins across call forms, provenance and failure persistence. Loaded element identity survives wide local 300, casts, retained fields and virtual/interface/ordinary-special calls. A private mapping test distinguishes all five explicit Java categories from wrapped heap null/bounds/negative and nested TypeJava errors.

The coordinator's original ReferenceArray fixtures supply 3,948 independent JDK outcomes across 42 receiver/method pairs. The exceptional outcomes comprise 313 ArrayStoreException, 1,406 NegativeArraySizeException, 1,810 bounds and twenty NullPointerException outcomes. Supplemental graph assertions are explicitly source-derived: they inspect every retained array type, length, element, immediate child sequence, alias/cycle, and primitive/reference counter and payload total. Selected genuine Code is decoded to require bd/32/53 and exclude c5. Host receivers use default storage; CrabSpot does not run fixture constructors.

## Frozen source

The director verified all seventeen integrated files against `target/coordinator/reference-arrays-freeze.json`, in addition to checking the individual worker manifests. Principal SHA-256 values below describe native working-copy bytes before Git line-ending normalization:

| File | SHA-256 |
| --- | --- |
| runtime/src/reference_array.rs | `A753BCE38419F573A125BE43B8A7E702D2A04DA1D505A6D22AAE999108679EA0` |
| runtime/src/heap.rs | `DCADAF6369F610766EA6A8FDEE90A4137386D36A6453C140DF4ECE1FE0891683` |
| interpreter/src/execute.rs | `313BA45132A14604A7A6A68F1176E81B5EDC73CE9E29CD7C7948856C8FB84C34` |
| interpreter/tests/reference_arrays.rs | `35E206742BCC9EA02C1CAC2539808047D9F9B49F152AE4BDAF462F060DB3BF49` |

## Integrated evidence

The coordinator independently observed on the frozen combined source:

- Formatting and strict workspace/all-target Clippy pass.
- All 595 workspace tests and 508 optimized runtime/interpreter tests pass with zero failures. The optimized split is 275 runtime and 233 interpreter tests.
- All 17,033 execution comparisons match in both profiles: 13,085 earlier cases plus 3,948 reference-array cases. The separate 151-case array and 151-case field service oracles also pass.
- All 86 genuine class fixtures and thirteen reference-output sets reproduce; the previous 80 class hashes remain unchanged.
- The 86-class parser smoke campaign completes 10,000 cases with zero failures: 136 accepted, 9,864 rejected, ten deterministic rechecks, final corpus 222, seed 4851011501752864596.

The coordinator reverified all seventeen frozen hashes after these checks. Fresh hosted Linux/Windows validation for this increment remains pending publication; the preceding checked-field increment's passing CI is separate evidence.

## Boundary and next direction

This increment provides reference-array storage and single-level allocation/load/store execution. Reference signatures, multidimensional allocation, ordinary construction/initialization, static writes, handlers, verification, mirrors and collection remain outside its approval. Full M2, full JVM conformance and performance superiority remain unapproved.

`m2-multidimensional-arrays-next.md` proposes bounded c5 allocation next. It requires a separate exact contract for dimension validation, checked aggregate geometry, iterative staging, quotas and atomic host-failure publication before implementation.

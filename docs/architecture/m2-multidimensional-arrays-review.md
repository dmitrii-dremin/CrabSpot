# Multidimensional allocation review

Status: approved by the technical director on 2026-11-04 for the bounded multidimensional-array allocation/execution increment. Frozen technical review and the coordinator's integrated gate satisfy `m2-multidimensional-arrays.md`. Full M2 remains open.

## Reviewed invariants

The CP service resolves/access-checks the complete target array type with the current declaring origin, then shares the typed allocator. Type and dimension shape validation precede an outer-to-inner charged sign scan. Every requested count is inspected until failure, including counts after zero. Reachable geometry stops after a reached empty level, so deeper positive sizes neither allocate nor consume heap quotas.

Preflight uses bounded level metadata and checked products/sums. Each reachable level checks cumulative arrays, per-node length, aggregate elements and payload bytes in the contract's order. Actual ranks decrease per level; only rank-one primitive leaves use primitive payload widths. Partial allocation leaves null components. Existing primitive/reference allocations share the same quotas, while instance quotas remain separate.

The new private bulk work charge reserves all node/link units atomically before staging. Failure reports the complete required value and preserves the precharge counter; prior single-step behavior remains unchanged. Payloads are created once in a breadth-first temporary, with checked future arena indices. The private publication boundary rebuilds the geometry, compares the heap snapshot, and validates every staged generation/type/length and exact child or null slot before reserving the full arena and moving entries. No fallible operation remains after publication begins. Failures drop only temporary payloads and retain old live state and child edges.

The interpreter adds one c5 branch to the existing runtime-aware loop. A fixed 255-count buffer receives reverse Int pops and supplies an outer-to-inner slice to the current-origin service. Typed extraction precedes resolution; signs are not inspected in the interpreter. Earlier modes remain gated. The existing explicit-only array Java mapper and MethodId/frame/cache/provenance logic are reused. Runtime-aware coverage is 190 effective opcodes.

No blocking semantic defect or contract change was required during review. Requested private tests isolate staging failure after earlier temporary payloads and failure after arena reservation, plus corrupted exact-link/null-slot publication. Checked multiplication was factored into a helper for deterministic overflow testing without changing quota ordering.

## Coverage

Fourteen public runtime cases cover complete and partial allocation for all eight primitive leaves and nominal/interface/abstract leaves, asymmetric shape, rank 255, zeros at each depth, negative-after-zero and enormous positive unreachable counts. They pin malformed identities/count shapes, CP access/loader behavior, shared quota precedence, mixed prior graphs and unchanged old state. The exact [2,3] work boundary is 15: limit 14 reports required 15 while the counter remains at 10 before the bulk charge. The rank-255 fixture pins total work at 1,025.

Five private tests cover bulk sum/counter overflow, future handles rejected before publication, twenty-five corrupt snapshot/geometry/generation/type/length/link cases, non-null unallocated slots, negative retained counts after zero, staged payload failures and post-reservation failure. Old nonempty child edges and primitive contents survive. Arithmetic/future-index guards remain structured; no host exhaustion experiment is used.

Sixteen interpreter cases cover all primitive leaves, asymmetric extraction, full/partial/single/rank-255 allocation, zero/negative ordering, wrong stack kinds and eager malformed decoding, target/access/universe failures, old-mode gates, exact work/quota/fuel/frame/slot/code limits, current declaring origins across call forms, provenance and retained completed graphs after failures. The prior explicit-only array-error mapping regression remains applicable and passing.

The coordinator's original MultiArray fixtures provide 7,388 independent JDK outcomes across twenty receiver/method pairs, including 3,364 NegativeArraySizeException, 1,666 bounds and fifteen ArrayStoreException outcomes. Supplemental state checks are explicitly source-derived and use a bounded depth-first fixture model distinct from production breadth-first staging. They inspect all reachable identities, actual ranks/types, lengths, primitive values/reference slots, child order and totals. Decoded genuine Code establishes rank/requested-dimension pairs (2,2), (3,2), (3,3), (4,3) and (4,4). Host receivers use default storage without constructor execution.

## Frozen source and integrated evidence

The director verified all ten integrated files against `target/coordinator/multidimensional-freeze.json`. Principal SHA-256 values describe native working-copy bytes before Git line-ending normalization:

| File | SHA-256 |
| --- | --- |
| runtime/src/multidimensional.rs | `20696DAC66B3321A33A8AA9E918D4360D184E447C328DD52C81A0D39573D01EC` |
| runtime/src/field.rs | `464922564317CA9CCD2C925A38BA315100050478012DE73623EC3612C09274B7` |
| interpreter/src/execute.rs | `06A5AFADBD5825DB949648D42D2ED4B47474EAFF15127EA70B68ABEB8C61B2FD` |

The coordinator independently observed on the frozen combined source:

- Formatting and strict workspace/all-target Clippy pass.
- All 631 workspace tests and 544 optimized runtime/interpreter tests pass with zero failures. The optimized split is 294 runtime and 250 interpreter tests.
- All 24,421 execution comparisons match in both profiles: 17,033 earlier cases plus 7,388 multidimensional cases. The separate 151-case array and 151-case field service oracles also pass.
- All 88 genuine class fixtures and fourteen reference-output sets reproduce; the previous 86 class hashes remain unchanged.
- The 88-class parser smoke campaign completes 10,000 cases with zero failures: 130 accepted, 9,870 rejected, ten deterministic rechecks, final corpus 218, seed 4851011501752864596.

The coordinator reverified all ten frozen hashes after these checks. Fresh hosted Linux/Windows validation for this increment remains pending publication; the preceding reference-array increment's passing CI is separate evidence.

## Boundary and next direction

This is bounded multidimensional array allocation and execution with atomic host-failure publication. It does not implement OutOfMemoryError/GC semantics, verification, mirrors, reference method signatures, ordinary construction/initialization, static writes, handlers or JVM startup. Full M2, full JVM conformance and performance superiority remain unapproved.

`m2-reference-calls-next.md` proposes reference arguments/results next, with a separate contract for descriptor identities, loading boundaries, argument/return validation, budgets and host-root behavior before implementation.

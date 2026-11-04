# Reference-type and cast review

Status: approved by the technical director on 2026-11-04 for the bounded reference-type, checkcast and instanceof increment. Frozen source review and the coordinator's integrated gate satisfy `m2-reference-types.md`. Full M2 remains open.

## Disposition and reviewed invariants

The heap binds an explicit Object/Cloneable/Serializable universe by registry and loader identity after complete validation. Binding and incompatible later layout admission are atomic. Pre-existing primitive arrays preserve identity, contents and accounting. Contexts detect a changed universe, heap replacement or additional admission. Old registry-free array operations remain available.

Structural array types preserve defining-loader identity, validate both public operands before shortcuts, and reduce component ranks iteratively. Nominal relation traversal uses admitted identities and charged edges; interface assignment to Object does not widen the class-only protected-access predicate. Resolution borrows exact UTF-16 target/array-leaf names through the shared binding cache, requires explicit extra target roots and retains unnamed-module access boundaries.

The interpreter adds c0/c1 to the existing runtime-aware loop with current-frame origin and typed extraction. The runtime checks CP shape even for null, then skips universe, name scans, target resolution and callbacks for null. Non-null handles are validated before target resolution/access. Cast success preserves the same reference; only explicit type-service Java categories become ClassCastException or IllegalAccess outcomes. Wrapped failures remain typed host errors with provenance. Earlier stores and allocations remain visible after failure.

No blocking semantic defect was found in the reviewed implementation. Review requested additional regression evidence for wide/duplicate interface fanout and same-size heap replacement before freeze. The final tests pin exhaustion before the first interface enqueue at limit 45/required 46 and before a duplicate Shared edge at limit 61/required 62. They also distinguish interface-to-Object assignment from protected subclass access. These checks strengthen the evidence without weakening or changing the contract.

## Evidence

The 24 new public runtime tests and three private tests cover explicit core identities, invalid/foreign roles, binding/admission atomicity, arrays allocated before binding, invalidation, extra target roots, exact UTF-16 aliases and loader separation, access, nominal diamonds, all primitive kinds, covariance and rank 255, malformed public types/handles, null/error order, exact budgets and deep iterative traversal. Private reservation failures preserve existing containers.

The 15 new public interpreter tests and private Java-mapping test cover both instructions, unchanged identity and later field/array/call use, current declaring origins across call forms, null without a universe, malformed CP/stack/encoding, old entry-point rejection, shared fuel/work limits, error provenance and retained effects. A 65,000-unit null target proves no name-scan work, symbol insertion or callback occurs.

The coordinator's original Cast fixtures supply 1,276 independent JDK outcomes across 36 receiver/method pairs, including 463 ClassCastException and 242 NegativeArraySizeException outcomes. Supplemental field/allocation assertions are explicitly source-derived. The Rust integration deliberately leaves the null-only target undefined and does not bind a universe for those methods; synthetic tests independently isolate that ordering. Host allocation supplies default instance storage without claiming constructor execution.

The coordinator independently observed on the frozen combined source:

- Formatting and strict workspace/all-target Clippy pass.
- 519 workspace tests and 432 optimized runtime/interpreter tests pass with zero failures; the optimized split is 238 runtime and 194 interpreter tests.
- All 12,395 execution comparisons match in debug and release: 11,119 earlier cases plus 1,276 type cases. The separate 151-case array and 151-case field service oracles also pass.
- All 74 genuine class fixtures and eleven reference-output sets reproduce; the previous 65 class hashes are unchanged.
- The 74-class parser smoke campaign completes 10,000 cases with zero failures: 115 accepted, 9,885 rejected, ten deterministic rechecks, final corpus 189, seed 4851011501752864596.

The director verified all 15 integrated files against `target/coordinator/types-freeze.json` after the reported final checks, in addition to reading the worker manifests. Principal SHA-256 fingerprints, for native working-copy bytes before Git line-ending normalization:

| File | SHA-256 |
| --- | --- |
| runtime/src/types.rs | `50283A6C01C62406AA677DF99DBDFD774C4739B30834EE93A2D346C146495394` |
| interpreter/src/execute.rs | `B4FB07B164DDF2B8329044825F343E81C4243F7F9110599C3726038BFAA04C12` |
| interpreter/src/error.rs | `78734555CE6A5F8413F45A755337B4DA8D1E09AFEF029E4B53CDA766FF09F3AE` |

The coordinator subsequently inspected complete [run 41798076784](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41798076784) for implementation commit f05fa69. Linux and Windows passed every check with Rust 1.99.1 and Temurin JDK 21.0.13: checked-in/regenerated fixtures, all eleven reference sets, strict lint, debug/optimized tests and parser smoke. Both smoke runs completed 10,000 cases with 115 accepted, 9,885 rejected, ten deterministic rechecks, corpus 189 and zero failures. This hosted evidence is recorded separately after frozen-source approval.

## Boundaries and next direction

This approval covers typed checks over explicitly admitted metadata, ordinary instances and primitive arrays. Structural reference-array target types do not imply reference-array allocation, array Class objects or reflection. Non-null reference stores, reference call signatures, constructors, lifecycle initialization, full verification/loading constraints, native binding, monitors, exception-object/handler dispatch, collection and JVM startup remain outside the increment. No full-M2, conformance or performance claim follows.

The proposed next increment is [checked non-null instance fields](m2-reference-fields-next.md). It requires a separate concrete contract and acceptance gate; no next production work is authorized by this review.

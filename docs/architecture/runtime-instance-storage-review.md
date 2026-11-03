# Ordinary-instance storage review

Date: 2026-11-03.

**The ordinary-instance storage and field-service increment is approved. Full M2 and subsequent milestones remain open. A field execution bridge requires its next concrete contract before implementation.**

## Review

The director reviewed the complete instance admission/allocation/field/children implementation, shared heap integration, both identity-allocator changes, private guard regressions, all 22 public instance tests, genuine metadata integration, the raw-store JDK oracle and runtime documentation. No blocking finding remains within `runtime-instance-storage.md`.

Admission binds a single registry only after a successful nonempty transaction. New metadata is staged, checked and reserved before publication. Existing and batch shapes are compared by ClassId and resolved relationships, including interface order; equal names or equal slot counts cannot hide conflicting delegated parents. Shared ancestors retain only their own fields and parent links. Invalid metadata, missing dependencies and cycles fail without publishing partial shapes or changing live-object interpretation.

Allocation, field queries and destruction use no recursive ownership or traversal. Each instance has a real typed value vector, including inherited locations; Long/Double each occupy one logical slot. Array and instance counts/quotas stay independent despite sharing one reference arena. Every reference access checks heap ownership, entry bounds/generation and the required entry kind. All new metadata/payload/arena reservations are fallible and proportional to actual admitted resources.

Defaults, ignored ConstantValue metadata, owner-qualified hiding, final-field rejection, numeric kinds, subword normalization and null-only reference stores match the contract. Failed writes preserve prior values. Child iteration borrows non-null handle values; it does not implement roots, assignability, retention or collection. The original heap constructor leaves instance services disabled.

Private tests cover malformed field lists/identities/counts, missing roots/dependencies, a metadata-consistent cycle, forged handles, damaged shapes/payloads, arithmetic overflow, deterministic reservation failure and child filtering. The 22 public tests include batch/sequential deduplication, equal-slot delegated-parent conflicts, interface conflicts, failed-admission recovery, all six policies, mixed array/object arenas and a 10,000-class admission/allocation/query/drop case. No production visibility was widened for tests.

## Raw field-store evidence

The original Java oracle emits a class in memory whose methods execute real allocation/construction followed by a computational load, putfield, getfield and return. There are no i2b/i2c/i2s instructions before the store, and Z/B/C/S methods return I, so return narrowing cannot mask the field result. The oracle runs only on the development JDK. Rust tests apply its results through the memory service using equivalent genuine javac field descriptors; they do not claim CrabSpot field-bytecode or constructor execution.

The 151 JDK 21 outcomes confirm Boolean masking, signed Byte/Short truncation, unsigned Char truncation and numeric round trips. NaNs are classified in differential output; Rust synthetic tests separately require exact stored payload bits. Genuine metadata tests also check 18 inherited/own logical locations, three hidden fields with distinct owners, ignored instance ConstantValue and final/static rejection.

## Toolchain correction

A hosted run for commit `2617637` passed checked-in tests and Java 21.0.13 fixture/reference regeneration on Linux and Windows, then failed strict Clippy on Rust 1.99.1 because AtomicU64::fetch_update was deprecated. The registry and heap identity allocators now use checked Relaxed compare_exchange_weak retry loops. Review and exhaustion/contention regressions establish that zero/MAX remain reserved, successful calls claim unique old values and IDs never wrap or reuse. No lint suppression or MSRV increase was introduced.

That earlier hosted run was not a fully passing CI run. Fresh hosted validation of this correction is pending publication. Rust 1.85 execution remains unobserved; using the older CAS primitive is not a claim that the entire workspace's declared MSRV has been tested.

## Final independent evidence

The coordinator integrated the frozen isolated worker files and independently checked the final source with native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 289 passed, zero failed |
| Release runtime/interpreter tests | 202 passed: 110 runtime, 92 interpreter |
| Existing interpreter/JDK outcomes | All 8,285 match in both profiles |
| Array-service/JDK round trips | All 151 match in both profiles |
| Instance-field-service/JDK round trips | All 151 match in both profiles |
| Fixture reproduction | 34 byte-identical classes; preceding 30 hashes unchanged |
| Reference reproduction | All six output sets reproduced |
| Expanded-corpus parser smoke campaign | 10,000 cases, zero failures |
| Existing array method-body demo | Positive and negative-size smoke cases passed |

The director reviewed source and reported coordinator evidence without claiming an additional independent execution. This increment demonstrates memory-service semantics, not complete conformance or a performance advantage.

## Next boundary

`m2-instance-field-bridge-next.md` records the next design work: runtime-identity-aware method-body evaluation with an explicit receiver and field services. Symbolic field resolution, access/finality checks, failure ordering and receiver compatibility need a concrete reviewed contract. Constructor execution, class initialization, verification, reference assignability/stores, general invocation, exception dispatch, monitors and GC remain separate responsibilities.

## Post-publication CI evidence

The coordinator inspected [run 41718502938](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41718502938)
for implementation commit `a27593f14cb1aa2d1c00c95b0ecaf0db840ab409` after
publication. Both Linux and Windows jobs completed successfully, including all
fixture/reference regeneration checks, formatting, strict Clippy, debug and
optimized tests and parser fuzz smoke. This supersedes the pending hosted status
above for this implementation commit; the earlier failed run remains historical
evidence of the corrected atomic API compatibility issue.

# Instance method-body and field execution review

Date: 2026-11-03.

**The bounded instance-body and field-execution increment is approved against `m2-instance-field-bridge.md`. Full M2 and later milestones remain open.**

## Reviewed behavior

The director reviewed runtime resolution/context code, checked graph accessors, interpreter integration, all 22 public runtime tests, 21 public interpreter tests, seven new private guards, genuine Java reference tooling, raw field bodies and documentation. No blocking finding remains. Runtime `field.rs` was frozen at SHA256 `B9E8E01C2F19C61A0AE2ABC25105C31B2203FBC4B33915FBAD3389E3E0A7E4EC`; the coordinator integrated the declared frozen files from both isolated worker worktrees before its final checks.

The context preserves exact UTF-16 names and defining/initiating loader identities. It seeds bindings from the admitted graph, detects contradictory loader/name bindings, checks resolver results and retains only successful bindings. Borrowed names avoid duplicated strings. All participating origins and resolved owners require admitted layouts. Heap replacement and layout extension invalidate the context; allocation and value mutation remain permitted. Iterative lookup preserves own/interface/superclass precedence, including diamond handling and the exclusion of interface Object dependencies from field inheritance.

Access rules distinguish runtime packages by loader and exact package text. Same-class private access works; unsupported nest and cross-loader reference constraints remain host errors. Public inherited fields do not incorrectly require an accessible declarer. Review found and corrected the protected-static qualifier: the additional symbolic-owner and actual-receiver restrictions apply only to non-static protected members. Regression tests cover sibling symbolic owners with null and non-null receivers for both reads and writes. The admitted-origin requirement was also made explicit and tested.

Resolution/access/loading checks precede staticness, write finality and null dereference. Only explicitly modeled Java failures become symbolic outcomes. Foreign identities, malformed inputs, unsupported checks and resource exhaustion remain structured host errors. Default storage, subword conversions, floating bits, owner-qualified hiding and null-only reference stores reuse the approved memory service.

The interpreter reuses one instruction loop, decoder, frame stack and service selection. The registry validates MethodId before reading metadata. Root local zero holds the checked receiver; numeric arguments and static helpers preserve their widths and respective local layouts. Field origin stays the declaring ClassId when the actual receiver is a subclass. The prepared cache cannot turn an instance root into an invokestatic target. Earlier entry points continue rejecting field instructions.

Tests cover exact fuel/frame/slot/code limits, context work/name limits, name aliases and resolver retries, equal-count heap replacement, cross-loader package distinctions, protected access, final/static/null ordering, malformed operands, unsupported declarations, category-two overlaps, wide reference aliases, persistent effects after every supported failure category and callee provenance. A 10,000-class test exercises lookup, ancestry and destruction without recursion. Private guards check forged metadata/handles, counter overflow, reservation failure and defensive frame capacity without weakening registry admission.

## Independent evidence

The coordinator independently ran final checks on the frozen integrated source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 341 passed, zero failed |
| Optimized runtime/interpreter tests | 254 passed: 137 runtime, 117 interpreter |
| Interpreter/JDK comparisons | All 8,838 match in both profiles |
| Array-service/JDK comparisons | All 151 match in both profiles |
| Instance-field-service/JDK comparisons | All 151 match in both profiles |
| Fixture reproduction | 36 classes; preceding 34 hashes unchanged |
| Reference reproduction | All seven output sets reproduced |
| Expanded-corpus parser smoke | 10,000 cases, zero failures |

The 553 new execution comparisons comprise 402 genuine Java 21 instance-body cases and 151 original synthetic raw-store bodies checked against the independently generated field-store oracle. The genuine cases cover 21 methods, including six mixed-width cases, inherited bodies, hiding, static helpers, arrays and symbolic failures. Their constructors leave fields at defaults, matching the explicitly supplied memory state without asserting CrabSpot constructor execution.

Raw Z/B/C/S bodies contain direct putfield/getfield with no narrowing conversion and return I, so instruction or return narrowing cannot hide a storage error. Floating comparisons classify NaNs; separate synthetic tests require exact stored payload bits. Persistence assertions after void/arithmetic/null exits are additional source-derived checks, not separately recorded JDK state outputs.

The director's approval combines source review with the coordinator's independent execution evidence; it does not claim another test run by the director. The earlier ordinary-instance-storage implementation passed the complete hosted Linux/Windows workflow. Hosted validation of this new field-execution increment is pending publication and must be recorded separately.

## Remaining boundary

This is an explicit unnamed-module method-body kernel with caller-supplied ordinary-instance storage. It does not establish constructor completion, class initialization, verification or general Java invocation. Reference parameters/results, non-null reference stores, named modules, nest resolution, cross-loader reference constraints, virtual/special/interface calls, exception dispatch, monitors and GC remain unimplemented in this profile. No complete-conformance or performance-superiority claim is made.

The next design direction is recorded in `m2-instance-calls-next.md`; it requires a concrete reviewed contract before implementation.

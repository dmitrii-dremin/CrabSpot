# Bounded virtual-call review

Date: 2026-11-03.

**The bounded virtual-call increment is approved against `m2-virtual-calls.md`. No blocking finding remains. Full M2 and later milestones remain open.**

## Reviewed behavior

The director reviewed the runtime method-resolution service, its reuse of the field context, the shared interpreter engine, all 24 new public runtime cases, 17 new public interpreter cases, four new private guards, genuine Java/reference tooling and documentation against `m2-virtual-calls.md`.

The frozen runtime `method.rs` has SHA256 `196CFCF411CFEDF2DA465B845CD17191D89B456BFFAD253D10C09BAEE335C059`. Frozen interpreter `execute.rs` has SHA256 `1244E77E53093197503EBAC64006B785A108DA3398E8D686C11AA3EA7DE7C9B1`, and `error.rs` has SHA256 `FBCAA6A75C94B3470A07DC73AD501D3CD3AB26E53AAFB2A01255925A1EF40BAA`. These hashes identify native working-copy bytes before Git line-ending normalization; an LF-normalized checkout may have different hashes. The director verified these hashes; the coordinator integrated the workers' declared files from their isolated worktrees.

Class-method resolution preserves symbolic-owner accessibility, exact name/descriptor matching, signature-polymorphic priority and member access before staticness/null. Public inherited declarations do not incorrectly require separate declaring-class accessibility. Protected static methods avoid the extra nonstatic owner/receiver restrictions. Unsupported eligible interface declarations are reported as witnesses requiring later resolution; the service neither silently skips them nor pretends to have selected a default. Private/static interface declarations do not prevent ordinary class resolution or count as eligible fallback candidates.

Selection preserves private targets, defining-loader package identity and transitive package overriding. The tests include a public bridge followed by later package declarations in different packages, and identical package text under different loaders. Defensive final-override checks inspect matching ancestors above the resolved declaration and preserve private-final independence. These checks are an explicitly limited host defense, not verification. Null precedes selected abstract-target failure; selected native/synchronized methods are returned by the runtime and rejected when the interpreter prepares execution.

The interpreter uses one loop with registered MethodId cache/frame identities. Each frame uses its own declaring class's constant pool, field origin and return metadata. Same-index targets in different classes cannot alias. Static helpers remain restricted to the current declaring class, including after crossing classes. Cache hits preserve invocation mode. Mixed-width argument placement, all numeric returns, subword narrowing, raw floating values and remaining caller operands reuse the established frame rules.

Diagnostics now retain optional registered owners. Review explicitly preserved the existing transition-site convention: a return-transfer failure identifies the receiving caller and that caller's invoke instruction, rather than substituting its parent call site. Cross-class regressions cover this behavior and malformed callee provenance. Only top-level MethodAccessError::Java variants become symbolic Java outcomes; nested context and policy failures remain host errors.

The context's existing bindings, invalidation and work/name policies apply to methods without a separate dispatch cache or graph. Tests exercise exact cumulative field/method work limits, symbol quotas, successful binding reuse, foreign/unadmitted identities, heap replacement and layout extension. Temporary reservation failures and forged private handles are checked. A 10,000-class hierarchy and 10,000 active interpreter frames exercise traversal, execution and destruction without host recursion. Fuel, frame slots, frame counts and distinct prepared-code bytes remain shared and bounded. Completed writes and allocations persist after symbolic, unsupported, malformed-target and policy failures.

## Independent evidence

The coordinator independently checked the final integrated frozen source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 387 passed, zero failed |
| Optimized runtime/interpreter tests | 300 passed: 164 runtime, 136 interpreter |
| Interpreter/JDK comparisons | All 9,332 match in both profiles |
| Array-service/JDK comparisons | All 151 match in both profiles |
| Instance-field-service/JDK comparisons | All 151 match in both profiles |
| Fixture reproduction | 43 classes; preceding 36 hashes unchanged |
| Reference reproduction | All eight output sets reproduced |
| Expanded-corpus parser smoke | 10,000 cases, zero failures |

The new oracle comprises 494 JDK 21 outcomes across 40 receiver/method pairs. Genuine fixtures cover overridden, inherited, final and private invokevirtual targets; base-selected bodies on child/leaf receivers; recursion; hidden fields; mixed-width values; arrays; static helpers and failures. Four cross-package types distinguish a same-signature non-override from a later public bridge and its transitive descendant override. All preceding 36 fixture hashes remain unchanged. Hidden-field persistence assertions are additional source-derived checks, not separately recorded JDK state outputs.

The director's approval combines source review with the coordinator's independent execution evidence and does not claim another director test run. The accepted field-execution baseline passed complete hosted Linux/Windows validation. After publication, the coordinator inspected complete [run 41730095972](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41730095972) for implementation commit d6558d8: Linux and Windows both passed with Rust 1.99.1 and Temurin JDK 21.0.13, including all eight regenerated reference sets, checked-in/regenerated fixture tests, strict lint, optimized tests and parser smoke. This hosted evidence is recorded separately after source approval.

## Remaining boundary

This increment evaluates bounded numeric/void body graphs on explicitly supplied ordinary-instance storage. It does not establish construction, initialization, verification or application startup. Interface/default selection, invokespecial, invokeinterface, reference parameters/results, non-null reference stores, generalized cross-class static calls, named modules, nest resolution, exception objects/dispatch, native binding, monitors and GC remain outside this profile. No full-M2, complete-conformance or performance-superiority claim follows.

The next bounded direction is `m2-interface-dispatch-next.md`. It remains a design note requiring a concrete contract and separate authorization before implementation.

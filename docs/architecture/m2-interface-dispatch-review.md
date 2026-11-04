# Interface and default-method dispatch review

Date: 2026-11-04.

**The bounded interface/default-dispatch increment is approved against `m2-interface-dispatch.md`. No unresolved finding remains. Full M2 and later milestones remain open.**

## Reviewed behavior

The director reviewed the shared runtime candidate/resolution/selection implementation, interpreter call and preparation modes, all 23 new public runtime tests, 18 new public interpreter tests, new/extended private guards, genuine Java/JDK tooling and documentation against `m2-interface-dispatch.md`. The implementation preserves one admitted graph/context and one interpreter loop. No application execution is delegated to the reference JDK.

Frozen runtime `method.rs` has SHA256 `88E786166F38468368AAF922931F8776702BDBDA1C2E0CC97680A931AEB67A19`; frozen interpreter `execute.rs` has SHA256 `992ED001DA7E6221B41352E9C1E4C94A7773591CC87379A999D53BDA6FAC592E`. These identify native working-copy bytes before Git line-ending normalization, so an LF-normalized checkout may have different hashes. The director verified the worker hashes; the coordinator integrated the declared frozen files and verified the matching production snapshots before final checks.

Resolution preserves class and own-interface priority, explicit Object metadata, eligible inherited declarations and deterministic permitted resolution witnesses. Receiver selection independently examines the full applicable interface closure. Diamond identities are deduplicated; abstract redeclarations suppress ancestor defaults, while unrelated abstract peers do not suppress a unique concrete maximal declaration. Competing defaults produce a conflict after the null check rather than a premature resolution error. Class implementations retain precedence, including inherited implementations whose declaring class does not itself list the interface.

Interface receiver membership uses exact ClassId identity, including defining-loader distinctions. Access/staticness precede null, selected visibility precedes abstractness, and private same-interface calls retain their exact target. Public inherited declarations do not incorrectly require a separately accessible declarer. Existing class package/final defenses remain in place. Inconsistent test-only Object identities, malformed/foreign handles, unsupported nests/descriptors and policy exhaustion remain host failures.

The interpreter distinguishes Static, DirectInstanceRoot and ResolvedInstance preparation. Selected concrete interface bodies execute, while direct host interface roots stay excluded. Static/instance cache checks remain active; the same MethodId reached through invokevirtual and invokeinterface consumes one prepared-code charge. Each interface frame uses its own CP, origin and diagnostic owner. Own-interface static helpers preserve existing version/kind restrictions. Argument counts include the receiver and category-two widths, and mismatch is checked before extraction/resolution at reached instructions. Decoder encoding checks remain eager; this is not unreachable CP/count verification.

Tests cover exact count 255 using 254 Int values and 127 Long values, required count 256, mixed widths, all numeric/void returns, subword narrowing, raw floating payloads, malformed operands, explicit Object bodies, class/default/private/static precedence, context invalidation and bindings, owner-aware failure provenance and persistent writes/allocations. Deep tests exercise 10,000 interface ancestors and 10,000 interpreter recursion steps through explicit frames without host recursion. Existing symbolic-versus-host error distinctions, frame/fuel/code policies and prior virtual-call regressions remain intact.

## Corrected review finding

Initial candidate dominance code compared a pair's MethodId identities before charging LookupSteps. Although dispatch and storage bounds were correct, self pairs escaped the contract's requirement to charge before every candidate-pair comparison. The worker moved the charge before that identity comparison. No correctness requirement or budget policy was relaxed.

Public regressions isolate the correction: a simple selection with limit 19 fails requiring step 20, while exact 20 succeeds. A peer-candidate case stops immediately before the first self pair at 55, then at the next pair/ancestry boundaries; a separate fanout limit stops before enqueueing the first interface edge. Private candidate-summary tests distinguish arbitrary resolution witnesses from abstract/conflict/unique-default selection. These complement cumulative field/method budgets and fallible reservation guards.

## Independent evidence

The coordinator independently checked the final integrated frozen source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 431 passed, zero failed |
| Optimized runtime/interpreter tests | 344 passed: 188 runtime, 156 interpreter |
| Interpreter/JDK comparisons | All 10,669 match in both profiles |
| Array-service/JDK comparisons | All 151 match in both profiles |
| Instance-field-service/JDK comparisons | All 151 match in both profiles |
| Fixture reproduction | 59 classes; preceding 43 hashes unchanged |
| Reference reproduction | All nine output sets reproduced |
| Expanded-corpus parser smoke | 10,000 cases, zero failures |

The new JDK oracle has 1,337 outcomes across 112 receiver/method pairs, including 98 null, seven arithmetic and 14 negative-array outcomes. The original Java hierarchies cover defaults, more-specific defaults, class overrides, diamonds, reabstraction with concrete implementations, private interface helpers, own static helpers, recursion, mixed values, fields and arrays. The coordinator confirmed javac emits invokeinterface for the private helper and InterfaceMethodref invokestatic for its own static helper. Rust tests supply default storage and explicit minimal Object metadata; they do not claim constructor execution. State/allocation checks are additional source-derived assertions, not separately recorded JDK state output. NaN results are compared by classification; other floating outputs are bit-exact, with synthetic tests preserving raw payloads.

Approval combines the director's source review with the coordinator's independent execution evidence; it does not claim a separate director test run. The preceding virtual increment passed complete hosted Linux/Windows validation. This increment's hosted evidence remains pending publication and must be recorded separately.

## Remaining boundary and next work

This is an unnamed-module numeric/void body-graph evaluator on explicitly supplied ordinary-instance storage. Constructors, initialization, verification, invokespecial, general cross-class static invocation, reference arguments/results, non-null reference stores, named modules, nest services, exception objects/dispatch, native binding, monitors and GC remain outside this profile. No complete-M2, full-conformance or performance-superiority claim follows.

`m2-special-calls-next.md` records the next direction: ordinary nonconstructor invokespecial using distinct selection origins and existing runtime/engine services. It remains design-only until the exact next contract is authorized. User authorization for autonomous continuation is already established.

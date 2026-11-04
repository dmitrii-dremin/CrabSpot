# Supplied-Throwable dispatch review

Status: approved by the technical director on 2026-11-04 for the bounded opt-in supplied-Throwable profile. Frozen technical review and the coordinator's integrated gate satisfy `m2-exception-dispatch.md`. Full M2 remains open. Hosted Linux/Windows CI also passes; see the evidence below.

## Reviewed invariants

An explicit Throwable ClassId is checked against the admitted registry, designated bootstrap loader, bound reference universe, exact name, flags and Object dependency. Opaque profiles retain the heap/layout/universe snapshot; opaque thrown tokens retain a checked ordinary-instance identity. Neither token certifies Java construction or initialized Throwable library state. Fresh contexts over an unchanged heap may reuse the profile; replacement, layout growth or incompatible universe invalidates it. Ordinary heap allocation alone does not.

Throw validation and typed catch matching use class-only ancestry. Typed catch resolution uses the searched method's declaring loader and caller access, with full name charging and existing binding limits. Catch-all matching performs no type resolution. The interpreter examines only active rows until the first match, preserving table order. Interfaces, arrays, unrelated catch types, forged handles and context failures remain structured host errors.

The shared engine has an optional exception state rather than a second execution loop. Cached bodies retain a Code attribute index and borrow original exception rows; handler metadata and names are not copied per prepared method. In the opt-in profile, code quota and decoding precede aggregate handler accounting and structural checks. Every handler boundary is checked against decoded instructions, with the exclusive end also permitted at code length. The aggregate handler count precedes the nonempty-table stack-capacity check. Older entry points retain their earlier handler rejection order.

Explicit athrow preserves the supplied identity. Iterative dispatch charges each frame probe, each examined row including inactive rows, and each unwind. A handler atomically replaces the operand stack with the original reference while preserving locals. Unwind validates caller provenance and slot accounting before removing a frame, then searches the actual invocation PC rather than the caller's advanced resume PC. Uncaught completion returns the original reference. Instruction fuel, dispatch work, runtime lookup work and prepared-handler limits remain distinct bounded counters.

All eleven existing VM-produced exception/error categories, including direct method-selection failures, become explicit UnsupportedVmException host failures in this profile. Null athrow follows the same policy. They do not search handlers, fabricate Throwable storage, or return a symbolic Java uncaught completion. Only direct designated Java variants are mapped; nested Java-shaped diagnostics remain host errors. Existing entry points and symbolic outcomes are unchanged. The new profile supports 192 effective opcodes; the earlier runtime-aware profile remains at 191.

## Review outcome and coverage

No unresolved correctness or contract finding remains in the frozen source. The unwind helper was extracted for private corruption tests without changing its algorithm or ordering. Review specifically checked the separate direct method-failure path, empty-table unwind charging, invocation-PC propagation, handler stack reservation before clearing, and direct versus nested error mapping.

Eighteen public runtime tests and three private tests cover profile identity and invalidation; exact 21-step anchor, three-step throw and nine-step Base-catch examples; zero-work profile/catch-all checks; repeated 4,096-unit names; loader delegation and runtime-package access; pre-callback symbol/unit limits; retained bindings on late failure; null, foreign, stale and forged sources; and a 10,000-class iterative ancestry/drop. Private cases validate profile/token corruption and preservation of nested error sources.

Eighteen interpreter tests and four private tests cover ordered overlapping handlers, rethrow/replacement identity, stack clearing and local preservation, all existing call forms, exclusive range endpoints, lazy catch resolution, structural boundary errors, and exact shared fuel/code/frame/slot/handler/dispatch limits. All eleven actual VM instruction outcomes plus null athrow are exercised with a catch-all present and zero dispatch allowance, proving the unsupported policy takes precedence. Earlier writes remain visible. A 10,001-frame unwind uses exactly 20,002 dispatch steps without recursion. Private cases check overflow atomicity, caller/slot corruption before mutation, nested-error mapping and reservation before operand-stack replacement.

The original ExceptionDispatch fixture adds 336 independent JDK outcomes over 36 receiver/method pairs: 240 numeric returns, nineteen uncaught-first and 77 uncaught-second identities. It covers ordered typed catches, cross-frame propagation, recursion, catch-all cleanup, rethrow/replacement, handler calls and all existing invocation paths. Supplemental assertions inspect state, saved/first/second identities, actual classes, child-edge order and unchanged allocation totals. Decoded bytecode confirms athrow and the call families; a handler ending exactly after an invoke distinguishes invocation-PC search from resume-PC search. Constructors occur only in the JDK oracle driver. Minimal core metadata and supplied exception storage are explicit test inputs, not a production library fallback.

## Frozen evidence

The director verified all sixteen combined files against `target/coordinator/exception-freeze.json`, after reviewing the four-file runtime and six-file interpreter components. Principal SHA-256 values describe native working-copy bytes before Git line-ending normalization:

| File | SHA-256 |
| --- | --- |
| runtime/src/exception.rs | `739916338856BFE4E7815B262A90042AE939AB43D934B7DB48199EF0DED7EA08` |
| runtime/tests/exceptions.rs | `F20BFF641386855218B72743B4CC480B4261C5A80437F206199E7772C22D05D7` |
| interpreter/src/execute.rs | `4C143D2E3883228192A00AAAB44C6420D01C66F6F8B289B9EFBBC4F57903D8E5` |
| interpreter/tests/exceptions.rs | `2B21DB040829BD6AA8C1BFCA7EF45B33F9B9A09DBA46193C65585098B9485151` |

The coordinator independently observed on the frozen combined source:

- Formatting and strict workspace/all-target Clippy pass.
- All 724 workspace tests and 637 optimized runtime/interpreter tests pass, with zero failures. The optimized split is 341 runtime and 296 interpreter.
- All 25,661 execution comparisons pass in both profiles: 25,325 prior cases plus 336 supplied-exception cases. Separate 151-case array and 151-case field service oracles pass.
- All 98 class fixtures and sixteen reference-output sets reproduce; all previous 92 class hashes remain unchanged. The 38-source/sixteen-record provenance preflight and six Python regressions pass.
- The 98-class corpus passed 10,000 seeded parser mutations: 131 accepted, 9,869 rejected, ten deterministic rechecks, corpus 229 and zero failures; seed 4851011501752864596.

All sixteen frozen hashes were reverified after the gate. Local validation used Rust/Cargo 1.98.1 and Zulu JDK 21.0.7. Implementation `215ec91ee10cd9b20e9860a33376193b82c89198` subsequently passed complete [Linux/Windows run 41824069552](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41824069552). The coordinator inspected both job logs: Rust/Cargo 1.99.1 and Temurin JDK 21.0.13; six Python regressions; 38-source/sixteen-record preflight; 98 reproduced classes and all sixteen reference sets; 724 workspace tests before and after regeneration plus 637 optimized tests (2,085 Rust test executions per platform); strict formatting/lint; and identical 10,000-case parser results with zero failures.

## Boundary and next direction

This approves supplied-reference throwing, ordered handlers and iterative abrupt completion in the selected-body kernel. It does not approve VM exception materialization, Throwable construction/messages/stack traces, native or synchronized methods, monitors, async exceptions, initialization, full verification/loading constraints, GC, thread startup, full M2, JVM conformance or performance superiority. Class preparation, static state and initialization responsibilities are the proposed next direction in `m2-class-initialization-next.md`; an exact contract must precede implementation.

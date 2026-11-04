# Registered static-call resolution review

Status: approved by the technical director on 2026-11-04 for the bounded registered static-call increment specified in `m2-static-call-resolution.md`. Frozen source review and the independent integrated gate pass. Full M2 and complete JVM linkage remain open; fresh hosted CI is pending publication.

## Reviewed behavior

The runtime exposes resolve_static over the existing admitted graph, loader/name bindings and work counters. Common class/interface method resolution, access and nominal loading checks precede the required static category. The refactor preserves the old instance-call resolver's phase order and costs. A nearer instance/private declaration is not skipped to find a static ancestor. Interface-owned statics are supported, while parent-interface static/private declarations are not inherited. Public Object or eligible default/abstract fallback results cannot become static targets. Protected access follows the actual member's flags, preserving symbolic comparability for a protected nonstatic result before its category failure.

The result is the actual declaring MethodId, with no receiver selection, override/finality audit, Code preparation or initialization performed by resolution. Runtime packages retain defining-loader identity. Reference-bearing nominal signatures conservatively require matching origin and actual-declarer loaders, including all-null argument cases; primitive-leaf arrays preserve their existing boundary. Successful bindings remain cached, while origin-sensitive method resolution and descriptor/value work repeat on each call.

The interpreter changes only registered b8 target routing. All runtime-aware profiles use the checked service and actual MethodId. Standalone ClassFile and heap-only execution retain their earlier same-class resolver. Target preparation and cache-role checks, frame/slot reservation, typed argument extraction and actual-declarer value checks preserve the static-call order. Code cache keys remain complete MethodIds, allowing symbolic aliases to share prepared Code without sharing authorization.

Host-controlled profiles do not acquire automatic initialization or extra field/exception capabilities. InitializationSession reuses its existing pending call: initialize the resolved declaring owner, then resume retained arguments without another selector call, pop or fuel debit. A symbolic subclass remains uninitialized when only its inherited static declaration is used. Cross-class initializer recursion, supplied Error propagation and later target-body exceptions use the same engine and original caller invoke PC.

## Review findings and migrations

No production defect was found in staged or frozen review. The runtime factors its existing common resolution phase and adds the static category wrapper. The interpreter routes the returned MethodId directly rather than reconstructing it under the caller's owner. Existing direct-only Java error mapping remains shared and covered by earlier private guards plus new public routing tests.

Seven old regression cases across six files migrate narrowly to the new registered-call contract. Static/instance mismatch now produces resolution-level IncompatibleClassChange rather than the old preparation host error. Initializer-name rejection is the shared boxed UnsupportedSpecialMethod. A cached-helper regression now uses a private helper to prove that entering from another declaring class cannot reuse prior authorization. Standalone test expectations remain unchanged.

## Frozen source and coverage

Hashes identify native working-copy bytes before Git line-ending normalization:

- Runtime method resolver: `2F7AA582849D747B77643A77239AF078666B0E48855A495C505E4E978E10977F`.
- Runtime public suite: `6BA5C7C69570607B13FE3531CF1529298B56CD952485F13CE6668AA20B2AD91A`.
- Interpreter engine: `F6E2B0BC43CEFA53F40E8AF0153C71DE0F2ECD33B3EEE4DA392CCE836FFBEBFE`.
- Interpreter public suite: `587FE2D33277EE8B643A28A55356F9FE7A2678E3A70BC50A721E188E0FECAC81`.

The director verified all three runtime and nine interpreter component files against immutable snapshots and integrated/worker copies, and all 37 combined manifest entries. Reviewed additions include twenty-one public/one private runtime tests and twenty public interpreter tests. Cases cover UTF-16 aliases, access/loading/category precedence, interface fallback, exact mixed work counters and callback limits, context replacement, 10,000-class traversal and 10,000 cross-class frames, actual-declarer references, all numeric widths/raw bits, cache sharing, session recursion, deferred arguments, Error versus body-throw provenance and sticky interrupted initialization.

The independent JDK oracle adds 154 distinct rows: 133 session scenarios and 21 ordinary-body scenarios. The latter run through all four host-controlled profiles, making 217 profile-level comparisons before successful-repeat checks. Fresh loader identities isolate JDK initialization cases. Independent models inspect all 21 static slots and 24 admitted statuses, aliases, arrays, roots, counters and failure effects. Explicit CP assertions pin inherited symbolic-child and interface/private helper targets and mixed signatures. A real Plain clinit would mutate Trace; all four host-controlled profiles leave every static slot at its default and a fresh subsequent session reports every class uninitialized.

The coordinator completed the independent gate on the frozen implementation:

- Formatting and strict workspace/all-target Clippy pass.
- Workspace debug validation passes 882 tests. Optimized runtime/interpreter validation passes 795 tests (410 runtime and 385 interpreter), with zero failures.
- All 26,324 distinct execution-reference rows pass in both profiles, with the documented multi-profile/repeated executions. The new 154 rows contain 113 Int, seven Long, seven Float, seven Double and twenty supplied uncaught-reference outcomes (nine first and eleven second identities). The separate 151 array-service and 151 field-service cases remain passing.
- All 151 genuine class fixtures and nineteen reference-output sets reproduce; all 132 previous class hashes remain unchanged. The forty-one-source/nineteen-record preflight and six Python provenance regressions pass.
- The expanded corpus passes 10,000 parser mutations: 110 accepted, 9,890 rejected, ten deterministic rechecks, corpus 256 and zero failures; seed 4851011501752864596.

All 37 combined source hashes were reverified after the gate; whitespace checking passes. Forty-four Rust tests were added: twenty-two runtime, twenty interpreter and two root-oracle tests. Local validation used Rust/Cargo 1.98.1 and Zulu JDK 21.0.7. Fresh hosted Linux/Windows evidence remains pending publication and must be recorded separately.

## Boundary

This approval extends registered static invocation over admitted metadata. It does not establish complete loading constraints, persistent runtime-constant-pool linkage/failure caching, nestmates, native binding, synchronization, constructors, String materialization, VM exception construction, Java startup, full M2 or the ultimate conformance/performance target. The next direction is `m2-construction-next.md`; its exact contract must precede implementation.

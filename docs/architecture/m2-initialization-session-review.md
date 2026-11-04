# Single-session initialization review

Status: approved by the technical director on 2026-11-04 for the bounded single-owner initialization session specified in `m2-initialization-session.md`. Frozen source review and the coordinator's integrated gate pass. Full M2 and complete JVM initialization remain open. Fresh Linux/Windows CI passed for implementation 6dbbef4; inspected evidence is recorded below.

## Reviewed behavior

An opaque interpreter session owns ordered lifecycle records and exclusively borrows the checked runtime context. Its constructor reserves from actual admitted membership before acquiring a permanent heap claim. Failed construction does not claim the heap; dropping a successful session never permits another session to replay initialization on it. Read-only status distinguishes admitted classes from merely registered definitions. No runtime API certifies completed initialization.

The runtime validates an explicit bootstrap Error identity against the designated Throwable. Plans retain compact dependency identities and numeric ConstantValue entries, with remaining quotas and charged metadata traversal. Static ConstantValue applies even without FINAL; instance attributes are ignored. Subword normalization and raw floating bits are preserved. Unsupported static String constants and historical parameterized initializers reject preflight. Numeric publication validates every destination and value before the first write and scans only the declaring class's checked static range.

Class dependencies are superclass first, then declared-order interface DFS postorder with identity deduplication. Concrete private, bridge and synthetic instance declarations qualify; static, abstract and initializer declarations do not. Interface initialization itself skips ancestors. Previously completed child interfaces cannot prune ancestor enumeration for a later class. Planning, scheduling and drop are iterative.

The shared engine prepares actual clinit()V bodies under a distinct role, including historical ignored flags. Numeric constants precede dependencies. Active-use static field operations and supported same-class static calls retain checked selections, operands and original PCs across initialization, without replaying fuel, pops or selector work. Final writes require the actual declaring initializer role; helpers and other initializers do not inherit that authority. Older body profiles keep their previous gates and host-controlled memory behavior.

Same-evaluation recursion observes partial state. Successful completion persists across evaluations. Supplied Error objects escaping an initializer become retained erroneous causes, including waiting dependency parents, before ordinary caller handler search resumes at the triggering instruction. A dependent Error caught within another initializer can permit that initializer to complete. Non-Error wrapping and subsequent NoClassDefFoundError materialization remain explicit unsupported host outcomes. All eleven VM-produced categories remain unsupported before handler search.

Every host exit and recovered resolver panic aborts remaining active records through allocation-free cleanup, preserving completed/erroneous records and prior effects. Aborted classes cannot retry. Scheduler, retained-plan, frame, Code, handler, fuel and context-work policies retain distinct checked accounting. Empty initializers require scheduler work but no Java frame or instruction fuel. Retained causes are enumerated in record-insertion order, including aliases.

## Review findings and corrections

The coordinator found repeated full-name/descriptor comparisons on reusable static continuation tokens. Scalar immutable CP/member fingerprints now replace those uncharged scans while preserving selector results and normal assignment work. A read-only admitted-class check was added for status, and atomic descriptor charging plus early-stop interface predicate scanning were pinned explicitly.

Private range review added an O(1) range-completeness guard and a late foreign-field regression. Constant publication rejects corrupted ranges, entries, locations, attributes, CP identities and normalized values before mutation; prior nonzero values and reference roots remain intact. Extension preserves existing and zero-field ranges.

Initializer diagnostics now attach an optional responsible ClassId centrally without replacing a more specific inner class or actual method/PC/call-site provenance. Ordinary root failures after completed initialization remain unlabelled. A large inline continuation token was replaced with a fallibly reserved reusable one-entry vector per participating frame; legacy frames allocate no token buffer. Normal and abrupt resumption consume/clear the token.

The root oracle's new twoFailures case was excluded from same-session repetition, matching the JDK driver: repeating an erroneous active use requires the deliberately unsupported later-failure protocol. An early reviewer concern about dependency scheduling was withdrawn after confirming that a return exits the local closure, while the outer scheduler continues; no production correction was needed for that point.

## Frozen source and evidence

Component hashes below identify native working-copy bytes before Git line-ending normalization:

- Runtime initialization: `0724E73541EB8D6CD242BBF6A6A3A39106BF613D10EF539BE25615B90DD34641`.
- Runtime static storage: `525259799B459989F5AAF9CA35A7CF1E9A8688205AE0D9C80CFF6F00863E451B`.
- Interpreter engine: `F5D4A972149A20E1C7F1727B8B7CA9FA6DB34F27CFC4AFF9C82A4C6B2EC071DA`.
- Interpreter session: `8B72156E699F186FED4A49DEC1A9AB3FCCF18EE374BEEFEB86733E4765A69C10`.
- Independent boundary suite: `C6762A533213F791F72AF5B8140F6CBC52D5D3076AB2C7FA194AB4EBD3E64193`.

The technical director independently verified all seven runtime, six interpreter and one independent-suite component files against their immutable snapshots and integrated copies, plus all 48 entries in the coordinator's combined source manifest. Reviewed new coverage comprises sixteen public/six private runtime tests, twenty-nine public/five private interpreter tests, and twelve independent boundary tests. It includes all VM production routes, resolver panic recovery, exact quotas/step boundaries, historical flags, preserved effects, token reuse, late Error-propagation failure and 10,000-dependency planning/scheduling/drop.

The independent oracle contains 145 cases: ninety genuine initialization scenarios and fifty-five original nonfinal numeric ConstantValue encodings. Fresh JDK class-loader identities isolate cases; successful repeats reuse the same loader. The Rust model independently checks all 38 static slots, all 32 admitted statuses, ordered retained causes, aliases and allocation counters. Raw subword probes use getstatic/ireturn without conversions that could mask store normalization.

The coordinator completed the independent integrated gate on the frozen source:

- `cargo fmt --check` and strict workspace/all-target Clippy pass.
- `cargo test --workspace` passes 838 tests; optimized runtime/interpreter validation passes 751 tests (388 runtime and 363 interpreter), with zero failures.
- All 26,170 execution comparisons pass in both profiles, including the 145 new initialization rows (141 numeric and four supplied uncaught Error outcomes). The separate 151 array-service and 151 field-service comparisons remain passing.
- All 132 genuine class fixtures and eighteen reference-output sets reproduce. All 104 prior class hashes remain unchanged. The forty-source/eighteen-record preflight and six Python provenance regressions pass.
- The expanded corpus passes 10,000 parser mutations: 134 accepted, 9,866 rejected, ten deterministic rechecks, corpus 256 and zero failures; seed 4851011501752864596.

All 48 combined source hashes were reverified after the gate, and whitespace checking passes. The seventy added Rust tests comprise twenty-two runtime, twenty-nine primary interpreter, five private interpreter, twelve independent boundary and two root-oracle tests. Local validation used Rust/Cargo 1.98.1 and Zulu JDK 21.0.7. The hosted evidence below is subsequent to this local approval and validates the published implementation.

## Hosted validation

Implementation `6dbbef4ad399f9d0f5938b3bea2485e08e45eff4` passed complete
[Linux/Windows run 41839995587](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41839995587).
Both job logs were inspected: Rust/Cargo 1.99.1, Temurin JDK 21.0.13, six Python
regressions, 40-source/18-record preflight, 132 reproduced fixtures and all 18
reference sets. Each platform passed 838 checked-in and 838 regenerated workspace
tests plus 751 optimized tests (2,427 Rust test executions), strict formatting/lint,
and parser smoke: 134 accepted, 9,866 rejected, ten rechecks, corpus 256 and zero
failures over 10,000 cases at seed 4851011501752864596.

## Boundary

This approves a single-owner supported initialization procedure over explicitly admitted metadata. It does not establish full verification/preparation, loader constraints, assertion-status services, String materialization, constructed VM exceptions or wrappers, concurrent initialization/JMM publication, constructors, Java startup, full M2 or the ultimate conformance/performance target. The next proposed direction is `m2-static-call-resolution-next.md`; its exact contract must precede production work.

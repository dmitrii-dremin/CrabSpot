# Current checkpoint: session object construction

Updated 2026-11-04. The [construction contract](../architecture/m2-construction.md)
is frozen, all components have passed source review, and the fresh-build
independent combined gate passes. [Bounded technical approval](../architecture/m2-construction-review.md)
is recorded; publication is authorized. M1 and the earlier bounded M2/runtime increments remain
approved; full M2 remains open. Continue autonomously without NanoSpec and ask
only when user intervention is necessary.

The ultimate objective remains complete Java SE 21 JVM conformance, required
observable compatibility with conforming JVMs, and demonstrated performance
superiority over HotSpot on a reviewed representative suite. Kotlin/Spring Boot
are integration milestones. No performance or full-conformance result is claimed.

## Implemented scope

InitializationSession executes new and constructor invokespecial using the shared
engine, scheduler, MethodId cache and heap. The effective session surface grows
from 194 to 195 opcode values. Other body profiles retain their construction gates.
The public Value/Reference representations remain unchanged; private locals and
operand values carry unfinished object aliases. Actual bootstrap Object constructor
Code runs, using an explicitly bound root identity rather than a production fallback.

New resolves/access-checks the target, rejects abstract/interface instantiation,
reserves publication capacity, funds an allocation-site scan, initializes the actual
class, then allocates and publishes once. Same-frame repeated-site unfinished stack
aliases fail; successful new invalidates older unfinished locals from that site.
Failed initialization/allocation retains prior effects without publishing a token.
Layout traversal uses incremental context work and existing heap quotas.

Constructor lookup preserves method-resolution/access/loading failures before the
resolved-owner rule; it does not inherit constructors or apply ACC_SUPER dispatch.
Private frame obligations distinguish Pending, Complete and Failed chains. A
successful this/super return promotes only its immediate caller's matching aliases;
escaping constructor exceptions invalidate only those unfinished aliases. Ordinary
helper exceptions caught before chaining leave Pending usable. Ordinary references
already escaped after an inner chain completes survive outer constructor failure.

Actual constructor frames may write final fields declared by their own class,
including through another lawful ordinary receiver. Before chaining, only the
current unfinished this may receive a write to its own declared field through its
own symbolic owner. Other value escapes remain rejected. Reference moves, legal
stack manipulation/discard and all four null/identity branches retain private tokens
without promotion. Separate ConstructionLimits fund alias scans; all existing
frame/slot/Code/fuel/handler/work/heap limits remain shared.

## Independent validation

The final integrated implementation passes two independent Rust oracle tests covering
184 distinct Java 21 rows: 112 genuine-source cases over 16 methods and 72 original
encoding cases over 9 methods, including 9 extra raw narrow-store boundaries. Every
retained field, hidden/final value, object/array type, element, alias/cycle edge,
static slot, admitted-class status and persistent root is checked separately from
JDK return values. Exact instance/slot/array/element/payload counts include failed
and unused allocations. The source loop allocates repeatedly at one new PC in one
frame; the sameSite factory case uses separate frames.

The corpus has 165 genuine javac classfiles (164 ordinary definitions and one module)
and 9 separately marked original class-file version 65 encodings. All 151 previous class hashes are
preserved. Provenance covers 42 Java sources and 20 reference records. Seven Python
regressions pass, including stale/missing/extra/corrupt synthetic artifact checks.
The raw JDK driver uses a child loader while Rust admits one defining loader;
these cases establish construction/value behavior, not cross-loader constraints.
The fixture Error constructors and reflection driver are outside evaluated Code.

All 55 frozen source/configuration/fixture hashes were rechecked after validation.
The final checks used a newly created build directory, not the superseded cache.

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 947 passed, zero failed |
| Optimized runtime/interpreter tests | 860 passed: 436 runtime + 424 interpreter |
| Distinct execution-reference rows | All 26,508 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match |
| Fixture/reference reproduction | 165 genuine classes, 9 original encodings and 20 output sets |
| Canonical provenance | 42 Java sources and 20 named reference records |
| Python provenance regressions | Seven passed |
| Expanded parser smoke | 10,000 cases, zero failures |

The increment adds 65 Rust tests: 21 public and 5 private runtime tests, 28 public and 9
private interpreter tests, and 2 root oracle tests. All 16 legal stack forms also run
with private aliases inside the existing stack-form test. New coverage includes
1,000 constructor frames, 10,000-class runtime traversal, raw metadata/authority
corruption, source/receiver identity, wrong roles, cache reuse, exact resource
boundaries, lifecycle failures and alias publication/invalidation atomicity.

The parser smoke used seed 4851011501752864596 with 174 initial classfiles:
119 accepted, 9,881 rejected, ten deterministic rechecks, corpus 256 and zero failures.
Local platform: Windows x86-64, Rust/Cargo 1.98.1, Zulu 21.0.7. Hosted construction
validation has not yet run. Full M2 remains open.

## Review corrections and boundaries

The draft review corrected an overrestriction: unfinished references may use
ifnull/ifnonnull and if_acmpeq/if_acmpne, and pop/pop2 may discard them. Original
class-file version 65 probes independently confirmed these rules. Ancestor-constructor access
errors precede the later constructor-owner mismatch; before-super own-field writes
and a caught ordinary helper exception were also checked against the JDK.

Implementation review corrected failed-alias removal to one pass, avoiding an unfunded
second stack scan. Constructor field/return authority is hardened against corrupted
activation/method bindings, with regression coverage before final freeze. The
runtime's private final-store proof also rechecks constructor role and static/final
metadata. Final diff review also found and fixed an observable if_acmp error-order
regression: both typed operands must be extracted before handle validation. New
foreign-handle plus missing/wrong-kind operand cases preserve the old precedence.
No correctness failure was hidden by weakening a test. The genuine
registry corpus count changes from 150 to 164 because fourteen classes were added.

This is executed-path construction protocol, not the full verifier. Stack-map
merges, global control-flow proof and verification rejection timing remain open.
Constructed VM exceptions/wrappers, String constants, full loading constraints,
named modules/nests, native/synchronized execution, concurrent initialization,
threads, libraries, GC/JIT and application startup remain incomplete. Failed
allocations persist in the arena; no active-frame GC root API or reclamation is
claimed. Kotlin/Spring Boot cannot run yet.

GPT-6 Astra/high owns interpretation and review; GPT-6.1 Sol/high workers use
separate mutable worktrees/output directories. Root owns fixtures, integration and
independent evidence. Hash-checked atomic integration preserves Admin ownership.
The user authorized autonomous review and publication. Source review and the
combined gate are approved; Linux/Windows evidence remains pending publication.
The next proposed direction is whole-method structural/code constraints followed
by type-state verification. Its exact contract requires a separate review.

A reused debug build directory initially omitted five new runtime unit tests
because copied snapshot timestamps predated the cached binary. Those results are
not acceptance evidence. The final gate uses a brand-new CARGO_TARGET_DIR and
rechecks the frozen source hashes; future snapshot integrations refresh mtimes.

## Previous published checkpoint

Registered static-call implementation 8ef7f43 and evidence commit d3ad54f passed
[Linux/Windows run 41845390025](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41845390025).
Both inspected jobs passed 882 checked-in and 882 regenerated workspace tests plus
795 optimized tests (2,559 Rust executions per platform), strict formatting/lint,
151 reproduced fixtures, 19 reference sets and parser smoke. Those results belong
to the preceding static-call checkpoint and are not construction CI evidence.

## Reproduce

```text
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p crabspot-runtime -p crabspot-interpreter --release --locked
python -m unittest discover -s tools -p test_fixture_source.py
python tools/check_fixture_sources.py
python tools/compile_fixtures.py --check
python tools/numeric_reference.py --check
python tools/floating_reference.py --check
python tools/call_reference.py --check
python tools/array_reference.py --check
python tools/field_reference.py --check
python tools/instance_reference.py --check
python tools/virtual_reference.py --check
python tools/interface_dispatch_reference.py --check
python tools/special_reference.py --check
python tools/type_reference.py --check
python tools/reference_field_reference.py --check
python tools/reference_array_reference.py --check
python tools/multidimensional_reference.py --check
python tools/reference_call_reference.py --check
python tools/exception_reference.py --check
python tools/static_reference.py --check
python tools/initialization_reference.py --check
python tools/cross_static_reference.py --check
python tools/construction_fixtures.py --check
python tools/construction_reference.py --check
cargo run -p crabspot-classfile --example fuzz_parser --locked -- tests/fixtures --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH, set
CARGO_INCREMENTAL=0, and use separate CARGO_TARGET_DIR and TMP/TEMP directories.
Coordinator final output is the fresh target/verification-construction-final; worker outputs are
target/construction-runtime-worker and target/construction-interpreter-worker.

# Current checkpoint: registered cross-class static calls

Updated 2026-11-04. M1 and earlier bounded M2/runtime increments are approved.
The static-call increment passes frozen-source review and its independent combined
gate. [Bounded approval](../architecture/m2-static-call-resolution-review.md) is
recorded; full M2 remains open. Continue
autonomously without NanoSpec; ask only when user intervention is necessary.

The ultimate objective remains complete Java SE 21 JVM conformance, required
observable compatibility with conforming JVMs, and demonstrated performance
superiority over HotSpot on a reviewed representative suite. Kotlin/Spring Boot
are integration milestones. No performance or complete-conformance result is claimed.

## Implemented scope

Five crates provide Java 21 class-file parsing, complete instruction-format decoding,
a bounded interpreter, checked runtime services and a metadata inspection launcher.
The existing primitive/reference, array, instance-field, virtual/interface/special,
exception, static-memory and initialization profiles remain available. The opt-in
static/Throwable instruction surface remains 194 opcodes; this change extends
registered invokestatic resolution without adding an opcode or execution loop.

FieldResolutionContext::resolve_static resolves a symbolic method to its actual
MethodId through the existing class/interface lookup, access and conservative
loading checks, then requires a static result. It preserves defining-loader package
identity, protected/private rules and interface-static noninheritance. It does not
skip a nearer nonstatic declaration or perform receiver dispatch. Public members
of a nonpublic declaring parent can remain accessible through a lawful public
symbolic owner. Nominal signatures crossing origin/declarer loaders still return
an explicit unsupported loading-constraint result, even for null arguments.

Every context-backed body profile now uses this resolver for b8, including
same-class calls. Standalone and heap-only APIs retain their previous same-class
resolver and budgets. The registered path deliberately gains shared resolution
work and structured method errors; existing instance-call phase order, charges
and diagnostics remain unchanged. A prepared-Code cache hit still performs
origin-sensitive resolution, and actual MethodId identity prevents aliases from
being rebound to the caller's class.

The engine retains its preparation and frame/slot checks before argument pops,
then validates mixed-width values through the actual declaration. In an
InitializationSession it suspends the checked call, initializes the actual owner,
and resumes once without repeating resolution, argument extraction or fuel debit.
An inherited static call through a child does not initialize that symbolic child.
Each callee uses its own constant pool and declaring identity. Initializer Error
propagation searches the original b8 PC; an ordinary callee throw uses normal
frame dispatch. Existing completed, erroneous and aborted session states persist.

The underlying initialization procedure remains single-owner and numeric-constant
only, with permanent heap claims, real clinit frames, superclass/default ordering,
recursive partial state and allocation-free abort cleanup. Other body profiles
remain explicit host-controlled execution and do not automatically initialize classes.

## Independent validation

All 37 frozen implementation, fixture and configuration hashes were rechecked
after the gate. Worker component manifests additionally cover READMEs.

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 882 passed, zero failed |
| Optimized runtime/interpreter tests | 795 passed: 410 runtime + 385 interpreter |
| Distinct execution-reference rows | All 26,324 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match |
| Fixture/reference reproduction | 151 classes and all 19 output sets reproduced |
| Canonical provenance | 41 Java sources and 19 named reference records verified |
| Python provenance regressions | Six passed |
| Expanded parser smoke | 10,000 cases, zero failures |

The increment adds 44 Rust tests: 21 public and one private runtime test,
20 interpreter tests and two root oracle tests. Runtime cases cover own/inherited/
hidden static declarations, interface/Object/default fallback, access and loader
boundaries, malformed tags/version/special names, exact budgets, stale/foreign
identities and a 10,000-class iterative traversal. Interpreter cases cover all
profiles, cache authorization, mixed reference/category-two calls, nested call
forms, preflight ordering, caller/callee provenance and initialization failures.

Seven old registered-policy tests across six files were migrated to the precise
new resolution outcomes. An old public cross-owner rejection became eligible;
its cache-authorization regression now uses a private cross-owner helper that
must still fail. Standalone tests were preserved. Review found no production
behavior defect; staged corrections were limited to explicit contracts, policy
expectations, test assertions and documentation.

The 154 new distinct JDK rows comprise 133 fresh-loader initialization scenarios
and 21 ordinary-body scenarios. The latter execute through four host-controlled
profiles: 84 evaluations, or 217 new profile comparisons including session rows,
before repeat calls. These counts must not be conflated with unique reference rows.
Outcomes are 113 Int, seven Long, seven Float, seven Double and twenty uncaught
supplied Error identities (nine first and eleven second).

Rust independently checks all 21 static locations, 24 admitted lifecycle statuses,
static/retained roots, exact supplied handles and array kinds/contents/counters.
Source metadata pins symbolic-child inheritance, interface/private method tags
and mixed-width descriptors. A real initializer on CrossStaticPlain deliberately
remains unapplied in the four host-body profiles; default-state and fresh-session
assertions verify that boundary. JDK constructors, reflection and class loaders
remain reference-driver operations; Rust uses explicit supplied objects.

All 132 previous class hashes remain unchanged. Parser smoke used seed
4851011501752864596: 110 accepted, 9,890 rejected, ten deterministic rechecks,
corpus 256 and zero failures. The unchanged parser's sustained M1 evidence remains
108,907,980 cases over 300 seconds, zero failures and an exact-source digest.
Finite tests are not proof of panic freedom. Local checks used Windows x86-64,
Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7. Rust 1.85 remains untested.

Hosted evidence for this increment is pending publication. The previous
initialization implementation 6dbbef4 passed both Linux and Windows in
[run 41839995587](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41839995587).
That earlier run does not validate the new static-call implementation.

## Review, limitations and next work

The exact contract is [m2-static-call-resolution.md](../architecture/m2-static-call-resolution.md).
GPT-6 Astra (high) owns interpretation and review; GPT-6.1 Sol (high) workers used
separate mutable worktrees/output directories. Root owns genuine fixtures,
integration and independent evidence. Hash-checked atomic integration preserved
Admin ownership. The user authorized autonomous review and publication.

Full loading constraints, named modules/nests, native/synchronized invocation,
String constants, constructed VM errors/wrappers, constructors/new, full verification,
concurrent initialization, monitors, Java libraries, threads, GC and application
startup remain incomplete. Kotlin/Spring Boot cannot run. No object-construction
or initialized-library claim follows from host-supplied storage.

The proposed next increment is new/constructor execution with an explicit
uninitialized-reference protocol, reviewed constructor chaining/final-field
authority and abrupt-exit alias rules. An exact contract must settle its verifier
boundary before production dispatch. Full M2 and the ultimate goal remain open.

## Reproduce

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p crabspot-runtime -p crabspot-interpreter --release
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
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH, set
CARGO_INCREMENTAL=0, and use separate CARGO_TARGET_DIR and TMP/TEMP directories.
Coordinator output is target/verification-m2; worker outputs are
target/static-call-runtime-worker and target/static-call-interpreter-worker.

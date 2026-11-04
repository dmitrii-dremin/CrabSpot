# Current checkpoint: single-session class initialization

Updated 2026-11-04. M1 and earlier M2/runtime increments are approved. The new
initialization increment passes its independent combined gate and frozen-source
review. [Bounded approval](../architecture/m2-initialization-session-review.md)
is recorded; full M2 remains open.
Continue autonomously without NanoSpec; ask only when user intervention is needed.

The ultimate goal remains complete Java SE 21 JVM conformance, required observable
compatibility with conforming JVMs, and demonstrated performance superiority over
HotSpot on a reviewed representative suite. Kotlin/Spring Boot are integration
milestones. No performance result or full JVM compatibility is claimed.

## Implemented scope

Five crates provide class-file parsing, instruction decoding, a bounded interpreter,
checked runtime services and a metadata inspection launcher. M1 covers all 30
standard class-file attributes. The interpreter retains earlier numeric, array,
instance-field, virtual/interface/special-call, cast, reference-transfer,
supplied-Throwable and explicit static-memory profiles. Static plus exception
execution supports 194 opcode values; initialization uses that existing instruction
surface. Earlier profiles and their eligibility rules remain unchanged.

The new InitializationSession exclusively borrows an admitted runtime context and
permanently claims its heap. It installs numeric ConstantValue entries, including
nonfinal statics and exact raw floating bits, then initializes the superclass and
qualifying default-method interfaces in prescribed order. Interface initialization
alone does not initialize its ancestors. Metadata planning traverses ancestors even
when a child interface has already completed. Actual clinit frames run in the same
iterative engine and share instruction, frame, slot, Code and handler limits.

Private lifecycle records distinguish in-progress, completed, erroneous and aborted
classes. Recursive use in the same evaluation observes partial state; successful
later requests do not rerun initialization. getstatic/putstatic and supported
same-class invokestatic retain their selected operations and operands while
initialization runs. They do not repeat fuel charges, resolution or stack pops.
Actual declaring clinit frames may write their own final statics; ordinary helpers
and other initializers may not. Root static methods are validated before their
owner is initialized and become active only afterward.

A supplied Error escaping a clinit retains its exact handle and fails dependent
initializations. A caller may catch it at the original active-use instruction.
A non-Error escape requires an unavailable wrapper and fails explicitly. Later
erroneous use likewise reports unsupported NoClassDefFoundError production.
Host errors and panics abort every remaining active initialization without erasing
completed classes, erroneous causes, installed constants or earlier effects.
Aborted classes cannot retry; unrelated classes remain usable. Dropping a session
does not reset its heap claim. This is the supported single-session procedure,
not concurrent initialization, complete linking, construction or Java startup.

## Independent validation

All 48 frozen implementation, fixture and configuration file hashes were rechecked
after the gate. Component manifests additionally cover worker READMEs.

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 838 passed, zero failed |
| Optimized runtime/interpreter tests | 751 passed: 388 runtime + 363 interpreter |
| Execution comparisons | All 26,170 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match |
| Fixture/reference reproduction | 132 classes and all 18 output sets reproduced |
| Canonical source provenance | 40 Java sources and 18 named reference records verified |
| Python provenance regressions | Six passed |
| Expanded parser smoke | 10,000 cases, zero failures |

The increment adds 70 Rust tests: 16 public and six private runtime tests,
29 primary and five private interpreter tests, 12 independent boundary tests,
and two genuine/raw reference integration tests. Coverage includes permanent claim
and failed-constructor ordering, exact quotas and overflow, scalar continuation
validation, atomic constant installation, ignored historical initializer flags,
unsupported parameterized initializers, actual final-write authority, all eleven
VM-exception routes, host panic cleanup, deferred store checks, caller push overflow,
retained Error ordering, and a 10,000-class iterative dependency chain with zero
Java frames and instruction fuel. Review tightened scalar token revalidation,
constant-range completeness and responsible-class diagnostics. Fallible reusable
one-entry token storage avoids inflating every frame by the full static token.

The new JDK oracle has 145 rows: 90 genuine scenarios across 18 methods and
55 original raw Z/B/C/S/I ConstantValue encodings. Results contain 141 numeric
returns and four uncaught exact supplied-Error identities. Each genuine row uses
a fresh JDK loader; repeatable successful roots reuse that identity. Rust uses a
fresh heap/session and independently checks all 38 static slots, 32 admitted class
statuses, two supplied Error handles, retained/static roots and allocation counts.
Raw probes use nonfinal constants and direct getstatic/ireturn, avoiding javac
folding or narrowing that could conceal storage errors. Constructors, reflection
and loaders run only in the reference driver. Rust does not fabricate Java errors.

All 104 prior class hashes remain unchanged. Expanded parser smoke used seed
4851011501752864596: 134 accepted, 9,866 rejected, ten deterministic rechecks,
corpus 256 and zero failures. The unchanged parser's sustained M1 evidence remains
108,907,980 cases over 300 seconds with zero failures and an exact-source digest.
Finite testing is not proof of panic freedom. Local checks used Windows x86-64,
Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7. Rust 1.85 remains untested.

Implementation `6dbbef4ad399f9d0f5938b3bea2485e08e45eff4` passed complete
[Linux/Windows run 41839995587](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41839995587).
Both job logs were inspected: Rust/Cargo 1.99.1, Temurin JDK 21.0.13, six Python
regressions, 40-source/18-record preflight, 132 reproduced fixtures and all 18
reference sets. Each platform passed 838 checked-in and 838 regenerated workspace
tests plus 751 optimized tests (2,427 Rust test executions), strict formatting/lint,
and parser smoke: 134 accepted, 9,866 rejected, ten rechecks, corpus 256 and zero
failures over 10,000 cases at seed 4851011501752864596.

## Review, limits and next work

The exact contract is [m2-initialization-session.md](../architecture/m2-initialization-session.md).
GPT-6 Astra (high) owns technical interpretation and review. GPT-6.1 Sol (high)
workers used separate mutable worktrees and output directories; the coordinator
owns genuine fixtures, integration and independent evidence. Atomic integration
verified file hashes and preserved the interactive Admin account's ownership.

String ConstantValue materialization, VM-created exceptions/wrappers, constructors,
new, general cross-class static invocation, full verification/linking/loading
constraints, concurrent initialization, monitors, native methods, Java libraries,
threads, GC and application startup remain incomplete. Kotlin/Spring Boot cannot
run. Host-supplied object storage is not a construction or initialized-library claim.

The proposed next coherent increment is checked cross-class invokestatic resolution
feeding the existing initialization continuation. It requires an exact reviewed
contract before implementation. The user authorized autonomous implementation,
review and publication; milestone acceptance remains bounded by actual evidence.

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
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH, set
CARGO_INCREMENTAL=0, and use separate CARGO_TARGET_DIR and TMP/TEMP directories.
Coordinator output is target/verification-m2; worker outputs are
target/init-runtime-worker and target/init-interpreter-worker.

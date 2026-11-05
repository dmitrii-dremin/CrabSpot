# CrabSpot

An independent JVM implementation in Rust. It parses Java 21 class files, decodes
JVM instructions, and executes supported primitive/reference method bodies independently in
Rust, including bounded virtual/interface/super calls across bodies, primitive/reference arrays,
instance fields, static access, single-session class initialization and construction, casts, type tests
and supplied-Throwable handlers. The long-term goal is a complete
JVM specification implementation with the required observable behavior of
conforming JVMs, using HotSpot as the primary reference, and demonstrated
performance superiority over HotSpot on representative workloads. Java SE 21 is
the first complete conformance target; later specification versions need explicit
support and validation. The directory launcher supports single-threaded startup
from a closed set of verified Java 21 application classes and platform inputs.
JAR class paths, general platform-library access and concurrent initialization
remain open.

Kotlin and Spring Boot are integration milestones toward that goal. Correctness
remains mandatory: performance claims require reproducible comparisons covering
startup, warmed-up throughput, latency and resource use, with regressions reported.

The project charter is [ARCHITECTURE.md](ARCHITECTURE.md). See the
[architecture](docs/architecture/overview.md),
[compatibility report](docs/compatibility/status.md), and
[current milestone](docs/milestones/current.md) for implementation boundaries,
evidence, and remaining work.

## Run a Java application

The launcher reads a platform directory and an application directory, verifies
the executable classes, then invokes a public static `main(String[])`. Pass
application arguments after `--`. Omitting them supplies an empty array.
A fresh run has its own loaders, heap, interned Strings and static state.

With the Java 21 tools on your path, create empty `target/app-platform` and
`target/app-classes` directories. Substitute your JDK's `java.base.jmod` path
in the first command.

```text
jmod extract --dir target/app-platform PATH_TO_JDK_21/jmods/java.base.jmod
javac --release 21 -d target/app-classes -sourcepath tests/fixtures/application-entry/src tests/fixtures/application-entry/src/app/Entry.java
cargo run --release -p crabspot-launcher --bin crabspot -- run --platform target/app-platform/classes --classes target/app-classes app.Entry
```

This example constructs two implementations of a Java interface, invokes both,
stores their combined result of 37 and exits successfully with empty stdout.
To inspect the resulting static fields, run the fixture observer through the same
public CrabSpot launcher API.

```text
cargo run --release -p crabspot-launcher --example replay_application_entry -- target/app-platform/classes target/app-classes app.Entry
```

The expected observation is `returned 1/1/37/1/0` for entries, completions,
result, helper initialization count and argument count. The launcher checks all
classes supplied in the application directory before running main. Its current
platform imports and runtime limits are listed in the [directory entry contract](docs/architecture/m2-verified-application-entry.md).

### Run synchronized Java code

Using the same extracted platform and a fresh `target/monitor-classes` directory, compile and launch the monitor example:

```text
javac --release 21 -d target/monitor-classes -sourcepath tests/fixtures/monitors/src tests/fixtures/monitors/src/locks/Entry.java
cargo run --release -p crabspot-launcher --bin crabspot -- run --platform target/app-platform/classes --classes target/monitor-classes locks.Entry
cargo run --release -p crabspot-launcher --example replay_monitor_entry -- target/app-platform/classes target/monitor-classes locks.Entry
```

The application synchronizes on its argument array, a primitive array and ordinary objects. It also invokes synchronized static and instance methods. The observer prints `returned 1/2/1/9` for entries, calls, completions and sum. This execution profile has one logical thread. [Monitor behavior and reference JVM results](docs/architecture/m2-monitor-reference.md).

### Run a program that uses class objects

With the same extracted platform, compile the class-object example into a fresh application directory and run its public launcher observer.

```text
javac --release 21 -d target/mirror-classes -sourcepath tests/fixtures/class-mirrors/src tests/fixtures/class-mirrors/src/mirrors/Entry.java
cargo run --release -p crabspot-launcher --example replay_class_mirror_entry -- target/app-platform/classes target/mirror-classes mirrors.Entry
```

The expected fields are `127/3/1`. The program compares class literals with the results of `getClass()` on objects, arrays and class objects. It also checks how many initializers ran and enters a static synchronized method while already holding the declaring class object's lock. The observer reports how many application and platform classes the launcher verified. [Class-object behavior and reference JVM results](docs/architecture/m2-class-mirror-reference.md).

### Run a program with String arguments

With the extracted platform above, compile the String-lock fixture into a fresh directory. The program interns its first two arguments and takes a nested lock on the resulting canonical String.

```text
javac --release 21 -d target/string-classes tests/fixtures/string-constants/src/locks/Strings.java
cargo run --release -p crabspot-launcher --bin crabspot -- run --platform target/app-platform/classes --classes target/string-classes locks.Strings -- crab crab shared
```

The program returns silently. Run `python tools/verify_string_entry.py` with a Java 21 JDK at `JAVA_HOME` to inspect the test results for literal identity, static constants, argument contents and resource limits. [String reference behavior](docs/architecture/m2-string-reference.md).

## Build and inspect

Install Rust stable with rustfmt and Clippy. Tests use checked-in Java 21 fixtures;
JDK 21 and Python 3 are needed only to regenerate those fixtures.

```text
cargo build --workspace
cargo run -p crabspot-launcher -- inspect tests/fixtures/classes/fixtures/Fixture.class
cargo run -p crabspot-interpreter --example primitive_array_body -- 17
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
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
python tools/code_validation_reference.py --check
python tools/type_state_reference.py --check
python tools/class_proof_reference.py --check
python tools/verify_java_base.py
cargo build -p crabspot-launcher
python tools/type_verification_facts.py --check
cargo test -p crabspot-verifier --release
cargo test -p crabspot-interpreter --test numeric_reference
cargo test -p crabspot-interpreter --test floating_reference --release
```

The standalone verifier can check a selected method or every declaration in one
class-file version 51–65 definition using caller-supplied loader and hierarchy facts. Reference
tests cover stack maps, instruction transfers, exception handlers and
constructors, including methods the interpreter cannot execute. See the
[method verification contract](docs/architecture/m2-type-state-verification.md).
The [class verification contract](docs/architecture/m2-class-type-verification.md)
specifies declaration order and cumulative limits. The optional
`tools/verify_java_base.py` check reads the installed JDK 21 module archive,
checks every ordinary bootstrap definition in Rust, and reports the archive
digest, definition count and number of verified declarations.

The checked-in JDK metadata capture records the exact reference build. To
reproduce the suite with a different JDK 21 build, regenerate the Java fixtures,
type-state reference and JDK fact projections with that build before using the
`--check` commands. Each projection records its JDK resource and original digest.

On Windows, ensure `%USERPROFILE%\.cargo\bin` is on PATH and the MSVC C++ build
tools are installed. Linux x86-64 remains the initial deployment target; CI is
configured for both Linux and Windows.

The array example independently executes the compiled `filledSum(17)` method in
Rust and returns `731`, using one managed array. It embeds an original test fixture
and uses explicit heap/fuel limits; it does not initialize classes or start Java
applications. A negative length reports a symbolic `NegativeArraySizeException`.

`crabspot inspect` reports class metadata. Malformed files, unreadable paths and
unsupported commands fail with a nonzero exit code. Java 21 preview class files
require `inspect --enable-preview`. Parsing does not perform bytecode verification,
linking, initialization or execution. Unknown extension attributes retain their
original payload bytes.

## Development

There are six crates: `crabspot-classfile` for pure byte parsing,
`crabspot-bytecode` for instruction decoding, `crabspot-interpreter` for the
method-body execution kernel, `crabspot-runtime` for bounded definition registration,
hierarchy/field-layout plans, shared values, static storage and a managed heap,
`crabspot-verifier` for standalone whole-method Code representation checks,
and `crabspot-launcher` for command-line inspection.

The validator checks every instruction, including unreachable code, constant-pool
operand categories, local ranges, handler boundaries and stack-map representation.
It returns a borrowed partial validation result with explicit resource accounting.
It does not establish stack/local type flow, linkage or execution permission, and
existing execution APIs do not acquire an implicit gate. Independent tests cover
all 890 Code bodies and 31 bodyless declarations in the genuine fixture corpus,
plus original encodings that distinguish static defects from deferred type errors.
See the [verifier contract](docs/architecture/m2-code-validation.md) and
[crate documentation](crates/verifier/README.md).

The differential tests compare 616 integral, 7,263 floating-point, 126 static-call,
280 primitive-array, 553 instance-field, 494 virtual-call, 1,337 interface/default,
450 ordinary special-call, 1,276 cast/type-test, 690 reference-field, 3,948
reference-array, 7,388 multidimensional-array, 904 reference-call, 336
supplied-exception, 364 static-memory, 145 initialization, 154 cross-class
static-call and 184 construction OpenJDK 21 outcomes with Rust execution,
including arithmetic exceptions, signed zero,
subnormals and NaN classification. Registered bodies support checked cross-class
static calls, ordinary virtual calls and interface/default dispatch. Standalone and
heap-only APIs retain their same-class static-call rules. An opt-in session initializes admitted classes through actual clinit bodies
and numeric constants. It does not load classes, perform general method resolution,
run native methods, or call Java. Static resolution preserves the actual declaring
MethodId, checks access/loading before static category, and does not inherit parent
interface statics. Only the initialization session initializes the resolved owner;
other body profiles retain host-controlled state. Code caches do not bypass
origin-sensitive resolution. Runtime registration preserves defining-loader
identity and immutable metadata, with resource limits and cached source reads;
registered definitions remain unverified and unlinked. Hierarchy planning checks
explicitly resolved ancestry and preserves declaring owners in field layouts,
with bounded nonrecursive traversal and no synthesized missing classes.
Static storage allocates typed zero/null defaults without applying constants or
running initializers. The opt-in checked static service owns one set of locations
per heap, preserves values across atomic storage extension, checks typed writes
and enumerates reference roots. Its body API adds getstatic/putstatic for 193
opcodes, or 194 when combined with supplied-Throwable dispatch. These explicit
memory APIs do not perform Java active-use initialization; final writes remain
rejected. The separate initialization session adds superclass/default-interface
ordering, recursive use, once-only completion and suspended active-use operations
without replaying fuel or popped operands. Actual initializer frames can write
their own final statics. Supplied Error identity survives dependency failure; host
interruption permanently aborts affected activations. The directory launcher's
String-enabled session installs static String constants. VM-created wrappers
and later NoClassDefFoundError production remain open. One session permanently
claims its heap.
A bounded noncollecting heap owns primitive arrays of all
eight kinds through checked opaque references. Heap-service tests additionally
match 151 OpenJDK array store/load outcomes. The opt-in instance service admits
shared, owner-qualified layouts atomically and allocates typed default fields.
Raw heap writes support numeric/null values with final-field protection and
explicit quotas;
151 raw-putfield JDK reference cases test its storage conversions. Constructor
execution belongs to the session profile described below. Checked context writes additionally
store compatible non-null instance and array references, preserving loader
identities and per-location child edges without implementing collection.
Typed reference arrays retain actual component identity, use checked covariant
stores, and share array quotas with primitive arrays. Reference payload accounting
uses the actual Rust Reference element size; it excludes container/allocator
overhead and is not total process memory. Multidimensional allocation preflights
complete reachable geometry, stages payloads iteratively, and publishes only after
all checks succeed. Partial ranks and zeros preserve JVM array defaults.
A runtime-aware instance-body API executes getfield/putfield, invokevirtual,
invokeinterface and ordinary nonconstructor invokespecial through bounded
loader-aware resolution and an explicit unnamed-module access profile.
It supports 191 opcode values, including checkcast/instanceof, reference-array
allocation/load/store, multianewarray and areturn; the earlier
explicit-heap/static API
retains 179 and the numeric-only APIs retain 148. Virtual selection handles
inherited declarations, private/final methods and package-aware overrides. Shared
interface analysis resolves defaults and abstract redeclarations, preserves
diamond identity and reports conflicting defaults during receiver selection.
Special calls select from the prescribed superclass or direct interface without
letting receiver overrides intercept the call. Reference operations use an explicit
Object/Cloneable/Serializable identity binding and bounded nominal/array assignment;
null skips target resolution after constant-pool shape checks. Each
frame uses its declaring class and shares execution limits with the whole call
graph. Reference arguments/results preserve opaque identities across all supported
call forms; compact prepared signatures avoid retaining copied descriptor names.
Non-null values are checked against the selected declarer's descriptor identity,
without cast-access checks. Null transfers still pay full signature-work charges.
Nominal-reference calls conservatively reject differing declaration loaders and
matching receiver-hierarchy declarations under another loader; general loading
constraints remain unimplemented.
A separate opt-in exception API adds athrow (192 effective opcodes), ordered
handlers and iterative unwind for already supplied, checked Throwable instances.
It preserves the thrown handle and searches callers at the invocation PC. Handler
metadata and dispatch work have explicit limits. All twelve unmaterialized
VM-produced categories fail explicitly in this profile, including null athrow;
no exception object is fabricated and no catch is silently bypassed. Earlier APIs
keep their symbolic outcomes and handler/athrow exclusions. Throwable construction,
initialized library state and general Java exception production remain unimplemented.
InitializationSession additionally executes new and actual constructor Code,
including an explicitly supplied bootstrap Object constructor. Private frame
values track unfinished aliases, lawful this/super chains, constructor-scoped
final writes and alias invalidation on failure. Escaped ordinary references remain
usable after an outer constructor throws. Separate ConstructionLimits bound frame
scans; failed/unused allocations remain in the noncollecting arena. This brings
the session surface to 195 opcodes. The bounded construction increment is approved after source review and a fresh
build gate; hosted validation is recorded in the current milestone.
Host-selected ordinary root bodies still require supplied instance storage and
retain their construction gates. Named-module access, cross-class nests and cross-loader reference loading
constraints remain explicit unsupported cases.
The implementation uses the
[JVMS Java SE 21](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html)
as its format reference. Reference JVM implementation code is not copied.

Follow [AGENTS.md](AGENTS.md). Every feature needs automated tests, and milestone
advancement requires architectural review. No NanoSpec workflow is used here.

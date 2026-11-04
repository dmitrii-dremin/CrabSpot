# CrabSpot

An independent JVM implementation in Rust. It parses Java 21 class files, decodes
JVM instructions, and executes supported primitive/reference method bodies independently in
Rust, including bounded virtual/interface/super calls across bodies, primitive/reference arrays,
instance fields, static access, single-session class initialization, casts, type tests
and supplied-Throwable handlers. The long-term goal is a complete
JVM specification implementation with the required observable behavior of
conforming JVMs, using HotSpot as the primary reference, and demonstrated
performance superiority over HotSpot on representative workloads. Java SE 21 is
the first complete conformance target; later specification versions need explicit
support and validation. Application startup, class loading and concurrent class
initialization are not implemented yet.

Kotlin and Spring Boot are integration milestones toward that goal. Correctness
remains mandatory: performance claims require reproducible comparisons covering
startup, warmed-up throughput, latency and resource use, with regressions reported.

The project charter is [ARCHITECTURE.md](ARCHITECTURE.md). See the
[architecture](docs/architecture/overview.md),
[compatibility report](docs/compatibility/status.md), and
[current milestone](docs/milestones/current.md) for implementation boundaries,
evidence, and remaining work.

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
cargo test -p crabspot-interpreter --test numeric_reference
cargo test -p crabspot-interpreter --test floating_reference --release
```

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

There are five crates: `crabspot-classfile` for pure byte parsing,
`crabspot-bytecode` for instruction decoding, `crabspot-interpreter` for the
method-body execution kernel, `crabspot-runtime` for bounded definition registration,
hierarchy/field-layout plans, shared values, static storage and a managed heap,
and `crabspot-launcher` for command-line inspection.
The differential tests compare 616 integral, 7,263 floating-point, 126 static-call,
280 primitive-array, 553 instance-field, 494 virtual-call, 1,337 interface/default,
450 ordinary special-call, 1,276 cast/type-test, 690 reference-field, 3,948
reference-array, 7,388 multidimensional-array, 904 reference-call, 336
supplied-exception, 364 static-memory, 145 initialization and 154 cross-class
static-call OpenJDK 21 outcomes with Rust execution,
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
interruption permanently aborts affected activations. String constants, VM-created
wrappers and later NoClassDefFoundError production remain explicit unsupported
cases. One session permanently claims its heap; this is not concurrent Java
initialization or application startup.
A bounded noncollecting heap owns primitive arrays of all
eight kinds through checked opaque references. Heap-service tests additionally
match 151 OpenJDK array store/load outcomes. The opt-in instance service admits
shared, owner-qualified layouts atomically and allocates typed default fields.
Raw heap writes support numeric/null values with final-field protection and
explicit quotas;
151 raw-putfield JDK reference cases test its storage conversions. Constructors
remain unimplemented. Checked context writes additionally
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
metadata and dispatch work have explicit limits. All eleven unmaterialized
VM-produced categories fail explicitly in this profile, including null athrow;
no exception object is fabricated and no catch is silently bypassed. Earlier APIs
keep their symbolic outcomes and handler/athrow exclusions. Throwable construction,
initialized library state and general Java exception production remain unimplemented.
Host-selected root bodies still require supplied instance storage; they
do not construct objects or start Java applications. Constructor invocation,
named-module access, cross-class nests and cross-loader reference loading
constraints remain explicit unsupported cases.
The implementation uses the
[JVMS Java SE 21](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html)
as its format reference. Reference JVM implementation code is not copied.

Follow [AGENTS.md](AGENTS.md). Every feature needs automated tests, and milestone
advancement requires architectural review. No NanoSpec workflow is used here.

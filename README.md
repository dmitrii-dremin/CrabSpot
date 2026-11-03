# CrabSpot

An independent JVM implementation in Rust. It parses Java 21 class files, decodes
JVM instructions, and executes supported numeric method bodies independently in
Rust, including bounded calls, primitive arrays and instance-field bodies. The long-term goal is a complete
JVM specification implementation with the required observable behavior of
conforming JVMs, using HotSpot as the primary reference, and demonstrated
performance superiority over HotSpot on representative workloads. Java SE 21 is
the first complete conformance target; later specification versions need explicit
support and validation. Application startup, class loading and initialization are
not implemented yet.

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
python tools/compile_fixtures.py --check
python tools/numeric_reference.py --check
python tools/floating_reference.py --check
python tools/call_reference.py --check
python tools/array_reference.py --check
python tools/field_reference.py --check
python tools/instance_reference.py --check
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
280 primitive-array and 553 instance-field OpenJDK 21 outcomes with Rust execution,
including arithmetic exceptions, signed zero,
subnormals and NaN classification. The kernel supports bounded same-class static
calls; it does not load or initialize classes, perform general method resolution,
run native methods, or call Java. Runtime registration preserves defining-loader
identity and immutable metadata, with resource limits and cached source reads;
registered definitions remain unverified and unlinked. Hierarchy planning checks
explicitly resolved ancestry and preserves declaring owners in field layouts,
with bounded nonrecursive traversal and no synthesized missing classes.
Static storage allocates typed zero/null defaults without applying constants or
running initializers. A bounded noncollecting heap owns primitive arrays of all
eight kinds through checked opaque references. Heap-service tests additionally
match 151 OpenJDK array store/load outcomes. The opt-in instance service admits
shared, owner-qualified layouts atomically and allocates typed default fields.
It supports numeric/null writes with final-field protection and explicit quotas;
151 raw-putfield JDK reference cases test its storage conversions. Constructors,
class initialization and non-null reference assignment remain unimplemented.
A runtime-aware instance-body API executes getfield/putfield through bounded
loader-aware resolution and an explicit unnamed-module access profile. It supports
181 opcode values; the earlier explicit-heap/static API retains 179 and the
numeric-only APIs retain 148. Host-selected bodies do not perform virtual dispatch
or Java invocation. Named-module access, cross-class nests and cross-loader
reference loading constraints remain explicit unsupported cases.
The implementation uses the
[JVMS Java SE 21](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html)
as its format reference. Reference JVM implementation code is not copied.

Follow [AGENTS.md](AGENTS.md). Every feature needs automated tests, and milestone
advancement requires architectural review. No NanoSpec workflow is used here.

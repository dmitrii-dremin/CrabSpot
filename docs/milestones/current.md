# Accepted checkpoint: reference types and cast instructions

Updated 2026-11-04. M1 and earlier M2/runtime increments through interface/default
dispatch are approved. Ordinary nonconstructor special calls are approved. Reference types and cast
instructions are approved after final integrated validation and frozen-source review;
see docs/architecture/m2-reference-types-review.md. Full M2
remains open. Continue autonomously without NanoSpec; ask only when user
intervention is unavoidable.

The long-term objective is complete JVM specification support, required observable
compatibility with conforming JVMs (HotSpot is the primary reference), and
demonstrated performance superiority on representative workloads. Java SE 21 is
the first complete conformance target. Kotlin/Spring Boot are integration
milestones, not the endpoint. No performance result is claimed.

## Implemented scope

- Five real crates: classfile, bytecode, interpreter, runtime and launcher.
- Java 21 class-file encodings and all 30 standard attributes, lossless UTF-16,
  checked reads and explicit parser limits; metadata inspection CLI.
- Full instruction-format decoder. Runtime-aware instance execution supports
  186 opcode values; the earlier heap/static mode retains 179 and numeric APIs 148.
- Numeric/void body graphs with shared instruction fuel, explicit iterative
  frames and frame/slot/prepared-code quotas. Registered frames and prepared
  cache entries use complete MethodIds, with each frame's own constant pool,
  fields, static helpers and diagnostic owner identity.
- Immutable definitions keyed by defining loader and exact UTF-16 name; bounded
  sources, explicit hierarchy resolution, parent-linked layouts, hidden fields,
  and typed default static storage without initialization.
- A noncollecting managed heap with primitive arrays and explicitly enabled
  ordinary instances, checked opaque references, atomic multi-plan admission,
  fallible allocation, numeric/null field stores and final-write protection.
- A reusable unnamed-module context with shared loader/name bindings, admitted
  graph, invalidation rules and cumulative symbol/unit/lookup budgets. Field
  resolution preserves interface-before-superclass lookup and access/null order.
- Class-method resolution and ordinary receiver dispatch: inherited declarations,
  private nonvirtual targets, package-aware transitive overrides, protected access,
  defensive final-override checks and explicit signature-polymorphic
  exclusions. Resolution/access/staticness precede null; abstract selection follows.
- invokevirtual/invokeinterface/ordinary invokespecial in the runtime-aware body
  API, mixed-width numeric arguments,
  numeric/void returns, static helpers relative to the current declaring class,
  recursion and persistent completed field/array effects after every exit.
  Symbolic Java outcomes remain distinct from host metadata/policy failures.

Shared interface analysis handles default inheritance, abstract redeclarations,
diamonds and unrelated default conflicts. Interface resolution and receiver
selection remain distinct; ordinary class implementations take precedence.
Concrete interface bodies and private helpers execute only after checked runtime
selection. Descriptor-aware invokeinterface count checks include receiver and
category-two slots. Direct interface roots remain unsupported.

Ordinary special calls separate symbolic resolution from the prescribed lookup
start. Class-super calls honor effective ACC_SUPER even for older/raw-bit-clear
files; direct-interface defaults and current-type private calls retain their
specified origins. Receiver identity must satisfy the current type, symbolic
owner and selected declarer. Existing virtual/interface budgets and behavior are
preserved. Constructors remain explicitly excluded.

Host-selected roots still require supplied storage. No constructor or class
initializer is run, and the root API is not proof of Java startup or construction.

The reference-type service binds an explicit Object/Cloneable/Serializable universe
to the admitted heap graph. It validates loader identities, preserves atomic
admission and invalidates contexts when binding changes. Nominal/interface and
structural array assignment is iterative and bounded; protected class-only access
remains separate. checkcast/instanceof use the current frame's class, preserve
reference identity, and skip target resolution for null after CP shape checks.
Only explicit Java failures become symbolic outcomes; malformed host input retains
structured errors. Reference-array types do not imply reference-array storage.

## Validation

Final independent native Windows checks passed on the verified frozen source:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 519 passed, zero failed |
| Optimized runtime/interpreter tests | 432 passed: 238 runtime + 194 interpreter |
| Execution comparisons | All 12,395 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match in both profiles |
| Fixture/reference reproduction | 74 classes and all eleven output sets reproduced |
| Expanded parser smoke | 10,000 cases, zero failures |

This increment adds 44 tests: 24 runtime public plus three private guards,
15 interpreter public plus one private error-map guard, and one independent
1,276-case JDK integration test. Coverage includes explicit universe identities,
atomic binding/admission, all primitive array kinds, rank-255 covariance,
loader-sensitive nominal and array leaves, UTF-16 aliases, deep iterative graphs,
null resolution skipping, CP/stack error order, per-frame origins after all call
kinds, exact work/fuel limits, and persistent effects after failure. All 65 prior
class hashes are unchanged. No executed test remains failing.

The complete execution corpus is 12,395 cases: 616 integral, 7,263 floating,
126 static-call, 280 primitive-array, 402 genuine instance-body, 151 raw-field,
494 virtual-call, 1,337 interface/default, 450 ordinary-special and 1,276 type cases.
Separate array and field memory services each have 151 oracle roundtrips, not
extra execution cases.

The new type oracle covers 36 receiver/method pairs, including 463 ClassCastException
and 242 NegativeArraySizeException outcomes. Source-derived field/allocation
assertions supplement JDK result rows. The Rust null-target wrappers deliberately
omit the referenced definition and universe binding. Original structural platform
metadata is explicit test input, not evidence of genuine Java-library execution.
Production execution never invokes Java.

The unchanged parser's sustained M1 evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds, zero failures, with exact-source fingerprint.
The expanded 74-class smoke run passed 10,000 cases at seed 4851011501752864596:
115 accepted, 9,885 rejected, 10 deterministic rechecks, corpus 189, zero failures.
No finite test proves panic freedom. Rust 1.85, the declared minimum, remains
untested; local tools are Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7.

The preceding special implementation 8d2c523 passed complete Linux/Windows
[run 41795065210](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41795065210)
with Rust 1.99.1 and Temurin JDK 21.0.13. Hosted CI for this increment is pending
publication; prior CI is not attributed to current changes.

## Reviews, limitations and next work

The exact contract is docs/architecture/m2-reference-types.md. Director review found no production or genuine-oracle blocker. Final frozen review
approved this increment in docs/architecture/m2-reference-types-review.md. Implementation uses separate runtime/interpreter worker worktrees; the
coordinator owns integration and genuine reference comparisons. The technical
director is GPT-6 Astra (high); workers are GPT-6.1 Sol (high).

Next direction is checked non-null instance-reference field stores, outlined in
docs/architecture/m2-reference-fields-next.md. A concrete director-approved contract
is required before its implementation; no repeated user approval is needed.

Constructor invocation, reference parameters/results and non-null stores,
reference-array storage, initialization, full linkage, verification, exception
objects/handlers, monitors, native methods, threads, GC and Java library integration
remain incomplete. Kotlin/Spring Boot is not yet executable. Full M2 and complete
JVM conformance remain open; no performance superiority has been demonstrated.

The user initialized Git and authorized autonomous publication to the CrabSpot
remote. Preserve interactive-user Git ownership and the Apache 2.0 license.

## Reproduce

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --quiet
cargo test -p crabspot-runtime -p crabspot-interpreter --release --quiet
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
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
types-runtime-worker and types-interpreter-worker output directories.

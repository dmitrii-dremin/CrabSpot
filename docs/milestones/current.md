# Accepted checkpoint: interface/default dispatch

Updated 2026-11-04. M1 and the earlier M2/runtime increments, through ordinary
virtual calls, are approved. Interface/default dispatch is approved after final
integrated validation and technical-director review; see
docs/architecture/m2-interface-dispatch-review.md. Full M2 remains open. Continue
autonomously without NanoSpec; ask only when user intervention is unavoidable.

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
  183 opcode values; the earlier heap/static mode retains 179 and numeric APIs 148.
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
- invokevirtual/invokeinterface in the runtime-aware body API, mixed-width numeric arguments,
  numeric/void returns, static helpers relative to the current declaring class,
  recursion and persistent completed field/array effects after every exit.
  Symbolic Java outcomes remain distinct from host metadata/policy failures.

Shared interface analysis handles default inheritance, abstract redeclarations,
diamonds and unrelated default conflicts. Interface resolution and receiver
selection remain distinct; ordinary class implementations take precedence.
Concrete interface bodies and private helpers execute only after checked runtime
selection. Descriptor-aware invokeinterface count checks include receiver and
category-two slots. Direct interface roots remain unsupported.

Host-selected roots still require supplied storage. No constructor or class
initializer is run, and the root API is not proof of Java startup or construction.

## Validation

Final frozen-source checks passed on native Windows:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 431 passed, zero failed |
| Optimized runtime/interpreter tests | 344 passed: 188 runtime + 156 interpreter |
| Execution comparisons | All 10,669 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match in both profiles |
| Fixture/reference reproduction | 59 classes and all nine reference sets reproduced |
| Expanded parser smoke | 10,000 cases, zero failures |

This increment adds 44 tests: 23 runtime public and one private, 18 interpreter
public and one private, plus the independent 1,337-case JDK integration test.
Existing forged-origin checks now include interface selection; prior interface
unsupported assertions retain their fixtures and check the newly supported
results. There are no remaining executed test failures. Deep tests use 10,000
interface nodes and 10,000 explicit frames; count 255 tests cover 254 Int and 127
Long arguments, and count 256 is rejected before extraction or resolution.

The new JDK interface/default comparisons span 112 receiver/method pairs.
The complete execution corpus is now 10,669 cases: 616 integral, 7,263 floating,
126 static-call, 280 primitive-array, 402 genuine instance-body, 151 raw-field,
494 virtual-call and 1,337 interface/default cases. The separate array and field
memory services each have 151 oracle roundtrips, not extra execution cases.

The new reference records contain 98 null, seven arithmetic and fourteen
negative-array-size outcomes. Additional state/allocation assertions are derived
from the original fixture source, separately from JDK result rows. All 59 class
fixtures and nine reference output sets reproduce with Zulu OpenJDK 21.0.7; all
43 preexisting class-file hashes are unchanged. Rust tests read checked-in data,
and production execution never invokes Java.

The parser's unchanged sustained M1 evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds, zero failures. A new 59-class smoke run passed
10,000 cases at seed 4851011501752864596: 128 accepted, 9,872 rejected,
10 deterministic rechecks, corpus 187 and zero failures. No finite test proves
panic freedom. The 70 classfile tests pass against the expanded corpus.

The local toolchain is Rust/Cargo 1.98.1. Rust 1.85, the declared minimum, remains
untested. The accepted virtual-call implementation d6558d8 passed complete
Linux/Windows [run 41730095972](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41730095972)
with Rust 1.99.1 and Temurin JDK 21.0.13. Hosted validation for this interface
increment remains pending publication.

## Reviews, limitations and next work

The exact interface contract is docs/architecture/m2-interface-dispatch.md.
Preliminary production and fixture review found a resource-accounting omission:
self-pair identity comparisons in candidate analysis were performed before
charging LookupSteps. The charge now precedes every comparison, including self
pairs. Dispatch results were unaffected. Exact-limit regressions cover that
boundary, dominance traversal and charging edges before growing pending work.
The frozen source and tests are approved, with no unresolved finding; see
docs/architecture/m2-interface-dispatch-review.md.

Next direction: ordinary nonconstructor invokespecial, described in
docs/architecture/m2-special-calls-next.md. It requires a concrete reviewed contract before implementation. Autonomous continuation is
already authorized; no additional user permission is required.

Special invocation, reference parameters/results and non-null stores,
reference-array/assignability services, constructors, initialization, full linkage,
verification, exception objects/handlers, monitors, native methods, threads, GC
and Java library integration remain incomplete. Kotlin/Spring Boot is not yet
executable. Synthetic Object roots are explicit metadata test inputs only.

The technical director is GPT-6 Astra (high); runtime/interpreter workers are
GPT-6.1 Sol (high), using isolated worktrees under target/agent-worktrees. The
user initialized Git and authorized publication to the CrabSpot remote. The
repository preserves its Apache 2.0 license and interactive-user Git ownership.

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
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
interface-runtime-worker and interface-interpreter-worker output directories.

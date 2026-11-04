# Accepted checkpoint: ordinary special calls

Updated 2026-11-04. M1 and earlier M2/runtime increments through interface/default
dispatch are approved. Ordinary nonconstructor special calls are approved after
final integrated validation and frozen-source review; see
docs/architecture/m2-special-calls-review.md. Full M2
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
  184 opcode values; the earlier heap/static mode retains 179 and numeric APIs 148.
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

## Validation

Final checks ran on the verified frozen source after the integration recovery
noted below, using native Windows Rust/Cargo 1.98.1:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 475 passed, zero failed |
| Optimized runtime/interpreter tests | 388 passed: 211 runtime + 177 interpreter |
| Execution comparisons | All 11,119 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match in both profiles |
| Fixture/reference reproduction | 65 classes and all ten reference sets reproduced |
| Expanded parser smoke | 10,000 cases, zero failures |

This increment adds 44 tests: 23 runtime public, 20 interpreter synthetic and one
independent 450-case JDK integration test. Existing private forged-handle/helper
and error-source guards were extended. The previous runtime-aware b7 rejection
fixture now asserts its malformed Methodref boundary. Deep checks use 10,000
classes and 10,000 explicit frames. Old/current major versions, ACC_SUPER absence,
owner/receiver restrictions, shared caches, exact limits and persistent effects
are covered. No executed test remains failing.

The complete execution corpus is 11,119 cases: 616 integral, 7,263 floating,
126 static-call, 280 primitive-array, 402 genuine instance-body, 151 raw-field,
494 virtual-call, 1,337 interface/default and 450 ordinary-special cases. The
separate array and field memory services each have 151 oracle roundtrips, not
extra execution cases.

New special results cover 36 receiver/method pairs, including two arithmetic
and four negative-array-size outcomes. Supplemental hidden-field/raw-value and
allocation assertions derive from fixture source, separately from JDK result
rows. Null/invalid owners and historical encodings use synthetic cases. All 65
fixtures and ten output sets reproduce with Zulu OpenJDK 21.0.7; all 59 preceding
class-file hashes are unchanged. Production execution never invokes Java.

The unchanged parser's sustained M1 evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds, zero failures. A new 65-class smoke run passed
10,000 cases at seed 4851011501752864596: 117 accepted, 9,883 rejected,
10 deterministic rechecks, corpus 182 and zero failures. The 70 classfile tests
also pass against the expanded corpus. No finite test proves panic freedom.
Rust 1.85, the declared minimum, remains untested.

The accepted interface implementation 3574484 passed complete Linux/Windows
[run 41792109658](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41792109658)
with Rust 1.99.1 and Temurin JDK 21.0.13. Special implementation 8d2c523 passed complete [run 41795065210](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41795065210)
on Linux and Windows with Rust 1.99.1 and Temurin JDK 21.0.13. Checked-in and
regenerated fixture tests, all ten reference sets, strict lint, optimized tests
and parser smoke passed. Both hosted smoke runs completed 10,000 cases with
117 accepted, 9,883 rejected, 10 rechecks, corpus 182 and zero failures. These
results belong to the implementation commit, separately from this evidence update.

## Reviews, limitations and next work

The exact contract is docs/architecture/m2-special-calls.md. Review corrected a
missing explicit charge for the interface-origin Object dependency edge during
special-owner checking. The edge is now charged before comparison, with an
inclusive regression. Selection results and existing b6/b9 budgets are unchanged.
The transient original JDK probe for interface-origin Object.finalize()V produced
IllegalAccessError; public Object.hashCode()I executed successfully. Its arbitrary
hash value is not compared and those probes are not added to the execution count.

One Windows copy was refused because execute.rs had a mapped section open. After
inspecting content and visible processes, a same-directory atomic replacement
restored the exact frozen bytes and preserved Admin ownership, without ACL or
process changes. A null-backup argument error in the replacement call was fixed
by supplying an explicit ignored backup path. Final checks were rerun after
hash verification; no infrastructure issue remains.

Final review disposition is recorded in docs/architecture/m2-special-calls-review.md.
Next direction is a shared reference-type relation with checkcast/instanceof,
outlined in docs/architecture/m2-reference-types-next.md. A concrete reviewed
contract must settle primitive-array/core-type identities before implementation.
No additional user permission is required for that architectural work.

Constructor invocation, reference parameters/results and non-null stores,
reference-array/assignability services, initialization, full linkage, verification,
exception objects/handlers, monitors, native methods, threads, GC and Java library
integration remain incomplete. Kotlin/Spring Boot is not yet executable.
Synthetic Object roots are explicit metadata test inputs only.

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
python tools/special_reference.py --check
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
special-runtime-worker and special-interpreter-worker output directories.

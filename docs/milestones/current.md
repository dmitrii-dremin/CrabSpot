# Accepted checkpoint: ordinary virtual calls

Updated 2026-11-03. M1 and the previously recorded M2/runtime increments, through
instance-field execution, are approved. Ordinary virtual calls are approved after
final integrated validation and technical-director review; see
docs/architecture/m2-virtual-calls-review.md.
Full M2 remains open. Continue autonomously without NanoSpec; ask the user only
when intervention is unavoidable.

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
  182 opcode values; the earlier heap/static mode retains 179 and numeric APIs 148.
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
  defensive final-override checks and explicit interface/signature-polymorphic
  exclusions. Resolution/access/staticness precede null; abstract selection follows.
- invokevirtual in the runtime-aware body API, mixed-width numeric arguments,
  numeric/void returns, static helpers relative to the current declaring class,
  recursion and persistent completed field/array effects after every exit.
  Symbolic Java outcomes remain distinct from host metadata/policy failures.

Host-selected roots still require supplied storage. No constructor or class
initializer is run, and the root API is not proof of Java startup or construction.

## Validation

Final frozen-code checks passed on native Windows:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 387 passed, zero failed |
| Optimized runtime/interpreter tests | 300 passed: 164 runtime + 136 interpreter |
| Execution differential outcomes | All 9,332 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 raw-field cases match in both profiles |
| Java 21 fixtures | All 43 reproduced |
| Reference output reproduction | All eight sets reproduced |
| Parser smoke | 10,000 cases, zero failures |

This increment adds 46 tests: 24 runtime public and three private, 17 interpreter
synthetic and one private, plus the independent 494-case integration test. Deep
resolution and call tests exercise 10,000 classes and 10,000 explicit recursive
frames. An obsolete unsupported-b6 assertion in an older field test was changed
to assert the appropriate malformed-Methodref boundary; its coverage was retained.
No executed test remains failing.

All 43 Java 21 class fixtures and eight reference output sets reproduce locally.
The new oracle covers 40 receiver/method pairs with 28 null, two arithmetic and
four negative-array-size outcomes. It includes cross-package non-overrides and
transitive overrides, cross-class fields/helpers, private/final/inherited calls,
recursion, arrays and all numeric argument/result kinds. Persistent-field checks
in the independent integration test are source-derived assertions, separate from
JDK result rows.

The complete execution corpus is 9,332 comparisons: 616 integral, 7,263 floating,
126 static-call, 280 primitive-array, 402 genuine instance-body, 151 raw-field
execution and 494 virtual-call cases. The separate array and raw-field memory
services each have 151 oracle roundtrips; these are not additional execution cases.

The accepted field-bridge baseline passed 341 workspace tests and 254 optimized
runtime/interpreter tests. Its implementation commit d6ef169 passed the complete
Linux/Windows [run 41726268353](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41726268353).
The subsequent documentation commit 29c5caa also passed
[run 41726644013](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41726644013).
Virtual calls are published as d6558d8. The complete [run 41730095972](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41730095972)
passed on Linux and Windows with Rust 1.99.1 and Temurin JDK 21.0.13: checked-in
and regenerated fixture tests, all eight reference sets, formatting, strict
Clippy, optimized runtime/interpreter tests and parser smoke. Each hosted parser
smoke completed 10,000 cases with zero failures; the regenerated corpus yielded
136 accepted, 9,864 rejected, 10 rechecks and corpus 179. This is evidence for the
implementation commit, recorded separately from this documentation update.

The local toolchains are Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7. Ordinary Rust
tests consume checked-in fixtures/results; production execution never invokes
Java. All 36 preexisting class-file hashes were preserved. Rust 1.85, the manifest
minimum, remains untested.

The unchanged parser's sustained M1 evidence remains in docs/milestones/evidence:
108,907,980 seeded cases over 300 seconds, zero failures. A fresh 43-class smoke
run passed 10,000 cases (seed 4851011501752864596): 128 accepted, 9,872 rejected,
10 deterministic rechecks, corpus 171, zero failures. No finite test proves panic
freedom.

## Reviews, limitations and next work

The accepted contract is docs/architecture/m2-virtual-calls.md, with final approval
in docs/architecture/m2-virtual-calls-review.md. No blocking finding remains.
Review preserved existing transition-site error provenance and added cross-class
regression coverage; no production correction was required after the compiling
snapshots. Prior acceptance records remain in docs/architecture.

Next direction: shared maximally-specific interface analysis and invokeinterface,
outlined in docs/architecture/m2-interface-dispatch-next.md. A concrete reviewed
contract is required before implementation.

Interface/special invocation, reference parameters/results, non-null reference
stores, reference arrays/assignability, constructors, initialization, full linkage,
verification, exceptions/handlers, monitors, native methods, threads, GC and Java
library integration remain incomplete. Kotlin/Spring Boot is not executable.
Synthetic Object roots are explicit metadata test inputs only.

The technical director is GPT-6 Astra (high); bounded runtime/interpreter workers
are GPT-6.1 Sol (high), using isolated detached worktrees under target/agent-worktrees.
The user initialized Git and authorized publication to
https://github.com/dmitrii-dremin/CrabSpot.git. The repository preserves its Apache
2.0 license and interactive Windows-user ownership of Git metadata.

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
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
call-runtime-worker and call-interpreter-worker output directories.

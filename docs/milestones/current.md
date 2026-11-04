# Accepted checkpoint: bounded multidimensional allocation

Updated 2026-11-04. M1 and earlier M2/runtime increments through reference arrays
are approved. Bounded multianewarray is approved after integrated validation and
frozen-source review; see docs/architecture/m2-multidimensional-arrays-review.md.
Full M2 remains open. Continue autonomously without NanoSpec; ask only when user
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
  190 opcode values; the earlier heap/static mode retains 179 and numeric APIs 148.
- Numeric/void body graphs with shared instruction fuel, explicit iterative
  frames and frame/slot/prepared-code quotas. Registered frames and prepared
  cache entries use complete MethodIds, with each frame's own constant pool,
  fields, static helpers and diagnostic owner identity.
- Immutable definitions keyed by defining loader and exact UTF-16 name; bounded
  sources, explicit hierarchy resolution, parent-linked layouts, hidden fields,
  and typed default static storage without initialization.
- A noncollecting managed heap with primitive/reference arrays and explicitly enabled
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
structured errors. Actual reference arrays now retain these same structural identities.

Checked context writes now accept assignment-compatible non-null ordinary-instance
and array references. They preserve field/access/static/final/receiver
ordering, resolve descriptor identity through the declaring loader, and commit
one slot only after all checks pass. Public raw writes remain numeric/null only.
The descriptor type need not be accessible to the caller; cast accessibility
checks remain separate. Incompatible stores are structured host errors, and child
enumeration preserves immediate locations, repeated aliases and cycles without
tracing or collection.

Typed reference arrays initialize elements to null and use the shared array count,
length, element and payload quotas. Payload accounting uses size_of::<Reference>()
per stored element and excludes container/allocator overhead. Checked covariant
stores use retained actual component identity; destination null and bounds checks
precede value validation and ArrayStoreException. Reads preserve handles, generic
arraylength accepts both array payloads, and immediate child enumeration preserves
index order, aliases and cycles without recursion or collection. The old primitive
heap APIs remain primitive-only and expose no raw reference-array mutation.

Multidimensional allocation validates the complete type and every requested count
before reachable-level quota preflight. It checks aggregate geometry, precharges
node/link work atomically, stages payloads breadth-first, validates future links
and publishes all entries/counters together. Negative counts after zero still
fail; positive unreachable sizes do not allocate or consume deeper quotas.
Complete and partial ranks preserve actual node types and primitive/null defaults.
Host failures preserve the old heap; no Java OutOfMemoryError or GC claim follows.

## Validation

Final independent checks passed on the verified frozen source:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 631 passed, zero failed |
| Optimized runtime/interpreter tests | 544 passed: 294 runtime + 250 interpreter |
| Execution comparisons | All 24,421 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match in both profiles |
| Fixture/reference reproduction | 88 classes and all fourteen output sets reproduced |
| Expanded parser smoke | 10,000 cases, zero failures |

This increment adds 36 tests: 14 public runtime, five private guards, 16 public
interpreter and one independent JDK integration. Coverage includes rank 255,
complete/partial shapes, all eight primitive leaves, abstract/interface leaves,
negative counts after zero, huge unreachable positive sizes, mixed prior graphs,
level-ordered quota conflicts, exact atomic bulk work, current-frame resolution,
stack/fuel/frame/code limits and persistent effects. The private corruption matrix
has 25 cases; staged-payload and post-arena-reserve injected failures preserve
nonempty old child edges. All 86 prior class hashes are unchanged. No executed
test remains failing.

The complete execution corpus is 24,421 cases: 616 integral, 7,263 floating,
126 static-call, 280 primitive-array, 402 genuine instance-body, 151 raw-field,
494 virtual-call, 1,337 interface/default, 450 ordinary-special, 1,276 type,
690 reference-field, 3,948 reference-array and 7,388 multidimensional cases.
Separate array and field memory services each have 151 oracle roundtrips,
not extra execution cases.

The new oracle covers 20 receiver/method pairs: 2,343 successful results and 5,045
Java failures (3,364 NegativeArraySizeException, 1,666 ArrayIndexOutOfBoundsException
and 15 ArrayStoreException). JDK rows record independent results/exceptions.
A source-derived depth-first model, bounded to four fixture levels, checks every
reachable array identity/type/value/edge, retained field and public counter.
Production stages breadth-first and never recursively owns child arrays. Decoded
genuine Code confirms c5 rank/dimension pairs (2,2), (3,2), (3,3), (4,3), (4,4).
The reference generator caught duplicate inputs from overlapping grids; removing
duplicate rows retained every unique case. The Java driver remains outside CrabSpot
execution, and host default receivers do not establish constructor execution.

The unchanged parser's sustained M1 evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds, zero failures, with exact-source fingerprint.
The expanded 88-class smoke run passed 10,000 cases at seed 4851011501752864596:
130 accepted, 9,870 rejected, ten deterministic rechecks, corpus 218, zero failures.
No finite test proves panic freedom. Rust 1.85, the declared minimum, remains
untested; local tools are Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7.

The preceding reference-array implementation c0ce0a3 passed complete Linux/Windows
[run 41804561054](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41804561054)
with Rust 1.99.1 and Temurin JDK 21.0.13. Hosted CI for this increment is pending
publication; prior CI is not attributed to current changes.

## Reviews, limitations and next work

The exact contract is docs/architecture/m2-multidimensional-arrays.md. Final director
review found no blocking semantic defect and approved the frozen increment in
docs/architecture/m2-multidimensional-arrays-review.md. Workers use isolated
runtime/interpreter worktrees; the coordinator owns genuine fixtures, reference
comparisons and integration. The technical director is GPT-6 Astra (high);
workers are GPT-6.1 Sol (high).

Next direction is reference call arguments/results, outlined in
m2-reference-calls-next.md under docs/architecture. An exact reviewed contract must
settle descriptor identity, loader restrictions, host-root validation and argument/
return budgets before dispatch. No repeated user approval is needed.

Constructor invocation, reference parameters/results, initialization, full linkage,
verification, loading constraints, exception objects/handlers, monitors, native
methods, threads, GC and Java library integration remain incomplete. Kotlin/Spring
Boot is not yet executable. Full M2 and complete JVM conformance remain open;
no performance superiority has been demonstrated.

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
python tools/reference_field_reference.py --check
python tools/reference_array_reference.py --check
python tools/multidimensional_reference.py --check
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
multidimensional-runtime-worker and multidimensional-interpreter-worker output directories.

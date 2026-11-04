# Accepted checkpoint: supplied-Throwable exception dispatch

Updated 2026-11-04. M1 and earlier M2/runtime increments are approved. The opt-in
supplied-Throwable dispatch increment is approved after integrated validation and
frozen-source review; see docs/architecture/m2-exception-dispatch-review.md.
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
  191 opcode values; opt-in explicit exception dispatch adds athrow for 192.
  The earlier heap/static mode retains 179 and numeric APIs 148.
- Runtime-aware primitive/reference body graphs with shared instruction fuel, explicit iterative
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
  primitive/reference/void returns, static helpers relative to the current declaring class,
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

Reference arguments/results now cross all supported call forms and host body
boundaries. Prepared bodies retain compact computational kinds and return tags;
full descriptors are borrowed and charged before inspection. Non-null values use
the selected declarer's identity-only descriptor resolution and bounded assignment.
Null skips target resolution while preserving full signature charges. areturn checks
the result before removing the callee frame and preserves its exact handle.
Nominal-reference signatures conservatively reject cross-loader resolution and
matching foreign-loader declarations throughout the receiver hierarchy, including
otherwise nonselected private/static declarations. This is an explicit unsupported
boundary, not full loading-constraint enforcement. Numeric-only budgets are retained.

The opt-in exception profile binds an explicit admitted bootstrap Throwable
identity and validates supplied ordinary-instance references. Opaque profile/token
fingerprints reject replaced heaps, universes and layout snapshots. Typed catches
resolve lazily through the searched method's declaring origin with caller access;
catch-all avoids resolution. Prepared rows borrow original Code tables by index,
with eager boundary/shape checks and an aggregate handler limit. Iterative dispatch
charges every frame probe, row and unwind, searches callers at their invocation
PC, replaces only the handler operand stack and preserves locals/heap effects.

The new API returns normal values or the exact uncaught reference. All eleven
existing VM-produced symbolic categories become explicit UnsupportedVmException
host failures in this profile, including null athrow and catch-access failures.
No object is fabricated or matching handler silently skipped. Existing APIs retain
their symbolic outcomes and handler/athrow rejection. Dispatch does not certify
Throwable constructors, initialized library fields, messages or stack traces.

## Validation

Final independent checks passed on the verified frozen source:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 724 passed, zero failed |
| Optimized runtime/interpreter tests | 637 passed: 341 runtime + 296 interpreter |
| Execution comparisons | All 25,661 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match in both profiles |
| Fixture/reference reproduction | 98 classes and all sixteen output sets reproduced |
| Canonical source provenance | 38 Java sources and sixteen named reference records verified |
| Python provenance regressions | Six passed |
| Expanded parser smoke | 10,000 cases, zero failures |

This increment adds 44 Rust tests: eighteen public and three private runtime cases,
eighteen public and four private interpreter cases, and the genuine execution/state
oracle. They cover explicit bootstrap/Throwable identity, foreign/replaced snapshots,
forged tokens/handles, actual cross-loader subclasses, lazy typed/catch-all matching,
caller access, exact work/name limits, eager structural boundaries, handler caching,
all eleven unsupported production paths, stack preservation on failed installation,
iterative unwind accounting/origins, legacy modes and retained effects.

The execution corpus comprises 25,325 earlier cases plus 336 new supplied-exception
cases across thirty-six type/method pairs. The JDK records 240 numeric completions
and 96 uncaught identities: nineteen first-object and seventy-seven second-object
outcomes. Separate source-derived checks inspect saved/first/second references,
state, repeated child edges and unchanged allocation totals. Decoded genuine Code
pins typed catch order, catch-all rows, athrow and all four invocation forms. A
protected call ending exactly at the resume PC proves invocation-PC unwind search.
Host objects are preseeded/default values with original core metadata; only the JDK
oracle driver invokes constructors. Implicit VM exceptions are outside this corpus.

All 92 previous class hashes remain unchanged. Canonical LF source guards and the
pre-regeneration source/JSON/TSV provenance checks remain active and pass.
The unchanged parser's sustained M1 evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds, zero failures, with exact-source fingerprint.
The expanded 98-class smoke passed 10,000 cases at seed 4851011501752864596:
131 accepted, 9,869 rejected, ten deterministic rechecks, corpus 229, zero failures.
No finite test proves panic freedom. Rust 1.85, the declared minimum, remains
untested; local tools are Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7.

Implementation `215ec91ee10cd9b20e9860a33376193b82c89198` passed complete
[Linux/Windows run 41824069552](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41824069552).
Both job logs were inspected: Rust/Cargo 1.99.1, Temurin JDK 21.0.13, six Python
regressions, 38-source/sixteen-record preflight, 98 reproduced classes and all
sixteen reference sets. Each platform passed 724 checked-in and 724 regenerated
workspace tests plus 637 optimized tests (2,085 Rust test executions), strict
formatting/lint, and the same 10,000-case parser smoke with zero failures.

## Reviews, limitations and next work

The exact contract and final approval are docs/architecture/m2-exception-dispatch.md
and docs/architecture/m2-exception-dispatch-review.md. Component manifests and all
sixteen combined source/evidence hashes were verified. The technical director is
GPT-6 Astra (high); isolated runtime/interpreter workers are GPT-6.1 Sol (high).
The coordinator owns genuine fixtures, integration and evidence. Integration uses
atomic replacements with explicit backups and verifies Admin ownership.

Next direction is class preparation/static state and initialization responsibilities,
under a separate exact contract. These shared lifecycle requirements precede real
VM exception materialization and ordinary new/constructor execution. Default storage
must not be described as initialized class or object state.

Constructor invocation, initialization, static writes, full linkage, verification,
loading constraints, VM exception production, monitors, native methods, threads, GC
and Java library integration remain incomplete. Kotlin/Spring Boot is not yet
executable. Full M2 and complete JVM conformance remain open; no performance
superiority has been demonstrated.

The user initialized Git and authorized autonomous publication to the CrabSpot
remote. Preserve interactive-user Git ownership and the Apache 2.0 license.

## Reproduce

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --quiet
cargo test -p crabspot-runtime -p crabspot-interpreter --release --quiet
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
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
exception-runtime-worker and exception-interpreter-worker output directories.

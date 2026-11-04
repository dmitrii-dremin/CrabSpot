# Accepted checkpoint: reference arguments and results

Updated 2026-11-04. M1 and earlier M2/runtime increments are approved. Bounded
reference arguments/results and areturn are approved after integrated validation
and frozen-source review; see docs/architecture/m2-reference-calls-review.md.
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
  191 opcode values; the earlier heap/static mode retains 179 and numeric APIs 148.
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

## Validation

Final independent checks passed on the verified frozen source:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 680 passed, zero failed |
| Optimized runtime/interpreter tests | 593 passed: 320 runtime + 273 interpreter |
| Execution comparisons | All 25,325 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match in both profiles |
| Fixture/reference reproduction | 92 classes and all fifteen output sets reproduced |
| Canonical source provenance | 37 Java sources and fifteen named reference records verified |
| Python provenance regressions | Six passed |
| Expanded parser smoke | 10,000 cases, zero failures |

This increment adds 49 Rust tests: 22 public and four private runtime cases,
21 public and one private interpreter case, and the genuine execution/state oracle.
They cover call-site/resolved/selected identities; intermediate foreign-loader,
interface/private/static declarations; source-first validation; all-null/unbound
transfers; inaccessible descriptor types; exact budgets and long shared signatures;
deep iterative audits and 10,001-frame reference recursion; failure provenance and
persistent earlier effects. Old reference exclusions now assert the newly supported
behavior or retained computational/owner/initialization guards. Invalidated contexts
are deliberately detected earlier by runtime shape preparation as MethodAccess.

The execution corpus comprises 24,421 earlier cases plus 904 new reference-call
cases across twenty receiver/method pairs. The new JDK results include 872 successes,
twelve NullPointerException, sixteen bounds and four ArrayStoreException outcomes.
Separate source-derived checks inspect retained state, aliases/cycles, every array
value/type/edge and public counters. Exact helper descriptors and decoded selected
Code pin reference signatures, areturn and all four invocation forms. Host receivers
use default storage; only the JDK oracle driver invokes constructors.

A concrete provenance defect was repaired: some historical working-copy CRLF source
hashes differed from Git's required LF blobs, while CI regeneration concealed that
mismatch. Sources now use canonical LF. Both generators reject CR bytes before JDK
calls or metadata writes and still hash exact accepted bytes. Reference records name
their source; CI checks manifest/reference source hashes and matching JSON/TSV sets
before regeneration. All 92 compiled class files and fifteen TSV outputs remained
byte-identical during repair; all 88 previous class hashes remain unchanged.

The unchanged parser's sustained M1 evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds, zero failures, with exact-source fingerprint.
The expanded 92-class smoke passed 10,000 cases at seed 4851011501752864596:
158 accepted, 9,842 rejected, ten deterministic rechecks, corpus 250, zero failures.
No finite test proves panic freedom. Rust 1.85, the declared minimum, remains
untested; local tools are Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7.

The preceding multidimensional implementation a753941 passed complete Linux/Windows
[run 41813463546](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41813463546)
with Rust/Cargo 1.99.1 and Temurin JDK 21.0.13. Hosted CI for this reference-call
increment is pending publication; prior CI is not attributed to the new source.

## Reviews, limitations and next work

The exact contract and final approval are docs/architecture/m2-reference-calls.md
and docs/architecture/m2-reference-calls-review.md. Both component manifests and
all 29 combined frozen source/evidence hashes were independently verified. The
technical director is GPT-6 Astra (high); isolated runtime/interpreter workers are
GPT-6.1 Sol (high). The coordinator owns genuine fixtures, integration and evidence.
A mapped-file copy refusal left types.rs intact and Admin-owned; its bytes already
matched the reviewed snapshot, so integration skipped the redundant copy. No ACL
change, escalation or process termination was needed.

Next direction is Throwable identity and exception propagation, proposed in
docs/architecture/m2-exception-dispatch-next.md. An exact materialization/handler
contract must precede implementation; symbolic tags must not masquerade as
Throwable objects or silently bypass catches.
Constructor invocation, initialization, static writes, full linkage, verification,
loading constraints, exception objects/handlers, monitors, native methods, threads,
GC and Java library integration remain incomplete. Kotlin/Spring Boot is not yet
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
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
reference-calls-runtime-worker and reference-calls-interpreter-worker output directories.

# Accepted checkpoint: checked static state and body access

Updated 2026-11-04. M1 and earlier M2/runtime increments are approved. Checked
heap-owned static state and the composable body-access profiles passed integrated
validation and frozen-source review; see docs/architecture/m2-static-state-review.md.
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
  Checked static access supports 193, or 194 with supplied exceptions.
  The earlier heap/static mode retains 179 and numeric APIs 148.
- Runtime-aware primitive/reference body graphs with shared instruction fuel, explicit iterative
  frames and frame/slot/prepared-code quotas. Registered frames and prepared
  cache entries use complete MethodIds, with each frame's own constant pool,
  fields, static helpers and diagnostic owner identity.
- Immutable definitions keyed by defining loader and exact UTF-16 name; bounded
  sources, explicit hierarchy resolution, parent-linked layouts, hidden fields,
  and typed static storage without initialization.
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

Checked static state belongs once to each managed heap, separately from the old
read-only StaticStorage. It retains one location per declared static field and
explicit zero-field class membership. Class/slot/actual-Value-byte limits and
fallible amortized staging bound allocation. Repeated allocation is idempotent;
extension after graph growth preserves every old value and location, appends only
new defaults, and publishes after all checks/reservations. Stale contexts/profiles
fail explicitly. Static allocation or writes alone do not invalidate other profiles.

The service reuses field resolution and defining declarations, protects final
fields, normalizes raw primitive stores and checks non-null reference assignment.
Borrowed roots retain repeated aliases in location order. The two opt-in body APIs
share the existing engine, resolve b2/b3 from each declaring frame and compose with
supplied-Throwable handlers. Existing entry points still reject static accesses.
Completed writes survive normal, symbolic, supplied-Throwable and host exits.

This is explicit static memory access, not complete Java active-use behavior.
ConstantValue and clinit remain unapplied; no Prepared/Initialized flag exists.
Complete preparation, class initialization, final initializer writes and general
cross-class invokestatic still require their own implementation and review.

## Validation

Final independent checks passed on all eighteen reverified frozen files:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 768 passed, zero failed |
| Optimized runtime/interpreter tests | 681 passed: 366 runtime + 315 interpreter |
| Execution comparisons | All 26,025 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match in both profiles |
| Fixture/reference reproduction | 104 classes and all seventeen output sets reproduced |
| Canonical source provenance | 39 Java sources and seventeen named reference records verified |
| Python provenance regressions | Six passed |
| Expanded parser smoke | 10,000 cases, zero failures |

This increment adds 44 Rust tests: eighteen public and seven private runtime cases,
seventeen public and one private interpreter case, and the genuine execution/state
oracle. Coverage includes identity/invalidation, atomic extension and retry, exact
class/field/descriptor work, 65,535-unit shared descriptors, a 10,000-class hierarchy,
access/finality/assignment ordering, corrupt positions/counters/handles, raw narrow
stores, all call families, old modes, direct/nested mapping and retained effects.
Review corrected repeated exact staging allocations and added an exact retained
field-to-position mapping check with a duplicate/phantom-key regression.

The execution corpus comprises 25,661 earlier cases plus 364 static-memory cases
across thirty-four receiver/method pairs. Results include 334 numeric returns,
six VM failures and 24 supplied uncaught identities. The JDK resets mutable state
before each row. Rust independently checks all twenty initial locations, ordered
aliases/roots, fields, array contents and allocation totals. Each row then admits
another genuine class and checks extension to twenty-four locations, retained
prior effects and four new defaults. ConstantValue/clinit metadata stays unapplied
in separately labeled memory assertions. Java constructors run only in the oracle
driver; this does not establish Java initialization or application startup.

All 98 prior class hashes remain unchanged. All seventeen reference sets reproduce.
The expanded 104-class parser smoke passed 10,000 cases at seed
4851011501752864596: 121 accepted, 9,879 rejected, ten deterministic rechecks,
corpus 225 and zero failures. The unchanged parser's sustained M1 evidence remains
in docs/milestones/evidence: 108,907,980 cases over 300 seconds, zero failures,
with exact-source fingerprint. No finite test proves panic freedom. Rust 1.85,
the declared minimum, remains untested. Local tools are Rust/Cargo 1.98.1 and
Zulu OpenJDK 21.0.7.

Implementation `e29b74fa47d1a19e8df8dc0153927336007ffab2` passed complete
[Linux/Windows run 41828535972](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41828535972).
Both job logs were inspected: Rust/Cargo 1.99.1, Temurin JDK 21.0.13, six Python
regressions, 39-source/seventeen-record preflight, 104 reproduced classes and all
seventeen reference sets. Each platform passed 768 checked-in and 768 regenerated
workspace tests plus 681 optimized tests (2,217 Rust test executions), strict
formatting/lint, and the same 10,000-case parser smoke with zero failures.

## Reviews, limitations and next work

The exact contract and review are docs/architecture/m2-static-state.md and
docs/architecture/m2-static-state-review.md. Component manifests and all eighteen
combined hashes were verified before/after the full gate. The technical director
is GPT-6 Astra (high); isolated runtime/interpreter workers are GPT-6.1 Sol (high).
The coordinator owns genuine fixtures, integration and evidence. Integration uses
atomic replacements with explicit backups and verifies Admin ownership.

Static locations now provide persistent state needed by the initialization engine.
Next work must address lifecycle and active-use responsibilities under a separate
exact contract, including prerequisites for real constants and failure identities.
Default storage must never be described as initialized class or object state.

Constructor invocation, initialization, final initializer writes, general static
method resolution, full linkage, verification, loading constraints, VM exception
production, monitors, native methods, threads, GC and Java library integration
remain incomplete. Kotlin/Spring Boot is not executable. Full M2 and complete JVM
conformance remain open; no performance superiority has been demonstrated.

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
python tools/static_reference.py --check
cargo run -p crabspot-classfile --example fuzz_parser -- tests/fixtures/classes --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
static-runtime-worker and static-interpreter-worker output directories.

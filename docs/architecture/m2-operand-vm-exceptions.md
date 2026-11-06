# Catchable operand-time array and type exceptions

Status: FROZEN IMPLEMENTATION CONTRACT. The preceding bounded Throwable increment is locally accepted and published at eca0cc85dbddbb494d5673709fd68b5672fedf1e. Runtime owner, execution owner and coordinator each read semantic edition 1EC368F101B9B3447502938E9B37B259B477E5A3273D08325B4A1BB6AEA53EC4 in full and reported no remaining blocker. The technical director authorizes this bounded implementation through coordinator dispatch to separately owned worktrees. Source review and a terminal exact-source combined gate remain required for acceptance. The predecessor hosted fixture line-ending defect is a separate packaging repair; this freeze does not claim hosted success or modify its preserved evidence.

## Objective and evidence

Extend the existing genuine Throwable VM-construction continuation to four operand-time faults: ArrayIndexOutOfBoundsException, NegativeArraySizeException, ArrayStoreException and ClassCastException. Together with the existing ArithmeticException, the enabled continuation has five sealed producer kinds. Cover sixteen array load/store instructions, newarray, anewarray, multianewarray and checkcast, without increasing the decoded-opcode count or creating another interpreter. Correct public behavior includes fresh objects, useful fact-derived messages, real constructors, original fault traces, retained earlier effects, same-frame/caller catches and current-object refill/rethrow semantics.

The preceding Throwable contract 1597DEBF remains authoritative for trace/native representation, actual constructors, original snapshot consumption, supplied replacement, nested-fault refusal, first dispatch and bounded export unless this document explicitly extends it. Its public getStackTrace population-native cutoff and unsupported NPE/monitor/linkage producers remain explicit. No JVM embedding, fake successful Throwable object, guest-library body replacement or fallback to an external JVM is allowed.

Basis: detailed-service memo F6B413016E68D7D13F9CF616DA6211DF85029000D92C1BC77E2A9A2D629872A7; private reference amendment 882855C578A4942B76968B931D424B40604286832CC2156C4741C971A3BB9DA5; director reference review 443C5048A06FA4BAD32DFA97643EFB40B6E0AACD6E18987A0ECA550EC3183223. The corrected reference has 230 cases across eight original/rebuilt, Zulu/OpenJ9, normal/-Xint runs. The earlier 1840 rows were also revalidated. Exact vendor messages are observations, not universal requirements. The reference remains private stable-input evidence; publication requires its own provenance gate.

Governing rules: [JVMS 21 instructions](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html), especially aaload/aastore, checkcast, anewarray and multianewarray. Preserve null-before-bounds, bounds-before-store-assignment, resolution/access-before-negative allocation where applicable, and null checkcast without type resolution. Choosing the first negative dimension's message matches the primary observation; the specification requires the exception, not that diagnostic selection. This increment is partial JVM progress, not M2 or conformance completion.

## Public detailed service results

Add public Copy/Debug/Eq fault enums and an ordinary generic outcome:

```rust
pub enum OperandOutcome<T, F> { Value(T), Fault(F) }
pub enum ArrayOperandFault {
    Bounds { index: i32, length: i32 },
    NegativeSize { length: i32, dimension: u8 },
    Store { source: ReferenceType, component: ReferenceType },
}
pub enum CastOperandFault {
    Mismatch { source: ReferenceType, target: ReferenceType },
}
pub enum OperandExceptionFault {
    Bounds { index: i32, length: i32 },
    NegativeSize { length: i32, dimension: u8 },
    Store { source: ReferenceType, component: ReferenceType },
    Cast { source: ReferenceType, target: ReferenceType },
}
```

The outcome derives conditional traits where T/F permit. Faults retain no heap reference, text, vector or recursive descriptor. Negative dimension is zero-based logical requested position; single-dimensional instructions use zero. Positions 0..254 are representable; 255 is invalid. Type identities preserve defining loader and array rank/leaf. These public structural facts do not authenticate bytecode history.

New FRC methods keep the current corresponding arguments:

| Name | Value | Fault | Allowed variants |
| --- | --- | --- | --- |
| read_reference_array_detailed | Reference | ArrayOperandFault | Bounds |
| write_reference_array_detailed | () | ArrayOperandFault | Bounds, Store |
| allocate_reference_array_detailed | Reference | ArrayOperandFault | NegativeSize, dimension0 |
| new_reference_array_detailed | Reference | ArrayOperandFault | NegativeSize, dimension0 |
| allocate_multidimensional_array_detailed | Reference | ArrayOperandFault | NegativeSize |
| new_multidimensional_array_detailed | Reference | ArrayOperandFault | NegativeSize |
| check_cast_detailed | Reference | CastOperandFault | Mismatch |

Return `Result<OperandOutcome<Value, Fault>, existing error>`, with ArrayAccessError or TypeAccessError unchanged outside the outcome. Existing public methods call the same detailed core once and project selected faults to their existing direct Java errors. Never run both operations or recover details from a global last-error field. Success and preexisting failure prefixes retain exact existing lookup/work/cache/scratch/heap behavior. Fixed result wrapping and erasure charge no new work.

Only the initial destination bounds check in reference-array services returns Bounds. Later raw load/commit failures remain their current host wrappers. Store facts arise only from Ok(false) of the original assignment check; a relation error remains host. Use actual runtime source/component types, including Class for mirror objects. Primitive heap APIs already retain bounds/negative values and need no additive API: only a direct matching error at the actual instruction's service can create an operand event. Do not search nested error sources for a Java-shaped variant.

Maintain typed operand extraction and primitive opcode-kind checks before primitive load/store. Raw primitive store keeps bounds, ValueKind and protected String-backing write refusal in current order. Reference null/bounds precede source validation. c5 enumerates the existing paid loop once before geometry; earlier zero does not suppress a later negative, and the first negative fact uses logical order rather than reverse stack-pop order. Cast retains structural CP validation, null fast path, source validation, target resolution and assignment. Instanceof remains unchanged; false is not an exception event.

## Optional sealed operand role plan

Do not add fields to existing input structs or expand old low-level Throwable sessions implicitly. Introduce an opaque Clone/Debug/Eq `OperandExceptionPlan`, a Copy `OperandExceptionRoles` with five named ClassIds (`index_out_of_bounds`, `array_index_out_of_bounds`, `negative_array_size`, `array_store`, `class_cast`), and:

```rust
pub struct OperandExceptionDiscoveryLimits { pub max_work_units: u64 }
pub struct OperandExceptionDiscoveryUsage { pub work_units: u64 }
pub struct OperandExceptionDiscovery { pub plan: OperandExceptionPlan, pub usage: OperandExceptionDiscoveryUsage }
pub fn probe_operand_exceptions(
    registry: &DefinitionRegistry,
    throwable: &ThrowablePlan,
    limits: OperandExceptionDiscoveryLimits,
) -> Result<OperandExceptionDiscovery, Box<OperandExceptionError>>;
```

Default discovery maximum: 4,194,304 work units. No None result: the supplied valid ThrowablePlan is the candidate trigger. Missing/malformed operand roles after that trigger are typed errors, including under Auto. Absence of the preceding Throwable profile is handled by the launcher before this probe and costs zero operand discovery. There is no silent partial producer set.

Discovery owns no heap, performs no class initiation or allocation, and cannot certify executable proof. It validates the registry and retained designated bootstrap identity from ThrowablePlan (never loader slot zero). Fixed required name order: IndexOutOfBoundsException, ArrayIndexOutOfBoundsException, NegativeArraySizeException, ArrayStoreException, ClassCastException, all in java/lang. One registry-order pass: Work1 before each row; skip nonbootstrap rows without name work; for a relevant row fetch self Class and Utf8 edges at Work1 each; compare outstanding names in fixed order, full equal-length UTF-16 comparison prepaid by its unit length, unequal lengths free. Stop when all found. Retain canonical IDs at paid matches, no lookup_class/name replay.

Then process the five found roles in that order: Work1 before class metadata revisit; scalar public/ordinary/concrete checks; superclass Class+Utf8 links Work2 and full equal-length expected-name comparison prepaid. Expected parents: IndexOutOfBoundsException->existing RuntimeException, ArrayIndexOutOfBoundsException->IndexOutOfBoundsException, the other three->RuntimeException. Verify bootstrap defining loader and capture those relationships; binding later proves actual admitted parent identities. Then, within that same retained class read, one declaration-order method pass for own public `(Ljava/lang/String;)V` constructor: Work1 per method row, Work1 name Utf8 edge, prepaid6 for equal-length `<init>` comparison, only matching name follows descriptor edge Work1 and full equal-length descriptor comparison. Stop on exact match; reject missing constructor or wrong public/static/native/abstract shape. Permit other parser-legal modifier bits. Retain MethodId and key at the match. No Code/attribute/body scan or body recipe. IndexOutOfBoundsException is a required genuine parent/constructor role, not an extra permitted VM producer.

`plan.public_roles()` returns the fixed five IDs. Add exactly those five explicit app-to-bootstrap imports and verifier bindings when enabled, with the usual paid Routes work. They are proof seeds; their real hierarchy/interface closure is included before all ordinary proof/witness/layout gates. Existing Throwable public/private roots remain unchanged. No broader java/lang or arbitrary platform import. No public preparation handle or proof issuer.

## Diagnostic policy

The following text policy is frozen. No guest toString/Class.getName invocation, locale dependence, synthetic module or Java ClassLoader labels, host address or identity hash is used.

| Fault | Exact message |
| --- | --- |
| Bounds | `Index ` + signed index + ` out of bounds for length ` + nonnegative length |
| NegativeSize | signed decimal first failing logical dimension value |
| Store | actual source type's diagnostic name only |
| Cast | `class ` + source diagnostic name + ` cannot be cast to class ` + target diagnostic name, with suffix ` (different defining loaders)` only when rendered names are equal and actual defining loaders differ |

Arithmetic keeps its exact prior `/ by zero` message and existing accounting. Numeric and source-only store text match the recorded primary cases. Cast's structural sentence omits unsupported module/Java-loader decoration deliberately. Its equal-name qualifier is a truthful CrabSpot diagnostic distinction, not exact HotSpot/OpenJ9 wording. Module/Java ClassLoader integration remains an open compatibility obligation. The source-only store form is intentionally allowed to be ambiguous about loaders, as observed on the primary; the retained component fact still identifies the actual failed relation.

Diagnostic name: nominal internal name with every slash replaced by dot, retaining all other UTF-16 units exactly. Arrays use rank copies of `[` followed by primitive descriptor letter or `L` + nominal dotted leaf + `;`. Render primitive array tags Z/B/C/S/I/J/F/D, never a fake nominal ClassId; rank1..255 is iterative. Ordinary classes named I or V stay nominal names. A Class mirror's runtime type is java.lang.Class, not its represented type. No escaping of valid name units inside the managed message; terminal diagnostic printing retains the existing safe escaping policy.

Same displayed type name is determined structurally: category/rank/leaf-category must agree; primitive tags compare scalarly; nominal leaf lengths must agree and the complete equal-length leaf comparison is prepaid before scanning. Canonical defining loaders, not displayed Java names, determine the suffix. Distinct type names incur no extra loader-description text. Malformed namespace/rank/leaf identities cause typed host errors, never a fallback generic message.

## Diagnostic limits, usage and exact charges

```rust
pub struct OperandDiagnosticLimits {
    pub max_work_units: u64,          // default 33_554_432
    pub max_message_units: u64,       // default 262_144, per buffer
    pub max_total_message_units: u64, // default 4_194_304, cumulative accepted buffer units
    pub max_temporary_bytes: u64,     // default 524_288, one temporary UTF-16 buffer
}
pub struct OperandDiagnosticUsage {
    pub work_units: u64,
    pub message_units: u64, // accepted cumulative buffer creation, not live managed bytes
    pub messages: u64,     // successfully published fresh managed message Strings
}
```

All fields u64, Copy/Debug/Eq; usage defaults zero. Limits apply per bound heap lifetime and do not reset per evaluation. Old Arithmetic is excluded from these new counters. Existing StringUsage and heap/instance budgets still account every managed String independently. The maximums do not authorize limit-sized allocations.

`operand_exception_spec` funds Diagnostic Work1 before checking fault scalars/issuing its fixed spec; recognized profile/claim mismatch checks precede this charge and cost zero. Bounds require length>=0 and index<0 or index>=length; NegativeSize requires length<0 and dimension<255. Type facts are retained without resolving names/assignment at preflight. These checks do not attest history; type structural validation occurs at Message with a funded prefix. No diagnostic buffer exists during original snapshot or prerequisite initialization.

Message creation first performs unchanged profile/claim/spec checks at zero work, then charges Work1. It obtains recipes for needed types: Store describes source AND validates component, Cast describes both. For each type, Work1 funds fixed rank/loader/handle structure validation. A nominal type or nominal array leaf then pays Work1 for its class/admitted-node metadata and Work1 for each self Class/Utf8 link (total4 for that type). Primitive array needs only its initial Work1. Loader validation is direct canonical loader/index identity, not a hidden lookup by a dummy name. There is no hierarchy walk or assignability replay. Name lengths come from immutable slices without scanning contents. Store's component has the same structural costs but need not read its name links: fixed Work1 plus nominal metadata Work1 (total2 nominal,1 primitive-array); this unused component name is not copied or scanned.

For Cast, equal-shape/equal-length nominal leaf comparison prepays that complete leaf length in Work before equality scanning. This comparison cost is separate from rendering. Mismatched lengths/shapes cost no content work. Static text lengths and array ranks are fixed scalars. Decimal conversion uses fixed stack storage of at most11 UTF-16 cells per i32; each generated digit/sign unit costs Work1 before generation. Handle i32::MIN without signed overflow. No speculative Vec or unbounded format operation is permitted while measuring.

Compute exact output L with checked arithmetic from immutable slice lengths, ranks, decimal scratch lengths and fixed text: bounds fixed32, cast fixed31 plus optional29 suffix, store/negative no fixed text. Before reservation, check in order: per-message L, prospective cumulative accepted message_units, checked 2*L temporary bytes and its limit, prospective Work+L, prospective messages+1 overflow. Guard failures publish none of these prospective counters; earlier accepted fixed/metadata/comparison/decimal work remains. If all pass, atomically debit Work by L and message_units by L, then `try_reserve_exact(L)` once. Allocation failure retains those accepted creation/work counters, publishes no managed String and leaves messages unchanged. Capacity may be allocator-rounded; accounting is logical requested cells, consistent with existing containers.

Emit exactly L units into the reserved Vec without further allocations; slash-to-dot replacement is part of the funded copy. Empty output is impossible for valid faults, but a generic internal zero case must not issue a reserve. Call the existing checked fresh_string on the borrowed completed buffer. This intentionally copies into the managed String: diagnostic Work pays its own emitted units, while unchanged StringWork pays L+1 and StringUsage pays one object/L retained units on success. Heap/Instance errors keep their typed sources. No interning; later guest intern behavior is ordinary. Increment messages only after fresh_string succeeds, with overflow already preflighted. Drop the temporary Vec before returning the Reference to the engine, hence before exception-instance allocation/constructor publication; at most one diagnostic buffer is live for this synchronous call. On a String failure, accepted diagnostic Work/units remain; no messages increment. No reusing heap payload limits as if temporary Vec memory were already a managed allocation.

Hand-count controls, excluding unchanged String and spec-preflight costs: NegativeSize -1 has message Work1+2 decimal+2 emit=5, L2; Bounds -1/length2 has Work1+3 decimal+35 emit=39, L35. Store nominal source lengthN and nominal component has Work1+4+2+N=7+N, L=N. Cast two nominal names S/T of unequal lengths costs Work1+4+4+L=9+L, L=31+|S|+|T|; equal-length leaf comparison adds that full leaf length regardless of equality. Equal rendered names with differing loaders add29 output units. Add spec Work1 and fresh String WorkL+1 separately. Resource tests must derive these without using implementation counters as their oracle.

## Runtime binding, errors and bridge API

New heap binding:

```rust
ManagedHeap::bind_operand_exceptions(&mut self, registry: &DefinitionRegistry,
    plan: &OperandExceptionPlan, limits: OperandDiagnosticLimits)
    -> Result<(), Box<OperandExceptionError>>;
ManagedHeap::operand_diagnostic_usage(&self) -> OperandDiagnosticUsage;
FieldResolutionContext::check_operand_exception_profile(&self)
    -> Result<(), Box<OperandExceptionError>>;
```

First binding follows existing Throwable binding, proof/witness/layout admission and universe binding. Require an empty managed arena, no initialization claim, and zero heap StringUsage and TraceUsage (including paid work). No issued mirror may exist; the empty arena already enforces that. Pure domain class-site resolution/cache work is allowed and is not inspected or certified by this heap binder. Require matching installed Throwable plan, registry, heap identity, universe/bootstrap and sealed layout count; validate all five actual admitted parent identities, fixed constructor keys and canonical roles. It is immutable thereafter; equal repeat binding is idempotent, differing plan/limits PolicyConflict. The first-bind empty-arena rule deliberately rejects prior primitive arrays, reference arrays and ordinary instances alike, matching the actual existing Throwable first-bind requirement. Do not add a history scan/counter or claim to detect prior host static writes. Wrong namespace/heap replacement/layout growth rejects. Readonly profile check must include checked class-site store and prerequisite String/equality/Throwable affinity, mint nothing, and charge zero work.

An opaque non-Clone `OperandExceptionSpec` retains fingerprint, initialization claim fingerprint, selected fixed kind/class/constructor/key and exact fixed fault. Getters only `class()`, `constructor()`, `kind()`. Do not extend the old VmExceptionSpec.message_units accessor to return a wrong fixed message for a new kind. Old Arithmetic APIs remain byte/behavior compatible. Add:

```rust
FRC::operand_exception_spec(&mut self, &InitializationClaim, OperandExceptionFault)
    -> Result<OperandExceptionSpec, Box<OperandExceptionError>>;
FRC::new_operand_exception_message(&mut self, &InitializationClaim, &OperandExceptionSpec)
    -> Result<Reference, Box<OperandExceptionError>>;
FRC::allocate_operand_exception(&mut self, &InitializationClaim, &OperandExceptionSpec)
    -> Result<(Reference, ResolvedConstructor), Box<OperandExceptionError>>;
FRC::authenticate_operand_exception(&mut self, &InitializationClaim, &ExceptionProfile,
    &OperandExceptionSpec, Reference)
    -> Result<ThrownException, Box<OperandExceptionError>>;
```

Internally share the existing sealed allocation/constructor/trace authentication machinery through a fixed role view, not copied algorithms. Validate the exact spec against its bound role row on every use: fixed kind/class/constructor/key and fingerprints must match, and the retained fault variant must map to that kind with its scalar validity rechecked. A role row has no per-event fault tuple. No duplicate tuple/hash or historical attestation is required; opaque spec storage retains the event facts, while only the engine authenticates their operation/site history. The only four new allocation targets are fixed ClassIds from the plan. IndexOutOfBoundsException is never accepted as a producer. Structural authentication requires actual exact class/current reciprocal trace and existing prepare_throw checks; engine alone certifies constructor completion and original snapshot consumption.

Define public `OperandExceptionError { kind: OperandExceptionErrorKind, discovery_usage: OperandExceptionDiscoveryUsage }`. Kind: Disabled, UnsupportedShape, PolicyConflict, InvalidBinding(&'static str), InvalidState(&'static str), InvalidFault, Limit{resource,limit,required}, Overflow{resource}, AllocationFailure; boxed Registry/Context/Type/Heap/Instance/String/Construction/Throwable/Exception causes. Resource enum: DiscoveryWork, DiagnosticWork, MessageUnits, TotalMessageUnits, TemporaryBytes, Messages. Messages has overflow only, no separate count quota. Discovery errors report exact accepted probe prefix; runtime errors use zero discovery_usage. Add boxed ExecutionErrorKind::OperandExceptionAccess and ApplicationErrorKind::OperandException, plus SetupKind::OperandException, preserving Display/source and error-size lint. No general conversion of these host errors to Java faults. No new Java-visible exception for resource exhaustion.

## Engine session and continuation

Add `InitializationSession::new_with_operand_exceptions` with the same seven arguments as new_with_throwables. Before permanent claim, check mirror, String, Throwable and operand profiles in that order; no mirror mint. Enable a private operand capability in addition to Throwable. Existing new_with_throwables remains Arithmetic-only even on an operand-bound heap. Other explicit constructors/body APIs preserve prior behavior. Native trace capture does not change.

Eligible detailed outcomes create a private operation-tagged fault event at the actual instruction site, one fuel debit and existing typed pops. The tag restricts allowed fact variants (including dimension0 for bc/bd and requested dimension bound for c5) before materialization. Store no original operand Reference or 255-count buffer. Preserve actual MethodId/PC/stack-depth/evaluation identity, not a host-created call site. No outer catch-all mapper converts arbitrary wrapped failures. Unsupported old outcomes stay symbolic/host as before.

The existing pending state uses a private Arithmetic-or-Operand spec variant, sharing one continuation. The initial preludes deliberately differ: Arithmetic preserves its existing zero-work vm_exception_spec validation before Started/per-evaluation admission in every profile; a new Operand event first atomically admits Started/per-evaluation budget, then obtains its charged sealed spec. Thus a zero producer quota consumes no new diagnostic work, and a spec failure after accepted admission retains Started. No secondary admission is attempted while materialization is pending. Both preludes converge on one shared continuation: selected real constructor Code/handler/frame/slot preflight; original VM snapshot; actual exception-class initialization; message (old Arithmetic fresh_string or new bounded method); actual instance/constructor capability; typed argument/receiver checks and ordinary frame/monitor publication; genuine super/constructor execution; required original snapshot consumption; structural authentication; Produced publication and first dispatch at original fault PC. No extra evaluation, fresh fuel, synthetic Code/CP, call frame or monitor lease for the message service.

Pending secondary VM faults of any kind remain NestedVmMaterialization before second admission. Invalid facts/diagnostic/heap/resource errors retain original typed kind plus existing producer context. Actual constructor decode/native/helper failure provenance and pure-service original fault provenance follow the preceding contract. Original PC remains prepared-instruction truth owned by engine. First-dispatch errors are no longer nested once pending is cleared. Genuine supplied constructor/initializer replacement retains effects/escaped references and follows existing unwind boundaries. The same object may refill later; completion requires initial snapshot consumed, not still current. Counters share existing VmExceptionUsage across all five kinds; Produced never exceeds Started and publication has no reachable fallible stage afterward.

## Launcher API and adoption

Add `OperandExceptionMode { Auto, RequireOperandExceptions, Disabled }` (Auto default), and Copy/Default `ApplicationOperandExceptionProfile { throwables: ApplicationThrowableProfile, mode: OperandExceptionMode, discovery: OperandExceptionDiscoveryLimits, diagnostics: OperandDiagnosticLimits }`. New six-argument `run_directory_application_with_operand_exceptions(directories, entry, observations, options, invocation, profile)` follows the existing wrappers. Existing ApplicationOptions, Invocation, StringLimits and ThrowableProfile input fields stay unchanged.

Ordinary directory/CLI paths and existing high-level convenience wrappers deliberately adopt Auto operand expansion using defaults, preserving any explicitly supplied lower-profile limits. `run_directory_application_with_throwables` remains the explicit prior Arithmetic-only wrapper; callers can also use the new wrapper with Disabled. Lower explicit low-level sessions remain unchanged as stated above. The image's private internal orchestration may take the new profile without changing public input literals. Document these distinctions, rather than silently changing the old explicit Throwable wrapper.

With Auto: if prior Throwable plan is absent/disabled, skip operand probe/bind with zero operand discovery and use the prior session/profile path. Once a valid Throwable plan exists, any missing/invalid operand role or budget failure is a typed preflight error; do not quietly retain Arithmetic-only execution. RequireOperandExceptions reports UnsupportedShape with zero operand discovery when prerequisites are absent. Disabled skips operand discovery/binding regardless of available facts. Matching plan goes through all existing image proof/witness/layout gates for public five-role closure, then binds before arguments/FRC/session. No proof exemptions or binding facts from arbitrary descriptors.

Probe errors and strict missing-prerequisite errors use ApplicationStage::Verification with ApplicationErrorKind::OperandException; heap binding errors use ApplicationStage::Layout with that same boxed runtime kind. New-session constructor errors use ApplicationStage::Setup and ApplicationErrorKind::Setup { kind: SetupKind::OperandException, source: boxed original ExecutionError }. After evaluation starts, retain the existing initialization/main phases and original typed execution errors. Import/routes/proof failures keep their existing stage and cause rather than being relabeled.

ApplicationUsage gains `operand_exception_discovery` and `operand_diagnostics` output fields. Existing output struct literals/exhaustive enums may require additive migration; only existing input struct-literal compatibility is promised. Session VM usage is captured immediately after run; heap diagnostic usage is captured after scoped context release on every post-bind outcome, including setup/initialization/materialization/export failures. Preflight errors retain discovery prefixes with zero heap/evaluation effects. Existing high-level replay tests must account for newly paid Auto discovery and executable closure using independently derived baselines, while preserving prior effect counters and failure precedence; do not waive their earlier invariants. Existing Uncaught current trace/class reporting and bounded export remain unchanged; do not persist original producer history after a Java catch/refill/rethrow.

## Public diagnostic comparison contract

A genuine public entry may receive expected diagnostic text through ordinary String[] arguments and compare caught getMessage().equals(expected) using actual Java bodies. Use identical guest class bytes for product/reference execution, with explicitly documented different expected-message arguments when policies differ. The product oracle supplies the frozen CrabSpot text from independent input facts; the external reference oracle supplies separately pinned vendor text. Never obtain expected text from the product result or the caught exception itself. Require exact message matches and the same independently expected class/identity/retained-state results, with deliberately wrong-message controls proving failure on each side. Primary numeric/source-name cases may share expected text directly; CCE decoration differences remain explicit. No new message observation API is needed for this evidence.

Directory-launchable controls must remain within the declared imports. Same-name separate-loader tests can use public low-level runtime/engine APIs and the separate private Java reference where ordinary directory entry cannot create arbitrary Java ClassLoaders. A separately named public projection records exact transforms and preserves the full original reference. Public API UTF-16 values and OS-representable physical CLI arguments are distinct transport tests.

## Required acceptance

Preceding bounded Throwable acceptance and all exact-byte owner/coordinator readbacks are complete. Implement only this frozen scope in isolated coordinator-dispatched owned worktrees and request staged/final reviews. Required checks remain fmt, strict workspace all-target Clippy, workspace debug/relevant optimized suites, existing genuine/reference/public regressions, parser corpus/fuzz, source pre/posthashes and honest hosted follow-up. Never replace original failed evidence or relax an independent result/resource oracle.

Automated matrix:

1. Detailed/legacy success, selected fault and host-error equivalence across every method; exact paid prefix/cache/heap/old contents. Null, stale/foreign, wrong kind, protected backing, destination bounds before foreign source, relation quota versus false, bad CP/null cast, access/rank before negative size. c5 first-negative positions0/1/254, earlier zero, partial rank. Old APIs never gain new materialization merely from heap binding.
2. All sixteen loads/stores with -1,length,MIN,MAX,zero/legal endpoints; all primitive kinds including shared boolean/byte opcode, raw float/double values and category2. New allocations bc/bd/c5 and class/interface/array cast failures/success/null; same-name separate defining loaders, marker covariance, ranks1/255 and primitive/nominal array distinctions. Captured message types use actual String/Class/backtrace runtime types, never represented mirror type.
3. Independent exact message units for every policy form, Unicode names, same-name loader qualifier, long names, decimal MIN/MAX, all array tags/ranks. Exact hand work examples above; zero/one-less/MAX limits, each prospective counter overflow, 2*L overflow and reserve failpoint. Compare diagnostic accepted-unit prefix versus managed String actual counts on failure; successful counters stay separate. No name/relation work on success. After message failure, no exception allocation or handler entry, earlier class-init effects retained.
4. Fixed-role discovery missing/duplicate/wrongloader/parent/ctor flags, 65,535-method scans, short unequal names, exact/one-short paid prefix, malformed ordinary constructor Code executed/prepared normally (no body recipe), plan/heap namespace replacement and layout/claim drift. Only five new imports; proof quota and fact-only failure retain existing authority boundaries.
5. Same-frame/caller catches for all four new classes and common RuntimeException; fresh identities, actual fault/helper PCs, cause/message, rethrow/refill; mixed Arithmetic plus four new kinds share one budget/evaluation. Constructor/initializer supplied replacement, escaping unfinished references, nested supported/unsupported fault, first-dispatch failure, frame/Code/handler/fuel limits and recursive same-method origins. All old Throwable lifecycle/native/trace corruption tests remain.
6. Genuine original/rebuilt public application/API and physical CLI with independent wrong expected-result guard. Check fields and existing message equality using real guest String Code, actual Uncaught class/current head, producer/trace/message counters and all profile modes. External JDK observer stays separate from product inputs; no broad reflection/loader imports solely for evidence. Private reference data may guide a separately approved public projection; preserve source transforms/provenance. Vendor messages are compared under the declared policy, without fabricated module labels or a comparator that ignores all text.

All prior current-increment failures and amendment evidence remain preserved. Passing this bounded set does not implement NPE extended-message/refill services, monitor-close or catch-resolution producer continuations, Java stack-trace element population/formatting, arbitrary standard-library bodies, threads/JMM/GC/JNI or full JVM conformance.

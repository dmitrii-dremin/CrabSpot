# Single-session initialization and active-use continuations

Status: bounded implementation approved by the technical director on 2026-11-04 after frozen source review and the 838-test workspace/751-test optimized integrated gate; see `m2-initialization-session-review.md`. Workers initially received the authorized 1A8177C5 contract after baseline 79fb727. Review clarifications added the read-only admitted-class check and optional diagnostic class context, and pinned atomic descriptor charges, predicate early stopping and scalar continuation rechecks. Fresh hosted CI remains pending. Full M2, complete linking, multithreaded initialization and Java startup remain open. No NanoSpec.

## Scope and specification decisions

Implement actual class/interface initialization in an opaque interpreter-owned session over explicitly admitted metadata. Execute real clinit bodies in the existing engine; install supported numeric ConstantValue data; schedule required dependencies; suspend/resume static active uses; and retain completion/failure state across repeated evaluations. Existing body APIs keep their host-controlled semantics and never acquire automatic initialization.

The normative control flow is [JVMS 5.5](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.5): recursive requests by the current execution may proceed, class dependencies precede its initializer, and escaping initializer failures differ from ordinary method failures. This profile is single-execution, without Java threads or initialization locks. Admission remains unverified/incompletely linked; Completed below certifies this session's supported initialization procedure, not full preparation, loading constraints or a complete JVM lifecycle.

Apply ConstantValue to every static field carrying the attribute, including nonfinal fields; ignore it on instance fields. This follows [JVMS 4.7.2](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.7.2). Original HotSpot 21.0.7 probes confirmed nonfinal static numeric attributes and B/C/S/Z normalization. String ConstantValue is explicitly unsupported. No null substitute, fabricated String or skipped constant may yield completion.

For class initialization, enumerate superclass first, then superinterfaces in declared-order depth-first postorder, selecting interfaces declaring any nonabstract, nonstatic ordinary method. Include private, bridge and synthetic concrete instance declarations; exclude clinit regardless of its raw flags. Deduplicate interfaces by actual ClassId while preserving first postorder occurrence. Initializing an interface itself does not initialize its Object dependency or superinterfaces. A previously completed interface must not prune traversal of its ancestors when constructing a later class's dependency list. HotSpot probing confirmed private-concrete inclusion and static-only exclusion.

Only exact clinit()V bodies are supported. Historical parameterized clinit descriptors are a distinct unsupported prerequisite. For accepted ()V initializers, honor the specification's ignored method flags rather than treating raw native/abstract/synchronized bits as ordinary-method semantics; semantic static execution has no receiver, including older files without ACC_STATIC. See [JVMS method flags](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.6). Ordinary preparation/call paths continue to reject initializer names.

No constructors, new opcode, general cross-class invokestatic resolution, reflection execution, Java assertion-status loader query, native services, String interning, VM error construction, monitors or threads are added. Unsupported instructions/services reached by real clinit code remain explicit host failures. Existing conservative access/loading boundaries remain intact.

## Session ownership and public interpreter API

The runtime must not expose a public completion setter or Initialized flag. Lifecycle records belong privately to InitializationSession in the interpreter. The session exclusively borrows a mutable FieldResolutionContext for its lifetime and retains the static and exception profiles by value. Its constructor acquires a permanent, non-resettable claim on the heap only after all constructor checks/reservations succeed. One heap can support at most one such session. There is no release/reclaim API.

Dropping the session ends that initialization domain. The heap and its static/ordinary values remain inspectable through existing APIs, but a new initialization session on it returns AlreadyClaimed. A fresh heap is required for a new domain. This prevents replaying initializers after dropping their private completion table. No Clone implementation exists for the session or claim. While the session exists, expose only read-only context/heap inspection; no mutable context escape, layout growth, static extension, universe rebinding, raw initializer invocation or host state setter is available.

Export from interpreter:

```rust
pub struct InitializationLimits {
    pub max_classes: usize,           // default 16_384
    pub max_plan_dependencies: usize, // default 1_048_576
    pub max_constant_values: usize,   // default 65_536
    pub max_pending: usize,           // default 16_384
    pub max_steps: u64,               // default 1_000_000 per evaluation
}
pub enum InitializationStatus {
    Uninitialized,
    InProgress,
    Completed,
    Erroneous { cause: Reference },
    Aborted,
}
pub enum InitializationOutcome { Completed, Uncaught(Reference) }
pub struct InitializationSession<'context, 'registry> { /* private */ }

impl<'context, 'registry> InitializationSession<'context, 'registry> {
    pub fn new(
        context: &'context mut FieldResolutionContext<'registry>,
        statics: &StaticStateProfile, exceptions: &ExceptionProfile,
        error_class: ClassId, limits: InitializationLimits,
    ) -> Result<Self, ExecutionError>;
    pub fn initialize(
        &mut self, class: ClassId, options: ExecutionOptions,
        call_limits: CallLimits, exception_limits: ExceptionLimits,
    ) -> Result<InitializationOutcome, ExecutionError>;
    pub fn run_static(
        &mut self, method: MethodId, arguments: &[Value],
        options: ExecutionOptions, call_limits: CallLimits,
        exception_limits: ExceptionLimits,
    ) -> Result<ExceptionOutcome, ExecutionError>;
    pub fn status(&self, class: ClassId)
        -> Result<InitializationStatus, ExecutionError>;
    pub fn context(&self) -> &FieldResolutionContext<'registry>;
    pub fn retained_roots(&self) -> impl Iterator<Item = HeapRef> + '_;
    pub fn last_initialization_steps(&self) -> u64;
}
```

Limits are Copy + Debug + Eq + PartialEq; statuses/outcomes may be Copy. Constructor ordering: validate static profile, exception profile, admitted count against max_classes, reserve a record-map capacity for that actual count, then validate/acquire the runtime claim including Error identity. No allocation from a policy maximum. A failed constructor before successful claim leaves the heap unclaimed. No fallible work follows claim publication.

The record map may populate lazily; an absent admitted class is Uninitialized. Retained plans and their dependency/constant totals are session-wide. Instruction fuel, Code/handler cache counts, frame reservations, exception-dispatch steps and initialization steps are fresh per public evaluation; runtime context lookup/symbol counters remain cumulative. Failed preflight does not install a plan or consume retained-plan totals, though actual lookup work remains spent. max_steps and max_pending use the constructor's policy for every evaluation.

initialize requests the supplied checked admitted ClassId explicitly. run_static accepts a registered ordinary static MethodId and supported existing numeric/reference arguments/results. It performs ordinary static body preparation, computational/reference argument validation and the root frame-capacity check before requesting initialization of that method's declaring class/interface. It does not execute a constructor or select an instance root. Only after successful initialization is the root body activated. There is no fictitious Java frame or catch region around root initialization. Both APIs use the same engine/cache/scheduler as nested active uses, not recursive interpreter calls.

status validates the class against the fixed admitted snapshot; it never resolves names or initializes anything. retained_roots yields one handle per retained Erroneous cause in first-record-insertion order, including duplicate causes, without allocation; these are additional roots to static/heap roots, not mutable collector locations or a synchronized GC protocol. Use bounded ordered record storage plus an identity index if needed, reserving both before the heap claim. Returned outcomes preserve actual references.

## Runtime services and checked identities

Add a private heap claim bit/identity without changing existing option struct literals or context invalidation. Export opaque non-Clone Debug types InitializationClaim, InitializationPlan and ResolvedInitializationStatic. These are checked metadata/operation tokens, not lifecycle-success certificates. A host may use low-level runtime services under their explicit method-context contract; only the interpreter session owns completion states. No runtime API can mark a class completed, clear a claim or accept arbitrary final-field assignments by FieldId.

Export Copy enums InitializationStaticKind::{Read, Write}, InitializationPlanResource::{Dependencies, Constants}; and:

```rust
pub struct InitializationPlanLimits {
    pub max_dependencies: usize,
    pub max_constants: usize,
}
// InitializationPlan accessors: class(), initializer() -> Option<MethodId>,
// dependencies() -> &[ClassId], constant_count() -> usize.
// ResolvedInitializationStatic accessors: field() -> FieldId, kind().

impl FieldResolutionContext<'_> {
    pub fn claim_initialization(
        &mut self, statics: &StaticStateProfile,
        exceptions: &ExceptionProfile, error_class: ClassId,
    ) -> Result<InitializationClaim, InitializationAccessError>;
    pub fn check_initialization_claim(
        &mut self, claim: &InitializationClaim,
    ) -> Result<(), InitializationAccessError>;
    pub fn check_initialization_class(
        &self, claim: &InitializationClaim, class: ClassId,
    ) -> Result<(), InitializationAccessError>;
    pub fn plan_initialization(
        &mut self, claim: &InitializationClaim, class: ClassId,
        limits: InitializationPlanLimits,
    ) -> Result<InitializationPlan, InitializationAccessError>;
    pub fn install_initialization_constants(
        &mut self, claim: &InitializationClaim, plan: &InitializationPlan,
    ) -> Result<(), InitializationAccessError>;
    pub fn resolve_initialization_static(
        &mut self, claim: &InitializationClaim, origin: MethodId,
        index: u16, kind: InitializationStaticKind,
    ) -> Result<ResolvedInitializationStatic, InitializationAccessError>;
    pub fn read_initialization_static(
        &mut self, claim: &InitializationClaim,
        access: &ResolvedInitializationStatic,
    ) -> Result<Value, InitializationAccessError>;
    pub fn write_initialization_static(
        &mut self, claim: &InitializationClaim,
        access: &ResolvedInitializationStatic, value: Value,
    ) -> Result<(), InitializationAccessError>;
    pub fn is_initialization_error(
        &mut self, claim: &InitializationClaim, thrown: &ThrownException,
    ) -> Result<bool, InitializationAccessError>;
}
```

The claim stores the checked static/exception fingerprints and explicit Error ClassId. Claim order is static check, exception check, existing-claim rejection, then Error validation. Error must be a public nonabstract ordinary admitted class with exact name java/lang/Error, defined by the universe's explicit bootstrap loader, and direct superclass exactly the profile's Throwable. Charge one class step, all actual name units before comparison, then one edge step before checking the parent. The valid anchor costs seventeen shared lookup steps. Claim publication is the final infallible action. Check operations validate fingerprints and ownership without new traversal/work. Static value changes and ordinary allocation remain compatible; graph/universe changes do not.

check_initialization_class is a read-only, zero-work check: validate the unchanged claim, then registry ownership and actual admitted graph membership for the supplied ClassId. It performs no resolution or planning and includes zero-method/zero-static classes. Session status uses this check rather than treating every registry definition as admitted.

is_initialization_error validates the claim and thrown token/profile/source using the existing checked catch-all validation, then uses the class-only ancestry relation against Error. It charges that ancestry's existing node work. Do not widen protected-access subclass logic or identify an Error by name alone. Foreign-loader subclasses with the correct actual ancestry are valid.

InitializationAccessError must have structured variants for Context(FieldAccessError), Static(Box<StaticAccessError>), Exception(Box<ExceptionAccessError>), AlreadyClaimed, ClaimMismatch, InvalidErrorAnchor { class: ClassId, reason: &'static str }, InvalidMetadata(&'static str), UnsupportedInitializerDescriptor { method: MethodId }, UnsupportedStringConstant { field: FieldId }, PlanLimit { resource: InitializationPlanResource, limit: usize, required: usize }, AccountingOverflow { resource: &'static str }, AllocationFailure { resource: &'static str }, and Java(StaticJavaError). Preserve Display/Error::source. Only direct selector/static Java failures deliberately elevated by the actual operation become the top-level Java variant; nested Java-shaped diagnostics stay host failures. This wrapper must not create recursive enum sizing.

## Plan preparation, constants and dependency work

plan_initialization validates claim/class/admission, then charges one class lookup step. It derives own constants and the initializer from immutable registry metadata, without executing code, resolving names through callbacks, or changing state/values.

Charge one step per declared field and one per examined field attribute, including ignored instance attributes. For each static ConstantValue, check remaining constant-entry quota before staging; charge the full descriptor once per (ClassId, descriptor_index) before classification. Numeric values use existing B/S signed truncation, C unsigned truncation, Z low-bit masking, exact I/J and raw F/D bits. Validate CP tag/type and retained location. Any static String ConstantValue rejects the complete plan before publication; instance ConstantValue is ignored. No owned descriptor/name tree is retained.

Charge one step per declared method. Identify at most one clinit; require exact ()V for this profile, without interpreting its ignored flags as ordinary native/abstract/synchronized behavior. Keyword/descriptor comparisons have bounded fixed lengths. The engine separately prepares its Code, role and handlers using existing resource/boundary checks before beginning initialization of this class. Unsupported parameterized historical descriptors do not get fake arguments.

For a class plan, append its direct superclass if present, charging the edge before append. Enumerate its interfaces iteratively in declared-order DFS postorder. Charge every examined/enqueued interface edge, including duplicates, before growing the worklist; charge each first-visited interface node and each examined declaration used to determine its nonabstract/nonstatic ordinary-method predicate. Validate all visited node kinds/identities. Append each qualifying interface once at postorder, checking the dependency quota before growth. Interface plans have no dependency list. Never consult lifecycle Completed status to prune this metadata enumeration. Traversal uses bounded fallible worklists/visited sets, not Rust recursion or copied ancestor trees.

Plans retain only the class, optional initializer MethodId, bounded ordered dependencies and numeric constant entries with checked scalar metadata indices/locations. They carry the claim fingerprint privately and expose no mutable slices. The engine supplies remaining aggregate dependency/constant quotas; only a fully prepared plan increments session totals and enters its cache. Failed preparation may be retried while the class remains Uninitialized. Plan caching must not duplicate Code/handler tables or nominal strings.

Once a class is marked InProgress, install its numeric constants before initializing dependencies. The runtime batch installer validates the claim/plan and every destination/value before writing any location, then publishes all supported values without a fallible operation after the first write. Values are exclusively those represented by immutable ConstantValue metadata, not caller-provided arbitrary final values. Charge the number of own static locations visited by this installation atomically before scanning them; a plan with no constants needs no slot scan. Work must be linear in the class's own storage, not the entire heap for every class. Preserve declaration order. No constant reinstall occurs on recursive or Completed requests.

After a later host failure, installed constants and prior Java effects remain. These values do not themselves imply successful initialization.

## State machine and failure finalization

Private records retain Uninitialized, InProgress { evaluation_owner, phase }, Completed, Erroneous { cause }, or Aborted. Evaluation owners are checked nonwrapping session counters. The external status omits private phase/owner details. There is no public transition API.

Every request charges one initialization step before examining state:

- Completed returns normally without rerunning constants, dependencies or code.
- InProgress by this evaluation is recursive use and returns normally. Another owner is an internal/session-busy host failure, never permission to skip another execution.
- Erroneous returns explicit unsupported NoClassDefFoundError production, preserving the stored cause. Do not throw the old cause again or fabricate a new object.
- Aborted returns a distinct InitializationAborted host error. It is never silently retried.
- Uninitialized prepares/caches its plan and initializer body, checks pending capacity and reserves scheduler storage, then charges one begin step before publishing InProgress. Install constants, process dependencies in order, execute its actual initializer if any, and complete only through the engine's checked transition.

Charge one scheduler step before visiting each dependency entry, one before entering the initializer-body/no-body stage, and one before normal completion. Requests for each dependency also consume their request step. No-body classes still require scheduler work but no Java instruction fuel or Java frame. Dependencies are initialized before the class body, while own constants are installed first. max_pending counts all outstanding class-initialization activations, including activations whose bodies are running or waiting for dependencies; recursive requests add none.

Normal clinit return marks its activation Completed and resumes its continuation. An exception caught inside that body is ordinary handler execution and does not fail the class. When a supplied throwable actually escapes its initializer frame, stop ordinary caller search at that boundary. Charge one abrupt-boundary scheduler step, classify it against explicit Error, then:

- Error or subclass: mark the class Erroneous with that same reference. Each dependency-waiting parent activation being failed consumes one propagation step and becomes Erroneous with the same cause. Stop propagation at the original Java active-use continuation, then search that caller's handlers at the triggering instruction PC. An initializer body that catches a dependent initializer's Error may still complete normally.
- Other Throwable: return UnsupportedInitializerWrapping { class, thrown } as a host failure. No ExceptionInInitializerError is created. The failing class and every still-in-progress activation become Aborted, not falsely Completed or modeled as having thrown an unavailable wrapper.

All eleven existing VM-produced categories remain UnsupportedVmException, including failures during method selection, field access, null athrow or catch resolution. They do not enter handlers. Other host failures retain their existing structured diagnostic. Any host error leaving an evaluation finalizes every still-InProgress record as Aborted, releases/discards all transient frames/continuations, and preserves Completed/Erroneous records and all static/heap effects. A failure before a class enters InProgress leaves that class Uninitialized; already-active ancestors still abort. Cleanup is allocation-free, cannot exhaust another work budget and is bounded by the configured class/task limits. No InProgress state may escape a returned Result. Use an unwind guard so a host callback panic cannot later be recovered into a reusable session with stranded InProgress records; do not catch/translate that panic into a Java exception.

A later evaluation may run unrelated classes after a host failure. It cannot retry Aborted classes. Subsequent Erroneous use is an explicit unsupported NCDFE host result. Dropping the session never clears the heap claim, regardless of success or interruption.

## Active-use suspension and actual initializer frames

In this session, b2/b3 and supported same-class b8 are initialization triggers; old APIs remain unchanged. Both root APIs run the supplied-Throwable policy and can return actual uncaught references. No initializer is invoked through an ordinary Methodref to clinit.

For b2/b3, debit the instruction's fuel exactly once. b3 first pops one Value as in the static-memory profile, then resolves a static operation token using the current MethodId. b2 resolves without reading its value. Resolution performs the existing access/loading/static checks and final-write authorization before scheduling. Retain the exact declaring FieldId, original method/PC, remaining next PC and optional popped value in a continuation. Request initialization of field.owner(), not the symbolic owner. On success read the current value or commit the retained store exactly once, without replaying selector work, fuel debit or stack pops. Reference kind/assignment work occurs on store resumption after initialization, following the established static-service order. Read push overflow may occur after successful initialization and must preserve it.

A write token permits a final destination only when the actual origin MethodId is the exact supported clinit of that same declaring class/interface. The engine issues such an origin only for a real scheduler-created initializer frame. Helpers, inherited declarations, ordinary roots and other initializer classes receive direct IllegalAccess. The checked runtime operation remains method/CP-based, never an arbitrary final FieldId setter or boolean permission. Token resumption revalidates claim, access kind, origin/declaration metadata, static/final relationship, location and supplied value/handle; it does not rerun symbolic resolution. A private commit remains mutation-last. Old write_static and all old profiles continue to reject every final write.

For supported b8, preserve the existing owner/tag/special-name, target preparation, reservation and typed argument validation order, then retain the resolved target/arguments/original invoke PC and request its declaring type's initialization before pushing the callee. Same-class scope remains; no new cross-class resolver is hidden in this change. A recursive helper call from its own clinit sees InProgress and proceeds. Suspended operand values move into one continuation per triggering frame; they remain covered by that frame's reserved operand capacity and are not duplicated into an unbounded pending store.

Initializer frames have a distinct role and preparation mode. They use zero arguments/no receiver and a Void shape, validate Code/handler boundaries through the existing machinery, count against the same frame/slot/Code/handler limits and consume the same instruction fuel. Cache role checks apply on every hit; an ordinary static cache hit cannot grant initializer authority. Historical ()V clinit flag handling must not relax ordinary native/synchronized/abstract restrictions.

Extend the one engine with explicit root/dependency/instruction continuations and initializer frame roles. Existing origin tuples alone are insufficient: a dependency activation may have no Java frame, and a root initialization has no Java caller. Do not fabricate method indices or invocation instructions. Normal return from an initializer completes its task instead of pushing a value. Exception dispatch must expose an escaped-initializer boundary before searching a suspended caller. Mark dependency failures before resuming caller handler search; use the original b2/b3/b8 PC, never its resume PC. Host unsupported/failure paths share one finalizer, including direct method_failure and eager preparation exits.

run_static prepares a registered ordinary static root before requesting its owner, but that root is not yet an active Java frame. Its initializer frames count as actual frames; sequential initializer/root frames do not count twice. Explicit initialize on an empty/no-body class may succeed with zero instruction fuel and zero Java frames if scheduler work allows it. Pending activation depth is independently bounded. No recursive call to another public execution entry point is permitted.

Descriptor precharges above use the existing atomic bulk-work charge: charge the complete descriptor length before classification, with no partial counter increment on failure. The interface predicate may stop after its first qualifying declaration, charging only declarations actually examined; ancestor traversal remains complete.

Reusable static continuation tokens retain scalar immutable CP/member indices and flags sufficient to recheck their origin, symbol, selected declaration and location. Resume must not repeatedly compare full UTF-16 names/descriptors without work accounting. Scalar validation preserves the no-selector-replay contract; value assignment work remains charged normally.

## Interpreter errors, work and diagnostics

Add ExecutionErrorKind variants:

```rust
InitializationAccess(Box<InitializationAccessError>),
InitializationAborted { class: ClassId },
UnsupportedInitializerWrapping { class: ClassId, thrown: Reference },
UnsupportedNoClassDefFoundError { class: ClassId },
InitializationLimit {
    resource: InitializationResource, limit: usize, required: usize,
},
InitializationStepLimit { limit: u64, required: u64 },
InitializationAccountingOverflow { resource: &'static str },
```

InitializationResource is Classes, PlanDependencies, ConstantValues or Pending. Use existing allocation/invalid-method diagnostics where appropriate. Runtime plan-limit errors preserve the supplied remaining allowance and required amount; session aggregate counter overflow is checked separately before caching. max_steps resets per evaluation; failed charges do not advance it. last_initialization_steps exposes the actual completed charge count, including on failure. Cleanup consumes no additional steps.

Only explicit InitializationAccessError::Java maps to the corresponding UnsupportedVmException in this session. Every nested Static/Exception/Context error remains host-side. No exception/error category is materialized by this increment. Normal supplied uncaught exceptions outside an initializer remain unchanged and need no wrapping.

Body errors retain the actual executing method/PC and its meaningful transition origin. Add public ExecutionError.initialization_class: Option<ClassId>, initialized to None by the existing constructor for all legacy errors. Initialization prerequisite, scheduler and body boundaries attach the responsible class when known without overwriting a more specific inner class; Display includes this context. This supplements, rather than changes, the specified error-kind variants. Test no-body plan/step-limit failures without inventing a MethodIdentity or bytecode offset. When triggered by Java code, retain its real method/PC. Error propagation resumed at a caller uses the caller's actual active-use instruction for handler range checks. Completed initialization and earlier writes must survive later caller stack overflow, operand/type failure, work failure or root-body failure.

## Worker split and acceptance obligations

Runtime worker owns claim/anchor/plan/token services, bounded metadata/default-interface traversal, numeric constant installation and checked initializer-scoped static operations, runtime tests and README. Interpreter worker owns session records/lifetime/cleanup, actual initializer roles, continuation scheduling, shared engine/unwind integration, public tests and README. Root owns genuine fresh-identity JDK fixtures/oracles, independent traces/state/status assertions and the full integrated gate. Separate mutable worktrees are mandatory. No worker may broaden constructor/static-resolution or lifecycle scope to simplify a fixture.

Runtime tests must pin anchor and claim ordering, failed-constructor/no-claim behavior, permanent claim, profile/heap/registry identity, CV all numeric kinds including nonfinal and ignored instance attributes, raw B/C/S/Z values, unsupported String before plan publication, atomic batch installation, default predicate including private/bridge/synthetic versus static/abstract/clinit, interface-only behavior, declared-order diamonds/dedup, completed-interface-independent plans, exact node/edge/method/field/attribute/descriptor charges, remaining quotas, long shared descriptors, deep iterative graphs, corrupted plans/tokens and mutation-last final authority.

Interpreter tests must pin once-only completion across calls, recursive C->D->C initialization, constants before dependencies, superclass/default order, direct interface behavior, already-completed child-interface ancestor traversal, actual declaring field versus symbolic subclass, root owner initialization, b3 retained operands and b8 retained mixed arguments, exact fuel/no double replay, current clinit role and forbidden helper final writes, all cache-role checks, empty/no-body and deep pending chains, actual clinit handlers, escaping Error identity, non-Error wrapping refusal, dependent erroneous propagation, catching a dependency Error inside another clinit, later erroneous/aborted requests, host interruption before/after effects, cleanup on all direct failure paths and session drop/permanent claim. Exercise historical ()V ignored flags and explicit parameterized-descriptor rejection. Test zero/exact/MAX/overflow limits and root/caller provenance.

Root should use a fresh original JDK class-loader identity per scenario, then repeat successful requests within that same identity. Compare independent trace/state/results for all supported paths; constructors/reflection/loaders execute only in the oracle driver. Preserve original raw CV emitters/provenance so nonfinal and out-of-range subword constants are not hidden by javac or return narrowing. Supplied Error instances are explicit test inputs on Rust; no default object is described as a constructed VM error. Unsupported String/wrapping/NCDFE/host interruption cases need synthetic policy assertions rather than misleading Java-outcome equality claims.

Acceptance requires frozen source review, formatting, strict workspace/all-target Clippy, full debug and optimized tests, all prior/new JDK outcomes, unchanged old fixture hashes, canonical source/oracle validation and expanded parser smoke. Hosted evidence follows publication. This approves only the supported single-session procedure; it cannot close full M2, verification/loading constraints, library/VM exception construction, concurrent initialization, Java startup or the ultimate conformance/performance goal.

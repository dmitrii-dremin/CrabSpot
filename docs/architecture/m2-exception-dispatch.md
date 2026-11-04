# Explicit Throwable dispatch and abrupt completion

Status: approved by the technical director on 2026-11-04 after frozen source review and the coordinator's integrated gate. See `m2-exception-dispatch-review.md` for evidence and boundaries. Approval covers only the bounded supplied-Throwable profile below; full M2 remains open. No NanoSpec.

## Executable boundary

Add one opt-in runtime-aware profile for throwing existing, supplied Throwable instances, ordered Code handlers, rethrow and iterative cross-frame unwind. It reuses the existing body engine, MethodId cache, heap identities, reference calls and shared context. The existing entry points and ExecutionOutcome remain unchanged: they still reject Code handlers and athrow and retain their symbolic uncaught VM outcomes.

The new profile does not construct exception objects. Every currently symbolic VM-produced exception/error instead returns an explicit UnsupportedVmException host failure in this profile, including null athrow. It must never skip a handler, return a symbolic Java uncaught result or fabricate a default-filled Throwable to imitate materialization. This is a useful explicit-throw kernel, not complete Java exception support. Supplied instances have the same host-provided object-state boundary as other supplied kernel receivers/arguments; dispatch does not certify constructors, initialized library fields, messages or stack traces.

Throwable identity and precise abrupt completion follow [JVMS 2.10](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-2.html#jvms-2.10). The athrow path preserves the thrown identity, installs a matching handler with a one-reference operand stack, or unwinds to the caller. Null requires NullPointerException production, which this profile explicitly does not implement. [JVMS athrow](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.athrow)

Native/synchronized methods, monitors, async exceptions, Java thread/uncaught-handler startup, constructors/class initialization, general static resolution, full verification/loading constraints, GC and VM error materialization remain outside scope. No additional reference opcode or signature boundary changes except bf in the opt-in profile. Its effective opcode count is 192; the existing runtime-aware profile remains 191.

## Runtime API and identity

Do not add fields to ReferenceTypeUniverse or change its public struct-literal API. Export opaque, Clone + Copy + Debug + Eq + PartialEq types ExceptionProfile and ThrownException. Their fields are private and no unchecked public constructor exists. ExceptionProfile exposes throwable() -> ClassId. ThrownException exposes reference() -> Reference and class() -> ClassId.

ExceptionProfile retains the explicit Throwable ClassId and the validated registry/heap identity, admitted-layout count and bound ReferenceTypeUniverse fingerprint. It does not copy metadata or create persistent heap state. ThrownException retains that profile fingerprint, one checked reference and its actual ClassId. These are validation tokens, not construction or initialization certificates.

Add these FieldResolutionContext methods:

```rust
pub fn prepare_exception_profile(
    &mut self, throwable: ClassId,
) -> Result<ExceptionProfile, ExceptionAccessError>;
pub fn check_exception_profile(
    &mut self, profile: &ExceptionProfile,
) -> Result<(), ExceptionAccessError>;
pub fn prepare_throw(
    &mut self, profile: &ExceptionProfile, reference: Reference,
) -> Result<ThrownException, ExceptionAccessError>;
pub fn exception_handler_matches(
    &mut self, profile: &ExceptionProfile, origin: ClassId,
    catch_type: u16, thrown: &ThrownException,
) -> Result<bool, ExceptionAccessError>;
```

prepare_exception_profile checks unchanged context, then requires the existing bound reference universe. Validate the supplied registry/loader handle and admitted node, charge one node step and the complete actual name length before comparing it, then require exact java/lang/Throwable spelling, the designated bootstrap defining loader and a public ordinary nonabstract class. Charge one edge step before requiring its direct superclass to be the universe's exact Object ClassId. No resolver callback, class loading or implicit well-known-name identity is allowed. This anchor check does not certify the complete Throwable library ABI or require copying its interfaces/fields. Only after all checks succeed return the token; no heap mutation occurs.

check_exception_profile first checks the context's existing invalidation rules, then compares the token's registry/heap/layout/universe fingerprint and validates the retained Throwable handle. A token made for another heap, registry, layout snapshot or universe is rejected, including same-count replacement. A fresh context over the same unchanged heap/registry/universe may reuse it. This constant-size validation adds no lookup work; it does not rescan the name or hierarchy.

prepare_throw checks the profile first. Null is a NullThrowable host error in this service (the interpreter handles null athrow separately). Otherwise use checked ordinary-instance inspection to reject foreign/stale/missing handles and either array payload. Require the actual class to reach the explicit Throwable through the existing iterative class-only superclass relation. A false relation is InvalidThrownType. Preserve that relation's existing node/edge charges and do not widen protected-access subclass semantics. Return a token only after success. Supplied subclasses may have other defining loaders if the actual admitted superclass identities reach this Throwable.

exception_handler_matches checks profile validity, current-origin registry/admission and the thrown token's profile match, then rechecks the reference's actual instance identity against the token. A corrupted/mismatched token cannot bypass handle or class checks. No repeated Throwable ancestry proof is needed for an intact token under its unchanged snapshot.

ProfileMismatch denotes a supplied profile that does not belong to the current context snapshot. A thrown token with a different profile fingerprint or inconsistent retained actual class instead yields InvalidThrownToken; ordinary foreign/stale/missing reference failures retain their checked heap/instance diagnostics. This distinction is tested privately without exposing token fields.

For catch_type zero, return true without name resolution, ancestry work or callbacks. Otherwise require a Class constant with Utf8 name and reject array spelling as InvalidCatchConstant. Resolve through the searched method's declaring origin using the existing caller-access type-resolution service, full catch-name work charge even on binding hits, and existing symbol/unit/pre-callback limits. The result must be an ordinary class, not an interface, and must reach the explicit Throwable via the class-only relation; otherwise InvalidCatchType. Finally return whether the thrown class reaches the catch class through that same relation. No owned catch-name cache, new resolver or full graph copy is introduced.

Typed catches are resolved lazily, only for an in-range row actually examined before the first matching row. Catch-all skips resolution; out-of-range and later rows cannot trigger lookup, access failure or callbacks. Interface/unrelated catch types are host validation failures when reached, not fabricated VerifyError. This dynamic guard does not claim full eager verifier obligations. Class access uses current defining-loader/runtime-package identity; it is not the identity-only descriptor policy used for method arguments.

## Runtime failures and VM-produced categories

Export VmExceptionKind with exactly these eleven variants:

```rust
ArithmeticException, ClassCastException, ArrayStoreException,
NullPointerException, ArrayIndexOutOfBoundsException, NegativeArraySizeException,
NoSuchFieldError, IllegalAccessError, IncompatibleClassChangeError,
NoSuchMethodError, AbstractMethodError
```

Add ExceptionAccessError (Clone + Debug + Eq + PartialEq, Display + Error):

```rust
Context(FieldAccessError),
Type(Box<TypeAccessError>),
InvalidProfile { throwable: ClassId, reason: &'static str },
ProfileMismatch,
NullThrowable,
InvalidThrownType { expected: ClassId, actual: ClassId },
InvalidCatchConstant { origin: ClassId, index: u16 },
InvalidCatchType { origin: ClassId, index: u16, class: ClassId },
InvalidThrownToken,
Java(VmExceptionKind),
```

Registry/instance/heap failures retain their existing structured forms through Context; missing universe and reference-type failures use boxed Type. Preserve Error::source. The catch resolver translates only a direct IllegalAccess result from its caller-access resolution into Java(IllegalAccessError). Other Type failures stay boxed, including any nested Java category. No generic nested-error inference is allowed.

The interpreter maps every explicit VM category above to ExecutionErrorKind::UnsupportedVmException(kind) in the opt-in profile. This includes the existing Action outcomes, direct method_failure results inside call selection and a direct catch-resolution Java(IllegalAccessError). These paths must all be audited; method selection currently exits through a separate path from ordinary Action outcomes. No instance is allocated and no handler is searched for an unmaterialized category. Null athrow first performs its typed Reference pop, then returns UnsupportedVmException(NullPointerException).

This policy also applies when the current method has no handlers or a catch-all would otherwise match. It prevents a symbolic placeholder from being presented as either a caught object or an uncaught Java object. Already completed heap effects remain. All existing host validation, policy, malformed-code and unsupported-operation errors remain host failures and cannot be caught.

## Interpreter API and bounded preparation

Keep all existing signatures/outcome variants unchanged. Export:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExceptionOutcome {
    Returned(Option<Value>),
    Uncaught(Reference),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExceptionLimits {
    pub max_prepared_handlers: usize,
    pub max_dispatch_steps: u64,
}
// Default: max_prepared_handlers = 65_536; max_dispatch_steps = 1_000_000.
pub fn execute_instance_method_body_with_exceptions(
    context: &mut FieldResolutionContext<'_>,
    profile: &ExceptionProfile,
    method: MethodId, receiver: Reference, arguments: &[Value],
    options: ExecutionOptions, call_limits: CallLimits,
    exception_limits: ExceptionLimits,
) -> Result<ExceptionOutcome, ExecutionError>;
```

The new wrapper eagerly calls check_exception_profile, then enters the existing root path: frame-count gate, ordinary root/method eligibility, shape/Code preparation, computational arguments, reservations, receiver, reference profile/values and locals. Invalid profile wins before the root frame gate in this new API. Existing entry points have no new profile check or work charge. Host-selected interface/static roots and all previous method eligibility restrictions stay unchanged.

Use a mode/optional exception service in the existing loop and preparation path, not another interpreter. Internal outcome plumbing may distinguish legacy symbolic completion from actual thrown references; public legacy conversion must never invent an Uncaught value. The new wrapper can return only Returned or a checked actual Uncaught reference, and never one of the old symbolic categories.

Prepared handler rows must borrow the immutable original Code exception table, or retain only indices into that table; do not clone owned handler arrays or catch names per cached body. Add an aggregate prepared-handler counter once per successfully prepared body, independent of original Code-byte accounting. Even a borrowed large table has bounded validation work. Cache hits do not recount handlers or redo structural preparation.

Preserve legacy preparation order: any nonempty table still fails as unsupported before its old Code-byte/decode stage. In the new profile, retain existing method eligibility and Code-byte quota checks, eagerly decode all Code, then check the aggregate handler count by checked addition before scanning the table. Nonempty tables require max_stack >= 1. Validate every row before the body is cached: start_pc < end_pc <= code_length; start_pc and handler_pc are decoded instruction boundaries; end_pc is a boundary or exactly code_length; nonzero catch_type is a Class/Utf8 reference with non-array spelling. Embedded wide opcodes, switch padding and operand bytes are not boundaries. Preserve table order and duplicates; overlapping/nested rows are lawful.

No catch class is resolved during preparation. Commit the prepared-handler count only on successful preparation. Failure is a host error with target-method/call-site provenance and no body execution. Structural checking does not validate StackMap frames, operand types at handlers, unreachable catch ancestry or constructor state; these remain explicit verifier obligations.

Add ExecutionErrorKind variants:

```rust
ExceptionAccess(Box<ExceptionAccessError>),
UnsupportedVmException(VmExceptionKind),
InvalidExceptionHandler { index: usize, reason: &'static str },
ExceptionHandlerLimit { limit: usize, required: usize },
ExceptionDispatchLimit { limit: u64, required: u64 },
ExceptionAccountingOverflow { resource: &'static str },
```

Use InvalidExceptionHandler index zero for the nonempty-table/max_stack-zero guard. Check the aggregate count before this guard. Checked arithmetic overflow is distinct from an ordinary inclusive-limit failure; no saturating arithmetic or inferred Java OOM/StackOverflowError. New fallible allocation paths use structured failures. Existing unsupported/numeric diagnostics are not broadly renamed.

## Throw, search and unwind order

bf is enabled only in the new profile. Pop a typed Reference first. Null follows the unsupported-production rule; non-null calls prepare_throw with the supplied profile before dispatch. Wrong stack kind/underflow precedes identity validation. Do not reject non-null handles in a special interpreter helper before the runtime service's pinned order.

Maintain one checked thrown token through the iterative search. Start with the actual athrow PC in the current frame. For each frame, charge one dispatch step before inspecting it; then walk its original handler rows in order, charging one step before reading/range-checking each row, including uncovered rows. A row is active iff start_pc <= search_pc < end_pc. Only an active row calls exception_handler_matches; the first true match wins and later rows are untouched.

Dispatch steps are a separate per-evaluation u64 counter, shared by every throw/rethrow and every frame. Each charge checks counter + 1 and the inclusive maximum before changing the counter or performing its action. Runtime catch-name/ancestry work additionally uses the existing shared context policies. There is no hidden unbounded scan, enqueue or unwind outside these policies.

On a match, first ensure one-slot operand capacity and any required backing capacity with fallible reservation, then clear all operand values and slot accounting, push exactly the original reference and set PC to handler_pc. Locals and frame reservation remain unchanged. Handler installation performs no instruction-budget charge, but the next handler instruction consumes fuel normally. A malformed zero-capacity stack is rejected before clearing it; private defensive coverage must preserve this invariant even though preparation already rejects zero max_stack for handlers.

When a frame has no match, charge one additional dispatch step before popping it or releasing its slot reservation. This applies even to empty handler tables and to the root. Save the popped frame's origin, release its slots with checked arithmetic, then search the actual remaining caller at that saved invocation offset. Never use the caller's already advanced resume PC. Validate that the saved method key matches the actual caller; missing/inconsistent origin is an internal host error. The next frame incurs its own probe step. No return value is pushed while unwinding.

If no frame remains after the charged root unwind, return ExceptionOutcome::Uncaught with the same reference. This is host-visible kernel completion, not Java thread termination or uncaught-handler invocation. A caught exception may later be rethrown or replaced by another explicitly supplied instance; each bf consumes one instruction and begins a new dispatch using its current PC.

athrow consumes the ordinary one instruction-fuel unit. Handler search/unwind consumes dispatch/context work, not extra instruction fuel. A final fueled athrow may complete uncaught without another instruction; entering a handler does not waive its next instruction's fuel check. All previous heap writes/allocations survive caught, uncaught and host-failed dispatch. No new object/array allocation or reference issuer is introduced.

Search/work/catch-resolution/handler-install failures identify the searched method and current search PC, with that frame's invocation provenance. For a caller this is the invocation instruction at which its callee completed abruptly, not the original descendant throw offset and not the resume offset. prepare_throw failures identify the actual bf method/PC. Unsupported VM categories identify their originating instruction; catch-access failure identifies the searched row's frame/search PC. Do not pop a frame before the charge or resolution that may fail there.

## Ownership and acceptance

The runtime worker owns exception profile/token identities, checked throw/catch services, VmExceptionKind/ExceptionAccessError, narrow shared accessors, runtime tests and README. The interpreter worker owns the opt-in wrapper/outcome/limits, borrowed prepared rows, bf, single iterative dispatch path, every VM-category interception, stack installation, diagnostic mappings and interpreter tests/README. Root owns genuine fixtures, JDK oracle, integration/state checks and combined evidence. No classfile or decoder grammar change is needed.

Runtime coverage must include explicit bootstrap/name/root/loader/admission checks; foreign, replaced and same-count contexts; profile/token forgery defenses; null, primitive/reference arrays, stale/foreign/missing/unrelated throws; actual cross-loader subclasses; typed/current-origin access and identity; invalid interface/array/unrelated catch types; catch-all; full cache-hit names and pre-callback limits; deep class ancestry; and unchanged heap contents/child edges. Pin exact resource charges and private invalid-token guards without exposing constructors.

Interpreter coverage must include same-frame first-match/overlap/end-exclusive order, catches in callers at invocation rather than resume PCs, all four call forms, catch-all/finally/rethrow/replacement, returned and uncaught identity, locals preserved/operand stack replaced, deep iterative unwind, empty-table frame charging, exact/repeated dispatch/handler/fuel/frame/slot/code limits, cache hits, eager invalid boundaries and max_stack, lazy unresolved/inaccessible catches and provenance, and retained earlier effects. Test all eleven VM-produced categories as explicit host unsupported failures in this profile, including the direct method-selection route; catch-all must not convert them to fabricated objects. Test legacy modes unchanged, including handler/bf rejection and all previous symbolic outcomes. Nested Java-shaped host errors remain host.

Genuine fixtures should pass supplied Base/Child Throwable references through reference parameters/fields, catch and rethrow them across existing call forms, and compare numeric observations plus uncaught-reference identity. JDK constructors run only in the oracle driver; Rust's preseeded/default host state is described honestly. Do not use implicit VM exceptions as successful matching-oracle cases in this profile. Inspect emitted bf and actual exception-table rows, avoiding unsupported constructor/native/monitor execution in selected bodies. Preserve all 25,325 previous execution comparisons, separate 151+151 service cases, 92 fixtures and fifteen existing output sets.

Acceptance requires frozen technical review, formatting, strict workspace/all-target Clippy, all debug tests, optimized runtime/interpreter tests, canonical source preflight/Python regressions, fixture/reference reproduction and expanded parser smoke. Record new hosted Linux/Windows evidence after publication. This increment does not close materialization, verification, lifecycle, JVM startup, full M2, conformance or performance goals.

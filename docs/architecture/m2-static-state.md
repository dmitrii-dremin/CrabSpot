# Checked static state and explicit body access

Status: approved by the technical director on 2026-11-04 after frozen source review and the coordinator's integrated gate. See `m2-static-state-review.md`. Approval covers the checked static-memory/body profiles below, not complete preparation or initialization. Full M2 remains open. No NanoSpec.

## Scope and initialization boundary

Add unique checked static storage to each ManagedHeap, reusable runtime access services, and opt-in getstatic/putstatic execution in the existing selected-body engine. Compose this capability with the supplied-Throwable profile. The existing default-only StaticStorage and every existing execution entry point retain their contracts.

This is a host-controlled static-memory/body profile. It operates on explicitly allocated defaults and later checked writes. It does not initialize classes, apply ConstantValue, execute clinit, certify verification/loading constraints, or create a Prepared/Initialized lifecycle state. Selecting this profile explicitly chooses that memory-service boundary; do not advertise its b2/b3 behavior as complete Java active-use semantics. A class containing ConstantValue or clinit may have memory allocated without either being applied. Tests must distinguish those observations from ordinary Java initialization results.

The specification separates default static allocation from initialization and additionally imposes preparation-time loading constraints. Static accesses normally require initialization of the resolved declaring type. Final static writes require the declaring initializer. These responsibilities remain unimplemented here; all final writes are rejected because initializer execution is excluded. See [JVMS preparation](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.2), [initialization](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.5), [getstatic](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.getstatic) and [putstatic](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.putstatic).

No new static root selection, general cross-class invokestatic, constructor, class initializer, String materialization, thread synchronization or VM exception materialization is authorized. Existing same-class static helpers may use the supplied static service when reached inside the new profile. There is no public raw store, reset, lifecycle-success setter or final-write permission.

## Runtime ownership and public API

ManagedHeap owns at most one private static-state container. It is separate from array/instance arenas and their quotas. Dropping or moving the heap has the existing identity semantics; static values contain handles, not recursively owned objects. Keep existing HeapOptions, InstanceOptions, StaticStorageOptions and ReferenceTypeUniverse struct literals source-compatible. Do not retrofit mutation onto the old standalone StaticStorage.

Export:

```rust
pub struct StaticStateOptions {
    pub max_classes: usize,       // default 16_384
    pub max_slots: usize,         // default 65_536
    pub max_payload_bytes: usize, // default 8_388_608
}
pub const STATIC_STATE_SLOT_BYTES: usize = core::mem::size_of::<Value>();
pub struct StaticStateProfile { /* private checked identity/count fields */ }
```

Options are Clone + Copy + Debug + Eq + PartialEq. The profile is Clone + Copy + Debug + Eq + PartialEq, with read-only class_count(), slot_count() and payload_bytes() accessors. It retains registry identity, heap identity and the admitted-layout count represented by the static container, plus its immutable counts. No public unchecked constructor exists. It is a memory snapshot token, not an initialization certificate.

Add these FieldResolutionContext methods:

```rust
pub fn allocate_static_state(
    &mut self, options: StaticStateOptions,
) -> Result<StaticStateProfile, StaticAccessError>;
pub fn check_static_profile(
    &mut self, profile: &StaticStateProfile,
) -> Result<(), StaticAccessError>;
pub fn static_fields(
    &self, profile: &StaticStateProfile,
) -> Result<&[FieldId], StaticAccessError>;
pub fn inspect_static(
    &self, profile: &StaticStateProfile, field: FieldId,
) -> Result<Value, StaticAccessError>;
pub fn static_roots(
    &self, profile: &StaticStateProfile,
) -> Result<impl Iterator<Item = HeapRef> + '_, StaticAccessError>;
pub fn read_static(
    &mut self, profile: &StaticStateProfile, origin: ClassId, index: u16,
) -> Result<Value, StaticAccessError>;
pub fn write_static(
    &mut self, profile: &StaticStateProfile, origin: ClassId,
    index: u16, value: Value,
) -> Result<(), StaticAccessError>;
```

Read-only inspection by FieldId is deliberate host introspection, not a Java access-control bypass for writes. static_roots enumerates one HeapRef for each non-null reference location, in static_fields order, preserving duplicate aliases. It allocates nothing, follows no child edges and yields identities rather than mutable root locations. ManagedHeap::children remains per-object/array edges. Neither API claims a synchronized collector snapshot.

A static profile is independent of whether a reference universe was bound when allocation occurred: defaults need no universe. The existing context still checks its captured universe on every operation. If the universe is first bound later, use a fresh context; the same heap-bound static profile remains usable. Non-null writes always use the current checked universe. Existing rules prevent replacing a bound universe with unrelated identities.

Static allocation or value writes do not change FieldResolutionContext::unchanged or invalidate existing exception/type/method snapshots. Extending after admitted-layout growth does not introduce another invalidation cause: that prior graph growth already requires fresh contexts and exception profiles under their existing rules. No independent static revision is needed because publication only adds previously unrepresented ClassIds, never replaces a same-membership container; the admitted count and heap identity distinguish each supported profile generation. Retain zero-static classes explicitly so this invariant holds.

## Allocation, extension and resource accounting

Allocate locations for every declared static field of every currently admitted class/interface, including abstract types, interfaces, zero-field types and multiple admitted roots. Each FieldId gets exactly one logical slot, including Long/Double; inheritance never duplicates a location. All primitives receive positive zero bits and all references receive null. Ignore ConstantValue attributes and clinit bodies. Descriptors are classified without resolving referenced types or copying nominal names per field.

The first successful allocation captures options permanently. Repeating allocation with identical options and unchanged admitted layouts returns the same profile/counts without resetting values, scanning fields or consuming lookup work. Different options return OptionsMismatch before new quota/metadata work. A new context over the same unchanged heap can use the profile. Heap replacement, another registry or another heap cannot.

Admitted-layout growth invalidates old profiles and old contexts. A fresh context may call allocate_static_state with the original options to extend storage atomically. Preserve all old locations, values and root order; append only newly admitted classes, sorted by registry declaration index, and each new class's static fields in declaration order. Initial allocation uses the same order. Previously admitted classes must still exist with immutable registry identities; impossible shrinkage or corrupt retained metadata is InvalidState. No general removal/reordering/reset is implemented. A failed extension preserves the old container exactly, though its old profile remains unusable against the enlarged graph until extension succeeds.

Order and bounds are fixed:

1. Check context validity. Inspect existing container identity/options and take the unchanged idempotent path if applicable. Check target admitted-class count against max_classes before allocating an inventory.
2. Atomically charge the full target class count to the shared lookup counter before collecting/sorting ClassIds. This covers inventory examination, including existing classes during extension. Fallibly reserve only actual bounded inventory size; do not allocate from a policy maximum.
3. For each new class in sorted order, validate registry/admission identity. Charge one lookup step before examining each declared field, including instance fields skipped by this service. For each static field, check the cumulative logical-slot limit, then checked total slot bytes and max_payload_bytes, before staging a location.
4. For each distinct (ClassId, descriptor_index) among new static fields, charge the complete descriptor UTF-16 length atomically before classifying it. Reuse the compact kind within this allocation operation. A shared long descriptor is not scanned/copied per field. Existing classes/locations are not reclassified on extension. Malformed private metadata is an explicit error.
5. Stage all new metadata/default values and reserve all destination capacity fallibly. Validate retained prefix/count/position invariants before publication. Publish only after every check/reservation succeeds; no fallible operation may follow the first published slot. Failure leaves values, field order, roots, counts and active container unchanged. Work already spent and any harmless capacity reserve need not be rolled back.

The static payload account is exactly logical_slots * STATIC_STATE_SLOT_BYTES, checked for overflow. It excludes allocator spare capacity, identity maps and metadata. Class and slot limits independently bound retained metadata; they are not a claim that payload bytes measure all process memory. Array/instance counts, element totals and payload quotas do not change when static storage is allocated or extended. Resource arithmetic overflow returns AccountingOverflow, not a wrapped/saturated success. All reserves are fallible, including staging vectors, maps and destination growth.

Profile checks compare constant-size fingerprints/counts after context validity and consume zero lookup work. A missing container or mismatched profile is ProfileMismatch. Successful object/array allocations and writes do not invalidate the static profile. There is no traversal of inherited field lists and no recursive drop/traversal.

## Resolution, access, stores and error order

Reuse the existing field selector and its interface-before-superclass lookup, binding aliases, loader-scoped runtime packages, class/member access, protected-static behavior and conservative nominal cross-loader constraint. Expose only narrow crate-private helpers; do not duplicate or relax the instance-field selector or alter its existing budgets. No instance receiver check belongs on a static access.

read_static order is profile/context check; current origin/Fieldref structural validation and ordinary shared selection; static-category check; declaring FieldId location/invariant check; read. An instance declaration produces direct Java IncompatibleClassChange. The selected declaring ClassId owns the location even when the symbolic owner is a subclass/interface or another admitted root. A stored non-null reference is checked for actual heap-handle validity before returning it; a corrupt stored value is a host error, not a manufactured Java exception.

write_static uses the same order through selection, then static-category check, then final rejection as direct Java IllegalAccess, then location/kind validation. Only after those checks validate the supplied value. Numeric stores use the established field semantics: B/S signed truncation, C unsigned truncation, Z low-bit masking, I/J exact value, F/D exact raw bits. Wrong computational kinds are host ValueKindMismatch errors. Null is accepted only for a reference location and requires no target resolution/universe.

For a non-null reference into a reference field, first validate its actual source type/heap identity using the existing type service, then resolve the complete selected declaration's descriptor under its declaring loader with identity-only access, then check assignment. Do not impose caller accessibility on a descriptor type. Preserve the existing full descriptor/name/symbol/work charges and conservative cross-loader field restriction even when the assigned value is null. An incompatible assignment is a host diagnostic containing field/source/target, not ArrayStoreException or a fabricated verifier outcome.

A narrow private commit rechecks profile/container, registry FieldId and static/final/category/location invariants, computational kind and non-null source handle before changing one value. The context is the sole caller after assignment proof; no resolver, allocation or other fallible operation follows mutation. This commit is not publicly exposed. Failure at any earlier stage preserves all static and heap values/edges, while already consumed work or retained successful resolver bindings remain visible. Reuse compact field normalization helpers where appropriate without widening public mutation authority.

static_fields/static_roots check context/profile first. inspect_static additionally validates the registry FieldId, admitted declaring class, static category and retained position; an ordinary instance field has no static location. Inspection does not resolve symbols or enforce Java caller access and consumes zero lookup work. Read validation must not silently filter or repair corrupt values.

Export StaticResource::{Classes, Slots, PayloadBytes}, StaticJavaError::{NoSuchField, IllegalAccess, IncompatibleClassChange}, and StaticAccessError with these variants:

```rust
Context(FieldAccessError),
Type(Box<TypeAccessError>),
Heap(HeapError),
Java(StaticJavaError),
ProfileMismatch,
OptionsMismatch,
InvalidState(&'static str),
MissingLocation { field: FieldId },
ValueKindMismatch { field: FieldId, expected: ValueKind, actual: ValueKind },
IncompatibleReference(Box<ReferenceFieldMismatch>),
ResourceLimit { resource: StaticResource, limit: usize, required: usize },
AccountingOverflow { resource: StaticResource },
AllocationFailure { resource: &'static str },
```

Use Context(Registry/Instance/InvalidState) for those existing metadata failures. Convert only direct Java errors returned by the actual shared selector to the corresponding direct StaticJava variant; a nested Java-shaped Context/Type/Heap error remains host-side. Preserve Display and Error::source chains. Adding this independent wrapper must not create an infinitely sized FieldAccess/TypeAccess cycle.

## Composable interpreter bridge

Export two additional APIs; preserve all existing signatures and gates:

```rust
pub fn execute_instance_method_body_with_static_state(
    context: &mut FieldResolutionContext<'_>,
    statics: &StaticStateProfile,
    method: MethodId, receiver: Reference, arguments: &[Value],
    options: ExecutionOptions, call_limits: CallLimits,
) -> Result<ExecutionOutcome, ExecutionError>;

pub fn execute_instance_method_body_with_static_state_and_exceptions(
    context: &mut FieldResolutionContext<'_>,
    statics: &StaticStateProfile, exceptions: &ExceptionProfile,
    method: MethodId, receiver: Reference, arguments: &[Value],
    options: ExecutionOptions, call_limits: CallLimits,
    exception_limits: ExceptionLimits,
) -> Result<ExceptionOutcome, ExecutionError>;
```

At entry, check the static profile first; in the composed API check the exception profile second; then follow the existing root method/shape/arguments/receiver/frame pipeline unchanged. No initializer runs. Add an optional static capability to the one existing engine; do not copy frame loops, method caches or exception dispatch. All nested static helpers and b6/b7/b9 targets share the same state/profile, while b2/b3 resolve from each current method's declaring ClassId/CP. The existing APIs reject b2/b3 even when their heap happens to contain allocated static state.

For b2, require the new capability, then call read_static with current origin/index and push the result using actual category width. For b3, require the capability, pop one Value with pop_any, then call write_static. Stack underflow therefore precedes service selection; wrong value kind is checked by the service after resolution/access/static/final/location checks. Do not pop a receiver. Existing eager code decoding and other opcode/version validation remain unchanged. Each b2/b3 consumes one ordinary instruction-fuel unit; no new per-instruction static counter exists.

Add ExecutionErrorKind::StaticAccess(Box<StaticAccessError>). Only direct StaticJava variants enter the existing symbolic outcome mapper. In the static-only API they become the established NoSuchFieldError/IllegalAccessError/IncompatibleClassChangeError outcomes. In the composed supplied-Throwable API they become UnsupportedVmException before handler search, exactly like other VM-produced outcomes. Nested Java-shaped wrappers remain StaticAccess host errors and are never caught. Actual supplied athrow still uses the existing ordered handler/unwind path, including after completed static writes.

Preserve current-method/offset and established call-site provenance for static failures, category-two push overflow, late descriptor/assignment failures and post-call/handler operations. Successful earlier static writes persist after Java symbolic outcomes, supplied uncaught references, host failures, exhausted fuel and failed later stores. Reads/writes do not create objects. Old runtime-aware and exception-only profiles remain at 191 and 192 effective opcodes; static-only and composed profiles respectively support 193 and 194. Opcode counts are not lifecycle-conformance claims.

## Worker ownership and acceptance

Runtime worker: private static container/profile/options/errors, heap integration, context allocation/extension/inspection/access, shared-selector/helper integration, public/private runtime tests and runtime README. Preserve old standalone StaticStorage behavior. Interpreter worker: optional service capability and two entry points, b2/b3, explicit error mapping, shared-engine tests and README. Root: original fixtures/oracle/state assertions, integration, full gate and publication. Use isolated mutable worktrees; no shared worker source edits.

Required runtime coverage includes all defaults and raw primitive stores; final/interface refusal; hidden/inherited/static-versus-instance selection; package/loader/private/protected access; aliases and delegated symbols; numeric versus nominal cross-loader boundaries; inaccessible descriptor types; null and non-null assignment across nominal/primitive/reference/multidimensional arrays; exact duplicate root locations; foreign/stale/missing handles; zero/exact/MAX and overflow policies; independent heap quotas; and ConstantValue/clinit unapplied observations.

Allocation tests must establish idempotence without work/reset, options mismatch, atomic extension with old values/roots preserved, old-profile rejection after growth, same-size heap replacement, fresh-context reuse, first universe binding, all field/class/descriptor charges and preallocation limits. Include a long shared descriptor, a deep admitted hierarchy with only declared-location storage, late quota/work/reservation failures, corrupt position/counter/profile guards, and commit failures before mutation. A failed extension must permit a later valid retry without losing existing writes.

Interpreter coverage must exercise both new APIs, all call families and declaring origins, primitive/reference transfer, retrieved static references used by fields/arrays/casts/calls, exact category-two widths and limits, b3 extraction order, old-mode rejection, nested mapping, and state persistence after successful catches, uncaught supplied identities and each relevant Java/host failure. A catch-all must not catch UnsupportedVmException from a static error. No class-init trigger or final-write success test belongs in this profile.

Root's genuine driver should reset mutable static fields to the chosen initial state before each differential case; the Rust side allocates fresh default state. Compare original numeric/identity outcomes and independently inspect declarations, all retained values, root order/aliases and unrelated heap totals. Use separately labeled metadata/memory assertions for ConstantValue/clinit remaining unapplied, not misleading differential initialization claims. Raw original bytecode is appropriate for B/C/S/Z stores and final/static-category errors that javac cannot emit directly.

Acceptance requires frozen architecture review, formatting, strict workspace/all-target Clippy, full debug and optimized runtime/interpreter tests, all prior/new JDK outcomes, unchanged old fixture hashes, canonical source/oracle checks and the expanded parser smoke. Hosted Linux/Windows evidence is recorded after publication. Neither successful access nor allocation closes full M2, complete preparation, initialization, Java startup or the ultimate conformance/performance goal.

## Subsequent lifecycle responsibilities

This increment supplies mutable static locations needed by a later initialization engine. That engine must separately model required verification/loading constraints, ConstantValue including real String/interning support, dependency order, recursive use and initialization ownership, actual clinit execution, failure/erroneous-state rules and thread synchronization. Host interruption after partial effects needs an explicit policy, not an automatic retry or an invented successful initialization. VM exception materialization remains a prerequisite wherever the lifecycle contract needs real newly produced Throwable identities. No lifecycle transition or constructor protocol is authorized here.

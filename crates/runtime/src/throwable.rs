//! Checked Throwable ABI and privileged structural trace storage. The interpreter
//! owns historical stack selection, lifecycle and successful construction truth.
use crate::initialization::InitializationFingerprint;
use crate::*;
use crabspot_classfile::Member;
use std::fmt;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThrowableDiscoveryLimits {
    pub max_work_units: u64,
}
impl Default for ThrowableDiscoveryLimits {
    fn default() -> Self {
        Self {
            max_work_units: 4_194_304,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThrowableDiscoveryUsage {
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceLimits {
    pub max_snapshots: u64,
    pub max_frames: u64,
    pub max_frames_per_snapshot: u64,
    pub max_work_units: u64,
}
impl Default for TraceLimits {
    fn default() -> Self {
        Self {
            max_snapshots: 65_536,
            max_frames: 1_048_576,
            max_frames_per_snapshot: 65_536,
            max_work_units: 33_554_432,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TraceUsage {
    pub snapshots: u64,
    pub frames: u64,
    pub backtraces: u64,
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceFrame {
    pub method: MethodId,
    pub pc: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceMethodKind {
    Other,
    FillInStackTrace,
    Constructor,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThrowableResource {
    DiscoveryWork,
    TraceWork,
    Snapshots,
    Frames,
    FramesPerSnapshot,
    PayloadBytes,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThrowableErrorKind {
    UnsupportedShape,
    Disabled,
    PolicyConflict,
    InvalidBinding(&'static str),
    InvalidState(&'static str),
    InvalidSnapshot,
    InvalidCapture,
    InvalidReceiver,
    UnsupportedProducer(VmExceptionKind),
    InvalidFrame {
        method: MethodId,
        pc: u32,
    },
    UnrepresentableDepth {
        required: u64,
    },
    Limit {
        resource: ThrowableResource,
        limit: u64,
        required: u64,
    },
    Overflow {
        resource: ThrowableResource,
    },
    AllocationFailure,
    Registry(Box<RegistryError>),
    Context(Box<FieldAccessError>),
    Heap(Box<HeapError>),
    Instance(Box<InstanceError>),
    Construction(Box<ConstructionAccessError>),
    Initialization(Box<InitializationAccessError>),
    Exception(Box<ExceptionAccessError>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThrowableAccessError {
    pub kind: ThrowableErrorKind,
    pub discovery_usage: ThrowableDiscoveryUsage,
}
impl fmt::Display for ThrowableErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            Self::Context(e) => e.fmt(f),
            Self::Heap(e) => e.fmt(f),
            Self::Instance(e) => e.fmt(f),
            Self::Construction(e) => e.fmt(f),
            Self::Initialization(e) => e.fmt(f),
            Self::Exception(e) => e.fmt(f),
            e => write!(f, "Throwable service: {e:?}"),
        }
    }
}
impl fmt::Display for ThrowableAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
impl std::error::Error for ThrowableErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e.as_ref()),
            Self::Context(e) => Some(e.as_ref()),
            Self::Heap(e) => Some(e.as_ref()),
            Self::Instance(e) => Some(e.as_ref()),
            Self::Construction(e) => Some(e.as_ref()),
            Self::Initialization(e) => Some(e.as_ref()),
            Self::Exception(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl std::error::Error for ThrowableAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.source()
    }
}
pub(crate) type R<T> = Result<T, Box<ThrowableAccessError>>;
pub(crate) fn error(kind: ThrowableErrorKind) -> Box<ThrowableAccessError> {
    Box::new(ThrowableAccessError {
        kind,
        discovery_usage: Default::default(),
    })
}
macro_rules! from_error {
    ($t:ty,$v:ident) => {
        impl From<$t> for Box<ThrowableAccessError> {
            fn from(e: $t) -> Self {
                error(ThrowableErrorKind::$v(Box::new(e)))
            }
        }
    };
}
from_error!(RegistryError, Registry);
from_error!(FieldAccessError, Context);
from_error!(HeapError, Heap);
from_error!(InstanceError, Instance);
from_error!(ConstructionAccessError, Construction);
from_error!(InitializationAccessError, Initialization);
from_error!(ExceptionAccessError, Exception);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThrowablePublicRoles {
    pub exception: ClassId,
    pub runtime_exception: ClassId,
    pub arithmetic_exception: ClassId,
    pub null_pointer_exception: ClassId,
    pub assertion_error: ClassId,
    pub stack_trace_element: ClassId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Key(u16, u16, u16);
impl Key {
    pub(crate) fn of(m: &Member) -> Self {
        Self(m.name_index, m.descriptor_index, m.access_flags)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThrowablePlan {
    pub(crate) registry: u64,
    pub(crate) boot: LoaderId,
    pub(crate) throwable: ClassId,
    pub(crate) object: ClassId,
    pub(crate) public: ThrowablePublicRoles,
    pub(crate) roots: [ClassId; 4],
    pub(crate) native: MethodId,
    pub(crate) native_key: Key,
    pub(crate) fields: [FieldId; 2],
    pub(crate) field_keys: [Key; 2],
    pub(crate) ordinals: [usize; 2],
    pub(crate) ctors: [MethodId; 8],
    pub(crate) ctor_keys: [Key; 8],
}
impl ThrowablePlan {
    pub fn private_roots(&self) -> &[ClassId] {
        &self.roots
    }
    pub fn public_roles(&self) -> ThrowablePublicRoles {
        self.public
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThrowableDiscovery {
    pub plan: Option<ThrowablePlan>,
    pub usage: ThrowableDiscoveryUsage,
}
#[path = "throwable_probe.rs"]
mod probe;
pub use probe::probe_throwable_support;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Fingerprint {
    registry: u64,
    heap: u64,
    classes: usize,
    universe: ReferenceTypeUniverse,
    throwable: ClassId,
}
#[derive(Debug)]
pub(crate) struct ThrowableState {
    pub(crate) plan: ThrowablePlan,
    pub(crate) slots: [usize; 2],
    pub(crate) fingerprint: Fingerprint,
    limits: TraceLimits,
    pub(crate) usage: TraceUsage,
    #[cfg(test)]
    pub(crate) fail_reserve_after: Option<usize>,
}
impl ThrowableState {
    fn work(&mut self, n: u64) -> R<()> {
        let required = checked(
            self.usage.work_units,
            n,
            self.limits.max_work_units,
            ThrowableResource::TraceWork,
        )?;
        self.usage.work_units = required;
        Ok(())
    }
    pub(crate) fn reserving(&mut self) -> R<()> {
        #[cfg(test)]
        if let Some(left) = &mut self.fail_reserve_after {
            if *left == 0 {
                return Err(error(ThrowableErrorKind::AllocationFailure));
            }
            *left -= 1;
        }
        Ok(())
    }
}
fn checked(current: u64, add: u64, limit: u64, resource: ThrowableResource) -> R<u64> {
    let required = current
        .checked_add(add)
        .ok_or_else(|| error(ThrowableErrorKind::Overflow { resource }))?;
    if required > limit {
        return Err(error(ThrowableErrorKind::Limit {
            resource,
            limit,
            required,
        }));
    }
    Ok(required)
}
#[derive(Debug)]
pub struct TraceSnapshotBuilder {
    fingerprint: Fingerprint,
    claim: InitializationFingerprint,
    expected: usize,
    frames: Vec<TraceFrame>,
}
#[derive(Debug)]
pub struct TraceSnapshot {
    expected: usize,
    fingerprint: Fingerprint,
    claim: InitializationFingerprint,
    frames: Vec<TraceFrame>,
}
#[derive(Debug)]
pub struct ThrowableCaptureAuthority {
    fingerprint: Fingerprint,
    claim: InitializationFingerprint,
    receiver: Reference,
    actual: ClassId,
    slots: [usize; 2],
}
#[derive(Debug)]
pub struct VmExceptionSpec {
    fingerprint: Fingerprint,
    claim: InitializationFingerprint,
    class: ClassId,
    constructor: MethodId,
    key: Key,
}
impl VmExceptionSpec {
    pub fn class(&self) -> ClassId {
        self.class
    }
    pub fn constructor(&self) -> MethodId {
        self.constructor
    }
    pub fn message_units(&self) -> &[u16] {
        &[47, 32, 98, 121, 32, 122, 101, 114, 111]
    }
}
#[derive(Debug)]
pub(crate) struct Backtrace {
    pub(crate) fingerprint: Fingerprint,
    pub(crate) owner: HeapRef,
    pub(crate) actual: ClassId,
    pub(crate) frames: Vec<TraceFrame>,
}
fn equality_error(e: Box<StringBodyError>) -> Box<ThrowableAccessError> {
    FieldAccessError::StringBody(e).into()
}
impl ManagedHeap {
    pub fn bind_throwable_support(
        &mut self,
        registry: &DefinitionRegistry,
        plan: &ThrowablePlan,
        limits: TraceLimits,
    ) -> R<()> {
        if registry.identity() != plan.registry {
            return Err(error(ThrowableErrorKind::InvalidBinding(
                "Throwable plan registry differs",
            )));
        }
        self.equality_affinity(registry).map_err(equality_error)?;
        let u = self
            .reference_type_universe()
            .ok_or_else(|| error(ThrowableErrorKind::Disabled))?;
        if u.object != plan.object
            || u.bootstrap_loader != plan.boot
            || self.instance_graph_registry()? != Some(plan.registry)
        {
            return Err(error(ThrowableErrorKind::InvalidBinding(
                "Throwable universe differs",
            )));
        }
        for id in [
            plan.throwable,
            plan.object,
            plan.public.exception,
            plan.public.runtime_exception,
            plan.public.arithmetic_exception,
            plan.public.null_pointer_exception,
            plan.public.assertion_error,
            plan.public.stack_trace_element,
            plan.roots[0],
            plan.roots[1],
            plan.roots[2],
            plan.roots[3],
        ] {
            registry.class(id)?;
            self.instance_graph_node(id)?;
            if registry.defining_loader(id)? != plan.boot {
                return Err(error(ThrowableErrorKind::InvalidBinding(
                    "Throwable role loader differs",
                )));
            }
        }
        for (child, parent) in [
            (plan.throwable, plan.object),
            (plan.public.exception, plan.throwable),
            (plan.public.runtime_exception, plan.public.exception),
            (
                plan.public.arithmetic_exception,
                plan.public.runtime_exception,
            ),
        ] {
            if self.instance_graph_node(child)?.superclass != Some(parent) {
                return Err(error(ThrowableErrorKind::InvalidBinding(
                    "Throwable role parent differs",
                )));
            }
        }
        if Key::of(registry.method(plan.native)?) != plan.native_key
            || plan.native.owner() != plan.throwable
        {
            return Err(error(ThrowableErrorKind::InvalidBinding(
                "Throwable native fingerprint differs",
            )));
        }
        for (id, key) in plan.fields.iter().zip(plan.field_keys) {
            if id.owner() != plan.throwable || Key::of(registry.field(*id)?) != key {
                return Err(error(ThrowableErrorKind::InvalidBinding(
                    "Throwable field fingerprint differs",
                )));
            }
        }
        let ctor_owners = [
            plan.throwable,
            plan.throwable,
            plan.throwable,
            plan.throwable,
            plan.public.exception,
            plan.public.runtime_exception,
            plan.public.arithmetic_exception,
            plan.public.arithmetic_exception,
        ];
        for ((id, key), owner) in plan.ctors.iter().zip(plan.ctor_keys).zip(ctor_owners) {
            if id.owner() != owner || Key::of(registry.method(*id)?) != key {
                return Err(error(ThrowableErrorKind::InvalidBinding(
                    "Throwable constructor fingerprint differs",
                )));
            }
        }
        let slots = self.throwable_binding_slots(plan.fields, plan.ordinals)?;
        let fingerprint = Fingerprint {
            registry: plan.registry,
            heap: self.identity,
            classes: self.retained_layout_classes(),
            universe: u,
            throwable: plan.throwable,
        };
        if let Some(old) = &self.throwables {
            if old.plan == *plan
                && old.fingerprint == fingerprint
                && old.limits == limits
                && old.slots == slots
            {
                return Ok(());
            }
            return Err(error(ThrowableErrorKind::PolicyConflict));
        }
        if !self.entries.is_empty() || self.initialization_claim.is_some() {
            return Err(error(ThrowableErrorKind::InvalidBinding(
                "first Throwable binding requires pristine unclaimed arena",
            )));
        }
        self.throwables = Some(ThrowableState {
            plan: plan.clone(),
            slots,
            fingerprint,
            limits,
            usage: Default::default(),
            #[cfg(test)]
            fail_reserve_after: None,
        });
        Ok(())
    }
    pub fn throwable_trace_usage(&self) -> TraceUsage {
        self.throwables
            .as_ref()
            .map_or(Default::default(), |s| s.usage)
    }
    pub(crate) fn throwable_field_is_native(&self, field: FieldId) -> bool {
        self.throwables
            .as_ref()
            .is_some_and(|s| s.plan.fields.contains(&field))
    }
    fn throwable_state(&self) -> R<&ThrowableState> {
        self.throwables
            .as_ref()
            .ok_or_else(|| error(ThrowableErrorKind::Disabled))
    }
    pub(crate) fn trace_from_pair(
        &self,
        receiver: Reference,
        actual: ClassId,
        pair: (Reference, i32),
    ) -> R<Option<&[TraceFrame]>> {
        let state = self.throwable_state()?;
        let (owner, depth) = pair;
        if owner.is_null() {
            if depth != 0 {
                return Err(error(ThrowableErrorKind::InvalidState(
                    "null backtrace has nonzero depth",
                )));
            }
            return Ok(None);
        }
        let crate::heap::EntryPayload::Backtrace(trace) = &self.entry(owner)?.payload else {
            return Err(error(ThrowableErrorKind::InvalidState(
                "backtrace field has wrong payload",
            )));
        };
        if trace.owner != receiver.heap_ref().ok_or(HeapError::NullReference)?
            || trace.actual != actual
            || trace.fingerprint != state.fingerprint
            || i32::try_from(trace.frames.len()).ok() != Some(depth)
        {
            return Err(error(ThrowableErrorKind::InvalidState(
                "backtrace reciprocal owner or depth differs",
            )));
        }
        Ok(Some(&trace.frames))
    }
}
impl FieldResolutionContext<'_> {
    pub fn check_throwable_profile(&self) -> R<()> {
        self.unchanged()?;
        self.check_class_mirror_profile()
            .map_err(|e| Box::<ThrowableAccessError>::from(FieldAccessError::ClassMirror(e)))?;
        self.heap
            .equality_affinity(self.registry)
            .map_err(equality_error)?;
        let s = self.heap.throwable_state()?;
        if s.fingerprint.registry != self.registry.identity()
            || s.fingerprint.heap != self.heap.identity
            || s.fingerprint.classes != self.heap.retained_layout_classes()
            || Some(s.fingerprint.universe) != self.heap.reference_type_universe()
        {
            return Err(error(ThrowableErrorKind::InvalidBinding(
                "Throwable context affinity differs",
            )));
        }
        Ok(())
    }
    fn trace_claim(&mut self, claim: &InitializationClaim) -> R<Fingerprint> {
        self.check_initialization_claim(claim)?;
        self.check_throwable_profile()?;
        Ok(self.heap.throwable_state()?.fingerprint)
    }
    fn trace_work(&mut self, n: u64) -> R<()> {
        self.heap
            .throwables
            .as_mut()
            .ok_or_else(|| error(ThrowableErrorKind::Disabled))?
            .work(n)
    }
    pub fn throwable_trace(&mut self, receiver: Reference) -> R<Option<&[TraceFrame]>> {
        self.check_throwable_profile()?;
        let fields = self.heap.throwable_state()?.plan.fields;
        let (actual, slots) =
            self.heap
                .throwable_pair_slots(receiver, fields, &mut self.steps, self.options)?;
        let pair = self.heap.throwable_pair(receiver, actual, slots)?;
        self.heap.trace_from_pair(receiver, actual, pair)
    }
    pub fn is_throwable_fill_in_stack_trace(&mut self, actual: MethodId) -> R<bool> {
        self.unchanged()?;
        self.registry.method(actual)?;
        let Some(s) = &self.heap.throwables else {
            return Ok(false);
        };
        if actual.owner() != s.plan.throwable {
            return Ok(false);
        }
        let found = actual == s.plan.native;
        self.step()?;
        self.check_throwable_profile()?;
        Ok(found)
    }
    pub fn prepare_throwable_capture(
        &mut self,
        claim: &InitializationClaim,
        actual: MethodId,
        receiver: Reference,
    ) -> R<ThrowableCaptureAuthority> {
        let fingerprint = self.trace_claim(claim)?;
        let s = self.heap.throwable_state()?;
        if actual != s.plan.native || Key::of(self.registry.method(actual)?) != s.plan.native_key {
            return Err(error(ThrowableErrorKind::InvalidCapture));
        }
        let fields = s.plan.fields;
        let (class, slots) =
            self.heap
                .throwable_pair_slots(receiver, fields, &mut self.steps, self.options)?;
        let pair = self.heap.throwable_pair(receiver, class, slots)?;
        self.heap.trace_from_pair(receiver, class, pair)?;
        Ok(ThrowableCaptureAuthority {
            fingerprint,
            claim: claim.fingerprint(),
            receiver,
            actual: class,
            slots,
        })
    }
    pub fn trace_method_kind(
        &mut self,
        claim: &InitializationClaim,
        method: MethodId,
    ) -> R<TraceMethodKind> {
        self.trace_claim(claim)?;
        self.trace_work(1)?;
        self.heap.instance_graph_node(method.owner())?;
        let member = self.registry.method(method)?;
        let c = self.registry.class(method.owner())?;
        self.trace_work(1)?;
        let name = c
            .constant_pool
            .utf8(member.name_index)
            .map_err(RegistryError::Parse)?;
        for (n, kind) in [
            ("fillInStackTrace", TraceMethodKind::FillInStackTrace),
            ("<init>", TraceMethodKind::Constructor),
        ] {
            if name.units().len() == n.len() {
                self.trace_work(n.len() as u64)?;
                if name.equals_ascii(n) {
                    return Ok(kind);
                }
            }
        }
        Ok(TraceMethodKind::Other)
    }
    pub fn begin_trace_snapshot(
        &mut self,
        claim: &InitializationClaim,
        retained_frames: usize,
    ) -> R<TraceSnapshotBuilder> {
        let fingerprint = self.trace_claim(claim)?;
        self.trace_work(1)?;
        let n = u64::try_from(retained_frames).map_err(|_| {
            error(ThrowableErrorKind::Overflow {
                resource: ThrowableResource::Frames,
            })
        })?;
        if n > i32::MAX as u64 {
            return Err(error(ThrowableErrorKind::UnrepresentableDepth {
                required: n,
            }));
        }
        let s = self.heap.throwable_state()?;
        checked(
            0,
            n,
            s.limits.max_frames_per_snapshot,
            ThrowableResource::FramesPerSnapshot,
        )?;
        let snapshots = checked(
            s.usage.snapshots,
            1,
            s.limits.max_snapshots,
            ThrowableResource::Snapshots,
        )?;
        let frames = checked(
            s.usage.frames,
            n,
            s.limits.max_frames,
            ThrowableResource::Frames,
        )?;
        let bytes = retained_frames
            .checked_mul(std::mem::size_of::<TraceFrame>())
            .ok_or_else(|| {
                error(ThrowableErrorKind::Overflow {
                    resource: ThrowableResource::PayloadBytes,
                })
            })?;
        crate::heap::increased(
            self.heap.bytes,
            bytes,
            self.heap.options.max_payload_bytes,
            HeapResource::PayloadBytes,
        )?;
        let s = self
            .heap
            .throwables
            .as_mut()
            .ok_or_else(|| error(ThrowableErrorKind::Disabled))?;
        s.usage.snapshots = snapshots;
        s.usage.frames = frames;
        let mut values = Vec::new();
        if retained_frames != 0 {
            s.reserving()?;
            values
                .try_reserve_exact(retained_frames)
                .map_err(|_| error(ThrowableErrorKind::AllocationFailure))?;
        }
        Ok(TraceSnapshotBuilder {
            fingerprint,
            claim: claim.fingerprint(),
            expected: retained_frames,
            frames: values,
        })
    }
    pub fn append_trace_frame(
        &mut self,
        claim: &InitializationClaim,
        builder: &mut TraceSnapshotBuilder,
        frame: TraceFrame,
    ) -> R<()> {
        let fingerprint = self.trace_claim(claim)?;
        self.trace_work(1)?;
        if builder.fingerprint != fingerprint
            || builder.claim != claim.fingerprint()
            || builder.frames.len() >= builder.expected
            || builder.frames.capacity() < builder.expected
        {
            return Err(error(ThrowableErrorKind::InvalidSnapshot));
        }
        self.registry.method(frame.method)?;
        self.heap.instance_graph_node(frame.method.owner())?;
        if frame.pc > 65534 {
            return Err(error(ThrowableErrorKind::InvalidFrame {
                method: frame.method,
                pc: frame.pc,
            }));
        }
        builder.frames.push(frame);
        Ok(())
    }
    pub fn finish_trace_snapshot(
        &mut self,
        claim: &InitializationClaim,
        builder: TraceSnapshotBuilder,
    ) -> R<TraceSnapshot> {
        let fingerprint = self.trace_claim(claim)?;
        self.trace_work(1)?;
        if builder.fingerprint != fingerprint
            || builder.claim != claim.fingerprint()
            || builder.frames.len() != builder.expected
        {
            return Err(error(ThrowableErrorKind::InvalidSnapshot));
        }
        Ok(TraceSnapshot {
            expected: builder.expected,
            fingerprint,
            claim: builder.claim,
            frames: builder.frames,
        })
    }
    pub fn commit_throwable_capture(
        &mut self,
        claim: &InitializationClaim,
        authority: &ThrowableCaptureAuthority,
        snapshot: TraceSnapshot,
    ) -> R<Reference> {
        let fingerprint = self.trace_claim(claim)?;
        self.trace_work(1)?;
        if authority.fingerprint != fingerprint
            || authority.claim != claim.fingerprint()
            || snapshot.fingerprint != fingerprint
            || snapshot.claim != claim.fingerprint()
            || snapshot.frames.len() != snapshot.expected
        {
            return Err(error(ThrowableErrorKind::InvalidCapture));
        }
        if authority.slots != self.heap.throwable_state()?.slots {
            return Err(error(ThrowableErrorKind::InvalidCapture));
        }
        let pair =
            self.heap
                .throwable_pair(authority.receiver, authority.actual, authority.slots)?;
        self.heap
            .trace_from_pair(authority.receiver, authority.actual, pair)?;
        let payload = Backtrace {
            fingerprint,
            owner: authority
                .receiver
                .heap_ref()
                .ok_or(HeapError::NullReference)?,
            actual: authority.actual,
            frames: snapshot.frames,
        };
        self.heap.publish_backtrace(
            authority.receiver,
            authority.actual,
            authority.slots,
            payload,
        )?;
        Ok(authority.receiver)
    }
    pub fn vm_exception_spec(
        &mut self,
        claim: &InitializationClaim,
        kind: VmExceptionKind,
    ) -> R<VmExceptionSpec> {
        let fingerprint = self.trace_claim(claim)?;
        if kind != VmExceptionKind::ArithmeticException {
            return Err(error(ThrowableErrorKind::UnsupportedProducer(kind)));
        }
        let s = self.heap.throwable_state()?;
        Ok(VmExceptionSpec {
            fingerprint,
            claim: claim.fingerprint(),
            class: s.plan.public.arithmetic_exception,
            constructor: s.plan.ctors[6],
            key: s.plan.ctor_keys[6],
        })
    }
    fn check_vm_spec(&mut self, claim: &InitializationClaim, spec: &VmExceptionSpec) -> R<()> {
        let fingerprint = self.trace_claim(claim)?;
        let s = self.heap.throwable_state()?;
        if spec.fingerprint != fingerprint
            || spec.claim != claim.fingerprint()
            || spec.class != s.plan.public.arithmetic_exception
            || spec.constructor != s.plan.ctors[6]
            || spec.key != s.plan.ctor_keys[6]
            || Key::of(self.registry.method(spec.constructor)?) != spec.key
        {
            return Err(error(ThrowableErrorKind::InvalidBinding(
                "VM exception spec differs",
            )));
        }
        Ok(())
    }
    pub fn allocate_vm_exception(
        &mut self,
        claim: &InitializationClaim,
        spec: &VmExceptionSpec,
    ) -> R<(Reference, ResolvedConstructor)> {
        self.check_vm_spec(claim, spec)?;
        let receiver = self.allocate_construction_class(spec.class)?;
        let constructor = self.vm_constructor_authority(claim, spec.constructor, receiver)?;
        Ok((receiver, constructor))
    }
    pub fn authenticate_vm_exception(
        &mut self,
        claim: &InitializationClaim,
        exceptions: &ExceptionProfile,
        spec: &VmExceptionSpec,
        receiver: Reference,
    ) -> R<ThrownException> {
        self.check_vm_spec(claim, spec)?;
        if exceptions.throwable() != self.heap.throwable_state()?.plan.throwable {
            return Err(error(ThrowableErrorKind::InvalidBinding(
                "VM exception Throwable profile differs",
            )));
        }
        if self.heap.instance_class(receiver)? != spec.class {
            return Err(error(ThrowableErrorKind::InvalidReceiver));
        }
        if self.throwable_trace(receiver)?.is_none() {
            return Err(error(ThrowableErrorKind::InvalidState(
                "VM exception has no completed capture",
            )));
        }
        Ok(self.prepare_throw(exceptions, receiver)?)
    }
}

#[cfg(test)]
#[path = "throwable_tests.rs"]
mod tests;

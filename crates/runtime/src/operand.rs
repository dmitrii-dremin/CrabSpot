//! Fixed operand facts, sealed exception roles and bounded message services.
//! The engine alone authenticates operation/site and completed-constructor history.
use crate::initialization::InitializationFingerprint;
use crate::throwable::{Fingerprint, Key};
use crate::*;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperandOutcome<T, F> {
    Value(T),
    Fault(F),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArrayOperandFault {
    Bounds {
        index: i32,
        length: i32,
    },
    NegativeSize {
        length: i32,
        dimension: u8,
    },
    Store {
        source: ReferenceType,
        component: ReferenceType,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CastOperandFault {
    Mismatch {
        source: ReferenceType,
        target: ReferenceType,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperandExceptionFault {
    Bounds {
        index: i32,
        length: i32,
    },
    NegativeSize {
        length: i32,
        dimension: u8,
    },
    Store {
        source: ReferenceType,
        component: ReferenceType,
    },
    Cast {
        source: ReferenceType,
        target: ReferenceType,
    },
}
impl OperandExceptionFault {
    fn role(self) -> R<usize> {
        match self {
            Self::Bounds { index, length } if length >= 0 && (index < 0 || index >= length) => {
                Ok(1)
            }
            Self::NegativeSize { length, dimension } if length < 0 && dimension < 255 => Ok(2),
            Self::Store { .. } => Ok(3),
            Self::Cast { .. } => Ok(4),
            _ => Err(error(OperandExceptionErrorKind::InvalidFault)),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperandExceptionDiscoveryLimits {
    pub max_work_units: u64,
}
impl Default for OperandExceptionDiscoveryLimits {
    fn default() -> Self {
        Self {
            max_work_units: 4_194_304,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperandExceptionDiscoveryUsage {
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperandDiagnosticLimits {
    pub max_work_units: u64,
    pub max_message_units: u64,
    pub max_total_message_units: u64,
    pub max_temporary_bytes: u64,
}
impl Default for OperandDiagnosticLimits {
    fn default() -> Self {
        Self {
            max_work_units: 33_554_432,
            max_message_units: 262_144,
            max_total_message_units: 4_194_304,
            max_temporary_bytes: 524_288,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperandDiagnosticUsage {
    pub work_units: u64,
    pub message_units: u64,
    pub messages: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperandExceptionResource {
    DiscoveryWork,
    DiagnosticWork,
    MessageUnits,
    TotalMessageUnits,
    TemporaryBytes,
    Messages,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperandExceptionErrorKind {
    Disabled,
    UnsupportedShape,
    PolicyConflict,
    InvalidBinding(&'static str),
    InvalidState(&'static str),
    InvalidFault,
    Limit {
        resource: OperandExceptionResource,
        limit: u64,
        required: u64,
    },
    Overflow {
        resource: OperandExceptionResource,
    },
    AllocationFailure,
    Registry(Box<RegistryError>),
    Context(Box<FieldAccessError>),
    Type(Box<TypeAccessError>),
    Heap(Box<HeapError>),
    Instance(Box<InstanceError>),
    String(Box<StringError>),
    Construction(Box<ConstructionAccessError>),
    Throwable(Box<ThrowableAccessError>),
    Exception(Box<ExceptionAccessError>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperandExceptionError {
    pub kind: OperandExceptionErrorKind,
    pub discovery_usage: OperandExceptionDiscoveryUsage,
}
impl fmt::Display for OperandExceptionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            Self::Context(e) => e.fmt(f),
            Self::Type(e) => e.fmt(f),
            Self::Heap(e) => e.fmt(f),
            Self::Instance(e) => e.fmt(f),
            Self::String(e) => e.fmt(f),
            Self::Construction(e) => e.fmt(f),
            Self::Throwable(e) => e.fmt(f),
            Self::Exception(e) => e.fmt(f),
            e => write!(f, "operand exception service: {e:?}"),
        }
    }
}
impl fmt::Display for OperandExceptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
impl std::error::Error for OperandExceptionErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e.as_ref()),
            Self::Context(e) => Some(e.as_ref()),
            Self::Type(e) => Some(e.as_ref()),
            Self::Heap(e) => Some(e.as_ref()),
            Self::Instance(e) => Some(e.as_ref()),
            Self::String(e) => Some(e.as_ref()),
            Self::Construction(e) => Some(e.as_ref()),
            Self::Throwable(e) => Some(e.as_ref()),
            Self::Exception(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl std::error::Error for OperandExceptionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.source()
    }
}
type R<T> = Result<T, Box<OperandExceptionError>>;
fn error(kind: OperandExceptionErrorKind) -> Box<OperandExceptionError> {
    Box::new(OperandExceptionError {
        kind,
        discovery_usage: Default::default(),
    })
}
macro_rules! convert {
    ($t:ty,$v:ident) => {
        impl From<$t> for Box<OperandExceptionError> {
            fn from(e: $t) -> Self {
                error(OperandExceptionErrorKind::$v(Box::new(e)))
            }
        }
        impl From<Box<$t>> for Box<OperandExceptionError> {
            fn from(e: Box<$t>) -> Self {
                error(OperandExceptionErrorKind::$v(e))
            }
        }
    };
}
convert!(RegistryError, Registry);
convert!(FieldAccessError, Context);
convert!(TypeAccessError, Type);
convert!(HeapError, Heap);
convert!(InstanceError, Instance);
convert!(StringError, String);
convert!(ConstructionAccessError, Construction);
convert!(ThrowableAccessError, Throwable);
convert!(ExceptionAccessError, Exception);
fn invalid(reason: &'static str) -> Box<OperandExceptionError> {
    error(OperandExceptionErrorKind::InvalidBinding(reason))
}
fn checked(current: u64, add: u64, limit: u64, resource: OperandExceptionResource) -> R<u64> {
    let required = current
        .checked_add(add)
        .ok_or_else(|| error(OperandExceptionErrorKind::Overflow { resource }))?;
    if required > limit {
        return Err(error(OperandExceptionErrorKind::Limit {
            resource,
            limit,
            required,
        }));
    }
    Ok(required)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperandExceptionRoles {
    pub index_out_of_bounds: ClassId,
    pub array_index_out_of_bounds: ClassId,
    pub negative_array_size: ClassId,
    pub array_store: ClassId,
    pub class_cast: ClassId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperandExceptionPlan {
    registry: u64,
    throwable: ThrowablePlan,
    ids: [ClassId; 5],
    ctors: [MethodId; 5],
    keys: [Key; 5],
}
impl OperandExceptionPlan {
    pub fn public_roles(&self) -> OperandExceptionRoles {
        OperandExceptionRoles {
            index_out_of_bounds: self.ids[0],
            array_index_out_of_bounds: self.ids[1],
            negative_array_size: self.ids[2],
            array_store: self.ids[3],
            class_cast: self.ids[4],
        }
    }
    fn parents(&self) -> [ClassId; 5] {
        let r = self.throwable.public.runtime_exception;
        [r, self.ids[0], r, r, r]
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperandExceptionDiscovery {
    pub plan: OperandExceptionPlan,
    pub usage: OperandExceptionDiscoveryUsage,
}
#[path = "operand_probe.rs"]
mod probe;
pub use probe::probe_operand_exceptions;
#[path = "operand_message.rs"]
mod message;
#[derive(Debug)]
pub(crate) struct OperandState {
    plan: OperandExceptionPlan,
    fingerprint: Fingerprint,
    limits: OperandDiagnosticLimits,
    usage: OperandDiagnosticUsage,
    #[cfg(test)]
    fail_reserve_after: Option<usize>,
}
impl OperandState {
    fn work(&mut self, n: u64) -> R<()> {
        self.usage.work_units = checked(
            self.usage.work_units,
            n,
            self.limits.max_work_units,
            OperandExceptionResource::DiagnosticWork,
        )?;
        Ok(())
    }
    fn reserving(&mut self) -> R<()> {
        #[cfg(test)]
        if let Some(left) = &mut self.fail_reserve_after {
            if *left == 0 {
                return Err(error(OperandExceptionErrorKind::AllocationFailure));
            }
            *left -= 1;
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct OperandExceptionSpec {
    fingerprint: Fingerprint,
    claim: InitializationFingerprint,
    kind: VmExceptionKind,
    class: ClassId,
    constructor: MethodId,
    key: Key,
    fault: OperandExceptionFault,
}
impl OperandExceptionSpec {
    pub fn class(&self) -> ClassId {
        self.class
    }
    pub fn constructor(&self) -> MethodId {
        self.constructor
    }
    pub fn kind(&self) -> VmExceptionKind {
        self.kind
    }
}
fn kind(role: usize) -> R<VmExceptionKind> {
    Ok(match role {
        1 => VmExceptionKind::ArrayIndexOutOfBoundsException,
        2 => VmExceptionKind::NegativeArraySizeException,
        3 => VmExceptionKind::ArrayStoreException,
        4 => VmExceptionKind::ClassCastException,
        _ => return Err(error(OperandExceptionErrorKind::InvalidFault)),
    })
}
impl ManagedHeap {
    fn operand_state(&self) -> R<&OperandState> {
        self.operand_exceptions
            .as_ref()
            .ok_or_else(|| error(OperandExceptionErrorKind::Disabled))
    }
    pub fn operand_diagnostic_usage(&self) -> OperandDiagnosticUsage {
        self.operand_exceptions
            .as_ref()
            .map_or(Default::default(), |s| s.usage)
    }
    pub fn bind_operand_exceptions(
        &mut self,
        registry: &DefinitionRegistry,
        plan: &OperandExceptionPlan,
        limits: OperandDiagnosticLimits,
    ) -> R<()> {
        if registry.identity() != plan.registry {
            return Err(invalid("operand registry differs"));
        }
        self.equality_affinity(registry)
            .map_err(|e| Box::<OperandExceptionError>::from(FieldAccessError::StringBody(e)))?;
        let t = self
            .throwables
            .as_ref()
            .ok_or_else(|| error(OperandExceptionErrorKind::Disabled))?;
        let fp = t.fingerprint;
        if t.plan != plan.throwable
            || fp.registry != registry.identity()
            || fp.heap != self.identity
            || fp.classes != self.retained_layout_classes()
            || Some(fp.universe) != self.reference_type_universe()
        {
            return Err(invalid("operand Throwable affinity differs"));
        }
        for (i, (&id, parent)) in plan.ids.iter().zip(plan.parents()).enumerate() {
            registry.class(id)?;
            if registry.defining_loader(id)? != plan.throwable.boot
                || self.instance_graph_node(id)?.superclass != Some(parent)
                || plan.ctors[i].owner() != id
                || Key::of(registry.method(plan.ctors[i])?) != plan.keys[i]
            {
                return Err(invalid("operand role parent or constructor differs"));
            }
        }
        if let Some(old) = &self.operand_exceptions {
            return if old.plan == *plan && old.fingerprint == fp && old.limits == limits {
                Ok(())
            } else {
                Err(error(OperandExceptionErrorKind::PolicyConflict))
            };
        }
        if !self.entries.is_empty()
            || self.initialization_claim.is_some()
            || self.string_usage() != StringUsage::default()
            || self.throwable_trace_usage() != TraceUsage::default()
        {
            return Err(invalid(
                "first operand binding requires pristine unclaimed heap",
            ));
        }
        self.operand_exceptions = Some(OperandState {
            plan: plan.clone(),
            fingerprint: fp,
            limits,
            usage: Default::default(),
            #[cfg(test)]
            fail_reserve_after: None,
        });
        Ok(())
    }
}
impl FieldResolutionContext<'_> {
    pub fn check_operand_exception_profile(&self) -> R<()> {
        self.check_throwable_profile()?;
        self.check_string_profile()?;
        let s = self.heap.operand_state()?;
        let t = self
            .heap
            .throwables
            .as_ref()
            .ok_or_else(|| error(OperandExceptionErrorKind::Disabled))?;
        if s.fingerprint != t.fingerprint
            || s.plan.throwable != t.plan
            || s.plan.registry != self.registry.identity()
        {
            return Err(invalid("operand context affinity differs"));
        }
        Ok(())
    }
    fn operand_claim(&mut self, claim: &InitializationClaim) -> R<Fingerprint> {
        let fp = self.trace_claim(claim)?;
        self.check_operand_exception_profile()?;
        Ok(fp)
    }
    pub fn operand_exception_spec(
        &mut self,
        claim: &InitializationClaim,
        fault: OperandExceptionFault,
    ) -> R<OperandExceptionSpec> {
        let fingerprint = self.operand_claim(claim)?;
        self.operand_work(1)?;
        let role = fault.role()?;
        let p = &self.heap.operand_state()?.plan;
        Ok(OperandExceptionSpec {
            fingerprint,
            claim: claim.fingerprint(),
            kind: kind(role)?,
            class: p.ids[role],
            constructor: p.ctors[role],
            key: p.keys[role],
            fault,
        })
    }
    fn check_operand_spec(
        &mut self,
        claim: &InitializationClaim,
        spec: &OperandExceptionSpec,
    ) -> R<()> {
        let fp = self.operand_claim(claim)?;
        let role = spec.fault.role()?;
        let p = &self.heap.operand_state()?.plan;
        if spec.fingerprint != fp
            || spec.claim != claim.fingerprint()
            || spec.kind != kind(role)?
            || spec.class != p.ids[role]
            || spec.constructor != p.ctors[role]
            || spec.key != p.keys[role]
            || Key::of(self.registry.method(spec.constructor)?) != spec.key
        {
            return Err(invalid("operand spec differs"));
        }
        Ok(())
    }
    fn operand_work(&mut self, n: u64) -> R<()> {
        self.heap
            .operand_exceptions
            .as_mut()
            .ok_or_else(|| error(OperandExceptionErrorKind::Disabled))?
            .work(n)
    }
    pub fn allocate_operand_exception(
        &mut self,
        claim: &InitializationClaim,
        spec: &OperandExceptionSpec,
    ) -> R<(Reference, ResolvedConstructor)> {
        self.check_operand_spec(claim, spec)?;
        Ok(self.allocate_fixed_vm_exception(claim, spec.class, spec.constructor)?)
    }
    pub fn authenticate_operand_exception(
        &mut self,
        claim: &InitializationClaim,
        exceptions: &ExceptionProfile,
        spec: &OperandExceptionSpec,
        receiver: Reference,
    ) -> R<ThrownException> {
        self.check_operand_spec(claim, spec)?;
        Ok(self.authenticate_fixed_vm_exception(exceptions, spec.class, receiver)?)
    }
}

#[cfg(test)]
#[path = "operand_tests.rs"]
mod tests;

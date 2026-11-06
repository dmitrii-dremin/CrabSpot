//! Private native capture and suspended VM-fault protocol; no public authority.
use super::*;
use crate::{
    VmExceptionContext, VmExceptionCounter, VmExceptionLimits, VmFaultOrigin,
    VmMaterializationPhase as PhaseVm,
};
use crabspot_runtime::{TraceFrame, TraceMethodKind, TraceSnapshot, VmExceptionSpec};

pub(super) struct VmExecution {
    limits: VmExceptionLimits,
    admitted: u64,
    pub context: Option<VmExceptionContext>,
    pub pending: Option<VmPending>,
}
pub(super) struct VmPending {
    pub origin: VmFaultOrigin,
    pub owner: u64,
    pub depth: usize,
    pub spec: VmExceptionSpec,
    pub snapshot: Option<TraceSnapshot>,
    pub receiver: Option<Reference>,
    pub consumed: bool,
}
impl VmExecution {
    pub fn new(limits: VmExceptionLimits) -> Self {
        Self {
            limits,
            admitted: 0,
            context: None,
            pending: None,
        }
    }
}
fn vm_protocol(reason: &'static str) -> ExecutionError {
    ExecutionError::new(None, E::VmExceptionProtocol { reason })
}
fn trace_error(error: Box<crabspot_runtime::ThrowableAccessError>) -> ExecutionError {
    ExecutionError::new(None, E::ThrowableAccess(error))
}
fn phase(java: &mut JavaState, value: PhaseVm) {
    if let Some(context) = java.vm.as_mut().and_then(|v| v.context.as_mut()) {
        context.phase = value;
    }
}
fn immutable_receiver(frame: &Frame, pc: u32) -> Result<Option<Reference>, ExecutionError> {
    if frame.role == FrameRole::Constructor {
        Ok(Some(constructor_activation(frame, pc)?.this.reference))
    } else {
        Ok(frame.entry_receiver)
    }
}
/// Prefix walk performs no allocation; a boundary kind is queried exactly once.
fn trace_skip(
    java: &JavaState,
    context: &mut FieldResolutionContext<'_>,
    claim: &InitializationClaim,
    receiver: Reference,
    pc: u32,
) -> Result<usize, ExecutionError> {
    let mut skip = 0;
    let mut constructors = false;
    for frame in java.frames.iter().rev() {
        if immutable_receiver(frame, pc)? != Some(receiver) {
            break;
        }
        let MethodKey::Registered(method) = frame.method else {
            return Err(vm_protocol("standalone frame in trace"));
        };
        let kind = context
            .trace_method_kind(claim, method)
            .map_err(trace_error)?;
        if !constructors && kind == TraceMethodKind::FillInStackTrace {
            skip += 1;
            continue;
        }
        constructors = true;
        if kind == TraceMethodKind::Constructor {
            skip += 1;
        } else {
            break;
        }
    }
    Ok(skip)
}
fn snapshot(
    java: &JavaState,
    context: &mut FieldResolutionContext<'_>,
    claim: &InitializationClaim,
    skip: usize,
    top_pc: u32,
) -> Result<TraceSnapshot, ExecutionError> {
    let count = java
        .frames
        .len()
        .checked_sub(skip)
        .ok_or_else(|| vm_protocol("trace prefix exceeds frames"))?;
    let mut builder = context
        .begin_trace_snapshot(claim, count)
        .map_err(trace_error)?;
    let mut pc = if count < java.frames.len() && count != 0 {
        let child = &java.frames[count];
        match child.origin {
            Some((key, pc)) if java.frames[count - 1].method == key => pc,
            _ => return Err(vm_protocol("filtered trace boundary origin differs")),
        }
    } else {
        top_pc
    };
    for index in (0..count).rev() {
        let frame = &java.frames[index];
        let MethodKey::Registered(method) = frame.method else {
            return Err(vm_protocol("trace frame is not registered"));
        };
        if java
            .cache
            .get(&frame.method)
            .is_none_or(|p| p.decoded.instruction_at(pc).is_none())
        {
            return Err(vm_protocol(
                "trace PC is not an executed instruction boundary",
            ));
        }
        {
            context
                .append_trace_frame(claim, &mut builder, TraceFrame { method, pc })
                .map_err(trace_error)?;
        }
        match (index.checked_sub(1), frame.origin) {
            (Some(parent), Some((key, invoke))) if java.frames[parent].method == key => pc = invoke,
            (None, None) => {}
            _ => return Err(vm_protocol("trace activation origin mismatch")),
        }
    }
    context
        .finish_trace_snapshot(claim, builder)
        .map_err(trace_error)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn capture_native(
    java: &mut JavaState,
    context: &mut FieldResolutionContext<'_>,
    init: &mut InitExecution<'_>,
    method: MethodId,
    receiver: Reference,
    args: &[Value],
    pc: u32,
) -> Result<(), ExecutionError> {
    let shape = context
        .method_value_shape(method)
        .map_err(|e| ExecutionError::new(Some(pc), E::MethodAccess(Box::new(e))))?;
    if shape.parameters != [ValueKind::Int]
        || shape.parameter_slots != 1
        || shape.result != MethodReturnKind::Reference
        || !matches!(args, [Value::Int(_)])
    {
        return Err(vm_protocol("native Throwable capture signature differs"));
    }
    context
        .check_method_reference_profile(method, Some(receiver))
        .and_then(|()| context.check_method_arguments(method, args))
        .and_then(|()| context.check_method_return(method, Some(Value::Reference(receiver))))
        .map_err(|e| ExecutionError::new(Some(pc), E::MethodAccess(Box::new(e))))?;
    java.frames
        .last_mut()
        .ok_or_else(|| vm_protocol("native caller absent"))?
        .stack
        .reserve_one(pc)?;
    let authority = context
        .prepare_throwable_capture(init.claim, method, receiver)
        .map_err(trace_error)?;
    let original = java
        .vm
        .as_ref()
        .and_then(|v| v.pending.as_ref())
        .is_some_and(|p| p.receiver == Some(receiver) && !p.consumed);
    if original {
        let pending = java
            .vm
            .as_ref()
            .and_then(|v| v.pending.as_ref())
            .ok_or_else(|| vm_protocol("VM original capture pending absent"))?;
        let boundary = pending
            .depth
            .checked_add(1)
            .and_then(|i| java.frames.get(i))
            .ok_or_else(|| vm_protocol("VM original capture boundary absent"))?;
        let activation = constructor_activation(boundary, pc)?;
        if pending.owner != init.owner
            || activation.this.reference != receiver
            || !matches!(activation.caller,ConstructorCaller::VmFault(owner) if owner==init.owner)
            || activation.access.method() != pending.spec.constructor()
        {
            return Err(vm_protocol("VM original capture authority differs"));
        }
    }
    let saved = if original {
        java.vm
            .as_mut()
            .and_then(|v| v.pending.as_mut())
            .and_then(|p| p.snapshot.take())
            .ok_or_else(|| vm_protocol("pending original snapshot absent"))?
    } else {
        let skip = trace_skip(java, context, init.claim, receiver, pc)?;
        snapshot(java, context, init.claim, skip, pc)?
    };
    // No semantic check, allocation or fallible operand operation after commit.
    let caller = java
        .frames
        .last_mut()
        .ok_or_else(|| vm_protocol("native caller absent"))?;
    caller.stack.check_reserved_one(pc)?;
    let result = context
        .commit_throwable_capture(init.claim, &authority, saved)
        .map_err(trace_error)?;
    if original {
        if let Some(pending) = java.vm.as_mut().and_then(|v| v.pending.as_mut()) {
            pending.consumed = true;
        }
    }
    caller.stack.publish_reserved_reference(result);
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn begin_vm_fault(
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    init: &mut InitExecution<'_>,
    exceptions: &mut ExceptionState<'_>,
    bodies: Bodies<'_>,
    limits: CallLimits,
    current: MethodKey,
    pc: u32,
) -> Result<Option<EngineOutcome>, ExecutionError> {
    let MethodKey::Registered(method) = current else {
        return Err(vm_protocol("VM fault origin is not registered"));
    };
    let origin = VmFaultOrigin {
        method,
        pc,
        kind: VmExceptionKind::ArithmeticException,
    };
    let vm = java
        .vm
        .as_mut()
        .ok_or_else(|| vm_protocol("VM producer disabled"))?;
    if let Some(pending) = &vm.pending {
        return Err(ExecutionError::new(
            Some(pc),
            E::NestedVmMaterialization(Box::new(crate::NestedVmMaterialization {
                original: pending.origin,
                secondary: origin,
            })),
        ));
    }
    vm.context = Some(VmExceptionContext {
        origin,
        phase: PhaseVm::Preflight,
    });
    let Services::Fields { context, .. } = services else {
        return Err(vm_protocol("VM fault context absent"));
    };
    let spec = context
        .vm_exception_spec(init.claim, origin.kind)
        .map_err(trace_error)?;
    let required = vm.admitted.checked_add(1).ok_or_else(|| {
        ExecutionError::new(
            Some(pc),
            E::VmExceptionAccountingOverflow {
                counter: VmExceptionCounter::Evaluation,
            },
        )
    })?;
    if required > vm.limits.max_materializations {
        return Err(ExecutionError::new(
            Some(pc),
            E::VmExceptionLimit {
                limit: vm.limits.max_materializations,
                required,
            },
        ));
    }
    let started = init
        .state
        .vm_exceptions
        .started
        .checked_add(1)
        .ok_or_else(|| {
            ExecutionError::new(
                Some(pc),
                E::VmExceptionAccountingOverflow {
                    counter: VmExceptionCounter::Started,
                },
            )
        })?;
    vm.admitted = required;
    init.state.vm_exceptions.started = started;
    let target = MethodKey::Registered(spec.constructor());
    let class = spec.class();
    let depth = java
        .frames
        .len()
        .checked_sub(1)
        .ok_or_else(|| vm_protocol("VM caller absent"))?;
    vm.pending = Some(VmPending {
        origin,
        owner: init.owner,
        depth,
        spec,
        snapshot: None,
        receiver: None,
        consumed: false,
    });
    if let std::collections::hash_map::Entry::Vacant(entry) = java.cache.entry(target) {
        let prepared = prepare_method_with_context(
            bodies.class(target)?,
            target.index(),
            &mut java.used_code_bytes,
            limits.max_prepared_code_bytes,
            Preparation::Constructor,
            Some((
                &mut **context,
                match target {
                    MethodKey::Registered(m) => m,
                    _ => return Err(vm_protocol("VM constructor unregistered")),
                },
                java.monitors.is_some(),
            )),
            Some(exceptions),
        )
        .map_err(|e| contextualize(bodies, e, target, Some((current, pc))))?;
        entry.insert(prepared);
    }
    let prepared = java
        .cache
        .get(&target)
        .ok_or_else(|| vm_protocol("VM constructor cache absent"))?;
    if !prepared.constructor || !prepared.instance || prepared.initializer {
        return Err(vm_protocol("VM constructor prepared role differs"));
    }
    check_reservation(java.frames.len(), java.reserved, prepared, limits)?;
    java.frames
        .try_reserve(1)
        .map_err(|_| init_allocation("VM constructor frame"))?;
    phase(java, PhaseVm::Snapshot);
    let saved = snapshot(java, context, init.claim, 0, pc)?;
    java.vm
        .as_mut()
        .and_then(|v| v.pending.as_mut())
        .ok_or_else(|| vm_protocol("VM pending absent"))?
        .snapshot = Some(saved);
    let caller = java
        .frames
        .get_mut(depth)
        .ok_or_else(|| vm_protocol("VM caller absent"))?;
    if caller.method != current || caller.pending.is_some() {
        return Err(vm_protocol("VM caller continuation conflicts"));
    }
    caller.pc = pc;
    caller.pending = Some(PendingUse::VmFault);
    phase(java, PhaseVm::Initialization);
    request_initialization(
        InitRequest {
            class,
            continuation: InitContinuation::Instruction {
                frame: depth,
                method: current,
                pc,
            },
            origin: Some((current, pc)),
        },
        bodies,
        java,
        services,
        Some(exceptions),
        init,
        limits,
    )
}
/// Called only after successful class active use, with the original caller restored.
// Shared scheduler services and policy are borrowed independently; no retained context bundle.
#[allow(clippy::too_many_arguments)]
pub(super) fn resume_vm_fault(
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    init: &mut InitExecution<'_>,
    bodies: Bodies<'_>,
    limits: CallLimits,
    frame: usize,
    method: MethodKey,
    pc: u32,
) -> Result<(), ExecutionError> {
    let pending = java
        .vm
        .as_ref()
        .and_then(|v| v.pending.as_ref())
        .ok_or_else(|| vm_protocol("VM pending absent"))?;
    if pending.owner != init.owner
        || pending.depth != frame
        || MethodKey::Registered(pending.origin.method) != method
        || pending.origin.pc != pc
        || pending.receiver.is_some()
        || pending.consumed
        || pending.snapshot.is_none()
    {
        return Err(vm_protocol("VM resume ownership differs"));
    }
    let target = MethodKey::Registered(pending.spec.constructor());
    let Services::Fields { context, .. } = services else {
        return Err(vm_protocol("VM context absent"));
    };
    phase(java, PhaseVm::Message);
    let pending = java
        .vm
        .as_ref()
        .and_then(|v| v.pending.as_ref())
        .ok_or_else(|| vm_protocol("VM pending absent"))?;
    let message = context
        .fresh_string(pending.spec.message_units())
        .map_err(|e| ExecutionError::new(None, E::String(e)))?;
    phase(java, PhaseVm::Allocation);
    let pending = java
        .vm
        .as_ref()
        .and_then(|v| v.pending.as_ref())
        .ok_or_else(|| vm_protocol("VM pending absent"))?;
    let (receiver, access) = context
        .allocate_vm_exception(init.claim, &pending.spec)
        .map_err(trace_error)?;
    if MethodKey::Registered(access.method()) != target {
        return Err(vm_protocol(
            "VM allocated constructor differs from preflight",
        ));
    }
    let prepared = java
        .cache
        .get(&target)
        .ok_or_else(|| vm_protocol("VM constructor preparation absent"))?;
    if !prepared.constructor || !prepared.instance || prepared.initializer {
        return Err(vm_protocol("VM constructor cache role differs"));
    }
    let reserved = check_reservation(java.frames.len(), java.reserved, prepared, limits)?;
    let args = [Value::Reference(message)];
    validate_args(prepared, &args)?;
    context
        .check_method_arguments(access.method(), &args)
        .map_err(|e| ExecutionError::new(None, E::MethodAccess(Box::new(e))))?;
    let activation = init.state.construction.next_activation(pc)?;
    let this = Uninitialized {
        reference: receiver,
        key: AliasKey::This(activation),
    };
    let mut callee = make_frame(target, prepared, &args, Some((method, pc)), Some(receiver))?;
    callee
        .constructor
        .try_reserve_exact(1)
        .map_err(|_| init_allocation("VM constructor activation"))?;
    callee
        .locals
        .store_frame(0, FrameValue::Uninitialized(this), 0)?;
    callee.role = FrameRole::Constructor;
    callee.constructor.push(ConstructorActivation {
        access,
        this,
        caller: ConstructorCaller::VmFault(init.owner),
        chain: Chain::Pending,
    });
    java.frames
        .try_reserve(1)
        .map_err(|_| init_allocation("VM constructor frame"))?;
    let depth = java.frames.len();
    let caller = java
        .frames
        .get_mut(frame)
        .ok_or_else(|| vm_protocol("VM caller absent"))?;
    if caller.method != method || caller.pc != pc || caller.pending.is_some() {
        return Err(vm_protocol("VM publication caller differs"));
    }
    activate_monitors(
        java.monitors.as_mut(),
        depth,
        &callee,
        prepared,
        services,
        Some(init.claim),
    )
    .map_err(|e| activation_error(bodies, e, &callee))?;
    caller.pending = Some(PendingUse::VmFault);
    if let Some(pending) = java.vm.as_mut().and_then(|v| v.pending.as_mut()) {
        pending.receiver = Some(receiver);
    }
    java.frames.push(callee);
    java.reserved = reserved;
    phase(java, PhaseVm::Constructor);
    Ok(())
}
pub(super) fn discard_vm_pending(java: &mut JavaState) -> Result<u32, ExecutionError> {
    let vm = java
        .vm
        .as_mut()
        .ok_or_else(|| vm_protocol("VM state absent"))?;
    let pending = vm
        .pending
        .take()
        .ok_or_else(|| vm_protocol("VM pending absent"))?;
    vm.context = Some(VmExceptionContext {
        origin: pending.origin,
        phase: PhaseVm::Dispatch,
    });
    Ok(pending.origin.pc)
}
pub(super) fn complete_vm_fault(
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    init: &mut InitExecution<'_>,
    exceptions: &ExceptionState<'_>,
) -> Result<ThrownException, ExecutionError> {
    phase(java, PhaseVm::Dispatch);
    let pending = java
        .vm
        .as_ref()
        .and_then(|v| v.pending.as_ref())
        .ok_or_else(|| vm_protocol("VM completion pending absent"))?;
    if pending.owner != init.owner
        || !pending.consumed
        || pending.snapshot.is_some()
        || java.frames.len() != pending.depth + 1
    {
        return Err(vm_protocol(
            "VM constructor did not consume original snapshot",
        ));
    }
    let reference = pending
        .receiver
        .ok_or_else(|| vm_protocol("VM completion receiver absent"))?;
    let Services::Fields { context, .. } = services else {
        return Err(vm_protocol("VM completion context absent"));
    };
    let thrown = context
        .authenticate_vm_exception(init.claim, exceptions.profile, &pending.spec, reference)
        .map_err(trace_error)?;
    let produced = init
        .state
        .vm_exceptions
        .produced
        .checked_add(1)
        .ok_or_else(|| {
            ExecutionError::new(
                None,
                E::VmExceptionAccountingOverflow {
                    counter: VmExceptionCounter::Produced,
                },
            )
        })?;
    if produced > init.state.vm_exceptions.started {
        return Err(vm_protocol("produced VM count exceeds admitted starts"));
    }
    init.state.vm_exceptions.produced = produced;
    discard_vm_pending(java)?;
    Ok(thrown)
}
pub(super) fn finish_vm_boundary(
    java: &mut JavaState,
    owner: u64,
    promote: bool,
    pc: u32,
) -> Result<u32, ExecutionError> {
    let index = java
        .frames
        .len()
        .checked_sub(1)
        .ok_or_else(|| vm_protocol("VM constructor absent"))?;
    let callee = &java.frames[index];
    let activation = constructor_activation(callee, pc)?;
    let pending = java
        .vm
        .as_ref()
        .and_then(|v| v.pending.as_ref())
        .ok_or_else(|| vm_protocol("VM boundary pending absent"))?;
    if pending.owner != owner
        || pending.receiver != Some(activation.this.reference)
        || pending.depth.checked_add(1) != Some(index)
        || activation.access.method() != pending.spec.constructor()
        || callee.origin
            != Some((
                MethodKey::Registered(pending.origin.method),
                pending.origin.pc,
            ))
    {
        return Err(vm_protocol("VM constructor boundary ownership differs"));
    }
    if promote && (!pending.consumed || pending.snapshot.is_some()) {
        return Err(vm_protocol(
            "VM constructor has not consumed original snapshot",
        ));
    }
    let released_slots = callee.reserved_slots;
    let parent = java
        .frames
        .get_mut(pending.depth)
        .ok_or_else(|| vm_protocol("VM caller absent"))?;
    if parent.method != MethodKey::Registered(pending.origin.method)
        || parent.pc != pending.origin.pc
        || !matches!(parent.pending, Some(PendingUse::VmFault))
    {
        return Err(vm_protocol("VM constructor caller tag differs"));
    }
    let invoke = pending.origin.pc;
    let reserved = java
        .reserved
        .checked_sub(released_slots)
        .ok_or_else(|| vm_protocol("VM constructor reservation differs"))?;
    parent.pending = None;
    java.frames.pop();
    java.reserved = reserved;
    if !promote {
        discard_vm_pending(java)?;
    }
    Ok(invoke)
}
pub(super) fn annotate_vm_error(
    java: &JavaState,
    bodies: Bodies<'_>,
    mut error: ExecutionError,
) -> ExecutionError {
    let Some(vm) = &java.vm else {
        return error;
    };
    if let (Some(pending), E::UnsupportedVmException(kind)) = (&vm.pending, &error.kind) {
        let method = java
            .frames
            .last()
            .and_then(|f| match f.method {
                MethodKey::Registered(m) => Some(m),
                _ => None,
            })
            .unwrap_or(pending.origin.method);
        let pc = error
            .offset
            .unwrap_or_else(|| java.frames.last().map_or(pending.origin.pc, |f| f.pc));
        error.kind = E::NestedVmMaterialization(Box::new(crate::NestedVmMaterialization {
            original: pending.origin,
            secondary: VmFaultOrigin {
                method,
                pc,
                kind: *kind,
            },
        }));
    }
    if error.method.is_none() {
        if let Some(context) = vm.context {
            let key = MethodKey::Registered(context.origin.method);
            let fault = match &vm.pending {
                Some(pending) => java.frames.get(pending.depth).filter(|f| f.method == key),
                None => java.frames.iter().rev().find(|f| f.method == key),
            };
            error.offset.get_or_insert(context.origin.pc);
            error = contextualize(bodies, error, key, fault.and_then(|f| f.origin));
        }
    }
    if error.vm_exception.is_none() {
        error.vm_exception = vm.context.map(Box::new);
    }
    error
}
pub(super) fn clear_vm_dispatch(java: &mut JavaState) {
    if let Some(vm) = java.vm.as_mut() {
        if vm.pending.is_none() {
            vm.context = None;
        }
    }
}

#[cfg(test)]
#[path = "throwable_tests.rs"]
mod tests;

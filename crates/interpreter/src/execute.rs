use crate::initialization::{
    InitializationResource, Phase, Record, RecordStatus, SessionState,
    allocation as init_allocation, overflow as init_overflow,
};
use crate::monitor::{MonitorFailure, MonitorKey, MonitorLedger};
use crate::{
    CallSite, ExecutionError, ExecutionErrorKind as E, MethodIdentity, Value, ValueKind,
    frame::{AliasKey, FrameValue, Locals, Stack, Uninitialized, protocol},
};
use crabspot_bytecode::{DecodedCode, Operands, decode};
use crabspot_classfile::{
    AttributeBody, ClassFile, Constant, ExceptionHandler,
    descriptor::{FieldType, parse_method_descriptor},
};
use crabspot_runtime::{
    ArrayAccessError, ArrayJavaError, ClassId, DefinitionRegistry, ExceptionAccessError,
    ExceptionProfile, FieldAccessError, FieldJavaError, FieldResolutionContext, HeapError,
    InstanceInvocationKind, ManagedHeap, MethodAccessError, MethodId, MethodJavaError,
    MethodReturnKind, MethodValueShape, PrimitiveArrayKind, Reference, ThrownException,
    TypeAccessError, TypeJavaError, VmExceptionKind,
};
use crabspot_runtime::{
    ConstructionAccessError, ConstructionJavaError, ConstructorFieldReceiverKind,
    ConstructorReceiverKind, ResolvedConstructor, ResolvedNew,
};
use crabspot_runtime::{
    InitializationAccessError, InitializationClaim, InitializationPlanLimits,
    InitializationStaticKind, ResolvedInitializationStatic,
};
use crabspot_runtime::{StaticAccessError, StaticJavaError, StaticStateProfile};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionOptions {
    pub instruction_budget: u64,
}
impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            instruction_budget: 1_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionOutcome {
    Returned(Option<Value>),
    /// A symbolic uncaught Java exception; no exception object is fabricated.
    ArithmeticException,
    ClassCastException,
    ArrayStoreException,
    NullPointerException,
    ArrayIndexOutOfBoundsException,
    NegativeArraySizeException,
    NoSuchFieldError,
    IllegalAccessError,
    IncompatibleClassChangeError,
    NoSuchMethodError,
    AbstractMethodError,
    LinkageError,
}

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
impl Default for ExceptionLimits {
    fn default() -> Self {
        Self {
            max_prepared_handlers: 65_536,
            max_dispatch_steps: 1_000_000,
        }
    }
}
struct ExceptionState<'a> {
    profile: &'a ExceptionProfile,
    limits: ExceptionLimits,
    prepared_handlers: usize,
    dispatch_steps: u64,
}
enum EngineOutcome {
    InitializationCompleted,
    Legacy(ExecutionOutcome),
    Uncaught(Reference),
}
fn vm_kind(outcome: ExecutionOutcome) -> Option<VmExceptionKind> {
    Some(match outcome {
        ExecutionOutcome::Returned(_) => return None,
        ExecutionOutcome::ArithmeticException => VmExceptionKind::ArithmeticException,
        ExecutionOutcome::ClassCastException => VmExceptionKind::ClassCastException,
        ExecutionOutcome::ArrayStoreException => VmExceptionKind::ArrayStoreException,
        ExecutionOutcome::NullPointerException => VmExceptionKind::NullPointerException,
        ExecutionOutcome::ArrayIndexOutOfBoundsException => {
            VmExceptionKind::ArrayIndexOutOfBoundsException
        }
        ExecutionOutcome::NegativeArraySizeException => VmExceptionKind::NegativeArraySizeException,
        ExecutionOutcome::NoSuchFieldError => VmExceptionKind::NoSuchFieldError,
        ExecutionOutcome::IllegalAccessError => VmExceptionKind::IllegalAccessError,
        ExecutionOutcome::IncompatibleClassChangeError => {
            VmExceptionKind::IncompatibleClassChangeError
        }
        ExecutionOutcome::NoSuchMethodError => VmExceptionKind::NoSuchMethodError,
        ExecutionOutcome::AbstractMethodError => VmExceptionKind::AbstractMethodError,
        ExecutionOutcome::LinkageError => VmExceptionKind::LinkageError,
    })
}
fn vm_completion(
    outcome: ExecutionOutcome,
    enabled: bool,
    pc: u32,
) -> Result<EngineOutcome, ExecutionError> {
    if enabled {
        if let Some(kind) = vm_kind(outcome) {
            return Err(ExecutionError::new(
                Some(pc),
                E::UnsupportedVmException(kind),
            ));
        }
    }
    Ok(EngineOutcome::Legacy(outcome))
}
fn exception_failure(error: ExceptionAccessError, pc: Option<u32>) -> ExecutionError {
    match error {
        ExceptionAccessError::Java(kind) => {
            ExecutionError::new(pc, E::UnsupportedVmException(kind))
        }
        error => ExecutionError::new(pc, E::ExceptionAccess(Box::new(error))),
    }
}

fn monitor_failure(error: MonitorFailure, pc: Option<u32>) -> ExecutionError {
    ExecutionError::new(
        pc,
        match error {
            MonitorFailure::Java(kind) => E::UnsupportedVmException(kind),
            MonitorFailure::Host(error) => E::Monitor(error),
        },
    )
}
fn finish_monitors(
    ledger: Option<&mut MonitorLedger>,
    depth: usize,
    pc: u32,
) -> Result<(), ExecutionError> {
    if let Some(ledger) = ledger {
        ledger
            .finish(depth)
            .map_err(|error| monitor_failure(error, Some(pc)))?;
    }
    Ok(())
}
// Invoked only for a fully provisioned frame, immediately before publication.
fn activate_monitors(
    ledger: Option<&mut MonitorLedger>,
    depth: usize,
    frame: &Frame,
    prepared: &PreparedMethod,
    services: &mut Services<'_, '_>,
    claim: Option<&InitializationClaim>,
) -> Result<(), ExecutionError> {
    let Some(ledger) = ledger else {
        if prepared.synchronized {
            return Err(ExecutionError::new(
                None,
                E::UnsupportedMethod("synchronized methods require monitor services"),
            ));
        }
        return Ok(());
    };
    ledger
        .check_new_frame(depth)
        .map_err(|e| monitor_failure(e, None))?;
    if !prepared.synchronized {
        return Ok(());
    }
    let key = if prepared.instance {
        let reference = frame.locals.load_reference(0, 0)?.reference(0)?;
        let handle = reference.heap_ref().ok_or_else(|| {
            ExecutionError::new(
                None,
                E::UnsupportedVmException(VmExceptionKind::NullPointerException),
            )
        })?;
        services
            .heap()
            .ok_or_else(|| operand_error(0))?
            .validate_root(handle)
            .map_err(|error| ExecutionError::new(None, E::Heap(error)))?;
        MonitorKey::Object(handle)
    } else {
        let Services::Fields { context, .. } = services else {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("monitor context absent"),
            ));
        };
        let class = frame.method.owner().ok_or_else(|| operand_error(0))?;
        let claim = claim.ok_or_else(|| operand_error(0))?;
        context
            .check_initialization_class(claim, class)
            .map_err(|error| initialization_failure(error, None))?;
        MonitorKey::Class(class)
    };
    ledger
        .enter_implicit(depth, key)
        .map_err(|e| monitor_failure(e, None))
}
fn activation_error(
    bodies: Bodies<'_>,
    mut error: ExecutionError,
    frame: &Frame,
) -> ExecutionError {
    if let Some((caller, pc)) = frame.origin {
        error.offset = Some(pc);
        contextualize(bodies, error, caller, frame.origin)
    } else {
        error.offset = None;
        contextualize(bodies, error, frame.method, None)
    }
}

fn numeric_kind(field: &FieldType) -> Result<ValueKind, ExecutionError> {
    match field {
        FieldType::Long => Ok(ValueKind::Long),
        FieldType::Float => Ok(ValueKind::Float),
        FieldType::Double => Ok(ValueKind::Double),
        FieldType::Byte
        | FieldType::Char
        | FieldType::Int
        | FieldType::Short
        | FieldType::Boolean => Ok(ValueKind::Int),
        _ => Err(ExecutionError::new(None, E::UnsupportedDescriptor)),
    }
}
fn operand_error(offset: u32) -> ExecutionError {
    ExecutionError::new(Some(offset), E::InvalidOperands)
}
fn local(operands: &Operands, offset: u32) -> Result<usize, ExecutionError> {
    if let Operands::Local(index) = operands {
        Ok(usize::from(*index))
    } else {
        Err(operand_error(offset))
    }
}
fn branch(decoded: &DecodedCode, offset: u32, displacement: i32) -> Result<u32, ExecutionError> {
    let destination = i64::from(offset) + i64::from(displacement);
    let target = u32::try_from(destination)
        .ok()
        .filter(|target| decoded.instruction_at(*target).is_some())
        .ok_or_else(|| ExecutionError::new(Some(offset), E::InstructionPointerOutsideCode))?;
    Ok(target)
}
fn constant(
    class: &ClassFile,
    index: u16,
    wide: bool,
    offset: u32,
) -> Result<Value, ExecutionError> {
    let invalid = || {
        ExecutionError::new(
            Some(offset),
            E::InvalidConstant {
                index,
                expected: if wide {
                    "a Long or Double constant for ldc2_w"
                } else {
                    "a category-one loadable constant for ldc/ldc_w"
                },
            },
        )
    };
    match class.constant_pool.get(index).map_err(|_| invalid())? {
        Constant::Integer(value) if !wide => Ok(Value::Int(*value)),
        Constant::Long(value) if wide => Ok(Value::Long(*value)),
        Constant::Double(bits) if wide => Ok(Value::Double(*bits)),
        Constant::Float(bits) if !wide => Ok(Value::Float(*bits)),
        Constant::Class { .. }
        | Constant::String { .. }
        | Constant::MethodHandle { .. }
        | Constant::MethodType { .. }
            if !wide =>
        {
            Err(ExecutionError::new(
                Some(offset),
                E::UnsupportedConstant { index },
            ))
        }
        Constant::Dynamic { .. } => Err(ExecutionError::new(
            Some(offset),
            E::UnsupportedConstant { index },
        )),
        _ => Err(invalid()),
    }
}

/// Explicit host policy for this per-evaluation body graph; not Java exceptions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallLimits {
    pub max_frames: usize,
    pub max_frame_slots: usize,
    /// Sum of original Code byte lengths prepared once and retained for this evaluation.
    pub max_prepared_code_bytes: usize,
}
impl Default for CallLimits {
    fn default() -> Self {
        Self {
            max_frames: 256,
            max_frame_slots: 1_000_000,
            max_prepared_code_bytes: 1_048_576,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Preparation {
    Static,
    Initializer,
    Constructor,
    DirectInstanceRoot,
    ResolvedInstance,
}
impl Preparation {
    fn is_instance(self) -> bool {
        matches!(
            self,
            Self::DirectInstanceRoot | Self::ResolvedInstance | Self::Constructor
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallKind {
    Static,
    Virtual,
    Special,
    Interface { count: u8 },
}
impl CallKind {
    fn is_instance(self) -> bool {
        self != Self::Static
    }
    fn opcode(self) -> u8 {
        match self {
            Self::Static => 0xb8,
            Self::Virtual => 0xb6,
            Self::Special => 0xb7,
            Self::Interface { .. } => 0xb9,
        }
    }
}
struct PreparedMethod {
    instance: bool,
    initializer: bool,
    constructor: bool,
    synchronized: bool,
    shape: MethodValueShape,
    // Index of the original Code attribute; rows stay borrowed from immutable metadata.
    handlers: Option<usize>,
    decoded: DecodedCode,
    max_stack: u16,
    max_locals: u16,
}
impl PreparedMethod {
    fn slots(&self) -> usize {
        usize::from(self.max_stack) + usize::from(self.max_locals)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum MethodKey {
    Standalone(usize),
    Registered(MethodId),
}
impl MethodKey {
    fn index(self) -> usize {
        match self {
            Self::Standalone(index) => index,
            Self::Registered(method) => method.index(),
        }
    }
    fn owner(self) -> Option<ClassId> {
        match self {
            Self::Standalone(_) => None,
            Self::Registered(method) => Some(method.owner()),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameRole {
    Ordinary,
    Initializer(ClassId),
    Constructor,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Chain {
    Pending,
    Complete,
    Failed,
}
struct ConstructorActivation {
    access: ResolvedConstructor,
    this: Uninitialized,
    caller: Uninitialized,
    chain: Chain,
}
fn constructor_activation(
    frame: &Frame,
    pc: u32,
) -> Result<&ConstructorActivation, ExecutionError> {
    let activation = frame
        .constructor
        .last()
        .ok_or_else(|| protocol(pc, "constructor activation absent"))?;
    if frame.role != FrameRole::Constructor
        || frame.constructor.len() != 1
        || frame.method != MethodKey::Registered(activation.access.method())
    {
        return Err(protocol(
            pc,
            "constructor authority does not match executing declaration",
        ));
    }
    Ok(activation)
}
fn pending_this(frame: &Frame, alias: Uninitialized, pc: u32) -> Result<(), ExecutionError> {
    let activation = constructor_activation(frame, pc)?;
    if activation.this != alias || activation.chain != Chain::Pending {
        return Err(protocol(
            pc,
            "constructor this is not the current pending activation",
        ));
    }
    Ok(())
}
fn finish_constructor_boundary(
    java: &mut JavaState,
    init: &mut InitExecution<'_>,
    promote: bool,
    pc: u32,
) -> Result<u32, ExecutionError> {
    let index = java
        .frames
        .len()
        .checked_sub(1)
        .ok_or_else(|| protocol(pc, "constructor frame absent"))?;
    let callee = java
        .frames
        .get(index)
        .ok_or_else(|| protocol(pc, "constructor frame absent"))?;
    let activation = constructor_activation(callee, pc)?;
    if promote && activation.chain != Chain::Complete {
        return Err(protocol(
            pc,
            "constructor returned before its chain completed",
        ));
    }
    let alias = activation.caller;
    let (method, invoke) = callee
        .origin
        .ok_or_else(|| protocol(pc, "constructor has no invoking frame"))?;
    let released = java
        .reserved
        .checked_sub(callee.reserved_slots)
        .ok_or_else(|| {
            ExecutionError::new(
                Some(pc),
                E::ConstructionAccountingOverflow {
                    resource: "constructor frame slots",
                },
            )
        })?;
    let parent = index
        .checked_sub(1)
        .ok_or_else(|| protocol(pc, "constructor cannot be a host root"))?;
    let caller = java
        .frames
        .get(parent)
        .ok_or_else(|| protocol(pc, "constructor caller absent"))?;
    if caller.method != method {
        return Err(protocol(pc, "constructor invocation origin mismatch"));
    }
    if matches!(alias.key, AliasKey::This(_)) {
        pending_this(caller, alias, pc)?;
    }
    init.state
        .construction
        .charge(caller.locals.len(), caller.stack.len(), pc)?;
    let caller = java
        .frames
        .get_mut(parent)
        .ok_or_else(|| protocol(pc, "constructor caller absent"))?;
    if matches!(alias.key, AliasKey::This(_)) {
        let own = caller
            .constructor
            .last_mut()
            .ok_or_else(|| protocol(pc, "constructor caller activation absent"))?;
        own.chain = if promote {
            Chain::Complete
        } else {
            Chain::Failed
        };
    }
    caller.locals.rewrite_alias(alias, promote);
    caller.stack.rewrite_alias(alias, promote);
    java.frames.pop();
    java.reserved = released;
    Ok(invoke)
}
fn construction_failure(error: ConstructionAccessError, pc: u32) -> ExecutionError {
    match error {
        ConstructionAccessError::Java(kind) => ExecutionError::new(
            Some(pc),
            E::UnsupportedVmException(match kind {
                ConstructionJavaError::NoSuchMethod => VmExceptionKind::NoSuchMethodError,
                ConstructionJavaError::NoSuchField => VmExceptionKind::NoSuchFieldError,
                ConstructionJavaError::IllegalAccess => VmExceptionKind::IllegalAccessError,
                ConstructionJavaError::IncompatibleClassChange => {
                    VmExceptionKind::IncompatibleClassChangeError
                }
                ConstructionJavaError::NullPointer => VmExceptionKind::NullPointerException,
                ConstructionJavaError::Instantiation => VmExceptionKind::InstantiationError,
                ConstructionJavaError::LinkageError => VmExceptionKind::LinkageError,
            }),
        ),
        error => ExecutionError::new(Some(pc), E::ConstructionAccess(Box::new(error))),
    }
}
fn constructor_reference(class: &ClassFile, index: u16) -> bool {
    let Ok(Constant::Methodref {
        name_and_type_index,
        ..
    }) = class.constant_pool.get(index)
    else {
        return false;
    };
    let Ok(Constant::NameAndType { name_index, .. }) =
        class.constant_pool.get(*name_and_type_index)
    else {
        return false;
    };
    class
        .constant_pool
        .utf8(*name_index)
        .is_ok_and(|name| name.equals_ascii("<init>"))
}
enum PendingUse {
    New {
        next: u32,
    },
    Static {
        value: Option<Value>,
        next: u32,
    },
    Call {
        target: MethodKey,
        args: Vec<Value>,
        next: u32,
    },
}
struct Frame {
    method: MethodKey,
    role: FrameRole,
    pending: Option<PendingUse>,
    static_access: Vec<ResolvedInitializationStatic>,
    new_access: Vec<ResolvedNew>,
    constructor: Vec<ConstructorActivation>,
    locals: Locals,
    stack: Stack,
    pc: u32,
    origin: Option<(MethodKey, u32)>,
    reserved_slots: usize,
}
enum Action {
    Monitor {
        reference: Reference,
        enter: bool,
        next: u32,
    },
    Continue,
    New {
        access: ResolvedNew,
        next: u32,
    },
    ConstructorCall {
        constant_index: u16,
    },
    InitializationStatic {
        access: ResolvedInitializationStatic,
        value: Option<Value>,
        next: u32,
    },
    Throw(ThrownException),
    Call {
        constant_index: u16,
        kind: CallKind,
    },
    Return(Option<Value>),
    ArithmeticException,
    ClassCastException,
    ArrayStoreException,
    NullPointerException,
    ArrayIndexOutOfBoundsException,
    NegativeArraySizeException,
    NoSuchFieldError,
    IllegalAccessError,
    IncompatibleClassChangeError,
    LinkageError,
}

#[derive(Clone, Copy)]
struct Bodies<'a> {
    standalone: Option<&'a ClassFile>,
    registry: Option<&'a DefinitionRegistry>,
}
impl<'a> Bodies<'a> {
    fn class(self, method: MethodKey) -> Result<&'a ClassFile, ExecutionError> {
        match method {
            MethodKey::Standalone(_) => self.standalone.ok_or_else(|| {
                ExecutionError::new(
                    None,
                    E::InvalidMethod("standalone metadata absent from initialization session"),
                )
            }),
            MethodKey::Registered(method) => self
                .registry
                .ok_or_else(|| {
                    ExecutionError::new(
                        None,
                        E::InvalidMethod("registered method without registry"),
                    )
                })?
                .class(method.owner())
                .map_err(|error| {
                    ExecutionError::new(
                        None,
                        E::MethodAccess(Box::new(MethodAccessError::Context(
                            FieldAccessError::Registry(error),
                        ))),
                    )
                }),
        }
    }
    fn static_key(self, current: MethodKey, index: usize) -> Result<MethodKey, ExecutionError> {
        let MethodKey::Registered(current) = current else {
            return Ok(MethodKey::Standalone(index));
        };
        let class = self.class(MethodKey::Registered(current))?;
        let member = class.methods.get(index).ok_or_else(|| {
            ExecutionError::new(
                None,
                E::InvalidMethod("resolved static method index missing"),
            )
        })?;
        let name = class.constant_pool.utf8(member.name_index).map_err(|_| {
            ExecutionError::new(None, E::InvalidMethod("invalid static method name"))
        })?;
        let descriptor = class
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(|_| ExecutionError::new(None, E::InvalidDescriptor))?;
        let method = self
            .registry
            .ok_or_else(|| {
                ExecutionError::new(
                    None,
                    E::InvalidMethod("registered static target without registry"),
                )
            })?
            .find_declared_method(current.owner(), name.units(), descriptor.units())
            .map_err(|error| {
                ExecutionError::new(
                    None,
                    E::MethodAccess(Box::new(MethodAccessError::Context(
                        FieldAccessError::Registry(error),
                    ))),
                )
            })?
            .ok_or_else(|| ExecutionError::new(None, E::MissingDeclaredMethod))?;
        Ok(MethodKey::Registered(method))
    }
    fn identity(self, key: MethodKey) -> Option<MethodIdentity> {
        let class = self.class(key).ok()?;
        let method = class.methods.get(key.index())?;
        Some(MethodIdentity {
            owner: key.owner(),
            index: key.index(),
            name: class.constant_pool.utf8(method.name_index).ok()?.clone(),
            descriptor: class
                .constant_pool
                .utf8(method.descriptor_index)
                .ok()?
                .clone(),
        })
    }
}
fn contextualize(
    bodies: Bodies<'_>,
    mut error: ExecutionError,
    method: MethodKey,
    origin: Option<(MethodKey, u32)>,
) -> ExecutionError {
    error.method = bodies.identity(method).map(Box::new);
    error.call_site = origin.and_then(|(method, offset)| {
        bodies
            .identity(method)
            .map(|method| Box::new(CallSite { method, offset }))
    });
    error
}
fn validate_args(prepared: &PreparedMethod, args: &[Value]) -> Result<(), ExecutionError> {
    if args.len() != prepared.shape.parameters.len() {
        return Err(ExecutionError::new(
            None,
            E::ArgumentCount {
                expected: prepared.shape.parameters.len(),
                actual: args.len(),
            },
        ));
    }
    for (index, (value, expected)) in args.iter().zip(&prepared.shape.parameters).enumerate() {
        if value.kind() != *expected {
            return Err(ExecutionError::new(
                None,
                E::ArgumentType {
                    index,
                    expected: *expected,
                    actual: value.kind(),
                },
            ));
        }
    }
    Ok(())
}
fn check_reservation(
    frame_count: usize,
    reserved: usize,
    prepared: &PreparedMethod,
    limits: CallLimits,
) -> Result<usize, ExecutionError> {
    let count = frame_count.checked_add(1).ok_or_else(|| {
        ExecutionError::new(
            None,
            E::FrameLimit {
                limit: limits.max_frames,
                required: usize::MAX,
            },
        )
    })?;
    if count > limits.max_frames {
        return Err(ExecutionError::new(
            None,
            E::FrameLimit {
                limit: limits.max_frames,
                required: count,
            },
        ));
    }
    let total = reserved.checked_add(prepared.slots()).ok_or_else(|| {
        ExecutionError::new(
            None,
            E::FrameSlotLimit {
                limit: limits.max_frame_slots,
                required: usize::MAX,
            },
        )
    })?;
    if total > limits.max_frame_slots {
        return Err(ExecutionError::new(
            None,
            E::FrameSlotLimit {
                limit: limits.max_frame_slots,
                required: total,
            },
        ));
    }
    Ok(total)
}
fn make_frame(
    method: MethodKey,
    prepared: &PreparedMethod,
    args: &[Value],
    origin: Option<(MethodKey, u32)>,
    receiver: Option<Reference>,
) -> Result<Frame, ExecutionError> {
    let mut locals = Locals::new(prepared.max_locals);
    let mut slot = 0;
    if let Some(receiver) = receiver {
        locals.store(0, Value::Reference(receiver), 0)?;
        slot = 1;
    }
    for argument in args {
        locals.store(slot, *argument, 0)?;
        slot += argument.slots();
    }
    Ok(Frame {
        method,
        role: FrameRole::Ordinary,
        pending: None,
        static_access: Vec::new(),
        new_access: Vec::new(),
        constructor: Vec::new(),
        locals,
        stack: Stack::new(prepared.max_stack),
        pc: 0,
        origin,
        reserved_slots: prepared.slots(),
    })
}
fn resolve_call(
    class: &ClassFile,
    index: u16,
    offset: u32,
    admitted: bool,
) -> Result<usize, ExecutionError> {
    let invalid =
        |reason| ExecutionError::new(Some(offset), E::InvalidCallReference { index, reason });
    let pool = &class.constant_pool;
    let (owner, nat, is_interface) = match pool
        .get(index)
        .map_err(|_| invalid("unusable constant-pool index"))?
    {
        Constant::Methodref {
            class_index,
            name_and_type_index,
        } => (*class_index, *name_and_type_index, false),
        Constant::InterfaceMethodref {
            class_index,
            name_and_type_index,
        } => (*class_index, *name_and_type_index, true),
        _ => return Err(invalid("expected Methodref or InterfaceMethodref")),
    };
    let owner = pool
        .class_name(owner)
        .map_err(|_| invalid("owner must be a Class constant with a Utf8 name"))?;
    let this_name = class
        .name()
        .map_err(|_| invalid("invalid supplied class identity"))?;
    if owner != this_name {
        return Err(ExecutionError::new(
            Some(offset),
            E::UnsupportedCall(
                "cross-class owner requires runtime class identity, resolution and initialization",
            ),
        ));
    }
    if is_interface && class.version.major < 52 {
        return Err(invalid(
            "InterfaceMethodref invokestatic requires class-file version 52 or later",
        ));
    }
    if is_interface != (class.access_flags & 0x0200 != 0) {
        return Err(invalid(
            "reference tag does not match the supplied class/interface owner kind",
        ));
    }
    let Constant::NameAndType {
        name_index,
        descriptor_index,
    } = pool
        .get(nat)
        .map_err(|_| invalid("invalid name-and-type index"))?
    else {
        return Err(invalid("expected a NameAndType constant"));
    };
    let name = pool
        .utf8(*name_index)
        .map_err(|_| invalid("invalid method name index"))?;
    let descriptor = pool
        .utf8(*descriptor_index)
        .map_err(|_| invalid("invalid method descriptor index"))?;
    // Immutable registry metadata already passed descriptor grammar validation.
    // Avoid owning nominal names before the runtime shape operation charges work.
    if !admitted {
        parse_method_descriptor(descriptor.units())
            .map_err(|_| invalid("invalid method descriptor"))?;
    }
    if name.equals_ascii("<init>") || name.equals_ascii("<clinit>") {
        return Err(ExecutionError::new(
            Some(offset),
            E::UnsupportedCall(
                "initialization methods are excluded from ordinary static body calls",
            ),
        ));
    }
    let mut matches = class.methods.iter().enumerate().filter(|(_, method)| {
        pool.utf8(method.name_index)
            .is_ok_and(|candidate| candidate == name)
            && pool
                .utf8(method.descriptor_index)
                .is_ok_and(|candidate| candidate == descriptor)
    });
    let Some((method_index, _)) = matches.next() else {
        return Err(ExecutionError::new(Some(offset), E::MissingDeclaredMethod));
    };
    if matches.next().is_some() {
        return Err(invalid("ambiguous duplicate declared method target"));
    }
    Ok(method_index)
}

/// Evaluate selected numeric method bodies. This is not Java invocation or class
/// initialization; supported invokestatic edges stay inside this supplied class.
pub fn execute_static_method_body(
    class: &ClassFile,
    method_index: usize,
    args: &[Value],
    options: ExecutionOptions,
) -> Result<ExecutionOutcome, ExecutionError> {
    execute_static_method_body_with_limits(
        class,
        method_index,
        args,
        options,
        CallLimits::default(),
    )
}
/// The same body-graph kernel with explicit active-frame, slot and prepared-Code policy.
pub fn execute_static_method_body_with_limits(
    class: &ClassFile,
    method_index: usize,
    args: &[Value],
    options: ExecutionOptions,
    limits: CallLimits,
) -> Result<ExecutionOutcome, ExecutionError> {
    execute_with_services(class, method_index, args, options, limits, Services::None)
}

/// Evaluate a numeric/void body graph using the caller's persistent primitive-array
/// heap. Successful earlier heap effects remain visible after every exit path.
pub fn execute_static_method_body_with_heap(
    class: &ClassFile,
    method_index: usize,
    arguments: &[Value],
    options: ExecutionOptions,
    limits: CallLimits,
    heap: &mut ManagedHeap,
) -> Result<ExecutionOutcome, ExecutionError> {
    execute_with_services(
        class,
        method_index,
        arguments,
        options,
        limits,
        Services::Heap(heap),
    )
}

fn instance_root_class<'registry>(
    context: &FieldResolutionContext<'registry>,
    method: MethodId,
) -> Result<&'registry ClassFile, ExecutionError> {
    let registry = context.registry();
    registry.method(method).map_err(|error| {
        ExecutionError::new(
            None,
            E::FieldAccess(Box::new(FieldAccessError::Registry(error))),
        )
    })?;
    registry.class(method.owner()).map_err(|error| {
        ExecutionError::new(
            None,
            E::FieldAccess(Box::new(FieldAccessError::Registry(error))),
        )
    })
}

/// Evaluate an explicitly selected ordinary instance root without Java root
/// invocation/dispatch or initialization. Its body may perform supported bounded
/// virtual/interface dispatch and ordinary special selection through its runtime context.
pub fn execute_instance_method_body(
    context: &mut FieldResolutionContext<'_>,
    method: MethodId,
    receiver: Reference,
    arguments: &[Value],
    options: ExecutionOptions,
    limits: CallLimits,
) -> Result<ExecutionOutcome, ExecutionError> {
    let class = instance_root_class(context, method)?;
    execute_with_services(
        class,
        method.index(),
        arguments,
        options,
        limits,
        Services::Fields {
            context,
            root: Some(method),
            receiver: Some(receiver),
            statics: None,
        },
    )
}

/// Opt in to throwing supplied checked Throwable identities and ordered handlers.
/// VM exception production remains an explicit host unsupported boundary.
#[allow(clippy::too_many_arguments)] // Exact public contract keeps the independent policies explicit.
pub fn execute_instance_method_body_with_exceptions(
    context: &mut FieldResolutionContext<'_>,
    profile: &ExceptionProfile,
    method: MethodId,
    receiver: Reference,
    arguments: &[Value],
    options: ExecutionOptions,
    call_limits: CallLimits,
    exception_limits: ExceptionLimits,
) -> Result<ExceptionOutcome, ExecutionError> {
    context
        .check_exception_profile(profile)
        .map_err(|error| exception_failure(error, None))?;
    let class = instance_root_class(context, method)?;
    exception_outcome(execute_engine(
        class,
        method.index(),
        arguments,
        options,
        call_limits,
        Services::Fields {
            context,
            root: Some(method),
            receiver: Some(receiver),
            statics: None,
        },
        Some(ExceptionState {
            profile,
            limits: exception_limits,
            prepared_handlers: 0,
            dispatch_steps: 0,
        }),
    )?)
}

/// Access explicitly allocated host-controlled static memory without initialization.
pub fn execute_instance_method_body_with_static_state(
    context: &mut FieldResolutionContext<'_>,
    statics: &StaticStateProfile,
    method: MethodId,
    receiver: Reference,
    arguments: &[Value],
    options: ExecutionOptions,
    call_limits: CallLimits,
) -> Result<ExecutionOutcome, ExecutionError> {
    context
        .check_static_profile(statics)
        .map_err(|error| ExecutionError::new(None, E::StaticAccess(Box::new(error))))?;
    let class = instance_root_class(context, method)?;
    execute_with_services(
        class,
        method.index(),
        arguments,
        options,
        call_limits,
        Services::Fields {
            context,
            root: Some(method),
            receiver: Some(receiver),
            statics: Some(statics),
        },
    )
}

/// Compose host-controlled static memory with supplied-Throwable dispatch.
#[allow(clippy::too_many_arguments)] // Exact public contract keeps independent capabilities/policies explicit.
pub fn execute_instance_method_body_with_static_state_and_exceptions(
    context: &mut FieldResolutionContext<'_>,
    statics: &StaticStateProfile,
    exceptions: &ExceptionProfile,
    method: MethodId,
    receiver: Reference,
    arguments: &[Value],
    options: ExecutionOptions,
    call_limits: CallLimits,
    exception_limits: ExceptionLimits,
) -> Result<ExceptionOutcome, ExecutionError> {
    context
        .check_static_profile(statics)
        .map_err(|error| ExecutionError::new(None, E::StaticAccess(Box::new(error))))?;
    context
        .check_exception_profile(exceptions)
        .map_err(|error| exception_failure(error, None))?;
    let class = instance_root_class(context, method)?;
    exception_outcome(execute_engine(
        class,
        method.index(),
        arguments,
        options,
        call_limits,
        Services::Fields {
            context,
            root: Some(method),
            receiver: Some(receiver),
            statics: Some(statics),
        },
        Some(ExceptionState {
            profile: exceptions,
            limits: exception_limits,
            prepared_handlers: 0,
            dispatch_steps: 0,
        }),
    )?)
}

fn exception_outcome(outcome: EngineOutcome) -> Result<ExceptionOutcome, ExecutionError> {
    match outcome {
        EngineOutcome::Legacy(ExecutionOutcome::Returned(value)) => {
            Ok(ExceptionOutcome::Returned(value))
        }
        EngineOutcome::Uncaught(reference) => Ok(ExceptionOutcome::Uncaught(reference)),
        EngineOutcome::Legacy(_) | EngineOutcome::InitializationCompleted => {
            Err(ExecutionError::new(
                None,
                E::InvalidMethod("symbolic completion escaped explicit exception profile"),
            ))
        }
    }
}

enum Services<'a, 'registry> {
    None,
    Heap(&'a mut ManagedHeap),
    Fields {
        context: &'a mut FieldResolutionContext<'registry>,
        root: Option<MethodId>,
        receiver: Option<Reference>,
        statics: Option<&'a StaticStateProfile>,
    },
}
impl<'registry> Services<'_, 'registry> {
    fn registry(&self) -> Option<&'registry DefinitionRegistry> {
        match self {
            Self::Fields { context, .. } => Some(context.registry()),
            _ => None,
        }
    }
    fn heap(&self) -> Option<&ManagedHeap> {
        match self {
            Self::None => None,
            Self::Heap(heap) => Some(heap),
            Self::Fields { context, .. } => Some(context.heap()),
        }
    }
    fn heap_mut(&mut self) -> Option<&mut ManagedHeap> {
        match self {
            Self::None => None,
            Self::Heap(heap) => Some(heap),
            Self::Fields { context, .. } => Some(context.heap_mut()),
        }
    }
}

fn execute_with_services(
    class: &ClassFile,
    method_index: usize,
    args: &[Value],
    options: ExecutionOptions,
    limits: CallLimits,
    services: Services<'_, '_>,
) -> Result<ExecutionOutcome, ExecutionError> {
    match execute_engine(class, method_index, args, options, limits, services, None)? {
        EngineOutcome::Legacy(outcome) => Ok(outcome),
        EngineOutcome::Uncaught(_) | EngineOutcome::InitializationCompleted => {
            Err(ExecutionError::new(
                None,
                E::InvalidMethod("thrown completion escaped legacy body profile"),
            ))
        }
    }
}
pub(crate) struct SessionExecution<'a, 'registry> {
    pub context: &'a mut FieldResolutionContext<'registry>,
    pub claim: &'a InitializationClaim,
    pub statics: &'a StaticStateProfile,
    pub exceptions: &'a ExceptionProfile,
    pub state: &'a mut SessionState,
    pub owner: u64,
    pub monitors: Option<crate::MonitorLimits>,
}
pub(crate) enum InitializationRoot<'a> {
    Application {
        named: ClassId,
        method: MethodId,
        arguments: &'a [Value],
    },
    Class(ClassId),
    Static {
        method: MethodId,
        arguments: &'a [Value],
    },
}
pub(crate) enum InitializationResult {
    Completed,
    Returned(Option<Value>),
    Uncaught(Reference),
}
pub(crate) fn execute_initializing(
    execution: SessionExecution<'_, '_>,
    root: InitializationRoot<'_>,
    options: ExecutionOptions,
    limits: CallLimits,
    exception_limits: ExceptionLimits,
) -> Result<InitializationResult, ExecutionError> {
    let SessionExecution {
        context,
        claim,
        statics,
        exceptions,
        state,
        owner,
        monitors,
    } = execution;
    context
        .check_initialization_claim(claim)
        .map_err(|error| initialization_failure(error, None))?;
    let root = match root {
        InitializationRoot::Class(class) => EngineRoot::Initialize(class),
        InitializationRoot::Application {
            named,
            method,
            arguments,
        } => EngineRoot::Static {
            named,
            method,
            args: arguments,
        },
        InitializationRoot::Static { method, arguments } => EngineRoot::Static {
            named: method.owner(),
            method,
            args: arguments,
        },
    };
    let outcome = run_engine(
        root,
        options,
        limits,
        Services::Fields {
            context,
            root: None,
            receiver: None,
            statics: Some(statics),
        },
        Some(ExceptionState {
            profile: exceptions,
            limits: exception_limits,
            prepared_handlers: 0,
            dispatch_steps: 0,
        }),
        Some(InitExecution {
            claim,
            state,
            owner,
            activations: Vec::new(),
        }),
        monitors,
    )?;
    match outcome {
        EngineOutcome::InitializationCompleted => Ok(InitializationResult::Completed),
        EngineOutcome::Legacy(ExecutionOutcome::Returned(value)) => {
            Ok(InitializationResult::Returned(value))
        }
        EngineOutcome::Uncaught(reference) => Ok(InitializationResult::Uncaught(reference)),
        EngineOutcome::Legacy(_) => Err(ExecutionError::new(
            None,
            E::InvalidMethod("symbolic outcome escaped initialization session"),
        )),
    }
}
enum InitContinuation {
    RootInitialize,
    RootStatic {
        target: MethodKey,
        args: Vec<Value>,
    },
    Instruction {
        frame: usize,
        method: MethodKey,
        pc: u32,
    },
    Dependency,
}
struct InitRequest {
    class: ClassId,
    continuation: InitContinuation,
    origin: Option<(MethodKey, u32)>,
}
struct Activation {
    class: ClassId,
    record: usize,
    next_dependency: usize,
    phase: Phase,
    continuation: InitContinuation,
    origin: Option<(MethodKey, u32)>,
}
struct InitExecution<'a> {
    claim: &'a InitializationClaim,
    state: &'a mut SessionState,
    owner: u64,
    activations: Vec<Activation>,
}
impl InitExecution<'_> {
    fn finish_body(&mut self, class: ClassId) -> Result<(), ExecutionError> {
        let activation = self.activations.last_mut().ok_or_else(|| {
            ExecutionError::new(None, E::InvalidMethod("initializer activation absent"))
        })?;
        if activation.class != class || activation.phase != Phase::Body {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("initializer activation role mismatch"),
            ));
        }
        let record = self
            .state
            .records
            .get_mut(activation.record)
            .ok_or_else(|| {
                ExecutionError::new(None, E::InvalidMethod("initializer record absent"))
            })?;
        if !matches!(record.status,RecordStatus::InProgress{owner,..} if owner==self.owner) {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("initializer record owner mismatch"),
            ));
        }
        activation.phase = Phase::Completing;
        record.status = RecordStatus::InProgress {
            owner: self.owner,
            phase: Phase::Completing,
        };
        Ok(())
    }
}
fn initialization_failure(error: InitializationAccessError, pc: Option<u32>) -> ExecutionError {
    match error {
        InitializationAccessError::Java(kind) => ExecutionError::new(
            pc,
            E::UnsupportedVmException(match kind {
                StaticJavaError::NoSuchField => VmExceptionKind::NoSuchFieldError,
                StaticJavaError::IllegalAccess => VmExceptionKind::IllegalAccessError,
                StaticJavaError::LinkageError => VmExceptionKind::LinkageError,
                StaticJavaError::IncompatibleClassChange => {
                    VmExceptionKind::IncompatibleClassChangeError
                }
            }),
        ),
        error => ExecutionError::new(pc, E::InitializationAccess(Box::new(error))),
    }
}
fn init_diagnose(
    mut error: ExecutionError,
    class: ClassId,
    origin: Option<(MethodKey, u32)>,
    bodies: Bodies<'_>,
    java: &JavaState,
) -> ExecutionError {
    error.initialization_class.get_or_insert(class);
    if error.method.is_none() {
        if let Some((method, pc)) = origin {
            error.offset = Some(pc);
            let ancestor = java
                .frames
                .iter()
                .rev()
                .find(|frame| frame.method == method)
                .and_then(|frame| frame.origin);
            error = contextualize(bodies, error, method, ancestor);
        }
    }
    error
}
fn check_static_continuation_empty(frame: &Frame, pc: u32) -> Result<(), ExecutionError> {
    if frame.pending.is_some() {
        return Err(ExecutionError::new(
            Some(pc),
            E::InvalidMethod("duplicate active-use continuation"),
        ));
    }
    if !frame.static_access.is_empty() {
        return Err(ExecutionError::new(
            Some(pc),
            E::InvalidMethod("static continuation token already retained"),
        ));
    }
    Ok(())
}
fn take_static_continuation(
    frame: &mut Frame,
    pc: u32,
) -> Result<ResolvedInitializationStatic, ExecutionError> {
    if frame.static_access.len() != 1 {
        return Err(ExecutionError::new(
            Some(pc),
            E::InvalidMethod("active-use continuation does not match suspended frame"),
        ));
    }
    frame.static_access.pop().ok_or_else(|| {
        ExecutionError::new(
            Some(pc),
            E::InvalidMethod("active-use continuation does not match suspended frame"),
        )
    })
}
fn session_totals(
    current: usize,
    additional: usize,
    limit: usize,
    resource: InitializationResource,
) -> Result<usize, ExecutionError> {
    let required = current
        .checked_add(additional)
        .ok_or_else(|| init_overflow("retained plan totals"))?;
    if required > limit {
        return Err(ExecutionError::new(
            None,
            E::InitializationLimit {
                resource,
                limit,
                required,
            },
        ));
    }
    Ok(required)
}
fn prepare_initializer(
    method: MethodId,
    bodies: Bodies<'_>,
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    exceptions: Option<&mut ExceptionState<'_>>,
    limits: CallLimits,
    origin: Option<(MethodKey, u32)>,
) -> Result<(), ExecutionError> {
    let class = method.owner();
    let key = MethodKey::Registered(method);
    if let std::collections::hash_map::Entry::Vacant(entry) = java.cache.entry(key) {
        let metadata = bodies.class(key)?;
        let Services::Fields { context, .. } = services else {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("initializer context absent"),
            ));
        };
        let prepared = prepare_method_with_context(
            metadata,
            method.index(),
            &mut java.used_code_bytes,
            limits.max_prepared_code_bytes,
            Preparation::Initializer,
            Some((&mut **context, method, java.monitors.is_some())),
            exceptions,
        )
        .map_err(|mut error| {
            error.initialization_class = Some(class);
            contextualize(bodies, error, key, origin)
        })?;
        entry.insert(prepared);
    }
    let prepared = java.cache.get(&key).ok_or_else(|| {
        ExecutionError::new(None, E::InvalidMethod("initializer preparation absent"))
    })?;
    if !prepared.initializer || prepared.constructor || prepared.instance {
        return Err(ExecutionError::new(
            None,
            E::InvalidMethod("ordinary cached body cannot execute as initializer"),
        ));
    }
    Ok(())
}
fn request_initialization(
    request: InitRequest,
    bodies: Bodies<'_>,
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    exceptions: Option<&mut ExceptionState<'_>>,
    init: &mut InitExecution<'_>,
    limits: CallLimits,
) -> Result<Option<EngineOutcome>, ExecutionError> {
    let InitRequest {
        class,
        continuation,
        origin,
    } = request;
    let result = (|| {
        init.state.charge()?;
        let Services::Fields { context, .. } = services else {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("initialization context absent"),
            ));
        };
        context
            .check_initialization_class(init.claim, class)
            .map_err(|error| initialization_failure(error, None))?;
        if let Some(index) = init.state.record(class)? {
            match init
                .state
                .records
                .get(index)
                .ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("initialization record absent"))
                })?
                .status
            {
                RecordStatus::Completed => {
                    return resume_initialization(
                        continuation,
                        bodies,
                        java,
                        services,
                        init,
                        limits,
                    );
                }
                RecordStatus::InProgress { owner, .. } if owner == init.owner => {
                    return resume_initialization(
                        continuation,
                        bodies,
                        java,
                        services,
                        init,
                        limits,
                    );
                }
                RecordStatus::InProgress { .. } => {
                    return Err(ExecutionError::new(
                        None,
                        E::InvalidMethod("initialization belongs to another evaluation"),
                    ));
                }
                RecordStatus::Erroneous { .. } => {
                    return Err(ExecutionError::new(
                        None,
                        E::UnsupportedNoClassDefFoundError { class },
                    ));
                }
                RecordStatus::Aborted => {
                    return Err(ExecutionError::new(
                        None,
                        E::InitializationAborted { class },
                    ));
                }
            }
        }
        let dependency_limit = init
            .state
            .limits
            .max_plan_dependencies
            .checked_sub(init.state.dependencies)
            .ok_or_else(|| init_overflow("remaining plan dependencies"))?;
        let constant_limit = init
            .state
            .limits
            .max_constant_values
            .checked_sub(init.state.constants)
            .ok_or_else(|| init_overflow("remaining constants"))?;
        let plan = context
            .plan_initialization(
                init.claim,
                class,
                InitializationPlanLimits {
                    max_dependencies: dependency_limit,
                    max_constants: constant_limit,
                },
            )
            .map_err(|error| initialization_failure(error, None))?;
        let dependencies = session_totals(
            init.state.dependencies,
            plan.dependencies().len(),
            init.state.limits.max_plan_dependencies,
            InitializationResource::PlanDependencies,
        )?;
        let constants = session_totals(
            init.state.constants,
            plan.constant_count(),
            init.state.limits.max_constant_values,
            InitializationResource::ConstantValues,
        )?;
        if let Some(method) = plan.initializer() {
            if method.owner() != class {
                return Err(ExecutionError::new(
                    None,
                    E::InvalidMethod("initializer plan owner mismatch"),
                ));
            }
            prepare_initializer(method, bodies, java, services, exceptions, limits, origin)?;
        }
        let pending = init
            .activations
            .len()
            .checked_add(1)
            .ok_or_else(|| init_overflow("pending activations"))?;
        if pending > init.state.limits.max_pending {
            return Err(ExecutionError::new(
                None,
                E::InitializationLimit {
                    resource: InitializationResource::Pending,
                    limit: init.state.limits.max_pending,
                    required: pending,
                },
            ));
        }
        init.activations
            .try_reserve(1)
            .map_err(|_| init_allocation("initialization activations"))?;
        if init.state.records.len() != init.state.indices.len()
            || init.state.records.len() >= init.state.admitted_count
            || init.state.records.len() >= init.state.records.capacity()
            || init.state.indices.len() >= init.state.indices.capacity()
        {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("initialization record reservation invariant"),
            ));
        }
        init.state.charge()?;
        let index = init.state.records.len();
        init.state.records.push(Record {
            class,
            status: RecordStatus::InProgress {
                owner: init.owner,
                phase: Phase::Dependencies,
            },
            plan,
        });
        init.state.indices.insert(class, index);
        init.state.dependencies = dependencies;
        init.state.constants = constants;
        init.activations.push(Activation {
            class,
            record: index,
            next_dependency: 0,
            phase: Phase::Dependencies,
            continuation,
            origin,
        });
        let Services::Fields { context, .. } = services else {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("initialization context absent"),
            ));
        };
        let plan = &init
            .state
            .records
            .get(index)
            .ok_or_else(|| ExecutionError::new(None, E::InvalidMethod("constant plan absent")))?
            .plan;
        context
            .install_initialization_constants(init.claim, plan)
            .map_err(|error| initialization_failure(error, None))?;
        Ok(None)
    })();
    result.map_err(|error| init_diagnose(error, class, origin, bodies, java))
}
fn resume_initialization(
    continuation: InitContinuation,
    bodies: Bodies<'_>,
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    init: &mut InitExecution<'_>,
    limits: CallLimits,
) -> Result<Option<EngineOutcome>, ExecutionError> {
    match continuation {
        InitContinuation::Dependency => Ok(None),
        InitContinuation::RootInitialize => {
            if !java.frames.is_empty() {
                return Err(ExecutionError::new(
                    None,
                    E::InvalidMethod("frames survived root initialization"),
                ));
            }
            Ok(Some(EngineOutcome::InitializationCompleted))
        }
        InitContinuation::RootStatic { target, args } => {
            if !java.frames.is_empty() {
                return Err(ExecutionError::new(
                    None,
                    E::InvalidMethod("frames survived static root initialization"),
                ));
            }
            let prepared = java.cache.get(&target).ok_or_else(|| {
                ExecutionError::new(None, E::InvalidMethod("deferred root preparation absent"))
            })?;
            if prepared.initializer || prepared.constructor || prepared.instance {
                return Err(ExecutionError::new(
                    None,
                    E::UnsupportedMethod("ordinary static root role mismatch"),
                ));
            }
            let reserved = check_reservation(0, 0, prepared, limits)
                .map_err(|error| contextualize(bodies, error, target, None))?;
            java.frames
                .try_reserve(1)
                .map_err(|_| init_allocation("static root frame"))?;
            let frame = make_frame(target, prepared, &args, None, None)
                .map_err(|error| contextualize(bodies, error, target, None))?;
            activate_monitors(
                java.monitors.as_mut(),
                java.frames.len(),
                &frame,
                prepared,
                services,
                Some(init.claim),
            )
            .map_err(|e| activation_error(bodies, e, &frame))?;
            java.frames.push(frame);
            java.reserved = reserved;
            Ok(None)
        }
        InitContinuation::Instruction { frame, method, pc } => {
            let invalid = || {
                ExecutionError::new(
                    Some(pc),
                    E::InvalidMethod("active-use continuation does not match suspended frame"),
                )
            };
            if frame.checked_add(1) != Some(java.frames.len()) {
                return Err(invalid());
            }
            let caller = java.frames.get_mut(frame).ok_or_else(invalid)?;
            if caller.method != method || caller.pc != pc {
                return Err(invalid());
            }
            let origin = caller.origin;
            let pending = caller.pending.take().ok_or_else(invalid)?;
            let diagnose = |error| contextualize(bodies, error, method, origin);
            match pending {
                PendingUse::New { next } => {
                    if caller.new_access.len() != 1 {
                        return Err(invalid());
                    }
                    let access = caller.new_access.pop().ok_or_else(invalid)?;
                    let Services::Fields { context, .. } = services else {
                        return Err(invalid());
                    };
                    let MethodKey::Registered(method) = method else {
                        return Err(invalid());
                    };
                    let reference = context
                        .allocate_new(init.claim, &access)
                        .map_err(|e| diagnose(construction_failure(e, pc)))?;
                    caller.locals.invalidate_site(method, pc);
                    caller.stack.publish(Uninitialized {
                        reference,
                        key: AliasKey::Allocation { method, offset: pc },
                    });
                    caller.pc = next;
                }
                PendingUse::Static { value, next } => {
                    let access = take_static_continuation(caller, pc)?;
                    let Services::Fields { context, .. } = services else {
                        return Err(invalid());
                    };
                    if let Some(value) = value {
                        context
                            .write_initialization_static(init.claim, &access, value)
                            .map_err(|error| diagnose(initialization_failure(error, Some(pc))))?;
                    } else {
                        let value = context
                            .read_initialization_static(init.claim, &access)
                            .map_err(|error| diagnose(initialization_failure(error, Some(pc))))?;
                        caller.stack.push(value, pc).map_err(diagnose)?;
                    }
                    caller.pc = next;
                }
                PendingUse::Call { target, args, next } => {
                    let prepared = java.cache.get(&target).ok_or_else(invalid)?;
                    if prepared.initializer || prepared.constructor || prepared.instance {
                        return Err(diagnose(ExecutionError::new(
                            Some(pc),
                            E::UnsupportedMethod("ordinary static continuation role mismatch"),
                        )));
                    }
                    let reserved =
                        check_reservation(java.frames.len(), java.reserved, prepared, limits)
                            .map_err(|error| {
                                contextualize(bodies, error, target, Some((method, pc)))
                            })?;
                    let callee = make_frame(target, prepared, &args, Some((method, pc)), None)
                        .map_err(|error| {
                            contextualize(bodies, error, target, Some((method, pc)))
                        })?;
                    java.frames
                        .try_reserve(1)
                        .map_err(|_| init_allocation("resumed static call frame"))?;
                    let depth = java.frames.len();
                    let caller = java.frames.get_mut(frame).ok_or_else(invalid)?;
                    activate_monitors(
                        java.monitors.as_mut(),
                        depth,
                        &callee,
                        prepared,
                        services,
                        Some(init.claim),
                    )
                    .map_err(|e| activation_error(bodies, e, &callee))?;
                    caller.pc = next;
                    java.frames.push(callee);
                    java.reserved = reserved;
                }
            }
            Ok(None)
        }
    }
}
fn advance_initialization(
    bodies: Bodies<'_>,
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    mut exceptions: Option<&mut ExceptionState<'_>>,
    init: &mut InitExecution<'_>,
    limits: CallLimits,
) -> Result<Option<EngineOutcome>, ExecutionError> {
    loop {
        let Some(activation) = init.activations.last() else {
            return Ok(None);
        };
        if activation.phase == Phase::Body {
            return Ok(None);
        }
        let class = activation.class;
        let origin = activation.origin;
        let record_index = activation.record;
        let phase = activation.phase;
        let next = activation.next_dependency;
        let result = (|| {
            if phase == Phase::Completing {
                init.state.charge()?;
                let record = init.state.records.get_mut(record_index).ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("completion record absent"))
                })?;
                if !matches!(record.status,RecordStatus::InProgress{owner,..} if owner==init.owner)
                {
                    return Err(ExecutionError::new(
                        None,
                        E::InvalidMethod("completion owner mismatch"),
                    ));
                }
                record.status = RecordStatus::Completed;
                let completed = init.activations.pop().ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("completion activation absent"))
                })?;
                return resume_initialization(
                    completed.continuation,
                    bodies,
                    java,
                    services,
                    init,
                    limits,
                );
            }
            let record = init.state.records.get(record_index).ok_or_else(|| {
                ExecutionError::new(None, E::InvalidMethod("dependency record absent"))
            })?;
            if let Some(&dependency) = record.plan.dependencies().get(next) {
                init.state.charge()?;
                let activation = init.activations.last_mut().ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("dependency activation absent"))
                })?;
                activation.next_dependency = activation
                    .next_dependency
                    .checked_add(1)
                    .ok_or_else(|| init_overflow("dependency cursor"))?;
                return request_initialization(
                    InitRequest {
                        class: dependency,
                        continuation: InitContinuation::Dependency,
                        origin,
                    },
                    bodies,
                    java,
                    services,
                    exceptions.as_deref_mut(),
                    init,
                    limits,
                );
            }
            let initializer = record.plan.initializer();
            init.state.charge()?;
            let activation = init.activations.last_mut().ok_or_else(|| {
                ExecutionError::new(None, E::InvalidMethod("body activation absent"))
            })?;
            let record =
                init.state.records.get_mut(record_index).ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("body record absent"))
                })?;
            if let Some(method) = initializer {
                let key = MethodKey::Registered(method);
                let prepared = java.cache.get(&key).ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("initializer cache absent"))
                })?;
                if !prepared.initializer || prepared.constructor || prepared.instance {
                    return Err(ExecutionError::new(
                        None,
                        E::InvalidMethod("initializer cache role mismatch"),
                    ));
                }
                let reserved =
                    check_reservation(java.frames.len(), java.reserved, prepared, limits)
                        .map_err(|error| contextualize(bodies, error, key, origin))?;
                java.frames
                    .try_reserve(1)
                    .map_err(|_| init_allocation("initializer frames"))?;
                let mut frame = make_frame(key, prepared, &[], origin, None)
                    .map_err(|error| contextualize(bodies, error, key, origin))?;
                frame.role = FrameRole::Initializer(class);
                activate_monitors(
                    java.monitors.as_mut(),
                    java.frames.len(),
                    &frame,
                    prepared,
                    services,
                    Some(init.claim),
                )
                .map_err(|e| activation_error(bodies, e, &frame))?;
                activation.phase = Phase::Body;
                record.status = RecordStatus::InProgress {
                    owner: init.owner,
                    phase: Phase::Body,
                };
                java.frames.push(frame);
                java.reserved = reserved;
            } else {
                activation.phase = Phase::Completing;
                record.status = RecordStatus::InProgress {
                    owner: init.owner,
                    phase: Phase::Completing,
                };
            }
            Ok(None)
        })();
        if let Some(outcome) =
            result.map_err(|error| init_diagnose(error, class, origin, bodies, java))?
        {
            return Ok(Some(outcome));
        }
    }
}

enum EngineRoot<'a> {
    Body {
        class: &'a ClassFile,
        method_index: usize,
        args: &'a [Value],
    },
    Initialize(ClassId),
    Static {
        named: ClassId,
        method: MethodId,
        args: &'a [Value],
    },
}
struct JavaState {
    cache: HashMap<MethodKey, PreparedMethod>,
    frames: Vec<Frame>,
    reserved: usize,
    used_code_bytes: usize,
    remaining: u64,
    monitors: Option<MonitorLedger>,
}
fn execute_engine(
    class: &ClassFile,
    method_index: usize,
    args: &[Value],
    options: ExecutionOptions,
    limits: CallLimits,
    services: Services<'_, '_>,
    exceptions: Option<ExceptionState<'_>>,
) -> Result<EngineOutcome, ExecutionError> {
    run_engine(
        EngineRoot::Body {
            class,
            method_index,
            args,
        },
        options,
        limits,
        services,
        exceptions,
        None,
        None,
    )
}
fn run_engine(
    root: EngineRoot<'_>,
    options: ExecutionOptions,
    limits: CallLimits,
    mut services: Services<'_, '_>,
    mut exceptions: Option<ExceptionState<'_>>,
    mut initialization: Option<InitExecution<'_>>,
    monitor_limits: Option<crate::MonitorLimits>,
) -> Result<EngineOutcome, ExecutionError> {
    let result = (|| {
        let bodies = Bodies {
            standalone: match &root {
                EngineRoot::Body { class, .. } => Some(*class),
                _ => None,
            },
            registry: services.registry(),
        };
        let mut java = JavaState {
            cache: HashMap::new(),
            frames: Vec::new(),
            reserved: 0,
            used_code_bytes: 0,
            remaining: options.instruction_budget,
            monitors: monitor_limits.map(MonitorLedger::new),
        };
        let requested = match root {
            EngineRoot::Initialize(class) => Some((class, InitContinuation::RootInitialize)),
            root => {
                let (root_key, args, receiver, active_class) = match root {
                    EngineRoot::Body {
                        method_index, args, ..
                    } => {
                        let key = match &services {
                            Services::Fields {
                                root: Some(method), ..
                            } => MethodKey::Registered(*method),
                            _ => MethodKey::Standalone(method_index),
                        };
                        let receiver = match &services {
                            Services::Fields { receiver, .. } => *receiver,
                            _ => None,
                        };
                        (key, args, receiver, None)
                    }
                    EngineRoot::Static {
                        named,
                        method,
                        args,
                    } => (MethodKey::Registered(method), args, None, Some(named)),
                    EngineRoot::Initialize(_) => {
                        return Err(ExecutionError::new(
                            None,
                            E::InvalidMethod("invalid initialization root routing"),
                        ));
                    }
                };
                let class = bodies
                    .class(root_key)
                    .map_err(|error| contextualize(bodies, error, root_key, None))?;
                if let MethodKey::Registered(method) = root_key {
                    bodies
                        .registry
                        .ok_or_else(|| operand_error(0))?
                        .method(method)
                        .map_err(|error| {
                            contextualize(
                                bodies,
                                ExecutionError::new(
                                    None,
                                    E::FieldAccess(Box::new(FieldAccessError::Registry(error))),
                                ),
                                root_key,
                                None,
                            )
                        })?;
                }
                if limits.max_frames == 0 {
                    return Err(contextualize(
                        bodies,
                        ExecutionError::new(
                            None,
                            E::FrameLimit {
                                limit: 0,
                                required: 1,
                            },
                        ),
                        root_key,
                        None,
                    ));
                }
                let root = prepare_method_with_context(
                    class,
                    root_key.index(),
                    &mut java.used_code_bytes,
                    limits.max_prepared_code_bytes,
                    if receiver.is_some() {
                        Preparation::DirectInstanceRoot
                    } else {
                        Preparation::Static
                    },
                    match (&mut services, root_key) {
                        (Services::Fields { context, .. }, MethodKey::Registered(method)) => {
                            Some((&mut **context, method, java.monitors.is_some()))
                        }
                        _ => None,
                    },
                    exceptions.as_mut(),
                )
                .map_err(|error| contextualize(bodies, error, root_key, None))?;
                validate_args(&root, args)
                    .map_err(|error| contextualize(bodies, error, root_key, None))?;
                let checked = check_reservation(0, 0, &root, limits)
                    .map_err(|error| contextualize(bodies, error, root_key, None))?;
                if let (
                    Some(receiver),
                    Services::Fields { context, .. },
                    MethodKey::Registered(method),
                ) = (receiver, &mut services, root_key)
                {
                    context
                        .check_receiver(method.owner(), receiver)
                        .map_err(|error| {
                            contextualize(
                                bodies,
                                ExecutionError::new(None, E::FieldAccess(Box::new(error))),
                                root_key,
                                None,
                            )
                        })?;
                }
                if let (Services::Fields { context, .. }, MethodKey::Registered(method)) =
                    (&mut services, root_key)
                {
                    context
                        .check_method_reference_profile(method, receiver)
                        .and_then(|()| context.check_method_arguments(method, args))
                        .map_err(|error| {
                            contextualize(
                                bodies,
                                ExecutionError::new(None, E::MethodAccess(Box::new(error))),
                                root_key,
                                None,
                            )
                        })?;
                }
                if initialization.is_some() {
                    let mut arguments = Vec::new();
                    arguments
                        .try_reserve_exact(args.len())
                        .map_err(|_| init_allocation("deferred root arguments"))?;
                    arguments.extend_from_slice(args);
                    java.cache.insert(root_key, root);
                    let MethodKey::Registered(method) = root_key else {
                        return Err(ExecutionError::new(
                            None,
                            E::InvalidMethod("initialization root is not registered"),
                        ));
                    };
                    Some((
                        active_class.unwrap_or(method.owner()),
                        InitContinuation::RootStatic {
                            target: root_key,
                            args: arguments,
                        },
                    ))
                } else {
                    let frame = make_frame(root_key, &root, args, None, receiver)
                        .map_err(|error| contextualize(bodies, error, root_key, None))?;
                    activate_monitors(
                        java.monitors.as_mut(),
                        0,
                        &frame,
                        &root,
                        &mut services,
                        None,
                    )
                    .map_err(|e| activation_error(bodies, e, &frame))?;
                    java.cache.insert(root_key, root);
                    java.frames.push(frame);
                    java.reserved = checked;
                    None
                }
            }
        };
        if let Some((class, continuation)) = requested {
            let init = initialization.as_mut().ok_or_else(|| {
                ExecutionError::new(None, E::InvalidMethod("initialization state absent"))
            })?;
            if let Some(outcome) = request_initialization(
                InitRequest {
                    class,
                    continuation,
                    origin: None,
                },
                bodies,
                &mut java,
                &mut services,
                exceptions.as_mut(),
                init,
                limits,
            )? {
                return Ok(outcome);
            }
        }
        loop {
            if let Some(init) = initialization.as_mut() {
                if let Some(outcome) = advance_initialization(
                    bodies,
                    &mut java,
                    &mut services,
                    exceptions.as_mut(),
                    init,
                    limits,
                )? {
                    return Ok(outcome);
                }
            }
            let Some(frame) = java.frames.last() else {
                return Err(ExecutionError::new(
                    None,
                    E::InvalidMethod("empty engine frame stack"),
                ));
            };
            let current = frame.method;
            let pc = frame.pc;
            let origin = frame.origin;
            if java.remaining == 0 {
                return Err(contextualize(
                    bodies,
                    ExecutionError::new(
                        Some(pc),
                        E::InstructionBudgetExhausted {
                            limit: options.instruction_budget,
                        },
                    ),
                    current,
                    origin,
                ));
            }
            let prepared = java.cache.get(&current).ok_or_else(|| {
                contextualize(
                    bodies,
                    ExecutionError::new(Some(pc), E::InvalidMethod("missing prepared method")),
                    current,
                    origin,
                )
            })?;
            let frame = java.frames.last_mut().ok_or_else(|| {
                ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
            })?;
            java.remaining -= 1;
            let current_class = bodies
                .class(current)
                .map_err(|error| contextualize(bodies, error, current, origin))?;
            let action = step(
                current_class,
                prepared,
                frame,
                &mut services,
                exceptions.as_ref().map(|state| state.profile),
                initialization.as_ref().map(|init| init.claim),
                java.monitors.is_some(),
            )
            .map_err(|error| contextualize(bodies, error, current, origin))?;
            match action {
                Action::Monitor {
                    reference,
                    enter,
                    next,
                } => {
                    let depth = java
                        .frames
                        .len()
                        .checked_sub(1)
                        .ok_or_else(|| operand_error(pc))?;
                    let ledger = java.monitors.as_mut().ok_or_else(|| operand_error(pc))?;
                    let handle = reference.heap_ref().ok_or_else(|| operand_error(pc))?;
                    let result = if enter {
                        ledger.enter_explicit(depth, MonitorKey::Object(handle))
                    } else {
                        ledger.exit_explicit(depth, MonitorKey::Object(handle))
                    };
                    result.map_err(|error| {
                        contextualize(bodies, monitor_failure(error, Some(pc)), current, origin)
                    })?;
                    java.frames.last_mut().ok_or_else(|| operand_error(pc))?.pc = next;
                }
                Action::New { access, next } => {
                    let class = access.class();
                    let index = java
                        .frames
                        .len()
                        .checked_sub(1)
                        .ok_or_else(|| operand_error(pc))?;
                    let frame = java.frames.last_mut().ok_or_else(|| operand_error(pc))?;
                    let MethodKey::Registered(method) = current else {
                        return Err(operand_error(pc));
                    };
                    if frame.pending.is_some() || !frame.new_access.is_empty() {
                        return Err(protocol(pc, "duplicate new continuation"));
                    }
                    frame
                        .stack
                        .reserve_one(pc)
                        .map_err(|e| contextualize(bodies, e, current, origin))?;
                    frame
                        .new_access
                        .try_reserve_exact(1)
                        .map_err(|_| init_allocation("new continuation token"))?;
                    let init = initialization.as_mut().ok_or_else(|| operand_error(pc))?;
                    init.state
                        .construction
                        .charge(frame.locals.len(), frame.stack.len(), pc)
                        .map_err(|e| contextualize(bodies, e, current, origin))?;
                    if frame.stack.contains_site(method, pc) {
                        return Err(contextualize(
                            bodies,
                            protocol(pc, "unfinished stack reference from repeated new site"),
                            current,
                            origin,
                        ));
                    }
                    frame.new_access.push(access);
                    frame.pending = Some(PendingUse::New { next });
                    if let Some(outcome) = request_initialization(
                        InitRequest {
                            class,
                            continuation: InitContinuation::Instruction {
                                frame: index,
                                method: current,
                                pc,
                            },
                            origin: Some((current, pc)),
                        },
                        bodies,
                        &mut java,
                        &mut services,
                        exceptions.as_mut(),
                        init,
                        limits,
                    )? {
                        return Ok(outcome);
                    }
                }
                Action::ConstructorCall { constant_index } => {
                    let site = Some((current, pc));
                    let MethodKey::Registered(method) = current else {
                        return Err(operand_error(pc));
                    };
                    let Services::Fields { context, .. } = &mut services else {
                        return Err(operand_error(pc));
                    };
                    let init = initialization.as_mut().ok_or_else(|| operand_error(pc))?;
                    let shape = context
                        .constructor_invocation_shape(method.owner(), constant_index)
                        .map_err(|e| {
                            contextualize(bodies, construction_failure(e, pc), current, site)
                        })?;
                    let caller = java.frames.last_mut().ok_or_else(|| operand_error(pc))?;
                    let mut args = Vec::new();
                    args.try_reserve_exact(shape.parameters.len())
                        .map_err(|_| init_allocation("constructor arguments"))?;
                    for kind in shape.parameters.iter().rev() {
                        args.push(
                            caller
                                .stack
                                .pop(*kind, pc)
                                .map_err(|e| contextualize(bodies, e, current, site))?,
                        );
                    }
                    args.reverse();
                    let receiver = caller
                        .stack
                        .pop_reference(pc)
                        .map_err(|e| contextualize(bodies, e, current, site))?;
                    let FrameValue::Uninitialized(alias) = receiver else {
                        return Err(contextualize(
                            bodies,
                            protocol(pc, "constructor receiver is not unfinished"),
                            current,
                            site,
                        ));
                    };
                    let kind = match alias.key {
                        AliasKey::Allocation { .. } => ConstructorReceiverKind::Allocation,
                        AliasKey::This(_) => {
                            pending_this(caller, alias, pc)
                                .map_err(|e| contextualize(bodies, e, current, site))?;
                            ConstructorReceiverKind::This
                        }
                    };
                    let access = context
                        .resolve_constructor(
                            init.claim,
                            method,
                            constant_index,
                            alias.reference,
                            kind,
                        )
                        .map_err(|e| {
                            contextualize(bodies, construction_failure(e, pc), current, site)
                        })?;
                    let target = MethodKey::Registered(access.method());
                    let metadata = bodies.class(target)?;
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        java.cache.entry(target)
                    {
                        let prepared = prepare_method_with_context(
                            metadata,
                            target.index(),
                            &mut java.used_code_bytes,
                            limits.max_prepared_code_bytes,
                            Preparation::Constructor,
                            Some((&mut **context, access.method(), java.monitors.is_some())),
                            exceptions.as_mut(),
                        )
                        .map_err(|e| contextualize(bodies, e, target, site))?;
                        entry.insert(prepared);
                    }
                    let prepared = java.cache.get(&target).ok_or_else(|| operand_error(pc))?;
                    if !prepared.constructor || !prepared.instance || prepared.initializer {
                        return Err(contextualize(
                            bodies,
                            protocol(pc, "constructor cache role mismatch"),
                            target,
                            site,
                        ));
                    }
                    let reserved =
                        check_reservation(java.frames.len(), java.reserved, prepared, limits)
                            .map_err(|e| contextualize(bodies, e, target, site))?;
                    validate_args(prepared, &args)
                        .map_err(|e| contextualize(bodies, e, target, site))?;
                    context
                        .check_method_arguments(access.method(), &args)
                        .map_err(|e| {
                            contextualize(
                                bodies,
                                ExecutionError::new(Some(pc), E::MethodAccess(Box::new(e))),
                                target,
                                site,
                            )
                        })?;
                    let activation = init
                        .state
                        .construction
                        .next_activation(pc)
                        .map_err(|e| contextualize(bodies, e, target, site))?;
                    let base = context
                        .heap()
                        .reference_type_universe()
                        .is_some_and(|u| u.object == access.method().owner());
                    let this = Uninitialized {
                        reference: alias.reference,
                        key: AliasKey::This(activation),
                    };
                    let mut frame =
                        make_frame(target, prepared, &args, site, Some(alias.reference))
                            .map_err(|e| contextualize(bodies, e, target, site))?;
                    frame
                        .constructor
                        .try_reserve_exact(1)
                        .map_err(|_| init_allocation("constructor activation"))?;
                    if !base {
                        frame
                            .locals
                            .store_frame(0, FrameValue::Uninitialized(this), 0)?;
                    }
                    frame.role = FrameRole::Constructor;
                    frame.constructor.push(ConstructorActivation {
                        access,
                        this,
                        caller: alias,
                        chain: if base {
                            Chain::Complete
                        } else {
                            Chain::Pending
                        },
                    });
                    java.frames
                        .try_reserve(1)
                        .map_err(|_| init_allocation("constructor frame"))?;
                    activate_monitors(
                        java.monitors.as_mut(),
                        java.frames.len(),
                        &frame,
                        prepared,
                        &mut services,
                        Some(init.claim),
                    )
                    .map_err(|e| activation_error(bodies, e, &frame))?;
                    java.frames.push(frame);
                    java.reserved = reserved;
                }
                Action::InitializationStatic {
                    access,
                    value,
                    next,
                } => {
                    let class = access.field().owner();
                    let index = java
                        .frames
                        .len()
                        .checked_sub(1)
                        .ok_or_else(|| operand_error(pc))?;
                    let frame = java.frames.last_mut().ok_or_else(|| operand_error(pc))?;
                    check_static_continuation_empty(frame, pc)
                        .map_err(|error| contextualize(bodies, error, current, origin))?;
                    frame.static_access.try_reserve_exact(1).map_err(|_| {
                        contextualize(
                            bodies,
                            init_allocation("static continuation token"),
                            current,
                            origin,
                        )
                    })?;
                    frame.static_access.push(access);
                    frame.pending = Some(PendingUse::Static { value, next });
                    let init = initialization.as_mut().ok_or_else(|| operand_error(pc))?;
                    if let Some(outcome) = request_initialization(
                        InitRequest {
                            class,
                            continuation: InitContinuation::Instruction {
                                frame: index,
                                method: current,
                                pc,
                            },
                            origin: Some((current, pc)),
                        },
                        bodies,
                        &mut java,
                        &mut services,
                        exceptions.as_mut(),
                        init,
                        limits,
                    )? {
                        return Ok(outcome);
                    }
                }
                Action::LinkageError => {
                    return vm_completion(ExecutionOutcome::LinkageError, exceptions.is_some(), pc)
                        .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::NoSuchFieldError => {
                    return vm_completion(
                        ExecutionOutcome::NoSuchFieldError,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::IllegalAccessError => {
                    return vm_completion(
                        ExecutionOutcome::IllegalAccessError,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::IncompatibleClassChangeError => {
                    return vm_completion(
                        ExecutionOutcome::IncompatibleClassChangeError,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::ArrayStoreException => {
                    return vm_completion(
                        ExecutionOutcome::ArrayStoreException,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::ClassCastException => {
                    return vm_completion(
                        ExecutionOutcome::ClassCastException,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::Throw(thrown) => {
                    let exception_state = exceptions
                        .as_mut()
                        .ok_or_else(|| contextualize(bodies, operand_error(pc), current, origin))?;
                    if let Some(outcome) = handle_throw(
                        &thrown,
                        pc,
                        bodies,
                        &mut java,
                        &mut services,
                        exception_state,
                        initialization.as_mut(),
                    )? {
                        return Ok(outcome);
                    }
                }
                Action::Continue => {}
                Action::ArithmeticException => {
                    return vm_completion(
                        ExecutionOutcome::ArithmeticException,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::NullPointerException => {
                    return vm_completion(
                        ExecutionOutcome::NullPointerException,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::ArrayIndexOutOfBoundsException => {
                    return vm_completion(
                        ExecutionOutcome::ArrayIndexOutOfBoundsException,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::NegativeArraySizeException => {
                    return vm_completion(
                        ExecutionOutcome::NegativeArraySizeException,
                        exceptions.is_some(),
                        pc,
                    )
                    .map_err(|error| contextualize(bodies, error, current, origin));
                }
                Action::Return(value) => {
                    if java
                        .frames
                        .last()
                        .is_some_and(|frame| frame.role == FrameRole::Constructor)
                    {
                        let frame = java.frames.last().ok_or_else(|| operand_error(pc))?;
                        let activation = constructor_activation(frame, pc)?;
                        if value.is_some() || activation.chain != Chain::Complete {
                            return Err(contextualize(
                                bodies,
                                protocol(pc, "constructor returned before its chain completed"),
                                current,
                                origin,
                            ));
                        }
                        let Services::Fields { context, .. } = &mut services else {
                            return Err(operand_error(pc));
                        };
                        context
                            .check_method_return(activation.access.method(), None)
                            .map_err(|e| {
                                contextualize(
                                    bodies,
                                    ExecutionError::new(Some(pc), E::MethodAccess(Box::new(e))),
                                    current,
                                    origin,
                                )
                            })?;
                        let init = initialization.as_mut().ok_or_else(|| operand_error(pc))?;
                        finish_monitors(java.monitors.as_mut(), java.frames.len() - 1, pc)
                            .map_err(|e| contextualize(bodies, e, current, origin))?;
                        finish_constructor_boundary(&mut java, init, true, pc)
                            .map_err(|e| contextualize(bodies, e, current, origin))?;
                        continue;
                    }
                    finish_monitors(java.monitors.as_mut(), java.frames.len() - 1, pc)
                        .map_err(|e| contextualize(bodies, e, current, origin))?;
                    let completed = java.frames.pop().ok_or_else(|| {
                        ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
                    })?;
                    java.reserved = java
                        .reserved
                        .checked_sub(completed.reserved_slots)
                        .ok_or_else(|| {
                            ExecutionError::new(
                                None,
                                E::InvalidMethod("invalid frame reservation accounting"),
                            )
                        })?;
                    if let FrameRole::Initializer(class) = completed.role {
                        if value.is_some() {
                            return Err(contextualize(
                                bodies,
                                ExecutionError::new(Some(pc), E::ReturnTypeMismatch),
                                current,
                                origin,
                            ));
                        }
                        let init = initialization.as_mut().ok_or_else(|| operand_error(pc))?;
                        init.finish_body(class)?;
                        continue;
                    }
                    if let Some(caller) = java.frames.last_mut() {
                        if let Some(value) = value {
                            let call_offset =
                                completed.origin.map_or(caller.pc, |(_, offset)| offset);
                            caller.stack.push(value, call_offset).map_err(|error| {
                                contextualize(bodies, error, caller.method, completed.origin)
                            })?;
                        }
                    } else {
                        return Ok(EngineOutcome::Legacy(ExecutionOutcome::Returned(value)));
                    }
                }
                Action::Call {
                    constant_index,
                    kind,
                } => {
                    let call_site = Some((current, pc));
                    let (target, mut arguments, receiver) = if kind.is_instance() {
                        let origin = current.owner().ok_or_else(|| {
                            ExecutionError::new(Some(pc), E::UnsupportedInstruction(kind.opcode()))
                        })?;
                        let Services::Fields { context, .. } = &mut services else {
                            return Err(ExecutionError::new(
                                Some(pc),
                                E::UnsupportedInstruction(kind.opcode()),
                            ));
                        };
                        let invocation = match kind {
                            CallKind::Virtual => InstanceInvocationKind::Virtual,
                            CallKind::Special => InstanceInvocationKind::Special,
                            CallKind::Interface { .. } => InstanceInvocationKind::Interface,
                            CallKind::Static => return Err(operand_error(pc)),
                        };
                        let parameters = context
                            .instance_invocation_shape(origin, constant_index, invocation)
                            .map_err(|error| {
                                contextualize(
                                    bodies,
                                    ExecutionError::new(Some(pc), E::MethodAccess(Box::new(error))),
                                    current,
                                    call_site,
                                )
                            })?;
                        if let CallKind::Interface { count } = kind {
                            let expected = usize::from(parameters.parameter_slots)
                                .checked_add(1)
                                .ok_or_else(|| {
                                contextualize(bodies, operand_error(pc), current, call_site)
                            })?;
                            if expected > usize::from(u8::MAX) || usize::from(count) != expected {
                                return Err(contextualize(
                                    bodies,
                                    operand_error(pc),
                                    current,
                                    call_site,
                                ));
                            }
                        }
                        let kinds = parameters.parameters;
                        let caller = java.frames.last_mut().ok_or_else(|| {
                            ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
                        })?;
                        let mut args = Vec::new();
                        args.try_reserve_exact(kinds.len()).map_err(|_| {
                            contextualize(
                                bodies,
                                ExecutionError::new(
                                    Some(pc),
                                    E::Heap(HeapError::AllocationFailure {
                                        resource: "call arguments",
                                    }),
                                ),
                                current,
                                call_site,
                            )
                        })?;
                        for kind in kinds.iter().rev() {
                            args.push(caller.stack.pop(*kind, pc).map_err(|error| {
                                contextualize(bodies, error, current, call_site)
                            })?);
                        }
                        args.reverse();
                        let receiver = caller
                            .stack
                            .reference(pc)
                            .map_err(|error| contextualize(bodies, error, current, call_site))?;
                        let Services::Fields { context, .. } = &mut services else {
                            return Err(ExecutionError::new(
                                Some(pc),
                                E::UnsupportedInstruction(kind.opcode()),
                            ));
                        };
                        let selection = match kind {
                            CallKind::Virtual => {
                                context.select_virtual(origin, constant_index, receiver)
                            }
                            CallKind::Special => {
                                context.select_special(origin, constant_index, receiver)
                            }
                            CallKind::Interface { .. } => {
                                context.select_interface(origin, constant_index, receiver)
                            }
                            CallKind::Static => return Err(operand_error(pc)),
                        };
                        let selected = match selection {
                            Ok(method) => method,
                            Err(error) => {
                                let outcome = method_failure(error, pc).map_err(|error| {
                                    contextualize(bodies, error, current, call_site)
                                })?;
                                return vm_completion(outcome, exceptions.is_some(), pc).map_err(
                                    |error| contextualize(bodies, error, current, call_site),
                                );
                            }
                        };
                        (MethodKey::Registered(selected), Some(args), Some(receiver))
                    } else if let Services::Fields { context, .. } = &mut services {
                        let origin = current.owner().ok_or_else(|| operand_error(pc))?;
                        let target = match context.resolve_static(origin, constant_index) {
                            Ok(method) => method,
                            Err(error) => {
                                let outcome = method_failure(error, pc).map_err(|error| {
                                    contextualize(bodies, error, current, call_site)
                                })?;
                                return vm_completion(outcome, exceptions.is_some(), pc).map_err(
                                    |error| contextualize(bodies, error, current, call_site),
                                );
                            }
                        };
                        (MethodKey::Registered(target), None, None)
                    } else {
                        let index = resolve_call(current_class, constant_index, pc, false)
                            .map_err(|error| contextualize(bodies, error, current, call_site))?;
                        let target = bodies
                            .static_key(current, index)
                            .map_err(|error| contextualize(bodies, error, current, call_site))?;
                        (target, None, None)
                    };
                    let target_class = bodies
                        .class(target)
                        .map_err(|error| contextualize(bodies, error, target, call_site))?;
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        java.cache.entry(target)
                    {
                        let prepared = prepare_method_with_context(
                            target_class,
                            target.index(),
                            &mut java.used_code_bytes,
                            limits.max_prepared_code_bytes,
                            if kind.is_instance() {
                                Preparation::ResolvedInstance
                            } else {
                                Preparation::Static
                            },
                            match (&mut services, target) {
                                (
                                    Services::Fields { context, .. },
                                    MethodKey::Registered(method),
                                ) => Some((&mut **context, method, java.monitors.is_some())),
                                _ => None,
                            },
                            exceptions.as_mut(),
                        )
                        .map_err(|error| contextualize(bodies, error, target, call_site))?;
                        entry.insert(prepared);
                    }
                    let prepared = java.cache.get(&target).ok_or_else(|| {
                        ExecutionError::new(None, E::InvalidMethod("missing prepared call target"))
                    })?;
                    if prepared.initializer || prepared.constructor {
                        return Err(contextualize(
                            bodies,
                            ExecutionError::new(
                                Some(pc),
                                E::UnsupportedMethod(
                                    "initializer bodies are not ordinary call targets",
                                ),
                            ),
                            target,
                            call_site,
                        ));
                    }
                    if prepared.instance != kind.is_instance() {
                        return Err(contextualize(
                            bodies,
                            ExecutionError::new(
                                Some(pc),
                                E::UnsupportedMethod(if kind.is_instance() {
                                    "instance body must be non-static"
                                } else {
                                    "method must be static"
                                }),
                            ),
                            target,
                            call_site,
                        ));
                    }
                    let next_reserved =
                        check_reservation(java.frames.len(), java.reserved, prepared, limits)
                            .map_err(|error| contextualize(bodies, error, target, call_site))?;
                    if arguments.is_none() {
                        let caller = java.frames.last_mut().ok_or_else(|| {
                            ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
                        })?;
                        let mut args = Vec::new();
                        args.try_reserve_exact(prepared.shape.parameters.len())
                            .map_err(|_| {
                                contextualize(
                                    bodies,
                                    ExecutionError::new(
                                        Some(pc),
                                        E::Heap(HeapError::AllocationFailure {
                                            resource: "call arguments",
                                        }),
                                    ),
                                    current,
                                    call_site,
                                )
                            })?;
                        for kind in prepared.shape.parameters.iter().rev() {
                            args.push(caller.stack.pop(*kind, pc).map_err(|error| {
                                contextualize(bodies, error, current, call_site)
                            })?);
                        }
                        args.reverse();
                        arguments = Some(args);
                    }
                    let args = arguments
                        .as_deref()
                        .ok_or_else(|| ExecutionError::new(None, E::InvalidOperands))?;
                    validate_args(prepared, args)
                        .map_err(|error| contextualize(bodies, error, target, call_site))?;
                    if let (Services::Fields { context, .. }, MethodKey::Registered(method)) =
                        (&mut services, target)
                    {
                        if !kind.is_instance() {
                            context
                                .check_method_reference_profile(method, None)
                                .map_err(|error| {
                                    contextualize(
                                        bodies,
                                        ExecutionError::new(
                                            Some(pc),
                                            E::MethodAccess(Box::new(error)),
                                        ),
                                        target,
                                        call_site,
                                    )
                                })?;
                        }
                        context
                            .check_method_arguments(method, args)
                            .map_err(|error| {
                                contextualize(
                                    bodies,
                                    ExecutionError::new(Some(pc), E::MethodAccess(Box::new(error))),
                                    target,
                                    call_site,
                                )
                            })?;
                    }
                    if !kind.is_instance() {
                        if let Some(init) = initialization.as_mut() {
                            let MethodKey::Registered(method) = target else {
                                return Err(operand_error(pc));
                            };
                            let saved = arguments.take().ok_or_else(|| operand_error(pc))?;
                            let index = java
                                .frames
                                .len()
                                .checked_sub(1)
                                .ok_or_else(|| operand_error(pc))?;
                            let frame = java.frames.last_mut().ok_or_else(|| operand_error(pc))?;
                            if frame.pending.is_some() {
                                return Err(operand_error(pc));
                            }
                            let next = frame.pc;
                            frame.pc = pc;
                            frame.pending = Some(PendingUse::Call {
                                target,
                                args: saved,
                                next,
                            });
                            if let Some(outcome) = request_initialization(
                                InitRequest {
                                    class: method.owner(),
                                    continuation: InitContinuation::Instruction {
                                        frame: index,
                                        method: current,
                                        pc,
                                    },
                                    origin: call_site,
                                },
                                bodies,
                                &mut java,
                                &mut services,
                                exceptions.as_mut(),
                                init,
                                limits,
                            )? {
                                return Ok(outcome);
                            }
                            continue;
                        }
                    }
                    let callee = make_frame(target, prepared, args, call_site, receiver)
                        .map_err(|error| contextualize(bodies, error, target, call_site))?;
                    java.frames
                        .try_reserve(1)
                        .map_err(|_| init_allocation("ordinary call frame"))?;
                    activate_monitors(
                        java.monitors.as_mut(),
                        java.frames.len(),
                        &callee,
                        prepared,
                        &mut services,
                        initialization.as_ref().map(|init| init.claim),
                    )
                    .map_err(|e| activation_error(bodies, e, &callee))?;
                    java.frames.push(callee);
                    java.reserved = next_reserved;
                }
            }
        }
    })();
    result.map_err(|mut error: ExecutionError| {
        if let Some(init) = initialization.as_ref() {
            if let Some(active) = init.activations.last() {
                error.initialization_class.get_or_insert(active.class);
            }
        }
        error
    })
}

enum AbruptResume {
    Root,
    Caller(u32),
}
fn handle_initializer_escape(
    thrown: &ThrownException,
    class: ClassId,
    pc: u32,
    bodies: Bodies<'_>,
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    init: &mut InitExecution<'_>,
) -> Result<AbruptResume, ExecutionError> {
    let frame = java.frames.last().ok_or_else(|| {
        ExecutionError::new(None, E::InvalidMethod("escaping initializer frame absent"))
    })?;
    let current = frame.method;
    let origin = frame.origin;
    let diagnose = |mut error: ExecutionError| {
        error.initialization_class.get_or_insert(class);
        error.offset = Some(pc);
        contextualize(bodies, error, current, origin)
    };
    if frame.role != FrameRole::Initializer(class) {
        return Err(diagnose(ExecutionError::new(
            None,
            E::InvalidMethod("escaping initializer role mismatch"),
        )));
    }
    init.state.charge().map_err(diagnose)?;
    let Services::Fields { context, .. } = services else {
        return Err(diagnose(operand_error(pc)));
    };
    let is_error = context
        .is_initialization_error(init.claim, thrown)
        .map_err(|error| diagnose(initialization_failure(error, Some(pc))))?;
    if !is_error {
        return Err(diagnose(ExecutionError::new(
            Some(pc),
            E::UnsupportedInitializerWrapping {
                class,
                thrown: thrown.reference(),
            },
        )));
    }
    let activation = init.activations.last().ok_or_else(|| {
        diagnose(ExecutionError::new(
            None,
            E::InvalidMethod("escaping activation absent"),
        ))
    })?;
    if activation.class != class || activation.phase != Phase::Body {
        return Err(diagnose(ExecutionError::new(
            None,
            E::InvalidMethod("escaping activation phase mismatch"),
        )));
    }
    let record = init.state.records.get(activation.record).ok_or_else(|| {
        diagnose(ExecutionError::new(
            None,
            E::InvalidMethod("escaping record absent"),
        ))
    })?;
    if !matches!(record.status,RecordStatus::InProgress{owner,..} if owner==init.owner) {
        return Err(diagnose(ExecutionError::new(
            None,
            E::InvalidMethod("escaping owner mismatch"),
        )));
    }
    let caller_pc = unwind_frame(&mut ActiveFrames {
        frames: &mut java.frames,
        reserved: &mut java.reserved,
        monitors: None,
    })
    .map_err(diagnose)?;
    let activation = init
        .activations
        .pop()
        .ok_or_else(|| diagnose(operand_error(pc)))?;
    init.state
        .records
        .get_mut(activation.record)
        .ok_or_else(|| diagnose(operand_error(pc)))?
        .status = RecordStatus::Erroneous {
        cause: thrown.reference(),
    };
    let mut continuation = activation.continuation;
    loop {
        match continuation {
            InitContinuation::Dependency => {
                let parent = init.activations.last().ok_or_else(|| {
                    diagnose(ExecutionError::new(
                        None,
                        E::InvalidMethod("failing dependency parent absent"),
                    ))
                })?;
                let parent_class = parent.class;
                let parent_origin = parent.origin;
                if parent.phase != Phase::Dependencies {
                    return Err(diagnose(ExecutionError::new(
                        None,
                        E::InvalidMethod("failing parent is not awaiting dependency"),
                    )));
                }
                init.state.charge().map_err(|error| {
                    init_diagnose(error, parent_class, parent_origin, bodies, java)
                })?;
                let parent = init
                    .activations
                    .pop()
                    .ok_or_else(|| diagnose(operand_error(pc)))?;
                let record = init
                    .state
                    .records
                    .get_mut(parent.record)
                    .ok_or_else(|| diagnose(operand_error(pc)))?;
                if !matches!(record.status,RecordStatus::InProgress{owner,..} if owner==init.owner)
                {
                    return Err(diagnose(ExecutionError::new(
                        None,
                        E::InvalidMethod("failing dependency owner mismatch"),
                    )));
                }
                record.status = RecordStatus::Erroneous {
                    cause: thrown.reference(),
                };
                continuation = parent.continuation;
            }
            InitContinuation::RootInitialize | InitContinuation::RootStatic { .. } => {
                if !java.frames.is_empty() || caller_pc.is_some() {
                    return Err(diagnose(ExecutionError::new(
                        None,
                        E::InvalidMethod("root initializer retained a Java caller"),
                    )));
                }
                return Ok(AbruptResume::Root);
            }
            InitContinuation::Instruction {
                frame,
                method,
                pc: invoke,
            } => {
                if frame.checked_add(1) != Some(java.frames.len()) || caller_pc != Some(invoke) {
                    return Err(diagnose(ExecutionError::new(
                        None,
                        E::InvalidMethod("abrupt active-use caller mismatch"),
                    )));
                }
                let caller = java
                    .frames
                    .get_mut(frame)
                    .ok_or_else(|| diagnose(operand_error(pc)))?;
                if caller.method != method || caller.pc != invoke || caller.pending.take().is_none()
                {
                    return Err(diagnose(ExecutionError::new(
                        None,
                        E::InvalidMethod("abrupt active-use continuation absent"),
                    )));
                }
                caller.static_access.clear();
                caller.new_access.clear();
                return Ok(AbruptResume::Caller(invoke));
            }
        }
    }
}
fn handle_throw(
    thrown: &ThrownException,
    mut pc: u32,
    bodies: Bodies<'_>,
    java: &mut JavaState,
    services: &mut Services<'_, '_>,
    exceptions: &mut ExceptionState<'_>,
    mut initialization: Option<&mut InitExecution<'_>>,
) -> Result<Option<EngineOutcome>, ExecutionError> {
    loop {
        match dispatch_throw(
            bodies,
            &java.cache,
            ActiveFrames {
                frames: &mut java.frames,
                reserved: &mut java.reserved,
                monitors: java.monitors.as_mut(),
            },
            services,
            exceptions,
            thrown,
            pc,
        )? {
            DispatchResult::Caught => return Ok(None),
            DispatchResult::Uncaught => {
                return Ok(Some(EngineOutcome::Uncaught(thrown.reference())));
            }
            DispatchResult::ConstructorBoundary { pc: boundary } => {
                let frame = java.frames.last().ok_or_else(|| operand_error(boundary))?;
                let method = frame.method;
                let origin = frame.origin;
                let init = initialization
                    .as_deref_mut()
                    .ok_or_else(|| protocol(boundary, "constructor escaped outside session"))?;
                pc = finish_constructor_boundary(java, init, false, boundary)
                    .map_err(|e| contextualize(bodies, e, method, origin))?;
            }
            DispatchResult::InitializerBoundary {
                class,
                pc: search_pc,
            } => {
                let init = initialization.as_deref_mut().ok_or_else(|| {
                    ExecutionError::new(
                        Some(search_pc),
                        E::InvalidMethod("initializer escaped outside session"),
                    )
                })?;
                match handle_initializer_escape(
                    thrown, class, search_pc, bodies, java, services, init,
                )? {
                    AbruptResume::Root => {
                        return Ok(Some(EngineOutcome::Uncaught(thrown.reference())));
                    }
                    AbruptResume::Caller(invoke) => pc = invoke,
                }
            }
        }
    }
}

fn prepared_handler_count(
    current: usize,
    additional: usize,
    limit: usize,
) -> Result<usize, ExecutionError> {
    let required = current.checked_add(additional).ok_or_else(|| {
        ExecutionError::new(
            None,
            E::ExceptionAccountingOverflow {
                resource: "prepared handlers",
            },
        )
    })?;
    if required > limit {
        return Err(ExecutionError::new(
            None,
            E::ExceptionHandlerLimit { limit, required },
        ));
    }
    Ok(required)
}
fn validate_handlers(
    class: &ClassFile,
    code: &crabspot_classfile::Code,
    decoded: &DecodedCode,
) -> Result<(), ExecutionError> {
    let invalid =
        |index, reason| ExecutionError::new(None, E::InvalidExceptionHandler { index, reason });
    if !code.exception_table.is_empty() && code.max_stack == 0 {
        return Err(invalid(0, "handler requires at least one operand slot"));
    }
    for (index, row) in code.exception_table.iter().enumerate() {
        let start = u32::from(row.start_pc);
        let end = u32::from(row.end_pc);
        let handler = u32::from(row.handler_pc);
        if start >= end || end > decoded.code_len() {
            return Err(invalid(index, "invalid protected range"));
        }
        if decoded.instruction_at(start).is_none() {
            return Err(invalid(index, "start is not an instruction boundary"));
        }
        if end != decoded.code_len() && decoded.instruction_at(end).is_none() {
            return Err(invalid(index, "end is not an instruction boundary"));
        }
        if decoded.instruction_at(handler).is_none() {
            return Err(invalid(index, "handler is not an instruction boundary"));
        }
        if row.catch_type != 0 {
            let name = class
                .constant_pool
                .class_name(row.catch_type)
                .map_err(|_| invalid(index, "catch is not a Class/Utf8 constant"))?;
            if name.units().first() == Some(&91) {
                return Err(invalid(index, "array catch type is unsupported"));
            }
        }
    }
    Ok(())
}
fn original_handlers<'a>(
    bodies: Bodies<'a>,
    method: MethodKey,
    prepared: &PreparedMethod,
) -> Result<&'a [ExceptionHandler], ExecutionError> {
    let invalid = || {
        ExecutionError::new(
            None,
            E::InvalidMethod("missing original prepared handler table"),
        )
    };
    let index = prepared.handlers.ok_or_else(invalid)?;
    let attribute = bodies
        .class(method)?
        .methods
        .get(method.index())
        .and_then(|method| method.attributes.get(index))
        .ok_or_else(invalid)?;
    let AttributeBody::Code(code) = &attribute.body else {
        return Err(invalid());
    };
    Ok(&code.exception_table)
}
fn charge_dispatch(used: &mut u64, limit: u64) -> Result<(), ExecutionError> {
    let required = used.checked_add(1).ok_or_else(|| {
        ExecutionError::new(
            None,
            E::ExceptionAccountingOverflow {
                resource: "dispatch steps",
            },
        )
    })?;
    if required > limit {
        return Err(ExecutionError::new(
            None,
            E::ExceptionDispatchLimit { limit, required },
        ));
    }
    *used = required;
    Ok(())
}
struct ActiveFrames<'a> {
    frames: &'a mut Vec<Frame>,
    reserved: &'a mut usize,
    monitors: Option<&'a mut MonitorLedger>,
}
fn unwind_frame(active: &mut ActiveFrames<'_>) -> Result<Option<u32>, ExecutionError> {
    let frame = active.frames.last().ok_or_else(|| {
        ExecutionError::new(None, E::InvalidMethod("empty exception frame stack"))
    })?;
    let released = active
        .reserved
        .checked_sub(frame.reserved_slots)
        .ok_or_else(|| {
            ExecutionError::new(
                None,
                E::ExceptionAccountingOverflow {
                    resource: "unwind frame slots",
                },
            )
        })?;
    let origin = frame.origin;
    let caller = active
        .frames
        .len()
        .checked_sub(2)
        .and_then(|index| active.frames.get(index));
    match (caller, origin) {
        (Some(caller), Some((key, _))) if caller.method == key => {}
        (None, None) => {}
        _ => {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("inconsistent exception invocation origin"),
            ));
        }
    }
    active.frames.pop();
    *active.reserved = released;
    Ok(origin.map(|(_, pc)| pc))
}
enum DispatchResult {
    Caught,
    Uncaught,
    InitializerBoundary { class: ClassId, pc: u32 },
    ConstructorBoundary { pc: u32 },
}
fn dispatch_throw(
    bodies: Bodies<'_>,
    cache: &HashMap<MethodKey, PreparedMethod>,
    mut active: ActiveFrames<'_>,
    services: &mut Services<'_, '_>,
    state: &mut ExceptionState<'_>,
    thrown: &ThrownException,
    mut search_pc: u32,
) -> Result<DispatchResult, ExecutionError> {
    loop {
        let frame = active.frames.last().ok_or_else(|| {
            ExecutionError::new(None, E::InvalidMethod("empty exception frame stack"))
        })?;
        let current = frame.method;
        let origin = frame.origin;
        let diagnose = |mut error: ExecutionError| {
            error.offset = Some(search_pc);
            contextualize(bodies, error, current, origin)
        };
        charge_dispatch(&mut state.dispatch_steps, state.limits.max_dispatch_steps)
            .map_err(diagnose)?;
        let prepared = cache.get(&current).ok_or_else(|| {
            diagnose(ExecutionError::new(
                None,
                E::InvalidMethod("missing exception method preparation"),
            ))
        })?;
        let rows = original_handlers(bodies, current, prepared).map_err(diagnose)?;
        for row in rows {
            charge_dispatch(&mut state.dispatch_steps, state.limits.max_dispatch_steps)
                .map_err(diagnose)?;
            if search_pc < u32::from(row.start_pc) || search_pc >= u32::from(row.end_pc) {
                continue;
            }
            let Services::Fields { context, .. } = services else {
                return Err(diagnose(operand_error(search_pc)));
            };
            let owner = current
                .owner()
                .ok_or_else(|| diagnose(operand_error(search_pc)))?;
            let matches = context
                .exception_handler_matches(state.profile, owner, row.catch_type, thrown)
                .map_err(|error| diagnose(exception_failure(error, Some(search_pc))))?;
            if matches {
                let frame = active
                    .frames
                    .last_mut()
                    .ok_or_else(|| diagnose(operand_error(search_pc)))?;
                frame
                    .stack
                    .install_exception(thrown.reference(), search_pc)
                    .map_err(diagnose)?;
                frame.pc = u32::from(row.handler_pc);
                return Ok(DispatchResult::Caught);
            }
        }
        charge_dispatch(&mut state.dispatch_steps, state.limits.max_dispatch_steps)
            .map_err(diagnose)?;
        finish_monitors(
            active.monitors.as_deref_mut(),
            active.frames.len() - 1,
            search_pc,
        )
        .map_err(diagnose)?;
        if frame.role == FrameRole::Constructor {
            return Ok(DispatchResult::ConstructorBoundary { pc: search_pc });
        }
        if let FrameRole::Initializer(class) = frame.role {
            return Ok(DispatchResult::InitializerBoundary {
                class,
                pc: search_pc,
            });
        }
        match unwind_frame(&mut active).map_err(diagnose)? {
            Some(pc) => search_pc = pc,
            None => return Ok(DispatchResult::Uncaught),
        }
    }
}

#[cfg(test)]
fn prepare_method(
    class: &ClassFile,
    method_index: usize,
    used_code_bytes: &mut usize,
    code_limit: usize,
    mode: Preparation,
) -> Result<PreparedMethod, ExecutionError> {
    prepare_method_with_context(
        class,
        method_index,
        used_code_bytes,
        code_limit,
        mode,
        None,
        None,
    )
}
fn prepare_method_with_context(
    class: &ClassFile,
    method_index: usize,
    used_code_bytes: &mut usize,
    code_limit: usize,
    mode: Preparation,
    runtime: Option<(&mut FieldResolutionContext<'_>, MethodId, bool)>,
    mut exceptions: Option<&mut ExceptionState<'_>>,
) -> Result<PreparedMethod, ExecutionError> {
    let instance = mode.is_instance();
    let initializer = mode == Preparation::Initializer;
    let constructor = mode == Preparation::Constructor;
    let method = class.methods.get(method_index).ok_or_else(|| {
        ExecutionError::new(
            None,
            E::MethodIndexOutOfBounds {
                index: method_index,
                count: class.methods.len(),
            },
        )
    })?;
    let name = class.constant_pool.utf8(method.name_index).map_err(|_| {
        ExecutionError::new(None, E::InvalidMethod("invalid method name reference"))
    })?;
    if (initializer && !name.equals_ascii("<clinit>"))
        || (constructor && !name.equals_ascii("<init>"))
        || (!initializer
            && !constructor
            && (name.equals_ascii("<init>") || name.equals_ascii("<clinit>")))
    {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("special initialization methods are not ordinary method bodies"),
        ));
    }
    if mode == Preparation::DirectInstanceRoot && class.access_flags & 0x0200 != 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("interface instance bodies are unsupported"),
        ));
    }
    if instance && method.access_flags & 0x0008 != 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("instance body must be non-static"),
        ));
    }
    if !instance && !initializer && method.access_flags & 0x0008 == 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("method must be static"),
        ));
    }
    if !initializer && method.access_flags & 0x0100 != 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("native methods require runtime services"),
        ));
    }
    if !initializer && method.access_flags & 0x0400 != 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("abstract methods have no executable body"),
        ));
    }
    if !initializer
        && method.access_flags & 0x0020 != 0
        && !runtime.as_ref().is_some_and(|(_, _, monitors)| *monitors)
    {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("synchronized methods require monitor services"),
        ));
    }
    let shape = if initializer {
        let descriptor = class
            .constant_pool
            .utf8(method.descriptor_index)
            .map_err(|_| ExecutionError::new(None, E::InvalidDescriptor))?;
        if !descriptor.equals_ascii("()V") {
            return Err(ExecutionError::new(None, E::UnsupportedDescriptor));
        }
        MethodValueShape {
            parameters: Vec::new(),
            parameter_slots: 0,
            result: MethodReturnKind::Void,
        }
    } else if let Some((context, method, _)) = runtime {
        context
            .method_value_shape(method)
            .map_err(|error| ExecutionError::new(None, E::MethodAccess(Box::new(error))))?
    } else {
        let descriptor = class
            .constant_pool
            .utf8(method.descriptor_index)
            .map_err(|_| ExecutionError::new(None, E::InvalidDescriptor))?;
        let descriptor = parse_method_descriptor(descriptor.units())
            .map_err(|_| ExecutionError::new(None, E::InvalidDescriptor))?;
        let parameters = descriptor
            .parameters
            .iter()
            .map(numeric_kind)
            .collect::<Result<_, _>>()?;
        let result = match descriptor.return_type {
            None => MethodReturnKind::Void,
            Some(field) => {
                numeric_kind(&field)?;
                match field {
                    FieldType::Boolean => MethodReturnKind::Boolean,
                    FieldType::Byte => MethodReturnKind::Byte,
                    FieldType::Char => MethodReturnKind::Char,
                    FieldType::Short => MethodReturnKind::Short,
                    FieldType::Int => MethodReturnKind::Int,
                    FieldType::Long => MethodReturnKind::Long,
                    FieldType::Float => MethodReturnKind::Float,
                    FieldType::Double => MethodReturnKind::Double,
                    _ => return Err(ExecutionError::new(None, E::UnsupportedDescriptor)),
                }
            }
        };
        MethodValueShape {
            parameters,
            parameter_slots: descriptor.parameter_slots,
            result,
        }
    };
    let mut codes = method
        .attributes
        .iter()
        .enumerate()
        .filter_map(|(index, attribute)| match &attribute.body {
            AttributeBody::Code(code) => Some((index, code)),
            _ => None,
        });
    let (code_index, code) = codes
        .next()
        .ok_or_else(|| ExecutionError::new(None, E::InvalidMethod("missing Code attribute")))?;
    if codes.next().is_some() {
        return Err(ExecutionError::new(
            None,
            E::InvalidMethod("multiple Code attributes"),
        ));
    }
    if exceptions.is_none() && !code.exception_table.is_empty() {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("exception handlers require exception dispatch"),
        ));
    }
    let required = used_code_bytes
        .checked_add(code.code.len())
        .ok_or_else(|| {
            ExecutionError::new(
                None,
                E::PreparedCodeLimit {
                    limit: code_limit,
                    required: usize::MAX,
                },
            )
        })?;
    if required > code_limit {
        return Err(ExecutionError::new(
            None,
            E::PreparedCodeLimit {
                limit: code_limit,
                required,
            },
        ));
    }
    let decoded = decode(&code.code, class.version.major)
        .map_err(|error| ExecutionError::new(Some(error.offset), E::Decode(error)))?;
    if let Some(state) = exceptions.as_mut() {
        let total = prepared_handler_count(
            state.prepared_handlers,
            code.exception_table.len(),
            state.limits.max_prepared_handlers,
        )?;
        validate_handlers(class, code, &decoded)?;
        state.prepared_handlers = total;
    }
    *used_code_bytes = required;
    Ok(PreparedMethod {
        instance,
        initializer,
        constructor,
        synchronized: !initializer && method.access_flags & 0x0020 != 0,
        shape,
        handlers: exceptions.is_some().then_some(code_index),
        decoded,
        max_stack: code.max_stack,
        max_locals: code.max_locals,
    })
}

fn step(
    class: &ClassFile,
    prepared: &PreparedMethod,
    frame: &mut Frame,
    services: &mut Services<'_, '_>,
    exception_profile: Option<&ExceptionProfile>,
    initialization_claim: Option<&InitializationClaim>,
    monitors: bool,
) -> Result<Action, ExecutionError> {
    let pc = frame.pc;
    let locals = &mut frame.locals;
    let stack = &mut frame.stack;
    let decoded = &prepared.decoded;
    let descriptor = prepared;
    let instruction = decoded
        .instruction_at(pc)
        .ok_or_else(|| ExecutionError::new(Some(pc), E::InstructionPointerOutsideCode))?;
    let opcode = instruction.opcode;
    if matches!(opcode, 0xc2 | 0xc3) && !monitors {
        return Err(ExecutionError::new(
            Some(pc),
            E::UnsupportedInstruction(opcode),
        ));
    }
    if is_heap_instruction(opcode) && services.heap().is_none() {
        return Err(ExecutionError::new(
            Some(pc),
            E::UnsupportedInstruction(opcode),
        ));
    }
    if matches!(
        opcode,
        0x32 | 0x53 | 0xb0 | 0xb4..=0xb7 | 0xb9 | 0xbd | 0xc0 | 0xc1 | 0xc5
    ) && !matches!(services, Services::Fields { .. })
    {
        return Err(ExecutionError::new(
            Some(pc),
            E::UnsupportedInstruction(opcode),
        ));
    }
    if opcode == 0xbf && exception_profile.is_none() {
        return Err(ExecutionError::new(
            Some(pc),
            E::UnsupportedInstruction(opcode),
        ));
    }
    if matches!(opcode, 0xb2 | 0xb3)
        && !matches!(
            services,
            Services::Fields {
                statics: Some(_),
                ..
            }
        )
    {
        return Err(ExecutionError::new(
            Some(pc),
            E::UnsupportedInstruction(opcode),
        ));
    }
    let operands = &instruction.operands;
    let mut next = pc
        .checked_add(instruction.length)
        .ok_or_else(|| ExecutionError::new(Some(pc), E::InstructionPointerOutsideCode))?;
    match opcode {
        0xc2 | 0xc3 => {
            let value = stack.pop_reference(pc)?;
            let reference = validate_frame_reference(services.heap(), value, pc)?;
            if reference.is_null() {
                return Err(ExecutionError::new(
                    Some(pc),
                    E::UnsupportedVmException(VmExceptionKind::NullPointerException),
                ));
            }
            return Ok(Action::Monitor {
                reference,
                enter: opcode == 0xc2,
                next,
            });
        }
        0xbb => {
            let Some(claim) = initialization_claim else {
                return Err(ExecutionError::new(
                    Some(pc),
                    E::UnsupportedInstruction(opcode),
                ));
            };
            let Operands::Constant(index) = operands else {
                return Err(operand_error(pc));
            };
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let origin = frame.method.owner().ok_or_else(|| operand_error(pc))?;
            let access = context
                .resolve_new(claim, origin, *index)
                .map_err(|error| construction_failure(error, pc))?;
            return Ok(Action::New { access, next });
        }
        0xb2 | 0xb3 => {
            let Operands::Constant(index) = operands else {
                return Err(operand_error(pc));
            };
            let value = if opcode == 0xb3 {
                Some(stack.pop_any(pc)?)
            } else {
                None
            };
            let Services::Fields {
                context,
                statics: Some(profile),
                ..
            } = services
            else {
                return Err(operand_error(pc));
            };
            let origin = frame.method.owner().ok_or_else(|| operand_error(pc))?;
            if let Some(claim) = initialization_claim {
                let MethodKey::Registered(method) = frame.method else {
                    return Err(operand_error(pc));
                };
                let access = context
                    .resolve_initialization_static(
                        claim,
                        method,
                        *index,
                        if opcode == 0xb2 {
                            InitializationStaticKind::Read
                        } else {
                            InitializationStaticKind::Write
                        },
                    )
                    .map_err(|error| initialization_failure(error, Some(pc)))?;
                return Ok(Action::InitializationStatic {
                    access,
                    value,
                    next,
                });
            }
            if let Some(value) = value {
                if let Err(error) = context.write_static(profile, origin, *index, value) {
                    return static_failure(error, pc);
                }
            } else {
                let value = match context.read_static(profile, origin, *index) {
                    Ok(value) => value,
                    Err(error) => return static_failure(error, pc),
                };
                stack.push(value, pc)?;
            }
        }
        0xbf => {
            let reference = stack.reference(pc)?;
            if reference.is_null() {
                return Err(ExecutionError::new(
                    Some(pc),
                    E::UnsupportedVmException(VmExceptionKind::NullPointerException),
                ));
            }
            let profile = exception_profile.ok_or_else(|| operand_error(pc))?;
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let thrown = context
                .prepare_throw(profile, reference)
                .map_err(|error| exception_failure(error, Some(pc)))?;
            return Ok(Action::Throw(thrown));
        }
        0xc5 => {
            let Operands::MultiANewArray { index, dimensions } = operands else {
                return Err(operand_error(pc));
            };
            let mut counts = [0_i32; 255];
            let requested = counts
                .get_mut(..usize::from(*dimensions))
                .ok_or_else(|| operand_error(pc))?;
            for count in requested.iter_mut().rev() {
                *count = stack.int(pc)?;
            }
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let origin = frame.method.owner().ok_or_else(|| operand_error(pc))?;
            let reference = match context.new_multidimensional_array(origin, *index, requested) {
                Ok(reference) => reference,
                Err(error) => return array_failure(error, pc),
            };
            stack.push(Value::Reference(reference), pc)?;
        }
        0xbd => {
            let Operands::Constant(index) = operands else {
                return Err(operand_error(pc));
            };
            let length = stack.int(pc)?;
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let origin = frame.method.owner().ok_or_else(|| operand_error(pc))?;
            let reference = match context.new_reference_array(origin, *index, length) {
                Ok(reference) => reference,
                Err(error) => return array_failure(error, pc),
            };
            stack.push(Value::Reference(reference), pc)?;
        }
        0x32 => {
            let index = stack.int(pc)?;
            let array = stack.reference(pc)?;
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let reference = match context.read_reference_array(array, index) {
                Ok(reference) => reference,
                Err(error) => return array_failure(error, pc),
            };
            stack.push(Value::Reference(reference), pc)?;
        }
        0x53 => {
            let value = stack.reference(pc)?;
            let index = stack.int(pc)?;
            let array = stack.reference(pc)?;
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            if let Err(error) = context.write_reference_array(array, index, value) {
                return array_failure(error, pc);
            }
        }
        0xc0 | 0xc1 => {
            let Operands::Constant(index) = operands else {
                return Err(operand_error(pc));
            };
            let reference = stack.reference(pc)?;
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let origin = frame.method.owner().ok_or_else(|| operand_error(pc))?;
            if opcode == 0xc0 {
                let reference = match context.check_cast(origin, *index, reference) {
                    Ok(reference) => reference,
                    Err(error) => return type_failure(error, pc),
                };
                stack.push(Value::Reference(reference), pc)?;
            } else {
                let matches = match context.instance_of(origin, *index, reference) {
                    Ok(matches) => matches,
                    Err(error) => return type_failure(error, pc),
                };
                stack.push(Value::Int(i32::from(matches)), pc)?;
            }
        }
        0xb4 | 0xb5 => {
            let Operands::Constant(index) = operands else {
                return Err(operand_error(pc));
            };
            let value = if opcode == 0xb5 {
                Some(stack.pop_any(pc)?)
            } else {
                None
            };
            if opcode == 0xb5 && frame.role == FrameRole::Constructor {
                let receiver = stack.pop_reference(pc)?;
                let activation = constructor_activation(frame, pc)?;
                let (reference, kind) = match receiver {
                    FrameValue::Ordinary(Value::Reference(reference)) => {
                        (reference, ConstructorFieldReceiverKind::Ordinary)
                    }
                    FrameValue::Uninitialized(alias)
                        if alias == activation.this && activation.chain == Chain::Pending =>
                    {
                        (
                            alias.reference,
                            ConstructorFieldReceiverKind::UninitializedThis,
                        )
                    }
                    _ => return Err(protocol(pc, "field receiver is not current pending this")),
                };
                let Some(claim) = initialization_claim else {
                    return Err(operand_error(pc));
                };
                let Services::Fields { context, .. } = services else {
                    return Err(operand_error(pc));
                };
                context
                    .write_constructor_field(
                        claim,
                        &activation.access,
                        *index,
                        reference,
                        value.ok_or_else(|| operand_error(pc))?,
                        kind,
                    )
                    .map_err(|e| construction_failure(e, pc))?;
                frame.pc = next;
                return Ok(Action::Continue);
            }
            let receiver = stack.reference(pc)?;
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let origin = frame.method.owner().ok_or_else(|| operand_error(pc))?;
            if let Some(value) = value {
                if let Err(error) = context.write_field(origin, *index, receiver, value) {
                    return field_failure(error, pc);
                }
            } else {
                let value = match context.read_field(origin, *index, receiver) {
                    Ok(value) => value,
                    Err(error) => return field_failure(error, pc),
                };
                stack.push(value, pc)?;
            }
        }
        0x01 => stack.push(Value::Reference(Reference::null()), pc)?,
        0x00 => {}
        0x02..=0x08 => stack.push(Value::Int(i32::from(opcode) - 3), pc)?,
        0x09..=0x0a => stack.push(Value::Long(i64::from(opcode) - 9), pc)?,
        0x0b..=0x0d => stack.push(Value::from_f32(f32::from(opcode - 0x0b)), pc)?,
        0x0e..=0x0f => stack.push(Value::from_f64(f64::from(opcode - 0x0e)), pc)?,
        0x10 => {
            let Operands::Byte(value) = operands else {
                return Err(operand_error(pc));
            };
            stack.push(Value::Int(i32::from(*value)), pc)?;
        }
        0x11 => {
            let Operands::Short(value) = operands else {
                return Err(operand_error(pc));
            };
            stack.push(Value::Int(i32::from(*value)), pc)?;
        }
        0x12..=0x14 => {
            let Operands::Constant(index) = operands else {
                return Err(operand_error(pc));
            };
            stack.push(constant(class, *index, opcode == 0x14, pc)?, pc)?;
        }
        0x15..=0x18 => {
            let index = local(operands, pc)?;
            let expected = match opcode {
                0x15 => ValueKind::Int,
                0x16 => ValueKind::Long,
                0x17 => ValueKind::Float,
                _ => ValueKind::Double,
            };
            stack.push(locals.load(index, expected, pc)?, pc)?;
        }
        0x19 => {
            let value = locals.load_reference(local(operands, pc)?, pc)?;
            validate_frame_reference(services.heap(), value, pc)?;
            stack.push_frame(value, pc)?;
        }
        0x2a..=0x2d => {
            let value = locals.load_reference(usize::from(opcode - 0x2a), pc)?;
            validate_frame_reference(services.heap(), value, pc)?;
            stack.push_frame(value, pc)?;
        }
        0x2e..=0x31 | 0x33..=0x35 => {
            let index = stack.int(pc)?;
            let reference = stack.reference(pc)?;
            let heap = services.heap_mut().ok_or_else(|| operand_error(pc))?;
            let actual = match heap.kind(reference) {
                Ok(kind) => kind,
                Err(error) => return heap_failure(error, pc),
            };
            check_array_kind(opcode, actual, pc)?;
            let value = match heap.load(reference, index) {
                Ok(value) => value,
                Err(error) => return heap_failure(error, pc),
            };
            stack.push(value, pc)?;
        }
        0x1a..=0x1d => stack.push(
            locals.load(usize::from(opcode - 0x1a), ValueKind::Int, pc)?,
            pc,
        )?,
        0x1e..=0x21 => stack.push(
            locals.load(usize::from(opcode - 0x1e), ValueKind::Long, pc)?,
            pc,
        )?,
        0x22..=0x25 => stack.push(
            locals.load(usize::from(opcode - 0x22), ValueKind::Float, pc)?,
            pc,
        )?,
        0x26..=0x29 => stack.push(
            locals.load(usize::from(opcode - 0x26), ValueKind::Double, pc)?,
            pc,
        )?,
        0x36..=0x39 => {
            let index = local(operands, pc)?;
            let expected = match opcode {
                0x36 => ValueKind::Int,
                0x37 => ValueKind::Long,
                0x38 => ValueKind::Float,
                _ => ValueKind::Double,
            };
            locals.store(index, stack.pop(expected, pc)?, pc)?;
        }
        0x3a => {
            let value = stack.pop_reference(pc)?;
            validate_frame_reference(services.heap(), value, pc)?;
            locals.store_frame(local(operands, pc)?, value, pc)?;
        }
        0x4b..=0x4e => {
            let value = stack.pop_reference(pc)?;
            validate_frame_reference(services.heap(), value, pc)?;
            locals.store_frame(usize::from(opcode - 0x4b), value, pc)?;
        }
        0x4f..=0x52 | 0x54..=0x56 => {
            let value_kind = match opcode {
                0x50 => ValueKind::Long,
                0x51 => ValueKind::Float,
                0x52 => ValueKind::Double,
                _ => ValueKind::Int,
            };
            let value = stack.pop(value_kind, pc)?;
            let index = stack.int(pc)?;
            let reference = stack.reference(pc)?;
            let heap = services.heap_mut().ok_or_else(|| operand_error(pc))?;
            let actual = match heap.kind(reference) {
                Ok(kind) => kind,
                Err(error) => return heap_failure(error, pc),
            };
            check_array_kind(opcode, actual, pc)?;
            if let Err(error) = heap.store(reference, index, value) {
                return heap_failure(error, pc);
            }
        }
        0x3b..=0x3e => locals.store(
            usize::from(opcode - 0x3b),
            stack.pop(ValueKind::Int, pc)?,
            pc,
        )?,
        0x3f..=0x42 => locals.store(
            usize::from(opcode - 0x3f),
            stack.pop(ValueKind::Long, pc)?,
            pc,
        )?,
        0x43..=0x46 => locals.store(
            usize::from(opcode - 0x43),
            stack.pop(ValueKind::Float, pc)?,
            pc,
        )?,
        0x47..=0x4a => locals.store(
            usize::from(opcode - 0x47),
            stack.pop(ValueKind::Double, pc)?,
            pc,
        )?,
        0x57..=0x5f => stack.manipulate(opcode, pc)?,
        0x60 | 0x64 | 0x68 | 0x6c | 0x70 | 0x7e | 0x80 | 0x82 => {
            let right = stack.int(pc)?;
            let left = stack.int(pc)?;
            if matches!(opcode, 0x6c | 0x70) && right == 0 {
                return Ok(Action::ArithmeticException);
            }
            let value = match opcode {
                0x60 => left.wrapping_add(right),
                0x64 => left.wrapping_sub(right),
                0x68 => left.wrapping_mul(right),
                0x6c => left.wrapping_div(right),
                0x70 => left.wrapping_rem(right),
                0x7e => left & right,
                0x80 => left | right,
                0x82 => left ^ right,
                _ => return Err(operand_error(pc)),
            };
            stack.push(Value::Int(value), pc)?;
        }
        0x61 | 0x65 | 0x69 | 0x6d | 0x71 | 0x7f | 0x81 | 0x83 => {
            let right = stack.long(pc)?;
            let left = stack.long(pc)?;
            if matches!(opcode, 0x6d | 0x71) && right == 0 {
                return Ok(Action::ArithmeticException);
            }
            let value = match opcode {
                0x61 => left.wrapping_add(right),
                0x65 => left.wrapping_sub(right),
                0x69 => left.wrapping_mul(right),
                0x6d => left.wrapping_div(right),
                0x71 => left.wrapping_rem(right),
                0x7f => left & right,
                0x81 => left | right,
                0x83 => left ^ right,
                _ => return Err(operand_error(pc)),
            };
            stack.push(Value::Long(value), pc)?;
        }
        0x62 | 0x66 | 0x6a | 0x6e | 0x72 => {
            let right = stack.float(pc)?;
            let left = stack.float(pc)?;
            let value = match opcode {
                0x62 => left + right,
                0x66 => left - right,
                0x6a => left * right,
                0x6e => left / right,
                _ => left % right,
            };
            // Round to binary32 at each bytecode; never fuse operations.
            stack.push(Value::from_f32(value), pc)?;
        }
        0x63 | 0x67 | 0x6b | 0x6f | 0x73 => {
            let right = stack.double(pc)?;
            let left = stack.double(pc)?;
            let value = match opcode {
                0x63 => left + right,
                0x67 => left - right,
                0x6b => left * right,
                0x6f => left / right,
                _ => left % right,
            };
            stack.push(Value::from_f64(value), pc)?;
        }
        0x76 => {
            let Value::Float(bits) = stack.pop(ValueKind::Float, pc)? else {
                return Err(operand_error(pc));
            };
            stack.push(Value::Float(bits ^ 0x8000_0000), pc)?;
        }
        0x77 => {
            let Value::Double(bits) = stack.pop(ValueKind::Double, pc)? else {
                return Err(operand_error(pc));
            };
            stack.push(Value::Double(bits ^ 0x8000_0000_0000_0000), pc)?;
        }
        0x74 => {
            let value = stack.int(pc)?;
            stack.push(Value::Int(value.wrapping_neg()), pc)?;
        }
        0x75 => {
            let value = stack.long(pc)?;
            stack.push(Value::Long(value.wrapping_neg()), pc)?;
        }
        0x78 | 0x7a | 0x7c => {
            let distance = stack.int(pc)? as u32 & 31;
            let value = stack.int(pc)?;
            let result = match opcode {
                0x78 => value.wrapping_shl(distance),
                0x7a => value.wrapping_shr(distance),
                _ => ((value as u32) >> distance) as i32,
            };
            stack.push(Value::Int(result), pc)?;
        }
        0x79 | 0x7b | 0x7d => {
            let distance = stack.int(pc)? as u32 & 63;
            let value = stack.long(pc)?;
            let result = match opcode {
                0x79 => value.wrapping_shl(distance),
                0x7b => value.wrapping_shr(distance),
                _ => ((value as u64) >> distance) as i64,
            };
            stack.push(Value::Long(result), pc)?;
        }
        0x84 => {
            let Operands::Iinc { index, increment } = operands else {
                return Err(operand_error(pc));
            };
            let Value::Int(value) = locals.load(usize::from(*index), ValueKind::Int, pc)? else {
                return Err(operand_error(pc));
            };
            locals.store(
                usize::from(*index),
                Value::Int(value.wrapping_add(i32::from(*increment))),
                pc,
            )?;
        }
        0x85 => {
            let value = stack.int(pc)?;
            stack.push(Value::Long(i64::from(value)), pc)?;
        }
        0x88 => {
            let value = stack.long(pc)?;
            stack.push(Value::Int(value as i32), pc)?;
        }
        0x86 => {
            let value = stack.int(pc)?;
            stack.push(Value::from_f32(value as f32), pc)?;
        }
        0x87 => {
            let value = stack.int(pc)?;
            stack.push(Value::from_f64(f64::from(value)), pc)?;
        }
        0x89 => {
            let value = stack.long(pc)?;
            stack.push(Value::from_f32(value as f32), pc)?;
        }
        0x8a => {
            let value = stack.long(pc)?;
            stack.push(Value::from_f64(value as f64), pc)?;
        }
        0x8b => {
            let value = stack.float(pc)?;
            stack.push(Value::Int(value as i32), pc)?;
        }
        0x8c => {
            let value = stack.float(pc)?;
            stack.push(Value::Long(value as i64), pc)?;
        }
        0x8d => {
            let value = stack.float(pc)?;
            stack.push(Value::from_f64(f64::from(value)), pc)?;
        }
        0x8e => {
            let value = stack.double(pc)?;
            stack.push(Value::Int(value as i32), pc)?;
        }
        0x8f => {
            let value = stack.double(pc)?;
            stack.push(Value::Long(value as i64), pc)?;
        }
        0x90 => {
            let value = stack.double(pc)?;
            stack.push(Value::from_f32(value as f32), pc)?;
        }
        0x91..=0x93 => {
            let value = stack.int(pc)?;
            let result = match opcode {
                0x91 => i32::from(value as i8),
                0x92 => i32::from(value as u16),
                _ => i32::from(value as i16),
            };
            stack.push(Value::Int(result), pc)?;
        }
        0x94 => {
            let right = stack.long(pc)?;
            let left = stack.long(pc)?;
            let value = match left.cmp(&right) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            stack.push(Value::Int(value), pc)?;
        }
        0x95 | 0x96 => {
            let right = stack.float(pc)?;
            let left = stack.float(pc)?;
            stack.push(
                Value::Int(floating_comparison(
                    left.partial_cmp(&right),
                    opcode == 0x95,
                )),
                pc,
            )?;
        }
        0x97 | 0x98 => {
            let right = stack.double(pc)?;
            let left = stack.double(pc)?;
            stack.push(
                Value::Int(floating_comparison(
                    left.partial_cmp(&right),
                    opcode == 0x97,
                )),
                pc,
            )?;
        }
        0x99..=0x9e => {
            let value = stack.int(pc)?;
            let taken = match opcode {
                0x99 => value == 0,
                0x9a => value != 0,
                0x9b => value < 0,
                0x9c => value >= 0,
                0x9d => value > 0,
                _ => value <= 0,
            };
            let Operands::Branch(displacement) = operands else {
                return Err(operand_error(pc));
            };
            if taken {
                next = branch(decoded, pc, *displacement)?;
            }
        }
        0x9f..=0xa4 => {
            let right = stack.int(pc)?;
            let left = stack.int(pc)?;
            let taken = match opcode {
                0x9f => left == right,
                0xa0 => left != right,
                0xa1 => left < right,
                0xa2 => left >= right,
                0xa3 => left > right,
                _ => left <= right,
            };
            let Operands::Branch(displacement) = operands else {
                return Err(operand_error(pc));
            };
            if taken {
                next = branch(decoded, pc, *displacement)?;
            }
        }
        0xa7 | 0xc8 => {
            let Operands::Branch(displacement) = operands else {
                return Err(operand_error(pc));
            };
            next = branch(decoded, pc, *displacement)?;
        }
        0xa5 | 0xa6 => {
            let right = stack.pop_reference(pc)?;
            let left = stack.pop_reference(pc)?;
            let right = validate_frame_reference(services.heap(), right, pc)?;
            let left = validate_frame_reference(services.heap(), left, pc)?;
            let Operands::Branch(displacement) = operands else {
                return Err(operand_error(pc));
            };
            if (left == right) == (opcode == 0xa5) {
                next = branch(decoded, pc, *displacement)?;
            }
        }
        0xc6 | 0xc7 => {
            let reference =
                validate_frame_reference(services.heap(), stack.pop_reference(pc)?, pc)?;
            let Operands::Branch(displacement) = operands else {
                return Err(operand_error(pc));
            };
            if reference.is_null() == (opcode == 0xc6) {
                next = branch(decoded, pc, *displacement)?;
            }
        }
        0xbc => {
            let Operands::NewArray(atype) = operands else {
                return Err(operand_error(pc));
            };
            let kind = match atype {
                4 => PrimitiveArrayKind::Boolean,
                5 => PrimitiveArrayKind::Char,
                6 => PrimitiveArrayKind::Float,
                7 => PrimitiveArrayKind::Double,
                8 => PrimitiveArrayKind::Byte,
                9 => PrimitiveArrayKind::Short,
                10 => PrimitiveArrayKind::Int,
                11 => PrimitiveArrayKind::Long,
                _ => return Err(operand_error(pc)),
            };
            let length = stack.int(pc)?;
            let heap = services.heap_mut().ok_or_else(|| operand_error(pc))?;
            let reference = match heap.allocate_primitive_array(kind, length) {
                Ok(reference) => reference,
                Err(error) => return heap_failure(error, pc),
            };
            stack.push(Value::Reference(reference), pc)?;
        }
        0xbe => {
            let reference = stack.reference(pc)?;
            let heap = services.heap_mut().ok_or_else(|| operand_error(pc))?;
            let length = match heap.length(reference) {
                Ok(length) => length,
                Err(error) => return heap_failure(error, pc),
            };
            stack.push(Value::Int(length), pc)?;
        }
        0xaa => {
            let key = stack.int(pc)?;
            let Operands::TableSwitch {
                default,
                low,
                offsets,
            } = operands
            else {
                return Err(operand_error(pc));
            };
            let index = i64::from(key) - i64::from(*low);
            let displacement = usize::try_from(index)
                .ok()
                .and_then(|index| offsets.get(index))
                .unwrap_or(default);
            next = branch(decoded, pc, *displacement)?;
        }
        0xab => {
            let key = stack.int(pc)?;
            let Operands::LookupSwitch { default, pairs } = operands else {
                return Err(operand_error(pc));
            };
            let displacement = pairs
                .binary_search_by_key(&key, |(key, _)| *key)
                .ok()
                .and_then(|index| pairs.get(index))
                .map_or(*default, |(_, offset)| *offset);
            next = branch(decoded, pc, displacement)?;
        }
        0xac => {
            let value = match descriptor.shape.result {
                MethodReturnKind::Boolean
                | MethodReturnKind::Byte
                | MethodReturnKind::Char
                | MethodReturnKind::Short
                | MethodReturnKind::Int => stack.int(pc)?,
                _ => return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch)),
            };
            let narrowed = match descriptor.shape.result {
                MethodReturnKind::Byte => i32::from(value as i8),
                MethodReturnKind::Char => i32::from(value as u16),
                MethodReturnKind::Short => i32::from(value as i16),
                MethodReturnKind::Boolean => value & 1,
                _ => value,
            };
            return Ok(Action::Return(Some(Value::Int(narrowed))));
        }
        0xad => {
            if descriptor.shape.result != MethodReturnKind::Long {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            }
            return Ok(Action::Return(Some(Value::Long(stack.long(pc)?))));
        }
        0xae | 0xaf => {
            let (result, kind) = if opcode == 0xae {
                (MethodReturnKind::Float, ValueKind::Float)
            } else {
                (MethodReturnKind::Double, ValueKind::Double)
            };
            if descriptor.shape.result != result {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            }
            return Ok(Action::Return(Some(stack.pop(kind, pc)?)));
        }
        0xb0 => {
            if descriptor.shape.result != MethodReturnKind::Reference {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            }
            let value = Value::Reference(stack.reference(pc)?);
            let Services::Fields { context, .. } = services else {
                return Err(operand_error(pc));
            };
            let MethodKey::Registered(method) = frame.method else {
                return Err(operand_error(pc));
            };
            context
                .check_method_return(method, Some(value))
                .map_err(|error| ExecutionError::new(Some(pc), E::MethodAccess(Box::new(error))))?;
            return Ok(Action::Return(Some(value)));
        }
        0xb9 => {
            let Operands::InvokeInterface { index, count } = operands else {
                return Err(operand_error(pc));
            };
            frame.pc = next;
            return Ok(Action::Call {
                constant_index: *index,
                kind: CallKind::Interface { count: *count },
            });
        }
        0xb6..=0xb8 => {
            let Operands::Constant(index) = operands else {
                return Err(operand_error(pc));
            };
            frame.pc = next;
            if opcode == 0xb7
                && initialization_claim.is_some()
                && constructor_reference(class, *index)
            {
                return Ok(Action::ConstructorCall {
                    constant_index: *index,
                });
            }
            return Ok(Action::Call {
                constant_index: *index,
                kind: match opcode {
                    0xb6 => CallKind::Virtual,
                    0xb7 => CallKind::Special,
                    _ => CallKind::Static,
                },
            });
        }
        0xb1 => {
            if descriptor.shape.result != MethodReturnKind::Void {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            }
            return Ok(Action::Return(None));
        }
        _ => {
            return Err(ExecutionError::new(
                Some(pc),
                E::UnsupportedInstruction(opcode),
            ));
        }
    }
    frame.pc = next;
    Ok(Action::Continue)
}

fn is_heap_instruction(opcode: u8) -> bool {
    matches!(opcode,0x01|0x19|0x2a..=0x31|0x33..=0x35|0x3a|0x4b..=0x52|0x54..=0x56|0xa5|0xa6|0xbc|0xbe|0xc6|0xc7)
}
fn static_failure(error: StaticAccessError, pc: u32) -> Result<Action, ExecutionError> {
    match error {
        StaticAccessError::Java(StaticJavaError::LinkageError) => Ok(Action::LinkageError),
        StaticAccessError::Java(StaticJavaError::NoSuchField) => Ok(Action::NoSuchFieldError),
        StaticAccessError::Java(StaticJavaError::IllegalAccess) => Ok(Action::IllegalAccessError),
        StaticAccessError::Java(StaticJavaError::IncompatibleClassChange) => {
            Ok(Action::IncompatibleClassChangeError)
        }
        error => Err(ExecutionError::new(
            Some(pc),
            E::StaticAccess(Box::new(error)),
        )),
    }
}

fn array_failure(error: ArrayAccessError, pc: u32) -> Result<Action, ExecutionError> {
    match error {
        ArrayAccessError::Java(ArrayJavaError::NullPointer) => Ok(Action::NullPointerException),
        ArrayAccessError::Java(ArrayJavaError::IndexOutOfBounds) => {
            Ok(Action::ArrayIndexOutOfBoundsException)
        }
        ArrayAccessError::Java(ArrayJavaError::NegativeArraySize) => {
            Ok(Action::NegativeArraySizeException)
        }
        ArrayAccessError::Java(ArrayJavaError::ArrayStore) => Ok(Action::ArrayStoreException),
        ArrayAccessError::Java(ArrayJavaError::IllegalAccess) => Ok(Action::IllegalAccessError),
        error => Err(ExecutionError::new(
            Some(pc),
            E::ArrayAccess(Box::new(error)),
        )),
    }
}

fn type_failure(error: TypeAccessError, pc: u32) -> Result<Action, ExecutionError> {
    match error {
        TypeAccessError::Java(TypeJavaError::ClassCast) => Ok(Action::ClassCastException),
        TypeAccessError::Java(TypeJavaError::IllegalAccess) => Ok(Action::IllegalAccessError),
        error => Err(ExecutionError::new(
            Some(pc),
            E::TypeAccess(Box::new(error)),
        )),
    }
}

fn method_failure(error: MethodAccessError, pc: u32) -> Result<ExecutionOutcome, ExecutionError> {
    match error {
        MethodAccessError::Java(MethodJavaError::LinkageError) => {
            Ok(ExecutionOutcome::LinkageError)
        }
        MethodAccessError::Java(MethodJavaError::NoSuchMethod) => {
            Ok(ExecutionOutcome::NoSuchMethodError)
        }
        MethodAccessError::Java(MethodJavaError::AbstractMethod) => {
            Ok(ExecutionOutcome::AbstractMethodError)
        }
        MethodAccessError::Java(MethodJavaError::IllegalAccess) => {
            Ok(ExecutionOutcome::IllegalAccessError)
        }
        MethodAccessError::Java(MethodJavaError::IncompatibleClassChange) => {
            Ok(ExecutionOutcome::IncompatibleClassChangeError)
        }
        MethodAccessError::Java(MethodJavaError::NullPointer) => {
            Ok(ExecutionOutcome::NullPointerException)
        }
        error => Err(ExecutionError::new(
            Some(pc),
            E::MethodAccess(Box::new(error)),
        )),
    }
}

fn field_failure(error: FieldAccessError, pc: u32) -> Result<Action, ExecutionError> {
    match error {
        FieldAccessError::Java(FieldJavaError::LinkageError) => Ok(Action::LinkageError),
        FieldAccessError::Java(FieldJavaError::NoSuchField) => Ok(Action::NoSuchFieldError),
        FieldAccessError::Java(FieldJavaError::IllegalAccess) => Ok(Action::IllegalAccessError),
        FieldAccessError::Java(FieldJavaError::IncompatibleClassChange) => {
            Ok(Action::IncompatibleClassChangeError)
        }
        FieldAccessError::Java(FieldJavaError::NullPointer) => Ok(Action::NullPointerException),
        error => Err(ExecutionError::new(
            Some(pc),
            E::FieldAccess(Box::new(error)),
        )),
    }
}

fn heap_failure(error: HeapError, pc: u32) -> Result<Action, ExecutionError> {
    match error {
        HeapError::NullReference => Ok(Action::NullPointerException),
        HeapError::IndexOutOfBounds { .. } => Ok(Action::ArrayIndexOutOfBoundsException),
        HeapError::NegativeLength { .. } => Ok(Action::NegativeArraySizeException),
        error => Err(ExecutionError::new(Some(pc), E::Heap(error))),
    }
}
fn validate_frame_reference(
    heap: Option<&ManagedHeap>,
    value: FrameValue,
    pc: u32,
) -> Result<Reference, ExecutionError> {
    let reference = value.reference(pc)?;
    if matches!(value, FrameValue::Uninitialized(_)) {
        heap.ok_or_else(|| operand_error(pc))?
            .instance_class(reference)
            .map_err(|error| {
                construction_failure(ConstructionAccessError::Instance(Box::new(error)), pc)
            })?;
    } else {
        validate_reference(heap, reference, pc)?;
    }
    Ok(reference)
}
fn validate_reference(
    heap: Option<&ManagedHeap>,
    reference: Reference,
    pc: u32,
) -> Result<(), ExecutionError> {
    if let Some(handle) = reference.heap_ref() {
        let heap = heap.ok_or_else(|| operand_error(pc))?;
        heap.validate_root(handle)
            .map_err(|error| ExecutionError::new(Some(pc), E::Heap(error)))?;
    }
    Ok(())
}
fn check_array_kind(opcode: u8, actual: PrimitiveArrayKind, pc: u32) -> Result<(), ExecutionError> {
    use PrimitiveArrayKind::*;
    let valid = matches!(
        (opcode, actual),
        (0x2e | 0x4f, Int)
            | (0x2f | 0x50, Long)
            | (0x30 | 0x51, Float)
            | (0x31 | 0x52, Double)
            | (0x33 | 0x54, Byte | Boolean)
            | (0x34 | 0x55, Char)
            | (0x35 | 0x56, Short)
    );
    if valid {
        Ok(())
    } else {
        Err(ExecutionError::new(
            Some(pc),
            E::WrongArrayKind { opcode, actual },
        ))
    }
}

fn floating_comparison(comparison: Option<std::cmp::Ordering>, nan_is_less: bool) -> i32 {
    match comparison {
        Some(std::cmp::Ordering::Less) => -1,
        Some(std::cmp::Ordering::Equal) => 0,
        Some(std::cmp::Ordering::Greater) => 1,
        None => {
            if nan_is_less {
                -1
            } else {
                1
            }
        }
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    #[test]
    fn foreign_managed_references_remain_host_errors_at_dereference_local_and_branch_boundaries() {
        let mut foreign_heap = ManagedHeap::new(crabspot_runtime::HeapOptions::default()).unwrap();
        let foreign = foreign_heap
            .allocate_primitive_array(PrimitiveArrayKind::Byte, 1)
            .unwrap();
        let mut heap = ManagedHeap::new(crabspot_runtime::HeapOptions::default()).unwrap();
        for (code, max_locals) in [
            (vec![0xbe, 0xac], 0),
            (vec![0x2a, 0x57, 0x03, 0xac], 1),
            (vec![0x4b, 0x03, 0xac], 1),
            (vec![0xc6, 0, 5, 0x03, 0xac, 0x04, 0xac], 0),
            (vec![0xa5, 0, 5, 0x03, 0xac, 0x04, 0xac], 0),
            (vec![0x2e, 0xac], 0),
        ] {
            let class = crate::tests::synthetic(&code, "()I", 2, max_locals);
            let prepared =
                prepare_method(&class, 0, &mut 0, usize::MAX, Preparation::Static).unwrap();
            let mut frame =
                make_frame(MethodKey::Standalone(0), &prepared, &[], None, None).unwrap();
            if code[0] == 0x2a {
                frame.locals.store(0, Value::Reference(foreign), 0).unwrap();
            } else {
                frame.stack.push(Value::Reference(foreign), 0).unwrap();
            }
            if code[0] == 0xa5 {
                frame
                    .stack
                    .push(Value::Reference(Reference::null()), 0)
                    .unwrap();
            }
            if code[0] == 0x2e {
                frame.stack.push(Value::Int(0), 0).unwrap();
            }
            let error = match step(
                &class,
                &prepared,
                &mut frame,
                &mut Services::Heap(&mut heap),
                None,
                None,
                false,
            ) {
                Err(error) => error,
                Ok(_) => panic!("foreign reference accepted"),
            };
            assert_eq!(error.offset, Some(0));
            assert_eq!(error.kind, E::Heap(HeapError::ForeignHeap));
            assert_eq!(heap.allocated_arrays(), 0);
        }
        assert!(validate_reference(Some(&heap), Reference::null(), 7).is_ok());
    }
    #[test]
    fn heap_failures_distinguish_java_array_outcomes_from_host_policy_and_identity_errors() {
        assert!(matches!(
            heap_failure(HeapError::NullReference, 7),
            Ok(Action::NullPointerException)
        ));
        assert!(matches!(
            heap_failure(
                HeapError::IndexOutOfBounds {
                    index: -1,
                    length: 0
                },
                7
            ),
            Ok(Action::ArrayIndexOutOfBoundsException)
        ));
        assert!(matches!(
            heap_failure(HeapError::NegativeLength { length: -1 }, 7),
            Ok(Action::NegativeArraySizeException)
        ));
        for failure in [
            HeapError::ForeignHeap,
            HeapError::MissingEntry { index: usize::MAX },
            HeapError::StaleGeneration {
                expected: 1,
                actual: 0,
            },
            HeapError::ResourceLimit {
                resource: crabspot_runtime::HeapResource::PayloadBytes,
                limit: 0,
                required: 1,
            },
            HeapError::AllocationFailure {
                resource: "test reservation",
            },
        ] {
            let error = match heap_failure(failure.clone(), 7) {
                Err(error) => error,
                Ok(_) => panic!("host failure became a Java outcome"),
            };
            assert_eq!(error.offset, Some(7));
            assert_eq!(error.kind, E::Heap(failure));
        }
    }
    #[test]
    fn type_failure_maps_only_explicit_java_variants_and_keeps_wrapped_errors_host_side() {
        assert!(matches!(
            type_failure(TypeAccessError::Java(TypeJavaError::ClassCast), 7),
            Ok(Action::ClassCastException)
        ));
        assert!(matches!(
            type_failure(TypeAccessError::Java(TypeJavaError::IllegalAccess), 7),
            Ok(Action::IllegalAccessError)
        ));
        for host in [
            TypeAccessError::Context(FieldAccessError::Java(FieldJavaError::IllegalAccess)),
            TypeAccessError::Heap(HeapError::ForeignHeap),
        ] {
            let error = match type_failure(host.clone(), 7) {
                Err(error) => error,
                Ok(_) => panic!("host type failure mapped as Java"),
            };
            assert_eq!(error.offset, Some(7));
            assert_eq!(error.kind, E::TypeAccess(Box::new(host)));
            assert!(std::error::Error::source(&error).is_some());
        }
    }
    #[test]
    fn method_failure_maps_only_top_level_java_variants_and_keeps_context_errors_host_side() {
        for (error, outcome) in [
            (
                MethodJavaError::NoSuchMethod,
                ExecutionOutcome::NoSuchMethodError,
            ),
            (
                MethodJavaError::AbstractMethod,
                ExecutionOutcome::AbstractMethodError,
            ),
            (
                MethodJavaError::IllegalAccess,
                ExecutionOutcome::IllegalAccessError,
            ),
            (
                MethodJavaError::IncompatibleClassChange,
                ExecutionOutcome::IncompatibleClassChangeError,
            ),
            (
                MethodJavaError::NullPointer,
                ExecutionOutcome::NullPointerException,
            ),
        ] {
            assert_eq!(
                method_failure(MethodAccessError::Java(error), 7).unwrap(),
                outcome
            );
        }
        let host = MethodAccessError::Context(FieldAccessError::Java(FieldJavaError::NullPointer));
        let error = method_failure(host.clone(), 7).unwrap_err();
        assert_eq!(error.offset, Some(7));
        assert_eq!(error.kind, E::MethodAccess(Box::new(host)));
        assert!(std::error::Error::source(&error).is_some());
    }
    #[test]
    fn array_failure_maps_only_explicit_java_variants_and_preserves_nested_host_errors() {
        for (kind, expected) in [
            (ArrayJavaError::NullPointer, 0),
            (ArrayJavaError::IndexOutOfBounds, 1),
            (ArrayJavaError::NegativeArraySize, 2),
            (ArrayJavaError::ArrayStore, 3),
            (ArrayJavaError::IllegalAccess, 4),
        ] {
            let action = array_failure(ArrayAccessError::Java(kind), 19).unwrap();
            assert!(matches!(
                (expected, action),
                (0, Action::NullPointerException)
                    | (1, Action::ArrayIndexOutOfBoundsException)
                    | (2, Action::NegativeArraySizeException)
                    | (3, Action::ArrayStoreException)
                    | (4, Action::IllegalAccessError)
            ));
        }
        for host in [
            ArrayAccessError::Heap(HeapError::NullReference),
            ArrayAccessError::Heap(HeapError::IndexOutOfBounds {
                index: -1,
                length: 0,
            }),
            ArrayAccessError::Heap(HeapError::NegativeLength { length: -1 }),
            ArrayAccessError::Type(Box::new(TypeAccessError::Java(
                TypeJavaError::IllegalAccess,
            ))),
            ArrayAccessError::Type(Box::new(TypeAccessError::Java(TypeJavaError::ClassCast))),
        ] {
            let error = match array_failure(host.clone(), 19) {
                Err(error) => error,
                Ok(_) => panic!("nested host failure became instruction Java"),
            };
            assert_eq!(error.offset, Some(19));
            assert_eq!(error.kind, E::ArrayAccess(Box::new(host)));
            assert!(std::error::Error::source(&error).is_some());
        }
    }
    #[test]
    fn method_reference_type_java_errors_remain_host_failures() {
        for kind in [TypeJavaError::ClassCast, TypeJavaError::IllegalAccess] {
            let host = MethodAccessError::Type(Box::new(TypeAccessError::Java(kind)));
            let error = method_failure(host.clone(), 11).unwrap_err();
            assert_eq!(error.offset, Some(11));
            assert_eq!(error.kind, E::MethodAccess(Box::new(host)));
            assert!(std::error::Error::source(&error).is_some());
        }
    }
    #[test]
    fn reference_field_nested_type_java_errors_remain_boxed_field_host_errors() {
        for kind in [TypeJavaError::ClassCast, TypeJavaError::IllegalAccess] {
            let host = FieldAccessError::ReferenceType(Box::new(TypeAccessError::Java(kind)));
            let error = match field_failure(host.clone(), 17) {
                Err(error) => error,
                Ok(_) => panic!("nested type error mapped as instruction Java"),
            };
            assert_eq!(error.offset, Some(17));
            assert_eq!(error.kind, E::FieldAccess(Box::new(host)));
            assert!(std::error::Error::source(&error).is_some());
        }
    }
    #[test]
    fn field_failure_maps_only_explicit_java_errors_and_keeps_host_null_structured() {
        for (error, expected) in [
            (FieldJavaError::NoSuchField, 0),
            (FieldJavaError::IllegalAccess, 1),
            (FieldJavaError::IncompatibleClassChange, 2),
            (FieldJavaError::NullPointer, 3),
        ] {
            let action = field_failure(FieldAccessError::Java(error), 17).unwrap();
            assert!(matches!(
                (expected, action),
                (0, Action::NoSuchFieldError)
                    | (1, Action::IllegalAccessError)
                    | (2, Action::IncompatibleClassChangeError)
                    | (3, Action::NullPointerException)
            ));
        }
        let host = FieldAccessError::Instance(crabspot_runtime::InstanceError::Heap(
            HeapError::NullReference,
        ));
        let error = match field_failure(host.clone(), 17) {
            Err(error) => error,
            Ok(_) => panic!("host error mapped as Java"),
        };
        assert_eq!(error.offset, Some(17));
        assert_eq!(error.kind, E::FieldAccess(Box::new(host)));
        assert!(std::error::Error::source(&error).is_some());
    }
    #[test]
    fn resolved_interface_preparation_does_not_remove_direct_root_or_static_guards() {
        let mut class = crate::tests::synthetic(&[0x04, 0xac], "()I", 1, 1);
        class.access_flags |= 0x0200;
        class.methods[0].access_flags &= !8;
        let resolved =
            prepare_method(&class, 0, &mut 0, usize::MAX, Preparation::ResolvedInstance).unwrap();
        assert!(resolved.instance);
        for (mode, expected) in [
            (
                Preparation::DirectInstanceRoot,
                "interface instance bodies are unsupported",
            ),
            (Preparation::Static, "method must be static"),
        ] {
            let error = match prepare_method(&class, 0, &mut 0, usize::MAX, mode) {
                Err(error) => error,
                Ok(_) => panic!("wrong preparation mode admitted"),
            };
            assert_eq!(error.kind, E::UnsupportedMethod(expected));
        }
        class.methods[0].access_flags |= 8;
        let error =
            match prepare_method(&class, 0, &mut 0, usize::MAX, Preparation::ResolvedInstance) {
                Err(error) => error,
                Ok(_) => panic!("static selected as instance"),
            };
        assert_eq!(
            error.kind,
            E::UnsupportedMethod("instance body must be non-static")
        );
    }
    #[test]
    fn instance_frame_receiver_and_argument_capacity_are_checked_privately() {
        for (descriptor, locals, args, expected) in [
            (
                "()V",
                0,
                vec![],
                E::LocalOutOfBounds {
                    index: 0,
                    width: 1,
                    capacity: 0,
                },
            ),
            (
                "(J)J",
                2,
                vec![Value::Long(1)],
                E::LocalOutOfBounds {
                    index: 1,
                    width: 2,
                    capacity: 2,
                },
            ),
        ] {
            // Registry admission rejects these capacities first. Mutated inspection
            // metadata exercises the frame's independent defensive boundary.
            let mut class = crate::tests::synthetic(&[0xb1], descriptor, 2, 3);
            class.methods[0].access_flags &= !8;
            let prepared = prepare_method(
                &class,
                0,
                &mut 0,
                usize::MAX,
                Preparation::DirectInstanceRoot,
            )
            .unwrap();
            let mut prepared = prepared;
            prepared.max_locals = locals;
            let error = match make_frame(
                MethodKey::Standalone(0),
                &prepared,
                &args,
                None,
                Some(Reference::null()),
            ) {
                Err(error) => error,
                Ok(_) => panic!("invalid instance locals admitted"),
            };
            assert_eq!(error.kind, expected);
        }
    }
    #[test]
    fn resource_accounting_overflow_is_a_structured_error() {
        let class = crate::tests::synthetic(&[0xb1], "()V", 1, 0);
        let mut used = usize::MAX;
        let error = match prepare_method(&class, 0, &mut used, usize::MAX, Preparation::Static) {
            Err(error) => error,
            Ok(_) => panic!("overflow accepted"),
        };
        assert_eq!(
            error.kind,
            E::PreparedCodeLimit {
                limit: usize::MAX,
                required: usize::MAX
            }
        );
        assert_eq!(used, usize::MAX);
        let mut used = 0;
        let prepared = prepare_method(&class, 0, &mut used, 1, Preparation::Static).unwrap();
        assert_eq!(used, 1);
        let limits = CallLimits {
            max_frames: usize::MAX,
            max_frame_slots: usize::MAX,
            max_prepared_code_bytes: usize::MAX,
        };
        assert_eq!(
            check_reservation(usize::MAX, 0, &prepared, limits)
                .unwrap_err()
                .kind,
            E::FrameLimit {
                limit: usize::MAX,
                required: usize::MAX
            }
        );
        assert_eq!(
            check_reservation(0, usize::MAX, &prepared, limits)
                .unwrap_err()
                .kind,
            E::FrameSlotLimit {
                limit: usize::MAX,
                required: usize::MAX
            }
        );
    }
    #[test]
    fn exception_count_and_dispatch_arithmetic_distinguish_overflow_and_leave_failed_counters() {
        assert_eq!(
            prepared_handler_count(1, 2, 2).unwrap_err().kind,
            E::ExceptionHandlerLimit {
                limit: 2,
                required: 3
            }
        );
        assert_eq!(
            prepared_handler_count(usize::MAX, 1, usize::MAX)
                .unwrap_err()
                .kind,
            E::ExceptionAccountingOverflow {
                resource: "prepared handlers"
            }
        );
        assert_eq!(
            prepared_handler_count(usize::MAX, 0, usize::MAX).unwrap(),
            usize::MAX
        );
        let mut used = 1;
        assert_eq!(
            charge_dispatch(&mut used, 1).unwrap_err().kind,
            E::ExceptionDispatchLimit {
                limit: 1,
                required: 2
            }
        );
        assert_eq!(used, 1);
        used = u64::MAX;
        assert_eq!(
            charge_dispatch(&mut used, u64::MAX).unwrap_err().kind,
            E::ExceptionAccountingOverflow {
                resource: "dispatch steps"
            }
        );
        assert_eq!(used, u64::MAX);
    }
    #[test]
    fn exception_unwind_checks_origin_and_slot_accounting_before_mutating_frames() {
        fn frame(method: usize, origin: Option<(MethodKey, u32)>) -> Frame {
            Frame {
                method: MethodKey::Standalone(method),
                role: FrameRole::Ordinary,
                pending: None,
                static_access: Vec::new(),
                new_access: Vec::new(),
                constructor: Vec::new(),
                locals: Locals::new(0),
                stack: Stack::new(0),
                pc: 90,
                origin,
                reserved_slots: 2,
            }
        }
        for origin in [None, Some((MethodKey::Standalone(8), 17))] {
            let mut frames = vec![frame(0, None), frame(1, origin)];
            let mut reserved = 4;
            assert!(matches!(
                unwind_frame(&mut ActiveFrames {
                    frames: &mut frames,
                    reserved: &mut reserved,
                    monitors: None,
                })
                .unwrap_err()
                .kind,
                E::InvalidMethod(_)
            ));
            assert_eq!(frames.len(), 2);
            assert_eq!(reserved, 4);
        }
        let mut frames = vec![frame(0, None)];
        let mut reserved = 1;
        assert_eq!(
            unwind_frame(&mut ActiveFrames {
                frames: &mut frames,
                reserved: &mut reserved,
                monitors: None,
            })
            .unwrap_err()
            .kind,
            E::ExceptionAccountingOverflow {
                resource: "unwind frame slots"
            }
        );
        assert_eq!(frames.len(), 1);
        assert_eq!(reserved, 1);
        let mut frames = vec![
            frame(0, None),
            frame(1, Some((MethodKey::Standalone(0), 17))),
        ];
        let mut reserved = 4;
        assert_eq!(
            unwind_frame(&mut ActiveFrames {
                frames: &mut frames,
                reserved: &mut reserved,
                monitors: None,
            })
            .unwrap(),
            Some(17)
        );
        assert_eq!(reserved, 2);
        assert_eq!(
            unwind_frame(&mut ActiveFrames {
                frames: &mut frames,
                reserved: &mut reserved,
                monitors: None,
            })
            .unwrap(),
            None
        );
        assert_eq!(reserved, 0);
        assert!(frames.is_empty());
    }
    #[test]
    fn exception_mapping_keeps_nested_java_host_errors_boxed_and_preserves_legacy_outcomes() {
        for host in [
            ExceptionAccessError::Type(Box::new(TypeAccessError::Java(
                TypeJavaError::IllegalAccess,
            ))),
            ExceptionAccessError::Type(Box::new(TypeAccessError::Java(TypeJavaError::ClassCast))),
            ExceptionAccessError::Context(FieldAccessError::Java(FieldJavaError::NullPointer)),
        ] {
            let error = exception_failure(host.clone(), Some(13));
            assert_eq!(error.kind, E::ExceptionAccess(Box::new(host)));
            assert_eq!(error.offset, Some(13));
            assert!(std::error::Error::source(&error).is_some());
        }
        for outcome in [
            ExecutionOutcome::ArithmeticException,
            ExecutionOutcome::ClassCastException,
            ExecutionOutcome::ArrayStoreException,
            ExecutionOutcome::NullPointerException,
            ExecutionOutcome::ArrayIndexOutOfBoundsException,
            ExecutionOutcome::NegativeArraySizeException,
            ExecutionOutcome::NoSuchFieldError,
            ExecutionOutcome::IllegalAccessError,
            ExecutionOutcome::IncompatibleClassChangeError,
            ExecutionOutcome::NoSuchMethodError,
            ExecutionOutcome::AbstractMethodError,
        ] {
            assert!(
                matches!(vm_completion(outcome,false,13).unwrap(),EngineOutcome::Legacy(value) if value==outcome)
            );
            let error = match vm_completion(outcome, true, 13) {
                Err(error) => error,
                Ok(_) => panic!("symbolic outcome escaped explicit profile"),
            };
            assert_eq!(
                error.kind,
                E::UnsupportedVmException(vm_kind(outcome).unwrap())
            );
        }
    }
    #[test]
    fn static_failure_maps_only_direct_java_variants_and_preserves_nested_host_errors() {
        for (error, expected) in [
            (StaticJavaError::NoSuchField, 0),
            (StaticJavaError::IllegalAccess, 1),
            (StaticJavaError::IncompatibleClassChange, 2),
        ] {
            let action = static_failure(StaticAccessError::Java(error), 17).unwrap();
            assert!(matches!(
                (expected, action),
                (0, Action::NoSuchFieldError)
                    | (1, Action::IllegalAccessError)
                    | (2, Action::IncompatibleClassChangeError)
            ));
        }
        for host in [
            StaticAccessError::Context(FieldAccessError::Java(FieldJavaError::IllegalAccess)),
            StaticAccessError::Context(FieldAccessError::Java(FieldJavaError::NullPointer)),
            StaticAccessError::Type(Box::new(TypeAccessError::Java(TypeJavaError::ClassCast))),
            StaticAccessError::Type(Box::new(TypeAccessError::Java(
                TypeJavaError::IllegalAccess,
            ))),
            StaticAccessError::Heap(HeapError::NullReference),
        ] {
            let error = match static_failure(host.clone(), 17) {
                Err(error) => error,
                Ok(_) => panic!("nested host error mapped to symbolic Java"),
            };
            assert_eq!(error.kind, E::StaticAccess(Box::new(host)));
            assert_eq!(error.offset, Some(17));
            assert!(std::error::Error::source(&error).is_some());
        }
    }
    #[test]
    fn initialization_plan_total_overflow_and_limits_are_checked_independently() {
        assert_eq!(
            session_totals(
                usize::MAX,
                1,
                usize::MAX,
                InitializationResource::PlanDependencies
            )
            .unwrap_err()
            .kind,
            E::InitializationAccountingOverflow {
                resource: "retained plan totals"
            }
        );
        assert_eq!(
            session_totals(2, 3, 4, InitializationResource::ConstantValues)
                .unwrap_err()
                .kind,
            E::InitializationLimit {
                resource: InitializationResource::ConstantValues,
                limit: 4,
                required: 5
            }
        );
        assert_eq!(
            session_totals(
                usize::MAX,
                0,
                usize::MAX,
                InitializationResource::PlanDependencies
            )
            .unwrap(),
            usize::MAX
        );
    }
    #[test]
    fn static_pending_token_guards_reject_missing_and_duplicate_uses_without_mutation() {
        let class = crate::tests::synthetic(&[0xb1], "()V", 1, 0);
        let prepared = prepare_method(&class, 0, &mut 0, usize::MAX, Preparation::Static).unwrap();
        let mut frame = make_frame(MethodKey::Standalone(0), &prepared, &[], None, None).unwrap();
        assert_eq!(
            frame.static_access.capacity(),
            0,
            "legacy frames allocate no token buffer"
        );
        check_static_continuation_empty(&frame, 7).unwrap();
        let error = take_static_continuation(&mut frame, 7).unwrap_err();
        assert_eq!(error.offset, Some(7));
        assert!(matches!(error.kind, E::InvalidMethod(_)));
        frame.pending = Some(PendingUse::Static {
            value: Some(Value::Int(3)),
            next: 10,
        });
        assert!(matches!(
            check_static_continuation_empty(&frame, 7).unwrap_err().kind,
            E::InvalidMethod("duplicate active-use continuation")
        ));
        assert!(matches!(
            frame.pending,
            Some(PendingUse::Static {
                value: Some(Value::Int(3)),
                next: 10
            })
        ));
        assert_eq!(frame.static_access.len(), 0);
        frame.pending.take();
        check_static_continuation_empty(&frame, 7).unwrap();
    }
    #[test]
    fn initialization_failure_maps_only_explicit_java_variants() {
        for (java, kind) in [
            (
                StaticJavaError::NoSuchField,
                VmExceptionKind::NoSuchFieldError,
            ),
            (
                StaticJavaError::IllegalAccess,
                VmExceptionKind::IllegalAccessError,
            ),
            (
                StaticJavaError::IncompatibleClassChange,
                VmExceptionKind::IncompatibleClassChangeError,
            ),
        ] {
            assert_eq!(
                initialization_failure(InitializationAccessError::Java(java), Some(8)).kind,
                E::UnsupportedVmException(kind)
            );
            let nested = InitializationAccessError::Static(Box::new(StaticAccessError::Java(java)));
            let error = initialization_failure(nested.clone(), Some(8));
            assert_eq!(error.kind, E::InitializationAccess(Box::new(nested)));
            assert_eq!(error.offset, Some(8));
            assert_eq!(error.initialization_class, None);
        }
    }
}

#[cfg(test)]
#[path = "construction_tests.rs"]
mod construction_tests;

#[cfg(test)]
#[path = "member_resolution_tests.rs"]
mod member_resolution_tests;

#[cfg(test)]
mod monitor_bridge_tests {
    use super::*;
    #[test]
    fn monitor_mapping_keeps_host_source_and_direct_java_production_separate() {
        use std::error::Error;
        let e = monitor_failure(
            MonitorFailure::Host(crate::MonitorError::InvalidState("ledger damaged")),
            Some(19),
        );
        assert_eq!(e.offset, Some(19));
        assert_eq!(
            e.kind,
            E::Monitor(crate::MonitorError::InvalidState("ledger damaged"))
        );
        assert!(
            e.source()
                .unwrap()
                .downcast_ref::<crate::MonitorError>()
                .is_some()
        );
        assert!(e.to_string().contains("ledger damaged"));
        let e = monitor_failure(
            MonitorFailure::Java(VmExceptionKind::IllegalMonitorStateException),
            Some(23),
        );
        assert_eq!(
            e.kind,
            E::UnsupportedVmException(VmExceptionKind::IllegalMonitorStateException)
        );
        assert!(e.source().is_none());
    }
    #[test]
    fn monitor_operand_validation_preserves_private_and_foreign_reference_boundaries() {
        let mut foreign_heap = ManagedHeap::new(Default::default()).unwrap();
        let foreign = foreign_heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
        let mut heap = ManagedHeap::new(Default::default()).unwrap();
        for opcode in [0xc2, 0xc3] {
            let class = crate::tests::synthetic(&[opcode, 0xb1], "()V", 1, 0);
            let prepared =
                prepare_method(&class, 0, &mut 0, usize::MAX, Preparation::Static).unwrap();
            for private in [false, true] {
                let mut frame =
                    make_frame(MethodKey::Standalone(0), &prepared, &[], None, None).unwrap();
                if private {
                    frame
                        .stack
                        .push_frame(
                            FrameValue::Uninitialized(Uninitialized {
                                reference: Reference::null(),
                                key: AliasKey::This(1),
                            }),
                            0,
                        )
                        .unwrap();
                } else {
                    frame.stack.push(Value::Reference(foreign), 0).unwrap();
                }
                let error = match step(
                    &class,
                    &prepared,
                    &mut frame,
                    &mut Services::Heap(&mut heap),
                    None,
                    None,
                    true,
                ) {
                    Err(e) => e,
                    Ok(_) => panic!("invalid monitor operand accepted"),
                };
                if private {
                    assert!(matches!(error.kind, E::ConstructionProtocol { .. }));
                } else {
                    assert_eq!(error.kind, E::Heap(HeapError::ForeignHeap));
                }
                assert_eq!(error.offset, Some(0));
            }
        }
    }
    #[test]
    fn warmed_synchronized_metadata_cannot_activate_without_monitor_capability() {
        let class = crate::tests::synthetic(&[0xb1], "()V", 0, 0);
        let mut prepared =
            prepare_method(&class, 0, &mut 0, usize::MAX, Preparation::Static).unwrap();
        prepared.synchronized = true;
        let frame = make_frame(MethodKey::Standalone(0), &prepared, &[], None, None).unwrap();
        let e =
            activate_monitors(None, 0, &frame, &prepared, &mut Services::None, None).unwrap_err();
        assert_eq!(
            e.kind,
            E::UnsupportedMethod("synchronized methods require monitor services")
        );
    }
}

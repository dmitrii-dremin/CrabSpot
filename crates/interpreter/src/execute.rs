use crate::{
    CallSite, ExecutionError, ExecutionErrorKind as E, MethodIdentity, Value, ValueKind,
    frame::{Locals, Stack},
};
use crabspot_bytecode::{DecodedCode, Operands, decode};
use crabspot_classfile::{
    AttributeBody, ClassFile, Constant,
    descriptor::{FieldType, parse_method_descriptor},
};
use crabspot_runtime::{
    ArrayAccessError, ArrayJavaError, ClassId, DefinitionRegistry, FieldAccessError,
    FieldJavaError, FieldResolutionContext, HeapError, ManagedHeap, MethodAccessError, MethodId,
    MethodJavaError, PrimitiveArrayKind, Reference, TypeAccessError, TypeJavaError,
};
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
    DirectInstanceRoot,
    ResolvedInstance,
}
impl Preparation {
    fn is_instance(self) -> bool {
        self != Self::Static
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
    parameters: Vec<ValueKind>,
    return_type: Option<FieldType>,
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
struct Frame {
    method: MethodKey,
    locals: Locals,
    stack: Stack,
    pc: u32,
    origin: Option<(MethodKey, u32)>,
    reserved_slots: usize,
}
enum Action {
    Continue,
    Call { constant_index: u16, kind: CallKind },
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
}

#[derive(Clone, Copy)]
struct Bodies<'a> {
    standalone: &'a ClassFile,
    registry: Option<&'a DefinitionRegistry>,
}
impl<'a> Bodies<'a> {
    fn class(self, method: MethodKey) -> Result<&'a ClassFile, ExecutionError> {
        match method {
            MethodKey::Standalone(_) => Ok(self.standalone),
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
    if args.len() != prepared.parameters.len() {
        return Err(ExecutionError::new(
            None,
            E::ArgumentCount {
                expected: prepared.parameters.len(),
                actual: args.len(),
            },
        ));
    }
    for (index, (value, expected)) in args.iter().zip(&prepared.parameters).enumerate() {
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
        locals,
        stack: Stack::new(prepared.max_stack),
        pc: 0,
        origin,
        reserved_slots: prepared.slots(),
    })
}
fn resolve_call(class: &ClassFile, index: u16, offset: u32) -> Result<usize, ExecutionError> {
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
    parse_method_descriptor(descriptor.units())
        .map_err(|_| invalid("invalid method descriptor"))?;
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
    let registry = context.registry();
    registry.method(method).map_err(|error| {
        ExecutionError::new(
            None,
            E::FieldAccess(Box::new(FieldAccessError::Registry(error))),
        )
    })?;
    let class = registry.class(method.owner()).map_err(|error| {
        ExecutionError::new(
            None,
            E::FieldAccess(Box::new(FieldAccessError::Registry(error))),
        )
    })?;
    execute_with_services(
        class,
        method.index(),
        arguments,
        options,
        limits,
        Services::Fields {
            context,
            root: method,
            receiver,
        },
    )
}

enum Services<'a, 'registry> {
    None,
    Heap(&'a mut ManagedHeap),
    Fields {
        context: &'a mut FieldResolutionContext<'registry>,
        root: MethodId,
        receiver: Reference,
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
    mut services: Services<'_, '_>,
) -> Result<ExecutionOutcome, ExecutionError> {
    let bodies = Bodies {
        standalone: class,
        registry: services.registry(),
    };
    let root_key = match &services {
        Services::Fields { root, .. } => MethodKey::Registered(*root),
        _ => MethodKey::Standalone(method_index),
    };
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
    let receiver = match &services {
        Services::Fields { receiver, .. } => Some(*receiver),
        _ => None,
    };
    let mut used_code_bytes = 0;
    let root = prepare_method(
        class,
        method_index,
        &mut used_code_bytes,
        limits.max_prepared_code_bytes,
        if receiver.is_some() {
            Preparation::DirectInstanceRoot
        } else {
            Preparation::Static
        },
    )
    .map_err(|error| contextualize(bodies, error, root_key, None))?;
    validate_args(&root, args).map_err(|error| contextualize(bodies, error, root_key, None))?;
    let mut reserved = check_reservation(0, 0, &root, limits)
        .map_err(|error| contextualize(bodies, error, root_key, None))?;
    if let (Some(receiver), Services::Fields { context, root, .. }) = (receiver, &mut services) {
        context
            .check_receiver(root.owner(), receiver)
            .map_err(|error| {
                contextualize(
                    bodies,
                    ExecutionError::new(None, E::FieldAccess(Box::new(error))),
                    root_key,
                    None,
                )
            })?;
    }
    let root_frame = make_frame(root_key, &root, args, None, receiver)
        .map_err(|error| contextualize(bodies, error, root_key, None))?;
    let mut cache = HashMap::new();
    cache.insert(root_key, root);
    let mut frames = vec![root_frame];
    let mut remaining = options.instruction_budget;
    loop {
        let Some(frame) = frames.last() else {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("empty engine frame stack"),
            ));
        };
        let current = frame.method;
        let pc = frame.pc;
        let origin = frame.origin;
        if remaining == 0 {
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
        let prepared = cache.get(&current).ok_or_else(|| {
            contextualize(
                bodies,
                ExecutionError::new(Some(pc), E::InvalidMethod("missing prepared method")),
                current,
                origin,
            )
        })?;
        let frame = frames.last_mut().ok_or_else(|| {
            ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
        })?;
        remaining -= 1;
        let current_class = bodies
            .class(current)
            .map_err(|error| contextualize(bodies, error, current, origin))?;
        let action = step(current_class, prepared, frame, &mut services)
            .map_err(|error| contextualize(bodies, error, current, origin))?;
        match action {
            Action::NoSuchFieldError => return Ok(ExecutionOutcome::NoSuchFieldError),
            Action::IllegalAccessError => return Ok(ExecutionOutcome::IllegalAccessError),
            Action::IncompatibleClassChangeError => {
                return Ok(ExecutionOutcome::IncompatibleClassChangeError);
            }
            Action::ArrayStoreException => return Ok(ExecutionOutcome::ArrayStoreException),
            Action::ClassCastException => return Ok(ExecutionOutcome::ClassCastException),
            Action::Continue => {}
            Action::ArithmeticException => return Ok(ExecutionOutcome::ArithmeticException),
            Action::NullPointerException => return Ok(ExecutionOutcome::NullPointerException),
            Action::ArrayIndexOutOfBoundsException => {
                return Ok(ExecutionOutcome::ArrayIndexOutOfBoundsException);
            }
            Action::NegativeArraySizeException => {
                return Ok(ExecutionOutcome::NegativeArraySizeException);
            }
            Action::Return(value) => {
                let completed = frames.pop().ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
                })?;
                reserved = reserved
                    .checked_sub(completed.reserved_slots)
                    .ok_or_else(|| {
                        ExecutionError::new(
                            None,
                            E::InvalidMethod("invalid frame reservation accounting"),
                        )
                    })?;
                if let Some(caller) = frames.last_mut() {
                    if let Some(value) = value {
                        let call_offset = completed.origin.map_or(caller.pc, |(_, offset)| offset);
                        caller.stack.push(value, call_offset).map_err(|error| {
                            contextualize(bodies, error, caller.method, completed.origin)
                        })?;
                    }
                } else {
                    return Ok(ExecutionOutcome::Returned(value));
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
                    let parameters =
                        invocation_parameters(current_class, origin, constant_index, kind)
                            .map_err(|error| {
                                contextualize(
                                    bodies,
                                    ExecutionError::new(Some(pc), E::MethodAccess(Box::new(error))),
                                    current,
                                    call_site,
                                )
                            })?;
                    if let CallKind::Interface { count } = kind {
                        let expected =
                            usize::from(parameters.slots)
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
                    let kinds = parameters.kinds;
                    let caller = frames.last_mut().ok_or_else(|| {
                        ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
                    })?;
                    let mut args = Vec::with_capacity(kinds.len());
                    for kind in kinds.iter().rev() {
                        args.push(
                            caller.stack.pop(*kind, pc).map_err(|error| {
                                contextualize(bodies, error, current, call_site)
                            })?,
                        );
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
                            return method_failure(error, pc)
                                .map_err(|error| contextualize(bodies, error, current, call_site));
                        }
                    };
                    (MethodKey::Registered(selected), Some(args), Some(receiver))
                } else {
                    let index = resolve_call(current_class, constant_index, pc)
                        .map_err(|error| contextualize(bodies, error, current, call_site))?;
                    let target = bodies
                        .static_key(current, index)
                        .map_err(|error| contextualize(bodies, error, current, call_site))?;
                    (target, None, None)
                };
                let target_class = bodies
                    .class(target)
                    .map_err(|error| contextualize(bodies, error, target, call_site))?;
                if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(target) {
                    let prepared = prepare_method(
                        target_class,
                        target.index(),
                        &mut used_code_bytes,
                        limits.max_prepared_code_bytes,
                        if kind.is_instance() {
                            Preparation::ResolvedInstance
                        } else {
                            Preparation::Static
                        },
                    )
                    .map_err(|error| contextualize(bodies, error, target, call_site))?;
                    entry.insert(prepared);
                }
                let prepared = cache.get(&target).ok_or_else(|| {
                    ExecutionError::new(None, E::InvalidMethod("missing prepared call target"))
                })?;
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
                let next_reserved = check_reservation(frames.len(), reserved, prepared, limits)
                    .map_err(|error| contextualize(bodies, error, target, call_site))?;
                if arguments.is_none() {
                    let caller = frames.last_mut().ok_or_else(|| {
                        ExecutionError::new(None, E::InvalidMethod("empty engine frame stack"))
                    })?;
                    let mut args = Vec::with_capacity(prepared.parameters.len());
                    for kind in prepared.parameters.iter().rev() {
                        args.push(
                            caller.stack.pop(*kind, pc).map_err(|error| {
                                contextualize(bodies, error, current, call_site)
                            })?,
                        );
                    }
                    args.reverse();
                    arguments = Some(args);
                }
                let args = arguments
                    .as_deref()
                    .ok_or_else(|| ExecutionError::new(None, E::InvalidOperands))?;
                validate_args(prepared, args)
                    .map_err(|error| contextualize(bodies, error, target, call_site))?;
                let callee = make_frame(target, prepared, args, call_site, receiver)
                    .map_err(|error| contextualize(bodies, error, target, call_site))?;
                frames.push(callee);
                reserved = next_reserved;
            }
        }
    }
}

fn prepare_method(
    class: &ClassFile,
    method_index: usize,
    used_code_bytes: &mut usize,
    code_limit: usize,
    mode: Preparation,
) -> Result<PreparedMethod, ExecutionError> {
    let instance = mode.is_instance();
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
    if name.equals_ascii("<init>") || name.equals_ascii("<clinit>") {
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
    if !instance && method.access_flags & 0x0008 == 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("method must be static"),
        ));
    }
    if method.access_flags & 0x0100 != 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("native methods require runtime services"),
        ));
    }
    if method.access_flags & 0x0400 != 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("abstract methods have no executable body"),
        ));
    }
    if method.access_flags & 0x0020 != 0 {
        return Err(ExecutionError::new(
            None,
            E::UnsupportedMethod("synchronized methods require monitor services"),
        ));
    }
    let descriptor = class
        .constant_pool
        .utf8(method.descriptor_index)
        .map_err(|_| ExecutionError::new(None, E::InvalidDescriptor))?;
    let descriptor = parse_method_descriptor(descriptor.units())
        .map_err(|_| ExecutionError::new(None, E::InvalidDescriptor))?;
    let kinds: Vec<ValueKind> = descriptor
        .parameters
        .iter()
        .map(numeric_kind)
        .collect::<Result<_, _>>()?;
    if let Some(field) = &descriptor.return_type {
        numeric_kind(field)?;
    }
    let mut codes = method
        .attributes
        .iter()
        .filter_map(|attribute| match &attribute.body {
            AttributeBody::Code(code) => Some(code),
            _ => None,
        });
    let code = codes
        .next()
        .ok_or_else(|| ExecutionError::new(None, E::InvalidMethod("missing Code attribute")))?;
    if codes.next().is_some() {
        return Err(ExecutionError::new(
            None,
            E::InvalidMethod("multiple Code attributes"),
        ));
    }
    if !code.exception_table.is_empty() {
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
    *used_code_bytes = required;
    Ok(PreparedMethod {
        instance,
        parameters: kinds,
        return_type: descriptor.return_type,
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
    if is_heap_instruction(opcode) && services.heap().is_none() {
        return Err(ExecutionError::new(
            Some(pc),
            E::UnsupportedInstruction(opcode),
        ));
    }
    if matches!(
        opcode,
        0x32 | 0x53 | 0xb4..=0xb7 | 0xb9 | 0xbd | 0xc0 | 0xc1 | 0xc5
    ) && !matches!(services, Services::Fields { .. })
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
            let value = locals.load(local(operands, pc)?, ValueKind::Reference, pc)?;
            let Value::Reference(reference) = value else {
                return Err(operand_error(pc));
            };
            validate_reference(services.heap(), reference, pc)?;
            stack.push(value, pc)?;
        }
        0x2a..=0x2d => {
            let value = locals.load(usize::from(opcode - 0x2a), ValueKind::Reference, pc)?;
            let Value::Reference(reference) = value else {
                return Err(operand_error(pc));
            };
            validate_reference(services.heap(), reference, pc)?;
            stack.push(value, pc)?;
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
            let reference = stack.reference(pc)?;
            validate_reference(services.heap(), reference, pc)?;
            locals.store(local(operands, pc)?, Value::Reference(reference), pc)?;
        }
        0x4b..=0x4e => {
            let reference = stack.reference(pc)?;
            validate_reference(services.heap(), reference, pc)?;
            locals.store(usize::from(opcode - 0x4b), Value::Reference(reference), pc)?;
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
            let right = stack.reference(pc)?;
            let left = stack.reference(pc)?;
            validate_reference(services.heap(), right, pc)?;
            validate_reference(services.heap(), left, pc)?;
            let Operands::Branch(displacement) = operands else {
                return Err(operand_error(pc));
            };
            if (left == right) == (opcode == 0xa5) {
                next = branch(decoded, pc, *displacement)?;
            }
        }
        0xc6 | 0xc7 => {
            let reference = stack.reference(pc)?;
            validate_reference(services.heap(), reference, pc)?;
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
            let Some(field) = &descriptor.return_type else {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            };
            if numeric_kind(field)? != ValueKind::Int {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            }
            let value = stack.int(pc)?;
            let narrowed = match field {
                FieldType::Byte => i32::from(value as i8),
                FieldType::Char => i32::from(value as u16),
                FieldType::Short => i32::from(value as i16),
                FieldType::Boolean => value & 1,
                _ => value,
            };
            return Ok(Action::Return(Some(Value::Int(narrowed))));
        }
        0xad => {
            if descriptor.return_type != Some(FieldType::Long) {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            }
            return Ok(Action::Return(Some(Value::Long(stack.long(pc)?))));
        }
        0xae | 0xaf => {
            let (field, kind) = if opcode == 0xae {
                (FieldType::Float, ValueKind::Float)
            } else {
                (FieldType::Double, ValueKind::Double)
            };
            if descriptor.return_type != Some(field) {
                return Err(ExecutionError::new(Some(pc), E::ReturnTypeMismatch));
            }
            return Ok(Action::Return(Some(stack.pop(kind, pc)?)));
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
            if descriptor.return_type.is_some() {
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
struct CallParameters {
    kinds: Vec<ValueKind>,
    slots: u16,
}
fn invocation_parameters(
    class: &ClassFile,
    origin: ClassId,
    index: u16,
    kind: CallKind,
) -> Result<CallParameters, MethodAccessError> {
    let invalid = || MethodAccessError::InvalidConstant { origin, index };
    let (class_index, name_and_type_index) =
        match (kind, class.constant_pool.get(index).map_err(|_| invalid())?) {
            (
                CallKind::Virtual | CallKind::Special,
                Constant::Methodref {
                    class_index,
                    name_and_type_index,
                },
            )
            | (
                CallKind::Interface { .. },
                Constant::InterfaceMethodref {
                    class_index,
                    name_and_type_index,
                },
            ) => (*class_index, *name_and_type_index),
            (
                CallKind::Special,
                Constant::InterfaceMethodref {
                    class_index,
                    name_and_type_index,
                },
            ) if class.version.major >= 52 => (*class_index, *name_and_type_index),
            _ => return Err(invalid()),
        };
    class
        .constant_pool
        .class_name(class_index)
        .map_err(|_| invalid())?;
    let Constant::NameAndType {
        name_index,
        descriptor_index,
    } = class
        .constant_pool
        .get(name_and_type_index)
        .map_err(|_| invalid())?
    else {
        return Err(invalid());
    };
    let name = class
        .constant_pool
        .utf8(*name_index)
        .map_err(|_| invalid())?;
    if name.equals_ascii("<init>") || name.equals_ascii("<clinit>") {
        return Err(MethodAccessError::UnsupportedSpecialMethod);
    }
    let descriptor = class
        .constant_pool
        .utf8(*descriptor_index)
        .map_err(|_| invalid())?;
    let descriptor = parse_method_descriptor(descriptor.units()).map_err(|_| invalid())?;
    if descriptor
        .return_type
        .as_ref()
        .is_some_and(|field| numeric_kind(field).is_err())
    {
        return Err(MethodAccessError::UnsupportedDescriptor);
    }
    let kinds = descriptor
        .parameters
        .iter()
        .map(|field| numeric_kind(field).map_err(|_| MethodAccessError::UnsupportedDescriptor))
        .collect::<Result<_, _>>()?;
    Ok(CallParameters {
        kinds,
        slots: descriptor.parameter_slots,
    })
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
}

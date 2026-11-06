use crate::{InitializationResource, ValueKind};
use crabspot_bytecode::DecodeError;
use crabspot_classfile::JavaString;
use crabspot_runtime::{
    ArrayAccessError, ClassId, ExceptionAccessError, FieldAccessError, HeapError,
    InitializationAccessError, MethodAccessError, PrimitiveArrayKind, Reference, StaticAccessError,
    TypeAccessError, VmExceptionKind,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionError {
    pub initialization_class: Option<ClassId>,
    pub vm_exception: Option<Box<crate::VmExceptionContext>>,
    pub offset: Option<u32>,
    pub kind: ExecutionErrorKind,
    pub method: Option<Box<MethodIdentity>>,
    pub call_site: Option<Box<CallSite>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodIdentity {
    pub owner: Option<ClassId>,
    pub index: usize,
    pub name: JavaString,
    pub descriptor: JavaString,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallSite {
    pub method: MethodIdentity,
    pub offset: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionErrorKind {
    ThrowableAccess(Box<crabspot_runtime::ThrowableAccessError>),
    VmExceptionLimit {
        limit: u64,
        required: u64,
    },
    VmExceptionAccountingOverflow {
        counter: crate::VmExceptionCounter,
    },
    VmExceptionProtocol {
        reason: &'static str,
    },
    NestedVmMaterialization(Box<crate::NestedVmMaterialization>),
    StringBody(Box<crabspot_runtime::StringBodyError>),
    AssertionQueryOverflow,
    ClassMirror(Box<crabspot_runtime::ClassMirrorError>),
    String(Box<crabspot_runtime::StringError>),
    Monitor(crate::MonitorError),
    ApplicationMain(Box<crabspot_runtime::ApplicationMainError>),
    ConstructionAccess(Box<crabspot_runtime::ConstructionAccessError>),
    ConstructionProtocol {
        reason: &'static str,
    },
    ConstructionStepLimit {
        limit: u64,
        required: u64,
    },
    ConstructionAccountingOverflow {
        resource: &'static str,
    },
    InitializationAccess(Box<InitializationAccessError>),
    InitializationAborted {
        class: ClassId,
    },
    UnsupportedInitializerWrapping {
        class: ClassId,
        thrown: Reference,
    },
    UnsupportedNoClassDefFoundError {
        class: ClassId,
    },
    InitializationLimit {
        resource: InitializationResource,
        limit: usize,
        required: usize,
    },
    InitializationStepLimit {
        limit: u64,
        required: u64,
    },
    InitializationAccountingOverflow {
        resource: &'static str,
    },
    StaticAccess(Box<StaticAccessError>),
    ExceptionAccess(Box<ExceptionAccessError>),
    UnsupportedVmException(VmExceptionKind),
    InvalidExceptionHandler {
        index: usize,
        reason: &'static str,
    },
    ExceptionHandlerLimit {
        limit: usize,
        required: usize,
    },
    ExceptionDispatchLimit {
        limit: u64,
        required: u64,
    },
    ExceptionAccountingOverflow {
        resource: &'static str,
    },
    MethodIndexOutOfBounds {
        index: usize,
        count: usize,
    },
    UnsupportedMethod(&'static str),
    InvalidMethod(&'static str),
    InvalidDescriptor,
    UnsupportedDescriptor,
    ArgumentCount {
        expected: usize,
        actual: usize,
    },
    ArgumentType {
        index: usize,
        expected: ValueKind,
        actual: ValueKind,
    },
    Decode(DecodeError),
    InvalidConstant {
        index: u16,
        expected: &'static str,
    },
    UnsupportedConstant {
        index: u16,
    },
    UnsupportedInstruction(u8),
    InvalidOperands,
    InstructionPointerOutsideCode,
    InstructionBudgetExhausted {
        limit: u64,
    },
    StackUnderflow,
    StackOverflow {
        required: usize,
        capacity: usize,
    },
    WrongValueType {
        expected: ValueKind,
        actual: ValueKind,
    },
    InvalidStackCategory {
        opcode: u8,
    },
    LocalOutOfBounds {
        index: usize,
        width: usize,
        capacity: usize,
    },
    LocalNotValue {
        index: usize,
    },
    ReturnTypeMismatch,
    FrameLimit {
        limit: usize,
        required: usize,
    },
    FrameSlotLimit {
        limit: usize,
        required: usize,
    },
    PreparedCodeLimit {
        limit: usize,
        required: usize,
    },
    InvalidCallReference {
        index: u16,
        reason: &'static str,
    },
    UnsupportedCall(&'static str),
    MissingDeclaredMethod,
    Heap(HeapError),
    FieldAccess(Box<FieldAccessError>),
    MethodAccess(Box<MethodAccessError>),
    TypeAccess(Box<TypeAccessError>),
    ArrayAccess(Box<ArrayAccessError>),
    WrongArrayKind {
        opcode: u8,
        actual: PrimitiveArrayKind,
    },
}

impl ExecutionError {
    pub(crate) fn new(offset: Option<u32>, kind: ExecutionErrorKind) -> Self {
        Self {
            initialization_class: None,
            vm_exception: None,
            offset,
            kind,
            method: None,
            call_site: None,
        }
    }
}
impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(vm) = &self.vm_exception {
            write!(
                formatter,
                "VM {:?} at {:?}@{} during {:?}: ",
                vm.origin.kind, vm.origin.method, vm.origin.pc, vm.phase
            )?;
        }
        if let Some(class) = self.initialization_class {
            write!(formatter, "initializing {class:?}: ")?;
        }
        if let Some(method) = &self.method {
            if let Some(owner) = method.owner {
                write!(formatter, "owner {owner:?} ")?;
            }
            write!(
                formatter,
                "method #{} {}{}: ",
                method.index,
                method.name.escaped(),
                method.descriptor.escaped()
            )?;
        }
        if let Some(site) = &self.call_site {
            if let Some(owner) = site.method.owner {
                write!(formatter, "caller owner {owner:?} ")?;
            }
            write!(
                formatter,
                "call site method #{} at bytecode offset {}: ",
                site.method.index, site.offset
            )?;
        }
        if let Some(offset) = self.offset {
            write!(formatter, "bytecode offset {offset}: ")?;
        }
        match &self.kind {
            ExecutionErrorKind::ThrowableAccess(error) => {
                write!(formatter, "Throwable service: {error}")
            }
            ExecutionErrorKind::VmExceptionLimit { limit, required } => write!(
                formatter,
                "VM materialization limit {limit}, requires {required}"
            ),
            ExecutionErrorKind::VmExceptionAccountingOverflow { counter } => write!(
                formatter,
                "VM materialization accounting overflow: {counter:?}"
            ),
            ExecutionErrorKind::VmExceptionProtocol { reason } => {
                write!(formatter, "VM materialization protocol: {reason}")
            }
            ExecutionErrorKind::NestedVmMaterialization(error) => write!(
                formatter,
                "nested VM materialization {:?} while producing {:?}",
                error.secondary, error.original
            ),
            ExecutionErrorKind::StringBody(error) => {
                write!(formatter, "bootstrap native operation failed: {error}")
            }
            ExecutionErrorKind::AssertionQueryOverflow => {
                formatter.write_str("bootstrap assertion query count overflow")
            }
            ExecutionErrorKind::String(error) => {
                write!(formatter, "String service failed: {error}")
            }
            ExecutionErrorKind::ClassMirror(error) => {
                write!(formatter, "class mirror operation failed: {error}")
            }
            ExecutionErrorKind::Monitor(error) => {
                write!(formatter, "monitor operation failed: {error}")
            }
            ExecutionErrorKind::ArrayAccess(error) => {
                write!(formatter, "reference array service failed: {error}")
            }
            ExecutionErrorKind::TypeAccess(error) => {
                write!(formatter, "type service failed: {error}")
            }
            ExecutionErrorKind::MethodAccess(error) => {
                write!(formatter, "method service failed: {error}")
            }
            ExecutionErrorKind::FieldAccess(error) => {
                write!(formatter, "field service failed: {error}")
            }
            ExecutionErrorKind::Heap(error) => write!(formatter, "heap service failed: {error}"),
            ExecutionErrorKind::WrongArrayKind { opcode, actual } => write!(
                formatter,
                "primitive array kind {actual:?} is invalid for instruction 0x{opcode:02x}"
            ),
            ExecutionErrorKind::ConstructionProtocol { reason } => {
                write!(formatter, "construction protocol failed: {reason}")
            }
            ExecutionErrorKind::ConstructionStepLimit { limit, required } => write!(
                formatter,
                "construction alias work {required} exceeds limit {limit}"
            ),
            ExecutionErrorKind::ConstructionAccountingOverflow { resource } => {
                write!(formatter, "construction accounting overflow: {resource}")
            }
            ExecutionErrorKind::ApplicationMain(error) => {
                write!(formatter, "application main failed: {error}")
            }
            ExecutionErrorKind::ConstructionAccess(error) => {
                write!(formatter, "construction service failed: {error}")
            }
            ExecutionErrorKind::InitializationAccess(error) => {
                write!(formatter, "initialization service failed: {error}")
            }
            ExecutionErrorKind::InitializationAborted { class } => {
                write!(formatter, "initialization was aborted for {class:?}")
            }
            ExecutionErrorKind::UnsupportedInitializerWrapping { class, thrown } => write!(
                formatter,
                "initializer wrapping is unsupported for {class:?}: {thrown:?}"
            ),
            ExecutionErrorKind::UnsupportedNoClassDefFoundError { class } => write!(
                formatter,
                "NoClassDefFoundError production is unsupported for {class:?}"
            ),
            ExecutionErrorKind::InitializationLimit {
                resource,
                limit,
                required,
            } => write!(
                formatter,
                "initialization {resource:?} limit {limit} exceeded (required {required})"
            ),
            ExecutionErrorKind::InitializationStepLimit { limit, required } => write!(
                formatter,
                "initialization step limit {limit} exceeded (required {required})"
            ),
            ExecutionErrorKind::InitializationAccountingOverflow { resource } => {
                write!(formatter, "initialization accounting overflow: {resource}")
            }
            ExecutionErrorKind::StaticAccess(error) => {
                write!(formatter, "static service failed: {error}")
            }
            ExecutionErrorKind::ExceptionAccess(error) => {
                write!(formatter, "exception service failed: {error}")
            }
            ExecutionErrorKind::UnsupportedVmException(kind) => write!(
                formatter,
                "VM exception production is unsupported: {kind:?}"
            ),
            ExecutionErrorKind::InvalidExceptionHandler { index, reason } => {
                write!(formatter, "invalid exception handler #{index}: {reason}")
            }
            ExecutionErrorKind::ExceptionHandlerLimit { limit, required } => write!(
                formatter,
                "prepared handler limit {limit} exceeded (required {required})"
            ),
            ExecutionErrorKind::ExceptionDispatchLimit { limit, required } => write!(
                formatter,
                "exception dispatch limit {limit} exceeded (required {required})"
            ),
            ExecutionErrorKind::ExceptionAccountingOverflow { resource } => {
                write!(formatter, "exception accounting overflow: {resource}")
            }
            ExecutionErrorKind::MethodIndexOutOfBounds { index, count } => {
                write!(formatter, "method index {index} is outside {count} methods")
            }
            ExecutionErrorKind::UnsupportedMethod(reason) => {
                write!(formatter, "unsupported method body: {reason}")
            }
            ExecutionErrorKind::InvalidMethod(reason) => {
                write!(formatter, "invalid method body: {reason}")
            }
            ExecutionErrorKind::InvalidDescriptor => {
                formatter.write_str("invalid method descriptor or descriptor reference")
            }
            ExecutionErrorKind::UnsupportedDescriptor => formatter
                .write_str("method descriptor contains an unsupported reference or array type"),
            ExecutionErrorKind::ArgumentCount { expected, actual } => write!(
                formatter,
                "expected {expected} arguments, received {actual}"
            ),
            ExecutionErrorKind::ArgumentType {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "argument {index} requires {expected:?}, received {actual:?}"
            ),
            ExecutionErrorKind::Decode(error) => {
                write!(formatter, "invalid instruction encoding: {error}")
            }
            ExecutionErrorKind::InvalidConstant { index, expected } => {
                write!(formatter, "constant-pool entry #{index} must be {expected}")
            }
            ExecutionErrorKind::UnsupportedConstant { index } => write!(
                formatter,
                "constant-pool entry #{index} requires unsupported reference or dynamic-constant execution"
            ),
            ExecutionErrorKind::UnsupportedInstruction(opcode) => {
                write!(formatter, "unsupported instruction 0x{opcode:02x}")
            }
            ExecutionErrorKind::InvalidOperands => {
                formatter.write_str("decoded operands do not match the instruction")
            }
            ExecutionErrorKind::InstructionPointerOutsideCode => {
                formatter.write_str("execution reached a position outside the instruction sequence")
            }
            ExecutionErrorKind::InstructionBudgetExhausted { limit } => {
                write!(formatter, "instruction budget {limit} exhausted")
            }
            ExecutionErrorKind::StackUnderflow => formatter.write_str("operand stack underflow"),
            ExecutionErrorKind::StackOverflow { required, capacity } => write!(
                formatter,
                "operand stack requires {required} slots but max_stack is {capacity}"
            ),
            ExecutionErrorKind::WrongValueType { expected, actual } => {
                write!(formatter, "expected {expected:?}, found {actual:?}")
            }
            ExecutionErrorKind::InvalidStackCategory { opcode } => write!(
                formatter,
                "invalid value categories for stack instruction 0x{opcode:02x}"
            ),
            ExecutionErrorKind::LocalOutOfBounds {
                index,
                width,
                capacity,
            } => write!(
                formatter,
                "local {index} needs {width} slots but max_locals is {capacity}"
            ),
            ExecutionErrorKind::LocalNotValue { index } => write!(
                formatter,
                "local {index} is empty or a category-two continuation"
            ),
            ExecutionErrorKind::ReturnTypeMismatch => {
                formatter.write_str("return instruction does not match the method descriptor")
            }
            ExecutionErrorKind::FrameLimit { limit, required } => write!(
                formatter,
                "active frame limit {limit} exceeded (required {required})"
            ),
            ExecutionErrorKind::FrameSlotLimit { limit, required } => write!(
                formatter,
                "aggregate frame-slot limit {limit} exceeded (required {required})"
            ),
            ExecutionErrorKind::PreparedCodeLimit { limit, required } => write!(
                formatter,
                "prepared Code-byte limit {limit} exceeded (required {required})"
            ),
            ExecutionErrorKind::InvalidCallReference { index, reason } => write!(
                formatter,
                "invalid invokestatic reference #{index}: {reason}"
            ),
            ExecutionErrorKind::UnsupportedCall(reason) => write!(
                formatter,
                "unsupported invokestatic body transition: {reason}"
            ),
            ExecutionErrorKind::MissingDeclaredMethod => formatter.write_str(
                "target is not declared by the supplied class; inherited lookup is unsupported",
            ),
        }
    }
}
impl std::error::Error for ExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ExecutionErrorKind::ThrowableAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::StringBody(error) => Some(error.as_ref()),
            ExecutionErrorKind::String(error) => Some(error.as_ref()),
            ExecutionErrorKind::ClassMirror(error) => Some(error.as_ref()),
            ExecutionErrorKind::Monitor(error) => Some(error),
            ExecutionErrorKind::ApplicationMain(error) => Some(error.as_ref()),
            ExecutionErrorKind::ConstructionAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::InitializationAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::StaticAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::ExceptionAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::Decode(error) => Some(error),
            ExecutionErrorKind::Heap(error) => Some(error),
            ExecutionErrorKind::FieldAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::MethodAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::TypeAccess(error) => Some(error.as_ref()),
            ExecutionErrorKind::ArrayAccess(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

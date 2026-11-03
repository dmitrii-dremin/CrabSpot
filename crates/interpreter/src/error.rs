use crate::ValueKind;
use crabspot_bytecode::DecodeError;
use crabspot_classfile::JavaString;
use crabspot_runtime::{FieldAccessError, HeapError, PrimitiveArrayKind};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionError {
    pub offset: Option<u32>,
    pub kind: ExecutionErrorKind,
    pub method: Option<Box<MethodIdentity>>,
    pub call_site: Option<Box<CallSite>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodIdentity {
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
    WrongArrayKind {
        opcode: u8,
        actual: PrimitiveArrayKind,
    },
}

impl ExecutionError {
    pub(crate) fn new(offset: Option<u32>, kind: ExecutionErrorKind) -> Self {
        Self {
            offset,
            kind,
            method: None,
            call_site: None,
        }
    }
}
impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(method) = &self.method {
            write!(
                formatter,
                "method #{} {}{}: ",
                method.index,
                method.name.escaped(),
                method.descriptor.escaped()
            )?;
        }
        if let Some(site) = &self.call_site {
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
            ExecutionErrorKind::FieldAccess(error) => {
                write!(formatter, "field service failed: {error}")
            }
            ExecutionErrorKind::Heap(error) => write!(formatter, "heap service failed: {error}"),
            ExecutionErrorKind::WrongArrayKind { opcode, actual } => write!(
                formatter,
                "primitive array kind {actual:?} is invalid for instruction 0x{opcode:02x}"
            ),
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
            ExecutionErrorKind::Decode(error) => Some(error),
            ExecutionErrorKind::Heap(error) => Some(error),
            ExecutionErrorKind::FieldAccess(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

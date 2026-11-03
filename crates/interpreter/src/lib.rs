//! A numeric static method-body kernel with optional primitive-array heap services.
//!
//! This is deliberately not Java invocation: it does not load, initialize, link,
//! or verify classes, allocate exception objects, or provide Java runtime invocation.
//! Unsupported declarations and executed instructions fail explicitly.
mod error;
mod execute;
mod frame;
#[cfg(test)]
mod tests;

pub use crabspot_runtime::{Value, ValueKind};
pub use error::{CallSite, ExecutionError, ExecutionErrorKind, MethodIdentity};
pub use execute::{
    CallLimits, ExecutionOptions, ExecutionOutcome, execute_static_method_body,
    execute_static_method_body_with_heap, execute_static_method_body_with_limits,
};

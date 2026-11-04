//! A numeric method-body kernel with explicit heap and instance-field services.
//!
//! Existing body entry points do not initialize classes. An explicit initialization
//! session schedules the supported single-owner procedure using the same engine.
//! Neither profile loads/fully links/verifies classes, allocates exception objects,
//! or provides Java startup.
//! Unsupported declarations and executed instructions fail explicitly.
mod error;
mod execute;
mod frame;
mod initialization;
#[cfg(test)]
mod tests;

pub use crabspot_runtime::{Value, ValueKind};
pub use error::{CallSite, ExecutionError, ExecutionErrorKind, MethodIdentity};
pub use execute::{
    CallLimits, ExceptionLimits, ExceptionOutcome, ExecutionOptions, ExecutionOutcome,
    execute_instance_method_body, execute_instance_method_body_with_exceptions,
    execute_instance_method_body_with_static_state,
    execute_instance_method_body_with_static_state_and_exceptions, execute_static_method_body,
    execute_static_method_body_with_heap, execute_static_method_body_with_limits,
};

pub use initialization::{
    InitializationLimits, InitializationOutcome, InitializationResource, InitializationSession,
    InitializationStatus,
};

//! A numeric method-body kernel with explicit heap and instance-field services.
//!
//! Existing body entry points do not initialize classes. An explicit initialization
//! session schedules the supported single-owner procedure using the same engine.
//! Legacy body profiles do not allocate exception objects. The opt-in Throwable
//! session executes real constructors and bounded VM arithmetic production.
//! Loading, full verification and Java startup belong to their separate layers.
//! Unsupported declarations and executed instructions fail explicitly.
// Reuse independently emitted public fixtures in private protocol-guard tests.
#[cfg(test)]
extern crate self as crabspot_interpreter;
mod construction;
mod error;
mod execute;
mod frame;
mod initialization;
mod monitor;
mod throwable;
pub use monitor::{MonitorError, MonitorLimits, MonitorResource};
pub use throwable::{
    NestedVmMaterialization, VmExceptionContext, VmExceptionCounter, VmExceptionLimits,
    VmExceptionUsage, VmFaultOrigin, VmMaterializationPhase,
};
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
    InitializationStatus, StringEqualityUsage,
};

pub use construction::ConstructionLimits;

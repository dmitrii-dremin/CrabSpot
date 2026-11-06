//! Separate opt-in surface keeps all existing input struct fields intact.
use crate::ApplicationThrowableProfile;
use crabspot_runtime::{OperandDiagnosticLimits, OperandExceptionDiscoveryLimits};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OperandExceptionMode {
    #[default]
    Auto,
    RequireOperandExceptions,
    Disabled,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApplicationOperandExceptionProfile {
    pub throwables: ApplicationThrowableProfile,
    pub mode: OperandExceptionMode,
    pub discovery: OperandExceptionDiscoveryLimits,
    pub diagnostics: OperandDiagnosticLimits,
}

//! Optional genuine Throwable support; existing invocation structs stay unchanged.
use crate::ApplicationStringEqualityProfile;
use crabspot_interpreter::VmExceptionLimits;
use crabspot_runtime::{ThrowableDiscoveryLimits, TraceLimits};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThrowableMode {
    #[default]
    Auto,
    RequireThrowable,
    Disabled,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApplicationThrowableProfile {
    pub equality: ApplicationStringEqualityProfile,
    pub mode: ThrowableMode,
    pub discovery: ThrowableDiscoveryLimits,
    pub traces: TraceLimits,
    pub vm_exceptions: VmExceptionLimits,
}

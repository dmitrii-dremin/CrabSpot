//! Image-scoped choice of genuine compact String Java body support.
use crabspot_runtime::{StringBodyDiscoveryLimits, StringBodyLimits};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StringBodyMode {
    #[default]
    Auto,
    RequireCompact,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApplicationStringBodyProfile {
    pub mode: StringBodyMode,
    pub discovery: StringBodyDiscoveryLimits,
    pub backing: StringBodyLimits,
}

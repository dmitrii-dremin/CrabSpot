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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StringEqualityMode {
    #[default]
    Auto,
    RequireCompactEquality,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApplicationStringEqualityProfile {
    pub bodies: ApplicationStringBodyProfile,
    pub mode: StringEqualityMode,
    pub discovery: StringBodyDiscoveryLimits,
    pub bootstrap_assertions: crabspot_runtime::BootstrapAssertionPolicy,
}

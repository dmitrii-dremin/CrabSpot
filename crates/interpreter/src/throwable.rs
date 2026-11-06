//! Opt-in, bounded VM exception production diagnostics.
use crabspot_runtime::{MethodId, VmExceptionKind};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VmExceptionLimits {
    pub max_materializations: u64,
}
impl Default for VmExceptionLimits {
    fn default() -> Self {
        Self {
            max_materializations: 4096,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VmExceptionUsage {
    pub started: u64,
    pub produced: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VmFaultOrigin {
    pub method: MethodId,
    pub pc: u32,
    pub kind: VmExceptionKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmMaterializationPhase {
    Preflight,
    Snapshot,
    Initialization,
    Message,
    Allocation,
    Constructor,
    Dispatch,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VmExceptionContext {
    pub origin: VmFaultOrigin,
    pub phase: VmMaterializationPhase,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NestedVmMaterialization {
    pub original: VmFaultOrigin,
    pub secondary: VmFaultOrigin,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmExceptionCounter {
    Evaluation,
    Started,
    Produced,
}

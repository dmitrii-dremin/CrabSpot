//! One-shot verified directory image. Parsed metadata, proofs and execution
//! authority are distinct; private image ownership connects their exact identities.
mod entry;
mod error;
mod image;
mod meter;
mod options;
mod source;
use crabspot_classfile::JavaString;
use crabspot_interpreter::ExecutionError;
use crabspot_verifier::ClassTypeVerificationUsage;
pub use error::*;
pub use options::*;
use std::path::Path;
pub struct ApplicationDirectories<'a> {
    pub platform: &'a Path,
    pub classes: &'a Path,
}
pub struct StaticIntObservation<'a> {
    pub binary_class: &'a str,
    pub field: &'a str,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryEvaluation {
    RootPreflight,
    NamedInitialization,
    MainEvaluation,
}
#[derive(Debug)]
pub enum ApplicationCompletion {
    Returned,
    SuppliedUncaught {
        phase: EntryEvaluation,
        class_name: JavaString,
    },
    ExecutionFailed {
        phase: EntryEvaluation,
        error: Box<ExecutionError>,
    },
}
#[derive(Debug)]
pub struct ApplicationRun {
    pub completion: ApplicationCompletion,
    pub static_ints: Vec<i32>,
    pub usage: ApplicationUsage,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceUsage {
    pub entries: u64,
    pub class_files: u64,
    pub class_bytes: u64,
    pub ordinary_definitions: u64,
    pub path_units: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OrchestrationUsage {
    pub work_units: u64,
    pub scratch_entries: u64,
    pub utf16_units: u64,
    pub inventory_work: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApplicationUsage {
    pub platform: SourceUsage,
    pub application: SourceUsage,
    pub module_descriptors: u64,
    pub physical_plan_rows: u64,
    pub physical_plan_edges: u64,
    pub proved_classes: u64,
    pub verification: ClassTypeVerificationUsage,
    pub preparation_witnesses: u64,
    pub admitted_executable_classes: u64,
    pub engine_evaluations: u64,
    pub argument_arrays: u64,
    pub orchestration: OrchestrationUsage,
}
#[cfg(test)]
mod tests;

/// Construct and execute one checked, sealed directory image.
pub fn run_directory_application(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
) -> std::result::Result<ApplicationRun, Box<ApplicationError>> {
    let prepared = image::prepare(directories, entry, observations, options)?;
    entry::run_prepared(prepared, observations)
}

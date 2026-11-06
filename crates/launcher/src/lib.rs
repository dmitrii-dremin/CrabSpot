//! One-shot verified directory image. Parsed metadata, proofs and execution
//! authority are distinct; private image ownership connects their exact identities.
mod entry;
mod invocation;
mod string_bodies;
mod throwable_export;
mod throwables;
pub use crabspot_interpreter::{VmExceptionLimits, VmExceptionUsage};
pub use crabspot_runtime::{
    ThrowableDiscoveryLimits, ThrowableDiscoveryUsage, TraceLimits, TraceUsage,
};
pub use invocation::*;
pub use string_bodies::*;
pub use throwables::*;
mod error;
mod image;
mod meter;
mod options;
mod source;
use crabspot_classfile::JavaString;
use crabspot_interpreter::ExecutionError;
pub use crabspot_interpreter::{MonitorLimits, StringEqualityUsage};
pub use crabspot_runtime::BootstrapAssertionPolicy;
pub use crabspot_runtime::{
    ClassMirrorLimits, StringBodyDiscoveryLimits, StringBodyDiscoveryUsage, StringBodyLimits,
    StringBodyUsage, StringLimits, StringUsage,
};
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
    Uncaught {
        phase: EntryEvaluation,
        class_name: JavaString,
        location: Option<ApplicationExceptionLocation>,
    },
    SuppliedUncaught {
        phase: EntryEvaluation,
        class_name: JavaString,
    },
    ExecutionFailed {
        phase: EntryEvaluation,
        error: Box<ExecutionError>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationExceptionLocation {
    pub class_name: JavaString,
    pub method_name: JavaString,
    pub descriptor: JavaString,
    pub offset: u32,
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
    pub argument_strings: u64,
    pub strings: StringUsage,
    pub string_body_discovery: StringBodyDiscoveryUsage,
    pub string_bodies: StringBodyUsage,
    pub string_equality_discovery: StringBodyDiscoveryUsage,
    pub string_equality: StringEqualityUsage,
    pub throwable_discovery: ThrowableDiscoveryUsage,
    pub traces: TraceUsage,
    pub vm_exceptions: VmExceptionUsage,
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
    run_directory_application_with_monitor_limits(
        directories,
        entry,
        observations,
        options,
        MonitorLimits::default(),
    )
}

/// Execute the same proved image with explicit single-thread monitor ceilings.
pub fn run_directory_application_with_monitor_limits(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
    monitors: MonitorLimits,
) -> std::result::Result<ApplicationRun, Box<ApplicationError>> {
    run_directory_application_with_mirror_limits(
        directories,
        entry,
        observations,
        options,
        monitors,
        ClassMirrorLimits::default(),
    )
}

/// Execute the same proved image with bounded canonical Class objects.
pub fn run_directory_application_with_mirror_limits(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
    monitors: MonitorLimits,
    mirrors: ClassMirrorLimits,
) -> std::result::Result<ApplicationRun, Box<ApplicationError>> {
    run_directory_application_with_arguments(
        directories,
        entry,
        observations,
        options,
        ApplicationInvocation {
            monitors,
            mirrors,
            ..Default::default()
        },
    )
}
/// Execute one verified image with exact UTF-16 arguments and bounded String services.
pub fn run_directory_application_with_arguments(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
    invocation: ApplicationInvocation<'_>,
) -> std::result::Result<ApplicationRun, Box<ApplicationError>> {
    invocation::validate(&invocation)?;
    let prepared = image::prepare(directories, entry, observations, options)?;
    entry::run_prepared(prepared, observations, invocation)
}

/// Probe and optionally require a supported genuine compact String layout and Code.
/// Existing invocation/options structs keep their earlier fields and semantics.
pub fn run_directory_application_with_string_bodies(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
    invocation: ApplicationInvocation<'_>,
    profile: ApplicationStringBodyProfile,
) -> std::result::Result<ApplicationRun, Box<ApplicationError>> {
    invocation::validate(&invocation)?;
    let prepared =
        image::prepare_with_string_bodies(directories, entry, observations, options, profile)?;
    entry::run_prepared(prepared, observations, invocation)
}

/// Extend a compatible genuine compact image with equality and bootstrap policy.
pub fn run_directory_application_with_string_equality(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
    invocation: ApplicationInvocation<'_>,
    profile: ApplicationStringEqualityProfile,
) -> std::result::Result<ApplicationRun, Box<ApplicationError>> {
    invocation::validate(&invocation)?;
    let prepared =
        image::prepare_with_string_equality(directories, entry, observations, options, profile)?;
    entry::run_prepared(prepared, observations, invocation)
}

/// Compose checked Throwable capture and VM arithmetic materialization with one image.
pub fn run_directory_application_with_throwables(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
    invocation: ApplicationInvocation<'_>,
    profile: ApplicationThrowableProfile,
) -> std::result::Result<ApplicationRun, Box<ApplicationError>> {
    invocation::validate(&invocation)?;
    let prepared =
        image::prepare_with_throwables(directories, entry, observations, options, profile)?;
    entry::run_prepared(prepared, observations, invocation)
}

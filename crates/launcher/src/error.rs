use crate::ApplicationUsage;
use crabspot_classfile::ParseError;
use crabspot_runtime::{
    ClassId, HierarchyError, LoadingConstraintError, MemberResolutionError, MethodId, RegistryError,
};
use crabspot_verifier::{
    ClassTypeVerificationError, ClassTypeVerificationResource, VerificationEnvironmentError,
};
use std::{error::Error, fmt, io};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputRole {
    Platform,
    Application,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationStage {
    Input,
    Definition,
    Imports,
    Dependencies,
    Hierarchy,
    Environment,
    Verification,
    Preparation,
    Layout,
    Entry,
    Observations,
    Setup,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationResource {
    Definitions,
    InputBytes,
    FileBytes,
    Entries,
    Depth,
    PathUnits,
    TotalPathUnits,
    Observations,
    InventoryWork,
    ScratchEntries,
    Utf16Units,
    WorkUnits,
    PlanRows,
    PlanEdges,
    Verification(ClassTypeVerificationResource),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupKind {
    StringBody,
    ClassMirror,
    String,
    Member,
    Heap,
    Instance,
    Field,
    Type,
    Array,
    Static,
    Exception,
    Initialization,
    Main,
}
#[derive(Debug)]
pub enum ApplicationErrorKind {
    StringBody(Box<crabspot_runtime::StringBodyError>),
    Arguments(crate::ArgumentError),
    InvalidInput(&'static str),
    InvalidPath(&'static str),
    OverlappingRoots,
    InputChanged,
    DuplicatePath,
    InvalidModule,
    UnsupportedVersion {
        major: u16,
        minor: u16,
    },
    ForbiddenApplicationName,
    NamespaceConflict,
    MissingRole(&'static str),
    MissingEntry,
    MissingApplicationDependency,
    UnsupportedPlatformReference,
    MissingHierarchyDependency,
    InvalidObservation,
    Invariant(&'static str),
    AllocationFailure,
    ResourceLimit {
        resource: ApplicationResource,
        limit: u64,
        required: u64,
    },
    AccountingOverflow {
        resource: ApplicationResource,
    },
    VerificationAccountingOverflow {
        resource: ClassTypeVerificationResource,
        source: Option<Box<ClassTypeVerificationError>>,
    },
    VerificationLimit {
        resource: ClassTypeVerificationResource,
        limit: u64,
        required: u64,
        source: Box<ClassTypeVerificationError>,
    },
    Io(io::Error),
    Parse(ParseError),
    Registry(RegistryError),
    Loading(Box<LoadingConstraintError>),
    Member(Box<MemberResolutionError>),
    Hierarchy(HierarchyError),
    Environment(Box<VerificationEnvironmentError>),
    Verification(Box<ClassTypeVerificationError>),
    Setup {
        kind: SetupKind,
        source: Box<dyn Error>,
    },
}
#[derive(Debug)]
pub struct ApplicationError {
    pub stage: ApplicationStage,
    pub kind: ApplicationErrorKind,
    pub usage: ApplicationUsage,
    pub role: Option<InputRole>,
    pub relative_path: Option<String>,
    pub class: Option<ClassId>,
    pub method: Option<MethodId>,
    pub cp_index: Option<u16>,
    pub observation: Option<usize>,
}
impl ApplicationError {
    pub(crate) fn new(
        stage: ApplicationStage,
        kind: ApplicationErrorKind,
        usage: ApplicationUsage,
    ) -> Box<Self> {
        Box::new(Self {
            stage,
            kind,
            usage,
            role: None,
            relative_path: None,
            class: None,
            method: None,
            cp_index: None,
            observation: None,
        })
    }
}
impl fmt::Display for ApplicationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StringBody(e) => e.fmt(f),
            Self::Arguments(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
            Self::Parse(e) => e.fmt(f),
            Self::Registry(e) => e.fmt(f),
            Self::Loading(e) => e.fmt(f),
            Self::Member(e) => e.fmt(f),
            Self::Hierarchy(e) => e.fmt(f),
            Self::Environment(e) => e.fmt(f),
            Self::Verification(e) => e.fmt(f),
            Self::Setup { kind, source } => write!(f, "{kind:?}: {source}"),
            other => write!(f, "{other:?}"),
        }
    }
}
impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} {:?} {:?} {:?}: {}",
            self.stage, self.class, self.method, self.cp_index, self.kind
        )
    }
}
impl Error for ApplicationErrorKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StringBody(e) => Some(e.as_ref()),
            Self::Arguments(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Registry(e) => Some(e),
            Self::Loading(e) => Some(e.as_ref()),
            Self::Member(e) => Some(e.as_ref()),
            Self::Hierarchy(e) => Some(e),
            Self::Environment(e) => Some(e.as_ref()),
            Self::Verification(e) | Self::VerificationLimit { source: e, .. } => Some(e.as_ref()),
            Self::VerificationAccountingOverflow { source, .. } => source
                .as_ref()
                .map(|e| e.as_ref() as &(dyn Error + 'static)),
            Self::Setup { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
impl Error for ApplicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.kind.source()
    }
}
pub(crate) type Result<T> = std::result::Result<T, Box<ApplicationError>>;

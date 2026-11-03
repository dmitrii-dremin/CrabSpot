use crate::{ClassId, SourceError};
use crabspot_classfile::{JavaString, ParseError};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandleKind {
    Loader,
    Class,
    Method,
    Field,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryResource {
    Loaders,
    Definitions,
    DefinitionBytes,
    TotalDefinitionBytes,
    SourceReadBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    IdentityExhausted,
    InvalidHandle {
        kind: HandleKind,
        reason: &'static str,
    },
    InvalidName(&'static str),
    UnsupportedDefinition(&'static str),
    DuplicateDefinition {
        existing: ClassId,
    },
    NameMismatch {
        expected: JavaString,
        actual: JavaString,
    },
    Parse(ParseError),
    Source(SourceError),
    NotFound {
        name: JavaString,
    },
    ResourceLimit {
        resource: RegistryResource,
        limit: usize,
        required: usize,
    },
}
impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentityExhausted => formatter.write_str("registry identity namespace exhausted"),
            Self::InvalidHandle { kind, reason } => {
                write!(formatter, "invalid {kind:?} handle: {reason}")
            }
            Self::InvalidName(reason) => {
                write!(formatter, "invalid ordinary binary class name: {reason}")
            }
            Self::UnsupportedDefinition(reason) => {
                write!(formatter, "unsupported definition: {reason}")
            }
            Self::DuplicateDefinition { .. } => formatter.write_str(
                "this defining loader already has a definition with the same semantic name",
            ),
            Self::NameMismatch { expected, actual } => write!(
                formatter,
                "expected class {}, found {}",
                expected.escaped(),
                actual.escaped()
            ),
            Self::Parse(error) => write!(formatter, "class-file rejection: {error}"),
            Self::Source(error) => write!(formatter, "definition source failed: {error}"),
            Self::NotFound { name } => write!(
                formatter,
                "definition source has no class {}",
                name.escaped()
            ),
            Self::ResourceLimit {
                resource,
                limit,
                required,
            } => write!(
                formatter,
                "{resource:?} limit {limit} exceeded (required {required})"
            ),
        }
    }
}
impl std::error::Error for RegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

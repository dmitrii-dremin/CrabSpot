//! Immutable loader facts and modern stack-map-guided method proofs.
mod checker;
mod class;
mod environment;
mod frames;
mod symbols;
#[cfg(test)]
mod tests;
use crate::{
    CodeValidationError, CodeValidationLocation, CodeValidationOptions, CodeValidationUsage,
    MethodCodeValidation,
};
pub use class::*;
use crabspot_runtime::{ClassId, LoaderId, RegistryError};
pub use environment::*;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};

/// Borrowed spelling with a prepaid fingerprint: growing a scalar-key index
/// must not rescan every previously retained UTF-16 spelling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Spelling<'a> {
    fingerprint: u64,
    units: &'a [u16],
}
impl<'a> Spelling<'a> {
    fn new(units: &'a [u16], builder: &impl BuildHasher) -> Self {
        Self {
            fingerprint: builder.hash_one(units),
            units,
        }
    }
}
impl Hash for Spelling<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fingerprint.hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeVerificationOptions {
    pub code: CodeValidationOptions,
    pub max_type_symbols: u64,
    pub max_state_cells: u64,
    pub max_edge_checks: u64,
    pub max_utf16_units: u64,
    pub max_work_units: u64,
}
impl Default for TypeVerificationOptions {
    fn default() -> Self {
        Self {
            code: CodeValidationOptions::default(),
            max_type_symbols: 65_536,
            max_state_cells: 4_194_304,
            max_edge_checks: 4_194_304,
            max_utf16_units: 4_194_304,
            max_work_units: 67_108_864,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TypeVerificationUsage {
    pub code: CodeValidationUsage,
    pub type_symbols: u64,
    pub state_cells: u64,
    pub edge_checks: u64,
    pub utf16_units: u64,
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum VerificationSymbolSource {
    Utf8 {
        class: ClassId,
        index: u16,
        start: u16,
        end: u16,
    },
    Definition(ClassId),
    Core(VerificationCoreRole),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeVerificationResource {
    TypeSymbols,
    StateCells,
    EdgeChecks,
    Utf16Units,
    WorkUnits,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeVerificationErrorKind {
    Registry(RegistryError),
    Code(Box<CodeValidationError>),
    UnretainedClass {
        class: ClassId,
    },
    UnsupportedHistory {
        major: u16,
    },
    UnavailableFact {
        initiating_loader: LoaderId,
        symbol: VerificationSymbolSource,
    },
    TypeRule {
        reason: &'static str,
    },
    FinalOverride {
        ancestor: ClassId,
        ancestor_method_index: usize,
    },
    ResourceLimit {
        resource: TypeVerificationResource,
        limit: u64,
        required: u64,
    },
    AccountingOverflow {
        resource: TypeVerificationResource,
    },
    AllocationFailure {
        resource: &'static str,
    },
}
impl From<RegistryError> for TypeVerificationErrorKind {
    fn from(e: RegistryError) -> Self {
        Self::Registry(e)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeVerificationError {
    pub class: ClassId,
    pub method_index: usize,
    pub location: CodeValidationLocation,
    pub usage: TypeVerificationUsage,
    pub kind: TypeVerificationErrorKind,
}
impl fmt::Display for TypeVerificationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            Self::Code(e) => e.fmt(f),
            Self::TypeRule { reason } => f.write_str(reason),
            e => write!(f, "{e:?}"),
        }
    }
}
impl std::error::Error for TypeVerificationErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e),
            Self::Code(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl fmt::Display for TypeVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "class {:?} method {} {:?}: {}",
            self.class, self.method_index, self.location, self.kind
        )
    }
}
impl std::error::Error for TypeVerificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.kind)
    }
}
#[derive(Debug)]
pub struct MethodTypeVerification<'e, 'a> {
    environment: &'e VerificationEnvironment<'a>,
    class: ClassId,
    method_index: usize,
    code: MethodCodeValidation<'a>,
    usage: TypeVerificationUsage,
}
impl<'e, 'a> MethodTypeVerification<'e, 'a> {
    pub fn environment(&self) -> &'e VerificationEnvironment<'a> {
        self.environment
    }
    pub fn class_id(&self) -> ClassId {
        self.class
    }
    pub fn method_index(&self) -> usize {
        self.method_index
    }
    pub fn code_validation(&self) -> &MethodCodeValidation<'a> {
        &self.code
    }
    pub fn usage(&self) -> TypeVerificationUsage {
        self.usage
    }
}
pub fn verify_method_types<'e, 'a: 'e>(
    environment: &'e VerificationEnvironment<'a>,
    class: ClassId,
    method_index: usize,
    options: TypeVerificationOptions,
) -> Result<MethodTypeVerification<'e, 'a>, Box<TypeVerificationError>> {
    let zero = |kind| {
        Box::new(TypeVerificationError {
            class,
            method_index,
            location: CodeValidationLocation::Method,
            usage: TypeVerificationUsage::default(),
            kind,
        })
    };
    let metadata = environment
        .registry
        .class(class)
        .map_err(|e| zero(TypeVerificationErrorKind::Registry(e)))?;
    if !environment.nodes.contains_key(&class) {
        return Err(zero(TypeVerificationErrorKind::UnretainedClass { class }));
    }
    let code = crate::validate_method_code(metadata, method_index, options.code).map_err(|e| {
        Box::new(TypeVerificationError {
            class,
            method_index,
            location: e.location,
            usage: TypeVerificationUsage {
                code: e.usage,
                ..Default::default()
            },
            kind: TypeVerificationErrorKind::Code(Box::new(e)),
        })
    })?;
    let mut cx = symbols::Proof::new(environment, class, options, code.usage());
    if let Err(kind) = cx.check(&code) {
        return Err(Box::new(TypeVerificationError {
            class,
            method_index,
            location: cx.location,
            usage: cx.usage,
            kind,
        }));
    }
    Ok(MethodTypeVerification {
        environment,
        class,
        method_index,
        code,
        usage: cx.usage,
    })
}
pub(super) type T<Ty> = Result<Ty, TypeVerificationErrorKind>;
pub(super) fn bad(reason: &'static str) -> TypeVerificationErrorKind {
    TypeVerificationErrorKind::TypeRule { reason }
}
pub(super) fn reserve<Ty>(v: &mut Vec<Ty>, n: usize, resource: &'static str) -> T<()> {
    v.try_reserve(n)
        .map_err(|_| TypeVerificationErrorKind::AllocationFailure { resource })
}

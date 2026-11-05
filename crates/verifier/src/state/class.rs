//! Whole-definition aggregation of the existing selected-method proof.
use super::*;
use crate::{CodeValidationErrorKind, CodeValidationResource};
use crabspot_classfile::ClassFile;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassTypeVerificationOptions {
    pub method: TypeVerificationOptions,
    pub total: ClassTypeVerificationLimits,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClassTypeVerificationLimits {
    pub max_methods: u64,
    pub max_code_bytes: u64,
    pub max_code_metadata_entries: u64,
    pub max_code_utf16_units: u64,
    pub max_code_work_units: u64,
    pub max_type_symbols: u64,
    pub max_state_cells: u64,
    pub max_edge_checks: u64,
    pub max_type_utf16_units: u64,
    pub max_type_work_units: u64,
}
impl Default for ClassTypeVerificationLimits {
    fn default() -> Self {
        Self {
            max_methods: 65_535,
            max_code_bytes: 8_388_608,
            max_code_metadata_entries: 4_194_304,
            max_code_utf16_units: 16_777_216,
            max_code_work_units: 67_108_864,
            max_type_symbols: 1_048_576,
            max_state_cells: 16_777_216,
            max_edge_checks: 16_777_216,
            max_type_utf16_units: 16_777_216,
            max_type_work_units: 268_435_456,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassTypeVerificationUsage {
    pub methods_started: u64,
    pub code_bodies: u64,
    pub bodyless: u64,
    pub verification: TypeVerificationUsage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassTypeVerificationResource {
    Methods,
    Code(CodeValidationResource),
    Type(TypeVerificationResource),
}
#[derive(Debug)]
pub struct ClassTypeVerification<'e, 'a> {
    environment: &'e VerificationEnvironment<'a>,
    class_id: ClassId,
    class: &'a ClassFile,
    usage: ClassTypeVerificationUsage,
}
impl<'e, 'a: 'e> ClassTypeVerification<'e, 'a> {
    pub fn environment(&self) -> &'e VerificationEnvironment<'a> {
        self.environment
    }
    pub fn class_id(&self) -> ClassId {
        self.class_id
    }
    pub fn class(&self) -> &'a ClassFile {
        self.class
    }
    pub fn usage(&self) -> ClassTypeVerificationUsage {
        self.usage
    }
}
#[derive(Debug)]
pub struct ClassTypeVerificationError {
    pub class: ClassId,
    pub method_index: Option<usize>,
    pub usage: ClassTypeVerificationUsage,
    pub kind: ClassTypeVerificationErrorKind,
}
#[derive(Debug)]
pub enum ClassTypeVerificationErrorKind {
    Registry(RegistryError),
    UnretainedClass {
        class: ClassId,
    },
    UnsupportedHistory {
        major: u16,
    },
    UnsupportedVersion {
        major: u16,
        minor: u16,
    },
    PreviewDisabled,
    Method(Box<TypeVerificationError>),
    AggregateLimit {
        resource: ClassTypeVerificationResource,
        limit: u64,
        required: u64,
        source: Option<Box<TypeVerificationError>>,
    },
    AccountingOverflow {
        resource: ClassTypeVerificationResource,
        source: Option<Box<TypeVerificationError>>,
    },
}
impl fmt::Display for ClassTypeVerificationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(f),
            Self::Method(error) => error.fmt(f),
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for ClassTypeVerificationErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(error) => Some(error),
            Self::Method(error) => Some(error.as_ref()),
            Self::AggregateLimit { source, .. } | Self::AccountingOverflow { source, .. } => source
                .as_deref()
                .map(|e| e as &(dyn std::error::Error + 'static)),
            _ => None,
        }
    }
}
impl fmt::Display for ClassTypeVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "class {:?} method {:?}: {}",
            self.class, self.method_index, self.kind
        )
    }
}
impl std::error::Error for ClassTypeVerificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.kind)
    }
}

type R = ClassTypeVerificationResource;
type E = ClassTypeVerificationErrorKind;
const RESOURCES: [R; 9] = [
    R::Code(CodeValidationResource::CodeBytes),
    R::Code(CodeValidationResource::MetadataEntries),
    R::Code(CodeValidationResource::Utf16Units),
    R::Code(CodeValidationResource::WorkUnits),
    R::Type(TypeVerificationResource::TypeSymbols),
    R::Type(TypeVerificationResource::StateCells),
    R::Type(TypeVerificationResource::EdgeChecks),
    R::Type(TypeVerificationResource::Utf16Units),
    R::Type(TypeVerificationResource::WorkUnits),
];
fn counters(usage: TypeVerificationUsage) -> [u64; 9] {
    [
        usage.code.code_bytes,
        usage.code.metadata_entries,
        usage.code.utf16_units,
        usage.code.work_units,
        usage.type_symbols,
        usage.state_cells,
        usage.edge_checks,
        usage.utf16_units,
        usage.work_units,
    ]
}
fn verification(v: [u64; 9]) -> TypeVerificationUsage {
    TypeVerificationUsage {
        code: CodeValidationUsage {
            code_bytes: v[0],
            metadata_entries: v[1],
            utf16_units: v[2],
            work_units: v[3],
        },
        type_symbols: v[4],
        state_cells: v[5],
        edge_checks: v[6],
        utf16_units: v[7],
        work_units: v[8],
    }
}
fn caps(options: TypeVerificationOptions) -> [u64; 9] {
    [
        options.code.max_code_bytes,
        options.code.max_metadata_entries,
        options.code.max_utf16_units,
        options.code.max_work_units,
        options.max_type_symbols,
        options.max_state_cells,
        options.max_edge_checks,
        options.max_utf16_units,
        options.max_work_units,
    ]
}
fn limits(total: ClassTypeVerificationLimits) -> [u64; 9] {
    [
        total.max_code_bytes,
        total.max_code_metadata_entries,
        total.max_code_utf16_units,
        total.max_code_work_units,
        total.max_type_symbols,
        total.max_state_cells,
        total.max_edge_checks,
        total.max_type_utf16_units,
        total.max_type_work_units,
    ]
}
fn remainder(
    prefix: TypeVerificationUsage,
    total: ClassTypeVerificationLimits,
) -> Result<[u64; 9], R> {
    let mut result = [0; 9];
    for (((out, limit), paid), resource) in result
        .iter_mut()
        .zip(limits(total))
        .zip(counters(prefix))
        .zip(RESOURCES)
    {
        *out = limit.checked_sub(paid).ok_or(resource)?;
    }
    Ok(result)
}
fn clamp(options: TypeVerificationOptions, remaining: [u64; 9]) -> TypeVerificationOptions {
    let mut local = caps(options);
    for (cap, available) in local.iter_mut().zip(remaining) {
        *cap = (*cap).min(available);
    }
    TypeVerificationOptions {
        code: CodeValidationOptions {
            allow_preview: options.code.allow_preview,
            max_code_bytes: local[0],
            max_metadata_entries: local[1],
            max_utf16_units: local[2],
            max_work_units: local[3],
        },
        max_type_symbols: local[4],
        max_state_cells: local[5],
        max_edge_checks: local[6],
        max_utf16_units: local[7],
        max_work_units: local[8],
    }
}
fn header(major: u16, minor: u16, preview: bool) -> Result<(), E> {
    if !(45..=65).contains(&major)
        || (major >= 56 && minor != 0 && !(major == 65 && minor == u16::MAX))
    {
        return Err(E::UnsupportedVersion { major, minor });
    }
    if major == 65 && minor == u16::MAX && !preview {
        return Err(E::PreviewDisabled);
    }
    if major < 51 {
        return Err(E::UnsupportedHistory { major });
    }
    Ok(())
}
impl ClassTypeVerificationUsage {
    fn start(&mut self, limit: u64) -> Result<(), E> {
        let required = self
            .methods_started
            .checked_add(1)
            .ok_or(E::AccountingOverflow {
                resource: R::Methods,
                source: None,
            })?;
        if required > limit {
            return Err(E::AggregateLimit {
                resource: R::Methods,
                limit,
                required,
                source: None,
            });
        }
        self.methods_started = required;
        Ok(())
    }
    fn prior_completed(&self) -> Result<(), R> {
        let completed = self
            .code_bodies
            .checked_add(self.bodyless)
            .ok_or(R::Methods)?;
        let prior = self.methods_started.checked_sub(1).ok_or(R::Methods)?;
        if completed != prior {
            return Err(R::Methods);
        }
        Ok(())
    }
    /// Publish only a complete accepted-usage/completed-count transaction. The
    /// separately funded entry count survives an impossible child-accounting guard.
    fn accept(
        &mut self,
        child: TypeVerificationUsage,
        completed: Option<bool>,
        bounds: ClassTypeVerificationLimits,
    ) -> Result<(), R> {
        let mut total = [0; 9];
        for ((((dst, prefix), paid), limit), resource) in total
            .iter_mut()
            .zip(counters(self.verification))
            .zip(counters(child))
            .zip(limits(bounds))
            .zip(RESOURCES)
        {
            *dst = prefix.checked_add(paid).ok_or(resource)?;
            if *dst > limit {
                return Err(resource);
            }
        }
        self.prior_completed()?;
        let mut next = *self;
        if let Some(body) = completed {
            let count = if body {
                &mut next.code_bodies
            } else {
                &mut next.bodyless
            };
            *count = count.checked_add(1).ok_or(R::Methods)?;
            if next.code_bodies.checked_add(next.bodyless) != Some(next.methods_started) {
                return Err(R::Methods);
            }
        }
        next.verification = verification(total);
        *self = next;
        Ok(())
    }
    fn failed(
        &mut self,
        error: Box<TypeVerificationError>,
        options: ClassTypeVerificationOptions,
        remaining: [u64; 9],
        prefix: TypeVerificationUsage,
    ) -> E {
        if let Err(resource) = self.accept(error.usage, None, options.total) {
            return E::AccountingOverflow {
                resource,
                source: Some(error),
            };
        }
        classify(error, options, remaining, prefix)
    }
}
fn direct_limit(error: &TypeVerificationError) -> Option<(R, u64, u64)> {
    match &error.kind {
        TypeVerificationErrorKind::ResourceLimit {
            resource,
            limit,
            required,
        } => Some((R::Type(*resource), *limit, *required)),
        TypeVerificationErrorKind::Code(error) => match error.kind {
            CodeValidationErrorKind::ResourceLimit {
                resource,
                limit,
                required,
            } => Some((R::Code(resource), limit, required)),
            _ => None,
        },
        _ => None,
    }
}
fn classify(
    error: Box<TypeVerificationError>,
    options: ClassTypeVerificationOptions,
    remaining: [u64; 9],
    prefix: TypeVerificationUsage,
) -> E {
    if let Some((resource, local_limit, required)) = direct_limit(&error) {
        // Nine fixed scalar identities, not an arbitrary diagnostic source walk.
        if let Some(index) = RESOURCES.iter().position(|r| *r == resource) {
            if remaining[index] < caps(options.method)[index] && local_limit == remaining[index] {
                return match counters(prefix)[index].checked_add(required) {
                    Some(required) => E::AggregateLimit {
                        resource,
                        limit: limits(options.total)[index],
                        required,
                        source: Some(error),
                    },
                    None => E::AccountingOverflow {
                        resource,
                        source: Some(error),
                    },
                };
            }
        }
    }
    E::Method(error)
}

pub fn verify_class_types<'e, 'a: 'e>(
    environment: &'e VerificationEnvironment<'a>,
    class: ClassId,
    options: ClassTypeVerificationOptions,
) -> Result<ClassTypeVerification<'e, 'a>, Box<ClassTypeVerificationError>> {
    let mut usage = ClassTypeVerificationUsage::default();
    let fail = |method_index, usage, kind| {
        Box::new(ClassTypeVerificationError {
            class,
            method_index,
            usage,
            kind,
        })
    };
    let metadata = environment
        .registry
        .class(class)
        .map_err(|error| fail(None, usage, E::Registry(error)))?;
    if !environment.nodes.contains_key(&class) {
        return Err(fail(None, usage, E::UnretainedClass { class }));
    }
    header(
        metadata.version.major,
        metadata.version.minor,
        options.method.code.allow_preview,
    )
    .map_err(|kind| fail(None, usage, kind))?;
    for index in 0..metadata.methods.len() {
        usage
            .start(options.total.max_methods)
            .map_err(|kind| fail(Some(index), usage, kind))?;
        let prefix = usage.verification;
        let remaining = remainder(prefix, options.total).map_err(|resource| {
            fail(
                Some(index),
                usage,
                E::AccountingOverflow {
                    resource,
                    source: None,
                },
            )
        })?;
        usage.prior_completed().map_err(|resource| {
            fail(
                Some(index),
                usage,
                E::AccountingOverflow {
                    resource,
                    source: None,
                },
            )
        })?;
        match verify_method_types(environment, class, index, clamp(options.method, remaining)) {
            Ok(proof) => {
                let paid = proof.usage();
                let body = proof.code_validation().code().is_some();
                // Only this fixed-size summary survives into the next iteration.
                drop(proof);
                usage
                    .accept(paid, Some(body), options.total)
                    .map_err(|resource| {
                        fail(
                            Some(index),
                            usage,
                            E::AccountingOverflow {
                                resource,
                                source: None,
                            },
                        )
                    })?;
            }
            Err(error) => {
                let kind = usage.failed(error, options, remaining, prefix);
                return Err(fail(Some(index), usage, kind));
            }
        }
    }
    Ok(ClassTypeVerification {
        environment,
        class_id: class,
        class: metadata,
        usage,
    })
}

#[cfg(test)]
#[path = "class_tests.rs"]
mod tests;

//! Bounded borrowed UTF-16 application arguments; no image I/O or Java effects.
use crate::{ApplicationError, ApplicationErrorKind, ApplicationStage, ApplicationUsage};
use crabspot_interpreter::MonitorLimits;
use crabspot_runtime::{ClassMirrorLimits, StringLimits};
use std::{error::Error, fmt};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationStringLimits {
    pub strings: StringLimits,
    pub max_arguments: u64,
    pub max_argument_units: u64,
    pub max_units_per_argument: u64,
}
impl Default for ApplicationStringLimits {
    fn default() -> Self {
        Self {
            strings: Default::default(),
            max_arguments: 65_536,
            max_argument_units: 4_194_304,
            max_units_per_argument: 1_048_576,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApplicationInvocation<'a> {
    pub arguments: &'a [&'a [u16]],
    pub monitors: MonitorLimits,
    pub mirrors: ClassMirrorLimits,
    pub strings: ApplicationStringLimits,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentResource {
    Arguments,
    ArgumentUnits,
    UnitsPerArgument,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArgumentError {
    Limit {
        index: Option<usize>,
        resource: ArgumentResource,
        limit: u64,
        required: u64,
    },
    Overflow {
        index: Option<usize>,
        resource: ArgumentResource,
    },
    Encoding {
        index: usize,
    },
    AllocationFailure {
        index: Option<usize>,
    },
}
impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "application arguments: {self:?}")
    }
}
impl Error for ArgumentError {}
pub(crate) fn validate(
    invocation: &ApplicationInvocation<'_>,
) -> Result<(), Box<ApplicationError>> {
    let check = || {
        let n = u64::try_from(invocation.arguments.len()).map_err(|_| ArgumentError::Overflow {
            index: None,
            resource: ArgumentResource::Arguments,
        })?;
        let limit = invocation.strings.max_arguments.min(i32::MAX as u64);
        if n > limit {
            return Err(ArgumentError::Limit {
                index: None,
                resource: ArgumentResource::Arguments,
                limit,
                required: n,
            });
        }
        let mut total = 0u64;
        for (index, value) in invocation.arguments.iter().enumerate() {
            let n = u64::try_from(value.len()).map_err(|_| ArgumentError::Overflow {
                index: Some(index),
                resource: ArgumentResource::UnitsPerArgument,
            })?;
            let limit = invocation.strings.max_units_per_argument;
            if n > limit {
                return Err(ArgumentError::Limit {
                    index: Some(index),
                    resource: ArgumentResource::UnitsPerArgument,
                    limit,
                    required: n,
                });
            }
            total = total.checked_add(n).ok_or(ArgumentError::Overflow {
                index: Some(index),
                resource: ArgumentResource::ArgumentUnits,
            })?;
            let limit = invocation.strings.max_argument_units;
            if total > limit {
                return Err(ArgumentError::Limit {
                    index: Some(index),
                    resource: ArgumentResource::ArgumentUnits,
                    limit,
                    required: total,
                });
            }
        }
        Ok(())
    };
    check().map_err(|e| {
        ApplicationError::new(
            ApplicationStage::Input,
            ApplicationErrorKind::Arguments(e),
            ApplicationUsage::default(),
        )
    })
}

//! Whole-method, file-local Code representation checks. This is not type-state
//! verification, linking, execution permission or a JVM compatibility certificate.
mod inspect;
mod scan;
mod stack_maps;

use crabspot_bytecode::{DecodeError, DecodedCode};
use crabspot_classfile::{ClassFile, Code, Member};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodeValidationOptions {
    pub allow_preview: bool,
    pub max_code_bytes: u64,
    pub max_metadata_entries: u64,
    pub max_utf16_units: u64,
    pub max_work_units: u64,
}
impl Default for CodeValidationOptions {
    fn default() -> Self {
        Self {
            allow_preview: false,
            max_code_bytes: 65_535,
            max_metadata_entries: 262_144,
            max_utf16_units: 1_048_576,
            max_work_units: 4_194_304,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CodeValidationUsage {
    pub code_bytes: u64,
    pub metadata_entries: u64,
    pub utf16_units: u64,
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeValidationResource {
    CodeBytes,
    MetadataEntries,
    Utf16Units,
    WorkUnits,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeValidationLocation {
    Method,
    MethodAttribute(usize),
    Instruction(u32),
    ExceptionHandler(usize),
    CodeAttribute(usize),
    StackMapFrame { attribute: usize, frame: usize },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodeValidationErrorKind {
    UnsupportedVersion {
        major: u16,
        minor: u16,
    },
    PreviewDisabled,
    UnsupportedHistoricalInitializer,
    MethodIndexOutOfBounds {
        index: usize,
        count: usize,
    },
    InvalidMetadata(&'static str),
    InvalidConstant {
        index: u16,
        expected: &'static str,
    },
    InvalidDescriptor {
        index: u16,
    },
    InvalidName {
        index: u16,
    },
    InvalidOperand(&'static str),
    LocalOutOfBounds {
        index: u16,
        width: u8,
        max_locals: u16,
    },
    InvalidBoundary {
        offset: u32,
        expected: &'static str,
    },
    InvalidStackMap(&'static str),
    Decode(DecodeError),
    ResourceLimit {
        resource: CodeValidationResource,
        limit: u64,
        required: u64,
    },
    AccountingOverflow {
        resource: CodeValidationResource,
    },
    AllocationFailure {
        resource: &'static str,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeValidationError {
    pub method_index: usize,
    pub location: CodeValidationLocation,
    pub usage: CodeValidationUsage,
    pub kind: CodeValidationErrorKind,
}
impl fmt::Display for CodeValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "method {} {:?}: ", self.method_index, self.location)?;
        self.kind.fmt(f)
    }
}
impl fmt::Display for CodeValidationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeValidationErrorKind::Decode(error) => error.fmt(f),
            CodeValidationErrorKind::InvalidMetadata(reason)
            | CodeValidationErrorKind::InvalidOperand(reason)
            | CodeValidationErrorKind::InvalidStackMap(reason) => f.write_str(reason),
            kind => write!(f, "{kind:?}"),
        }
    }
}
impl std::error::Error for CodeValidationErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}
impl std::error::Error for CodeValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            CodeValidationErrorKind::Decode(error) => Some(error),
            _ => None,
        }
    }
}
#[derive(Debug)]
enum Body<'a> {
    Absent,
    Present {
        code: &'a Code,
        decoded: DecodedCode,
    },
}
#[derive(Debug)]
pub struct MethodCodeValidation<'a> {
    class: &'a ClassFile,
    method_index: usize,
    method: &'a Member,
    body: Body<'a>,
    usage: CodeValidationUsage,
}
impl<'a> MethodCodeValidation<'a> {
    pub fn class(&self) -> &'a ClassFile {
        self.class
    }
    pub fn method_index(&self) -> usize {
        self.method_index
    }
    pub fn method(&self) -> &'a Member {
        self.method
    }
    pub fn code(&self) -> Option<&'a Code> {
        match &self.body {
            Body::Absent => None,
            Body::Present { code, .. } => Some(code),
        }
    }
    pub fn decoded(&self) -> Option<&DecodedCode> {
        match &self.body {
            Body::Absent => None,
            Body::Present { decoded, .. } => Some(decoded),
        }
    }
    pub fn usage(&self) -> CodeValidationUsage {
        self.usage
    }
}
pub fn validate_method_code(
    class: &ClassFile,
    method_index: usize,
    options: CodeValidationOptions,
) -> Result<MethodCodeValidation<'_>, CodeValidationError> {
    let mut context = inspect::Context::new(class, method_index, options);
    match context.run() {
        Ok((method, body)) => Ok(MethodCodeValidation {
            class,
            method_index,
            method,
            body,
            usage: context.usage,
        }),
        Err(kind) => Err(CodeValidationError {
            method_index,
            location: context.location,
            usage: context.usage,
            kind,
        }),
    }
}
type Checked<T> = Result<T, CodeValidationErrorKind>;

#[cfg(test)]
mod tests;

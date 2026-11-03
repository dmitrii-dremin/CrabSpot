use std::fmt;

/// A byte offset into the original input and the structure being parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub context: &'static str,
    pub kind: ErrorKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    UnexpectedEof {
        needed: usize,
        remaining: usize,
    },
    InvalidMagic(u32),
    UnsupportedVersion {
        major: u16,
        minor: u16,
    },
    PreviewDisabled,
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    InvalidConstantPoolTag(u8),
    InvalidIndex {
        index: u16,
        expected: &'static str,
    },
    InvalidModifiedUtf8,
    InvalidDescriptor,
    InvalidStructure(&'static str),
    TrailingBytes(usize),
}

impl ParseError {
    pub(crate) fn new(offset: usize, context: &'static str, kind: ErrorKind) -> Self {
        Self {
            offset,
            context,
            kind,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte 0x{:x}: ", self.context, self.offset)?;
        match &self.kind {
            ErrorKind::UnexpectedEof { needed, remaining } => {
                write!(f, "need {needed} bytes, only {remaining} remain")
            }
            ErrorKind::InvalidMagic(magic) => {
                write!(f, "invalid magic 0x{magic:08x}; expected 0xcafebabe")
            }
            ErrorKind::UnsupportedVersion { major, minor } => {
                write!(f, "unsupported class-file version {major}.{minor}")
            }
            ErrorKind::PreviewDisabled => {
                write!(f, "Java 21 preview class requires explicit preview opt-in")
            }
            ErrorKind::ResourceLimit {
                resource,
                limit,
                actual,
            } => write!(
                f,
                "{resource} exceeds configured limit {limit} (actual {actual})"
            ),
            ErrorKind::InvalidConstantPoolTag(tag) => write!(f, "unknown constant-pool tag {tag}"),
            ErrorKind::InvalidIndex { index, expected } => {
                write!(f, "constant-pool index #{index} must refer to {expected}")
            }
            ErrorKind::InvalidModifiedUtf8 => write!(f, "invalid modified UTF-8 encoding"),
            ErrorKind::InvalidDescriptor => write!(f, "invalid JVM descriptor"),
            ErrorKind::InvalidStructure(reason) => f.write_str(reason),
            ErrorKind::TrailingBytes(count) => write!(f, "{count} unconsumed bytes"),
        }
    }
}

impl std::error::Error for ParseError {}

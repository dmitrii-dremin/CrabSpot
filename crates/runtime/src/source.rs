//! Definition-byte retrieval, independent of filesystems and loader delegation.
use std::{collections::BTreeMap, fmt};

/// A host-side retrieval failure, distinct from an absent definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceError {
    pub message: String,
}

impl SourceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

/// Retrieve bytes for one exact semantic Java name in an explicit loader request.
/// Implementations must honor `max_bytes` before materializing external data.
/// Absence is not a failure, and neither outcome implies parent delegation.
pub trait DefinitionSource {
    fn read_definition(
        &mut self,
        name: &[u16],
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, SourceError>;
}

/// A deterministic source keyed by exact UTF-16 names, without display conversion.
#[derive(Clone, Debug, Default)]
pub struct InMemorySource {
    definitions: BTreeMap<Vec<u16>, Vec<u8>>,
}

impl InMemorySource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure a source entry. This does not register or validate a class.
    pub fn insert(&mut self, name: impl Into<Vec<u16>>, bytes: Vec<u8>) -> Option<Vec<u8>> {
        self.definitions.insert(name.into(), bytes)
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

impl DefinitionSource for InMemorySource {
    fn read_definition(
        &mut self,
        name: &[u16],
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        let Some(bytes) = self.definitions.get(name) else {
            return Ok(None);
        };
        if bytes.len() > max_bytes {
            return Err(SourceError::new(format!(
                "in-memory definition contains {} bytes, exceeding the requested read limit of {max_bytes} bytes",
                bytes.len()
            )));
        }
        Ok(Some(bytes.clone()))
    }
}

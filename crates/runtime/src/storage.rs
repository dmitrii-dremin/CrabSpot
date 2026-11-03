//! Default-only typed static storage. This does not perform complete preparation,
//! apply ConstantValue, run initializers, or expose a mutation/final-field bypass.
use crate::{
    ClassId, DefinitionRegistry, FieldId, HandleKind, HeapRef, HierarchyPlan, Reference,
    RegistryError, Value,
};
use crabspot_classfile::{
    ParseError,
    descriptor::{FieldType, parse_field_descriptor},
};
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticStorageOptions {
    pub max_slots: usize,
}
impl Default for StaticStorageOptions {
    fn default() -> Self {
        Self { max_slots: 65536 }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageError {
    Registry(RegistryError),
    InvalidMetadata(&'static str),
    InvalidDescriptor { field: FieldId, error: ParseError },
    MissingStaticLocation { field: FieldId },
    ResourceLimit { limit: usize, required: usize },
}
impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => write!(f, "static-storage metadata lookup failed: {error}"),
            Self::InvalidMetadata(reason) => write!(f, "invalid static-storage metadata: {reason}"),
            Self::InvalidDescriptor { field, error } => {
                write!(f, "invalid descriptor for {field:?}: {error}")
            }
            Self::MissingStaticLocation { field } => {
                write!(f, "no retained static location for {field:?}")
            }
            Self::ResourceLimit { limit, required } => write!(
                f,
                "static-storage slot limit {limit} exceeded (required {required})"
            ),
        }
    }
}
impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(error) => Some(error),
            Self::InvalidDescriptor { error, .. } => Some(error),
            _ => None,
        }
    }
}

fn count_slots(current: usize, amount: usize, limit: usize) -> Result<usize, StorageError> {
    let required = current
        .checked_add(amount)
        .ok_or(StorageError::ResourceLimit {
            limit,
            required: usize::MAX,
        })?;
    if required > limit {
        Err(StorageError::ResourceLimit { limit, required })
    } else {
        Ok(required)
    }
}
fn default_value(field: FieldId, descriptor: &[u16]) -> Result<Value, StorageError> {
    let field_type = parse_field_descriptor(descriptor)
        .map_err(|error| StorageError::InvalidDescriptor { field, error })?;
    Ok(match field_type {
        FieldType::Byte
        | FieldType::Char
        | FieldType::Int
        | FieldType::Short
        | FieldType::Boolean => Value::Int(0),
        FieldType::Long => Value::Long(0),
        FieldType::Float => Value::Float(0),
        FieldType::Double => Value::Double(0),
        FieldType::Object(_) | FieldType::Array { .. } => Value::Reference(Reference::null()),
    })
}
fn validate_field(
    registry: &DefinitionRegistry,
    owner: ClassId,
    field: FieldId,
    is_static: bool,
) -> Result<(), StorageError> {
    let metadata = registry.field(field).map_err(StorageError::Registry)?;
    if field.owner() != owner {
        return Err(StorageError::InvalidMetadata(
            "retained field owner does not match its class layout",
        ));
    }
    if (metadata.access_flags & 0x0008 != 0) != is_static {
        return Err(StorageError::InvalidMetadata(
            "retained field has the wrong static/instance category",
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub struct StaticStorage {
    registry: u64,
    fields: Vec<FieldId>,
    values: Vec<Value>,
    positions: HashMap<FieldId, usize>,
}
impl StaticStorage {
    pub fn new(
        registry: &DefinitionRegistry,
        plan: &HierarchyPlan,
        options: StaticStorageOptions,
    ) -> Result<Self, StorageError> {
        registry
            .class(plan.root())
            .map_err(StorageError::Registry)?;
        let mut count = 0;
        for class in plan.classes() {
            registry.class(*class).map_err(StorageError::Registry)?;
            let layout = plan.layout(*class).ok_or(StorageError::InvalidMetadata(
                "class has no retained hierarchy layout",
            ))?;
            for field in layout.declared_instance_fields() {
                validate_field(registry, *class, *field, false)?;
            }
            for field in layout.declared_static_fields() {
                validate_field(registry, *class, *field, true)?;
            }
            count = count_slots(
                count,
                layout.declared_static_fields().len(),
                options.max_slots,
            )?;
        }
        // Allocation is based on the actual checked count, never on max_slots.
        let mut fields = Vec::with_capacity(count);
        let mut values = Vec::with_capacity(count);
        let mut positions = HashMap::with_capacity(count);
        // Classify each immutable CP descriptor entry once. Scalar keys avoid
        // repeatedly hashing a long descriptor reused by many fields, and no
        // descriptor copy is retained per slot.
        let mut defaults: HashMap<(ClassId, u16), Value> = HashMap::new();
        for class in plan.classes() {
            let metadata = registry.class(*class).map_err(StorageError::Registry)?;
            let layout = plan.layout(*class).ok_or(StorageError::InvalidMetadata(
                "class has no retained hierarchy layout",
            ))?;
            for field in layout.declared_static_fields() {
                let member = registry.field(*field).map_err(StorageError::Registry)?;
                let key = (*class, member.descriptor_index);
                let value = if let Some(value) = defaults.get(&key) {
                    *value
                } else {
                    let descriptor = metadata
                        .constant_pool
                        .utf8(member.descriptor_index)
                        .map_err(|error| StorageError::Registry(RegistryError::Parse(error)))?
                        .units();
                    let value = default_value(*field, descriptor)?;
                    defaults.insert(key, value);
                    value
                };
                if positions.insert(*field, fields.len()).is_some() {
                    return Err(StorageError::InvalidMetadata(
                        "duplicate retained static field handle",
                    ));
                }
                fields.push(*field);
                values.push(value);
            }
        }
        Ok(Self {
            registry: plan.root().registry,
            fields,
            values,
            positions,
        })
    }
    pub fn fields(&self) -> &[FieldId] {
        &self.fields
    }
    pub fn slot_count(&self) -> usize {
        self.fields.len()
    }
    pub fn get(&self, field: FieldId) -> Result<Value, StorageError> {
        if field.owner().registry != self.registry {
            return Err(StorageError::Registry(RegistryError::InvalidHandle {
                kind: HandleKind::Field,
                reason: "belongs to another registry",
            }));
        }
        let position = self
            .positions
            .get(&field)
            .ok_or(StorageError::MissingStaticLocation { field })?;
        self.values
            .get(*position)
            .copied()
            .ok_or(StorageError::InvalidMetadata(
                "static slot index is outside storage",
            ))
    }
    /// Allocation-free borrowed enumeration of non-null managed identities.
    /// Default-only static reference slots remain null, so this is currently empty.
    /// This is neither mutable root locations nor a synchronized collector snapshot.
    pub fn heap_roots(&self) -> impl Iterator<Item = HeapRef> + '_ {
        self.values.iter().filter_map(|value| {
            if let Value::Reference(reference) = value {
                reference.heap_ref()
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field() -> FieldId {
        FieldId {
            owner: ClassId {
                registry: 1,
                index: 0,
            },
            index: 0,
        }
    }
    #[test]
    fn storage_count_arithmetic_is_checked_without_allocating_from_limits() {
        assert_eq!(
            count_slots(usize::MAX, 1, usize::MAX).unwrap_err(),
            StorageError::ResourceLimit {
                limit: usize::MAX,
                required: usize::MAX
            }
        );
        assert_eq!(count_slots(usize::MAX, 0, usize::MAX).unwrap(), usize::MAX);
        assert_eq!(count_slots(2, 3, 5).unwrap(), 5);
        assert_eq!(
            count_slots(2, 4, 5).unwrap_err(),
            StorageError::ResourceLimit {
                limit: 5,
                required: 6
            }
        );
    }
    #[test]
    fn invalid_descriptors_cannot_be_defaulted_as_successful_values() {
        for text in ["", "V", "()V", "[V", "L;", "Ljava/lang/Object", "II"] {
            assert!(
                matches!(
                    default_value(field(), &text.encode_utf16().collect::<Vec<_>>()),
                    Err(StorageError::InvalidDescriptor { .. })
                ),
                "{text}"
            );
        }
        for (text, expected) in [
            ("I", Value::Int(0)),
            ("J", Value::Long(0)),
            ("F", Value::Float(0)),
            ("D", Value::Double(0)),
            ("Ljava/lang/Object;", Value::Reference(Reference::null())),
            ("[[I", Value::Reference(Reference::null())),
        ] {
            assert_eq!(
                default_value(field(), &text.encode_utf16().collect::<Vec<_>>()).unwrap(),
                expected
            );
        }
    }
    #[test]
    fn null_filtering_and_private_corrupt_or_foreign_locations_are_checked() {
        let field = field();
        let mut storage = StaticStorage {
            registry: 1,
            fields: vec![field],
            values: vec![Value::Reference(Reference::null())],
            positions: HashMap::from([(field, 0)]),
        };
        assert_eq!(storage.heap_roots().count(), 0);
        assert_eq!(
            storage.get(field).unwrap(),
            Value::Reference(Reference::null())
        );
        assert!(matches!(
            storage.get(FieldId {
                owner: field.owner(),
                index: usize::MAX
            }),
            Err(StorageError::MissingStaticLocation { .. })
        ));
        assert!(matches!(
            storage.get(FieldId {
                owner: ClassId {
                    registry: 2,
                    index: 0
                },
                index: 0
            }),
            Err(StorageError::Registry(_))
        ));
        storage.positions.insert(field, usize::MAX);
        assert!(matches!(
            storage.get(field),
            Err(StorageError::InvalidMetadata(_))
        ));
    }
}

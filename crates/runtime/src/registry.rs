use crate::{
    ClassId, DefinitionSource, FieldId, HandleKind, LoaderId, MethodId, RegistryError as E,
    RegistryResource as R,
};
use crabspot_classfile::{ClassFile, JavaString, Member, ParseOptions, parse_with_options};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

// Only identity allocation is global; no definitions, loader policies or execution
// state live here. Relaxed ordering suffices for uniqueness of checked atomic IDs.
// Zero and the exhausted terminal value are never issued, and IDs never wrap/reuse.
static NEXT_REGISTRY: AtomicU64 = AtomicU64::new(1);
fn allocate_identity(allocator: &AtomicU64) -> Result<u64, E> {
    let mut current = allocator.load(Ordering::Relaxed);
    loop {
        let next = if current == 0 {
            None
        } else {
            current.checked_add(1)
        }
        .ok_or(E::IdentityExhausted)?;
        match allocator.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(current),
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, sync::Arc};

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn bytes() -> Vec<u8> {
        // A class with one native method and one field. Definitions retain these
        // declarations without pretending either one is resolved or executable.
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 9];
        for (tag, data) in [
            (1, b"T".as_slice()),
            (7, &[0, 1]),
            (1, b"java/lang/Object".as_slice()),
            (7, &[0, 3]),
            (1, b"m".as_slice()),
            (1, b"()V".as_slice()),
            (1, b"f".as_slice()),
            (1, b"I".as_slice()),
        ] {
            bytes.push(tag);
            if tag == 1 {
                bytes.extend((data.len() as u16).to_be_bytes());
            }
            bytes.extend(data);
        }
        for word in [0x21u16, 2, 4, 0, 1, 1, 7, 8, 0, 1, 0x0109, 5, 6, 0, 0] {
            bytes.extend(word.to_be_bytes());
        }
        bytes
    }
    fn registry() -> DefinitionRegistry {
        DefinitionRegistry::new(RegistryOptions::default()).unwrap()
    }

    #[test]
    fn private_forged_loader_class_and_member_indices_are_rejected() {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        let class = registry
            .define(loader, &units("T"), &bytes(), ParseOptions::default())
            .unwrap();
        for index in [1, usize::MAX] {
            let invalid = LoaderId {
                registry: registry.identity,
                index,
            };
            assert!(matches!(
                registry.lookup_class(invalid, &units("T")),
                Err(E::InvalidHandle {
                    kind: HandleKind::Loader,
                    ..
                })
            ));
        }
        for index in [1, usize::MAX] {
            let invalid = ClassId {
                registry: registry.identity,
                index,
            };
            assert!(matches!(
                registry.class(invalid),
                Err(E::InvalidHandle {
                    kind: HandleKind::Class,
                    ..
                })
            ));
            assert!(registry.defining_loader(invalid).is_err());
            assert!(
                registry
                    .find_declared_method(invalid, &units("m"), &units("()V"))
                    .is_err()
            );
            assert!(
                registry
                    .find_declared_field(invalid, &units("f"), &units("I"))
                    .is_err()
            );
            assert!(matches!(
                registry.method(MethodId {
                    owner: invalid,
                    index: 0
                }),
                Err(E::InvalidHandle {
                    kind: HandleKind::Method,
                    ..
                })
            ));
            assert!(matches!(
                registry.field(FieldId {
                    owner: invalid,
                    index: 0
                }),
                Err(E::InvalidHandle {
                    kind: HandleKind::Field,
                    ..
                })
            ));
        }
        for index in [1, usize::MAX] {
            assert!(matches!(
                registry.method(MethodId {
                    owner: class,
                    index
                }),
                Err(E::InvalidHandle {
                    kind: HandleKind::Method,
                    ..
                })
            ));
            assert!(matches!(
                registry.field(FieldId {
                    owner: class,
                    index
                }),
                Err(E::InvalidHandle {
                    kind: HandleKind::Field,
                    ..
                })
            ));
        }
        assert_eq!(
            registry
                .method(MethodId {
                    owner: class,
                    index: 0
                })
                .unwrap()
                .access_flags,
            0x0109
        );
        assert_eq!(
            registry
                .field(FieldId {
                    owner: class,
                    index: 0
                })
                .unwrap()
                .access_flags,
            1
        );
        let foreign = ClassId {
            registry: 0,
            index: 0,
        };
        assert!(registry.class(foreign).is_err());
        assert!(
            registry
                .method(MethodId {
                    owner: foreign,
                    index: 0
                })
                .is_err()
        );
        assert!(
            registry
                .field(FieldId {
                    owner: foreign,
                    index: 0
                })
                .is_err()
        );
    }

    #[test]
    fn identity_allocator_exhausts_without_reuse_or_wraparound() {
        let allocator = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate_identity(&allocator).unwrap(), u64::MAX - 1);
        assert_eq!(
            allocate_identity(&allocator).unwrap_err(),
            E::IdentityExhausted
        );
        assert_eq!(
            allocate_identity(&allocator).unwrap_err(),
            E::IdentityExhausted
        );
        assert_eq!(allocator.load(Ordering::Relaxed), u64::MAX);
        assert_eq!(
            allocate_identity(&AtomicU64::new(0)).unwrap_err(),
            E::IdentityExhausted
        );
        let allocator = Arc::new(AtomicU64::new(1));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let allocator = allocator.clone();
                std::thread::spawn(move || {
                    (0..128)
                        .map(|_| allocate_identity(&allocator).unwrap())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let identities: HashSet<_> = threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(identities.len(), 1024);
        assert!(!identities.contains(&0));
    }

    #[test]
    fn accounting_checks_overflow_and_preserves_failed_publication_state() {
        for resource in [
            R::Loaders,
            R::Definitions,
            R::DefinitionBytes,
            R::TotalDefinitionBytes,
            R::SourceReadBytes,
        ] {
            assert_eq!(
                increased(usize::MAX, 1, usize::MAX, resource).unwrap_err(),
                E::ResourceLimit {
                    resource,
                    limit: usize::MAX,
                    required: usize::MAX
                }
            );
            assert_eq!(increased(3, 2, 5, resource).unwrap(), 5);
            assert!(increased(3, 3, 5, resource).is_err());
        }
        let mut registry = DefinitionRegistry::new(RegistryOptions {
            max_total_definition_bytes: usize::MAX,
            ..Default::default()
        })
        .unwrap();
        let loader = registry.create_loader().unwrap();
        registry.total_bytes = usize::MAX - 1;
        assert!(matches!(
            registry.define(loader, &units("T"), &bytes(), ParseOptions::default()),
            Err(E::ResourceLimit {
                resource: R::TotalDefinitionBytes,
                ..
            })
        ));
        assert_eq!(registry.definition_count(), 0);
        assert_eq!(registry.total_definition_bytes(), usize::MAX - 1);
        assert_eq!(registry.lookup_class(loader, &units("T")).unwrap(), None);
        registry.total_bytes = 0;
        assert!(matches!(
            registry.define(loader, &units("T"), &[0], ParseOptions::default()),
            Err(E::Parse(_))
        ));
        assert_eq!(registry.definition_count(), 0);
        assert_eq!(registry.total_definition_bytes(), 0);
        assert!(matches!(
            registry.define(loader, &units("Wrong"), &bytes(), ParseOptions::default()),
            Err(E::NameMismatch { .. })
        ));
        assert_eq!(registry.definition_count(), 0);
        assert_eq!(registry.total_definition_bytes(), 0);
        let class = registry
            .define(loader, &units("T"), &bytes(), ParseOptions::default())
            .unwrap();
        assert_eq!(class.index, 0);
        assert_eq!(registry.total_definition_bytes(), bytes().len());
        assert_eq!(
            registry
                .define(loader, &units("T"), &bytes(), ParseOptions::default())
                .unwrap_err(),
            E::DuplicateDefinition { existing: class }
        );
        assert_eq!(registry.definition_count(), 1);
        assert_eq!(
            registry.class(class).unwrap().name().unwrap().units(),
            units("T")
        );
    }

    #[test]
    fn name_keys_preserve_code_units_and_bound_their_representation() {
        for name in [
            &[0xd800][..],
            &[0xdc00],
            &[0],
            &[0x41, 0xd800, 0x2f, 0xdc00],
        ] {
            assert!(validate_name(name).is_ok());
        }
        for text in ["", "/T", "T/", "T//U", "T.U", "LT;"] {
            assert!(validate_name(&units(text)).is_err(), "{text}");
        }
        assert!(matches!(
            validate_name(&units("[I")),
            Err(E::UnsupportedDefinition(_))
        ));
        assert!(validate_name(&vec![65; usize::from(u16::MAX)]).is_ok());
        assert!(matches!(
            validate_name(&vec![65; usize::from(u16::MAX) + 1]),
            Err(E::InvalidName(_))
        ));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistryOptions {
    pub max_loaders: usize,
    pub max_definitions: usize,
    pub max_definition_bytes: usize,
    pub max_total_definition_bytes: usize,
}
impl Default for RegistryOptions {
    fn default() -> Self {
        Self {
            max_loaders: 256,
            max_definitions: 1024,
            max_definition_bytes: 16 * 1024 * 1024,
            max_total_definition_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
struct Definition {
    loader: LoaderId,
    class: ClassFile,
}
pub(crate) struct StagedDefinition {
    registry: u64,
    base_count: usize,
    base_bytes: usize,
    loader: LoaderId,
    loader_index: usize,
    class: ClassFile,
    key: Vec<u16>,
    total: usize,
}

/// Owns immutable parsed definitions. Admission does not prove verification,
/// linking, access legality, hierarchy validity, or initialization.
#[derive(Debug)]
pub struct DefinitionRegistry {
    identity: u64,
    options: RegistryOptions,
    names: Vec<BTreeMap<Vec<u16>, ClassId>>,
    definitions: Vec<Definition>,
    total_bytes: usize,
}

fn increased(current: usize, amount: usize, limit: usize, resource: R) -> Result<usize, E> {
    let required = current.checked_add(amount).ok_or(E::ResourceLimit {
        resource,
        limit,
        required: usize::MAX,
    })?;
    if required > limit {
        Err(E::ResourceLimit {
            resource,
            limit,
            required,
        })
    } else {
        Ok(required)
    }
}
pub(crate) fn validate_name(name: &[u16]) -> Result<(), E> {
    if name.first() == Some(&91) {
        return Err(E::UnsupportedDefinition(
            "array classes are outside ordinary byte definitions",
        ));
    }
    if name.len() > usize::from(u16::MAX) {
        return Err(E::InvalidName(
            "name exceeds the class-file Utf8 code-unit bound",
        ));
    }
    if name.is_empty()
        || !name
            .split(|unit| *unit == 47)
            .all(|part| !part.is_empty() && !part.iter().any(|unit| matches!(*unit, 46 | 59 | 91)))
    {
        return Err(E::InvalidName(
            "use nonempty unqualified names separated by single internal-form slashes",
        ));
    }
    Ok(())
}

impl DefinitionRegistry {
    pub(crate) fn identity(&self) -> u64 {
        self.identity
    }
    pub fn new(options: RegistryOptions) -> Result<Self, E> {
        Ok(Self {
            identity: allocate_identity(&NEXT_REGISTRY)?,
            options,
            names: Vec::new(),
            definitions: Vec::new(),
            total_bytes: 0,
        })
    }
    pub fn loader_count(&self) -> usize {
        self.names.len()
    }
    pub fn definition_count(&self) -> usize {
        self.definitions.len()
    }
    /// Aggregate admitted input-byte volume, not an exact heap-allocation measure.
    pub fn total_definition_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn create_loader(&mut self) -> Result<LoaderId, E> {
        increased(self.names.len(), 1, self.options.max_loaders, R::Loaders)?;
        let loader = LoaderId {
            registry: self.identity,
            index: self.names.len(),
        };
        self.names.push(BTreeMap::new());
        Ok(loader)
    }
    pub(crate) fn loader_index(&self, loader: LoaderId) -> Result<usize, E> {
        if loader.registry != self.identity {
            return Err(E::InvalidHandle {
                kind: HandleKind::Loader,
                reason: "belongs to another registry",
            });
        }
        if self.names.get(loader.index).is_none() {
            return Err(E::InvalidHandle {
                kind: HandleKind::Loader,
                reason: "loader index is outside this registry",
            });
        }
        Ok(loader.index)
    }
    fn definition(&self, class: ClassId, kind: HandleKind) -> Result<&Definition, E> {
        if class.registry != self.identity {
            return Err(E::InvalidHandle {
                kind,
                reason: "belongs to another registry",
            });
        }
        self.definitions.get(class.index).ok_or(E::InvalidHandle {
            kind,
            reason: "owner class index is outside this registry",
        })
    }

    pub fn lookup_class(&self, loader: LoaderId, name: &[u16]) -> Result<Option<ClassId>, E> {
        let index = self.loader_index(loader)?;
        validate_name(name)?;
        Ok(self
            .names
            .get(index)
            .and_then(|names| names.get(name))
            .copied())
    }
    pub fn class(&self, class: ClassId) -> Result<&ClassFile, E> {
        Ok(&self.definition(class, HandleKind::Class)?.class)
    }
    pub fn defining_loader(&self, class: ClassId) -> Result<LoaderId, E> {
        Ok(self.definition(class, HandleKind::Class)?.loader)
    }

    /// Publish one unverified, unlinked class definition after all fallible checks.
    /// Explicit duplicates fail; use load_from_source for cache-aware retrieval.
    pub fn define(
        &mut self,
        loader: LoaderId,
        expected_name: &[u16],
        bytes: &[u8],
        parse_options: ParseOptions,
    ) -> Result<ClassId, E> {
        let prepared = self.stage_definition(loader, expected_name, bytes, parse_options)?;
        Ok(self.publish_definition(prepared))
    }

    /// Owns all admission work while leaving the registry's logical state intact.
    pub(crate) fn stage_definition(
        &self,
        loader: LoaderId,
        expected_name: &[u16],
        bytes: &[u8],
        parse_options: ParseOptions,
    ) -> Result<StagedDefinition, E> {
        let loader_index = self.loader_index(loader)?;
        validate_name(expected_name)?;
        if let Some(existing) = self.lookup_class(loader, expected_name)? {
            return Err(E::DuplicateDefinition { existing });
        }
        increased(
            self.definitions.len(),
            1,
            self.options.max_definitions,
            R::Definitions,
        )?;
        increased(
            0,
            bytes.len(),
            self.options.max_definition_bytes,
            R::DefinitionBytes,
        )?;
        let total = increased(
            self.total_bytes,
            bytes.len(),
            self.options.max_total_definition_bytes,
            R::TotalDefinitionBytes,
        )?;
        let class = parse_with_options(bytes, parse_options).map_err(E::Parse)?;
        if class.access_flags & 0x8000 != 0 {
            return Err(E::UnsupportedDefinition(
                "module descriptors are outside ordinary class definitions",
            ));
        }
        let declared = class.name().map_err(E::Parse)?;
        validate_name(declared.units())?;
        if declared.units() != expected_name {
            return Err(E::NameMismatch {
                expected: JavaString::from_units(expected_name.to_vec()),
                actual: declared.clone(),
            });
        }
        let key = declared.units().to_vec();
        Ok(StagedDefinition {
            registry: self.identity,
            base_count: self.definitions.len(),
            base_bytes: self.total_bytes,
            loader,
            loader_index,
            class,
            key,
            total,
        })
    }

    /// Private callers retain exclusive registry authority between staging and
    /// publication. Existing registry storage allocation conventions are unchanged.
    pub(crate) fn publish_definition(&mut self, staged: StagedDefinition) -> ClassId {
        let StagedDefinition {
            registry: _,
            base_count: _,
            base_bytes: _,
            loader,
            loader_index,
            class,
            key,
            total,
        } = staged;
        let id = ClassId {
            registry: self.identity,
            index: self.definitions.len(),
        };
        // No recoverable operation after publication begins can reject admission.
        self.definitions.push(Definition { loader, class });
        self.names[loader_index].insert(key, id);
        self.total_bytes = total;
        id
    }
    pub(crate) fn validate_definition_stage(&self, stage: &StagedDefinition) -> Result<(), E> {
        if stage.registry != self.identity
            || stage.base_count != self.definitions.len()
            || stage.base_bytes != self.total_bytes
            || stage.loader.registry != self.identity
            || stage.loader.index != stage.loader_index
            || self.names.get(stage.loader_index).is_none()
        {
            return Err(E::InvalidHandle {
                kind: HandleKind::Class,
                reason: "stale or foreign private admission stage",
            });
        }
        Ok(())
    }

    pub fn load_from_source<S: DefinitionSource + ?Sized>(
        &mut self,
        loader: LoaderId,
        name: &[u16],
        source: &mut S,
        parse_options: ParseOptions,
    ) -> Result<ClassId, E> {
        self.loader_index(loader)?;
        validate_name(name)?;
        if let Some(existing) = self.lookup_class(loader, name)? {
            return Ok(existing);
        }
        increased(
            self.definitions.len(),
            1,
            self.options.max_definitions,
            R::Definitions,
        )?;
        let remaining = self
            .options
            .max_total_definition_bytes
            .checked_sub(self.total_bytes)
            .ok_or(E::ResourceLimit {
                resource: R::TotalDefinitionBytes,
                limit: self.options.max_total_definition_bytes,
                required: self.total_bytes,
            })?;
        let limit = remaining
            .min(self.options.max_definition_bytes)
            .min(parse_options.max_class_bytes);
        let bytes = source
            .read_definition(name, limit)
            .map_err(E::Source)?
            .ok_or_else(|| E::NotFound {
                name: JavaString::from_units(name.to_vec()),
            })?;
        increased(0, bytes.len(), limit, R::SourceReadBytes)?;
        self.define(loader, name, &bytes, parse_options)
    }

    pub fn find_declared_method(
        &self,
        class: ClassId,
        name: &[u16],
        descriptor: &[u16],
    ) -> Result<Option<MethodId>, E> {
        let classfile = self.class(class)?;
        for (index, method) in classfile.methods.iter().enumerate() {
            if classfile
                .constant_pool
                .utf8(method.name_index)
                .map_err(E::Parse)?
                .units()
                == name
                && classfile
                    .constant_pool
                    .utf8(method.descriptor_index)
                    .map_err(E::Parse)?
                    .units()
                    == descriptor
            {
                return Ok(Some(MethodId {
                    owner: class,
                    index,
                }));
            }
        }
        Ok(None)
    }
    pub fn find_declared_field(
        &self,
        class: ClassId,
        name: &[u16],
        descriptor: &[u16],
    ) -> Result<Option<FieldId>, E> {
        let classfile = self.class(class)?;
        for (index, field) in classfile.fields.iter().enumerate() {
            if classfile
                .constant_pool
                .utf8(field.name_index)
                .map_err(E::Parse)?
                .units()
                == name
                && classfile
                    .constant_pool
                    .utf8(field.descriptor_index)
                    .map_err(E::Parse)?
                    .units()
                    == descriptor
            {
                return Ok(Some(FieldId {
                    owner: class,
                    index,
                }));
            }
        }
        Ok(None)
    }
    pub fn method(&self, method: MethodId) -> Result<&Member, E> {
        self.definition(method.owner, HandleKind::Method)?
            .class
            .methods
            .get(method.index)
            .ok_or(E::InvalidHandle {
                kind: HandleKind::Method,
                reason: "method index is outside its owner definition",
            })
    }
    pub fn field(&self, field: FieldId) -> Result<&Member, E> {
        self.definition(field.owner, HandleKind::Field)?
            .class
            .fields
            .get(field.index)
            .ok_or(E::InvalidHandle {
                kind: HandleKind::Field,
                reason: "field index is outside its owner definition",
            })
    }
}

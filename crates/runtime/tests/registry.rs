use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    DefinitionRegistry, DefinitionSource, HandleKind, InMemorySource, RegistryError,
    RegistryOptions, RegistryResource, SourceError,
};

fn name(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}

#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn add(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let mut bytes = vec![tag];
        bytes.extend(payload);
        self.0.push(bytes);
        self.0.len() as u16
    }

    fn utf8(&mut self, units: &[u16]) -> u16 {
        let mut bytes = Vec::new();
        for &unit in units {
            match unit {
                1..=0x7f => bytes.push(unit as u8),
                0..=0x7ff => bytes.extend([0xc0 | (unit >> 6) as u8, 0x80 | (unit & 0x3f) as u8]),
                _ => bytes.extend([
                    0xe0 | (unit >> 12) as u8,
                    0x80 | ((unit >> 6) & 0x3f) as u8,
                    0x80 | (unit & 0x3f) as u8,
                ]),
            }
        }
        let mut payload = words(&[bytes.len() as u16]);
        payload.extend(bytes);
        self.add(1, &payload)
    }

    fn text(&mut self, text: &str) -> u16 {
        self.utf8(&name(text))
    }
    fn class(&mut self, units: &[u16]) -> u16 {
        let index = self.utf8(units);
        self.add(7, &words(&[index]))
    }

    fn write(self, bytes: &mut Vec<u8>) {
        bytes.extend(words(&[(self.0.len() + 1) as u16]));
        bytes.extend(self.0.into_iter().flatten());
    }
}

fn definition(class_name: &[u16], members: bool) -> Vec<u8> {
    let mut pool = Pool::default();
    let this = pool.class(class_name);
    let superclass = pool.class(&name("java/lang/Object"));
    let field_name = pool.text("value");
    let int = pool.text("I");
    let long = pool.text("J");
    let method_name = pool.text("pick");
    let int_method = pool.text("(I)I");
    let long_method = pool.text("(J)J");
    let code_name = pool.text("Code");
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    pool.write(&mut bytes);
    bytes.extend(words(&[
        0x21,
        this,
        superclass,
        0,
        if members { 2 } else { 0 },
    ]));
    if members {
        bytes.extend(words(&[9, field_name, int, 0, 0x1a, field_name, long, 0]));
    }
    bytes.extend(words(&[if members { 2 } else { 0 }]));
    if members {
        for (flags, descriptor, width, code) in [
            (9, int_method, 1, [0x1a, 0xac]),
            (10, long_method, 2, [0x1e, 0xad]),
        ] {
            bytes.extend(words(&[flags, method_name, descriptor, 1, code_name]));
            bytes.extend(14_u32.to_be_bytes());
            bytes.extend(words(&[width, width]));
            bytes.extend(2_u32.to_be_bytes());
            bytes.extend(code);
            bytes.extend(words(&[0, 0]));
        }
    }
    bytes.extend(words(&[0]));
    bytes
}

fn module_definition() -> Vec<u8> {
    let mut pool = Pool::default();
    let this = pool.class(&name("module-info"));
    let module_name = pool.text("java.base");
    let module = pool.add(19, &words(&[module_name]));
    let attribute = pool.text("Module");
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    pool.write(&mut bytes);
    bytes.extend(words(&[0x8000, this, 0, 0, 0, 0, 1, attribute]));
    bytes.extend(16_u32.to_be_bytes());
    bytes.extend(words(&[module, 0, 0, 0, 0, 0, 0, 0]));
    bytes
}

fn registry(options: RegistryOptions) -> DefinitionRegistry {
    DefinitionRegistry::new(options).unwrap()
}

#[derive(Default)]
struct Probe {
    requests: Vec<(Vec<u16>, usize)>,
    answer: Option<Vec<u8>>,
    failure: Option<SourceError>,
}
impl DefinitionSource for Probe {
    fn read_definition(
        &mut self,
        name: &[u16],
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        self.requests.push((name.to_vec(), max_bytes));
        if let Some(error) = &self.failure {
            Err(error.clone())
        } else {
            Ok(self.answer.clone())
        }
    }
}

#[test]
fn in_memory_source_uses_exact_names_and_checks_limit_before_returning_owned_bytes() {
    let mut source = InMemorySource::new();
    assert!(source.is_empty());
    assert_eq!(source.insert(vec![0xd800], vec![1, 2, 3]), None);
    source.insert(vec![0xd801], vec![4]);
    assert_eq!(source.len(), 2);
    assert_eq!(
        source.read_definition(&[0xd800], 3).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(source.read_definition(&[0xd801], 1).unwrap(), Some(vec![4]));
    assert_eq!(source.read_definition(&[0xfffd], 0).unwrap(), None);
    let error = source.read_definition(&[0xd800], 2).unwrap_err();
    assert!(error.message.contains('3'));
    assert!(error.message.contains('2'));
    assert_eq!(error.to_string(), error.message);
    assert_eq!(
        source.read_definition(&[0xd800], 3).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(source.insert(vec![0xd800], vec![]), Some(vec![1, 2, 3]));
    assert_eq!(source.read_definition(&[0xd800], 0).unwrap(), Some(vec![]));
}

#[test]
fn identities_are_registry_and_defining_loader_scoped_and_metadata_stays_immutable() {
    let mut first = registry(RegistryOptions::default());
    assert_eq!(first.loader_count(), 0);
    let loader = first.create_loader().unwrap();
    let other_loader = first.create_loader().unwrap();
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, true);
    let first_id = first
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    let second_id = first
        .define(other_loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    assert_ne!(first_id, second_id);
    assert_eq!(first.defining_loader(first_id).unwrap(), loader);
    assert_eq!(first.defining_loader(second_id).unwrap(), other_loader);
    assert_eq!(
        first.lookup_class(loader, &class_name).unwrap(),
        Some(first_id)
    );
    assert_eq!(
        first.lookup_class(other_loader, &class_name).unwrap(),
        Some(second_id)
    );
    assert_eq!(
        first.class(first_id).unwrap().name().unwrap().units(),
        class_name
    );
    assert_eq!(first.definition_count(), 2);
    assert_eq!(first.total_definition_bytes(), 2 * bytes.len());
    assert_ne!(loader, other_loader);
    let mut second = registry(RegistryOptions::default());
    let foreign_loader = second.create_loader().unwrap();
    let foreign_id = second
        .define(foreign_loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    assert_ne!(loader, foreign_loader);
    assert_ne!(first_id, foreign_id);
}

#[test]
fn duplicate_definition_preserves_original_identity_model_and_accounting() {
    let mut registry = registry(RegistryOptions::default());
    let loader = registry.create_loader().unwrap();
    let class_name = name("example/Definition");
    let original = definition(&class_name, false);
    let replacement = definition(&class_name, true);
    let id = registry
        .define(loader, &class_name, &original, ParseOptions::default())
        .unwrap();
    assert!(
        matches!(registry.define(loader, &class_name, &replacement, ParseOptions::default()).unwrap_err(), RegistryError::DuplicateDefinition { existing } if existing == id)
    );
    assert_eq!(
        registry.lookup_class(loader, &class_name).unwrap(),
        Some(id)
    );
    assert!(registry.class(id).unwrap().methods.is_empty());
    assert!(registry.class(id).unwrap().fields.is_empty());
    assert_eq!(registry.definition_count(), 1);
    assert_eq!(registry.total_definition_bytes(), original.len());
}

#[test]
fn declared_member_lookup_uses_exact_name_descriptor_and_owner_handles() {
    let mut registry = registry(RegistryOptions::default());
    let loader = registry.create_loader().unwrap();
    let class_name = name("example/Definition");
    let id = registry
        .define(
            loader,
            &class_name,
            &definition(&class_name, true),
            ParseOptions::default(),
        )
        .unwrap();
    let int_method = registry
        .find_declared_method(id, &name("pick"), &name("(I)I"))
        .unwrap()
        .unwrap();
    let long_method = registry
        .find_declared_method(id, &name("pick"), &name("(J)J"))
        .unwrap()
        .unwrap();
    assert_ne!(int_method, long_method);
    assert_eq!(int_method.owner(), id);
    assert_eq!(long_method.owner(), id);
    assert_eq!(registry.method(int_method).unwrap().access_flags, 9);
    assert_eq!(registry.method(long_method).unwrap().access_flags, 10);
    assert!(
        registry
            .find_declared_method(id, &name("pick"), &name("()I"))
            .unwrap()
            .is_none()
    );
    assert!(
        registry
            .find_declared_method(id, &name("Pick"), &name("(I)I"))
            .unwrap()
            .is_none()
    );
    let int_field = registry
        .find_declared_field(id, &name("value"), &name("I"))
        .unwrap()
        .unwrap();
    let long_field = registry
        .find_declared_field(id, &name("value"), &name("J"))
        .unwrap()
        .unwrap();
    assert_ne!(int_field, long_field);
    assert_eq!(int_field.owner(), id);
    assert_eq!(registry.field(int_field).unwrap().access_flags, 9);
    assert_eq!(registry.field(long_field).unwrap().access_flags, 0x1a);
    assert!(
        registry
            .find_declared_field(id, &name("value"), &name("D"))
            .unwrap()
            .is_none()
    );
    assert!(
        registry
            .find_declared_field(id, &name("Value"), &name("I"))
            .unwrap()
            .is_none()
    );
    // This service does not search java/lang/Object or any unresolved parent.
    assert!(
        registry
            .find_declared_method(id, &name("hashCode"), &name("()I"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn all_handle_kinds_reject_foreign_registries_before_using_metadata_or_sources() {
    let mut owner = registry(RegistryOptions::default());
    let loader = owner.create_loader().unwrap();
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, true);
    let class = owner
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    let method = owner
        .find_declared_method(class, &name("pick"), &name("(I)I"))
        .unwrap()
        .unwrap();
    let field = owner
        .find_declared_field(class, &name("value"), &name("I"))
        .unwrap()
        .unwrap();
    let mut foreign = registry(RegistryOptions::default());
    let mut source = Probe::default();
    let errors = [
        foreign.lookup_class(loader, &class_name).unwrap_err(),
        foreign
            .define(loader, &class_name, &bytes, ParseOptions::default())
            .unwrap_err(),
        foreign
            .load_from_source(loader, &class_name, &mut source, ParseOptions::default())
            .unwrap_err(),
    ];
    for error in errors {
        assert!(matches!(
            error,
            RegistryError::InvalidHandle {
                kind: HandleKind::Loader,
                ..
            }
        ));
    }
    assert!(source.requests.is_empty());
    for error in [
        foreign.class(class).unwrap_err(),
        foreign.defining_loader(class).unwrap_err(),
        foreign
            .find_declared_method(class, &name("pick"), &name("(I)I"))
            .unwrap_err(),
        foreign
            .find_declared_field(class, &name("value"), &name("I"))
            .unwrap_err(),
    ] {
        assert!(matches!(
            error,
            RegistryError::InvalidHandle {
                kind: HandleKind::Class,
                ..
            }
        ));
    }
    assert!(matches!(
        foreign.method(method).unwrap_err(),
        RegistryError::InvalidHandle {
            kind: HandleKind::Method,
            ..
        }
    ));
    assert!(matches!(
        foreign.field(field).unwrap_err(),
        RegistryError::InvalidHandle {
            kind: HandleKind::Field,
            ..
        }
    ));
    assert_eq!(foreign.loader_count(), 0);
    assert_eq!(foreign.definition_count(), 0);
    assert_eq!(foreign.total_definition_bytes(), 0);
}

#[test]
fn names_are_exact_utf16_including_unpaired_surrogates_without_lossy_aliasing() {
    let mut registry = registry(RegistryOptions::default());
    let loader = registry.create_loader().unwrap();
    let names = [
        vec![0x78, 47, 0xd800],
        vec![0x78, 47, 0xd801],
        vec![0x78, 47, 0xfffd],
        name("x/Case"),
        name("x/case"),
    ];
    let mut ids = Vec::new();
    for class_name in &names {
        let id = registry
            .define(
                loader,
                class_name,
                &definition(class_name, false),
                ParseOptions::default(),
            )
            .unwrap();
        assert!(!ids.contains(&id));
        ids.push(id);
    }
    for (class_name, id) in names.iter().zip(ids) {
        assert_eq!(registry.lookup_class(loader, class_name).unwrap(), Some(id));
        assert_eq!(
            registry.class(id).unwrap().name().unwrap().units(),
            class_name
        );
    }
    assert_eq!(
        registry.lookup_class(loader, &name("x/Missing")).unwrap(),
        None
    );
}

#[test]
fn invalid_names_wrong_declarations_parser_failures_and_modules_publish_nothing() {
    let mut registry = registry(RegistryOptions::default());
    let loader = registry.create_loader().unwrap();
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, false);
    for invalid in ["", "a.b", "/a", "a/", "a//b", "a;b"] {
        assert!(matches!(
            registry
                .define(loader, &name(invalid), &bytes, ParseOptions::default())
                .unwrap_err(),
            RegistryError::InvalidName(_)
        ));
    }
    assert!(matches!(
        registry
            .define(loader, &name("[I"), &bytes, ParseOptions::default())
            .unwrap_err(),
        RegistryError::UnsupportedDefinition(_)
    ));
    let error = registry
        .define(loader, &name("other/Name"), &bytes, ParseOptions::default())
        .unwrap_err();
    assert!(
        matches!(error, RegistryError::NameMismatch { expected, actual } if expected.units() == name("other/Name") && actual.units() == class_name)
    );
    assert!(matches!(
        registry
            .define(loader, &class_name, &[0xca, 0xfe], ParseOptions::default())
            .unwrap_err(),
        RegistryError::Parse(_)
    ));
    assert!(matches!(
        registry
            .define(
                loader,
                &name("module-info"),
                &module_definition(),
                ParseOptions::default()
            )
            .unwrap_err(),
        RegistryError::UnsupportedDefinition(_)
    ));
    assert_eq!(registry.definition_count(), 0);
    assert_eq!(registry.total_definition_bytes(), 0);
    assert_eq!(registry.lookup_class(loader, &class_name).unwrap(), None);
    let id = registry
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    assert_eq!(
        registry.lookup_class(loader, &class_name).unwrap(),
        Some(id)
    );
    assert_eq!(registry.definition_count(), 1);
    assert_eq!(registry.total_definition_bytes(), bytes.len());
}

#[test]
fn successful_source_loads_are_cached_before_reads_or_revalidation() {
    let mut registry = registry(RegistryOptions {
        max_definitions: 1,
        ..RegistryOptions::default()
    });
    let loader = registry.create_loader().unwrap();
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, true);
    let mut source = Probe {
        answer: Some(bytes.clone()),
        ..Probe::default()
    };
    let id = registry
        .load_from_source(loader, &class_name, &mut source, ParseOptions::default())
        .unwrap();
    assert_eq!(source.requests.len(), 1);
    source.answer = None;
    source.failure = Some(SourceError::new("cached lookup must not read this source"));
    let restrictive_parser = ParseOptions {
        max_class_bytes: 0,
        ..ParseOptions::default()
    };
    assert_eq!(
        registry
            .load_from_source(loader, &class_name, &mut source, restrictive_parser)
            .unwrap(),
        id
    );
    assert_eq!(source.requests.len(), 1);
    assert_eq!(registry.definition_count(), 1);
    assert_eq!(registry.total_definition_bytes(), bytes.len());
    assert!(
        matches!(registry.define(loader, &class_name, &bytes, ParseOptions::default()).unwrap_err(), RegistryError::DuplicateDefinition { existing } if existing == id)
    );
}

#[test]
fn absence_source_failure_parser_failure_and_name_failure_stay_distinct_and_retryable() {
    let mut registry = registry(RegistryOptions::default());
    let loader = registry.create_loader().unwrap();
    let class_name = name("example/Definition");
    let mut source = Probe::default();
    assert!(
        matches!(registry.load_from_source(loader, &class_name, &mut source, ParseOptions::default()).unwrap_err(), RegistryError::NotFound { name: requested } if requested.units() == class_name)
    );
    source.failure = Some(SourceError::new("fixture transport refused the request"));
    assert!(
        matches!(registry.load_from_source(loader, &class_name, &mut source, ParseOptions::default()).unwrap_err(), RegistryError::Source(error) if error.message.contains("transport"))
    );
    source.failure = None;
    source.answer = Some(vec![0]);
    assert!(matches!(
        registry
            .load_from_source(loader, &class_name, &mut source, ParseOptions::default())
            .unwrap_err(),
        RegistryError::Parse(_)
    ));
    source.answer = Some(definition(&name("wrong/Definition"), false));
    assert!(matches!(
        registry
            .load_from_source(loader, &class_name, &mut source, ParseOptions::default())
            .unwrap_err(),
        RegistryError::NameMismatch { .. }
    ));
    assert_eq!(registry.definition_count(), 0);
    assert_eq!(registry.total_definition_bytes(), 0);
    source.answer = Some(definition(&class_name, false));
    let id = registry
        .load_from_source(loader, &class_name, &mut source, ParseOptions::default())
        .unwrap();
    assert_eq!(source.requests.len(), 5);
    assert_eq!(
        registry.lookup_class(loader, &class_name).unwrap(),
        Some(id)
    );
}

#[test]
fn policy_defaults_and_zero_and_exact_loader_and_definition_limits_are_explicit() {
    let defaults = RegistryOptions::default();
    assert_eq!(defaults.max_loaders, 256);
    assert_eq!(defaults.max_definitions, 1024);
    assert_eq!(defaults.max_definition_bytes, 16 * 1024 * 1024);
    assert_eq!(defaults.max_total_definition_bytes, 64 * 1024 * 1024);
    let mut none = registry(RegistryOptions {
        max_loaders: 0,
        ..defaults
    });
    assert!(matches!(
        none.create_loader().unwrap_err(),
        RegistryError::ResourceLimit {
            resource: RegistryResource::Loaders,
            limit: 0,
            required: 1
        }
    ));
    assert_eq!(none.loader_count(), 0);
    let mut one = registry(RegistryOptions {
        max_loaders: 1,
        max_definitions: 1,
        ..defaults
    });
    let loader = one.create_loader().unwrap();
    assert!(matches!(
        one.create_loader().unwrap_err(),
        RegistryError::ResourceLimit {
            resource: RegistryResource::Loaders,
            limit: 1,
            required: 2
        }
    ));
    assert_eq!(one.loader_count(), 1);
    let class_name = name("example/First");
    let bytes = definition(&class_name, false);
    let id = one
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    let other = name("example/Other");
    assert!(matches!(
        one.define(
            loader,
            &other,
            &definition(&other, false),
            ParseOptions::default()
        )
        .unwrap_err(),
        RegistryError::ResourceLimit {
            resource: RegistryResource::Definitions,
            limit: 1,
            required: 2
        }
    ));
    assert_eq!(one.definition_count(), 1);
    assert_eq!(one.total_definition_bytes(), bytes.len());
    assert_eq!(one.lookup_class(loader, &class_name).unwrap(), Some(id));
    let mut zero = registry(RegistryOptions {
        max_definitions: 0,
        ..defaults
    });
    let loader = zero.create_loader().unwrap();
    assert!(matches!(
        zero.define(loader, &class_name, &bytes, ParseOptions::default())
            .unwrap_err(),
        RegistryError::ResourceLimit {
            resource: RegistryResource::Definitions,
            limit: 0,
            required: 1
        }
    ));
    assert_eq!(zero.definition_count(), 0);
}

#[test]
fn individual_and_aggregate_byte_boundaries_are_atomic_and_account_input_volume() {
    let class_name = name("example/A");
    let bytes = definition(&class_name, false);
    for limit in [0, bytes.len() - 1] {
        let mut registry = registry(RegistryOptions {
            max_definition_bytes: limit,
            ..RegistryOptions::default()
        });
        let loader = registry.create_loader().unwrap();
        assert!(
            matches!(registry.define(loader, &class_name, &bytes, ParseOptions::default()).unwrap_err(), RegistryError::ResourceLimit { resource: RegistryResource::DefinitionBytes, limit: actual_limit, required } if actual_limit == limit && required == bytes.len())
        );
        assert_eq!(registry.definition_count(), 0);
        assert_eq!(registry.total_definition_bytes(), 0);
    }
    let mut exact = registry(RegistryOptions {
        max_definition_bytes: bytes.len(),
        max_total_definition_bytes: bytes.len(),
        ..RegistryOptions::default()
    });
    let loader = exact.create_loader().unwrap();
    exact
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    assert_eq!(exact.total_definition_bytes(), bytes.len());
    let other = name("example/B");
    let other_bytes = definition(&other, false);
    assert_eq!(bytes.len(), other_bytes.len());
    assert!(
        matches!(exact.define(loader, &other, &other_bytes, ParseOptions::default()).unwrap_err(), RegistryError::ResourceLimit { resource: RegistryResource::TotalDefinitionBytes, limit, required } if limit == bytes.len() && required == bytes.len() + other_bytes.len())
    );
    assert_eq!(exact.definition_count(), 1);
    assert_eq!(exact.total_definition_bytes(), bytes.len());
    let mut double = registry(RegistryOptions {
        max_total_definition_bytes: bytes.len() + other_bytes.len(),
        ..RegistryOptions::default()
    });
    let loader = double.create_loader().unwrap();
    double
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    double
        .define(loader, &other, &other_bytes, ParseOptions::default())
        .unwrap();
    assert_eq!(double.definition_count(), 2);
    assert_eq!(
        double.total_definition_bytes(),
        bytes.len() + other_bytes.len()
    );
}

#[test]
fn parser_byte_policy_failure_does_not_consume_registry_capacity() {
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, false);
    let mut registry = registry(RegistryOptions {
        max_definitions: 1,
        max_total_definition_bytes: bytes.len(),
        ..RegistryOptions::default()
    });
    let loader = registry.create_loader().unwrap();
    let parse_options = ParseOptions {
        max_class_bytes: bytes.len() - 1,
        ..ParseOptions::default()
    };
    assert!(matches!(
        registry
            .define(loader, &class_name, &bytes, parse_options)
            .unwrap_err(),
        RegistryError::Parse(_)
    ));
    assert_eq!(registry.definition_count(), 0);
    assert_eq!(registry.total_definition_bytes(), 0);
    registry
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    assert_eq!(registry.definition_count(), 1);
}

#[test]
fn source_read_allowance_is_the_minimum_of_all_three_byte_policies() {
    let class_name = name("example/Definition");
    for (per_definition, parser, total, expected) in [
        (1000, 100, 1000, 100),
        (50, 100, 1000, 50),
        (1000, 100, 40, 40),
        (0, 100, 1000, 0),
        (1000, 0, 1000, 0),
        (1000, 100, 0, 0),
        (usize::MAX, usize::MAX, usize::MAX, usize::MAX),
    ] {
        let mut registry = registry(RegistryOptions {
            max_definition_bytes: per_definition,
            max_total_definition_bytes: total,
            ..RegistryOptions::default()
        });
        let loader = registry.create_loader().unwrap();
        let mut source = Probe::default();
        let options = ParseOptions {
            max_class_bytes: parser,
            ..ParseOptions::default()
        };
        assert!(matches!(
            registry
                .load_from_source(loader, &class_name, &mut source, options)
                .unwrap_err(),
            RegistryError::NotFound { .. }
        ));
        assert_eq!(source.requests, [(class_name.clone(), expected)]);
    }
    let bytes = definition(&class_name, false);
    let mut registry = registry(RegistryOptions {
        max_definition_bytes: 1000,
        max_total_definition_bytes: bytes.len() + 7,
        ..RegistryOptions::default()
    });
    let loader = registry.create_loader().unwrap();
    registry
        .define(loader, &class_name, &bytes, ParseOptions::default())
        .unwrap();
    let missing = name("example/Missing");
    let mut source = Probe::default();
    registry
        .load_from_source(loader, &missing, &mut source, ParseOptions::default())
        .unwrap_err();
    assert_eq!(source.requests, [(missing, 7)]);
}

#[test]
fn violating_sources_are_rechecked_and_compliant_source_failures_remain_distinct() {
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, false);
    let mut registry = registry(RegistryOptions {
        max_definition_bytes: bytes.len() - 1,
        ..RegistryOptions::default()
    });
    let loader = registry.create_loader().unwrap();
    let mut violating = Probe {
        answer: Some(bytes.clone()),
        ..Probe::default()
    };
    assert!(
        matches!(registry.load_from_source(loader, &class_name, &mut violating, ParseOptions::default()).unwrap_err(), RegistryError::ResourceLimit { resource: RegistryResource::SourceReadBytes, limit, required } if limit == bytes.len() - 1 && required == bytes.len())
    );
    assert_eq!(registry.definition_count(), 0);
    assert_eq!(registry.total_definition_bytes(), 0);
    let mut compliant = InMemorySource::new();
    compliant.insert(class_name.clone(), bytes);
    assert!(matches!(
        registry
            .load_from_source(loader, &class_name, &mut compliant, ParseOptions::default())
            .unwrap_err(),
        RegistryError::Source(_)
    ));
    assert_eq!(registry.definition_count(), 0);
}

#[test]
fn maximum_policy_values_are_limits_not_eager_allocation_sizes() {
    let mut registry = registry(RegistryOptions {
        max_loaders: usize::MAX,
        max_definitions: usize::MAX,
        max_definition_bytes: usize::MAX,
        max_total_definition_bytes: usize::MAX,
    });
    let loader = registry.create_loader().unwrap();
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, false);
    registry
        .define(
            loader,
            &class_name,
            &bytes,
            ParseOptions {
                max_class_bytes: usize::MAX,
                ..ParseOptions::default()
            },
        )
        .unwrap();
    assert_eq!(registry.loader_count(), 1);
    assert_eq!(registry.definition_count(), 1);
    assert_eq!(registry.total_definition_bytes(), bytes.len());
}

#[test]
fn name_length_boundary_is_checked_before_source_reads_and_supports_largest_ascii_name() {
    let mut registry = registry(RegistryOptions::default());
    let loader = registry.create_loader().unwrap();
    let largest = vec![65_u16; usize::from(u16::MAX)];
    let bytes = definition(&largest, false);
    let id = registry
        .define(loader, &largest, &bytes, ParseOptions::default())
        .unwrap();
    assert_eq!(registry.lookup_class(loader, &largest).unwrap(), Some(id));
    let too_large = vec![65_u16; usize::from(u16::MAX) + 1];
    let mut source = Probe::default();
    assert!(matches!(
        registry
            .load_from_source(loader, &too_large, &mut source, ParseOptions::default())
            .unwrap_err(),
        RegistryError::InvalidName(_)
    ));
    assert!(source.requests.is_empty());
    assert_eq!(registry.definition_count(), 1);
    assert_eq!(registry.total_definition_bytes(), bytes.len());
}

#[test]
fn source_cache_is_scoped_to_the_explicit_defining_loader() {
    let mut registry = registry(RegistryOptions::default());
    let first_loader = registry.create_loader().unwrap();
    let second_loader = registry.create_loader().unwrap();
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, false);
    let mut source = Probe {
        answer: Some(bytes),
        ..Probe::default()
    };
    let first = registry
        .load_from_source(
            first_loader,
            &class_name,
            &mut source,
            ParseOptions::default(),
        )
        .unwrap();
    let second = registry
        .load_from_source(
            second_loader,
            &class_name,
            &mut source,
            ParseOptions::default(),
        )
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(source.requests.len(), 2);
    assert_eq!(
        registry
            .load_from_source(
                first_loader,
                &class_name,
                &mut source,
                ParseOptions::default()
            )
            .unwrap(),
        first
    );
    assert_eq!(source.requests.len(), 2);
}

#[test]
fn source_contract_violations_are_rechecked_against_parser_and_remaining_aggregate_allowances() {
    let class_name = name("example/Definition");
    let bytes = definition(&class_name, false);
    for aggregate_is_minimum in [false, true] {
        let mut registry = registry(RegistryOptions {
            max_total_definition_bytes: if aggregate_is_minimum {
                bytes.len() - 1
            } else {
                usize::MAX
            },
            ..RegistryOptions::default()
        });
        let loader = registry.create_loader().unwrap();
        let mut source = Probe {
            answer: Some(bytes.clone()),
            ..Probe::default()
        };
        let parser = ParseOptions {
            max_class_bytes: if aggregate_is_minimum {
                usize::MAX
            } else {
                bytes.len() - 1
            },
            ..ParseOptions::default()
        };
        assert!(
            matches!(registry.load_from_source(loader, &class_name, &mut source, parser).unwrap_err(), RegistryError::ResourceLimit { resource: RegistryResource::SourceReadBytes, limit, required } if limit == bytes.len() - 1 && required == bytes.len())
        );
        assert_eq!(source.requests[0].1, bytes.len() - 1);
        assert_eq!(registry.definition_count(), 0);
        assert_eq!(registry.total_definition_bytes(), 0);
    }
}

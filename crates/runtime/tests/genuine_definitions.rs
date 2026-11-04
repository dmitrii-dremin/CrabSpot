use crabspot_classfile::{ParseOptions, parse};
use crabspot_runtime::{DefinitionRegistry, DefinitionSource, RegistryOptions, SourceError};
use std::path::{Path, PathBuf};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/Fixture.class");
const CALLS: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/CallCases.class");
const MODULE: &[u8] = include_bytes!("../../../tests/fixtures/classes/module-info.class");

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn class_files(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            class_files(&entry.path(), files);
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "class")
        {
            files.push(entry.path());
        }
    }
}

#[test]
fn registers_all_131_genuine_ordinary_definitions_without_claiming_linkage() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let mut files = Vec::new();
    class_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/classes"),
        &mut files,
    );
    files.sort();
    let mut expected_bytes = 0;
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let parsed = parse(&bytes).unwrap();
        let expected = parsed.name().unwrap().units();
        if parsed.access_flags & 0x8000 != 0 {
            continue;
        }
        let id = registry
            .define(loader, expected, &bytes, ParseOptions::default())
            .unwrap();
        assert_eq!(registry.class(id).unwrap(), &parsed);
        assert_eq!(registry.defining_loader(id).unwrap(), loader);
        assert_eq!(registry.lookup_class(loader, expected).unwrap(), Some(id));
        expected_bytes += bytes.len();
    }
    assert_eq!(registry.definition_count(), 131);
    assert_eq!(registry.total_definition_bytes(), expected_bytes);
    // The real module descriptor is valid metadata, but not an ordinary definition.
    assert!(
        registry
            .define(
                loader,
                &units("module-info"),
                MODULE,
                ParseOptions::default()
            )
            .is_err()
    );
    assert_eq!(registry.definition_count(), 131);
    assert_eq!(registry.total_definition_bytes(), expected_bytes);
}

#[test]
fn genuine_definitions_keep_loader_identity_and_declared_member_ownership() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let first_loader = registry.create_loader().unwrap();
    let second_loader = registry.create_loader().unwrap();
    let first = registry
        .define(
            first_loader,
            &units("fixtures/Fixture"),
            FIXTURE,
            ParseOptions::default(),
        )
        .unwrap();
    let second = registry
        .define(
            second_loader,
            &units("fixtures/Fixture"),
            FIXTURE,
            ParseOptions::default(),
        )
        .unwrap();
    assert_ne!(first, second);
    assert!(
        registry
            .define(
                first_loader,
                &units("fixtures/Fixture"),
                FIXTURE,
                ParseOptions::default()
            )
            .is_err()
    );
    let field = registry
        .find_declared_field(first, &units("WIDE"), &units("J"))
        .unwrap()
        .unwrap();
    assert_eq!(field.owner(), first);
    assert_eq!(registry.field(field).unwrap().access_flags & 0x18, 0x18);
    let calls = registry
        .define(
            first_loader,
            &units("fixtures/CallCases"),
            CALLS,
            ParseOptions::default(),
        )
        .unwrap();
    let int_method = registry
        .find_declared_method(calls, &units("overloaded"), &units("(I)I"))
        .unwrap()
        .unwrap();
    let long_method = registry
        .find_declared_method(calls, &units("overloaded"), &units("(J)J"))
        .unwrap()
        .unwrap();
    assert_ne!(int_method, long_method);
    assert_eq!(int_method.owner(), calls);
    assert_eq!(
        registry.method(int_method).unwrap().access_flags & 0x0a,
        0x0a
    );
    assert!(
        registry
            .find_declared_method(first, &units("hashCode"), &units("()I"))
            .unwrap()
            .is_none()
    );
    let foreign = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    assert!(foreign.class(first).is_err());
    assert!(foreign.method(int_method).is_err());
    assert!(foreign.field(field).is_err());
}

struct CountingSource {
    calls: usize,
}
impl DefinitionSource for CountingSource {
    fn read_definition(
        &mut self,
        name: &[u16],
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        self.calls += 1;
        if name != units("fixtures/Fixture") {
            return Ok(None);
        }
        if FIXTURE.len() > max_bytes {
            return Err(SourceError::new("fixture exceeds source allowance"));
        }
        Ok(Some(FIXTURE.to_vec()))
    }
}

#[test]
fn loader_scoped_source_caching_preserves_real_metadata_and_failure_atomicity() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let other_loader = registry.create_loader().unwrap();
    let mut source = CountingSource { calls: 0 };
    let name = units("fixtures/Fixture");
    let first = registry
        .load_from_source(loader, &name, &mut source, ParseOptions::default())
        .unwrap();
    let cached = registry
        .load_from_source(
            loader,
            &name,
            &mut source,
            ParseOptions {
                max_class_bytes: 0,
                ..ParseOptions::default()
            },
        )
        .unwrap();
    assert_eq!(first, cached);
    assert_eq!(source.calls, 1);
    let other = registry
        .load_from_source(other_loader, &name, &mut source, ParseOptions::default())
        .unwrap();
    assert_ne!(first, other);
    assert_eq!(source.calls, 2);
    let before = registry.total_definition_bytes();
    assert!(
        registry
            .define(
                loader,
                &units("fixtures/WrongName"),
                FIXTURE,
                ParseOptions::default()
            )
            .is_err()
    );
    assert!(
        registry
            .define(
                loader,
                &units("fixtures/Broken"),
                &FIXTURE[..20],
                ParseOptions::default()
            )
            .is_err()
    );
    assert_eq!(registry.definition_count(), 2);
    assert_eq!(registry.total_definition_bytes(), before);
    assert_eq!(registry.class(first).unwrap().name().unwrap().units(), name);
}

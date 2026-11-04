//! Validate every method, including driver Code outside the interpreter's executable subset.
use crabspot_classfile::{AttributeBody, parse};
use crabspot_verifier::{CodeValidationOptions, validate_method_code};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn class_files(directory: &Path) -> Vec<PathBuf> {
    let mut pending = vec![directory.to_owned()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else if entry
                .path()
                .extension()
                .is_some_and(|value| value == "class")
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    files
}

#[test]
fn validates_every_genuine_method_without_mutation_or_execution() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/classes");
    let files = class_files(&root);
    assert_eq!(files.len(), 168);
    let (mut bodies, mut bodyless, mut instructions) = (0, 0, 0);
    let mut opcodes = BTreeSet::new();
    for path in files {
        let class = parse(&std::fs::read(&path).unwrap()).unwrap();
        let before = class.clone();
        for (index, method) in class.methods.iter().enumerate() {
            let result = validate_method_code(&class, index, CodeValidationOptions::default())
                .unwrap_or_else(|error| panic!("{} method {index}: {error}", path.display()));
            assert!(std::ptr::eq(result.class(), &class));
            assert!(std::ptr::eq(result.method(), method));
            assert_eq!(result.method_index(), index);
            let code = method.attributes.iter().find_map(|attribute| {
                if let AttributeBody::Code(code) = &attribute.body {
                    Some(code)
                } else {
                    None
                }
            });
            match (code, result.code(), result.decoded()) {
                (Some(original), Some(checked), Some(decoded)) => {
                    bodies += 1;
                    assert!(std::ptr::eq(original, checked));
                    assert_eq!(decoded.code_len() as usize, original.code.len());
                    assert_eq!(result.usage().code_bytes as usize, original.code.len());
                    instructions += decoded.instructions().len();
                    opcodes.extend(
                        decoded
                            .instructions()
                            .iter()
                            .map(|instruction| instruction.opcode),
                    );
                }
                (None, None, None) => {
                    bodyless += 1;
                    assert_ne!(method.access_flags & (0x100 | 0x400), 0);
                    assert_eq!(result.usage().code_bytes, 0);
                }
                _ => panic!("inconsistent body result"),
            }
            let repeated =
                validate_method_code(&class, index, CodeValidationOptions::default()).unwrap();
            assert_eq!(result.usage(), repeated.usage());
            assert_eq!(result.decoded(), repeated.decoded());
        }
        assert_eq!(class, before);
    }
    println!(
        "Genuine corpus: 168 classes, {bodies} Code bodies, {bodyless} bodyless declarations, {instructions} instructions, {} distinct opcodes",
        opcodes.len()
    );
    assert_eq!(
        (bodies, bodyless, instructions, opcodes.len()),
        (890, 31, 25_236, 173)
    );
    // invokedynamic appears in genuine driver/string-concatenation Code even though
    // execution is unsupported. Static validation must not use interpreter gates.
    assert!(opcodes.contains(&0xba));
}

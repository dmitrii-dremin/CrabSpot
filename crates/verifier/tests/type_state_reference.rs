//! Compare selected method proofs with the first observed JDK stage, including
//! examples where checking succeeds and a later invocation throws.
#[path = "corpus/environment.rs"]
mod corpus;
#[path = "corpus/files.rs"]
mod files;

use crabspot_classfile::{Constant, parse};
use crabspot_verifier::{TypeVerificationErrorKind, verify_method_types};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[test]
fn whole_class_proofs_distinguish_jdk_definition_method_lookup_and_dependency_failures() {
    use crabspot_classfile::{ErrorKind, ParseOptions};
    use crabspot_runtime::{HierarchyError, RegistryError};
    use crabspot_verifier::{
        ClassTypeVerificationErrorKind as E, ClassTypeVerificationOptions, CodeValidationLocation,
        verify_class_types,
    };

    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let reference = std::fs::read_to_string(fixtures.join("reference/class-proof.tsv")).unwrap();
    let input = |name: &str| {
        let suffix = match name {
            "NativeWithBody" | "EmptyPreview" => ".class.bin",
            _ => ".class",
        };
        let path = fixtures.join("class-proof").join(format!("{name}{suffix}"));
        (format!("classprobe/{name}"), std::fs::read(path).unwrap())
    };
    let mut seen = BTreeSet::new();
    for line in reference.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        let [name, policy, stage, outcome] = columns[..] else {
            panic!("invalid stage row: {line}")
        };
        assert!(seen.insert((name, policy)));
        let mut targets = vec![input(name)];
        match name {
            "ChildValid" => targets.push(input("BadParent")),
            "ChildOfFinal" => targets.push(input("FinalParent")),
            _ => {}
        }
        // Admit the preview fixture in order to exercise the separate verifier policy.
        let admitted = corpus::Corpus::with_options(
            &fixtures.join("type-state-facts"),
            &targets,
            ParseOptions {
                allow_preview: true,
                ..Default::default()
            },
        );
        let corpus = match admitted {
            Ok(corpus) => corpus,
            Err(HierarchyError::FinalSuperclass { .. }) => {
                assert_eq!(
                    (name, policy, stage, outcome),
                    (
                        "ChildOfFinal",
                        "standard",
                        "define",
                        "IncompatibleClassChangeError"
                    )
                );
                continue;
            }
            Err(HierarchyError::Registry(RegistryError::Parse(e))) => {
                assert_eq!(
                    (name, policy, stage, outcome),
                    ("NativeWithBody", "standard", "define", "ClassFormatError")
                );
                assert!(
                    matches!(e.kind, ErrorKind::InvalidStructure(reason) if reason.contains("abstract/native"))
                );
                continue;
            }
            Err(e) => panic!("{line}: {e}"),
        };
        let environment = corpus.environment();
        let id = corpus.targets[0].1;
        let mut options = ClassTypeVerificationOptions::default();
        options.method.code.allow_preview = match policy {
            "standard" => false,
            "preview" if name == "EmptyPreview" => true,
            _ => panic!("unexpected preview policy: {line}"),
        };
        let result = verify_class_types(&environment, id, options);
        match (name, stage, outcome) {
            ("EmptyHistory", "methods", "0") => {
                let e = result.unwrap_err();
                assert!(matches!(e.kind, E::UnsupportedHistory { major: 50 }));
                assert_eq!((e.method_index, e.usage), (None, Default::default()));
            }
            ("EmptyPreview", "define", "UnsupportedClassVersionError") => {
                let e = result.unwrap_err();
                assert!(matches!(e.kind, E::PreviewDisabled));
                assert_eq!((e.method_index, e.usage), (None, Default::default()));
            }
            ("ChildValid", "methods", "VerifyError") => {
                // Reflection also checks the parent. The two own-class proofs explain the split.
                assert_eq!(result.unwrap().usage().code_bodies, 1);
                let parent =
                    verify_class_types(&environment, corpus.targets[1].1, options).unwrap_err();
                assert_eq!(parent.method_index, Some(0));
                assert!(matches!(parent.kind, E::Method(_)));
            }
            (
                "BadParent" | "BadThenGood" | "GoodThenBad" | "InvalidSecondAfterNative",
                "methods",
                "VerifyError",
            ) => {
                let e = result.unwrap_err();
                let index = usize::from(matches!(name, "GoodThenBad" | "InvalidSecondAfterNative"));
                assert_eq!(e.method_index, Some(index));
                let E::Method(source) = e.kind else {
                    panic!("expected method diagnostic: {line}")
                };
                assert_eq!(source.location, CodeValidationLocation::Instruction(1));
                assert!(matches!(
                    source.kind,
                    TypeVerificationErrorKind::TypeRule { .. }
                ));
                if matches!(name, "GoodThenBad" | "BadThenGood") {
                    verify_method_types(&environment, id, 1 - index, options.method).unwrap();
                }
            }
            ("EmptyModern" | "EmptyPreview" | "EmptyInterface" | "FinalParent", "methods", "0")
            | ("AbstractOnly" | "NativeOnly", "methods", "1")
            | ("AbstractAndNative", "methods", "2") => {
                let proof = result.unwrap();
                assert_eq!(
                    proof.usage().methods_started,
                    outcome.parse::<u64>().unwrap()
                );
                assert_eq!(proof.usage().bodyless, proof.usage().methods_started);
                assert_eq!(proof.usage().code_bodies, 0);
            }
            _ => panic!("unexpected class proof outcome: {line}"),
        }
    }
    assert_eq!(seen.len(), 16);
}

fn reachable_inputs(root: &str, files: &BTreeMap<String, Vec<u8>>) -> Vec<(String, Vec<u8>)> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let class = parse(&files[&name]).unwrap();
        for entry in class.constant_pool.entries() {
            if let Constant::Class { name_index } = entry {
                let name = class
                    .constant_pool
                    .utf8(*name_index)
                    .unwrap()
                    .to_string_lossy();
                if files.contains_key(&name) {
                    pending.push(name);
                }
            }
        }
    }
    seen.into_iter()
        .map(|name| {
            let bytes = files[&name].clone();
            (name, bytes)
        })
        .collect()
}

#[test]
fn selected_methods_match_independent_jdk_stages_in_fresh_loader_graphs() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let mut inputs = BTreeMap::new();
    for path in files::class_files(&fixtures.join("type-state")) {
        let bytes = std::fs::read(path).unwrap();
        let name = parse(&bytes).unwrap().name().unwrap().to_string_lossy();
        assert!(inputs.insert(name, bytes).is_none());
    }
    assert_eq!(inputs.len(), 69);
    let reference = std::fs::read_to_string(fixtures.join("reference/type-state.tsv")).unwrap();
    let mut seen = BTreeSet::new();
    let mut counts = [0; 3];
    for line in reference.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        let [name, method, descriptor, stage, outcome] = columns[..] else {
            panic!("invalid stage row: {line}");
        };
        assert!(seen.insert(name));
        let targets = reachable_inputs(name, &inputs);
        let corpus = corpus::Corpus::new(&fixtures.join("type-state-facts"), &targets);
        let environment = corpus.environment();
        let (_, id) = corpus
            .targets
            .iter()
            .find(|(class, _)| class == name)
            .unwrap();
        let class = corpus.registry.class(*id).unwrap();
        let index = class
            .methods
            .iter()
            .position(|member| {
                class
                    .constant_pool
                    .utf8(member.name_index)
                    .unwrap()
                    .equals_ascii(method)
                    && class
                        .constant_pool
                        .utf8(member.descriptor_index)
                        .unwrap()
                        .equals_ascii(descriptor)
            })
            .unwrap_or_else(|| panic!("missing {name}.{method}{descriptor}"));
        let result = verify_method_types(&environment, *id, index, Default::default());
        let aggregate =
            crabspot_verifier::verify_class_types(&environment, *id, Default::default());
        match (&result, aggregate) {
            (Ok(_), Ok(class_proof)) => {
                assert_eq!(
                    class_proof.usage().methods_started,
                    class.methods.len() as u64
                );
            }
            (Err(selected), Err(class_error)) => {
                assert_eq!(class_error.method_index, Some(index), "{line}");
                let crabspot_verifier::ClassTypeVerificationErrorKind::Method(nested) =
                    class_error.kind
                else {
                    panic!("expected matching method failure for {line}");
                };
                assert_eq!(*selected, nested, "{line}");
            }
            (selected, class_result) => {
                panic!("mismatch for {line}: {selected:?}, {class_result:?}")
            }
        }
        match (stage, outcome) {
            (
                "invoke",
                "void"
                | "null"
                | "I:1"
                | "I:2"
                | "I:7"
                | "NullPointerException"
                | "ArrayStoreException"
                | "IncompatibleClassChangeError"
                | "NoClassDefFoundError",
            ) => {
                result.unwrap_or_else(|error| panic!("{line}: {error}"));
                counts[0] += 1;
            }
            ("lookup", "VerifyError") => {
                assert!(
                    matches!(
                        result.unwrap_err().kind,
                        TypeVerificationErrorKind::TypeRule { .. }
                    ),
                    "{line}"
                );
                counts[1] += 1;
            }
            ("define", "IncompatibleClassChangeError") => {
                assert!(
                    matches!(
                        result.unwrap_err().kind,
                        TypeVerificationErrorKind::FinalOverride { .. }
                    ),
                    "{line}"
                );
                counts[2] += 1;
            }
            _ => panic!("unexpected JDK result: {line}"),
        }
    }
    assert_eq!(counts, [29, 23, 5]);
}

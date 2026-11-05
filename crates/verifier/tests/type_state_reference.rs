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

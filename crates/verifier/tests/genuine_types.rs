//! Exercise every genuine declaration with loader-scoped facts from a reference JDK.
#[path = "corpus/environment.rs"]
mod corpus;
#[path = "corpus/files.rs"]
mod files;
use crabspot_verifier::{TypeVerificationUsage, verify_class_types, verify_method_types};
use std::path::Path;

#[test]
fn projected_headers_and_ordered_members_match_the_captured_jdk_records() {
    use crabspot_classfile::{AttributeBody, JavaString, parse};
    use std::fmt::Write;

    fn units(value: &JavaString) -> String {
        let mut text = String::new();
        for unit in value.units() {
            write!(text, "{unit:04x}").unwrap();
        }
        text
    }

    let facts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/type-state-facts");
    let mut actual = Vec::new();
    for path in files::class_files(&facts.join("classes")) {
        let class = parse(&std::fs::read(path).unwrap()).unwrap();
        let name = units(class.name().unwrap());
        let parent = if class.super_class == 0 {
            String::new()
        } else {
            units(class.constant_pool.class_name(class.super_class).unwrap())
        };
        let mut rows = vec![format!(
            "C\t{name}\t{}\t{}\t{}\t{parent}",
            class.version.major, class.version.minor, class.access_flags
        )];
        for interface in class.interfaces {
            rows.push(format!(
                "I\t{}",
                units(class.constant_pool.class_name(interface).unwrap())
            ));
        }
        for (tag, members) in [("F", class.fields), ("M", class.methods)] {
            for member in members {
                let mut row = format!(
                    "{tag}\t{}\t{}\t{}",
                    member.access_flags,
                    units(class.constant_pool.utf8(member.name_index).unwrap()),
                    units(class.constant_pool.utf8(member.descriptor_index).unwrap())
                );
                if tag == "M" {
                    let body = member
                        .attributes
                        .iter()
                        .any(|attr| matches!(attr.body, AttributeBody::Code(_)));
                    write!(row, "\t{}", u8::from(body)).unwrap();
                }
                rows.push(row);
            }
        }
        actual.push(rows);
    }
    let recorded = std::fs::read_to_string(facts.join("facts.tsv")).unwrap();
    let mut expected: Vec<Vec<String>> = Vec::new();
    for line in recorded.lines() {
        if line.starts_with("C\t") {
            let columns: Vec<_> = line.split('\t').collect();
            assert_eq!(columns.len(), 9);
            expected.push(vec![format!("C\t{}", columns[2..7].join("\t"))]);
        } else {
            expected.last_mut().unwrap().push(line.to_owned());
        }
    }
    actual.sort();
    expected.sort();
    assert!(!actual.is_empty());
    assert_eq!(actual, expected);
}

#[test]
fn proves_every_genuine_method_using_captured_jdk_member_and_hierarchy_facts() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let classes = fixtures.join("classes");
    let mut targets = Vec::new();
    for path in files::class_files(&classes) {
        let name = path
            .strip_prefix(&classes)
            .unwrap()
            .with_extension("")
            .to_str()
            .unwrap()
            .replace('\\', "/");
        if name != "module-info" {
            targets.push((name, std::fs::read(path).unwrap()));
        }
    }
    assert_eq!(targets.len(), 174);
    let corpus = corpus::Corpus::new(&fixtures.join("type-state-facts"), &targets);
    let environment = corpus.environment();
    let mut failures = Vec::new();
    let (mut bodies, mut bodyless) = (0, 0);
    for (name, id) in &corpus.targets {
        let class = corpus.registry.class(*id).unwrap();
        let proof = verify_class_types(&environment, *id, Default::default())
            .unwrap_or_else(|e| panic!("whole class {name}: {e}"));
        assert_eq!(proof.class_id(), *id);
        assert!(std::ptr::eq(proof.class(), class));
        assert!(std::ptr::eq(proof.environment(), &environment));
        assert_eq!(proof.usage().methods_started, class.methods.len() as u64);
        let (start_bodies, start_bodyless) = (bodies, bodyless);
        let mut method_sum = [0u64; 9];
        for (index, method) in class.methods.iter().enumerate() {
            match verify_method_types(&environment, *id, index, Default::default()) {
                Ok(proof) => {
                    assert!(std::ptr::eq(proof.environment(), &environment));
                    assert!(std::ptr::eq(proof.code_validation().class(), class));
                    assert!(std::ptr::eq(proof.code_validation().method(), method));
                    assert_eq!((proof.class_id(), proof.method_index()), (*id, index));
                    if proof.code_validation().code().is_some() {
                        bodies += 1;
                    } else {
                        bodyless += 1;
                    }
                    let repeat =
                        verify_method_types(&environment, *id, index, Default::default()).unwrap();
                    assert_eq!(proof.usage(), repeat.usage());
                    for (sum, value) in method_sum.iter_mut().zip(counters(proof.usage())) {
                        *sum = sum.checked_add(value).unwrap();
                    }
                }
                Err(error) => {
                    let method_name = class.constant_pool.utf8(method.name_index).unwrap();
                    failures.push(format!("{name}.{method_name:?} #{index}: {error}"));
                }
            }
        }
        assert_eq!(method_sum, counters(proof.usage().verification), "{name}");
        assert_eq!(proof.usage().code_bodies, bodies - start_bodies, "{name}");
        assert_eq!(proof.usage().bodyless, bodyless - start_bodyless, "{name}");
    }
    assert!(
        failures.is_empty(),
        "{} rejected declarations:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!((bodies, bodyless), (937, 31));
}

fn counters(usage: TypeVerificationUsage) -> [u64; 9] {
    [
        usage.code.code_bytes,
        usage.code.metadata_entries,
        usage.code.utf16_units,
        usage.code.work_units,
        usage.type_symbols,
        usage.state_cells,
        usage.edge_checks,
        usage.utf16_units,
        usage.work_units,
    ]
}

#[test]
fn separates_structural_and_type_failures_in_the_existing_jdk_stage_oracle() {
    use crabspot_verifier::TypeVerificationErrorKind;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let reference =
        std::fs::read_to_string(fixtures.join("reference/code-validation.tsv")).unwrap();
    let cases: Vec<_> = reference
        .lines()
        .map(|line| {
            let columns: Vec<_> = line.split('\t').collect();
            let [name, expected, stage, outcome] = columns[..] else {
                panic!("invalid oracle row: {line}")
            };
            (name, expected, stage, outcome)
        })
        .collect();
    let targets = cases
        .iter()
        .map(|(name, _, _, _)| {
            let bytes = std::fs::read(
                fixtures
                    .join("code-validation")
                    .join(format!("{name}.class")),
            )
            .unwrap();
            (format!("validation/{name}"), bytes)
        })
        .collect::<Vec<_>>();
    let corpus = corpus::Corpus::new(&fixtures.join("type-state-facts"), &targets);
    let environment = corpus.environment();
    let mut counts = [0; 3];
    for ((_, id), (name, expected, stage, outcome)) in corpus.targets.iter().zip(cases) {
        let result = verify_method_types(&environment, *id, 0, Default::default());
        match (expected, stage, outcome) {
            ("accept", "invoke", "I:1") => {
                result.unwrap_or_else(|e| panic!("{name}: {e}"));
                counts[0] += 1;
            }
            ("reject", "lookup", "VerifyError" | "ClassFormatError") => {
                assert!(
                    matches!(result.unwrap_err().kind, TypeVerificationErrorKind::Code(_)),
                    "{name}"
                );
                counts[1] += 1;
            }
            ("deferred-type-error", "lookup", "VerifyError") => {
                assert!(
                    matches!(
                        result.unwrap_err().kind,
                        TypeVerificationErrorKind::TypeRule { .. }
                    ),
                    "{name}"
                );
                counts[2] += 1;
            }
            _ => panic!("unexpected result for {name}: {expected} {stage} {outcome}"),
        }
    }
    assert_eq!(counts, [4, 12, 2]);
}

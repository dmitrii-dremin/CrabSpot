//! Original classfiles tested independently with JDK21 -Xverify:all.
//! Lookup forces verification; defineClass acceptance alone proves nothing here.
use crabspot_classfile::parse;
use crabspot_verifier::{
    CodeValidationErrorKind as Kind, CodeValidationLocation as Location, CodeValidationOptions,
    validate_method_code,
};
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn original_encodings_distinguish_static_rejections_from_deferred_type_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let rows = include_str!("../../../tests/fixtures/reference/code-validation.tsv");
    let mut seen = BTreeSet::new();
    let (mut accepted, mut rejected, mut deferred) = (0, 0, 0);
    for line in rows.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (name, classification, stage, outcome) =
            (columns[0], columns[1], columns[2], columns[3]);
        assert!(seen.insert(name));
        let bytes =
            std::fs::read(root.join("code-validation").join(format!("{name}.class"))).unwrap();
        let class = parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(class.methods.len(), 1);
        let before = class.clone();
        let result = validate_method_code(&class, 0, CodeValidationOptions::default());
        match classification {
            "accept" => {
                accepted += 1;
                assert_eq!((stage, outcome), ("invoke", "I:1"));
                assert!(result.is_ok(), "{name}: {result:?}");
            }
            "deferred-type-error" => {
                deferred += 1;
                assert!(matches!(
                    name,
                    "DeadTypeMismatch" | "DeadUninitializedLocal"
                ));
                assert_eq!((stage, outcome), ("lookup", "VerifyError"));
                assert!(
                    result.is_ok(),
                    "outside the partial contract: {name}: {result:?}"
                );
            }
            "reject" => {
                rejected += 1;
                assert_eq!(stage, "lookup");
                assert!(matches!(outcome, "ClassFormatError" | "VerifyError"));
                let error = result.unwrap_err();
                assert_eq!(error.method_index, 0);
                match name {
                    "DeadBadField" | "DeadBadInvoke" | "DeadBadLdc" | "DeadInterface51" => {
                        assert_eq!(error.location, Location::Instruction(2));
                        assert!(matches!(error.kind, Kind::InvalidConstant { .. }));
                    }
                    "DeadBadLocal" | "DeadBadLong" | "DeadBadWide" => {
                        assert_eq!(error.location, Location::Instruction(2));
                        assert!(matches!(error.kind, Kind::LocalOutOfBounds { .. }));
                    }
                    "HandlerStartMiddle" | "HandlerEndMiddle" | "HandlerTargetMiddle" => {
                        assert_eq!(error.location, Location::ExceptionHandler(0));
                        assert!(matches!(
                            error.kind,
                            Kind::InvalidBoundary { offset: 1, .. }
                        ));
                    }
                    "FrameMiddle" => {
                        assert_eq!(
                            error.location,
                            Location::StackMapFrame {
                                attribute: 0,
                                frame: 0
                            }
                        );
                        assert!(matches!(
                            error.kind,
                            Kind::InvalidBoundary { offset: 1, .. }
                        ));
                    }
                    "UninitWrongSite" => {
                        assert_eq!(
                            error.location,
                            Location::StackMapFrame {
                                attribute: 0,
                                frame: 0
                            }
                        );
                        assert!(matches!(error.kind, Kind::InvalidStackMap(_)));
                    }
                    _ => panic!("undeclared structural negative: {name}"),
                }
                assert_eq!(
                    validate_method_code(&class, 0, CodeValidationOptions::default()).unwrap_err(),
                    error
                );
            }
            _ => panic!("unknown classification {classification}"),
        }
        assert_eq!(class, before);
    }
    assert_eq!((accepted, rejected, deferred), (4, 12, 2));
    assert_eq!(seen.len(), 18);
    let files: BTreeSet<_> = std::fs::read_dir(root.join("code-validation"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|value| value == "class"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, seen.into_iter().map(str::to_owned).collect());
}

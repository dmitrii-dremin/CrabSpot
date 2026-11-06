//! Independently emitted image/CLI controls; no vendor class bytes or host equality.
#[path = "../../interpreter/tests/support/equality_classes.rs"]
mod recipes;
use crabspot_launcher::*;
use recipes::{CLASS, Class, LATIN, STRING, instruction};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Image {
    root: PathBuf,
    boot: PathBuf,
    app: PathBuf,
}
impl Image {
    fn new(c: Class, classes: Vec<Class>) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "string-equality-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let boot = root.join("boot");
        let app = root.join("app");
        std::fs::create_dir_all(&boot).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        put(&boot, &recipes::object());
        for n in ["java/lang/Cloneable", "java/io/Serializable"] {
            put(&boot, &Class::new(n, Some("java/lang/Object"), 0x601));
        }
        for n in [
            "java/lang/invoke/MethodType",
            "java/lang/invoke/MethodHandle",
        ] {
            put(&boot, &Class::new(n, Some("java/lang/Object"), 0x31));
        }
        put(&boot, &Class::plain("java/lang/Throwable"));
        put(
            &boot,
            &Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        put(&boot, &recipes::class());
        for c in classes {
            put(&boot, &c);
        }
        put(&app, &c);
        Self { root, boot, app }
    }
    fn run(
        &self,
        args: &[&[u16]],
        profile: ApplicationStringEqualityProfile,
        options: ApplicationOptions,
    ) -> Result<ApplicationRun, Box<ApplicationError>> {
        run_directory_application_with_string_equality(
            ApplicationDirectories {
                platform: &self.boot,
                classes: &self.app,
            },
            "C",
            &[
                StaticIntObservation {
                    binary_class: "C",
                    field: "out",
                },
                StaticIntObservation {
                    binary_class: "C",
                    field: "count",
                },
            ],
            options,
            ApplicationInvocation {
                arguments: args,
                ..Default::default()
            },
            profile,
        )
    }
    fn patch_code(&self, owner: &str, method: &str, pc: usize, value: u8) {
        let p = self.boot.join(format!("{owner}.class"));
        let mut b = std::fs::read(&p).unwrap();
        let c = crabspot_classfile::parse(&b).unwrap();
        let m = c
            .methods
            .iter()
            .find(|m| {
                c.constant_pool
                    .utf8(m.name_index)
                    .unwrap()
                    .equals_ascii(method)
            })
            .unwrap();
        let a = m
            .attributes
            .iter()
            .find(|a| matches!(a.body, crabspot_classfile::AttributeBody::Code(_)))
            .unwrap();
        let crabspot_classfile::AttributeBody::Code(code) = &a.body else {
            unreachable!()
        };
        assert!(pc < code.code.len());
        assert_eq!(
            &b[a.offset + 14..a.offset + 14 + code.code.len()],
            code.code
        );
        b[a.offset + 14 + pc] = value;
        std::fs::write(p, b).unwrap();
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn put(root: &Path, c: &Class) {
    let p = root.join(format!("{}.class", c.name));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, c.bytes()).unwrap();
}
fn strict() -> ApplicationStringEqualityProfile {
    ApplicationStringEqualityProfile {
        mode: StringEqualityMode::RequireCompactEquality,
        ..Default::default()
    }
}
fn program() -> Class {
    program_with_expected(None)
}
fn program_with_expected(expected: Option<i16>) -> Class {
    let mut c = Class::plain("C");
    c.field("out", "I", 9);
    c.field("count", "I", 9);
    let out = c.reference(9, "C", "out", "I");
    let count = c.reference(9, "C", "count", "I");
    let equals = c.reference(10, "java/lang/Object", "equals", "(Ljava/lang/Object;)Z");
    let compare = c.reference(
        10,
        "C",
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
    );
    let ctor = c.reference(10, "java/lang/Object", "<init>", "()V");
    let object = c.class("java/lang/Object");
    c.method(
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        9,
        &[
            instruction(0xb2, count),
            vec![4, 0x60],
            instruction(0xb3, count),
            vec![0x2a, 0x2b],
            instruction(0xb6, equals),
            vec![0xac],
        ]
        .concat(),
        2,
        2,
    );
    let mut code = vec![];
    // Nine true predicates and a final false Object comparison. Only predicate2
    // changes when the second ASCII argument is changed, yielding511 then507.
    let pairs = [
        (0, Some(0), false),
        (6, Some(6), false),
        (0, Some(1), false),
        (0, Some(2), true),
        (3, Some(4), false),
        (3, Some(5), true),
        (6, Some(7), false),
        (3, Some(8), true),
        (0, None, true),
        (0, None, false),
    ];
    for (bit, (a, b, invert)) in pairs.into_iter().enumerate() {
        code.extend(instruction(0xb2, out));
        code.extend([0x2a, 0x10, a, 0x32]);
        if bit == 9 {
            code.extend(instruction(0xbb, object));
            code.push(0x59);
            code.extend(instruction(0xb7, ctor));
        } else if let Some(b) = b {
            code.extend([0x2a, 0x10, b, 0x32]);
        } else {
            code.push(1);
        }
        code.extend(instruction(0xb8, compare));
        if invert {
            code.extend([4, 0x82]);
        }
        code.extend([0x10, bit as u8, 0x78, 0x80]);
        code.extend(instruction(0xb3, out));
    }
    if let Some(expected) = expected {
        code.extend(instruction(0xb2, out));
        code.push(0x11);
        code.extend(expected.to_be_bytes());
        code.extend([0x9f, 0, 7, 4, 3, 0x6c, 0x57]);
    }
    let end = code.len();
    code.push(0xb1);
    c.method("main", "([Ljava/lang/String;)V", 9, &code, 5, 1);
    if expected.is_some() {
        let end = u16::try_from(end).unwrap();
        c.stackmap(&[0, 1, 251, (end >> 8) as u8, end as u8]);
    }
    c
}
fn vectors() -> Vec<Vec<u16>> {
    [
        "A~a", "A~a", "A~b", "Wxyz", "Wxyz", "WxyQ", "same", "same", "Wxy",
    ]
    .into_iter()
    .map(|s| s.encode_utf16().collect())
    .collect()
}
fn run_ok(
    image: &Image,
    args: &[&[u16]],
    profile: ApplicationStringEqualityProfile,
) -> ApplicationRun {
    let r = image.run(args, profile, Default::default()).unwrap();
    assert!(
        matches!(r.completion, ApplicationCompletion::Returned),
        "{:?}",
        r.completion
    );
    r
}
#[test]
fn full_verified_image_uses_one_native_registration_and_query_for_both_policies() {
    let image = Image::new(program(), recipes::platform(false));
    let args = vectors();
    let refs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    for policy in [
        BootstrapAssertionPolicy::Disabled,
        BootstrapAssertionPolicy::Enabled,
    ] {
        let r = run_ok(
            &image,
            &refs,
            ApplicationStringEqualityProfile {
                bootstrap_assertions: policy,
                ..strict()
            },
        );
        assert_eq!(r.static_ints, [511, 10]);
        assert_eq!(
            r.usage.string_equality,
            StringEqualityUsage {
                natives_registered: true,
                assertion_queries: 1
            }
        );
        assert!(r.usage.string_equality_discovery.work_units > 0);
        assert!(r.usage.string_body_discovery.work_units > 19);
        assert_eq!(r.usage.argument_strings, 9);
        assert_eq!(r.usage.engine_evaluations, 1);
    }
    let mut changed = args.clone();
    changed[1] = args[2].clone();
    let refs: Vec<_> = changed.iter().map(Vec::as_slice).collect();
    assert_eq!(run_ok(&image, &refs, strict()).static_ints, [507, 10]);
}
#[test]
fn physical_cli_defaults_run_the_exact_ascii_true_and_false_vectors() {
    for (second, expected) in [("A~a", 511), ("A~b", 507)] {
        let image = Image::new(
            program_with_expected(Some(expected)),
            recipes::platform(false),
        );
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_crabspot"))
            .args(["run", "--platform"])
            .arg(&image.boot)
            .arg("--classes")
            .arg(&image.app)
            .args([
                "C", "--", "A~a", second, "A~b", "Wxyz", "Wxyz", "WxyQ", "same", "same", "Wxy",
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        assert!(o.stdout.is_empty());
        assert!(o.stderr.is_empty());
    }
    // The same guest asserts the opposite mask, reaching real division by zero.
    let image = Image::new(program_with_expected(Some(507)), recipes::platform(false));
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_crabspot"))
        .args(["run", "--platform"])
        .arg(&image.boot)
        .arg("--classes")
        .arg(&image.app)
        .args([
            "C", "--", "A~a", "A~a", "A~b", "Wxyz", "Wxyz", "WxyQ", "same", "same", "Wxy",
        ])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(
        String::from_utf8(o.stderr)
            .unwrap()
            .contains("ArithmeticException")
    );
}
#[test]
fn equality_discovery_is_independent_inclusive_and_fails_before_arguments_or_execution() {
    let image = Image::new(program(), recipes::platform(false));
    let a = vectors();
    let refs: Vec<_> = a.iter().map(Vec::as_slice).collect();
    let good = run_ok(&image, &refs, strict());
    let exact = ApplicationStringEqualityProfile {
        discovery: StringBodyDiscoveryLimits {
            max_work_units: good.usage.string_equality_discovery.work_units,
        },
        ..strict()
    };
    assert_eq!(run_ok(&image, &refs, exact).usage, good.usage);
    for limit in [0, good.usage.string_equality_discovery.work_units - 1] {
        let e = image
            .run(
                &refs,
                ApplicationStringEqualityProfile {
                    discovery: StringBodyDiscoveryLimits {
                        max_work_units: limit,
                    },
                    ..strict()
                },
                Default::default(),
            )
            .unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Verification);
        assert!(matches!(e.kind, ApplicationErrorKind::StringBody(_)));
        assert!(e.usage.string_body_discovery.work_units > 19);
        assert!(e.usage.string_equality_discovery.work_units <= limit);
        assert_eq!(e.usage.argument_arrays, 0);
        assert_eq!(e.usage.string_equality, StringEqualityUsage::default());
    }
}
#[test]
fn absence_keeps_compact_capability_but_strict_equality_refuses_and_old_wrappers_remain_auto() {
    let image = Image::new(program(), recipes::compact_platform());
    let a = vectors();
    let refs: Vec<_> = a.iter().map(Vec::as_slice).collect();
    let e = image.run(&refs, strict(), Default::default()).unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert!(e.usage.string_equality_discovery.work_units > 0);
    assert_eq!(e.usage.argument_arrays, 0);
    let r = image
        .run(&refs, Default::default(), Default::default())
        .unwrap();
    assert!(matches!(r.completion, ApplicationCompletion::Returned));
    assert_eq!(r.static_ints, [427, 10]);
    assert_eq!(r.usage.string_equality, StringEqualityUsage::default());
    let r = run_directory_application_with_string_bodies(
        ApplicationDirectories {
            platform: &image.boot,
            classes: &image.app,
        },
        "C",
        &[],
        Default::default(),
        ApplicationInvocation {
            arguments: &refs,
            ..Default::default()
        },
        Default::default(),
    )
    .unwrap();
    assert_eq!(r.usage.string_equality, StringEqualityUsage::default());
}
#[test]
fn unsupported_compact_has_zero_equality_work_and_preserves_kernel_mode() {
    let image = Image::new(
        program(),
        vec![Class::new(STRING, Some("java/lang/Object"), 0x31)],
    );
    let e = image.run(&[], strict(), Default::default()).unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert_eq!(e.usage.string_equality_discovery.work_units, 0);
    assert_eq!(e.usage.string_equality, StringEqualityUsage::default());
    assert_eq!(e.usage.argument_arrays, 0);
}
#[test]
fn full_claimed_recipe_checks_dead_branch_handler_and_loop_even_in_auto() {
    for (owner, name, pc, value) in [
        (CLASS, "desiredAssertionStatus", 36, 0x2b),
        (LATIN, "equals", 29, 2),
        (CLASS, "<clinit>", 3, 4),
    ] {
        let image = Image::new(program(), recipes::platform(false));
        image.patch_code(owner, name, pc, value);
        let e = image
            .run(&[], Default::default(), Default::default())
            .unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Verification);
        assert!(
            matches!(e.kind, ApplicationErrorKind::StringBody(_)),
            "{:?}",
            e.kind
        );
        assert_eq!(e.usage.argument_arrays, 0);
        assert_eq!(e.usage.engine_evaluations, 0);
        assert_eq!(e.usage.string_equality, StringEqualityUsage::default());
    }
    // Change only the first catch target in the otherwise unused nonnull branch.
    let image = Image::new(program(), recipes::platform(false));
    let p = image.boot.join(format!("{CLASS}.class"));
    let mut b = std::fs::read(&p).unwrap();
    let c = crabspot_classfile::parse(&b).unwrap();
    let m = c
        .methods
        .iter()
        .find(|m| {
            c.constant_pool
                .utf8(m.name_index)
                .unwrap()
                .equals_ascii("desiredAssertionStatus")
        })
        .unwrap();
    let a = m
        .attributes
        .iter()
        .find(|a| matches!(a.body, crabspot_classfile::AttributeBody::Code(_)))
        .unwrap();
    let handler = a.offset + 14 + 54 + 2;
    b[handler + 4..handler + 6].copy_from_slice(&49u16.to_be_bytes());
    std::fs::write(p, b).unwrap();
    let e = image
        .run(&[], Default::default(), Default::default())
        .unwrap_err();
    assert!(matches!(e.kind, ApplicationErrorKind::StringBody(_)));
    assert_eq!(e.usage.argument_arrays, 0);
}
#[test]
fn added_helper_is_private_proof_seed_and_new_metadata_facts_are_not_app_imports() {
    for owner in [LATIN, "java/lang/ClassLoader", "java/util/Map"] {
        let mut c = program();
        c.class(owner);
        let image = Image::new(c, recipes::platform(false));
        let e = image.run(&[], strict(), Default::default()).unwrap_err();
        assert!(matches!(
            e.kind,
            ApplicationErrorKind::UnsupportedPlatformReference
        ));
        // Discovery now precedes dependency checking to establish matched public roles.
        assert!(e.usage.string_equality_discovery.work_units > 0);
    }
    let image = Image::new(program(), recipes::platform(false));
    let a = vectors();
    let refs: Vec<_> = a.iter().map(Vec::as_slice).collect();
    let r = run_ok(&image, &refs, strict());
    assert!(r.usage.preparation_witnesses > 0);
    let mut opts = ApplicationOptions::default();
    opts.verification_total.max_methods = r.usage.verification.methods_started - 1;
    let e = image.run(&refs, strict(), opts).unwrap_err();
    assert!(
        matches!(e.kind, ApplicationErrorKind::VerificationLimit { .. }),
        "{:?}",
        e.kind
    );
    assert!(e.usage.string_equality_discovery.work_units > 0);
    assert_eq!(e.usage.argument_arrays, 0);
    assert_eq!(e.usage.string_equality, StringEqualityUsage::default());
}
#[test]
fn registration_prefix_survives_class_array_failure_in_public_report() {
    let image = Image::new(program(), recipes::platform(false));
    let a = vectors();
    let refs: Vec<_> = a.iter().map(Vec::as_slice).collect();
    let mut o = ApplicationOptions::default();
    o.heap.max_arrays = 5;
    let r = image.run(&refs, strict(), o).unwrap();
    let ApplicationCompletion::ExecutionFailed { error, .. } = r.completion else {
        panic!("{:?}", r.completion)
    };
    assert!(error.method.as_ref().unwrap().name.equals_ascii("<clinit>"));
    assert_eq!(
        r.usage.string_equality,
        StringEqualityUsage {
            natives_registered: true,
            assertion_queries: 0
        }
    );
    assert_eq!(r.usage.argument_strings, 9);
    assert_eq!(r.usage.engine_evaluations, 1);
}

#[test]
fn bootstrap_policy_is_observable_through_both_actual_class_java_queries() {
    let mut c = Class::plain("C");
    c.field("out", "I", 9);
    c.field("count", "I", 9);
    let out = c.reference(9, "C", "out", "I");
    let count = c.reference(9, "C", "count", "I");
    let query = c.reference(10, CLASS, "desiredAssertionStatus", "()Z");
    let string = c.class(STRING);
    let object = c.class("java/lang/Object");
    c.method(
        "main",
        "([Ljava/lang/String;)V",
        9,
        &[
            instruction(0x13, string),
            instruction(0xb6, query),
            instruction(0xb3, out),
            instruction(0x13, object),
            instruction(0xb6, query),
            instruction(0xb3, count),
            vec![0xb1],
        ]
        .concat(),
        1,
        1,
    );
    let image = Image::new(c, recipes::platform(false));
    for p in [
        BootstrapAssertionPolicy::Disabled,
        BootstrapAssertionPolicy::Enabled,
    ] {
        let r = run_ok(
            &image,
            &[],
            ApplicationStringEqualityProfile {
                bootstrap_assertions: p,
                ..strict()
            },
        );
        assert_eq!(
            r.static_ints,
            [i32::from(p == BootstrapAssertionPolicy::Enabled); 2]
        );
        assert_eq!(
            r.usage.string_equality,
            StringEqualityUsage {
                natives_registered: true,
                assertion_queries: 2
            }
        );
        assert_eq!(r.usage.string_bodies.backings, 0);
        assert_eq!(r.usage.argument_strings, 0);
    }
}
#[test]
fn public_utf16_arguments_cover_distinct_empty_nul_surrogate_and_both_coders() {
    let image = Image::new(program(), recipes::platform(true));
    for pair in [
        vec![],
        vec![0],
        vec![65, 0, 255],
        vec![0xdfff],
        vec![0x20ac, 0xd83d, 0xde80],
    ] {
        let mut args = vectors();
        args[0] = pair.clone();
        args[1] = pair;
        let refs: Vec<_> = args.iter().map(Vec::as_slice).collect();
        let r = run_ok(&image, &refs, strict());
        assert_eq!(r.static_ints, [511, 10]);
        assert_eq!(r.usage.argument_strings, 9);
        assert_eq!(r.usage.string_equality.assertion_queries, 1);
    }
}
#[test]
fn query_of_application_or_array_mirror_retains_real_getfield_failure_and_no_registration() {
    for owner in ["C", "[I"] {
        let mut c = Class::plain("C");
        c.field("out", "I", 9);
        c.field("count", "I", 9);
        let ty = c.class(owner);
        let query = c.reference(10, CLASS, "desiredAssertionStatus", "()Z");
        c.method(
            "main",
            "([Ljava/lang/String;)V",
            9,
            &[
                instruction(0x13, ty),
                instruction(0xb6, query),
                vec![0x57, 0xb1],
            ]
            .concat(),
            1,
            1,
        );
        let image = Image::new(c, recipes::platform(false));
        let r = image.run(&[], strict(), Default::default()).unwrap();
        let ApplicationCompletion::ExecutionFailed { phase, error } = r.completion else {
            panic!()
        };
        assert_eq!(phase, EntryEvaluation::MainEvaluation);
        assert!(
            error
                .method
                .as_ref()
                .unwrap()
                .name
                .equals_ascii("desiredAssertionStatus")
        );
        assert_eq!(error.offset, Some(1));
        assert!(
            matches!(&error.kind,crabspot_interpreter::ExecutionErrorKind::FieldAccess(e)if matches!(**e,crabspot_runtime::FieldAccessError::StringBody(_)))
        );
        assert_eq!(r.usage.string_equality, StringEqualityUsage::default());
        assert_eq!(r.usage.argument_arrays, 1);
    }
}
#[test]
fn claimed_role_flags_and_fields_are_checked_before_any_arguments_are_issued() {
    for case in 0..4 {
        let image = Image::new(program(), recipes::platform(false));
        let p = image.boot.join(format!("{CLASS}.class"));
        let mut b = std::fs::read(&p).unwrap();
        let c = crabspot_classfile::parse(&b).unwrap();
        let at = if case < 2 {
            c.methods
                .iter()
                .find(|m| {
                    c.constant_pool
                        .utf8(m.name_index)
                        .unwrap()
                        .equals_ascii(if case == 0 {
                            "registerNatives"
                        } else {
                            "desiredAssertionStatus"
                        })
                })
                .unwrap()
                .offset
        } else {
            c.fields
                .iter()
                .find(|m| {
                    c.constant_pool
                        .utf8(m.name_index)
                        .unwrap()
                        .equals_ascii(if case == 2 {
                            "classLoader"
                        } else {
                            "EMPTY_CLASS_ARRAY"
                        })
                })
                .unwrap()
                .offset
        };
        let flags = u16::from_be_bytes([b[at], b[at + 1]]);
        let altered = if case == 1 {
            flags | 0x20
        } else {
            flags ^ 0x10
        };
        b[at..at + 2].copy_from_slice(&altered.to_be_bytes());
        std::fs::write(p, b).unwrap();
        let e = image
            .run(&[], Default::default(), Default::default())
            .unwrap_err();
        assert!(
            matches!(e.kind, ApplicationErrorKind::StringBody(_)),
            "{:?}",
            e.kind
        );
        assert_eq!(e.stage, ApplicationStage::Verification);
        assert_eq!(e.usage.argument_strings, 0);
        assert_eq!(e.usage.argument_arrays, 0);
        assert_eq!(e.usage.string_equality, StringEqualityUsage::default());
    }
}

//! Image policy and actual Java bodies with independently emitted class bytes.
#[path = "../../interpreter/tests/support/string_body_world.rs"]
mod world;
use crabspot_interpreter::ExecutionErrorKind as E;
use crabspot_launcher::*;
use crabspot_runtime::*;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use world::{CASE, COMPARATOR, Class, STREAM, STRING, instruction};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Image {
    root: PathBuf,
    boot: PathBuf,
    app: PathBuf,
}
impl Image {
    fn new(c: Class, classes: Vec<Class>) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "string-body-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let boot = root.join("boot");
        let app = root.join("app");
        std::fs::create_dir_all(&boot).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        let mut object = Class::new("java/lang/Object", None, 0x21);
        object.method("<init>", "()V", 1, &[0xb1], 0, 1);
        put(&boot, &object);
        for name in ["java/lang/Cloneable", "java/io/Serializable"] {
            put(&boot, &Class::new(name, Some("java/lang/Object"), 0x601));
        }
        for name in [
            "java/lang/Class",
            "java/lang/invoke/MethodType",
            "java/lang/invoke/MethodHandle",
        ] {
            put(&boot, &Class::new(name, Some("java/lang/Object"), 0x31));
        }
        put(&boot, &Class::plain("java/lang/Throwable"));
        put(
            &boot,
            &Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        for c in classes {
            put(&boot, &c);
        }
        put(&app, &c);
        Self { root, boot, app }
    }
    fn dirs(&self) -> ApplicationDirectories<'_> {
        ApplicationDirectories {
            platform: &self.boot,
            classes: &self.app,
        }
    }
    fn run(
        &self,
        args: &[&[u16]],
        profile: ApplicationStringBodyProfile,
        options: ApplicationOptions,
    ) -> Result<ApplicationRun, Box<ApplicationError>> {
        run_directory_application_with_string_bodies(
            self.dirs(),
            "C",
            &[StaticIntObservation {
                binary_class: "C",
                field: "out",
            }],
            options,
            ApplicationInvocation {
                arguments: args,
                ..Default::default()
            },
            profile,
        )
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
fn program(target: Option<&str>) -> Class {
    let mut c = Class::plain("C");
    c.field("out", "I", 9);
    let out = c.reference(9, "C", "out", "I");
    let mut code = if let Some(target) = target {
        let desc = if target == "isEmpty" { "()Z" } else { "()I" };
        let method = c.reference(10, STRING, target, desc);
        [vec![0x2a, 3, 0x32], instruction(0xb6, method)].concat()
    } else {
        vec![4]
    };
    code.extend(instruction(0xb3, out));
    code.push(0xb1);
    c.method("main", "([Ljava/lang/String;)V", 9, &code, 2, 1);
    c
}
fn strict() -> ApplicationStringBodyProfile {
    ApplicationStringBodyProfile {
        mode: StringBodyMode::RequireCompact,
        ..Default::default()
    }
}
#[test]
fn public_auto_and_strict_execute_length_and_empty_with_real_argument_storage() {
    for target in ["length", "isEmpty"] {
        let image = Image::new(program(Some(target)), world::platform());
        for units in [
            vec![],
            vec![65, 0, 255],
            vec![65, 0x20ac, 0xd83d, 0xde80],
            vec![0xdfff],
        ] {
            for profile in [ApplicationStringBodyProfile::default(), strict()] {
                let run = image.run(&[&units], profile, Default::default()).unwrap();
                assert!(matches!(run.completion, ApplicationCompletion::Returned));
                assert_eq!(
                    run.static_ints,
                    [if target == "length" {
                        units.len() as i32
                    } else {
                        i32::from(units.is_empty())
                    }]
                );
                assert_eq!(run.usage.argument_strings, 1);
                assert_eq!(run.usage.engine_evaluations, 1);
                assert_eq!(run.usage.string_bodies.backings, 1);
                assert_eq!(
                    run.usage.string_bodies.backing_bytes,
                    (units.len()
                        * if units.iter().all(|u| *u <= 255) {
                            1
                        } else {
                            2
                        }) as u64
                );
                assert!(run.usage.string_body_discovery.work_units > 19);
            }
        }
    }
}
#[test]
fn auto_kernel_fallback_and_strict_refusal_have_identical_exact_discovery_prefix() {
    let role = Class::new(STRING, Some("java/lang/Object"), 0x31);
    let image = Image::new(program(None), vec![role]);
    let profile = ApplicationStringBodyProfile {
        discovery: StringBodyDiscoveryLimits { max_work_units: 19 },
        ..Default::default()
    };
    let run = image.run(&[], profile, Default::default()).unwrap();
    assert!(matches!(run.completion, ApplicationCompletion::Returned));
    assert_eq!(run.static_ints, [1]);
    assert_eq!(run.usage.string_body_discovery.work_units, 19);
    assert_eq!(run.usage.string_bodies, StringBodyUsage::default());
    let error = image
        .run(
            &[],
            ApplicationStringBodyProfile {
                mode: StringBodyMode::RequireCompact,
                ..profile
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(error.stage, ApplicationStage::Verification);
    assert_eq!(error.usage.string_body_discovery.work_units, 19);
    assert!(
        matches!(error.kind,ApplicationErrorKind::StringBody(ref e) if matches!(e.kind,StringBodyErrorKind::UnsupportedShape))
    );
    assert_eq!(error.usage.argument_arrays, 0);
    assert_eq!(error.usage.engine_evaluations, 0);
    let error = image
        .run(
            &[],
            ApplicationStringBodyProfile {
                discovery: StringBodyDiscoveryLimits { max_work_units: 18 },
                ..profile
            },
            Default::default(),
        )
        .unwrap_err();
    assert!(
        matches!(error.kind,ApplicationErrorKind::StringBody(ref e) if matches!(e.kind,StringBodyErrorKind::Limit{resource:StringBodyResource::DiscoveryWork,limit:18,required:19}))
    );
    assert_eq!(error.usage.string_body_discovery.work_units, 3);
}
#[test]
fn matched_candidate_errors_never_fall_back_and_cp_renumbering_is_accepted() {
    for bad in 0..5 {
        let mut classes = world::platform();
        match bad {
            0 => classes[0].field_flags("value", 0x11),
            1 => classes[0].method_flags("length", 0x21),
            2 => classes[0].code_byte("length", 10, 0xb0),
            3 => {
                classes.remove(1);
            }
            _ => {
                classes[2] = Class::plain(COMPARATOR);
            }
        }
        let image = Image::new(program(None), classes);
        let e = image
            .run(&[], Default::default(), Default::default())
            .unwrap_err();
        if bad == 4 {
            // Ordinary hierarchy collection precedes probing and rejects a class
            // substituted into the comparator's interface edge immediately.
            assert_eq!(e.stage, ApplicationStage::Hierarchy);
            assert!(matches!(e.kind, ApplicationErrorKind::Hierarchy(_)));
            assert_eq!(e.usage.string_body_discovery.work_units, 0);
        } else {
            assert_eq!(e.stage, ApplicationStage::Verification);
            assert!(
                matches!(e.kind, ApplicationErrorKind::StringBody(_)),
                "{e:?}"
            );
        }
        assert_eq!(e.usage.argument_arrays, 0);
        assert_eq!(e.usage.engine_evaluations, 0);
    }
    let mut classes = world::platform();
    classes[0] = world::string_class(300);
    let image = Image::new(program(Some("length")), classes);
    assert_eq!(
        image
            .run(&[&[0x20ac]], strict(), Default::default())
            .unwrap()
            .static_ints,
        [1]
    );
}
#[test]
fn unsupported_recipe_falls_back_only_in_auto_not_for_policy_failure() {
    let mut classes = world::platform();
    classes[0].code_byte("<clinit>", 0, 3);
    let image = Image::new(program(Some("length")), classes);
    let run = image
        .run(&[&[120]], Default::default(), Default::default())
        .unwrap();
    assert!(
        matches!(run.completion,ApplicationCompletion::ExecutionFailed{error,..} if matches!(error.kind,E::MethodAccess(_)))
    );
    assert_eq!(run.usage.string_bodies.backings, 0);
    assert!(matches!(
        image
            .run(&[], strict(), Default::default())
            .unwrap_err()
            .kind,
        ApplicationErrorKind::StringBody(_)
    ));
}
#[test]
fn mandatory_private_helpers_require_proof_without_becoming_application_imports() {
    let full = Image::new(program(None), world::platform());
    let full_run = full
        .run(&[], Default::default(), Default::default())
        .unwrap();
    let mut fallback = world::platform();
    fallback[0].code_byte("<clinit>", 0, 3);
    let fallback = Image::new(program(None), fallback);
    let fallback = fallback
        .run(&[], Default::default(), Default::default())
        .unwrap();
    assert_eq!(
        full_run.usage.proved_classes,
        fallback.usage.proved_classes + 3
    );
    assert_eq!(
        full_run.usage.proved_classes,
        full_run.usage.preparation_witnesses
    );
    assert_eq!(
        full_run.usage.proved_classes,
        full_run.usage.admitted_executable_classes
    );
    let mut classes = world::platform();
    classes[3].method("bad", "()I", 9, &[0xb1], 0, 0);
    let invalid = Image::new(program(None), classes);
    let e = invalid
        .run(&[], Default::default(), Default::default())
        .unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert!(matches!(e.kind, ApplicationErrorKind::Verification(_)));
    assert!(e.usage.string_body_discovery.work_units > 19);
    assert_eq!(e.usage.argument_arrays, 0);
    for name in [CASE, COMPARATOR, STREAM] {
        let mut c = program(None);
        c.class(name);
        let image = Image::new(c, world::platform());
        let e = image
            .run(&[], Default::default(), Default::default())
            .unwrap_err();
        assert!(matches!(
            e.kind,
            ApplicationErrorKind::UnsupportedPlatformReference
        ));
        assert_eq!(e.usage.string_body_discovery.work_units, 0);
    }
}
#[test]
fn real_body_work_failure_reports_actual_method_and_preserves_discovery_and_arguments() {
    let image = Image::new(program(Some("length")), world::platform());
    let run = image
        .run(
            &[&[0x20ac]],
            ApplicationStringBodyProfile {
                backing: StringBodyLimits { max_work_units: 0 },
                ..strict()
            },
            Default::default(),
        )
        .unwrap();
    let ApplicationCompletion::ExecutionFailed { phase, error } = run.completion else {
        panic!()
    };
    assert_eq!(phase, EntryEvaluation::MainEvaluation);
    assert_eq!(error.offset, Some(1));
    assert!(error.method.as_ref().unwrap().name.equals_ascii("length"));
    assert!(matches!(error.kind, E::FieldAccess(_)));
    assert_eq!(run.usage.argument_strings, 1);
    assert_eq!(run.usage.strings.objects, 1);
    assert_eq!(run.usage.string_bodies, StringBodyUsage::default());
    assert!(run.usage.string_body_discovery.work_units > 19);
}
#[test]
fn matched_image_proof_limit_remains_mandatory_and_does_not_enter_opaque_mode() {
    let image = Image::new(program(None), world::platform());
    let good = image.run(&[], strict(), Default::default()).unwrap();
    let mut options = ApplicationOptions::default();
    options.verification_total.max_methods = good.usage.verification.methods_started - 1;
    let e = image.run(&[], Default::default(), options).unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert!(e.usage.string_body_discovery.work_units > 19);
    assert_eq!(e.usage.argument_arrays, 0);
    assert_eq!(e.usage.engine_evaluations, 0);
}
#[test]
fn physical_cli_uses_auto_profile_and_actual_compact_code() {
    let image = Image::new(program(Some("length")), world::platform());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_crabspot"))
        .args(["run", "--platform"])
        .arg(&image.boot)
        .arg("--classes")
        .arg(&image.app)
        .args(["C", "--", "A\u{20ac}\u{1f680}"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
}

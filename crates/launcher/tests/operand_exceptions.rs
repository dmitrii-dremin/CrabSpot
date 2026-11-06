//! Independent proved-image controls for operand profile adoption and exact error stages.
#[path = "../../interpreter/tests/support/operand_world.rs"]
mod world;
use crabspot_interpreter::ExecutionErrorKind as E;
use crabspot_launcher::*;
use crabspot_runtime::{OperandExceptionErrorKind, OperandExceptionResource, VmExceptionKind};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use world::{ARITHMETIC, Class, THROWABLE, instruction, recipes};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Image {
    root: PathBuf,
    boot: PathBuf,
    app: PathBuf,
}
fn put(root: &Path, c: &Class) {
    let path = root.join(format!("{}.class", c.name));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, c.bytes()).unwrap();
}
impl Image {
    fn new(c: Class, matched: bool) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "operand-image-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let boot = root.join("boot");
        let app = root.join("app");
        std::fs::create_dir_all(&boot).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        put(&boot, &recipes::object());
        put(&boot, &recipes::class());
        for name in ["java/lang/Cloneable", "java/io/Serializable"] {
            put(&boot, &Class::new(name, Some("java/lang/Object"), 0x601));
        }
        for name in [
            "java/lang/invoke/MethodType",
            "java/lang/invoke/MethodHandle",
        ] {
            put(&boot, &Class::new(name, Some("java/lang/Object"), 0x31));
        }
        put(
            &boot,
            &if matched {
                world::throwable()
            } else {
                Class::plain(THROWABLE)
            },
        );
        put(&boot, &Class::new("java/lang/Error", Some(THROWABLE), 0x21));
        for c in recipes::platform(false) {
            put(&boot, &c);
        }
        if matched {
            for (name, parent) in [
                ("java/lang/Exception", THROWABLE),
                ("java/lang/RuntimeException", "java/lang/Exception"),
                (ARITHMETIC, "java/lang/RuntimeException"),
            ] {
                put(&boot, &world::exception(name, parent));
            }
            for (name, parent) in [
                (
                    "java/lang/IndexOutOfBoundsException",
                    "java/lang/RuntimeException",
                ),
                (
                    "java/lang/ArrayIndexOutOfBoundsException",
                    "java/lang/IndexOutOfBoundsException",
                ),
                (
                    "java/lang/NegativeArraySizeException",
                    "java/lang/RuntimeException",
                ),
                (
                    "java/lang/ArrayStoreException",
                    "java/lang/RuntimeException",
                ),
                ("java/lang/ClassCastException", "java/lang/RuntimeException"),
            ] {
                put(&boot, &world::exception(name, parent));
            }
            for name in [
                "java/lang/NullPointerException",
                "java/lang/AssertionError",
                "java/lang/StackTraceElement",
                "java/util/Collections",
                "java/util/Collections$EmptySet",
                "java/util/Collections$EmptyList",
                "java/util/Collections$EmptyMap",
            ] {
                let parent = if name == "java/lang/NullPointerException" {
                    "java/lang/RuntimeException"
                } else if name == "java/lang/AssertionError" {
                    "java/lang/Error"
                } else {
                    "java/lang/Object"
                };
                put(
                    &boot,
                    &Class::new(
                        name,
                        Some(parent),
                        if name.starts_with("java/util/") {
                            0x20
                        } else {
                            0x21
                        },
                    ),
                );
            }
        }
        put(&app, &c);
        Self { root, boot, app }
    }
    fn run(
        &self,
        profile: ApplicationOperandExceptionProfile,
        options: ApplicationOptions,
    ) -> Result<ApplicationRun, Box<ApplicationError>> {
        run_directory_application_with_operand_exceptions(
            ApplicationDirectories {
                platform: &self.boot,
                classes: &self.app,
            },
            "C",
            &[StaticIntObservation {
                binary_class: "C",
                field: "effect",
            }],
            options,
            Default::default(),
            profile,
        )
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn program() -> Class {
    let mut c = Class::plain("C");
    c.field("effect", "I", 9);
    let f = c.reference(9, "C", "effect", "I");
    c.method(
        "main",
        "([Ljava/lang/String;)V",
        9,
        &[
            vec![0x10, 7],
            instruction(0xb3, f),
            vec![2, 0xbc, 10, 0x57, 0xb1],
        ]
        .concat(),
        1,
        1,
    );
    c
}
#[test]
fn auto_strict_and_convenience_use_real_exception_with_original_main_head() {
    let image = Image::new(program(), true);
    for mode in [
        OperandExceptionMode::Auto,
        OperandExceptionMode::RequireOperandExceptions,
    ] {
        let run = image
            .run(
                ApplicationOperandExceptionProfile {
                    mode,
                    ..Default::default()
                },
                Default::default(),
            )
            .unwrap();
        assert_eq!(run.static_ints, [7]);
        let ApplicationCompletion::Uncaught {
            phase,
            class_name,
            location: Some(head),
        } = run.completion
        else {
            panic!("{:?}", run.completion)
        };
        assert_eq!(phase, EntryEvaluation::MainEvaluation);
        assert_eq!(class_name.escaped(), "java/lang/NegativeArraySizeException");
        assert_eq!(head.method_name.escaped(), "main");
        assert_eq!(head.offset, 6);
        assert_eq!(
            run.usage.vm_exceptions,
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
        assert_eq!(
            (
                run.usage.operand_diagnostics.work_units,
                run.usage.operand_diagnostics.message_units,
                run.usage.operand_diagnostics.messages
            ),
            (6, 2, 1)
        );
        assert!(run.usage.operand_exception_discovery.work_units > 0);
        assert_eq!(run.usage.engine_evaluations, 1);
    }
    let run = run_directory_application(
        ApplicationDirectories {
            platform: &image.boot,
            classes: &image.app,
        },
        "C",
        &[],
        Default::default(),
    )
    .unwrap();
    assert!(matches!(
        run.completion,
        ApplicationCompletion::Uncaught { .. }
    ));
    assert_eq!(run.usage.operand_diagnostics.messages, 1);
}
#[test]
fn explicit_old_throwable_wrapper_and_disabled_new_profile_do_not_expand_execution() {
    let image = Image::new(program(), true);
    let old = run_directory_application_with_throwables(
        ApplicationDirectories {
            platform: &image.boot,
            classes: &image.app,
        },
        "C",
        &[],
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let disabled = image
        .run(
            ApplicationOperandExceptionProfile {
                mode: OperandExceptionMode::Disabled,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap();
    for run in [old, disabled] {
        assert!(
            matches!(run.completion,ApplicationCompletion::ExecutionFailed{error,..}if error.kind==E::UnsupportedVmException(VmExceptionKind::NegativeArraySizeException))
        );
        assert_eq!(run.usage.operand_exception_discovery, Default::default());
        assert_eq!(run.usage.operand_diagnostics, Default::default());
        assert_eq!(run.usage.vm_exceptions, Default::default());
    }
}
#[test]
fn absent_throwable_plan_is_auto_fallback_but_strict_is_verification_before_operand_work() {
    let image = Image::new(program(), false);
    let run = image.run(Default::default(), Default::default()).unwrap();
    assert!(matches!(
        run.completion,
        ApplicationCompletion::ExecutionFailed { .. }
    ));
    assert_eq!(run.usage.operand_exception_discovery, Default::default());
    let e = image
        .run(
            ApplicationOperandExceptionProfile {
                mode: OperandExceptionMode::RequireOperandExceptions,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert!(
        matches!(e.kind,ApplicationErrorKind::OperandException(ref x) if x.kind==OperandExceptionErrorKind::UnsupportedShape)
    );
    assert_eq!(e.usage.operand_exception_discovery, Default::default());
    assert_eq!(e.usage.engine_evaluations, 0);
}
#[test]
fn matching_throwable_plan_missing_operand_role_is_fatal_even_in_auto() {
    let image = Image::new(program(), true);
    std::fs::remove_file(image.boot.join("java/lang/ArrayStoreException.class")).unwrap();
    for mode in [
        OperandExceptionMode::Auto,
        OperandExceptionMode::RequireOperandExceptions,
    ] {
        let e = image
            .run(
                ApplicationOperandExceptionProfile {
                    mode,
                    ..Default::default()
                },
                Default::default(),
            )
            .unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Verification);
        assert!(matches!(e.kind, ApplicationErrorKind::OperandException(_)));
        assert!(e.usage.operand_exception_discovery.work_units > 0);
        assert_eq!(e.usage.argument_arrays, 0);
        assert_eq!(e.usage.engine_evaluations, 0);
    }
}
#[test]
fn zero_discovery_work_never_falls_back_or_issues_arguments() {
    let image = Image::new(program(), true);
    let e = image
        .run(
            ApplicationOperandExceptionProfile {
                discovery: OperandExceptionDiscoveryLimits { max_work_units: 0 },
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert!(
        matches!(e.kind,ApplicationErrorKind::OperandException(ref x)if matches!(x.kind,OperandExceptionErrorKind::Limit{resource:OperandExceptionResource::DiscoveryWork,..}))
    );
    assert_eq!(e.usage.engine_evaluations, 0);
    assert_eq!(e.usage.argument_arrays, 0);
}
#[test]
fn producer_and_message_limits_keep_typed_origin_effect_and_usage_prefixes() {
    let image = Image::new(program(), true);
    for (producer, diag, started, work, phase) in [
        (
            0,
            100,
            0,
            0,
            crabspot_interpreter::VmMaterializationPhase::Preflight,
        ),
        (
            1,
            0,
            1,
            0,
            crabspot_interpreter::VmMaterializationPhase::Preflight,
        ),
        (
            1,
            5,
            1,
            4,
            crabspot_interpreter::VmMaterializationPhase::Message,
        ),
    ] {
        let mut p = ApplicationOperandExceptionProfile {
            diagnostics: OperandDiagnosticLimits {
                max_work_units: diag,
                ..Default::default()
            },
            ..Default::default()
        };
        p.throwables.vm_exceptions.max_materializations = producer;
        let run = image.run(p, Default::default()).unwrap();
        assert_eq!(run.static_ints, [7]);
        assert_eq!(
            run.usage.vm_exceptions,
            VmExceptionUsage {
                started,
                produced: 0
            }
        );
        assert_eq!(run.usage.operand_diagnostics.work_units, work);
        assert_eq!(run.usage.operand_diagnostics.message_units, 0);
        let ApplicationCompletion::ExecutionFailed { error, .. } = run.completion else {
            panic!()
        };
        assert_eq!(error.offset, Some(6));
        let vm = error.vm_exception.unwrap();
        assert_eq!(vm.origin.pc, 6);
        assert_eq!(vm.phase, phase);
    }
}
#[test]
fn output_quota_failure_retains_completed_operand_message_trace_and_producer_usage() {
    let image = Image::new(program(), true);
    let run = image.run(Default::default(), Default::default()).unwrap();
    let mut options = ApplicationOptions::default();
    options.image.max_utf16_units = run.usage.orchestration.utf16_units - 1;
    let e = image.run(Default::default(), options).unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Setup);
    assert!(matches!(e.kind, ApplicationErrorKind::ResourceLimit { .. }));
    assert_eq!(
        e.usage.vm_exceptions,
        VmExceptionUsage {
            started: 1,
            produced: 1
        }
    );
    assert_eq!(e.usage.operand_diagnostics.messages, 1);
    assert_eq!(e.usage.traces.backtraces, 1);
}
#[test]
fn only_enabled_operand_profile_adds_five_public_imports() {
    let mut c = program();
    c.class("java/lang/ArrayStoreException");
    let image = Image::new(c, true);
    assert!(image.run(Default::default(), Default::default()).is_ok());
    let e = image
        .run(
            ApplicationOperandExceptionProfile {
                mode: OperandExceptionMode::Disabled,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Dependencies);
    assert!(matches!(
        e.kind,
        ApplicationErrorKind::UnsupportedPlatformReference
    ));
}

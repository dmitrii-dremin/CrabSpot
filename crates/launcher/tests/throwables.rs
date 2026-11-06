//! Independent proved-image controls for the additive Throwable profile.
#[path = "../../interpreter/tests/support/throwable_world.rs"]
mod world;
use crabspot_interpreter::ExecutionErrorKind as E;
use crabspot_launcher::*;
use crabspot_runtime::{ThrowableErrorKind, VmExceptionKind};
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
            "throwable-image-{}-{}",
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
        profile: ApplicationThrowableProfile,
        options: ApplicationOptions,
    ) -> Result<ApplicationRun, Box<ApplicationError>> {
        run_directory_application_with_throwables(
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
    let field = c.reference(9, "C", "effect", "I");
    c.method(
        "main",
        "([Ljava/lang/String;)V",
        9,
        &[
            vec![0x10, 7],
            instruction(0xb3, field),
            vec![0x10, 9, 3, 0x6c, 0x57, 0xb1],
        ]
        .concat(),
        2,
        1,
    );
    c
}
#[test]
fn auto_and_strict_use_real_constructor_and_bounded_original_fault_head() {
    let image = Image::new(program(), true);
    for mode in [ThrowableMode::Auto, ThrowableMode::RequireThrowable] {
        let run = image
            .run(
                ApplicationThrowableProfile {
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
        assert_eq!(class_name.escaped(), ARITHMETIC);
        assert_eq!(head.class_name.escaped(), "C");
        assert_eq!(head.method_name.escaped(), "main");
        assert_eq!(head.descriptor.escaped(), "([Ljava/lang/String;)V");
        assert_eq!(head.offset, 8);
        assert_eq!(
            run.usage.vm_exceptions,
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
        assert_eq!(run.usage.traces.snapshots, 1);
        assert_eq!(run.usage.traces.backtraces, 1);
        assert_eq!(run.usage.traces.frames, 1);
        assert_eq!(run.usage.traces.work_units, 4);
        assert_eq!(run.usage.engine_evaluations, 1);
        assert_eq!(run.usage.argument_arrays, 1);
    }
}
#[test]
fn disabled_does_not_probe_or_materialize_and_absence_falls_back_only_in_auto() {
    for matched in [false, true] {
        let image = Image::new(program(), matched);
        let run = image
            .run(
                ApplicationThrowableProfile {
                    mode: ThrowableMode::Disabled,
                    ..Default::default()
                },
                Default::default(),
            )
            .unwrap();
        assert_eq!(run.static_ints, [7]);
        assert_eq!(run.usage.throwable_discovery.work_units, 0);
        assert_eq!(run.usage.traces, Default::default());
        assert_eq!(run.usage.vm_exceptions, Default::default());
        assert!(
            matches!(run.completion,ApplicationCompletion::ExecutionFailed{error,..}if error.kind==E::UnsupportedVmException(VmExceptionKind::ArithmeticException))
        );
    }
    let image = Image::new(program(), false);
    let run = image.run(Default::default(), Default::default()).unwrap();
    assert!(matches!(
        run.completion,
        ApplicationCompletion::ExecutionFailed { .. }
    ));
    assert!(run.usage.throwable_discovery.work_units > 0);
    assert_eq!(run.usage.vm_exceptions, Default::default());
    let error = image
        .run(
            ApplicationThrowableProfile {
                mode: ThrowableMode::RequireThrowable,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(error.stage, ApplicationStage::Verification);
    assert!(
        matches!(error.kind,ApplicationErrorKind::Throwable(ref e)if e.kind==ThrowableErrorKind::UnsupportedShape)
    );
    assert_eq!(error.usage.argument_arrays, 0);
}
#[test]
fn zero_discovery_budget_never_silently_falls_back_or_starts_engine() {
    let image = Image::new(program(), true);
    let error = image
        .run(
            ApplicationThrowableProfile {
                discovery: ThrowableDiscoveryLimits { max_work_units: 0 },
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(error.stage, ApplicationStage::Verification);
    assert_eq!(error.usage.throwable_discovery.work_units, 0);
    assert_eq!(error.usage.engine_evaluations, 0);
    assert_eq!(error.usage.argument_strings, 0);
    assert!(matches!(error.kind, ApplicationErrorKind::Throwable(_)));
}
#[test]
fn exact_vm_admission_failure_keeps_effects_and_typed_origin_context() {
    let image = Image::new(program(), true);
    let run = image
        .run(
            ApplicationThrowableProfile {
                vm_exceptions: VmExceptionLimits {
                    max_materializations: 0,
                },
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap();
    assert_eq!(run.static_ints, [7]);
    let ApplicationCompletion::ExecutionFailed { error, .. } = run.completion else {
        panic!()
    };
    assert_eq!(
        error.kind,
        E::VmExceptionLimit {
            limit: 0,
            required: 1
        }
    );
    assert_eq!(error.offset, Some(8));
    assert_eq!(
        error.vm_exception.unwrap().phase,
        crabspot_interpreter::VmMaterializationPhase::Preflight
    );
    assert_eq!(run.usage.vm_exceptions, Default::default());
    assert_eq!(run.usage.traces, Default::default());
}
#[test]
fn snapshot_failure_keeps_started_and_heap_trace_prefix_on_execution_report() {
    let image = Image::new(program(), true);
    let run = image
        .run(
            ApplicationThrowableProfile {
                traces: TraceLimits {
                    max_work_units: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap();
    assert_eq!(run.static_ints, [7]);
    assert_eq!(
        run.usage.vm_exceptions,
        VmExceptionUsage {
            started: 1,
            produced: 0
        }
    );
    assert_eq!(run.usage.traces.snapshots, 1);
    assert_eq!(run.usage.traces.frames, 1);
    assert_eq!(run.usage.traces.backtraces, 0);
    assert!(
        matches!(run.completion,ApplicationCompletion::ExecutionFailed{error,..}if matches!(error.kind,E::ThrowableAccess(_)))
    );
}
#[test]
fn output_quota_failure_preserves_completed_production_and_trace_usage() {
    let image = Image::new(program(), true);
    let run = image.run(Default::default(), Default::default()).unwrap();
    let mut options = ApplicationOptions::default();
    options.image.max_utf16_units = run.usage.orchestration.utf16_units - 1;
    let error = image.run(Default::default(), options).unwrap_err();
    assert_eq!(error.stage, ApplicationStage::Setup);
    assert!(matches!(
        error.kind,
        ApplicationErrorKind::ResourceLimit {
            resource: ApplicationResource::Utf16Units,
            ..
        }
    ));
    assert_eq!(
        error.usage.vm_exceptions,
        VmExceptionUsage {
            started: 1,
            produced: 1
        }
    );
    assert_eq!(error.usage.traces.backtraces, 1);
    assert_eq!(error.usage.engine_evaluations, 1);
}
#[test]
fn require_without_equality_is_zero_throwable_work_and_disabled_adds_no_imports() {
    let image = Image::new(program(), true);
    put(&image.boot, &recipes::compact_platform().remove(0));
    let error = image
        .run(
            ApplicationThrowableProfile {
                mode: ThrowableMode::RequireThrowable,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(error.stage, ApplicationStage::Verification);
    assert_eq!(error.usage.throwable_discovery.work_units, 0);
    assert!(matches!(error.kind, ApplicationErrorKind::Throwable(_)));
    let mut c = program();
    c.class("java/lang/ArithmeticException");
    let image = Image::new(c, true);
    let error = image
        .run(
            ApplicationThrowableProfile {
                mode: ThrowableMode::Disabled,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(error.stage, ApplicationStage::Dependencies);
    assert!(matches!(
        error.kind,
        ApplicationErrorKind::UnsupportedPlatformReference
    ));
    assert_eq!(error.usage.throwable_discovery.work_units, 0);
}
#[test]
fn ordinary_wrapper_deliberately_adopts_auto_without_new_input_struct_fields() {
    let image = Image::new(program(), true);
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
    assert_eq!(run.usage.vm_exceptions.produced, 1);
}

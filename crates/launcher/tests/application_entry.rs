//! Public one-shot image/CLI boundaries use independent synthetic bytes, not a JDK.
#[path = "../../runtime/tests/member_support/mod.rs"]
mod support;
use crabspot_interpreter::ExecutionErrorKind as E;
use crabspot_launcher::*;
use crabspot_runtime::VmExceptionKind;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use support::{Class, instruction};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Inputs {
    root: PathBuf,
    platform: PathBuf,
    application: PathBuf,
}
impl Inputs {
    fn new(classes: Vec<Class>) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "app-entry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let platform = root.join("platform");
        let application = root.join("application");
        std::fs::create_dir_all(&platform).unwrap();
        std::fs::create_dir_all(&application).unwrap();
        let mut object = Class::new("java/lang/Object", None, 0x21);
        object.method("<init>", "()V", 1, &[0xb1], 0, 1);
        write(&platform, &object);
        for name in [
            "java/lang/String",
            "java/lang/Class",
            "java/lang/invoke/MethodType",
            "java/lang/invoke/MethodHandle",
        ] {
            let mut class = Class::plain(name);
            if name == "java/lang/Class" {
                class.flags |= 0x10;
            }
            write(&platform, &class);
        }
        for name in ["java/lang/Cloneable", "java/io/Serializable"] {
            write(
                &platform,
                &Class::new(name, Some("java/lang/Object"), 0x601),
            );
        }
        for (name, parent) in [
            ("java/lang/Throwable", "java/lang/Object"),
            ("java/lang/Error", "java/lang/Throwable"),
        ] {
            let mut c = Class::new(name, Some(parent), 0x21);
            let init = c.reference(10, parent, "<init>", "()V");
            let mut body = vec![0x2a];
            body.extend(instruction(0xb7, init));
            body.push(0xb1);
            c.method("<init>", "()V", 1, &body, 1, 1);
            write(&platform, &c);
        }
        for c in classes {
            write(&application, &c);
        }
        Self {
            root,
            platform,
            application,
        }
    }
    fn dirs(&self) -> ApplicationDirectories<'_> {
        ApplicationDirectories {
            platform: &self.platform,
            classes: &self.application,
        }
    }
    fn run(
        &self,
        entry: &str,
        queries: &[StaticIntObservation<'_>],
        options: ApplicationOptions,
    ) -> Result<ApplicationRun, Box<ApplicationError>> {
        run_directory_application(self.dirs(), entry, queries, options)
    }
}
impl Drop for Inputs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn write(root: &Path, c: &Class) {
    let path = root.join(format!("{}.class", c.name));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, c.bytes()).unwrap();
}
fn plain_main() -> Class {
    let mut c = Class::plain("app/Main");
    c.field("value", "I", 9);
    c.field("slots", "I", 10);
    let value = c.reference(9, "app/Main", "value", "I");
    let slots = c.reference(9, "app/Main", "slots", "I");
    let mut code = vec![0x10, 37];
    code.extend(instruction(0xb3, value));
    code.extend([0x2a, 0xbe]);
    code.extend(instruction(0xb3, slots));
    code.push(0xb1);
    c.method("main", "([Ljava/lang/String;)V", 9, &code, 1, 1);
    c
}
fn query<'a>(owner: &'a str, field: &'a str) -> StaticIntObservation<'a> {
    StaticIntObservation {
        binary_class: owner,
        field,
    }
}
#[test]
fn real_array_no_string_class_constant_and_private_static_observation() {
    let inputs = Inputs::new(vec![plain_main()]);
    let data = std::fs::read(inputs.application.join("app/Main.class")).unwrap();
    let parsed = crabspot_classfile::parse(&data).unwrap();
    assert!(!parsed.constant_pool.entries().iter().any(|entry|matches!(entry,crabspot_classfile::Constant::Class{name_index}if parsed.constant_pool.utf8(*name_index).unwrap().equals_ascii("java/lang/String"))));
    let result = inputs
        .run(
            "app.Main",
            &[
                query("app.Main", "value"),
                query("app.Main", "slots"),
                query("app.Main", "value"),
            ],
            Default::default(),
        )
        .unwrap();
    assert!(matches!(result.completion, ApplicationCompletion::Returned));
    assert_eq!(result.static_ints, [37, 0, 37]);
    assert_eq!(result.usage.argument_arrays, 1);
    assert_eq!(result.usage.engine_evaluations, 1);
    assert_eq!(
        result.usage.proved_classes,
        result.usage.admitted_executable_classes
    );
    assert_eq!(
        result.usage.proved_classes,
        result.usage.preparation_witnesses
    );
}
#[test]
fn invalid_unused_application_body_fails_proof_before_heap_or_execution() {
    let mut invalid = Class::plain("app/Unused");
    invalid.method("defect", "()I", 9, &[3, 0xb0], 1, 0);
    let inputs = Inputs::new(vec![plain_main(), invalid]);
    let error = inputs.run("app.Main", &[], Default::default()).unwrap_err();
    assert_eq!(error.stage, ApplicationStage::Verification);
    assert!(matches!(error.kind, ApplicationErrorKind::Verification(_)));
    assert_eq!(error.usage.argument_arrays, 0);
    assert_eq!(error.usage.engine_evaluations, 0);
    assert_eq!(error.usage.admitted_executable_classes, 0);
}
#[test]
fn missing_or_wrong_flags_main_is_entry_failure_without_argument_allocation() {
    for flags in [1, 10, 0x109, 0x401] {
        let mut c = Class::plain("app/Main");
        if flags & 0x400 != 0 {
            c.flags = 0x421;
        }
        c.method("main", "([Ljava/lang/String;)V", flags, &[0xb1], 0, 2);
        let inputs = Inputs::new(vec![c]);
        let error = inputs.run("app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(error.stage, ApplicationStage::Entry);
        assert_eq!(error.usage.argument_arrays, 0);
        assert_eq!(error.usage.engine_evaluations, 0);
    }
}
#[test]
fn inherited_only_observation_is_rejected_without_initialization() {
    let inputs = Inputs::new(vec![
        plain_main(),
        Class::new("app/Child", Some("app/Main"), 0x21),
    ]);
    let e = inputs
        .run(
            "app.Main",
            &[query("app.Child", "value")],
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Observations);
    assert_eq!(e.observation, Some(0));
    assert_eq!(e.usage.argument_arrays, 0);
    assert_eq!(e.usage.engine_evaluations, 0);
}
#[test]
fn observation_does_not_activate_unused_class() {
    let mut unused = Class::plain("app/Unused");
    unused.field("value", "I", 9);
    let field = unused.reference(9, "app/Unused", "value", "I");
    let mut init = vec![0x10, 99];
    init.extend(instruction(0xb3, field));
    init.push(0xb1);
    unused.method("<clinit>", "()V", 8, &init, 1, 0);
    let inputs = Inputs::new(vec![plain_main(), unused]);
    let r = inputs
        .run(
            "app.Main",
            &[query("app.Unused", "value")],
            Default::default(),
        )
        .unwrap();
    assert!(matches!(r.completion, ApplicationCompletion::Returned));
    assert_eq!(r.static_ints, [0]);
}
#[test]
fn root_preflight_and_main_vm_failure_are_distinct_and_retain_observations() {
    let inputs = Inputs::new(vec![plain_main()]);
    let mut options = ApplicationOptions::default();
    options.calls.max_frames = 0;
    let r = inputs
        .run("app.Main", &[query("app.Main", "value")], options)
        .unwrap();
    assert!(
        matches!(r.completion,ApplicationCompletion::ExecutionFailed{phase:EntryEvaluation::RootPreflight,error}if matches!(error.kind,E::FrameLimit{..}))
    );
    assert_eq!(r.static_ints, [0]);
    assert_eq!(r.usage.argument_arrays, 1);
    assert_eq!(r.usage.engine_evaluations, 1);
    let mut c = Class::plain("app/Abrupt");
    c.field("value", "I", 9);
    let f = c.reference(9, "app/Abrupt", "value", "I");
    let mut body = vec![0x10, 31];
    body.extend(instruction(0xb3, f));
    body.extend([1, 0xbf]);
    c.method("main", "([Ljava/lang/String;)V", 9, &body, 1, 1);
    let inputs = Inputs::new(vec![c]);
    let r = inputs
        .run(
            "app.Abrupt",
            &[query("app.Abrupt", "value")],
            Default::default(),
        )
        .unwrap();
    assert!(
        matches!(r.completion,ApplicationCompletion::ExecutionFailed{phase:EntryEvaluation::MainEvaluation,error}if error.kind==E::UnsupportedVmException(VmExceptionKind::NullPointerException))
    );
    assert_eq!(r.static_ints, [31]);
}
#[test]
fn preview_option_invalid_name_and_query_syntax_are_input_failures_before_io() {
    let absent = Path::new("this-directory-is-not-opened");
    for which in 0..3 {
        let mut options = ApplicationOptions::default();
        if which == 0 {
            options.parse.allow_preview = true;
        }
        let query_bad = [query("app.Main", "bad/name")];
        let result = run_directory_application(
            ApplicationDirectories {
                platform: absent,
                classes: absent,
            },
            if which == 1 { "app/Bad" } else { "app.Main" },
            if which == 2 { &query_bad } else { &[] },
            options,
        );
        assert_eq!(result.unwrap_err().stage, ApplicationStage::Input);
    }
}

fn edged_inputs() -> Inputs {
    let mut c = Class::plain("app/Main");
    let sm = c.utf("StackMapTable");
    c.method(
        "main",
        "([Ljava/lang/String;)V",
        9,
        &[0x2a, 0xbe, 0x99, 0, 5, 1, 0xbf, 0xb1],
        1,
        1,
    );
    let inputs = Inputs::new(vec![c]);
    let path = inputs.application.join("app/Main.class");
    let mut bytes = std::fs::read(&path).unwrap();
    let parsed = crabspot_classfile::parse(&bytes).unwrap();
    let offset = parsed.methods[0].attributes[0].offset;
    let length = u32::from_be_bytes(bytes[offset + 2..offset + 6].try_into().unwrap());
    let end = offset + 6 + length as usize;
    bytes[end - 2..end].copy_from_slice(&1_u16.to_be_bytes());
    let mut nested = sm.to_be_bytes().to_vec();
    nested.extend(3_u32.to_be_bytes());
    nested.extend([0, 1, 7]);
    bytes.splice(end..end, nested);
    bytes[offset + 2..offset + 6].copy_from_slice(&(length + 9).to_be_bytes());
    std::fs::write(path, bytes).unwrap();
    inputs
}
#[test]
fn all_ten_image_verification_counters_are_inclusive_and_one_less_is_typed() {
    let inputs = edged_inputs();
    let baseline = inputs.run("app.Main", &[], Default::default()).unwrap();
    let u = baseline.usage.verification;
    let t = u.verification;
    let c = t.code;
    let counts = [
        u.methods_started,
        c.code_bytes,
        c.metadata_entries,
        c.utf16_units,
        c.work_units,
        t.type_symbols,
        t.state_cells,
        t.edge_checks,
        t.utf16_units,
        t.work_units,
    ];
    for (which, n) in counts.into_iter().enumerate() {
        assert!(n > 0, "counter{which}");
        for short in [false, true] {
            let mut o = ApplicationOptions::default();
            let value = n - u64::from(short);
            let total = &mut o.verification_total;
            match which {
                0 => total.max_methods = value,
                1 => total.max_code_bytes = value,
                2 => total.max_code_metadata_entries = value,
                3 => total.max_code_utf16_units = value,
                4 => total.max_code_work_units = value,
                5 => total.max_type_symbols = value,
                6 => total.max_state_cells = value,
                7 => total.max_edge_checks = value,
                8 => total.max_type_utf16_units = value,
                _ => total.max_type_work_units = value,
            };
            let result = inputs.run("app.Main", &[], o);
            if short {
                let e = result.unwrap_err();
                assert_eq!(e.stage, ApplicationStage::Verification);
                assert!(matches!(
                    e.kind,
                    ApplicationErrorKind::VerificationLimit { .. }
                ));
                assert_eq!(e.usage.argument_arrays, 0);
                assert_eq!(e.usage.engine_evaluations, 0);
            } else {
                let r = result.unwrap();
                assert!(matches!(r.completion, ApplicationCompletion::Returned));
                assert_eq!(r.usage.verification, baseline.usage.verification);
            }
        }
    }
}
#[test]
fn direct_class_cap_wins_equal_image_remaining_cap_without_reclassification() {
    let inputs = Inputs::new(vec![plain_main()]);
    let mut o = ApplicationOptions::default();
    o.class.total.max_methods = 0;
    o.verification_total.max_methods = 0;
    let e = inputs.run("app.Main", &[], o).unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert!(matches!(e.kind, ApplicationErrorKind::Verification(_)));
    assert_eq!(e.usage.engine_evaluations, 0);
}
#[test]
fn source_inventory_definition_and_byte_caps_have_exact_inclusive_boundaries() {
    let inputs = Inputs::new(vec![plain_main()]);
    let r = inputs.run("app.Main", &[], Default::default()).unwrap();
    let u = r.usage;
    let limits = [
        u.platform.ordinary_definitions + u.application.ordinary_definitions,
        u.platform.class_bytes + u.application.class_bytes,
        u.platform.entries + u.application.entries,
        u.platform.path_units + u.application.path_units,
    ];
    for (which, n) in limits.into_iter().enumerate() {
        for short in [false, true] {
            let mut o = ApplicationOptions::default();
            let value = n - u64::from(short);
            match which {
                0 => o.image.max_definitions = value,
                1 => o.image.max_input_bytes = value,
                2 => o.image.max_entries = value,
                _ => o.image.max_total_path_units = value,
            };
            let result = inputs.run("app.Main", &[], o);
            if short {
                let e = result.unwrap_err();
                assert!(matches!(e.kind, ApplicationErrorKind::ResourceLimit { .. }));
                assert_eq!(e.usage.engine_evaluations, 0);
            } else {
                assert!(matches!(
                    result.unwrap().completion,
                    ApplicationCompletion::Returned
                ));
            }
        }
    }
}
#[test]
fn orchestration_utf_work_and_scratch_caps_are_inclusive_and_max_does_not_preallocate() {
    let inputs = Inputs::new(vec![plain_main()]);
    let queries = [query("app.Main", "value")];
    let r = inputs
        .run("app.Main", &queries, Default::default())
        .unwrap();
    let usage = r.usage.orchestration;
    for (which, n) in [usage.utf16_units, usage.scratch_entries, usage.work_units]
        .into_iter()
        .enumerate()
    {
        for short in [false, true] {
            let mut o = ApplicationOptions::default();
            let value = n - u64::from(short);
            match which {
                0 => o.image.max_utf16_units = value,
                1 => o.image.max_scratch_entries = value,
                _ => o.image.max_inventory_work = value,
            };
            let result = inputs.run("app.Main", &queries, o);
            if short {
                assert!(matches!(
                    result.unwrap_err().kind,
                    ApplicationErrorKind::ResourceLimit { .. }
                ));
            } else {
                assert_eq!(result.unwrap().usage.orchestration, usage);
            }
        }
    }
    let mut o = ApplicationOptions::default();
    o.image.max_definitions = u64::MAX;
    o.image.max_input_bytes = u64::MAX;
    o.image.max_entries = u64::MAX;
    o.image.max_inventory_work = u64::MAX;
    o.image.max_scratch_entries = u64::MAX;
    o.image.max_utf16_units = u64::MAX;
    assert!(matches!(
        inputs.run("app.Main", &[], o).unwrap().completion,
        ApplicationCompletion::Returned
    ));
}
#[test]
fn empty_inventory_and_observation_boundaries_have_no_execution_effects() {
    let inputs = Inputs::new(vec![]);
    std::fs::remove_dir_all(&inputs.platform).unwrap();
    std::fs::create_dir(&inputs.platform).unwrap();
    let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
    assert_eq!(e.usage.platform.entries, 0);
    assert_eq!(e.usage.application.entries, 0);
    assert_eq!(e.usage.platform.class_bytes, 0);
    assert_eq!(e.usage.application.class_bytes, 0);
    assert_eq!(e.usage.argument_arrays, 0);
    assert_eq!(e.usage.engine_evaluations, 0);
    let inputs = Inputs::new(vec![plain_main()]);
    let q = [query("app.Main", "value"), query("app.Main", "slots")];
    for cap in [0, 1, 2] {
        let mut o = ApplicationOptions::default();
        o.image.max_observations = cap;
        let r = inputs.run("app.Main", &q, o);
        if cap < 2 {
            let e = r.unwrap_err();
            assert_eq!(e.stage, ApplicationStage::Input);
            assert_eq!(e.usage.engine_evaluations, 0);
        } else {
            assert_eq!(r.unwrap().static_ints, [37, 0]);
        }
    }
}

#[test]
fn cli_runs_only_explicit_inputs_and_preserves_legacy_inspect_exit_status() {
    use std::process::Command;
    let inputs = Inputs::new(vec![plain_main()]);
    let exe = env!("CARGO_BIN_EXE_crabspot");
    for reverse in [false, true] {
        let mut cmd = Command::new(exe);
        cmd.arg("run");
        if reverse {
            cmd.arg("--classes")
                .arg(&inputs.application)
                .arg("--platform")
                .arg(&inputs.platform);
        } else {
            cmd.arg("--platform")
                .arg(&inputs.platform)
                .arg("--classes")
                .arg(&inputs.application);
        }
        let output = cmd
            .arg("app.Main")
            .env("JAVA_HOME", "must-not-be-consulted")
            .env("CLASSPATH", "must-not-be-consulted")
            .env_remove("PATH")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    for args in [vec!["inspect"], vec!["inspect", "one.class", "two.class"]] {
        let o = Command::new(exe).args(args).output().unwrap();
        assert_eq!(o.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&o.stderr).contains("usage:"));
    }
    for args in [
        vec!["run"],
        vec!["run", "--platform", "x", "app.Main"],
        vec!["run", "--platform", "x", "--platform", "y", "app.Main"],
        vec![
            "run",
            "--classes",
            "x",
            "--platform",
            "y",
            "app.Main",
            "extra",
        ],
        vec!["unknown"],
    ] {
        let o = Command::new(exe).args(args).output().unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&o.stderr).contains("usage:"));
    }
    let output = Command::new(exe)
        .args(["run", "--platform"])
        .arg(&inputs.platform)
        .arg("--classes")
        .arg(&inputs.application)
        .arg("app/Main")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Input"));
}
#[test]
fn missing_and_closed_platform_cp_references_are_eager_without_fake_loads() {
    for present in [false, true] {
        let mut c = plain_main();
        let index = c.class(if present {
            "closed/Fact"
        } else {
            "app/Missing"
        });
        let inputs = Inputs::new(vec![c]);
        if present {
            write(&inputs.platform, &Class::plain("closed/Fact"));
        }
        let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Dependencies);
        assert_eq!(e.cp_index, Some(index));
        assert!(matches!(
            (present, e.kind),
            (true, ApplicationErrorKind::UnsupportedPlatformReference)
                | (false, ApplicationErrorKind::MissingApplicationDependency)
        ));
        assert_eq!(e.usage.engine_evaluations, 0);
        assert_eq!(e.usage.argument_arrays, 0);
    }
}
#[test]
fn fact_only_target_cannot_gain_execution_authority_from_a_platform_caller() {
    let mut app = Class::plain("app/Main");
    app.field("value", "I", 9);
    let field = app.reference(9, "app/Main", "value", "I");
    let call = app.reference(10, "java/lang/String", "call", "()V");
    let mut body = vec![0x10, 31];
    body.extend(instruction(0xb3, field));
    body.extend(instruction(0xb8, call));
    body.extend([0x10, 51]);
    body.extend(instruction(0xb3, field));
    body.push(0xb1);
    app.method("main", "([Ljava/lang/String;)V", 9, &body, 1, 1);
    let inputs = Inputs::new(vec![app]);
    let mut string = Class::plain("java/lang/String");
    let hidden = string.reference(10, "closed/Fact", "call", "()V");
    let mut body = instruction(0xb8, hidden);
    body.push(0xb1);
    string.method("call", "()V", 9, &body, 0, 0);
    write(&inputs.platform, &string);
    let mut fact = Class::plain("closed/Fact");
    fact.method("call", "()V", 9, &[0xb1], 0, 0);
    fact.method("invalidButUnproved", "()I", 9, &[3, 0xb0], 1, 0);
    write(&inputs.platform, &fact);
    let result = inputs
        .run(
            "app.Main",
            &[query("app.Main", "value")],
            Default::default(),
        )
        .unwrap();
    assert!(matches!(
        result.completion,
        ApplicationCompletion::ExecutionFailed {
            phase: EntryEvaluation::MainEvaluation,
            ..
        }
    ));
    assert_eq!(result.static_ints, [31]);
    assert_eq!(result.usage.engine_evaluations, 1);
    assert_eq!(
        result.usage.admitted_executable_classes,
        result.usage.proved_classes
    );
}

#[test]
fn source_name_format_and_version_failures_are_definition_errors() {
    for which in 0..4 {
        let inputs = Inputs::new(vec![plain_main()]);
        let path = inputs.application.join("app/Main.class");
        match which {
            0 => std::fs::rename(&path, inputs.application.join("app/Wrong.class")).unwrap(),
            1 => std::fs::write(&path, [0xca, 0xfe, 0xba, 0xbe]).unwrap(),
            _ => {
                let mut data = std::fs::read(&path).unwrap();
                if which == 2 {
                    data[6..8].copy_from_slice(&50_u16.to_be_bytes());
                } else {
                    data[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
                }
                std::fs::write(&path, data).unwrap();
            }
        }
        let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Definition);
        assert_eq!(e.usage.engine_evaluations, 0);
        assert_eq!(e.usage.argument_arrays, 0);
    }
}
#[test]
fn application_cannot_shadow_bootstrap_names_or_platform_namespace() {
    for java in [false, true] {
        let inputs = Inputs::new(vec![plain_main()]);
        let name = if java {
            "java/lang/String"
        } else {
            "closed/Own"
        };
        if !java {
            write(&inputs.platform, &Class::plain(name));
        }
        write(&inputs.application, &Class::plain(name));
        let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Definition);
        assert!(matches!(
            (java, e.kind),
            (true, ApplicationErrorKind::ForbiddenApplicationName)
                | (false, ApplicationErrorKind::NamespaceConflict)
        ));
        assert_eq!(e.usage.engine_evaluations, 0);
    }
}
#[test]
fn overlapping_or_parent_traversing_roots_fail_before_definition() {
    let inputs = Inputs::new(vec![plain_main()]);
    for which in 0..3 {
        let parent = inputs.application.join("..").join("application");
        let dirs = match which {
            0 => ApplicationDirectories {
                platform: &inputs.platform,
                classes: &inputs.platform,
            },
            1 => ApplicationDirectories {
                platform: &inputs.root,
                classes: &inputs.application,
            },
            _ => ApplicationDirectories {
                platform: &inputs.platform,
                classes: &parent,
            },
        };
        let e = run_directory_application(dirs, "app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Input);
        assert_eq!(e.usage.application.ordinary_definitions, 0);
        assert_eq!(e.usage.platform.ordinary_definitions, 0);
    }
}
#[test]
fn hierarchy_cycles_and_final_parents_fail_before_proof_and_execution() {
    for cycle in [false, true] {
        let mut main = Class::new("app/Main", Some("app/Parent"), 0x21);
        main.method("main", "([Ljava/lang/String;)V", 9, &[0xb1], 0, 1);
        let parent = Class::new(
            "app/Parent",
            Some(if cycle {
                "app/Main"
            } else {
                "java/lang/Object"
            }),
            if cycle { 0x21 } else { 0x31 },
        );
        let inputs = Inputs::new(vec![main, parent]);
        let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Hierarchy);
        assert_eq!(e.usage.engine_evaluations, 0);
        assert_eq!(e.usage.argument_arrays, 0);
    }
}
#[test]
fn constructor_initializer_and_selectable_method_bodies_all_require_proof() {
    for which in 0..3 {
        let mut bad = Class::plain("app/Unused");
        let mut classes = vec![plain_main()];
        match which {
            0 => bad.method("<init>", "()V", 1, &[0xb1], 0, 1),
            1 => bad.method("<clinit>", "()V", 8, &[3, 0xac], 1, 0),
            _ => {
                let mut it = Class::new("app/Calc", Some("java/lang/Object"), 0x601);
                it.method("apply", "()I", 0x401, &[], 0, 1);
                classes.push(it);
                bad.interface("app/Calc");
                bad.method("apply", "()I", 1, &[1, 0xac], 1, 1);
            }
        }
        classes.push(bad);
        let inputs = Inputs::new(classes);
        let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Verification);
        assert_eq!(e.usage.engine_evaluations, 0);
        assert_eq!(e.usage.admitted_executable_classes, 0);
    }
}
#[test]
fn field_query_uses_unqualified_grammar_not_filesystem_component_grammar() {
    let mut c = plain_main();
    c.field("CON", "I", 9);
    c.field("<x>", "I", 10);
    let inputs = Inputs::new(vec![c]);
    let r = inputs
        .run(
            "app.Main",
            &[query("app.Main", "CON"), query("app.Main", "<x>")],
            Default::default(),
        )
        .unwrap();
    assert_eq!(r.static_ints, [0, 0]);
}
#[test]
fn imported_boot_class_is_not_an_application_observation_owner() {
    let inputs = Inputs::new(vec![plain_main()]);
    let mut string = Class::plain("java/lang/String");
    string.field("value", "I", 9);
    write(&inputs.platform, &string);
    let e = inputs
        .run(
            "app.Main",
            &[query("java.lang.String", "value")],
            Default::default(),
        )
        .unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Observations);
    assert_eq!(e.usage.argument_arrays, 0);
    assert_eq!(e.usage.engine_evaluations, 0);
}
#[cfg(unix)]
#[test]
fn unix_symlink_and_non_utf8_entries_are_rejected_without_following() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    for badname in [false, true] {
        let inputs = Inputs::new(vec![plain_main()]);
        if badname {
            let path = inputs
                .application
                .join(std::ffi::OsString::from_vec(vec![0xff]));
            std::fs::write(path, [0]).unwrap();
        } else {
            symlink(&inputs.platform, inputs.application.join("link")).unwrap();
        }
        let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
        assert_eq!(e.stage, ApplicationStage::Input);
        assert_eq!(e.usage.engine_evaluations, 0);
    }
}

#[test]
fn synchronized_main_is_enabled_and_monitor_entry_exit_limits_preserve_effects_and_phase() {
    for steps in [0, 1, 2] {
        let mut c = Class::plain("app/Main");
        c.field("value", "I", 9);
        let field = c.reference(9, "app/Main", "value", "I");
        let mut init = vec![0x10, 7];
        init.extend(instruction(0xb3, field));
        init.push(0xb1);
        c.method("<clinit>", "()V", 0x28, &init, 1, 0);
        let mut body = vec![0x10, 9];
        body.extend(instruction(0xb3, field));
        body.push(0xb1);
        c.method("main", "([Ljava/lang/String;)V", 0x29, &body, 1, 1);
        let inputs = Inputs::new(vec![c]);
        let result = run_directory_application_with_monitor_limits(
            inputs.dirs(),
            "app.Main",
            &[query("app.Main", "value")],
            Default::default(),
            MonitorLimits {
                max_steps: steps,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.static_ints, [if steps == 0 { 7 } else { 9 }]);
        assert_eq!(result.usage.engine_evaluations, 1);
        assert_eq!(result.usage.argument_arrays, 1);
        if steps == 2 {
            assert!(matches!(result.completion, ApplicationCompletion::Returned));
        } else {
            let ApplicationCompletion::ExecutionFailed { phase, error } = result.completion else {
                panic!("typed execution failure required")
            };
            assert_eq!(phase, EntryEvaluation::MainEvaluation);
            assert!(
                matches!(error.kind,E::Monitor(crabspot_interpreter::MonitorError::Limit{resource:crabspot_interpreter::MonitorResource::Steps,limit,required})if limit==steps&&required==steps+1)
            );
            assert_eq!(error.offset, if steps == 0 { None } else { Some(5) });
        }
    }
}
#[test]
fn default_library_and_physical_cli_run_synchronized_main_with_actual_argument_array_lock() {
    let mut c = Class::plain("app/Main");
    c.field("value", "I", 9);
    let value = c.reference(9, "app/Main", "value", "I");
    let mut body = vec![0x2a, 0xc2, 0x10, 23];
    body.extend(instruction(0xb3, value));
    body.extend([0x2a, 0xc3, 0xb1]);
    c.method("main", "([Ljava/lang/String;)V", 0x29, &body, 1, 1);
    let inputs = Inputs::new(vec![c]);
    let result = inputs
        .run(
            "app.Main",
            &[query("app.Main", "value")],
            Default::default(),
        )
        .unwrap();
    assert!(matches!(result.completion, ApplicationCompletion::Returned));
    assert_eq!(result.static_ints, [23]);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_crabspot"))
        .args(["run", "--platform"])
        .arg(&inputs.platform)
        .arg("--classes")
        .arg(&inputs.application)
        .arg("app.Main")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

fn install_object_native(inputs: &Inputs, java_replacement: bool) {
    let mut object = Class::new("java/lang/Object", None, 0x21);
    object.method("<init>", "()V", 1, &[0xb1], 0, 1);
    object.method(
        "getClass",
        "()Ljava/lang/Class;",
        if java_replacement { 0x11 } else { 0x111 },
        if java_replacement { &[1, 0xb0] } else { &[] },
        1,
        1,
    );
    write(&inputs.platform, &object);
}
#[test]
fn descriptor_free_literals_use_the_universal_class_import_and_exact_mirror_site_caps() {
    let mut c = Class::plain("app/Main");
    c.field("value", "I", 9);
    let value = c.reference(9, "app/Main", "value", "I");
    let literal = c.class("app/Main");
    let mut code = vec![0x10, 7];
    code.extend(instruction(0xb3, value));
    code.extend(
        [
            vec![
                0x12,
                literal as u8,
                0x57,
                0x12,
                literal as u8,
                0x57,
                0x10,
                9,
            ],
            instruction(0xb3, value),
            vec![0xb1],
        ]
        .concat(),
    );
    c.method("main", "([Ljava/lang/String;)V", 9, &code, 1, 1);
    let inputs = Inputs::new(vec![c]);
    let data = std::fs::read(inputs.application.join("app/Main.class")).unwrap();
    let parsed = crabspot_classfile::parse(&data).unwrap();
    assert!(!parsed.constant_pool.entries().iter().any(
        |e| matches!(e,crabspot_classfile::Constant::Utf8(v)if v.equals_ascii("java/lang/Class"))
    ));
    for limits in [
        ClassMirrorLimits {
            max_mirrors: 1,
            max_class_sites: 1,
        },
        ClassMirrorLimits {
            max_mirrors: 0,
            max_class_sites: 1,
        },
        ClassMirrorLimits {
            max_mirrors: 1,
            max_class_sites: 0,
        },
        ClassMirrorLimits {
            max_mirrors: u64::MAX,
            max_class_sites: u64::MAX,
        },
    ] {
        let r = run_directory_application_with_mirror_limits(
            inputs.dirs(),
            "app.Main",
            &[query("app.Main", "value")],
            Default::default(),
            Default::default(),
            limits,
        )
        .unwrap();
        assert_eq!(r.usage.proved_classes, 8);
        assert_eq!(r.usage.proved_classes, r.usage.admitted_executable_classes);
        if limits.max_mirrors == 0 || limits.max_class_sites == 0 {
            assert_eq!(r.static_ints, [7]);
            let ApplicationCompletion::ExecutionFailed { phase, error } = r.completion else {
                panic!("typed quota failure required")
            };
            assert_eq!(phase, EntryEvaluation::MainEvaluation);
            let resource = if limits.max_class_sites == 0 {
                crabspot_runtime::ClassMirrorResource::ClassSites
            } else {
                crabspot_runtime::ClassMirrorResource::Mirrors
            };
            assert_eq!(
                error.kind,
                E::ClassMirror(Box::new(crabspot_runtime::ClassMirrorError::Limit {
                    resource,
                    limit: 0,
                    required: 1
                }))
            );
            assert_eq!(error.offset, Some(5));
        } else {
            assert_eq!(r.static_ints, [9]);
            assert!(matches!(r.completion, ApplicationCompletion::Returned));
        }
    }
}
#[test]
fn mirror_uses_instance_quota_and_invalid_class_binding_fails_before_argument_allocation() {
    let mut c = Class::plain("app/Main");
    c.field("value", "I", 9);
    let literal = c.class("app/Main");
    c.method(
        "main",
        "([Ljava/lang/String;)V",
        9,
        &[vec![0x12, literal as u8, 0x57, 0xb1]].concat(),
        1,
        1,
    );
    let inputs = Inputs::new(vec![c]);
    let mut options = ApplicationOptions::default();
    options.instance.max_instances = 0;
    let r = run_directory_application(inputs.dirs(), "app.Main", &[], options).unwrap();
    let ApplicationCompletion::ExecutionFailed { error, .. } = r.completion else {
        panic!("instance quota required")
    };
    assert!(
        matches!(error.kind,E::ClassMirror(ref e)if matches!(**e,crabspot_runtime::ClassMirrorError::Instance(crabspot_runtime::InstanceError::ResourceLimit{resource:crabspot_runtime::InstanceResource::Instances,limit:0,required:1})))
    );
    write(&inputs.platform, &Class::plain("java/lang/Class"));
    let e = inputs.run("app.Main", &[], Default::default()).unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Setup);
    assert!(matches!(
        e.kind,
        ApplicationErrorKind::Setup {
            kind: SetupKind::ClassMirror,
            ..
        }
    ));
    assert_eq!(e.usage.argument_arrays, 0);
    assert_eq!(e.usage.engine_evaluations, 0);
}
#[test]
fn public_native_result_shares_static_lock_and_physical_cli_needs_no_class_library_initializer() {
    let mut c = Class::plain("app/Main");
    c.field("value", "I", 9);
    let value = c.reference(9, "app/Main", "value", "I");
    let parent = c.reference(10, "java/lang/Object", "<init>", "()V");
    c.method(
        "<init>",
        "()V",
        1,
        &[vec![0x2a], instruction(0xb7, parent), vec![0xb1]].concat(),
        1,
        1,
    );
    let own_init = c.reference(10, "app/Main", "<init>", "()V");
    let own_class = c.class("app/Main");
    let get = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
    let m = c.reference(10, "app/Main", "m", "()V");
    c.method(
        "m",
        "()V",
        0x29,
        &[vec![0x10, 9], instruction(0xb3, value), vec![0xb1]].concat(),
        1,
        0,
    );
    let mut body = vec![0x10, 7];
    body.extend(instruction(0xb3, value));
    body.extend(instruction(0xbb, own_class));
    body.push(0x59);
    body.extend(instruction(0xb7, own_init));
    body.extend(instruction(0xb6, get));
    body.extend([0x59, 0x4c, 0xc2]);
    let invoke = body.len() as u32;
    body.extend(instruction(0xb8, m));
    body.extend([0x2b, 0xc3, 0xb1]);
    c.method("main", "([Ljava/lang/String;)V", 9, &body, 2, 2);
    let inputs = Inputs::new(vec![c]);
    install_object_native(&inputs, false);
    for short in [false, true] {
        let r = run_directory_application_with_mirror_limits(
            inputs.dirs(),
            "app.Main",
            &[query("app.Main", "value")],
            Default::default(),
            MonitorLimits {
                max_recursion: if short { 1 } else { 2 },
                max_active_monitors: 1,
                max_active_claims: 2,
                max_steps: 4,
            },
            ClassMirrorLimits {
                max_mirrors: 1,
                max_class_sites: 0,
            },
        )
        .unwrap();
        assert_eq!(r.static_ints, [if short { 7 } else { 9 }]);
        if short {
            let ApplicationCompletion::ExecutionFailed { phase, error } = r.completion else {
                panic!("shared key quota failure expected")
            };
            assert_eq!(phase, EntryEvaluation::MainEvaluation);
            assert_eq!(error.offset, Some(invoke));
            assert!(matches!(
                error.kind,
                E::Monitor(crabspot_interpreter::MonitorError::Limit {
                    resource: crabspot_interpreter::MonitorResource::Recursion,
                    limit: 1,
                    required: 2
                })
            ));
        } else {
            assert!(matches!(r.completion, ApplicationCompletion::Returned));
        }
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_crabspot"))
        .args(["run", "--platform"])
        .arg(&inputs.platform)
        .arg("--classes")
        .arg(&inputs.application)
        .arg("app.Main")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
#[test]
fn verified_object_typed_array_helper_accepts_markers_and_refuses_nonimplemented_interface() {
    for owner in ["java/lang/Cloneable", "java/io/Serializable", "app/Face"] {
        let mut c = Class::plain("app/Main");
        c.field("value", "I", 9);
        let value = c.reference(9, "app/Main", "value", "I");
        let target = c.reference(11, owner, "getClass", "()Ljava/lang/Class;");
        let helper = c.reference(
            10,
            "app/Main",
            "helper",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
        );
        c.method(
            "helper",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            9,
            &[vec![0x2a], instruction(0xb9, target), vec![1, 0, 0xb0]].concat(),
            1,
            1,
        );
        let class = c.class("java/lang/Class");
        let mut main = vec![0x10, 7];
        main.extend(instruction(0xb3, value));
        main.extend([3, 0xbc, 10]);
        main.extend(instruction(0xb8, helper));
        main.extend(instruction(0xc1, class));
        main.extend(instruction(0xb3, value));
        main.push(0xb1);
        c.method("main", "([Ljava/lang/String;)V", 9, &main, 1, 1);
        let inputs = Inputs::new(vec![
            c,
            Class::new("app/Face", Some("java/lang/Object"), 0x601),
        ]);
        install_object_native(&inputs, false);
        let r = inputs
            .run(
                "app.Main",
                &[query("app.Main", "value")],
                Default::default(),
            )
            .unwrap();
        if owner == "app/Face" {
            assert_eq!(r.static_ints, [7]);
            let ApplicationCompletion::ExecutionFailed { phase, error } = r.completion else {
                panic!("ICCE required")
            };
            assert_eq!(phase, EntryEvaluation::MainEvaluation);
            assert_eq!(
                error.kind,
                E::UnsupportedVmException(VmExceptionKind::IncompatibleClassChangeError)
            );
            assert_eq!(error.offset, Some(1));
            assert_eq!(error.method.as_ref().unwrap().name.escaped(), "helper");
        } else {
            assert_eq!(r.static_ints, [1]);
            assert!(matches!(r.completion, ApplicationCompletion::Returned));
        }
    }
}
#[test]
fn unused_missing_getclass_is_allowed_but_exact_java_replacement_cannot_run_as_native() {
    let mut c = Class::plain("app/Main");
    c.field("value", "I", 9);
    let target = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
    c.method(
        "main",
        "([Ljava/lang/String;)V",
        9,
        &[vec![0x2a], instruction(0xb6, target), vec![0x57, 0xb1]].concat(),
        1,
        1,
    );
    let inputs = Inputs::new(vec![c]);
    install_object_native(&inputs, true);
    let r = inputs.run("app.Main", &[], Default::default()).unwrap();
    let ApplicationCompletion::ExecutionFailed { error, .. } = r.completion else {
        panic!("invalid binding required")
    };
    assert!(
        matches!(error.kind,E::ClassMirror(ref e)if matches!(**e,crabspot_runtime::ClassMirrorError::InvalidBinding(_)))
    );
    assert_eq!(error.offset, Some(1));
}

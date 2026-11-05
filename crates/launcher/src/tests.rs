use super::*;
use crate::{image, meter::Meter, source};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Tree {
    root: PathBuf,
    pub(crate) boot: PathBuf,
    app: PathBuf,
}
impl Tree {
    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "crabspot-image-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let boot = root.join("boot");
        let app = root.join("app");
        fs::create_dir(&boot).unwrap();
        fs::create_dir(&app).unwrap();
        Self { root, boot, app }
    }
    pub(crate) fn put(&self, boot: bool, name: &str, bytes: &[u8]) {
        let p = if boot { &self.boot } else { &self.app }.join(format!("{name}.class"));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap()
    }
    fn dirs(&self) -> ApplicationDirectories<'_> {
        ApplicationDirectories {
            platform: &self.boot,
            classes: &self.app,
        }
    }
    fn core(&self) {
        for (name, parent, flags) in [
            ("java/lang/Object", None, 0x21),
            ("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
            ("java/io/Serializable", Some("java/lang/Object"), 0x601),
            ("java/lang/Throwable", Some("java/lang/Object"), 0x21),
            ("java/lang/Error", Some("java/lang/Throwable"), 0x21),
            ("java/lang/String", Some("java/lang/Object"), 0x31),
            ("java/lang/Class", Some("java/lang/Object"), 0x31),
            (
                "java/lang/invoke/MethodType",
                Some("java/lang/Object"),
                0x31,
            ),
            (
                "java/lang/invoke/MethodHandle",
                Some("java/lang/Object"),
                0x421,
            ),
        ] {
            self.put(true, name, &definition(name, parent, flags, false, false))
        }
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        if self.root.starts_with(std::env::temp_dir()) {
            fs::remove_dir_all(&self.root).unwrap()
        }
    }
}
fn words(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|n| n.to_be_bytes()).collect()
}
pub(crate) fn definition(
    name: &str,
    parent: Option<&str>,
    flags: u16,
    main: bool,
    bad: bool,
) -> Vec<u8> {
    let mut pool = Vec::<Vec<u8>>::new();
    let utf = |pool: &mut Vec<Vec<u8>>, s: &str| {
        let mut v = vec![1];
        v.extend(words(&[s.len() as u16]));
        v.extend(s.bytes());
        pool.push(v);
        pool.len() as u16
    };
    let n = utf(&mut pool, name);
    pool.push([vec![7], words(&[n])].concat());
    let this = pool.len() as u16;
    let parent = parent
        .map(|s| {
            let n = utf(&mut pool, s);
            pool.push([vec![7], words(&[n])].concat());
            pool.len() as u16
        })
        .unwrap_or(0);
    let method = utf(&mut pool, "main");
    let desc = utf(&mut pool, "([Ljava/lang/String;)V");
    let code = utf(&mut pool, "Code");
    let mut b = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    b.extend(words(&[(pool.len() + 1) as u16]));
    b.extend(pool.into_iter().flatten());
    b.extend(words(&[flags, this, parent, 0, 0, u16::from(main)]));
    if main {
        b.extend(words(&[9, method, desc, 1, code]));
        let body = if bad { vec![3, 0xb0] } else { vec![0xb1] };
        b.extend(((12 + body.len()) as u32).to_be_bytes());
        b.extend(words(&[1, 1]));
        b.extend((body.len() as u32).to_be_bytes());
        b.extend(body);
        b.extend(words(&[0, 0]));
    }
    b.extend(words(&[0]));
    b
}
fn prepared(t: &Tree, o: ApplicationOptions) -> image::PreparedImage {
    image::prepare(t.dirs(), "app.A", &[], o).unwrap()
}
#[test]
fn sealed_subset_proves_every_application_and_excludes_fact_only_layouts() {
    let t = Tree::new();
    t.core();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    let p = prepared(&t, Default::default());
    assert_eq!(p.usage.proved_classes, 7);
    assert_eq!(p.usage.preparation_witnesses, 7);
    assert_eq!(p.heap.retained_layout_classes(), 7);
    assert_eq!(p.domain.registry().definition_count(), 10);
    assert_eq!(p.heap.allocated_arrays(), 0);
    assert_eq!(p.heap.allocated_instances(), 0);
    assert_eq!(p.usage.verification.code_bodies, 1);
}
#[test]
fn invalid_unused_sibling_never_publishes_an_executable_image() {
    let t = Tree::new();
    t.core();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    t.put(
        false,
        "app/Z",
        &definition("app/Z", Some("java/lang/Object"), 0x21, true, true),
    );
    let e = image::prepare(t.dirs(), "app.A", &[], Default::default())
        .err()
        .unwrap();
    assert_eq!(e.stage, ApplicationStage::Verification);
    assert!(e.usage.proved_classes > 0);
    assert_eq!(e.usage.admitted_executable_classes, 0);
    assert_eq!(e.usage.preparation_witnesses, 0);
    assert!(matches!(e.kind, ApplicationErrorKind::Verification(_)));
}
#[test]
fn input_validation_and_preview_precede_any_io() {
    let t = Tree::new();
    let missing = t.root.join("absent");
    let dirs = || ApplicationDirectories {
        platform: &missing,
        classes: &t.app,
    };
    for bad in ["", "a..B", "a/B", "a\\B", "[I", "a.CON", "a.B;"] {
        assert_eq!(
            image::prepare(dirs(), bad, &[], Default::default())
                .err()
                .unwrap()
                .stage,
            ApplicationStage::Input
        )
    }
    let mut o = ApplicationOptions::default();
    o.parse.allow_preview = true;
    assert!(matches!(
        image::prepare(dirs(), "a.B", &[], o).err().unwrap().kind,
        ApplicationErrorKind::InvalidInput(_)
    ));
}
#[test]
fn portable_components_and_ascii_case_collisions_are_rejected() {
    for s in [
        "CON.txt",
        "com1.class",
        "AUX",
        "a.",
        "a ",
        "a:b",
        "a?b",
        "\u{7f}",
        ".",
        "..",
    ] {
        assert!(!source::component(s), "{s}")
    }
    assert!(source::component("package-info.class"));
    assert!(source::component("A$B.class"));
    #[cfg(not(windows))]
    {
        let t = Tree::new();
        fs::write(t.app.join("A.class"), []).unwrap();
        fs::write(t.app.join("a.CLASS"), []).unwrap();
        let mut m = Meter::new(ImageLimits::default());
        let mut u = ApplicationUsage::default();
        assert!(matches!(
            source::inventory(&t.app, InputRole::Application, &mut m, &mut u)
                .err()
                .unwrap()
                .kind,
            ApplicationErrorKind::DuplicatePath
        ));
    }
}
#[test]
fn root_overlap_and_parent_components_fail_without_reading_bytes() {
    let t = Tree::new();
    let mut m = Meter::new(Default::default());
    for classes in [&t.boot, &t.root] {
        assert!(matches!(
            source::roots(
                ApplicationDirectories {
                    platform: &t.boot,
                    classes
                },
                &mut m,
                Default::default()
            )
            .err()
            .unwrap()
            .kind,
            ApplicationErrorKind::OverlappingRoots
        ));
    }
    let parent = t.root.join("app/../boot");
    assert!(
        source::roots(
            ApplicationDirectories {
                platform: &parent,
                classes: &t.app
            },
            &mut m,
            Default::default()
        )
        .is_err()
    );
}
#[test]
fn ignored_entries_still_pay_paths_depth_entries_and_sort_cost() {
    let t = Tree::new();
    fs::create_dir(t.app.join("nested")).unwrap();
    fs::write(t.app.join("nested/readme.txt"), b"ignored").unwrap();
    let mut m = Meter::new(Default::default());
    let mut u = ApplicationUsage::default();
    assert!(
        source::inventory(&t.app, InputRole::Application, &mut m, &mut u)
            .unwrap()
            .is_empty()
    );
    assert_eq!(u.application.entries, 2);
    assert_eq!(u.application.path_units, 6 + 17);
    assert!(m.usage.inventory_work > 0);
    for (r, limits) in [
        (
            ApplicationResource::Entries,
            ImageLimits {
                max_entries: 1,
                ..Default::default()
            },
        ),
        (
            ApplicationResource::Depth,
            ImageLimits {
                max_depth: 1,
                ..Default::default()
            },
        ),
        (
            ApplicationResource::TotalPathUnits,
            ImageLimits {
                max_total_path_units: 6,
                ..Default::default()
            },
        ),
    ] {
        let mut m = Meter::new(limits);
        let e = source::inventory(
            &t.app,
            InputRole::Application,
            &mut m,
            &mut ApplicationUsage::default(),
        )
        .err()
        .unwrap();
        assert!(matches!(e.kind,ApplicationErrorKind::ResourceLimit{resource,..}if resource==r));
    }
}
#[test]
fn source_file_and_aggregate_byte_caps_precede_checked_admission() {
    let t = Tree::new();
    fs::write(t.app.join("A.class"), [1, 2, 3]).unwrap();
    for limits in [
        ImageLimits {
            max_file_bytes: 2,
            ..Default::default()
        },
        ImageLimits {
            max_input_bytes: 2,
            ..Default::default()
        },
    ] {
        let mut m = Meter::new(limits);
        let mut u = ApplicationUsage::default();
        let leaves = source::inventory(&t.app, InputRole::Application, &mut m, &mut u).unwrap();
        let e = source::read(&leaves[0], InputRole::Application, &m, &mut u).unwrap_err();
        assert!(matches!(e.kind, ApplicationErrorKind::ResourceLimit { .. }));
        assert_eq!(u.application.class_bytes, 0)
    }
    let mut m = Meter::new(Default::default());
    let mut u = ApplicationUsage::default();
    let leaves = source::inventory(&t.app, InputRole::Application, &mut m, &mut u).unwrap();
    assert_eq!(
        source::read(&leaves[0], InputRole::Application, &m, &mut u).unwrap(),
        [1, 2, 3]
    );
    assert_eq!(u.application.class_bytes, 3);
}
#[test]
fn missing_role_wrong_declared_name_and_namespace_collision_keep_distinct_errors() {
    let t = Tree::new();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    assert!(matches!(
        image::prepare(t.dirs(), "app.A", &[], Default::default())
            .err()
            .unwrap()
            .kind,
        ApplicationErrorKind::MissingRole(_)
    ));
    t.core();
    t.put(
        false,
        "app/A",
        &definition("wrong/A", Some("java/lang/Object"), 0x21, true, false),
    );
    assert!(matches!(
        image::prepare(t.dirs(), "app.A", &[], Default::default())
            .err()
            .unwrap()
            .kind,
        ApplicationErrorKind::Loading(_)
    ));
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    t.put(
        true,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    assert!(matches!(
        image::prepare(t.dirs(), "app.A", &[], Default::default())
            .err()
            .unwrap()
            .kind,
        ApplicationErrorKind::NamespaceConflict
    ));
}
#[test]
fn image_proof_ceiling_is_distinct_from_unchanged_child_policy() {
    let t = Tree::new();
    t.core();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    let mut o = ApplicationOptions::default();
    o.verification_total.max_code_bytes = 0;
    let e = image::prepare(t.dirs(), "app.A", &[], o).err().unwrap();
    assert!(matches!(
        e.kind,
        ApplicationErrorKind::VerificationLimit {
            limit: 0,
            required: 1,
            ..
        }
    ));
    assert_eq!(e.usage.verification.methods_started, 1);
    assert_eq!(e.usage.verification.verification.code.code_bytes, 0);
    let mut o = ApplicationOptions::default();
    o.class.total.max_code_bytes = 0;
    let e = image::prepare(t.dirs(), "app.A", &[], o).err().unwrap();
    assert!(matches!(e.kind, ApplicationErrorKind::Verification(_)));
}
#[test]
fn atomic_meter_batches_and_max_policy_do_not_reserve_policy_size() {
    let mut m = Meter::new(ImageLimits {
        max_scratch_entries: 0,
        ..Default::default()
    });
    assert!(
        m.charge(1, 1, 1, ApplicationStage::Input, Default::default())
            .is_err()
    );
    assert_eq!(m.usage, OrchestrationUsage::default());
    m.limits.max_scratch_entries = u64::MAX;
    m.usage.scratch_entries = u64::MAX;
    assert!(matches!(
        m.retain(1, ApplicationStage::Input, Default::default())
            .unwrap_err()
            .kind,
        ApplicationErrorKind::AccountingOverflow {
            resource: ApplicationResource::ScratchEntries
        }
    ));
    let t = Tree::new();
    let mut m = Meter::new(ImageLimits {
        max_entries: u64::MAX,
        max_scratch_entries: u64::MAX,
        max_inventory_work: u64::MAX,
        ..Default::default()
    });
    let mut u = ApplicationUsage::default();
    assert!(
        source::inventory(&t.app, InputRole::Application, &mut m, &mut u)
            .unwrap()
            .is_empty()
    );
    assert_eq!(m.usage.inventory_work, 0);
}

#[test]
#[ignore = "explicit genuine directory preparation; no installed JDK required by ordinary tests"]
fn genuine_directory_preparation_evidence() {
    let boot = std::env::var_os("CRABSPOT_APPLICATION_BOOT").unwrap();
    let app = std::env::var_os("CRABSPOT_APPLICATION_CLASSES").unwrap();
    let p = image::prepare(
        ApplicationDirectories {
            platform: std::path::Path::new(&boot),
            classes: std::path::Path::new(&app),
        },
        "app.Entry",
        &[],
        Default::default(),
    )
    .unwrap();
    println!("GENUINE PREPARED {:?}", p.meter.snapshot(p.usage));
    assert_eq!(p.usage.application.ordinary_definitions, 6);
    assert_eq!(p.usage.proved_classes, p.usage.preparation_witnesses);
}
#[test]
fn application_inventory_error_precedes_malformed_platform_admission() {
    let t = Tree::new();
    fs::write(t.boot.join("broken.class"), [0]).unwrap();
    fs::write(t.app.join("ignored.txt"), []).unwrap();
    let mut o = ApplicationOptions::default();
    o.image.max_entries = 1;
    let e = image::prepare(t.dirs(), "app.A", &[], o).err().unwrap();
    assert_eq!(e.stage, ApplicationStage::Input);
    assert_eq!(e.role, Some(InputRole::Application));
    assert_eq!(e.usage.platform.class_files, 0);
    assert!(matches!(
        e.kind,
        ApplicationErrorKind::ResourceLimit {
            resource: ApplicationResource::Entries,
            ..
        }
    ));
}
#[test]
fn path_quota_is_checked_before_retaining_leaf_or_copying_parent() {
    let t = Tree::new();
    fs::write(t.app.join("long.class"), []).unwrap();
    let mut m = Meter::new(ImageLimits {
        max_path_units: 1,
        max_scratch_entries: 1,
        ..Default::default()
    });
    let mut u = ApplicationUsage::default();
    let e = source::inventory(&t.app, InputRole::Application, &mut m, &mut u)
        .err()
        .unwrap();
    assert!(matches!(
        e.kind,
        ApplicationErrorKind::ResourceLimit {
            resource: ApplicationResource::PathUnits,
            ..
        }
    ));
    assert_eq!(m.usage.scratch_entries, 1);
    assert_eq!(u.application.path_units, 0);
}
#[test]
fn missing_or_closed_platform_cp_and_cycle_fail_before_verification() {
    let t = Tree::new();
    t.core();
    for (parent, expected) in [("missing/Base", 0), ("java/lang/Class", 1), ("app/A", 2)] {
        t.put(
            false,
            "app/A",
            &definition("app/A", Some(parent), 0x21, true, false),
        );
        let e = image::prepare(t.dirs(), "app.A", &[], Default::default())
            .err()
            .unwrap();
        assert_eq!(e.usage.proved_classes, 0);
        match expected {
            0 => assert!(matches!(
                e.kind,
                ApplicationErrorKind::MissingApplicationDependency
            )),
            1 => assert!(matches!(
                e.kind,
                ApplicationErrorKind::UnsupportedPlatformReference
            )),
            _ => assert_eq!(e.stage, ApplicationStage::Hierarchy),
        }
    }
}
#[test]
fn aggregate_plan_limit_never_publishes_partial_child_rows_or_edges() {
    let t = Tree::new();
    t.core();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    for rows in [true, false] {
        let mut o = ApplicationOptions::default();
        if rows {
            o.environment.max_class_rows = 1
        } else {
            o.environment.max_edges = 0
        }
        let e = image::prepare(t.dirs(), "app.A", &[], o).err().unwrap();
        assert_eq!(e.stage, ApplicationStage::Hierarchy);
        assert_eq!(e.usage.physical_plan_rows, 0);
        assert_eq!(e.usage.physical_plan_edges, 0);
        assert_eq!(e.usage.proved_classes, 0);
    }
}
#[test]
fn exact_image_proof_budget_passes_one_less_fails_with_child_source() {
    use std::error::Error;
    let t = Tree::new();
    t.core();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    let p = prepared(&t, Default::default());
    let amount = p.usage.verification.verification.code.work_units;
    assert!(amount > 0);
    let mut o = ApplicationOptions::default();
    o.verification_total.max_code_work_units = amount;
    assert_eq!(
        prepared(&t, o)
            .usage
            .verification
            .verification
            .code
            .work_units,
        amount
    );
    o.verification_total.max_code_work_units = amount - 1;
    let e = image::prepare(t.dirs(), "app.A", &[], o).err().unwrap();
    assert!(matches!(
        e.kind,
        ApplicationErrorKind::VerificationLimit { .. }
    ));
    assert!(
        e.source()
            .unwrap()
            .downcast_ref::<crabspot_verifier::ClassTypeVerificationError>()
            .is_some()
    );
    assert_eq!(e.usage.admitted_executable_classes, 0);
}
#[test]
fn max_small_image_policy_and_all_boundaries_use_actual_additions() {
    let t = Tree::new();
    t.core();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    let o = ApplicationOptions {
        image: ImageLimits {
            max_definitions: u64::MAX,
            max_input_bytes: u64::MAX,
            max_file_bytes: u64::MAX,
            max_entries: u64::MAX,
            max_depth: u64::MAX,
            max_path_units: u64::MAX,
            max_total_path_units: u64::MAX,
            max_observations: u64::MAX,
            max_inventory_work: u64::MAX,
            max_scratch_entries: u64::MAX,
            max_utf16_units: u64::MAX,
        },
        ..Default::default()
    };
    assert_eq!(prepared(&t, o).usage.proved_classes, 7);
}
#[test]
fn huge_semantic_input_is_rejected_before_width_walk_or_io() {
    let t = Tree::new();
    let text = "x".repeat(4096);
    let mut o = ApplicationOptions::default();
    o.image.max_inventory_work = 8;
    let e = image::prepare(t.dirs(), &text, &[], o).err().unwrap();
    assert!(matches!(
        e.kind,
        ApplicationErrorKind::ResourceLimit {
            resource: ApplicationResource::WorkUnits,
            ..
        }
    ));
    assert_eq!(e.usage.orchestration.utf16_units, 0);
    assert_eq!(e.usage.platform.entries, 0);
}
#[cfg(unix)]
#[test]
fn unix_symlink_special_and_nonunicode_root_are_refused() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let t = Tree::new();
    symlink(&t.boot, t.app.join("linked")).unwrap();
    let mut m = Meter::new(Default::default());
    assert!(
        source::inventory(
            &t.app,
            InputRole::Application,
            &mut m,
            &mut Default::default()
        )
        .is_err()
    );
    let invalid = t.root.join(std::ffi::OsString::from_vec(vec![0xff]));
    let e = source::roots(
        ApplicationDirectories {
            platform: &invalid,
            classes: &t.app,
        },
        &mut m,
        Default::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(e.kind, ApplicationErrorKind::InvalidPath(_)));
}
fn field_definition(name: &str, field: &str) -> Vec<u8> {
    let b = definition(name, Some("java/lang/Object"), 0x21, true, false);
    let mut p = 10;
    for _ in 1..8 {
        match b[p] {
            1 => {
                let n = u16::from_be_bytes([b[p + 1], b[p + 2]]) as usize;
                p += 3 + n
            }
            7 => p += 3,
            _ => panic!("fixture tag"),
        }
    }
    let mut out = b[..p].to_vec();
    out[8..10].copy_from_slice(&10u16.to_be_bytes());
    for s in [field, "I"] {
        out.push(1);
        out.extend(words(&[s.len() as u16]));
        out.extend(s.bytes())
    }
    out.extend(&b[p..p + 8]);
    out.extend(words(&[1, 8, 8, 9, 0]));
    out.extend(&b[p + 10..]);
    out
}
#[test]
fn ordinary_field_names_and_repeated_app_observations_preserve_exact_identity() {
    let t = Tree::new();
    t.core();
    for field in ["CON", "<x>"] {
        t.put(false, "app/A", &field_definition("app/A", field));
        let queries = [
            StaticIntObservation {
                binary_class: "app.A",
                field,
            },
            StaticIntObservation {
                binary_class: "app.A",
                field,
            },
        ];
        let p = image::prepare(t.dirs(), "app.A", &queries, Default::default()).unwrap();
        let mut meter = p.meter;
        let fields = image::resolve_observations(
            &p.routes,
            p.domain.registry(),
            p.app_loader,
            &queries,
            &mut meter,
            p.usage,
        )
        .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], fields[1]);
        let report =
            run_directory_application(t.dirs(), "app.A", &queries, Default::default()).unwrap();
        assert!(matches!(report.completion, ApplicationCompletion::Returned));
        assert_eq!(report.static_ints, [0, 0]);
    }
}
#[test]
fn platform_delegation_does_not_authorize_an_observation() {
    let t = Tree::new();
    t.core();
    t.put(
        true,
        "java/lang/String",
        &field_definition("java/lang/String", "value"),
    );
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    let query = [StaticIntObservation {
        binary_class: "java.lang.String",
        field: "value",
    }];
    let e = run_directory_application(t.dirs(), "app.A", &query, Default::default()).unwrap_err();
    assert_eq!(e.stage, ApplicationStage::Observations);
    assert_eq!(e.observation, Some(0));
    assert!(matches!(e.kind, ApplicationErrorKind::InvalidObservation));
    assert_eq!(e.usage.argument_arrays, 0);
    assert_eq!(e.usage.engine_evaluations, 0);
}
#[test]
fn image_and_class_limit_tie_preserves_the_child_failure() {
    let t = Tree::new();
    t.core();
    t.put(
        false,
        "app/A",
        &definition("app/A", Some("java/lang/Object"), 0x21, true, false),
    );
    let mut o = ApplicationOptions::default();
    o.class.total.max_methods = 0;
    o.verification_total.max_methods = 0;
    let e = image::prepare(t.dirs(), "app.A", &[], o).err().unwrap();
    assert!(matches!(e.kind, ApplicationErrorKind::Verification(_)));
}
#[test]
#[ignore = "explicit public API genuine-directory execution evidence"]
fn genuine_public_entry_evidence() {
    let boot = std::env::var_os("CRABSPOT_APPLICATION_BOOT").unwrap();
    let app = std::env::var_os("CRABSPOT_APPLICATION_CLASSES").unwrap();
    let fields = ["entered", "completed", "total", "initialized", "slots"];
    let queries: Vec<_> = fields
        .iter()
        .map(|field| StaticIntObservation {
            binary_class: "app.Marks",
            field,
        })
        .collect();
    let report = run_directory_application(
        ApplicationDirectories {
            platform: std::path::Path::new(&boot),
            classes: std::path::Path::new(&app),
        },
        "app.Entry",
        &queries,
        Default::default(),
    )
    .unwrap();
    println!("GENUINE RUN {:?}", report);
    assert!(matches!(report.completion, ApplicationCompletion::Returned));
    assert_eq!(report.static_ints, [1, 1, 37, 1, 0]);
    assert_eq!(report.usage.argument_arrays, 1);
    assert_eq!(report.usage.engine_evaluations, 1);
}

//! Independent checked nest tests, separate from String recipe granting.
#[path = "support/string_body_class.rs"]
mod support;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use support::{Class, Graph, instruction};
#[derive(Clone, Copy)]
enum Shape {
    Valid,
    NoAHost,
    NoBHost,
    NoMembers,
    OmitA,
    OmitB,
    Different,
    ForeignPackage,
    InaccessibleHost,
    Unrelated,
    ArrayHost,
    LongList,
}
struct W {
    g: Graph,
    h: ManagedHeap,
    a: ClassId,
    b: ClassId,
    host: ClassId,
    field: u16,
    method: u16,
    own: u16,
}
fn build(shape: Shape, nests: bool, admit_host: bool, sites: bool) -> W {
    let mut g = Graph::new();
    let l = g.loader();
    let host_name = if matches!(shape, Shape::ForeignPackage | Shape::InaccessibleHost) {
        "q/H"
    } else {
        "p/H"
    };
    let mut host = Class::new(
        host_name,
        Some("java/lang/Object"),
        if matches!(shape, Shape::InaccessibleHost) {
            0x20
        } else {
            0x21
        },
    );
    host.field("secret", "I", 2);
    match shape {
        Shape::NoMembers => {}
        Shape::OmitA => host.nest_members(&["p/B"]),
        Shape::OmitB => host.nest_members(&["p/A"]),
        Shape::Unrelated => host.nest_members(&["absent/C", "p/A", "absent/D", "p/B"]),
        Shape::LongList => host.long_nest_members(65_533, &["p/A", "p/B"]),
        _ => host.nest_members(&["p/A", "p/B"]),
    }
    let host = g.add(l, host);
    let mut j = Class::plain("p/J");
    j.nest_members(&["p/H", "p/B"]);
    g.add(l, j);
    let mut a = Class::plain("p/A");
    a.field("secret", "I", 2);
    a.method("secret", "()I", 2, &[4, 0xac], 1, 1);
    let own = a.reference(9, "p/A", "secret", "I");
    if !matches!(shape, Shape::NoAHost) {
        a.nest_host(host_name);
    }
    let a = g.add(l, a);
    let mut b = Class::plain("p/B");
    let field = b.reference(9, "p/A", "secret", "I");
    let method = b.reference(10, "p/A", "secret", "()I");
    if !matches!(shape, Shape::NoBHost) {
        b.nest_host(if matches!(shape, Shape::Different) {
            "p/J"
        } else if matches!(shape, Shape::ArrayHost) {
            "[Lp/H;"
        } else {
            host_name
        });
    }
    b.method(
        "invoke",
        "(Lp/A;)I",
        1,
        &[vec![0x2b], instruction(0xb6, method), vec![0xac]].concat(),
        1,
        2,
    );
    let b = g.add(l, b);
    g.enable(Default::default());
    if nests {
        g.domain.enable_nest_access().unwrap();
    }
    let ids = g.classes.clone();
    g.prepare(&ids);
    let ids = ids
        .into_iter()
        .filter(|id| admit_host || *id != host)
        .collect::<Vec<_>>();
    let mut h = g.heap(&ids);
    g.bind_core(&mut h);
    if sites {
        g.domain
            .enable_class_mirror_sites(ClassMirrorLimits::default().max_class_sites)
            .unwrap();
    }
    W {
        g,
        h,
        a,
        b,
        host,
        field,
        method,
        own,
    }
}
fn context(w: &mut W) -> FieldResolutionContext<'_> {
    w.g.domain
        .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
        .unwrap()
}
#[test]
fn independent_nine_nominal_shapes_match_private_field_and_method_access() {
    for (shape, allowed) in [
        (Shape::Valid, true),
        (Shape::NoAHost, false),
        (Shape::NoBHost, false),
        (Shape::NoMembers, false),
        (Shape::OmitA, false),
        (Shape::OmitB, false),
        (Shape::Different, false),
        (Shape::ForeignPackage, false),
        (Shape::Unrelated, true),
        (Shape::ArrayHost, false),
        (Shape::InaccessibleHost, false),
    ] {
        let mut w = build(shape, true, true, true);
        let a = w.a;
        let b = w.b;
        let f = w.field;
        let m = w.method;
        let r = w.h.allocate_instance(a).unwrap();
        let receiver = w.h.allocate_instance(b).unwrap();
        let invoke = w.g.method(b, "invoke", "(Lp/A;)I");
        let mut c = context(&mut w);
        let field = c.read_field(b, f, r);
        if allowed {
            assert_eq!(field.unwrap(), Value::Int(0));
            c.write_field(b, f, r, Value::Int(7)).unwrap();
            assert_eq!(c.read_field(b, f, r).unwrap(), Value::Int(7));
            assert_eq!(
                execute_instance_method_body(
                    &mut c,
                    invoke,
                    receiver,
                    &[Value::Reference(r)],
                    Default::default(),
                    Default::default()
                )
                .unwrap(),
                ExecutionOutcome::Returned(Some(Value::Int(1)))
            );
            assert_eq!(c.select_virtual(b, m, r).unwrap().owner(), a);
        } else {
            assert!(matches!(
                field.unwrap_err(),
                FieldAccessError::Java(FieldJavaError::IllegalAccess)
            ));
            assert!(matches!(
                c.select_virtual(b, m, r),
                Err(MethodAccessError::Java(MethodJavaError::IllegalAccess))
            ));
        }
    }
}
#[test]
fn mutually_exclusive_host_members_are_parser_errors_and_same_class_access_is_immediate() {
    let mut invalid = Class::plain("p/H");
    invalid.nest_host("p/J");
    invalid.nest_members(&["p/A"]);
    let error = crabspot_classfile::parse(&invalid.bytes()).unwrap_err();
    assert!(matches!(
        error.kind,
        crabspot_classfile::ErrorKind::InvalidStructure(_)
    ));
    let mut w = build(Shape::Valid, true, true, true);
    let a = w.a;
    let own = w.own;
    let r = w.h.allocate_instance(a).unwrap();
    let before = w.g.domain.class_site_count();
    assert_eq!(
        context(&mut w).read_field(a, own, r).unwrap(),
        Value::Int(0)
    );
    assert_eq!(w.g.domain.class_site_count(), before);
}
#[test]
fn successful_and_failed_terminal_sites_do_not_rewalk_nests_on_warm_use() {
    for shape in [Shape::Valid, Shape::OmitA] {
        let mut w = build(shape, true, true, true);
        let a = w.a;
        let b = w.b;
        let f = w.field;
        let r = w.h.allocate_instance(a).unwrap();
        let mut c = context(&mut w);
        let first = c.read_field(b, f, r);
        let before = c.lookup_steps();
        let work = c.member_loading_work().unwrap();
        let second = c.read_field(b, f, r);
        let after = c.member_loading_work().unwrap();
        assert_eq!(first, second);
        assert_eq!(c.lookup_steps() - before, usize::from(first.is_ok()));
        assert_eq!(after.work_units, work.work_units + 1);
        assert_eq!(after.utf16_units, work.utf16_units);
        assert_eq!(after.scratch_entries, work.scratch_entries);
    }
}
#[test]
fn missing_host_layout_is_retryable_but_certain_java_denial_is_terminal() {
    let mut w = build(Shape::Valid, true, false, true);
    let a = w.a;
    let b = w.b;
    let f = w.field;
    let r = w.h.allocate_instance(a).unwrap();
    let error = context(&mut w).read_field(b, f, r).unwrap_err();
    assert!(!matches!(error, FieldAccessError::Java(_)));
    assert_eq!(w.g.domain.member_site(b, f).unwrap(), None);
    let plan = w.g.plan(w.host);
    w.h.admit_instance_layouts(w.g.domain.registry(), &[&plan])
        .unwrap();
    assert_eq!(context(&mut w).read_field(b, f, r).unwrap(), Value::Int(0));
    assert!(matches!(
        w.g.domain.member_site(b, f).unwrap(),
        Some(MemberResolutionOutcome::Field(_))
    ));
    let mut denied = build(Shape::InaccessibleHost, true, true, true);
    let a = denied.a;
    let b = denied.b;
    let f = denied.field;
    let r = denied.h.allocate_instance(a).unwrap();
    assert!(matches!(
        context(&mut denied).read_field(b, f, r),
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    ));
    assert_eq!(
        denied.g.domain.member_site(b, f).unwrap(),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::IllegalAccess
        ))
    );
}
#[test]
fn class_site_prerequisite_is_not_downgraded_to_java_access_denial() {
    let mut w = build(Shape::Valid, true, true, false);
    let b = w.b;
    let f = w.field;
    let r = w.h.allocate_instance(w.a).unwrap();
    let error = context(&mut w).read_field(b, f, r).unwrap_err();
    assert!(matches!(error, FieldAccessError::ClassMirror(_)));
    assert_eq!(w.g.domain.member_site(b, f).unwrap(), None);
    w.g.domain
        .enable_class_mirror_sites(Default::default())
        .unwrap();
    // Enabling zero slots supplies the capability but still cannot resolve a host.
    let error = context(&mut w).read_field(b, f, r).unwrap_err();
    assert!(matches!(error, FieldAccessError::ClassMirror(_)));
    assert_eq!(w.g.domain.member_site(b, f).unwrap(), None);
}
#[test]
fn legacy_boundary_and_late_enable_refusal_preserve_prior_sites() {
    let mut w = build(Shape::Valid, false, true, true);
    let b = w.b;
    let f = w.field;
    let r = w.h.allocate_instance(w.a).unwrap();
    assert!(matches!(
        context(&mut w).read_field(b, f, r),
        Err(FieldAccessError::UnsupportedNestAccess { .. })
    ));
    assert_eq!(w.g.domain.member_site(b, f).unwrap(), None);
    w.g.domain.enable_nest_access().unwrap();
    assert_eq!(context(&mut w).read_field(b, f, r).unwrap(), Value::Int(0));
    w.g.domain.enable_nest_access().unwrap();
    let mut w = build(Shape::Valid, false, true, true);
    let a = w.a;
    let own = w.own;
    let r = w.h.allocate_instance(a).unwrap();
    context(&mut w).read_field(a, own, r).unwrap();
    assert!(w.g.domain.enable_nest_access().is_err());
    assert!(matches!(
        w.g.domain.member_site(a, own).unwrap(),
        Some(MemberResolutionOutcome::Field(_))
    ));
}
#[test]
fn maximal_nest_list_is_funded_and_unrelated_names_are_never_loaded() {
    let mut w = build(Shape::LongList, true, true, true);
    let b = w.b;
    let f = w.field;
    let r = w.h.allocate_instance(w.a).unwrap();
    let mut c =
        w.g.domain
            .member_context_unnamed_modules(
                &mut w.h,
                FieldResolutionOptions {
                    max_lookup_steps: 5_000,
                    ..Default::default()
                },
                Default::default(),
            )
            .unwrap();
    let error = c.read_field(b, f, r).unwrap_err();
    assert!(!matches!(error, FieldAccessError::Java(_)));
    assert!(c.lookup_steps() <= 5_000);
    assert!(c.lookup_steps() > 1_000);
    drop(c);
    assert_eq!(w.g.domain.member_site(b, f).unwrap(), None);
    let mut c =
        w.g.domain
            .member_context_unnamed_modules(
                &mut w.h,
                FieldResolutionOptions {
                    max_lookup_steps: 3_000_000,
                    ..Default::default()
                },
                Default::default(),
            )
            .unwrap();
    assert_eq!(c.read_field(b, f, r).unwrap(), Value::Int(0));
    assert!(c.lookup_steps() > 65_535);
}
#[test]
fn access_denial_precedes_null_receiver_for_definite_non_nestmates() {
    let mut w = build(Shape::OmitA, true, true, true);
    let b = w.b;
    let f = w.field;
    let m = w.method;
    let mut c = context(&mut w);
    assert_eq!(
        c.read_field(b, f, Reference::null()).unwrap_err(),
        FieldAccessError::Java(FieldJavaError::IllegalAccess)
    );
    assert_eq!(
        c.select_virtual(b, m, Reference::null()).unwrap_err(),
        MethodAccessError::Java(MethodJavaError::IllegalAccess)
    );
}

#[test]
fn identical_package_text_in_distinct_defining_loaders_denies_before_nest_resolution() {
    let mut g = Graph::new();
    let left = g.loader();
    let right = g.loader();
    let mut h = Class::plain("p/H");
    h.nest_members(&["p/A", "p/B"]);
    let h = g.add(left, h);
    let mut a = Class::plain("p/A");
    a.nest_host("p/H");
    a.field("secret", "I", 2);
    let a = g.add(left, a);
    g.bind(right, "p/A", a);
    g.bind(right, "p/H", h);
    let mut b = Class::plain("p/B");
    b.nest_host("p/H");
    let field = b.reference(9, "p/A", "secret", "I");
    let b = g.add(right, b);
    g.enable(Default::default());
    g.domain.enable_nest_access().unwrap();
    let ids = g.classes.clone();
    g.prepare(&ids);
    let mut heap = g.heap(&ids);
    g.bind_core(&mut heap);
    g.domain.enable_class_mirror_sites(100).unwrap();
    let r = heap.allocate_instance(a).unwrap();
    let mut context = g
        .domain
        .member_context_unnamed_modules(&mut heap, Default::default(), Default::default())
        .unwrap();
    assert_eq!(
        context.read_field(b, field, r).unwrap_err(),
        FieldAccessError::Java(FieldJavaError::IllegalAccess)
    );
    drop(context);
    assert_eq!(g.domain.class_site_count(), 0);
}
#[test]
fn maximal_native_method_table_is_streamed_under_discovery_work_limit() {
    let mut g = Graph::new();
    let mut string = Class::new("java/lang/String", Some("java/lang/Object"), 0x31);
    string.maximal_native_table();
    let string = g.add(g.boot, string);
    assert_eq!(
        g.domain.registry().class(string).unwrap().methods.len(),
        65_535
    );
    let error = probe_compact_string_bodies(
        g.domain.registry(),
        g.boot,
        string,
        StringBodyDiscoveryLimits { max_work_units: 20 },
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.kind,
        StringBodyErrorKind::Limit {
            resource: StringBodyResource::DiscoveryWork,
            ..
        }
    ));
    assert!(error.discovery_usage.work_units <= 20);
    let complete = probe_compact_string_bodies(
        g.domain.registry(),
        g.boot,
        string,
        StringBodyDiscoveryLimits {
            max_work_units: 1_000_000,
        },
    )
    .unwrap();
    assert!(complete.plan.is_none());
    assert!(complete.usage.work_units >= 19 + 65_535);
    assert!(complete.usage.work_units < 1_000_000);
    let used = complete.usage.work_units;
    assert!(
        probe_compact_string_bodies(
            g.domain.registry(),
            g.boot,
            string,
            StringBodyDiscoveryLimits {
                max_work_units: used
            }
        )
        .is_ok()
    );
    assert!(matches!(
        probe_compact_string_bodies(
            g.domain.registry(),
            g.boot,
            string,
            StringBodyDiscoveryLimits {
                max_work_units: used - 1
            }
        )
        .err()
        .unwrap()
        .kind,
        StringBodyErrorKind::Limit { .. }
    ));
}

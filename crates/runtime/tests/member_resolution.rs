//! Independent public member-site, ledger and preparation transactions.
mod member_support;
use crabspot_runtime::*;
use member_support::*;
use std::error::Error;
fn ctx<'a>(g: &'a mut Graph, h: &'a mut ManagedHeap) -> FieldResolutionContext<'a> {
    g.domain
        .member_context_unnamed_modules(
            h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap()
}
struct Fixture {
    g: Graph,
    h: ManagedHeap,
    d: ClassId,
    c: ClassId,
    e: ClassId,
    ld: LoaderId,
    lc: LoaderId,
    le: LoaderId,
    site: u16,
    duplicate: u16,
}
fn fixture(tag: u8, descriptor: &str, flags: u16) -> Fixture {
    fixture_caps(tag, descriptor, flags, MemberResolutionLimits::default())
}
fn fixture_caps(tag: u8, descriptor: &str, flags: u16, caps: MemberResolutionLimits) -> Fixture {
    let mut g = Graph::new();
    let ld = g.loader();
    let lc = g.loader();
    let le = g.loader();
    let mut e = Class::plain("e/Base");
    if tag == 9 {
        e.field("item", descriptor, flags);
    } else {
        let result = descriptor.as_bytes()[descriptor.find(')').unwrap() + 1];
        let code = if result == b'V' {
            vec![0xb1]
        } else if result == b'L' || result == b'[' {
            vec![1, 0xb0]
        } else {
            vec![4, 0xac]
        };
        e.method("item", descriptor, flags, &code, 1, 8);
    }
    let e = g.add(le, e);
    g.bind(lc, "e/Base", e);
    let c = g.add(lc, Class::new("c/Sub", Some("e/Base"), 0x21));
    g.bind(ld, "c/Sub", c);
    let mut d = Class::plain("d/Caller");
    let site = d.reference(tag, "c/Sub", "item", descriptor);
    let duplicate = d.duplicate(site);
    let d = g.add(ld, d);
    g.enable(caps);
    let ids = g.classes.clone();
    g.prepare(&ids);
    let h = g.heap(&ids);
    Fixture {
        g,
        h,
        d,
        c,
        e,
        ld,
        lc,
        le,
        site,
        duplicate,
    }
}
fn relation(g: &mut Graph, n: &str, a: LoaderId, b: LoaderId) -> bool {
    g.domain
        .context(LoadingWorkLimits::default())
        .constrained(&units(n), a, b)
        .unwrap()
}
fn read(f: &mut Fixture, i: u16, r: Reference) -> Result<Value, FieldAccessError> {
    ctx(&mut f.g, &mut f.h).read_field(f.d, i, r)
}
fn read_site(f: &mut Fixture, r: Reference) -> Result<Value, FieldAccessError> {
    let i = f.site;
    read(f, i, r)
}
fn resolved(f: &Fixture) -> Option<MemberResolutionOutcome> {
    f.g.domain.member_site(f.d, f.site).unwrap()
}
#[test]
fn disabled_enable_idempotence_and_scalar_inspection() {
    let mut g = Graph::new();
    let mut c = Class::plain("D");
    let i = c.reference(9, "D", "x", "I");
    let d = g.add(g.boot, c);
    assert_eq!(g.domain.member_resolution_usage().sites, 0);
    assert_eq!(g.domain.member_site(d, i).unwrap(), None);
    assert!(g.domain.member_site(d, 0).is_err());
    let caps = MemberResolutionLimits {
        max_prepared_classes: 0,
        max_sites: 0,
    };
    g.enable(caps);
    g.enable(caps);
    assert!(matches!(
        g.domain
            .enable_member_resolution(MemberResolutionLimits::default())
            .unwrap_err()
            .kind,
        MemberResolutionErrorKind::AlreadyEnabledWithDifferentLimits
    ));
    assert_eq!(g.domain.member_site(d, i).unwrap(), None);
    let foreign = Graph::new();
    assert!(g.domain.member_site(foreign.object, i).is_err());
}
#[test]
fn old_preparation_does_not_mint_witness_and_zero_slot_class_is_checked() {
    let mut g = Graph::new();
    let c = g.add(g.boot, Class::plain("D"));
    g.enable(MemberResolutionLimits::default());
    let p = g.plan(c);
    g.domain
        .context(LoadingWorkLimits::default())
        .prepare_constraints(&p)
        .unwrap();
    let mut h = g.heap(&[c]);
    let err = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .err()
        .unwrap();
    assert!(matches!(
        err.kind,
        MemberResolutionErrorKind::MissingPreparation { .. }
    ));
    assert_eq!(g.domain.member_resolution_usage().prepared_classes, 0);
    g.prepare(&[g.object, c]);
    let _ = ctx(&mut g, &mut h);
    let extra = g.add(g.boot, Class::plain("Empty"));
    let p = g.plan(extra);
    h.admit_instance_layouts(g.domain.registry(), &[&p])
        .unwrap();
    assert!(
        matches!(g.domain.member_context_unnamed_modules(&mut h,FieldResolutionOptions::default(),LoadingWorkLimits::default()).err().unwrap().kind,MemberResolutionErrorKind::MissingPreparation{class} if class==extra)
    );
}
#[test]
fn witness_limit_is_inclusive_and_same_witness_revalidates_without_growth() {
    for cap in [0, 1, u64::MAX] {
        let mut g = Graph::new();
        g.enable(MemberResolutionLimits {
            max_prepared_classes: cap,
            max_sites: 0,
        });
        let p = g.plan(g.object);
        let result = g
            .domain
            .context(LoadingWorkLimits::default())
            .prepare_member_owner(&p);
        if cap == 0 {
            assert!(matches!(
                result.unwrap_err().kind,
                MemberResolutionErrorKind::ResourceLimit {
                    resource: MemberResolutionResource::PreparedClasses,
                    limit: 0,
                    required: 1
                }
            ));
        } else {
            result.unwrap();
            g.domain
                .context(LoadingWorkLimits::default())
                .prepare_member_owner(&p)
                .unwrap();
            assert_eq!(g.domain.member_resolution_usage().prepared_classes, 1);
        }
    }
}
#[test]
fn actual_declarer_not_symbolic_subclass_constrains_nominal_field() {
    let mut f = fixture(9, "Lp/K;", 1);
    let k = f.g.add(f.le, Class::plain("p/K"));
    f.g.bind(f.ld, "p/K", k);
    let other = f.g.add(f.lc, Class::plain("p/K"));
    assert_ne!(k, other);
    let r = f.h.allocate_instance(f.c).unwrap();
    assert_eq!(
        read_site(&mut f, r).unwrap(),
        Value::Reference(Reference::null())
    );
    let expected = f.g.field(f.e, "item", "Lp/K;");
    assert_eq!(resolved(&f), Some(MemberResolutionOutcome::Field(expected)));
    assert!(relation(&mut f.g, "p/K", f.ld, f.le));
    assert!(!relation(&mut f.g, "p/K", f.ld, f.lc));
}
#[test]
fn null_receiver_happens_after_constraint_and_success_publication() {
    let mut f = fixture(9, "[Lp/K;", 1);
    assert!(matches!(
        read_site(&mut f, Reference::null()),
        Err(FieldAccessError::Java(FieldJavaError::NullPointer))
    ));
    assert!(matches!(
        resolved(&f),
        Some(MemberResolutionOutcome::Field(_))
    ));
    assert!(relation(&mut f.g, "p/K", f.ld, f.le));
}
#[test]
fn category_failure_is_not_a_resolution_failure_and_same_slot_can_be_reused() {
    let mut f = fixture(9, "Lp/K;", 9);
    let r = f.h.allocate_instance(f.c).unwrap();
    assert!(matches!(
        read_site(&mut f, r),
        Err(FieldAccessError::Java(
            FieldJavaError::IncompatibleClassChange
        ))
    ));
    assert!(matches!(
        resolved(&f),
        Some(MemberResolutionOutcome::Field(_))
    ));
    let statics = ctx(&mut f.g, &mut f.h)
        .allocate_static_state(StaticStateOptions::default())
        .unwrap();
    assert_eq!(
        ctx(&mut f.g, &mut f.h)
            .read_static(&statics, f.d, f.site)
            .unwrap(),
        Value::Reference(Reference::null())
    );
    assert_eq!(f.g.domain.member_resolution_usage().sites, 1);
}
#[test]
fn final_write_failure_preserves_resolution_and_read_can_use_the_slot() {
    let mut f = fixture(9, "Lp/K;", 0x11);
    let r = f.h.allocate_instance(f.c).unwrap();
    let result =
        ctx(&mut f.g, &mut f.h).write_field(f.d, f.site, r, Value::Reference(Reference::null()));
    assert!(matches!(
        result,
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    ));
    assert!(matches!(
        resolved(&f),
        Some(MemberResolutionOutcome::Field(_))
    ));
    assert_eq!(
        read_site(&mut f, r).unwrap(),
        Value::Reference(Reference::null())
    );
    assert!(relation(&mut f.g, "p/K", f.ld, f.le));
}
#[test]
fn inaccessible_member_failure_is_sticky_and_creates_no_leaf() {
    let mut f = fixture(9, "Lp/K;", 0);
    let r = f.h.allocate_instance(f.c).unwrap();
    assert!(matches!(
        read_site(&mut f, r),
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    ));
    assert_eq!(
        resolved(&f),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::IllegalAccess
        ))
    );
    assert!(!relation(&mut f.g, "p/K", f.ld, f.le));
    let mut smaller = f.g.heap(&[f.d]);
    assert!(matches!(
        ctx(&mut f.g, &mut smaller).read_field(f.d, f.site, Reference::null()),
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    ));
}
#[test]
fn nominal_conflict_is_terminal_and_failure_replays_without_owner_layout() {
    let mut f = fixture(9, "Lp/K;", 1);
    f.g.add(f.ld, Class::plain("p/K"));
    f.g.add(f.le, Class::plain("p/K"));
    assert!(matches!(
        read_site(&mut f, Reference::null()),
        Err(FieldAccessError::Java(FieldJavaError::LinkageError))
    ));
    assert_eq!(
        resolved(&f),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::LinkageError
        ))
    );
    assert!(!relation(&mut f.g, "p/K", f.ld, f.le));
    let mut smaller = f.g.heap(&[f.d]);
    assert!(matches!(
        ctx(&mut f.g, &mut smaller).read_field(f.d, f.site, Reference::null()),
        Err(FieldAccessError::Java(FieldJavaError::LinkageError))
    ));
}
#[test]
fn success_requires_current_symbolic_owner_layout_and_survives_missing_layout() {
    let mut f = fixture(9, "I", 1);
    let r = f.h.allocate_instance(f.c).unwrap();
    read_site(&mut f, r).unwrap();
    let terminal = resolved(&f);
    let mut smaller = f.g.heap(&[f.d]);
    assert!(
        ctx(&mut f.g, &mut smaller)
            .read_field(f.d, f.site, Reference::null())
            .is_err()
    );
    assert_eq!(resolved(&f), terminal);
    assert_eq!(read_site(&mut f, r).unwrap(), Value::Int(0));
}
#[test]
fn byte_identical_cp_slots_have_distinct_terminal_records() {
    let mut f = fixture(9, "I", 1);
    let r = f.h.allocate_instance(f.c).unwrap();
    for i in [f.site, f.site, f.duplicate, f.site] {
        assert_eq!(read(&mut f, i, r).unwrap(), Value::Int(0));
    }
    assert_eq!(f.g.domain.member_resolution_usage().sites, 2);
    assert_eq!(
        f.g.domain.member_site(f.d, f.site).unwrap(),
        f.g.domain.member_site(f.d, f.duplicate).unwrap()
    );
}
#[test]
fn warm_field_transaction_costs_exactly_one_loading_work_and_no_utf_or_scratch() {
    let mut f = fixture(9, "Lp/K;", 1);
    let r = f.h.allocate_instance(f.c).unwrap();
    let mut c = ctx(&mut f.g, &mut f.h);
    c.read_field(f.d, f.site, r).unwrap();
    let before = c.member_loading_work().unwrap();
    c.read_field(f.d, f.site, r).unwrap();
    let after = c.member_loading_work().unwrap();
    assert_eq!(after.work_units - before.work_units, 1);
    assert_eq!(after.utf16_units, before.utf16_units);
    assert_eq!(after.scratch_entries, before.scratch_entries);
    assert_eq!(c.resolved_symbols(), 0);
    assert_eq!(c.symbol_units(), 0);
}
#[test]
fn same_loader_cold_primitive_descriptor_is_funded_but_creates_no_equality() {
    let mut g = Graph::new();
    let mut c = Class::plain("D");
    c.field("f", "I", 1);
    let i = c.reference(9, "D", "f", "I");
    let d = g.add(g.boot, c);
    let mut h = g.ready();
    let r = h.allocate_instance(d).unwrap();
    let ledger = g.domain.usage();
    let mut cx = ctx(&mut g, &mut h);
    let before = cx.member_loading_work().unwrap();
    cx.read_field(d, i, r).unwrap();
    let cold = cx.member_loading_work().unwrap();
    cx.read_field(d, i, r).unwrap();
    let warm = cx.member_loading_work().unwrap();
    assert!(cold.utf16_units > before.utf16_units);
    assert_eq!(warm.work_units - cold.work_units, 1);
    drop(cx);
    assert_eq!(g.domain.usage(), ledger);
}
#[test]
fn wrong_method_owner_kind_is_terminal_but_no_signature_leaf_is_imposed() {
    let mut g = Graph::new();
    let l = g.loader();
    let it = g.add(l, Class::new("I", Some("java/lang/Object"), 0x601));
    let mut d = Class::plain("D");
    let i = d.reference(10, "I", "m", "(Lp/K;)V");
    let d = g.add(l, d);
    let mut h = g.ready();
    assert!(matches!(
        ctx(&mut g, &mut h).resolve_static(d, i),
        Err(MethodAccessError::Java(
            MethodJavaError::IncompatibleClassChange
        ))
    ));
    assert_eq!(
        g.domain.member_site(d, i).unwrap(),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::IncompatibleClassChange
        ))
    );
    assert_eq!(g.domain.member_resolution_usage().sites, 1);
    let _ = it;
}
#[test]
fn no_such_method_is_terminal_and_error_sources_remain_finite() {
    let mut f = fixture(10, "()V", 9);
    let mut d = Class::plain("Absent");
    let i = d.reference(10, "c/Sub", "absent", "()V");
    let a = f.g.add(f.ld, d);
    f.g.prepare(&[a]);
    let p = f.g.plan(a);
    f.h.admit_instance_layouts(f.g.domain.registry(), &[&p])
        .unwrap();
    assert!(matches!(
        ctx(&mut f.g, &mut f.h).resolve_static(a, i),
        Err(MethodAccessError::Java(MethodJavaError::NoSuchMethod))
    ));
    assert_eq!(
        f.g.domain.member_site(a, i).unwrap(),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::NoSuchMethod
        ))
    );
    let err = f.g.domain.member_site(a, 0).unwrap_err();
    let mut n = 0;
    let mut next = err.source();
    while let Some(e) = next {
        n += 1;
        assert!(n < 12);
        next = e.source();
    }
}

#[test]
fn site_quota_zero_exact_and_max_preserve_warm_sites() {
    for cap in [0, 1, u64::MAX] {
        let mut f = fixture_caps(
            9,
            "Lp/K;",
            1,
            MemberResolutionLimits {
                max_prepared_classes: u64::MAX,
                max_sites: cap,
            },
        );
        let r = f.h.allocate_instance(f.c).unwrap();
        let result = read_site(&mut f, r);
        if cap == 0 {
            assert!(
                matches!(result,Err(FieldAccessError::MemberResolution(e)) if matches!(e.kind,MemberResolutionErrorKind::ResourceLimit{resource:MemberResolutionResource::Sites,limit:0,required:1}))
            );
            assert_eq!(resolved(&f), None);
            assert!(!relation(&mut f.g, "p/K", f.ld, f.le));
        } else {
            result.unwrap();
            read_site(&mut f, r).unwrap();
            let i = f.duplicate;
            let result = read(&mut f, i, r);
            if cap == 1 {
                assert!(
                    matches!(result,Err(FieldAccessError::MemberResolution(e)) if matches!(e.kind,MemberResolutionErrorKind::ResourceLimit{resource:MemberResolutionResource::Sites,limit:1,required:2}))
                );
                assert_eq!(f.g.domain.member_site(f.d, i).unwrap(), None);
            } else {
                result.unwrap();
            }
        }
    }
}
#[test]
fn cold_missing_initiation_is_host_repairable_and_implied_equality_cannot_resolve() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let mut e = Class::plain("Owner");
    e.field("f", "I", 1);
    let e = g.add(b, e);
    let mut d = Class::plain("D");
    let i = d.reference(9, "Owner", "f", "I");
    let d = g.add(a, d);
    g.domain
        .context(LoadingWorkLimits::default())
        .impose(&units("Owner"), a, b)
        .unwrap();
    let mut h = g.ready();
    let r = h.allocate_instance(e).unwrap();
    let error = ctx(&mut g, &mut h).read_field(d, i, r).unwrap_err();
    assert!(
        matches!(error,FieldAccessError::MemberResolution(e) if matches!(e.kind,MemberResolutionErrorKind::MissingInitiation{origin,loader} if origin==d && loader==a))
    );
    assert_eq!(g.domain.member_site(d, i).unwrap(), None);
    g.bind(a, "Owner", e);
    assert_eq!(
        ctx(&mut g, &mut h).read_field(d, i, r).unwrap(),
        Value::Int(0)
    );
}
#[test]
fn cold_missing_layout_can_be_repaired_by_fresh_admission() {
    let mut f = fixture(9, "I", 1);
    let mut h = f.g.heap(&[f.d]);
    assert!(
        ctx(&mut f.g, &mut h)
            .read_field(f.d, f.site, Reference::null())
            .is_err()
    );
    assert_eq!(resolved(&f), None);
    let p = f.g.plan(f.c);
    h.admit_instance_layouts(f.g.domain.registry(), &[&p])
        .unwrap();
    let r = h.allocate_instance(f.c).unwrap();
    assert_eq!(
        ctx(&mut f.g, &mut h).read_field(f.d, f.site, r).unwrap(),
        Value::Int(0)
    );
}
#[test]
fn failure_on_second_method_leaf_retains_first_and_is_sticky() {
    let mut f = fixture(10, "(Lp/P;Lp/Q;)Lp/R;", 9);
    f.g.add(f.ld, Class::plain("p/Q"));
    f.g.add(f.le, Class::plain("p/Q"));
    assert!(matches!(
        ctx(&mut f.g, &mut f.h).resolve_static(f.d, f.site),
        Err(MethodAccessError::Java(MethodJavaError::LinkageError))
    ));
    assert!(relation(&mut f.g, "p/P", f.ld, f.le));
    assert!(!relation(&mut f.g, "p/Q", f.ld, f.le));
    assert!(!relation(&mut f.g, "p/R", f.ld, f.le));
    let old = f.g.domain.usage();
    assert!(matches!(
        ctx(&mut f.g, &mut f.h).resolve_static(f.d, f.site),
        Err(MethodAccessError::Java(MethodJavaError::LinkageError))
    ));
    assert_eq!(f.g.domain.usage(), old);
    assert_eq!(
        resolved(&f),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::LinkageError
        ))
    );
}
#[test]
fn array_nominal_leaf_and_return_are_constrained_primitive_arrays_are_not() {
    for desc in ["([[Lp/P;[[I)[Lp/Q;", "([I[J)I"] {
        let mut f = fixture(10, desc, 9);
        ctx(&mut f.g, &mut f.h).resolve_static(f.d, f.site).unwrap();
        assert_eq!(relation(&mut f.g, "p/P", f.ld, f.le), desc.contains("p/P"));
        assert_eq!(relation(&mut f.g, "p/Q", f.ld, f.le), desc.contains("p/Q"));
        assert_eq!(f.g.domain.member_resolution_usage().sites, 1);
    }
}
#[test]
fn all_three_work_caps_are_inclusive_and_new_context_resets_work_only() {
    // Measure this independently emitted fixture's opening, then exercise exact paid
    // prefixes and one-less limits. Absolute warm transaction cost is tested above.
    for resource in 0..3 {
        let mut f = fixture(9, "Lp/K;", 1);
        let opening = ctx(&mut f.g, &mut f.h).member_loading_work().unwrap();
        let r = f.h.allocate_instance(f.c).unwrap();
        let mut c = ctx(&mut f.g, &mut f.h);
        c.read_field(f.d, f.site, r).unwrap();
        let paid = c.member_loading_work().unwrap();
        drop(c);
        let caps = |n: u64| {
            let mut x = LoadingWorkLimits::default();
            match resource {
                0 => x.max_utf16_units = n,
                1 => x.max_work_units = n,
                _ => x.max_scratch_entries = n,
            };
            x
        };
        let required = match resource {
            0 => paid.utf16_units,
            1 => paid.work_units,
            _ => paid.scratch_entries,
        };
        assert!(required > 0);
        let mut fresh = fixture(9, "Lp/K;", 1);
        let r = fresh.h.allocate_instance(fresh.c).unwrap();
        let mut c = fresh
            .g
            .domain
            .member_context_unnamed_modules(
                &mut fresh.h,
                FieldResolutionOptions::default(),
                caps(required),
            )
            .unwrap();
        c.read_field(fresh.d, fresh.site, r).unwrap();
        assert_eq!(c.member_loading_work().unwrap(), paid);
        drop(c);
        let mut short = fixture(9, "Lp/K;", 1);
        let r = short.h.allocate_instance(short.c).unwrap();
        if let Ok(mut c) = short.g.domain.member_context_unnamed_modules(
            &mut short.h,
            FieldResolutionOptions::default(),
            caps(required - 1),
        ) {
            assert!(c.read_field(short.d, short.site, r).is_err());
        }
        assert_eq!(resolved(&short), None);
        let reset = ctx(&mut f.g, &mut f.h).member_loading_work().unwrap();
        assert_eq!(reset, opening);
        assert_eq!(f.g.domain.member_resolution_usage().sites, 1);
    }
}
#[test]
fn checked_context_does_not_use_legacy_symbol_storage_quota() {
    let mut f = fixture(9, "I", 1);
    let r = f.h.allocate_instance(f.c).unwrap();
    let options = FieldResolutionOptions {
        max_symbols: 0,
        max_symbol_units: 0,
        ..Default::default()
    };
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(&mut f.h, options, LoadingWorkLimits::default())
            .unwrap();
    assert_eq!(c.read_field(f.d, f.site, r).unwrap(), Value::Int(0));
    assert_eq!(c.resolved_symbols(), 0);
    assert_eq!(c.symbol_units(), 0);
}
#[test]
fn foreign_heap_and_foreign_plan_do_not_publish_witnesses_or_sites() {
    let mut f = fixture(9, "I", 1);
    let mut other = Graph::new();
    let mut h = other.ready();
    assert!(
        f.g.domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default()
            )
            .is_err()
    );
    let plan = other.plan(other.object);
    let before = f.g.domain.member_resolution_usage();
    assert!(
        f.g.domain
            .context(LoadingWorkLimits::default())
            .prepare_member_owner(&plan)
            .is_err()
    );
    assert_eq!(f.g.domain.member_resolution_usage(), before);
    assert_eq!(resolved(&f), None);
}
#[test]
fn long_descriptor_is_borrowed_and_warm_transaction_does_not_repeat_leaf_scan() {
    let name = "x".repeat(65533);
    let descriptor = format!("L{name};");
    let mut f = fixture(9, &descriptor, 1);
    let r = f.h.allocate_instance(f.c).unwrap();
    let mut c = ctx(&mut f.g, &mut f.h);
    c.read_field(f.d, f.site, r).unwrap();
    let old = c.member_loading_work().unwrap();
    c.read_field(f.d, f.site, r).unwrap();
    let new = c.member_loading_work().unwrap();
    assert_eq!(new.utf16_units, old.utf16_units);
    assert_eq!(new.work_units - old.work_units, 1);
}

#[test]
fn repeated_witness_checks_actual_parent_even_after_success() {
    let mut g = Graph::new();
    let l = g.loader();
    let p = g.add(g.boot, Class::plain("P"));
    g.bind(l, "P", p);
    let c = g.add(l, Class::new("C", Some("P"), 0x21));
    g.enable(MemberResolutionLimits::default());
    g.prepare(&[g.object, p, c]);
    let other = g.loader();
    let wrong = g.add(other, Class::plain("P"));
    g.intended.insert((l, units("P")), wrong);
    let alternate = g.plan(c);
    let before = g.domain.member_resolution_usage();
    assert!(
        g.domain
            .context(LoadingWorkLimits::default())
            .prepare_member_owner(&alternate)
            .is_err()
    );
    assert_eq!(g.domain.member_resolution_usage(), before);
}
#[test]
fn failed_preparation_retains_prefix_without_publishing_owner_witness() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let mut base = Class::plain("Base");
    base.method("m", "(Lp/P;Lp/Q;)V", 1, &[0xb1], 0, 3);
    let base = g.add(a, base);
    g.bind(b, "Base", base);
    let mut sub = Class::new("Sub", Some("Base"), 0x21);
    sub.method("m", "(Lp/P;Lp/Q;)V", 1, &[0xb1], 0, 3);
    let sub = g.add(b, sub);
    g.add(a, Class::plain("p/Q"));
    g.add(b, Class::plain("p/Q"));
    g.enable(MemberResolutionLimits::default());
    g.prepare(&[g.object, base]);
    let before = g.domain.member_resolution_usage();
    let p = g.plan(sub);
    let err = g
        .domain
        .context(LoadingWorkLimits::default())
        .prepare_member_owner(&p)
        .unwrap_err();
    assert!(matches!(err.kind, MemberResolutionErrorKind::Loading(_)));
    assert_eq!(g.domain.member_resolution_usage(), before);
    assert!(relation(&mut g, "p/P", a, b));
    assert!(!relation(&mut g, "p/Q", a, b));
    let mut h = g.heap(&[sub]);
    assert!(
        g.domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default()
            )
            .is_err()
    );
}
#[test]
fn protected_qualifier_is_rechecked_on_each_warm_receiver() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let mut base = Class::plain("p/Base");
    base.field("f", "Lp/K;", 4);
    let base = g.add(a, base);
    g.bind(b, "p/Base", base);
    let mut sub = Class::new("q/Sub", Some("p/Base"), 0x21);
    let i = sub.reference(9, "p/Base", "f", "Lp/K;");
    let sub = g.add(b, sub);
    let mut h = g.ready();
    let good = h.allocate_instance(sub).unwrap();
    let bad = h.allocate_instance(base).unwrap();
    ctx(&mut g, &mut h).read_field(sub, i, good).unwrap();
    let terminal = g.domain.member_site(sub, i).unwrap();
    assert!(ctx(&mut g, &mut h).read_field(sub, i, bad).is_err());
    assert_eq!(g.domain.member_site(sub, i).unwrap(), terminal);
    assert_eq!(
        ctx(&mut g, &mut h).read_field(sub, i, good).unwrap(),
        Value::Reference(Reference::null())
    );
    assert_eq!(g.domain.member_resolution_usage().sites, 1);
}
#[test]
fn special_selected_private_signature_requires_readonly_e_s_proof() {
    for repair in 0..2 {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let c = g.loader();
        let mut base = Class::plain("A");
        base.method("m", "(Lp/K;)V", 1, &[0xb1], 0, 2);
        let base = g.add(a, base);
        g.bind(b, "A", base);
        let mut mid = Class::new("B", Some("A"), 0x21);
        mid.method("m", "(Lp/K;)V", 2, &[0xb1], 0, 2);
        let mid = g.add(b, mid);
        g.bind(c, "B", mid);
        g.bind(c, "A", base);
        let mut leaf = Class::new("C", Some("B"), 0x21);
        let i = leaf.reference(10, "A", "m", "(Lp/K;)V");
        let leaf = g.add(c, leaf);
        let mut h = g.ready();
        let receiver = h.allocate_instance(leaf).unwrap();
        let error = ctx(&mut g, &mut h)
            .select_special(leaf, i, receiver)
            .unwrap_err();
        let resolved = g.method(base, "m", "(Lp/K;)V");
        let selected = g.method(mid, "m", "(Lp/K;)V");
        assert!(
            matches!(error,MethodAccessError::MemberResolution(e) if matches!(e.kind,MemberResolutionErrorKind::UnprovenSelection{resolved:r,selected:s} if r==resolved&&s==selected))
        );
        assert_eq!(
            g.domain.member_site(leaf, i).unwrap(),
            Some(MemberResolutionOutcome::Method(resolved))
        );
        assert!(!relation(&mut g, "p/K", a, b));
        if repair == 0 {
            g.domain
                .context(LoadingWorkLimits::default())
                .impose(&units("p/K"), a, b)
                .unwrap();
        } else {
            let k = g.add(a, Class::plain("p/K"));
            g.bind(b, "p/K", k);
        }
        assert_eq!(
            ctx(&mut g, &mut h)
                .select_special(leaf, i, receiver)
                .unwrap(),
            selected
        );
    }
}
#[test]
fn coincidental_private_match_does_not_constrain_actual_receiver_loader() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let mut base = Class::plain("A");
    base.method("m", "(Lp/K;)V", 1, &[0xb1], 0, 2);
    let base = g.add(a, base);
    g.bind(b, "A", base);
    let mut child = Class::new("B", Some("A"), 0x21);
    child.method("m", "(Lp/K;)V", 2, &[0xb1], 0, 2);
    let child = g.add(b, child);
    let mut caller = Class::plain("D");
    let i = caller.reference(10, "A", "m", "(Lp/K;)V");
    let caller = g.add(a, caller);
    g.add(a, Class::plain("p/K"));
    g.add(b, Class::plain("p/K"));
    let mut h = g.ready();
    let r = h.allocate_instance(child).unwrap();
    assert_eq!(
        ctx(&mut g, &mut h).select_virtual(caller, i, r).unwrap(),
        g.method(base, "m", "(Lp/K;)V")
    );
    assert!(!relation(&mut g, "p/K", a, b));
}
#[test]
fn identical_class_bytes_in_another_defining_loader_are_another_site() {
    let mut f = fixture(9, "I", 1);
    let r = f.h.allocate_instance(f.c).unwrap();
    read_site(&mut f, r).unwrap();
    let other = f.g.loader();
    f.g.bind(other, "c/Sub", f.c);
    let mut d = Class::plain("d/Caller");
    let i = d.reference(9, "c/Sub", "item", "I");
    let duplicate = d.duplicate(i);
    assert_eq!(i, f.site);
    assert_eq!(duplicate, f.duplicate);
    let d = f.g.add(other, d);
    f.g.prepare(&[d]);
    let p = f.g.plan(d);
    f.h.admit_instance_layouts(f.g.domain.registry(), &[&p])
        .unwrap();
    ctx(&mut f.g, &mut f.h).read_field(d, i, r).unwrap();
    assert_eq!(f.g.domain.member_resolution_usage().sites, 2);
}
#[test]
fn many_identical_slots_consume_one_record_each_with_no_name_amplification() {
    let mut g = Graph::new();
    let mut c = Class::plain("D");
    c.field("f", "I", 1);
    let first = c.reference(9, "D", "f", "I");
    let mut indices = vec![first];
    for _ in 0..1023 {
        indices.push(c.duplicate(first));
    }
    let d = g.add(g.boot, c);
    let mut h = g.ready();
    let r = h.allocate_instance(d).unwrap();
    let mut cx = ctx(&mut g, &mut h);
    for i in indices {
        cx.read_field(d, i, r).unwrap();
    }
    drop(cx);
    assert_eq!(g.domain.member_resolution_usage().sites, 1024);
}
#[test]
fn deep_checked_snapshot_and_field_lookup_are_iterative() {
    let mut g = Graph::new();
    let mut root = Class::plain("N0");
    root.field("f", "I", 1);
    let mut leaf = g.add(g.boot, root);
    for i in 1..2048 {
        let class = Class::new(&format!("N{i}"), Some(&format!("N{}", i - 1)), 0x21);
        leaf = g.add(g.boot, class);
    }
    let mut caller = Class::plain("D");
    let i = caller.reference(9, "N2047", "f", "I");
    let caller = g.add(g.boot, caller);
    g.enable(MemberResolutionLimits::default());
    let ids = g.classes.clone();
    g.prepare(&ids);
    let mut h = g.heap(&[leaf, caller]);
    let r = h.allocate_instance(leaf).unwrap();
    let mut c = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions {
                max_lookup_steps: 10000000,
                ..Default::default()
            },
            LoadingWorkLimits::default(),
        )
        .unwrap();
    assert_eq!(c.read_field(caller, i, r).unwrap(), Value::Int(0));
}

#[test]
fn single_object_opening_has_independently_counted_span_and_scalar_events() {
    for (utf, work, success) in [
        (32, 41, true),
        (31, 41, false),
        (32, 40, false),
        (u64::MAX, u64::MAX, true),
    ] {
        let mut g = Graph::new();
        let mut h = g.ready();
        let before = g.domain.usage();
        let result = g.domain.member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits {
                max_utf16_units: utf,
                max_work_units: work,
                max_scratch_entries: 0,
            },
        );
        if success {
            let c = result.unwrap();
            assert_eq!(
                c.member_loading_work(),
                Some(LoadingWorkUsage {
                    utf16_units: 32,
                    work_units: 41,
                    scratch_entries: 0
                })
            );
        } else {
            assert!(result.is_err());
        }
        assert_eq!(g.domain.usage(), before);
        assert_eq!(g.domain.member_resolution_usage().sites, 0);
    }
}
#[test]
fn context_opening_funds_registry_entries_absent_from_heap_in_index_order() {
    let mut g = Graph::new();
    let mut h = g.ready();
    let before = ctx(&mut g, &mut h).member_loading_work().unwrap();
    g.add(g.boot, Class::plain("NotInHeap"));
    let after = ctx(&mut g, &mut h).member_loading_work().unwrap();
    assert_eq!(after.work_units, before.work_units + 1);
    assert_eq!(after.utf16_units, before.utf16_units);
    assert_eq!(g.domain.member_resolution_usage().prepared_classes, 1);
}

#[test]
fn private_access_distinguishes_runtime_package_from_unimplemented_nest_access() {
    for (same_loader, same_spelling) in [(false, false), (false, true), (true, false), (true, true)]
    {
        for tag in [9, 10] {
            let mut g = Graph::new();
            let a = g.loader();
            let b = if same_loader { a } else { g.loader() };
            let mut owner = Class::plain("p/E");
            if tag == 9 {
                owner.field("secret", "Lp/K;", 2);
            } else {
                owner.method("secret", "(Lp/K;)V", 10, &[0xb1], 0, 1);
            }
            let e = g.add(a, owner);
            g.bind(b, "p/E", e);
            let mut d = Class::plain(if same_spelling { "p/D" } else { "q/D" });
            let i = d.reference(
                tag,
                "p/E",
                "secret",
                if tag == 9 { "Lp/K;" } else { "(Lp/K;)V" },
            );
            let d = g.add(b, d);
            let mut h = g.ready();
            let r = h.allocate_instance(e).unwrap();
            let java = !(same_loader && same_spelling);
            if tag == 9 {
                let result = ctx(&mut g, &mut h).read_field(d, i, r);
                if java {
                    assert!(matches!(
                        result,
                        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
                    ));
                } else {
                    assert!(matches!(
                        result,
                        Err(FieldAccessError::UnsupportedNestAccess { .. })
                    ));
                }
            } else {
                let result = ctx(&mut g, &mut h).resolve_static(d, i);
                if java {
                    assert!(matches!(
                        result,
                        Err(MethodAccessError::Java(MethodJavaError::IllegalAccess))
                    ));
                } else {
                    assert!(matches!(
                        result,
                        Err(MethodAccessError::UnsupportedNestAccess { .. })
                    ));
                }
            }
            if java {
                assert_eq!(
                    g.domain.member_site(d, i).unwrap(),
                    Some(MemberResolutionOutcome::Failure(
                        MemberResolutionFailure::IllegalAccess
                    ))
                );
            } else {
                assert_eq!(g.domain.member_site(d, i).unwrap(), None);
            }
            assert!(!relation(&mut g, "p/K", a, b) || a == b);
            let mut resolver = Resolver(&g.intended);
            let mut legacy = FieldResolutionContext::new_unnamed_modules(
                g.domain.registry(),
                &mut h,
                &mut resolver,
                FieldResolutionOptions::default(),
            )
            .unwrap();
            if tag == 9 {
                assert!(matches!(
                    legacy.read_field(d, i, r),
                    Err(FieldAccessError::UnsupportedNestAccess { .. })
                ));
            } else {
                assert!(matches!(
                    legacy.resolve_static(d, i),
                    Err(MethodAccessError::UnsupportedNestAccess { .. })
                ));
            }
        }
    }
}

#[test]
fn paid_host_interruption_between_two_leaves_keeps_prefix_and_retries_vacant_site() {
    let mut reference = fixture(10, "(Lp/P;Lp/Q;)V", 9);
    let full = {
        let mut c = ctx(&mut reference.g, &mut reference.h);
        c.resolve_static(reference.d, reference.site).unwrap();
        c.member_loading_work().unwrap().work_units
    };
    assert!(full < 5000);
    let mut exercised = false;
    for cap in 0..full {
        let mut f = fixture(10, "(Lp/P;Lp/Q;)V", 9);
        let outcome = match f.g.domain.member_context_unnamed_modules(
            &mut f.h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits {
                max_work_units: cap,
                ..Default::default()
            },
        ) {
            Ok(mut c) => c.resolve_static(f.d, f.site).map(|_| ()),
            Err(_) => continue,
        };
        if outcome.is_ok() {
            continue;
        }
        let p = relation(&mut f.g, "p/P", f.ld, f.le);
        let q = relation(&mut f.g, "p/Q", f.ld, f.le);
        if p && !q {
            assert_eq!(resolved(&f), None);
            let error = outcome.unwrap_err();
            assert!(
                matches!(error,MethodAccessError::MemberResolution(e) if e.position==Some(MemberDescriptorPosition::Parameter(1)))
            );
            let before = f.g.domain.usage();
            ctx(&mut f.g, &mut f.h).resolve_static(f.d, f.site).unwrap();
            assert!(relation(&mut f.g, "p/P", f.ld, f.le));
            assert!(relation(&mut f.g, "p/Q", f.ld, f.le));
            assert_eq!(f.g.domain.usage().unions, before.unions + 1);
            assert!(matches!(
                resolved(&f),
                Some(MemberResolutionOutcome::Method(_))
            ));
            exercised = true;
            break;
        }
    }
    assert!(
        exercised,
        "a host work interruption must be possible after the committed first insertion"
    );
}

#[test]
fn warm_static_method_resolution_skips_descriptor_and_all_capacity_work() {
    let mut f = fixture(10, "(Lp/P;[[Lp/Q;)Lp/P;", 9);
    let mut c = ctx(&mut f.g, &mut f.h);
    let method = c.resolve_static(f.d, f.site).unwrap();
    let first = c.member_loading_work().unwrap();
    assert_eq!(c.resolve_static(f.d, f.site).unwrap(), method);
    let next = c.member_loading_work().unwrap();
    assert_eq!(next.work_units - first.work_units, 1);
    assert_eq!(next.utf16_units, first.utf16_units);
    assert_eq!(next.scratch_entries, first.scratch_entries);
}
#[test]
fn failed_field_descriptor_precharge_preserves_position_and_vacant_site() {
    let mut reference = fixture(9, "I", 1);
    let receiver = reference.h.allocate_instance(reference.c).unwrap();
    let total = {
        let mut c = ctx(&mut reference.g, &mut reference.h);
        c.read_field(reference.d, reference.site, receiver).unwrap();
        c.member_loading_work().unwrap().utf16_units
    };
    let mut f = fixture(9, "I", 1);
    let receiver = f.h.allocate_instance(f.c).unwrap();
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(
                &mut f.h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits {
                    max_utf16_units: total - 1,
                    ..Default::default()
                },
            )
            .unwrap();
    let error = c.read_field(f.d, f.site, receiver).unwrap_err();
    assert!(
        matches!(error,FieldAccessError::MemberResolution(e) if e.origin==Some(f.d)&&e.index==Some(f.site)&&e.position==Some(MemberDescriptorPosition::Field)&&matches!(e.kind,MemberResolutionErrorKind::Loading(_)))
    );
    drop(c);
    assert_eq!(resolved(&f), None);
}

#[test]
fn same_method_slot_rechecks_opcode_category_without_caching_later_icce() {
    for flags in [1, 9] {
        let mut f = fixture_caps(
            10,
            "(Lp/K;)V",
            flags,
            MemberResolutionLimits {
                max_prepared_classes: u64::MAX,
                max_sites: 1,
            },
        );
        let r = f.h.allocate_instance(f.c).unwrap();
        if flags == 9 {
            assert!(matches!(
                ctx(&mut f.g, &mut f.h).select_virtual(f.d, f.site, r),
                Err(MethodAccessError::Java(
                    MethodJavaError::IncompatibleClassChange
                ))
            ));
            assert!(matches!(
                resolved(&f),
                Some(MemberResolutionOutcome::Method(_))
            ));
            ctx(&mut f.g, &mut f.h).resolve_static(f.d, f.site).unwrap();
        } else {
            assert!(matches!(
                ctx(&mut f.g, &mut f.h).resolve_static(f.d, f.site),
                Err(MethodAccessError::Java(
                    MethodJavaError::IncompatibleClassChange
                ))
            ));
            assert!(matches!(
                resolved(&f),
                Some(MemberResolutionOutcome::Method(_))
            ));
            ctx(&mut f.g, &mut f.h)
                .select_virtual(f.d, f.site, r)
                .unwrap();
        }
        assert_eq!(f.g.domain.member_resolution_usage().sites, 1);
    }
}

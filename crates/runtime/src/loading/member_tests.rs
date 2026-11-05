use super::super::tests::{object, snapshot};
use super::*;
use crate::{
    HeapOptions, HierarchyOptions, InstanceOptions, Reference, SameLoaderResolver, build_hierarchy,
};

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn words(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|n| n.to_be_bytes()).collect()
}
fn bytes() -> Vec<u8> {
    let mut pool = Vec::<Vec<u8>>::new();
    for text in ["p/C", "java/lang/Object", "x", "I", "m", "()V", "missing"] {
        let mut row = vec![1];
        row.extend(words(&[text.len() as u16]));
        row.extend(text.bytes());
        pool.push(row);
    }
    // 8=C, 9=Object, 10=x:I, 11=Fieldref, 12=m:()V, 13=Methodref,
    // 14=missing:I, 15=missing Fieldref, 16=alias of x:I.
    for (tag, values) in [
        (7, vec![1]),
        (7, vec![2]),
        (12, vec![3, 4]),
        (9, vec![8, 10]),
        (12, vec![5, 6]),
        (10, vec![8, 12]),
        (12, vec![7, 4]),
        (9, vec![8, 14]),
        (9, vec![8, 10]),
    ] {
        let mut row = vec![tag];
        row.extend(words(&values));
        pool.push(row);
    }
    let mut b = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    b.extend(words(&[17]));
    b.extend(pool.into_iter().flatten());
    b.extend(words(&[0x21, 8, 9, 0, 1, 1, 3, 4, 0, 1, 0x109, 5, 6, 0, 0]));
    b
}
fn fixture() -> (LoadingDomain, ManagedHeap, ClassId) {
    let mut d =
        LoadingDomain::new(RegistryOptions::default(), LoadingDomainLimits::default()).unwrap();
    let loader = d.create_loader().unwrap();
    let o = object(&mut d, loader);
    let c = d
        .context(LoadingWorkLimits::default())
        .define(loader, &units("p/C"), &bytes(), ParseOptions::default())
        .unwrap();
    d.enable_member_resolution(MemberResolutionLimits::default())
        .unwrap();
    let op = build_hierarchy(
        &d.registry,
        o,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let cp = build_hierarchy(
        &d.registry,
        c,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    for plan in [&op, &cp] {
        d.context(LoadingWorkLimits::default())
            .prepare_member_owner(plan)
            .unwrap();
    }
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&d.registry, &[&cp]).unwrap();
    (d, heap, c)
}
fn open<'a>(d: &'a mut LoadingDomain, h: &'a mut ManagedHeap) -> FieldResolutionContext<'a> {
    d.member_context_unnamed_modules(
        h,
        FieldResolutionOptions::default(),
        LoadingWorkLimits::default(),
    )
    .unwrap()
}
fn paid(d: &mut LoadingDomain, h: &mut ManagedHeap, c: ClassId) -> FieldId {
    open(d, h).select_field_id(c, 11).unwrap()
}
fn invalid(e: &MemberResolutionError) -> bool {
    matches!(e.kind, M::InvalidState(_))
}
#[test]
fn inclusive_member_caps_overflow_and_opt_in_are_distinct() {
    for r in [
        MemberResolutionResource::Sites,
        MemberResolutionResource::PreparedClasses,
    ] {
        assert_eq!(next(0, 1, r).unwrap(), 1);
        assert!(matches!(
            next(0, 0, r),
            Err(M::ResourceLimit { required: 1, .. })
        ));
        assert!(matches!(
            next(u64::MAX, u64::MAX, r),
            Err(M::AccountingOverflow { .. })
        ));
    }
    let (mut d, _, _) = fixture();
    let before = d.member_resolution_usage();
    d.enable_member_resolution(MemberResolutionLimits::default())
        .unwrap();
    assert!(matches!(
        d.enable_member_resolution(MemberResolutionLimits {
            max_sites: 0,
            ..Default::default()
        })
        .unwrap_err()
        .kind,
        M::AlreadyEnabledWithDifferentLimits
    ));
    assert_eq!(d.member_resolution_usage(), before);
}
#[test]
fn warm_hit_costs_one_and_cold_reservation_failure_keeps_nonempty_state() {
    let (mut d, mut heap, c) = fixture();
    let field = paid(&mut d, &mut heap, c);
    let ledger = snapshot(&d.ledger);
    let usage = d.member_resolution_usage();
    {
        let mut context = open(&mut d, &mut heap);
        let before = context.member_loading_work().unwrap();
        assert_eq!(context.select_field_id(c, 11).unwrap(), field);
        let after = context.member_loading_work().unwrap();
        assert_eq!(after.work_units, before.work_units + 1);
        assert_eq!(after.utf16_units, before.utf16_units);
        assert_eq!(after.scratch_entries, before.scratch_entries);
        context.member.as_mut().unwrap().work.fail_reserve_after = Some(0);
        assert!(matches!(
            context.select_field_id(c, 16),
            Err(FieldAccessError::MemberResolution(_))
        ));
        assert_eq!(context.select_field_id(c, 11).unwrap(), field);
    }
    assert_eq!(d.member_resolution_usage(), usage);
    assert_eq!(snapshot(&d.ledger), ledger);
    assert_eq!(d.member_site(c, 16).unwrap(), None);
}
#[test]
fn paired_scratch_funding_and_site_cap_fail_before_descriptor_or_publication() {
    for scratch in [true, false] {
        let (mut d, mut heap, c) = fixture();
        paid(&mut d, &mut heap, c);
        let before = snapshot(&d.ledger);
        let usage = d.member_resolution_usage();
        if !scratch {
            d.members.as_mut().unwrap().limits.max_sites = 1;
        }
        {
            let mut context = open(&mut d, &mut heap);
            let a = context.member.as_mut().unwrap();
            if scratch {
                a.work.limits.max_scratch_entries = a.work.usage.scratch_entries;
            }
            let before_work = a.work.usage;
            let err = context.select_field_id(c, 16).unwrap_err();
            assert!(matches!(err, FieldAccessError::MemberResolution(_)));
            let after = context.member_loading_work().unwrap();
            assert_eq!(after.utf16_units, before_work.utf16_units);
            assert_eq!(after.scratch_entries, before_work.scratch_entries);
            assert_eq!(after.work_units, before_work.work_units + 2);
        }
        assert_eq!(d.member_resolution_usage(), usage);
        assert_eq!(snapshot(&d.ledger), before);
    }
}
#[test]
fn terminal_missing_field_is_replayed_without_late_category_or_receiver_authority() {
    let (mut d, mut heap, c) = fixture();
    {
        let mut context = open(&mut d, &mut heap);
        for _ in 0..2 {
            assert_eq!(
                context.read_field(c, 15, Reference::null()),
                Err(FieldAccessError::Java(FieldJavaError::NoSuchField))
            );
        }
    }
    assert_eq!(
        d.member_site(c, 15).unwrap(),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::NoSuchField
        ))
    );
    {
        let mut context = open(&mut d, &mut heap);
        assert_eq!(
            context.read_field(c, 11, Reference::null()),
            Err(FieldAccessError::Java(FieldJavaError::NullPointer))
        );
    }
    assert!(matches!(
        d.member_site(c, 11).unwrap(),
        Some(MemberResolutionOutcome::Field(_))
    ));
}
#[test]
fn corrupted_terminal_key_tag_failure_and_qualifier_are_rejected_before_observation() {
    for damaged in 0..6 {
        let (mut d, mut heap, c) = fixture();
        paid(&mut d, &mut heap, c);
        let record = d.members.as_mut().unwrap().sites.get_mut(&(c, 11)).unwrap();
        match damaged {
            0 => record.key.1 = 16,
            1 => record.tag = 10,
            2 => record.value = Cached::Failure(MemberResolutionFailure::NoSuchMethod),
            3 => {
                if let Cached::Field(ref mut f) = record.value {
                    f.protected_receiver = Some(ClassId {
                        registry: c.registry,
                        index: usize::MAX,
                    });
                }
            }
            4 => {
                record.owner = ClassId {
                    registry: c.registry,
                    index: usize::MAX,
                }
            }
            _ => {
                if let Cached::Field(ref mut f) = record.value {
                    f.field = FieldId {
                        owner: ClassId {
                            registry: c.registry,
                            index: usize::MAX,
                        },
                        index: 0,
                    };
                }
            }
        }
        assert!(d.member_site(c, 11).is_err());
        assert!(open(&mut d, &mut heap).select_field_id(c, 11).is_err());
    }
}
#[test]
fn damaged_ticket_never_displaces_an_existing_terminal_or_publishes_an_alias() {
    for damage in 0..5 {
        let (mut d, mut heap, c) = fixture();
        let field = paid(&mut d, &mut heap, c);
        let usage = d.member_resolution_usage();
        let before = snapshot(&d.ledger);
        {
            let mut context = open(&mut d, &mut heap);
            let a = context.member.as_mut().unwrap();
            let mut ticket = match a.begin(context.registry, context.heap, c, 16).unwrap() {
                Err(t) => t,
                _ => panic!("cold alias"),
            };
            match damage {
                0 => ticket.next += 1,
                1 => ticket.tag = 10,
                2 => ticket.key.1 = 11,
                3 => a.site.origin = None,
                _ => a.store.usage.sites += 1,
            }
            let value = Cached::Field(Selected {
                field,
                symbolic_owner: c,
                protected_receiver: None,
            });
            assert!(a.commit(context.registry, ticket, c, value).is_err());
            if damage == 4 {
                a.store.usage.sites -= 1;
            }
        }
        assert_eq!(snapshot(&d.ledger), before);
        assert_eq!(d.member_resolution_usage(), usage);
        assert_eq!(d.member_site(c, 16).unwrap(), None);
        assert_eq!(
            d.member_site(c, 11).unwrap(),
            Some(MemberResolutionOutcome::Field(field))
        );
    }
}
#[test]
fn corrupt_witness_counts_and_caps_refuse_context_or_preparation_without_effects() {
    for damage in 0..3 {
        let (mut d, mut heap, c) = fixture();
        let before = snapshot(&d.ledger);
        let plan = build_hierarchy(
            &d.registry,
            c,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        let store = d.members.as_mut().unwrap();
        match damage {
            0 => store.usage.prepared_classes += 1,
            1 => store.limits.max_prepared_classes = 0,
            _ => {
                store.prepared.remove(&c);
                store.usage.prepared_classes -= 1;
            }
        }
        assert!(
            d.member_context_unnamed_modules(
                &mut heap,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default()
            )
            .is_err()
        );
        if damage != 2 {
            assert!(
                d.context(LoadingWorkLimits::default())
                    .prepare_member_owner(&plan)
                    .is_err()
            );
        }
        assert_eq!(snapshot(&d.ledger), before);
    }
}
#[test]
fn witness_reservation_failure_and_old_preparation_do_not_mint_evidence() {
    let (mut d, plan, _, _) = super::super::tests::graph();
    d.enable_member_resolution(MemberResolutionLimits::default())
        .unwrap();
    d.context(LoadingWorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
    assert_eq!(d.member_resolution_usage().prepared_classes, 0);
    let before = snapshot(&d.ledger);
    {
        let mut cx = d.context(LoadingWorkLimits::default());
        cx.work.fail_reserve_after = Some(0);
        assert!(cx.prepare_member_owner(&plan).is_err());
    }
    assert_eq!(snapshot(&d.ledger), before);
    assert_eq!(d.member_resolution_usage().prepared_classes, 0);
    d.context(LoadingWorkLimits::default())
        .prepare_member_owner(&plan)
        .unwrap();
    assert_eq!(d.member_resolution_usage().prepared_classes, 1);
}
#[test]
fn damaged_actual_record_and_foreign_heap_are_host_failures_without_cache_effects() {
    let (mut d, mut heap, c) = fixture();
    let before = d.member_resolution_usage();
    let loader = d.registry.defining_loader(c).unwrap();
    let name = units("p/C");
    let mut w = Work {
        limits: LoadingWorkLimits::default(),
        usage: Default::default(),
        site: None,
        fail_reserve_after: None,
    };
    w.name(&name).unwrap();
    let id = d.ledger.name_id(&name, &mut w).unwrap().0.unwrap();
    let pair = d.ledger.pair_index[&(id, loader)];
    d.ledger.pairs[pair].actual = None;
    assert!(matches!(
        d.member_context_unnamed_modules(
            &mut heap,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default()
        )
        .err()
        .unwrap()
        .kind,
        M::MissingInitiation { .. }
    ));
    assert_eq!(d.member_resolution_usage(), before);
    let (mut other, _, _) = fixture();
    assert!(
        other
            .member_context_unnamed_modules(
                &mut heap,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default()
            )
            .is_err()
    );
}
#[test]
fn original_sources_clone_and_direct_java_projection_remain_distinct() {
    use std::error::Error;
    let source = FieldAccessError::InvalidState("retained source");
    let e = Site::default().error(M::Field(Box::new(source.clone())));
    assert_eq!(*e, e.clone().as_ref().clone());
    assert_eq!(e.source().unwrap().to_string(), source.to_string());
    assert_eq!(field_failure(FieldJavaError::NullPointer), None);
    assert_eq!(method_failure(MethodJavaError::AbstractMethod), None);
    assert!(invalid(&Site::default().error(M::InvalidState("guard"))));
}
#[test]
fn misdelegated_witnessed_heap_is_incompatible_not_missing_initiation() {
    let (mut d, _, c) = fixture();
    let loader = d.create_loader().unwrap();
    let alternate = object(&mut d, loader);
    let alternate_plan = build_hierarchy(
        &d.registry,
        alternate,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    d.context(LoadingWorkLimits::default())
        .prepare_member_owner(&alternate_plan)
        .unwrap();
    struct Alternate(ClassId);
    impl crate::HierarchyResolver for Alternate {
        fn resolve(
            &mut self,
            _: &DefinitionRegistry,
            _: LoaderId,
            _: &[u16],
        ) -> std::result::Result<Option<ClassId>, crate::ResolveError> {
            Ok(Some(self.0))
        }
    }
    let wrong = build_hierarchy(
        &d.registry,
        c,
        &mut Alternate(alternate),
        HierarchyOptions::default(),
    )
    .unwrap();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&d.registry, &[&wrong]).unwrap();
    let before = snapshot(&d.ledger);
    let usage = d.member_resolution_usage();
    let error = d
        .member_context_unnamed_modules(
            &mut heap,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .err()
        .unwrap();
    assert!(invalid(&error));
    assert_eq!(snapshot(&d.ledger), before);
    assert_eq!(d.member_resolution_usage(), usage);
}
#[test]
fn failed_witness_producer_keeps_paid_leaf_prefix_without_minting_root() {
    let (mut probe, plan, _, _) = super::super::tests::graph();
    probe
        .enable_member_resolution(MemberResolutionLimits::default())
        .unwrap();
    let mut cx = probe.context(LoadingWorkLimits::default());
    cx.prepare_member_owner(&plan).unwrap();
    let end = cx.usage().work_units;
    let mut observed = false;
    for max_work_units in 0..end {
        let (mut d, plan, a, b) = super::super::tests::graph();
        d.enable_member_resolution(MemberResolutionLimits::default())
            .unwrap();
        let result = d
            .context(LoadingWorkLimits {
                max_work_units,
                ..Default::default()
            })
            .prepare_member_owner(&plan);
        if result.is_err() && d.usage().unions != 0 {
            assert_eq!(d.member_resolution_usage().prepared_classes, 0);
            assert!(
                d.context(LoadingWorkLimits::default())
                    .constrained(&units("p/K"), a, b)
                    .unwrap()
            );
            d.context(LoadingWorkLimits::default())
                .prepare_member_owner(&plan)
                .unwrap();
            assert_eq!(d.member_resolution_usage().prepared_classes, 1);
            observed = true;
            break;
        }
    }
    assert!(observed, "late producer failure after a committed leaf");
}
#[test]
fn descriptor_utf_failure_reports_field_and_preserves_nonempty_ledger() {
    let (mut d, mut heap, c) = fixture();
    paid(&mut d, &mut heap, c);
    let before = snapshot(&d.ledger);
    {
        let mut cx = open(&mut d, &mut heap);
        let a = cx.member.as_mut().unwrap();
        a.site = Site {
            origin: Some(c),
            index: Some(16),
            position: Some(MemberDescriptorPosition::Return),
        };
        a.work.limits.max_utf16_units = a.work.usage.utf16_units;
        let loader = cx.registry.defining_loader(c).unwrap();
        let e = a
            .descriptor(
                cx.registry,
                &units("Ljava/lang/Object;"),
                true,
                loader,
                loader,
                None,
            )
            .unwrap_err();
        assert_eq!(e.position, Some(MemberDescriptorPosition::Field));
    }
    assert_eq!(snapshot(&d.ledger), before);
    assert_eq!(d.member_site(c, 16).unwrap(), None);
}
#[test]
fn method_terminal_corruptions_cannot_cross_role_or_affinity_boundaries() {
    for damage in 0..4 {
        let (mut d, mut heap, c) = fixture();
        open(&mut d, &mut heap).resolve_static(c, 13).unwrap();
        let r = d.members.as_mut().unwrap().sites.get_mut(&(c, 13)).unwrap();
        match damage {
            0 => r.value = Cached::Failure(MemberResolutionFailure::NoSuchField),
            1 => {
                if let Cached::Method(ref mut m) = r.value {
                    m.protected_receiver = Some(ClassId {
                        registry: c.registry,
                        index: usize::MAX,
                    });
                }
            }
            2 => {
                if let Cached::Method(ref mut m) = r.value {
                    m.method = MethodId {
                        owner: ClassId {
                            registry: u64::MAX,
                            index: 0,
                        },
                        index: 0,
                    };
                }
            }
            _ => {
                r.key.0 = ClassId {
                    registry: c.registry,
                    index: usize::MAX,
                }
            }
        }
        assert!(d.member_site(c, 13).is_err());
        assert!(open(&mut d, &mut heap).resolve_static(c, 13).is_err());
    }
}

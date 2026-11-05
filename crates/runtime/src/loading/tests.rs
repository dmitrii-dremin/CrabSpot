use super::*;

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn words(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|n| n.to_be_bytes()).collect()
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn utf(&mut self, s: &str) -> u16 {
        assert!(s.is_ascii());
        let mut b = vec![1];
        b.extend(words(&[s.len() as u16]));
        b.extend(s.bytes());
        self.0.push(b);
        self.0.len() as u16
    }
    fn class(&mut self, s: &str) -> u16 {
        let n = self.utf(s);
        let mut b = vec![7];
        b.extend(words(&[n]));
        self.0.push(b);
        self.0.len() as u16
    }
}
pub(super) fn definition(
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    methods: &[(&str, &str, u16)],
) -> Vec<u8> {
    let mut pool = Pool::default();
    let this = pool.class(name);
    let parent = parent.map(|s| pool.class(s)).unwrap_or(0);
    let interfaces: Vec<_> = interfaces.iter().map(|s| pool.class(s)).collect();
    let methods: Vec<_> = methods
        .iter()
        .map(|(n, d, f)| (pool.utf(n), pool.utf(d), *f))
        .collect();
    let mut b = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    b.extend(words(&[(pool.0.len() + 1) as u16]));
    b.extend(pool.0.into_iter().flatten());
    b.extend(words(&[flags, this, parent, interfaces.len() as u16]));
    b.extend(words(&interfaces));
    b.extend(words(&[0, methods.len() as u16]));
    for (n, d, f) in methods {
        b.extend(words(&[f, n, d, 0]));
    }
    b.extend(words(&[0]));
    b
}
fn domain() -> LoadingDomain {
    LoadingDomain::new(RegistryOptions::default(), LoadingDomainLimits::default()).unwrap()
}
fn work() -> Work {
    Work {
        limits: LoadingWorkLimits::default(),
        usage: LoadingWorkUsage::default(),
        site: None,
        fail_reserve_after: None,
    }
}
#[derive(Debug, PartialEq)]
pub(super) struct Snapshot {
    usage: LoadingDomainUsage,
    names: Vec<Vec<u16>>,
    pairs: Vec<(usize, usize, Option<ClassId>, Option<ClassId>)>,
    name_index: HashMap<u64, Vec<usize>>,
    pair_index: HashMap<(usize, LoaderId), usize>,
}
pub(super) fn snapshot(ledger: &Ledger) -> Snapshot {
    Snapshot {
        usage: ledger.usage,
        names: ledger.names.clone(),
        pairs: ledger
            .pairs
            .iter()
            .map(|p| (p.parent, p.size, p.known, p.actual))
            .collect(),
        name_index: ledger.name_index.clone(),
        pair_index: ledger.pair_index.clone(),
    }
}
pub(super) fn object(domain: &mut LoadingDomain, loader: LoaderId) -> ClassId {
    domain
        .context(LoadingWorkLimits::default())
        .define(
            loader,
            &units("java/lang/Object"),
            &definition("java/lang/Object", None, &[], 0x21, &[]),
            ParseOptions::default(),
        )
        .unwrap()
}

#[test]
fn empty_and_self_work_boundaries_are_hand_counted_and_nonmutating() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let n = units("p/K");
    let before = snapshot(&d.ledger);
    for op in 0..3 {
        let mut c = d.context(LoadingWorkLimits {
            max_utf16_units: 3,
            max_work_units: 5,
            max_scratch_entries: 0,
        });
        match op {
            0 => assert_eq!(c.lookup_initiated(a, &n).unwrap(), None),
            1 => assert!(c.constrained(&n, a, a).unwrap()),
            _ => c.impose(&n, a, a).unwrap(),
        };
        assert_eq!(
            c.usage(),
            LoadingWorkUsage {
                utf16_units: 3,
                work_units: 5,
                scratch_entries: 0
            }
        );
    }
    assert_eq!(snapshot(&d.ledger), before);
}
#[test]
fn all_work_batches_and_retained_batches_fail_atomically_on_last_counter() {
    let mut w = work();
    w.limits.max_scratch_entries = 0;
    assert!(w.charge(1, 1, 1).is_err());
    assert_eq!(w.usage, LoadingWorkUsage::default());
    w.limits.max_scratch_entries = u64::MAX;
    w.usage.scratch_entries = u64::MAX;
    let before = w.usage;
    assert!(matches!(
        w.charge(1, 1, 1),
        Err(E::AccountingOverflow {
            resource: R::ScratchEntries
        })
    ));
    assert_eq!(w.usage, before);
    let mut ledger = Ledger::new(LoadingDomainLimits {
        max_names: u64::MAX,
        max_name_units: u64::MAX,
        max_pairs: u64::MAX,
        max_initiations: u64::MAX,
        max_unions: u64::MAX,
    });
    ledger.usage.unions = u64::MAX;
    let before = snapshot(&ledger);
    assert!(matches!(
        ledger.prospective_for_test(1, 1, 1, 1, 1),
        Err(E::AccountingOverflow {
            resource: R::Unions
        })
    ));
    assert_eq!(snapshot(&ledger), before);
}
#[test]
fn every_added_definition_reservation_failure_preserves_nonempty_registry_and_ledger() {
    for failed in 0..6 {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let existing = object(&mut d, a);
        let before = snapshot(&d.ledger);
        let count = d.registry.definition_count();
        let bytes = d.registry.total_definition_bytes();
        {
            let mut c = d.context(LoadingWorkLimits::default());
            c.work.fail_reserve_after = Some(failed);
            assert!(matches!(
                c.define(
                    a,
                    &units("p/New"),
                    &definition("p/New", Some("java/lang/Object"), &[], 0x21, &[]),
                    ParseOptions::default()
                )
                .unwrap_err()
                .kind,
                E::AllocationFailure
            ));
        }
        assert_eq!(snapshot(&d.ledger), before);
        assert_eq!(d.registry.definition_count(), count);
        assert_eq!(d.registry.total_definition_bytes(), bytes);
        assert_eq!(
            d.registry
                .lookup_class(a, &units("java/lang/Object"))
                .unwrap(),
            Some(existing)
        );
        let next = d
            .context(LoadingWorkLimits::default())
            .define(
                a,
                &units("p/New"),
                &definition("p/New", Some("java/lang/Object"), &[], 0x21, &[]),
                ParseOptions::default(),
            )
            .unwrap();
        assert_eq!(next.index, count);
    }
}
#[test]
fn every_late_work_and_utf_failure_of_one_union_preserves_existing_meaning() {
    let mut prototype = domain();
    let a = prototype.create_loader().unwrap();
    let b = prototype.create_loader().unwrap();
    object(&mut prototype, a);
    let n = units("p/K");
    let mut w = work();
    w.name(&n).unwrap();
    prototype.ledger.impose(&n, a, b, &mut w).unwrap();
    let full = w.usage;
    for limit in 0..full.work_units {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        object(&mut d, a);
        let before = snapshot(&d.ledger);
        let mut w = work();
        w.limits.max_work_units = limit;
        let result = w.name(&n).and_then(|_| d.ledger.impose(&n, a, b, &mut w));
        assert!(result.is_err());
        assert_eq!(snapshot(&d.ledger), before);
    }
    for limit in 0..full.utf16_units {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        object(&mut d, a);
        let before = snapshot(&d.ledger);
        let mut w = work();
        w.limits.max_utf16_units = limit;
        assert!(
            w.name(&n)
                .and_then(|_| d.ledger.impose(&n, a, b, &mut w))
                .is_err()
        );
        assert_eq!(snapshot(&d.ledger), before);
    }
}
#[test]
fn staged_definition_authority_is_bound_to_registry_and_append_snapshot() {
    let mut a = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let l = a.create_loader().unwrap();
    let data = definition("java/lang/Object", None, &[], 0x21, &[]);
    let prepared = a
        .stage_definition(
            l,
            &units("java/lang/Object"),
            &data,
            ParseOptions::default(),
        )
        .unwrap();
    a.validate_definition_stage(&prepared).unwrap();
    let b = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    assert!(b.validate_definition_stage(&prepared).is_err());
    a.define(
        l,
        &units("p/X"),
        &definition("p/X", Some("java/lang/Object"), &[], 0x21, &[]),
        ParseOptions::default(),
    )
    .unwrap();
    assert!(a.validate_definition_stage(&prepared).is_err());
}
#[test]
fn work_and_domain_debug_do_not_dump_inventory_and_error_source_is_registry() {
    use std::error::Error;
    let mut d = domain();
    let l = d.create_loader().unwrap();
    object(&mut d, l);
    let foreign = domain().create_loader().unwrap();
    let error = d
        .context(LoadingWorkLimits::default())
        .lookup_initiated(foreign, &units("p/X"))
        .unwrap_err();
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<RegistryError>()
            .is_some()
    );
    assert!(!format!("{d:?}").contains("java/lang/Object"));
}

// The fixture deliberately supplies only metadata; it is never executed.
pub(super) fn graph() -> (LoadingDomain, crate::HierarchyPlan, LoaderId, LoaderId) {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let object = object(&mut d, a);
    let base = d
        .context(LoadingWorkLimits::default())
        .define(
            a,
            &units("p/Base"),
            &definition(
                "p/Base",
                Some("java/lang/Object"),
                &[],
                0x21,
                &[("m", "(Lp/K;)V", 0x101)],
            ),
            ParseOptions::default(),
        )
        .unwrap();
    d.context(LoadingWorkLimits::default())
        .record_initiation(b, &units("java/lang/Object"), object)
        .unwrap();
    d.context(LoadingWorkLimits::default())
        .record_initiation(b, &units("p/Base"), base)
        .unwrap();
    let child = d
        .context(LoadingWorkLimits::default())
        .define(
            b,
            &units("q/Child"),
            &definition(
                "q/Child",
                Some("p/Base"),
                &[],
                0x21,
                &[("m", "(Lp/K;)V", 0x101)],
            ),
            ParseOptions::default(),
        )
        .unwrap();
    struct Resolver {
        object: ClassId,
        base: ClassId,
    }
    impl crate::HierarchyResolver for Resolver {
        fn resolve(
            &mut self,
            registry: &DefinitionRegistry,
            loader: LoaderId,
            name: &[u16],
        ) -> std::result::Result<Option<ClassId>, crate::ResolveError> {
            if name == units("java/lang/Object") {
                Ok(Some(self.object))
            } else if name == units("p/Base") {
                Ok(Some(self.base))
            } else {
                registry
                    .lookup_class(loader, name)
                    .map_err(|e| crate::ResolveError::new(e.to_string()))
            }
        }
    }
    let plan = crate::build_hierarchy(
        &d.registry,
        child,
        &mut Resolver { object, base },
        crate::HierarchyOptions::default(),
    )
    .unwrap();
    (d, plan, a, b)
}
#[test]
fn successful_emission_restores_origin_before_later_charged_operations() {
    let (mut d, plan, a, b) = graph();
    let mut saw_late_work = false;
    let mut saw_late_scratch = false;
    let mut probe = d.context(LoadingWorkLimits::default());
    probe.prepare_constraints(&plan).unwrap();
    let full = probe.usage();
    for resource in [R::WorkUnits, R::ScratchEntries] {
        let end = if resource == R::WorkUnits {
            full.work_units
        } else {
            full.scratch_entries
        };
        for cap in 0..end {
            let (mut d, plan, a, b) = graph();
            let mut limits = LoadingWorkLimits::default();
            if resource == R::WorkUnits {
                limits.max_work_units = cap
            } else {
                limits.max_scratch_entries = cap
            }
            let error = d.context(limits).prepare_constraints(&plan).unwrap_err();
            if d.context(LoadingWorkLimits::default())
                .constrained(&units("p/K"), a, b)
                .unwrap()
            {
                let site = error.site.unwrap();
                assert_eq!(site.related, None);
                assert_eq!(site.position, None);
                if resource == R::WorkUnits {
                    saw_late_work = true
                } else {
                    saw_late_scratch = true
                }
            }
        }
    }
    assert!(saw_late_work);
    assert!(saw_late_scratch);
    let _ = (a, b);
}

#[test]
fn new_wordy_definition_pays_query_copy_and_validation_spelling_scans() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    object(&mut d, a);
    let name = "LongDefinitionSpelling";
    let n = units(name);
    let mut c = d.context(LoadingWorkLimits::default());
    c.define(
        a,
        &n,
        &definition(name, Some("java/lang/Object"), &[], 0x21, &[]),
        ParseOptions::default(),
    )
    .unwrap();
    assert_eq!(c.usage().utf16_units, 3 * n.len() as u64);
}
#[test]
fn final_definition_prepublication_work_failure_keeps_the_next_index_available() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    object(&mut d, a);
    let name = units("p/X");
    let bytes = definition("p/X", Some("java/lang/Object"), &[], 0x21, &[]);
    let mut probe = d.context(LoadingWorkLimits::default());
    probe
        .define(a, &name, &bytes, ParseOptions::default())
        .unwrap();
    let full = probe.usage().work_units;
    let mut d = domain();
    let a = d.create_loader().unwrap();
    object(&mut d, a);
    let prior = snapshot(&d.ledger);
    let count = d.registry.definition_count();
    let mut c = d.context(LoadingWorkLimits {
        max_work_units: full - 1,
        ..Default::default()
    });
    assert!(matches!(
        c.define(a, &name, &bytes, ParseOptions::default())
            .unwrap_err()
            .kind,
        E::ResourceLimit {
            resource: R::WorkUnits,
            ..
        }
    ));
    assert_eq!(snapshot(&d.ledger), prior);
    assert_eq!(d.registry.definition_count(), count);
    let next = d
        .context(LoadingWorkLimits::default())
        .define(a, &name, &bytes, ParseOptions::default())
        .unwrap();
    assert_eq!(next.index, count);
}

#[test]
fn every_counter_overflow_is_structured_without_advancing_siblings() {
    for resource in [R::Utf16Units, R::WorkUnits, R::ScratchEntries] {
        let mut w = work();
        w.limits = LoadingWorkLimits {
            max_utf16_units: u64::MAX,
            max_work_units: u64::MAX,
            max_scratch_entries: u64::MAX,
        };
        match resource {
            R::Utf16Units => w.usage.utf16_units = u64::MAX,
            R::WorkUnits => w.usage.work_units = u64::MAX,
            _ => w.usage.scratch_entries = u64::MAX,
        }
        let before = w.usage;
        assert!(matches!(w.charge(1,1,1),Err(E::AccountingOverflow{resource:r})if r==resource));
        assert_eq!(w.usage, before);
    }
    for resource in [R::Names, R::NameUnits, R::Pairs, R::Initiations, R::Unions] {
        let mut ledger = Ledger::new(LoadingDomainLimits {
            max_names: u64::MAX,
            max_name_units: u64::MAX,
            max_pairs: u64::MAX,
            max_initiations: u64::MAX,
            max_unions: u64::MAX,
        });
        match resource {
            R::Names => ledger.usage.names = u64::MAX,
            R::NameUnits => ledger.usage.name_units = u64::MAX,
            R::Pairs => ledger.usage.pairs = u64::MAX,
            R::Initiations => ledger.usage.initiations = u64::MAX,
            _ => ledger.usage.unions = u64::MAX,
        }
        let before = snapshot(&ledger);
        assert!(
            matches!(ledger.prospective_for_test(1,1,1,1,1),Err(E::AccountingOverflow{resource:r})if r==resource)
        );
        assert_eq!(snapshot(&ledger), before);
    }
}
#[test]
fn existing_pair_binding_and_join_require_no_added_reservations() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let c = d.create_loader().unwrap();
    let object = object(&mut d, c);
    let name = units("java/lang/Object");
    d.context(LoadingWorkLimits::default())
        .impose(&name, a, b)
        .unwrap();
    let mut ctx = d.context(LoadingWorkLimits::default());
    ctx.work.fail_reserve_after = Some(0);
    ctx.record_initiation(a, &name, object).unwrap();
    ctx.impose(&name, a, c).unwrap();
    assert_eq!(ctx.lookup_initiated(b, &name).unwrap(), None);
}
#[test]
fn existing_name_delegation_reservation_failures_preserve_known_identity() {
    for fail in 0..2 {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        let object = object(&mut d, a);
        let before = snapshot(&d.ledger);
        let mut ctx = d.context(LoadingWorkLimits::default());
        ctx.work.fail_reserve_after = Some(fail);
        assert!(matches!(
            ctx.record_initiation(b, &units("java/lang/Object"), object)
                .unwrap_err()
                .kind,
            E::AllocationFailure
        ));
        assert_eq!(snapshot(&d.ledger), before);
        d.context(LoadingWorkLimits::default())
            .record_initiation(b, &units("java/lang/Object"), object)
            .unwrap();
    }
}

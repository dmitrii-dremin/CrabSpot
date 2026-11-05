//! Independent checked-loading state tests; no application class loader or reference JVM runs here.
pub mod loading_support;
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, HandleKind, LoaderId, LoadingConstraintError as Error,
    LoadingConstraintErrorKind as E, LoadingConstraintResource as R,
    LoadingDescriptorPosition as Position, LoadingDomain, LoadingDomainLimits as Limits,
    LoadingDomainUsage as Usage, LoadingWorkLimits as WorkLimits, LoadingWorkUsage as WorkUsage,
    RegistryError, RegistryOptions,
};
use loading_support::*;
fn impose(d: &mut LoadingDomain, n: &str, a: LoaderId, b: LoaderId) -> Result<(), Box<Error>> {
    d.context(WorkLimits::default()).impose(&name(n), a, b)
}
fn linked(d: &mut LoadingDomain, n: &str, a: LoaderId, b: LoaderId) -> bool {
    d.context(WorkLimits::default())
        .constrained(&name(n), a, b)
        .unwrap()
}
fn actual(d: &mut LoadingDomain, n: &str, l: LoaderId) -> Option<ClassId> {
    d.context(WorkLimits::default())
        .lookup_initiated(l, &name(n))
        .unwrap()
}
#[test]
fn fresh_domain_loaders_and_queries_do_not_create_loading_records() {
    let mut d = domain();
    assert_eq!(d.usage(), Usage::default());
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    assert_eq!(d.usage(), Usage::default());
    assert_eq!(actual(&mut d, "unknown/T", a), None);
    assert!(!linked(&mut d, "unknown/T", a, b));
    assert!(linked(&mut d, "unknown/T", a, a));
    impose(&mut d, "unknown/T", a, a).unwrap();
    assert_eq!(d.usage(), Usage::default());
}
#[test]
fn direct_definition_records_its_loader_and_delegation_does_not_change_defining_lookup() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let class = define(&mut d, a, &Class::new("p/C"));
    assert_eq!(actual(&mut d, "p/C", a), Some(class));
    assert_eq!(actual(&mut d, "p/C", b), None);
    d.context(WorkLimits::default())
        .record_initiation(b, &name("p/C"), class)
        .unwrap();
    assert_eq!(actual(&mut d, "p/C", b), Some(class));
    assert_eq!(d.registry().lookup_class(b, &name("p/C")).unwrap(), None);
    assert_eq!(
        d.registry().lookup_class(a, &name("p/C")).unwrap(),
        Some(class)
    );
    assert_eq!(
        d.usage(),
        Usage {
            names: 1,
            name_units: 3,
            pairs: 2,
            initiations: 2,
            unions: 0
        }
    );
    assert!(!linked(&mut d, "p/C", a, b));
    impose(&mut d, "p/C", a, b).unwrap();
    assert!(linked(&mut d, "p/C", a, b));
    assert_eq!(d.usage().unions, 1);
}
#[test]
fn unknown_pair_union_and_first_actual_identity_have_hand_derived_retained_counts() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    impose(&mut d, "p/C", a, b).unwrap();
    assert_eq!(
        d.usage(),
        Usage {
            names: 1,
            name_units: 3,
            pairs: 2,
            initiations: 0,
            unions: 1
        }
    );
    let class = define(&mut d, a, &Class::new("p/C"));
    assert_eq!(
        d.usage(),
        Usage {
            names: 1,
            name_units: 3,
            pairs: 2,
            initiations: 1,
            unions: 1
        }
    );
    assert_eq!(actual(&mut d, "p/C", b), None);
    assert!(linked(&mut d, "p/C", a, b));
    let before = d.usage();
    d.context(WorkLimits::default())
        .record_initiation(b, &name("p/C"), class)
        .unwrap();
    assert_eq!(d.usage().initiations, before.initiations + 1);
    assert_eq!(d.usage().pairs, before.pairs);
}
#[test]
fn three_loader_transitive_components_preserve_known_identity_without_initiating_the_third() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let c = d.create_loader().unwrap();
    let class = define(&mut d, a, &Class::new("p/C"));
    impose(&mut d, "p/C", a, b).unwrap();
    impose(&mut d, "p/C", b, c).unwrap();
    assert!(linked(&mut d, "p/C", a, c));
    assert_eq!(actual(&mut d, "p/C", b), None);
    assert_eq!(actual(&mut d, "p/C", c), None);
    let before = d.usage();
    for (left, right) in [(a, c), (c, a), (b, a), (c, b)] {
        impose(&mut d, "p/C", left, right).unwrap();
    }
    assert_eq!(d.usage(), before);
    d.context(WorkLimits::default())
        .record_initiation(c, &name("p/C"), class)
        .unwrap();
    assert_eq!(
        d.usage(),
        Usage {
            names: 1,
            name_units: 3,
            pairs: 3,
            initiations: 2,
            unions: 2
        }
    );
    assert!(!linked(&mut d, "other/C", a, c));
}
#[test]
fn both_known_same_identity_stays_disconnected_until_an_explicit_nonredundant_union() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let class = define(&mut d, a, &Class::new("p/C"));
    d.context(WorkLimits::default())
        .record_initiation(b, &name("p/C"), class)
        .unwrap();
    assert!(!linked(&mut d, "p/C", a, b));
    let before = d.usage();
    impose(&mut d, "p/C", a, b).unwrap();
    assert_eq!(d.usage().unions, before.unions + 1);
    assert!(linked(&mut d, "p/C", a, b));
}
#[test]
fn conflicting_known_components_fail_without_merging_and_preserve_both_direction_payloads() {
    for reverse in [false, true] {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        let ca = define(&mut d, a, &Class::new("p/C"));
        let cb = define(&mut d, b, &Class::new("p/C"));
        let before = d.usage();
        let (left, right, lc, rc) = if reverse {
            (b, a, cb, ca)
        } else {
            (a, b, ca, cb)
        };
        let e = impose(&mut d, "p/C", left, right).unwrap_err();
        assert!(
            matches!(e.kind,E::ConstraintConflict{left:l,right:r,left_class:x,right_class:y}if l==left&&r==right&&x==lc&&y==rc)
        );
        assert!(e.site.is_none());
        assert_eq!(d.usage(), before);
        assert!(!linked(&mut d, "p/C", a, b));
        assert_eq!(actual(&mut d, "p/C", a), Some(ca));
        assert_eq!(actual(&mut d, "p/C", b), Some(cb));
    }
}
#[test]
fn late_actual_record_conflict_preserves_earlier_transitive_obligations() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let c = d.create_loader().unwrap();
    let other = d.create_loader().unwrap();
    let ca = define(&mut d, a, &Class::new("p/C"));
    let cb = define(&mut d, other, &Class::new("p/C"));
    impose(&mut d, "p/C", a, b).unwrap();
    impose(&mut d, "p/C", b, c).unwrap();
    let before = d.usage();
    let e = d
        .context(WorkLimits::default())
        .record_initiation(c, &name("p/C"), cb)
        .unwrap_err();
    assert!(
        matches!(e.kind,E::InitiationConflict{loader,existing,incoming}if loader==c&&existing==ca&&incoming==cb)
    );
    assert_eq!(d.usage(), before);
    assert_eq!(actual(&mut d, "p/C", c), None);
    assert!(linked(&mut d, "p/C", a, c));
    d.context(WorkLimits::default())
        .record_initiation(c, &name("p/C"), ca)
        .unwrap();
    assert_eq!(actual(&mut d, "p/C", c), Some(ca));
}
#[test]
fn wrong_returned_name_precedes_record_conflicts_but_direct_mismatch_keeps_registry_category() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let ca = define(&mut d, a, &Class::new("p/A"));
    let cb = define(&mut d, a, &Class::new("p/B"));
    d.context(WorkLimits::default())
        .record_initiation(b, &name("p/A"), ca)
        .unwrap();
    let before = d.usage();
    let e = d
        .context(WorkLimits::default())
        .record_initiation(b, &name("p/A"), cb)
        .unwrap_err();
    assert!(matches!(e.kind,E::WrongReturnedName{loader,class}if loader==b&&class==cb));
    assert_eq!(d.usage(), before);
    let e = d
        .context(WorkLimits::default())
        .define(
            b,
            &name("p/D"),
            &Class::new("p/E").bytes(),
            ParseOptions::default(),
        )
        .unwrap_err();
    assert!(
        matches!(e.kind,E::Registry(ref r)if matches!(r.as_ref(),RegistryError::NameMismatch{..}))
    );
    assert_eq!(d.usage(), before);
}
#[test]
fn duplicate_actual_initiation_rejects_derivation_before_malformed_bytes_are_parsed() {
    for delegated in [false, true] {
        let mut d = domain();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        let ca = define(&mut d, a, &Class::new("p/C"));
        let loader = if delegated {
            d.context(WorkLimits::default())
                .record_initiation(b, &name("p/C"), ca)
                .unwrap();
            b
        } else {
            a
        };
        let before = d.usage();
        let definitions = d.registry().definition_count();
        let e = d
            .context(WorkLimits::default())
            .define(loader, &name("p/C"), &[0], ParseOptions::default())
            .unwrap_err();
        assert!(
            matches!(e.kind,E::DuplicateDerivation{loader:l,existing}if l==loader&&existing==ca)
        );
        assert_eq!(d.usage(), before);
        assert_eq!(d.registry().definition_count(), definitions);
    }
}
#[test]
fn implied_identity_is_checked_after_admission_and_failed_definition_publishes_nothing() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let ca = define(&mut d, a, &Class::new("p/C"));
    impose(&mut d, "p/C", a, b).unwrap();
    let before = d.usage();
    let count = d.registry().definition_count();
    let bytes = d.registry().total_definition_bytes();
    let e = d
        .context(WorkLimits::default())
        .define(b, &name("p/C"), &[0], ParseOptions::default())
        .unwrap_err();
    assert!(matches!(e.kind,E::Registry(ref r)if matches!(r.as_ref(),RegistryError::Parse(_))));
    let e = d
        .context(WorkLimits::default())
        .define(
            b,
            &name("p/C"),
            &Class::new("p/C").bytes(),
            ParseOptions::default(),
        )
        .unwrap_err();
    assert!(matches!(e.kind,E::DefinitionConflict{loader,required}if loader==b&&required==ca));
    assert_eq!(d.usage(), before);
    assert_eq!(d.registry().definition_count(), count);
    assert_eq!(d.registry().total_definition_bytes(), bytes);
    assert_eq!(d.registry().lookup_class(b, &name("p/C")).unwrap(), None);
    assert_eq!(actual(&mut d, "p/C", b), None);
    d.context(WorkLimits::default())
        .record_initiation(b, &name("p/C"), ca)
        .unwrap();
    assert_eq!(actual(&mut d, "p/C", b), Some(ca));
    let other = define(&mut d, b, &Class::new("p/Other"));
    assert_eq!(d.registry().definition_count(), count + 1);
    assert_eq!(actual(&mut d, "p/Other", b), Some(other));
}
#[test]
fn foreign_handles_and_invalid_names_have_checked_left_right_then_name_order() {
    let mut d = domain();
    let valid = d.create_loader().unwrap();
    let mut foreign = domain();
    let alien = foreign.create_loader().unwrap();
    for (left, right, work) in [(alien, valid, 1), (valid, alien, 2)] {
        let mut ctx = d.context(WorkLimits::default());
        let e = ctx.constrained(&name("bad.name"), left, right).unwrap_err();
        assert!(
            matches!(e.kind,E::Registry(ref r)if matches!(r.as_ref(),RegistryError::InvalidHandle{kind:HandleKind::Loader,..}))
        );
        assert_eq!(ctx.usage().work_units, work);
    }
    for n in ["", "bad.name", "[I", "a//b"] {
        let e = d
            .context(WorkLimits::default())
            .constrained(&name(n), valid, valid)
            .unwrap_err();
        assert!(matches!(e.kind, E::Registry(_)));
    }
    assert_eq!(d.usage(), Usage::default());
}
#[test]
fn legal_ordinary_i_v_names_and_lossless_surrogates_are_not_primitive_descriptors() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    for n in ["I", "V"] {
        let class = define(&mut d, a, &Class::new(n));
        impose(&mut d, n, a, b).unwrap();
        assert!(linked(&mut d, n, a, b));
        assert_eq!(actual(&mut d, n, b), None);
        d.context(WorkLimits::default())
            .record_initiation(b, &name(n), class)
            .unwrap();
    }
    let mut c = Class::new("unused");
    c.name = vec![0xd800, 0x78];
    let class = define(&mut d, a, &c);
    d.context(WorkLimits::default())
        .impose(&c.name, a, b)
        .unwrap();
    assert!(
        d.context(WorkLimits::default())
            .constrained(&c.name, b, a)
            .unwrap()
    );
    d.context(WorkLimits::default())
        .record_initiation(b, &c.name, class)
        .unwrap();
    assert_eq!(
        d.context(WorkLimits::default())
            .lookup_initiated(b, &c.name)
            .unwrap(),
        Some(class)
    );
}
fn declared(
    desc: &str,
    parent_flags: u16,
    child_flags: u16,
) -> (Graph, LoaderId, LoaderId, ClassId, ClassId) {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let parent = g.define(a, Class::new("p/Base").method("take", desc, parent_flags));
    g.bind(b, "p/Base", parent);
    let child = g.define(
        b,
        Class::new("c/Child")
            .parent("p/Base")
            .method("take", desc, child_flags),
    );
    (g, a, b, parent, child)
}
#[test]
fn declared_override_produces_one_unloaded_nominal_leaf_obligation() {
    let (mut g, a, b, parent, child) = declared("(Lvalue/K;)V", 0x101, 0x101);
    let plan = g.plan(child);
    let before = g.domain.usage();
    g.domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
    let after = g.domain.usage();
    assert_eq!(
        (
            after.names - before.names,
            after.name_units - before.name_units,
            after.pairs - before.pairs,
            after.initiations - before.initiations,
            after.unions - before.unions
        ),
        (1, 7, 2, 0, 1)
    );
    assert!(linked(&mut g.domain, "value/K", a, b));
    assert_eq!(actual(&mut g.domain, "value/K", a), None);
    assert_eq!(actual(&mut g.domain, "value/K", b), None);
    assert_ne!(parent, child);
}
#[test]
fn first_conflicting_descriptor_leaf_reports_origin_related_and_parameter_or_return_position() {
    for (desc, position, keeps_p) in [
        ("(Lvalue/P;Lvalue/Q;)V", Position::Parameter(1), true),
        ("(Lvalue/Q;Lvalue/P;)V", Position::Parameter(0), false),
        ("(Lvalue/P;)Lvalue/Q;", Position::Return, true),
        ("(Lvalue/Q;)Lvalue/P;", Position::Parameter(0), false),
    ] {
        let (mut g, a, b, parent, child) = declared(desc, 0x101, 0x101);
        g.define(a, Class::new("value/Q"));
        g.define(b, Class::new("value/Q"));
        let plan = g.plan(child);
        let e = g
            .domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap_err();
        assert!(matches!(e.kind, E::ConstraintConflict { .. }));
        let site = e.site.unwrap();
        assert_eq!(site.class, child);
        assert_eq!(site.origin.unwrap().owner(), child);
        assert_eq!(site.origin.unwrap().index(), 0);
        assert_eq!(site.related.unwrap().owner(), parent);
        assert_eq!(site.position, Some(position));
        assert_eq!(linked(&mut g.domain, "value/P", a, b), keeps_p);
        assert!(!linked(&mut g.domain, "value/Q", a, b));
    }
}
#[test]
fn primitive_and_void_descriptors_skip_constraints_but_reference_array_ranks_use_the_leaf() {
    for (desc, leaves) in [
        ("(IJD[Z[[I)V", vec![]),
        ("([[Lvalue/K;)[[[Lvalue/K;", vec!["value/K"]),
        ("(LI;)LV;", vec!["I", "V"]),
    ] {
        let (mut g, a, b, _, child) = declared(desc, 0x101, 0x101);
        let plan = g.plan(child);
        let before = g.domain.usage();
        g.domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap();
        for leaf in &leaves {
            assert!(linked(&mut g.domain, leaf, a, b));
        }
        let after = g.domain.usage();
        assert_eq!(after.names - before.names, leaves.len() as u64);
        assert_eq!(after.pairs - before.pairs, 2 * leaves.len() as u64);
        assert_eq!(after.unions - before.unions, leaves.len() as u64);
        assert_eq!(after.initiations, before.initiations);
    }
}
#[test]
fn duplicate_leaf_occurrences_and_replayed_preparation_pay_work_without_more_relations() {
    let (mut g, a, b, _, child) = declared("(Lvalue/K;Lvalue/K;)Lvalue/K;", 0x101, 0x101);
    let plan = g.plan(child);
    let mut ctx = g.domain.context(WorkLimits::default());
    ctx.prepare_constraints(&plan).unwrap();
    let first = ctx.usage();
    ctx.prepare_constraints(&plan).unwrap();
    assert!(ctx.usage().work_units > first.work_units);
    let before = g.domain.usage();
    g.domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
    assert_eq!(g.domain.usage(), before);
    assert!(linked(&mut g.domain, "value/K", a, b));
    assert_eq!(g.domain.usage().unions, 1);
}
#[test]
fn private_static_and_package_inaccessible_declarations_do_not_generate_override_obligations() {
    for (p, c) in [
        (0x101, 0x102),
        (0x101, 0x109),
        (0x102, 0x101),
        (0x109, 0x101),
        (0x100, 0x101),
    ] {
        let (mut g, a, b, _, child) = declared("(Lvalue/K;)V", p, c);
        let plan = g.plan(child);
        let before = g.domain.usage();
        g.domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap();
        assert_eq!(g.domain.usage(), before);
        assert!(!linked(&mut g.domain, "value/K", a, b));
    }
}
#[test]
fn bridge_synthetic_abstract_and_protected_instance_declarations_remain_real_producers() {
    for flags in [0x1141, 0x401, 0x104] {
        let (mut g, a, b, _, child) = declared("(Lvalue/K;)V", 0x101, flags);
        let plan = g.plan(child);
        g.domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap();
        assert!(linked(&mut g.domain, "value/K", a, b));
    }
}
#[test]
fn all_hierarchy_prerequisites_are_checked_before_the_first_signature_effect() {
    let (mut g, a, b, parent, child) = declared("(Lvalue/K;)V", 0x101, 0x101);
    let interface_loader = g.loader();
    let interface = g.define(interface_loader, Class::new("i/I").interface());
    let mut c = Class::new("c/WithInterface")
        .parent("p/Base")
        .implements("i/I")
        .method("take", "(Lvalue/K;)V", 0x101);
    c.flags = 0x21;
    let child2 = g.define(b, c);
    g.intended.insert((b, name("i/I")), interface);
    let plan = g.plan(child2);
    let before = g.domain.usage();
    let e = g
        .domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(
        matches!(e.kind,E::MissingInitiation{origin,dependency}if origin==child2&&dependency==interface)
    );
    let site = e.site.unwrap();
    assert_eq!(site.class, child2);
    assert!(site.origin.is_none() && site.related.is_none() && site.position.is_none());
    assert_eq!(g.domain.usage(), before);
    assert!(!linked(&mut g.domain, "value/K", a, b));
    g.bind(b, "i/I", interface);
    g.domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
    assert!(linked(&mut g.domain, "value/K", a, b));
    assert_ne!(parent, child);
}
#[test]
fn same_named_but_different_actual_parent_record_rejects_a_stale_external_plan() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let c = g.loader();
    let parent_a = g.define(a, Class::new("p/Base"));
    let parent_b = g.define(c, Class::new("p/Base"));
    let child = g.define(b, Class::new("x/Child").parent("p/Base"));
    g.bind(b, "p/Base", parent_b);
    g.intended.insert((b, name("p/Base")), parent_a);
    let plan = g.plan(child);
    let before = g.domain.usage();
    let e = g
        .domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(matches!(e.kind, E::InconsistentHierarchy { .. }));
    assert_eq!(e.site.unwrap().class, child);
    assert_eq!(g.domain.usage(), before);
}
#[test]
fn interface_object_edge_requires_actual_initiation_but_empty_object_itself_has_no_parent() {
    let mut g = Graph::new();
    let plan = g.plan(g.object);
    let before = g.domain.usage();
    g.domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
    assert_eq!(g.domain.usage(), before);
    let loader = g.domain.create_loader().unwrap();
    g.intended
        .insert((loader, name("java/lang/Object")), g.object);
    let interface = g.define(loader, Class::new("i/I").interface());
    let plan = g.plan(interface);
    let e = g
        .domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(
        matches!(e.kind,E::MissingInitiation{origin,dependency}if origin==interface&&dependency==g.object)
    );
    g.bind(loader, "java/lang/Object", g.object);
    g.domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
}
#[test]
fn owned_hierarchy_plan_remains_valid_after_unrelated_definitions_are_appended() {
    let (mut g, a, b, _, child) = declared("(Lvalue/K;)V", 0x101, 0x101);
    let plan = g.plan(child);
    g.define(g.boot, Class::new("other/Later"));
    g.domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
    assert!(linked(&mut g.domain, "value/K", a, b));
}
#[test]
fn declared_method_table_order_precedes_ancestor_order_and_retains_only_accepted_prefix() {
    for p_first in [true, false] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let base = g.define(
            a,
            Class::new("p/Base")
                .method("q", "(Lvalue/Q;)V", 0x101)
                .method("p", "(Lvalue/P;)V", 0x101),
        );
        g.bind(b, "p/Base", base);
        let child = if p_first {
            Class::new("c/C")
                .parent("p/Base")
                .method("p", "(Lvalue/P;)V", 0x101)
                .method("q", "(Lvalue/Q;)V", 0x101)
        } else {
            Class::new("c/C")
                .parent("p/Base")
                .method("q", "(Lvalue/Q;)V", 0x101)
                .method("p", "(Lvalue/P;)V", 0x101)
        };
        let child = g.define(b, child);
        g.define(a, Class::new("value/Q"));
        g.define(b, Class::new("value/Q"));
        let plan = g.plan(child);
        let e = g
            .domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap_err();
        let site = e.site.unwrap();
        assert_eq!(site.origin.unwrap().index(), usize::from(p_first));
        assert_eq!(site.related.unwrap().index(), 0);
        assert_eq!(site.position, Some(Position::Parameter(0)));
        assert_eq!(linked(&mut g.domain, "value/P", a, b), p_first);
    }
}
fn inherited(
    candidate_flags: u16,
    child_flags: Option<u16>,
    same_leaf: bool,
) -> (
    Graph,
    LoaderId,
    LoaderId,
    LoaderId,
    ClassId,
    ClassId,
    ClassId,
    ClassId,
    ClassId,
) {
    let mut g = Graph::new();
    let a = g.loader();
    let i = g.loader();
    let c = g.loader();
    let ka = g.define(a, Class::new("value/K"));
    let ki = if same_leaf {
        g.bind(i, "value/K", ka);
        ka
    } else {
        g.define(i, Class::new("value/K"))
    };
    let kc = g.define(c, Class::new("value/K"));
    let mut base = Class::new("a/Base").method("take", "(Lvalue/K;)V", candidate_flags);
    if candidate_flags & 0x400 != 0 {
        base.flags |= 0x400;
    }
    let base = g.define(a, base);
    let interface = g.define(
        i,
        Class::new("i/I")
            .interface()
            .method("take", "(Lvalue/K;)V", 0x401),
    );
    g.bind(c, "a/Base", base);
    g.bind(c, "i/I", interface);
    let mut child = Class::new("c/Child").parent("a/Base").implements("i/I");
    if let Some(flags) = child_flags {
        child = child.method("take", "(Lvalue/K;)V", flags);
    }
    let child = g.define(c, child);
    (g, a, i, c, base, interface, child, ki, kc)
}
#[test]
fn inherited_selected_loader_pair_excludes_the_childs_distinct_signature_class() {
    for child_flags in [None, Some(0x102), Some(0x109)] {
        let (mut g, a, i, c, _, _, child, ki, kc) = inherited(0x101, child_flags, true);
        assert_ne!(ki, kc);
        let plan = g.plan(child);
        g.domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap();
        assert!(linked(&mut g.domain, "value/K", a, i));
        assert!(!linked(&mut g.domain, "value/K", a, c));
        assert!(!linked(&mut g.domain, "value/K", i, c));
        assert_eq!(actual(&mut g.domain, "value/K", i), Some(ki));
        assert_eq!(actual(&mut g.domain, "value/K", c), Some(kc));
    }
}
#[test]
fn inherited_abstract_and_nonpublic_class_candidates_still_constrain_without_invocation_checks() {
    for flags in [0x401, 0x104, 0x100] {
        let (mut g, a, i, c, base, intf, child, _, _) = inherited(flags, None, false);
        let plan = g.plan(child);
        let e = g
            .domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap_err();
        assert!(matches!(e.kind,E::ConstraintConflict{left,right,..}if left==a&&right==i));
        let site = e.site.unwrap();
        assert_eq!(site.origin.unwrap().owner(), intf);
        assert_eq!(site.related.unwrap().owner(), base);
        assert_eq!(site.class, child);
        assert!(!linked(&mut g.domain, "value/K", a, c));
    }
}
#[test]
fn declaring_a_public_override_uses_child_loader_before_inherited_interface_selection() {
    let (mut g, a, _, c, base, _, child, _, _) = inherited(0x101, Some(0x101), true);
    let plan = g.plan(child);
    let e = g
        .domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(matches!(e.kind,E::ConstraintConflict{left,right,..}if left==c&&right==a));
    let site = e.site.unwrap();
    assert_eq!(site.origin.unwrap().owner(), child);
    assert_eq!(site.related.unwrap().owner(), base);
}
#[test]
fn interface_root_uses_unique_default_against_abstract_regardless_declaration_order() {
    for reverse in [false, true] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let c = g.loader();
        let ka = g.define(a, Class::new("value/K"));
        g.bind(b, "value/K", ka);
        let kc = g.define(c, Class::new("value/K"));
        let ia = g.define(
            a,
            Class::new("i/Abstract")
                .interface()
                .method("take", "(Lvalue/K;)V", 0x401),
        );
        let ib = g.define(
            b,
            Class::new("i/Default")
                .interface()
                .method("take", "(Lvalue/K;)V", 1),
        );
        g.bind(c, "i/Abstract", ia);
        g.bind(c, "i/Default", ib);
        let root = if reverse {
            Class::new("i/Child")
                .interface()
                .implements("i/Default")
                .implements("i/Abstract")
        } else {
            Class::new("i/Child")
                .interface()
                .implements("i/Abstract")
                .implements("i/Default")
        };
        let child = g.define(c, root);
        let plan = g.plan(child);
        g.domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap();
        assert!(linked(&mut g.domain, "value/K", a, b));
        assert!(!linked(&mut g.domain, "value/K", a, c));
        assert_eq!(actual(&mut g.domain, "value/K", c), Some(kc));
    }
}
#[test]
fn abstract_subinterface_suppresses_ancestor_default_while_concrete_redeclaration_selects_itself() {
    for abstract_child in [true, false] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let c = g.loader();
        g.define(a, Class::new("value/K"));
        g.define(b, Class::new("value/K"));
        let parent = g.define(
            a,
            Class::new("i/Parent")
                .interface()
                .method("take", "(Lvalue/K;)V", 1),
        );
        g.bind(b, "i/Parent", parent);
        let sub = g.define(
            b,
            Class::new("i/Sub")
                .interface()
                .implements("i/Parent")
                .method(
                    "take",
                    "(Lvalue/K;)V",
                    if abstract_child { 0x401 } else { 1 },
                ),
        );
        g.bind(c, "i/Sub", sub);
        let child = g.define(c, Class::new("c/C").implements("i/Sub"));
        let plan = g.plan(child);
        let result = g
            .domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan);
        assert_eq!(result.is_ok(), abstract_child, "{result:?}");
        if let Err(e) = result {
            let site = e.site.unwrap();
            assert_eq!(site.origin.unwrap().owner(), parent);
            assert_eq!(site.related.unwrap().owner(), sub);
        }
        assert!(!linked(&mut g.domain, "value/K", a, b));
    }
}
#[test]
fn private_targets_and_no_unique_default_selection_do_not_invent_constraints_or_errors() {
    for (left, right) in [(0x401, 0x401), (1, 1), (2, 1), (1, 2)] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let c = g.loader();
        g.define(a, Class::new("value/K"));
        g.define(b, Class::new("value/K"));
        let ia = g.define(
            a,
            Class::new("i/A")
                .interface()
                .method("take", "(Lvalue/K;)V", left),
        );
        let ib = g.define(
            b,
            Class::new("i/B")
                .interface()
                .method("take", "(Lvalue/K;)V", right),
        );
        g.bind(c, "i/A", ia);
        g.bind(c, "i/B", ib);
        let child = g.define(c, Class::new("c/C").implements("i/A").implements("i/B"));
        let plan = g.plan(child);
        let before = g.domain.usage();
        g.domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap();
        assert_eq!(g.domain.usage(), before);
        assert!(!linked(&mut g.domain, "value/K", a, b));
    }
}
#[test]
fn package_spelling_alone_does_not_enable_override_but_public_bridge_does() {
    for bridge in [false, true] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let hidden = g.define(
            a,
            Class::new("p/Hidden").method("take", "(Lvalue/K;)V", 0x100),
        );
        let (parent, n) = if bridge {
            let parent = g.define(
                a,
                Class::new("p/Bridge")
                    .parent("p/Hidden")
                    .method("take", "(Lvalue/K;)V", 0x101),
            );
            (parent, "p/Bridge")
        } else {
            (hidden, "p/Hidden")
        };
        g.bind(b, n, parent);
        let child = g.define(
            b,
            Class::new("p/Child")
                .parent(n)
                .method("take", "(Lvalue/K;)V", 0x101),
        );
        let plan = g.plan(child);
        g.domain
            .context(WorkLimits::default())
            .prepare_constraints(&plan)
            .unwrap();
        assert_eq!(linked(&mut g.domain, "value/K", a, b), bridge);
    }
}
fn maximum_limits() -> Limits {
    Limits {
        max_names: u64::MAX,
        max_name_units: u64::MAX,
        max_pairs: u64::MAX,
        max_initiations: u64::MAX,
        max_unions: u64::MAX,
    }
}
fn maximum_work() -> WorkLimits {
    WorkLimits {
        max_utf16_units: u64::MAX,
        max_work_units: u64::MAX,
        max_scratch_entries: u64::MAX,
    }
}
#[test]
fn defaults_zero_and_maximum_are_host_limits_not_reservation_sizes() {
    let l = Limits::default();
    assert_eq!(
        (
            l.max_names,
            l.max_name_units,
            l.max_pairs,
            l.max_initiations,
            l.max_unions
        ),
        (65536, 4194304, 262144, 131072, 262144)
    );
    let w = WorkLimits::default();
    assert_eq!(
        (w.max_utf16_units, w.max_work_units, w.max_scratch_entries),
        (16777216, 67108864, 4194304)
    );
    let mut d = LoadingDomain::new(RegistryOptions::default(), maximum_limits()).unwrap();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    d.context(maximum_work())
        .impose(&name("p/C"), a, b)
        .unwrap();
    assert_eq!(
        d.usage(),
        Usage {
            names: 1,
            name_units: 3,
            pairs: 2,
            initiations: 0,
            unions: 1
        }
    );
}
#[test]
fn all_five_persistent_resources_reject_an_entire_insertion_before_publication() {
    for resource in [R::Names, R::NameUnits, R::Pairs, R::Initiations, R::Unions] {
        let mut limits = maximum_limits();
        match resource {
            R::Names => limits.max_names = 0,
            R::NameUnits => limits.max_name_units = 2,
            R::Pairs => limits.max_pairs = 1,
            R::Initiations => limits.max_initiations = 0,
            R::Unions => limits.max_unions = 0,
            _ => unreachable!(),
        };
        let mut d = LoadingDomain::new(RegistryOptions::default(), limits).unwrap();
        let a = d.create_loader().unwrap();
        let b = d.create_loader().unwrap();
        let e = if resource == R::Initiations {
            d.context(maximum_work())
                .define(
                    a,
                    &name("p/C"),
                    &Class::new("p/C").bytes(),
                    ParseOptions::default(),
                )
                .unwrap_err()
        } else {
            d.context(maximum_work())
                .impose(&name("p/C"), a, b)
                .unwrap_err()
        };
        assert!(matches!(e.kind,E::ResourceLimit{resource:r,..}if r==resource));
        assert_eq!(d.usage(), Usage::default());
        assert_eq!(d.registry().definition_count(), 0);
        assert_eq!(d.registry().total_definition_bytes(), 0);
        assert_eq!(d.registry().lookup_class(a, &name("p/C")).unwrap(), None);
        assert_eq!(actual(&mut d, "p/C", a), None);
        assert!(!linked(&mut d, "p/C", a, b));
    }
}
#[test]
fn persistent_multi_resource_failure_uses_declared_order_and_context_reset_cannot_raise_it() {
    let limits = Limits {
        max_names: 0,
        max_name_units: 0,
        max_pairs: 0,
        max_initiations: 0,
        max_unions: 0,
    };
    let mut d = LoadingDomain::new(RegistryOptions::default(), limits).unwrap();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    for _ in 0..2 {
        let e = d
            .context(maximum_work())
            .impose(&name("p/C"), a, b)
            .unwrap_err();
        assert!(matches!(
            e.kind,
            E::ResourceLimit {
                resource: R::Names,
                limit: 0,
                required: 1
            }
        ));
        assert_eq!(d.usage(), Usage::default());
    }
    impose(&mut d, "p/C", a, a).unwrap();
    assert_eq!(d.usage(), Usage::default());
}
#[test]
fn exact_retained_join_capacity_and_transitive_duplicates_allow_idempotent_work_at_the_limit() {
    let limits = Limits {
        max_names: 1,
        max_name_units: 3,
        max_pairs: 3,
        max_initiations: 0,
        max_unions: 2,
    };
    let mut d = LoadingDomain::new(RegistryOptions::default(), limits).unwrap();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let c = d.create_loader().unwrap();
    impose(&mut d, "p/C", a, b).unwrap();
    impose(&mut d, "p/C", b, c).unwrap();
    assert_eq!(
        d.usage(),
        Usage {
            names: 1,
            name_units: 3,
            pairs: 3,
            initiations: 0,
            unions: 2
        }
    );
    let before = d.usage();
    impose(&mut d, "p/C", c, a).unwrap();
    assert_eq!(d.usage(), before);
    assert!(linked(&mut d, "p/C", a, c));
    let extra = d.create_loader().unwrap();
    let e = impose(&mut d, "p/C", c, extra).unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::Pairs,
            limit: 3,
            required: 4
        }
    ));
    assert_eq!(d.usage(), before);
    assert!(!linked(&mut d, "p/C", a, extra));
}
#[test]
fn full_initiation_limit_preserves_existing_bindings_and_allows_exact_duplicates() {
    let mut limits = maximum_limits();
    limits.max_initiations = 2;
    let mut d = LoadingDomain::new(RegistryOptions::default(), limits).unwrap();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let c = d.create_loader().unwrap();
    let class = define(&mut d, a, &Class::new("p/C"));
    d.context(maximum_work())
        .record_initiation(b, &name("p/C"), class)
        .unwrap();
    let before = d.usage();
    let e = d
        .context(maximum_work())
        .record_initiation(c, &name("p/C"), class)
        .unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::Initiations,
            limit: 2,
            required: 3
        }
    ));
    assert_eq!(d.usage(), before);
    assert_eq!(actual(&mut d, "p/C", c), None);
    d.context(maximum_work())
        .record_initiation(b, &name("p/C"), class)
        .unwrap();
    assert_eq!(d.usage(), before);
    assert_eq!(actual(&mut d, "p/C", b), Some(class));
}
#[test]
fn self_and_empty_lookup_work_are_hand_counted_and_queries_have_no_scratch() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let limits = WorkLimits {
        max_scratch_entries: 0,
        ..maximum_work()
    };
    let mut ctx = d.context(limits);
    ctx.impose(&name("p/C"), a, a).unwrap();
    assert_eq!(
        ctx.usage(),
        WorkUsage {
            utf16_units: 3,
            work_units: 5,
            scratch_entries: 0
        }
    );
    let mut ctx = d.context(limits);
    assert!(ctx.constrained(&name("p/C"), a, a).unwrap());
    assert_eq!(
        ctx.usage(),
        WorkUsage {
            utf16_units: 3,
            work_units: 5,
            scratch_entries: 0
        }
    );
    let mut ctx = d.context(limits);
    assert_eq!(ctx.lookup_initiated(a, &name("p/C")).unwrap(), None);
    assert_eq!(
        ctx.usage(),
        WorkUsage {
            utf16_units: 3,
            work_units: 5,
            scratch_entries: 0
        }
    );
    let mut ctx = d.context(limits);
    assert!(!ctx.constrained(&name("p/C"), a, b).unwrap());
    assert_eq!(
        ctx.usage(),
        WorkUsage {
            utf16_units: 3,
            work_units: 6,
            scratch_entries: 0
        }
    );
}
#[test]
fn utf16_and_work_failure_batches_do_not_partially_pay_the_other_counter() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    for (resource, limits, limit, required) in [
        (
            R::Utf16Units,
            WorkLimits {
                max_utf16_units: 2,
                ..maximum_work()
            },
            2,
            3,
        ),
        (
            R::WorkUnits,
            WorkLimits {
                max_utf16_units: 3,
                max_work_units: 4,
                max_scratch_entries: 0,
            },
            4,
            5,
        ),
    ] {
        let mut ctx = d.context(limits);
        let e = ctx.impose(&name("p/C"), a, a).unwrap_err();
        assert!(
            matches!(e.kind,E::ResourceLimit{resource:r,limit:l,required:q}if r==resource&&l==limit&&q==required)
        );
        assert_eq!(
            ctx.usage(),
            WorkUsage {
                utf16_units: 0,
                work_units: 2,
                scratch_entries: 0
            }
        );
    }
    assert_eq!(d.usage(), Usage::default());
}
#[test]
fn preparation_persistent_quota_failure_retains_first_leaf_and_retracts_only_second_insertion() {
    let mut limits = maximum_limits();
    limits.max_unions = 1;
    let mut g = Graph::with_limits(limits);
    let a = g.loader();
    let b = g.loader();
    let base = g.define(
        a,
        Class::new("p/Base").method("take", "(Lvalue/P;Lvalue/Q;)V", 0x101),
    );
    g.bind(b, "p/Base", base);
    let child = g.define(
        b,
        Class::new("c/C")
            .parent("p/Base")
            .method("take", "(Lvalue/P;Lvalue/Q;)V", 0x101),
    );
    let plan = g.plan(child);
    let before = g.domain.usage();
    let e = g
        .domain
        .context(maximum_work())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::Unions,
            limit: 1,
            required: 2
        }
    ));
    assert_eq!(e.site.unwrap().position, Some(Position::Parameter(1)));
    assert!(linked(&mut g.domain, "value/P", a, b));
    assert!(!linked(&mut g.domain, "value/Q", a, b));
    let after = g.domain.usage();
    assert_eq!(
        (
            after.names - before.names,
            after.pairs - before.pairs,
            after.unions - before.unions
        ),
        (1, 2, 1)
    );
    let e = g
        .domain
        .context(maximum_work())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::Unions,
            ..
        }
    ));
    assert_eq!(g.domain.usage(), after);
}
#[test]
fn work_context_counters_accumulate_across_calls_but_reset_does_not_erase_relations() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let mut ctx = d.context(WorkLimits {
        max_utf16_units: 6,
        max_work_units: 10,
        max_scratch_entries: 0,
    });
    ctx.impose(&name("p/C"), a, a).unwrap();
    ctx.impose(&name("p/C"), a, a).unwrap();
    assert_eq!(ctx.usage().work_units, 10);
    assert!(ctx.impose(&name("p/C"), a, a).is_err());
    impose(&mut d, "p/C", a, b).unwrap();
    let before = d.usage();
    assert!(linked(&mut d, "p/C", a, b));
    let ctx = d.context(maximum_work());
    assert_eq!(ctx.usage(), WorkUsage::default());
    assert_eq!(d.usage(), before);
}
fn preparation_usage(
    limits: WorkLimits,
) -> (
    Graph,
    LoaderId,
    LoaderId,
    ClassId,
    Result<(), Box<Error>>,
    WorkUsage,
) {
    let (mut g, a, b, _, child) = declared("(Lvalue/P;Lvalue/Q;)V", 0x101, 0x101);
    let plan = g.plan(child);
    let mut ctx = g.domain.context(limits);
    let result = ctx.prepare_constraints(&plan);
    let usage = ctx.usage();
    (g, a, b, child, result, usage)
}
#[test]
fn all_work_resources_have_measured_exact_and_one_less_preparation_boundaries() {
    let (_, _, _, _, result, full) = preparation_usage(maximum_work());
    result.unwrap();
    assert!(full.utf16_units > 0 && full.work_units > 0 && full.scratch_entries > 0);
    let exact = WorkLimits {
        max_utf16_units: full.utf16_units,
        max_work_units: full.work_units,
        max_scratch_entries: full.scratch_entries,
    };
    let (_, _, _, _, result, u) = preparation_usage(exact);
    result.unwrap();
    assert_eq!(u, full);
    for (resource, limit) in [
        (R::Utf16Units, full.utf16_units - 1),
        (R::WorkUnits, full.work_units - 1),
        (R::ScratchEntries, full.scratch_entries - 1),
    ] {
        let mut limits = maximum_work();
        match resource {
            R::Utf16Units => limits.max_utf16_units = limit,
            R::WorkUnits => limits.max_work_units = limit,
            R::ScratchEntries => limits.max_scratch_entries = limit,
            _ => unreachable!(),
        };
        let (_, _, _, _, result, u) = preparation_usage(limits);
        let e = result.unwrap_err();
        assert!(matches!(e.kind,E::ResourceLimit{resource:r,limit:l,..}if r==resource&&l==limit));
        assert!(
            u.utf16_units <= limits.max_utf16_units
                && u.work_units <= limits.max_work_units
                && u.scratch_entries <= limits.max_scratch_entries
        );
    }
}
#[test]
fn late_work_exhaustion_keeps_prior_relations_and_fresh_context_can_retry() {
    let (_, _, _, _, result, full) = preparation_usage(maximum_work());
    result.unwrap();
    let (mut g, a, b, child, result, usage) = preparation_usage(WorkLimits {
        max_work_units: full.work_units - 1,
        ..maximum_work()
    });
    let e = result.unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::WorkUnits,
            ..
        }
    ));
    assert_eq!(e.site.unwrap().class, child);
    assert!(usage.work_units < full.work_units);
    assert!(linked(&mut g.domain, "value/P", a, b));
    let before = g.domain.usage();
    let plan = g.plan(child);
    g.domain
        .context(maximum_work())
        .prepare_constraints(&plan)
        .unwrap();
    assert!(linked(&mut g.domain, "value/P", a, b));
    assert!(linked(&mut g.domain, "value/Q", a, b));
    assert!(g.domain.usage().unions >= before.unions);
}
#[test]
fn maximum_ordinary_name_equal_queries_pay_full_spans_without_retaining_query_copies() {
    let mut d = domain();
    let a = d.create_loader().unwrap();
    let b = d.create_loader().unwrap();
    let spelling = vec![120; 65535];
    let mut ctx = d.context(maximum_work());
    ctx.impose(&spelling, a, b).unwrap();
    let before = d.usage();
    assert_eq!(before.name_units, 65535);
    let mut ctx = d.context(WorkLimits {
        max_scratch_entries: 0,
        ..maximum_work()
    });
    let fresh = spelling.clone();
    assert!(ctx.constrained(&fresh, a, b).unwrap());
    assert!(ctx.usage().utf16_units >= 2 * 65535);
    assert_eq!(ctx.usage().scratch_entries, 0);
    assert_eq!(d.usage(), before);
    let mut ctx = d.context(WorkLimits {
        max_utf16_units: 0,
        max_work_units: 2,
        max_scratch_entries: 0,
    });
    let e = ctx.impose(&vec![120; 65536], a, b).unwrap_err();
    assert!(
        matches!(e.kind,E::Registry(ref r)if matches!(r.as_ref(),RegistryError::InvalidName(_)))
    );
    assert_eq!(
        ctx.usage(),
        WorkUsage {
            utf16_units: 0,
            work_units: 2,
            scratch_entries: 0
        }
    );
}
#[test]
fn maximum_descriptor_and_array_rank_constrain_the_nominal_leaf_without_loading_it() {
    for desc in [
        format!("(L{};)V", "x".repeat(65530)),
        format!("({}Lvalue/K;){}Lvalue/K;", "[".repeat(255), "[".repeat(255)),
    ] {
        let (mut g, a, b, _, child) = declared(&desc, 0x101, 0x101);
        let plan = g.plan(child);
        g.domain
            .context(maximum_work())
            .prepare_constraints(&plan)
            .unwrap();
        let leaf = if desc.len() == 65535 {
            "x".repeat(65530)
        } else {
            "value/K".into()
        };
        assert!(linked(&mut g.domain, &leaf, a, b));
        assert_eq!(actual(&mut g.domain, &leaf, a), None);
        assert_eq!(actual(&mut g.domain, &leaf, b), None);
    }
}
#[test]
fn balanced_union_forest_has_no_query_compression_and_survives_an_unpaid_walk() {
    let mut d = LoadingDomain::new(
        RegistryOptions {
            max_loaders: 17000,
            ..Default::default()
        },
        maximum_limits(),
    )
    .unwrap();
    let loaders = (0..16384)
        .map(|_| d.create_loader().unwrap())
        .collect::<Vec<_>>();
    let mut ctx = d.context(maximum_work());
    let mut width = 1;
    while width < loaders.len() {
        for at in (0..loaders.len()).step_by(width * 2) {
            ctx.impose(&name("K"), loaders[at], loaders[at + width])
                .unwrap();
        }
        width *= 2;
    }
    assert_eq!(
        d.usage(),
        Usage {
            names: 1,
            name_units: 1,
            pairs: 16384,
            initiations: 0,
            unions: 16383
        }
    );
    let mut ctx = d.context(WorkLimits {
        max_scratch_entries: 0,
        ..maximum_work()
    });
    assert!(
        ctx.constrained(&name("K"), loaders[16383], loaders[0])
            .unwrap()
    );
    let one = ctx.usage();
    assert!(
        ctx.constrained(&name("K"), loaders[16383], loaders[0])
            .unwrap()
    );
    let two = ctx.usage();
    assert_eq!(two.work_units - one.work_units, one.work_units);
    assert_eq!(two.utf16_units - one.utf16_units, one.utf16_units);
    assert_eq!(two.scratch_entries, 0);
    assert!(one.work_units < 100);
    let before = d.usage();
    let mut ctx = d.context(WorkLimits {
        max_work_units: one.work_units - 1,
        ..maximum_work()
    });
    assert!(
        ctx.constrained(&name("K"), loaders[16383], loaders[0])
            .is_err()
    );
    let mut ctx = d.context(maximum_work());
    assert!(
        ctx.constrained(&name("K"), loaders[16383], loaders[0])
            .unwrap()
    );
    assert_eq!(ctx.usage().work_units, one.work_units);
    assert_eq!(d.usage(), before);
}
#[test]
fn ten_thousand_class_hierarchy_preparation_is_iterative_and_adds_no_unmatched_signature_constraints()
 {
    let mut g = Graph::with_options(
        RegistryOptions {
            max_definitions: 11000,
            ..Default::default()
        },
        maximum_limits(),
    );
    let mut last = g.object;
    for i in 0..10000 {
        let parent = if i == 0 {
            "java/lang/Object".into()
        } else {
            format!("deep/C{}", i - 1)
        };
        let mut c = Class::new(&format!("deep/C{i}")).parent(&parent);
        if i == 9999 {
            c = c.method("take", "(Lvalue/K;)V", 0x101);
        }
        last = g.define(g.boot, c);
    }
    let plan = g.plan(last);
    let before = g.domain.usage();
    let mut limited = g.domain.context(WorkLimits {
        max_work_units: 0,
        ..maximum_work()
    });
    let e = limited.prepare_constraints(&plan).unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::WorkUnits,
            limit: 0,
            required: 1
        }
    ));
    assert_eq!(limited.usage(), WorkUsage::default());
    assert_eq!(g.domain.usage(), before);
    g.domain
        .context(WorkLimits::default())
        .prepare_constraints(&plan)
        .unwrap();
    assert_eq!(g.domain.usage(), before);
}
#[test]
fn two_prior_preparations_survive_a_third_transitive_component_conflict() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let c = g.loader();
    let d = g.loader();
    let ka = g.define(a, Class::new("value/K"));
    let kc = g.define(c, Class::new("value/K"));
    let aa = g.define(a, Class::new("a/A").method("take", "(Lvalue/K;)V", 0x101));
    g.bind(b, "a/A", aa);
    let bb = g.define(
        b,
        Class::new("b/B")
            .parent("a/A")
            .method("take", "(Lvalue/K;)V", 0x101),
    );
    let cc = g.define(c, Class::new("c/C").method("take", "(Lvalue/K;)V", 0x101));
    g.bind(d, "c/C", cc);
    let dd = g.define(
        d,
        Class::new("d/D")
            .parent("c/C")
            .method("take", "(Lvalue/K;)V", 0x101),
    );
    for class in [bb, dd] {
        let plan = g.plan(class);
        g.domain
            .context(maximum_work())
            .prepare_constraints(&plan)
            .unwrap();
    }
    assert!(linked(&mut g.domain, "value/K", a, b));
    assert!(linked(&mut g.domain, "value/K", c, d));
    g.bind(b, "d/D", dd);
    let mixed = g.define(
        b,
        Class::new("b/Mixed")
            .parent("d/D")
            .method("take", "(Lvalue/K;)V", 0x101),
    );
    let plan = g.plan(mixed);
    let before = g.domain.usage();
    let e = g
        .domain
        .context(maximum_work())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(
        matches!(e.kind,E::ConstraintConflict{left,right,left_class,right_class}if left==b&&right==d&&left_class==ka&&right_class==kc)
    );
    assert_eq!(g.domain.usage(), before);
    assert!(linked(&mut g.domain, "value/K", a, b));
    assert!(linked(&mut g.domain, "value/K", c, d));
    assert!(!linked(&mut g.domain, "value/K", b, d));
    assert_eq!(actual(&mut g.domain, "value/K", b), None);
    let e = g
        .domain
        .context(maximum_work())
        .record_initiation(b, &name("value/K"), kc)
        .unwrap_err();
    assert!(
        matches!(e.kind,E::InitiationConflict{existing,incoming,..}if existing==ka&&incoming==kc)
    );
}
#[test]
fn declared_phase_finishes_before_inherited_interface_obligations_and_keeps_its_prefix() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let i = g.loader();
    let base = g.define(
        a,
        Class::new("a/Base")
            .method("p", "(Lvalue/P;)V", 0x101)
            .method("q", "(Lvalue/Q;)V", 0x101),
    );
    let interface = g.define(
        i,
        Class::new("i/I")
            .interface()
            .method("q", "(Lvalue/Q;)V", 0x401),
    );
    g.bind(b, "a/Base", base);
    g.bind(b, "i/I", interface);
    let child = g.define(
        b,
        Class::new("b/C")
            .parent("a/Base")
            .implements("i/I")
            .method("p", "(Lvalue/P;)V", 0x101),
    );
    g.define(a, Class::new("value/Q"));
    g.define(i, Class::new("value/Q"));
    let plan = g.plan(child);
    let e = g
        .domain
        .context(maximum_work())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(matches!(e.kind,E::ConstraintConflict{left,right,..}if left==a&&right==i));
    let site = e.site.unwrap();
    assert_eq!(site.class, child);
    assert_eq!(site.origin.unwrap().owner(), interface);
    assert_eq!(site.related.unwrap().owner(), base);
    assert_eq!(site.related.unwrap().index(), 1);
    assert!(linked(&mut g.domain, "value/P", a, b));
    assert!(!linked(&mut g.domain, "value/Q", a, i));
}
#[test]
fn wide_shared_interface_diamond_deduplicates_identity_and_excludes_child_loader() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let c = g.loader();
    let ka = g.define(a, Class::new("value/K"));
    g.bind(b, "value/K", ka);
    let kc = g.define(c, Class::new("value/K"));
    let common = g.define(
        a,
        Class::new("i/Common")
            .interface()
            .method("take", "(Lvalue/K;)V", 1),
    );
    let abstract_i = g.define(
        b,
        Class::new("i/Abstract")
            .interface()
            .method("take", "(Lvalue/K;)V", 0x401),
    );
    g.bind(c, "i/Abstract", abstract_i);
    let mut root = Class::new("c/C");
    for at in 0..256 {
        let n = format!("i/Mid{at}");
        let mid = g.define(a, Class::new(&n).interface().implements("i/Common"));
        g.bind(c, &n, mid);
        root = root.implements(&n);
    }
    root = root.implements("i/Abstract");
    let child = g.define(c, root);
    let plan = g.plan(child);
    assert_eq!(plan.classes().iter().filter(|&&id| id == common).count(), 1);
    let before = g.domain.usage();
    let mut bounded = g.domain.context(WorkLimits {
        max_work_units: 50000,
        ..WorkLimits::default()
    });
    bounded.prepare_constraints(&plan).unwrap();
    assert!(bounded.usage().work_units < 50000);
    assert_eq!(g.domain.usage().unions - before.unions, 1);
    assert!(linked(&mut g.domain, "value/K", a, b));
    assert!(!linked(&mut g.domain, "value/K", a, c));
    assert_eq!(actual(&mut g.domain, "value/K", c), Some(kc));
}
#[test]
fn foreign_class_records_and_foreign_hierarchy_plans_are_host_errors_without_publication() {
    let mut g = Graph::new();
    let mut other = Graph::new();
    let foreign = other.define(other.boot, Class::new("p/A"));
    let before = g.domain.usage();
    let error = g
        .domain
        .context(maximum_work())
        .record_initiation(g.boot, &name("p/A"), foreign)
        .unwrap_err();
    assert!(
        matches!(error.kind,E::Registry(ref e)if matches!(e.as_ref(),RegistryError::InvalidHandle{kind:HandleKind::Class,..}))
    );
    assert_eq!(g.domain.usage(), before);
    let error = g
        .domain
        .context(maximum_work())
        .record_initiation(g.boot, &name("[I"), foreign)
        .unwrap_err();
    assert!(
        matches!(error.kind,E::Registry(ref e)if matches!(e.as_ref(),RegistryError::UnsupportedDefinition(_)))
    );
    let plan = other.plan(other.object);
    let error = g
        .domain
        .context(maximum_work())
        .prepare_constraints(&plan)
        .unwrap_err();
    assert!(
        matches!(error.kind,E::Registry(ref e)if matches!(e.as_ref(),RegistryError::InvalidHandle{kind:HandleKind::Class,..}))
    );
    assert_eq!(g.domain.usage(), before);
}

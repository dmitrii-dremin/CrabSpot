//! Public main selection is checked metadata authority, never a verifier certificate.
mod member_support;
use crabspot_runtime::*;
use member_support::*;
const DESC: &str = "([Ljava/lang/String;)V";
fn context<'a>(g: &'a mut Graph, h: &'a mut ManagedHeap) -> FieldResolutionContext<'a> {
    g.domain
        .member_context_unnamed_modules(h, Default::default(), Default::default())
        .unwrap()
}
#[test]
fn direct_and_inherited_main_keep_named_and_actual_identities() {
    let mut g = Graph::new();
    let mut p = Class::plain("Parent");
    p.method("main", DESC, 9, &[0xb1], 0, 1);
    let p = g.add(g.boot, p);
    let c = g.add(g.boot, Class::new("Child", Some("Parent"), 0x20));
    let mut h = g.ready();
    let expected = g.method(p, "main", DESC);
    let before = g.domain.usage();
    let mut cx = context(&mut g, &mut h);
    let entry = cx.resolve_application_main(c).unwrap();
    assert_eq!(entry.named_class(), c);
    assert_eq!(entry.method(), expected);
    let steps = cx.lookup_steps();
    cx.check_application_main(&entry).unwrap();
    assert_eq!(cx.lookup_steps(), steps);
    drop(cx);
    assert_eq!(g.domain.usage(), before);
    assert_eq!(g.domain.member_resolution_usage().sites, 0);
}
#[test]
fn nearest_public_declaration_hides_parent_and_wrong_instance_never_skips() {
    for flags in [9, 1, 0x29, 0x109, 0x401] {
        let mut g = Graph::new();
        let mut p = Class::plain("Parent");
        p.method("main", DESC, 9, &[0xb1], 0, 1);
        let p = g.add(g.boot, p);
        let mut c = Class::new(
            "Child",
            Some("Parent"),
            if flags & 0x400 != 0 { 0x421 } else { 0x21 },
        );
        c.method("main", DESC, flags, &[0xb1], 0, 2);
        let c = g.add(g.boot, c);
        let mut h = g.ready();
        let expected = g.method(c, "main", DESC);
        let result = context(&mut g, &mut h).resolve_application_main(c);
        if flags == 9 {
            assert_eq!(result.unwrap().method(), expected);
        } else {
            assert!(
                matches!(*result.unwrap_err(),ApplicationMainError::WrongMainFlags{method}if method==expected)
            );
        }
        let _ = p;
    }
}
#[test]
fn private_or_nonvoid_only_declaration_is_not_an_entry() {
    for (flags, desc, body) in [
        (10, DESC, vec![0xb1]),
        (9, "([Ljava/lang/String;)I", vec![3, 0xac]),
    ] {
        let mut g = Graph::new();
        let mut d = Class::plain("D");
        d.method("main", desc, flags, &body, 1, 1);
        let d = g.add(g.boot, d);
        let mut h = g.ready();
        assert!(
            matches!(*context(&mut g,&mut h).resolve_application_main(d).unwrap_err(),ApplicationMainError::NoEligibleMain{named}if named==d)
        );
    }
}
#[test]
fn interface_entry_is_direct_and_not_inherited_by_class_or_subinterface() {
    let mut g = Graph::new();
    let mut it = Class::new("I", Some("java/lang/Object"), 0x601);
    it.method("main", DESC, 9, &[0xb1], 0, 1);
    let it = g.add(g.boot, it);
    let mut child = Class::new("J", Some("java/lang/Object"), 0x601);
    child.interface("I");
    let child = g.add(g.boot, child);
    let mut c = Class::plain("C");
    c.interface("I");
    let c = g.add(g.boot, c);
    let mut h = g.ready();
    assert_eq!(
        context(&mut g, &mut h)
            .resolve_application_main(it)
            .unwrap()
            .method(),
        g.method(it, "main", DESC)
    );
    for id in [child, c] {
        assert!(
            matches!(*context(&mut g,&mut h).resolve_application_main(id).unwrap_err(),ApplicationMainError::NoEligibleMain{named}if named==id)
        );
    }
}
#[test]
fn token_affinity_rejects_new_heap_foreign_named_and_legacy_context() {
    let mut g = Graph::new();
    let mut d = Class::plain("D");
    d.method("main", DESC, 9, &[0xb1], 0, 1);
    let d = g.add(g.boot, d);
    let mut h = g.ready();
    let entry = context(&mut g, &mut h).resolve_application_main(d).unwrap();
    let mut other = g.heap(&[d]);
    assert!(matches!(
        *context(&mut g, &mut other)
            .check_application_main(&entry)
            .unwrap_err(),
        ApplicationMainError::TokenMismatch
    ));
    let foreign = Graph::new();
    assert!(
        context(&mut g, &mut h)
            .resolve_application_main(foreign.object)
            .is_err()
    );
    let mut resolver = Resolver(&g.intended);
    let mut legacy = FieldResolutionContext::new_unnamed_modules(
        g.domain.registry(),
        &mut h,
        &mut resolver,
        Default::default(),
    )
    .unwrap();
    assert!(matches!(
        *legacy.resolve_application_main(d).unwrap_err(),
        ApplicationMainError::Context(_)
    ));
    assert!(matches!(
        *legacy.check_application_main(&entry).unwrap_err(),
        ApplicationMainError::Context(_)
    ));
}
#[test]
fn exact_semantic_main_scan_cost_has_inclusive_lookup_limit() {
    for short in [false, true] {
        let mut g = Graph::new();
        let mut d = Class::plain("D");
        d.method("main", DESC, 9, &[0xb1], 0, 1);
        let d = g.add(g.boot, d);
        let mut h = g.ready();
        let opening = context(&mut g, &mut h).lookup_steps();
        let cost = 1 + 1 + 4 + DESC.encode_utf16().count();
        let mut c = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions {
                    max_lookup_steps: opening + cost - usize::from(short),
                    ..Default::default()
                },
                Default::default(),
            )
            .unwrap();
        if short {
            assert!(c.resolve_application_main(d).is_err());
        } else {
            c.resolve_application_main(d).unwrap();
            assert_eq!(c.lookup_steps(), opening + cost);
        }
    }
}
#[test]
fn token_rejects_changed_admitted_snapshot_before_consuming_authority() {
    let mut g = Graph::new();
    let mut d = Class::plain("D");
    d.method("main", DESC, 9, &[0xb1], 0, 1);
    let d = g.add(g.boot, d);
    let mut h = g.ready();
    let extra = g.add(g.boot, Class::plain("Extra"));
    g.prepare(&[extra]);
    let p = g.plan(extra);
    let mut c = context(&mut g, &mut h);
    let entry = c.resolve_application_main(d).unwrap();
    let registry = c.registry();
    c.heap_mut()
        .admit_instance_layouts(registry, &[&p])
        .unwrap();
    assert!(c.check_application_main(&entry).is_err());
}

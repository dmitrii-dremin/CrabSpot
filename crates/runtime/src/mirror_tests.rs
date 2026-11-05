use super::*;
use crate::mirror_support::*;

#[test]
fn every_reservation_failure_preserves_nonempty_roots_payloads_counters_and_edges() {
    for fail in 0..3 {
        let mut w = world(ClassMirrorLimits::default());
        let d = w.caller;
        let object = w.u.object;
        let mut c = ctx(&mut w);
        let old = c.class_mirror_for_type(ReferenceType::Class(d)).unwrap();
        let before = (
            c.heap().entries.len(),
            c.heap().allocated_instances(),
            c.heap().total_instance_slots(),
            c.heap().class_mirror_roots().collect::<Vec<_>>(),
        );
        c.heap_mut().mirrors.as_mut().unwrap().fail_reserve_after = Some(fail);
        assert_eq!(
            *c.class_mirror_for_type(ReferenceType::Class(object))
                .unwrap_err(),
            ClassMirrorError::AllocationFailure
        );
        assert_eq!(
            (
                c.heap().entries.len(),
                c.heap().allocated_instances(),
                c.heap().total_instance_slots(),
                c.heap().class_mirror_roots().collect::<Vec<_>>()
            ),
            before
        );
        assert_eq!(
            c.heap().mirror_represented_type(old).unwrap(),
            Some(ReferenceType::Class(d))
        );
        assert_eq!(c.heap().children(old).unwrap().count(), 0);
        c.heap_mut().mirrors.as_mut().unwrap().fail_reserve_after = None;
        let next = c
            .class_mirror_for_type(ReferenceType::Class(object))
            .unwrap();
        assert_ne!(old, next);
        assert_eq!(c.heap().class_mirror_count(), 2);
    }
}
#[test]
fn corrupt_index_roots_and_payloads_are_host_refusals_before_new_publication() {
    for damage in 0..4 {
        let mut w = world(ClassMirrorLimits::default());
        let d = w.caller;
        let object = w.u.object;
        let mut c = ctx(&mut w);
        let old = c.class_mirror_for_type(ReferenceType::Class(d)).unwrap();
        let m = c.heap_mut().mirrors.as_mut().unwrap();
        match damage {
            0 => {
                m.roots.clear();
            }
            1 => {
                m.limit = 0;
            }
            2 => {
                m.index.insert(
                    ReferenceType::Class(d),
                    Reference::issued(999, 0, 1).heap_ref().unwrap(),
                );
            }
            _ => {
                let entry = c.heap_mut().entries.first_mut().unwrap();
                entry.payload = EntryPayload::Mirror(ReferenceType::Class(object));
            }
        }
        let count = c.heap().allocated_instances();
        let entries = c.heap().entries.len();
        assert!(c.class_mirror_for_type(ReferenceType::Class(d)).is_err());
        assert_eq!(c.heap().allocated_instances(), count);
        assert_eq!(c.heap().entries.len(), entries);
        assert!(old.heap_ref().is_some());
    }
}
#[test]
fn instance_count_overflow_and_cap_precede_reservations_and_preserve_old_mirror() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let object = w.u.object;
    let mut c = ctx(&mut w);
    let old = c.class_mirror_for_type(ReferenceType::Class(d)).unwrap();
    for (count, cap) in [(1, 1), (usize::MAX, usize::MAX)] {
        let s = c.heap_mut().instances.as_mut().unwrap();
        s.count = count;
        s.options.max_instances = cap;
        c.heap_mut().mirrors.as_mut().unwrap().fail_reserve_after = Some(0);
        assert!(matches!(
            *c.class_mirror_for_type(ReferenceType::Class(object))
                .unwrap_err(),
            ClassMirrorError::Instance(InstanceError::ResourceLimit {
                resource: InstanceResource::Instances,
                ..
            })
        ));
        assert_eq!(c.heap().entries.len(), 1);
        assert_eq!(c.heap().class_mirror_count(), 1);
        assert_eq!(
            c.class_mirror_for_type(ReferenceType::Class(d)).unwrap(),
            old
        );
    }
}
#[test]
fn corrupt_class_site_success_rank_and_loader_are_checked_and_denial_stays_terminal() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let index = w.sites[0];
    let boot = w.u.bootstrap_loader;
    let mut c = ctx(&mut w);
    c.resolve_class_mirror_type(d, index).unwrap();
    let s = c
        .member
        .as_mut()
        .unwrap()
        .store
        .class_sites
        .as_mut()
        .unwrap();
    s.rows.insert(
        (d, index),
        ClassSiteOutcome::Resolved(ReferenceType::Array {
            dimensions: 0,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
            defining_loader: boot,
        }),
    );
    assert!(matches!(
        *c.resolve_class_mirror_type(d, index).unwrap_err(),
        ClassMirrorError::Type(_)
    ));
    c.member
        .as_mut()
        .unwrap()
        .store
        .class_sites
        .as_mut()
        .unwrap()
        .rows
        .insert((d, index), ClassSiteOutcome::IllegalAccess);
    assert_eq!(
        *c.resolve_class_mirror_type(d, index).unwrap_err(),
        ClassMirrorError::IllegalAccess
    );
    assert_eq!(c.heap().entries.len(), 0);
}
#[test]
fn stale_and_foreign_mirror_handles_remain_checked_in_inspection_type_and_fields() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let class = w.class;
    let field = w.g.field(class, "opaque", "I");
    let mut c = ctx(&mut w);
    let old = c.class_mirror_for_type(ReferenceType::Class(d)).unwrap();
    let (heap, index, generation) = old.heap_ref().unwrap().components();
    for r in [
        Reference::issued(heap + 1, index, generation),
        Reference::issued(heap, index + 1, generation),
        Reference::issued(heap, index, generation + 1),
    ] {
        assert!(c.heap().mirror_represented_type(r).is_err());
        assert!(c.reference_type(r).is_err());
        assert!(c.heap().get_field(r, field).is_err());
    }
    assert_eq!(
        c.heap().mirror_represented_type(old).unwrap(),
        Some(ReferenceType::Class(d))
    );
}
#[test]
fn policies_overflow_and_error_sources_are_structured() {
    assert!(matches!(
        *next(u64::MAX, u64::MAX, ClassMirrorResource::Mirrors).unwrap_err(),
        ClassMirrorError::Overflow {
            resource: ClassMirrorResource::Mirrors
        }
    ));
    assert!(
        std::error::Error::source(&ClassMirrorError::Instance(InstanceError::Disabled)).is_some()
    );
    assert!(
        std::error::Error::source(&ClassMirrorError::Type(Box::new(
            TypeAccessError::MissingUniverse
        )))
        .is_some()
    );
    assert!(
        std::error::Error::source(&ClassMirrorError::Context(Box::new(
            FieldAccessError::ContextChanged
        )))
        .is_some()
    );
}

#[test]
fn failed_class_site_reservation_preserves_terminal_prefix_without_resolving_or_minting() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let good = w.sites[0];
    let vacant = w.sites[1];
    let denied = w.sites[9];
    let mut c = ctx(&mut w);
    let ty = c.resolve_class_mirror_type(d, good).unwrap();
    assert!(c.resolve_class_mirror_type(d, denied).is_err());
    let old = c
        .member
        .as_ref()
        .unwrap()
        .store
        .class_sites
        .as_ref()
        .unwrap()
        .rows
        .clone();
    c.member
        .as_mut()
        .unwrap()
        .store
        .class_sites
        .as_mut()
        .unwrap()
        .fail_reserve = true;
    let before = c.lookup_steps();
    assert_eq!(
        *c.resolve_class_mirror_type(d, vacant).unwrap_err(),
        ClassMirrorError::AllocationFailure
    );
    assert_eq!(c.lookup_steps() - before, 2);
    assert_eq!(
        c.member
            .as_ref()
            .unwrap()
            .store
            .class_sites
            .as_ref()
            .unwrap()
            .rows,
        old
    );
    assert_eq!(c.heap().class_mirror_count(), 0);
    assert_eq!(c.resolve_class_mirror_type(d, good).unwrap(), ty);
    assert_eq!(
        *c.resolve_class_mirror_type(d, denied).unwrap_err(),
        ClassMirrorError::IllegalAccess
    );
    c.member
        .as_mut()
        .unwrap()
        .store
        .class_sites
        .as_mut()
        .unwrap()
        .fail_reserve = false;
    assert_eq!(c.resolve_class_mirror_type(d, vacant).unwrap(), ty);
}

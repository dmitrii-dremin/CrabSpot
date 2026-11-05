//! Public canonical mirror service and separate domain-site transactions.
mod mirror_support;
use crabspot_runtime::*;
use mirror_support::*;
#[test]
fn domain_enablement_and_pristine_binding_are_idempotent_and_policy_checked() {
    let mut g = Graph::new();
    assert_eq!(
        *g.domain.enable_class_mirror_sites(1).unwrap_err(),
        ClassMirrorError::Disabled
    );
    g.enable(MemberResolutionLimits::default());
    g.domain.enable_class_mirror_sites(0).unwrap();
    g.domain.enable_class_mirror_sites(0).unwrap();
    assert_eq!(
        *g.domain.enable_class_mirror_sites(1).unwrap_err(),
        ClassMirrorError::PolicyConflict
    );
    let mut w = world(ClassMirrorLimits::default());
    w.h.bind_class_mirrors(w.g.domain.registry(), w.class, 16384)
        .unwrap();
    assert_eq!(
        *w.h.bind_class_mirrors(w.g.domain.registry(), w.class, 1)
            .unwrap_err(),
        ClassMirrorError::PolicyConflict
    );
    assert_eq!(w.h.class_mirror_roots().len(), 0);
    assert_eq!(w.g.domain.class_site_count(), 0);
    assert!(w.g.domain.class_site(w.caller, 0).is_err());
    ctx(&mut w).check_class_mirror_profile().unwrap();
}
#[test]
fn separate_cp_aliases_share_heap_identity_but_not_domain_site_rows() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let indices = w.sites.clone();
    let mut c = ctx(&mut w);
    let a = c.resolve_class_mirror_type(d, indices[0]).unwrap();
    let r = c.class_mirror_for_type(a).unwrap();
    let duplicate = c.resolve_class_mirror_type(d, indices[1]).unwrap();
    assert_eq!(a, duplicate);
    assert_eq!(r, c.class_mirror_for_type(duplicate).unwrap());
    assert_eq!(c.heap().class_mirror_count(), 1);
    assert_eq!(c.heap().allocated_instances(), 1);
    assert_eq!(c.heap().total_instance_slots(), 0);
    assert_eq!(c.heap().allocated_arrays(), 0);
    assert_eq!(c.heap().payload_bytes(), 0);
    assert_eq!(c.heap().children(r).unwrap().count(), 0);
    assert_eq!(
        c.heap().class_mirror_roots().collect::<Vec<_>>(),
        [r.heap_ref().unwrap()]
    );
    assert_eq!(w.g.domain.class_site_count(), 2);
    assert_eq!(w.g.domain.member_resolution_usage().sites, 0);
}
#[test]
fn scalar_i_v_and_array_spellings_are_distinct_and_class_is_self_typed() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let class = w.class;
    let sites = w.sites.clone();
    let mut c = ctx(&mut w);
    let mut refs = Vec::new();
    for index in &sites[2..9] {
        let ty = c.resolve_class_mirror_type(d, *index).unwrap();
        refs.push(c.class_mirror_for_type(ty).unwrap());
    }
    for (i, a) in refs.iter().enumerate() {
        for b in &refs[i + 1..] {
            assert_ne!(a, b);
        }
        assert_eq!(
            c.reference_type(*a).unwrap(),
            Some(ReferenceType::Class(class))
        );
    }
    assert_eq!(
        c.heap().mirror_represented_type(refs[5]).unwrap(),
        Some(ReferenceType::Class(class))
    );
    let actual = c.reference_type(refs[5]).unwrap().unwrap();
    assert_eq!(refs[5], c.class_mirror_for_type(actual).unwrap());
    assert_eq!(
        c.heap().mirror_represented_type(Reference::null()).unwrap(),
        None
    );
}
#[test]
fn all_primitive_kinds_and_maximum_rank_are_canonical_strong_roots() {
    let mut w = world(ClassMirrorLimits::default());
    let boot = w.u.bootstrap_loader;
    let class = w.class;
    let mut c = ctx(&mut w);
    let mut roots = Vec::new();
    for k in [
        PrimitiveArrayKind::Boolean,
        PrimitiveArrayKind::Byte,
        PrimitiveArrayKind::Char,
        PrimitiveArrayKind::Short,
        PrimitiveArrayKind::Int,
        PrimitiveArrayKind::Long,
        PrimitiveArrayKind::Float,
        PrimitiveArrayKind::Double,
    ] {
        for dimensions in [1, 2, 255] {
            let ty = ReferenceType::Array {
                dimensions,
                element: ArrayElement::Primitive(k),
                defining_loader: boot,
            };
            let r = c.class_mirror_for_type(ty).unwrap();
            assert_eq!(r, c.class_mirror_for_type(ty).unwrap());
            assert_eq!(c.heap().instance_class(r).unwrap(), class);
            roots.push(r.heap_ref().unwrap());
        }
    }
    assert_eq!(c.heap().class_mirror_roots().collect::<Vec<_>>(), roots);
    assert_eq!(c.heap().allocated_instances(), 24);
}
#[test]
fn exact_mirror_and_instance_caps_leave_old_roots_and_hits_usable() {
    let mut w = world(ClassMirrorLimits {
        max_mirrors: 1,
        max_class_sites: 1,
    });
    let d = w.caller;
    let sites = w.sites.clone();
    let mut c = ctx(&mut w);
    let ty = c.resolve_class_mirror_type(d, sites[0]).unwrap();
    let r = c.class_mirror_for_type(ty).unwrap();
    assert!(matches!(
        *c.resolve_class_mirror_type(d, sites[1]).unwrap_err(),
        ClassMirrorError::Limit {
            resource: ClassMirrorResource::ClassSites,
            limit: 1,
            required: 2
        }
    ));
    let other = ReferenceType::Class(d);
    assert!(matches!(
        *c.class_mirror_for_type(other).unwrap_err(),
        ClassMirrorError::Limit {
            resource: ClassMirrorResource::Mirrors,
            limit: 1,
            required: 2
        }
    ));
    assert_eq!(r, c.class_mirror_for_type(ty).unwrap());
    assert_eq!(c.heap().class_mirror_count(), 1);
    let mut z = world(ClassMirrorLimits {
        max_mirrors: 0,
        max_class_sites: 0,
    });
    let d = z.caller;
    let site = z.sites[0];
    assert!(matches!(
        *ctx(&mut z).resolve_class_mirror_type(d, site).unwrap_err(),
        ClassMirrorError::Limit {
            resource: ClassMirrorResource::ClassSites,
            limit: 0,
            required: 1
        }
    ));
    assert_eq!(z.g.domain.class_site_count(), 0);
    assert_eq!(z.h.class_mirror_count(), 0);
}
#[test]
fn warm_hits_pay_two_steps_and_funded_failed_mint_retains_work_without_publication() {
    let mut w = world(ClassMirrorLimits::default());
    let ty = ReferenceType::Class(w.caller);
    let mut c = ctx(&mut w);
    let start = c.lookup_steps();
    let r = c.class_mirror_for_type(ty).unwrap();
    assert_eq!(c.lookup_steps() - start, 3);
    let start = c.lookup_steps();
    assert_eq!(r, c.class_mirror_for_type(ty).unwrap());
    assert_eq!(c.lookup_steps() - start, 2);
    for _ in 0..10_000 {
        assert_eq!(r, c.class_mirror_for_type(ty).unwrap());
    }
    assert_eq!(c.heap().class_mirror_count(), 1);
    assert_eq!(c.heap().class_mirror_roots().len(), 1);
}
#[test]
fn mirrors_use_ordinary_reference_assignment_but_never_class_backing_fields() {
    let mut w = world(ClassMirrorLimits::default());
    let class = w.class;
    let d = w.caller;
    let object = w.u.object;
    let opaque = w.g.field(class, "opaque", "I");
    let inherited = w.g.field(object, "root", "I");
    let inspect = w.g.method(class, "inspect", "()I");
    let mut c = ctx(&mut w);
    let r = c.class_mirror_for_type(ReferenceType::Class(d)).unwrap();
    assert_eq!(
        c.heap().get_field(r, opaque),
        Err(InstanceError::OpaqueClassObject { class })
    );
    assert_eq!(
        c.heap().get_field(r, inherited),
        Err(InstanceError::OpaqueClassObject { class })
    );
    assert_eq!(
        c.heap_mut().put_field(r, opaque, Value::Int(1)),
        Err(InstanceError::OpaqueClassObject { class })
    );
    assert_eq!(
        c.heap_mut().allocate_instance(class),
        Err(InstanceError::ReservedClassObject { class })
    );
    assert_eq!(
        c.check_method_reference_profile(inspect, Some(r)),
        Err(MethodAccessError::Context(FieldAccessError::Instance(
            InstanceError::OpaqueClassObject { class }
        )))
    );
    let actual = c.reference_type(r).unwrap().unwrap();
    assert!(
        c.is_assignable(actual, ReferenceType::Class(object))
            .unwrap()
    );
}
#[test]
fn exact_native_binding_and_nominal_array_marker_and_wrong_opcode_paths() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let methods = w.methods.clone();
    let object = w.u.object;
    let native = w.g.method(object, "getClass", "()Ljava/lang/Class;");
    let plain = w.g.method(object, "plain", "()V");
    let array =
        w.h.allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
    let instance = w.h.allocate_instance(d).unwrap();
    let mut c = ctx(&mut w);
    assert!(c.is_object_get_class(native).unwrap());
    assert!(!c.is_object_get_class(plain).unwrap());
    for i in [0, 1] {
        assert_eq!(c.select_virtual(d, methods[i], array).unwrap(), native);
        assert_eq!(c.select_virtual(d, methods[i], array).unwrap(), native);
    }
    for i in [3, 4] {
        assert_eq!(c.select_interface(d, methods[i], array).unwrap(), native);
        assert_eq!(
            c.select_interface(d, methods[i], instance),
            Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange
            ))
        );
        assert_eq!(c.select_interface(d, methods[i], array).unwrap(), native);
    }
    assert_eq!(
        c.resolve_static(d, methods[1]),
        Err(MethodAccessError::Java(
            MethodJavaError::IncompatibleClassChange
        ))
    );
    assert_eq!(
        c.select_interface(d, methods[5], array),
        Err(MethodAccessError::Java(
            MethodJavaError::IncompatibleClassChange
        ))
    );
    assert!(matches!(
        c.select_special(d, methods[1], array),
        Err(MethodAccessError::Context(
            FieldAccessError::UnsupportedArrayOwner
        ))
    ));
    assert!(matches!(
        c.select_virtual(d, methods[6], array),
        Err(MethodAccessError::Context(
            FieldAccessError::UnsupportedArrayOwner
        ))
    ));
    assert_eq!(
        c.select_virtual(d, methods[1], Reference::null()),
        Err(MethodAccessError::Java(MethodJavaError::NullPointer))
    );
    assert_eq!(c.heap().class_mirror_count(), 0);
}
#[test]
fn array_owner_covariance_uses_symbolic_array_identity() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let i = w.methods[2];
    let native = w.g.method(w.u.object, "getClass", "()Ljava/lang/Class;");
    // Allocation uses its own checked CP type; Class[] is an Object[] subtype.
    let class_array = w.sites[8];
    let mut c = ctx(&mut w);
    let ty = c.resolve_class_mirror_type(d, class_array).unwrap();
    let r = c.allocate_reference_array(ty, 0).unwrap();
    assert_eq!(c.select_virtual(d, i, r).unwrap(), native);
    let primitive = c
        .heap_mut()
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    assert!(c.select_virtual(d, i, primitive).is_err());
}
#[test]
fn heap_affinity_opaque_handles_and_identical_rebind_survive_moving_the_heap() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let r = ctx(&mut w)
        .class_mirror_for_type(ReferenceType::Class(d))
        .unwrap();
    w.h.bind_class_mirrors(w.g.domain.registry(), w.class, 16384)
        .unwrap();
    let mut other = world(ClassMirrorLimits::default());
    assert_eq!(
        other.h.mirror_represented_type(r),
        Err(HeapError::ForeignHeap)
    );
    assert!(
        ctx(&mut other)
            .class_mirror_for_type(ReferenceType::Class(d))
            .is_err()
    );
    let moved = w.h;
    assert_eq!(moved.class_mirror_roots().next(), r.heap_ref());
    assert_eq!(
        moved.mirror_represented_type(r).unwrap(),
        Some(ReferenceType::Class(d))
    );
}
#[test]
fn sealed_layout_growth_is_atomic_but_identical_readmission_is_allowed() {
    let mut w = world(ClassMirrorLimits::default());
    let plan = w.g.plan(w.caller);
    w.h.admit_instance_layouts(w.g.domain.registry(), &[&plan])
        .unwrap();
    let late = w.g.add(w.g.boot, Class::plain("p/Late"));
    let p = w.g.plan(late);
    let before = w.h.retained_layout_classes();
    assert_eq!(
        w.h.admit_instance_layouts(w.g.domain.registry(), &[&p]),
        Err(InstanceError::MirrorLayoutSealed)
    );
    assert_eq!(w.h.retained_layout_classes(), before);
    let d = w.caller;
    let r = ctx(&mut w)
        .class_mirror_for_type(ReferenceType::Class(d))
        .unwrap();
    assert!(r.heap_ref().is_some());
}

#[test]
fn class_denial_and_member_category_replay_on_smaller_unbound_heap() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let denied = w.sites[9];
    let array_denied = w.sites[10];
    let wrong = w.methods[5];
    let hidden_call = w.methods[7];
    let mut c = ctx(&mut w);
    for i in [denied, array_denied] {
        assert_eq!(
            *c.resolve_class_mirror_type(d, i).unwrap_err(),
            ClassMirrorError::IllegalAccess
        );
    }
    let array = c
        .heap_mut()
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    assert_eq!(
        c.select_interface(d, wrong, array),
        Err(MethodAccessError::Java(
            MethodJavaError::IncompatibleClassChange
        ))
    );
    assert_eq!(
        c.select_virtual(d, hidden_call, Reference::null()),
        Err(MethodAccessError::Java(MethodJavaError::IllegalAccess))
    );
    let count = w.g.domain.class_site_count();
    let mut smaller = w.g.heap(&[d]);
    assert_eq!(smaller.reference_type_universe(), None);
    let mut c =
        w.g.domain
            .member_context_unnamed_modules(
                &mut smaller,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
    assert_eq!(
        *c.resolve_class_mirror_type(d, denied).unwrap_err(),
        ClassMirrorError::IllegalAccess
    );
    assert_eq!(
        c.select_interface(d, wrong, Reference::null()),
        Err(MethodAccessError::Java(
            MethodJavaError::IncompatibleClassChange
        ))
    );
    assert_eq!(
        c.select_virtual(d, hidden_call, Reference::null()),
        Err(MethodAccessError::Java(MethodJavaError::IllegalAccess))
    );
    assert_eq!(c.heap().class_mirror_count(), 0);
    assert_eq!(w.g.domain.class_site_count(), count);
}
#[test]
fn successful_sites_require_current_leaf_and_universe_but_remain_repairable() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let site = w.sites[0];
    let missing = w.sites[11];
    let ty = ctx(&mut w).resolve_class_mirror_type(d, site).unwrap();
    let count = w.g.domain.class_site_count();
    assert!(ctx(&mut w).resolve_class_mirror_type(d, missing).is_err());
    assert_eq!(w.g.domain.class_site_count(), count);
    assert_eq!(w.g.domain.class_site(d, missing).unwrap(), None);
    let mut smaller = w.g.heap(&[d]);
    assert!(
        w.g.domain
            .member_context_unnamed_modules(
                &mut smaller,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default()
            )
            .unwrap()
            .resolve_class_mirror_type(d, site)
            .is_err()
    );
    assert_eq!(
        w.g.domain.class_site(d, site).unwrap(),
        Some(ClassSiteOutcome::Resolved(ty))
    );
    let target = w.g.add(w.g.boot, Class::plain("missing/Target"));
    w.g.prepare(&[target]);
    let ids = w.g.classes.clone();
    let mut repaired = w.g.heap(&ids);
    repaired
        .bind_reference_types(w.g.domain.registry(), w.u)
        .unwrap();
    let mut c =
        w.g.domain
            .member_context_unnamed_modules(
                &mut repaired,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
    assert_eq!(
        c.resolve_class_mirror_type(d, missing).unwrap(),
        ReferenceType::Class(target)
    );
    assert_eq!(
        *c.class_mirror_for_type(ReferenceType::Class(target))
            .unwrap_err(),
        ClassMirrorError::Disabled
    );
}
#[test]
fn two_compatible_heaps_reuse_domain_type_success_but_issue_distinct_references() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let site = w.sites[0];
    let mut c = ctx(&mut w);
    let ty = c.resolve_class_mirror_type(d, site).unwrap();
    let a = c.class_mirror_for_type(ty).unwrap();
    let mut second = w.g.heap(&w.g.classes);
    second
        .bind_reference_types(w.g.domain.registry(), w.u)
        .unwrap();
    second
        .bind_class_mirrors(w.g.domain.registry(), w.class, 1)
        .unwrap();
    let mut c =
        w.g.domain
            .member_context_unnamed_modules(
                &mut second,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
    assert_eq!(c.resolve_class_mirror_type(d, site).unwrap(), ty);
    let b = c.class_mirror_for_type(ty).unwrap();
    assert_ne!(a, b);
    assert_eq!(w.h.class_mirror_count(), 1);
    assert_eq!(second.class_mirror_count(), 1);
    assert_eq!(w.g.domain.class_site_count(), 1);
}
#[test]
fn malformed_exact_native_roles_refuse_binding_and_different_methods_remain_ordinary() {
    for flags in [0x101, 0x119, 0x131, 0x114, 1] {
        let mut w = world_role(ClassMirrorLimits::default(), flags);
        let m = w.g.method(w.u.object, "getClass", "()Ljava/lang/Class;");
        assert!(matches!(
            *ctx(&mut w).is_object_get_class(m).unwrap_err(),
            ClassMirrorError::InvalidBinding(_)
        ));
        assert_eq!(w.h.class_mirror_count(), 0);
    }
}
#[test]
fn invalid_rank_loader_and_foreign_nominal_types_never_mint_or_alias() {
    let mut w = world(ClassMirrorLimits {
        max_mirrors: u64::MAX,
        max_class_sites: u64::MAX,
    });
    let boot = w.u.bootstrap_loader;
    let foreign_loader = w.g.loader();
    let other = w.g.add(foreign_loader, Class::plain("p/A"));
    let types = [
        ReferenceType::Array {
            dimensions: 0,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
            defining_loader: boot,
        },
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
            defining_loader: foreign_loader,
        },
        ReferenceType::Class(other),
    ];
    let mut c = ctx(&mut w);
    for ty in types {
        assert!(c.class_mirror_for_type(ty).is_err());
        assert_eq!(c.heap().class_mirror_count(), 0);
    }
}
#[test]
fn checked_reference_fields_and_class_arrays_retain_actual_mirror_edges() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let class = w.class;
    let held = w.held;
    let opaque = w.opaque;
    let receiver = w.h.allocate_instance(d).unwrap();
    let mut c = ctx(&mut w);
    let mirror = c.class_mirror_for_type(ReferenceType::Class(d)).unwrap();
    c.write_field(d, held, receiver, Value::Reference(mirror))
        .unwrap();
    assert_eq!(
        c.read_field(d, held, receiver).unwrap(),
        Value::Reference(mirror)
    );
    assert_eq!(
        c.heap().children(receiver).unwrap().collect::<Vec<_>>(),
        [mirror.heap_ref().unwrap()]
    );
    let array = c
        .allocate_reference_array(ReferenceType::Class(class), 2)
        .unwrap();
    c.write_reference_array(array, 1, mirror).unwrap();
    assert_eq!(c.read_reference_array(array, 1).unwrap(), mirror);
    assert_eq!(
        c.heap().children(array).unwrap().collect::<Vec<_>>(),
        [mirror.heap_ref().unwrap()]
    );
    assert_eq!(
        c.read_field(d, opaque, mirror),
        Err(FieldAccessError::Instance(
            InstanceError::OpaqueClassObject { class }
        ))
    );
    assert_eq!(
        c.write_field(d, opaque, mirror, Value::Int(7)),
        Err(FieldAccessError::Instance(
            InstanceError::OpaqueClassObject { class }
        ))
    );
    assert_eq!(c.heap().children(mirror).unwrap().count(), 0);
}
#[test]
fn same_spelling_nominal_and_array_types_from_distinct_defining_loaders_do_not_alias() {
    let mut w = world(ClassMirrorLimits::default());
    let l = w.g.loader();
    let other = w.g.add(l, Class::plain("p/A"));
    w.g.prepare(&[other]);
    let original =
        w.g.domain
            .registry()
            .lookup_class(w.g.boot, &units("p/A"))
            .unwrap()
            .unwrap();
    let mut h = w.g.heap(&w.g.classes);
    h.bind_reference_types(w.g.domain.registry(), w.u).unwrap();
    h.bind_class_mirrors(w.g.domain.registry(), w.class, 4)
        .unwrap();
    let mut c =
        w.g.domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
    let a = c
        .class_mirror_for_type(ReferenceType::Class(original))
        .unwrap();
    let b = c
        .class_mirror_for_type(ReferenceType::Class(other))
        .unwrap();
    assert_ne!(a, b);
    let a = c
        .class_mirror_for_type(ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Class(original),
            defining_loader: w.g.boot,
        })
        .unwrap();
    let b = c
        .class_mirror_for_type(ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Class(other),
            defining_loader: l,
        })
        .unwrap();
    assert_ne!(a, b);
}
#[test]
fn exact_mint_work_boundaries_fail_before_publication_and_keep_paid_prefix() {
    for additional in [0, 1, 2, 3] {
        let mut w = world(ClassMirrorLimits::default());
        let d = w.caller;
        let base = ctx(&mut w).lookup_steps();
        let mut c =
            w.g.domain
                .member_context_unnamed_modules(
                    &mut w.h,
                    FieldResolutionOptions {
                        max_lookup_steps: base + additional,
                        ..Default::default()
                    },
                    LoadingWorkLimits::default(),
                )
                .unwrap();
        let result = c.class_mirror_for_type(ReferenceType::Class(d));
        if additional == 3 {
            assert!(result.is_ok());
            assert_eq!(c.heap().class_mirror_count(), 1);
        } else {
            assert!(result.is_err());
            assert_eq!(c.heap().class_mirror_count(), 0);
        }
        assert_eq!(c.lookup_steps(), base + additional);
    }
}
#[test]
fn legacy_unbound_class_allocation_cannot_be_reinterpreted_by_late_binding() {
    let mut w = world(ClassMirrorLimits::default());
    let mut h = w.g.heap(&w.g.classes);
    h.bind_reference_types(w.g.domain.registry(), w.u).unwrap();
    let ordinary = h.allocate_instance(w.class).unwrap();
    assert_eq!(h.mirror_represented_type(ordinary).unwrap(), None);
    assert!(matches!(
        *h.bind_class_mirrors(w.g.domain.registry(), w.class, 1)
            .unwrap_err(),
        ClassMirrorError::InvalidBinding(_)
    ));
    assert_eq!(h.class_mirror_count(), 0);
    assert_eq!(h.instance_class(ordinary).unwrap(), w.class);
    assert!(matches!(
        *w.h.bind_class_mirrors(w.g.domain.registry(), w.caller, 1)
            .unwrap_err(),
        ClassMirrorError::InvalidBinding(_)
    ));
}
#[test]
fn nonfinal_or_wrong_loader_class_roles_and_native_code_fail_at_checked_boundaries() {
    for (flags, foreign) in [(0x21, false), (0x31, true)] {
        let mut g = Graph::new();
        let loader = if foreign { g.loader() } else { g.boot };
        let class = g.add(
            loader,
            Class::new("java/lang/Class", Some("java/lang/Object"), flags),
        );
        let mut h = g.ready();
        g.bind_core(&mut h);
        assert!(matches!(
            *h.bind_class_mirrors(g.domain.registry(), class, 1)
                .unwrap_err(),
            ClassMirrorError::InvalidBinding(_)
        ));
        assert_eq!(h.class_mirror_count(), 0);
    }
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 6];
    for (tag, text) in [
        (1, b"java/lang/Object".as_slice()),
        (7, &[0, 1]),
        (1, b"getClass"),
        (1, b"()Ljava/lang/Class;"),
        (1, b"Code"),
    ] {
        bytes.push(tag);
        if tag == 1 {
            bytes.extend((text.len() as u16).to_be_bytes());
        }
        bytes.extend(text);
    }
    for word in [0x21_u16, 2, 0, 0, 0, 1, 0x111, 3, 4, 1, 5] {
        bytes.extend(word.to_be_bytes());
    }
    bytes.extend(14_u32.to_be_bytes());
    bytes.extend([0, 1, 0, 1, 0, 0, 0, 2, 1, 0xb0, 0, 0, 0, 0, 0, 0]);
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    assert!(matches!(
        registry.define(
            loader,
            &units("java/lang/Object"),
            &bytes,
            crabspot_classfile::ParseOptions::default()
        ),
        Err(RegistryError::Parse(crabspot_classfile::ParseError {
            kind: crabspot_classfile::ErrorKind::InvalidStructure(
                "concrete methods require Code; abstract/native methods must not have Code"
            ),
            ..
        }))
    ));
}

#[test]
fn constructor_authority_refuses_existing_mirrors_after_member_resolution() {
    let mut w = world(ClassMirrorLimits::default());
    let d = w.caller;
    let class = w.class;
    let indices = w.constructors;
    let cc = w.g.method(class, "<init>", "()V");
    let dc = w.g.method(d, "<init>", "()V");
    let throwable =
        w.g.domain
            .registry()
            .lookup_class(w.g.boot, &units("java/lang/Throwable"))
            .unwrap()
            .unwrap();
    let error =
        w.g.domain
            .registry()
            .lookup_class(w.g.boot, &units("java/lang/Error"))
            .unwrap()
            .unwrap();
    let mut c = ctx(&mut w);
    let ordinary = c.heap_mut().allocate_instance(d).unwrap();
    let mirror = c.class_mirror_for_type(ReferenceType::Class(d)).unwrap();
    let statics = c.allocate_static_state(Default::default()).unwrap();
    let exceptions = c.prepare_exception_profile(throwable).unwrap();
    let claim = c
        .claim_initialization(&statics, &exceptions, error)
        .unwrap();
    for (i, kind) in [
        (indices[0], ConstructorReceiverKind::Allocation),
        (indices[1], ConstructorReceiverKind::This),
    ] {
        assert_eq!(
            c.resolve_constructor(&claim, cc, i, mirror, kind)
                .unwrap_err(),
            ConstructionAccessError::Instance(Box::new(InstanceError::OpaqueClassObject { class }))
        );
    }
    assert_eq!(
        c.resolve_constructor(
            &claim,
            dc,
            indices[2],
            ordinary,
            ConstructorReceiverKind::Allocation
        )
        .unwrap()
        .method(),
        dc
    );
    assert_eq!(c.heap().class_mirror_count(), 1);
    assert!(matches!(
        w.g.domain.member_site(class, indices[0]).unwrap(),
        Some(MemberResolutionOutcome::Method(_))
    ));
    assert!(matches!(
        w.g.domain.member_site(class, indices[1]).unwrap(),
        Some(MemberResolutionOutcome::Method(_))
    ));
}

//! Independent bytecode boundaries for passive canonical Class objects.
#[path = "support/mirror_world.rs"]
mod world;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use world::*;
#[test]
fn repeated_sites_duplicate_class_slots_and_wide_indices_share_one_heap_mirror() {
    let mut c = app();
    let a = c.class("C");
    let alias = c.class("C");
    c.method(
        "one",
        "()Ljava/lang/Class;",
        9,
        &[ldc(a), vec![0xb0]].concat(),
        1,
        0,
    );
    c.method(
        "two",
        "()Ljava/lang/Class;",
        9,
        &[ldc(alias), vec![0xb0]].concat(),
        1,
        0,
    );
    for i in 0..270 {
        c.utf(&format!("padding{i}"));
    }
    let wide = c.class("C");
    assert!(wide > 255);
    c.method(
        "wide",
        "()Ljava/lang/Class;",
        9,
        &[ldc(wide), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::new(c);
    let one = w.g.method(w.owner, "one", "()Ljava/lang/Class;");
    let two = w.g.method(w.owner, "two", "()Ljava/lang/Class;");
    let wide_method = w.g.method(w.owner, "wide", "()Ljava/lang/Class;");
    let (a, b, c) = w.with(true, |s, _| {
        let run = |s: &mut InitializationSession<'_, '_>, m| {
            returned(s.run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            ))
        };
        (run(s, one), run(s, two), run(s, wide_method))
    });
    assert_eq!(a, b);
    assert_eq!(b, c);
    assert_eq!(w.h.class_mirror_count(), 1);
    assert_eq!(w.h.allocated_instances(), 1);
    assert_eq!(
        w.h.class_mirror_roots().collect::<Vec<_>>(),
        [a.heap_ref().unwrap()]
    );
    assert_eq!(w.g.domain.class_site_count(), 3);
    for i in [a, b, c] {
        assert_eq!(w.h.instance_class(i).unwrap(), w.class);
        assert_eq!(w.h.children(i).unwrap().count(), 0);
        assert_eq!(
            w.h.mirror_represented_type(i).unwrap(),
            Some(ReferenceType::Class(w.owner))
        );
    }
}
#[test]
fn class_literal_resolution_commits_before_stack_failure_but_mint_does_not() {
    let mut c = app();
    let class = c.class("C");
    c.method(
        "bad",
        "()V",
        9,
        &[ldc(class), vec![0x57, 0xb1]].concat(),
        0,
        0,
    );
    let mut w = World::new(c);
    let e = w.run("bad", "()V", &[]).unwrap_err();
    assert_eq!(
        e.kind,
        ExecutionErrorKind::StackOverflow {
            required: 1,
            capacity: 0
        }
    );
    assert_eq!(e.offset, Some(0));
    assert_eq!(w.h.class_mirror_count(), 0);
    assert_eq!(
        w.g.domain.class_site(w.owner, class).unwrap(),
        Some(ClassSiteOutcome::Resolved(ReferenceType::Class(w.owner)))
    );
}
#[test]
fn mirror_and_class_site_caps_are_distinct_and_late_mint_failure_retains_resolution() {
    for (mirrors, sites, resource) in [
        (0, 1, ClassMirrorResource::Mirrors),
        (1, 0, ClassMirrorResource::ClassSites),
    ] {
        let mut c = app();
        let class = c.class("C");
        c.method(
            "run",
            "()Ljava/lang/Class;",
            9,
            &[ldc(class), vec![0xb0]].concat(),
            1,
            0,
        );
        let mut w = World::build(
            c,
            ObjectNative::Valid,
            ClassMirrorLimits {
                max_mirrors: mirrors,
                max_class_sites: sites,
            },
            true,
        );
        let e = w.run("run", "()Ljava/lang/Class;", &[]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::ClassMirror(Box::new(ClassMirrorError::Limit {
                resource,
                limit: 0,
                required: 1
            }))
        );
        assert_eq!(w.h.class_mirror_count(), 0);
        assert_eq!(w.g.domain.class_site_count(), u64::from(mirrors == 0));
    }
    let mut c = app();
    let class = c.class("C");
    c.method(
        "run",
        "()Ljava/lang/Class;",
        9,
        &[ldc(class), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::build(
        c,
        ObjectNative::Valid,
        ClassMirrorLimits {
            max_mirrors: 1,
            max_class_sites: 1,
        },
        true,
    );
    let m = w.g.method(w.owner, "run", "()Ljava/lang/Class;");
    let (a, b) = w.with(true, |s, _| {
        (
            returned(s.run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )),
            returned(s.run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )),
        )
    });
    assert_eq!(a, b);
    assert_eq!(w.h.class_mirror_count(), 1);
    assert_eq!(w.g.domain.class_site_count(), 1);
}
#[test]
fn exact_native_has_no_java_frame_code_or_extra_fuel_and_class_self_is_canonical() {
    let mut c = app();
    let target = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
    let mut body = vec![0x2a];
    body.extend(instruction(0xb6, target));
    body.push(0xb0);
    c.method(
        "run",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        9,
        &body,
        1,
        1,
    );
    let literal = c.class("java/lang/Class");
    c.method(
        "self",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), instruction(0xb6, target), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::new(c);
    let receiver = w.object();
    let method =
        w.g.method(w.owner, "run", "(Ljava/lang/Object;)Ljava/lang/Class;");
    let self_method = w.g.method(w.owner, "self", "()Ljava/lang/Class;");
    let class = w.class;
    let (actual, self_result) = w.with(true, |s, _| {
        let limits = CallLimits {
            max_frames: 1,
            max_frame_slots: 2,
            max_prepared_code_bytes: 5,
        };
        let r = returned(s.run_static(
            method,
            &[Value::Reference(receiver)],
            ExecutionOptions {
                instruction_budget: 3,
            },
            limits,
            Default::default(),
        ));
        let self_result = returned(s.run_static(
            self_method,
            &[],
            ExecutionOptions {
                instruction_budget: 3,
            },
            CallLimits {
                max_frames: 1,
                max_frame_slots: 1,
                max_prepared_code_bytes: 6,
            },
            Default::default(),
        ));
        (r, self_result)
    });
    assert_eq!(
        w.h.mirror_represented_type(actual).unwrap(),
        Some(ReferenceType::Class(w.owner))
    );
    assert_eq!(
        w.h.mirror_represented_type(self_result).unwrap(),
        Some(ReferenceType::Class(class))
    );
    assert_eq!(w.h.instance_class(actual).unwrap(), class);
    assert_eq!(w.h.class_mirror_count(), 2);
}
#[test]
fn null_and_native_aliases_obey_member_resolution_before_materialization() {
    let mut c = app();
    let target = c.reference(10, "C", "getClass", "()Ljava/lang/Class;");
    let mut body = vec![0x2a];
    body.extend(instruction(0xb6, target));
    body.push(0xb0);
    c.method(
        "run",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        9,
        &body,
        1,
        1,
    );
    let mut w = World::build(
        c,
        ObjectNative::Valid,
        ClassMirrorLimits {
            max_mirrors: 0,
            max_class_sites: 0,
        },
        true,
    );
    let e = w
        .run(
            "run",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            &[Value::Reference(Reference::null())],
        )
        .unwrap_err();
    assert_eq!(
        e.kind,
        ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException)
    );
    assert_eq!(e.offset, Some(1));
    assert_eq!(e.method.as_ref().unwrap().owner, Some(w.owner));
    assert_eq!(w.h.class_mirror_count(), 0);
    assert!(matches!(
        w.g.domain.member_site(w.owner, target).unwrap(),
        Some(MemberResolutionOutcome::Method(_))
    ));
}

#[test]
fn all_real_native_dispatch_forms_include_marker_interfaces_and_nominal_super() {
    for (opcode, owner) in [
        (0xb6, "java/lang/Object"),
        (0xb6, "C"),
        (0xb6, "[I"),
        (0xb9, "java/lang/Cloneable"),
        (0xb9, "java/io/Serializable"),
        (0xb7, "java/lang/Object"),
    ] {
        let mut c = app();
        let index = c.reference(
            if opcode == 0xb9 { 11 } else { 10 },
            owner,
            "getClass",
            "()Ljava/lang/Class;",
        );
        let mut body = vec![0x2a];
        body.extend(instruction(opcode, index));
        if opcode == 0xb9 {
            body.extend([1, 0]);
        }
        body.push(0xb0);
        c.method(
            "run",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            9,
            &body,
            1,
            1,
        );
        let mut w = World::new(c);
        let array = owner.starts_with('[') || opcode == 0xb9;
        let receiver = if array {
            w.h.allocate_primitive_array(PrimitiveArrayKind::Int, 0)
                .unwrap()
        } else {
            w.object()
        };
        let r = returned(w.run(
            "run",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            &[Value::Reference(receiver)],
        ));
        let expected = if array {
            ReferenceType::Array {
                dimensions: 1,
                element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
                defining_loader: w.g.boot,
            }
        } else {
            ReferenceType::Class(w.owner)
        };
        assert_eq!(
            w.h.mirror_represented_type(r).unwrap(),
            Some(expected),
            "opcode{opcode:x} owner{owner}"
        );
        assert!(
            matches!(w.g.domain.member_site(w.owner,index).unwrap(),Some(MemberResolutionOutcome::Method(m))if m==w.g.method(w.g.object,"getClass","()Ljava/lang/Class;"))
        );
    }
}
#[test]
fn object_typed_helper_keeps_runtime_interface_test_strict_for_arrays_and_plain_objects() {
    for (owner, array) in [("java/io/Serializable", false), ("Face", true)] {
        let mut c = app();
        let index = c.reference(11, owner, "getClass", "()Ljava/lang/Class;");
        let mut body = vec![0x2a];
        body.extend(instruction(0xb9, index));
        body.extend([1, 0, 0xb0]);
        c.method(
            "helper",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            9,
            &body,
            1,
            1,
        );
        let mut w = World::with_dependencies(
            c,
            vec![Class::new("Face", Some("java/lang/Object"), 0x601)],
            ObjectNative::Valid,
            ClassMirrorLimits {
                max_mirrors: 0,
                ..Default::default()
            },
            true,
        );
        let r = if array {
            w.h.allocate_primitive_array(PrimitiveArrayKind::Int, 0)
                .unwrap()
        } else {
            w.object()
        };
        let e = w
            .run(
                "helper",
                "(Ljava/lang/Object;)Ljava/lang/Class;",
                &[Value::Reference(r)],
            )
            .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(
                VmExceptionKind::IncompatibleClassChangeError
            )
        );
        assert_eq!(e.offset, Some(1));
        assert_eq!(w.h.class_mirror_count(), 0);
    }
}
#[test]
fn array_symbolic_reference_owner_allows_covariance_but_returns_actual_array_type() {
    let mut c = app();
    let target = c.reference(10, "[Ljava/lang/Object;", "getClass", "()Ljava/lang/Class;");
    let mut code = vec![0x2a];
    code.extend(instruction(0xb6, target));
    code.push(0xb0);
    c.method(
        "run",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        9,
        &code,
        1,
        1,
    );
    let mut w = World::new(c);
    let array =
        w.g.domain
            .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
            .unwrap()
            .allocate_reference_array(ReferenceType::Class(w.owner), 0)
            .unwrap();
    let mirror = returned(w.run(
        "run",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        &[Value::Reference(array)],
    ));
    assert_eq!(
        w.h.mirror_represented_type(mirror).unwrap(),
        Some(ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Class(w.owner),
            defining_loader: w.app
        })
    );
}
#[test]
fn array_wrong_tag_static_category_special_clone_and_count_remain_explicit() {
    for which in 0..5 {
        let mut c = app();
        let target = c.reference(
            if which == 0 || which == 4 { 11 } else { 10 },
            "[I",
            if which == 3 { "clone" } else { "getClass" },
            if which == 3 {
                "()Ljava/lang/Object;"
            } else {
                "()Ljava/lang/Class;"
            },
        );
        let mut code = vec![];
        if which != 1 {
            code.push(0x2a);
        }
        code.extend(instruction(
            match which {
                0 | 4 => 0xb9,
                1 => 0xb8,
                2 => 0xb7,
                _ => 0xb6,
            },
            target,
        ));
        if which == 0 {
            code.extend([1, 0]);
        }
        if which == 4 {
            code.extend([2, 0]);
        }
        code.push(0xb0);
        c.method(
            "run",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            9,
            &code,
            1,
            1,
        );
        let mut w = World::new(c);
        let r =
            w.h.allocate_primitive_array(PrimitiveArrayKind::Int, 0)
                .unwrap();
        let e = w
            .run(
                "run",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[Value::Reference(r)],
            )
            .unwrap_err();
        match which {
            0 | 1 => assert_eq!(
                e.kind,
                ExecutionErrorKind::UnsupportedVmException(
                    VmExceptionKind::IncompatibleClassChangeError
                )
            ),
            2 => assert!(matches!(e.kind, ExecutionErrorKind::MethodAccess(_))),
            3 => assert!(matches!(e.kind, ExecutionErrorKind::MethodAccess(_))),
            _ => assert_eq!(e.kind, ExecutionErrorKind::InvalidOperands),
        }
        assert_eq!(w.h.class_mirror_count(), 0);
    }
}
#[test]
fn native_overload_and_java_overload_do_not_bind_the_exact_service() {
    for native in [false, true] {
        let mut c = app();
        c.method(
            "getClass",
            "(I)I",
            if native { 0x101 } else { 1 },
            &[0x1b, 0xac],
            1,
            2,
        );
        let target = c.reference(10, "C", "getClass", "(I)I");
        let mut code = vec![0x2a, 0x10, 17];
        code.extend(instruction(0xb6, target));
        code.push(0xac);
        c.method("run", "(LC;)I", 9, &code, 2, 1);
        let mut w = World::new(c);
        let r = w.object();
        let result = w.run("run", "(LC;)I", &[Value::Reference(r)]);
        if native {
            assert!(
                matches!(result.unwrap_err().kind,ExecutionErrorKind::UnsupportedMethod(s)if s.contains("native"))
            )
        } else {
            assert_eq!(
                result.unwrap(),
                ExceptionOutcome::Returned(Some(Value::Int(17)))
            )
        }
        assert_eq!(w.h.class_mirror_count(), 0);
    }
}
#[test]
fn malformed_exact_boot_native_fails_lazily_without_fabricated_java_body_entry() {
    for native in [ObjectNative::JavaReplacement, ObjectNative::Flags(0x101)] {
        let mut c = app();
        let target = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
        c.method("unrelated", "()I", 9, &[4, 0xac], 1, 0);
        let mut body = vec![0x2a];
        body.extend(instruction(0xb6, target));
        body.push(0xb0);
        c.method(
            "run",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            9,
            &body,
            1,
            1,
        );
        let mut w = World::build(c, native, Default::default(), true);
        let receiver = w.object();
        let ordinary = w.g.method(w.owner, "unrelated", "()I");
        let get =
            w.g.method(w.owner, "run", "(Ljava/lang/Object;)Ljava/lang/Class;");
        w.with(true,|s,_|{assert_eq!(s.run_static(ordinary,&[],Default::default(),Default::default(),Default::default()).unwrap(),ExceptionOutcome::Returned(Some(Value::Int(1))));let e=s.run_static(get,&[Value::Reference(receiver)],Default::default(),Default::default(),Default::default()).unwrap_err();assert!(matches!(e.kind,ExecutionErrorKind::ClassMirror(ref source)if matches!(**source,ClassMirrorError::InvalidBinding(_))));assert_eq!(e.offset,Some(1));});
        assert_eq!(w.h.class_mirror_count(), 0);
    }
}
#[test]
fn passive_literals_do_not_initialize_represented_classes_or_class_and_i_v_are_nominal() {
    let mut c = app();
    let mut code = vec![];
    let mut indices = vec![];
    for name in ["I", "V", "[I", "[LI;", "[[LI;", "Face"] {
        let i = c.class(name);
        indices.push(i);
        code.extend(ldc(i));
        code.push(0x57);
    }
    code.push(0xb1);
    c.method("run", "()V", 9, &code, 1, 0);
    let mut i = Class::plain("I");
    i.field("mark", "I", 9);
    let mark = i.reference(9, "I", "mark", "I");
    i.method(
        "<clinit>",
        "()V",
        8,
        &[vec![4], instruction(0xb3, mark), vec![0xb1]].concat(),
        1,
        0,
    );
    let mut w = World::with_dependencies(
        c,
        vec![
            i,
            Class::plain("V"),
            Class::new("Face", Some("java/lang/Object"), 0x601),
        ],
        ObjectNative::Valid,
        Default::default(),
        true,
    );
    let method = w.g.method(w.owner, "run", "()V");
    let i =
        w.g.domain
            .registry()
            .lookup_class(w.app, &units("I"))
            .unwrap()
            .unwrap();
    let v =
        w.g.domain
            .registry()
            .lookup_class(w.app, &units("V"))
            .unwrap()
            .unwrap();
    let class = w.class;
    w.with(true, |s, p| {
        assert_eq!(
            s.run_static(
                method,
                &[],
                Default::default(),
                Default::default(),
                Default::default()
            )
            .unwrap(),
            ExceptionOutcome::Returned(None)
        );
        for id in [i, v, class] {
            assert_eq!(s.status(id).unwrap(), InitializationStatus::Uninitialized);
        }
        let field = s
            .context()
            .registry()
            .find_declared_field(i, &units("mark"), &units("I"))
            .unwrap()
            .unwrap();
        assert_eq!(
            s.context().inspect_static(&p, field).unwrap(),
            Value::Int(0)
        );
    });
    assert_eq!(w.h.class_mirror_count(), 6);
    let types = indices
        .iter()
        .map(
            |index| match w.g.domain.class_site(w.owner, *index).unwrap().unwrap() {
                ClassSiteOutcome::Resolved(ty) => ty,
                ClassSiteOutcome::IllegalAccess => panic!("unexpected access failure"),
            },
        )
        .collect::<Vec<_>>();
    assert!(types.contains(&ReferenceType::Class(i)));
    assert!(types.contains(&ReferenceType::Class(v)));
}

#[test]
fn class_references_roundtrip_through_ordinary_statics_instance_fields_and_class_array() {
    let mut c = app();
    c.field("saved", "Ljava/lang/Class;", 9);
    c.field("held", "Ljava/lang/Class;", 1);
    let saved = c.reference(9, "C", "saved", "Ljava/lang/Class;");
    let held = c.reference(9, "C", "held", "Ljava/lang/Class;");
    let class = c.class("C");
    let array_class = c.class("java/lang/Class");
    let mut code = ldc(class);
    code.extend(instruction(0xb3, saved));
    code.push(0x2a);
    code.extend(instruction(0xb2, saved));
    code.extend(instruction(0xb5, held));
    code.push(4);
    code.extend(instruction(0xbd, array_class));
    code.extend([0x4c, 0x2b, 3, 0x2a]);
    code.extend(instruction(0xb4, held));
    code.extend([0x53, 0x2b, 3, 0x32, 0xb0]);
    c.method("run", "(LC;)Ljava/lang/Class;", 9, &code, 3, 2);
    let mut w = World::new(c);
    let receiver = w.object();
    let r = returned(w.run(
        "run",
        "(LC;)Ljava/lang/Class;",
        &[Value::Reference(receiver)],
    ));
    let field = w.g.field(w.owner, "held", "Ljava/lang/Class;");
    assert_eq!(w.h.get_field(receiver, field).unwrap(), Value::Reference(r));
    assert_eq!(
        w.h.mirror_represented_type(r).unwrap(),
        Some(ReferenceType::Class(w.owner))
    );
    assert_eq!(w.h.class_mirror_count(), 1);
    assert_eq!(w.h.allocated_arrays(), 1);
}
#[test]
fn explicit_nominal_mirror_matches_static_and_instance_implicit_monitor_keys() {
    for instance in [false, true] {
        for short in [false, true] {
            let mut c = app();
            c.method("m", "()V", 0x29, &[0xb1], 0, 0);
            let class = c.class("C");
            let target = c.reference(
                10,
                if instance { "java/lang/Object" } else { "C" },
                if instance { "ordinarySync" } else { "m" },
                "()V",
            );
            let mut code = ldc(class);
            code.extend([0x59, 0x4b, 0xc2]);
            if instance {
                code.push(0x2a);
            }
            let invoke = code.len() as u32;
            code.extend(instruction(if instance { 0xb6 } else { 0xb8 }, target));
            code.extend([0x2a, 0xc3, 0xb1]);
            c.method("run", "()V", 9, &code, 2, 1);
            let mut w = World::build(c, ObjectNative::ValidWithSync, Default::default(), true);
            let m = w.g.method(w.owner, "run", "()V");
            let mut context = w
                .g
                .domain
                .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
                .unwrap();
            let statics = context.allocate_static_state(Default::default()).unwrap();
            let exceptions = context.prepare_exception_profile(w.throwable).unwrap();
            let mut session = InitializationSession::new_with_class_mirrors(
                &mut context,
                &statics,
                &exceptions,
                w.error,
                Default::default(),
                MonitorLimits {
                    max_steps: 4,
                    max_active_monitors: 1,
                    max_active_claims: 2,
                    max_recursion: if short { 1 } else { 2 },
                },
            )
            .unwrap();
            let result = session.run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            );
            if short {
                let e = result.unwrap_err();
                assert_eq!(
                    e.kind,
                    ExecutionErrorKind::Monitor(MonitorError::Limit {
                        resource: MonitorResource::Recursion,
                        limit: 1,
                        required: 2
                    })
                );
                assert_eq!(e.offset, Some(invoke));
            } else {
                assert_eq!(result.unwrap(), ExceptionOutcome::Returned(None));
            }
            drop(session);
            drop(context);
            assert_eq!(w.h.class_mirror_count(), 1);
        }
    }
}
#[test]
fn distinct_class_mirror_does_not_mint_or_lock_the_static_callees_mirror() {
    let mut c = app();
    c.method("m", "()V", 0x29, &[0xb1], 0, 0);
    let other = c.class("Other");
    let target = c.reference(10, "C", "m", "()V");
    let mut code = ldc(other);
    code.extend([0x59, 0x4b, 0xc2]);
    code.extend(instruction(0xb8, target));
    code.extend([0x2a, 0xc3, 0xb1]);
    c.method("run", "()V", 9, &code, 2, 1);
    let mut w = World::with_dependencies(
        c,
        vec![Class::plain("Other")],
        ObjectNative::Valid,
        ClassMirrorLimits {
            max_mirrors: 1,
            max_class_sites: 1,
        },
        true,
    );
    let method = w.g.method(w.owner, "run", "()V");
    let mut context =
        w.g.domain
            .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
            .unwrap();
    let p = context.allocate_static_state(Default::default()).unwrap();
    let e = context.prepare_exception_profile(w.throwable).unwrap();
    let mut session = InitializationSession::new_with_class_mirrors(
        &mut context,
        &p,
        &e,
        w.error,
        Default::default(),
        MonitorLimits {
            max_recursion: 1,
            max_steps: 4,
            max_active_monitors: 2,
            max_active_claims: 2,
        },
    )
    .unwrap();
    assert_eq!(
        session
            .run_static(
                method,
                &[],
                Default::default(),
                Default::default(),
                Default::default()
            )
            .unwrap(),
        ExceptionOutcome::Returned(None)
    );
    drop(session);
    drop(context);
    assert_eq!(w.h.class_mirror_count(), 1);
}
#[test]
fn reserved_class_new_refuses_before_class_initialization_or_alias_publication() {
    let mut c = app();
    let class = c.class("java/lang/Class");
    c.method(
        "run",
        "()V",
        9,
        &[instruction(0xbb, class), vec![0x57, 0xb1]].concat(),
        1,
        0,
    );
    let mut w = World::new(c);
    let method = w.g.method(w.owner, "run", "()V");
    let class = w.class;
    w.with(true, |s, _| {
        let e = s
            .run_static(
                method,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::ConstructionAccess(Box::new(ConstructionAccessError::Instance(
                Box::new(InstanceError::ReservedClassObject { class })
            )))
        );
        assert_eq!(e.offset, Some(0));
        assert_eq!(
            s.status(class).unwrap(),
            InitializationStatus::Uninitialized
        );
        assert_eq!(s.last_construction_steps(), 0);
    });
    assert_eq!(w.h.allocated_instances(), 0);
    assert_eq!(w.h.class_mirror_count(), 0);
}
#[test]
fn opaque_class_state_is_closed_at_direct_host_and_selected_instance_entry_and_raw_fields() {
    for direct in [false, true] {
        let mut c = app();
        let literal = c.class("C");
        let get = c.reference(10, "java/lang/Class", "expose", "()I");
        let mut code = ldc(literal);
        code.extend(instruction(0xb6, get));
        code.push(0xac);
        c.method("run", "()I", 9, &code, 1, 0);
        let mut w = World::new(c);
        let class = w.class;
        if direct {
            let expose = w.g.method(class, "expose", "()I");
            let field = w.g.field(class, "hidden", "I");
            let native = w.g.method(w.g.object, "getClass", "()Ljava/lang/Class;");
            let mut context = w
                .g
                .domain
                .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
                .unwrap();
            let mirror = context
                .class_mirror_for_type(ReferenceType::Class(w.owner))
                .unwrap();
            assert_eq!(
                context.heap().get_field(mirror, field).unwrap_err(),
                InstanceError::OpaqueClassObject { class }
            );
            let e = execute_instance_method_body(
                &mut context,
                expose,
                mirror,
                &[],
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
            assert!(
                matches!(e.kind,ExecutionErrorKind::MethodAccess(ref inner)if matches!(**inner,MethodAccessError::Context(FieldAccessError::Instance(InstanceError::OpaqueClassObject{class: actual}))if actual==class))
            );
            let e = execute_instance_method_body(
                &mut context,
                native,
                mirror,
                &[],
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
            assert!(
                matches!(e.kind,ExecutionErrorKind::UnsupportedMethod(s)if s.contains("native"))
            );
        } else {
            let e = w.run("run", "()I", &[]).unwrap_err();
            assert!(!matches!(
                e.kind,
                ExecutionErrorKind::UnsupportedVmException(_)
            ));
            assert!(e.to_string().contains("opaque"));
        }
    }
}
#[test]
fn mirror_profile_rejection_leaves_permanent_claim_available_and_old_profiles_stay_closed() {
    let mut c = app();
    let literal = c.class("C");
    c.method(
        "run",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::build(c, ObjectNative::Missing, Default::default(), false);
    let method = w.g.method(w.owner, "run", "()Ljava/lang/Class;");
    let mut context =
        w.g.domain
            .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
            .unwrap();
    let p = context.allocate_static_state(Default::default()).unwrap();
    let e = context.prepare_exception_profile(w.throwable).unwrap();
    let failed = InitializationSession::new_with_class_mirrors(
        &mut context,
        &p,
        &e,
        w.error,
        Default::default(),
        Default::default(),
    );
    assert!(matches!(
        failed.err().unwrap().kind,
        ExecutionErrorKind::ClassMirror(_)
    ));
    let mut old = InitializationSession::new_with_monitors(
        &mut context,
        &p,
        &e,
        w.error,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let e = old
        .run_static(
            method,
            &[],
            Default::default(),
            Default::default(),
            Default::default(),
        )
        .unwrap_err();
    assert!(matches!(
        e.kind,
        ExecutionErrorKind::UnsupportedConstant { .. }
    ));
    let mut c = app();
    let literal = c.class("C");
    let target = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
    c.method(
        "literal",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), vec![0xb0]].concat(),
        1,
        0,
    );
    c.method(
        "native",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        9,
        &[vec![0x2a], instruction(0xb6, target), vec![0xb0]].concat(),
        1,
        1,
    );
    let mut w = World::new(c);
    let receiver = w.object();
    let literal = w.g.method(w.owner, "literal", "()Ljava/lang/Class;");
    let native =
        w.g.method(w.owner, "native", "(Ljava/lang/Object;)Ljava/lang/Class;");
    w.with(false,|s,_|{assert!(matches!(s.run_static(literal,&[],Default::default(),Default::default(),Default::default()).unwrap_err().kind,ExecutionErrorKind::UnsupportedConstant{..}));assert!(matches!(s.run_static(native,&[Value::Reference(receiver)],Default::default(),Default::default(),Default::default()).unwrap_err().kind,ExecutionErrorKind::UnsupportedMethod(s)if s.contains("native")));});
    assert_eq!(w.h.class_mirror_count(), 0);
}
#[test]
fn permanent_inaccessible_class_and_array_literal_denials_replay_without_target_heap_state() {
    for array in [false, true] {
        let mut c = app();
        let literal = c.class(if array { "[Lp/Hidden;" } else { "p/Hidden" });
        c.method(
            "run",
            "()Ljava/lang/Class;",
            9,
            &[ldc(literal), vec![0xb0]].concat(),
            1,
            0,
        );
        let mut hidden = Class::plain("p/Hidden");
        hidden.flags = 0x20;
        let mut w = World::with_dependencies(
            c,
            vec![hidden],
            ObjectNative::Valid,
            ClassMirrorLimits {
                max_class_sites: 1,
                ..Default::default()
            },
            true,
        );
        let method = w.g.method(w.owner, "run", "()Ljava/lang/Class;");
        w.with(true, |s, _| {
            for _ in 0..2 {
                let e = s
                    .run_static(
                        method,
                        &[],
                        Default::default(),
                        Default::default(),
                        Default::default(),
                    )
                    .unwrap_err();
                assert_eq!(
                    e.kind,
                    ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::IllegalAccessError)
                );
                assert_eq!(e.offset, Some(0));
            }
        });
        assert_eq!(
            w.g.domain.class_site(w.owner, literal).unwrap(),
            Some(ClassSiteOutcome::IllegalAccess)
        );
        assert_eq!(w.h.class_mirror_count(), 0);
        let mut smaller = w.g.heap(&[w.owner]);
        assert!(smaller.reference_type_universe().is_none());
        let mut context = w
            .g
            .domain
            .member_context_unnamed_modules(&mut smaller, Default::default(), Default::default())
            .unwrap();
        assert_eq!(
            *context
                .resolve_class_mirror_type(w.owner, literal)
                .unwrap_err(),
            ClassMirrorError::IllegalAccess
        );
    }
}

#[test]
fn missing_true_initiation_is_host_repairable_and_never_a_terminal_class_site() {
    let mut c = app();
    let literal = c.class("p/Target");
    c.method(
        "run",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::build(c, ObjectNative::Valid, Default::default(), false);
    let loader = w.g.loader();
    let target = w.g.add(loader, Class::plain("p/Target"));
    w.g.prepare(&[target]);
    let plan = w.g.plan(target);
    w.h.admit_instance_layouts(w.g.domain.registry(), &[&plan])
        .unwrap();
    w.g.domain.enable_class_mirror_sites(1).unwrap();
    w.h.bind_class_mirrors(w.g.domain.registry(), w.class, 1)
        .unwrap();
    {
        let mut context =
            w.g.domain
                .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
                .unwrap();
        let e = context
            .resolve_class_mirror_type(w.owner, literal)
            .unwrap_err();
        assert!(!matches!(*e, ClassMirrorError::IllegalAccess));
    }
    assert_eq!(w.g.domain.class_site(w.owner, literal).unwrap(), None);
    w.g.bind(w.app, "p/Target", target);
    let r = returned(w.run("run", "()Ljava/lang/Class;", &[]));
    assert_eq!(
        w.h.mirror_represented_type(r).unwrap(),
        Some(ReferenceType::Class(target))
    );
}
#[test]
fn identical_nominal_names_and_array_spellings_follow_defining_loader_identity() {
    let mut c = app();
    let class = c.class("p/Target");
    let array = c.class("[Lp/Target;");
    c.method(
        "nominal",
        "()Ljava/lang/Class;",
        9,
        &[ldc(class), vec![0xb0]].concat(),
        1,
        0,
    );
    c.method(
        "array",
        "()Ljava/lang/Class;",
        9,
        &[ldc(array), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::build(c, ObjectNative::Valid, Default::default(), false);
    let a = w.g.loader();
    let b = w.g.loader();
    let ta = w.g.add(a, Class::plain("p/Target"));
    let tb = w.g.add(b, Class::plain("p/Target"));
    w.g.bind(w.app, "p/Target", ta);
    w.g.bind(b, "java/lang/Class", w.class);
    let mut other = Class::plain("D");
    let class = other.class("p/Target");
    let array = other.class("[Lp/Target;");
    other.method(
        "nominal",
        "()Ljava/lang/Class;",
        9,
        &[ldc(class), vec![0xb0]].concat(),
        1,
        0,
    );
    other.method(
        "array",
        "()Ljava/lang/Class;",
        9,
        &[ldc(array), vec![0xb0]].concat(),
        1,
        0,
    );
    let d = w.g.add(b, other);
    w.g.prepare(&[ta, tb, d]);
    let plans = [w.g.plan(ta), w.g.plan(tb), w.g.plan(d)];
    w.h.admit_instance_layouts(w.g.domain.registry(), &plans.iter().collect::<Vec<_>>())
        .unwrap();
    w.g.domain.enable_class_mirror_sites(4).unwrap();
    w.h.bind_class_mirrors(w.g.domain.registry(), w.class, 4)
        .unwrap();
    let methods = [
        w.g.method(w.owner, "nominal", "()Ljava/lang/Class;"),
        w.g.method(d, "nominal", "()Ljava/lang/Class;"),
        w.g.method(w.owner, "array", "()Ljava/lang/Class;"),
        w.g.method(d, "array", "()Ljava/lang/Class;"),
    ];
    let r = w.with(true, |s, _| {
        methods.map(|m| {
            returned(s.run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            ))
        })
    });
    assert_ne!(r[0], r[1]);
    assert_ne!(r[2], r[3]);
    let expected = [
        ReferenceType::Class(ta),
        ReferenceType::Class(tb),
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Class(ta),
            defining_loader: a,
        },
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Class(tb),
            defining_loader: b,
        },
    ];
    for (r, ty) in r.into_iter().zip(expected) {
        assert_eq!(w.h.mirror_represented_type(r).unwrap(), Some(ty));
    }
    assert_eq!(w.h.class_mirror_count(), 4);
}
#[test]
fn domain_resolution_survives_context_drop_but_two_compatible_heaps_issue_distinct_objects() {
    let mut c = app();
    let literal = c.class("C");
    c.method(
        "run",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::new(c);
    let a = returned(w.run("run", "()Ljava/lang/Class;", &[]));
    let u = w.h.reference_type_universe().unwrap();
    let mut next = w.g.heap(&w.g.classes);
    next.bind_reference_types(w.g.domain.registry(), u).unwrap();
    next.bind_class_mirrors(w.g.domain.registry(), w.class, 1)
        .unwrap();
    let first = std::mem::replace(&mut w.h, next);
    let b = returned(w.run("run", "()Ljava/lang/Class;", &[]));
    assert_ne!(a, b);
    assert_eq!(w.g.domain.class_site_count(), 1);
    assert_eq!(first.class_mirror_count(), 1);
    assert_eq!(w.h.class_mirror_count(), 1);
    assert_eq!(
        w.h.mirror_represented_type(a).unwrap_err(),
        HeapError::ForeignHeap
    );
    assert_eq!(
        first.mirror_represented_type(b).unwrap_err(),
        HeapError::ForeignHeap
    );
    let other = World::new(app());
    let before = w.h.class_mirror_count();
    let mut context =
        w.g.domain
            .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
            .unwrap();
    assert!(
        context
            .class_mirror_for_type(ReferenceType::Class(other.owner))
            .is_err()
    );
    drop(context);
    assert_eq!(w.h.class_mirror_count(), before);
}
#[test]
fn rank_255_array_literal_and_runtime_getclass_share_one_canonical_key() {
    let name = format!("{}I", "[".repeat(255));
    let mut c = app();
    let literal = c.class(&name);
    let get = c.reference(10, &name, "getClass", "()Ljava/lang/Class;");
    c.method(
        "literal",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), vec![0xb0]].concat(),
        1,
        0,
    );
    c.method(
        "actual",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        9,
        &[vec![0x2a], instruction(0xb6, get), vec![0xb0]].concat(),
        1,
        1,
    );
    let mut w = World::build(
        c,
        ObjectNative::Valid,
        ClassMirrorLimits {
            max_mirrors: 1,
            max_class_sites: 2,
        },
        true,
    );
    let actual =
        w.g.domain
            .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
            .unwrap()
            .allocate_reference_array(
                ReferenceType::Array {
                    dimensions: 254,
                    element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
                    defining_loader: w.g.boot,
                },
                0,
            )
            .unwrap();
    let literal = w.g.method(w.owner, "literal", "()Ljava/lang/Class;");
    let get =
        w.g.method(w.owner, "actual", "(Ljava/lang/Object;)Ljava/lang/Class;");
    let (a, b) = w.with(true, |s, _| {
        (
            returned(s.run_static(
                literal,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )),
            returned(s.run_static(
                get,
                &[Value::Reference(actual)],
                Default::default(),
                Default::default(),
                Default::default(),
            )),
        )
    });
    assert_eq!(a, b);
    assert_eq!(w.h.class_mirror_count(), 1);
    assert_eq!(
        w.h.mirror_represented_type(a).unwrap(),
        Some(ReferenceType::Array {
            dimensions: 255,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
            defining_loader: w.g.boot
        })
    );
}

#[test]
fn invalid_ldc2_class_and_other_reference_constants_do_not_use_mirror_sites() {
    for wide in [false, true] {
        let mut c = app();
        let index = if wide {
            c.class("C")
        } else {
            let text = c.utf("not implemented String");
            c.cp(8, &text.to_be_bytes())
        };
        let mut code = if wide {
            instruction(0x14, index)
        } else {
            ldc(index)
        };
        code.extend([0x57, 0xb1]);
        c.method("run", "()V", 9, &code, 2, 0);
        let mut w = World::build(
            c,
            ObjectNative::Missing,
            ClassMirrorLimits {
                max_mirrors: 0,
                max_class_sites: 0,
            },
            true,
        );
        let e = w.run("run", "()V", &[]).unwrap_err();
        if wide {
            assert!(matches!(e.kind,ExecutionErrorKind::InvalidConstant{index:i,..}if i==index));
        } else {
            assert_eq!(e.kind, ExecutionErrorKind::UnsupportedConstant { index });
        }
        assert_eq!(w.g.domain.class_site_count(), 0);
        assert_eq!(w.h.class_mirror_count(), 0);
    }
}
#[test]
fn unfinished_getclass_receiver_is_rejected_before_selection_or_mirror_quota() {
    let mut c = app();
    let class = c.class("C");
    let get = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
    c.method(
        "run",
        "()Ljava/lang/Class;",
        9,
        &[instruction(0xbb, class), instruction(0xb6, get), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::build(
        c,
        ObjectNative::Valid,
        ClassMirrorLimits {
            max_mirrors: 0,
            max_class_sites: 0,
        },
        true,
    );
    let e = w.run("run", "()Ljava/lang/Class;", &[]).unwrap_err();
    assert!(matches!(
        e.kind,
        ExecutionErrorKind::ConstructionProtocol { .. }
    ));
    assert_eq!(e.offset, Some(3));
    assert_eq!(w.g.domain.member_site(w.owner, get).unwrap(), None);
    assert_eq!(w.h.class_mirror_count(), 0);
}
#[test]
fn native_mirror_is_retained_after_later_fuel_failure_and_reused_by_next_evaluation() {
    let mut c = app();
    let get = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
    c.method(
        "run",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        9,
        &[vec![0x2a], instruction(0xb6, get), vec![0xb0]].concat(),
        1,
        1,
    );
    let mut w = World::build(
        c,
        ObjectNative::Valid,
        ClassMirrorLimits {
            max_mirrors: 1,
            max_class_sites: 0,
        },
        true,
    );
    let r = w.object();
    let method =
        w.g.method(w.owner, "run", "(Ljava/lang/Object;)Ljava/lang/Class;");
    let result = w.with(true, |s, _| {
        let e = s
            .run_static(
                method,
                &[Value::Reference(r)],
                ExecutionOptions {
                    instruction_budget: 2,
                },
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::InstructionBudgetExhausted { limit: 2 }
        );
        assert_eq!(e.offset, Some(4));
        assert_eq!(s.context().heap().class_mirror_count(), 1);
        returned(s.run_static(
            method,
            &[Value::Reference(r)],
            ExecutionOptions {
                instruction_budget: 3,
            },
            Default::default(),
            Default::default(),
        ))
    });
    assert_eq!(w.h.class_mirror_count(), 1);
    assert_eq!(
        w.h.class_mirror_roots().collect::<Vec<_>>(),
        [result.heap_ref().unwrap()]
    );
}
#[test]
fn two_different_array_mirrors_are_independent_monitor_keys_with_no_fake_classid() {
    let mut c = app();
    let a = c.class("[I");
    let b = c.class("[J");
    let mut code = ldc(a);
    code.extend([0x59, 0x4b, 0xc2]);
    code.extend(ldc(b));
    code.extend([0x59, 0x4c, 0xc2, 0x2a, 0xc3, 0x2b, 0xc3, 0xb1]);
    c.method("run", "()V", 9, &code, 2, 2);
    let mut w = World::new(c);
    let m = w.g.method(w.owner, "run", "()V");
    let mut context =
        w.g.domain
            .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
            .unwrap();
    let p = context.allocate_static_state(Default::default()).unwrap();
    let e = context.prepare_exception_profile(w.throwable).unwrap();
    let mut s = InitializationSession::new_with_class_mirrors(
        &mut context,
        &p,
        &e,
        w.error,
        Default::default(),
        MonitorLimits {
            max_steps: 4,
            max_active_monitors: 2,
            max_active_claims: 2,
            max_recursion: 1,
        },
    )
    .unwrap();
    assert_eq!(
        s.run_static(
            m,
            &[],
            Default::default(),
            Default::default(),
            Default::default()
        )
        .unwrap(),
        ExceptionOutcome::Returned(None)
    );
}

#[test]
fn warm_permanent_failure_pays_exactly_one_probe_and_zero_limit_keeps_cached_outcome() {
    let mut c = app();
    let literal = c.class("p/Hidden");
    c.method(
        "run",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut hidden = Class::plain("p/Hidden");
    hidden.flags = 0x20;
    let mut w = World::with_dependencies(
        c,
        vec![hidden],
        ObjectNative::Valid,
        Default::default(),
        true,
    );
    assert!(w.run("run", "()Ljava/lang/Class;", &[]).is_err());
    // Opening a checked context has its own already-paid prefix. The contract
    // independently fixes exactly one additional probe for this warm denial.
    let opening = {
        let mut smaller = w.g.heap(&[w.owner]);
        let context = w
            .g
            .domain
            .member_context_unnamed_modules(&mut smaller, Default::default(), Default::default())
            .unwrap();
        context.lookup_steps()
    };
    for steps in [0, 1] {
        let mut smaller = w.g.heap(&[w.owner]);
        let mut context =
            w.g.domain
                .member_context_unnamed_modules(
                    &mut smaller,
                    FieldResolutionOptions {
                        max_lookup_steps: opening + steps,
                        ..Default::default()
                    },
                    Default::default(),
                )
                .unwrap();
        let e = *context
            .resolve_class_mirror_type(w.owner, literal)
            .unwrap_err();
        if steps == 0 {
            assert_eq!(
                e,
                ClassMirrorError::Context(Box::new(FieldAccessError::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit: opening,
                    required: opening + 1
                }))
            );
        } else {
            assert_eq!(e, ClassMirrorError::IllegalAccess);
        }
        assert_eq!(context.lookup_steps(), opening + steps);
    }
    assert_eq!(
        w.g.domain.class_site(w.owner, literal).unwrap(),
        Some(ClassSiteOutcome::IllegalAccess)
    );
}
#[test]
fn nested_lookup_and_intrinsic_diagnostics_identify_transition_not_invented_native_frame() {
    for interface in [false, true] {
        let mut c = app();
        let target = c.reference(
            if interface { 11 } else { 10 },
            if interface {
                "java/io/Serializable"
            } else {
                "java/lang/Object"
            },
            "getClass",
            "()Ljava/lang/Class;",
        );
        let mut helper = vec![0x2a];
        helper.extend(instruction(if interface { 0xb9 } else { 0xb6 }, target));
        if interface {
            helper.extend([1, 0]);
        }
        helper.push(0xb0);
        c.method(
            "helper",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            9,
            &helper,
            1,
            1,
        );
        let call = c.reference(10, "C", "helper", "(Ljava/lang/Object;)Ljava/lang/Class;");
        c.method(
            "outer",
            "(Ljava/lang/Object;)Ljava/lang/Class;",
            9,
            &[vec![0x2a], instruction(0xb8, call), vec![0xb0]].concat(),
            1,
            1,
        );
        let mut w = World::new(c);
        let r = if interface {
            w.object()
        } else {
            Reference::null()
        };
        let e = w
            .run(
                "outer",
                "(Ljava/lang/Object;)Ljava/lang/Class;",
                &[Value::Reference(r)],
            )
            .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(if interface {
                VmExceptionKind::IncompatibleClassChangeError
            } else {
                VmExceptionKind::NullPointerException
            })
        );
        assert_eq!(e.method.as_ref().unwrap().name.escaped(), "helper");
        assert_eq!(e.offset, Some(1));
        let site = e.call_site.as_ref().unwrap();
        assert_eq!(site.method.name.escaped(), "helper");
        assert_eq!(site.offset, 1);
    }
}

#[test]
fn shared_class_slot_denial_is_visible_before_array_member_failure_or_null_receiver() {
    let mut c = app();
    let literal = c.class("[Lp/Hidden;");
    let name = c.utf("getClass");
    let desc = c.utf("()Ljava/lang/Class;");
    let nat = c.cp(12, &[name.to_be_bytes(), desc.to_be_bytes()].concat());
    let method = c.cp(10, &[literal.to_be_bytes(), nat.to_be_bytes()].concat());
    c.method(
        "literal",
        "()Ljava/lang/Class;",
        9,
        &[ldc(literal), vec![0xb0]].concat(),
        1,
        0,
    );
    c.method(
        "invoke",
        "(Ljava/lang/Object;)Ljava/lang/Class;",
        9,
        &[vec![0x2a], instruction(0xb6, method), vec![0xb0]].concat(),
        1,
        1,
    );
    let mut hidden = Class::plain("p/Hidden");
    hidden.flags = 0x20;
    let mut w = World::with_dependencies(
        c,
        vec![hidden],
        ObjectNative::Valid,
        ClassMirrorLimits {
            max_class_sites: 1,
            max_mirrors: 0,
        },
        true,
    );
    let literal_method = w.g.method(w.owner, "literal", "()Ljava/lang/Class;");
    let invocation =
        w.g.method(w.owner, "invoke", "(Ljava/lang/Object;)Ljava/lang/Class;");
    w.with(true, |s, _| {
        for (m, args, pc) in [
            (literal_method, vec![], 0),
            (invocation, vec![Value::Reference(Reference::null())], 1),
        ] {
            let e = s
                .run_static(
                    m,
                    &args,
                    Default::default(),
                    Default::default(),
                    Default::default(),
                )
                .unwrap_err();
            assert_eq!(
                e.kind,
                ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::IllegalAccessError)
            );
            assert_eq!(e.offset, Some(pc));
        }
    });
    assert_eq!(
        w.g.domain.class_site(w.owner, literal).unwrap(),
        Some(ClassSiteOutcome::IllegalAccess)
    );
    assert_eq!(w.g.domain.member_site(w.owner, method).unwrap(), None);
    assert_eq!(w.h.class_mirror_count(), 0);
}
#[test]
fn bytecode_class_field_reads_and_writes_keep_opaque_guard_after_ordinary_selection() {
    for write in [false, true] {
        let mut c = app();
        let field = c.reference(9, "java/lang/Class", "hidden", "I");
        let mut code = vec![0x2a];
        if write {
            code.push(0x1b);
        }
        code.extend(instruction(if write { 0xb5 } else { 0xb4 }, field));
        code.push(if write { 0xb1 } else { 0xac });
        let descriptor = if write {
            "(Ljava/lang/Class;I)V"
        } else {
            "(Ljava/lang/Class;)I"
        };
        c.method("run", descriptor, 9, &code, 2, 2);
        let mut w = World::new(c);
        let method = w.g.method(w.owner, "run", descriptor);
        let class = w.class;
        let mirror = {
            let mut context = w
                .g
                .domain
                .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
                .unwrap();
            context
                .class_mirror_for_type(ReferenceType::Class(w.owner))
                .unwrap()
        };
        let mut args = vec![Value::Reference(mirror)];
        if write {
            args.push(Value::Int(17));
        }
        let e = w
            .with(true, |s, _| {
                s.run_static(
                    method,
                    &args,
                    Default::default(),
                    Default::default(),
                    Default::default(),
                )
            })
            .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::FieldAccess(Box::new(FieldAccessError::Instance(
                InstanceError::OpaqueClassObject { class }
            )))
        );
        assert_eq!(e.offset, Some(if write { 2 } else { 1 }));
    }
}

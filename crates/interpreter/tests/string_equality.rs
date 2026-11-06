//! Independent Code, native continuation and bootstrap-policy controls.
#[path = "support/equality_world.rs"]
mod world;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use world::*;
fn program() -> Class {
    let mut c = app();
    compare(&mut c);
    let status = c.reference(10, CLASS, "desiredAssertionStatus", "()Z");
    c.method(
        "status",
        "(Ljava/lang/Class;)Z",
        9,
        &[vec![0x2a], instruction(0xb6, status), vec![0xac]].concat(),
        1,
        1,
    );
    c
}
#[test]
fn dynamic_object_equals_executes_both_coders_and_real_native_registration() {
    for policy in [
        BootstrapAssertionPolicy::Disabled,
        BootstrapAssertionPolicy::Enabled,
    ] {
        let mut w = World::new(program(), policy);
        let mut refs = Vec::new();
        for u in [
            vec![],
            vec![],
            vec![65, 0, 255],
            vec![65, 0, 255],
            vec![65, 0, 254],
            vec![0x20ac, 0xdfff],
            vec![0x20ac, 0xdfff],
            vec![0x20ac, 0xdffe],
        ] {
            refs.push(w.fresh(&u));
        }
        let m = w.method("compare", "(Ljava/lang/Object;Ljava/lang/Object;)Z");
        let class = w.class;
        let latin = w.latin;
        let flag = w.g.field(latin, "$assertionsDisabled", "Z");
        let empty = w.g.field(class, "EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;");
        let serial = w.g.field(
            class,
            "serialPersistentFields",
            "[Ljava/io/ObjectStreamField;",
        );
        let stream = w.stream;
        w.with(|s, statics| {
            for (a,b,expected) in [(0,0,1),(0,1,1),(2,3,1),(2,4,0),(5,6,1),(5,7,0),(2,5,0)] {
                assert_eq!(int(run(s,m,&[Value::Reference(refs[a]),Value::Reference(refs[b])])),expected);
            }
            assert_eq!(s.status(class).unwrap(),InitializationStatus::Completed);
            assert_eq!(s.status(latin).unwrap(),InitializationStatus::Completed);
            assert_eq!(s.status(stream).unwrap(),InitializationStatus::Uninitialized);
            assert_eq!(s.string_equality_usage(),StringEqualityUsage{natives_registered:true,assertion_queries:1});
            assert_eq!(s.context().inspect_static(&statics,flag).unwrap(),Value::Int(i32::from(policy==BootstrapAssertionPolicy::Disabled)));
            let arrays=[empty,serial].map(|f| {
                match s.context().inspect_static(&statics,f).unwrap() {
                    Value::Reference(r)=>r,
                    other=>panic!("{other:?}"),
                }
            });
            assert_ne!(arrays[0],arrays[1]);
            for (r,component) in arrays.into_iter().zip([class,stream]) {
                assert_eq!(s.context().heap().length(r).unwrap(),0);
                let actual=s.context().heap().reference_array_type(r).unwrap();
                assert!(matches!(actual,ReferenceType::Array{dimensions:1,element:ArrayElement::Class(c),..}if c==component));
            }
        });
    }
}
#[test]
fn bootstrap_queries_have_observable_policy_and_cumulative_session_count() {
    for policy in [
        BootstrapAssertionPolicy::Disabled,
        BootstrapAssertionPolicy::Enabled,
    ] {
        let mut w = World::new(program(), policy);
        let object = w.g.object;
        let mirror = w.mirror(object);
        let string_mirror = w.mirror(w.string);
        let m = w.method("status", "(Ljava/lang/Class;)Z");
        let string = w.string;
        w.with(|s, _| {
            for (i, r) in [mirror, string_mirror, mirror].into_iter().enumerate() {
                assert_eq!(
                    int(run(s, m, &[Value::Reference(r)])),
                    i32::from(policy == BootstrapAssertionPolicy::Enabled)
                );
                assert_eq!(s.string_equality_usage().assertion_queries, i as u64 + 1);
            }
            assert_eq!(
                s.status(string).unwrap(),
                InitializationStatus::Uninitialized
            );
            assert!(s.string_equality_usage().natives_registered);
        });
    }
}

#[test]
fn fast_paths_do_not_activate_string_or_helper_and_coder_mismatch_stops_before_helper() {
    for kind in 0..4 {
        let mut w = World::new(program(), BootstrapAssertionPolicy::Disabled);
        let a = w.fresh(&[65]);
        let b = match kind {
            0 => a,
            1 => Reference::null(),
            2 => w.h.allocate_instance(w.owner).unwrap(),
            _ => w.fresh(&[0x20ac]),
        };
        let m = w.method("compare", "(Ljava/lang/Object;Ljava/lang/Object;)Z");
        let string = w.string;
        let latin = w.latin;
        let class = w.class;
        w.with(|s, _| {
            assert_eq!(
                int(run(s, m, &[Value::Reference(a), Value::Reference(b)])),
                i32::from(kind == 0)
            );
            assert_eq!(
                s.status(latin).unwrap(),
                InitializationStatus::Uninitialized
            );
            assert_eq!(
                s.status(class).unwrap(),
                InitializationStatus::Uninitialized
            );
            assert_eq!(
                s.status(string).unwrap(),
                if kind == 3 {
                    InitializationStatus::Completed
                } else {
                    InitializationStatus::Uninitialized
                }
            );
            assert_eq!(s.string_equality_usage(), StringEqualityUsage::default());
        });
    }
}
#[test]
fn application_and_array_mirrors_fail_at_actual_getfield_before_class_registration() {
    let mut w = World::new(program(), BootstrapAssertionPolicy::Enabled);
    let app = w.mirror(w.owner);
    let boot = w.g.boot;
    let array = w
        .context()
        .class_mirror_for_type(ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
            defining_loader: boot,
        })
        .unwrap();
    let m = w.method("status", "(Ljava/lang/Class;)Z");
    let class = w.class;
    w.with(|s, _| {
        for target in [app, array] {
            let error = run(s, m, &[Value::Reference(target)]).unwrap_err();
            assert_eq!(error.offset, Some(1));
            assert!(
                error
                    .method
                    .as_ref()
                    .unwrap()
                    .name
                    .equals_ascii("desiredAssertionStatus")
            );
            let ExecutionErrorKind::FieldAccess(e) = error.kind else {
                panic!()
            };
            let FieldAccessError::StringBody(e) = e.as_ref() else {
                panic!()
            };
            assert!(matches!(
                e.kind,
                StringBodyErrorKind::UnsupportedAssertionTarget { .. }
            ));
            assert_eq!(
                s.status(class).unwrap(),
                InitializationStatus::Uninitialized
            );
            assert_eq!(s.string_equality_usage(), StringEqualityUsage::default());
        }
    });
}
#[test]
fn full_query_code_and_three_handlers_are_prepared_before_the_null_branch() {
    for (handlers, code) in [(2, usize::MAX), (3, 58), (3, 59)] {
        let mut w = World::new(program(), BootstrapAssertionPolicy::Enabled);
        let mirror = w.mirror(w.g.object);
        let m = w.method("status", "(Ljava/lang/Class;)Z");
        let class = w.class;
        w.with(|s, _| {
            let result = s.run_static(
                m,
                &[Value::Reference(mirror)],
                Default::default(),
                CallLimits {
                    max_prepared_code_bytes: code,
                    ..Default::default()
                },
                ExceptionLimits {
                    max_prepared_handlers: handlers,
                    ..Default::default()
                },
            );
            let e = result.unwrap_err();
            if handlers == 2 {
                assert!(matches!(
                    e.kind,
                    ExecutionErrorKind::ExceptionHandlerLimit { .. }
                ));
            } else {
                assert!(matches!(
                    e.kind,
                    ExecutionErrorKind::PreparedCodeLimit { .. }
                ));
            }
            assert_eq!(
                s.status(class).unwrap(),
                InitializationStatus::Uninitialized
            );
            assert_eq!(s.string_equality_usage(), StringEqualityUsage::default());
        });
    }
}
#[test]
fn class_array_cutoff_preserves_registration_and_first_write_without_query_or_retry() {
    let mut w = World::build(
        program(),
        recipes::platform(false),
        recipes::class(),
        HeapOptions {
            max_arrays: 1,
            ..Default::default()
        },
        BootstrapAssertionPolicy::Enabled,
        true,
    );
    let mirror = w.mirror(w.g.object);
    let m = w.method("status", "(Ljava/lang/Class;)Z");
    let class = w.class;
    let first = w.g.field(class, "EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;");
    let second = w.g.field(
        class,
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
    );
    w.with(|s, statics| {
        let e=run(s,m,&[Value::Reference(mirror)]).unwrap_err();
        assert_eq!(e.offset,Some(11));
        assert!(e.method.as_ref().unwrap().name.equals_ascii("<clinit>"));
        assert_eq!(e.initialization_class,Some(class));
        assert_eq!(s.status(class).unwrap(),InitializationStatus::Aborted);
        let prefix=StringEqualityUsage{natives_registered:true,assertion_queries:0};
        assert_eq!(s.string_equality_usage(),prefix);
        let Value::Reference(array)=s.context().inspect_static(&statics,first).unwrap() else {panic!()};
        assert!(!array.is_null());
        assert_eq!(s.context().heap().length(array).unwrap(),0);
        let actual=s.context().heap().reference_array_type(array).unwrap();
        assert!(matches!(actual,ReferenceType::Array{dimensions:1,element:ArrayElement::Class(c),..}if c==class));
        assert_eq!(s.context().inspect_static(&statics,second).unwrap(),Value::Reference(Reference::null()));
        let again=run(s,m,&[Value::Reference(mirror)]).unwrap_err();
        assert_eq!(again.kind,ExecutionErrorKind::InitializationAborted{class});
        assert_eq!(s.string_equality_usage(),prefix);
        assert_eq!(s.context().inspect_static(&statics,first).unwrap(),Value::Reference(array));
        assert_eq!(s.context().inspect_static(&statics,second).unwrap(),Value::Reference(Reference::null()));
        assert_eq!(s.status(class).unwrap(),InitializationStatus::Aborted);
    });
}
#[test]
fn ordinary_object_equals_and_native_roots_never_become_intrinsic_authority() {
    let mut w = World::new(program(), BootstrapAssertionPolicy::Disabled);
    let a = w.h.allocate_instance(w.owner).unwrap();
    let b = w.h.allocate_instance(w.owner).unwrap();
    let m = w.method("compare", "(Ljava/lang/Object;Ljava/lang/Object;)Z");
    let register = w.g.method(w.class, "registerNatives", "()V");
    let query =
        w.g.method(w.class, "desiredAssertionStatus0", "(Ljava/lang/Class;)Z");
    let mirror = w.mirror(w.g.object);
    w.with(|s, _| {
        assert_eq!(
            int(run(s, m, &[Value::Reference(a), Value::Reference(a)])),
            1
        );
        assert_eq!(
            int(run(s, m, &[Value::Reference(a), Value::Reference(b)])),
            0
        );
        for (method, args) in [(register, vec![]), (query, vec![Value::Reference(mirror)])] {
            assert!(matches!(
                run(s, method, &args).unwrap_err().kind,
                ExecutionErrorKind::UnsupportedMethod(_)
            ));
        }
        assert_eq!(s.string_equality_usage(), StringEqualityUsage::default());
    });
}
#[test]
fn wide_helper_class_literal_and_retained_real_loop_fuel_are_not_shortcuts() {
    let mut w = World::build(
        program(),
        recipes::platform(true),
        recipes::class(),
        Default::default(),
        BootstrapAssertionPolicy::Enabled,
        true,
    );
    let a = w.fresh(&vec![120; 200]);
    let b = w.fresh(&vec![120; 200]);
    let m = w.method("compare", "(Ljava/lang/Object;Ljava/lang/Object;)Z");
    let latin = w.latin;
    w.with(|s, _| {
        let e = s
            .run_static(
                m,
                &[Value::Reference(a), Value::Reference(b)],
                ExecutionOptions {
                    instruction_budget: 500,
                },
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::InstructionBudgetExhausted { limit: 500 }
        );
        assert_eq!(e.method.as_ref().unwrap().owner, Some(latin));
        assert!(e.method.as_ref().unwrap().name.equals_ascii("equals"));
        assert_eq!(
            s.string_equality_usage(),
            StringEqualityUsage {
                natives_registered: true,
                assertion_queries: 1
            }
        );
        assert_eq!(s.status(latin).unwrap(), InitializationStatus::Completed);
        assert_eq!(
            int(run(s, m, &[Value::Reference(a), Value::Reference(b)])),
            1
        );
        assert_eq!(s.string_equality_usage().assertion_queries, 1);
    });
}

#[test]
fn native_continuations_add_no_code_frame_slots_monitor_or_fuel_and_keep_precise_cutoffs() {
    for (frames, slots, code, fuel, registration, queries, status, location) in [
        (3, 9, 77, 19, true, 1, InitializationStatus::Completed, None),
        (
            1,
            9,
            77,
            19,
            false,
            0,
            InitializationStatus::Uninitialized,
            Some(("desiredAssertionStatus", None)),
        ),
        (
            2,
            9,
            77,
            19,
            false,
            0,
            InitializationStatus::Aborted,
            Some(("<clinit>", None)),
        ),
        (
            3,
            7,
            77,
            19,
            false,
            0,
            InitializationStatus::Uninitialized,
            Some(("desiredAssertionStatus", None)),
        ),
        (
            3,
            8,
            77,
            19,
            false,
            0,
            InitializationStatus::Aborted,
            Some(("<clinit>", None)),
        ),
        (
            3,
            9,
            58,
            19,
            false,
            0,
            InitializationStatus::Uninitialized,
            Some(("desiredAssertionStatus", None)),
        ),
        (
            3,
            9,
            76,
            19,
            false,
            0,
            InitializationStatus::Uninitialized,
            Some(("<clinit>", None)),
        ),
        (
            3,
            9,
            77,
            9,
            false,
            0,
            InitializationStatus::Aborted,
            Some(("<clinit>", Some(0))),
        ),
        (
            3,
            9,
            77,
            10,
            true,
            0,
            InitializationStatus::Aborted,
            Some(("<clinit>", Some(3))),
        ),
        (
            3,
            9,
            77,
            13,
            true,
            0,
            InitializationStatus::Aborted,
            Some(("<clinit>", Some(10))),
        ),
        (
            3,
            9,
            77,
            16,
            true,
            0,
            InitializationStatus::Aborted,
            Some(("<clinit>", Some(17))),
        ),
        (
            3,
            9,
            77,
            17,
            true,
            1,
            InitializationStatus::Completed,
            Some(("desiredAssertionStatus", Some(13))),
        ),
        (
            3,
            9,
            77,
            18,
            true,
            1,
            InitializationStatus::Completed,
            Some(("status", Some(4))),
        ),
    ] {
        let mut w = World::new(program(), BootstrapAssertionPolicy::Enabled);
        let r = w.mirror(w.g.object);
        let m = w.method("status", "(Ljava/lang/Class;)Z");
        let class = w.class;
        let throwable = w.throwable;
        let error = w.error;
        let mut context = w.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let mut session = InitializationSession::new_with_strings(
            &mut context,
            &statics,
            &exceptions,
            error,
            Default::default(),
            MonitorLimits {
                max_steps: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let result = session.run_static(
            m,
            &[Value::Reference(r)],
            ExecutionOptions {
                instruction_budget: fuel,
            },
            CallLimits {
                max_frames: frames,
                max_frame_slots: slots,
                max_prepared_code_bytes: code,
            },
            ExceptionLimits {
                max_prepared_handlers: 3,
                max_dispatch_steps: 0,
            },
        );
        if let Some((name, pc)) = location {
            let e = result.unwrap_err();
            assert!(
                e.method.as_ref().unwrap().name.equals_ascii(name),
                "{frames}/{slots}/{code}/{fuel}: {e:?}"
            );
            assert_eq!(e.offset, pc, "{frames}/{slots}/{code}/{fuel}");
        } else {
            assert_eq!(int(result), 1);
        }
        assert_eq!(
            session.status(class).unwrap(),
            status,
            "{frames}/{slots}/{code}/{fuel}"
        );
        assert_eq!(
            session.string_equality_usage(),
            StringEqualityUsage {
                natives_registered: registration,
                assertion_queries: queries
            },
            "{frames}/{slots}/{code}/{fuel}"
        );
        if status == InitializationStatus::Aborted {
            // Retry on this permanent session with fresh, higher per-evaluation
            // fuel/frame/slot/Code ceilings. No old exhausted allowance can hide
            // a replay of Class initialization or a new native answer.
            let prefix = session.string_equality_usage();
            let again = run(&mut session, m, &[Value::Reference(r)]).unwrap_err();
            assert_eq!(
                again.kind,
                ExecutionErrorKind::InitializationAborted { class }
            );
            assert_eq!(
                session.status(class).unwrap(),
                InitializationStatus::Aborted
            );
            assert_eq!(session.string_equality_usage(), prefix);
        }
    }
}
#[test]
fn last_byte_difference_requires_real_helper_loop_work() {
    for last in [false, true] {
        let mut w = World::new(program(), BootstrapAssertionPolicy::Disabled);
        let a = w.fresh(&vec![120; 200]);
        let mut data = vec![120; 200];
        data[if last { 199 } else { 0 }] = 121;
        let b = w.fresh(&data);
        let m = w.method("compare", "(Ljava/lang/Object;Ljava/lang/Object;)Z");
        let latin = w.latin;
        w.with(|s, _| {
            let result = s.run_static(
                m,
                &[Value::Reference(a), Value::Reference(b)],
                ExecutionOptions {
                    instruction_budget: 500,
                },
                Default::default(),
                Default::default(),
            );
            if last {
                let e = result.unwrap_err();
                assert_eq!(e.method.as_ref().unwrap().owner, Some(latin));
                assert!(matches!(
                    e.kind,
                    ExecutionErrorKind::InstructionBudgetExhausted { limit: 500 }
                ));
            } else {
                assert_eq!(int(result), 0);
            }
            assert_eq!(s.string_equality_usage().assertion_queries, 1);
        });
    }
}
#[test]
fn resolution_then_stack_errors_precede_native_site_denial_without_query_effect() {
    for kind in 0..5 {
        let mut class = recipes::class();
        let cp = match kind {
            0 => class.reference(9, CLASS, "EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;"),
            1..=3 => class.reference(10, CLASS, "desiredAssertionStatus0", "(Ljava/lang/Class;)Z"),
            _ => class.reference(10, CLASS, "absent", "(Ljava/lang/Class;)Z"),
        };
        let mut body = match kind {
            2 => vec![3],
            3 => vec![1],
            _ => vec![],
        };
        let pc = body.len() as u32;
        body.extend(instruction(0xb8, cp));
        body.extend([0x57, 0xb1]);
        class.method("bad", "()V", 9, &body, 1, 0);
        let mut w = World::build(
            program(),
            recipes::platform(false),
            class,
            Default::default(),
            BootstrapAssertionPolicy::Enabled,
            true,
        );
        let class = w.class;
        let bad = w.g.method(class, "bad", "()V");
        w.with(|s, _| {
            let e = run(s, bad, &[]).unwrap_err();
            assert_eq!(e.offset, Some(pc));
            assert_eq!(e.method.as_ref().unwrap().owner, Some(class));
            assert!(e.method.as_ref().unwrap().name.equals_ascii("bad"));
            match kind {
                0 => assert!(matches!(e.kind, ExecutionErrorKind::MethodAccess(_))),
                1 => assert_eq!(e.kind, ExecutionErrorKind::StackUnderflow),
                2 => assert_eq!(
                    e.kind,
                    ExecutionErrorKind::WrongValueType {
                        expected: ValueKind::Reference,
                        actual: ValueKind::Int
                    }
                ),
                3 => assert!(matches!(e.kind, ExecutionErrorKind::StringBody(_))),
                _ => assert_eq!(
                    e.kind,
                    ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NoSuchMethodError)
                ),
            }
            assert_eq!(
                s.string_equality_usage(),
                StringEqualityUsage {
                    natives_registered: true,
                    assertion_queries: 0
                }
            );
        });
    }
}

#[test]
fn ordinary_override_and_overload_code_is_not_replaced_by_string_or_identity_logic() {
    let mut c = program();
    c.method("equals", "(Ljava/lang/Object;)Z", 1, &[3, 0xac], 1, 2);
    c.method("equals", "(I)Z", 1, &[4, 0xac], 1, 2);
    let mut w = World::new(c, BootstrapAssertionPolicy::Disabled);
    let a = w.h.allocate_instance(w.owner).unwrap();
    let m = w.method("compare", "(Ljava/lang/Object;Ljava/lang/Object;)Z");
    w.with(|s, _| {
        assert_eq!(
            int(run(s, m, &[Value::Reference(a), Value::Reference(a)])),
            0
        );
        assert_eq!(s.string_equality_usage(), StringEqualityUsage::default());
    });
}
#[test]
fn older_bound_session_still_rejects_native_before_active_use() {
    let mut w = World::new(program(), BootstrapAssertionPolicy::Enabled);
    let class = w.class;
    let error = w.error;
    let throwable = w.throwable;
    let method = w.method("status", "(Ljava/lang/Class;)Z");
    let mirror = w.mirror(w.g.object);
    let mut context = w.context();
    let statics = context.allocate_static_state(Default::default()).unwrap();
    let exceptions = context.prepare_exception_profile(throwable).unwrap();
    let mut s = InitializationSession::new_with_class_mirrors(
        &mut context,
        &statics,
        &exceptions,
        error,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let e = run(&mut s, method, &[Value::Reference(mirror)]).unwrap_err();
    assert!(matches!(e.kind, ExecutionErrorKind::UnsupportedMethod(_)));
    assert!(
        e.method
            .as_ref()
            .unwrap()
            .name
            .equals_ascii("desiredAssertionStatus0")
    );
    assert_eq!(e.offset, None);
    assert_eq!(e.call_site.as_ref().unwrap().offset, 10);
    assert_eq!(
        s.status(class).unwrap(),
        InitializationStatus::Uninitialized
    );
    assert_eq!(s.string_equality_usage(), StringEqualityUsage::default());
}

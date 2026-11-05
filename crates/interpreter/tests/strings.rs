//! Exact-unit String issuance, engine profiles and initialization transactions.
#[path = "support/string_world.rs"]
mod world;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use world::*;
fn run(
    s: &mut InitializationSession<'_, '_>,
    m: MethodId,
    args: &[Value],
) -> Result<ExceptionOutcome, ExecutionError> {
    s.run_static(
        m,
        args,
        Default::default(),
        Default::default(),
        Default::default(),
    )
}
fn literal(c: &mut Class, name: &str, units: &[u16]) -> u16 {
    let cp = c.string(units);
    c.method(
        name,
        "()Ljava/lang/String;",
        9,
        &[ldc(cp), vec![0xb0]].concat(),
        1,
        0,
    );
    cp
}
fn ctx(w: &mut World) -> FieldResolutionContext<'_> {
    w.g.domain
        .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
        .unwrap()
}
#[test]
fn exact_units_duplicate_sites_wide_indices_and_no_explicit_string_class() {
    let units = [0, 0x20ac, 0xd800, 0xdc00, 0xdfff];
    let mut c = app();
    let a = literal(&mut c, "a", &units);
    let b = literal(&mut c, "b", &units);
    for i in 0..270 {
        c.utf(&format!("padding{i}"));
    }
    let wide = literal(&mut c, "wide", &units);
    assert!(wide > 255);
    let mut w = World::new(c);
    assert!(!w.g.domain.registry().class(w.owner).unwrap().constant_pool.entries().iter().any(|e|
        matches!(e,crabspot_classfile::Constant::Class{name_index} if w.g.domain.registry().class(w.owner).unwrap().constant_pool.utf8(*name_index).unwrap().equals_ascii("java/lang/String"))));
    let methods = ["a", "b", "wide"].map(|n| w.g.method(w.owner, n, "()Ljava/lang/String;"));
    let references = w.with(true, |s, _| methods.map(|m| returned(run(s, m, &[]))));
    assert_eq!(references, [references[0]; 3]);
    assert_eq!(
        w.h.string_units(references[0]).unwrap(),
        Some(units.as_slice())
    );
    assert_eq!(w.h.instance_class(references[0]).unwrap(), w.string);
    assert_eq!(w.h.children(references[0]).unwrap().count(), 0);
    let usage = w.h.string_usage();
    assert_eq!(usage.objects, 1);
    assert_eq!(usage.literal_sites, 3);
    assert_eq!(usage.interned, 1);
    assert_eq!(usage.retained_units, 5);
    assert_eq!(usage.work_units, 15 + 14 + 14);
    assert_ne!(a, b);
}
#[test]
fn fresh_receiver_first_and_literal_first_use_actual_canonical_identity() {
    for receiver_first in [false, true] {
        let mut c = app();
        let index = literal(&mut c, "lit", &[120]);
        let mut w = World::new(c);
        let receiver = w.fresh(&[120]);
        let equal = w.fresh(&[120]);
        assert_ne!(receiver, equal);
        let owner = w.owner;
        let mut context = ctx(&mut w);
        let canonical = if receiver_first {
            context.intern_string(receiver).unwrap()
        } else {
            context.string_literal(owner, index).unwrap()
        };
        assert_eq!(context.intern_string(equal).unwrap(), canonical);
        assert_eq!(context.string_literal(owner, index).unwrap(), canonical);
        assert_eq!(context.intern_string(canonical).unwrap(), canonical);
        if receiver_first {
            assert_eq!(canonical, receiver)
        } else {
            assert_ne!(canonical, receiver)
        }
        assert_eq!(
            context.heap().string_roots().collect::<Vec<_>>(),
            [canonical.heap_ref().unwrap()]
        );
    }
}
#[test]
fn exact_public_work_formulas_and_warm_site_constant_cost() {
    let mut c = app();
    let a = literal(&mut c, "a", &[1, 2, 3]);
    let b = literal(&mut c, "b", &[1, 2, 3]);
    let mut w = World::new(c);
    let owner = w.owner;
    let mut context = ctx(&mut w);
    let r = context.fresh_string(&[4, 5, 6]).unwrap();
    assert_eq!(context.heap().string_usage().work_units, 4);
    context.intern_string(r).unwrap();
    assert_eq!(context.heap().string_usage().work_units, 10);
    context.intern_string(r).unwrap();
    assert_eq!(context.heap().string_usage().work_units, 11);
    let r2 = context.fresh_string(&[4, 5, 6]).unwrap();
    context.intern_string(r2).unwrap();
    assert_eq!(context.heap().string_usage().work_units, 24);
    context.string_literal(owner, a).unwrap();
    assert_eq!(context.heap().string_usage().work_units, 35);
    context.string_literal(owner, b).unwrap();
    assert_eq!(context.heap().string_usage().work_units, 45);
    context.string_literal(owner, a).unwrap();
    assert_eq!(context.heap().string_usage().work_units, 46);
}
#[test]
fn maximal_constant_thousands_of_warm_loads_do_not_rescan_units() {
    let mut c = app();
    let index = literal(&mut c, "large", &vec![120; 65_535]);
    let mut w = World::new(c);
    let owner = w.owner;
    let mut context = ctx(&mut w);
    let r = context.string_literal(owner, index).unwrap();
    let cold = context.heap().string_usage();
    assert_eq!(cold.work_units, 131_075);
    for _ in 0..4096 {
        assert_eq!(context.string_literal(owner, index).unwrap(), r);
    }
    assert_eq!(
        context.heap().string_usage().work_units,
        cold.work_units + 4096
    );
    assert_eq!(
        context.heap().string_units(r).unwrap().unwrap().len(),
        65_535
    );
}
#[test]
fn literal_stack_space_and_wrong_load_category_precede_pool_effects() {
    for wide2 in [false, true] {
        let mut c = app();
        let index = c.string(&[120]);
        let code = if wide2 {
            instruction(0x14, index)
        } else {
            ldc(index)
        };
        c.method("bad", "()V", 9, &[code, vec![0x57, 0xb1]].concat(), 0, 0);
        let mut w = World::new(c);
        let error = w.run("bad", "()V", &[]).unwrap_err();
        if wide2 {
            assert!(matches!(
                error.kind,
                ExecutionErrorKind::InvalidConstant { .. }
            ));
        } else {
            assert!(matches!(
                error.kind,
                ExecutionErrorKind::StackOverflow { .. }
            ));
        }
        assert_eq!(error.offset, Some(0));
        assert_eq!(w.h.string_usage(), StringUsage::default());
    }
}
#[test]
fn intern_native_returns_in_caller_without_frame_or_code_charge() {
    let mut c = app();
    let m = c.reference(10, "java/lang/String", "intern", "()Ljava/lang/String;");
    c.method(
        "intern",
        "(Ljava/lang/String;)Ljava/lang/String;",
        9,
        &[vec![0x2a], instruction(0xb6, m), vec![0xb0]].concat(),
        1,
        1,
    );
    let mut w = World::new(c);
    let r = w.fresh(&[0, 0xd800]);
    let method =
        w.g.method(w.owner, "intern", "(Ljava/lang/String;)Ljava/lang/String;");
    w.with(true, |s, _| {
        assert_eq!(
            returned(s.run_static(
                method,
                &[Value::Reference(r)],
                ExecutionOptions {
                    instruction_budget: 3
                },
                CallLimits {
                    max_frames: 1,
                    max_frame_slots: 2,
                    max_prepared_code_bytes: 5
                },
                Default::default()
            )),
            r
        );
    });
    assert_eq!(w.h.string_usage().interned, 1);
}
#[test]
fn null_intern_is_vm_failure_before_string_work_with_transition_provenance() {
    let mut c = app();
    let m = c.reference(10, "java/lang/String", "intern", "()Ljava/lang/String;");
    c.method(
        "bad",
        "()V",
        9,
        &[vec![1], instruction(0xb6, m), vec![0x57, 0xb1]].concat(),
        1,
        0,
    );
    let mut w = World::new(c);
    let error = w.run("bad", "()V", &[]).unwrap_err();
    assert_eq!(
        error.kind,
        ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException)
    );
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.call_site.unwrap().offset, 1);
    assert_eq!(w.h.string_usage(), StringUsage::default());
}
#[test]
fn direct_native_and_java_substitution_never_gain_name_only_authority() {
    for native in [InternNative::Valid, InternNative::Java] {
        let mut c = app();
        let cp = c.reference(10, "java/lang/String", "intern", "()Ljava/lang/String;");
        c.method(
            "call",
            "(Ljava/lang/String;)Ljava/lang/String;",
            9,
            &[vec![0x2a], instruction(0xb6, cp), vec![0xb0]].concat(),
            1,
            1,
        );
        let mut w = World::build(c, vec![], native, Default::default(), true);
        let r = w.fresh(&[120]);
        if matches!(native, InternNative::Java) {
            let e = w
                .run(
                    "call",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[Value::Reference(r)],
                )
                .unwrap_err();
            assert!(
                matches!(e.kind,ExecutionErrorKind::MethodAccess(ref e) if matches!(e.as_ref(),MethodAccessError::String(s) if matches!(s.as_ref(),StringError::InvalidBinding(_))))
            );
            assert_eq!(w.h.string_usage().interned, 0);
        } else {
            let method = w.g.method(w.string, "intern", "()Ljava/lang/String;");
            let mut context = ctx(&mut w);
            let e = execute_instance_method_body(
                &mut context,
                method,
                r,
                &[],
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
            assert!(matches!(e.kind, ExecutionErrorKind::UnsupportedMethod(_)));
        }
    }
}
#[test]
fn opaque_string_body_fields_and_allocation_are_guarded() {
    let mut c = app();
    let cp = c.reference(10, "java/lang/String", "expose", "()I");
    c.method(
        "body",
        "(Ljava/lang/String;)I",
        9,
        &[vec![0x2a], instruction(0xb6, cp), vec![0xac]].concat(),
        1,
        1,
    );
    let mut w = World::new(c);
    let r = w.fresh(&[]);
    let e = w
        .run("body", "(Ljava/lang/String;)I", &[Value::Reference(r)])
        .unwrap_err();
    assert!(
        matches!(e.kind, ExecutionErrorKind::MethodAccess(ref e)
        if matches!(e.as_ref(), MethodAccessError::Context(FieldAccessError::Instance(InstanceError::OpaqueStringObject{class})) if *class == w.string)),
        "{e:?}"
    );
    assert!(matches!(
        w.h.allocate_instance(w.string),
        Err(InstanceError::ReservedStringObject { .. })
    ));
    let field = w.g.field(w.string, "hidden", "I");
    assert!(matches!(
        w.h.get_field(r, field),
        Err(InstanceError::OpaqueStringObject { .. })
    ));
}
#[test]
fn legacy_profile_keeps_literals_and_intern_closed_even_on_bound_heap() {
    for intern in [false, true] {
        let mut c = app();
        let code = if intern {
            let cp = c.reference(10, "java/lang/String", "intern", "()Ljava/lang/String;");
            [vec![0x2a], instruction(0xb6, cp), vec![0xb0]].concat()
        } else {
            let cp = c.string(&[]);
            [ldc(cp), vec![0xb0]].concat()
        };
        let desc = if intern {
            "(Ljava/lang/String;)Ljava/lang/String;"
        } else {
            "()Ljava/lang/String;"
        };
        c.method("run", desc, 9, &code, 1, u16::from(intern));
        let mut w = World::new(c);
        let r = w.fresh(&[]);
        let method = w.g.method(w.owner, "run", desc);
        let args = [Value::Reference(r)];
        let e = w
            .with(false, |s, _| {
                run(s, method, if intern { &args } else { &[] })
            })
            .unwrap_err();
        assert!(matches!(
            e.kind,
            ExecutionErrorKind::UnsupportedConstant { .. }
                | ExecutionErrorKind::UnsupportedMethod(_)
        ));
        assert_eq!(w.h.string_usage().interned, 0);
    }
}
#[test]
fn string_services_do_not_initialize_string_but_real_active_use_still_does() {
    let mut c = app();
    let cp = c.string(&[120]);
    let flag = c.reference(9, "java/lang/String", "initialized", "I");
    c.method(
        "literal",
        "()Ljava/lang/String;",
        9,
        &[ldc(cp), vec![0xb0]].concat(),
        1,
        0,
    );
    c.method(
        "active",
        "()I",
        9,
        &[instruction(0xb2, flag), vec![0xac]].concat(),
        1,
        0,
    );
    let mut w = World::new(c);
    let string = w.string;
    let lit = w.g.method(w.owner, "literal", "()Ljava/lang/String;");
    let active = w.g.method(w.owner, "active", "()I");
    w.with(true, |s, _| {
        returned(run(s, lit, &[]));
        assert_eq!(
            s.status(string).unwrap(),
            InitializationStatus::Uninitialized
        );
        assert_eq!(
            run(s, active, &[]).unwrap(),
            ExceptionOutcome::Returned(Some(Value::Int(1)))
        );
        assert_eq!(s.status(string).unwrap(), InitializationStatus::Completed);
    });
}
#[test]
fn constant_values_use_declaration_order_and_atomic_whole_class_write() {
    for cap in [1, 2] {
        let mut c = app();
        let a = c.string(&[97]);
        let b = c.string(&[98]);
        c.constant_field("a", "Ljava/lang/String;", 0x19, a);
        c.constant_field("b", "Ljava/lang/String;", 0x19, b);
        let mut w = World::build(
            c,
            vec![],
            InternNative::Valid,
            StringLimits {
                max_objects: cap,
                ..Default::default()
            },
            true,
        );
        let owner = w.owner;
        let fields = [
            w.g.field(owner, "a", "Ljava/lang/String;"),
            w.g.field(owner, "b", "Ljava/lang/String;"),
        ];
        w.with(true, |s, statics| {
            let result = s.initialize(
                owner,
                Default::default(),
                Default::default(),
                Default::default(),
            );
            if cap == 1 {
                let e = result.unwrap_err();
                assert_eq!(e.initialization_class, Some(owner));
                assert_eq!(s.status(owner).unwrap(), InitializationStatus::Aborted);
                for field in fields {
                    assert_eq!(
                        s.context().inspect_static(&statics, field).unwrap(),
                        Value::Reference(Reference::null())
                    );
                }
            } else {
                assert_eq!(result.unwrap(), InitializationOutcome::Completed);
                let refs = fields.map(|f| match s.context().inspect_static(&statics, f).unwrap() {
                    Value::Reference(r) => r,
                    _ => panic!(),
                });
                assert_ne!(refs[0], refs[1]);
            }
        });
        assert_eq!(w.h.string_usage().objects, cap);
        assert_eq!(w.h.string_usage().interned, cap);
        assert_eq!(w.h.string_usage().literal_sites, cap);
    }
}
#[test]
fn constant_planning_and_old_sessions_have_no_pool_effects() {
    let mut c = app();
    let cp = c.string(&[120]);
    c.constant_field("text", "Ljava/lang/String;", 0x19, cp);
    let mut w = World::new(c);
    let owner = w.owner;
    let e = w
        .with(false, |s, _| {
            s.initialize(
                owner,
                Default::default(),
                Default::default(),
                Default::default(),
            )
        })
        .unwrap_err();
    assert!(
        matches!(e.kind,ExecutionErrorKind::InitializationAccess(ref e) if matches!(e.as_ref(),InitializationAccessError::UnsupportedStringConstant{..}))
    );
    assert_eq!(w.h.string_usage(), StringUsage::default());
}
#[test]
fn zero_exact_one_less_and_max_string_resource_controls_are_transactional() {
    for resource in [
        StringResource::Objects,
        StringResource::RetainedUnits,
        StringResource::Interned,
        StringResource::LiteralSites,
        StringResource::WorkUnits,
    ] {
        for exact in [false, true] {
            let mut c = app();
            let cp = literal(&mut c, "lit", &[120]);
            let mut limits = StringLimits {
                max_objects: u64::MAX,
                max_retained_units: u64::MAX,
                max_interned: u64::MAX,
                max_literal_sites: u64::MAX,
                max_work_units: u64::MAX,
            };
            let value = if exact {
                if resource == StringResource::WorkUnits {
                    7
                } else {
                    1
                }
            } else {
                0
            };
            match resource {
                StringResource::Objects => limits.max_objects = value,
                StringResource::RetainedUnits => limits.max_retained_units = value,
                StringResource::Interned => limits.max_interned = value,
                StringResource::LiteralSites => limits.max_literal_sites = value,
                StringResource::WorkUnits => limits.max_work_units = value,
            }
            let mut w = World::build(c, vec![], InternNative::Valid, limits, true);
            let owner = w.owner;
            let result = ctx(&mut w).string_literal(owner, cp);
            if exact {
                assert!(result.is_ok());
                assert_eq!(w.h.string_usage().objects, 1);
            } else {
                assert!(
                    matches!(result.unwrap_err().as_ref(),StringError::Limit{resource:r,..} if *r==resource)
                );
                let usage = w.h.string_usage();
                assert_eq!(usage.objects, 0);
                assert_eq!(usage.interned, 0);
                assert_eq!(usage.literal_sites, 0);
                assert_eq!(usage.retained_units, 0);
            }
        }
    }
}
#[test]
fn foreign_and_non_string_inputs_stay_typed_host_failures() {
    let mut a = World::new(app());
    let foreign = a.fresh(&[120]);
    let mut b = World::new(app());
    let ordinary = b.object();
    assert!(b.h.string_units(foreign).is_err());
    assert_eq!(b.h.string_units(ordinary).unwrap(), None);
    let mut context = ctx(&mut b);
    assert!(matches!(
        context
            .intern_string(Reference::null())
            .unwrap_err()
            .as_ref(),
        StringError::InvalidReceiver
    ));
    assert!(matches!(
        context.intern_string(ordinary).unwrap_err().as_ref(),
        StringError::InvalidReceiver
    ));
    assert!(context.intern_string(foreign).is_err());
    assert_eq!(context.heap().string_usage(), StringUsage::default());
}

#[test]
fn cross_loader_constant_literal_and_prior_intern_share_exact_object() {
    let units = [0, 0x20ac, 0xd800];
    let mut c = app();
    let cp = literal(&mut c, "literal", &units);
    c.constant_field("value", "Ljava/lang/String;", 0x19, cp);
    let mut other = None;
    let mut other_cp = 0;
    let mut w = World::custom(
        c,
        vec![],
        InternNative::Valid,
        Default::default(),
        true,
        |g, _, string| {
            let loader = g.loader();
            g.bind(loader, "java/lang/String", string);
            let mut c = Class::plain("D");
            other_cp = literal(&mut c, "literal", &units);
            other = Some(g.add(loader, c));
        },
    );
    let r = w.fresh(&units);
    assert_eq!(ctx(&mut w).intern_string(r).unwrap(), r);
    let other = other.unwrap();
    let owner = w.owner;
    let field = w.g.field(owner, "value", "Ljava/lang/String;");
    w.with(true, |s, statics| {
        assert_eq!(
            s.initialize(
                owner,
                Default::default(),
                Default::default(),
                Default::default()
            )
            .unwrap(),
            InitializationOutcome::Completed
        );
        assert_eq!(
            s.context().inspect_static(&statics, field).unwrap(),
            Value::Reference(r)
        );
    });
    assert_eq!(ctx(&mut w).string_literal(other, other_cp).unwrap(), r);
    assert_eq!(w.h.string_usage().objects, 1);
    assert_eq!(w.h.string_usage().literal_sites, 2);
}
#[test]
fn constant_values_are_published_before_superclass_dependencies() {
    let mut parent = Class::plain("Parent");
    parent.field("seen", "Ljava/lang/String;", 9);
    let read = parent.reference(9, "C", "value", "Ljava/lang/String;");
    let store = parent.reference(9, "Parent", "seen", "Ljava/lang/String;");
    parent.method(
        "<clinit>",
        "()V",
        8,
        &[
            instruction(0xb2, read),
            instruction(0xb3, store),
            vec![0xb1],
        ]
        .concat(),
        1,
        0,
    );
    let mut c = Class::new("C", Some("Parent"), 0x21);
    let cp = c.string(&[120]);
    c.constant_field("value", "Ljava/lang/String;", 0x19, cp);
    let mut w = World::build(
        c,
        vec![parent],
        InternNative::Valid,
        Default::default(),
        true,
    );
    let parent =
        w.g.classes
            .iter()
            .copied()
            .find(|id| {
                w.g.domain
                    .registry()
                    .class(*id)
                    .unwrap()
                    .name()
                    .unwrap()
                    .equals_ascii("Parent")
            })
            .unwrap();
    let value = w.g.field(w.owner, "value", "Ljava/lang/String;");
    let seen = w.g.field(parent, "seen", "Ljava/lang/String;");
    let owner = w.owner;
    w.with(true, |s, statics| {
        s.initialize(
            owner,
            Default::default(),
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let value = s.context().inspect_static(&statics, value).unwrap();
        assert_ne!(value, Value::Reference(Reference::null()));
        assert_eq!(s.context().inspect_static(&statics, seen).unwrap(), value);
    });
}
#[test]
fn strings_cast_store_load_getclass_and_native_calls_keep_actual_identity() {
    let mut c = app();
    let cast = c.class("java/lang/String");
    let get = c.reference(10, "java/lang/Object", "getClass", "()Ljava/lang/Class;");
    let intern = c.reference(10, "java/lang/String", "intern", "()Ljava/lang/String;");
    c.method(
        "roundtrip",
        "(Ljava/lang/String;)Ljava/lang/String;",
        9,
        &[
            vec![4],
            instruction(0xbd, cast),
            vec![0x59, 3, 0x2a, 0x53, 3, 0x32],
            instruction(0xc0, cast),
            instruction(0xb6, intern),
            vec![0xb0],
        ]
        .concat(),
        4,
        1,
    );
    c.method(
        "class",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        9,
        &[vec![0x2a], instruction(0xb6, get), vec![0xb0]].concat(),
        1,
        1,
    );
    let mut w = World::new(c);
    let r = w.fresh(&[120]);
    let string = w.string;
    let round = w.g.method(
        w.owner,
        "roundtrip",
        "(Ljava/lang/String;)Ljava/lang/String;",
    );
    let class =
        w.g.method(w.owner, "class", "(Ljava/lang/String;)Ljava/lang/Class;");
    let mirror = w.with(true, |s, _| {
        assert_eq!(returned(run(s, round, &[Value::Reference(r)])), r);
        let mirror = returned(run(s, class, &[Value::Reference(r)]));
        assert_eq!(
            s.status(string).unwrap(),
            InitializationStatus::Uninitialized
        );
        mirror
    });
    assert_eq!(
        w.h.mirror_represented_type(mirror).unwrap(),
        Some(ReferenceType::Class(string))
    );
    assert_ne!(mirror, r);
}
#[test]
fn pooled_reentrancy_and_distinct_fresh_and_mirror_lock_keys() {
    for kind in 0..3 {
        let mut c = app();
        c.method(
            "locks",
            "(Ljava/lang/Object;Ljava/lang/Object;)V",
            9,
            &[0x2a, 0xc2, 0x2b, 0xc2, 0x2b, 0xc3, 0x2a, 0xc3, 0xb1],
            1,
            2,
        );
        let mut w = World::new(c);
        let a = w.fresh(&[120]);
        let b = if kind == 0 {
            ctx(&mut w).intern_string(a).unwrap()
        } else if kind == 1 {
            w.fresh(&[120])
        } else {
            let ty = ReferenceType::Class(w.string);
            ctx(&mut w).class_mirror_for_type(ty).unwrap()
        };
        let method =
            w.g.method(w.owner, "locks", "(Ljava/lang/Object;Ljava/lang/Object;)V");
        let mut context = ctx(&mut w);
        let statics = context.allocate_static_state(Default::default()).unwrap();
        drop(context);
        let limits = MonitorLimits {
            max_active_monitors: 1,
            ..Default::default()
        };
        let throwable = w.throwable;
        let error = w.error;
        let mut context = ctx(&mut w);
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let mut session = InitializationSession::new_with_strings(
            &mut context,
            &statics,
            &exceptions,
            error,
            Default::default(),
            limits,
        )
        .unwrap();
        let result = run(
            &mut session,
            method,
            &[Value::Reference(a), Value::Reference(b)],
        );
        if kind == 0 {
            assert_eq!(result.unwrap(), ExceptionOutcome::Returned(None));
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                ExecutionErrorKind::Monitor(MonitorError::Limit {
                    resource: MonitorResource::ActiveMonitors,
                    required: 2,
                    ..
                })
            ));
        }
    }
}
#[test]
fn failed_string_capability_check_does_not_consume_initialization_claim() {
    let mut w = World::build(
        app(),
        vec![],
        InternNative::Valid,
        Default::default(),
        false,
    );
    let throwable = w.throwable;
    let error = w.error;
    {
        let mut context = ctx(&mut w);
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let result = InitializationSession::new_with_strings(
            &mut context,
            &statics,
            &exceptions,
            error,
            Default::default(),
            Default::default(),
        );
        assert!(
            matches!(result.err().unwrap().kind,ExecutionErrorKind::String(ref e) if matches!(e.as_ref(),StringError::Disabled))
        );
    }
    w.h.bind_strings(w.g.domain.registry(), w.string, Default::default())
        .unwrap();
    w.with(true, |_, _| {});
}
#[test]
fn typed_host_string_failure_has_original_call_pc_and_prior_effects() {
    let mut c = app();
    c.field("mark", "I", 9);
    let mark = c.reference(9, "C", "mark", "I");
    let cp = c.string(&[120]);
    c.method(
        "call",
        "()Ljava/lang/String;",
        9,
        &[vec![4], instruction(0xb3, mark), ldc(cp), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut w = World::build(
        c,
        vec![],
        InternNative::Valid,
        StringLimits {
            max_objects: 0,
            ..Default::default()
        },
        true,
    );
    let method = w.g.method(w.owner, "call", "()Ljava/lang/String;");
    let field = w.g.field(w.owner, "mark", "I");
    w.with(true,|s,statics|{
        let error=run(s,method,&[]).unwrap_err();assert_eq!(error.offset,Some(4));
        assert!(matches!(error.kind,ExecutionErrorKind::String(ref e) if matches!(e.as_ref(),StringError::Limit{resource:StringResource::Objects,..})));
        assert_eq!(s.context().inspect_static(&statics,field).unwrap(),Value::Int(1));
    });
}

#[test]
fn cross_class_body_uses_its_own_literal_pool_and_heap_prefix_is_persistent() {
    let mut c = app();
    let own = literal(&mut c, "own", &[120]);
    let call = c.reference(10, "D", "other", "()Ljava/lang/String;");
    c.method(
        "call",
        "()Ljava/lang/String;",
        9,
        &[instruction(0xb8, call), vec![0xb0]].concat(),
        1,
        0,
    );
    let mut other = Class::plain("D");
    literal(&mut other, "other", &[121]);
    let mut w = World::build(
        c,
        vec![other],
        InternNative::Valid,
        Default::default(),
        true,
    );
    let a = w.g.method(w.owner, "own", "()Ljava/lang/String;");
    let b = w.g.method(w.owner, "call", "()Ljava/lang/String;");
    let (a, b) = w.with(true, |s, _| {
        (returned(run(s, a, &[])), returned(run(s, b, &[])))
    });
    assert_ne!(a, b);
    assert_eq!(w.h.string_units(a).unwrap(), Some(&[120][..]));
    assert_eq!(w.h.string_units(b).unwrap(), Some(&[121][..]));
    let owner = w.owner;
    assert_eq!(ctx(&mut w).string_literal(owner, own).unwrap(), a);
    assert_eq!(w.h.string_usage().literal_sites, 2);
}
#[test]
fn one_less_cold_work_and_native_flags_have_precise_boundaries() {
    for work in [6, 7] {
        let mut c = app();
        let cp = literal(&mut c, "literal", &[120]);
        let mut w = World::build(
            c,
            vec![],
            InternNative::Valid,
            StringLimits {
                max_work_units: work,
                ..Default::default()
            },
            true,
        );
        let owner = w.owner;
        let result = ctx(&mut w).string_literal(owner, cp);
        if work == 6 {
            assert!(matches!(
                result.unwrap_err().as_ref(),
                StringError::Limit {
                    resource: StringResource::WorkUnits,
                    limit: 6,
                    required: 7
                }
            ));
            assert_eq!(w.h.string_usage().objects, 0);
            assert_eq!(w.h.string_usage().literal_sites, 0);
        } else {
            assert!(result.is_ok());
            assert_eq!(w.h.string_usage().work_units, 7);
        }
    }
    for (flags, valid) in [
        (0x101, true),
        (0x111, true),
        (0x121, false),
        (0x109, false),
        (0x102, false),
        (0x104, false),
    ] {
        let mut w = World::build(
            app(),
            vec![],
            InternNative::Flags(flags),
            Default::default(),
            true,
        );
        let method = w.g.method(w.string, "intern", "()Ljava/lang/String;");
        let result = ctx(&mut w).is_string_intern(method);
        if valid {
            assert!(result.unwrap());
        } else {
            assert!(matches!(
                result.unwrap_err().as_ref(),
                StringError::InvalidBinding(_)
            ));
        }
        assert_eq!(w.h.string_usage(), StringUsage::default());
    }
}

//! Independent bytecode tests for the opt-in session; no host exception fabrication.
#[path = "support/throwable_world.rs"]
mod world;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use world::recipes::support::units;
use world::*;
fn reference(outcome: ExceptionOutcome) -> Reference {
    match outcome {
        ExceptionOutcome::Returned(Some(Value::Reference(r))) | ExceptionOutcome::Uncaught(r) => r,
        x => panic!("{x:?}"),
    }
}
fn program() -> Class {
    let mut c = app();
    let ty = c.class(THROWABLE);
    let ctor = c.reference(10, THROWABLE, "<init>", "()V");
    c.method(
        "make",
        "()Ljava/lang/Throwable;",
        9,
        &[
            instruction(0xbb, ty),
            vec![0x59],
            instruction(0xb7, ctor),
            vec![0xb0],
        ]
        .concat(),
        2,
        0,
    );
    let make = c.reference(10, "C", "make", "()Ljava/lang/Throwable;");
    c.method(
        "wrap",
        "()Ljava/lang/Throwable;",
        9,
        &[instruction(0xb8, make), vec![0xb0]].concat(),
        1,
        0,
    );
    let refill = c.reference(10, THROWABLE, "fillInStackTrace", "()Ljava/lang/Throwable;");
    c.method(
        "refill",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        9,
        &[vec![0x2a], instruction(0xb6, refill), vec![0xb0]].concat(),
        1,
        1,
    );
    c.method(
        "rethrow",
        "(Ljava/lang/Throwable;)V",
        9,
        &[0x2a, 0xbf],
        1,
        1,
    );
    for (name, desc, op, load, ret, locals) in [
        ("idiv", "(II)I", 0x6c, vec![0x1a, 0x1b], 0xac, 2),
        ("irem", "(II)I", 0x70, vec![0x1a, 0x1b], 0xac, 2),
        ("ldiv", "(JJ)J", 0x6d, vec![0x1e, 0x20], 0xad, 4),
        ("lrem", "(JJ)J", 0x71, vec![0x1e, 0x20], 0xad, 4),
    ] {
        c.method(name, desc, 9, &[load, vec![op, ret]].concat(), 4, locals);
    }
    let div = c.reference(10, "C", "idiv", "(II)I");
    c.method(
        "caught",
        "()Ljava/lang/Throwable;",
        9,
        &[
            vec![0x10, 19, 3],
            instruction(0xb8, div),
            vec![0x57, 1, 0xb0, 0xb0],
        ]
        .concat(),
        2,
        0,
    );
    let arithmetic = c.class(ARITHMETIC);
    c.handler([0, 6, 9, arithmetic]);
    c
}
#[test]
fn constructor_capture_uses_invocation_pc_rethrow_identity_and_refill_head() {
    let mut w = World::new(program());
    let make = w.method("make", "()Ljava/lang/Throwable;");
    let wrap = w.method("wrap", "()Ljava/lang/Throwable;");
    let refill = w.method("refill", "(Ljava/lang/Throwable;)Ljava/lang/Throwable;");
    let rethrow = w.method("rethrow", "(Ljava/lang/Throwable;)V");
    w.with(|s, _| {
        let r = reference(run(s, wrap, &[]).unwrap());
        assert_eq!(
            s.throwable_trace(r).unwrap().unwrap(),
            &[
                TraceFrame {
                    method: make,
                    pc: 4
                },
                TraceFrame {
                    method: wrap,
                    pc: 0
                }
            ]
        );
        let initial = s.throwable_trace(r).unwrap().unwrap().to_vec();
        assert_eq!(
            run(s, rethrow, &[Value::Reference(r)]).unwrap(),
            ExceptionOutcome::Uncaught(r)
        );
        assert_eq!(s.throwable_trace(r).unwrap().unwrap(), initial);
        assert_eq!(
            reference(run(s, refill, &[Value::Reference(r)]).unwrap()),
            r
        );
        assert_eq!(
            s.throwable_trace(r).unwrap().unwrap(),
            &[TraceFrame {
                method: refill,
                pc: 1
            }]
        );
        assert_eq!(s.vm_exception_usage(), VmExceptionUsage::default());
    });
    assert_eq!(w.h.throwable_trace_usage().backtraces, 2);
}
#[test]
fn all_four_vm_producers_allocate_distinct_real_instances_and_preserve_original_pc() {
    let mut w = World::new(program());
    let methods = [
        w.method("idiv", "(II)I"),
        w.method("irem", "(II)I"),
        w.method("ldiv", "(JJ)J"),
        w.method("lrem", "(JJ)J"),
    ];
    let arithmetic = w.arithmetic;
    let message =
        w.g.field(w.throwable, "detailMessage", "Ljava/lang/String;");
    w.with(|s, _| {
        let mut values = Vec::new();
        for (index, m) in methods.into_iter().enumerate() {
            for _ in 0..2 {
                let args = if index < 2 {
                    [Value::Int(17), Value::Int(0)]
                } else {
                    [Value::Long(17), Value::Long(0)]
                };
                let result = run(s, m, &args).unwrap();
                assert!(matches!(result, ExceptionOutcome::Uncaught(_)));
                let r = reference(result);
                assert!(!values.contains(&r));
                values.push(r);
                assert_eq!(s.context().heap().instance_class(r).unwrap(), arithmetic);
                assert_eq!(
                    s.throwable_trace(r).unwrap().unwrap(),
                    &[TraceFrame { method: m, pc: 2 }]
                );
                let Value::Reference(text) = s.context().heap().get_field(r, message).unwrap()
                else {
                    panic!()
                };
                assert_eq!(
                    s.context().heap().string_units(text).unwrap().unwrap(),
                    units("/ by zero")
                );
            }
        }
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 8,
                produced: 8
            }
        );
    });
}
#[test]
fn caught_vm_exception_uses_real_callee_and_caller_invocation_sites() {
    let mut w = World::new(program());
    let m = w.method("caught", "()Ljava/lang/Throwable;");
    let div = w.method("idiv", "(II)I");
    w.with(|s, _| {
        let r = reference(run(s, m, &[]).unwrap());
        assert_eq!(
            s.throwable_trace(r).unwrap().unwrap(),
            &[
                TraceFrame { method: div, pc: 2 },
                TraceFrame { method: m, pc: 3 }
            ]
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
    });
}
#[test]
fn explicit_legacy_session_retains_unsupported_native_and_vm_paths() {
    for native in [false, true] {
        let mut w = World::build(
            program(),
            Config {
                enabled: false,
                ..Default::default()
            },
        );
        let method = if native {
            w.method("make", "()Ljava/lang/Throwable;")
        } else {
            w.method("idiv", "(II)I")
        };
        w.with(|s, _| {
            let args = if native {
                vec![]
            } else {
                vec![Value::Int(1), Value::Int(0)]
            };
            let e = run(s, method, &args).unwrap_err();
            if native {
                assert!(matches!(e.kind, ExecutionErrorKind::UnsupportedMethod(_)));
            } else {
                assert_eq!(
                    e.kind,
                    ExecutionErrorKind::UnsupportedVmException(
                        VmExceptionKind::ArithmeticException
                    )
                );
            }
            assert!(e.vm_exception.is_none());
            assert_eq!(s.vm_exception_usage(), VmExceptionUsage::default());
        });
    }
}
#[test]
fn materialization_admission_is_per_evaluation_but_usage_is_cumulative() {
    let mut w = World::build(
        program(),
        Config {
            vm_limits: VmExceptionLimits {
                max_materializations: 1,
            },
            ..Default::default()
        },
    );
    let div = w.method("idiv", "(II)I");
    w.with(|s, _| {
        for count in 1..=3 {
            assert!(matches!(
                run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap(),
                ExceptionOutcome::Uncaught(_)
            ));
            assert_eq!(
                s.vm_exception_usage(),
                VmExceptionUsage {
                    started: count,
                    produced: count
                }
            );
        }
    });
    let mut w = World::build(
        program(),
        Config {
            vm_limits: VmExceptionLimits {
                max_materializations: 0,
            },
            ..Default::default()
        },
    );
    let div = w.method("idiv", "(II)I");
    let arithmetic = w.arithmetic;
    w.with(|s, _| {
        let e = run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::VmExceptionLimit {
                limit: 0,
                required: 1
            }
        );
        assert_eq!(
            e.vm_exception.unwrap().phase,
            VmMaterializationPhase::Preflight
        );
        assert_eq!(s.vm_exception_usage(), Default::default());
        assert_eq!(
            s.status(arithmetic).unwrap(),
            InitializationStatus::Uninitialized
        );
    });
}
#[test]
fn snapshot_quota_failure_has_original_fault_and_preserves_started_without_initializing() {
    let mut w = World::build(
        program(),
        Config {
            traces: TraceLimits {
                max_snapshots: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let m = w.method("idiv", "(II)I");
    let arithmetic = w.arithmetic;
    w.with(|s,_|{let e=run(s,m,&[Value::Int(1),Value::Int(0)]).unwrap_err();assert_eq!(e.offset,Some(2));assert_eq!(e.method.as_ref().unwrap().index,m.index());assert_eq!(e.method.as_ref().unwrap().owner,Some(m.owner()));assert_eq!(e.vm_exception.unwrap(),Box::new(VmExceptionContext{origin:VmFaultOrigin{method:m,pc:2,kind:VmExceptionKind::ArithmeticException},phase:VmMaterializationPhase::Snapshot}));assert!(matches!(e.kind,ExecutionErrorKind::ThrowableAccess(ref x) if matches!(x.kind,ThrowableErrorKind::Limit{resource:ThrowableResource::Snapshots,..})));assert_eq!(s.vm_exception_usage(),VmExceptionUsage{started:1,produced:0});assert_eq!(s.status(arithmetic).unwrap(),InitializationStatus::Uninitialized);});
}
#[test]
fn user_capture_failure_reports_actual_native_invoke_without_vm_context() {
    let mut w = World::build(
        program(),
        Config {
            traces: TraceLimits {
                max_work_units: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let m = w.method("make", "()Ljava/lang/Throwable;");
    let wrapper =
        w.g.method(w.throwable, "fillInStackTrace", "()Ljava/lang/Throwable;");
    w.with(|s, _| {
        let e = run(s, m, &[]).unwrap_err();
        assert_eq!(e.method.as_ref().unwrap().owner, Some(wrapper.owner()));
        assert_eq!(e.method.as_ref().unwrap().index, wrapper.index());
        assert_eq!(e.offset, Some(2));
        assert!(e.vm_exception.is_none());
    });
}
#[test]
fn nested_vm_producer_in_real_constructor_is_bounded_and_does_not_claim_production() {
    // A separate valid String constructor executes division before any chain.
    // Replace the default class completely to select the intended declaration.
    let mut arithmetic = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
    arithmetic.method("<init>", "()V", 1, &[0x03, 0x03, 0x6c, 0x57, 0xb1], 2, 1);
    arithmetic.method(
        "<init>",
        "(Ljava/lang/String;)V",
        1,
        &[0x03, 0x03, 0x6c, 0x57, 0xb1],
        2,
        2,
    );
    let mut w = World::build(
        program(),
        Config {
            arithmetic_class: arithmetic,
            ..Default::default()
        },
    );
    let m = w.method("idiv", "(II)I");
    let ctor = w.g.method(w.arithmetic, "<init>", "(Ljava/lang/String;)V");
    w.with(|s, _| {
        let e = run(s, m, &[Value::Int(1), Value::Int(0)]).unwrap_err();
        let ExecutionErrorKind::NestedVmMaterialization(nested) = e.kind else {
            panic!("{e:?}")
        };
        assert_eq!(
            nested.original,
            VmFaultOrigin {
                method: m,
                pc: 2,
                kind: VmExceptionKind::ArithmeticException
            }
        );
        assert_eq!(
            nested.secondary,
            VmFaultOrigin {
                method: ctor,
                pc: 2,
                kind: VmExceptionKind::ArithmeticException
            }
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 0
            }
        );
    });
}

#[test]
fn exact_instruction_frame_slot_code_and_trace_costs_have_one_less_controls() {
    // idiv's three instructions + three four-instruction ancestor constructors +
    // Throwable's nine, Object's return, and wrapper's four =29; native has no frame.
    for (fuel, frames, slots, code, ok) in [
        (29, 6, 25, 44, true),
        (28, 6, 25, 44, false),
        (29, 5, 25, 44, false),
        (29, 6, 24, 44, false),
        (29, 6, 25, 43, false),
    ] {
        let mut w = World::new(program());
        let m = w.method("idiv", "(II)I");
        w.with(|s, _| {
            let result = s.run_static(
                m,
                &[Value::Int(1), Value::Int(0)],
                ExecutionOptions {
                    instruction_budget: fuel,
                },
                CallLimits {
                    max_frames: frames,
                    max_frame_slots: slots,
                    max_prepared_code_bytes: code,
                },
                Default::default(),
            );
            if ok {
                assert!(matches!(result.unwrap(), ExceptionOutcome::Uncaught(_)));
            } else {
                let e = result.unwrap_err();
                assert!(
                    matches!(
                        e.kind,
                        ExecutionErrorKind::InstructionBudgetExhausted { .. }
                            | ExecutionErrorKind::FrameLimit { .. }
                            | ExecutionErrorKind::FrameSlotLimit { .. }
                            | ExecutionErrorKind::PreparedCodeLimit { .. }
                    ),
                    "{e:?}"
                );
            }
        });
    }
}
#[test]
fn exact_vm_trace_cost_and_zero_maximum_controls() {
    for limit in [0, 3, 4, u64::MAX] {
        let mut w = World::build(
            program(),
            Config {
                traces: TraceLimits {
                    max_work_units: limit,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let m = w.method("idiv", "(II)I");
        w.with(|s, _| {
            let result = run(s, m, &[Value::Int(1), Value::Int(0)]);
            assert_eq!(result.is_ok(), limit >= 4);
            if limit == 3 {
                let e = result.unwrap_err();
                assert_eq!(e.offset, Some(2));
                assert_eq!(
                    e.method.as_ref().unwrap().name.escaped(),
                    "fillInStackTrace"
                );
                let vm = e.vm_exception.unwrap();
                assert_eq!(vm.origin.method, m);
                assert_eq!(vm.origin.pc, 2);
                assert_eq!(vm.phase, VmMaterializationPhase::Constructor);
            }
        });
        let u = w.h.throwable_trace_usage();
        if limit >= 4 {
            assert_eq!(
                (u.snapshots, u.frames, u.backtraces, u.work_units),
                (1, 1, 1, 4)
            );
        } else {
            assert_eq!(u.backtraces, 0);
        }
    }
}
#[test]
fn refill_quota_failure_preserves_pair_and_old_token_remains_a_strong_child() {
    let mut w = World::build(
        program(),
        Config {
            traces: TraceLimits {
                max_snapshots: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let make = w.method("make", "()Ljava/lang/Throwable;");
    let refill = w.method("refill", "(Ljava/lang/Throwable;)Ljava/lang/Throwable;");
    let backtrace = w.g.field(w.throwable, "backtrace", "Ljava/lang/Object;");
    let depth = w.g.field(w.throwable, "depth", "I");
    w.with(|s, _| {
        let r = reference(run(s, make, &[]).unwrap());
        let before = s.throwable_trace(r).unwrap().unwrap().to_vec();
        let token = s.context().heap().get_field(r, backtrace).unwrap();
        let previous_depth = s.context().heap().get_field(r, depth).unwrap();
        let e = run(s, refill, &[Value::Reference(r)]).unwrap_err();
        assert!(matches!(e.kind, ExecutionErrorKind::ThrowableAccess(_)));
        assert_eq!(s.throwable_trace(r).unwrap().unwrap(), before);
        assert_eq!(s.context().heap().get_field(r, backtrace).unwrap(), token);
        assert_eq!(
            s.context().heap().get_field(r, depth).unwrap(),
            previous_depth
        );
        let Value::Reference(token) = token else {
            panic!()
        };
        assert!(
            s.context()
                .heap()
                .children(token)
                .unwrap()
                .any(|edge| Some(edge) == r.heap_ref())
        );
    });
    assert_eq!(w.h.throwable_trace_usage().backtraces, 1);
}
#[test]
fn exact_snapshot_cell_and_per_snapshot_boundaries() {
    for (frames, per, ok) in [
        (1, 1, true),
        (0, 1, false),
        (1, 0, false),
        (u64::MAX, u64::MAX, true),
    ] {
        let mut w = World::build(
            program(),
            Config {
                traces: TraceLimits {
                    max_frames: frames,
                    max_frames_per_snapshot: per,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let m = w.method("idiv", "(II)I");
        w.with(|s, _| {
            assert_eq!(run(s, m, &[Value::Int(1), Value::Int(0)]).is_ok(), ok);
        });
    }
}
#[test]
fn same_frame_handlers_preserve_all_four_fault_pcs_and_skip_normal_successors() {
    let mut c = program();
    for (name, load, opcode, maxlocals) in [
        ("i", vec![4, 3], 0x6c, 0),
        ("r", vec![4, 3], 0x70, 0),
        ("l", vec![0x0a, 9], 0x6d, 0),
        ("lr", vec![0x0a, 9], 0x71, 0),
    ] {
        let ret = if opcode == 0x6d || opcode == 0x71 {
            0x58
        } else {
            0x57
        };
        c.method(
            name,
            "()Ljava/lang/Throwable;",
            9,
            &[load, vec![opcode, ret, 1, 0xb0, 0xb0]].concat(),
            4,
            maxlocals,
        );
        let ty = c.class(ARITHMETIC);
        c.handler([0, 3, 6, ty]);
    }
    let mut w = World::new(c);
    let methods = ["i", "r", "l", "lr"].map(|n| w.method(n, "()Ljava/lang/Throwable;"));
    w.with(|s, _| {
        for m in methods {
            let r = reference(run(s, m, &[]).unwrap());
            assert_eq!(
                s.throwable_trace(r).unwrap().unwrap(),
                &[TraceFrame { method: m, pc: 2 }]
            );
        }
    });
}
#[test]
fn immutable_entry_receiver_controls_name_prefix_even_after_local_zero_rewrite() {
    let mut child = exception("Child", THROWABLE);
    let refill = child.reference(10, THROWABLE, "fillInStackTrace", "()Ljava/lang/Throwable;");
    // Keep original this in local2, replace local0, then delegate on the original.
    child.method(
        "fillInStackTrace",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        1,
        &[
            vec![0x2a, 0x4d, 0x2b, 0x4b, 0x2c],
            instruction(0xb7, refill),
            vec![0xb0],
        ]
        .concat(),
        1,
        3,
    );
    child.method(
        "other",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        1,
        &[vec![0x2b], instruction(0xb6, refill), vec![0xb0]].concat(),
        1,
        2,
    );
    let mut c = program();
    let call = c.reference(
        10,
        "Child",
        "fillInStackTrace",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
    );
    c.method(
        "changed",
        "(LChild;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        9,
        &[vec![0x2a, 0x2b], instruction(0xb6, call), vec![0xb0]].concat(),
        2,
        2,
    );
    let mut w = World::build(
        c,
        Config {
            extra: vec![child],
            ..Default::default()
        },
    );
    let id =
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
                    .equals_ascii("Child")
            })
            .unwrap();
    let a = w.context().heap_mut().allocate_instance(id).unwrap();
    let throwable = w.throwable;
    let b = w.context().heap_mut().allocate_instance(throwable).unwrap();
    let m = w.method(
        "changed",
        "(LChild;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
    );
    w.with(|s, _| {
        assert_eq!(
            reference(run(s, m, &[Value::Reference(a), Value::Reference(b)]).unwrap()),
            a
        );
        assert_eq!(
            s.throwable_trace(a).unwrap().unwrap(),
            &[TraceFrame { method: m, pc: 2 }]
        );
    });
}
#[test]
fn same_named_prefix_with_different_receiver_is_retained() {
    let mut child = exception("Child", THROWABLE);
    let refill = child.reference(10, THROWABLE, "fillInStackTrace", "()Ljava/lang/Throwable;");
    child.method(
        "fillInStackTrace",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        1,
        &[vec![0x2b], instruction(0xb6, refill), vec![0xb0]].concat(),
        1,
        2,
    );
    let mut c = program();
    let call = c.reference(
        10,
        "Child",
        "fillInStackTrace",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
    );
    c.method(
        "different",
        "(LChild;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        9,
        &[vec![0x2a, 0x2b], instruction(0xb6, call), vec![0xb0]].concat(),
        2,
        2,
    );
    let mut w = World::build(
        c,
        Config {
            extra: vec![child],
            ..Default::default()
        },
    );
    let id =
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
                    .equals_ascii("Child")
            })
            .unwrap();
    let a = w.context().heap_mut().allocate_instance(id).unwrap();
    let throwable = w.throwable;
    let b = w.context().heap_mut().allocate_instance(throwable).unwrap();
    let m = w.method(
        "different",
        "(LChild;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
    );
    let helper = w.g.method(
        id,
        "fillInStackTrace",
        "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
    );
    w.with(|s, _| {
        assert_eq!(
            reference(run(s, m, &[Value::Reference(a), Value::Reference(b)]).unwrap()),
            b
        );
        assert_eq!(
            s.throwable_trace(b).unwrap().unwrap(),
            &[
                TraceFrame {
                    method: helper,
                    pc: 1
                },
                TraceFrame { method: m, pc: 2 }
            ]
        );
    });
}

fn replacement_program() -> Class {
    let mut c = program();
    c.field("replacement", "Ljava/lang/Throwable;", 9);
    c.field("effect", "I", 9);
    let saved = c.reference(9, "C", "replacement", "Ljava/lang/Throwable;");
    c.method(
        "save",
        "(Ljava/lang/Throwable;)V",
        9,
        &[vec![0x2a], instruction(0xb3, saved), vec![0xb1]].concat(),
        1,
        1,
    );
    c
}
fn throwing_arithmetic(initializer: bool) -> Class {
    let mut c = exception(ARITHMETIC, "java/lang/RuntimeException");
    let effect = c.reference(9, "C", "effect", "I");
    let saved = c.reference(9, "C", "replacement", "Ljava/lang/Throwable;");
    let code = [
        vec![0x10, 7],
        instruction(0xb3, effect),
        instruction(0xb2, saved),
        vec![0xbf],
    ]
    .concat();
    if initializer {
        c.method("<clinit>", "()V", 8, &code, 1, 0);
    } else {
        // Independent construction recipe, no body whitelist: String ctor escapes before chaining.
        let mut own = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
        let effect = own.reference(9, "C", "effect", "I");
        let saved = own.reference(9, "C", "replacement", "Ljava/lang/Throwable;");
        own.method("<init>", "()V", 1, &[0xb1], 0, 1);
        own.method(
            "<init>",
            "(Ljava/lang/String;)V",
            1,
            &[
                vec![0x10, 7],
                instruction(0xb3, effect),
                instruction(0xb2, saved),
                vec![0xbf],
            ]
            .concat(),
            1,
            2,
        );
        c = own;
    }
    c
}
#[test]
fn escaping_supplied_constructor_throw_replaces_fault_and_preserves_prior_effects() {
    let mut w = World::build(
        replacement_program(),
        Config {
            arithmetic_class: throwing_arithmetic(false),
            ..Default::default()
        },
    );
    let throwable = w.throwable;
    let replacement = w.context().heap_mut().allocate_instance(throwable).unwrap();
    let save = w.method("save", "(Ljava/lang/Throwable;)V");
    let div = w.method("idiv", "(II)I");
    let effect = w.g.field(w.owner, "effect", "I");
    w.with(|s, statics| {
        run(s, save, &[Value::Reference(replacement)]).unwrap();
        assert_eq!(
            run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap(),
            ExceptionOutcome::Uncaught(replacement)
        );
        assert_eq!(
            s.context().inspect_static(&statics, effect).unwrap(),
            Value::Int(7)
        );
        assert_eq!(s.throwable_trace(replacement).unwrap(), None);
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 0
            }
        );
    });
}
#[test]
fn initializer_error_replacement_is_classified_before_original_fault_dispatch() {
    let mut w = World::build(
        replacement_program(),
        Config {
            arithmetic_class: throwing_arithmetic(true),
            ..Default::default()
        },
    );
    let error = w.error;
    let replacement = w.context().heap_mut().allocate_instance(error).unwrap();
    let arithmetic = w.arithmetic;
    let save = w.method("save", "(Ljava/lang/Throwable;)V");
    let div = w.method("idiv", "(II)I");
    let effect = w.g.field(w.owner, "effect", "I");
    w.with(|s, statics| {
        run(s, save, &[Value::Reference(replacement)]).unwrap();
        assert_eq!(
            run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap(),
            ExceptionOutcome::Uncaught(replacement)
        );
        assert_eq!(
            s.status(arithmetic).unwrap(),
            InitializationStatus::Erroneous { cause: replacement }
        );
        assert_eq!(
            s.retained_roots().collect::<Vec<_>>(),
            vec![replacement.heap_ref().unwrap()]
        );
        assert_eq!(
            s.context().inspect_static(&statics, effect).unwrap(),
            Value::Int(7)
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 0
            }
        );
    });
}
#[test]
fn non_error_initializer_throw_remains_host_wrapping_boundary_and_aborts_without_retry() {
    let mut w = World::build(
        replacement_program(),
        Config {
            arithmetic_class: throwing_arithmetic(true),
            ..Default::default()
        },
    );
    let throwable = w.throwable;
    let replacement = w.context().heap_mut().allocate_instance(throwable).unwrap();
    let arithmetic = w.arithmetic;
    let save = w.method("save", "(Ljava/lang/Throwable;)V");
    let div = w.method("idiv", "(II)I");
    w.with(|s, _| {
        run(s, save, &[Value::Reference(replacement)]).unwrap();
        let e = run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedInitializerWrapping {
                class: arithmetic,
                thrown: replacement
            }
        );
        assert_eq!(e.initialization_class, Some(arithmetic));
        assert_eq!(
            e.vm_exception.unwrap().phase,
            VmMaterializationPhase::Initialization
        );
        assert_eq!(s.status(arithmetic).unwrap(), InitializationStatus::Aborted);
        let e = run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::InitializationAborted { class: arithmetic }
        );
        assert_eq!(e.offset, Some(2));
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 2,
                produced: 0
            }
        );
    });
}
#[test]
fn supplied_throw_caught_inside_constructor_allows_normal_capture_completion() {
    let mut own = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
    let saved = own.reference(9, "C", "replacement", "Ljava/lang/Throwable;");
    let base = own.reference(
        10,
        "java/lang/RuntimeException",
        "<init>",
        "(Ljava/lang/String;)V",
    );
    own.method("<init>", "()V", 1, &[0xb1], 0, 1);
    own.method(
        "<init>",
        "(Ljava/lang/String;)V",
        1,
        &[
            instruction(0xb2, saved),
            vec![0xbf, 0x57, 0x2a, 0x2b],
            instruction(0xb7, base),
            vec![0xb1],
        ]
        .concat(),
        2,
        2,
    );
    own.handler([0, 4, 4, 0]);
    let mut w = World::build(
        replacement_program(),
        Config {
            arithmetic_class: own,
            ..Default::default()
        },
    );
    let throwable = w.throwable;
    let replacement = w.context().heap_mut().allocate_instance(throwable).unwrap();
    let save = w.method("save", "(Ljava/lang/Throwable;)V");
    let div = w.method("idiv", "(II)I");
    w.with(|s, _| {
        run(s, save, &[Value::Reference(replacement)]).unwrap();
        let r = reference(run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap());
        assert_ne!(r, replacement);
        assert_eq!(
            s.throwable_trace(r).unwrap().unwrap(),
            &[TraceFrame { method: div, pc: 2 }]
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
    });
}
#[test]
fn later_constructor_refill_replaces_original_snapshot_without_failing_completion() {
    let mut own = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
    let base = own.reference(
        10,
        "java/lang/RuntimeException",
        "<init>",
        "(Ljava/lang/String;)V",
    );
    let refill = own.reference(10, THROWABLE, "fillInStackTrace", "()Ljava/lang/Throwable;");
    own.method("<init>", "()V", 1, &[0xb1], 0, 1);
    own.method(
        "<init>",
        "(Ljava/lang/String;)V",
        1,
        &[
            vec![0x2a, 0x2b],
            instruction(0xb7, base),
            vec![0x2a],
            instruction(0xb6, refill),
            vec![0x57, 0xb1],
        ]
        .concat(),
        2,
        2,
    );
    let mut w = World::build(
        program(),
        Config {
            arithmetic_class: own,
            ..Default::default()
        },
    );
    let div = w.method("idiv", "(II)I");
    w.with(|s, _| {
        let r = reference(run(s, div, &[Value::Int(1), Value::Int(0)]).unwrap());
        assert_eq!(
            s.throwable_trace(r).unwrap().unwrap(),
            &[TraceFrame { method: div, pc: 2 }]
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
    });
    assert_eq!(w.h.throwable_trace_usage().backtraces, 2);
}
#[test]
fn nested_other_vm_kind_and_arbitrary_host_error_stay_distinct() {
    for null in [true, false] {
        let mut own = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
        own.method("<init>", "()V", 1, &[0xb1], 0, 1);
        own.method(
            "<init>",
            "(Ljava/lang/String;)V",
            1,
            if null { &[1, 0xbf] } else { &[0x60, 0xb1] },
            2,
            2,
        );
        let mut w = World::build(
            program(),
            Config {
                arithmetic_class: own,
                ..Default::default()
            },
        );
        let m = w.method("idiv", "(II)I");
        w.with(|s,_|{let e=run(s,m,&[Value::Int(1),Value::Int(0)]).unwrap_err();if null{assert!(matches!(e.kind,ExecutionErrorKind::NestedVmMaterialization(ref n) if n.secondary.kind==VmExceptionKind::NullPointerException));}else{assert_eq!(e.kind,ExecutionErrorKind::StackUnderflow);}assert_eq!(e.vm_exception.unwrap().phase,VmMaterializationPhase::Constructor);assert_eq!(s.vm_exception_usage(),VmExceptionUsage{started:1,produced:0});});
    }
}

#[test]
fn message_and_allocation_failures_keep_original_fault_phase_and_paid_prefix() {
    for (instances, phase) in [
        (0, VmMaterializationPhase::Message),
        (1, VmMaterializationPhase::Allocation),
    ] {
        let mut w = World::build(
            program(),
            Config {
                instances: InstanceOptions {
                    max_instances: instances,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let m = w.method("idiv", "(II)I");
        let ae = w.arithmetic;
        w.with(|s, _| {
            let e = run(s, m, &[Value::Int(1), Value::Int(0)]).unwrap_err();
            assert_eq!(e.offset, Some(2));
            assert_eq!(e.method.as_ref().unwrap().index, m.index());
            let vm = e.vm_exception.unwrap();
            assert_eq!(vm.origin.method, m);
            assert_eq!(vm.phase, phase);
            assert_eq!(
                s.vm_exception_usage(),
                VmExceptionUsage {
                    started: 1,
                    produced: 0
                }
            );
            assert_eq!(s.status(ae).unwrap(), InitializationStatus::Completed);
            assert_eq!(s.context().heap().allocated_instances(), instances);
        });
        assert_eq!(w.h.throwable_trace_usage().snapshots, 1);
    }
}
#[test]
fn no_capture_constructor_cannot_dispatch_an_unfinished_exception() {
    let mut own = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
    let base = own.reference(
        10,
        "java/lang/RuntimeException",
        "<init>",
        "(Ljava/lang/String;)V",
    );
    own.method("<init>", "()V", 1, &[0xb1], 0, 1);
    own.method(
        "<init>",
        "(Ljava/lang/String;)V",
        1,
        &[vec![0x2a, 0x2b], instruction(0xb7, base), vec![0xb1]].concat(),
        2,
        2,
    );
    own.method(
        "fillInStackTrace",
        "()Ljava/lang/Throwable;",
        1,
        &[0x2a, 0xb0],
        1,
        1,
    );
    let mut w = World::build(
        program(),
        Config {
            arithmetic_class: own,
            ..Default::default()
        },
    );
    let m = w.method("caught", "()Ljava/lang/Throwable;");
    w.with(|s, _| {
        let e = run(s, m, &[]).unwrap_err();
        assert!(matches!(
            e.kind,
            ExecutionErrorKind::VmExceptionProtocol { .. }
        ));
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 0
            }
        );
    });
    assert_eq!(w.h.throwable_trace_usage().backtraces, 0);
}

#[test]
fn a_second_fault_in_one_evaluation_hits_the_inclusive_admission_limit() {
    let mut c = program();
    c.method(
        "twice",
        "()V",
        9,
        &[4, 3, 0x6c, 0x57, 0xb1, 0x57, 4, 3, 0x6c, 0x57, 0xb1],
        2,
        0,
    );
    let ae = c.class(ARITHMETIC);
    c.handler([0, 3, 5, ae]);
    let mut w = World::build(
        c,
        Config {
            vm_limits: VmExceptionLimits {
                max_materializations: 1,
            },
            ..Default::default()
        },
    );
    let m = w.method("twice", "()V");
    w.with(|s, _| {
        let e = run(s, m, &[]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::VmExceptionLimit {
                limit: 1,
                required: 2
            }
        );
        assert_eq!(e.offset, Some(8));
        assert_eq!(
            e.vm_exception.unwrap().origin,
            VmFaultOrigin {
                method: m,
                pc: 8,
                kind: VmExceptionKind::ArithmeticException
            }
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
    });
}
#[test]
fn selected_constructor_decode_failure_is_target_preflight_before_snapshot_or_initialization() {
    let mut own = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
    own.method("<init>", "()V", 1, &[0xb1], 0, 1);
    own.method("<init>", "(Ljava/lang/String;)V", 1, &[0x10], 1, 2);
    let mut w = World::build(
        program(),
        Config {
            arithmetic_class: own,
            ..Default::default()
        },
    );
    let m = w.method("idiv", "(II)I");
    let constructor = w.g.method(w.arithmetic, "<init>", "(Ljava/lang/String;)V");
    let ae = w.arithmetic;
    w.with(|s, _| {
        let e = run(s, m, &[Value::Int(1), Value::Int(0)]).unwrap_err();
        assert!(matches!(e.kind, ExecutionErrorKind::Decode(_)));
        assert_eq!(e.method.as_ref().unwrap().owner, Some(constructor.owner()));
        assert_eq!(e.method.as_ref().unwrap().index, constructor.index());
        assert_eq!(e.offset, Some(1));
        let site = e.call_site.unwrap();
        assert_eq!(site.method.index, m.index());
        assert_eq!(site.offset, 2);
        assert_eq!(
            e.vm_exception.unwrap().phase,
            VmMaterializationPhase::Preflight
        );
        assert_eq!(s.status(ae).unwrap(), InitializationStatus::Uninitialized);
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 0
            }
        );
    });
    assert_eq!(w.h.throwable_trace_usage(), Default::default());
    assert_eq!(w.h.allocated_instances(), 0);
}
#[test]
fn different_throwable_captured_during_vm_constructor_crosses_saved_fault_boundary() {
    let mut own = Class::new(ARITHMETIC, Some("java/lang/RuntimeException"), 0x21);
    let saved = own.reference(9, "C", "replacement", "Ljava/lang/Throwable;");
    let refill = own.reference(10, THROWABLE, "fillInStackTrace", "()Ljava/lang/Throwable;");
    let base = own.reference(
        10,
        "java/lang/RuntimeException",
        "<init>",
        "(Ljava/lang/String;)V",
    );
    own.method("<init>", "()V", 1, &[0xb1], 0, 1);
    own.method(
        "<init>",
        "(Ljava/lang/String;)V",
        1,
        &[
            instruction(0xb2, saved),
            instruction(0xb6, refill),
            vec![0x57, 0x2a, 0x2b],
            instruction(0xb7, base),
            vec![0xb1],
        ]
        .concat(),
        2,
        2,
    );
    let mut w = World::build(
        replacement_program(),
        Config {
            arithmetic_class: own,
            ..Default::default()
        },
    );
    let t = w.throwable;
    let other = w.context().heap_mut().allocate_instance(t).unwrap();
    let save = w.method("save", "(Ljava/lang/Throwable;)V");
    let m = w.method("idiv", "(II)I");
    let ctor = w.g.method(w.arithmetic, "<init>", "(Ljava/lang/String;)V");
    w.with(|s, _| {
        run(s, save, &[Value::Reference(other)]).unwrap();
        let r = reference(run(s, m, &[Value::Int(1), Value::Int(0)]).unwrap());
        assert_ne!(r, other);
        assert_eq!(
            s.throwable_trace(other).unwrap().unwrap(),
            &[
                TraceFrame {
                    method: ctor,
                    pc: 3
                },
                TraceFrame { method: m, pc: 2 }
            ]
        );
        assert_eq!(
            s.throwable_trace(r).unwrap().unwrap(),
            &[TraceFrame { method: m, pc: 2 }]
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
    });
}

#[test]
fn failure_during_first_handler_search_is_not_nested_materialization() {
    let hidden = Class::new("q/Hidden", Some(THROWABLE), 0x20);
    let mut c = program();
    let catch = c.class("q/Hidden");
    c.method("search", "()I", 9, &[4, 3, 0x6c, 0xac], 2, 0);
    c.handler([0, 3, 3, catch]);
    let mut w = World::build(
        c,
        Config {
            extra: vec![hidden],
            ..Default::default()
        },
    );
    let method = w.method("search", "()I");
    w.with(|s, _| {
        let e = run(s, method, &[]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::IllegalAccessError)
        );
        assert_eq!(e.offset, Some(2));
        assert_eq!(
            e.vm_exception.unwrap().phase,
            VmMaterializationPhase::Dispatch
        );
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 1,
                produced: 1
            }
        );
    });
}
#[test]
fn covariant_bridge_and_delegating_override_are_filtered_but_bypass_has_no_capture() {
    for bypass in [false, true] {
        let mut child = exception("Child", THROWABLE);
        if bypass {
            child.method(
                "fillInStackTrace",
                "()Ljava/lang/Throwable;",
                1,
                &[0x2a, 0xb0],
                1,
                1,
            );
        } else {
            let base =
                child.reference(10, THROWABLE, "fillInStackTrace", "()Ljava/lang/Throwable;");
            let own = child.class("Child");
            child.method(
                "fillInStackTrace",
                "()LChild;",
                0x21,
                &[
                    vec![0x2a],
                    instruction(0xb7, base),
                    instruction(0xc0, own),
                    vec![0xb0],
                ]
                .concat(),
                1,
                1,
            );
            let target = child.reference(10, "Child", "fillInStackTrace", "()LChild;");
            child.method(
                "fillInStackTrace",
                "()Ljava/lang/Throwable;",
                0x1041,
                &[vec![0x2a], instruction(0xb6, target), vec![0xb0]].concat(),
                1,
                1,
            );
        }
        let mut c = program();
        let ty = c.class("Child");
        let ctor = c.reference(10, "Child", "<init>", "()V");
        c.method(
            "child",
            "()Ljava/lang/Throwable;",
            9,
            &[
                instruction(0xbb, ty),
                vec![0x59],
                instruction(0xb7, ctor),
                vec![0xb0],
            ]
            .concat(),
            2,
            0,
        );
        let mut w = World::build(
            c,
            Config {
                extra: vec![child],
                ..Default::default()
            },
        );
        let method = w.method("child", "()Ljava/lang/Throwable;");
        w.with(|s, _| {
            let r = reference(run(s, method, &[]).unwrap());
            if bypass {
                assert_eq!(s.throwable_trace(r).unwrap(), None);
            } else {
                assert_eq!(
                    s.throwable_trace(r).unwrap().unwrap(),
                    &[TraceFrame { method, pc: 4 }]
                );
            }
        });
        assert_eq!(w.h.throwable_trace_usage().backtraces, u64::from(!bypass));
    }
}
#[test]
fn unsupported_native_direct_root_remains_closed_even_with_profile_enabled() {
    let mut w = World::new(program());
    let native =
        w.g.method(w.throwable, "fillInStackTrace", "(I)Ljava/lang/Throwable;");
    w.with(|s, _| {
        let error = run(s, native, &[Value::Int(0)]).unwrap_err();
        assert!(matches!(
            error.kind,
            ExecutionErrorKind::UnsupportedMethod(_)
        ));
        assert!(error.vm_exception.is_none());
        assert_eq!(s.vm_exception_usage(), Default::default());
    });
    assert_eq!(w.h.throwable_trace_usage(), Default::default());
}

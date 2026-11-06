//! Independent operation tests: real guest constructors and original fault PCs.
#[path = "support/operand_world.rs"]
mod world;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use world::*;

fn result_reference(x: ExceptionOutcome) -> Reference {
    match x {
        ExceptionOutcome::Uncaught(r) | ExceptionOutcome::Returned(Some(Value::Reference(r))) => r,
        x => panic!("{x:?}"),
    }
}
fn program() -> Class {
    let mut c = app();
    let get = c.reference(10, THROWABLE, "getMessage", "()Ljava/lang/String;");
    c.method(
        "message",
        "(Ljava/lang/Throwable;)Ljava/lang/String;",
        9,
        &[vec![0x2a], instruction(0xb6, get), vec![0xb0]].concat(),
        1,
        1,
    );
    for (name, atype, load, store, value, slots) in [
        ("int", 10, 0x2e, 0x4f, 3, 1),
        ("long", 11, 0x2f, 0x50, 9, 2),
        ("float", 6, 0x30, 0x51, 0x0b, 1),
        ("double", 7, 0x31, 0x52, 0x0e, 2),
        ("byte", 8, 0x33, 0x54, 3, 1),
        ("boolean", 4, 0x33, 0x54, 3, 1),
        ("char", 5, 0x34, 0x55, 3, 1),
        ("short", 9, 0x35, 0x56, 3, 1),
    ] {
        c.method(
            &format!("load_{name}"),
            "(II)V",
            9,
            &[
                0x1a,
                0xbc,
                atype,
                0x1b,
                load,
                if slots == 2 { 0x58 } else { 0x57 },
                0xb1,
            ],
            2,
            2,
        );
        c.method(
            &format!("store_{name}"),
            "(II)V",
            9,
            &[0x1a, 0xbc, atype, 0x1b, value, store, 0xb1],
            2 + slots,
            2,
        );
        c.method(
            &format!("new_{name}"),
            "(I)V",
            9,
            &[0x1a, 0xbc, atype, 0x57, 0xb1],
            1,
            1,
        );
    }
    let string = c.class(STRING);
    let two = c.class("[[Ljava/lang/String;");
    c.method(
        "refnew",
        "(I)V",
        9,
        &[vec![0x1a], instruction(0xbd, string), vec![0x57, 0xb1]].concat(),
        1,
        1,
    );
    c.method(
        "multi",
        "(II)V",
        9,
        &[
            vec![0x1a, 0x1b, 0xc5],
            two.to_be_bytes().to_vec(),
            vec![2, 0x57, 0xb1],
        ]
        .concat(),
        2,
        2,
    );
    c.method(
        "refload",
        "(II)V",
        9,
        &[
            vec![0x1a],
            instruction(0xbd, string),
            vec![0x1b, 0x32, 0x57, 0xb1],
        ]
        .concat(),
        2,
        2,
    );
    c.method(
        "refstore",
        "(II)V",
        9,
        &[
            vec![0x1a],
            instruction(0xbd, string),
            vec![0x1b, 1, 0x53, 0xb1],
        ]
        .concat(),
        3,
        2,
    );
    c.method(
        "store",
        "([Ljava/lang/Object;Ljava/lang/Object;)V",
        9,
        &[0x2a, 3, 0x2b, 0x53, 0xb1],
        3,
        2,
    );
    c.method(
        "cast",
        "(Ljava/lang/Object;)V",
        9,
        &[vec![0x2a], instruction(0xc0, string), vec![0x57, 0xb1]].concat(),
        1,
        1,
    );

    c.method(
        "raw_store",
        "([BII)V",
        9,
        &[0x2a, 0x1b, 0x1c, 0x54, 0xb1],
        3,
        3,
    );
    c.method(
        "rethrow",
        "(Ljava/lang/Throwable;)V",
        9,
        &[0x2a, 0xbf],
        1,
        1,
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
    let fault = c.reference(10, "C", "refnew", "(I)V");
    c.method(
        "caller",
        "(I)V",
        9,
        &[vec![0x1a], instruction(0xb8, fault), vec![0xb1]].concat(),
        1,
        1,
    );
    let runtime = c.class("java/lang/RuntimeException");
    c.method(
        "caller_catch",
        "(I)Ljava/lang/Throwable;",
        9,
        &[vec![0x1a], instruction(0xb8, fault), vec![1, 0xb0, 0xb0]].concat(),
        1,
        1,
    );
    c.handler([0, 4, 6, runtime]);

    for (name, atype, load, store, load_arg, desc, ret, slots) in [
        ("int", 10, 0x2e, 0x4f, 0x1a, "(I)I", 0xac, 1),
        ("long", 11, 0x2f, 0x50, 0x1e, "(J)J", 0xad, 2),
        ("float", 6, 0x30, 0x51, 0x22, "(F)F", 0xae, 1),
        ("double", 7, 0x31, 0x52, 0x26, "(D)D", 0xaf, 2),
        ("byte", 8, 0x33, 0x54, 0x1a, "(I)I", 0xac, 1),
        ("boolean", 4, 0x33, 0x54, 0x1a, "(I)I", 0xac, 1),
        ("char", 5, 0x34, 0x55, 0x1a, "(I)I", 0xac, 1),
        ("short", 9, 0x35, 0x56, 0x1a, "(I)I", 0xac, 1),
    ] {
        c.method(
            &format!("round_{name}"),
            desc,
            9,
            &[4, 0xbc, atype, 0x59, 3, load_arg, store, 3, load, ret],
            3 + slots,
            slots,
        );
    }
    c.method("null", "()V", 9, &[1, 3, 0x2e, 0x57, 0xb1], 2, 0);
    c.method("div", "()V", 9, &[4, 3, 0x6c, 0x57, 0xb1], 2, 0);
    c.method(
        "catch",
        "(II)Ljava/lang/Throwable;",
        9,
        &[0x1a, 0xbc, 10, 0x1b, 0x2e, 0x57, 1, 0xb0, 0xb0],
        2,
        2,
    );
    let bounds = c.class(OPERAND_CLASSES[1]);
    c.handler([0, 6, 8, bounds]);
    c
}
fn message(s: &mut InitializationSession<'_, '_>, m: MethodId, r: Reference) -> String {
    let text = result_reference(run(s, m, &[Value::Reference(r)]).unwrap());
    String::from_utf16(s.context().heap().string_units(text).unwrap().unwrap()).unwrap()
}
fn expect_fault(
    s: &mut InitializationSession<'_, '_>,
    m: MethodId,
    msg: MethodId,
    args: &[Value],
    pc: u32,
    class: &str,
    text: &str,
) -> Reference {
    let r = result_reference(run(s, m, args).unwrap());
    let id = s.context().heap().instance_class(r).unwrap();
    assert!(
        s.context()
            .registry()
            .class(id)
            .unwrap()
            .name()
            .unwrap()
            .equals_ascii(class)
    );
    assert_eq!(
        s.throwable_trace(r).unwrap().unwrap(),
        &[TraceFrame { method: m, pc }]
    );
    assert_eq!(message(s, msg, r), text);
    r
}
#[test]
fn all_primitive_load_store_families_keep_original_index_length_and_fault_pc() {
    let mut w = World::new(program());
    let msg = w.method("message", "(Ljava/lang/Throwable;)Ljava/lang/String;");
    let methods = [
        "int", "long", "float", "double", "byte", "boolean", "char", "short",
    ]
    .map(|n| {
        [
            w.method(&format!("load_{n}"), "(II)V"),
            w.method(&format!("store_{n}"), "(II)V"),
        ]
    });
    w.with(|s, _| {
        for pair in methods {
            for (i, m) in pair.into_iter().enumerate() {
                for index in [-1, 2, i32::MIN, i32::MAX] {
                    expect_fault(
                        s,
                        m,
                        msg,
                        &[Value::Int(2), Value::Int(index)],
                        if i == 0 { 4 } else { 5 },
                        OPERAND_CLASSES[1],
                        &format!("Index {index} out of bounds for length 2"),
                    );
                }
            }
        }
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 64,
                produced: 64
            }
        );
    });
}
#[test]
fn every_primitive_allocation_captures_minimum_negative_length() {
    let mut w = World::new(program());
    let msg = w.method("message", "(Ljava/lang/Throwable;)Ljava/lang/String;");
    let methods = [
        "int", "long", "float", "double", "byte", "boolean", "char", "short",
    ]
    .map(|n| w.method(&format!("new_{n}"), "(I)V"));
    w.with(|s, _| {
        for m in methods {
            expect_fault(
                s,
                m,
                msg,
                &[Value::Int(i32::MIN)],
                1,
                OPERAND_CLASSES[2],
                "-2147483648",
            );
        }
    });
}
#[test]
fn reference_bounds_and_negative_allocation_use_actual_operation_facts() {
    let mut w = World::new(program());
    let msg = w.method("message", "(Ljava/lang/Throwable;)Ljava/lang/String;");
    let new = w.method("refnew", "(I)V");
    let load = w.method("refload", "(II)V");
    let store = w.method("refstore", "(II)V");
    w.with(|s, _| {
        expect_fault(s, new, msg, &[Value::Int(-9)], 1, OPERAND_CLASSES[2], "-9");
        for (m, pc) in [(load, 5), (store, 6)] {
            expect_fault(
                s,
                m,
                msg,
                &[Value::Int(0), Value::Int(0)],
                pc,
                OPERAND_CLASSES[1],
                "Index 0 out of bounds for length 0",
            );
        }
    });
}
#[test]
fn multianewarray_checks_all_requested_counts_after_zero_and_uses_first_negative() {
    let mut w = World::new(program());
    let msg = w.method("message", "(Ljava/lang/Throwable;)Ljava/lang/String;");
    let m = w.method("multi", "(II)V");
    w.with(|s, _| {
        for (a, b, text) in [(0, -7, "-7"), (-3, -8, "-3"), (2, -9, "-9")] {
            expect_fault(
                s,
                m,
                msg,
                &[Value::Int(a), Value::Int(b)],
                2,
                OPERAND_CLASSES[2],
                text,
            );
        }
    });
}
#[test]
fn store_uses_actual_covariant_component_and_source_not_symbolic_parameter() {
    let mut w = World::new(program());
    let obj = w.g.object;
    let string = w.string;
    let cls = w.class;
    let array = w
        .context()
        .allocate_reference_array(ReferenceType::Class(string), 1)
        .unwrap();
    let source = w.mirror(obj);
    let m = w.method("store", "([Ljava/lang/Object;Ljava/lang/Object;)V");
    let msg = w.method("message", "(Ljava/lang/Throwable;)Ljava/lang/String;");
    w.with(|s, _| {
        expect_fault(
            s,
            m,
            msg,
            &[Value::Reference(array), Value::Reference(source)],
            3,
            OPERAND_CLASSES[3],
            "java.lang.Class",
        );
        assert_eq!(s.context().heap().instance_class(source).unwrap(), cls);
    });
    assert_eq!(
        w.context().read_reference_array(array, 0).unwrap(),
        Reference::null()
    );
}
#[test]
fn cast_uses_actual_source_type_and_null_is_a_no_fault_success() {
    let mut w = World::new(program());
    let obj = w.g.object;
    let source = w.mirror(obj);
    let m = w.method("cast", "(Ljava/lang/Object;)V");
    let msg = w.method("message", "(Ljava/lang/Throwable;)Ljava/lang/String;");
    w.with(|s, _| {
        assert_eq!(
            run(s, m, &[Value::Reference(Reference::null())]).unwrap(),
            ExceptionOutcome::Returned(None)
        );
        expect_fault(
            s,
            m,
            msg,
            &[Value::Reference(source)],
            1,
            OPERAND_CLASSES[4],
            "class java.lang.Class cannot be cast to class java.lang.String",
        );
    });
}
#[test]
fn caught_reference_identity_and_refill_free_trace_survive_repeated_evaluations() {
    let mut w = World::new(program());
    let m = w.method("catch", "(II)Ljava/lang/Throwable;");
    let msg = w.method("message", "(Ljava/lang/Throwable;)Ljava/lang/String;");
    w.with(|s, _| {
        let a = expect_fault(
            s,
            m,
            msg,
            &[Value::Int(1), Value::Int(2)],
            4,
            OPERAND_CLASSES[1],
            "Index 2 out of bounds for length 1",
        );
        let b = expect_fault(
            s,
            m,
            msg,
            &[Value::Int(1), Value::Int(2)],
            4,
            OPERAND_CLASSES[1],
            "Index 2 out of bounds for length 1",
        );
        assert_ne!(a, b);
        assert_eq!(
            s.vm_exception_usage(),
            VmExceptionUsage {
                started: 2,
                produced: 2
            }
        );
    });
}
#[test]
fn successful_paths_and_unhandled_null_producer_do_not_spend_diagnostics() {
    let mut w = World::new(program());
    let m = w.method("load_int", "(II)V");
    let null = w.method("null", "()V");
    w.with(|s, _| {
        assert_eq!(
            run(s, m, &[Value::Int(2), Value::Int(1)]).unwrap(),
            ExceptionOutcome::Returned(None)
        );
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            Default::default()
        );
        let e = run(s, null, &[]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException)
        );
        assert_eq!(s.vm_exception_usage(), Default::default());
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            Default::default()
        );
    });
}
#[test]
fn explicit_older_throwable_session_keeps_symbolic_unsupported_operand_boundary() {
    let mut w = World::build(
        program(),
        Config {
            operands: false,
            ..Default::default()
        },
    );
    let m = w.method("refnew", "(I)V");
    w.with(|s, _| {
        let e = run(s, m, &[Value::Int(-1)]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NegativeArraySizeException)
        );
        assert!(e.vm_exception.is_none());
        assert_eq!(s.vm_exception_usage(), Default::default());
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            Default::default()
        );
    });
}
#[test]
fn zero_producer_limit_precedes_new_diagnostic_work_and_allocation() {
    let mut w = World::build(
        program(),
        Config {
            vm_limits: VmExceptionLimits {
                max_materializations: 0,
            },
            ..Default::default()
        },
    );
    let m = w.method("refnew", "(I)V");
    w.with(|s, _| {
        let e = run(s, m, &[Value::Int(-1)]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::VmExceptionLimit {
                limit: 0,
                required: 1
            }
        );
        assert_eq!(
            e.vm_exception.unwrap().origin,
            VmFaultOrigin {
                method: m,
                pc: 1,
                kind: VmExceptionKind::NegativeArraySizeException
            }
        );
        assert_eq!(s.vm_exception_usage(), Default::default());
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            Default::default()
        );
    });
}
#[test]
fn spec_failure_keeps_started_and_original_preflight_location() {
    let mut w = World::build(
        program(),
        Config {
            diagnostics: OperandDiagnosticLimits {
                max_work_units: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let m = w.method("refnew", "(I)V");
    w.with(|s,_|{let e=run(s,m,&[Value::Int(-1)]).unwrap_err();assert!(matches!(e.kind,ExecutionErrorKind::OperandExceptionAccess(ref x) if matches!(x.kind,OperandExceptionErrorKind::Limit{resource:OperandExceptionResource::DiagnosticWork,limit:0,required:1})));assert_eq!(e.offset,Some(1));assert_eq!(e.method.as_ref().unwrap().owner,Some(m.owner()));assert_eq!(e.vm_exception.unwrap().phase,VmMaterializationPhase::Preflight);assert_eq!(s.vm_exception_usage(),VmExceptionUsage{started:1,produced:0});assert_eq!(s.context().heap().operand_diagnostic_usage(),Default::default());});
}
#[test]
fn exact_negative_message_work_and_units_are_inclusive_and_arithmetic_is_excluded() {
    for (limit, success) in [(5, false), (6, true)] {
        let mut w = World::build(
            program(),
            Config {
                diagnostics: OperandDiagnosticLimits {
                    max_work_units: limit,
                    max_message_units: 2,
                    max_total_message_units: 2,
                    max_temporary_bytes: 4,
                },
                ..Default::default()
            },
        );
        let m = w.method("refnew", "(I)V");
        let div = w.method("div", "()V");
        w.with(|s, _| {
            let result = run(s, m, &[Value::Int(-1)]);
            assert_eq!(result.is_ok(), success, "{result:?}");
            let u = s.context().heap().operand_diagnostic_usage();
            if success {
                assert_eq!(
                    u,
                    OperandDiagnosticUsage {
                        work_units: 6,
                        message_units: 2,
                        messages: 1
                    }
                );
            } else {
                assert_eq!(u.message_units, 0);
                assert_eq!(u.messages, 0);
            }
            let before = u;
            assert!(matches!(
                run(s, div, &[]).unwrap(),
                ExceptionOutcome::Uncaught(_)
            ));
            assert_eq!(s.context().heap().operand_diagnostic_usage(), before);
        });
    }
}
#[test]
fn message_unit_total_and_temporary_one_less_fail_without_publication() {
    for diagnostics in [
        OperandDiagnosticLimits {
            max_message_units: 1,
            ..Default::default()
        },
        OperandDiagnosticLimits {
            max_total_message_units: 1,
            ..Default::default()
        },
        OperandDiagnosticLimits {
            max_temporary_bytes: 3,
            ..Default::default()
        },
    ] {
        let mut w = World::build(
            program(),
            Config {
                diagnostics,
                ..Default::default()
            },
        );
        let m = w.method("refnew", "(I)V");
        w.with(|s, _| {
            let e = run(s, m, &[Value::Int(-1)]).unwrap_err();
            assert!(matches!(
                e.kind,
                ExecutionErrorKind::OperandExceptionAccess(_)
            ));
            assert_eq!(
                e.vm_exception.unwrap().phase,
                VmMaterializationPhase::Message
            );
            let u = s.context().heap().operand_diagnostic_usage();
            assert_eq!((u.messages, u.message_units), (0, 0));
            assert_eq!(
                s.vm_exception_usage(),
                VmExceptionUsage {
                    started: 1,
                    produced: 0
                }
            );
        });
    }
}

#[test]
fn caller_catch_rethrow_and_refill_preserve_identity_and_actual_activation_origins() {
    let mut w = World::new(program());
    let caller = w.method("caller_catch", "(I)Ljava/lang/Throwable;");
    let fault = w.method("refnew", "(I)V");
    let rethrow = w.method("rethrow", "(Ljava/lang/Throwable;)V");
    let refill = w.method("refill", "(Ljava/lang/Throwable;)Ljava/lang/Throwable;");
    w.with(|s, _| {
        let r = result_reference(run(s, caller, &[Value::Int(-2)]).unwrap());
        let expected = [
            TraceFrame {
                method: fault,
                pc: 1,
            },
            TraceFrame {
                method: caller,
                pc: 1,
            },
        ];
        assert_eq!(s.throwable_trace(r).unwrap().unwrap(), expected);
        assert_eq!(
            result_reference(run(s, rethrow, &[Value::Reference(r)]).unwrap()),
            r
        );
        assert_eq!(s.throwable_trace(r).unwrap().unwrap(), expected);
        assert_eq!(
            result_reference(run(s, refill, &[Value::Reference(r)]).unwrap()),
            r
        );
        assert_eq!(
            s.throwable_trace(r).unwrap().unwrap(),
            [TraceFrame {
                method: refill,
                pc: 1
            }]
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
fn protected_string_backing_bounds_precede_readonly_host_refusal() {
    let mut c = program();
    let length = c.reference(10, STRING, "length", "()I");
    c.method(
        "length",
        "(Ljava/lang/String;)I",
        9,
        &[vec![0x2a], instruction(0xb6, length), vec![0xac]].concat(),
        1,
        1,
    );
    let mut w = World::new(c);
    let text = w.fresh(&[65]);
    let touch = w.method("length", "(Ljava/lang/String;)I");
    let backing = value_reference(&mut w, text);
    let m = w.method("raw_store", "([BII)V");
    w.with(|s, _| {
        assert_eq!(int(run(s, touch, &[Value::Reference(text)])), 1);
        let e = run(
            s,
            m,
            &[Value::Reference(backing), Value::Int(0), Value::Int(7)],
        )
        .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::Heap(HeapError::ReadOnlyStringBacking)
        );
        assert!(e.vm_exception.is_none());
        assert_eq!(s.vm_exception_usage(), Default::default());
        assert!(matches!(
            run(
                s,
                m,
                &[Value::Reference(backing), Value::Int(1), Value::Int(7)]
            )
            .unwrap(),
            ExceptionOutcome::Uncaught(_)
        ));
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
fn snapshot_and_native_commit_failures_keep_original_and_actual_inner_locations() {
    for limit in [0, 3] {
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
        let m = w.method("refnew", "(I)V");
        let wrapper =
            w.g.method(w.throwable, "fillInStackTrace", "()Ljava/lang/Throwable;");
        w.with(|s, _| {
            let e = run(s, m, &[Value::Int(-1)]).unwrap_err();
            assert_eq!(
                e.vm_exception.as_ref().unwrap().origin,
                VmFaultOrigin {
                    method: m,
                    pc: 1,
                    kind: VmExceptionKind::NegativeArraySizeException
                }
            );
            if limit == 0 {
                assert_eq!(e.offset, Some(1));
                assert_eq!(e.method.as_ref().unwrap().owner, Some(m.owner()));
                assert_eq!(
                    e.vm_exception.unwrap().phase,
                    VmMaterializationPhase::Snapshot
                );
            } else {
                assert_eq!(e.offset, Some(2));
                assert_eq!(e.method.as_ref().unwrap().owner, Some(wrapper.owner()));
                assert_eq!(e.method.as_ref().unwrap().index, wrapper.index());
                assert_eq!(
                    e.vm_exception.unwrap().phase,
                    VmMaterializationPhase::Constructor
                );
            }
            assert_eq!(
                s.vm_exception_usage(),
                VmExceptionUsage {
                    started: 1,
                    produced: 0
                }
            );
            assert_eq!(
                s.context().heap().operand_diagnostic_usage().messages,
                if limit == 0 { 0 } else { 1 }
            );
        });
    }
}
#[test]
fn nested_supported_and_unsupported_vm_faults_in_actual_operand_constructor_are_bounded() {
    for (code, secondary, pc) in [
        (
            vec![3, 3, 0x6c, 0x57, 0xb1],
            VmExceptionKind::ArithmeticException,
            2,
        ),
        (vec![1, 0xbf], VmExceptionKind::NullPointerException, 1),
        (
            vec![2, 0xbc, 10, 0x57, 0xb1],
            VmExceptionKind::NegativeArraySizeException,
            1,
        ),
    ] {
        let mut classes = Config::default().operand_classes;
        let mut bad = Class::new(OPERAND_CLASSES[2], Some("java/lang/RuntimeException"), 0x21);
        bad.method("<init>", "(Ljava/lang/String;)V", 1, &code, 2, 2);
        classes[2] = bad;
        let mut w = World::build(
            program(),
            Config {
                operand_classes: classes,
                ..Default::default()
            },
        );
        let m = w.method("refnew", "(I)V");
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
                        .equals_ascii(OPERAND_CLASSES[2])
                })
                .unwrap();
        let ctor = w.g.method(id, "<init>", "(Ljava/lang/String;)V");
        w.with(|s, _| {
            let e = run(s, m, &[Value::Int(-1)]).unwrap_err();
            let ExecutionErrorKind::NestedVmMaterialization(n) = e.kind else {
                panic!("{e:?}")
            };
            assert_eq!(
                n.original,
                VmFaultOrigin {
                    method: m,
                    pc: 1,
                    kind: VmExceptionKind::NegativeArraySizeException
                }
            );
            assert_eq!(
                n.secondary,
                VmFaultOrigin {
                    method: ctor,
                    pc,
                    kind: secondary
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
}

#[test]
fn legal_primitive_endpoints_preserve_raw_category_two_bits_and_normalize_integrals() {
    let mut w = World::new(program());
    let cases = [
        ("int", "(I)I", Value::Int(i32::MIN), Value::Int(i32::MIN)),
        ("long", "(J)J", Value::Long(i64::MIN), Value::Long(i64::MIN)),
        (
            "float",
            "(F)F",
            Value::Float(0xffc01234),
            Value::Float(0xffc01234),
        ),
        (
            "double",
            "(D)D",
            Value::Double(0xfff8000000001234),
            Value::Double(0xfff8000000001234),
        ),
        ("boolean", "(I)I", Value::Int(258), Value::Int(0)),
        ("byte", "(I)I", Value::Int(255), Value::Int(-1)),
        ("char", "(I)I", Value::Int(-1), Value::Int(65535)),
        ("short", "(I)I", Value::Int(65535), Value::Int(-1)),
    ];
    let methods = cases.map(|(n, d, a, r)| (w.method(&format!("round_{n}"), d), a, r));
    w.with(|s, _| {
        for (m, a, r) in methods {
            assert_eq!(
                run(s, m, &[a]).unwrap(),
                ExceptionOutcome::Returned(Some(r))
            );
        }
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            Default::default()
        );
        assert_eq!(s.vm_exception_usage(), Default::default());
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
#[test]
fn operand_constructor_and_initializer_replacement_follow_existing_java_boundaries() {
    for (initializer, error) in [(false, false), (true, true), (true, false)] {
        let mut own = exception(OPERAND_CLASSES[2], "java/lang/RuntimeException");
        let effect = own.reference(9, "C", "effect", "I");
        let saved = own.reference(9, "C", "replacement", "Ljava/lang/Throwable;");
        let code = [
            vec![0x10, 7],
            instruction(0xb3, effect),
            instruction(0xb2, saved),
            vec![0xbf],
        ]
        .concat();
        if initializer {
            own.method("<clinit>", "()V", 8, &code, 1, 0);
        } else {
            own = Class::new(OPERAND_CLASSES[2], Some("java/lang/RuntimeException"), 0x21);
            let effect = own.reference(9, "C", "effect", "I");
            let saved = own.reference(9, "C", "replacement", "Ljava/lang/Throwable;");
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
        }
        let mut classes = Config::default().operand_classes;
        classes[2] = own;
        let mut w = World::build(
            replacement_program(),
            Config {
                operand_classes: classes,
                ..Default::default()
            },
        );
        let replacement_class = if error { w.error } else { w.throwable };
        let replacement = w
            .context()
            .heap_mut()
            .allocate_instance(replacement_class)
            .unwrap();
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
                        .equals_ascii(OPERAND_CLASSES[2])
                })
                .unwrap();
        let save = w.method("save", "(Ljava/lang/Throwable;)V");
        let m = w.method("refnew", "(I)V");
        let effect = w.g.field(w.owner, "effect", "I");
        w.with(|s, statics| {
            run(s, save, &[Value::Reference(replacement)]).unwrap();
            let result = run(s, m, &[Value::Int(-1)]);
            if initializer && !error {
                let e = result.unwrap_err();
                assert_eq!(
                    e.kind,
                    ExecutionErrorKind::UnsupportedInitializerWrapping {
                        class: id,
                        thrown: replacement
                    }
                );
                assert_eq!(
                    e.vm_exception.unwrap().phase,
                    VmMaterializationPhase::Initialization
                );
                assert_eq!(s.status(id).unwrap(), InitializationStatus::Aborted);
                let again = run(s, m, &[Value::Int(-1)]).unwrap_err();
                assert_eq!(
                    again.kind,
                    ExecutionErrorKind::InitializationAborted { class: id }
                );
            } else {
                assert_eq!(result.unwrap(), ExceptionOutcome::Uncaught(replacement));
                if initializer {
                    assert_eq!(
                        s.status(id).unwrap(),
                        InitializationStatus::Erroneous { cause: replacement }
                    );
                    assert_eq!(
                        s.retained_roots().collect::<Vec<_>>(),
                        [replacement.heap_ref().unwrap()]
                    );
                }
            }
            assert_eq!(
                s.context().inspect_static(&statics, effect).unwrap(),
                Value::Int(7)
            );
            assert_eq!(s.vm_exception_usage().produced, 0);
            assert_eq!(
                s.context().heap().operand_diagnostic_usage().messages,
                u64::from(!initializer)
            );
        });
    }
}
#[test]
fn exact_operand_constructor_fuel_frame_slot_code_and_dispatch_boundaries() {
    // refnew's two executed instructions plus 3*4 constructor instructions,
    // Throwable9, Object1 and wrapper4 =28. Code bytes6+3*6+15+1+6=46.
    // Root2 + three ctor4 + Throwable4 + wrapper3 =21 slots at deepest capture.
    for (fuel, frames, slots, code, dispatch, ok) in [
        (28, 6, 21, 46, 100, true),
        (27, 6, 21, 46, 100, false),
        (28, 5, 21, 46, 100, false),
        (28, 6, 20, 46, 100, false),
        (28, 6, 21, 45, 100, false),
        (28, 6, 21, 46, 0, false),
    ] {
        let mut w = World::new(program());
        let m = w.method("refnew", "(I)V");
        w.with(|s, _| {
            let result = s.run_static(
                m,
                &[Value::Int(-1)],
                ExecutionOptions {
                    instruction_budget: fuel,
                },
                CallLimits {
                    max_frames: frames,
                    max_frame_slots: slots,
                    max_prepared_code_bytes: code,
                },
                ExceptionLimits {
                    max_dispatch_steps: dispatch,
                    ..Default::default()
                },
            );
            assert_eq!(
                result.is_ok(),
                ok,
                "{fuel}/{frames}/{slots}/{code}/{dispatch}: {result:?}"
            );
            if dispatch == 0 {
                assert_eq!(
                    s.vm_exception_usage(),
                    VmExceptionUsage {
                        started: 1,
                        produced: 1
                    }
                );
            }
        });
    }
}
#[test]
fn arithmetic_and_all_four_new_kinds_share_one_per_evaluation_admission_budget() {
    fn mixed_program() -> Class {
        let mut c = program();
        let string = c.class(STRING);
        let object = c.class("java/lang/Object");
        let mut code = vec![];
        let mut handlers = vec![];
        for body in [
            vec![4, 3, 0x6c, 0x57],
            vec![2, 0xbc, 10, 0x57],
            vec![3, 0xbc, 10, 3, 0x2e, 0x57],
            [
                vec![4],
                instruction(0xbd, string),
                vec![3],
                instruction(0x13, object),
                vec![0x53],
            ]
            .concat(),
            [
                instruction(0x13, object),
                instruction(0xc0, string),
                vec![0x57],
            ]
            .concat(),
        ] {
            let start = code.len() as u16;
            code.extend(body);
            let end = code.len() as u16;
            code.extend([0xa7, 0, 4, 0x57]);
            handlers.push([start, end, end + 3, 0]);
        }
        code.push(0xb1);
        c.method("mix", "()V", 9, &code, 3, 0);
        for h in handlers {
            c.handler(h);
        }
        c
    }
    for limit in [4, 5] {
        let mut w = World::build(
            mixed_program(),
            Config {
                vm_limits: VmExceptionLimits {
                    max_materializations: limit,
                },
                ..Default::default()
            },
        );
        let m = w.method("mix", "()V");
        w.with(|s, _| {
            let result = run(s, m, &[]);
            assert_eq!(result.is_ok(), limit == 5, "{result:?}");
            assert_eq!(
                s.vm_exception_usage(),
                VmExceptionUsage {
                    started: limit,
                    produced: limit
                }
            );
            assert_eq!(
                s.context().heap().operand_diagnostic_usage().messages,
                limit - 1
            );
        });
    }
}

#[test]
fn all_four_classes_are_caught_by_exact_and_runtime_exception_handlers() {
    for generic in [false, true] {
        let mut c = program();
        let string = c.class(STRING);
        let object = c.class("java/lang/Object");
        let runtime = c.class("java/lang/RuntimeException");
        let cases = [
            (
                "bounds",
                OPERAND_CLASSES[1],
                vec![3, 0xbc, 10, 3, 0x2e, 0x57],
                4,
            ),
            ("negative", OPERAND_CLASSES[2], vec![2, 0xbc, 10, 0x57], 1),
            (
                "store",
                OPERAND_CLASSES[3],
                [
                    vec![4],
                    instruction(0xbd, string),
                    vec![3],
                    instruction(0x13, object),
                    vec![0x53],
                ]
                .concat(),
                8,
            ),
            (
                "cast",
                OPERAND_CLASSES[4],
                [
                    instruction(0x13, object),
                    instruction(0xc0, string),
                    vec![0x57],
                ]
                .concat(),
                3,
            ),
        ];
        for (name, ty, body, _) in &cases {
            let target = if generic { runtime } else { c.class(ty) };
            let end = body.len() as u16;
            c.method(
                name,
                "()Ljava/lang/Throwable;",
                9,
                &[body.clone(), vec![1, 0xb0, 0xb0]].concat(),
                3,
                0,
            );
            c.handler([0, end, end + 2, target]);
        }
        let mut w = World::new(c);
        let methods =
            cases.map(|(name, ty, _, pc)| (w.method(name, "()Ljava/lang/Throwable;"), ty, pc));
        w.with(|s, _| {
            for (m, ty, pc) in methods {
                let ExceptionOutcome::Returned(Some(Value::Reference(r))) = run(s, m, &[]).unwrap()
                else {
                    panic!()
                };
                let id = s.context().heap().instance_class(r).unwrap();
                assert!(
                    s.context()
                        .registry()
                        .class(id)
                        .unwrap()
                        .name()
                        .unwrap()
                        .equals_ascii(ty)
                );
                assert_eq!(
                    s.throwable_trace(r).unwrap().unwrap(),
                    [TraceFrame { method: m, pc }]
                );
            }
            assert_eq!(
                s.vm_exception_usage(),
                VmExceptionUsage {
                    started: 4,
                    produced: 4
                }
            );
        });
    }
}
#[test]
fn message_failure_keeps_completed_class_initializer_write_but_no_string_or_exception() {
    let mut own = exception(OPERAND_CLASSES[2], "java/lang/RuntimeException");
    let effect = own.reference(9, "C", "effect", "I");
    own.method(
        "<clinit>",
        "()V",
        8,
        &[vec![0x10, 7], instruction(0xb3, effect), vec![0xb1]].concat(),
        1,
        0,
    );
    let mut classes = Config::default().operand_classes;
    classes[2] = own;
    let mut w = World::build(
        replacement_program(),
        Config {
            operand_classes: classes,
            diagnostics: OperandDiagnosticLimits {
                max_work_units: 1,
                ..Default::default()
            },
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
                    .equals_ascii(OPERAND_CLASSES[2])
            })
            .unwrap();
    let m = w.method("refnew", "(I)V");
    let effect = w.g.field(w.owner, "effect", "I");
    w.with(|s, statics| {
        let before = s.context().heap().allocated_instances();
        let e = run(s, m, &[Value::Int(-1)]).unwrap_err();
        assert_eq!(e.offset, Some(1));
        assert_eq!(
            e.vm_exception.unwrap().phase,
            VmMaterializationPhase::Message
        );
        assert_eq!(s.status(id).unwrap(), InitializationStatus::Completed);
        assert_eq!(
            s.context().inspect_static(&statics, effect).unwrap(),
            Value::Int(7)
        );
        assert_eq!(s.context().heap().allocated_instances(), before);
        assert_eq!(s.context().heap().string_usage().objects, 0);
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            OperandDiagnosticUsage {
                work_units: 1,
                message_units: 0,
                messages: 0
            }
        );
    });
}
#[test]
fn selected_constructor_decode_failure_retains_actual_target_before_snapshot_or_diagnostic_message()
{
    let mut own = Class::new(OPERAND_CLASSES[2], Some("java/lang/RuntimeException"), 0x21);
    own.method("<init>", "(Ljava/lang/String;)V", 1, &[0x10], 1, 2);
    let mut classes = Config::default().operand_classes;
    classes[2] = own;
    let mut w = World::build(
        program(),
        Config {
            operand_classes: classes,
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
                    .equals_ascii(OPERAND_CLASSES[2])
            })
            .unwrap();
    let ctor = w.g.method(id, "<init>", "(Ljava/lang/String;)V");
    let m = w.method("refnew", "(I)V");
    w.with(|s, _| {
        let e = run(s, m, &[Value::Int(-1)]).unwrap_err();
        assert_eq!(e.method.as_ref().unwrap().owner, Some(ctor.owner()));
        assert_eq!(e.method.as_ref().unwrap().index, ctor.index());
        assert_eq!(e.offset, Some(1));
        assert_eq!(e.call_site.as_ref().unwrap().offset, 1);
        assert_eq!(
            e.vm_exception.unwrap().phase,
            VmMaterializationPhase::Preflight
        );
        assert_eq!(
            s.context().heap().throwable_trace_usage(),
            Default::default()
        );
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            OperandDiagnosticUsage {
                work_units: 1,
                message_units: 0,
                messages: 0
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
fn missing_operand_binding_fails_before_permanent_claim_and_old_session_can_still_open() {
    let mut w = World::build(
        program(),
        Config {
            bind_operands: false,
            ..Default::default()
        },
    );
    let error = w.error;
    let throwable = w.throwable;
    let mut c = w.context();
    let statics = c.allocate_static_state(Default::default()).unwrap();
    let exceptions = c.prepare_exception_profile(throwable).unwrap();
    let e = InitializationSession::new_with_operand_exceptions(
        &mut c,
        &statics,
        &exceptions,
        error,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .err()
    .unwrap();
    assert!(
        matches!(e.kind,ExecutionErrorKind::OperandExceptionAccess(ref e)if e.kind==OperandExceptionErrorKind::Disabled)
    );
    assert_eq!(c.heap().operand_diagnostic_usage(), Default::default());
    let _legacy = InitializationSession::new_with_throwables(
        &mut c,
        &statics,
        &exceptions,
        error,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
}
#[test]
fn typed_extraction_kind_null_and_bad_cp_keep_host_or_old_java_precedence_without_admission() {
    let mut c = program();
    let string = c.class(STRING);
    let bad = c.utf("not a Class CP");
    c.method(
        "instance",
        "(Ljava/lang/Object;)I",
        9,
        &[vec![0x2a], instruction(0xc1, string), vec![0xac]].concat(),
        1,
        1,
    );
    c.method(
        "badcast",
        "()V",
        9,
        &[vec![1], instruction(0xc0, bad), vec![0x57, 0xb1]].concat(),
        1,
        0,
    );
    c.method("badpop", "()V", 9, &[3, 3, 3, 0x53, 0xb1], 3, 0);
    c.method(
        "kind",
        "(Ljava/lang/Object;)V",
        9,
        &[0x2a, 3, 3, 0x54, 0xb1],
        3,
        1,
    );
    let mut w = World::new(c);
    let object = w.g.object;
    let source = w.mirror(object);
    let instance = w.method("instance", "(Ljava/lang/Object;)I");
    let badcast = w.method("badcast", "()V");
    let badpop = w.method("badpop", "()V");
    let kind = w.method("kind", "(Ljava/lang/Object;)V");
    w.with(|s, _| {
        assert_eq!(int(run(s, instance, &[Value::Reference(source)])), 0);
        let e = run(s, badcast, &[]).unwrap_err();
        assert!(matches!(e.kind, ExecutionErrorKind::TypeAccess(_)));
        let e = run(s, badpop, &[]).unwrap_err();
        assert!(matches!(e.kind, ExecutionErrorKind::WrongValueType { .. }));
        let e = run(s, kind, &[Value::Reference(source)]).unwrap_err();
        assert!(e.vm_exception.is_none());
        let e = run(s, kind, &[Value::Reference(Reference::null())]).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException)
        );
        assert_eq!(s.vm_exception_usage(), Default::default());
        assert_eq!(
            s.context().heap().operand_diagnostic_usage(),
            Default::default()
        );
    });
}

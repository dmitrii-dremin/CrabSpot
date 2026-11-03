use crate::*;
use crabspot_classfile::{AttributeBody, ClassFile, Code, ExceptionHandler, parse};

fn u2(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend(value.to_be_bytes());
}
fn u4(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_be_bytes());
}
fn utf8(text: &str) -> Vec<u8> {
    let mut bytes = vec![1];
    u2(&mut bytes, text.len() as u16);
    bytes.extend(text.bytes());
    bytes
}
pub(crate) fn synthetic(
    code: &[u8],
    descriptor: &str,
    max_stack: u16,
    max_locals: u16,
) -> ClassFile {
    with_constants(code, descriptor, max_stack, max_locals, &[])
}
fn with_constants(
    code: &[u8],
    descriptor: &str,
    max_stack: u16,
    max_locals: u16,
    extra: &[Vec<u8>],
) -> ClassFile {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    let extra_slots: usize = extra
        .iter()
        .map(|entry| if matches!(entry[0], 5 | 6) { 2 } else { 1 })
        .sum();
    u2(&mut bytes, 8 + extra_slots as u16);
    for entry in [
        utf8("Synthetic"),
        vec![7, 0, 1],
        utf8("java/lang/Object"),
        vec![7, 0, 3],
        utf8("m"),
        utf8(descriptor),
        utf8("Code"),
    ] {
        bytes.extend(entry);
    }
    for entry in extra {
        bytes.extend(entry);
    }
    for value in [0x21, 2, 4, 0, 0, 1, 0x0009, 5, 6, 1, 7] {
        u2(&mut bytes, value);
    }
    u4(&mut bytes, 12 + code.len() as u32);
    u2(&mut bytes, max_stack);
    u2(&mut bytes, max_locals);
    u4(&mut bytes, code.len() as u32);
    bytes.extend(code);
    for value in [0, 0, 0] {
        u2(&mut bytes, value);
    }
    parse(&bytes).unwrap()
}
fn code_mut(class: &mut ClassFile) -> &mut Code {
    let AttributeBody::Code(code) = &mut class.methods[0].attributes[0].body else {
        panic!()
    };
    code
}
fn run(
    code: &[u8],
    descriptor: &str,
    max_stack: u16,
    max_locals: u16,
    args: &[Value],
) -> Result<ExecutionOutcome, ExecutionError> {
    execute_static_method_body(
        &synthetic(code, descriptor, max_stack, max_locals),
        0,
        args,
        ExecutionOptions::default(),
    )
}
fn returned(value: Value) -> ExecutionOutcome {
    ExecutionOutcome::Returned(Some(value))
}
fn cp(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut result = vec![tag];
    result.extend(payload);
    result
}

#[test]
fn int_and_long_arithmetic_edges_follow_java_wrapping_and_truncation() {
    use Value::{Int as I, Long as L};
    for (opcode, left, right, expected) in [
        (0x60, i32::MAX, 1, i32::MIN),
        (0x64, i32::MIN, 1, i32::MAX),
        (0x68, i32::MAX, 2, -2),
        (0x6c, i32::MIN, -1, i32::MIN),
        (0x6c, -7, 3, -2),
        (0x70, i32::MIN, -1, 0),
        (0x70, -7, 3, -1),
        (0x7e, 0x1234, 0x00ff, 0x34),
        (0x80, 0x1200, 0x34, 0x1234),
        (0x82, 0x1234, 0x00ff, 0x12cb),
    ] {
        assert_eq!(
            run(
                &[0x1a, 0x1b, opcode, 0xac],
                "(II)I",
                2,
                2,
                &[I(left), I(right)]
            )
            .unwrap(),
            returned(I(expected)),
            "{opcode:x}"
        );
    }
    for (opcode, left, right, expected) in [
        (0x61, i64::MAX, 1, i64::MIN),
        (0x65, i64::MIN, 1, i64::MAX),
        (0x69, i64::MAX, 2, -2),
        (0x6d, i64::MIN, -1, i64::MIN),
        (0x6d, -7, 3, -2),
        (0x71, i64::MIN, -1, 0),
        (0x71, -7, 3, -1),
        (0x7f, 0x1234, 0x00ff, 0x34),
        (0x81, 0x1200, 0x34, 0x1234),
        (0x83, 0x1234, 0x00ff, 0x12cb),
    ] {
        assert_eq!(
            run(
                &[0x1e, 0x20, opcode, 0xad],
                "(JJ)J",
                4,
                4,
                &[L(left), L(right)]
            )
            .unwrap(),
            returned(L(expected)),
            "{opcode:x}"
        );
    }
    assert_eq!(
        run(&[0x1a, 0x74, 0xac], "(I)I", 1, 1, &[I(i32::MIN)]).unwrap(),
        returned(I(i32::MIN))
    );
    assert_eq!(
        run(&[0x1e, 0x75, 0xad], "(J)J", 2, 2, &[L(i64::MIN)]).unwrap(),
        returned(L(i64::MIN))
    );
}

#[test]
fn zero_divisors_are_java_exception_outcomes() {
    for opcode in [0x6c, 0x70] {
        assert_eq!(
            run(
                &[0x1a, 0x1b, opcode, 0xac],
                "(II)I",
                2,
                2,
                &[Value::Int(1), Value::Int(0)]
            )
            .unwrap(),
            ExecutionOutcome::ArithmeticException
        );
    }
    for opcode in [0x6d, 0x71] {
        assert_eq!(
            run(
                &[0x1e, 0x20, opcode, 0xad],
                "(JJ)J",
                4,
                4,
                &[Value::Long(i64::MIN), Value::Long(0)]
            )
            .unwrap(),
            ExecutionOutcome::ArithmeticException
        );
    }
}

#[test]
fn masked_shifts_use_int_distances_and_signed_or_logical_semantics() {
    for (opcode, value, distance, expected) in [
        (0x78, 1, 32, 1),
        (0x78, 1, -1, i32::MIN),
        (0x7a, i32::MIN, 31, -1),
        (0x7c, -1, -1, 1),
        (0x7c, -1, 32, -1),
    ] {
        assert_eq!(
            run(
                &[0x1a, 0x1b, opcode, 0xac],
                "(II)I",
                2,
                2,
                &[Value::Int(value), Value::Int(distance)]
            )
            .unwrap(),
            returned(Value::Int(expected))
        );
    }
    for (opcode, value, distance, expected) in [
        (0x79, 1, 64, 1),
        (0x79, 1, -1, i64::MIN),
        (0x7b, i64::MIN, 63, -1),
        (0x7d, -1, -1, 1),
        (0x7d, -1, 64, -1),
    ] {
        assert_eq!(
            run(
                &[0x1e, 0x1c, opcode, 0xad],
                "(JI)J",
                3,
                3,
                &[Value::Long(value), Value::Int(distance)]
            )
            .unwrap(),
            returned(Value::Long(expected))
        );
    }
    assert!(matches!(
        run(
            &[0x1e, 0x20, 0x79, 0xad],
            "(JJ)J",
            4,
            4,
            &[Value::Long(1), Value::Long(1)]
        )
        .unwrap_err()
        .kind,
        ExecutionErrorKind::WrongValueType {
            expected: ValueKind::Int,
            actual: ValueKind::Long
        }
    ));
}

#[test]
fn constants_conversions_and_narrow_returns() {
    for (code, expected) in [
        (vec![0x02, 0xac], -1),
        (vec![0x08, 0xac], 5),
        (vec![0x10, 0x80, 0xac], -128),
        (vec![0x11, 0x80, 0x00, 0xac], -32768),
    ] {
        assert_eq!(
            run(&code, "()I", 1, 0, &[]).unwrap(),
            returned(Value::Int(expected))
        );
    }
    assert_eq!(
        run(&[0x1a, 0x85, 0xad], "(I)J", 2, 1, &[Value::Int(i32::MIN)]).unwrap(),
        returned(Value::Long(i64::from(i32::MIN)))
    );
    assert_eq!(
        run(
            &[0x1e, 0x88, 0xac],
            "(J)I",
            2,
            2,
            &[Value::Long(0x1234ffffffff)]
        )
        .unwrap(),
        returned(Value::Int(-1))
    );
    for (opcode, expected) in [(0x91, -1), (0x92, 0x80ff), (0x93, -32513)] {
        assert_eq!(
            run(
                &[0x1a, opcode, 0xac],
                "(I)I",
                1,
                1,
                &[Value::Int(0x123480ff)]
            )
            .unwrap(),
            returned(Value::Int(expected))
        );
    }
    for (descriptor, input, expected) in [
        ("(I)B", 255, -1),
        ("(I)S", 65535, -1),
        ("(I)C", -1, 65535),
        ("(I)Z", 2, 0),
        ("(I)Z", -1, 1),
    ] {
        assert_eq!(
            run(&[0x1a, 0xac], descriptor, 1, 1, &[Value::Int(input)]).unwrap(),
            returned(Value::Int(expected))
        );
    }
    for (left, right, expected) in [
        (i64::MIN, i64::MAX, -1),
        (i64::MAX, i64::MIN, 1),
        (42, 42, 0),
    ] {
        assert_eq!(
            run(
                &[0x1e, 0x20, 0x94, 0xac],
                "(JJ)I",
                4,
                4,
                &[Value::Long(left), Value::Long(right)]
            )
            .unwrap(),
            returned(Value::Int(expected))
        );
    }
}

#[test]
fn numeric_ldc_checks_slots_kinds_and_supported_constant_types() {
    let int = cp(3, &i32::MIN.to_be_bytes());
    let long = cp(5, &i64::MIN.to_be_bytes());
    for code in [&[0x12, 8, 0xac][..], &[0x13, 0, 8, 0xac]] {
        let class = with_constants(code, "()I", 1, 0, std::slice::from_ref(&int));
        assert_eq!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default()).unwrap(),
            returned(Value::Int(i32::MIN))
        );
    }
    let class = with_constants(
        &[0x14, 0, 8, 0xad],
        "()J",
        2,
        0,
        std::slice::from_ref(&long),
    );
    assert_eq!(
        execute_static_method_body(&class, 0, &[], ExecutionOptions::default()).unwrap(),
        returned(Value::Long(i64::MIN))
    );
    for (extra, code) in [
        (vec![cp(8, &1u16.to_be_bytes())], vec![0x12, 8, 0xac]),
        (vec![], vec![0x12, 2, 0xac]),
    ] {
        let class = with_constants(&code, "()I", 4, 0, &extra);
        assert!(matches!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
                .unwrap_err()
                .kind,
            ExecutionErrorKind::UnsupportedConstant { .. }
        ));
    }
    for (extra, code) in [
        (vec![long.clone()], vec![0x12, 8, 0xac]),
        (vec![int], vec![0x14, 0, 8, 0xad]),
        (vec![long], vec![0x12, 9, 0xac]),
        (vec![], vec![0x13, 255, 255, 0xac]),
    ] {
        let class = with_constants(&code, "()I", 4, 0, &extra);
        assert!(matches!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
                .unwrap_err()
                .kind,
            ExecutionErrorKind::InvalidConstant { .. }
        ));
    }
}

#[test]
fn branches_take_both_paths_without_reversing_comparison_operands() {
    for (opcode, value, expected) in [
        (0x99, 0, 1),
        (0x99, 1, 0),
        (0x9a, 0, 0),
        (0x9a, -1, 1),
        (0x9b, -1, 1),
        (0x9b, 0, 0),
        (0x9c, 0, 1),
        (0x9c, -1, 0),
        (0x9d, 1, 1),
        (0x9d, 0, 0),
        (0x9e, 0, 1),
        (0x9e, 1, 0),
    ] {
        assert_eq!(
            run(
                &[0x1a, opcode, 0, 7, 0x03, 0xa7, 0, 4, 0x04, 0xac],
                "(I)I",
                1,
                1,
                &[Value::Int(value)]
            )
            .unwrap(),
            returned(Value::Int(expected))
        );
    }
    for (opcode, left, right, expected) in [
        (0x9f, 1, 1, 1),
        (0x9f, 1, 2, 0),
        (0xa0, 1, 2, 1),
        (0xa0, 2, 2, 0),
        (0xa1, 1, 2, 1),
        (0xa1, 2, 1, 0),
        (0xa2, 2, 1, 1),
        (0xa2, 1, 2, 0),
        (0xa3, 2, 1, 1),
        (0xa3, 1, 2, 0),
        (0xa4, 1, 2, 1),
        (0xa4, 2, 1, 0),
    ] {
        assert_eq!(
            run(
                &[0x1a, 0x1b, opcode, 0, 7, 0x03, 0xa7, 0, 4, 0x04, 0xac],
                "(II)I",
                2,
                2,
                &[Value::Int(left), Value::Int(right)]
            )
            .unwrap(),
            returned(Value::Int(expected))
        );
    }
    assert_eq!(
        run(&[0xc8, 0, 0, 0, 6, 0x01, 0x04, 0xac], "()I", 1, 0, &[]).unwrap(),
        returned(Value::Int(1))
    );
}

#[test]
fn switches_use_signed_keys_and_default_destinations() {
    // Switch opcode at1, two padding bytes, default/low/high then two targets.
    let mut table = vec![0x1a, 0xaa, 0xff, 0x7f];
    for value in [27i32, -1, 0, 23, 25] {
        table.extend(value.to_be_bytes());
    }
    table.extend([0x04, 0xac, 0x05, 0xac, 0x02, 0xac]);
    for (key, expected) in [
        (-2, -1),
        (-1, 1),
        (0, 2),
        (1, -1),
        (i32::MIN, -1),
        (i32::MAX, -1),
    ] {
        assert_eq!(
            run(&table, "(I)I", 1, 1, &[Value::Int(key)]).unwrap(),
            returned(Value::Int(expected))
        );
    }
    let mut lookup = vec![0x1a, 0xab, 0, 0];
    for value in [31i32, 2, -100, 27, 1000, 29] {
        lookup.extend(value.to_be_bytes());
    }
    lookup.extend([0x04, 0xac, 0x05, 0xac, 0x02, 0xac]);
    for (key, expected) in [
        (-100, 1),
        (1000, 2),
        (0, -1),
        (i32::MIN, -1),
        (i32::MAX, -1),
    ] {
        assert_eq!(
            run(&lookup, "(I)I", 1, 1, &[Value::Int(key)]).unwrap(),
            returned(Value::Int(expected))
        );
    }
}

#[test]
fn wide_load_store_and_iinc_support_large_indices_and_signed_increments() {
    assert_eq!(
        run(
            &[
                0x08, 0xc4, 0x36, 1, 0, 0xc4, 0x84, 1, 0, 0x7f, 0xff, 0xc4, 0x84, 1, 0, 0x80, 0,
                0xc4, 0x15, 1, 0, 0xac
            ],
            "()I",
            1,
            257,
            &[]
        )
        .unwrap(),
        returned(Value::Int(4))
    );
    assert_eq!(
        run(
            &[0x0a, 0xc4, 0x37, 1, 0, 0xc4, 0x16, 1, 0, 0xad],
            "()J",
            2,
            258,
            &[]
        )
        .unwrap(),
        returned(Value::Long(1))
    );
    assert_eq!(
        run(
            &[0x84, 0, 1, 0x1a, 0xac],
            "(I)I",
            1,
            1,
            &[Value::Int(i32::MAX)]
        )
        .unwrap(),
        returned(Value::Int(i32::MIN))
    );
}

#[test]
fn returns_discard_other_operands_but_check_declared_type() {
    assert_eq!(
        run(&[0x04, 0x05, 0xac], "()I", 2, 0, &[]).unwrap(),
        returned(Value::Int(2))
    );
    assert_eq!(
        run(&[0x04, 0x0a, 0xad], "()J", 3, 0, &[]).unwrap(),
        returned(Value::Long(1))
    );
    assert_eq!(
        run(&[0x04, 0xb1], "()V", 1, 0, &[]).unwrap(),
        ExecutionOutcome::Returned(None)
    );
    for (code, descriptor) in [
        (vec![0xb1], "()I"),
        (vec![0x03, 0xac], "()J"),
        (vec![0x09, 0xad], "()I"),
        (vec![0x03, 0xac], "()V"),
    ] {
        assert_eq!(
            run(&code, descriptor, 2, 0, &[]).unwrap_err().kind,
            ExecutionErrorKind::ReturnTypeMismatch
        );
    }
}

#[test]
fn instruction_budget_stops_loops_and_counts_returns_and_exception_operations() {
    let looping = synthetic(&[0xa7, 0, 0], "()V", 0, 0);
    for instruction_budget in [0, 1, 100] {
        let error =
            execute_static_method_body(&looping, 0, &[], ExecutionOptions { instruction_budget })
                .unwrap_err();
        assert_eq!(error.offset, Some(0));
        assert_eq!(
            error.kind,
            ExecutionErrorKind::InstructionBudgetExhausted {
                limit: instruction_budget
            }
        );
    }
    let returning = synthetic(&[0x03, 0xac], "()I", 1, 0);
    assert!(
        execute_static_method_body(
            &returning,
            0,
            &[],
            ExecutionOptions {
                instruction_budget: 2
            }
        )
        .is_ok()
    );
    let error = execute_static_method_body(
        &returning,
        0,
        &[],
        ExecutionOptions {
            instruction_budget: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.offset, Some(1));
    let dividing = synthetic(&[0x04, 0x03, 0x6c, 0xac], "()I", 2, 0);
    assert_eq!(
        execute_static_method_body(
            &dividing,
            0,
            &[],
            ExecutionOptions {
                instruction_budget: 3
            }
        )
        .unwrap(),
        ExecutionOutcome::ArithmeticException
    );
    assert!(matches!(
        execute_static_method_body(
            &dividing,
            0,
            &[],
            ExecutionOptions {
                instruction_budget: 2
            }
        )
        .unwrap_err()
        .kind,
        ExecutionErrorKind::InstructionBudgetExhausted { .. }
    ));
}

#[test]
fn selection_descriptor_argument_and_declaration_errors_are_explicit() {
    let class = synthetic(&[0x1e, 0xad], "(J)J", 2, 2);
    assert!(matches!(
        execute_static_method_body(&class, usize::MAX, &[], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        ExecutionErrorKind::MethodIndexOutOfBounds { .. }
    ));
    assert_eq!(
        execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        ExecutionErrorKind::ArgumentCount {
            expected: 1,
            actual: 0
        }
    );
    assert_eq!(
        execute_static_method_body(&class, 0, &[Value::Int(1)], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        ExecutionErrorKind::ArgumentType {
            index: 0,
            expected: ValueKind::Long,
            actual: ValueKind::Int
        }
    );
    for flags in [1, 0x0109, 0x0409, 0x0029] {
        let mut class = synthetic(&[0xb1], "()V", 0, 0);
        class.methods[0].access_flags = flags;
        assert!(matches!(
            execute_static_method_body(
                &class,
                0,
                &[],
                ExecutionOptions {
                    instruction_budget: 0
                }
            )
            .unwrap_err()
            .kind,
            ExecutionErrorKind::UnsupportedMethod(_)
        ));
    }
    for descriptor in ["([I)I", "(Ljava/lang/Object;)V", "()Ljava/lang/Object;"] {
        assert_eq!(
            run(&[0xb1], descriptor, 0, 4, &[]).unwrap_err().kind,
            ExecutionErrorKind::UnsupportedDescriptor
        );
    }
    for name in ["<init>", "<clinit>"] {
        let mut class = with_constants(&[0xb1], "()V", 0, 0, &[utf8(name)]);
        class.methods[0].name_index = 8;
        assert!(matches!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
                .unwrap_err()
                .kind,
            ExecutionErrorKind::UnsupportedMethod(_)
        ));
    }
    let mut class = synthetic(&[0xb1], "()V", 0, 0);
    code_mut(&mut class).exception_table.push(ExceptionHandler {
        start_pc: 0,
        end_pc: 1,
        handler_pc: 0,
        catch_type: 0,
    });
    assert!(matches!(
        execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        ExecutionErrorKind::UnsupportedMethod(_)
    ));
    let mut class = synthetic(&[0xb1], "()V", 0, 0);
    class.methods[0].attributes.clear();
    assert!(matches!(
        execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        ExecutionErrorKind::InvalidMethod(_)
    ));
}

#[test]
fn malformed_execution_states_and_unsupported_instructions_report_offsets() {
    for (code, stack, locals, kind) in [
        (
            vec![0x03, 0x60, 0xac],
            2,
            0,
            ExecutionErrorKind::StackUnderflow,
        ),
        (
            vec![0x03, 0xac],
            0,
            0,
            ExecutionErrorKind::StackOverflow {
                required: 1,
                capacity: 0,
            },
        ),
        (
            vec![0x09, 0x88, 0xac],
            1,
            0,
            ExecutionErrorKind::StackOverflow {
                required: 2,
                capacity: 1,
            },
        ),
        (
            vec![0x1a, 0xac],
            1,
            1,
            ExecutionErrorKind::LocalNotValue { index: 0 },
        ),
        (
            vec![0x09, 0x3f, 0xb1],
            2,
            1,
            ExecutionErrorKind::LocalOutOfBounds {
                index: 0,
                width: 2,
                capacity: 1,
            },
        ),
        (
            vec![0x00],
            0,
            0,
            ExecutionErrorKind::InstructionPointerOutsideCode,
        ),
    ] {
        let error = run(
            &code,
            if code.contains(&0xb1) { "()V" } else { "()I" },
            stack,
            locals,
            &[],
        )
        .unwrap_err();
        assert_eq!(error.kind, kind);
        assert!(error.offset.is_some());
    }
    let error = run(&[0x00, 0x01, 0xb1], "()V", 1, 0, &[]).unwrap_err();
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.kind, ExecutionErrorKind::UnsupportedInstruction(1));
    let error = run(&[0x03, 0xac, 0xff], "()I", 1, 0, &[]).unwrap_err();
    assert_eq!(error.offset, Some(2));
    assert!(matches!(error.kind, ExecutionErrorKind::Decode(_)));
    let mut class = synthetic(&[0x1e, 0xad], "(J)J", 2, 2);
    code_mut(&mut class).max_locals = 1;
    assert!(matches!(
        execute_static_method_body(&class, 0, &[Value::Long(1)], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        ExecutionErrorKind::LocalOutOfBounds { .. }
    ));
    assert!(matches!(
        run(
            &[0x03, 0x3c, 0x1e, 0x88, 0xac],
            "(J)I",
            2,
            2,
            &[Value::Long(5)]
        )
        .unwrap_err()
        .kind,
        ExecutionErrorKind::LocalNotValue { index: 0 }
    ));
}

#[test]
fn float_value_representation_constructors_and_widths_are_explicit() {
    assert_eq!(Value::from_f32(-0.0), Value::Float(0x8000_0000));
    assert_eq!(Value::from_f64(-0.0), Value::Double(0x8000_0000_0000_0000));
    assert_ne!(Value::from_f32(0.0), Value::from_f32(-0.0));
    assert_eq!(Value::Float(0x7fc1_2345), Value::Float(0x7fc1_2345));
    assert_ne!(Value::Float(0x7fc1_2345), Value::Float(0x7fc1_2346));
    assert_eq!(Value::Float(1).as_f32().unwrap().to_bits(), 1);
    assert_eq!(Value::Double(1).as_f64().unwrap().to_bits(), 1);
    assert!(Value::Int(1).as_f32().is_none());
    assert!(Value::Float(1).as_f64().is_none());
    assert_eq!(Value::Float(0).slots(), 1);
    assert_eq!(Value::Double(0).slots(), 2);
}

#[test]
fn float_double_arithmetic_handles_zero_infinity_subnormals_and_remainder() {
    for (opcode, left, right, expected) in [
        (0x62, 7.5, 2.5, 10.0),
        (0x66, 7.5, 2.5, 5.0),
        (0x6a, 7.5, 2.0, 15.0),
        (0x6e, 7.5, 2.0, 3.75),
        (0x72, -7.0, 2.0, -1.0),
    ] {
        assert_eq!(
            run(
                &[0x22, 0x23, opcode, 0xae],
                "(FF)F",
                2,
                2,
                &[Value::from_f32(left), Value::from_f32(right)]
            )
            .unwrap(),
            returned(Value::from_f32(expected))
        );
    }
    for (opcode, left, right, expected) in [
        (0x63, 7.5, 2.5, 10.0),
        (0x67, 7.5, 2.5, 5.0),
        (0x6b, 7.5, 2.0, 15.0),
        (0x6f, 7.5, 2.0, 3.75),
        (0x73, -7.0, 2.0, -1.0),
    ] {
        assert_eq!(
            run(
                &[0x26, 0x28, opcode, 0xaf],
                "(DD)D",
                4,
                4,
                &[Value::from_f64(left), Value::from_f64(right)]
            )
            .unwrap(),
            returned(Value::from_f64(expected))
        );
    }
    assert_eq!(
        run(
            &[0x22, 0x23, 0x6e, 0xae],
            "(FF)F",
            2,
            2,
            &[Value::from_f32(1.0), Value::from_f32(-0.0)]
        )
        .unwrap(),
        returned(Value::from_f32(f32::NEG_INFINITY))
    );
    assert_eq!(
        run(
            &[0x26, 0x28, 0x6f, 0xaf],
            "(DD)D",
            4,
            4,
            &[Value::from_f64(1.0), Value::from_f64(-0.0)]
        )
        .unwrap(),
        returned(Value::from_f64(f64::NEG_INFINITY))
    );
    assert_eq!(
        run(
            &[0x22, 0x23, 0x6e, 0xae],
            "(FF)F",
            2,
            2,
            &[Value::Float(0x0080_0000), Value::from_f32(2.0)]
        )
        .unwrap(),
        returned(Value::Float(0x0040_0000))
    );
    assert_eq!(
        run(
            &[0x26, 0x28, 0x6f, 0xaf],
            "(DD)D",
            4,
            4,
            &[Value::Double(0x0010_0000_0000_0000), Value::from_f64(2.0)]
        )
        .unwrap(),
        returned(Value::Double(0x0008_0000_0000_0000))
    );
    for (input, expected) in [(1, 0), (3, 2)] {
        assert_eq!(
            run(
                &[0x22, 0x23, 0x6e, 0xae],
                "(FF)F",
                2,
                2,
                &[Value::Float(input), Value::from_f32(2.0)]
            )
            .unwrap(),
            returned(Value::Float(expected))
        );
    }
    assert_eq!(
        run(
            &[0x22, 0x23, 0x72, 0xae],
            "(FF)F",
            2,
            2,
            &[Value::from_f32(-0.0), Value::from_f32(3.0)]
        )
        .unwrap(),
        returned(Value::from_f32(-0.0))
    );
    let outcome = run(
        &[0x22, 0x23, 0x6e, 0xae],
        "(FF)F",
        2,
        2,
        &[Value::from_f32(0.0), Value::from_f32(0.0)],
    )
    .unwrap();
    let ExecutionOutcome::Returned(Some(value)) = outcome else {
        panic!()
    };
    assert!(value.as_f32().unwrap().is_nan());
}

#[test]
fn floating_comparisons_handle_unordered_both_directions_and_signed_zero() {
    for opcode in [0x95, 0x96] {
        for (left, right, expected) in [
            (
                Value::Float(0x7fc1_2345),
                Value::from_f32(1.0),
                if opcode == 0x95 { -1 } else { 1 },
            ),
            (
                Value::from_f32(1.0),
                Value::Float(0x7fc1_2345),
                if opcode == 0x95 { -1 } else { 1 },
            ),
            (Value::from_f32(-0.0), Value::from_f32(0.0), 0),
            (Value::from_f32(-1.0), Value::from_f32(1.0), -1),
        ] {
            assert_eq!(
                run(&[0x22, 0x23, opcode, 0xac], "(FF)I", 2, 2, &[left, right]).unwrap(),
                returned(Value::Int(expected))
            );
        }
    }
    for opcode in [0x97, 0x98] {
        for (left, right, expected) in [
            (
                Value::Double(0x7ff8_0000_0000_0042),
                Value::from_f64(1.0),
                if opcode == 0x97 { -1 } else { 1 },
            ),
            (
                Value::from_f64(1.0),
                Value::Double(0x7ff8_0000_0000_0042),
                if opcode == 0x97 { -1 } else { 1 },
            ),
            (Value::from_f64(-0.0), Value::from_f64(0.0), 0),
            (Value::from_f64(1.0), Value::from_f64(-1.0), 1),
        ] {
            assert_eq!(
                run(&[0x26, 0x28, opcode, 0xac], "(DD)I", 4, 4, &[left, right]).unwrap(),
                returned(Value::Int(expected))
            );
        }
    }
}

#[test]
fn floating_conversions_saturate_truncate_and_round_without_double_rounding() {
    for (input, expected) in [
        (f32::NAN, 0),
        (f32::INFINITY, i32::MAX),
        (f32::NEG_INFINITY, i32::MIN),
        (-7.75, -7),
    ] {
        assert_eq!(
            run(&[0x22, 0x8b, 0xac], "(F)I", 1, 1, &[Value::from_f32(input)]).unwrap(),
            returned(Value::Int(expected))
        );
    }
    for (input, expected) in [
        (f64::NAN, 0),
        (f64::INFINITY, i64::MAX),
        (f64::NEG_INFINITY, i64::MIN),
        (-7.75, -7),
    ] {
        assert_eq!(
            run(&[0x26, 0x8f, 0xad], "(D)J", 2, 2, &[Value::from_f64(input)]).unwrap(),
            returned(Value::Long(expected))
        );
    }
    let long = (1_i64 << 62) + (1_i64 << 38) + 1;
    assert_eq!(
        run(&[0x1e, 0x89, 0xae], "(J)F", 2, 2, &[Value::Long(long)]).unwrap(),
        returned(Value::Float(0x5e80_0001))
    );
    assert_eq!(
        run(
            &[0x26, 0x90, 0xae],
            "(D)F",
            2,
            2,
            &[Value::from_f64(16_777_217.0)]
        )
        .unwrap(),
        returned(Value::from_f32(16_777_216.0))
    );
    assert_eq!(
        run(&[0x1a, 0x86, 0xae], "(I)F", 1, 1, &[Value::Int(16_777_217)]).unwrap(),
        returned(Value::from_f32(16_777_216.0))
    );
    assert_eq!(
        run(&[0x22, 0x8d, 0xaf], "(F)D", 2, 1, &[Value::Float(1)]).unwrap(),
        returned(Value::from_f64(f64::from(f32::from_bits(1))))
    );
}

#[test]
fn floating_operations_round_each_bytecode_and_negation_preserves_raw_payload() {
    assert_eq!(
        run(
            &[0x22, 0x23, 0x6a, 0x24, 0x62, 0xae],
            "(FFF)F",
            2,
            3,
            &[
                Value::Float(0x3f80_0001),
                Value::Float(0x3f7f_ffff),
                Value::from_f32(-1.0)
            ]
        )
        .unwrap(),
        returned(Value::from_f32(0.0))
    );
    assert_eq!(
        run(
            &[0x26, 0x28, 0x6b, 0x18, 4, 0x63, 0xaf],
            "(DDD)D",
            4,
            6,
            &[
                Value::Double(0x3ff0_0000_0000_0001),
                Value::Double(0x3fef_ffff_ffff_ffff),
                Value::from_f64(-1.0)
            ]
        )
        .unwrap(),
        returned(Value::from_f64(0.0))
    );
    for bits in [0, 0x8000_0000, 0x7fc1_2345, 0x7f80_0000] {
        assert_eq!(
            run(&[0x22, 0x76, 0xae], "(F)F", 1, 1, &[Value::Float(bits)]).unwrap(),
            returned(Value::Float(bits ^ 0x8000_0000))
        );
    }
    for bits in [
        0,
        0x8000_0000_0000_0000,
        0x7ff8_0000_0000_0042,
        0x7ff0_0000_0000_0000,
    ] {
        assert_eq!(
            run(&[0x26, 0x77, 0xaf], "(D)D", 2, 2, &[Value::Double(bits)]).unwrap(),
            returned(Value::Double(bits ^ 0x8000_0000_0000_0000))
        );
    }
}

#[test]
fn floating_constants_preserve_raw_bits_and_mismatched_return_types_fail() {
    let float = cp(4, &0x7fc1_2345u32.to_be_bytes());
    for code in [&[0x12, 8, 0xae][..], &[0x13, 0, 8, 0xae]] {
        let class = with_constants(code, "()F", 1, 0, std::slice::from_ref(&float));
        assert_eq!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default()).unwrap(),
            returned(Value::Float(0x7fc1_2345))
        );
    }
    let double = cp(6, &0x8000_0000_0000_0000u64.to_be_bytes());
    let class = with_constants(&[0x14, 0, 8, 0xaf], "()D", 2, 0, &[double]);
    assert_eq!(
        execute_static_method_body(&class, 0, &[], ExecutionOptions::default()).unwrap(),
        returned(Value::Double(0x8000_0000_0000_0000))
    );
    assert_eq!(
        run(&[0x0b, 0xae], "()D", 1, 0, &[]).unwrap_err().kind,
        ExecutionErrorKind::ReturnTypeMismatch
    );
    assert_eq!(
        run(&[0x0e, 0xaf], "()F", 2, 0, &[]).unwrap_err().kind,
        ExecutionErrorKind::ReturnTypeMismatch
    );
    assert!(matches!(
        run(&[0x03, 0x76, 0xae], "()F", 1, 0, &[]).unwrap_err().kind,
        ExecutionErrorKind::WrongValueType {
            expected: ValueKind::Float,
            actual: ValueKind::Int
        }
    ));
}

#[test]
fn root_call_policies_prevent_entry_and_reserve_full_sparse_capacities() {
    let mut class = synthetic(&[0xb1], "()V", 0, 0);
    code_mut(&mut class).code = vec![0xff];
    let error = execute_static_method_body_with_limits(
        &class,
        0,
        &[],
        ExecutionOptions::default(),
        CallLimits {
            max_frames: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ExecutionErrorKind::FrameLimit {
            limit: 0,
            required: 1
        }
    );
    let error = execute_static_method_body_with_limits(
        &class,
        0,
        &[],
        ExecutionOptions::default(),
        CallLimits {
            max_prepared_code_bytes: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ExecutionErrorKind::PreparedCodeLimit {
            limit: 0,
            required: 1
        }
    );
    let class = synthetic(&[0xb1], "()V", u16::MAX, u16::MAX);
    let required = usize::from(u16::MAX) * 2;
    let error = execute_static_method_body_with_limits(
        &class,
        0,
        &[],
        ExecutionOptions::default(),
        CallLimits {
            max_frame_slots: required - 1,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ExecutionErrorKind::FrameSlotLimit {
            limit: required - 1,
            required
        }
    );
    assert_eq!(
        execute_static_method_body_with_limits(
            &class,
            0,
            &[],
            ExecutionOptions::default(),
            CallLimits {
                max_frame_slots: required,
                ..Default::default()
            }
        )
        .unwrap(),
        ExecutionOutcome::Returned(None)
    );
}

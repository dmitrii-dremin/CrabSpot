//! Synthetic floating-point cases for encodings and frame states javac omits.
use crabspot_classfile::{ClassFile, parse};
use crabspot_interpreter::{
    ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome, Value, ValueKind,
    execute_static_method_body,
};

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
fn constant(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![tag];
    bytes.extend(payload);
    bytes
}
fn class(
    code: &[u8],
    descriptor: &str,
    stack: u16,
    locals: u16,
    constants: &[Vec<u8>],
) -> ClassFile {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    let slots: usize = constants
        .iter()
        .map(|entry| if matches!(entry[0], 5 | 6) { 2 } else { 1 })
        .sum();
    u2(&mut bytes, 8 + slots as u16);
    for entry in [
        utf8("SyntheticFloat"),
        vec![7, 0, 1],
        utf8("java/lang/Object"),
        vec![7, 0, 3],
        utf8("method"),
        utf8(descriptor),
        utf8("Code"),
    ] {
        bytes.extend(entry);
    }
    for entry in constants {
        bytes.extend(entry);
    }
    for value in [0x21, 2, 4, 0, 0, 1, 9, 5, 6, 1, 7] {
        u2(&mut bytes, value);
    }
    u4(&mut bytes, 12 + code.len() as u32);
    u2(&mut bytes, stack);
    u2(&mut bytes, locals);
    u4(&mut bytes, code.len() as u32);
    bytes.extend(code);
    for _ in 0..3 {
        u2(&mut bytes, 0);
    }
    parse(&bytes).unwrap()
}
fn run(
    code: &[u8],
    descriptor: &str,
    stack: u16,
    locals: u16,
    args: &[Value],
) -> Result<ExecutionOutcome, ExecutionError> {
    execute_static_method_body(
        &class(code, descriptor, stack, locals, &[]),
        0,
        args,
        ExecutionOptions::default(),
    )
}
fn returned(value: Value) -> ExecutionOutcome {
    ExecutionOutcome::Returned(Some(value))
}
fn binary_float(opcode: u8, left: u32, right: u32) -> Value {
    let ExecutionOutcome::Returned(Some(value)) = run(
        &[0x22, 0x23, opcode, 0xae],
        "(FF)F",
        2,
        2,
        &[Value::Float(left), Value::Float(right)],
    )
    .unwrap() else {
        panic!("expected a returned float")
    };
    value
}
fn binary_double(opcode: u8, left: u64, right: u64) -> Value {
    let ExecutionOutcome::Returned(Some(value)) = run(
        &[0x26, 0x28, opcode, 0xaf],
        "(DD)D",
        4,
        4,
        &[Value::Double(left), Value::Double(right)],
    )
    .unwrap() else {
        panic!("expected a returned double")
    };
    value
}

#[test]
fn values_expose_numeric_accessors_but_keep_representation_identity() {
    assert_eq!(Value::from_f32(-0.0), Value::Float(0x8000_0000));
    assert_eq!(Value::from_f64(-0.0), Value::Double(0x8000_0000_0000_0000));
    assert_eq!(Value::Float(1).as_f32().unwrap().to_bits(), 1);
    assert_eq!(Value::Double(1).as_f64().unwrap().to_bits(), 1);
    assert_eq!(Value::Float(0).kind(), ValueKind::Float);
    assert_eq!(Value::Double(0).kind(), ValueKind::Double);
    assert_eq!(Value::Float(0).slots(), 1);
    assert_eq!(Value::Double(0).slots(), 2);
    assert_eq!(Value::Float(0x7fc0_1234), Value::Float(0x7fc0_1234));
    assert_ne!(Value::Float(0), Value::Float(0x8000_0000));
    assert!(Value::Int(0).as_f32().is_none());
    assert!(Value::Long(0).as_f64().is_none());
    assert!(Value::Double(0).as_f32().is_none());
    assert!(Value::Float(0).as_f64().is_none());
}

#[test]
fn floating_constants_and_constant_pool_loads_preserve_raw_bits() {
    for (opcode, expected) in [(0x0b, 0), (0x0c, 0x3f80_0000), (0x0d, 0x4000_0000)] {
        assert_eq!(
            run(&[opcode, 0xae], "()F", 1, 0, &[]).unwrap(),
            returned(Value::Float(expected))
        );
    }
    for (opcode, expected) in [(0x0e, 0), (0x0f, 0x3ff0_0000_0000_0000)] {
        assert_eq!(
            run(&[opcode, 0xaf], "()D", 2, 0, &[]).unwrap(),
            returned(Value::Double(expected))
        );
    }
    for bits in [0_u32, 0x8000_0000, 1, 0x7fa0_1234, 0xffc0_5678] {
        for code in [vec![0x12, 8, 0xae], vec![0x13, 0, 8, 0xae]] {
            let class = class(&code, "()F", 1, 0, &[constant(4, &bits.to_be_bytes())]);
            assert_eq!(
                execute_static_method_body(&class, 0, &[], ExecutionOptions::default()).unwrap(),
                returned(Value::Float(bits))
            );
        }
    }
    for bits in [
        0_u64,
        0x8000_0000_0000_0000,
        1,
        0x7ff4_0000_0000_1234,
        0xfff8_0000_0000_5678,
    ] {
        let class = class(
            &[0x14, 0, 8, 0xaf],
            "()D",
            2,
            0,
            &[constant(6, &bits.to_be_bytes())],
        );
        assert_eq!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default()).unwrap(),
            returned(Value::Double(bits))
        );
    }
}

#[test]
fn float_and_double_arithmetic_produces_known_exact_results() {
    for (opcode, left, right, expected) in [
        (0x62, 0x3fc0_0000, 0x4010_0000, 0x4070_0000), // 1.5 + 2.25 = 3.75
        (0x66, 0x3fc0_0000, 0x4010_0000, 0xbf40_0000),
        (0x6a, 0x3fc0_0000, 0xc000_0000, 0xc040_0000),
        (0x6e, 0x40e0_0000, 0xc000_0000, 0xc060_0000),
        (0x72, 0x40e0_0000, 0x4000_0000, 0x3f80_0000),
        (0x72, 0xc0e0_0000, 0x4000_0000, 0xbf80_0000),
    ] {
        assert_eq!(binary_float(opcode, left, right), Value::Float(expected));
    }
    for (opcode, left, right, expected) in [
        (
            0x63,
            0x3ff8_0000_0000_0000,
            0x4002_0000_0000_0000,
            0x400e_0000_0000_0000,
        ),
        (
            0x67,
            0x3ff8_0000_0000_0000,
            0x4002_0000_0000_0000,
            0xbfe8_0000_0000_0000,
        ),
        (
            0x6b,
            0x3ff8_0000_0000_0000,
            0xc000_0000_0000_0000,
            0xc008_0000_0000_0000,
        ),
        (
            0x6f,
            0x401c_0000_0000_0000,
            0xc000_0000_0000_0000,
            0xc00c_0000_0000_0000,
        ),
        (
            0x73,
            0x401c_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x3ff0_0000_0000_0000,
        ),
        (
            0x73,
            0xc01c_0000_0000_0000,
            0x4000_0000_0000_0000,
            0xbff0_0000_0000_0000,
        ),
    ] {
        assert_eq!(binary_double(opcode, left, right), Value::Double(expected));
    }
}

#[test]
fn signed_zero_gradual_underflow_overflow_and_rounding_ties_are_preserved() {
    for (opcode, left, right, expected) in [
        (0x62, 0, 0x8000_0000, 0),
        (0x62, 0x8000_0000, 0x8000_0000, 0x8000_0000),
        (0x66, 0x8000_0000, 0, 0x8000_0000),
        (0x62, 1, 1, 2),
        (0x66, 0x0080_0000, 0x007f_ffff, 1),
        (0x6a, 0x0080_0000, 0x3f00_0000, 0x0040_0000),
        (0x6e, 1, 0x4000_0000, 0),
        (0x6e, 0x8000_0001, 0x4000_0000, 0x8000_0000),
        (0x6a, 0x7f7f_ffff, 0x4000_0000, 0x7f80_0000),
        (0x62, 0x3f80_0000, 0x3380_0000, 0x3f80_0000), // halfway rounds to even
        (0x62, 0x3f80_0001, 0x3380_0000, 0x3f80_0002),
    ] {
        assert_eq!(binary_float(opcode, left, right), Value::Float(expected));
    }
    for (opcode, left, right, expected) in [
        (0x63, 0, 0x8000_0000_0000_0000, 0),
        (
            0x63,
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
        ),
        (0x67, 0x8000_0000_0000_0000, 0, 0x8000_0000_0000_0000),
        (0x63, 1, 1, 2),
        (0x67, 0x0010_0000_0000_0000, 0x000f_ffff_ffff_ffff, 1),
        (
            0x6b,
            0x0010_0000_0000_0000,
            0x3fe0_0000_0000_0000,
            0x0008_0000_0000_0000,
        ),
        (0x6f, 1, 0x4000_0000_0000_0000, 0),
        (
            0x6f,
            0x8000_0000_0000_0001,
            0x4000_0000_0000_0000,
            0x8000_0000_0000_0000,
        ),
        (
            0x6b,
            0x7fef_ffff_ffff_ffff,
            0x4000_0000_0000_0000,
            0x7ff0_0000_0000_0000,
        ),
        (
            0x63,
            0x3ff0_0000_0000_0000,
            0x3ca0_0000_0000_0000,
            0x3ff0_0000_0000_0000,
        ),
        (
            0x63,
            0x3ff0_0000_0000_0001,
            0x3ca0_0000_0000_0000,
            0x3ff0_0000_0000_0002,
        ),
    ] {
        assert_eq!(binary_double(opcode, left, right), Value::Double(expected));
    }
}

#[test]
fn floating_zero_divisors_infinities_and_remainders_follow_java_rules() {
    assert_eq!(
        binary_float(0x6e, 0x3f80_0000, 0x8000_0000),
        Value::Float(0xff80_0000)
    );
    assert_eq!(
        binary_double(0x6f, 0x3ff0_0000_0000_0000, 0),
        Value::Double(0x7ff0_0000_0000_0000)
    );
    for (left, right) in [(0, 0), (0x7f80_0000, 0x7f80_0000)] {
        assert!(binary_float(0x6e, left, right).as_f32().unwrap().is_nan());
    }
    for (left, right) in [(0, 0), (0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000)] {
        assert!(binary_double(0x6f, left, right).as_f64().unwrap().is_nan());
    }
    for (left, right) in [
        (0x7f80_0000, 0x3f80_0000),
        (0x3f80_0000, 0),
        (0x7fc0_1234, 0x3f80_0000),
        (0x3f80_0000, 0x7fa0_5678),
    ] {
        assert!(binary_float(0x72, left, right).as_f32().unwrap().is_nan());
    }
    for (left, right) in [
        (0x7ff0_0000_0000_0000, 0x3ff0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 0),
        (0x7ff8_0000_0000_1234, 0x3ff0_0000_0000_0000),
        (0x3ff0_0000_0000_0000, 0x7ff4_0000_0000_5678),
    ] {
        assert!(binary_double(0x73, left, right).as_f64().unwrap().is_nan());
    }
    assert_eq!(
        binary_float(0x72, 0x8000_0000, 0x4000_0000),
        Value::Float(0x8000_0000)
    );
    assert_eq!(
        binary_double(0x73, 0x8000_0000_0000_0000, 0x4000_0000_0000_0000),
        Value::Double(0x8000_0000_0000_0000)
    );
    assert_eq!(
        binary_float(0x72, 0xc060_0000, 0x7f80_0000),
        Value::Float(0xc060_0000)
    );
    assert_eq!(
        binary_double(0x73, 0xc00c_0000_0000_0000, 0x7ff0_0000_0000_0000),
        Value::Double(0xc00c_0000_0000_0000)
    );
}

#[test]
fn negation_distinguishes_both_zeros_and_changes_finite_and_infinity_signs() {
    for bits in [0_u32, 0x8000_0000, 1, 0x7f80_0000, 0xff80_0000] {
        assert_eq!(
            run(&[0x22, 0x76, 0xae], "(F)F", 1, 1, &[Value::Float(bits)]).unwrap(),
            returned(Value::Float(bits ^ 0x8000_0000))
        );
    }
    for bits in [
        0_u64,
        0x8000_0000_0000_0000,
        1,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
    ] {
        assert_eq!(
            run(&[0x26, 0x77, 0xaf], "(D)D", 2, 2, &[Value::Double(bits)]).unwrap(),
            returned(Value::Double(bits ^ 0x8000_0000_0000_0000))
        );
    }
}

#[test]
fn compare_instructions_handle_ordering_both_nan_directions_and_signed_zero() {
    for (opcode, unordered) in [(0x95, -1), (0x96, 1)] {
        for (left, right, expected) in [
            (0x3f80_0000, 0x4000_0000, -1),
            (0x4000_0000, 0x3f80_0000, 1),
            (0, 0x8000_0000, 0),
            (0x8000_0000, 0, 0),
            (0x7fc0_1234, 0x3f80_0000, unordered),
            (0x3f80_0000, 0x7fa0_5678, unordered),
        ] {
            assert_eq!(
                run(
                    &[0x22, 0x23, opcode, 0xac],
                    "(FF)I",
                    2,
                    2,
                    &[Value::Float(left), Value::Float(right)]
                )
                .unwrap(),
                returned(Value::Int(expected))
            );
        }
    }
    for (opcode, unordered) in [(0x97, -1), (0x98, 1)] {
        for (left, right, expected) in [
            (0x3ff0_0000_0000_0000, 0x4000_0000_0000_0000, -1),
            (0x4000_0000_0000_0000, 0x3ff0_0000_0000_0000, 1),
            (0, 0x8000_0000_0000_0000, 0),
            (0x8000_0000_0000_0000, 0, 0),
            (0x7ff8_0000_0000_1234, 0x3ff0_0000_0000_0000, unordered),
            (0x3ff0_0000_0000_0000, 0x7ff4_0000_0000_5678, unordered),
        ] {
            assert_eq!(
                run(
                    &[0x26, 0x28, opcode, 0xac],
                    "(DD)I",
                    4,
                    4,
                    &[Value::Double(left), Value::Double(right)]
                )
                .unwrap(),
                returned(Value::Int(expected))
            );
        }
    }
}

#[test]
fn fixed_indexed_and_wide_float_double_locals_preserve_payloads() {
    let float = Value::Float(0xffa0_1234);
    let double = Value::Double(0xfff4_0000_0000_5678);
    for index in 0..4 {
        assert_eq!(
            run(
                &[0x22, 0x43 + index, 0x22 + index, 0xae],
                "(F)F",
                1,
                4,
                &[float]
            )
            .unwrap(),
            returned(float)
        );
        assert_eq!(
            run(
                &[0x26, 0x47 + index, 0x26 + index, 0xaf],
                "(D)D",
                2,
                5,
                &[double]
            )
            .unwrap(),
            returned(double)
        );
    }
    assert_eq!(
        run(&[0x17, 0, 0x38, 7, 0x17, 7, 0xae], "(F)F", 1, 8, &[float]).unwrap(),
        returned(float)
    );
    assert_eq!(
        run(&[0x18, 0, 0x39, 7, 0x18, 7, 0xaf], "(D)D", 2, 9, &[double]).unwrap(),
        returned(double)
    );
    assert_eq!(
        run(
            &[0x22, 0xc4, 0x38, 1, 44, 0xc4, 0x17, 1, 44, 0xae],
            "(F)F",
            1,
            301,
            &[float]
        )
        .unwrap(),
        returned(float)
    );
    assert_eq!(
        run(
            &[0x26, 0xc4, 0x39, 1, 44, 0xc4, 0x18, 1, 44, 0xaf],
            "(D)D",
            2,
            302,
            &[double]
        )
        .unwrap(),
        returned(double)
    );
    // Mixed arguments occupy slots F=0, I=1, D=2..3, J=4..5, F=6.
    assert_eq!(
        run(
            &[0x28, 0xaf],
            "(FIDJF)D",
            2,
            7,
            &[float, Value::Int(2), double, Value::Long(4), float]
        )
        .unwrap(),
        returned(double)
    );
    assert_eq!(
        run(
            &[0x17, 6, 0xae],
            "(FIDJF)F",
            1,
            7,
            &[float, Value::Int(2), double, Value::Long(4), float]
        )
        .unwrap(),
        returned(float)
    );
}

#[test]
fn double_and_mixed_long_double_local_overlaps_invalidate_the_previous_pair() {
    for store in [0x0b, 0x03] {
        // overwriting either half with a category-one value
        for slot in [0, 1] {
            let opcode = if store == 0x0b { 0x38 } else { 0x36 };
            let error = run(
                &[store, opcode, slot, 0x26, 0xaf],
                "(D)D",
                2,
                2,
                &[Value::Double(1)],
            )
            .unwrap_err();
            if slot == 0 {
                assert!(matches!(
                    error.kind,
                    E::WrongValueType {
                        expected: ValueKind::Double,
                        ..
                    }
                ));
            } else {
                assert!(matches!(error.kind, E::LocalNotValue { index: 0 }));
            }
        }
    }
    // A Double at slots1..2 overwrites the tail of an incoming Long0..1.
    assert!(matches!(
        run(&[0x0e, 0x48, 0x1e, 0xad], "(J)J", 2, 3, &[Value::Long(7)])
            .unwrap_err()
            .kind,
        E::LocalNotValue { index: 0 }
    ));
    // A Long at slots1..2 overwrites the tail of an incoming Double0..1.
    assert!(matches!(
        run(&[0x09, 0x40, 0x26, 0xaf], "(D)D", 2, 3, &[Value::Double(1)])
            .unwrap_err()
            .kind,
        E::LocalNotValue { index: 0 }
    ));
    assert!(matches!(
        run(&[0x18, 1, 0xaf], "(D)D", 2, 3, &[Value::Double(1)])
            .unwrap_err()
            .kind,
        E::LocalNotValue { index: 1 }
    ));
}

#[test]
fn floating_values_follow_category_based_stack_manipulation() {
    let float = Value::Float(0xffa0_1234);
    let double = Value::Double(0xfff4_0000_0000_5678);
    // dup / swap with category-one float values preserve all payload bits.
    assert_eq!(
        run(&[0x22, 0x59, 0x57, 0xae], "(F)F", 2, 1, &[float]).unwrap(),
        returned(float)
    );
    assert_eq!(
        run(&[0x22, 0x0b, 0x5f, 0xae], "(F)F", 2, 1, &[float]).unwrap(),
        returned(float)
    );
    // dup2's category-two form, followed by pop2, retains the original Double.
    assert_eq!(
        run(&[0x26, 0x5c, 0x58, 0xaf], "(D)D", 4, 2, &[double]).unwrap(),
        returned(double)
    );
    // dup_x2 category-two lower value: D,F -> F,D,F -> F,D -> F.
    assert_eq!(
        run(
            &[0x26, 0x24, 0x5b, 0x57, 0x58, 0xae],
            "(DF)F",
            4,
            3,
            &[double, float]
        )
        .unwrap(),
        returned(float)
    );
    assert!(matches!(
        run(&[0x26, 0x59, 0xaf], "(D)D", 4, 2, &[double])
            .unwrap_err()
            .kind,
        E::InvalidStackCategory { opcode: 0x59 }
    ));
    assert!(matches!(
        run(&[0x26, 0x57, 0xb1], "(D)V", 2, 2, &[double])
            .unwrap_err()
            .kind,
        E::InvalidStackCategory { opcode: 0x57 }
    ));
}

#[test]
fn floating_frames_constants_and_returns_reject_invalid_kinds_and_capacities() {
    assert!(matches!(
        run(&[0x22, 0xae], "(F)F", 1, 1, &[Value::Int(0)])
            .unwrap_err()
            .kind,
        E::ArgumentType {
            expected: ValueKind::Float,
            actual: ValueKind::Int,
            ..
        }
    ));
    assert!(matches!(
        run(&[0x26, 0xaf], "(D)D", 2, 2, &[Value::Long(0)])
            .unwrap_err()
            .kind,
        E::ArgumentType {
            expected: ValueKind::Double,
            actual: ValueKind::Long,
            ..
        }
    ));
    for (code, descriptor, stack, locals, args) in [
        (
            vec![0x22, 0x0b, 0x62, 0xae],
            "(I)F",
            2,
            1,
            vec![Value::Int(1)],
        ),
        (
            vec![0x26, 0x0e, 0x63, 0xaf],
            "(J)D",
            4,
            2,
            vec![Value::Long(1)],
        ),
    ] {
        assert!(matches!(
            run(&code, descriptor, stack, locals, &args)
                .unwrap_err()
                .kind,
            E::WrongValueType { .. }
        ));
    }
    assert!(matches!(
        run(&[0x0e, 0xaf], "()D", 1, 0, &[]).unwrap_err().kind,
        E::StackOverflow {
            required: 2,
            capacity: 1
        }
    ));
    assert!(matches!(
        run(&[0x26, 0x39, 1, 0xaf], "(D)D", 2, 2, &[Value::Double(0)])
            .unwrap_err()
            .kind,
        E::LocalOutOfBounds {
            index: 1,
            width: 2,
            capacity: 2
        }
    ));
    assert!(matches!(
        run(&[0x62, 0xae], "()F", 2, 0, &[]).unwrap_err().kind,
        E::StackUnderflow
    ));
    for (code, descriptor) in [
        (vec![0x0b, 0xae], "()D"),
        (vec![0x0e, 0xaf], "()F"),
        (vec![0x0b, 0xac], "()I"),
        (vec![0x0e, 0xad], "()J"),
    ] {
        assert!(matches!(
            run(&code, descriptor, 2, 0, &[]).unwrap_err().kind,
            E::ReturnTypeMismatch | E::WrongValueType { .. }
        ));
    }
    for (code, tag, payload) in [
        (vec![0x12, 8, 0xae], 6, 0_u64.to_be_bytes().to_vec()),
        (vec![0x14, 0, 8, 0xaf], 4, 0_u32.to_be_bytes().to_vec()),
    ] {
        let class = class(&code, "()F", 2, 0, &[constant(tag, &payload)]);
        assert!(matches!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
                .unwrap_err()
                .kind,
            E::InvalidConstant { index: 8, .. }
        ));
    }
}

fn conversion(opcode: u8, input: Value, output: ValueKind) -> Value {
    let (load, source) = match input {
        Value::Int(_) => (0x1a, "I"),
        Value::Long(_) => (0x1e, "J"),
        Value::Float(_) => (0x22, "F"),
        Value::Double(_) => (0x26, "D"),
        Value::Reference(_) => panic!("floating conversion input must be numeric"),
    };
    let (ret, destination) = match output {
        ValueKind::Int => (0xac, "I"),
        ValueKind::Long => (0xad, "J"),
        ValueKind::Float => (0xae, "F"),
        ValueKind::Double => (0xaf, "D"),
        ValueKind::Reference => panic!("floating conversion output must be numeric"),
    };
    let descriptor = format!("({source}){destination}");
    let ExecutionOutcome::Returned(Some(value)) =
        run(&[load, opcode, ret], &descriptor, 2, 2, &[input]).unwrap()
    else {
        panic!("conversion did not return")
    };
    value
}

#[test]
fn float_to_integer_conversions_truncate_saturate_and_map_nan_to_zero() {
    for (bits, expected) in [
        (0x7fc0_1234, 0),
        (0xffa0_5678, 0),
        (0x8000_0000, 0),
        (0x4070_0000, 3),
        (0xc070_0000, -3),
        (0x7f80_0000, i32::MAX),
        (0xff80_0000, i32::MIN),
        (0x4f00_0000, i32::MAX),
        (0x4eff_ffff, 2_147_483_520),
        (0xcf00_0000, i32::MIN),
        (0xcf00_0001, i32::MIN),
        (1, 0),
        (0x8000_0001, 0),
    ] {
        assert_eq!(
            conversion(0x8b, Value::Float(bits), ValueKind::Int),
            Value::Int(expected)
        );
    }
    for (bits, expected) in [
        (0x7fc0_1234, 0),
        (0xffa0_5678, 0),
        (0x8000_0000, 0),
        (0x4070_0000, 3),
        (0xc070_0000, -3),
        (0x7f80_0000, i64::MAX),
        (0xff80_0000, i64::MIN),
        (0x5f00_0000, i64::MAX),
        (0x5eff_ffff, i64::MAX - (1_i64 << 39) + 1),
        (0xdf00_0000, i64::MIN),
        (0xdeff_ffff, i64::MIN + (1_i64 << 39)),
    ] {
        assert_eq!(
            conversion(0x8c, Value::Float(bits), ValueKind::Long),
            Value::Long(expected)
        );
    }
}

#[test]
fn double_to_integer_conversions_use_exact_adjacent_thresholds() {
    for (input, expected) in [
        (Value::Double(0x7ff8_0000_0000_1234), 0),
        (Value::Double(0xfff4_0000_0000_5678), 0),
        (Value::Double(0x8000_0000_0000_0000), 0),
        (Value::from_f64(3.75), 3),
        (Value::from_f64(-3.75), -3),
        (Value::from_f64(2_147_483_646.75), 2_147_483_646),
        (Value::from_f64(2_147_483_648.0), i32::MAX),
        (Value::from_f64(-2_147_483_649.0), i32::MIN),
        (Value::Double(0x7ff0_0000_0000_0000), i32::MAX),
        (Value::Double(0xfff0_0000_0000_0000), i32::MIN),
    ] {
        assert_eq!(
            conversion(0x8e, input, ValueKind::Int),
            Value::Int(expected)
        );
    }
    for (bits, expected) in [
        (0x7ff8_0000_0000_1234, 0),
        (0xfff4_0000_0000_5678, 0),
        (0x400e_0000_0000_0000, 3),
        (0xc00e_0000_0000_0000, -3),
        (0x7ff0_0000_0000_0000, i64::MAX),
        (0xfff0_0000_0000_0000, i64::MIN),
        (0x43e0_0000_0000_0000, i64::MAX),
        (0x43df_ffff_ffff_ffff, i64::MAX - 1023),
        (0xc3e0_0000_0000_0000, i64::MIN),
        (0xc3df_ffff_ffff_ffff, i64::MIN + 1024),
    ] {
        assert_eq!(
            conversion(0x8f, Value::Double(bits), ValueKind::Long),
            Value::Long(expected)
        );
    }
}

#[test]
fn integer_to_floating_conversions_round_ties_to_even_without_double_rounding() {
    for (input, bits) in [
        (16_777_217, 0x4b80_0000),
        (16_777_219, 0x4b80_0002),
        (i32::MAX, 0x4f00_0000),
        (i32::MIN, 0xcf00_0000),
    ] {
        assert_eq!(
            conversion(0x86, Value::Int(input), ValueKind::Float),
            Value::Float(bits)
        );
    }
    assert_eq!(
        conversion(0x87, Value::Int(i32::MAX), ValueKind::Double),
        Value::Double(0x41df_ffff_ffc0_0000)
    );
    for (input, bits) in [
        ((1_i64 << 53) + 1, 0x4340_0000_0000_0000),
        ((1_i64 << 53) + 3, 0x4340_0000_0000_0002),
    ] {
        assert_eq!(
            conversion(0x8a, Value::Long(input), ValueKind::Double),
            Value::Double(bits)
        );
    }
    // x lies just above a binary32 midpoint. A preliminary binary64 conversion
    // erases its final bit and would incorrectly select the even lower float.
    let counterexample = (1_i64 << 62) + (1_i64 << 38) + 1;
    assert_eq!(
        conversion(0x89, Value::Long(counterexample), ValueKind::Float),
        Value::Float(0x5e80_0001)
    );
    assert_eq!(
        conversion(0x89, Value::Long(-counterexample), ValueKind::Float),
        Value::Float(0xde80_0001)
    );
    assert_eq!(
        conversion(
            0x90,
            conversion(0x8a, Value::Long(counterexample), ValueKind::Double),
            ValueKind::Float
        ),
        Value::Float(0x5e80_0000)
    );
}

#[test]
fn float_double_conversions_preserve_zero_underflow_ties_and_classify_nan() {
    for (input, output) in [
        (0_u32, 0_u64),
        (0x8000_0000, 0x8000_0000_0000_0000),
        (1, 0x36a0_0000_0000_0000),
        (0x7f7f_ffff, 0x47ef_ffff_e000_0000),
        (0x7f80_0000, 0x7ff0_0000_0000_0000),
    ] {
        assert_eq!(
            conversion(0x8d, Value::Float(input), ValueKind::Double),
            Value::Double(output)
        );
    }
    for (input, output) in [
        (0_u64, 0_u32),
        (0x8000_0000_0000_0000, 0x8000_0000),
        (0x3690_0000_0000_0000, 0),
        (0x3690_0000_0000_0001, 1),
        (0x36a0_0000_0000_0000, 1),
        (0x3ff0_0000_1000_0000, 0x3f80_0000),
        (0x3ff0_0000_1000_0001, 0x3f80_0001),
        (0x7fef_ffff_ffff_ffff, 0x7f80_0000),
    ] {
        assert_eq!(
            conversion(0x90, Value::Double(input), ValueKind::Float),
            Value::Float(output)
        );
    }
    assert!(
        conversion(0x8d, Value::Float(0x7fa0_1234), ValueKind::Double)
            .as_f64()
            .unwrap()
            .is_nan()
    );
    assert!(
        conversion(0x90, Value::Double(0xfff4_0000_0000_5678), ValueKind::Float)
            .as_f32()
            .unwrap()
            .is_nan()
    );
}

#[test]
fn arithmetic_rounds_each_instruction_without_fusing_multiply_and_add() {
    // (1+2^-p)*(1-2^-p) is 1-2^(-2p), which rounds to 1 at the
    // instruction's width. The subsequent addition of -1 must return +0.
    assert_eq!(
        run(
            &[0x22, 0x23, 0x6a, 0x24, 0x62, 0xae],
            "(FFF)F",
            2,
            3,
            &[
                Value::Float(0x3f80_0001),
                Value::Float(0x3f7f_fffe),
                Value::Float(0xbf80_0000)
            ]
        )
        .unwrap(),
        returned(Value::Float(0))
    );
    assert_eq!(
        run(
            &[0x26, 0x28, 0x6b, 0x18, 4, 0x63, 0xaf],
            "(DDD)D",
            4,
            6,
            &[
                Value::Double(0x3ff0_0000_0000_0001),
                Value::Double(0x3fef_ffff_ffff_fffe),
                Value::Double(0xbff0_0000_0000_0000)
            ]
        )
        .unwrap(),
        returned(Value::Double(0))
    );
}

#[test]
fn nan_arithmetic_is_checked_by_classification_across_all_operation_families() {
    for opcode in [0x62, 0x66, 0x6a, 0x6e, 0x72] {
        for (left, right) in [(0x7fa0_1234, 0x3f80_0000), (0x3f80_0000, 0xffc0_5678)] {
            assert!(binary_float(opcode, left, right).as_f32().unwrap().is_nan());
        }
    }
    for opcode in [0x63, 0x67, 0x6b, 0x6f, 0x73] {
        for (left, right) in [
            (0x7ff4_0000_0000_1234, 0x3ff0_0000_0000_0000),
            (0x3ff0_0000_0000_0000, 0xfff8_0000_0000_5678),
        ] {
            assert!(
                binary_double(opcode, left, right)
                    .as_f64()
                    .unwrap()
                    .is_nan()
            );
        }
    }
    for (code, descriptor, argument) in [
        (vec![0x22, 0x76, 0xae], "(F)F", Value::Float(0x7fa0_1234)),
        (
            vec![0x26, 0x77, 0xaf],
            "(D)D",
            Value::Double(0xfff4_0000_0000_5678),
        ),
    ] {
        let ExecutionOutcome::Returned(Some(value)) =
            run(&code, descriptor, 2, 2, &[argument]).unwrap()
        else {
            panic!("expected NaN return")
        };
        assert!(match value {
            Value::Float(bits) => f32::from_bits(bits).is_nan(),
            Value::Double(bits) => f64::from_bits(bits).is_nan(),
            _ => false,
        });
    }
}

#[test]
fn floating_operators_and_conversions_check_the_values_on_the_operand_stack() {
    for (opcode, prefix, expected, actual) in [
        (0x62, vec![0x03, 0x0b], ValueKind::Float, ValueKind::Int),
        (0x63, vec![0x09, 0x0e], ValueKind::Double, ValueKind::Long),
        (0x76, vec![0x03], ValueKind::Float, ValueKind::Int),
        (0x77, vec![0x09], ValueKind::Double, ValueKind::Long),
        (0x95, vec![0x03, 0x0b], ValueKind::Float, ValueKind::Int),
        (0x97, vec![0x09, 0x0e], ValueKind::Double, ValueKind::Long),
        (0x86, vec![0x0b], ValueKind::Int, ValueKind::Float),
        (0x87, vec![0x0b], ValueKind::Int, ValueKind::Float),
        (0x89, vec![0x0e], ValueKind::Long, ValueKind::Double),
        (0x8a, vec![0x0e], ValueKind::Long, ValueKind::Double),
        (0x8b, vec![0x03], ValueKind::Float, ValueKind::Int),
        (0x8c, vec![0x03], ValueKind::Float, ValueKind::Int),
        (0x8d, vec![0x03], ValueKind::Float, ValueKind::Int),
        (0x8e, vec![0x09], ValueKind::Double, ValueKind::Long),
        (0x8f, vec![0x09], ValueKind::Double, ValueKind::Long),
        (0x90, vec![0x09], ValueKind::Double, ValueKind::Long),
    ] {
        let mut code = prefix;
        let offset = code.len() as u32;
        code.extend([opcode, 0xb1]);
        let error = run(&code, "()V", 4, 0, &[]).unwrap_err();
        assert_eq!(error.offset, Some(offset));
        assert_eq!(error.kind, E::WrongValueType { expected, actual });
    }
}

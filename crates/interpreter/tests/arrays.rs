use crabspot_classfile::{ClassFile, ParseOptions, parse_with_options};
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_static_method_body, execute_static_method_body_with_heap,
    execute_static_method_body_with_limits,
};
use crabspot_runtime::{
    HeapError, HeapOptions, HeapResource, ManagedHeap, PrimitiveArrayKind as K, Reference,
};

fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}
struct Method {
    name: u16,
    descriptor: u16,
    code: Vec<u8>,
    stack: u16,
    locals: u16,
}
struct Program {
    pool: Vec<Vec<u8>>,
    methods: Vec<Method>,
    owner: u16,
    code_name: u16,
}
impl Program {
    fn new() -> Self {
        let mut program = Self {
            pool: Vec::new(),
            methods: Vec::new(),
            owner: 0,
            code_name: 0,
        };
        let name = program.utf8("SyntheticArrays");
        program.owner = program.constant(7, &words(&[name]));
        let name = program.utf8("java/lang/Object");
        assert_eq!(program.constant(7, &words(&[name])), 4);
        program.code_name = program.utf8("Code");
        program
    }
    fn constant(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let mut bytes = vec![tag];
        bytes.extend(payload);
        self.pool.push(bytes);
        self.pool.len() as u16
    }
    fn utf8(&mut self, text: &str) -> u16 {
        let mut bytes = words(&[text.len() as u16]);
        bytes.extend(text.bytes());
        self.constant(1, &bytes)
    }
    fn method(&mut self, name: &str, descriptor: &str, code: &[u8], stack: u16, locals: u16) {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        self.methods.push(Method {
            name,
            descriptor,
            code: code.to_vec(),
            stack,
            locals,
        });
    }
    fn call(&mut self, name: &str, descriptor: &str) -> Vec<u8> {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let nat = self.constant(12, &words(&[name, descriptor]));
        let index = self.constant(10, &words(&[self.owner, nat]));
        let mut code = vec![0xb8];
        code.extend(index.to_be_bytes());
        code
    }
    fn finish(self) -> ClassFile {
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        bytes.extend(words(&[(self.pool.len() + 1) as u16]));
        bytes.extend(self.pool.into_iter().flatten());
        bytes.extend(words(&[
            0x21,
            self.owner,
            4,
            0,
            0,
            self.methods.len() as u16,
        ]));
        for method in self.methods {
            bytes.extend(words(&[
                9,
                method.name,
                method.descriptor,
                1,
                self.code_name,
            ]));
            bytes.extend((12 + method.code.len() as u32).to_be_bytes());
            bytes.extend(words(&[method.stack, method.locals]));
            bytes.extend((method.code.len() as u32).to_be_bytes());
            bytes.extend(method.code);
            bytes.extend(words(&[0, 0]));
        }
        bytes.extend(words(&[0]));
        parse_with_options(&bytes, ParseOptions::default()).unwrap()
    }
}
fn method(code: &[u8], descriptor: &str, stack: u16, locals: u16) -> ClassFile {
    let mut program = Program::new();
    program.method("root", descriptor, code, stack, locals);
    program.finish()
}
fn run(
    heap: &mut ManagedHeap,
    code: &[u8],
    descriptor: &str,
    stack: u16,
    locals: u16,
    args: &[Value],
) -> Result<O, ExecutionError> {
    execute_static_method_body_with_heap(
        &method(code, descriptor, stack, locals),
        0,
        args,
        ExecutionOptions::default(),
        CallLimits::default(),
        heap,
    )
}
fn heap() -> ManagedHeap {
    ManagedHeap::new(HeapOptions::default()).unwrap()
}
fn returned(value: Value) -> O {
    O::Returned(Some(value))
}
fn branch(mut prefix: Vec<u8>, opcode: u8) -> Vec<u8> {
    prefix.extend([opcode, 0, 5, 0x03, 0xac, 0x04, 0xac]);
    prefix
}

#[test]
fn every_newarray_kind_uses_the_matching_load_store_and_default_value() {
    for (atype, kind, load, store, descriptor, value, push, ret, width) in [
        (
            4,
            K::Boolean,
            0x33,
            0x54,
            "(I)I",
            Value::Int(3),
            vec![0x1a],
            0xac,
            1,
        ),
        (
            8,
            K::Byte,
            0x33,
            0x54,
            "(I)I",
            Value::Int(255),
            vec![0x1a],
            0xac,
            1,
        ),
        (
            5,
            K::Char,
            0x34,
            0x55,
            "(I)I",
            Value::Int(-1),
            vec![0x1a],
            0xac,
            1,
        ),
        (
            9,
            K::Short,
            0x35,
            0x56,
            "(I)I",
            Value::Int(32768),
            vec![0x1a],
            0xac,
            1,
        ),
        (
            10,
            K::Int,
            0x2e,
            0x4f,
            "(I)I",
            Value::Int(i32::MIN),
            vec![0x1a],
            0xac,
            1,
        ),
        (
            11,
            K::Long,
            0x2f,
            0x50,
            "(J)J",
            Value::Long(i64::MIN),
            vec![0x1e],
            0xad,
            2,
        ),
        (
            6,
            K::Float,
            0x30,
            0x51,
            "(F)F",
            Value::Float(0xffa0_1234),
            vec![0x22],
            0xae,
            1,
        ),
        (
            7,
            K::Double,
            0x31,
            0x52,
            "(D)D",
            Value::Double(0xfff4_0000_0000_5678),
            vec![0x26],
            0xaf,
            2,
        ),
    ] {
        let mut heap = heap();
        let mut code = vec![0x04, 0xbc, atype, 0x4e, 0x2d, 0x03]; // array stored in local3
        code.extend(push);
        code.extend([store, 0x2d, 0x03, load, ret]);
        let expected = match kind {
            K::Boolean => Value::Int(1),
            K::Byte => Value::Int(-1),
            K::Char => Value::Int(65535),
            K::Short => Value::Int(-32768),
            _ => value,
        };
        assert_eq!(
            run(&mut heap, &code, descriptor, 2 + width, 4, &[value]).unwrap(),
            returned(expected)
        );
        assert_eq!(heap.allocated_arrays(), 1);
        assert_eq!(heap.total_elements(), 1);
        assert_eq!(heap.payload_bytes(), kind.element_bytes());
        let code = vec![0x04, 0xbc, atype, 0x03, load, ret];
        let descriptor = if width == 2 && kind == K::Long {
            "()J"
        } else if kind == K::Float {
            "()F"
        } else if kind == K::Double {
            "()D"
        } else {
            "()I"
        };
        let default = match kind {
            K::Long => Value::Long(0),
            K::Float => Value::Float(0),
            K::Double => Value::Double(0),
            _ => Value::Int(0),
        };
        assert_eq!(
            run(&mut heap, &code, descriptor, 2, 0, &[]).unwrap(),
            returned(default)
        );
    }
}

#[test]
fn reference_locals_cover_every_fixed_indexed_and_wide_form() {
    for index in 0..4 {
        let mut heap = heap();
        assert_eq!(
            run(
                &mut heap,
                &[0x06, 0xbc, 8, 0x4b + index, 0x2a + index, 0xbe, 0xac],
                "()I",
                1,
                4,
                &[]
            )
            .unwrap(),
            returned(Value::Int(3))
        );
    }
    for (store, load, locals) in [
        (vec![0x3a, 7], vec![0x19, 7], 8),
        (vec![0xc4, 0x3a, 1, 44], vec![0xc4, 0x19, 1, 44], 301),
    ] {
        let mut code = vec![0x06, 0xbc, 10];
        code.extend(store);
        code.extend(load);
        code.extend([0xbe, 0xac]);
        assert_eq!(
            run(&mut heap(), &code, "()I", 1, locals, &[]).unwrap(),
            returned(Value::Int(3))
        );
    }
}

#[test]
fn identity_and_null_branches_cover_both_outcomes_without_comparing_contents() {
    for (prefix, equal) in [
        (vec![0x01, 0x01], true),
        (vec![0x03, 0xbc, 10, 0x59], true),
        (vec![0x03, 0xbc, 10, 0x03, 0xbc, 10], false),
        (vec![0x01, 0x03, 0xbc, 10], false),
    ] {
        for (opcode, expected) in [(0xa5, i32::from(equal)), (0xa6, i32::from(!equal))] {
            assert_eq!(
                run(
                    &mut heap(),
                    &branch(prefix.clone(), opcode),
                    "()I",
                    2,
                    0,
                    &[]
                )
                .unwrap(),
                returned(Value::Int(expected))
            );
        }
    }
    for (prefix, null) in [(vec![0x01], true), (vec![0x03, 0xbc, 10], false)] {
        for (opcode, expected) in [(0xc6, i32::from(null)), (0xc7, i32::from(!null))] {
            assert_eq!(
                run(
                    &mut heap(),
                    &branch(prefix.clone(), opcode),
                    "()I",
                    1,
                    0,
                    &[]
                )
                .unwrap(),
                returned(Value::Int(expected))
            );
        }
    }
}

#[test]
fn reference_locals_invalidate_overlapping_category_two_values_and_check_types() {
    for (descriptor, arg, load, ret, kind) in [
        ("(J)J", Value::Long(9), 0x1e, 0xad, ValueKind::Long),
        ("(D)D", Value::Double(1), 0x26, 0xaf, ValueKind::Double),
    ] {
        for slot in [0, 1] {
            let error = run(
                &mut heap(),
                &[0x01, 0x3a, slot, load, ret],
                descriptor,
                2,
                2,
                &[arg],
            )
            .unwrap_err();
            assert_eq!(
                error.kind,
                if slot == 0 {
                    E::WrongValueType {
                        expected: kind,
                        actual: ValueKind::Reference,
                    }
                } else {
                    E::LocalNotValue { index: 0 }
                }
            );
        }
    }
    assert!(matches!(
        run(
            &mut heap(),
            &[0x01, 0x4c, 0x09, 0x3f, 0x2b, 0x57, 0xb1],
            "()V",
            2,
            2,
            &[]
        )
        .unwrap_err()
        .kind,
        E::LocalNotValue { index: 1 }
    ));
    for code in [vec![0x03, 0x4b, 0xb1], vec![0x1a, 0x19, 0, 0x57, 0xb1]] {
        assert!(matches!(
            run(&mut heap(), &code, "(I)V", 2, 1, &[Value::Int(0)])
                .unwrap_err()
                .kind,
            E::WrongValueType {
                expected: ValueKind::Reference,
                actual: ValueKind::Int
            }
        ));
    }
}

#[test]
fn references_participate_in_existing_category_based_stack_forms() {
    assert_eq!(
        run(
            &mut heap(),
            &branch(vec![0x01, 0x59], 0xa5),
            "()I",
            2,
            0,
            &[]
        )
        .unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(
        run(
            &mut heap(),
            &branch(vec![0x09, 0x01, 0x5b, 0x57, 0x58], 0xc6),
            "()I",
            4,
            0,
            &[]
        )
        .unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(
        run(
            &mut heap(),
            &branch(vec![0x01, 0x03, 0xbc, 8, 0x5f], 0xc6),
            "()I",
            2,
            0,
            &[]
        )
        .unwrap(),
        returned(Value::Int(1))
    );
}

#[test]
fn floating_stores_preserve_signed_zero_subnormals_and_nan_payloads() {
    for bits in [0_u32, 0x8000_0000, 1, 0x7fa0_1234, 0xffc0_5678] {
        assert_eq!(
            run(
                &mut heap(),
                &[0x04, 0xbc, 6, 0x59, 0x03, 0x22, 0x51, 0x03, 0x30, 0xae],
                "(F)F",
                4,
                1,
                &[Value::Float(bits)]
            )
            .unwrap(),
            returned(Value::Float(bits))
        );
    }
    for bits in [
        0_u64,
        0x8000_0000_0000_0000,
        1,
        0x7ff4_0000_0000_1234,
        0xfff8_0000_0000_5678,
    ] {
        assert_eq!(
            run(
                &mut heap(),
                &[0x04, 0xbc, 7, 0x59, 0x03, 0x26, 0x52, 0x03, 0x31, 0xaf],
                "(D)D",
                5,
                2,
                &[Value::Double(bits)]
            )
            .unwrap(),
            returned(Value::Double(bits))
        );
    }
}

#[test]
fn opcode_specific_array_kinds_are_checked_for_every_load_and_store_family() {
    for (atype, actual) in [
        (4, K::Boolean),
        (8, K::Byte),
        (5, K::Char),
        (9, K::Short),
        (10, K::Int),
        (11, K::Long),
        (6, K::Float),
        (7, K::Double),
    ] {
        for (load, store, expected, constant, pop) in [
            (0x2e, 0x4f, K::Int, 0x03, 0x57),
            (0x2f, 0x50, K::Long, 0x09, 0x58),
            (0x30, 0x51, K::Float, 0x0b, 0x57),
            (0x31, 0x52, K::Double, 0x0e, 0x58),
            (0x33, 0x54, K::Byte, 0x03, 0x57),
            (0x34, 0x55, K::Char, 0x03, 0x57),
            (0x35, 0x56, K::Short, 0x03, 0x57),
        ] {
            if actual == expected || (actual == K::Boolean && expected == K::Byte) {
                continue;
            }
            let error = run(
                &mut heap(),
                &[0x04, 0xbc, atype, 0x03, load, pop, 0xb1],
                "()V",
                4,
                0,
                &[],
            )
            .unwrap_err();
            assert_eq!(
                error.kind,
                E::WrongArrayKind {
                    opcode: load,
                    actual
                }
            );
            assert_eq!(error.offset, Some(4));
            let error = run(
                &mut heap(),
                &[0x04, 0xbc, atype, 0x03, constant, store, 0xb1],
                "()V",
                4,
                0,
                &[],
            )
            .unwrap_err();
            assert_eq!(
                error.kind,
                E::WrongArrayKind {
                    opcode: store,
                    actual
                }
            );
            assert_eq!(error.offset, Some(5));
        }
    }
}

#[test]
fn malformed_computational_values_remain_host_errors() {
    for (code, expected, actual) in [
        (
            vec![0x0b, 0xbc, 10, 0x57, 0xb1],
            ValueKind::Int,
            ValueKind::Float,
        ),
        (
            vec![0x03, 0xbe, 0x57, 0xb1],
            ValueKind::Reference,
            ValueKind::Int,
        ),
        (
            vec![0x03, 0x03, 0x2e, 0x57, 0xb1],
            ValueKind::Reference,
            ValueKind::Int,
        ),
        (
            vec![0x04, 0xbc, 10, 0x0b, 0x2e, 0x57, 0xb1],
            ValueKind::Int,
            ValueKind::Float,
        ),
        (
            vec![0x04, 0xbc, 10, 0x03, 0x0b, 0x4f, 0xb1],
            ValueKind::Int,
            ValueKind::Float,
        ),
        (
            branch(vec![0x03], 0xc6),
            ValueKind::Reference,
            ValueKind::Int,
        ),
        (
            branch(vec![0x01, 0x03], 0xa5),
            ValueKind::Reference,
            ValueKind::Int,
        ),
    ] {
        assert_eq!(
            run(&mut heap(), &code, "()V", 4, 0, &[]).unwrap_err().kind,
            E::WrongValueType { expected, actual }
        );
    }
    assert!(matches!(
        run(&mut heap(), &[0x2e, 0x57, 0xb1], "()V", 2, 0, &[])
            .unwrap_err()
            .kind,
        E::StackUnderflow
    ));
}

#[test]
fn null_precedes_bounds_and_negative_lengths_precede_heap_policy_limits() {
    for index in [i32::MIN, -1, 0, i32::MAX] {
        assert_eq!(
            run(
                &mut heap(),
                &[0x01, 0x1a, 0x2e, 0xac],
                "(I)I",
                2,
                1,
                &[Value::Int(index)]
            )
            .unwrap(),
            O::NullPointerException
        );
        assert_eq!(
            run(
                &mut heap(),
                &[0x01, 0x1a, 0x03, 0x4f, 0xb1],
                "(I)V",
                3,
                1,
                &[Value::Int(index)]
            )
            .unwrap(),
            O::NullPointerException
        );
    }
    assert_eq!(
        run(&mut heap(), &[0x01, 0xbe, 0xac], "()I", 1, 0, &[]).unwrap(),
        O::NullPointerException
    );
    for atype in 4..=11 {
        let mut heap = ManagedHeap::new(HeapOptions {
            max_arrays: 0,
            max_array_length: 0,
            max_total_elements: 0,
            max_payload_bytes: 0,
        })
        .unwrap();
        assert_eq!(
            run(
                &mut heap,
                &[0x02, 0xbc, atype, 0x57, 0xb1],
                "()V",
                1,
                0,
                &[]
            )
            .unwrap(),
            O::NegativeArraySizeException
        );
        assert_eq!(heap.allocated_arrays(), 0);
    }
}

#[test]
fn valid_array_signed_index_failures_are_symbolic_bounds_outcomes() {
    for index in [i32::MIN, -1, 1, i32::MAX] {
        assert_eq!(
            run(
                &mut heap(),
                &[0x04, 0xbc, 10, 0x1a, 0x2e, 0xac],
                "(I)I",
                2,
                1,
                &[Value::Int(index)]
            )
            .unwrap(),
            O::ArrayIndexOutOfBoundsException
        );
        assert_eq!(
            run(
                &mut heap(),
                &[0x04, 0xbc, 10, 0x1a, 0x03, 0x4f, 0xb1],
                "(I)V",
                3,
                1,
                &[Value::Int(index)]
            )
            .unwrap(),
            O::ArrayIndexOutOfBoundsException
        );
    }
    assert_eq!(
        run(
            &mut heap(),
            &[0x03, 0xbc, 10, 0x03, 0x2e, 0xac],
            "()I",
            2,
            0,
            &[]
        )
        .unwrap(),
        O::ArrayIndexOutOfBoundsException
    );
}

#[test]
fn heap_policy_failures_stay_structured_and_successful_effects_survive_every_exit() {
    let mut heap = ManagedHeap::new(HeapOptions {
        max_arrays: 2,
        ..HeapOptions::default()
    })
    .unwrap();
    let code = [0x04, 0xbc, 10, 0x59, 0x03, 0x10, 42, 0x4f, 0x03, 0x2e, 0xac];
    for expected_count in 1..=2 {
        assert_eq!(
            run(&mut heap, &code, "()I", 4, 0, &[]).unwrap(),
            returned(Value::Int(42))
        );
        assert_eq!(heap.allocated_arrays(), expected_count);
    }
    assert_eq!(
        run(&mut heap, &code, "()I", 4, 0, &[]).unwrap_err().kind,
        E::Heap(HeapError::ResourceLimit {
            resource: HeapResource::Arrays,
            limit: 2,
            required: 3
        })
    );
    assert_eq!(heap.total_elements(), 2);
    assert_eq!(heap.payload_bytes(), 8);
    for (suffix, expected) in [
        (vec![0x01, 0xbe, 0x57, 0xb1], O::NullPointerException),
        (
            vec![0x2a, 0x04, 0x2e, 0x57, 0xb1],
            O::ArrayIndexOutOfBoundsException,
        ),
        (
            vec![0x02, 0xbc, 8, 0x57, 0xb1],
            O::NegativeArraySizeException,
        ),
        (vec![0x04, 0x03, 0x6c, 0x57, 0xb1], O::ArithmeticException),
    ] {
        let mut heap = heap_with_retained_array();
        let mut code = vec![0x04, 0xbc, 10, 0x59, 0x03, 0x10, 42, 0x4f, 0x4b];
        code.extend(suffix);
        assert_eq!(run(&mut heap.0, &code, "()V", 4, 1, &[]).unwrap(), expected);
        assert_eq!(heap.0.allocated_arrays(), 2);
        assert_eq!(heap.0.total_elements(), 2);
        assert_eq!(heap.0.payload_bytes(), 8);
        assert_eq!(heap.0.load(heap.1, 0).unwrap(), Value::Int(17));
    }
    let mut retained = heap_with_retained_array();
    let code = [
        0x04, 0xbc, 10, 0x59, 0x03, 0x10, 42, 0x4f, 0x57, 0xbb, 0, 2, 0x57, 0xb1,
    ];
    assert_eq!(
        run(&mut retained.0, &code, "()V", 4, 0, &[])
            .unwrap_err()
            .kind,
        E::UnsupportedInstruction(0xbb)
    );
    assert_eq!(retained.0.allocated_arrays(), 2);
    assert_eq!(retained.0.total_elements(), 2);
    assert_eq!(retained.0.payload_bytes(), 8);
    assert_eq!(retained.0.load(retained.1, 0).unwrap(), Value::Int(17));
}

fn heap_with_retained_array() -> (ManagedHeap, Reference) {
    let mut heap = heap();
    let reference = heap.allocate_primitive_array(K::Int, 1).unwrap();
    heap.store(reference, 0, Value::Int(17)).unwrap();
    (heap, reference)
}

#[test]
fn exact_instruction_fuel_charges_array_and_local_instructions_once() {
    let class = method(&[0x04, 0xbc, 10, 0x4b, 0x2a, 0xbe, 0xac], "()I", 1, 1);
    for fuel in 0..=6 {
        let mut heap = heap();
        let result = execute_static_method_body_with_heap(
            &class,
            0,
            &[],
            ExecutionOptions {
                instruction_budget: fuel,
            },
            CallLimits::default(),
            &mut heap,
        );
        if fuel == 6 {
            assert_eq!(result.unwrap(), returned(Value::Int(1)));
        } else {
            assert_eq!(
                result.unwrap_err().kind,
                E::InstructionBudgetExhausted { limit: fuel }
            );
        }
        assert_eq!(heap.allocated_arrays(), usize::from(fuel >= 2));
    }
}

#[test]
fn one_heap_is_shared_across_call_frames_and_its_failures_have_method_context() {
    let mut program = Program::new();
    let mut code = program.call("leaf", "()I");
    code.extend([0x04, 0xbc, 8, 0xbe, 0x60, 0xac]);
    program.method("root", "()I", &code, 2, 0);
    program.method("leaf", "()I", &[0x05, 0xbc, 10, 0xbe, 0xac], 1, 0);
    let class = program.finish();
    let mut allowed = ManagedHeap::new(HeapOptions {
        max_arrays: 2,
        ..HeapOptions::default()
    })
    .unwrap();
    assert_eq!(
        execute_static_method_body_with_heap(
            &class,
            0,
            &[],
            ExecutionOptions::default(),
            CallLimits::default(),
            &mut allowed
        )
        .unwrap(),
        returned(Value::Int(3))
    );
    assert_eq!(allowed.allocated_arrays(), 2);
    assert_eq!(allowed.total_elements(), 3);
    assert_eq!(allowed.payload_bytes(), 9);
    let mut bounded = ManagedHeap::new(HeapOptions {
        max_arrays: 1,
        ..HeapOptions::default()
    })
    .unwrap();
    let error = execute_static_method_body_with_heap(
        &class,
        0,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
        &mut bounded,
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        E::Heap(HeapError::ResourceLimit {
            resource: HeapResource::Arrays,
            limit: 1,
            required: 2
        })
    );
    assert_eq!(error.method.as_ref().unwrap().index, 0);
    assert_eq!(error.offset, Some(4));
    assert_eq!(bounded.allocated_arrays(), 1);
    assert_eq!(bounded.total_elements(), 2);
    let mut bounded = ManagedHeap::new(HeapOptions {
        max_payload_bytes: 0,
        ..HeapOptions::default()
    })
    .unwrap();
    let error = execute_static_method_body_with_heap(
        &class,
        0,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
        &mut bounded,
    )
    .unwrap_err();
    assert_eq!(error.method.as_ref().unwrap().index, 1);
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.call_site.as_ref().unwrap().method.index, 0);
    assert_eq!(error.call_site.as_ref().unwrap().offset, 0);
}

#[test]
fn uncaught_array_outcomes_propagate_from_descendants_without_continuing_callers() {
    for (body, expected) in [
        (vec![0x01, 0xbe, 0xac], O::NullPointerException),
        (
            vec![0x03, 0xbc, 10, 0x03, 0x2e, 0xac],
            O::ArrayIndexOutOfBoundsException,
        ),
        (
            vec![0x02, 0xbc, 10, 0xbe, 0xac],
            O::NegativeArraySizeException,
        ),
    ] {
        let mut program = Program::new();
        let mut code = program.call("leaf", "()I");
        code.extend([0x57, 0xbb, 0, 2, 0xac]); // unsupported new must not execute after the outcome
        program.method("root", "()I", &code, 1, 0);
        program.method("leaf", "()I", &body, 2, 0);
        assert_eq!(
            execute_static_method_body_with_heap(
                &program.finish(),
                0,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                &mut heap()
            )
            .unwrap(),
            expected
        );
    }
}

#[test]
fn old_numeric_entrypoints_and_unsupported_reference_features_remain_explicit() {
    for opcode in [
        0x01, 0x19, 0x3a, 0x2a, 0x2b, 0x2c, 0x2d, 0x4b, 0x4c, 0x4d, 0x4e, 0xbe, 0xbc, 0xa5, 0xa6,
        0xc6, 0xc7, 0x2e, 0x2f, 0x30, 0x31, 0x33, 0x34, 0x35, 0x4f, 0x50, 0x51, 0x52, 0x54, 0x55,
        0x56,
    ] {
        let code = if matches!(opcode, 0x19 | 0x3a) {
            vec![opcode, 0, 0xb1]
        } else if opcode == 0xbc {
            vec![opcode, 10, 0xb1]
        } else if matches!(opcode, 0xa5 | 0xa6 | 0xc6 | 0xc7) {
            vec![opcode, 0, 0, 0xb1]
        } else {
            vec![opcode, 0xb1]
        };
        let class = method(&code, "()V", 4, 4);
        assert_eq!(
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
                .unwrap_err()
                .kind,
            E::UnsupportedInstruction(opcode)
        );
        assert_eq!(
            execute_static_method_body_with_limits(
                &class,
                0,
                &[],
                ExecutionOptions::default(),
                CallLimits::default()
            )
            .unwrap_err()
            .kind,
            E::UnsupportedInstruction(opcode)
        );
    }
    for code in [
        vec![0x32, 0xb1],
        vec![0x53, 0xb1],
        vec![0xbd, 0, 2, 0xb1],
        vec![0xc5, 0, 2, 1, 0xb1],
        vec![0xb0, 0xb1],
    ] {
        let expected = code[0];
        assert_eq!(
            run(&mut heap(), &code, "()V", 4, 0, &[]).unwrap_err().kind,
            E::UnsupportedInstruction(expected)
        );
    }
    assert_eq!(
        run(&mut heap(), &[0xb1, 0xbc, 3], "()V", 1, 0, &[])
            .unwrap_err()
            .method
            .as_ref()
            .unwrap()
            .index,
        0
    );
    assert!(matches!(
        run(&mut heap(), &[0xb1, 0xbc, 3], "()V", 1, 0, &[])
            .unwrap_err()
            .kind,
        E::Decode(_)
    ));
    assert_eq!(
        run(
            &mut heap(),
            &[0xb1],
            "([I)V",
            0,
            1,
            &[Value::Reference(Reference::null())]
        )
        .unwrap_err()
        .kind,
        E::UnsupportedDescriptor
    );
}

#[test]
fn heap_entrypoint_preserves_call_limit_and_shared_fuel_boundaries() {
    let mut program = Program::new();
    let mut code = program.call("leaf", "()I");
    code.push(0xac);
    program.method("root", "()I", &code, 1, 0);
    program.method("leaf", "()I", &[0x04, 0xbc, 10, 0xbe, 0xac], 1, 0);
    let class = program.finish();
    let exact = CallLimits {
        max_frames: 2,
        max_frame_slots: 2,
        max_prepared_code_bytes: 9,
    };
    assert_eq!(
        execute_static_method_body_with_heap(
            &class,
            0,
            &[],
            ExecutionOptions {
                instruction_budget: 6
            },
            exact,
            &mut heap()
        )
        .unwrap(),
        returned(Value::Int(1))
    );
    for (limits, expected) in [
        (
            CallLimits {
                max_frames: 1,
                ..exact
            },
            E::FrameLimit {
                limit: 1,
                required: 2,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 1,
                ..exact
            },
            E::FrameSlotLimit {
                limit: 1,
                required: 2,
            },
        ),
        (
            CallLimits {
                max_prepared_code_bytes: 8,
                ..exact
            },
            E::PreparedCodeLimit {
                limit: 8,
                required: 9,
            },
        ),
    ] {
        let mut heap = heap();
        assert_eq!(
            execute_static_method_body_with_heap(
                &class,
                0,
                &[],
                ExecutionOptions::default(),
                limits,
                &mut heap
            )
            .unwrap_err()
            .kind,
            expected
        );
        assert_eq!(heap.allocated_arrays(), 0);
    }
    let mut heap = heap();
    let error = execute_static_method_body_with_heap(
        &class,
        0,
        &[],
        ExecutionOptions {
            instruction_budget: 5,
        },
        exact,
        &mut heap,
    )
    .unwrap_err();
    assert_eq!(error.kind, E::InstructionBudgetExhausted { limit: 5 });
    assert_eq!(error.method.as_ref().unwrap().index, 0);
    assert_eq!(error.offset, Some(3));
    assert_eq!(heap.allocated_arrays(), 1);
}

#[test]
fn heap_mode_still_rejects_reference_descriptors_in_callees() {
    for descriptor in ["([I)V", "()Ljava/lang/Object;"] {
        let mut program = Program::new();
        let mut code = program.call("reference", descriptor);
        code.push(0xb1);
        program.method("root", "()V", &code, 1, 0);
        program.method("reference", descriptor, &[0xb1], 1, 1);
        let mut heap = heap();
        let error = execute_static_method_body_with_heap(
            &program.finish(),
            0,
            &[],
            ExecutionOptions::default(),
            CallLimits::default(),
            &mut heap,
        )
        .unwrap_err();
        assert_eq!(error.kind, E::UnsupportedDescriptor);
        assert_eq!(error.method.as_ref().unwrap().index, 1);
        assert_eq!(error.call_site.as_ref().unwrap().offset, 0);
        assert_eq!(heap.allocated_arrays(), 0);
    }
}

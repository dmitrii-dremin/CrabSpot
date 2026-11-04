use crabspot_bytecode::Operands;
use crabspot_classfile::{AttributeBody, parse};
use crabspot_interpreter::{
    CallLimits, ExecutionOptions, ExecutionOutcome, Value, execute_static_method_body_with_heap,
};
use crabspot_runtime::{HeapOptions, ManagedHeap};
use std::collections::{BTreeMap, BTreeSet};

const CLASS: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/ArrayCases.class");
const REFERENCE: &str = include_str!("../../../tests/fixtures/reference/arrays.tsv");

fn input(text: &str) -> Value {
    let (kind, number) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(number.parse().unwrap()),
        "L" => Value::Long(number.parse().unwrap()),
        "F" => Value::Float(u32::from_str_radix(number, 16).unwrap()),
        "D" => Value::Double(u64::from_str_radix(number, 16).unwrap()),
        _ => panic!("unexpected oracle input {text}"),
    }
}
fn observed(outcome: ExecutionOutcome) -> String {
    match outcome {
        ExecutionOutcome::Returned(None) => "V".to_owned(),
        ExecutionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExecutionOutcome::Returned(Some(Value::Long(value))) => format!("L:{value}"),
        ExecutionOutcome::Returned(Some(Value::Float(bits))) if f32::from_bits(bits).is_nan() => {
            "F:NaN".to_owned()
        }
        ExecutionOutcome::Returned(Some(Value::Float(bits))) => format!("F:{bits:08x}"),
        ExecutionOutcome::Returned(Some(Value::Double(bits))) if f64::from_bits(bits).is_nan() => {
            "D:NaN".to_owned()
        }
        ExecutionOutcome::Returned(Some(Value::Double(bits))) => format!("D:{bits:016x}"),
        ExecutionOutcome::Returned(Some(Value::Reference(reference))) => {
            panic!("numeric fixture returned {reference:?}")
        }
        unexpected @ (ExecutionOutcome::NoSuchFieldError
        | ExecutionOutcome::IllegalAccessError
        | ExecutionOutcome::IncompatibleClassChangeError
        | ExecutionOutcome::NoSuchMethodError
        | ExecutionOutcome::AbstractMethodError
        | ExecutionOutcome::ClassCastException
        | ExecutionOutcome::ArrayStoreException) => {
            panic!("unexpected field outcome: {unexpected:?}")
        }
        ExecutionOutcome::ArithmeticException => "E:java/lang/ArithmeticException".to_owned(),
        ExecutionOutcome::NullPointerException => "E:java/lang/NullPointerException".to_owned(),
        ExecutionOutcome::ArrayIndexOutOfBoundsException => {
            "E:java/lang/ArrayIndexOutOfBoundsException".to_owned()
        }
        ExecutionOutcome::NegativeArraySizeException => {
            "E:java/lang/NegativeArraySizeException".to_owned()
        }
    }
}

#[test]
fn executes_all_280_openjdk_primitive_array_cases_independently() {
    let class = parse(CLASS).unwrap();
    assert_eq!(class.version.major, 65);
    let mut count = 0;
    let mut exceptions = BTreeMap::<String, usize>::new();
    let mut methods = BTreeSet::new();
    for line in REFERENCE.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let method = class
            .methods
            .iter()
            .position(|method| {
                class
                    .constant_pool
                    .utf8(method.name_index)
                    .unwrap()
                    .equals_ascii(columns[0])
                    && class
                        .constant_pool
                        .utf8(method.descriptor_index)
                        .unwrap()
                        .equals_ascii(columns[1])
            })
            .unwrap();
        let arguments: Vec<_> = if columns[2].is_empty() {
            Vec::new()
        } else {
            columns[2].split(',').map(input).collect()
        };
        // Every case gets an explicit isolated heap. Production execution never
        // runs the Java oracle driver or looks up expected reference results.
        let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        let result = execute_static_method_body_with_heap(
            &class,
            method,
            &arguments,
            ExecutionOptions::default(),
            CallLimits::default(),
            &mut heap,
        )
        .unwrap_or_else(|error| panic!("{line}: {error}"));
        let actual = observed(result);
        assert_eq!(actual, columns[3], "{line}");
        if actual.starts_with("E:") {
            *exceptions.entry(actual).or_default() += 1;
        }
        methods.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 280);
    assert_eq!(methods.len(), 20);
    assert_eq!(exceptions.len(), 3);
    for (expected, count) in [
        ("E:java/lang/NullPointerException", 11),
        ("E:java/lang/ArrayIndexOutOfBoundsException", 36),
        ("E:java/lang/NegativeArraySizeException", 20),
    ] {
        assert_eq!(exceptions.get(expected).copied(), Some(count));
    }
}

#[test]
fn selected_javac_methods_exercise_every_primitive_kind_and_array_access_opcode() {
    let class = parse(CLASS).unwrap();
    let selected: BTreeSet<_> = REFERENCE
        .lines()
        .map(|line| line.split('\t').next().unwrap())
        .collect();
    let mut kinds = BTreeSet::new();
    let mut opcodes = BTreeSet::new();
    for method in &class.methods {
        let name = class.constant_pool.utf8(method.name_index).unwrap();
        if !selected
            .iter()
            .any(|candidate| name.equals_ascii(candidate))
        {
            continue;
        }
        for attribute in &method.attributes {
            if let AttributeBody::Code(code) = &attribute.body {
                assert!(code.exception_table.is_empty());
                for instruction in crabspot_bytecode::decode(&code.code, class.version.major)
                    .unwrap()
                    .instructions()
                {
                    opcodes.insert(instruction.opcode);
                    if let Operands::NewArray(kind) = instruction.operands {
                        kinds.insert(kind);
                    }
                }
            }
        }
    }
    assert_eq!(kinds, (4..=11).collect());
    for opcode in [
        0x01, 0x2e, 0x2f, 0x30, 0x31, 0x33, 0x34, 0x35, 0x4f, 0x50, 0x51, 0x52, 0x54, 0x55, 0x56,
        0xa5, 0xa6, 0xbc, 0xbe, 0xc6, 0xc7,
    ] {
        assert!(
            opcodes.contains(&opcode),
            "selected fixture did not emit {opcode:#x}"
        );
    }
}

use crabspot_classfile::{AttributeBody, parse};
use crabspot_interpreter::{ExecutionOptions, ExecutionOutcome, Value, execute_static_method_body};

const CLASS: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/FloatingCases.class");
const REFERENCE: &str = include_str!("../../../tests/fixtures/reference/floating.tsv");

fn input(text: &str) -> Value {
    let (kind, number) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(number.parse().unwrap()),
        "L" => Value::Long(number.parse().unwrap()),
        "F" => Value::Float(u32::from_str_radix(number, 16).unwrap()),
        "D" => Value::Double(u64::from_str_radix(number, 16).unwrap()),
        _ => panic!("unknown reference input: {text}"),
    }
}

fn observed(outcome: ExecutionOutcome) -> String {
    match outcome {
        ExecutionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExecutionOutcome::Returned(Some(Value::Long(value))) => format!("L:{value}"),
        ExecutionOutcome::Returned(Some(Value::Float(bits))) => {
            if f32::from_bits(bits).is_nan() {
                "F:NaN".to_owned()
            } else {
                format!("F:{bits:08x}")
            }
        }
        ExecutionOutcome::Returned(Some(Value::Double(bits))) => {
            if f64::from_bits(bits).is_nan() {
                "D:NaN".to_owned()
            } else {
                format!("D:{bits:016x}")
            }
        }
        other => panic!("unexpected floating fixture outcome: {other:?}"),
    }
}

#[test]
fn matches_all_7263_openjdk_floating_cases_with_spec_nan_normalization() {
    let class = parse(CLASS).unwrap();
    let mut cases = 0;
    let mut nan_cases = 0;
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
        let result =
            execute_static_method_body(&class, method, &arguments, ExecutionOptions::default())
                .unwrap_or_else(|error| panic!("{line}: {error}"));
        assert_eq!(observed(result), columns[3], "{line}");
        if columns[3].ends_with(":NaN") {
            nan_cases += 1;
        }
        cases += 1;
    }
    assert_eq!(cases, 7263);
    assert!(nan_cases > 0);
}

#[test]
fn genuine_fixture_exercises_both_nan_orderings_and_both_precision_widths() {
    let class = parse(CLASS).unwrap();
    let mut opcodes = std::collections::BTreeSet::new();
    for method in &class.methods {
        for attribute in &method.attributes {
            if let AttributeBody::Code(code) = &attribute.body {
                for instruction in crabspot_bytecode::decode(&code.code, class.version.major)
                    .unwrap()
                    .instructions()
                {
                    opcodes.insert(instruction.opcode);
                }
            }
        }
    }
    for opcode in [
        0x62, 0x63, 0x66, 0x67, 0x6a, 0x6b, 0x6e, 0x6f, 0x72, 0x73, 0x76, 0x77, 0x86, 0x87, 0x89,
        0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x95, 0x96, 0x97, 0x98, 0xae, 0xaf,
    ] {
        assert!(
            opcodes.contains(&opcode),
            "fixture did not emit {opcode:#x}"
        );
    }
}

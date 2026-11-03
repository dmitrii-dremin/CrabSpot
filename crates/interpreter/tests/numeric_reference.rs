use crabspot_classfile::parse;
use crabspot_interpreter::{ExecutionOptions, ExecutionOutcome, Value, execute_static_method_body};

const CLASS: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/NumericCases.class");
const REFERENCE: &str = include_str!("../../../tests/fixtures/reference/numeric.tsv");

fn value(text: &str) -> Value {
    let (kind, number) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(number.parse().unwrap()),
        "L" => Value::Long(number.parse().unwrap()),
        _ => panic!("unknown reference value: {text}"),
    }
}

fn outcome(text: &str) -> ExecutionOutcome {
    match text {
        "V" => ExecutionOutcome::Returned(None),
        "E:java/lang/ArithmeticException" => ExecutionOutcome::ArithmeticException,
        _ => ExecutionOutcome::Returned(Some(value(text))),
    }
}

#[test]
fn executes_all_616_openjdk_numeric_reference_cases() {
    let class = parse(CLASS).unwrap();
    let mut cases = 0;
    let mut exceptions = 0;
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
            columns[2].split(',').map(value).collect()
        };
        let actual =
            execute_static_method_body(&class, method, &arguments, ExecutionOptions::default())
                .unwrap_or_else(|error| panic!("{line}: {error:?}"));
        let expected = outcome(columns[3]);
        assert_eq!(actual, expected, "{line}");
        if expected == ExecutionOutcome::ArithmeticException {
            exceptions += 1;
        }
        cases += 1;
    }
    assert_eq!(cases, 616);
    assert_eq!(exceptions, 34);
}

#[test]
fn compiler_produced_wide_and_switch_encodings_are_exercised() {
    let class = parse(CLASS).unwrap();
    let mut wide_increment = false;
    let mut table = false;
    let mut lookup = false;
    for method in &class.methods {
        for attribute in &method.attributes {
            if let crabspot_classfile::AttributeBody::Code(code) = &attribute.body {
                let decoded = crabspot_bytecode::decode(&code.code, class.version.major).unwrap();
                for instruction in decoded.instructions() {
                    wide_increment |= instruction.opcode == 0x84 && instruction.wide;
                    table |= instruction.opcode == 0xaa;
                    lookup |= instruction.opcode == 0xab;
                }
            }
        }
    }
    assert!(wide_increment);
    assert!(table);
    assert!(lookup);
}

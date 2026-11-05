use crabspot_classfile::parse;
use crabspot_interpreter::{ExecutionOptions, ExecutionOutcome, Value, execute_static_method_body};

const CASES: &[(&[u8], &str, usize)] = &[
    (
        include_bytes!("../../../tests/fixtures/classes/fixtures/CallCases.class"),
        include_str!("../../../tests/fixtures/reference/calls.tsv"),
        115,
    ),
    (
        include_bytes!("../../../tests/fixtures/classes/fixtures/InterfaceCalls.class"),
        include_str!("../../../tests/fixtures/reference/interface_calls.tsv"),
        11,
    ),
];

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
        ExecutionOutcome::Returned(None) => "V".to_owned(),
        ExecutionOutcome::ArithmeticException => "E:java/lang/ArithmeticException".to_owned(),
        unexpected @ (ExecutionOutcome::NullPointerException
        | ExecutionOutcome::ArrayIndexOutOfBoundsException
        | ExecutionOutcome::NegativeArraySizeException
        | ExecutionOutcome::NoSuchFieldError
        | ExecutionOutcome::IllegalAccessError
        | ExecutionOutcome::IncompatibleClassChangeError
        | ExecutionOutcome::NoSuchMethodError
        | ExecutionOutcome::AbstractMethodError
        | ExecutionOutcome::LinkageError
        | ExecutionOutcome::ClassCastException
        | ExecutionOutcome::ArrayStoreException) => {
            panic!("numeric call fixture unexpectedly raised array exception: {unexpected:?}")
        }
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
        ExecutionOutcome::Returned(Some(Value::Reference(reference))) => {
            panic!("numeric call fixture unexpectedly returned reference: {reference:?}")
        }
    }
}

#[test]
fn matches_126_openjdk_same_class_and_interface_call_graphs() {
    let mut total = 0;
    for (bytes, reference, expected_count) in CASES {
        let class = parse(bytes).unwrap();
        let mut count = 0;
        for line in reference.lines() {
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
            count += 1;
        }
        assert_eq!(count, *expected_count);
        total += count;
    }
    assert_eq!(total, 126);
}

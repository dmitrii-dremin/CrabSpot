//! Heap-service differential checks only. The later interpreter bridge executes
//! the complete array fixture table; these checks do not claim bytecode support.
use crabspot_classfile::parse;
use crabspot_runtime::{HeapOptions, ManagedHeap, PrimitiveArrayKind as Kind, Value};
use std::collections::BTreeSet;

const CLASS: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/ArrayCases.class");
const REFERENCE: &str = include_str!("../../../tests/fixtures/reference/arrays.tsv");

fn value(text: &str) -> Value {
    let (kind, number) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(number.parse().unwrap()),
        "L" => Value::Long(number.parse().unwrap()),
        "F" => Value::Float(u32::from_str_radix(number, 16).unwrap()),
        "D" => Value::Double(u64::from_str_radix(number, 16).unwrap()),
        _ => panic!("unexpected oracle input {text}"),
    }
}
fn observed(value: Value) -> String {
    match value {
        Value::Int(value) => format!("I:{value}"),
        Value::Long(value) => format!("L:{value}"),
        Value::Float(bits) if f32::from_bits(bits).is_nan() => "F:NaN".to_owned(),
        Value::Float(bits) => format!("F:{bits:08x}"),
        Value::Double(bits) if f64::from_bits(bits).is_nan() => "D:NaN".to_owned(),
        Value::Double(bits) => format!("D:{bits:016x}"),
        Value::Reference(_) => panic!("primitive heap returned a reference"),
    }
}

#[test]
fn primitive_heap_storage_matches_151_independent_jdk_round_trips() {
    let class = parse(CLASS).unwrap();
    assert_eq!(class.version.major, 65);
    let mut count = 0;
    let mut methods = BTreeSet::new();
    for line in REFERENCE.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let kind = match columns[0] {
            "booleanRoundTrip" => Kind::Boolean,
            "byteRoundTrip" => Kind::Byte,
            "charRoundTrip" => Kind::Char,
            "shortRoundTrip" => Kind::Short,
            "intRoundTrip" => Kind::Int,
            "longRoundTrip" => Kind::Long,
            "floatRoundTrip" => Kind::Float,
            "doubleRoundTrip" => Kind::Double,
            _ => continue, // Other oracle rows require the future execution bridge.
        };
        assert!(class.methods.iter().any(|method| {
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
        }));
        let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        let array = heap.allocate_primitive_array(kind, 1).unwrap();
        heap.store(array, 0, value(columns[2])).unwrap();
        assert_eq!(observed(heap.load(array, 0).unwrap()), columns[3], "{line}");
        assert_eq!(heap.allocated_arrays(), 1);
        assert_eq!(heap.total_elements(), 1);
        assert_eq!(heap.payload_bytes(), kind.element_bytes());
        heap.validate_root(array.heap_ref().unwrap()).unwrap();
        count += 1;
        methods.insert(columns[0]);
    }
    assert_eq!(count, 151);
    assert_eq!(methods.len(), 8);
}

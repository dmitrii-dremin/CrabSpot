//! Reference values do not authorize reference execution in the numeric kernel.
use crabspot_classfile::{ClassFile, parse};
use crabspot_interpreter::{
    ExecutionErrorKind as E, ExecutionOptions, Value, ValueKind, execute_static_method_body,
};
use crabspot_runtime::{HeapOptions, ManagedHeap, PrimitiveArrayKind, Reference};

fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}
fn utf8(text: &str) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend(words(&[text.len() as u16]));
    bytes.extend(text.bytes());
    bytes
}
fn method(descriptor: &str, code: &[u8], locals: u16) -> ClassFile {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[8]));
    for entry in [
        utf8("ReferenceBoundary"),
        vec![7, 0, 1],
        utf8("java/lang/Object"),
        vec![7, 0, 3],
        utf8("method"),
        utf8(descriptor),
        utf8("Code"),
    ] {
        bytes.extend(entry);
    }
    bytes.extend(words(&[0x21, 2, 4, 0, 0, 1, 9, 5, 6, 1, 7]));
    bytes.extend((12 + code.len() as u32).to_be_bytes());
    bytes.extend(words(&[4, locals]));
    bytes.extend((code.len() as u32).to_be_bytes());
    bytes.extend(code);
    bytes.extend(words(&[0, 0, 0]));
    parse(&bytes).unwrap()
}

#[test]
fn null_and_allocated_arguments_are_typed_errors_for_every_numeric_parameter_kind() {
    let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
    let allocated = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    assert!(!allocated.is_null());
    for (descriptor, expected) in [
        ("(Z)V", ValueKind::Int),
        ("(B)V", ValueKind::Int),
        ("(C)V", ValueKind::Int),
        ("(S)V", ValueKind::Int),
        ("(I)V", ValueKind::Int),
        ("(J)V", ValueKind::Long),
        ("(F)V", ValueKind::Float),
        ("(D)V", ValueKind::Double),
    ] {
        let class = method(descriptor, &[0xb1], 2);
        for reference in [Reference::null(), allocated] {
            let error = execute_static_method_body(
                &class,
                0,
                &[Value::Reference(reference)],
                ExecutionOptions::default(),
            )
            .unwrap_err();
            assert_eq!(
                error.kind,
                E::ArgumentType {
                    index: 0,
                    expected,
                    actual: ValueKind::Reference
                }
            );
            assert_eq!(error.method.as_ref().unwrap().index, 0);
        }
    }
    assert_eq!(heap.load(allocated, 0).unwrap(), Value::Int(0));
}

#[test]
fn object_and_array_descriptors_remain_explicitly_unsupported() {
    for descriptor in ["(Ljava/lang/Object;)V", "([I)V", "()[[Ljava/lang/String;"] {
        let class = method(descriptor, &[0xb1], 1);
        let error = execute_static_method_body(
            &class,
            0,
            &[Value::Reference(Reference::null())],
            ExecutionOptions::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind, E::UnsupportedDescriptor);
    }
}

#[test]
fn reference_instructions_still_fail_instead_of_fabricating_numeric_values() {
    for opcode in [0x01, 0x2a, 0x4b, 0xb0] {
        let class = method("()V", &[opcode, 0xb1], 1);
        let error =
            execute_static_method_body(&class, 0, &[], ExecutionOptions::default()).unwrap_err();
        assert_eq!(error.kind, E::UnsupportedInstruction(opcode));
        assert_eq!(error.offset, Some(0));
    }
}

#[test]
fn interpreter_value_reexports_are_the_same_runtime_types() {
    let value: crabspot_runtime::Value = Value::Reference(Reference::null());
    let kind: crabspot_runtime::ValueKind = ValueKind::Reference;
    assert_eq!(value.kind(), kind);
    assert_eq!(value.slots(), 1);
}

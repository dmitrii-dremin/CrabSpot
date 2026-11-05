//! Independent checked-in JDK comparisons for instance method bodies. Default
//! storage is supplied explicitly; these tests do not claim constructor execution.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionOptions, ExecutionOutcome, execute_instance_method_body,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldResolutionContext, FieldResolutionOptions, HeapOptions,
    InstanceOptions, ManagedHeap, RegistryOptions, SameLoaderResolver, Value, build_hierarchy,
};
use std::collections::{BTreeMap, BTreeSet};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn structural_root() -> Vec<u8> {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 3, 1, 0, 16];
    bytes.extend_from_slice(b"java/lang/Object");
    bytes.extend_from_slice(&[7, 0, 1, 0, 0x21, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn input(text: &str) -> Value {
    let (kind, value) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(value.parse().unwrap()),
        "L" => Value::Long(value.parse().unwrap()),
        "F" => Value::Float(u32::from_str_radix(value, 16).unwrap()),
        "D" => Value::Double(u64::from_str_radix(value, 16).unwrap()),
        _ => panic!("unexpected oracle input {text}"),
    }
}

// Original minimal synthetic body fixture. Numeric arguments start at local one
// after this. Z/B/C/S methods return I and contain no pre-store conversions.
fn raw_field_bodies() -> Vec<u8> {
    fn utf(pool: &mut Vec<Vec<u8>>, value: &str) -> u16 {
        let mut entry = vec![1];
        entry.extend_from_slice(&(value.len() as u16).to_be_bytes());
        entry.extend_from_slice(value.as_bytes());
        pool.push(entry);
        pool.len() as u16
    }
    fn cp(pool: &mut Vec<Vec<u8>>, tag: u8, first: u16, second: u16) -> u16 {
        let mut entry = vec![tag];
        entry.extend_from_slice(&first.to_be_bytes());
        if tag != 7 {
            entry.extend_from_slice(&second.to_be_bytes());
        }
        pool.push(entry);
        pool.len() as u16
    }
    let mut pool = Vec::new();
    let name = utf(&mut pool, "fixtures/RawFieldBodies");
    let class = cp(&mut pool, 7, name, 0);
    let name = utf(&mut pool, "java/lang/Object");
    let parent = cp(&mut pool, 7, name, 0);
    let code_name = utf(&mut pool, "Code");
    let mut fields = Vec::new();
    let mut methods = Vec::new();
    for (name, descriptor, load, ret) in [
        ("flag", "Z", 0x1b, 0xac),
        ("small", "B", 0x1b, 0xac),
        ("letter", "C", 0x1b, 0xac),
        ("narrow", "S", 0x1b, 0xac),
        ("integer", "I", 0x1b, 0xac),
        ("wide", "J", 0x1f, 0xad),
        ("single", "F", 0x23, 0xae),
        ("precise", "D", 0x27, 0xaf),
    ] {
        let name_index = utf(&mut pool, name);
        let descriptor_index = utf(&mut pool, descriptor);
        let nat = cp(&mut pool, 12, name_index, descriptor_index);
        let field = cp(&mut pool, 9, class, nat);
        fields.push((name_index, descriptor_index));
        let method = utf(&mut pool, &format!("{name}RoundTrip"));
        let computational = if "ZBCSI".contains(descriptor) {
            "I"
        } else {
            descriptor
        };
        let signature = utf(&mut pool, &format!("({computational}){computational}"));
        let [hi, lo] = field.to_be_bytes();
        methods.push((
            method,
            signature,
            if descriptor == "J" || descriptor == "D" {
                3u16
            } else {
                2u16
            },
            vec![0x2a, load, 0xb5, hi, lo, 0x2a, 0xb4, hi, lo, ret],
        ));
    }
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend_from_slice(&(pool.len() as u16 + 1).to_be_bytes());
    for entry in pool {
        bytes.extend(entry);
    }
    for word in [0x21u16, class, parent, 0, fields.len() as u16] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    for (name, descriptor) in fields {
        for word in [1u16, name, descriptor, 0] {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
    }
    bytes.extend_from_slice(&(methods.len() as u16).to_be_bytes());
    for (name, descriptor, locals, code) in methods {
        for word in [1u16, name, descriptor, 1, code_name] {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        bytes.extend_from_slice(&(12u32 + code.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&3u16.to_be_bytes());
        bytes.extend_from_slice(&locals.to_be_bytes());
        bytes.extend_from_slice(&(code.len() as u32).to_be_bytes());
        bytes.extend(code);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
    }
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

fn observed(outcome: ExecutionOutcome) -> String {
    match outcome {
        ExecutionOutcome::Returned(None) => "V".to_owned(),
        ExecutionOutcome::Returned(Some(Value::Int(v))) => format!("I:{v}"),
        ExecutionOutcome::Returned(Some(Value::Long(v))) => format!("L:{v}"),
        ExecutionOutcome::Returned(Some(Value::Float(v))) if f32::from_bits(v).is_nan() => {
            "F:NaN".to_owned()
        }
        ExecutionOutcome::Returned(Some(Value::Float(v))) => format!("F:{v:08x}"),
        ExecutionOutcome::Returned(Some(Value::Double(v))) if f64::from_bits(v).is_nan() => {
            "D:NaN".to_owned()
        }
        ExecutionOutcome::Returned(Some(Value::Double(v))) => format!("D:{v:016x}"),
        ExecutionOutcome::Returned(Some(Value::Reference(_))) => {
            panic!("unexpected reference result")
        }
        ExecutionOutcome::ArrayStoreException => "E:java/lang/ArrayStoreException".to_owned(),
        ExecutionOutcome::ClassCastException => "E:java/lang/ClassCastException".to_owned(),
        ExecutionOutcome::ArithmeticException => "E:java/lang/ArithmeticException".to_owned(),
        ExecutionOutcome::NullPointerException => "E:java/lang/NullPointerException".to_owned(),
        ExecutionOutcome::ArrayIndexOutOfBoundsException => {
            "E:java/lang/ArrayIndexOutOfBoundsException".to_owned()
        }
        ExecutionOutcome::NegativeArraySizeException => {
            "E:java/lang/NegativeArraySizeException".to_owned()
        }
        ExecutionOutcome::NoSuchFieldError => "E:java/lang/NoSuchFieldError".to_owned(),
        ExecutionOutcome::IllegalAccessError => "E:java/lang/IllegalAccessError".to_owned(),
        ExecutionOutcome::NoSuchMethodError => "E:java/lang/NoSuchMethodError".to_owned(),
        ExecutionOutcome::LinkageError => "E:java/lang/LinkageError".to_owned(),
        ExecutionOutcome::AbstractMethodError => "E:java/lang/AbstractMethodError".to_owned(),
        ExecutionOutcome::IncompatibleClassChangeError => {
            "E:java/lang/IncompatibleClassChangeError".to_owned()
        }
    }
}

fn registry(raw: bool) -> (DefinitionRegistry, ClassId, ClassId) {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    registry
        .define(
            loader,
            &units("java/lang/Object"),
            &structural_root(),
            ParseOptions::default(),
        )
        .unwrap();
    if raw {
        let owner = registry
            .define(
                loader,
                &units("fixtures/RawFieldBodies"),
                &raw_field_bodies(),
                ParseOptions::default(),
            )
            .unwrap();
        return (registry, owner, owner);
    }
    let base = registry
        .define(
            loader,
            &units("fixtures/InstanceBodyBase"),
            include_bytes!("../../../tests/fixtures/classes/fixtures/InstanceBodyBase.class"),
            ParseOptions::default(),
        )
        .unwrap();
    let owner = registry
        .define(
            loader,
            &units("fixtures/InstanceBodyCases"),
            include_bytes!("../../../tests/fixtures/classes/fixtures/InstanceBodyCases.class"),
            ParseOptions::default(),
        )
        .unwrap();
    assert_eq!(registry.class(base).unwrap().version.major, 65);
    assert_eq!(registry.class(owner).unwrap().version.major, 65);
    (registry, owner, base)
}

fn compare_cases(raw: bool, table: &str) -> (usize, BTreeSet<String>, BTreeMap<String, usize>) {
    let (registry, owner, base) = registry(raw);
    let plan = build_hierarchy(
        &registry,
        owner,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let mut methods = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in table.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let declaring = if columns[0] == "inheritedBody" {
            base
        } else {
            owner
        };
        let method = registry
            .find_declared_method(declaring, &units(columns[0]), &units(columns[1]))
            .unwrap()
            .unwrap();
        let arguments: Vec<_> = columns[2].split(',').map(input).collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        let receiver = heap.allocate_instance(owner).unwrap();
        let integer = registry
            .find_declared_field(owner, &units("integer"), &units("I"))
            .unwrap()
            .unwrap();
        if columns[0] == "storeBeforeDivide" || columns[0] == "nullWrite" {
            heap.put_field(receiver, integer, Value::Int(999)).unwrap();
        }
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let outcome = execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &arguments,
            ExecutionOptions::default(),
            CallLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{line}: {error}"));
        let actual = observed(outcome);
        assert_eq!(actual, columns[3], "{line}");
        if actual.starts_with("E:") {
            *exceptions.entry(actual).or_default() += 1;
        }
        if columns[0] == "writeOnly" || columns[0] == "storeBeforeDivide" {
            assert_eq!(
                context.heap().get_field(receiver, integer).unwrap(),
                arguments[0],
                "persistent store: {line}"
            );
        }
        if columns[0] == "nullWrite" {
            assert_eq!(
                context.heap().get_field(receiver, integer).unwrap(),
                Value::Int(999)
            );
        }
        assert_eq!(context.heap().allocated_instances(), 1);
        methods.insert(columns[0].to_owned());
        count += 1;
    }
    (count, methods, exceptions)
}

#[test]
fn executes_402_genuine_instance_body_cases_against_jdk21() {
    let (count, methods, exceptions) = compare_cases(
        false,
        include_str!("../../../tests/fixtures/reference/instances.tsv"),
    );
    assert_eq!(count, 402);
    assert_eq!(methods.len(), 21);
    assert_eq!(
        exceptions.get("E:java/lang/NullPointerException"),
        Some(&46)
    );
    assert_eq!(exceptions.get("E:java/lang/ArithmeticException"), Some(&1));
    assert_eq!(
        exceptions.get("E:java/lang/NegativeArraySizeException"),
        Some(&2)
    );
    assert_eq!(exceptions.len(), 3);
}

#[test]
fn executes_151_raw_field_stores_without_narrowing_against_jdk21() {
    let (count, methods, exceptions) = compare_cases(
        true,
        include_str!("../../../tests/fixtures/reference/fields.tsv"),
    );
    assert_eq!(count, 151);
    assert_eq!(methods.len(), 8);
    assert!(exceptions.is_empty());
}

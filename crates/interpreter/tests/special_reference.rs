//! JDK comparisons for ordinary superclass and direct-superinterface body graphs.
//! The host supplies default storage; constructors and class initialization are not executed.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionOptions, ExecutionOutcome, execute_instance_method_body,
};
use crabspot_runtime::{
    DefinitionRegistry, FieldResolutionContext, FieldResolutionOptions, HeapOptions,
    InstanceOptions, ManagedHeap, RegistryOptions, SameLoaderResolver, Value, build_hierarchy,
};
use std::collections::{BTreeMap, BTreeSet};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn input(text: &str) -> Value {
    let (kind, value) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(value.parse().unwrap()),
        "L" => Value::Long(value.parse().unwrap()),
        "F" => Value::Float(u32::from_str_radix(value, 16).unwrap()),
        "D" => Value::Double(u64::from_str_radix(value, 16).unwrap()),
        _ => panic!("unexpected input {text}"),
    }
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
        ExecutionOutcome::Returned(Some(Value::Reference(_))) => panic!("unexpected reference"),
        ExecutionOutcome::ArithmeticException => "E:java/lang/ArithmeticException".to_owned(),
        ExecutionOutcome::NullPointerException => "E:java/lang/NullPointerException".to_owned(),
        ExecutionOutcome::NegativeArraySizeException => {
            "E:java/lang/NegativeArraySizeException".to_owned()
        }
        other => panic!("unexpected oracle outcome {other:?}"),
    }
}

#[test]
fn executes_450_ordinary_special_body_graphs_against_jdk21() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    // Explicit original structural input, not a production Object substitute.
    let mut root = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 3, 1, 0, 16];
    root.extend_from_slice(b"java/lang/Object");
    root.extend_from_slice(&[7, 0, 1, 0, 0x21, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    registry
        .define(
            loader,
            &units("java/lang/Object"),
            &root,
            ParseOptions::default(),
        )
        .unwrap();
    for (name, bytes) in [
        (
            "SpecialRootDefault",
            include_bytes!("../../../tests/fixtures/classes/fixtures/SpecialRootDefault.class")
                .as_slice(),
        ),
        (
            "SpecialChildDefault",
            include_bytes!("../../../tests/fixtures/classes/fixtures/SpecialChildDefault.class")
                .as_slice(),
        ),
        (
            "SpecialCallBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/SpecialCallBase.class")
                .as_slice(),
        ),
        (
            "SpecialCallMiddle",
            include_bytes!("../../../tests/fixtures/classes/fixtures/SpecialCallMiddle.class")
                .as_slice(),
        ),
        (
            "SpecialCallCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/SpecialCallCases.class")
                .as_slice(),
        ),
        (
            "SpecialCallLeaf",
            include_bytes!("../../../tests/fixtures/classes/fixtures/SpecialCallLeaf.class")
                .as_slice(),
        ),
    ] {
        let id = registry
            .define(
                loader,
                &units(&format!("fixtures/{name}")),
                bytes,
                ParseOptions::default(),
            )
            .unwrap();
        assert_eq!(registry.class(id).unwrap().version.major, 65);
    }
    let class = |name: &str| {
        registry
            .lookup_class(loader, &units(&format!("fixtures/{name}")))
            .unwrap()
            .unwrap()
    };
    let owner = class("SpecialCallCases");
    let base = class("SpecialCallBase");
    let leaf = class("SpecialCallLeaf");
    let plan =
        build_hierarchy(&registry, leaf, &mut SameLoaderResolver, Default::default()).unwrap();
    let field = |owner, name: &str, descriptor: &str| {
        registry
            .find_declared_field(owner, &units(name), &units(descriptor))
            .unwrap()
            .unwrap()
    };
    let base_hidden = field(base, "hidden", "I");
    let own_hidden = field(owner, "hidden", "I");
    let wide = field(base, "wide", "J");
    let single = field(base, "single", "F");
    let precise = field(base, "precise", "D");
    let mut identities = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/special_calls.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (receiver_name, name) = columns[0].split_once('/').unwrap();
        let actual = match receiver_name {
            "child" => owner,
            "leaf" => leaf,
            _ => panic!("unknown receiver"),
        };
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let args: Vec<_> = columns[2].split(',').map(input).collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        let receiver = heap.allocate_instance(actual).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let result = execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &args,
            ExecutionOptions::default(),
            CallLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{line}: {error}"));
        let output = observed(result);
        assert_eq!(output, columns[3], "{line}");
        if output.starts_with("E:") {
            *exceptions.entry(output).or_insert(0) += 1;
        }
        // These source-derived state checks supplement the JDK result-only rows.
        let integer = match args[0] {
            Value::Int(value) => value,
            _ => 0,
        };
        let expected_base = match name {
            "callSuper" => Value::Int(integer.wrapping_add(1)),
            "callGrand" | "callBeforeDivide" | "callSuperVoid" | "callSuperMixed" => args[0],
            "callSuperStatic" => Value::Int(integer.wrapping_mul(19).wrapping_sub(3)),
            "callSuperRecursive" => Value::Int(7),
            _ => Value::Int(0),
        };
        assert_eq!(
            context.heap().get_field(receiver, base_hidden).unwrap(),
            expected_base,
            "{line}"
        );
        assert_eq!(
            context.heap().get_field(receiver, own_hidden).unwrap(),
            Value::Int(if name == "callBeforeDivide" { 99 } else { 0 }),
            "{line}"
        );
        for (field, expected) in [
            (
                wide,
                if name == "callSuperLong" {
                    args[0]
                } else if name == "callSuperMixed" {
                    args[1]
                } else {
                    Value::Long(0)
                },
            ),
            (
                single,
                if name == "callSuperFloat" {
                    args[0]
                } else if name == "callSuperMixed" {
                    args[2]
                } else {
                    Value::Float(0)
                },
            ),
            (
                precise,
                if name == "callSuperDouble" {
                    args[0]
                } else if name == "callSuperMixed" {
                    args[3]
                } else {
                    Value::Double(0)
                },
            ),
        ] {
            assert_eq!(
                context.heap().get_field(receiver, field).unwrap(),
                expected,
                "{line}"
            );
        }
        assert_eq!(context.heap().allocated_instances(), 1);
        assert_eq!(
            context.heap().allocated_arrays(),
            usize::from(name == "callSuperArray" && integer >= 0)
        );
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 450);
    assert_eq!(identities.len(), 36);
    assert_eq!(exceptions.get("E:java/lang/ArithmeticException"), Some(&2));
    assert_eq!(
        exceptions.get("E:java/lang/NegativeArraySizeException"),
        Some(&4)
    );
    assert_eq!(exceptions.len(), 2);
}

//! JDK comparisons for explicitly selected body graphs with invokevirtual.
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
fn executes_494_virtual_body_graphs_against_jdk21() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    // Explicit structural test input, not a synthesized production Object class.
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
            "VirtualCallBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/VirtualCallBase.class")
                .as_slice(),
        ),
        (
            "VirtualCallCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/VirtualCallCases.class")
                .as_slice(),
        ),
        (
            "VirtualCallLeaf",
            include_bytes!("../../../tests/fixtures/classes/fixtures/VirtualCallLeaf.class")
                .as_slice(),
        ),
        (
            "virtualp/PackageCallBase",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/virtualp/PackageCallBase.class"
            )
            .as_slice(),
        ),
        (
            "virtualq/PackageCallMiddle",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/virtualq/PackageCallMiddle.class"
            )
            .as_slice(),
        ),
        (
            "virtualp/PackageCallBridge",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/virtualp/PackageCallBridge.class"
            )
            .as_slice(),
        ),
        (
            "virtualq/PackageCallLeaf",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/virtualq/PackageCallLeaf.class"
            )
            .as_slice(),
        ),
    ] {
        let class = registry
            .define(
                loader,
                &units(&format!("fixtures/{name}")),
                bytes,
                ParseOptions::default(),
            )
            .unwrap();
        assert_eq!(registry.class(class).unwrap().version.major, 65);
    }
    let class = |name: &str| {
        registry
            .lookup_class(loader, &units(&format!("fixtures/{name}")))
            .unwrap()
            .unwrap()
    };
    let base = class("VirtualCallBase");
    let child = class("VirtualCallCases");
    let leaf = class("VirtualCallLeaf");
    let package_base = class("virtualp/PackageCallBase");
    let package_middle = class("virtualq/PackageCallMiddle");
    let package_bridge = class("virtualp/PackageCallBridge");
    let package_leaf = class("virtualq/PackageCallLeaf");
    let package_plan = build_hierarchy(
        &registry,
        package_leaf,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let plan =
        build_hierarchy(&registry, leaf, &mut SameLoaderResolver, Default::default()).unwrap();
    let base_hidden = registry
        .find_declared_field(base, &units("hidden"), &units("I"))
        .unwrap()
        .unwrap();
    let child_hidden = registry
        .find_declared_field(child, &units("hidden"), &units("I"))
        .unwrap()
        .unwrap();
    let mut methods = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/virtual_calls.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (receiver_name, name) = columns[0].split_once('/').unwrap();
        let actual_class = match receiver_name {
            "child" => child,
            "leaf" => leaf,
            "package-base" => package_base,
            "package-middle" => package_middle,
            "package-bridge" => package_bridge,
            "package-leaf" => package_leaf,
            _ => panic!("invalid receiver"),
        };
        let owner = if name == "dispatch" {
            package_base
        } else if name == "dispatchFromBase" {
            base
        } else {
            child
        };
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let arguments: Vec<_> = columns[2].split(',').map(input).collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan, &package_plan])
            .unwrap();
        let receiver = heap.allocate_instance(actual_class).unwrap();
        if name == "callBeforeDivide" {
            heap.put_field(receiver, child_hidden, Value::Int(999))
                .unwrap();
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
        let output = observed(outcome);
        assert_eq!(output, columns[3], "{line}");
        if output.starts_with("E:") {
            *exceptions.entry(output).or_insert(0) += 1;
        }
        if name == "callBeforeDivide" {
            assert_eq!(
                context.heap().get_field(receiver, base_hidden).unwrap(),
                Value::Int(99)
            );
            assert_eq!(
                context.heap().get_field(receiver, child_hidden).unwrap(),
                arguments[0]
            );
        }
        if name == "callVoid" {
            assert_eq!(
                context.heap().get_field(receiver, child_hidden).unwrap(),
                arguments[0]
            );
        }
        if name == "dispatchFromBase" || name == "callBaseBody" {
            assert_eq!(
                context.heap().get_field(receiver, base_hidden).unwrap(),
                arguments[0]
            );
            let Value::Int(value) = arguments[0] else {
                panic!("integer input expected")
            };
            let expected = if actual_class == child {
                value.wrapping_add(1)
            } else {
                0
            };
            assert_eq!(
                context.heap().get_field(receiver, child_hidden).unwrap(),
                Value::Int(expected)
            );
        }
        assert_eq!(context.heap().allocated_instances(), 1);
        methods.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 494);
    assert_eq!(methods.len(), 40);
    assert_eq!(
        exceptions.get("E:java/lang/NullPointerException"),
        Some(&28)
    );
    assert_eq!(exceptions.get("E:java/lang/ArithmeticException"), Some(&2));
    assert_eq!(
        exceptions.get("E:java/lang/NegativeArraySizeException"),
        Some(&4)
    );
    assert_eq!(exceptions.len(), 3);
}

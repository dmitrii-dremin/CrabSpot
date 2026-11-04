//! JDK comparisons for interface/default body graphs with invokevirtual and invokeinterface.
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
fn executes_1337_interface_and_default_graphs_against_jdk21() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    // Explicit structural test input; no synthetic production Object fallback.
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
            "DispatchRoot",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchRoot.class")
                .as_slice(),
        ),
        (
            "DispatchChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchChild.class")
                .as_slice(),
        ),
        (
            "DispatchLeft",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchLeft.class")
                .as_slice(),
        ),
        (
            "DispatchRight",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchRight.class")
                .as_slice(),
        ),
        (
            "DispatchDiamond",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchDiamond.class")
                .as_slice(),
        ),
        (
            "DispatchSpecific",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchSpecific.class")
                .as_slice(),
        ),
        (
            "DispatchSpecificDiamond",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/DispatchSpecificDiamond.class"
            )
            .as_slice(),
        ),
        (
            "DispatchRedeclared",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchRedeclared.class")
                .as_slice(),
        ),
        (
            "DispatchChildCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchChildCases.class")
                .as_slice(),
        ),
        (
            "DispatchLeafCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchLeafCases.class")
                .as_slice(),
        ),
        (
            "DispatchDiamondCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchDiamondCases.class")
                .as_slice(),
        ),
        (
            "DispatchSpecificCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchSpecificCases.class")
                .as_slice(),
        ),
        (
            "DispatchMaskBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchMaskBase.class")
                .as_slice(),
        ),
        (
            "DispatchMaskedCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchMaskedCases.class")
                .as_slice(),
        ),
        (
            "DispatchRedeclaredCases",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/DispatchRedeclaredCases.class"
            )
            .as_slice(),
        ),
        (
            "InterfaceDispatchCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/InterfaceDispatchCases.class")
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
    let owner = class("InterfaceDispatchCases");
    let state = registry
        .find_declared_field(owner, &units("state"), &units("I"))
        .unwrap()
        .unwrap();
    let mut identities = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/interface_dispatch.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (receiver_name, name) = columns[0].split_once('/').unwrap();
        let actual = class(match receiver_name {
            "root" => "InterfaceDispatchCases",
            "child" => "DispatchChildCases",
            "leaf" => "DispatchLeafCases",
            "diamond" => "DispatchDiamondCases",
            "specific" => "DispatchSpecificCases",
            "masked" => "DispatchMaskedCases",
            "redeclared" => "DispatchRedeclaredCases",
            _ => panic!("unknown receiver {receiver_name}"),
        });
        let plan = build_hierarchy(
            &registry,
            actual,
            &mut SameLoaderResolver,
            Default::default(),
        )
        .unwrap();
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let arguments: Vec<_> = columns[2].split(',').map(input).collect();
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
            &arguments,
            ExecutionOptions::default(),
            CallLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{line}: {error}"));
        let output = observed(result);
        assert_eq!(output, columns[3], "{line}");
        if output.starts_with("E:") {
            *exceptions.entry(output).or_insert(0) += 1;
        }
        // Source-derived state checks are separate from the recorded JDK result rows.
        let expected_state = match name {
            "interfaceWrite"
            | "interfaceVoid"
            | "interfaceBeforeDivide"
            | "interfaceNull"
            | "interfaceMixed" => arguments[0],
            "interfaceRecursive" => Value::Int(7),
            "interfaceCross" => Value::Int(11),
            _ => Value::Int(0),
        };
        assert_eq!(
            context.heap().get_field(receiver, state).unwrap(),
            expected_state,
            "{line}"
        );
        assert_eq!(context.heap().allocated_instances(), 1);
        let expected_arrays = usize::from(
            name == "interfaceArray" && matches!(arguments[0],Value::Int(length) if length >= 0),
        );
        assert_eq!(context.heap().allocated_arrays(), expected_arrays, "{line}");
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 1337);
    assert_eq!(identities.len(), 112);
    assert_eq!(
        exceptions.get("E:java/lang/NullPointerException"),
        Some(&98)
    );
    assert_eq!(exceptions.get("E:java/lang/ArithmeticException"), Some(&7));
    assert_eq!(
        exceptions.get("E:java/lang/NegativeArraySizeException"),
        Some(&14)
    );
    assert_eq!(exceptions.len(), 3);
}

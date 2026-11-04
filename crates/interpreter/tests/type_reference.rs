//! Java 21 reference comparisons for checkcast and instanceof.
//! Original structural core metadata supplies identities, not a Java library.
//! Host allocations bypass constructors; this tests selected numeric method bodies.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionOptions, ExecutionOutcome, execute_instance_method_body,
};
use crabspot_runtime::{
    DefinitionRegistry, FieldResolutionContext, FieldResolutionOptions, HeapOptions,
    InstanceOptions, ManagedHeap, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver,
    Value, build_hierarchy,
};
use std::collections::{BTreeMap, BTreeSet};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn core(name: &str, interface: bool) -> Vec<u8> {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend_from_slice(&(if interface { 5u16 } else { 3u16 }).to_be_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&[7, 0, 1]);
    if interface {
        bytes.extend_from_slice(&[1, 0, 16]);
        bytes.extend_from_slice(b"java/lang/Object");
        bytes.extend_from_slice(&[7, 0, 3]);
    }
    bytes.extend_from_slice(&(if interface { 0x601u16 } else { 0x21u16 }).to_be_bytes());
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&(if interface { 4u16 } else { 0u16 }).to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes
}
fn observed(result: ExecutionOutcome) -> String {
    match result {
        ExecutionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExecutionOutcome::ClassCastException => "E:java/lang/ClassCastException".to_owned(),
        ExecutionOutcome::NegativeArraySizeException => {
            "E:java/lang/NegativeArraySizeException".to_owned()
        }
        other => panic!("unexpected oracle outcome {other:?}"),
    }
}
#[test]
fn executes_1276_type_body_graphs_against_jdk21() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let mut define_core = |name, interface| {
        registry
            .define(
                loader,
                &units(name),
                &core(name, interface),
                ParseOptions::default(),
            )
            .unwrap()
    };
    let object = define_core("java/lang/Object", false);
    let cloneable = define_core("java/lang/Cloneable", true);
    let serializable = define_core("java/io/Serializable", true);
    let universe = ReferenceTypeUniverse {
        bootstrap_loader: loader,
        object,
        cloneable,
        serializable,
    };
    for (name, bytes) in [
        (
            "CastBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastBase.class").as_slice(),
        ),
        (
            "CastCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastCases.class").as_slice(),
        ),
        (
            "CastChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastChild.class").as_slice(),
        ),
        (
            "CastRootMarker",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastRootMarker.class")
                .as_slice(),
        ),
        (
            "CastLeftMarker",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastLeftMarker.class")
                .as_slice(),
        ),
        (
            "CastRightMarker",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastRightMarker.class")
                .as_slice(),
        ),
        (
            "CastOtherMarker",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastOtherMarker.class")
                .as_slice(),
        ),
        (
            "CastUnrelated",
            include_bytes!("../../../tests/fixtures/classes/fixtures/CastUnrelated.class")
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
    // CastNullTarget is intentionally absent. Null operations must skip its resolution,
    // including when the heap has no reference universe at all.
    let class = |name: &str| {
        registry
            .lookup_class(loader, &units(&format!("fixtures/{name}")))
            .unwrap()
            .unwrap()
    };
    let owner = class("CastCases");
    let child = class("CastChild");
    let base = class("CastBase");
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let unrelated = build_hierarchy(
        &registry,
        class("CastUnrelated"),
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let state = registry
        .find_declared_field(owner, &units("state"), &units("I"))
        .unwrap()
        .unwrap();
    let inherited = registry
        .find_declared_field(base, &units("inherited"), &units("I"))
        .unwrap()
        .unwrap();
    let mut identities = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/types.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (receiver_name, name) = columns[0].split_once('/').unwrap();
        let actual = match receiver_name {
            "root" => owner,
            "child" => child,
            _ => panic!("unknown receiver"),
        };
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let integers: Vec<i32> = columns[2]
            .split(',')
            .map(|text| text.strip_prefix("I:").unwrap().parse().unwrap())
            .collect();
        let args: Vec<_> = integers.iter().copied().map(Value::Int).collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan, &unrelated])
            .unwrap();
        if !matches!(name, "nullUnregistered" | "nullTestUnregistered") {
            heap.bind_reference_types(&registry, universe).unwrap();
        }
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
        // Source-derived effects are checked even when a cast or allocation fails.
        assert_eq!(
            context.heap().get_field(receiver, state).unwrap(),
            args[0],
            "{line}"
        );
        assert_eq!(
            context.heap().get_field(receiver, inherited).unwrap(),
            Value::Int(if matches!(name, "callBase" | "callSpecialBase") {
                integers[0]
            } else {
                0
            }),
            "{line}"
        );
        let allocated = match name {
            "callStaticArray" => integers[0] >= 0,
            "arrayMask" | "arrayCast" => integers[0] < 8 && *integers.last().unwrap() >= 0,
            _ => false,
        };
        assert_eq!(
            context.heap().allocated_arrays(),
            usize::from(allocated),
            "{line}"
        );
        assert_eq!(context.heap().allocated_instances(), 1, "{line}");
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 1276);
    assert_eq!(identities.len(), 36);
    assert_eq!(exceptions.get("E:java/lang/ClassCastException"), Some(&463));
    assert_eq!(
        exceptions.get("E:java/lang/NegativeArraySizeException"),
        Some(&242)
    );
    assert_eq!(exceptions.len(), 2);
}

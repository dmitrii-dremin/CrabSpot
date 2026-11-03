//! Memory-service checks against genuine metadata and a development-only raw
//! putfield JDK oracle. CrabSpot does not execute these field bytecodes yet.
use crabspot_classfile::{AttributeBody, ParseOptions};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldId, HeapOptions, InstanceError, InstanceOptions, LoaderId,
    ManagedHeap, Reference, RegistryOptions, SameLoaderResolver, Value, build_hierarchy,
};
use std::collections::BTreeSet;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn setup() -> (DefinitionRegistry, LoaderId, ClassId, ManagedHeap) {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    // Explicit test-only structural root; no production Object behavior.
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
    let inputs: &[(&str, &[u8])] = &[
        (
            "HierarchyBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/HierarchyBase.class"),
        ),
        (
            "HierarchyChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/HierarchyChild.class"),
        ),
        (
            "HierarchyParentContract",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/HierarchyParentContract.class"
            ),
        ),
        (
            "HierarchyContract",
            include_bytes!("../../../tests/fixtures/classes/fixtures/HierarchyContract.class"),
        ),
        (
            "InstanceFields",
            include_bytes!("../../../tests/fixtures/classes/fixtures/InstanceFields.class"),
        ),
    ];
    for (name, bytes) in inputs {
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
    let owner = registry
        .lookup_class(loader, &units("fixtures/InstanceFields"))
        .unwrap()
        .unwrap();
    let plan = build_hierarchy(
        &registry,
        owner,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    (registry, loader, owner, heap)
}

fn field(registry: &DefinitionRegistry, owner: ClassId, name: &str, descriptor: &str) -> FieldId {
    registry
        .find_declared_field(owner, &units(name), &units(descriptor))
        .unwrap()
        .unwrap()
}

#[test]
fn genuine_instances_have_defaults_and_owner_qualified_fields_without_construction() {
    let (registry, loader, owner, mut heap) = setup();
    let object = heap.allocate_instance(owner).unwrap();
    assert_eq!(heap.instance_class(object).unwrap(), owner);
    assert_eq!(heap.total_instance_slots(), 18);
    assert_eq!(heap.retained_layout_fields(), 18);
    for (name, descriptor, default) in [
        ("flag", "Z", Value::Int(0)),
        ("small", "B", Value::Int(0)),
        ("letter", "C", Value::Int(0)),
        ("narrow", "S", Value::Int(0)),
        ("integer", "I", Value::Int(0)),
        ("wide", "J", Value::Long(0)),
        ("single", "F", Value::Float(0)),
        ("precise", "D", Value::Double(0)),
        (
            "reference",
            "Ljava/lang/Object;",
            Value::Reference(Reference::null()),
        ),
        ("primitiveArray", "[I", Value::Reference(Reference::null())),
        (
            "objectArray",
            "[[Ljava/lang/Object;",
            Value::Reference(Reference::null()),
        ),
        ("hidden", "I", Value::Int(0)),
        ("immutable", "I", Value::Int(0)),
    ] {
        assert_eq!(
            heap.get_field(object, field(&registry, owner, name, descriptor))
                .unwrap(),
            default,
            "{name}"
        );
    }
    let immutable = field(&registry, owner, "immutable", "I");
    assert!(
        registry
            .field(immutable)
            .unwrap()
            .attributes
            .iter()
            .any(|a| matches!(a.body, AttributeBody::ConstantValue(_)))
    );
    assert_eq!(
        heap.put_field(object, immutable, Value::Int(42)),
        Err(InstanceError::FinalField { field: immutable })
    );
    let static_field = field(&registry, owner, "staticOnly", "I");
    assert_eq!(
        heap.get_field(object, static_field),
        Err(InstanceError::MissingField {
            class: owner,
            field: static_field
        })
    );
    let mut hidden = Vec::new();
    for (index, name) in ["HierarchyBase", "HierarchyChild", "InstanceFields"]
        .iter()
        .enumerate()
    {
        let ancestor = registry
            .lookup_class(loader, &units(&format!("fixtures/{name}")))
            .unwrap()
            .unwrap();
        let handle = field(&registry, ancestor, "hidden", "I");
        heap.put_field(object, handle, Value::Int(index as i32 + 11))
            .unwrap();
        hidden.push(handle);
    }
    for (index, handle) in hidden.into_iter().enumerate() {
        assert_eq!(
            heap.get_field(object, handle).unwrap(),
            Value::Int(index as i32 + 11)
        );
    }
    assert_eq!(heap.children(object).unwrap().count(), 0);
    assert_eq!(heap.allocated_arrays(), 0);
    assert_eq!(heap.allocated_instances(), 1);
}

fn value(text: &str) -> Value {
    let (kind, number) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(number.parse().unwrap()),
        "L" => Value::Long(number.parse().unwrap()),
        "F" => Value::Float(u32::from_str_radix(number, 16).unwrap()),
        "D" => Value::Double(u64::from_str_radix(number, 16).unwrap()),
        _ => panic!("unexpected input {text}"),
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
        Value::Reference(_) => panic!("numeric field returned a reference"),
    }
}

#[test]
fn instance_fields_match_151_raw_putfield_jdk21_outcomes() {
    let (registry, _, owner, mut heap) = setup();
    let object = heap.allocate_instance(owner).unwrap();
    let mut methods = BTreeSet::new();
    let reference = include_str!("../../../tests/fixtures/reference/fields.tsv");
    for line in reference.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let name = columns[0].strip_suffix("RoundTrip").unwrap();
        let descriptor = match name {
            "flag" => "Z",
            "small" => "B",
            "letter" => "C",
            "narrow" => "S",
            "integer" => "I",
            "wide" => "J",
            "single" => "F",
            "precise" => "D",
            _ => panic!("unexpected oracle method"),
        };
        let computational = if "ZBCSI".contains(descriptor) {
            "I"
        } else {
            descriptor
        };
        assert_eq!(columns[1], format!("({computational}){computational}"));
        let handle = field(&registry, owner, name, descriptor);
        heap.put_field(object, handle, value(columns[2])).unwrap();
        assert_eq!(
            observed(heap.get_field(object, handle).unwrap()),
            columns[3],
            "{line}"
        );
        methods.insert(name);
    }
    assert_eq!(reference.lines().count(), 151);
    assert_eq!(methods.len(), 8);
    assert_eq!(heap.allocated_instances(), 1);
}

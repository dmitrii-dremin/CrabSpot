use crabspot_classfile::{AttributeBody, ParseOptions};
use crabspot_runtime::{
    DefinitionRegistry, Reference, RegistryOptions, SameLoaderResolver, StaticStorage,
    StaticStorageOptions, Value, build_hierarchy,
};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn structural_root() -> Vec<u8> {
    // Explicit synthetic ancestry input, with no behavior or production fallback.
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 3, 1, 0, 16];
    bytes.extend_from_slice(b"java/lang/Object");
    bytes.extend_from_slice(&[7, 0, 1, 0, 0x21, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

#[test]
fn compiled_static_declarations_receive_defaults_without_constants_or_initializer_execution() {
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
            "StorageDefaults",
            include_bytes!("../../../tests/fixtures/classes/fixtures/StorageDefaults.class"),
        ),
    ];
    for (name, bytes) in inputs {
        registry
            .define(
                loader,
                &units(&format!("fixtures/{name}")),
                bytes,
                ParseOptions::default(),
            )
            .unwrap();
    }
    let owner = registry
        .lookup_class(loader, &units("fixtures/StorageDefaults"))
        .unwrap()
        .unwrap();
    let class = registry.class(owner).unwrap();
    assert_eq!(class.version.major, 65);
    let constant_fields = class
        .fields
        .iter()
        .filter(|field| {
            field
                .attributes
                .iter()
                .any(|attribute| matches!(attribute.body, AttributeBody::ConstantValue(_)))
        })
        .count();
    assert_eq!(constant_fields, 5);
    let initializer = registry
        .find_declared_method(owner, &units("<clinit>"), &units("()V"))
        .unwrap()
        .unwrap();
    assert!(registry.method(initializer).unwrap().attributes.iter().any(
        |attribute| matches!(&attribute.body, AttributeBody::Code(code) if !code.code.is_empty())
    ));

    let plan = build_hierarchy(
        &registry,
        owner,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let before = registry.total_definition_bytes();
    let storage =
        StaticStorage::new(&registry, &plan, StaticStorageOptions { max_slots: 21 }).unwrap();
    assert_eq!(storage.slot_count(), 21);
    let expected_order: Vec<_> = plan
        .classes()
        .iter()
        .flat_map(|class| plan.layout(*class).unwrap().declared_static_fields())
        .copied()
        .collect();
    assert_eq!(storage.fields(), expected_order);

    let expected = [
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
            "[[Ljava/lang/String;",
            Value::Reference(Reference::null()),
        ),
        ("counter", "I", Value::Int(0)),
        ("INTEGER_CONSTANT", "I", Value::Int(0)),
        ("LONG_CONSTANT", "J", Value::Long(0)),
        ("FLOAT_CONSTANT", "F", Value::Float(0)),
        ("DOUBLE_CONSTANT", "D", Value::Double(0)),
        (
            "STRING_CONSTANT",
            "Ljava/lang/String;",
            Value::Reference(Reference::null()),
        ),
    ];
    for (name, descriptor, default) in expected {
        let field = registry
            .find_declared_field(owner, &units(name), &units(descriptor))
            .unwrap()
            .unwrap();
        assert_eq!(storage.get(field).unwrap(), default, "{name}");
    }
    // Hidden inherited statics and interface constants each keep their owner.
    for (name, field_name, descriptor, default) in [
        ("HierarchyBase", "counter", "I", Value::Int(0)),
        ("HierarchyChild", "counter", "J", Value::Long(0)),
        ("HierarchyParentContract", "TOKEN", "J", Value::Long(0)),
        (
            "HierarchyContract",
            "TEXT",
            "Ljava/lang/String;",
            Value::Reference(Reference::null()),
        ),
    ] {
        let ancestor = registry
            .lookup_class(loader, &units(&format!("fixtures/{name}")))
            .unwrap()
            .unwrap();
        let field = registry
            .find_declared_field(ancestor, &units(field_name), &units(descriptor))
            .unwrap()
            .unwrap();
        assert_eq!(storage.get(field).unwrap(), default);
        assert_eq!(
            storage
                .fields()
                .iter()
                .filter(|&&candidate| candidate == field)
                .count(),
            1
        );
    }
    let instance = registry
        .find_declared_field(owner, &units("onlyInstance"), &units("I"))
        .unwrap()
        .unwrap();
    assert!(storage.get(instance).is_err());
    assert!(storage.heap_roots().next().is_none());
    assert!(StaticStorage::new(&registry, &plan, StaticStorageOptions { max_slots: 20 }).is_err());
    assert_eq!(registry.total_definition_bytes(), before);
    assert_eq!(registry.definition_count(), 6);
    assert_eq!(storage.slot_count(), 21);
}

use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldId, HierarchyOptions, LoaderId, RegistryOptions,
    SameLoaderResolver, build_hierarchy,
};

const BASE: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/HierarchyBase.class");
const CHILD: &[u8] =
    include_bytes!("../../../tests/fixtures/classes/fixtures/HierarchyChild.class");
const PARENT_CONTRACT: &[u8] =
    include_bytes!("../../../tests/fixtures/classes/fixtures/HierarchyParentContract.class");
const CONTRACT: &[u8] =
    include_bytes!("../../../tests/fixtures/classes/fixtures/HierarchyContract.class");

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

// Explicit synthetic test input, not copied JDK code or a production Object
// fallback. It has no methods/fields and supplies ancestry only. Nothing here
// executes a constructor, verifies a method, or claims java.base compatibility.
fn structural_root() -> Vec<u8> {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 3, 1, 0, 16];
    bytes.extend_from_slice(b"java/lang/Object");
    bytes.extend_from_slice(&[
        7, 0, 1, // Class #1
        0, 0x21, 0, 2, 0, 0, // public/super, this, no superclass
        0, 0, 0, 0, 0, 0, 0, 0, // no interfaces, fields, methods, attributes
    ]);
    bytes
}

fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    bytes: &[u8],
) -> ClassId {
    registry
        .define(loader, &units(name), bytes, ParseOptions::default())
        .unwrap()
}

fn fixture_graph(registry: &mut DefinitionRegistry, loader: LoaderId) -> [ClassId; 5] {
    // Deliberately register leaves before dependencies. Registration order must
    // not masquerade as a resolved graph order.
    let child = define(registry, loader, "fixtures/HierarchyChild", CHILD);
    let contract = define(registry, loader, "fixtures/HierarchyContract", CONTRACT);
    let parent = define(
        registry,
        loader,
        "fixtures/HierarchyParentContract",
        PARENT_CONTRACT,
    );
    let base = define(registry, loader, "fixtures/HierarchyBase", BASE);
    let root = define(registry, loader, "java/lang/Object", &structural_root());
    [root, base, parent, contract, child]
}

fn field(registry: &DefinitionRegistry, owner: ClassId, name: &str, descriptor: &str) -> FieldId {
    registry
        .find_declared_field(owner, &units(name), &units(descriptor))
        .unwrap()
        .unwrap()
}

#[test]
fn compiled_inheritance_preserves_hidden_field_locations_and_declaring_static_owners() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let [root, base, parent, contract, child] = fixture_graph(&mut registry, loader);
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    assert_eq!(plan.root(), child);
    assert_eq!(plan.classes().len(), 5);
    for &class in plan.classes() {
        let layout = plan.layout(class).unwrap();
        let current = plan.classes().iter().position(|&id| id == class).unwrap();
        for dependency in layout
            .direct_superclass()
            .iter()
            .chain(layout.direct_interfaces())
        {
            let position = plan
                .classes()
                .iter()
                .position(|id| id == dependency)
                .unwrap();
            assert!(position < current);
        }
    }
    assert_eq!(plan.layout(root).unwrap().instance_slots(), 0);
    let base_layout = plan.layout(base).unwrap();
    assert_eq!(base_layout.instance_slots(), 3);
    assert_eq!(base_layout.inherited_instance_slots(), 0);
    let child_layout = plan.layout(child).unwrap();
    assert_eq!(child_layout.direct_superclass(), Some(base));
    assert_eq!(child_layout.direct_interfaces(), &[contract]);
    assert_eq!(child_layout.instance_slots(), 5);
    assert_eq!(child_layout.inherited_instance_slots(), 3);
    assert_eq!(child_layout.declared_instance_fields().len(), 2);

    let inherited_hidden = field(&registry, base, "hidden", "I");
    let child_hidden = field(&registry, child, "hidden", "I");
    assert_ne!(inherited_hidden, child_hidden);
    assert_eq!(plan.instance_field_slot(child, inherited_hidden), Some(0));
    assert_eq!(plan.instance_field_slot(child, child_hidden), Some(3));
    assert_eq!(plan.instance_field_slot(base, child_hidden), None);
    assert_eq!(
        plan.instance_field_slot(child, field(&registry, base, "wide", "J")),
        Some(1)
    );
    assert_eq!(
        plan.instance_field_slot(
            child,
            field(&registry, base, "reference", "Ljava/lang/Object;")
        ),
        Some(2)
    );
    assert_eq!(
        plan.instance_field_slot(child, field(&registry, child, "fraction", "D")),
        Some(4)
    );

    let base_static = field(&registry, base, "counter", "I");
    let child_static = field(&registry, child, "counter", "J");
    assert_eq!(base_layout.declared_static_fields(), &[base_static]);
    assert_eq!(child_layout.declared_static_fields(), &[child_static]);
    assert_eq!(plan.declared_static_field_slot(base_static), Some(0));
    assert_eq!(plan.declared_static_field_slot(child_static), Some(0));
    assert_eq!(plan.instance_field_slot(child, base_static), None);
    assert_eq!(plan.declared_static_field_slot(child_hidden), None);

    for interface in [parent, contract] {
        let layout = plan.layout(interface).unwrap();
        assert_eq!(layout.instance_slots(), 0);
        assert_eq!(layout.inherited_instance_slots(), 0);
        assert!(layout.declared_instance_fields().is_empty());
        assert_eq!(layout.declared_static_fields().len(), 1);
    }
    assert_eq!(
        plan.layout(contract).unwrap().direct_interfaces(),
        &[parent]
    );
    assert_eq!(registry.definition_count(), 5);
}

#[test]
fn genuine_metadata_does_not_synthesize_missing_ancestry() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let base = define(&mut registry, loader, "fixtures/HierarchyBase", BASE);
    let before = registry.total_definition_bytes();
    assert!(
        build_hierarchy(
            &registry,
            base,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .is_err()
    );
    assert_eq!(registry.definition_count(), 1);
    assert_eq!(registry.total_definition_bytes(), before);
    assert!(
        registry
            .lookup_class(loader, &units("java/lang/Object"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn two_compiled_graphs_with_equal_names_retain_separate_layout_identities() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let first_loader = registry.create_loader().unwrap();
    let second_loader = registry.create_loader().unwrap();
    let [_, base, _, _, child] = fixture_graph(&mut registry, first_loader);
    let [_, other_base, _, _, other_child] = fixture_graph(&mut registry, second_loader);
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let other_plan = build_hierarchy(
        &registry,
        other_child,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let hidden = field(&registry, base, "hidden", "I");
    let other_hidden = field(&registry, other_base, "hidden", "I");
    assert!(plan.layout(other_child).is_none());
    assert!(other_plan.layout(child).is_none());
    assert_eq!(plan.instance_field_slot(child, hidden), Some(0));
    assert_eq!(
        other_plan.instance_field_slot(other_child, other_hidden),
        Some(0)
    );
    assert_eq!(plan.instance_field_slot(child, other_hidden), None);
    assert_eq!(other_plan.instance_field_slot(other_child, hidden), None);
}

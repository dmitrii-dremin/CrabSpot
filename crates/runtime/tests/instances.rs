//! Public memory-service tests. Allocation here does not execute Java constructors.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldId, HeapEntryKind, HeapError, HeapOptions, HierarchyOptions,
    HierarchyPlan, HierarchyResolver, InstanceClassKind, InstanceError as E, InstanceOptions,
    InstanceResource as R, LoaderId, ManagedHeap, PrimitiveArrayKind, Reference, RegistryOptions,
    ResolveError, SameLoaderResolver, Value, ValueKind, build_hierarchy,
};
use std::collections::HashMap;

fn name(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}

#[derive(Clone)]
enum Constant<'a> {
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    String(&'a str),
}
struct Field<'a> {
    name: &'a str,
    descriptor: &'a str,
    flags: u16,
    constant: Option<Constant<'a>>,
}
impl<'a> Field<'a> {
    fn new(name: &'a str, descriptor: &'a str) -> Self {
        Self {
            name,
            descriptor,
            flags: 1,
            constant: None,
        }
    }
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn add(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let index = (self.0.len() + 1) as u16;
        let mut bytes = vec![tag];
        bytes.extend(payload);
        self.0.push(bytes);
        if matches!(tag, 5 | 6) {
            self.0.push(Vec::new());
        }
        index
    }
    fn text(&mut self, text: &str) -> u16 {
        assert!(text.is_ascii());
        let mut bytes = words(&[text.len() as u16]);
        bytes.extend(text.bytes());
        self.add(1, &bytes)
    }
    fn class(&mut self, text: &str) -> u16 {
        let index = self.text(text);
        self.add(7, &words(&[index]))
    }
    fn constant(&mut self, value: &Constant<'_>) -> u16 {
        match value {
            Constant::Int(value) => self.add(3, &value.to_be_bytes()),
            Constant::Long(value) => self.add(5, &value.to_be_bytes()),
            Constant::Float(bits) => self.add(4, &bits.to_be_bytes()),
            Constant::Double(bits) => self.add(6, &bits.to_be_bytes()),
            Constant::String(text) => {
                let index = self.text(text);
                self.add(8, &words(&[index]))
            }
        }
    }
}
fn definition(
    class: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[Field<'_>],
) -> Vec<u8> {
    let mut pool = Pool::default();
    let this = pool.class(class);
    let parent = parent.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let constant_name = pool.text("ConstantValue");
    let fields: Vec<_> = fields
        .iter()
        .map(|field| {
            (
                pool.text(field.name),
                pool.text(field.descriptor),
                field.flags,
                field.constant.as_ref().map(|value| pool.constant(value)),
            )
        })
        .collect();
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, this, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[fields.len() as u16]));
    for (name, descriptor, flags, constant) in fields {
        bytes.extend(words(&[
            flags,
            name,
            descriptor,
            u16::from(constant.is_some()),
        ]));
        if let Some(index) = constant {
            bytes.extend(words(&[constant_name]));
            bytes.extend(2_u32.to_be_bytes());
            bytes.extend(words(&[index]));
        }
    }
    bytes.extend(words(&[0, 0]));
    bytes
}
fn registry() -> DefinitionRegistry {
    DefinitionRegistry::new(RegistryOptions::default()).unwrap()
}
fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    class: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[Field<'_>],
) -> ClassId {
    registry
        .define(
            loader,
            &name(class),
            &definition(class, parent, interfaces, flags, fields),
            ParseOptions::default(),
        )
        .unwrap()
}
fn object(registry: &mut DefinitionRegistry, loader: LoaderId) -> ClassId {
    define(registry, loader, "java/lang/Object", None, &[], 0x21, &[])
}
fn field(registry: &DefinitionRegistry, class: ClassId, field: &str, descriptor: &str) -> FieldId {
    registry
        .find_declared_field(class, &name(field), &name(descriptor))
        .unwrap()
        .unwrap()
}
fn plan(registry: &DefinitionRegistry, class: ClassId) -> HierarchyPlan {
    build_hierarchy(
        registry,
        class,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap()
}
fn heap(options: InstanceOptions) -> ManagedHeap {
    ManagedHeap::new_with_instances(HeapOptions::default(), options).unwrap()
}
fn layout_counts(heap: &ManagedHeap) -> (usize, usize, usize) {
    (
        heap.retained_layout_classes(),
        heap.retained_layout_edges(),
        heap.retained_layout_fields(),
    )
}
fn instance_counts(heap: &ManagedHeap) -> (usize, usize) {
    (heap.allocated_instances(), heap.total_instance_slots())
}

#[derive(Default)]
struct Resolver(HashMap<(LoaderId, Vec<u16>), ClassId>);
impl HierarchyResolver for Resolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        requested: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        if let Some(class) = self.0.get(&(loader, requested.to_vec())) {
            return Ok(Some(*class));
        }
        registry
            .lookup_class(loader, requested)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

#[test]
fn original_constructor_disables_every_instance_operation_but_keeps_array_children() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "example/Disabled",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let value = field(&registry, class, "value", "I");
    let plan = plan(&registry, class);
    let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
    assert_eq!(
        heap.admit_instance_layouts(&registry, &[]).unwrap_err(),
        E::Disabled
    );
    assert_eq!(
        heap.admit_instance_layouts(&registry, &[&plan])
            .unwrap_err(),
        E::Disabled
    );
    assert_eq!(heap.allocate_instance(class).unwrap_err(), E::Disabled);
    assert_eq!(
        heap.instance_class(Reference::null()).unwrap_err(),
        E::Disabled
    );
    assert_eq!(
        heap.get_field(Reference::null(), value).unwrap_err(),
        E::Disabled
    );
    assert_eq!(
        heap.put_field(Reference::null(), value, Value::Int(0))
            .unwrap_err(),
        E::Disabled
    );
    assert_eq!(layout_counts(&heap), (0, 0, 0));
    assert_eq!(instance_counts(&heap), (0, 0));
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    assert_eq!(heap.children(array).unwrap().count(), 0);
    assert_eq!(
        heap.children(Reference::null()).err().unwrap(),
        HeapError::NullReference
    );
}

#[test]
fn enabled_empty_admission_never_binds_a_registry_even_after_another_binding() {
    let mut first = registry();
    let first_loader = first.create_loader().unwrap();
    let first_object = object(&mut first, first_loader);
    let first_plan = plan(&first, first_object);
    let mut second = registry();
    let second_loader = second.create_loader().unwrap();
    let second_object = object(&mut second, second_loader);
    let second_plan = plan(&second, second_object);
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&first, &[]).unwrap();
    heap.admit_instance_layouts(&second, &[]).unwrap();
    assert_eq!(layout_counts(&heap), (0, 0, 0));
    heap.admit_instance_layouts(&second, &[&second_plan])
        .unwrap();
    heap.admit_instance_layouts(&first, &[]).unwrap();
    assert_eq!(
        heap.admit_instance_layouts(&first, &[&first_plan])
            .unwrap_err(),
        E::ForeignRegistry
    );
    assert_eq!(
        heap.allocate_instance(first_object).unwrap_err(),
        E::ForeignRegistry
    );
    let reference = heap.allocate_instance(second_object).unwrap();
    assert_eq!(heap.instance_class(reference).unwrap(), second_object);
}

#[test]
fn every_field_descriptor_has_an_exact_default_and_constant_values_are_ignored() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let rank_255 = format!("{}I", "[".repeat(255));
    let descriptors = [
        "Z",
        "B",
        "C",
        "S",
        "I",
        "J",
        "F",
        "D",
        "Ljava/lang/Object;",
        "Ljava/lang/String;",
        "Lmissing/Unresolved;",
        "[Z",
        "[B",
        "[C",
        "[S",
        "[I",
        "[J",
        "[F",
        "[D",
        "[Ljava/lang/Object;",
        "[[I",
        &rank_255,
    ];
    let names: Vec<_> = (0..descriptors.len())
        .map(|index| format!("field{index}"))
        .collect();
    let mut fields: Vec<_> = names
        .iter()
        .zip(descriptors)
        .map(|(name, descriptor)| Field::new(name, descriptor))
        .collect();
    for (index, value) in [
        Constant::Int(1),
        Constant::Int(127),
        Constant::Int(65),
        Constant::Int(999),
        Constant::Int(123),
        Constant::Long(456),
        Constant::Float(0x8000_0000),
        Constant::Double(0x8000_0000_0000_0000),
    ]
    .into_iter()
    .enumerate()
    {
        fields[index].constant = Some(value);
    }
    fields[9].constant = Some(Constant::String("no constructor or initializer runs"));
    let class = define(
        &mut registry,
        loader,
        "example/Defaults",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
    );
    let plan = plan(&registry, class);
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let reference = heap.allocate_instance(class).unwrap();
    assert!(!reference.is_null());
    assert_eq!(heap.instance_class(reference).unwrap(), class);
    for (field_name, descriptor) in names.iter().zip(descriptors) {
        let expected = match descriptor {
            "Z" | "B" | "C" | "S" | "I" => Value::Int(0),
            "J" => Value::Long(0),
            "F" => Value::Float(0),
            "D" => Value::Double(0),
            _ => Value::Reference(Reference::null()),
        };
        assert_eq!(
            heap.get_field(reference, field(&registry, class, field_name, descriptor))
                .unwrap(),
            expected,
            "{descriptor}"
        );
    }
    assert_eq!(instance_counts(&heap), (1, descriptors.len()));
    assert_eq!(heap.children(reference).unwrap().count(), 0);
    heap.validate_root(reference.heap_ref().unwrap()).unwrap();
}

#[test]
fn hidden_inherited_fields_keep_distinct_owners_and_static_fields_use_no_instance_slots() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "example/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("hidden", "I"),
            Field {
                name: "static",
                descriptor: "J",
                flags: 9,
                constant: None,
            },
        ],
    );
    let child = define(
        &mut registry,
        loader,
        "example/Child",
        Some("example/Base"),
        &[],
        0x21,
        &[Field::new("hidden", "J")],
    );
    let base_field = field(&registry, base, "hidden", "I");
    let child_field = field(&registry, child, "hidden", "J");
    let static_field = field(&registry, base, "static", "J");
    let plan = plan(&registry, child);
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(child).unwrap();
    heap.put_field(receiver, base_field, Value::Int(17))
        .unwrap();
    heap.put_field(receiver, child_field, Value::Long(900))
        .unwrap();
    assert_eq!(
        heap.get_field(receiver, base_field).unwrap(),
        Value::Int(17)
    );
    assert_eq!(
        heap.get_field(receiver, child_field).unwrap(),
        Value::Long(900)
    );
    assert_eq!(
        heap.get_field(receiver, static_field).unwrap_err(),
        E::MissingField {
            class: child,
            field: static_field
        }
    );
    let base_receiver = heap.allocate_instance(base).unwrap();
    assert_eq!(
        heap.get_field(base_receiver, child_field).unwrap_err(),
        E::MissingField {
            class: base,
            field: child_field
        }
    );
    assert_eq!(
        heap.get_field(base_receiver, base_field).unwrap(),
        Value::Int(0)
    );
    assert_eq!(layout_counts(&heap), (3, 2, 2));
    assert_eq!(instance_counts(&heap), (2, 3));
}

#[test]
fn batch_and_sequential_admission_deduplicate_shared_ancestors_interfaces_and_roots() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "batch/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("base", "I")],
    );
    let interface = define(
        &mut registry,
        loader,
        "batch/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[Field {
            name: "constant",
            descriptor: "I",
            flags: 0x19,
            constant: Some(Constant::Int(7)),
        }],
    );
    let left = define(
        &mut registry,
        loader,
        "batch/Left",
        Some("batch/Base"),
        &["batch/I"],
        0x21,
        &[Field::new("left", "J")],
    );
    let right = define(
        &mut registry,
        loader,
        "batch/Right",
        Some("batch/Base"),
        &[],
        0x21,
        &[Field::new("right", "D")],
    );
    let left_plan = plan(&registry, left);
    let right_plan = plan(&registry, right);
    let options = InstanceOptions {
        max_layout_classes: 5,
        max_layout_edges: 5,
        max_layout_fields: 3,
        ..InstanceOptions::default()
    };
    let mut batch = heap(options);
    batch
        .admit_instance_layouts(&registry, &[&left_plan, &right_plan, &left_plan])
        .unwrap();
    let mut sequential = heap(options);
    sequential
        .admit_instance_layouts(&registry, &[&left_plan])
        .unwrap();
    sequential
        .admit_instance_layouts(&registry, &[&right_plan])
        .unwrap();
    assert_eq!(layout_counts(&batch), (5, 5, 3));
    assert_eq!(layout_counts(&sequential), layout_counts(&batch));
    for receiver_class in [left, right] {
        let first = batch.allocate_instance(receiver_class).unwrap();
        let second = sequential.allocate_instance(receiver_class).unwrap();
        let inherited = field(&registry, base, "base", "I");
        batch.put_field(first, inherited, Value::Int(17)).unwrap();
        sequential
            .put_field(second, inherited, Value::Int(17))
            .unwrap();
        assert_eq!(
            batch.get_field(first, inherited).unwrap(),
            sequential.get_field(second, inherited).unwrap()
        );
    }
    assert_eq!(
        batch.allocate_instance(interface).unwrap_err(),
        E::NonInstantiable {
            class: interface,
            kind: InstanceClassKind::Interface
        }
    );
    batch
        .admit_instance_layouts(&registry, &[&right_plan, &left_plan])
        .unwrap();
    assert_eq!(layout_counts(&batch), (5, 5, 3));
    assert_eq!(instance_counts(&batch), (2, 4));
}

struct Delegated {
    registry: DefinitionRegistry,
    first: HierarchyPlan,
    second: HierarchyPlan,
    root: ClassId,
    other_root: ClassId,
    middle: ClassId,
    first_parent: ClassId,
    second_parent: ClassId,
}
fn delegated_parents() -> Delegated {
    let mut registry = registry();
    let shared = registry.create_loader().unwrap();
    let first_loader = registry.create_loader().unwrap();
    let second_loader = registry.create_loader().unwrap();
    let object = object(&mut registry, shared);
    let first_parent = define(
        &mut registry,
        first_loader,
        "delegated/Parent",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let second_parent = define(
        &mut registry,
        second_loader,
        "delegated/Parent",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let middle = define(
        &mut registry,
        shared,
        "delegated/Middle",
        Some("delegated/Parent"),
        &[],
        0x21,
        &[Field::new("middle", "I")],
    );
    let root = define(
        &mut registry,
        shared,
        "delegated/First",
        Some("delegated/Middle"),
        &[],
        0x21,
        &[Field::new("leaf", "I")],
    );
    let other_root = define(
        &mut registry,
        shared,
        "delegated/Second",
        Some("delegated/Middle"),
        &[],
        0x21,
        &[Field::new("leaf", "I")],
    );
    let mut first_resolver = Resolver::default();
    first_resolver
        .0
        .insert((shared, name("delegated/Parent")), first_parent);
    first_resolver
        .0
        .insert((first_loader, name("java/lang/Object")), object);
    let first = build_hierarchy(
        &registry,
        root,
        &mut first_resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let mut second_resolver = Resolver::default();
    second_resolver
        .0
        .insert((shared, name("delegated/Parent")), second_parent);
    second_resolver
        .0
        .insert((second_loader, name("java/lang/Object")), object);
    let second = build_hierarchy(
        &registry,
        other_root,
        &mut second_resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    Delegated {
        registry,
        first,
        second,
        root,
        other_root,
        middle,
        first_parent,
        second_parent,
    }
}

#[test]
fn same_dependency_class_with_different_equal_slot_parents_conflicts_in_batch_and_sequence() {
    let fixture = delegated_parents();
    assert_eq!(
        fixture
            .first
            .layout(fixture.middle)
            .unwrap()
            .instance_slots(),
        fixture
            .second
            .layout(fixture.middle)
            .unwrap()
            .instance_slots()
    );
    assert_ne!(fixture.first_parent, fixture.second_parent);
    let mut batch = heap(InstanceOptions::default());
    assert_eq!(
        batch
            .admit_instance_layouts(&fixture.registry, &[&fixture.first, &fixture.second])
            .unwrap_err(),
        E::LayoutConflict {
            class: fixture.middle
        }
    );
    assert_eq!(layout_counts(&batch), (0, 0, 0));
    assert_eq!(
        batch.allocate_instance(fixture.root).unwrap_err(),
        E::LayoutMissing {
            class: fixture.root
        }
    );
    let mut sequential = heap(InstanceOptions::default());
    sequential
        .admit_instance_layouts(&fixture.registry, &[&fixture.first])
        .unwrap();
    let receiver = sequential.allocate_instance(fixture.root).unwrap();
    let inherited = field(&fixture.registry, fixture.first_parent, "value", "I");
    sequential
        .put_field(receiver, inherited, Value::Int(41))
        .unwrap();
    let before = layout_counts(&sequential);
    assert_eq!(
        sequential
            .admit_instance_layouts(&fixture.registry, &[&fixture.second])
            .unwrap_err(),
        E::LayoutConflict {
            class: fixture.middle
        }
    );
    assert_eq!(layout_counts(&sequential), before);
    assert_eq!(instance_counts(&sequential), (1, 3));
    assert_eq!(
        sequential.get_field(receiver, inherited).unwrap(),
        Value::Int(41)
    );
    assert_eq!(
        sequential
            .allocate_instance(fixture.other_root)
            .unwrap_err(),
        E::LayoutMissing {
            class: fixture.other_root
        }
    );
    sequential
        .admit_instance_layouts(&fixture.registry, &[&fixture.first])
        .unwrap();
    assert_eq!(
        sequential.get_field(receiver, inherited).unwrap(),
        Value::Int(41)
    );
}

#[test]
fn interface_identity_conflicts_are_atomic_and_a_failed_first_admission_leaves_binding_open() {
    let mut registry = registry();
    let shared = registry.create_loader().unwrap();
    let first_loader = registry.create_loader().unwrap();
    let second_loader = registry.create_loader().unwrap();
    let shared_object = object(&mut registry, shared);
    let first_interface = define(
        &mut registry,
        first_loader,
        "delegated/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let second_interface = define(
        &mut registry,
        second_loader,
        "delegated/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let root = define(
        &mut registry,
        shared,
        "delegated/Root",
        Some("java/lang/Object"),
        &["delegated/I"],
        0x21,
        &[],
    );
    let mut plans = Vec::new();
    for (interface, loader) in [
        (first_interface, first_loader),
        (second_interface, second_loader),
    ] {
        let mut resolver = Resolver::default();
        resolver.0.insert((shared, name("delegated/I")), interface);
        resolver
            .0
            .insert((loader, name("java/lang/Object")), shared_object);
        plans.push(
            build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap(),
        );
    }
    let mut failed = heap(InstanceOptions::default());
    assert_eq!(
        failed
            .admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
            .unwrap_err(),
        E::LayoutConflict { class: root }
    );
    assert_eq!(layout_counts(&failed), (0, 0, 0));
    let mut other = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let other_loader = other.create_loader().unwrap();
    let other_object = object(&mut other, other_loader);
    let other_plan = plan(&other, other_object);
    failed
        .admit_instance_layouts(&other, &[&other_plan])
        .unwrap();
    let receiver = failed.allocate_instance(other_object).unwrap();
    assert_eq!(failed.instance_class(receiver).unwrap(), other_object);
    assert_eq!(
        failed
            .admit_instance_layouts(&registry, &[&plans[0]])
            .unwrap_err(),
        E::ForeignRegistry
    );
    let mut sequential = heap(InstanceOptions::default());
    sequential
        .admit_instance_layouts(&registry, &[&plans[0]])
        .unwrap();
    let receiver = sequential.allocate_instance(root).unwrap();
    assert_eq!(
        sequential
            .admit_instance_layouts(&registry, &[&plans[1]])
            .unwrap_err(),
        E::LayoutConflict { class: root }
    );
    assert_eq!(layout_counts(&sequential), (3, 3, 0));
    assert_eq!(sequential.instance_class(receiver).unwrap(), root);
}

#[test]
fn late_batch_quota_failure_preserves_shapes_live_objects_and_allows_smaller_retry() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let classes: Vec<_> = ["late/A", "late/B", "late/C"]
        .into_iter()
        .map(|class| {
            define(
                &mut registry,
                loader,
                class,
                Some("java/lang/Object"),
                &[],
                0x21,
                &[Field::new("value", "I")],
            )
        })
        .collect();
    let plans: Vec<_> = classes
        .iter()
        .map(|class| plan(&registry, *class))
        .collect();
    let mut heap = heap(InstanceOptions {
        max_layout_fields: 2,
        ..InstanceOptions::default()
    });
    heap.admit_instance_layouts(&registry, &[&plans[0]])
        .unwrap();
    let receiver = heap.allocate_instance(classes[0]).unwrap();
    let value = field(&registry, classes[0], "value", "I");
    heap.put_field(receiver, value, Value::Int(73)).unwrap();
    assert_eq!(
        heap.admit_instance_layouts(&registry, &[&plans[1], &plans[2]])
            .unwrap_err(),
        E::ResourceLimit {
            resource: R::LayoutFields,
            limit: 2,
            required: 3
        }
    );
    assert_eq!(layout_counts(&heap), (2, 1, 1));
    assert_eq!(instance_counts(&heap), (1, 1));
    assert_eq!(heap.get_field(receiver, value).unwrap(), Value::Int(73));
    assert_eq!(
        heap.allocate_instance(classes[1]).unwrap_err(),
        E::LayoutMissing { class: classes[1] }
    );
    heap.admit_instance_layouts(&registry, &[&plans[1]])
        .unwrap();
    assert_eq!(layout_counts(&heap), (3, 2, 2));
    assert_eq!(heap.get_field(receiver, value).unwrap(), Value::Int(73));
}

#[test]
fn concrete_interface_abstract_missing_and_foreign_class_allocation_are_distinct() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let interface = define(
        &mut registry,
        loader,
        "kind/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let abstract_class = define(
        &mut registry,
        loader,
        "kind/Abstract",
        Some("java/lang/Object"),
        &[],
        0x421,
        &[Field::new("base", "I")],
    );
    let concrete = define(
        &mut registry,
        loader,
        "kind/Concrete",
        Some("kind/Abstract"),
        &["kind/I"],
        0x21,
        &[],
    );
    let missing = define(
        &mut registry,
        loader,
        "kind/Missing",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    );
    let plan = plan(&registry, concrete);
    let mut heap = heap(InstanceOptions::default());
    assert_eq!(
        heap.allocate_instance(concrete).unwrap_err(),
        E::LayoutMissing { class: concrete }
    );
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    assert_eq!(
        heap.allocate_instance(interface).unwrap_err(),
        E::NonInstantiable {
            class: interface,
            kind: InstanceClassKind::Interface
        }
    );
    assert_eq!(
        heap.allocate_instance(abstract_class).unwrap_err(),
        E::NonInstantiable {
            class: abstract_class,
            kind: InstanceClassKind::Abstract
        }
    );
    assert_eq!(
        heap.allocate_instance(missing).unwrap_err(),
        E::LayoutMissing { class: missing }
    );
    let receiver = heap.allocate_instance(concrete).unwrap();
    assert_eq!(heap.instance_class(receiver).unwrap(), concrete);
    let mut foreign = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign.create_loader().unwrap();
    let foreign_object = object(&mut foreign, foreign_loader);
    assert_eq!(
        heap.allocate_instance(foreign_object).unwrap_err(),
        E::ForeignRegistry
    );
    assert_eq!(instance_counts(&heap), (1, 1));
}

#[test]
fn instance_fields_apply_subword_rules_preserve_numeric_bits_and_reject_wrong_kinds() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let descriptors = ["Z", "B", "C", "S", "I", "J", "F", "D"];
    let fields: Vec<_> = descriptors
        .iter()
        .map(|descriptor| Field::new(descriptor, descriptor))
        .collect();
    let class = define(
        &mut registry,
        loader,
        "fields/Numeric",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
    );
    let plan = plan(&registry, class);
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(class).unwrap();
    for (descriptor, cases) in [
        (
            "Z",
            vec![(0, 0), (1, 1), (2, 0), (3, 1), (-1, 1), (i32::MIN, 0)],
        ),
        (
            "B",
            vec![(127, 127), (128, -128), (255, -1), (256, 0), (-129, 127)],
        ),
        (
            "C",
            vec![(-1, 65535), (65536, 0), (65537, 1), (i32::MIN, 0)],
        ),
        (
            "S",
            vec![
                (32767, 32767),
                (32768, -32768),
                (65535, -1),
                (65536, 0),
                (-32769, 32767),
            ],
        ),
        (
            "I",
            vec![(i32::MIN, i32::MIN), (i32::MAX, i32::MAX), (-1, -1)],
        ),
    ] {
        let id = field(&registry, class, descriptor, descriptor);
        for (input, expected) in cases {
            heap.put_field(receiver, id, Value::Int(input)).unwrap();
            assert_eq!(heap.get_field(receiver, id).unwrap(), Value::Int(expected));
        }
    }
    let long = field(&registry, class, "J", "J");
    for value in [i64::MIN, i64::MAX, -1] {
        heap.put_field(receiver, long, Value::Long(value)).unwrap();
        assert_eq!(heap.get_field(receiver, long).unwrap(), Value::Long(value));
    }
    let float = field(&registry, class, "F", "F");
    for bits in [0_u32, 0x8000_0000, 1, 0x7fa0_1234, 0xffc0_5678] {
        heap.put_field(receiver, float, Value::Float(bits)).unwrap();
        assert_eq!(heap.get_field(receiver, float).unwrap(), Value::Float(bits));
    }
    let double = field(&registry, class, "D", "D");
    for bits in [
        0_u64,
        0x8000_0000_0000_0000,
        1,
        0x7ff4_0000_0000_1234,
        0xfff8_0000_0000_5678,
    ] {
        heap.put_field(receiver, double, Value::Double(bits))
            .unwrap();
        assert_eq!(
            heap.get_field(receiver, double).unwrap(),
            Value::Double(bits)
        );
    }
    for descriptor in descriptors {
        let id = field(&registry, class, descriptor, descriptor);
        let before = heap.get_field(receiver, id).unwrap();
        for value in [
            Value::Int(0),
            Value::Long(0),
            Value::Float(0),
            Value::Double(0),
            Value::Reference(Reference::null()),
            Value::Reference(receiver),
        ] {
            if value.kind() == before.kind() {
                continue;
            }
            assert_eq!(
                heap.put_field(receiver, id, value).unwrap_err(),
                E::Heap(HeapError::ValueKindMismatch {
                    expected: before.kind(),
                    actual: value.kind()
                })
            );
            assert_eq!(heap.get_field(receiver, id).unwrap(), before);
        }
    }
    assert_eq!(instance_counts(&heap), (1, 8));
    assert_eq!(heap.children(receiver).unwrap().count(), 0);
}

#[test]
fn finality_precedes_value_validation_and_reference_fields_accept_only_null() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "fields/References",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field {
                name: "final",
                descriptor: "I",
                flags: 0x11,
                constant: None,
            },
            Field::new("object", "Ljava/lang/Object;"),
            Field::new("array", "[I"),
        ],
    );
    let plan = plan(&registry, class);
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(class).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    let final_field = field(&registry, class, "final", "I");
    for value in [Value::Int(0), Value::Int(1), Value::Reference(array)] {
        assert_eq!(
            heap.put_field(receiver, final_field, value).unwrap_err(),
            E::FinalField { field: final_field }
        );
        assert_eq!(
            heap.get_field(receiver, final_field).unwrap(),
            Value::Int(0)
        );
    }
    let foreign_reference = {
        let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
        foreign
            .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
            .unwrap()
    };
    for (name, descriptor) in [("object", "Ljava/lang/Object;"), ("array", "[I")] {
        let id = field(&registry, class, name, descriptor);
        heap.put_field(receiver, id, Value::Reference(Reference::null()))
            .unwrap();
        for reference in [receiver, array, foreign_reference] {
            assert_eq!(
                heap.put_field(receiver, id, Value::Reference(reference))
                    .unwrap_err(),
                E::UnsupportedReferenceStore { field: id }
            );
            assert_eq!(
                heap.get_field(receiver, id).unwrap(),
                Value::Reference(Reference::null())
            );
        }
        assert_eq!(
            heap.put_field(receiver, id, Value::Int(0)).unwrap_err(),
            E::Heap(HeapError::ValueKindMismatch {
                expected: ValueKind::Reference,
                actual: ValueKind::Int
            })
        );
    }
    assert_eq!(heap.children(receiver).unwrap().count(), 0);
    assert_eq!(heap.children(array).unwrap().count(), 0);
}

#[test]
fn receiver_identity_and_entry_kind_precede_field_registry_relationship_and_finality() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "checks/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let unrelated = define(
        &mut registry,
        loader,
        "checks/Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field {
            name: "final",
            descriptor: "I",
            flags: 0x11,
            constant: None,
        }],
    );
    let value = field(&registry, class, "value", "I");
    let unrelated_field = field(&registry, unrelated, "final", "I");
    let plan = plan(&registry, class);
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(class).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut foreign_registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign_registry.create_loader().unwrap();
    object(&mut foreign_registry, foreign_loader);
    let foreign_class = define(
        &mut foreign_registry,
        foreign_loader,
        "checks/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let foreign_field = field(&foreign_registry, foreign_class, "value", "I");
    let foreign_reference = {
        let mut foreign_heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        foreign_heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap()
    };
    for reference in [Reference::null(), foreign_reference, array] {
        let expected = if reference.is_null() {
            HeapError::NullReference
        } else if reference == array {
            HeapError::WrongEntryKind {
                expected: HeapEntryKind::Instance,
                actual: HeapEntryKind::PrimitiveArray,
            }
        } else {
            HeapError::ForeignHeap
        };
        assert_eq!(
            heap.get_field(reference, foreign_field).unwrap_err(),
            E::Heap(expected.clone())
        );
        assert_eq!(
            heap.put_field(reference, foreign_field, Value::Int(0))
                .unwrap_err(),
            E::Heap(expected)
        );
    }
    assert_eq!(
        heap.get_field(receiver, foreign_field).unwrap_err(),
        E::ForeignRegistry
    );
    assert_eq!(
        heap.put_field(receiver, foreign_field, Value::Int(0))
            .unwrap_err(),
        E::ForeignRegistry
    );
    assert_eq!(
        heap.put_field(receiver, unrelated_field, Value::Reference(receiver))
            .unwrap_err(),
        E::MissingField {
            class,
            field: unrelated_field
        }
    );
    assert_eq!(heap.get_field(receiver, value).unwrap(), Value::Int(0));
}

#[test]
fn all_layout_limits_are_atomic_and_failed_admission_can_retry_another_registry() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let object_class = object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "limits/One",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let plan = plan(&registry, class);
    let mut other = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let other_loader = other.create_loader().unwrap();
    let other_object = object(&mut other, other_loader);
    let other_plan = build_hierarchy(
        &other,
        other_object,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    for (options, resource, limit, required) in [
        (
            InstanceOptions {
                max_layout_classes: 1,
                ..InstanceOptions::default()
            },
            R::LayoutClasses,
            1,
            2,
        ),
        (
            InstanceOptions {
                max_layout_edges: 0,
                ..InstanceOptions::default()
            },
            R::LayoutEdges,
            0,
            1,
        ),
        (
            InstanceOptions {
                max_layout_fields: 0,
                ..InstanceOptions::default()
            },
            R::LayoutFields,
            0,
            1,
        ),
    ] {
        let mut heap = heap(options);
        assert_eq!(
            heap.admit_instance_layouts(&registry, &[&plan])
                .unwrap_err(),
            E::ResourceLimit {
                resource,
                limit,
                required
            }
        );
        assert_eq!(layout_counts(&heap), (0, 0, 0));
        heap.admit_instance_layouts(&other, &[&other_plan]).unwrap();
        let receiver = heap.allocate_instance(other_object).unwrap();
        assert_eq!(heap.instance_class(receiver).unwrap(), other_object);
        assert_eq!(
            heap.allocate_instance(object_class).unwrap_err(),
            E::ForeignRegistry
        );
    }
    let object_plan = build_hierarchy(
        &registry,
        object_class,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let mut zero = heap(InstanceOptions {
        max_layout_classes: 0,
        ..InstanceOptions::default()
    });
    zero.admit_instance_layouts(&registry, &[]).unwrap();
    assert_eq!(
        zero.admit_instance_layouts(&registry, &[&object_plan])
            .unwrap_err(),
        E::ResourceLimit {
            resource: R::LayoutClasses,
            limit: 0,
            required: 1
        }
    );
    assert_eq!(layout_counts(&zero), (0, 0, 0));
}

#[test]
fn exact_layout_limits_count_interface_edges_and_own_instance_fields_once() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    define(
        &mut registry,
        loader,
        "exact/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("base", "J")],
    );
    define(
        &mut registry,
        loader,
        "exact/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[Field {
            name: "static",
            descriptor: "I",
            flags: 0x19,
            constant: None,
        }],
    );
    let class = define(
        &mut registry,
        loader,
        "exact/Root",
        Some("exact/Base"),
        &["exact/I"],
        0x21,
        &[Field::new("own", "D")],
    );
    let plan = plan(&registry, class);
    let exact = InstanceOptions {
        max_layout_classes: 4,
        max_layout_edges: 4,
        max_layout_fields: 2,
        ..InstanceOptions::default()
    };
    let mut heap = heap(exact);
    heap.admit_instance_layouts(&registry, &[&plan, &plan])
        .unwrap();
    assert_eq!(layout_counts(&heap), (4, 4, 2));
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    assert_eq!(layout_counts(&heap), (4, 4, 2));
    let receiver = heap.allocate_instance(class).unwrap();
    assert_eq!(heap.instance_class(receiver).unwrap(), class);
    assert_eq!(instance_counts(&heap), (1, 2));
    for (options, resource, limit, required) in [
        (
            InstanceOptions {
                max_layout_classes: 3,
                ..exact
            },
            R::LayoutClasses,
            3,
            4,
        ),
        (
            InstanceOptions {
                max_layout_edges: 3,
                ..exact
            },
            R::LayoutEdges,
            3,
            4,
        ),
        (
            InstanceOptions {
                max_layout_fields: 1,
                ..exact
            },
            R::LayoutFields,
            1,
            2,
        ),
    ] {
        let mut heap = ManagedHeap::new_with_instances(HeapOptions::default(), options).unwrap();
        assert_eq!(
            heap.admit_instance_layouts(&registry, &[&plan])
                .unwrap_err(),
            E::ResourceLimit {
                resource,
                limit,
                required
            }
        );
        assert_eq!(layout_counts(&heap), (0, 0, 0));
    }
}

#[test]
fn instance_count_and_slot_quotas_include_empty_entries_and_allow_recovery() {
    let defaults = InstanceOptions::default();
    assert_eq!(defaults.max_layout_classes, 1024);
    assert_eq!(defaults.max_layout_edges, 8192);
    assert_eq!(defaults.max_layout_fields, 65536);
    assert_eq!(defaults.max_instances, 4096);
    assert_eq!(defaults.max_instance_slots, 65536);
    assert_eq!(defaults.max_total_instance_slots, 1_048_576);
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let object = object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "allocation/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("base", "I")],
    );
    let child = define(
        &mut registry,
        loader,
        "allocation/Child",
        Some("allocation/Base"),
        &[],
        0x21,
        &[Field::new("own", "J")],
    );
    let plan = plan(&registry, child);
    for (options, resource) in [
        (
            InstanceOptions {
                max_instances: 0,
                ..defaults
            },
            R::Instances,
        ),
        (
            InstanceOptions {
                max_instance_slots: 0,
                ..defaults
            },
            R::InstanceSlots,
        ),
        (
            InstanceOptions {
                max_total_instance_slots: 0,
                ..defaults
            },
            R::TotalInstanceSlots,
        ),
    ] {
        let mut heap = heap(options);
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        assert_eq!(
            heap.allocate_instance(base).unwrap_err(),
            E::ResourceLimit {
                resource,
                limit: 0,
                required: 1
            }
        );
        assert_eq!(instance_counts(&heap), (0, 0));
        if resource != R::Instances {
            let empty = heap.allocate_instance(object).unwrap();
            assert_eq!(heap.instance_class(empty).unwrap(), object);
            assert_eq!(instance_counts(&heap), (1, 0));
        }
    }
    let mut empty = heap(InstanceOptions {
        max_instances: 2,
        max_instance_slots: 0,
        max_total_instance_slots: 0,
        ..defaults
    });
    empty.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let first = empty.allocate_instance(object).unwrap();
    let second = empty.allocate_instance(object).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        empty.allocate_instance(object).unwrap_err(),
        E::ResourceLimit {
            resource: R::Instances,
            limit: 2,
            required: 3
        }
    );
    assert_eq!(instance_counts(&empty), (2, 0));
    let mut bounded = heap(InstanceOptions {
        max_instance_slots: 2,
        max_total_instance_slots: 3,
        ..defaults
    });
    bounded.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = bounded.allocate_instance(child).unwrap();
    let inherited = field(&registry, base, "base", "I");
    bounded
        .put_field(receiver, inherited, Value::Int(73))
        .unwrap();
    assert_eq!(
        bounded.allocate_instance(child).unwrap_err(),
        E::ResourceLimit {
            resource: R::TotalInstanceSlots,
            limit: 3,
            required: 4
        }
    );
    assert_eq!(instance_counts(&bounded), (1, 2));
    bounded.allocate_instance(base).unwrap();
    assert_eq!(instance_counts(&bounded), (2, 3));
    assert_eq!(
        bounded.get_field(receiver, inherited).unwrap(),
        Value::Int(73)
    );
    let mut per_object = heap(InstanceOptions {
        max_instance_slots: 1,
        ..defaults
    });
    per_object
        .admit_instance_layouts(&registry, &[&plan])
        .unwrap();
    assert_eq!(
        per_object.allocate_instance(child).unwrap_err(),
        E::ResourceLimit {
            resource: R::InstanceSlots,
            limit: 1,
            required: 2
        }
    );
    per_object.allocate_instance(base).unwrap();
    assert_eq!(instance_counts(&per_object), (1, 1));
}

#[test]
fn array_and_instance_entries_share_identity_validation_but_keep_independent_quotas() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "arena/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let value = field(&registry, class, "value", "I");
    let plan = plan(&registry, class);
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions {
            max_arrays: 1,
            ..HeapOptions::default()
        },
        InstanceOptions {
            max_instances: 1,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Long, 1)
        .unwrap();
    let instance = heap.allocate_instance(class).unwrap();
    assert_ne!(array, instance);
    heap.store(array, 0, Value::Long(77)).unwrap();
    heap.put_field(instance, value, Value::Int(41)).unwrap();
    for reference in [array, instance] {
        heap.validate_root(reference.heap_ref().unwrap()).unwrap();
        assert_eq!(heap.children(reference).unwrap().count(), 0);
    }
    let wrong_array = HeapError::WrongEntryKind {
        expected: HeapEntryKind::PrimitiveArray,
        actual: HeapEntryKind::Instance,
    };
    assert_eq!(heap.kind(instance).unwrap_err(), wrong_array);
    assert_eq!(
        heap.length(instance).unwrap_err(),
        HeapError::WrongEntryKind {
            expected: HeapEntryKind::Array,
            actual: HeapEntryKind::Instance,
        }
    );
    assert_eq!(heap.load(instance, 0).unwrap_err(), wrong_array);
    assert_eq!(
        heap.store(instance, 0, Value::Int(0)).unwrap_err(),
        wrong_array
    );
    assert_eq!(
        heap.instance_class(array).unwrap_err(),
        E::Heap(HeapError::WrongEntryKind {
            expected: HeapEntryKind::Instance,
            actual: HeapEntryKind::PrimitiveArray
        })
    );
    assert_eq!(heap.allocated_arrays(), 1);
    assert_eq!(heap.total_elements(), 1);
    assert_eq!(heap.payload_bytes(), 8);
    assert_eq!(instance_counts(&heap), (1, 1));
    assert_eq!(
        heap.allocate_instance(class).unwrap_err(),
        E::ResourceLimit {
            resource: R::Instances,
            limit: 1,
            required: 2
        }
    );
    assert!(
        heap.allocate_primitive_array(PrimitiveArrayKind::Byte, 0)
            .is_err()
    );
    assert_eq!(heap.load(array, 0).unwrap(), Value::Long(77));
    assert_eq!(heap.get_field(instance, value).unwrap(), Value::Int(41));
    let mut no_arrays = ManagedHeap::new_with_instances(
        HeapOptions {
            max_arrays: 0,
            ..HeapOptions::default()
        },
        InstanceOptions::default(),
    )
    .unwrap();
    no_arrays
        .admit_instance_layouts(&registry, &[&plan])
        .unwrap();
    no_arrays.allocate_instance(class).unwrap();
    assert_eq!(no_arrays.allocated_arrays(), 0);
    let mut no_instances = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_instances: 0,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    no_instances
        .admit_instance_layouts(&registry, &[&plan])
        .unwrap();
    no_instances
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    assert_eq!(instance_counts(&no_instances), (0, 0));
}

#[test]
fn old_instance_references_survive_new_layouts_arena_growth_and_heap_moves() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "growth/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let value = field(&registry, class, "value", "I");
    let first_plan = plan(&registry, class);
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&registry, &[&first_plan])
        .unwrap();
    let receiver = heap.allocate_instance(class).unwrap();
    heap.put_field(receiver, value, Value::Int(42)).unwrap();
    let original_handle = receiver.heap_ref().unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    heap.store(array, 0, Value::Int(73)).unwrap();
    let mut plans = Vec::new();
    for index in 0..128 {
        let name = format!("growth/C{index}");
        let class = define(
            &mut registry,
            loader,
            &name,
            Some("java/lang/Object"),
            &[],
            0x21,
            &[Field::new("value", "J")],
        );
        plans.push(plan(&registry, class));
    }
    let borrowed: Vec<_> = plans.iter().collect();
    heap.admit_instance_layouts(&registry, &borrowed).unwrap();
    for _ in 0..128 {
        heap.allocate_instance(class).unwrap();
        heap.allocate_primitive_array(PrimitiveArrayKind::Byte, 0)
            .unwrap();
    }
    let boxed = Box::new(heap);
    let mut moved = *boxed;
    moved.validate_root(original_handle).unwrap();
    assert_eq!(moved.get_field(receiver, value).unwrap(), Value::Int(42));
    moved.put_field(receiver, value, Value::Int(-17)).unwrap();
    assert_eq!(moved.get_field(receiver, value).unwrap(), Value::Int(-17));
    assert_eq!(moved.load(array, 0).unwrap(), Value::Int(73));
    assert_eq!(instance_counts(&moved), (129, 129));
    assert_eq!(moved.allocated_arrays(), 129);
    assert_eq!(moved.payload_bytes(), 4);
}

#[test]
fn foreign_plan_admission_is_checked_before_publication_and_does_not_bind() {
    let mut first = registry();
    let first_loader = first.create_loader().unwrap();
    let first_object = object(&mut first, first_loader);
    let first_plan = plan(&first, first_object);
    let mut second = registry();
    let second_loader = second.create_loader().unwrap();
    let second_object = object(&mut second, second_loader);
    let second_plan = plan(&second, second_object);
    let mut heap = heap(InstanceOptions::default());
    assert!(matches!(
        heap.admit_instance_layouts(&first, &[&second_plan])
            .unwrap_err(),
        E::Registry(_)
    ));
    assert_eq!(layout_counts(&heap), (0, 0, 0));
    heap.admit_instance_layouts(&second, &[&second_plan])
        .unwrap();
    assert_eq!(
        heap.admit_instance_layouts(&first, &[&first_plan])
            .unwrap_err(),
        E::ForeignRegistry
    );
    assert_eq!(layout_counts(&heap), (1, 0, 0));
    heap.allocate_instance(second_object).unwrap();
}

#[test]
fn equal_names_in_separate_defining_loaders_keep_distinct_shapes_fields_and_instances() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let first_class = define(
        &mut registry,
        first,
        "identity/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let second_class = define(
        &mut registry,
        second,
        "identity/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "I")],
    );
    let first_plan = plan(&registry, first_class);
    let second_plan = plan(&registry, second_class);
    let first_field = field(&registry, first_class, "value", "I");
    let second_field = field(&registry, second_class, "value", "I");
    let mut heap = heap(InstanceOptions::default());
    heap.admit_instance_layouts(&registry, &[&first_plan, &second_plan])
        .unwrap();
    let first_receiver = heap.allocate_instance(first_class).unwrap();
    let second_receiver = heap.allocate_instance(second_class).unwrap();
    assert_ne!(first_receiver, second_receiver);
    heap.put_field(first_receiver, first_field, Value::Int(17))
        .unwrap();
    heap.put_field(second_receiver, second_field, Value::Int(73))
        .unwrap();
    assert_eq!(
        heap.get_field(first_receiver, first_field).unwrap(),
        Value::Int(17)
    );
    assert_eq!(
        heap.get_field(second_receiver, second_field).unwrap(),
        Value::Int(73)
    );
    assert_eq!(
        heap.get_field(first_receiver, second_field).unwrap_err(),
        E::MissingField {
            class: first_class,
            field: second_field
        }
    );
    assert_eq!(layout_counts(&heap), (4, 2, 2));
    assert_eq!(instance_counts(&heap), (2, 2));
}

#[test]
fn static_only_classes_allow_zero_retained_field_and_instance_slot_limits() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "statics/Only",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field {
            name: "wide",
            descriptor: "D",
            flags: 9,
            constant: Some(Constant::Double(0x3ff0_0000_0000_0000)),
        }],
    );
    let plan = plan(&registry, class);
    let id = field(&registry, class, "wide", "D");
    let mut heap = heap(InstanceOptions {
        max_layout_fields: 0,
        max_instance_slots: 0,
        max_total_instance_slots: 0,
        ..InstanceOptions::default()
    });
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let first = heap.allocate_instance(class).unwrap();
    let second = heap.allocate_instance(class).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        heap.get_field(first, id).unwrap_err(),
        E::MissingField { class, field: id }
    );
    assert_eq!(layout_counts(&heap), (2, 1, 0));
    assert_eq!(instance_counts(&heap), (2, 0));
}

#[test]
fn maximum_instance_policy_values_do_not_trigger_eager_allocation() {
    let options = InstanceOptions {
        max_layout_classes: usize::MAX,
        max_layout_edges: usize::MAX,
        max_layout_fields: usize::MAX,
        max_instances: usize::MAX,
        max_instance_slots: usize::MAX,
        max_total_instance_slots: usize::MAX,
    };
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let class = define(
        &mut registry,
        loader,
        "max/Small",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("value", "D")],
    );
    let plan = plan(&registry, class);
    let mut heap = heap(options);
    assert_eq!(layout_counts(&heap), (0, 0, 0));
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(class).unwrap();
    assert_eq!(
        heap.get_field(receiver, field(&registry, class, "value", "D"))
            .unwrap(),
        Value::Double(0)
    );
    assert_eq!(layout_counts(&heap), (2, 1, 1));
    assert_eq!(instance_counts(&heap), (1, 1));
}

#[test]
fn deep_admission_allocation_field_access_and_drop_use_bounded_iterative_walks() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 1,
        ..RegistryOptions::default()
    })
    .unwrap();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let mut previous = String::from("java/lang/Object");
    let mut classes = Vec::new();
    for index in 0..DEPTH {
        let class_name = format!("deep/C{index}");
        classes.push(define(
            &mut registry,
            loader,
            &class_name,
            Some(&previous),
            &[],
            0x21,
            &[Field::new("value", "J")],
        ));
        previous = class_name;
    }
    let class = *classes.last().unwrap();
    let plan = build_hierarchy(
        &registry,
        class,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 1,
            max_edges: DEPTH,
            max_declared_fields: DEPTH,
            max_instance_slots: DEPTH,
        },
    )
    .unwrap();
    let mut heap = heap(InstanceOptions {
        max_layout_classes: DEPTH + 1,
        max_layout_edges: DEPTH,
        max_layout_fields: DEPTH,
        max_instances: 1,
        max_instance_slots: DEPTH,
        max_total_instance_slots: DEPTH,
    });
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(class).unwrap();
    let first = field(&registry, classes[0], "value", "J");
    let last = field(&registry, class, "value", "J");
    assert_eq!(heap.get_field(receiver, first).unwrap(), Value::Long(0));
    assert_eq!(heap.get_field(receiver, last).unwrap(), Value::Long(0));
    heap.put_field(receiver, first, Value::Long(11)).unwrap();
    heap.put_field(receiver, last, Value::Long(99)).unwrap();
    assert_eq!(heap.get_field(receiver, first).unwrap(), Value::Long(11));
    assert_eq!(heap.get_field(receiver, last).unwrap(), Value::Long(99));
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    assert_eq!(layout_counts(&heap), (DEPTH + 1, DEPTH, DEPTH));
    assert_eq!(instance_counts(&heap), (1, DEPTH));
    assert_eq!(heap.children(receiver).unwrap().count(), 0);
    drop(heap);
    drop(plan);
    assert_eq!(registry.definition_count(), DEPTH + 1);
}

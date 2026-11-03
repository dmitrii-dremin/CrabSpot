use crabspot_classfile::{AttributeBody, ParseOptions};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, HandleKind, HeapRef, HierarchyOptions, LoaderId, Reference,
    RegistryError, RegistryOptions, SameLoaderResolver, StaticStorage, StaticStorageOptions,
    StorageError, Value, ValueKind, build_hierarchy,
};
use std::{collections::HashSet, hash::Hash};

fn name(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}

#[derive(Clone)]
enum ConstantValue<'a> {
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
    constant: Option<ConstantValue<'a>>,
}
impl<'a> Field<'a> {
    fn static_field(name: &'a str, descriptor: &'a str) -> Self {
        Self {
            name,
            descriptor,
            flags: 9,
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
    fn utf8(&mut self, text: &str) -> u16 {
        assert!(text.is_ascii());
        let mut bytes = words(&[text.len() as u16]);
        bytes.extend(text.bytes());
        self.add(1, &bytes)
    }
    fn class(&mut self, text: &str) -> u16 {
        let name = self.utf8(text);
        self.add(7, &words(&[name]))
    }
    fn value(&mut self, value: &ConstantValue<'_>) -> u16 {
        match value {
            ConstantValue::Int(value) => self.add(3, &value.to_be_bytes()),
            ConstantValue::Long(value) => self.add(5, &value.to_be_bytes()),
            ConstantValue::Float(bits) => self.add(4, &bits.to_be_bytes()),
            ConstantValue::Double(bits) => self.add(6, &bits.to_be_bytes()),
            ConstantValue::String(text) => {
                let text = self.utf8(text);
                self.add(8, &words(&[text]))
            }
        }
    }
}

fn definition(
    class_name: &str,
    superclass: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[Field<'_>],
) -> Vec<u8> {
    let mut pool = Pool::default();
    let this = pool.class(class_name);
    let superclass = superclass.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<u16> = interfaces.iter().map(|name| pool.class(name)).collect();
    let constant_value_name = pool.utf8("ConstantValue");
    let fields: Vec<_> = fields
        .iter()
        .map(|field| {
            (
                pool.utf8(field.name),
                pool.utf8(field.descriptor),
                field.flags,
                field.constant.as_ref().map(|value| pool.value(value)),
            )
        })
        .collect();
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, this, superclass, interfaces.len() as u16]));
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
            bytes.extend(words(&[constant_value_name]));
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
    class_name: &str,
    superclass: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[Field<'_>],
) -> ClassId {
    registry
        .define(
            loader,
            &name(class_name),
            &definition(class_name, superclass, interfaces, flags, fields),
            ParseOptions::default(),
        )
        .unwrap()
}
fn object(registry: &mut DefinitionRegistry, loader: LoaderId) -> ClassId {
    define(registry, loader, "java/lang/Object", None, &[], 0x21, &[])
}

#[test]
fn null_reference_is_an_opaque_hashable_single_slot_value_without_a_heap_handle() {
    fn identity_traits<T: Copy + Eq + Hash>() {}
    identity_traits::<Reference>();
    identity_traits::<HeapRef>();
    let null = Reference::null();
    assert_eq!(null, Reference::default());
    assert!(null.is_null());
    assert!(null.heap_ref().is_none());
    assert_eq!(HashSet::from([null, Reference::default()]).len(), 1);
    let value = Value::Reference(null);
    assert_eq!(value, Value::Reference(Reference::default()));
    assert_eq!(value.kind(), ValueKind::Reference);
    assert_eq!(value.slots(), 1);
    assert_eq!(ValueKind::Reference.slots(), 1);
    assert!(value.as_f32().is_none());
    assert!(value.as_f64().is_none());
    assert_ne!(value, Value::Int(0));
}

#[test]
fn every_primitive_object_and_array_descriptor_receives_its_exact_default() {
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
        "[Ljava/lang/String;",
        "[[[I",
        "[[Lmissing/Unresolved;",
        &rank_255,
    ];
    let names: Vec<_> = (0..descriptors.len())
        .map(|index| format!("field{index}"))
        .collect();
    let fields: Vec<_> = names
        .iter()
        .zip(descriptors)
        .map(|(name, descriptor)| Field::static_field(name, descriptor))
        .collect();
    let root = define(
        &mut registry,
        loader,
        "example/Defaults",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
    );
    let plan = build_hierarchy(
        &registry,
        root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let storage = StaticStorage::new(&registry, &plan, StaticStorageOptions::default()).unwrap();
    assert_eq!(storage.slot_count(), descriptors.len());
    assert_eq!(storage.fields().len(), descriptors.len());
    for ((field_name, descriptor), stored) in names.iter().zip(descriptors).zip(storage.fields()) {
        let field = registry
            .find_declared_field(root, &name(field_name), &name(descriptor))
            .unwrap()
            .unwrap();
        assert_eq!(stored, &field);
        let expected = match descriptor {
            "Z" | "B" | "C" | "S" | "I" => Value::Int(0),
            "J" => Value::Long(0),
            "F" => Value::Float(0),
            "D" => Value::Double(0),
            _ => Value::Reference(Reference::null()),
        };
        assert_eq!(storage.get(field).unwrap(), expected, "{descriptor}");
    }
    assert_eq!(storage.heap_roots().count(), 0);
    assert_eq!(Value::Long(0).slots(), 2);
    assert_eq!(Value::Double(0).slots(), 2);
}

#[test]
fn nonzero_constant_values_on_final_and_interface_fields_are_not_applied() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let fields = [
        Field {
            name: "boolean",
            descriptor: "Z",
            flags: 0x19,
            constant: Some(ConstantValue::Int(1)),
        },
        Field {
            name: "byte",
            descriptor: "B",
            flags: 0x19,
            constant: Some(ConstantValue::Int(7)),
        },
        Field {
            name: "char",
            descriptor: "C",
            flags: 0x19,
            constant: Some(ConstantValue::Int(65)),
        },
        Field {
            name: "short",
            descriptor: "S",
            flags: 0x19,
            constant: Some(ConstantValue::Int(300)),
        },
        Field {
            name: "integer",
            descriptor: "I",
            flags: 0x19,
            constant: Some(ConstantValue::Int(123)),
        },
        Field {
            name: "long",
            descriptor: "J",
            flags: 0x19,
            constant: Some(ConstantValue::Long(456)),
        },
        Field {
            name: "float",
            descriptor: "F",
            flags: 0x19,
            constant: Some(ConstantValue::Float(0xbf80_0000)),
        },
        Field {
            name: "double",
            descriptor: "D",
            flags: 0x19,
            constant: Some(ConstantValue::Double(0xbff0_0000_0000_0000)),
        },
        Field {
            name: "string",
            descriptor: "Ljava/lang/String;",
            flags: 0x19,
            constant: Some(ConstantValue::String("must not become an allocated String")),
        },
    ];
    let interface = define(
        &mut registry,
        loader,
        "example/Constants",
        Some("java/lang/Object"),
        &[],
        0x601,
        &fields,
    );
    let root = define(
        &mut registry,
        loader,
        "example/Root",
        Some("java/lang/Object"),
        &["example/Constants"],
        0x21,
        &[],
    );
    let plan = build_hierarchy(
        &registry,
        root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let storage = StaticStorage::new(&registry, &plan, StaticStorageOptions::default()).unwrap();
    assert_eq!(storage.slot_count(), fields.len());
    for field in &fields {
        let id = registry
            .find_declared_field(interface, &name(field.name), &name(field.descriptor))
            .unwrap()
            .unwrap();
        assert!(matches!(
            registry.field(id).unwrap().attributes[0].body,
            AttributeBody::ConstantValue(_)
        ));
        let expected = match field.descriptor {
            "J" => Value::Long(0),
            "F" => Value::Float(0),
            "D" => Value::Double(0),
            "Ljava/lang/String;" => Value::Reference(Reference::null()),
            _ => Value::Int(0),
        };
        assert_eq!(storage.get(id).unwrap(), expected);
    }
    assert_eq!(storage.heap_roots().count(), 0);
}

#[test]
fn hidden_static_fields_have_distinct_owners_and_follow_plan_declaration_order() {
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
            Field::static_field("hidden", "I"),
            Field {
                name: "instance",
                descriptor: "J",
                flags: 1,
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
        &[
            Field::static_field("hidden", "J"),
            Field::static_field("another", "D"),
        ],
    );
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let expected: Vec<_> = plan
        .classes()
        .iter()
        .flat_map(|id| plan.layout(*id).unwrap().declared_static_fields())
        .copied()
        .collect();
    let storage = StaticStorage::new(&registry, &plan, StaticStorageOptions::default()).unwrap();
    assert_eq!(storage.fields(), expected);
    assert_eq!(storage.slot_count(), 3);
    let base_field = registry
        .find_declared_field(base, &name("hidden"), &name("I"))
        .unwrap()
        .unwrap();
    let child_field = registry
        .find_declared_field(child, &name("hidden"), &name("J"))
        .unwrap()
        .unwrap();
    assert_ne!(base_field, child_field);
    assert_eq!(storage.fields()[0], base_field);
    assert_eq!(storage.fields()[1], child_field);
    assert_eq!(storage.get(base_field).unwrap(), Value::Int(0));
    assert_eq!(storage.get(child_field).unwrap(), Value::Long(0));
}

#[test]
fn ordinary_class_constant_values_also_remain_default_without_initialization() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let fields = [
        Field {
            name: "integer",
            descriptor: "I",
            flags: 0x19,
            constant: Some(ConstantValue::Int(999)),
        },
        Field {
            name: "long",
            descriptor: "J",
            flags: 0x19,
            constant: Some(ConstantValue::Long(-111)),
        },
        Field {
            name: "float",
            descriptor: "F",
            flags: 0x19,
            constant: Some(ConstantValue::Float(0x8000_0000)),
        },
        Field {
            name: "double",
            descriptor: "D",
            flags: 0x19,
            constant: Some(ConstantValue::Double(0x8000_0000_0000_0000)),
        },
        Field {
            name: "string",
            descriptor: "Ljava/lang/String;",
            flags: 0x19,
            constant: Some(ConstantValue::String("not initialized")),
        },
    ];
    let root = define(
        &mut registry,
        loader,
        "example/Constants",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
    );
    let plan = build_hierarchy(
        &registry,
        root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let storage = StaticStorage::new(&registry, &plan, StaticStorageOptions::default()).unwrap();
    let expected = [
        Value::Int(0),
        Value::Long(0),
        Value::Float(0),
        Value::Double(0),
        Value::Reference(Reference::null()),
    ];
    for (field, expected) in storage.fields().iter().zip(expected) {
        assert_eq!(storage.get(*field).unwrap(), expected);
    }
    assert_eq!(storage.heap_roots().count(), 0);
}

#[test]
fn instance_unretained_and_foreign_fields_fail_without_disturbing_static_values() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let root = define(
        &mut registry,
        loader,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::static_field("static", "I"),
            Field {
                name: "instance",
                descriptor: "J",
                flags: 1,
                constant: None,
            },
        ],
    );
    let unrelated = define(
        &mut registry,
        loader,
        "example/Unrelated",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::static_field("static", "I")],
    );
    let static_field = registry
        .find_declared_field(root, &name("static"), &name("I"))
        .unwrap()
        .unwrap();
    let instance = registry
        .find_declared_field(root, &name("instance"), &name("J"))
        .unwrap()
        .unwrap();
    let missing = registry
        .find_declared_field(unrelated, &name("static"), &name("I"))
        .unwrap()
        .unwrap();
    let plan = build_hierarchy(
        &registry,
        root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let storage = StaticStorage::new(&registry, &plan, StaticStorageOptions::default()).unwrap();
    for field in [instance, missing] {
        assert!(
            matches!(storage.get(field).unwrap_err(), StorageError::MissingStaticLocation { field: rejected } if rejected == field)
        );
    }
    let mut foreign = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign.create_loader().unwrap();
    object(&mut foreign, foreign_loader);
    let foreign_root = define(
        &mut foreign,
        foreign_loader,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::static_field("static", "I")],
    );
    let foreign_field = foreign
        .find_declared_field(foreign_root, &name("static"), &name("I"))
        .unwrap()
        .unwrap();
    assert!(matches!(
        storage.get(foreign_field).unwrap_err(),
        StorageError::Registry(RegistryError::InvalidHandle {
            kind: HandleKind::Field,
            ..
        })
    ));
    assert_eq!(storage.fields(), &[static_field]);
    assert_eq!(storage.get(static_field).unwrap(), Value::Int(0));
    let foreign_plan = build_hierarchy(
        &foreign,
        foreign_root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    assert!(matches!(
        StaticStorage::new(&registry, &foreign_plan, StaticStorageOptions::default()).unwrap_err(),
        StorageError::Registry(RegistryError::InvalidHandle { .. })
    ));
    assert!(matches!(
        StaticStorage::new(&foreign, &plan, StaticStorageOptions::default()).unwrap_err(),
        StorageError::Registry(RegistryError::InvalidHandle { .. })
    ));
}

#[test]
fn static_locations_remain_scoped_to_class_identity_across_equal_loader_names() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let first_class = define(
        &mut registry,
        first,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::static_field("value", "I")],
    );
    let second_class = define(
        &mut registry,
        second,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::static_field("value", "I")],
    );
    let first_field = registry
        .find_declared_field(first_class, &name("value"), &name("I"))
        .unwrap()
        .unwrap();
    let second_field = registry
        .find_declared_field(second_class, &name("value"), &name("I"))
        .unwrap()
        .unwrap();
    let first_plan = build_hierarchy(
        &registry,
        first_class,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let second_plan = build_hierarchy(
        &registry,
        second_class,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let first_storage =
        StaticStorage::new(&registry, &first_plan, StaticStorageOptions::default()).unwrap();
    let second_storage =
        StaticStorage::new(&registry, &second_plan, StaticStorageOptions::default()).unwrap();
    assert_eq!(first_storage.fields(), &[first_field]);
    assert_eq!(second_storage.fields(), &[second_field]);
    assert!(
        matches!(first_storage.get(second_field).unwrap_err(), StorageError::MissingStaticLocation { field } if field == second_field)
    );
    assert!(
        matches!(second_storage.get(first_field).unwrap_err(), StorageError::MissingStaticLocation { field } if field == first_field)
    );
}

#[test]
fn zero_exact_and_maximum_storage_limits_count_locations_before_allocation() {
    assert_eq!(StaticStorageOptions::default().max_slots, 65536);
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let root_object = object(&mut registry, loader);
    let empty_plan = build_hierarchy(
        &registry,
        root_object,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let empty = StaticStorage::new(
        &registry,
        &empty_plan,
        StaticStorageOptions { max_slots: 0 },
    )
    .unwrap();
    assert!(empty.fields().is_empty());
    assert_eq!(empty.slot_count(), 0);
    assert_eq!(empty.heap_roots().count(), 0);
    let root = define(
        &mut registry,
        loader,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::static_field("long", "J"),
            Field::static_field("double", "D"),
            Field::static_field("reference", "Ljava/lang/Object;"),
        ],
    );
    let plan = build_hierarchy(
        &registry,
        root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let count = registry.definition_count();
    let volume = registry.total_definition_bytes();
    for limit in [0, 2] {
        assert!(
            matches!(StaticStorage::new(&registry, &plan, StaticStorageOptions { max_slots: limit }).unwrap_err(), StorageError::ResourceLimit { limit: actual_limit, required: 3 } if actual_limit == limit)
        );
    }
    let exact =
        StaticStorage::new(&registry, &plan, StaticStorageOptions { max_slots: 3 }).unwrap();
    let huge = StaticStorage::new(
        &registry,
        &plan,
        StaticStorageOptions {
            max_slots: usize::MAX,
        },
    )
    .unwrap();
    assert_eq!(exact.slot_count(), 3);
    assert_eq!(huge.slot_count(), 3);
    assert_eq!(exact.fields(), huge.fields());
    for field in exact.fields() {
        assert_eq!(exact.get(*field).unwrap(), huge.get(*field).unwrap());
    }
    assert_eq!(exact.heap_roots().count(), 0);
    assert_eq!(registry.definition_count(), count);
    assert_eq!(registry.total_definition_bytes(), volume);
    assert_eq!(
        plan.layout(root).unwrap().declared_static_fields(),
        exact.fields()
    );
}

//! Structural fixtures exercise hierarchy planning without inventing a runtime JDK.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, HandleKind, HierarchyError, HierarchyOptions, HierarchyResolver,
    HierarchyResource, LoaderId, RegistryError, RegistryOptions, ResolveError, SameLoaderResolver,
    build_hierarchy,
};
use std::collections::HashMap;

fn name(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}

#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn utf8(&mut self, text: &str) -> u16 {
        assert!(text.is_ascii());
        let mut bytes = vec![1];
        bytes.extend(words(&[text.len() as u16]));
        bytes.extend(text.bytes());
        self.0.push(bytes);
        self.0.len() as u16
    }
    fn class(&mut self, text: &str) -> u16 {
        let index = self.utf8(text);
        let mut bytes = vec![7];
        bytes.extend(words(&[index]));
        self.0.push(bytes);
        self.0.len() as u16
    }
}

fn definition(
    class_name: &str,
    superclass: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[(&str, &str, u16)],
) -> Vec<u8> {
    let mut pool = Pool::default();
    let this = pool.class(class_name);
    let parent = superclass.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<u16> = interfaces.iter().map(|name| pool.class(name)).collect();
    let fields: Vec<(u16, u16, u16)> = fields
        .iter()
        .map(|(name, descriptor, flags)| (pool.utf8(name), pool.utf8(descriptor), *flags))
        .collect();
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, this, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[fields.len() as u16]));
    for (name, descriptor, flags) in fields {
        bytes.extend(words(&[flags, name, descriptor, 0]));
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
    fields: &[(&str, &str, u16)],
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
    // Explicit test-only root metadata, not a production java/lang/Object fallback.
    define(registry, loader, "java/lang/Object", None, &[], 0x21, &[])
}

#[derive(Default)]
struct Resolver {
    selected: HashMap<(LoaderId, Vec<u16>), ClassId>,
    requests: Vec<(LoaderId, Vec<u16>)>,
    failure: Option<ResolveError>,
}
impl HierarchyResolver for Resolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        requested: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        self.requests.push((loader, requested.to_vec()));
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if let Some(id) = self.selected.get(&(loader, requested.to_vec())) {
            return Ok(Some(*id));
        }
        registry
            .lookup_class(loader, requested)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

#[test]
fn same_loader_resolution_preserves_isolated_class_identities() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    let first_object = object(&mut registry, first);
    let second_object = object(&mut registry, second);
    let first_root = define(
        &mut registry,
        first,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    );
    let second_root = define(
        &mut registry,
        second,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    );
    let first_plan = build_hierarchy(
        &registry,
        first_root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let second_plan = build_hierarchy(
        &registry,
        second_root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    assert_eq!(first_plan.root(), first_root);
    assert_eq!(first_plan.classes(), &[first_object, first_root]);
    assert_eq!(second_plan.classes(), &[second_object, second_root]);
    assert_eq!(
        first_plan.layout(first_root).unwrap().direct_superclass(),
        Some(first_object)
    );
    assert!(first_plan.layout(second_root).is_none());
    assert!(first_plan.layout(second_object).is_none());
}

#[test]
fn explicit_delegation_can_select_another_defining_loader_without_changing_identity() {
    let mut registry = registry();
    let parent = registry.create_loader().unwrap();
    let child = registry.create_loader().unwrap();
    let parent_object = object(&mut registry, parent);
    let root = define(
        &mut registry,
        child,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    );
    assert!(
        build_hierarchy(
            &registry,
            root,
            &mut SameLoaderResolver,
            HierarchyOptions::default()
        )
        .is_err()
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((child, name("java/lang/Object")), parent_object);
    let plan =
        build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap();
    assert_eq!(plan.classes(), &[parent_object, root]);
    assert_eq!(
        plan.layout(root).unwrap().direct_superclass(),
        Some(parent_object)
    );
    assert_eq!(registry.defining_loader(parent_object).unwrap(), parent);
    assert_eq!(registry.defining_loader(root).unwrap(), child);
    assert_eq!(registry.definition_count(), 2);
}

#[test]
fn layout_uses_owner_qualified_hiding_logical_slots_and_declaring_static_owners() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let root_object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("head", "D", 1)],
    );
    let base = define(
        &mut registry,
        loader,
        "example/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("hidden", "I", 1), ("counter", "J", 9)],
    );
    let interface = define(
        &mut registry,
        loader,
        "example/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("constant", "I", 0x19)],
    );
    let child = define(
        &mut registry,
        loader,
        "example/Child",
        Some("example/Base"),
        &["example/I"],
        0x21,
        &[
            ("hidden", "J", 1),
            ("wide", "D", 1),
            ("reference", "Ljava/lang/Object;", 1),
            ("counter", "I", 9),
        ],
    );
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let object_field = registry
        .find_declared_field(root_object, &name("head"), &name("D"))
        .unwrap()
        .unwrap();
    let base_hidden = registry
        .find_declared_field(base, &name("hidden"), &name("I"))
        .unwrap()
        .unwrap();
    let child_hidden = registry
        .find_declared_field(child, &name("hidden"), &name("J"))
        .unwrap()
        .unwrap();
    let child_wide = registry
        .find_declared_field(child, &name("wide"), &name("D"))
        .unwrap()
        .unwrap();
    let reference = registry
        .find_declared_field(child, &name("reference"), &name("Ljava/lang/Object;"))
        .unwrap()
        .unwrap();
    let base_static = registry
        .find_declared_field(base, &name("counter"), &name("J"))
        .unwrap()
        .unwrap();
    let child_static = registry
        .find_declared_field(child, &name("counter"), &name("I"))
        .unwrap()
        .unwrap();
    assert_eq!(plan.instance_field_slot(child, object_field), Some(0));
    assert_eq!(plan.instance_field_slot(child, base_hidden), Some(1));
    assert_eq!(plan.instance_field_slot(child, child_hidden), Some(2));
    assert_eq!(plan.instance_field_slot(child, child_wide), Some(3));
    assert_eq!(plan.instance_field_slot(child, reference), Some(4));
    assert_eq!(plan.instance_field_slot(base, child_hidden), None);
    assert_eq!(plan.instance_field_slot(child, base_static), None);
    assert_eq!(plan.declared_static_field_slot(base_static), Some(0));
    assert_eq!(plan.declared_static_field_slot(child_static), Some(0));
    assert_eq!(plan.declared_static_field_slot(base_hidden), None);
    let child_layout = plan.layout(child).unwrap();
    assert_eq!(child_layout.class_id(), child);
    assert_eq!(child_layout.direct_interfaces(), &[interface]);
    assert_eq!(child_layout.inherited_instance_slots(), 2);
    assert_eq!(child_layout.instance_slots(), 5);
    assert_eq!(
        child_layout.declared_instance_fields(),
        &[child_hidden, child_wide, reference]
    );
    assert_eq!(child_layout.declared_static_fields(), &[child_static]);
    assert_eq!(
        plan.layout(base).unwrap().declared_static_fields(),
        &[base_static]
    );
    // Object is a dependency for an interface, but contributes no interface storage.
    let interface_layout = plan.layout(interface).unwrap();
    assert_eq!(interface_layout.direct_superclass(), Some(root_object));
    assert_eq!(interface_layout.inherited_instance_slots(), 0);
    assert_eq!(interface_layout.instance_slots(), 0);
    assert_eq!(plan.instance_field_slot(interface, object_field), None);
    let reference_member = registry.field(reference).unwrap();
    assert!(
        registry
            .class(child)
            .unwrap()
            .constant_pool
            .utf8(reference_member.descriptor_index)
            .unwrap()
            .equals_ascii("Ljava/lang/Object;")
    );
}

#[test]
fn context_name_cache_keeps_each_loaders_object_identity_distinct() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    let first_object = object(&mut registry, first);
    let second_object = object(&mut registry, second);
    let interface = define(
        &mut registry,
        second,
        "example/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let root = define(
        &mut registry,
        first,
        "example/Root",
        Some("java/lang/Object"),
        &["example/I"],
        0x21,
        &[],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((first, name("example/I")), interface);
    let plan =
        build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap();
    assert_eq!(
        plan.layout(root).unwrap().direct_superclass(),
        Some(first_object)
    );
    assert_eq!(
        plan.layout(interface).unwrap().direct_superclass(),
        Some(second_object)
    );
    assert!(plan.layout(first_object).is_some());
    assert!(plan.layout(second_object).is_some());
    assert_eq!(resolver.requests.iter().filter(|(loader, requested)| *loader == first && requested == &name("java/lang/Object")).count(), 1);
    assert_eq!(
        resolver
            .requests
            .iter()
            .filter(
                |(loader, requested)| *loader == second && requested == &name("java/lang/Object")
            )
            .count(),
        1
    );
}

struct AlternatingObject {
    first: ClassId,
    second: ClassId,
    object_requests: usize,
}
impl HierarchyResolver for AlternatingObject {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        requested: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        if requested == name("java/lang/Object") {
            self.object_requests += 1;
            return Ok(Some(if self.object_requests == 1 {
                self.first
            } else {
                self.second
            }));
        }
        registry
            .lookup_class(loader, requested)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

#[test]
fn repeated_context_name_edges_use_one_consistent_resolver_selection() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let other_loader = registry.create_loader().unwrap();
    let first_object = object(&mut registry, loader);
    let second_object = object(&mut registry, other_loader);
    let first_interface = define(
        &mut registry,
        loader,
        "example/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let second_interface = define(
        &mut registry,
        loader,
        "example/J",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let root = define(
        &mut registry,
        loader,
        "example/Root",
        Some("java/lang/Object"),
        &["example/I", "example/J"],
        0x21,
        &[],
    );
    let mut resolver = AlternatingObject {
        first: first_object,
        second: second_object,
        object_requests: 0,
    };
    let plan =
        build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap();
    assert_eq!(resolver.object_requests, 1);
    for class in [root, first_interface, second_interface] {
        assert_eq!(
            plan.layout(class).unwrap().direct_superclass(),
            Some(first_object)
        );
    }
    assert!(plan.layout(second_object).is_none());
    assert_eq!(plan.classes().len(), 4);
    assert_eq!(plan.classes().last(), Some(&root));
    let mut resolver = AlternatingObject {
        first: first_object,
        second: second_object,
        object_requests: 0,
    };
    let repeated =
        build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap();
    assert_eq!(plan.classes(), repeated.classes());
}

#[test]
fn plan_queries_reject_foreign_unknown_unrelated_and_wrong_field_categories() {
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
        &[("instance", "J", 1), ("static", "D", 9)],
    );
    let unrelated = define(
        &mut registry,
        loader,
        "example/Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("instance", "I", 1), ("static", "I", 9)],
    );
    let instance = registry
        .find_declared_field(root, &name("instance"), &name("J"))
        .unwrap()
        .unwrap();
    let static_field = registry
        .find_declared_field(root, &name("static"), &name("D"))
        .unwrap()
        .unwrap();
    let unrelated_field = registry
        .find_declared_field(unrelated, &name("instance"), &name("I"))
        .unwrap()
        .unwrap();
    let unrelated_static = registry
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
    assert_eq!(plan.instance_field_slot(root, instance), Some(0));
    assert_eq!(plan.declared_static_field_slot(static_field), Some(0));
    assert_eq!(plan.instance_field_slot(root, static_field), None);
    assert_eq!(plan.declared_static_field_slot(instance), None);
    assert_eq!(plan.instance_field_slot(root, unrelated_field), None);
    assert_eq!(plan.instance_field_slot(unrelated, instance), None);
    assert_eq!(plan.declared_static_field_slot(unrelated_static), None);
    assert!(plan.layout(unrelated).is_none());
    let mut foreign_registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign_registry.create_loader().unwrap();
    object(&mut foreign_registry, foreign_loader);
    let foreign = define(
        &mut foreign_registry,
        foreign_loader,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("instance", "J", 1), ("static", "D", 9)],
    );
    let foreign_field = foreign_registry
        .find_declared_field(foreign, &name("instance"), &name("J"))
        .unwrap()
        .unwrap();
    let foreign_static = foreign_registry
        .find_declared_field(foreign, &name("static"), &name("D"))
        .unwrap()
        .unwrap();
    assert!(plan.layout(foreign).is_none());
    assert_eq!(plan.instance_field_slot(foreign, instance), None);
    assert_eq!(plan.instance_field_slot(root, foreign_field), None);
    assert_eq!(plan.declared_static_field_slot(foreign_static), None);
}

#[test]
fn deep_graph_traversal_logical_field_queries_and_plan_drop_do_not_recurse() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 1,
        ..RegistryOptions::default()
    })
    .unwrap();
    let loader = registry.create_loader().unwrap();
    let root_object = object(&mut registry, loader);
    let mut parent = String::from("java/lang/Object");
    let mut classes = Vec::new();
    for index in 0..DEPTH {
        let class_name = format!("deep/C{index}");
        let class = define(
            &mut registry,
            loader,
            &class_name,
            Some(&parent),
            &[],
            0x21,
            &[("value", "J", 1)],
        );
        classes.push(class);
        parent = class_name;
    }
    let root = *classes.last().unwrap();
    let first_field = registry
        .find_declared_field(classes[0], &name("value"), &name("J"))
        .unwrap()
        .unwrap();
    let last_field = registry
        .find_declared_field(root, &name("value"), &name("J"))
        .unwrap()
        .unwrap();
    let plan = build_hierarchy(
        &registry,
        root,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 1,
            max_edges: DEPTH,
            max_declared_fields: DEPTH,
            max_instance_slots: DEPTH,
        },
    )
    .unwrap();
    assert_eq!(plan.classes().len(), DEPTH + 1);
    assert_eq!(plan.classes()[0], root_object);
    assert_eq!(plan.classes()[1..], classes);
    assert_eq!(plan.instance_field_slot(root, first_field), Some(0));
    assert_eq!(plan.instance_field_slot(root, last_field), Some(DEPTH - 1));
    assert_eq!(plan.layout(root).unwrap().instance_slots(), DEPTH);
    assert_eq!(
        plan.classes()
            .iter()
            .map(|class| plan
                .layout(*class)
                .unwrap()
                .declared_instance_fields()
                .len())
            .sum::<usize>(),
        DEPTH
    );
    drop(plan);
    assert_eq!(registry.definition_count(), DEPTH + 1);
    assert_eq!(registry.class(root).unwrap().fields.len(), 1);
}

#[test]
fn missing_definitions_and_resolver_failures_are_distinct_and_do_not_stick_across_plans() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let root = define(
        &mut registry,
        loader,
        "example/Root",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    );
    let bytes = registry.total_definition_bytes();
    let error = build_hierarchy(
        &registry,
        root,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(error, HierarchyError::MissingDefinition { origin, initiating_loader, name: missing } if origin == root && initiating_loader == loader && missing.equals_ascii("java/lang/Object"))
    );
    assert_eq!(registry.definition_count(), 1);
    assert_eq!(registry.total_definition_bytes(), bytes);
    let root_object = object(&mut registry, loader);
    let mut resolver = Resolver {
        failure: Some(ResolveError::new("delegation policy refused the lookup")),
        ..Resolver::default()
    };
    let error =
        build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap_err();
    assert!(
        matches!(error, HierarchyError::Resolve { origin, name: requested, error } if origin == root && requested.equals_ascii("java/lang/Object") && error.message.contains("delegation"))
    );
    resolver.failure = None;
    let plan =
        build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap();
    assert_eq!(plan.classes(), &[root_object, root]);
    assert_eq!(registry.definition_count(), 2);
}

#[test]
fn root_and_resolver_results_must_belong_to_registry_and_match_requested_name() {
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
        &[],
    );
    let other = define(
        &mut registry,
        loader,
        "example/Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    );
    let mut foreign_registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign_registry.create_loader().unwrap();
    let foreign_object = object(&mut foreign_registry, foreign_loader);
    assert!(matches!(
        build_hierarchy(
            &registry,
            foreign_object,
            &mut SameLoaderResolver,
            HierarchyOptions::default()
        )
        .unwrap_err(),
        HierarchyError::Registry(RegistryError::InvalidHandle {
            kind: HandleKind::Class,
            ..
        })
    ));
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((loader, name("java/lang/Object")), foreign_object);
    assert!(matches!(
        build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap_err(),
        HierarchyError::Registry(RegistryError::InvalidHandle {
            kind: HandleKind::Class,
            ..
        })
    ));
    resolver
        .selected
        .insert((loader, name("java/lang/Object")), other);
    assert!(
        matches!(build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap_err(), HierarchyError::NameMismatch { origin, requested, returned, actual } if origin == root && requested.equals_ascii("java/lang/Object") && returned == other && actual.equals_ascii("example/Other"))
    );
    assert_eq!(registry.definition_count(), 3);
}

#[test]
fn final_superclasses_and_class_interface_role_mismatches_are_rejected() {
    for mode in 0..4 {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let dependency = define(
            &mut registry,
            loader,
            "example/Dependency",
            Some("java/lang/Object"),
            &[],
            if mode == 0 {
                0x31
            } else if mode == 1 {
                0x601
            } else {
                0x21
            },
            &[],
        );
        let root = define(
            &mut registry,
            loader,
            "example/Root",
            Some(if mode <= 1 {
                "example/Dependency"
            } else {
                "java/lang/Object"
            }),
            if mode <= 1 {
                &[]
            } else {
                &["example/Dependency"]
            },
            if mode == 3 { 0x601 } else { 0x21 },
            &[],
        );
        let error = build_hierarchy(
            &registry,
            root,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap_err();
        assert!(match error {
            HierarchyError::FinalSuperclass { class, superclass } if mode == 0 =>
                class == root && superclass == dependency,
            HierarchyError::WrongSuperclassKind { class, superclass } if mode == 1 =>
                class == root && superclass == dependency,
            HierarchyError::WrongInterfaceKind { class, interface } if mode >= 2 =>
                class == root && interface == dependency,
            _ => false,
        });
        assert_eq!(registry.definition_count(), 3);
    }
}

#[test]
fn genuine_class_and_interface_cycles_are_detected_without_changing_definitions() {
    for interfaces in [false, true] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let first = define(
            &mut registry,
            loader,
            "cycle/A",
            Some(if interfaces {
                "java/lang/Object"
            } else {
                "cycle/B"
            }),
            if interfaces { &["cycle/B"] } else { &[] },
            if interfaces { 0x601 } else { 0x21 },
            &[],
        );
        let second = define(
            &mut registry,
            loader,
            "cycle/B",
            Some(if interfaces {
                "java/lang/Object"
            } else {
                "cycle/A"
            }),
            if interfaces { &["cycle/A"] } else { &[] },
            if interfaces { 0x601 } else { 0x21 },
            &[],
        );
        let count = registry.definition_count();
        let volume = registry.total_definition_bytes();
        assert!(
            matches!(build_hierarchy(&registry, first, &mut SameLoaderResolver, HierarchyOptions::default()).unwrap_err(), HierarchyError::Cycle { class, dependency } if [first, second].contains(&class) && [first, second].contains(&dependency))
        );
        assert_eq!(registry.definition_count(), count);
        assert_eq!(registry.total_definition_bytes(), volume);
        assert!(registry.class(first).is_ok());
    }
}

#[test]
fn equal_names_in_different_loaders_do_not_create_a_false_cycle() {
    let mut registry = registry();
    let first_loader = registry.create_loader().unwrap();
    let second_loader = registry.create_loader().unwrap();
    let root_object = object(&mut registry, second_loader);
    let second_a = define(
        &mut registry,
        second_loader,
        "example/A",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    );
    let second_base = define(
        &mut registry,
        second_loader,
        "example/Base",
        Some("example/A"),
        &[],
        0x21,
        &[],
    );
    let first_a = define(
        &mut registry,
        first_loader,
        "example/A",
        Some("example/Base"),
        &[],
        0x21,
        &[],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((first_loader, name("example/Base")), second_base);
    let plan = build_hierarchy(
        &registry,
        first_a,
        &mut resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    assert_eq!(
        plan.classes(),
        &[root_object, second_a, second_base, first_a]
    );
}

#[test]
fn shared_interface_diamond_is_acyclic_and_counts_all_direct_object_edges() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let root_object = object(&mut registry, loader);
    let shared = define(
        &mut registry,
        loader,
        "diamond/K",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let left = define(
        &mut registry,
        loader,
        "diamond/I",
        Some("java/lang/Object"),
        &["diamond/K"],
        0x601,
        &[],
    );
    let right = define(
        &mut registry,
        loader,
        "diamond/J",
        Some("java/lang/Object"),
        &["diamond/K"],
        0x601,
        &[],
    );
    let root = define(
        &mut registry,
        loader,
        "diamond/Root",
        Some("java/lang/Object"),
        &["diamond/I", "diamond/J"],
        0x21,
        &[],
    );
    let options = HierarchyOptions {
        max_classes: 5,
        max_edges: 8,
        max_declared_fields: 0,
        max_instance_slots: 0,
    };
    let mut resolver = Resolver::default();
    let plan = build_hierarchy(&registry, root, &mut resolver, options).unwrap();
    assert_eq!(plan.classes().len(), 5);
    assert_eq!(plan.classes().first(), Some(&root_object));
    assert_eq!(plan.classes().last(), Some(&root));
    assert_eq!(plan.layout(left).unwrap().direct_interfaces(), &[shared]);
    assert_eq!(plan.layout(right).unwrap().direct_interfaces(), &[shared]);
    assert_eq!(
        resolver
            .requests
            .iter()
            .filter(|(_, requested)| requested == &name("diamond/K"))
            .count(),
        1
    );
    assert_eq!(
        resolver
            .requests
            .iter()
            .filter(|(_, requested)| requested == &name("java/lang/Object"))
            .count(),
        1
    );
    assert!(matches!(
        build_hierarchy(
            &registry,
            root,
            &mut SameLoaderResolver,
            HierarchyOptions {
                max_edges: 7,
                ..options
            }
        )
        .unwrap_err(),
        HierarchyError::ResourceLimit {
            resource: HierarchyResource::Edges,
            limit: 7,
            required: 8
        }
    ));
}

#[test]
fn independent_hierarchy_policies_support_exact_zero_and_inclusive_boundaries() {
    let defaults = HierarchyOptions::default();
    assert_eq!(defaults.max_classes, 1024);
    assert_eq!(defaults.max_edges, 8192);
    assert_eq!(defaults.max_declared_fields, 65536);
    assert_eq!(defaults.max_instance_slots, 65536);
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let root_object = object(&mut registry, loader);
    build_hierarchy(
        &registry,
        root_object,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: 1,
            max_edges: 0,
            max_declared_fields: 0,
            max_instance_slots: 0,
        },
    )
    .unwrap();
    let base = define(
        &mut registry,
        loader,
        "bounds/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("instance", "I", 1), ("static", "J", 9)],
    );
    let root = define(
        &mut registry,
        loader,
        "bounds/Root",
        Some("bounds/Base"),
        &[],
        0x21,
        &[("instance", "D", 1), ("static", "I", 9)],
    );
    let exact = HierarchyOptions {
        max_classes: 3,
        max_edges: 2,
        max_declared_fields: 4,
        max_instance_slots: 2,
    };
    let plan = build_hierarchy(&registry, root, &mut SameLoaderResolver, exact).unwrap();
    assert_eq!(plan.classes(), &[root_object, base, root]);
    assert_eq!(plan.layout(root).unwrap().instance_slots(), 2);
    for (options, resource, limit, required) in [
        (
            HierarchyOptions {
                max_classes: 2,
                ..exact
            },
            HierarchyResource::Classes,
            2,
            3,
        ),
        (
            HierarchyOptions {
                max_edges: 1,
                ..exact
            },
            HierarchyResource::Edges,
            1,
            2,
        ),
        (
            HierarchyOptions {
                max_declared_fields: 3,
                ..exact
            },
            HierarchyResource::DeclaredFields,
            3,
            4,
        ),
        (
            HierarchyOptions {
                max_instance_slots: 1,
                ..exact
            },
            HierarchyResource::InstanceSlots,
            1,
            2,
        ),
        (
            HierarchyOptions {
                max_classes: 0,
                ..exact
            },
            HierarchyResource::Classes,
            0,
            1,
        ),
    ] {
        assert!(
            matches!(build_hierarchy(&registry, root, &mut SameLoaderResolver, options).unwrap_err(), HierarchyError::ResourceLimit { resource: actual, limit: actual_limit, required: actual_required } if actual == resource && actual_limit == limit && actual_required == required)
        );
    }
    let huge = HierarchyOptions {
        max_classes: usize::MAX,
        max_edges: usize::MAX,
        max_declared_fields: usize::MAX,
        max_instance_slots: usize::MAX,
    };
    assert_eq!(
        build_hierarchy(&registry, root, &mut SameLoaderResolver, huge)
            .unwrap()
            .classes(),
        plan.classes()
    );
    assert_eq!(registry.definition_count(), 3);
}

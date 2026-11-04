//! Independent prescribed-origin special lookup; no method Code is executed here.
use crabspot_classfile::{ParseOptions, descriptor::parse_method_descriptor};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError as C, FieldResolutionContext,
    FieldResolutionOptions, FieldResource, HeapError, HeapOptions, HierarchyOptions, HierarchyPlan,
    HierarchyResolver, InstanceError, InstanceOptions, LoaderId, ManagedHeap,
    MethodAccessError as E, MethodId, MethodJavaError as J, PrimitiveArrayKind, Reference,
    RegistryOptions, ResolveError, SameLoaderResolver, build_hierarchy,
};
use std::{cell::Cell, collections::HashMap, rc::Rc};
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn add(&mut self, tag: u8, data: &[u8]) -> u16 {
        let mut bytes = vec![tag];
        bytes.extend(data);
        self.0.push(bytes);
        self.0.len() as u16
    }
    fn text(&mut self, text: &str) -> u16 {
        assert!(text.is_ascii());
        let mut bytes = words(&[text.len() as u16]);
        bytes.extend(text.bytes());
        self.add(1, &bytes)
    }
    fn class(&mut self, name: &str) -> u16 {
        let index = self.text(name);
        self.add(7, &words(&[index]))
    }
}
struct Def {
    class: ClassId,
    refs: Vec<u16>,
    wrong: u16,
}
// Keeping class-file and symbolic-call properties explicit makes negative fixtures reviewable.
#[allow(clippy::too_many_arguments)]
fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    methods: &[(&str, &str, u16)],
    refs: &[(&str, &str, &str, u8)],
) -> Def {
    define_version(
        registry, loader, name, parent, interfaces, flags, methods, refs, 65,
    )
}
#[allow(clippy::too_many_arguments)]
fn define_version(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    methods: &[(&str, &str, u16)],
    refs: &[(&str, &str, &str, u8)],
    major: u16,
) -> Def {
    let mut pool = Pool::default();
    let owner = pool.class(name);
    let parent = parent.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let code_name = pool.text("Code");
    let methods: Vec<_> = methods
        .iter()
        .map(|(name, desc, flags)| {
            (
                pool.text(name),
                pool.text(desc),
                *flags,
                parse_method_descriptor(&units(desc))
                    .unwrap()
                    .parameter_slots
                    + u16::from(flags & 8 == 0),
            )
        })
        .collect();
    let mut indices = Vec::new();
    for (owner, name, desc, tag) in refs {
        let owner = pool.class(owner);
        let name = pool.text(name);
        let desc = pool.text(desc);
        let nat = pool.add(12, &words(&[name, desc]));
        indices.push(pool.add(*tag, &words(&[owner, nat])));
    }
    let wrong = pool.text("not Methodref");
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0];
    bytes.extend(major.to_be_bytes());
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, owner, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[0, methods.len() as u16]));
    for (name, desc, flags, locals) in methods {
        let has_code = flags & 0x500 == 0;
        bytes.extend(words(&[flags, name, desc, u16::from(has_code)]));
        if has_code {
            bytes.extend(words(&[code_name]));
            bytes.extend(13_u32.to_be_bytes());
            bytes.extend(words(&[0, locals]));
            bytes.extend(1_u32.to_be_bytes());
            bytes.push(0xb1);
            bytes.extend(words(&[0, 0]));
        }
    }
    bytes.extend(words(&[0]));
    Def {
        class: registry
            .define(loader, &units(name), &bytes, ParseOptions::default())
            .unwrap(),
        refs: indices,
        wrong,
    }
}
fn registry() -> DefinitionRegistry {
    DefinitionRegistry::new(RegistryOptions::default()).unwrap()
}
fn object(registry: &mut DefinitionRegistry, loader: LoaderId) -> ClassId {
    define(
        registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[],
        &[],
    )
    .class
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
fn heap(registry: &DefinitionRegistry, classes: &[ClassId]) -> ManagedHeap {
    let plans: Vec<_> = classes.iter().map(|class| plan(registry, *class)).collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap
}
fn method(registry: &DefinitionRegistry, class: ClassId, name: &str, desc: &str) -> MethodId {
    registry
        .find_declared_method(class, &units(name), &units(desc))
        .unwrap()
        .unwrap()
}
#[derive(Default)]
struct Resolver {
    selected: HashMap<(LoaderId, Vec<u16>), ClassId>,
    calls: Rc<Cell<usize>>,
}
impl HierarchyResolver for Resolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        self.calls.set(self.calls.get() + 1);
        if let Some(class) = self.selected.get(&(loader, name.to_vec())) {
            return Ok(Some(*class));
        }
        registry
            .lookup_class(loader, name)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

fn interface(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    supers: &[&str],
    methods: &[(&str, &str, u16)],
    refs: &[(&str, &str, &str, u8)],
) -> Def {
    define(
        registry,
        loader,
        name,
        Some("java/lang/Object"),
        supers,
        0x601,
        methods,
        refs,
    )
}

fn select(
    registry: &DefinitionRegistry,
    heap: &mut ManagedHeap,
    origin: &Def,
    index: usize,
    receiver: Reference,
) -> Result<MethodId, E> {
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        registry,
        heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    context.select_special(origin.class, origin.refs[index], receiver)
}

#[test]
fn current_private_class_and_interface_calls_retain_the_exact_owner_and_overload() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let own = interface(
        &mut registry,
        loader,
        "I",
        &[],
        &[("m", "()I", 2), ("m", "(I)I", 1)],
        &[("I", "m", "()I", 11), ("I", "m", "(I)I", 11)],
    );
    let base = define(
        &mut registry,
        loader,
        "Base",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[("m", "()I", 2), ("m", "(I)I", 1)],
        &[("Base", "m", "()I", 10), ("Base", "m", "(I)I", 10)],
    );
    let child = define(
        &mut registry,
        loader,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[("m", "()I", 1), ("m", "(I)I", 1)],
        &[],
    );
    let mut heap = heap(&registry, &[child.class]);
    let receiver = heap.allocate_instance(child.class).unwrap();
    for origin in [&own, &base] {
        for (index, desc) in [(0, "()I"), (1, "(I)I")] {
            assert_eq!(
                select(&registry, &mut heap, origin, index, receiver).unwrap(),
                method(&registry, origin.class, "m", desc)
            );
        }
    }
}

#[test]
fn superclass_lookup_uses_the_direct_parent_even_for_older_ancestor_symbols_and_clear_super() {
    for major in [45, 51, 52, 65] {
        for flags in [1, 0x21] {
            let mut registry = registry();
            let loader = registry.create_loader().unwrap();
            object(&mut registry, loader);
            define(
                &mut registry,
                loader,
                "Grand",
                Some("java/lang/Object"),
                &[],
                0x21,
                &[("m", "()I", 1)],
                &[],
            );
            let parent = define(
                &mut registry,
                loader,
                "Parent",
                Some("Grand"),
                &[],
                0x21,
                &[("m", "()I", 1)],
                &[],
            );
            let origin = define_version(
                &mut registry,
                loader,
                "Origin",
                Some("Parent"),
                &[],
                flags,
                &[("m", "()I", 1)],
                &[("Grand", "m", "()I", 10), ("Parent", "m", "()I", 10)],
                major,
            );
            let child = define(
                &mut registry,
                loader,
                "Child",
                Some("Origin"),
                &[],
                0x21,
                &[("m", "()I", 1)],
                &[],
            );
            let mut heap = heap(&registry, &[child.class]);
            let receiver = heap.allocate_instance(child.class).unwrap();
            for index in [0, 1] {
                assert_eq!(
                    select(&registry, &mut heap, &origin, index, receiver).unwrap(),
                    method(&registry, parent.class, "m", "()I")
                );
            }
        }
    }
}

#[test]
fn special_selection_skips_static_but_accepts_private_or_package_candidates_without_reaccess() {
    for flags in [9, 2, 0, 4] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let grand = define(
            &mut registry,
            loader,
            "p/Grand",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()I", 1)],
            &[],
        );
        let parent = define(
            &mut registry,
            loader,
            "p/Parent",
            Some("p/Grand"),
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[],
        );
        let origin = define(
            &mut registry,
            loader,
            "q/Origin",
            Some("p/Parent"),
            &[],
            0x21,
            &[],
            &[("p/Grand", "m", "()I", 10)],
        );
        let mut heap = heap(&registry, &[origin.class]);
        let receiver = heap.allocate_instance(origin.class).unwrap();
        let expected = if flags == 9 {
            grand.class
        } else {
            parent.class
        };
        assert_eq!(
            select(&registry, &mut heap, &origin, 0, receiver).unwrap(),
            method(&registry, expected, "m", "()I")
        );
    }
}

#[test]
fn resolved_staticness_and_private_access_precede_owner_and_null_checks() {
    for flags in [9, 2] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let unrelated = define(
            &mut registry,
            loader,
            "Unrelated",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[],
        );
        let origin = define(
            &mut registry,
            loader,
            "Origin",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[("Unrelated", "m", "()I", 10)],
        );
        let mut heap = heap(&registry, &[origin.class, unrelated.class]);
        assert_eq!(
            select(&registry, &mut heap, &origin, 0, Reference::null()),
            Err(if flags == 9 {
                E::Java(J::IncompatibleClassChange)
            } else {
                E::UnsupportedNestAccess {
                    origin: origin.class,
                    owner: unrelated.class,
                }
            })
        );
    }
}

#[test]
fn direct_interface_default_is_selected_without_receiver_override_or_unrelated_defaults() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let top = interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
    interface(&mut registry, loader, "Direct", &["Top"], &[], &[]);
    interface(&mut registry, loader, "Other", &[], &[("m", "()I", 1)], &[]);
    let origin = define(
        &mut registry,
        loader,
        "Origin",
        Some("java/lang/Object"),
        &["Direct", "Other"],
        0x21,
        &[("m", "()I", 1)],
        &[("Direct", "m", "()I", 11)],
    );
    let child = define(
        &mut registry,
        loader,
        "Child",
        Some("Origin"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let mut heap = heap(&registry, &[child.class]);
    let receiver = heap.allocate_instance(child.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &origin, 0, receiver).unwrap(),
        method(&registry, top.class, "m", "()I")
    );
}

#[test]
fn superclass_interface_fallback_uses_the_prescribed_class_not_the_actual_receiver() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let own = interface(&mut registry, loader, "I", &[], &[("m", "()I", 1)], &[]);
    interface(&mut registry, loader, "Other", &[], &[("m", "()I", 1)], &[]);
    define(
        &mut registry,
        loader,
        "Parent",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "Origin",
        Some("Parent"),
        &["Other"],
        0x21,
        &[],
        &[("Parent", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[origin.class]);
    let receiver = heap.allocate_instance(origin.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &origin, 0, receiver).unwrap(),
        method(&registry, own.class, "m", "()I")
    );
}

#[test]
fn abstract_masking_reabstraction_and_default_conflicts_follow_null() {
    for variant in 0..3 {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
        interface(&mut registry, loader, "Other", &[], &[("m", "()I", 1)], &[]);
        let (parent, tag) = if variant == 0 {
            define(
                &mut registry,
                loader,
                "Parent",
                Some("java/lang/Object"),
                &["Top"],
                0x421,
                &[("m", "()I", 0x401)],
                &[],
            );
            ("Parent", 10)
        } else {
            interface(
                &mut registry,
                loader,
                "Parent",
                if variant == 1 {
                    &["Top"]
                } else {
                    &["Top", "Other"]
                },
                if variant == 1 {
                    &[("m", "()I", 0x401)]
                } else {
                    &[]
                },
                &[],
            );
            ("Parent", 11)
        };
        let origin = define(
            &mut registry,
            loader,
            "Origin",
            Some(if tag == 10 {
                parent
            } else {
                "java/lang/Object"
            }),
            if tag == 11 { &["Parent"] } else { &[] },
            0x21,
            &[],
            &[(parent, "m", "()I", tag)],
        );
        let mut heap = heap(&registry, &[origin.class]);
        let receiver = heap.allocate_instance(origin.class).unwrap();
        assert_eq!(
            select(&registry, &mut heap, &origin, 0, Reference::null()),
            Err(E::Java(J::NullPointer))
        );
        assert_eq!(
            select(&registry, &mut heap, &origin, 0, receiver),
            Err(E::Java(if variant == 2 {
                J::IncompatibleClassChange
            } else {
                J::AbstractMethod
            }))
        );
    }
}

#[test]
fn symbolic_owner_must_be_current_superclass_or_direct_interface() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let top = interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
    interface(&mut registry, loader, "Direct", &["Top"], &[], &[]);
    let sibling = define(
        &mut registry,
        loader,
        "Sibling",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "Origin",
        Some("java/lang/Object"),
        &["Direct"],
        0x21,
        &[],
        &[("Top", "m", "()I", 11), ("Sibling", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[origin.class, sibling.class]);
    for (index, owner) in [(0, top.class), (1, sibling.class)] {
        assert_eq!(
            select(&registry, &mut heap, &origin, index, Reference::null()),
            Err(E::InvalidSpecialOwner {
                origin: origin.class,
                owner
            })
        );
    }
}

#[test]
fn receiver_must_be_below_the_current_origin_even_for_public_super_methods() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let parent = define(
        &mut registry,
        loader,
        "Parent",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "Origin",
        Some("Parent"),
        &[],
        0x21,
        &[],
        &[("Parent", "m", "()I", 10)],
    );
    let sibling = define(
        &mut registry,
        loader,
        "Sibling",
        Some("Parent"),
        &[],
        0x21,
        &[],
        &[],
    );
    let mut heap = heap(&registry, &[origin.class, sibling.class]);
    for actual in [parent.class, sibling.class] {
        let receiver = heap.allocate_instance(actual).unwrap();
        assert_eq!(
            select(&registry, &mut heap, &origin, 0, receiver),
            Err(E::Context(C::InvalidReceiver {
                expected: origin.class,
                actual
            }))
        );
    }
}

#[test]
fn interface_object_dependency_permits_public_but_not_protected_access() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("public", "()I", 1), ("protected", "()V", 4)],
        &[],
    );
    interface(
        &mut registry,
        loader,
        "Top",
        &[],
        &[("public", "()I", 1)],
        &[],
    );
    let own = interface(
        &mut registry,
        loader,
        "I",
        &["Top"],
        &[],
        &[
            ("java/lang/Object", "public", "()I", 10),
            ("java/lang/Object", "protected", "()V", 10),
            ("I", "public", "()I", 11),
        ],
    );
    let root = define(
        &mut registry,
        loader,
        "Root",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &own, 0, receiver).unwrap(),
        method(&registry, object.class, "public", "()I")
    );
    assert_eq!(
        select(&registry, &mut heap, &own, 1, Reference::null()),
        Err(E::Java(J::IllegalAccess))
    );
    assert_eq!(
        select(&registry, &mut heap, &own, 2, receiver).unwrap(),
        method(&registry, object.class, "public", "()I")
    );
}

#[test]
fn interface_reference_version_boundary_and_kind_errors_are_distinct() {
    for major in [45, 51, 52, 65] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let own = interface(&mut registry, loader, "I", &[], &[("m", "()I", 1)], &[]);
        let origin = define_version(
            &mut registry,
            loader,
            "Origin",
            Some("java/lang/Object"),
            &["I"],
            0x21,
            &[("m", "()I", 1)],
            &[
                ("I", "m", "()I", 11),
                ("I", "m", "()I", 10),
                ("Origin", "m", "()I", 11),
            ],
            major,
        );
        let mut heap = heap(&registry, &[origin.class]);
        let receiver = heap.allocate_instance(origin.class).unwrap();
        if major < 52 {
            for index in [0, 2] {
                assert_eq!(
                    select(&registry, &mut heap, &origin, index, receiver),
                    Err(E::InvalidConstant {
                        origin: origin.class,
                        index: origin.refs[index]
                    })
                );
            }
        } else {
            assert_eq!(
                select(&registry, &mut heap, &origin, 0, receiver).unwrap(),
                method(&registry, own.class, "m", "()I")
            );
            assert_eq!(
                select(&registry, &mut heap, &origin, 2, Reference::null()),
                Err(E::Java(J::IncompatibleClassChange))
            );
        }
        assert_eq!(
            select(&registry, &mut heap, &origin, 1, Reference::null()),
            Err(E::Java(J::IncompatibleClassChange))
        );
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        for index in [0, u16::MAX, origin.wrong] {
            assert_eq!(
                context.select_special(origin.class, index, Reference::null()),
                Err(E::InvalidConstant {
                    origin: origin.class,
                    index
                })
            );
        }
    }
}

#[test]
fn special_reference_descriptors_reach_lookup_but_initializers_and_array_owners_fail() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let origin = define(
        &mut registry,
        loader,
        "Origin",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("Origin", "<init>", "()V", 10),
            ("Origin", "m", "(Ljava/lang/Object;)I", 10),
            ("Origin", "m", "()[I", 10),
            ("[I", "m", "()I", 10),
        ],
    );
    let mut heap = heap(&registry, &[origin.class]);
    for (index, expected) in [
        (0, E::UnsupportedSpecialMethod),
        (1, E::Java(J::NoSuchMethod)),
        (2, E::Java(J::NoSuchMethod)),
        (3, E::Context(C::UnsupportedArrayOwner)),
    ] {
        assert_eq!(
            select(&registry, &mut heap, &origin, index, Reference::null()),
            Err(expected)
        );
    }
}

#[test]
fn foreign_same_named_origins_and_wrong_heap_kinds_are_host_failures() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let own = interface(
        &mut registry,
        first,
        "I",
        &[],
        &[("m", "()I", 1)],
        &[("I", "m", "()I", 11)],
    );
    interface(&mut registry, second, "I", &[], &[("m", "()I", 1)], &[]);
    let root = define(
        &mut registry,
        second,
        "C",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[],
    );
    let mut heap = heap(&registry, &[own.class, root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &own, 0, receiver),
        Err(E::Context(C::InvalidReceiver {
            expected: own.class,
            actual: root.class
        }))
    );
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    assert!(matches!(
        select(&registry, &mut heap, &own, 0, array),
        Err(E::Context(C::Instance(InstanceError::Heap(
            HeapError::WrongEntryKind { .. }
        ))))
    ));
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let reference = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    assert_eq!(
        select(&registry, &mut heap, &own, 0, reference),
        Err(E::Context(C::Instance(InstanceError::Heap(
            HeapError::ForeignHeap
        ))))
    );
}

#[test]
fn inconsistent_object_identity_is_checked_for_symbolic_and_selected_owners() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    let first_object = define(
        &mut registry,
        first,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    object(&mut registry, second);
    let own = interface(
        &mut registry,
        first,
        "I",
        &[],
        &[],
        &[("I", "m", "()I", 11), ("java/lang/Object", "m", "()I", 10)],
    );
    let root = define(
        &mut registry,
        second,
        "C",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[],
    );
    let mut resolver = Resolver::default();
    resolver.selected.insert((second, units("I")), own.class);
    let plan = build_hierarchy(
        &registry,
        root.class,
        &mut resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(root.class).unwrap();
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for index in &own.refs {
        assert_eq!(
            context.select_special(own.class, *index, receiver),
            Err(E::Context(C::InvalidReceiver {
                expected: first_object.class,
                actual: root.class
            }))
        );
    }
}

#[test]
fn public_inherited_special_targets_do_not_recheck_hidden_declarers() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let hidden = define(
        &mut registry,
        loader,
        "p/Hidden",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[("m", "()I", 1)],
        &[],
    );
    define(
        &mut registry,
        loader,
        "p/Parent",
        Some("p/Hidden"),
        &[],
        0x21,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "q/Origin",
        Some("p/Parent"),
        &[],
        0x21,
        &[],
        &[("p/Parent", "m", "()I", 10), ("p/Hidden", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[origin.class]);
    let receiver = heap.allocate_instance(origin.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &origin, 0, receiver).unwrap(),
        method(&registry, hidden.class, "m", "()I")
    );
    assert_eq!(
        select(&registry, &mut heap, &origin, 1, Reference::null()),
        Err(E::Java(J::IllegalAccess))
    );
}

#[test]
fn native_synchronized_and_receiver_final_overrides_are_not_prepared_or_verified_here() {
    for flags in [0x101, 0x21, 0x11] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let parent = define(
            &mut registry,
            loader,
            "Parent",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[],
        );
        let origin = define(
            &mut registry,
            loader,
            "Origin",
            Some("Parent"),
            &[],
            0x21,
            &[],
            &[("Parent", "m", "()I", 10)],
        );
        let child = define(
            &mut registry,
            loader,
            "Child",
            Some("Origin"),
            &[],
            0x21,
            &[("m", "()I", 1)],
            &[],
        );
        let mut heap = heap(&registry, &[child.class]);
        let receiver = heap.allocate_instance(child.class).unwrap();
        assert_eq!(
            select(&registry, &mut heap, &origin, 0, receiver).unwrap(),
            method(&registry, parent.class, "m", "()I")
        );
    }
}

#[test]
fn exact_special_work_boundaries_include_relation_selection_and_receiver_checks() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[("C", "m", "()I", 10), ("java/lang/Object", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[origin.class]);
    let receiver = heap.allocate_instance(origin.class).unwrap();
    // Current-type: resolution2 + origin/owner receiver checks4 + lookup2 +
    // selected-declarer check2. Super-type adds owner relation4 and longer checks.
    for (index, exact) in [(0, 13), (1, 19)] {
        for limit in [exact - 1, exact, usize::MAX] {
            let mut resolver = SameLoaderResolver;
            let mut context = FieldResolutionContext::new_unnamed_modules(
                &registry,
                &mut heap,
                &mut resolver,
                FieldResolutionOptions {
                    max_lookup_steps: limit,
                    ..FieldResolutionOptions::default()
                },
            )
            .unwrap();
            assert_eq!(context.lookup_steps(), 3);
            let result = context.select_special(origin.class, origin.refs[index], receiver);
            if limit < exact {
                assert_eq!(
                    result,
                    Err(E::Context(C::ResourceLimit {
                        resource: FieldResource::LookupSteps,
                        limit,
                        required: exact
                    }))
                );
            } else {
                assert!(result.is_ok());
                assert_eq!(context.lookup_steps(), exact);
            }
        }
    }
}

#[test]
fn special_virtual_interface_and_field_calls_share_one_cumulative_counter() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    interface(&mut registry, loader, "I", &[], &[("m", "()I", 0x401)], &[]);
    let origin = define(
        &mut registry,
        loader,
        "C",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[("m", "()I", 1)],
        &[
            ("C", "m", "()I", 10),
            ("I", "m", "()I", 11),
            ("C", "missing", "I", 9),
        ],
    );
    let mut heap = heap(&registry, &[origin.class]);
    let receiver = heap.allocate_instance(origin.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: 33,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert_eq!(context.lookup_steps(), 6);
    let target = method(&registry, origin.class, "m", "()I");
    assert_eq!(
        context
            .select_special(origin.class, origin.refs[0], receiver)
            .unwrap(),
        target
    );
    assert_eq!(context.lookup_steps(), 16);
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[0], receiver)
            .unwrap(),
        target
    );
    assert_eq!(context.lookup_steps(), 23);
    assert_eq!(
        context
            .select_interface(origin.class, origin.refs[1], receiver)
            .unwrap(),
        target
    );
    assert_eq!(context.lookup_steps(), 33);
    assert_eq!(
        context.read_field(origin.class, origin.refs[2], receiver),
        Err(C::ResourceLimit {
            resource: FieldResource::LookupSteps,
            limit: 33,
            required: 34
        })
    );
}

#[test]
fn owner_binding_limits_precede_callbacks_and_successful_rejected_owners_are_cached() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let other = define(
        &mut registry,
        second,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        first,
        "D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("C", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[origin.class, other.class]);
    for (symbols, units_limit, error) in [
        (4, 35, Some((FieldResource::Symbols, 4, 5))),
        (5, 34, Some((FieldResource::SymbolUnits, 34, 35))),
        (5, 35, None),
    ] {
        let mut resolver = Resolver::default();
        resolver.selected.insert((first, units("C")), other.class);
        let calls = resolver.calls.clone();
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_symbols: symbols,
                max_symbol_units: units_limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        if let Some((resource, limit, required)) = error {
            assert_eq!(
                context.select_special(origin.class, origin.refs[0], Reference::null()),
                Err(E::Context(C::ResourceLimit {
                    resource,
                    limit,
                    required
                }))
            );
            assert_eq!(calls.get(), 0);
        } else {
            for _ in 0..2 {
                assert_eq!(
                    context.select_special(origin.class, origin.refs[0], Reference::null()),
                    Err(E::InvalidSpecialOwner {
                        origin: origin.class,
                        owner: other.class
                    })
                );
            }
            assert_eq!(calls.get(), 1);
            assert_eq!(context.resolved_symbols(), 5);
            assert_eq!(context.symbol_units(), 35);
        }
    }
}

#[test]
fn special_context_checks_origin_admission_and_heap_invalidation_before_resolution() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let origin = define(
        &mut registry,
        loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[("C", "m", "()I", 10)],
    );
    let missing = define(
        &mut registry,
        loader,
        "Missing",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("C", "m", "()I", 10)],
    );
    let mut replacement = heap(&registry, &[origin.class]);
    let mut heap = heap(&registry, &[origin.class]);
    assert_eq!(
        select(&registry, &mut heap, &missing, 0, Reference::null()),
        Err(E::Context(C::Instance(InstanceError::LayoutMissing {
            class: missing.class
        })))
    );
    let mut foreign = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign.create_loader().unwrap();
    let foreign_object = object(&mut foreign, foreign_loader);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert!(matches!(
        context.select_special(foreign_object, 0, Reference::null()),
        Err(E::Context(C::Registry(_)))
    ));
    std::mem::swap(context.heap_mut(), &mut replacement);
    assert_eq!(
        context.select_special(origin.class, origin.refs[0], Reference::null()),
        Err(E::Context(C::ContextChanged))
    );
    drop(context);
    let extra_plan = plan(&registry, missing.class);
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    context
        .heap_mut()
        .admit_instance_layouts(&registry, &[&extra_plan])
        .unwrap();
    assert_eq!(
        context.select_special(origin.class, origin.refs[0], Reference::null()),
        Err(E::Context(C::ContextChanged))
    );
}

#[test]
fn deep_prescribed_super_lookup_and_context_drop_are_iterative() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 1,
        ..RegistryOptions::default()
    })
    .unwrap();
    let loader = registry.create_loader().unwrap();
    let object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let mut previous = String::from("java/lang/Object");
    let mut last = None;
    for index in 0..DEPTH {
        let name = format!("C{index}");
        last = Some(define(
            &mut registry,
            loader,
            &name,
            Some(&previous),
            &[],
            0x21,
            &[],
            if index + 1 == DEPTH {
                &[("java/lang/Object", "m", "()I", 10)]
            } else {
                &[]
            },
        ));
        previous = name;
    }
    let origin = last.unwrap();
    let plan = build_hierarchy(
        &registry,
        origin.class,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 1,
            max_edges: DEPTH,
            max_declared_fields: 0,
            max_instance_slots: 0,
        },
    )
    .unwrap();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 1,
            max_layout_edges: DEPTH,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(origin.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &origin, 0, receiver).unwrap(),
        method(&registry, object.class, "m", "()I")
    );
    drop(heap);
    drop(plan);
    drop(registry);
}

#[test]
fn direct_interface_and_object_owner_edges_have_inclusive_precomparison_budgets() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    interface(&mut registry, loader, "Unused", &[], &[], &[]);
    let own = interface(
        &mut registry,
        loader,
        "I",
        &[],
        &[("m", "()I", 1)],
        &[("java/lang/Object", "m", "()I", 10)],
    );
    let origin = define(
        &mut registry,
        loader,
        "C",
        Some("java/lang/Object"),
        &["Unused", "I"],
        0x21,
        &[],
        &[("I", "m", "()I", 11)],
    );
    let mut heap = heap(&registry, &[origin.class]);
    // Four nodes + five edges seed9. Resolution2 and owner nodes2 use four.
    // The two direct-interface comparisons use two more: exact total15.
    // Interface->Object has one dependency comparison: exact total14.
    for (caller, exact) in [(&origin, 15), (&own, 14)] {
        for limit in [exact - 1, exact] {
            let mut resolver = SameLoaderResolver;
            let mut context = FieldResolutionContext::new_unnamed_modules(
                &registry,
                &mut heap,
                &mut resolver,
                FieldResolutionOptions {
                    max_lookup_steps: limit,
                    ..FieldResolutionOptions::default()
                },
            )
            .unwrap();
            assert_eq!(context.lookup_steps(), 9);
            let result = context.select_special(caller.class, caller.refs[0], Reference::null());
            assert_eq!(
                result,
                Err(if limit < exact {
                    E::Context(C::ResourceLimit {
                        resource: FieldResource::LookupSteps,
                        limit,
                        required: exact,
                    })
                } else {
                    E::Java(J::NullPointer)
                })
            );
            assert_eq!(context.lookup_steps(), limit);
        }
    }
}

#[test]
fn special_interface_object_fallback_ignores_nonpublic_and_static_object_methods() {
    for flags in [0, 2, 4, 9] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        define(
            &mut registry,
            loader,
            "java/lang/Object",
            None,
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[],
        );
        let top = interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
        let own = interface(
            &mut registry,
            loader,
            "I",
            &["Top"],
            &[],
            &[("I", "m", "()I", 11)],
        );
        let root = define(
            &mut registry,
            loader,
            "C",
            Some("java/lang/Object"),
            &["I"],
            0x21,
            &[],
            &[],
        );
        let mut heap = heap(&registry, &[root.class]);
        let receiver = heap.allocate_instance(root.class).unwrap();
        assert_eq!(
            select(&registry, &mut heap, &own, 0, receiver).unwrap(),
            method(&registry, top.class, "m", "()I")
        );
    }
}

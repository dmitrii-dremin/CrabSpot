//! Original synthetic interface-resolution tests; no Code is prepared or executed.
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
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
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
    interface: bool,
) -> Result<MethodId, E> {
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        registry,
        heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    if interface {
        context.select_interface(origin.class, origin.refs[index], receiver)
    } else {
        context.select_virtual(origin.class, origin.refs[index], receiver)
    }
}

#[test]
fn inherited_default_diamond_and_unrelated_abstract_peer_have_one_concrete_target() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let top = interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
    interface(&mut registry, loader, "Left", &["Top"], &[], &[]);
    interface(&mut registry, loader, "Right", &["Top"], &[], &[]);
    interface(
        &mut registry,
        loader,
        "AbstractPeer",
        &[],
        &[("m", "()I", 0x401)],
        &[],
    );
    let root = define(
        &mut registry,
        loader,
        "Root",
        Some("java/lang/Object"),
        &["Left", "Right", "AbstractPeer"],
        0x21,
        &[],
        &[
            ("Root", "m", "()I", 10),
            ("Left", "m", "()I", 11),
            ("AbstractPeer", "m", "()I", 11),
        ],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    for (index, interface) in [(0, false), (1, true), (2, true)] {
        assert_eq!(
            select(&registry, &mut heap, &root, index, receiver, interface).unwrap(),
            method(&registry, top.class, "m", "()I")
        );
    }
}

#[test]
fn a_more_specific_concrete_default_wins_for_both_call_kinds_and_owner_choices() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
    let child = interface(
        &mut registry,
        loader,
        "Child",
        &["Top"],
        &[("m", "()I", 1)],
        &[],
    );
    interface(
        &mut registry,
        loader,
        "Peer",
        &[],
        &[("m", "()I", 0x401)],
        &[],
    );
    let root = define(
        &mut registry,
        loader,
        "Root",
        Some("java/lang/Object"),
        &["Top", "Peer", "Child"],
        0x21,
        &[],
        &[
            ("Root", "m", "()I", 10),
            ("Top", "m", "()I", 11),
            ("Peer", "m", "()I", 11),
        ],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    for (index, interface) in [(0, false), (1, true), (2, true)] {
        assert_eq!(
            select(&registry, &mut heap, &root, index, receiver, interface).unwrap(),
            method(&registry, child.class, "m", "()I")
        );
    }
}

#[test]
fn more_specific_abstract_declaration_suppresses_the_ancestor_default() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
    interface(
        &mut registry,
        loader,
        "Abstract",
        &["Top"],
        &[("m", "()I", 0x401)],
        &[],
    );
    // Top is discovered first and can be the resolution witness. Selection must
    // still discard it after observing the abstract declaration in Abstract.
    let root = define(
        &mut registry,
        loader,
        "Root",
        Some("java/lang/Object"),
        &["Top", "Abstract"],
        0x21,
        &[],
        &[("Root", "m", "()I", 10), ("Top", "m", "()I", 11)],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    for (index, interface) in [(0, false), (1, true)] {
        assert_eq!(
            select(
                &registry,
                &mut heap,
                &root,
                index,
                Reference::null(),
                interface
            ),
            Err(E::Java(J::NullPointer))
        );
        assert_eq!(
            select(&registry, &mut heap, &root, index, receiver, interface),
            Err(E::Java(J::AbstractMethod))
        );
    }
}

#[test]
fn unrelated_defaults_conflict_across_the_complete_receiver_closure_after_null() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    interface(&mut registry, loader, "Left", &[], &[("m", "()I", 1)], &[]);
    interface(&mut registry, loader, "Right", &[], &[("m", "()I", 1)], &[]);
    let base = define(
        &mut registry,
        loader,
        "Base",
        Some("java/lang/Object"),
        &["Left"],
        0x21,
        &[],
        &[],
    );
    let root = define(
        &mut registry,
        loader,
        "Root",
        Some("Base"),
        &["Right"],
        0x21,
        &[],
        &[
            ("Base", "m", "()I", 10),
            ("Root", "m", "()I", 10),
            ("Left", "m", "()I", 11),
        ],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    for (index, interface) in [(0, false), (1, false), (2, true)] {
        assert_eq!(
            select(
                &registry,
                &mut heap,
                &root,
                index,
                Reference::null(),
                interface
            ),
            Err(E::Java(J::NullPointer))
        );
        assert_eq!(
            select(&registry, &mut heap, &root, index, receiver, interface),
            Err(E::Java(J::IncompatibleClassChange))
        );
    }
    assert!(registry.class(base.class).is_ok());
}

#[test]
fn private_and_static_superinterface_declarations_neither_inherit_nor_dominate() {
    for flags in [2, 9] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let top = interface(&mut registry, loader, "Top", &[], &[("m", "()I", 1)], &[]);
        interface(
            &mut registry,
            loader,
            "Child",
            &["Top"],
            &[("m", "()I", flags), ("other", "(I)I", 1)],
            &[],
        );
        let root = define(
            &mut registry,
            loader,
            "Root",
            Some("java/lang/Object"),
            &["Child"],
            0x21,
            &[],
            &[("Root", "m", "()I", 10), ("Child", "m", "()I", 11)],
        );
        let mut heap = heap(&registry, &[root.class]);
        let receiver = heap.allocate_instance(root.class).unwrap();
        assert_eq!(
            select(&registry, &mut heap, &root, 0, receiver, false).unwrap(),
            method(&registry, top.class, "m", "()I")
        );
        // An own declaration does resolve: its access or staticness wins over null.
        assert_eq!(
            select(&registry, &mut heap, &root, 1, Reference::null(), true),
            Err(if flags == 2 {
                E::UnsupportedNestAccess {
                    origin: root.class,
                    owner: registry
                        .lookup_class(loader, &units("Child"))
                        .unwrap()
                        .unwrap(),
                }
            } else {
                E::Java(J::IncompatibleClassChange)
            })
        );
    }
}

#[test]
fn own_private_interface_declaration_is_exact_and_nonvirtual() {
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
    let root = define(
        &mut registry,
        loader,
        "Root",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    for (index, desc) in [(0, "()I"), (1, "(I)I")] {
        assert_eq!(
            select(&registry, &mut heap, &own, index, receiver, true).unwrap(),
            method(&registry, own.class, "m", desc)
        );
    }
    assert_eq!(
        select(&registry, &mut heap, &own, 0, Reference::null(), true),
        Err(E::Java(J::NullPointer))
    );
}

#[test]
fn class_implementations_mask_defaults_without_declaring_the_interface() {
    for flags in [1, 0, 4, 0x401, 0x400] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        interface(&mut registry, loader, "I", &[], &[("m", "()I", 1)], &[]);
        let base = define(
            &mut registry,
            loader,
            "Base",
            Some("java/lang/Object"),
            &[],
            0x421,
            &[("m", "()I", flags)],
            &[],
        );
        let root = define(
            &mut registry,
            loader,
            "Root",
            Some("Base"),
            &["I"],
            0x21,
            &[],
            &[("I", "m", "()I", 11)],
        );
        let mut heap = heap(&registry, &[root.class]);
        let receiver = heap.allocate_instance(root.class).unwrap();
        let result = select(&registry, &mut heap, &root, 0, receiver, true);
        if flags == 1 {
            assert_eq!(result.unwrap(), method(&registry, base.class, "m", "()I"));
        } else {
            assert_eq!(
                result,
                Err(E::Java(if flags & 5 == 0 || flags == 4 {
                    J::IllegalAccess
                } else {
                    J::AbstractMethod
                }))
            );
        }
    }
}

#[test]
fn private_and_static_class_matches_are_skipped_for_public_interface_resolution() {
    for flags in [2, 9] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let own = interface(&mut registry, loader, "I", &[], &[("m", "()I", 1)], &[]);
        let root = define(
            &mut registry,
            loader,
            "Root",
            Some("java/lang/Object"),
            &["I"],
            0x21,
            &[("m", "()I", flags)],
            &[("I", "m", "()I", 11)],
        );
        let mut heap = heap(&registry, &[root.class]);
        let receiver = heap.allocate_instance(root.class).unwrap();
        assert_eq!(
            select(&registry, &mut heap, &root, 0, receiver, true).unwrap(),
            method(&registry, own.class, "m", "()I")
        );
    }
}

#[test]
fn own_interface_resolution_precedes_object_and_overloads_require_exact_descriptors() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("m", "()I", 1), ("n", "()I", 1)],
        &[],
    );
    interface(
        &mut registry,
        loader,
        "Parent",
        &[],
        &[("m", "()I", 1)],
        &[],
    );
    let own = interface(
        &mut registry,
        loader,
        "I",
        &["Parent"],
        &[("m", "(I)I", 1), ("n", "()I", 9)],
        &[
            ("I", "m", "()I", 11),
            ("I", "m", "(I)I", 11),
            ("I", "n", "()I", 11),
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
        select(&registry, &mut heap, &own, 0, receiver, true).unwrap(),
        method(&registry, object.class, "m", "()I")
    );
    assert_eq!(
        select(&registry, &mut heap, &own, 1, receiver, true).unwrap(),
        method(&registry, own.class, "m", "(I)I")
    );
    assert_eq!(
        select(&registry, &mut heap, &own, 2, Reference::null(), true),
        Err(E::Java(J::IncompatibleClassChange))
    );
}

#[test]
fn object_fallback_requires_an_own_public_nonstatic_exact_declaration() {
    for flags in [1, 0, 2, 4, 9] {
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
        let own = interface(
            &mut registry,
            loader,
            "I",
            &[],
            &[],
            &[("I", "m", "()I", 11)],
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
        assert_eq!(
            select(&registry, &mut heap, &own, 0, Reference::null(), true),
            Err(E::Java(if flags == 1 {
                J::NullPointer
            } else {
                J::NoSuchMethod
            }))
        );
    }
}

#[test]
fn interface_owner_access_precedes_kind_and_wrong_cp_tags_are_host_failures() {
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
        &[],
        &[],
    );
    let public = define(
        &mut registry,
        loader,
        "p/Public",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    interface(&mut registry, loader, "p/I", &[], &[("m", "()I", 1)], &[]);
    let origin = define(
        &mut registry,
        loader,
        "q/Origin",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("p/Hidden", "m", "()I", 11),
            ("p/Public", "m", "()I", 11),
            ("p/I", "m", "()I", 10),
        ],
    );
    let mut heap = heap(
        &registry,
        &[
            origin.class,
            hidden.class,
            public.class,
            registry
                .lookup_class(loader, &units("p/I"))
                .unwrap()
                .unwrap(),
        ],
    );
    for (index, expected) in [
        (0, E::Java(J::IllegalAccess)),
        (1, E::Java(J::IncompatibleClassChange)),
        (
            2,
            E::InvalidConstant {
                origin: origin.class,
                index: origin.refs[2],
            },
        ),
    ] {
        assert_eq!(
            select(
                &registry,
                &mut heap,
                &origin,
                index,
                Reference::null(),
                true
            ),
            Err(expected)
        );
    }
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
            context.select_interface(origin.class, index, Reference::null()),
            Err(E::InvalidConstant {
                origin: origin.class,
                index
            })
        );
    }
}

#[test]
fn receiver_membership_uses_loader_identity_and_follows_inherited_interfaces() {
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
    define(
        &mut registry,
        first,
        "Base",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[],
    );
    let good = define(
        &mut registry,
        first,
        "Good",
        Some("Base"),
        &[],
        0x21,
        &[],
        &[],
    );
    let wrong = define(
        &mut registry,
        second,
        "Wrong",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[],
    );
    let mut heap = heap(&registry, &[good.class, wrong.class]);
    let good_receiver = heap.allocate_instance(good.class).unwrap();
    let wrong_receiver = heap.allocate_instance(wrong.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &own, 0, good_receiver, true).unwrap(),
        method(&registry, own.class, "m", "()I")
    );
    assert_eq!(
        select(&registry, &mut heap, &own, 0, wrong_receiver, true),
        Err(E::Java(J::IncompatibleClassChange))
    );
}

#[test]
fn inherited_public_interface_members_do_not_recheck_the_hidden_declarer() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let hidden = define(
        &mut registry,
        loader,
        "p/Hidden",
        Some("java/lang/Object"),
        &[],
        0x600,
        &[("m", "()I", 1)],
        &[],
    );
    interface(&mut registry, loader, "p/Public", &["p/Hidden"], &[], &[]);
    let root = define(
        &mut registry,
        loader,
        "q/Root",
        Some("java/lang/Object"),
        &["p/Public"],
        0x21,
        &[],
        &[("p/Public", "m", "()I", 11)],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &root, 0, receiver, true).unwrap(),
        method(&registry, hidden.class, "m", "()I")
    );
}

#[test]
fn selected_native_and_synchronized_class_bodies_remain_preparation_responsibilities() {
    for flags in [0x101, 0x21] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        interface(&mut registry, loader, "I", &[], &[("m", "()I", 0x401)], &[]);
        let root = define(
            &mut registry,
            loader,
            "Root",
            Some("java/lang/Object"),
            &["I"],
            0x21,
            &[("m", "()I", flags)],
            &[("I", "m", "()I", 11)],
        );
        let mut heap = heap(&registry, &[root.class]);
        let receiver = heap.allocate_instance(root.class).unwrap();
        assert_eq!(
            select(&registry, &mut heap, &root, 0, receiver, true).unwrap(),
            method(&registry, root.class, "m", "()I")
        );
    }
}

#[test]
fn foreign_unadmitted_and_array_receivers_keep_checked_host_errors() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let own = interface(
        &mut registry,
        loader,
        "I",
        &[],
        &[("m", "()I", 1)],
        &[("I", "m", "()I", 11)],
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
    let missing = define(
        &mut registry,
        loader,
        "Missing",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("I", "m", "()I", 11)],
    );
    let mut heap = heap(&registry, &[root.class]);
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    assert_eq!(
        select(&registry, &mut heap, &own, 0, foreign_ref, true),
        Err(E::Context(C::Instance(InstanceError::Heap(
            HeapError::ForeignHeap
        ))))
    );
    assert!(matches!(
        select(&registry, &mut heap, &own, 0, array, true),
        Err(E::Context(C::Instance(InstanceError::Heap(
            HeapError::WrongEntryKind { .. }
        ))))
    ));
    assert_eq!(
        select(&registry, &mut heap, &missing, 0, Reference::null(), true),
        Err(E::Context(C::Instance(InstanceError::LayoutMissing {
            class: missing.class
        })))
    );
    let other = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let mut resolver = SameLoaderResolver;
    // Foreign origins are checked before CP lookup, even for null.
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    // A class identity from another registry is obtained without fabricating a handle.
    let mut other = other;
    let other_loader = other.create_loader().unwrap();
    let other_object = object(&mut other, other_loader);
    assert!(matches!(
        context.select_interface(other_object, own.refs[0], Reference::null()),
        Err(E::Context(C::Registry(_)))
    ));
}

#[test]
fn interface_reference_descriptors_reach_lookup_but_array_owners_remain_unsupported() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let own = interface(
        &mut registry,
        loader,
        "I",
        &[],
        &[],
        &[
            ("I", "m", "(Ljava/lang/Object;)I", 11),
            ("I", "m", "()[I", 11),
            ("[I", "m", "()I", 11),
        ],
    );
    let mut heap = heap(&registry, &[own.class]);
    for (index, expected) in [
        (0, E::Java(J::NoSuchMethod)),
        (1, E::Java(J::NoSuchMethod)),
        (2, E::Context(C::UnsupportedArrayOwner)),
    ] {
        assert_eq!(
            select(&registry, &mut heap, &own, index, Reference::null(), true),
            Err(expected)
        );
    }
}

#[test]
fn interface_selection_detects_layout_growth_and_same_size_heap_replacement() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let own = interface(
        &mut registry,
        loader,
        "I",
        &[],
        &[("m", "()I", 1)],
        &[("I", "m", "()I", 11)],
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
    let extra = define(
        &mut registry,
        loader,
        "Extra",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let extra_plan = plan(&registry, extra.class);
    let mut replacement = heap(&registry, &[root.class]);
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context.heap().retained_layout_classes(),
        replacement.retained_layout_classes()
    );
    std::mem::swap(context.heap_mut(), &mut replacement);
    assert_eq!(
        context.select_interface(own.class, own.refs[0], receiver),
        Err(E::Context(C::ContextChanged))
    );
    drop(context);
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
        context.select_interface(own.class, own.refs[0], Reference::null()),
        Err(E::Context(C::ContextChanged))
    );
}

#[test]
fn exact_interface_work_and_field_work_use_the_same_cumulative_budget() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    interface(&mut registry, loader, "I", &[], &[("m", "()I", 1)], &[]);
    let root = define(
        &mut registry,
        loader,
        "C",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[("I", "m", "()I", 11), ("C", "missing", "I", 9)],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    for limit in [19, 20, 23, usize::MAX] {
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
        assert_eq!(context.lookup_steps(), 6); // 3 nodes + 3 structural edges.
        let result = context.select_interface(root.class, root.refs[0], receiver);
        if limit == 19 {
            assert_eq!(
                result,
                Err(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 20
                }))
            );
            assert_eq!(context.lookup_steps(), 19);
            continue;
        }
        result.unwrap();
        assert_eq!(context.lookup_steps(), 20);
        let result = context.read_field(root.class, root.refs[1], receiver);
        if limit == 20 {
            assert_eq!(
                result,
                Err(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 21
                })
            );
        } else {
            assert_eq!(
                result,
                Err(C::Java(crabspot_runtime::FieldJavaError::NoSuchField))
            );
            assert_eq!(context.lookup_steps(), 23);
            if limit == 23 {
                assert_eq!(
                    context.select_interface(root.class, root.refs[0], receiver),
                    Err(E::Context(C::ResourceLimit {
                        resource: FieldResource::LookupSteps,
                        limit,
                        required: 24
                    }))
                );
            }
        }
    }
}

#[test]
fn interface_alias_bindings_cache_success_and_enforce_exact_symbol_quotas() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let own = interface(&mut registry, second, "I", &[], &[("m", "()I", 1)], &[]);
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
    let origin = define(
        &mut registry,
        first,
        "D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("I", "m", "()I", 11), ("I", "m", "()I", 11)],
    );
    let mut heap = heap(&registry, &[root.class, origin.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    for (symbols, units_limit, expected) in [
        (5, 36, Some((FieldResource::Symbols, 5, 6))),
        (6, 35, Some((FieldResource::SymbolUnits, 35, 36))),
        (6, 36, None),
    ] {
        let mut resolver = Resolver::default();
        resolver.selected.insert((first, units("I")), own.class);
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
        assert_eq!(context.resolved_symbols(), 5);
        assert_eq!(context.symbol_units(), 35);
        if let Some((resource, limit, required)) = expected {
            assert_eq!(
                context.select_interface(origin.class, origin.refs[0], receiver),
                Err(E::Context(C::ResourceLimit {
                    resource,
                    limit,
                    required
                }))
            );
            assert_eq!(calls.get(), 0);
        } else {
            for index in &origin.refs {
                assert_eq!(
                    context
                        .select_interface(origin.class, *index, receiver)
                        .unwrap(),
                    method(&registry, own.class, "m", "()I")
                );
            }
            assert_eq!(calls.get(), 1);
            assert_eq!(context.resolved_symbols(), 6);
            assert_eq!(context.symbol_units(), 36);
        }
    }
}

#[test]
fn candidate_self_pairs_ancestry_and_fanout_edges_fail_before_unbudgeted_work() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let names: Vec<_> = (0..8).map(|index| format!("I{index}")).collect();
    for name in &names {
        interface(&mut registry, loader, name, &[], &[("m", "()I", 1)], &[]);
    }
    let interfaces: Vec<_> = names.iter().map(String::as_str).collect();
    let root = define(
        &mut registry,
        loader,
        "C",
        Some("java/lang/Object"),
        &interfaces,
        0x21,
        &[],
        &[("C", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[root.class]);
    // Constructor: 10 nodes + 17 edges = 27. Resolution class nodes: 2.
    // Candidate roots/edges: 10. Candidate nodes/declarations: 16.
    // Thus 55 ends immediately before the first (self) candidate comparison;
    // 56 permits it, 57 permits the next pair, and 58 starts its ancestry work.
    for limit in [30, 55, 56, 57] {
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
        assert_eq!(context.lookup_steps(), 27);
        assert_eq!(
            context.select_virtual(root.class, root.refs[0], Reference::null()),
            Err(E::Context(C::ResourceLimit {
                resource: FieldResource::LookupSteps,
                limit,
                required: limit + 1
            }))
        );
        assert_eq!(context.lookup_steps(), limit);
    }
    assert_eq!(
        select(&registry, &mut heap, &root, 0, Reference::null(), false),
        Err(E::Java(J::NullPointer))
    );
}

#[test]
fn inconsistent_cross_loader_object_ownership_is_a_checked_host_failure() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    define(
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
        &[("I", "m", "()I", 11)],
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
    assert!(matches!(
        context.select_interface(own.class, own.refs[0], receiver),
        Err(E::Context(C::InvalidState(_)))
    ));
}

#[test]
fn failed_interface_owner_resolution_is_retryable_and_success_is_cached() {
    struct Retry {
        attempts: usize,
        target: ClassId,
    }
    impl HierarchyResolver for Retry {
        fn resolve(
            &mut self,
            _: &DefinitionRegistry,
            _: LoaderId,
            _: &[u16],
        ) -> Result<Option<ClassId>, ResolveError> {
            self.attempts += 1;
            match self.attempts {
                1 => Err(ResolveError::new("temporarily unavailable")),
                2 => Ok(None),
                _ => Ok(Some(self.target)),
            }
        }
    }
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let own = interface(&mut registry, second, "I", &[], &[("m", "()I", 1)], &[]);
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
    let origin = define(
        &mut registry,
        first,
        "D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("I", "m", "()I", 11)],
    );
    let mut heap = heap(&registry, &[root.class, origin.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    let mut resolver = Retry {
        attempts: 0,
        target: own.class,
    };
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert!(matches!(
        context.select_interface(origin.class, origin.refs[0], receiver),
        Err(E::Context(C::Resolve { .. }))
    ));
    assert_eq!(context.resolved_symbols(), 5);
    assert!(matches!(
        context.select_interface(origin.class, origin.refs[0], receiver),
        Err(E::Context(C::MissingDefinition { .. }))
    ));
    assert_eq!(context.resolved_symbols(), 5);
    for _ in 0..2 {
        assert_eq!(
            context
                .select_interface(origin.class, origin.refs[0], receiver)
                .unwrap(),
            method(&registry, own.class, "m", "()I")
        );
    }
    assert_eq!(context.resolved_symbols(), 6);
    drop(context);
    assert_eq!(resolver.attempts, 3);
}

#[test]
fn deep_interface_resolution_selection_and_drop_are_iterative() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 2,
        ..RegistryOptions::default()
    })
    .unwrap();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let top = interface(&mut registry, loader, "I0", &[], &[("m", "()I", 1)], &[]);
    let mut previous = String::from("I0");
    for index in 1..DEPTH {
        let name = format!("I{index}");
        interface(&mut registry, loader, &name, &[&previous], &[], &[]);
        previous = name;
    }
    let root = define(
        &mut registry,
        loader,
        "C",
        Some("java/lang/Object"),
        &[&previous],
        0x21,
        &[],
        &[("C", "m", "()I", 10), (&previous, "m", "()I", 11)],
    );
    let plan = build_hierarchy(
        &registry,
        root.class,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 2,
            max_edges: DEPTH * 2 + 1,
            max_declared_fields: 0,
            max_instance_slots: 0,
        },
    )
    .unwrap();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 2,
            max_layout_edges: DEPTH * 2 + 1,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(root.class).unwrap();
    assert_eq!(
        select(&registry, &mut heap, &root, 0, receiver, false).unwrap(),
        method(&registry, top.class, "m", "()I")
    );
    assert_eq!(
        select(&registry, &mut heap, &root, 1, receiver, true).unwrap(),
        method(&registry, top.class, "m", "()I")
    );
    drop(heap);
    drop(plan);
    drop(registry);
}

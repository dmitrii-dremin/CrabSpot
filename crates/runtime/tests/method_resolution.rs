//! Independent class-resolution/selection tests; these do not prepare or execute Code.
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

#[test]
fn exact_resolution_and_runtime_overrides_preserve_method_owner_and_descriptor() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "p/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "(I)I", 1), ("m", "(J)J", 1)],
        &[
            ("p/Base", "m", "(I)I", 10),
            ("p/Base", "m", "(J)J", 10),
            ("p/Base", "m", "()I", 10),
        ],
    );
    let child = define(
        &mut registry,
        loader,
        "p/Child",
        Some("p/Base"),
        &[],
        0x21,
        &[("m", "(I)I", 1)],
        &[],
    );
    let mut heap = heap(&registry, &[child.class]);
    let receiver = heap.allocate_instance(child.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(base.class, base.refs[0], receiver)
            .unwrap(),
        method(&registry, child.class, "m", "(I)I")
    );
    assert_eq!(
        context
            .select_virtual(base.class, base.refs[1], receiver)
            .unwrap(),
        method(&registry, base.class, "m", "(J)J")
    );
    assert_eq!(
        context
            .select_virtual(base.class, base.refs[2], Reference::null())
            .unwrap_err(),
        E::Java(J::NoSuchMethod)
    );
}

#[test]
fn private_resolution_is_nonvirtual_and_static_or_private_candidates_do_not_override() {
    for child_flags in [1, 2, 9] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let base = define(
            &mut registry,
            loader,
            "p/Base",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("private", "()I", 2), ("public", "()I", 1)],
            &[
                ("p/Base", "private", "()I", 10),
                ("p/Base", "public", "()I", 10),
            ],
        );
        let child = define(
            &mut registry,
            loader,
            "p/Child",
            Some("p/Base"),
            &[],
            0x21,
            &[
                ("private", "()I", child_flags),
                ("public", "()I", child_flags),
            ],
            &[],
        );
        let mut heap = heap(&registry, &[child.class]);
        let receiver = heap.allocate_instance(child.class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            context
                .select_virtual(base.class, base.refs[0], receiver)
                .unwrap(),
            method(&registry, base.class, "private", "()I")
        );
        assert_eq!(
            context
                .select_virtual(base.class, base.refs[1], receiver)
                .unwrap()
                .owner(),
            if child_flags == 1 {
                child.class
            } else {
                base.class
            }
        );
    }
}

#[test]
fn owner_access_member_access_staticness_and_abstractness_have_required_null_order() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "owner/E",
        Some("java/lang/Object"),
        &[],
        0x421,
        &[
            ("package", "()I", 0),
            ("private", "()I", 2),
            ("static", "()I", 9),
            ("abstract", "()I", 0x401),
        ],
        &[],
    );
    let hidden = define(
        &mut registry,
        loader,
        "owner/Hidden",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "other/D",
        Some("owner/E"),
        &[],
        0x21,
        &[],
        &[
            ("owner/Hidden", "missing", "()I", 10),
            ("owner/E", "package", "()I", 10),
            ("owner/E", "private", "()I", 10),
            ("owner/E", "static", "()I", 10),
            ("owner/E", "abstract", "()I", 10),
        ],
    );
    let mut heap = heap(&registry, &[origin.class, hidden.class]);
    let receiver = heap.allocate_instance(origin.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for (index, expected) in [
        (0, J::IllegalAccess),
        (1, J::IllegalAccess),
        (3, J::IncompatibleClassChange),
        (4, J::NullPointer),
    ] {
        assert_eq!(
            context
                .select_virtual(origin.class, origin.refs[index], Reference::null())
                .unwrap_err(),
            E::Java(expected)
        );
    }
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[2], Reference::null())
            .unwrap_err(),
        E::UnsupportedNestAccess {
            origin: origin.class,
            owner: owner.class
        }
    );
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[4], receiver)
            .unwrap_err(),
        E::Java(J::AbstractMethod)
    );
}

#[test]
fn protected_static_siblings_bypass_owner_relation_while_nonstatic_receivers_do_not() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "base/E",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("static", "()I", 0xc), ("instance", "()I", 4)],
        &[],
    );
    let sibling = define(
        &mut registry,
        loader,
        "third/C",
        Some("base/E"),
        &[],
        0x21,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "origin/D",
        Some("base/E"),
        &[],
        0x21,
        &[],
        &[
            ("third/C", "static", "()I", 10),
            ("third/C", "instance", "()I", 10),
            ("base/E", "instance", "()I", 10),
        ],
    );
    let mut heap = heap(&registry, &[origin.class, sibling.class]);
    let base_receiver = heap.allocate_instance(base.class).unwrap();
    let receiver = heap.allocate_instance(origin.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[0], Reference::null())
            .unwrap_err(),
        E::Java(J::IncompatibleClassChange)
    );
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[1], Reference::null())
            .unwrap_err(),
        E::Java(J::IllegalAccess)
    );
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[2], base_receiver)
            .unwrap_err(),
        E::Context(C::InvalidReceiver {
            expected: origin.class,
            actual: base.class
        })
    );
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[2], receiver)
            .unwrap()
            .owner(),
        base.class
    );
}

#[test]
fn public_inherited_methods_from_inaccessible_declarers_remain_callable() {
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
    let public = define(
        &mut registry,
        loader,
        "p/Public",
        Some("p/Hidden"),
        &[],
        0x21,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "other/D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/Public", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[origin.class, public.class]);
    let receiver = heap.allocate_instance(public.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[0], receiver)
            .unwrap(),
        method(&registry, hidden.class, "m", "()I")
    );
}

#[test]
fn package_nonoverrides_and_transitive_public_bridges_keep_all_reachability() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "p/A",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 0)],
        &[("p/A", "m", "()I", 10)],
    );
    let nonoverride = define(
        &mut registry,
        loader,
        "q/B",
        Some("p/A"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let bridge = define(
        &mut registry,
        loader,
        "p/C",
        Some("q/B"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let package_after_bridge = define(
        &mut registry,
        loader,
        "p/D",
        Some("p/C"),
        &[],
        0x21,
        &[("m", "()I", 0)],
        &[],
    );
    let leaf = define(
        &mut registry,
        loader,
        "r/E",
        Some("p/D"),
        &[],
        0x21,
        &[("m", "()I", 0)],
        &[],
    );
    let mut heap = heap(&registry, &[leaf.class]);
    let mut receivers = Vec::new();
    for class in [
        nonoverride.class,
        bridge.class,
        package_after_bridge.class,
        leaf.class,
    ] {
        receivers.push(heap.allocate_instance(class).unwrap());
    }
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for (receiver, expected) in receivers.into_iter().zip([
        base.class,
        bridge.class,
        package_after_bridge.class,
        leaf.class,
    ]) {
        assert_eq!(
            context
                .select_virtual(base.class, base.refs[0], receiver)
                .unwrap()
                .owner(),
            expected
        );
    }
}

#[test]
fn matching_package_text_in_another_loader_is_not_a_package_override() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    let object = object(&mut registry, first);
    let base = define(
        &mut registry,
        first,
        "p/A",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 0)],
        &[("p/A", "m", "()I", 10)],
    );
    let child = define(
        &mut registry,
        second,
        "p/B",
        Some("p/A"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let mut resolver = Resolver::default();
    resolver.selected.insert((second, units("p/A")), base.class);
    resolver
        .selected
        .insert((second, units("java/lang/Object")), object);
    let plan = build_hierarchy(
        &registry,
        child.class,
        &mut resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(child.class).unwrap();
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(base.class, base.refs[0], receiver)
            .unwrap()
            .owner(),
        base.class
    );
}

#[test]
fn final_checks_cover_declarations_above_resolved_method_and_preserve_null_order() {
    for flags in [0x11, 0x14] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let base = define(
            &mut registry,
            loader,
            "p/Base",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[("p/Base", "m", "()I", 10)],
        );
        let child = define(
            &mut registry,
            loader,
            "p/Child",
            Some("p/Base"),
            &[],
            0x21,
            &[("m", "()I", 1)],
            &[("p/Child", "m", "()I", 10)],
        );
        let mut heap = heap(&registry, &[child.class]);
        let base_receiver = heap.allocate_instance(base.class).unwrap();
        let receiver = heap.allocate_instance(child.class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let base_method = method(&registry, base.class, "m", "()I");
        let child_method = method(&registry, child.class, "m", "()I");
        assert_eq!(
            context
                .select_virtual(base.class, base.refs[0], base_receiver)
                .unwrap(),
            base_method
        );
        assert_eq!(
            context
                .select_virtual(child.class, child.refs[0], Reference::null())
                .unwrap_err(),
            E::Java(J::NullPointer)
        );
        for (origin, index) in [(base.class, base.refs[0]), (child.class, child.refs[0])] {
            assert_eq!(
                context.select_virtual(origin, index, receiver).unwrap_err(),
                E::InvalidOverride {
                    method: child_method,
                    overridden: base_method
                }
            );
        }
    }
}

#[test]
fn package_final_nonoverrides_are_skipped_but_original_package_override_is_rejected() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "p/A",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 0x10)],
        &[("p/A", "m", "()I", 10)],
    );
    let other = define(
        &mut registry,
        loader,
        "q/B",
        Some("p/A"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let bad = define(
        &mut registry,
        loader,
        "p/C",
        Some("q/B"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let mut heap = heap(&registry, &[bad.class]);
    let other_receiver = heap.allocate_instance(other.class).unwrap();
    let bad_receiver = heap.allocate_instance(bad.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(base.class, base.refs[0], other_receiver)
            .unwrap(),
        method(&registry, base.class, "m", "()I")
    );
    assert_eq!(
        context
            .select_virtual(base.class, base.refs[0], bad_receiver)
            .unwrap_err(),
        E::InvalidOverride {
            method: method(&registry, bad.class, "m", "()I"),
            overridden: method(&registry, base.class, "m", "()I")
        }
    );
}

#[test]
fn private_final_methods_remain_nonvirtual_and_do_not_prohibit_public_descendants() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let base = define(
        &mut registry,
        loader,
        "p/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 0x12)],
        &[("p/Base", "m", "()I", 10)],
    );
    let child = define(
        &mut registry,
        loader,
        "p/Child",
        Some("p/Base"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[],
    );
    let mut heap = heap(&registry, &[child.class]);
    let receiver = heap.allocate_instance(child.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(base.class, base.refs[0], receiver)
            .unwrap()
            .owner(),
        base.class
    );
}

#[test]
fn interface_fallback_reports_an_eligible_witness_and_ignores_private_static_or_unrelated_methods()
{
    for (flags, expected) in [(1, true), (0x401, true), (2, false), (9, false)] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let interface = define(
            &mut registry,
            loader,
            "p/K",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[("m", "()I", flags)],
            &[],
        );
        define(
            &mut registry,
            loader,
            "p/I",
            Some("java/lang/Object"),
            &["p/K"],
            0x601,
            &[("unrelated", "()I", 0x401)],
            &[],
        );
        let root = define(
            &mut registry,
            loader,
            "p/Root",
            Some("java/lang/Object"),
            &["p/I"],
            0x21,
            &[],
            &[("p/Root", "m", "()I", 10)],
        );
        let mut heap = heap(&registry, &[root.class]);
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            context
                .select_virtual(root.class, root.refs[0], Reference::null())
                .unwrap_err(),
            if expected {
                E::UnsupportedInterfaceMethods {
                    owner: interface.class,
                }
            } else {
                E::Java(J::NoSuchMethod)
            }
        );
    }
}

#[test]
fn class_declarations_mask_interface_defaults_including_abstract_and_static_methods() {
    for flags in [0x401, 9] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        define(
            &mut registry,
            loader,
            "p/I",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[("m", "()I", 1)],
            &[],
        );
        define(
            &mut registry,
            loader,
            "p/Base",
            Some("java/lang/Object"),
            &[],
            0x421,
            &[("m", "()I", flags)],
            &[],
        );
        let root = define(
            &mut registry,
            loader,
            "p/Root",
            Some("p/Base"),
            &["p/I"],
            0x21,
            &[],
            &[("p/Root", "m", "()I", 10)],
        );
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
            context
                .select_virtual(root.class, root.refs[0], Reference::null())
                .unwrap_err(),
            E::Java(if flags & 8 != 0 {
                J::IncompatibleClassChange
            } else {
                J::NullPointer
            })
        );
        assert_eq!(
            context
                .select_virtual(root.class, root.refs[0], receiver)
                .unwrap_err(),
            E::Java(if flags & 8 != 0 {
                J::IncompatibleClassChange
            } else {
                J::AbstractMethod
            })
        );
    }
}

#[test]
fn symbolic_interface_owner_and_interface_methodref_tag_are_distinct_failures() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    define(
        &mut registry,
        loader,
        "p/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("m", "()I", 1)],
        &[],
    );
    define(
        &mut registry,
        loader,
        "p/Hidden",
        Some("java/lang/Object"),
        &[],
        0x600,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "other/D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("p/I", "m", "()I", 10),
            ("p/I", "m", "()I", 11),
            ("p/Hidden", "m", "()I", 10),
        ],
    );
    let interface = registry
        .lookup_class(loader, &units("p/I"))
        .unwrap()
        .unwrap();
    let hidden = registry
        .lookup_class(loader, &units("p/Hidden"))
        .unwrap()
        .unwrap();
    let mut heap = heap(&registry, &[origin.class, interface, hidden]);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[0], Reference::null())
            .unwrap_err(),
        E::Java(J::IncompatibleClassChange)
    );
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[1], Reference::null())
            .unwrap_err(),
        E::InvalidConstant {
            origin: origin.class,
            index: origin.refs[1]
        }
    );
    assert_eq!(
        context
            .select_virtual(origin.class, origin.refs[2], Reference::null())
            .unwrap_err(),
        E::Java(J::IllegalAccess)
    );
}

#[test]
fn signature_polymorphic_name_priority_precedes_exact_descriptor_search() {
    for class_name in [
        "java/lang/invoke/MethodHandle",
        "java/lang/invoke/VarHandle",
    ] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let special = define(
            &mut registry,
            loader,
            class_name,
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("invoke", "([Ljava/lang/Object;)Ljava/lang/Object;", 0x181)],
            &[(class_name, "invoke", "(I)I", 10)],
        );
        let mut heap = heap(&registry, &[special.class]);
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            context
                .select_virtual(special.class, special.refs[0], Reference::null())
                .unwrap_err(),
            E::UnsupportedSignaturePolymorphic {
                method: method(
                    &registry,
                    special.class,
                    "invoke",
                    "([Ljava/lang/Object;)Ljava/lang/Object;"
                )
            }
        );
    }
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let special = define(
        &mut registry,
        loader,
        "java/lang/invoke/MethodHandle",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("invoke", "([Ljava/lang/Object;)Ljava/lang/Object;", 0x181),
            ("invoke", "()I", 1),
        ],
        &[("java/lang/invoke/MethodHandle", "invoke", "()I", 10)],
    );
    let mut heap = heap(&registry, &[special.class]);
    let receiver = heap.allocate_instance(special.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(special.class, special.refs[0], receiver)
            .unwrap(),
        method(&registry, special.class, "invoke", "()I")
    );
}

#[test]
fn native_and_synchronized_selected_targets_are_returned_for_later_preparation() {
    for flags in [0x101, 0x21] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let owner = define(
            &mut registry,
            loader,
            "p/Owner",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[("p/Owner", "m", "()I", 10)],
        );
        let mut heap = heap(&registry, &[owner.class]);
        let receiver = heap.allocate_instance(owner.class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            context
                .select_virtual(owner.class, owner.refs[0], receiver)
                .unwrap(),
            method(&registry, owner.class, "m", "()I")
        );
    }
}

#[test]
fn malformed_symbols_reference_descriptors_and_special_names_are_host_boundaries() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("p/Owner", "m", "(Ljava/lang/Object;)I", 10),
            ("p/Owner", "m", "()[I", 10),
            ("p/Owner", "<init>", "()V", 10),
            ("[I", "m", "()I", 10),
        ],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for index in [0, u16::MAX, owner.wrong] {
        assert_eq!(
            context
                .select_virtual(owner.class, index, Reference::null())
                .unwrap_err(),
            E::InvalidConstant {
                origin: owner.class,
                index
            }
        );
    }
    for index in [0, 1] {
        assert_eq!(
            context
                .select_virtual(owner.class, owner.refs[index], Reference::null())
                .unwrap_err(),
            E::UnsupportedDescriptor
        );
    }
    assert_eq!(
        context
            .select_virtual(owner.class, owner.refs[2], Reference::null())
            .unwrap_err(),
        E::UnsupportedSpecialMethod
    );
    assert_eq!(
        context
            .select_virtual(owner.class, owner.refs[3], Reference::null())
            .unwrap_err(),
        E::Context(C::UnsupportedArrayOwner)
    );
}

#[test]
fn signature_polymorphic_detection_requires_the_exact_definition() {
    for (name, descriptor, flags) in [
        (
            "other/MethodHandle",
            "([Ljava/lang/Object;)Ljava/lang/Object;",
            0x181,
        ),
        (
            "java/lang/invoke/MethodHandle",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            0x181,
        ),
        (
            "java/lang/invoke/VarHandle",
            "([[Ljava/lang/Object;)Ljava/lang/Object;",
            0x181,
        ),
        (
            "java/lang/invoke/MethodHandle",
            "([Ljava/lang/Object;)Ljava/lang/Object;",
            0x101,
        ),
        (
            "java/lang/invoke/VarHandle",
            "([Ljava/lang/Object;)Ljava/lang/Object;",
            0x81,
        ),
    ] {
        let mut registry = registry();
        let loader = registry.create_loader().unwrap();
        object(&mut registry, loader);
        let owner = define(
            &mut registry,
            loader,
            name,
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("invoke", descriptor, flags)],
            &[(name, "invoke", "(I)I", 10)],
        );
        let mut heap = heap(&registry, &[owner.class]);
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            context
                .select_virtual(owner.class, owner.refs[0], Reference::null())
                .unwrap_err(),
            E::Java(J::NoSuchMethod)
        );
    }
}

#[test]
fn interface_fallback_visits_indirect_diamonds_once_and_includes_superclass_interfaces() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let shared = define(
        &mut registry,
        loader,
        "p/K",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("m", "()I", 2)],
        &[],
    );
    define(
        &mut registry,
        loader,
        "p/I",
        Some("java/lang/Object"),
        &["p/K"],
        0x601,
        &[],
        &[],
    );
    define(
        &mut registry,
        loader,
        "p/J",
        Some("java/lang/Object"),
        &["p/K"],
        0x601,
        &[],
        &[],
    );
    let root = define(
        &mut registry,
        loader,
        "p/Root",
        Some("java/lang/Object"),
        &["p/I", "p/J"],
        0x21,
        &[],
        &[("p/Root", "m", "()I", 10)],
    );
    let eligible = define(
        &mut registry,
        loader,
        "q/Eligible",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("m", "()I", 0x401)],
        &[],
    );
    let base = define(
        &mut registry,
        loader,
        "q/Base",
        Some("java/lang/Object"),
        &["q/Eligible"],
        0x21,
        &[],
        &[],
    );
    let child = define(
        &mut registry,
        loader,
        "q/Child",
        Some("q/Base"),
        &[],
        0x21,
        &[],
        &[("q/Child", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[root.class, child.class]);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let before = context.lookup_steps();
    assert_eq!(
        context
            .select_virtual(root.class, root.refs[0], Reference::null())
            .unwrap_err(),
        E::Java(J::NoSuchMethod)
    );
    assert_eq!(context.lookup_steps() - before, 8);
    assert_eq!(
        context
            .select_virtual(child.class, child.refs[0], Reference::null())
            .unwrap_err(),
        E::UnsupportedInterfaceMethods {
            owner: eligible.class
        }
    );
    assert!(context.registry().class(shared.class).is_ok());
    assert!(context.registry().class(base.class).is_ok());
}

#[test]
fn origin_and_receiver_membership_are_checked_by_the_shared_context() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[("p/Owner", "m", "()I", 10)],
    );
    let other = define(
        &mut registry,
        loader,
        "p/Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let unadmitted = define(
        &mut registry,
        loader,
        "p/Unadmitted",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/Owner", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[owner.class, other.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let wrong = heap.allocate_instance(other.class).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let foreign = {
        let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        heap.allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap()
    };
    let mut foreign_registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign_registry.create_loader().unwrap();
    let foreign_class = object(&mut foreign_registry, foreign_loader);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(unadmitted.class, unadmitted.refs[0], receiver)
            .unwrap_err(),
        E::Context(C::Instance(InstanceError::LayoutMissing {
            class: unadmitted.class
        }))
    );
    assert!(matches!(
        context.select_virtual(foreign_class, 1, receiver),
        Err(E::Context(C::Registry(_)))
    ));
    assert_eq!(
        context
            .select_virtual(owner.class, owner.refs[0], wrong)
            .unwrap_err(),
        E::Context(C::InvalidReceiver {
            expected: owner.class,
            actual: other.class
        })
    );
    assert_eq!(
        context
            .select_virtual(owner.class, owner.refs[0], array)
            .unwrap_err(),
        E::Context(C::Instance(InstanceError::Heap(
            HeapError::WrongEntryKind {
                expected: crabspot_runtime::HeapEntryKind::Instance,
                actual: crabspot_runtime::HeapEntryKind::PrimitiveArray
            }
        )))
    );
    assert_eq!(
        context
            .select_virtual(owner.class, owner.refs[0], foreign)
            .unwrap_err(),
        E::Context(C::Instance(InstanceError::Heap(HeapError::ForeignHeap)))
    );
}

#[test]
fn cumulative_work_limits_are_shared_without_a_dispatch_cache_and_are_inclusive() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[("p/Owner", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    {
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_symbols: 2,
                max_symbol_units: 23,
                max_lookup_steps: 10,
            },
        )
        .unwrap();
        assert_eq!(context.lookup_steps(), 3);
        assert_eq!(
            context
                .select_virtual(owner.class, owner.refs[0], receiver)
                .unwrap()
                .owner(),
            owner.class
        );
        assert_eq!(context.lookup_steps(), 10);
        assert_eq!(
            context
                .select_virtual(owner.class, owner.refs[0], receiver)
                .unwrap_err(),
            E::Context(C::ResourceLimit {
                resource: FieldResource::LookupSteps,
                limit: 10,
                required: 11
            })
        );
        assert_eq!(context.lookup_steps(), 10);
        assert_eq!(context.resolved_symbols(), 2);
        assert_eq!(context.symbol_units(), 23);
    }
    for options in [
        FieldResolutionOptions {
            max_symbols: usize::MAX,
            max_symbol_units: usize::MAX,
            max_lookup_steps: usize::MAX,
        },
        FieldResolutionOptions::default(),
    ] {
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            options,
        )
        .unwrap();
        assert_eq!(
            context
                .select_virtual(owner.class, owner.refs[0], receiver)
                .unwrap()
                .owner(),
            owner.class
        );
        let first = context.lookup_steps();
        context
            .select_virtual(owner.class, owner.refs[0], receiver)
            .unwrap();
        assert_eq!(context.lookup_steps() - first, 7);
    }
}

#[test]
fn virtual_owner_bindings_use_existing_symbol_limits_and_cache_success_once() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let owner = define(
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
    let mut heap = heap(&registry, &[origin.class, owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    for (options, expected) in [
        (
            FieldResolutionOptions {
                max_symbols: 4,
                ..FieldResolutionOptions::default()
            },
            Some(C::ResourceLimit {
                resource: FieldResource::Symbols,
                limit: 4,
                required: 5,
            }),
        ),
        (
            FieldResolutionOptions {
                max_symbol_units: 34,
                ..FieldResolutionOptions::default()
            },
            Some(C::ResourceLimit {
                resource: FieldResource::SymbolUnits,
                limit: 34,
                required: 35,
            }),
        ),
        (
            FieldResolutionOptions {
                max_symbols: 5,
                max_symbol_units: 35,
                ..FieldResolutionOptions::default()
            },
            None,
        ),
    ] {
        let mut resolver = Resolver::default();
        resolver.selected.insert((first, units("C")), owner.class);
        let calls = resolver.calls.clone();
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            options,
        )
        .unwrap();
        if let Some(expected) = expected {
            assert_eq!(
                context
                    .select_virtual(origin.class, origin.refs[0], receiver)
                    .unwrap_err(),
                E::Context(expected)
            );
            assert_eq!(calls.get(), 0);
        } else {
            for _ in 0..2 {
                assert_eq!(
                    context
                        .select_virtual(origin.class, origin.refs[0], receiver)
                        .unwrap()
                        .owner(),
                    owner.class
                );
            }
            assert_eq!(calls.get(), 1);
            assert_eq!(context.resolved_symbols(), 5);
            assert_eq!(context.symbol_units(), 35);
        }
    }
}

#[test]
fn virtual_selection_rejects_layout_extension_and_equal_size_heap_replacement() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[("p/Owner", "m", "()I", 10)],
    );
    let extra = define(
        &mut registry,
        loader,
        "p/Extra",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let extra_plan = plan(&registry, extra.class);
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    {
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
            context
                .select_virtual(owner.class, owner.refs[0], receiver)
                .unwrap_err(),
            E::Context(C::ContextChanged)
        );
    }
    let plans = [plan(&registry, owner.class), extra_plan];
    let mut replacement =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    replacement
        .admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
        .unwrap();
    let new_receiver = replacement.allocate_instance(owner.class).unwrap();
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    std::mem::swap(context.heap_mut(), &mut replacement);
    assert_eq!(
        context
            .select_virtual(owner.class, owner.refs[0], new_receiver)
            .unwrap_err(),
        E::Context(C::ContextChanged)
    );
}

#[test]
fn deep_method_resolution_selection_and_drop_do_not_recurse() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 1,
        ..RegistryOptions::default()
    })
    .unwrap();
    let loader = registry.create_loader().unwrap();
    let base = define(
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
        let class = format!("deep/C{index}");
        let refs = if index + 1 == DEPTH {
            vec![(class.as_str(), "m", "()I", 10)]
        } else {
            Vec::new()
        };
        last = Some(define(
            &mut registry,
            loader,
            &class,
            Some(&previous),
            &[],
            0x21,
            &[],
            &refs,
        ));
        previous = class;
    }
    let last = last.unwrap();
    let plan = build_hierarchy(
        &registry,
        last.class,
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
    let receiver = heap.allocate_instance(last.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .select_virtual(last.class, last.refs[0], receiver)
            .unwrap(),
        method(&registry, base.class, "m", "()I")
    );
    drop(context);
    drop(heap);
    drop(plan);
}

#[test]
fn field_and_method_lookup_charge_the_same_cumulative_work_counter() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 1)],
        &[("p/Owner", "m", "()I", 10), ("p/Owner", "missing", "I", 9)],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: 12,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert_eq!(context.lookup_steps(), 3);
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[1], receiver)
            .unwrap_err(),
        C::Java(crabspot_runtime::FieldJavaError::NoSuchField)
    );
    assert_eq!(context.lookup_steps(), 5);
    assert_eq!(
        context
            .select_virtual(owner.class, owner.refs[0], receiver)
            .unwrap()
            .owner(),
        owner.class
    );
    assert_eq!(context.lookup_steps(), 12);
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[1], receiver)
            .unwrap_err(),
        C::ResourceLimit {
            resource: FieldResource::LookupSteps,
            limit: 12,
            required: 13
        }
    );
}

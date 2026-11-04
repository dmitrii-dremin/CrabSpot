//! Shared symbolic static-call resolution, access/category order and bounded work.
use crabspot_classfile::{ParseOptions, descriptor::parse_method_descriptor};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError as C, FieldResolutionContext,
    FieldResolutionOptions, FieldResource, HeapOptions, HierarchyOptions, HierarchyPlan,
    HierarchyResolver, InstanceError, InstanceOptions, LoaderId, ManagedHeap,
    MethodAccessError as E, MethodId, MethodJavaError as J, Reference, ReferenceTypeUniverse as U,
    RegistryOptions, ResolveError, SameLoaderResolver, Value, build_hierarchy,
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
        let mut encoded = Vec::new();
        for unit in text.encode_utf16() {
            match unit {
                1..=127 => encoded.push(unit as u8),
                0..=0x7ff => encoded.extend([0xc0 | (unit >> 6) as u8, 0x80 | (unit & 63) as u8]),
                _ => encoded.extend([
                    0xe0 | (unit >> 12) as u8,
                    0x80 | ((unit >> 6) & 63) as u8,
                    0x80 | (unit & 63) as u8,
                ]),
            }
        }
        let mut bytes = vec![1];
        bytes.extend(words(&[encoded.len() as u16]));
        bytes.extend(encoded);
        if let Some(index) = self.0.iter().position(|entry| *entry == bytes) {
            return (index + 1) as u16;
        }
        self.0.push(bytes);
        self.0.len() as u16
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
    define_full(
        registry,
        loader,
        name,
        parent,
        interfaces,
        flags,
        &[],
        methods,
        refs,
        major,
    )
}
#[allow(clippy::too_many_arguments)]
fn define_full(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[(&str, &str, u16)],
    methods: &[(&str, &str, u16)],
    refs: &[(&str, &str, &str, u8)],
    major: u16,
) -> Def {
    let mut pool = Pool::default();
    let owner = pool.class(name);
    let parent = parent.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let code_name = pool.text("Code");
    let fields: Vec<_> = fields
        .iter()
        .map(|(name, desc, flags)| (pool.text(name), pool.text(desc), *flags))
        .collect();
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
    bytes.extend(words(&[fields.len() as u16]));
    for (name, desc, flags) in fields {
        bytes.extend(words(&[flags, name, desc, 0]));
    }
    bytes.extend(words(&[methods.len() as u16]));
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
    response: Option<Result<Option<ClassId>, ResolveError>>,
}
impl HierarchyResolver for Resolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        self.calls.set(self.calls.get() + 1);
        if let Some(response) = &self.response {
            return response.clone();
        }
        if let Some(class) = self.selected.get(&(loader, name.to_vec())) {
            return Ok(Some(*class));
        }
        registry
            .lookup_class(loader, name)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

fn setup() -> (DefinitionRegistry, U) {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let object = object(&mut registry, loader);
    let cloneable = define(
        &mut registry,
        loader,
        "java/lang/Cloneable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
    )
    .class;
    let serializable = define(
        &mut registry,
        loader,
        "java/io/Serializable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
    )
    .class;
    (
        registry,
        U {
            bootstrap_loader: loader,
            object,
            cloneable,
            serializable,
        },
    )
}
fn bound_heap(registry: &DefinitionRegistry, u: U, classes: &[ClassId]) -> ManagedHeap {
    let mut roots = classes.to_vec();
    roots.extend([u.cloneable, u.serializable]);
    let mut heap = heap(registry, &roots);
    heap.bind_reference_types(registry, u).unwrap();
    heap
}
fn context<'a>(
    registry: &'a DefinitionRegistry,
    heap: &'a mut ManagedHeap,
    resolver: &'a mut dyn HierarchyResolver,
) -> FieldResolutionContext<'a> {
    FieldResolutionContext::new_unnamed_modules(
        registry,
        heap,
        resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap()
}

#[test]
fn own_inherited_hidden_overloads_and_semantic_aliases_return_actual_declarations() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()I", 9), ("m", "(I)I", 9)],
        &[],
    );
    let child = define(
        &mut registry,
        l,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[("m", "()I", 9)],
        &[],
    );
    let caller = define(
        &mut registry,
        l,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("Base", "m", "()I", 10),
            ("Child", "m", "()I", 10),
            ("Child", "m", "(I)I", 10),
            ("Child", "m", "(I)I", 10),
        ],
    );
    let mut heap = heap(&registry, &[child.class, caller.class]);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (index, target) in [
        (0, method(&registry, base.class, "m", "()I")),
        (1, method(&registry, child.class, "m", "()I")),
        (2, method(&registry, base.class, "m", "(I)I")),
        (3, method(&registry, base.class, "m", "(I)I")),
    ] {
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[index]),
            Ok(target)
        );
    }
    assert_eq!(calls.get(), 0);
    assert_eq!(
        (
            ctx.heap().allocated_arrays(),
            ctx.heap().allocated_instances()
        ),
        (0, 0)
    );
}

#[test]
fn nearer_instance_or_private_declarations_block_static_ancestors_before_category_check() {
    for flags in [1, 2, 0x401] {
        let mut registry = registry();
        let l = registry.create_loader().unwrap();
        object(&mut registry, l);
        define(
            &mut registry,
            l,
            "Base",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()I", 9)],
            &[],
        );
        let child = define(
            &mut registry,
            l,
            "Child",
            Some("Base"),
            &[],
            if flags & 0x400 != 0 { 0x421 } else { 0x21 },
            &[("m", "()I", flags)],
            &[("Child", "m", "()I", 10)],
        );
        let caller = define(
            &mut registry,
            l,
            "Caller",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[("Child", "m", "()I", 10)],
        );
        let mut heap = heap(&registry, &[child.class, caller.class]);
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.resolve_static(child.class, child.refs[0]),
            Err(E::Java(J::IncompatibleClassChange))
        );
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[0]),
            Err(if flags == 2 {
                E::UnsupportedNestAccess {
                    origin: caller.class,
                    owner: child.class,
                }
            } else {
                E::Java(J::IncompatibleClassChange)
            })
        );
    }
}

#[test]
fn static_resolution_does_not_run_a_final_override_or_receiver_audit() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()V", 0x19)],
        &[],
    );
    let child = define(
        &mut registry,
        l,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[("m", "()V", 9)],
        &[("Child", "m", "()V", 10)],
    );
    let mut heap = heap(&registry, &[child.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.resolve_static(child.class, child.refs[0]),
        Ok(method(&registry, child.class, "m", "()V"))
    );
    assert_eq!(ctx.lookup_steps() - before, 2);
}

#[test]
fn interface_own_statics_and_private_helpers_resolve_but_parent_statics_do_not_inherit() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let parent = define(
        &mut registry,
        l,
        "Parent",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("m", "()V", 9), ("private", "()V", 10)],
        &[("Parent", "private", "()V", 11)],
    );
    let child = define(
        &mut registry,
        l,
        "Child",
        Some("java/lang/Object"),
        &["Parent"],
        0x601,
        &[],
        &[],
    );
    let own = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &["Child"],
        0x21,
        &[],
        &[
            ("Parent", "m", "()V", 11),
            ("Child", "m", "()V", 11),
            ("C", "m", "()V", 10),
            ("Parent", "private", "()V", 11),
            ("Child", "private", "()V", 11),
            ("C", "private", "()V", 10),
        ],
    );
    let mut heap = heap(&registry, &[own.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.resolve_static(own.class, own.refs[0]),
        Ok(method(&registry, parent.class, "m", "()V"))
    );
    assert_eq!(
        ctx.resolve_static(parent.class, parent.refs[0]),
        Ok(method(&registry, parent.class, "private", "()V"))
    );
    for index in [1, 2, 4, 5] {
        assert_eq!(
            ctx.resolve_static(own.class, own.refs[index]),
            Err(E::Java(J::NoSuchMethod))
        );
    }
    assert_eq!(
        ctx.resolve_static(own.class, own.refs[3]),
        Err(E::UnsupportedNestAccess {
            origin: own.class,
            owner: parent.class
        })
    );
    assert_ne!(child.class, parent.class);
}

#[test]
fn object_default_abstract_and_conflicting_interface_fallbacks_resolve_then_fail_static_category() {
    for flags in [1, 0x401] {
        let mut registry = registry();
        let l = registry.create_loader().unwrap();
        define(
            &mut registry,
            l,
            "java/lang/Object",
            None,
            &[],
            0x21,
            &[("object", "()I", 1)],
            &[],
        );
        define(
            &mut registry,
            l,
            "I",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[("m", "()I", flags)],
            &[],
        );
        define(
            &mut registry,
            l,
            "J",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[("m", "()I", 1)],
            &[],
        );
        let child = define(
            &mut registry,
            l,
            "Child",
            Some("java/lang/Object"),
            &["I"],
            0x601,
            &[],
            &[],
        );
        let own = define(
            &mut registry,
            l,
            "C",
            Some("java/lang/Object"),
            &["Child", "J"],
            0x21,
            &[],
            &[
                ("Child", "object", "()I", 11),
                ("Child", "m", "()I", 11),
                ("C", "m", "()I", 10),
            ],
        );
        let mut heap = heap(&registry, &[own.class]);
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        for index in 0..3 {
            assert_eq!(
                ctx.resolve_static(own.class, own.refs[index]),
                Err(E::Java(J::IncompatibleClassChange))
            );
        }
        assert!(ctx.heap().retained_layout_classes() > 0);
        assert_ne!(child.class, own.class);
    }
}

#[test]
fn public_static_members_from_hidden_declaring_parents_remain_accessible_via_public_owners() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let hidden = define(
        &mut registry,
        l,
        "p/Hidden",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[("m", "()V", 9)],
        &[],
    );
    let public = define(
        &mut registry,
        l,
        "p/Public",
        Some("p/Hidden"),
        &[],
        0x21,
        &[],
        &[],
    );
    let caller = define(
        &mut registry,
        l,
        "q/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("p/Public", "m", "()V", 10),
            ("p/Hidden", "m", "()V", 10),
            ("p/Hidden", "m", "()V", 11),
        ],
    );
    let mut heap = heap(&registry, &[public.class, caller.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.resolve_static(caller.class, caller.refs[0]),
        Ok(method(&registry, hidden.class, "m", "()V"))
    );
    for index in [1, 2] {
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[index]),
            Err(E::Java(J::IllegalAccess))
        );
    }
}

#[test]
fn protected_sibling_owner_uses_actual_static_flag_before_eventual_category_failure() {
    for flags in [12, 4] {
        let mut registry = registry();
        let l = registry.create_loader().unwrap();
        object(&mut registry, l);
        let base = define(
            &mut registry,
            l,
            "p/Base",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[],
        );
        define(
            &mut registry,
            l,
            "p/Sibling",
            Some("p/Base"),
            &[],
            0x21,
            &[],
            &[],
        );
        let origin = define(
            &mut registry,
            l,
            "q/Origin",
            Some("p/Base"),
            &[],
            0x21,
            &[],
            &[("p/Sibling", "m", "()I", 10), ("p/Base", "m", "()I", 10)],
        );
        let other = define(
            &mut registry,
            l,
            "q/Other",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[("p/Base", "m", "()I", 10)],
        );
        let mut heap = heap(
            &registry,
            &[
                origin.class,
                other.class,
                registry
                    .lookup_class(l, &units("p/Sibling"))
                    .unwrap()
                    .unwrap(),
            ],
        );
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        if flags & 8 != 0 {
            assert_eq!(
                ctx.resolve_static(origin.class, origin.refs[0]),
                Ok(method(&registry, base.class, "m", "()I"))
            );
            assert_eq!(
                ctx.resolve_static(origin.class, origin.refs[1]),
                Ok(method(&registry, base.class, "m", "()I"))
            );
        } else {
            assert_eq!(
                ctx.resolve_static(origin.class, origin.refs[0]),
                Err(E::Java(J::IllegalAccess))
            );
            assert_eq!(
                ctx.resolve_static(origin.class, origin.refs[1]),
                Err(E::Java(J::IncompatibleClassChange))
            );
        }
        assert_eq!(
            ctx.resolve_static(other.class, other.refs[0]),
            Err(E::Java(J::IllegalAccess))
        );
    }
}

#[test]
fn cp_kind_version_array_special_names_and_owner_kind_have_distinct_ordered_failures() {
    for major in [45, 51, 52, 65] {
        let mut registry = registry();
        let l = registry.create_loader().unwrap();
        object(&mut registry, l);
        define(
            &mut registry,
            l,
            "I",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[("m", "()V", 9)],
            &[],
        );
        let own = define_version(
            &mut registry,
            l,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", "()V", 9)],
            &[
                ("I", "m", "()V", 11),
                ("I", "m", "()V", 10),
                ("C", "m", "()V", 11),
                ("[I", "m", "()V", 10),
                ("C", "<init>", "()V", 10),
            ],
            major,
        );
        let mut heap = heap(
            &registry,
            &[
                own.class,
                registry.lookup_class(l, &units("I")).unwrap().unwrap(),
            ],
        );
        let mut resolver = Resolver::default();
        let calls = resolver.calls.clone();
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        for index in [0, own.wrong, u16::MAX] {
            assert_eq!(
                ctx.resolve_static(own.class, index),
                Err(E::InvalidConstant {
                    origin: own.class,
                    index
                })
            );
        }
        if major < 52 {
            assert_eq!(
                ctx.resolve_static(own.class, own.refs[0]),
                Err(E::InvalidConstant {
                    origin: own.class,
                    index: own.refs[0]
                })
            );
            assert_eq!(
                ctx.resolve_static(own.class, own.refs[2]),
                Err(E::InvalidConstant {
                    origin: own.class,
                    index: own.refs[2]
                })
            );
        } else {
            assert_eq!(
                ctx.resolve_static(own.class, own.refs[0]).unwrap().owner(),
                registry.lookup_class(l, &units("I")).unwrap().unwrap()
            );
            assert_eq!(
                ctx.resolve_static(own.class, own.refs[2]),
                Err(E::Java(J::IncompatibleClassChange))
            );
        }
        assert_eq!(
            ctx.resolve_static(own.class, own.refs[1]),
            Err(E::Java(J::IncompatibleClassChange))
        );
        assert_eq!(
            ctx.resolve_static(own.class, own.refs[3]),
            Err(E::Context(C::UnsupportedArrayOwner))
        );
        assert_eq!(
            ctx.resolve_static(own.class, own.refs[4]),
            Err(E::UnsupportedSpecialMethod)
        );
        assert_eq!(calls.get(), 0);
    }
    assert_eq!(
        E::UnsupportedSpecialMethod.to_string(),
        "initialization methods are excluded from ordinary calls"
    );
}

#[test]
fn native_synchronized_and_signature_polymorphic_targets_keep_resolution_preparation_boundaries() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let own = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("native", "()V", 0x109), ("sync", "()V", 0x29)],
        &[("C", "native", "()V", 10), ("C", "sync", "()V", 10)],
    );
    let mh = define(
        &mut registry,
        l,
        "java/lang/invoke/MethodHandle",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("invoke", "([Ljava/lang/Object;)Ljava/lang/Object;", 0x181)],
        &[("java/lang/invoke/MethodHandle", "invoke", "(I)I", 10)],
    );
    let mut heap = heap(&registry, &[own.class, mh.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (index, name) in [(0, "native"), (1, "sync")] {
        assert_eq!(
            ctx.resolve_static(own.class, own.refs[index]),
            Ok(method(&registry, own.class, name, "()V"))
        );
    }
    assert_eq!(
        ctx.resolve_static(mh.class, mh.refs[0]),
        Err(E::UnsupportedSignaturePolymorphic {
            method: method(
                &registry,
                mh.class,
                "invoke",
                "([Ljava/lang/Object;)Ljava/lang/Object;"
            )
        })
    );
    assert_eq!(
        (
            ctx.heap().allocated_arrays(),
            ctx.heap().allocated_instances()
        ),
        (0, 0)
    );
}

fn delegated_heap(
    registry: &DefinitionRegistry,
    roots: &[ClassId],
    resolver: &mut Resolver,
) -> ManagedHeap {
    let plans: Vec<_> = roots
        .iter()
        .map(|class| {
            build_hierarchy(registry, *class, resolver, HierarchyOptions::default()).unwrap()
        })
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap
}

#[test]
fn loader_scoped_packages_private_access_and_nominal_boundary_precede_static_category() {
    for flags in [9, 1] {
        let (mut registry, u) = setup();
        let second = registry.create_loader().unwrap();
        let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
        let owner = define(
            &mut registry,
            second,
            "p/Remote",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[
                ("m", descriptor, flags),
                ("numeric", "()I", 9),
                ("package", "()I", 8),
                ("private", "()I", 10),
                ("array", "([I)[[D", 9),
            ],
            &[("p/Remote", "private", "()I", 10)],
        );
        let caller = define(
            &mut registry,
            u.bootstrap_loader,
            "p/Caller",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[
                ("p/Remote", "m", descriptor, 10),
                ("p/Remote", "numeric", "()I", 10),
                ("p/Remote", "package", "()I", 10),
                ("p/Remote", "private", "()I", 10),
                ("p/Remote", "array", "([I)[[D", 10),
            ],
        );
        let mut resolver = Resolver::default();
        resolver
            .selected
            .insert((second, units("java/lang/Object")), u.object);
        resolver
            .selected
            .insert((u.bootstrap_loader, units("p/Remote")), owner.class);
        let mut heap = delegated_heap(&registry, &[owner.class, caller.class], &mut resolver);
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[0]),
            Err(E::UnsupportedLoadingConstraint {
                origin: caller.class,
                method: method(&registry, owner.class, "m", descriptor)
            })
        );
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[1]),
            Ok(method(&registry, owner.class, "numeric", "()I"))
        );
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[2]),
            Err(E::Java(J::IllegalAccess))
        );
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[3]),
            Err(E::UnsupportedNestAccess {
                origin: caller.class,
                owner: owner.class
            })
        );
        assert_eq!(
            ctx.resolve_static(owner.class, owner.refs[0]),
            Ok(method(&registry, owner.class, "private", "()I"))
        );
        let target = ctx.resolve_static(caller.class, caller.refs[4]).unwrap();
        ctx.check_method_reference_profile(target, None).unwrap();
        ctx.check_method_arguments(target, &[Value::Reference(Reference::null())])
            .unwrap();
        ctx.check_method_return(target, Some(Value::Reference(Reference::null())))
            .unwrap();
    }
}

#[test]
fn package_static_access_requires_the_same_package_text_and_defining_loader() {
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
        &[("m", "()I", 8)],
        &[],
    );
    let same = define(
        &mut registry,
        loader,
        "p/Same",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/Owner", "m", "()I", 10)],
    );
    let other = define(
        &mut registry,
        loader,
        "q/Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/Owner", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[owner.class, same.class, other.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.resolve_static(same.class, same.refs[0]),
        Ok(method(&registry, owner.class, "m", "()I"))
    );
    assert_eq!(
        ctx.resolve_static(other.class, other.refs[0]),
        Err(E::Java(J::IllegalAccess))
    );
}

#[test]
fn foreign_symbolic_owner_loader_is_allowed_when_origin_and_actual_declarer_agree() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", descriptor, 9)],
        &[],
    );
    let child = define(
        &mut registry,
        second,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[],
        &[],
    );
    let caller = define(
        &mut registry,
        l,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("Child", "m", descriptor, 10)],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("Base")), base.class);
    resolver.selected.insert((l, units("Child")), child.class);
    let mut heap = delegated_heap(&registry, &[child.class, caller.class], &mut resolver);
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let target = ctx.resolve_static(caller.class, caller.refs[0]).unwrap();
    assert_eq!(target, method(&registry, base.class, "m", descriptor));
    ctx.check_method_reference_profile(target, None).unwrap();
    ctx.check_method_arguments(target, &[Value::Reference(Reference::null())])
        .unwrap();
}

#[test]
fn reference_signature_work_is_incremental_on_every_resolution_without_an_extra_profile_audit() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let descriptor = "(Ljava/lang/Object;)V";
    let own = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", descriptor, 9)],
        &[("C", "m", descriptor, 10)],
    );
    let mut heap = heap(&registry, &[own.class]);
    let mut resolver = SameLoaderResolver;
    let seed = context(&registry, &mut heap, &mut resolver).lookup_steps();
    let exact = descriptor.len() + 2;
    for extra in [
        0,
        1,
        5,
        descriptor.len() - 1,
        descriptor.len(),
        exact - 1,
        exact,
    ] {
        let limit = seed + extra;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let result = ctx.resolve_static(own.class, own.refs[0]);
        if extra < exact {
            assert_eq!(
                result,
                Err(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: limit + 1
                }))
            );
            assert_eq!(ctx.lookup_steps(), limit);
        } else {
            result.unwrap();
            assert_eq!(ctx.lookup_steps(), seed + exact);
        }
    }
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    for pass in 1..=2 {
        ctx.resolve_static(own.class, own.refs[0]).unwrap();
        assert_eq!(ctx.lookup_steps() - before, pass * exact);
    }
}

#[test]
fn own_inherited_and_interface_numeric_costs_are_exact_and_share_field_work() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let interface = define(
        &mut registry,
        l,
        "I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("i", "()V", 9)],
        &[],
    );
    let own = define_full(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("value", "I", 1)],
        &[("m", "()V", 9)],
        &[
            ("C", "m", "()V", 10),
            ("C", "value", "I", 9),
            ("I", "i", "()V", 11),
        ],
        65,
    );
    let child = define(
        &mut registry,
        l,
        "Child",
        Some("C"),
        &[],
        0x21,
        &[],
        &[("Child", "m", "()V", 10)],
    );
    let mut heap = heap(&registry, &[child.class, interface.class]);
    let value = heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let seed = context(&registry, &mut heap, &mut resolver).lookup_steps();
    for extra in [1, 2] {
        let limit = seed + extra;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let result = ctx.resolve_static(own.class, own.refs[0]);
        if extra == 1 {
            assert_eq!(
                result,
                Err(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: limit + 1
                }))
            );
        } else {
            result.unwrap();
        }
        assert_eq!(ctx.lookup_steps(), limit);
    }
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    ctx.resolve_static(own.class, own.refs[0]).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 2);
    assert_eq!(
        ctx.read_field(own.class, own.refs[1], value),
        Ok(Value::Int(0))
    );
    assert_eq!(ctx.lookup_steps() - before, 5);
    ctx.resolve_static(own.class, own.refs[2]).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 7);
    let before = ctx.lookup_steps();
    ctx.resolve_static(child.class, child.refs[0]).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 3);
}

#[test]
fn interface_diamond_fallback_keeps_duplicate_edge_and_self_pair_charges_before_category() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    define(
        &mut registry,
        l,
        "Top",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("m", "()I", 1)],
        &[],
    );
    for name in ["Left", "Right"] {
        define(
            &mut registry,
            l,
            name,
            Some("java/lang/Object"),
            &["Top"],
            0x601,
            &[],
            &[],
        );
    }
    let own = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &["Left", "Right"],
        0x21,
        &[],
        &[("C", "m", "()I", 10)],
    );
    let mut heap = heap(&registry, &[own.class]);
    let mut resolver = SameLoaderResolver;
    let seed = context(&registry, &mut heap, &mut resolver).lookup_steps();
    assert_eq!(seed, 13);
    for extra in [11, 12, 13] {
        let limit = seed + extra;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let result = ctx.resolve_static(own.class, own.refs[0]);
        if extra < 13 {
            assert_eq!(
                result,
                Err(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: limit + 1
                }))
            );
        } else {
            assert_eq!(result, Err(E::Java(J::IncompatibleClassChange)));
        }
        assert_eq!(ctx.lookup_steps(), limit);
    }
}

#[test]
fn owner_binding_limits_are_pre_callback_success_is_cached_and_failed_resolution_is_retryable() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let target = define(
        &mut registry,
        second,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()V", 9)],
        &[],
    );
    let caller = define(
        &mut registry,
        u.bootstrap_loader,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("Target", "m", "()V", 10),
            ("Target", "m", "()V", 10),
            ("Target", "absent", "()V", 10),
        ],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .selected
        .insert((u.bootstrap_loader, units("Target")), target.class);
    let mut heap = delegated_heap(&registry, &[target.class, caller.class], &mut resolver);
    let (symbols, units_count) = {
        let ctx = context(&registry, &mut heap, &mut resolver);
        (ctx.resolved_symbols(), ctx.symbol_units())
    };
    for options in [
        FieldResolutionOptions {
            max_symbols: symbols,
            ..FieldResolutionOptions::default()
        },
        FieldResolutionOptions {
            max_symbol_units: units_count + 5,
            ..FieldResolutionOptions::default()
        },
    ] {
        resolver.calls.set(0);
        let calls = resolver.calls.clone();
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            options,
        )
        .unwrap();
        assert!(matches!(
            ctx.resolve_static(caller.class, caller.refs[0]),
            Err(E::Context(C::ResourceLimit {
                resource: FieldResource::Symbols | FieldResource::SymbolUnits,
                ..
            }))
        ));
        assert_eq!(calls.get(), 0);
        assert_eq!(ctx.resolved_symbols(), symbols);
    }
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_symbols: symbols + 1,
            max_symbol_units: units_count + 6,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    for index in [0, 1] {
        assert_eq!(
            ctx.resolve_static(caller.class, caller.refs[index]),
            Ok(method(&registry, target.class, "m", "()V"))
        );
    }
    assert_eq!(
        ctx.resolve_static(caller.class, caller.refs[2]),
        Err(E::Java(J::NoSuchMethod))
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(ctx.resolved_symbols(), symbols + 1);
    assert_eq!(ctx.symbol_units(), units_count + 6);
}

#[test]
fn malformed_resolver_results_do_not_publish_bindings_or_become_java_failures() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let target = define(
        &mut registry,
        second,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()V", 9)],
        &[],
    );
    let caller = define(
        &mut registry,
        u.bootstrap_loader,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("Target", "m", "()V", 10)],
    );
    let (mut foreign, fu) = setup();
    let foreign_target = define(
        &mut foreign,
        fu.bootstrap_loader,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()V", 9)],
        &[],
    );
    let mut heap = heap(&registry, &[caller.class]);
    let mut resolver = Resolver::default();
    for (case, response) in [
        Err(ResolveError::new("test failure")),
        Ok(None),
        Ok(Some(caller.class)),
        Ok(Some(foreign_target.class)),
        Ok(Some(target.class)),
    ]
    .into_iter()
    .enumerate()
    {
        resolver.response = Some(response);
        resolver.calls.set(0);
        let calls = resolver.calls.clone();
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let symbols = ctx.resolved_symbols();
        let before = ctx.lookup_steps();
        for _ in 0..2 {
            let error = ctx
                .resolve_static(caller.class, caller.refs[0])
                .unwrap_err();
            match case {
                0 => assert!(
                    matches!(error,E::Context(C::Resolve {origin,error,..}) if origin==caller.class &&error.message=="test failure")
                ),
                1 => assert!(
                    matches!(error,E::Context(C::MissingDefinition {origin,..}) if origin==caller.class)
                ),
                2 => assert!(
                    matches!(error,E::Context(C::NameMismatch {returned,..}) if returned==caller.class)
                ),
                3 => assert!(matches!(
                    error,
                    E::Context(C::Registry(
                        crabspot_runtime::RegistryError::InvalidHandle {
                            kind: crabspot_runtime::HandleKind::Class,
                            ..
                        }
                    ))
                )),
                _ => assert_eq!(
                    error,
                    E::Context(C::Instance(InstanceError::LayoutMissing {
                        class: target.class
                    }))
                ),
            }
            assert_eq!(ctx.resolved_symbols(), symbols);
            assert_eq!(ctx.lookup_steps(), before);
        }
        assert_eq!(calls.get(), 2);
    }
}

#[test]
fn object_fallback_does_not_inherit_nonpublic_or_static_object_methods() {
    for flags in [4, 9] {
        let mut registry = registry();
        let l = registry.create_loader().unwrap();
        let object = define(
            &mut registry,
            l,
            "java/lang/Object",
            None,
            &[],
            0x21,
            &[("m", "()I", flags)],
            &[],
        );
        let interface = define(
            &mut registry,
            l,
            "I",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[],
            &[],
        );
        let own = define(
            &mut registry,
            l,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[("I", "m", "()I", 11), ("java/lang/Object", "m", "()I", 10)],
        );
        let mut heap = heap(&registry, &[own.class, interface.class]);
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.resolve_static(own.class, own.refs[0]),
            Err(E::Java(J::NoSuchMethod))
        );
        if flags == 9 {
            assert_eq!(
                ctx.resolve_static(own.class, own.refs[1]),
                Ok(method(&registry, object.class, "m", "()I"))
            );
        }
    }
}

#[test]
fn exact_utf16_names_do_not_normalize_display_spellings_or_constant_pool_indices() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let target = define(
        &mut registry,
        l,
        "p/Ω",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("é", "()I", 9), ("e\u{301}", "()I", 9), ("😀", "()I", 9)],
        &[],
    );
    let caller = define(
        &mut registry,
        l,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("p/Ω", "é", "()I", 10),
            ("p/Ω", "e\u{301}", "()I", 10),
            ("p/Ω", "😀", "()I", 10),
        ],
    );
    let mut heap = heap(&registry, &[target.class, caller.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let mut resolved = Vec::new();
    for (index, name) in ["é", "e\u{301}", "😀"].iter().enumerate() {
        let id = ctx
            .resolve_static(caller.class, caller.refs[index])
            .unwrap();
        assert_eq!(id, method(&registry, target.class, name, "()I"));
        resolved.push(id);
    }
    assert_ne!(resolved[0], resolved[1]);
    assert_ne!(resolved[1], resolved[2]);
}

#[test]
fn foreign_unadmitted_grown_replaced_and_rebound_contexts_fail_before_symbol_work() {
    let (mut registry, u) = setup();
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()V", 9)],
        &[("C", "m", "()V", 10)],
    );
    let extra = define(
        &mut registry,
        u.bootstrap_loader,
        "Extra",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let (mut foreign, fu) = setup();
    let foreign_class = define(
        &mut foreign,
        fu.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let mut heap = heap(&registry, &[own.class, u.cloneable, u.serializable]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    assert!(matches!(
        ctx.resolve_static(foreign_class.class, u16::MAX),
        Err(E::Context(C::Registry(_)))
    ));
    assert_eq!(
        ctx.resolve_static(extra.class, u16::MAX),
        Err(E::Context(C::Instance(InstanceError::LayoutMissing {
            class: extra.class
        })))
    );
    assert_eq!(ctx.lookup_steps(), before);
    ctx.heap_mut().bind_reference_types(&registry, u).unwrap();
    assert_eq!(
        ctx.resolve_static(own.class, own.refs[0]),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(ctx.lookup_steps(), before);
    drop(ctx);
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert!(ctx.resolve_static(own.class, own.refs[0]).is_ok());
    let extra_plan = plan(&registry, extra.class);
    ctx.heap_mut()
        .admit_instance_layouts(&registry, &[&extra_plan])
        .unwrap();
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.resolve_static(own.class, own.refs[0]),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(ctx.lookup_steps(), before);
    drop(ctx);
    let replacement = bound_heap(&registry, u, &[own.class, extra.class]);
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let old = std::mem::replace(ctx.heap_mut(), replacement);
    assert_eq!(
        old.retained_layout_classes(),
        ctx.heap().retained_layout_classes()
    );
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.resolve_static(own.class, own.refs[0]),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(ctx.lookup_steps(), before);
}

#[test]
fn inherited_static_lookup_and_metadata_drop_remain_iterative_at_ten_thousand_classes() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 2,
        ..RegistryOptions::default()
    })
    .unwrap();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", "()V", 9)],
        &[],
    );
    let mut previous = String::from("Base");
    let mut last = None;
    for index in 0..DEPTH {
        let name = format!("C{index}");
        let refs = if index + 1 == DEPTH {
            vec![(name.as_str(), "m", "()V", 10)]
        } else {
            Vec::new()
        };
        let own = define(
            &mut registry,
            l,
            &name,
            Some(&previous),
            &[],
            0x21,
            &[],
            &refs,
        );
        previous = name;
        last = Some(own);
    }
    let last = last.unwrap();
    let plan = build_hierarchy(
        &registry,
        last.class,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 2,
            max_edges: DEPTH + 1,
            max_declared_fields: 0,
            max_instance_slots: 0,
        },
    )
    .unwrap();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 2,
            max_layout_edges: DEPTH + 1,
            max_layout_fields: 0,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.resolve_static(last.class, last.refs[0]),
        Ok(method(&registry, base.class, "m", "()V"))
    );
    assert_eq!(ctx.lookup_steps() - before, DEPTH + 2);
    drop(ctx);
    drop(heap);
    drop(plan);
    drop(registry);
}

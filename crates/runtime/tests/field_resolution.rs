//! Resolution/access tests for the explicitly selected unnamed-module profile.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError as E, FieldJavaError as J,
    FieldResolutionContext, FieldResolutionOptions, FieldResource as R, HeapEntryKind, HeapError,
    HeapOptions, HierarchyOptions, HierarchyPlan, HierarchyResolver, InstanceError,
    InstanceOptions, LoaderId, ManagedHeap, PrimitiveArrayKind, Reference, RegistryOptions,
    ResolveError, SameLoaderResolver, Value, ValueKind, build_hierarchy,
};
use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

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
        let mut bytes = words(&[encoded.len() as u16]);
        bytes.extend(encoded);
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
    wrong_constant: u16,
}
fn registry() -> DefinitionRegistry {
    DefinitionRegistry::new(RegistryOptions::default()).unwrap()
}
// The fixture keeps each independent class-file property explicit.
#[allow(clippy::too_many_arguments)]
fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[(&str, &str, u16)],
    refs: &[(&str, &str, &str)],
) -> Def {
    let mut pool = Pool::default();
    let this = pool.class(name);
    let parent = parent.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let fields: Vec<_> = fields
        .iter()
        .map(|(name, desc, flags)| (pool.text(name), pool.text(desc), *flags))
        .collect();
    let mut indices = Vec::new();
    for (owner, name, desc) in refs {
        let owner = pool.class(owner);
        let name = pool.text(name);
        let desc = pool.text(desc);
        let nat = pool.add(12, &words(&[name, desc]));
        indices.push(pool.add(9, &words(&[owner, nat])));
    }
    let wrong_constant = pool.text("not a Fieldref");
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, this, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[fields.len() as u16]));
    for (name, desc, flags) in fields {
        bytes.extend(words(&[flags, name, desc, 0]));
    }
    bytes.extend(words(&[0, 0]));
    Def {
        class: registry
            .define(loader, &units(name), &bytes, ParseOptions::default())
            .unwrap(),
        refs: indices,
        wrong_constant,
    }
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
fn field(
    registry: &DefinitionRegistry,
    class: ClassId,
    name: &str,
    desc: &str,
) -> crabspot_runtime::FieldId {
    registry
        .find_declared_field(class, &units(name), &units(desc))
        .unwrap()
        .unwrap()
}
#[derive(Default)]
struct Resolver {
    selected: HashMap<(LoaderId, Vec<u16>), ClassId>,
    responses: VecDeque<Result<Option<ClassId>, ResolveError>>,
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
        if let Some(response) = self.responses.pop_front() {
            return response;
        }
        if let Some(class) = self.selected.get(&(loader, name.to_vec())) {
            return Ok(Some(*class));
        }
        registry
            .lookup_class(loader, name)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

#[test]
fn own_declarations_match_name_and_descriptor_and_read_write_through_alias_symbols() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let origin = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("same", "I", 1), ("same", "J", 1)],
        &[
            ("p/Owner", "same", "I"),
            ("p/Owner", "same", "J"),
            ("p/Owner", "same", "I"),
            ("p/Owner", "same", "D"),
        ],
    );
    let mut heap = heap(&registry, &[origin.class]);
    let receiver = heap.allocate_instance(origin.class).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let symbols = context.resolved_symbols();
    let units = context.symbol_units();
    context
        .write_field(origin.class, origin.refs[0], receiver, Value::Int(17))
        .unwrap();
    context
        .write_field(origin.class, origin.refs[1], receiver, Value::Long(73))
        .unwrap();
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[2], receiver)
            .unwrap(),
        Value::Int(17)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[1], receiver)
            .unwrap(),
        Value::Long(73)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[3], Reference::null())
            .unwrap_err(),
        E::Java(J::NoSuchField)
    );
    assert_eq!(context.resolved_symbols(), symbols);
    assert_eq!(context.symbol_units(), units);
    assert_eq!(calls.get(), 0);
}

#[test]
fn interface_depth_first_precedence_beats_superclass_and_diamonds_are_visited_once() {
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
        &[("x", "I", 1)],
        &[],
    );
    define(
        &mut registry,
        loader,
        "p/K",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("x", "I", 0x19)],
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
        Some("p/Base"),
        &["p/I", "p/J"],
        0x21,
        &[],
        &[("p/Root", "x", "I"), ("p/Root", "missing", "I")],
    );
    let mut heap = heap(&registry, &[root.class]);
    let receiver = heap.allocate_instance(root.class).unwrap();
    heap.put_field(
        receiver,
        field(&registry, base.class, "x", "I"),
        Value::Int(17),
    )
    .unwrap();
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
            .read_field(root.class, root.refs[0], Reference::null())
            .unwrap_err(),
        E::Java(J::IncompatibleClassChange)
    );
    assert_eq!(
        context
            .read_field(root.class, root.refs[0], receiver)
            .unwrap_err(),
        E::Java(J::IncompatibleClassChange)
    );
    let before = context.lookup_steps();
    assert_eq!(
        context
            .read_field(root.class, root.refs[1], receiver)
            .unwrap_err(),
        E::Java(J::NoSuchField)
    );
    assert_eq!(context.lookup_steps() - before, 8); // six nodes plus the two x declarations; K appears once
}

#[test]
fn own_field_wins_over_interface_and_interface_object_dependency_is_not_inherited() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("objectField", "I", 1)],
        &[],
    );
    let interface = define(
        &mut registry,
        loader,
        "p/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("x", "I", 0x19)],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &["p/I"],
        0x21,
        &[("x", "I", 1)],
        &[("p/Owner", "x", "I"), ("p/I", "objectField", "I")],
    );
    let mut heap = heap(&registry, &[origin.class]);
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
            .read_field(origin.class, origin.refs[0], receiver)
            .unwrap(),
        Value::Int(0)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[1], Reference::null())
            .unwrap_err(),
        E::Java(J::NoSuchField)
    );
    assert_eq!(
        context.heap().instance_class(receiver).unwrap(),
        origin.class
    );
    assert!(context.registry().class(interface.class).is_ok());
}

#[test]
fn resolution_staticness_finality_and_access_failures_precede_null() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "owner/Fields",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("static", "I", 9),
            ("final", "I", 0x11),
            ("package", "I", 0),
            ("normal", "I", 1),
        ],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "other/Origin",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("owner/Fields", "missing", "I"),
            ("owner/Fields", "static", "I"),
            ("owner/Fields", "final", "I"),
            ("owner/Fields", "package", "I"),
            ("owner/Fields", "normal", "I"),
        ],
    );
    let mut heap = heap(&registry, &[origin.class, owner.class]);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for (index, expected) in [
        (0, J::NoSuchField),
        (1, J::IncompatibleClassChange),
        (3, J::IllegalAccess),
        (4, J::NullPointer),
    ] {
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[index], Reference::null())
                .unwrap_err(),
            E::Java(expected)
        );
        assert_eq!(
            context
                .write_field(
                    origin.class,
                    origin.refs[index],
                    Reference::null(),
                    Value::Int(0)
                )
                .unwrap_err(),
            E::Java(expected)
        );
    }
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[2], Reference::null())
            .unwrap_err(),
        E::Java(J::NullPointer)
    );
    assert_eq!(
        context
            .write_field(
                origin.class,
                origin.refs[2],
                Reference::null(),
                Value::Int(0)
            )
            .unwrap_err(),
        E::Java(J::IllegalAccess)
    );
}

#[test]
fn owner_access_is_checked_first_but_inaccessible_declarers_do_not_hide_public_inherited_fields() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let hidden = define(
        &mut registry,
        loader,
        "hidden/Base",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[("x", "I", 1)],
        &[],
    );
    let public = define(
        &mut registry,
        loader,
        "hidden/Public",
        Some("hidden/Base"),
        &[],
        0x21,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "other/Origin",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("hidden/Base", "missing", "I"), ("hidden/Public", "x", "I")],
    );
    let mut heap = heap(&registry, &[origin.class, public.class]);
    let receiver = heap.allocate_instance(public.class).unwrap();
    heap.put_field(
        receiver,
        field(&registry, hidden.class, "x", "I"),
        Value::Int(17),
    )
    .unwrap();
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
            .read_field(origin.class, origin.refs[0], Reference::null())
            .unwrap_err(),
        E::Java(J::IllegalAccess)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[1], receiver)
            .unwrap(),
        Value::Int(17)
    );
}

#[test]
fn private_access_is_same_class_only_and_package_access_uses_exact_package_text() {
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
        &[("private", "I", 2), ("package", "I", 0)],
        &[("p/Owner", "private", "I")],
    );
    let peer = define(
        &mut registry,
        loader,
        "p/Peer",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/Owner", "private", "I"), ("p/Owner", "package", "I")],
    );
    let other = define(
        &mut registry,
        loader,
        "p2/Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/Owner", "package", "I")],
    );
    let mut heap = heap(&registry, &[owner.class, peer.class, other.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    context
        .write_field(owner.class, owner.refs[0], receiver, Value::Int(17))
        .unwrap();
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[0], receiver)
            .unwrap(),
        Value::Int(17)
    );
    assert_eq!(
        context
            .read_field(peer.class, peer.refs[0], Reference::null())
            .unwrap_err(),
        E::UnsupportedNestAccess {
            origin: peer.class,
            owner: owner.class
        }
    );
    assert_eq!(
        context
            .read_field(peer.class, peer.refs[1], receiver)
            .unwrap(),
        Value::Int(0)
    );
    assert_eq!(
        context
            .read_field(other.class, other.refs[0], receiver)
            .unwrap_err(),
        E::Java(J::IllegalAccess)
    );
}

#[test]
fn protected_nonstatic_symbolic_owner_and_actual_receiver_restrictions_are_separate() {
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
        &[("x", "I", 4)],
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
            ("base/E", "x", "I"),
            ("origin/D", "x", "I"),
            ("third/C", "x", "I"),
        ],
    );
    let child = define(
        &mut registry,
        loader,
        "origin/Child",
        Some("origin/D"),
        &[],
        0x21,
        &[],
        &[],
    );
    let mut heap = heap(
        &registry,
        &[base.class, sibling.class, origin.class, child.class],
    );
    let base_receiver = heap.allocate_instance(base.class).unwrap();
    let origin_receiver = heap.allocate_instance(origin.class).unwrap();
    let child_receiver = heap.allocate_instance(child.class).unwrap();
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
            .read_field(origin.class, origin.refs[0], origin_receiver)
            .unwrap(),
        Value::Int(0)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[0], child_receiver)
            .unwrap(),
        Value::Int(0)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[0], base_receiver)
            .unwrap_err(),
        E::InvalidReceiver {
            expected: origin.class,
            actual: base.class
        }
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[2], Reference::null())
            .unwrap_err(),
        E::Java(J::IllegalAccess)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[1], child_receiver)
            .unwrap(),
        Value::Int(0)
    );
}

#[test]
fn protected_static_sibling_owner_passes_access_then_reports_incompatible_class_change() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    define(
        &mut registry,
        loader,
        "base/E",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 0x0c)],
        &[],
    );
    let symbolic = define(
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
        &[("third/C", "x", "I")],
    );
    let mut heap = heap(&registry, &[symbolic.class, origin.class]);
    let receiver = heap.allocate_instance(symbolic.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for receiver in [Reference::null(), receiver] {
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[0], receiver)
                .unwrap_err(),
            E::Java(J::IncompatibleClassChange)
        );
        assert_eq!(
            context
                .write_field(origin.class, origin.refs[0], receiver, Value::Int(0))
                .unwrap_err(),
            E::Java(J::IncompatibleClassChange)
        );
    }
}

#[test]
fn nonpublic_owner_and_package_fields_require_equal_defining_loader_even_in_empty_package() {
    for prefix in ["", "same/"] {
        let mut registry = registry();
        let first = registry.create_loader().unwrap();
        let second = registry.create_loader().unwrap();
        object(&mut registry, first);
        object(&mut registry, second);
        let owner_name = format!("{prefix}Owner");
        let origin_name = format!("{prefix}Origin");
        let owner = define(
            &mut registry,
            second,
            &owner_name,
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("package", "I", 0)],
            &[],
        );
        let hidden_name = format!("{prefix}Hidden");
        let hidden = define(
            &mut registry,
            second,
            &hidden_name,
            Some("java/lang/Object"),
            &[],
            0x20,
            &[("x", "I", 1)],
            &[],
        );
        let origin = define(
            &mut registry,
            first,
            &origin_name,
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[
                (&owner_name, "package", "I"),
                (&hidden_name, "missing", "I"),
            ],
        );
        let mut heap = heap(&registry, &[origin.class, owner.class, hidden.class]);
        let receiver = heap.allocate_instance(owner.class).unwrap();
        let mut resolver = Resolver::default();
        resolver
            .selected
            .insert((first, units(&owner_name)), owner.class);
        resolver
            .selected
            .insert((first, units(&hidden_name)), hidden.class);
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[0], receiver)
                .unwrap_err(),
            E::Java(J::IllegalAccess)
        );
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[1], Reference::null())
                .unwrap_err(),
            E::Java(J::IllegalAccess)
        );
    }
}

#[test]
fn cross_loader_reference_leaves_require_explicit_unsupported_loading_constraints() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let owner = define(
        &mut registry,
        second,
        "other/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("primitive", "I", 1),
            ("primitiveArray", "[[I", 1),
            ("object", "Ljava/lang/Object;", 1),
            ("objectArray", "[[Ljava/lang/Object;", 1),
        ],
        &[("other/Owner", "object", "Ljava/lang/Object;")],
    );
    let origin = define(
        &mut registry,
        first,
        "origin/D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("other/Owner", "primitive", "I"),
            ("other/Owner", "primitiveArray", "[[I"),
            ("other/Owner", "object", "Ljava/lang/Object;"),
            ("other/Owner", "objectArray", "[[Ljava/lang/Object;"),
        ],
    );
    let mut heap = heap(&registry, &[origin.class, owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((first, units("other/Owner")), owner.class);
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[0], receiver)
            .unwrap(),
        Value::Int(0)
    );
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[1], receiver)
            .unwrap(),
        Value::Reference(Reference::null())
    );
    for index in [2, 3] {
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[index], Reference::null())
                .unwrap_err(),
            E::UnsupportedLoadingConstraint {
                origin: origin.class,
                owner: owner.class
            }
        );
    }
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[0], receiver)
            .unwrap(),
        Value::Reference(Reference::null())
    );
}

#[test]
fn symbolic_constants_and_receivers_are_checked_without_fabricated_java_errors() {
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
        &[("x", "I", 1)],
        &[("p/Owner", "x", "I"), ("[I", "x", "I")],
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
    let mut heap = heap(&registry, &[owner.class, other.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let wrong_receiver = heap.allocate_instance(other.class).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let foreign = {
        let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
        heap.allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap()
    };
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for index in [0, u16::MAX, owner.wrong_constant] {
        assert_eq!(
            context
                .read_field(owner.class, index, Reference::null())
                .unwrap_err(),
            E::InvalidConstant {
                origin: owner.class,
                index
            }
        );
    }
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[1], receiver)
            .unwrap_err(),
        E::UnsupportedArrayOwner
    );
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[0], wrong_receiver)
            .unwrap_err(),
        E::InvalidReceiver {
            expected: owner.class,
            actual: other.class
        }
    );
    assert_eq!(
        context
            .check_receiver(owner.class, Reference::null())
            .unwrap_err(),
        E::Instance(InstanceError::Heap(HeapError::NullReference))
    );
    assert_eq!(
        context.check_receiver(owner.class, array).unwrap_err(),
        E::Instance(InstanceError::Heap(HeapError::WrongEntryKind {
            expected: HeapEntryKind::Instance,
            actual: HeapEntryKind::PrimitiveArray
        }))
    );
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[0], foreign)
            .unwrap_err(),
        E::Instance(InstanceError::Heap(HeapError::ForeignHeap))
    );
    context.check_receiver(owner.class, receiver).unwrap();
}

#[test]
fn registered_but_unadmitted_origin_cannot_use_an_admitted_public_field() {
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
        &[("x", "I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "p/Unadmitted",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/Owner", "x", "I")],
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
            .read_field(origin.class, origin.refs[0], receiver)
            .unwrap_err(),
        E::Instance(InstanceError::LayoutMissing {
            class: origin.class
        })
    );
    assert_eq!(
        context
            .write_field(origin.class, origin.refs[0], receiver, Value::Int(17))
            .unwrap_err(),
        E::Instance(InstanceError::LayoutMissing {
            class: origin.class
        })
    );
}

#[test]
fn typed_storage_semantics_and_null_only_reference_stores_survive_resolution() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let descriptors = ["Z", "B", "C", "S", "I", "J", "F", "D", "Ljava/lang/Object;"];
    let names: Vec<_> = (0..descriptors.len())
        .map(|index| format!("field{index}"))
        .collect();
    let fields: Vec<_> = names
        .iter()
        .zip(descriptors)
        .map(|(name, desc)| (name.as_str(), desc, 1))
        .collect();
    let refs: Vec<_> = names
        .iter()
        .zip(descriptors)
        .map(|(name, desc)| ("p/Owner", name.as_str(), desc))
        .collect();
    let owner = define(
        &mut registry,
        loader,
        "p/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &refs,
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
    for (index, input, expected) in [
        (0, Value::Int(2), Value::Int(0)),
        (1, Value::Int(255), Value::Int(-1)),
        (2, Value::Int(-1), Value::Int(65535)),
        (3, Value::Int(32768), Value::Int(-32768)),
        (4, Value::Int(i32::MIN), Value::Int(i32::MIN)),
        (5, Value::Long(i64::MIN), Value::Long(i64::MIN)),
        (6, Value::Float(0xffa0_1234), Value::Float(0xffa0_1234)),
        (
            7,
            Value::Double(0xfff4_0000_0000_5678),
            Value::Double(0xfff4_0000_0000_5678),
        ),
        (
            8,
            Value::Reference(Reference::null()),
            Value::Reference(Reference::null()),
        ),
    ] {
        context
            .write_field(owner.class, owner.refs[index], receiver, input)
            .unwrap();
        assert_eq!(
            context
                .read_field(owner.class, owner.refs[index], receiver)
                .unwrap(),
            expected
        );
    }
    let reference_field = field(&registry, owner.class, &names[8], "Ljava/lang/Object;");
    assert_eq!(
        context
            .write_field(
                owner.class,
                owner.refs[8],
                receiver,
                Value::Reference(receiver)
            )
            .unwrap_err(),
        E::ReferenceType(Box::new(crabspot_runtime::TypeAccessError::MissingUniverse))
    );
    assert_eq!(
        context
            .heap_mut()
            .put_field(receiver, reference_field, Value::Reference(receiver)),
        Err(InstanceError::UnsupportedReferenceStore {
            field: reference_field
        })
    );
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[8], receiver)
            .unwrap(),
        Value::Reference(Reference::null())
    );
    assert_eq!(
        context
            .write_field(owner.class, owner.refs[4], receiver, Value::Long(0))
            .unwrap_err(),
        E::Instance(InstanceError::Heap(HeapError::ValueKindMismatch {
            expected: ValueKind::Int,
            actual: ValueKind::Long
        }))
    );
    assert_eq!(
        context
            .read_field(owner.class, owner.refs[4], receiver)
            .unwrap(),
        Value::Int(i32::MIN)
    );
}

#[test]
fn constructor_rejects_disabled_unbound_or_foreign_heaps() {
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    let object = object(&mut registry, loader);
    let mut disabled = ManagedHeap::new(HeapOptions::default()).unwrap();
    let mut resolver = SameLoaderResolver;
    assert!(matches!(
        FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut disabled,
            &mut resolver,
            FieldResolutionOptions::default()
        ),
        Err(E::Instance(InstanceError::Disabled))
    ));
    let mut unbound =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    assert!(matches!(
        FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut unbound,
            &mut resolver,
            FieldResolutionOptions::default()
        ),
        Err(E::InvalidState(_))
    ));
    let mut admitted = heap(&registry, &[object]);
    let foreign = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    assert!(matches!(
        FieldResolutionContext::new_unnamed_modules(
            &foreign,
            &mut admitted,
            &mut resolver,
            FieldResolutionOptions::default()
        ),
        Err(E::Instance(InstanceError::ForeignRegistry))
    ));
}

#[test]
fn successful_new_bindings_are_cached_once_and_failed_callbacks_are_retryable() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let owner = define(
        &mut registry,
        second,
        "other/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 1)],
        &[],
    );
    let wrong = define(
        &mut registry,
        second,
        "other/Wrong",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let origin = define(
        &mut registry,
        first,
        "origin/D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("other/Owner", "x", "I")],
    );
    let mut foreign_registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let foreign_loader = foreign_registry.create_loader().unwrap();
    let foreign = object(&mut foreign_registry, foreign_loader);
    let mut heap = heap(&registry, &[origin.class, owner.class, wrong.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    for (failure, expected) in [
        (Ok(None), 0),
        (Err(ResolveError::new("source policy failed")), 1),
        (Ok(Some(wrong.class)), 2),
        (Ok(Some(foreign)), 3),
    ] {
        let mut resolver = Resolver {
            responses: VecDeque::from([failure, Ok(Some(owner.class)), Ok(Some(wrong.class))]),
            ..Resolver::default()
        };
        let calls = resolver.calls.clone();
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let symbols = context.resolved_symbols();
        let names = context.symbol_units();
        let error = context
            .read_field(origin.class, origin.refs[0], receiver)
            .unwrap_err();
        assert!(match error {
            E::MissingDefinition {
                origin: actual,
                name,
            } if expected == 0 => actual == origin.class && name.equals_ascii("other/Owner"),
            E::Resolve { error, .. } if expected == 1 => error.message.contains("policy"),
            E::NameMismatch { returned, .. } if expected == 2 => returned == wrong.class,
            E::Registry(_) if expected == 3 => true,
            _ => false,
        });
        assert_eq!(context.resolved_symbols(), symbols);
        assert_eq!(context.symbol_units(), names);
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[0], receiver)
                .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[0], receiver)
                .unwrap(),
            Value::Int(0)
        );
        assert_eq!(context.resolved_symbols(), symbols + 1);
        assert_eq!(context.symbol_units(), names + units("other/Owner").len());
        assert_eq!(calls.get(), 2);
    }
}

#[test]
fn resolver_returned_owner_must_have_an_admitted_layout_before_binding() {
    let mut registry = registry();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    object(&mut registry, first);
    object(&mut registry, second);
    let owner = define(
        &mut registry,
        second,
        "other/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        first,
        "origin/D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("other/Owner", "x", "I")],
    );
    let mut heap = heap(&registry, &[origin.class]);
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((first, units("other/Owner")), owner.class);
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let before = context.resolved_symbols();
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[0], Reference::null())
            .unwrap_err(),
        E::Instance(InstanceError::LayoutMissing { class: owner.class })
    );
    assert_eq!(context.resolved_symbols(), before);
}

#[test]
fn authoritative_hierarchy_alias_bindings_prevent_a_resolver_from_changing_parent_identity() {
    let mut registry = registry();
    let shared = registry.create_loader().unwrap();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    let object = object(&mut registry, shared);
    let first_parent = define(
        &mut registry,
        first,
        "Parent",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 1)],
        &[],
    );
    let second_parent = define(
        &mut registry,
        second,
        "Parent",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        shared,
        "D",
        Some("Parent"),
        &[],
        0x21,
        &[],
        &[("Parent", "x", "I"), ("Parent", "x", "I")],
    );
    let mut graph_resolver = Resolver::default();
    graph_resolver
        .selected
        .insert((shared, units("Parent")), first_parent.class);
    graph_resolver
        .selected
        .insert((first, units("java/lang/Object")), object);
    graph_resolver
        .selected
        .insert((second, units("java/lang/Object")), object);
    let first_plan = build_hierarchy(
        &registry,
        origin.class,
        &mut graph_resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let other_plan = build_hierarchy(
        &registry,
        second_parent.class,
        &mut graph_resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &[&first_plan, &other_plan])
        .unwrap();
    let receiver = heap.allocate_instance(origin.class).unwrap();
    heap.put_field(
        receiver,
        field(&registry, first_parent.class, "x", "I"),
        Value::Int(17),
    )
    .unwrap();
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((shared, units("Parent")), second_parent.class);
    let calls = resolver.calls.clone();
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    for index in origin.refs {
        assert_eq!(
            context.read_field(origin.class, index, receiver).unwrap(),
            Value::Int(17)
        );
    }
    assert_eq!(calls.get(), 0);
}

#[test]
fn contradictory_admitted_loader_name_bindings_reject_context_without_mutating_heap() {
    let mut registry = registry();
    let shared = registry.create_loader().unwrap();
    let first = registry.create_loader().unwrap();
    let second = registry.create_loader().unwrap();
    let object = object(&mut registry, shared);
    let first_parent = define(
        &mut registry,
        first,
        "Parent",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 1)],
        &[],
    );
    let second_parent = define(
        &mut registry,
        second,
        "Parent",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 1)],
        &[],
    );
    let first_root = define(
        &mut registry,
        shared,
        "D1",
        Some("Parent"),
        &[],
        0x21,
        &[],
        &[],
    );
    let second_root = define(
        &mut registry,
        shared,
        "D2",
        Some("Parent"),
        &[],
        0x21,
        &[],
        &[],
    );
    let mut plans = Vec::new();
    for (root, parent, loader) in [
        (first_root.class, first_parent.class, first),
        (second_root.class, second_parent.class, second),
    ] {
        let mut resolver = Resolver::default();
        resolver.selected.insert((shared, units("Parent")), parent);
        resolver
            .selected
            .insert((loader, units("java/lang/Object")), object);
        plans.push(
            build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap(),
        );
    }
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
        .unwrap();
    let receiver = heap.allocate_instance(first_root.class).unwrap();
    let target = field(&registry, first_parent.class, "x", "I");
    heap.put_field(receiver, target, Value::Int(73)).unwrap();
    let mut resolver = SameLoaderResolver;
    let result = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    );
    assert!(
        matches!(result, Err(E::ConflictingBinding { loader, name }) if loader == shared && name.equals_ascii("Parent"))
    );
    assert_eq!(heap.get_field(receiver, target).unwrap(), Value::Int(73));
    assert_eq!(heap.allocated_instances(), 1);
}

#[test]
fn context_detects_layout_extension_and_same_size_heap_replacement_but_allows_allocation() {
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
        &[("x", "I", 1)],
        &[("p/Owner", "x", "I")],
    );
    let extension = define(
        &mut registry,
        loader,
        "p/Extra",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let extension_plan = plan(&registry, extension.class);
    let mut heap = heap(&registry, &[owner.class]);
    let mut resolver = SameLoaderResolver;
    {
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let receiver = context.heap_mut().allocate_instance(owner.class).unwrap();
        context
            .heap_mut()
            .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
            .unwrap();
        assert_eq!(
            context
                .read_field(owner.class, owner.refs[0], receiver)
                .unwrap(),
            Value::Int(0)
        );
        context
            .heap_mut()
            .admit_instance_layouts(&registry, &[&extension_plan])
            .unwrap();
        assert_eq!(
            context
                .read_field(owner.class, owner.refs[0], receiver)
                .unwrap_err(),
            E::ContextChanged
        );
        assert_eq!(
            context.check_receiver(owner.class, receiver).unwrap_err(),
            E::ContextChanged
        );
    }
    let mut replacement = heap_for_replacement(&registry, &[owner.class, extension.class]);
    let replacement_receiver = replacement.allocate_instance(owner.class).unwrap();
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
            .read_field(owner.class, owner.refs[0], replacement_receiver)
            .unwrap_err(),
        E::ContextChanged
    );
}
fn heap_for_replacement(registry: &DefinitionRegistry, classes: &[ClassId]) -> ManagedHeap {
    heap(registry, classes)
}

#[test]
fn symbol_and_work_budgets_are_inclusive_cumulative_and_count_utf16_units() {
    let defaults = FieldResolutionOptions::default();
    assert_eq!(defaults.max_symbols, 16384);
    assert_eq!(defaults.max_symbol_units, 1_048_576);
    assert_eq!(defaults.max_lookup_steps, 1_000_000);
    let mut registry = registry();
    let loader = registry.create_loader().unwrap();
    object(&mut registry, loader);
    let owner = define(
        &mut registry,
        loader,
        "p/🦀",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 1)],
        &[],
    );
    let origin = define(
        &mut registry,
        loader,
        "p/人",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("p/🦀", "x", "I")],
    );
    let mut heap = heap(&registry, &[owner.class, origin.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let exact = FieldResolutionOptions {
        max_symbols: 3,
        max_symbol_units: 23,
        max_lookup_steps: 8,
    };
    {
        let mut context =
            FieldResolutionContext::new_unnamed_modules(&registry, &mut heap, &mut resolver, exact)
                .unwrap();
        assert_eq!(context.resolved_symbols(), 3);
        assert_eq!(context.symbol_units(), 23);
        assert_eq!(context.lookup_steps(), 5);
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[0], receiver)
                .unwrap(),
            Value::Int(0)
        );
        assert_eq!(context.lookup_steps(), 8);
        assert_eq!(
            context
                .read_field(origin.class, origin.refs[0], receiver)
                .unwrap_err(),
            E::ResourceLimit {
                resource: R::LookupSteps,
                limit: 8,
                required: 9
            }
        );
        assert_eq!(context.lookup_steps(), 8);
    }
    for (options, resource, limit, required) in [
        (
            FieldResolutionOptions {
                max_symbols: 2,
                ..exact
            },
            R::Symbols,
            2,
            3,
        ),
        (
            FieldResolutionOptions {
                max_symbol_units: 22,
                ..exact
            },
            R::SymbolUnits,
            22,
            23,
        ),
        (
            FieldResolutionOptions {
                max_lookup_steps: 4,
                ..exact
            },
            R::LookupSteps,
            4,
            5,
        ),
    ] {
        assert_eq!(
            FieldResolutionContext::new_unnamed_modules(
                &registry,
                &mut heap,
                &mut resolver,
                options
            )
            .err()
            .unwrap(),
            E::ResourceLimit {
                resource,
                limit,
                required
            }
        );
    }
    for options in [
        FieldResolutionOptions {
            max_symbols: 0,
            ..defaults
        },
        FieldResolutionOptions {
            max_symbol_units: 0,
            ..defaults
        },
        FieldResolutionOptions {
            max_lookup_steps: 0,
            ..defaults
        },
    ] {
        assert!(matches!(
            FieldResolutionContext::new_unnamed_modules(
                &registry,
                &mut heap,
                &mut resolver,
                options
            ),
            Err(E::ResourceLimit { limit: 0, .. })
        ));
    }
    let huge = FieldResolutionOptions {
        max_symbols: usize::MAX,
        max_symbol_units: usize::MAX,
        max_lookup_steps: usize::MAX,
    };
    let mut context =
        FieldResolutionContext::new_unnamed_modules(&registry, &mut heap, &mut resolver, huge)
            .unwrap();
    assert_eq!(
        context
            .read_field(origin.class, origin.refs[0], receiver)
            .unwrap(),
        Value::Int(0)
    );
}

#[test]
fn new_owner_binding_limits_fail_before_callbacks_and_do_not_charge_names() {
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
        &[("x", "I", 1)],
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
        &[("C", "x", "I")],
    );
    let mut heap = heap(&registry, &[origin.class, owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    for symbols_are_tight in [true, false] {
        let mut resolver = Resolver::default();
        resolver.selected.insert((first, units("C")), owner.class);
        let calls = resolver.calls.clone();
        let options = if symbols_are_tight {
            FieldResolutionOptions {
                max_symbols: 4,
                ..FieldResolutionOptions::default()
            }
        } else {
            FieldResolutionOptions {
                max_symbol_units: 34,
                ..FieldResolutionOptions::default()
            }
        };
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            options,
        )
        .unwrap();
        let error = context
            .read_field(origin.class, origin.refs[0], receiver)
            .unwrap_err();
        assert_eq!(calls.get(), 0);
        assert_eq!(context.resolved_symbols(), 4);
        assert_eq!(context.symbol_units(), 34);
        assert_eq!(
            error,
            if symbols_are_tight {
                E::ResourceLimit {
                    resource: R::Symbols,
                    limit: 4,
                    required: 5,
                }
            } else {
                E::ResourceLimit {
                    resource: R::SymbolUnits,
                    limit: 34,
                    required: 35,
                }
            }
        );
    }
}

#[test]
fn deep_lookup_relationship_checks_and_context_drop_are_iterative() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 1,
        ..RegistryOptions::default()
    })
    .unwrap();
    let loader = registry.create_loader().unwrap();
    let root = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[("hit", "I", 1)],
        &[],
    );
    let mut previous = String::from("java/lang/Object");
    let mut last = None;
    for index in 0..DEPTH {
        let name = format!("deep/C{index}");
        let refs = if index + 1 == DEPTH {
            vec![(name.as_str(), "hit", "I")]
        } else {
            Vec::new()
        };
        last = Some(define(
            &mut registry,
            loader,
            &name,
            Some(&previous),
            &[],
            0x21,
            &[("other", "I", 1)],
            &refs,
        ));
        previous = name;
    }
    let last = last.unwrap();
    let plan = build_hierarchy(
        &registry,
        last.class,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 1,
            max_edges: DEPTH,
            max_declared_fields: DEPTH + 1,
            max_instance_slots: DEPTH + 1,
        },
    )
    .unwrap();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 1,
            max_layout_edges: DEPTH,
            max_layout_fields: DEPTH + 1,
            max_instance_slots: DEPTH + 1,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(last.class).unwrap();
    heap.put_field(
        receiver,
        field(&registry, root.class, "hit", "I"),
        Value::Int(73),
    )
    .unwrap();
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
            .read_field(last.class, last.refs[0], receiver)
            .unwrap(),
        Value::Int(73)
    );
    context.check_receiver(root.class, receiver).unwrap();
    drop(context);
    drop(heap);
    drop(plan);
}

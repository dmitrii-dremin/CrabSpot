//! Borrowed method signature, value and conservative executed-profile boundaries.
use crabspot_classfile::{ParseOptions, descriptor::parse_method_descriptor};
use crabspot_runtime::{
    ArrayElement as A, ClassId, DefinitionRegistry, FieldAccessError as C, FieldResolutionContext,
    FieldResolutionOptions, FieldResource, HeapError, HeapOptions, HierarchyOptions, HierarchyPlan,
    HierarchyResolver, InstanceError, InstanceInvocationKind as K, InstanceOptions, LoaderId,
    ManagedHeap, MethodAccessError as E, MethodId, MethodReferenceMismatch, MethodReturnKind as RK,
    MethodValuePosition as P, MethodValueShape, PrimitiveArrayKind, Reference, ReferenceType as T,
    ReferenceTypeUniverse as U, RegistryOptions, ResolveError, SameLoaderResolver,
    TypeAccessError as TE, Value, ValueKind, build_hierarchy,
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
fn mismatch(method: MethodId, position: P, source: T, target: T) -> E {
    E::IncompatibleReference(Box::new(MethodReferenceMismatch {
        method,
        position,
        source,
        target,
    }))
}

#[test]
fn compact_shapes_cover_all_numeric_tags_mixed_widths_and_reference_kinds() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let descriptor = "(ZBCSIJFDLjava/lang/Object;[I[[Ljava/lang/Object;)Ljava/lang/Object;";
    let owner = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", descriptor, 1)],
        &[("C", "m", descriptor, 10)],
    );
    let id = method(&registry, owner.class, "m", descriptor);
    let mut heap = heap(&registry, &[owner.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let expected = MethodValueShape {
        parameters: vec![
            ValueKind::Int,
            ValueKind::Int,
            ValueKind::Int,
            ValueKind::Int,
            ValueKind::Int,
            ValueKind::Long,
            ValueKind::Float,
            ValueKind::Double,
            ValueKind::Reference,
            ValueKind::Reference,
            ValueKind::Reference,
        ],
        parameter_slots: 13,
        result: RK::Reference,
    };
    for _ in 0..2 {
        let before = ctx.lookup_steps();
        assert_eq!(ctx.method_value_shape(id), Ok(expected.clone()));
        assert_eq!(
            ctx.lookup_steps() - before,
            descriptor.encode_utf16().count()
        );
    }
    assert_eq!(
        ctx.instance_invocation_shape(owner.class, owner.refs[0], K::Virtual),
        Ok(expected)
    );
}

#[test]
fn numeric_result_shapes_and_value_checks_preserve_zero_extra_work_and_no_normalization() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let returns = [
        ("()V", RK::Void, None),
        ("()Z", RK::Boolean, Some(Value::Int(7))),
        ("()B", RK::Byte, Some(Value::Int(300))),
        ("()C", RK::Char, Some(Value::Int(-1))),
        ("()S", RK::Short, Some(Value::Int(70_000))),
        ("()I", RK::Int, Some(Value::Int(-1))),
        ("()J", RK::Long, Some(Value::Long(i64::MAX))),
        ("()F", RK::Float, Some(Value::Float(0x7fc12345))),
        ("()D", RK::Double, Some(Value::Double(0xfff8000000000001))),
    ];
    let methods: Vec<_> = returns.iter().map(|(desc, _, _)| ("m", *desc, 1)).collect();
    let owner = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &methods,
        &[],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    for (desc, result, value) in returns {
        let id = method(&registry, owner.class, "m", desc);
        assert_eq!(
            ctx.method_value_shape(id),
            Ok(MethodValueShape {
                parameters: vec![],
                parameter_slots: 0,
                result
            })
        );
        ctx.check_method_arguments(id, &[]).unwrap();
        ctx.check_method_return(id, value).unwrap();
        ctx.check_method_reference_profile(id, Some(receiver))
            .unwrap();
    }
    assert_eq!(ctx.lookup_steps(), before);
}

#[test]
fn counts_and_all_computational_kinds_precede_any_reference_handle_or_target_check() {
    let (mut registry, u) = setup();
    let descriptor = "(Ljava/lang/Missing;J[I)Ljava/lang/Missing;";
    let owner = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", descriptor, 1)],
        &[],
    );
    let id = method(&registry, owner.class, "m", descriptor);
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.check_method_reference_profile(id, Some(receiver))
        .unwrap();
    assert_eq!(
        ctx.check_method_arguments(id, &[]),
        Err(E::ArgumentCount {
            method: id,
            expected: 3,
            actual: 0
        })
    );
    assert_eq!(
        ctx.check_method_arguments(
            id,
            &[
                Value::Reference(foreign_ref),
                Value::Int(0),
                Value::Reference(Reference::null())
            ]
        ),
        Err(E::ValueKindMismatch {
            method: id,
            position: P::Argument(1),
            expected: Some(ValueKind::Long),
            actual: Some(ValueKind::Int)
        })
    );
    ctx.check_method_arguments(
        id,
        &[
            Value::Reference(Reference::null()),
            Value::Long(1),
            Value::Reference(Reference::null()),
        ],
    )
    .unwrap();
    ctx.check_method_return(id, Some(Value::Reference(Reference::null())))
        .unwrap();
    assert_eq!(calls.get(), 0);
    assert_eq!(
        ctx.check_method_arguments(
            id,
            &[
                Value::Reference(foreign_ref),
                Value::Long(0),
                Value::Reference(Reference::null())
            ]
        ),
        Err(E::Type(Box::new(TE::MissingUniverse)))
    );
}

#[test]
fn declared_argument_and_result_identities_differ_from_the_receiver_and_keep_positions() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let target = define(
        &mut registry,
        l,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let desc = "(JLTarget;)LTarget;";
    let owner = define(
        &mut registry,
        l,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", desc, 1), ("v", "()V", 1)],
        &[],
    );
    let id = method(&registry, owner.class, "m", desc);
    let void = method(&registry, owner.class, "v", "()V");
    let mut heap = bound_heap(&registry, u, &[target, owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let value = heap.allocate_instance(target).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.check_method_arguments(id, &[Value::Long(7), Value::Reference(value)])
        .unwrap();
    ctx.check_method_return(id, Some(Value::Reference(value)))
        .unwrap();
    assert_eq!(
        ctx.check_method_arguments(id, &[Value::Long(7), Value::Reference(receiver)]),
        Err(mismatch(
            id,
            P::Argument(1),
            T::Class(owner.class),
            T::Class(target)
        ))
    );
    assert_eq!(
        ctx.check_method_return(id, Some(Value::Reference(receiver))),
        Err(mismatch(
            id,
            P::Return,
            T::Class(owner.class),
            T::Class(target)
        ))
    );
    assert_eq!(
        ctx.check_method_return(id, None),
        Err(E::ValueKindMismatch {
            method: id,
            position: P::Return,
            expected: Some(ValueKind::Reference),
            actual: None
        })
    );
    assert_eq!(
        ctx.check_method_return(void, Some(Value::Reference(value))),
        Err(E::ValueKindMismatch {
            method: void,
            position: P::Return,
            expected: None,
            actual: Some(ValueKind::Reference)
        })
    );
}

#[test]
fn inaccessible_descriptor_types_resolve_by_identity_without_new_caller_access() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let hidden = define(
        &mut registry,
        l,
        "p/Hidden",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[],
        &[],
    )
    .class;
    let desc = "(Lp/Hidden;)Lp/Hidden;";
    let owner = define(
        &mut registry,
        l,
        "q/Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", desc, 1)],
        &[],
    );
    let caller = define(
        &mut registry,
        l,
        "r/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("q/Holder", "m", desc, 10)],
    );
    let mut heap = bound_heap(&registry, u, &[hidden, owner.class, caller.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let value = heap.allocate_instance(hidden).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let selected = ctx
        .select_virtual(caller.class, caller.refs[0], receiver)
        .unwrap();
    assert_eq!(selected.owner(), owner.class);
    ctx.check_method_arguments(selected, &[Value::Reference(value)])
        .unwrap();
    ctx.check_method_return(selected, Some(Value::Reference(value)))
        .unwrap();
}

fn delegated_heap(
    registry: &DefinitionRegistry,
    u: Option<U>,
    classes: &[ClassId],
    resolver: &mut Resolver,
) -> ManagedHeap {
    let mut roots = classes.to_vec();
    if let Some(u) = u {
        roots.extend([u.cloneable, u.serializable]);
    }
    let plans: Vec<_> = roots
        .into_iter()
        .map(|class| {
            build_hierarchy(registry, class, resolver, HierarchyOptions::default()).unwrap()
        })
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    if let Some(u) = u {
        heap.bind_reference_types(registry, u).unwrap();
    }
    heap
}

#[test]
fn origin_resolved_nominal_loader_mismatch_precedes_staticness_and_null() {
    for flags in [1, 9] {
        let (mut registry, u) = setup();
        let first = u.bootstrap_loader;
        let second = registry.create_loader().unwrap();
        let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
        let owner = define(
            &mut registry,
            second,
            "Remote",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", descriptor, flags)],
            &[],
        );
        let caller = define(
            &mut registry,
            first,
            "Caller",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[("Remote", "m", descriptor, 10)],
        );
        let mut resolver = Resolver::default();
        resolver
            .selected
            .insert((second, units("java/lang/Object")), u.object);
        resolver
            .selected
            .insert((first, units("Remote")), owner.class);
        let mut heap = delegated_heap(&registry, None, &[owner.class, caller.class], &mut resolver);
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.select_virtual(caller.class, caller.refs[0], Reference::null()),
            Err(E::UnsupportedLoadingConstraint {
                origin: caller.class,
                method: method(&registry, owner.class, "m", descriptor)
            })
        );
    }
}

#[test]
fn intermediate_l2_matching_public_private_and_static_declarations_fail_between_l1_endpoints() {
    for flags in [1, 2, 9] {
        let (mut registry, u) = setup();
        let first = u.bootstrap_loader;
        let second = registry.create_loader().unwrap();
        let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
        let base = define(
            &mut registry,
            first,
            "Base",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[("m", descriptor, 1)],
            &[],
        );
        let middle = define(
            &mut registry,
            second,
            "Middle",
            Some("Base"),
            &[],
            0x21,
            &[("m", descriptor, flags)],
            &[],
        );
        let leaf = define(
            &mut registry,
            first,
            "Leaf",
            Some("Middle"),
            &[],
            0x21,
            &[("m", descriptor, 1)],
            &[("Base", "m", descriptor, 10)],
        );
        let mut resolver = Resolver::default();
        resolver
            .selected
            .insert((second, units("Base")), base.class);
        resolver
            .selected
            .insert((first, units("Middle")), middle.class);
        let mut heap = delegated_heap(&registry, None, &[leaf.class], &mut resolver);
        let receiver = heap.allocate_instance(leaf.class).unwrap();
        resolver.calls.set(0);
        let calls = resolver.calls.clone();
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let selected = method(&registry, leaf.class, "m", descriptor);
        let declaration = method(&registry, middle.class, "m", descriptor);
        assert_eq!(
            ctx.select_virtual(leaf.class, leaf.refs[0], receiver),
            Err(E::UnsupportedOverrideConstraint {
                method: selected,
                declaration
            })
        );
        assert_eq!(
            ctx.check_method_reference_profile(selected, Some(receiver)),
            Err(E::UnsupportedOverrideConstraint {
                method: selected,
                declaration
            })
        );
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn interface_default_and_nonselected_private_static_matches_participate_in_profile_audit() {
    for flags in [0x401, 1, 2, 9] {
        let (mut registry, u) = setup();
        let first = u.bootstrap_loader;
        let second = registry.create_loader().unwrap();
        let descriptor = "()Ljava/lang/Object;";
        let interface = define(
            &mut registry,
            second,
            "I",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[("m", descriptor, flags)],
            &[],
        );
        let leaf = define(
            &mut registry,
            first,
            "Leaf",
            Some("java/lang/Object"),
            &["I"],
            0x21,
            &[("m", descriptor, 1)],
            &[("Leaf", "m", descriptor, 10)],
        );
        let mut resolver = Resolver::default();
        resolver
            .selected
            .insert((second, units("java/lang/Object")), u.object);
        resolver
            .selected
            .insert((first, units("I")), interface.class);
        let mut heap = delegated_heap(&registry, None, &[leaf.class], &mut resolver);
        let receiver = heap.allocate_instance(leaf.class).unwrap();
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.select_virtual(leaf.class, leaf.refs[0], receiver),
            Err(E::UnsupportedOverrideConstraint {
                method: method(&registry, leaf.class, "m", descriptor),
                declaration: method(&registry, interface.class, "m", descriptor)
            })
        );
    }
}

#[test]
fn unrelated_method_signatures_do_not_trigger_nominal_override_constraint() {
    let (mut registry, u) = setup();
    let first = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let descriptor = "()Ljava/lang/Object;";
    let interface = define(
        &mut registry,
        second,
        "I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("other", descriptor, 1), ("m", "(I)Ljava/lang/Object;", 1)],
        &[],
    );
    let leaf = define(
        &mut registry,
        first,
        "Leaf",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[("m", descriptor, 1)],
        &[("Leaf", "m", descriptor, 10)],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .selected
        .insert((first, units("I")), interface.class);
    let mut heap = delegated_heap(&registry, None, &[leaf.class], &mut resolver);
    let receiver = heap.allocate_instance(leaf.class).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.select_virtual(leaf.class, leaf.refs[0], receiver),
        Ok(method(&registry, leaf.class, "m", descriptor))
    );
}

#[test]
fn primitive_array_only_profiles_cross_loaders_without_new_receiver_traversal() {
    let (mut registry, u) = setup();
    let first = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let descriptor = "([I)[[D";
    let owner = define(
        &mut registry,
        second,
        "Remote",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("m", descriptor, 1),
            ("n", "()I", 1),
            ("s", "([I)V", 9),
            ("ns", "()V", 9),
        ],
        &[],
    );
    let caller = define(
        &mut registry,
        first,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("Remote", "m", descriptor, 10)],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .selected
        .insert((first, units("Remote")), owner.class);
    let mut heap = delegated_heap(&registry, None, &[owner.class, caller.class], &mut resolver);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let id = method(&registry, owner.class, "m", descriptor);
    assert_eq!(
        ctx.select_virtual(caller.class, caller.refs[0], receiver),
        Ok(id)
    );
    for receiver in [Some(Reference::null()), Some(foreign_ref)] {
        let before = ctx.lookup_steps();
        ctx.check_method_reference_profile(id, receiver).unwrap();
        assert_eq!(ctx.lookup_steps() - before, descriptor.len());
    }
    let before = ctx.lookup_steps();
    assert!(matches!(
        ctx.check_method_reference_profile(id, None),
        Err(E::Context(C::InvalidState(_)))
    ));
    assert_eq!(ctx.lookup_steps() - before, descriptor.len());
    let numeric = method(&registry, owner.class, "n", "()I");
    let before = ctx.lookup_steps();
    ctx.check_method_reference_profile(numeric, Some(foreign_ref))
        .unwrap();
    assert!(matches!(
        ctx.check_method_reference_profile(numeric, None),
        Err(E::Context(C::InvalidState(_)))
    ));
    assert_eq!(ctx.lookup_steps(), before);
    let numeric_static = method(&registry, owner.class, "ns", "()V");
    let before = ctx.lookup_steps();
    ctx.check_method_reference_profile(numeric_static, None)
        .unwrap();
    assert!(matches!(
        ctx.check_method_reference_profile(numeric_static, Some(foreign_ref)),
        Err(E::Context(C::InvalidState(_)))
    ));
    assert_eq!(ctx.lookup_steps(), before);
    let static_method = method(&registry, owner.class, "s", "([I)V");
    ctx.check_method_reference_profile(static_method, None)
        .unwrap();
    assert!(matches!(
        ctx.check_method_reference_profile(static_method, Some(receiver)),
        Err(E::Context(C::InvalidState(_)))
    ));
}

#[test]
fn actual_values_from_other_loaders_are_accepted_by_nominal_and_array_assignment() {
    let (mut registry, u) = setup();
    let first = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let base = define(
        &mut registry,
        first,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let child = define(
        &mut registry,
        second,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let descriptor = "(LBase;[LBase;)[Ljava/lang/Object;";
    let owner = define(
        &mut registry,
        first,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", descriptor, 1)],
        &[],
    );
    let mut resolver = Resolver::default();
    resolver.selected.insert((second, units("Base")), base);
    let mut heap = delegated_heap(&registry, Some(u), &[owner.class, child], &mut resolver);
    let value = heap.allocate_instance(child).unwrap();
    let primitive = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let child_array = ctx.allocate_reference_array(T::Class(child), 1).unwrap();
    let int2 = ctx
        .allocate_reference_array(
            T::Array {
                dimensions: 1,
                element: A::Primitive(PrimitiveArrayKind::Int),
                defining_loader: first,
            },
            0,
        )
        .unwrap();
    let id = method(&registry, owner.class, "m", descriptor);
    ctx.check_method_arguments(
        id,
        &[Value::Reference(value), Value::Reference(child_array)],
    )
    .unwrap();
    ctx.check_method_return(id, Some(Value::Reference(int2)))
        .unwrap();
    assert!(matches!(
        ctx.check_method_return(id, Some(Value::Reference(primitive))),
        Err(E::IncompatibleReference(_))
    ));
}

#[test]
fn reference_defaults_private_helpers_and_special_selection_share_the_same_declared_shape() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let desc = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let interface = define(
        &mut registry,
        l,
        "I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("m", desc, 1), ("p", desc, 2)],
        &[("I", "p", desc, 11)],
    );
    let owner = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[("C", "m", desc, 10), ("I", "m", desc, 11)],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let expected = method(&registry, interface.class, "m", desc);
    assert_eq!(
        ctx.select_virtual(owner.class, owner.refs[0], receiver),
        Ok(expected)
    );
    assert_eq!(
        ctx.select_interface(owner.class, owner.refs[1], receiver),
        Ok(expected)
    );
    assert_eq!(
        ctx.select_special(owner.class, owner.refs[1], receiver),
        Ok(expected)
    );
    let private = method(&registry, interface.class, "p", desc);
    assert_eq!(
        ctx.select_interface(interface.class, interface.refs[0], receiver),
        Ok(private)
    );
    assert_eq!(
        ctx.select_special(interface.class, interface.refs[0], receiver),
        Ok(private)
    );
    ctx.check_method_arguments(private, &[Value::Reference(Reference::null())])
        .unwrap();
    ctx.check_method_return(private, Some(Value::Reference(Reference::null())))
        .unwrap();
}

#[test]
fn extraction_shape_checks_tags_versions_special_names_but_defers_array_owner_eligibility() {
    for major in [51, 52] {
        let (mut registry, u) = setup();
        let l = u.bootstrap_loader;
        let interface = define(
            &mut registry,
            l,
            "I",
            Some("java/lang/Object"),
            &[],
            0x601,
            &[("m", "()I", 1)],
            &[],
        );
        let owner = define_version(
            &mut registry,
            l,
            "C",
            Some("java/lang/Object"),
            &["I"],
            0x21,
            &[],
            &[
                ("I", "m", "()I", 11),
                ("[I", "m", "()I", 10),
                ("C", "<init>", "(Ljava/lang/Object;)V", 10),
            ],
            major,
        );
        let mut heap = heap(&registry, &[owner.class]);
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let numeric = MethodValueShape {
            parameters: vec![],
            parameter_slots: 0,
            result: RK::Int,
        };
        assert_eq!(
            ctx.instance_invocation_shape(owner.class, owner.refs[0], K::Interface),
            Ok(numeric.clone())
        );
        if major == 51 {
            assert_eq!(
                ctx.instance_invocation_shape(owner.class, owner.refs[0], K::Special),
                Err(E::InvalidConstant {
                    origin: owner.class,
                    index: owner.refs[0]
                })
            );
        } else {
            assert_eq!(
                ctx.instance_invocation_shape(owner.class, owner.refs[0], K::Special),
                Ok(numeric.clone())
            );
        }
        assert_eq!(
            ctx.instance_invocation_shape(owner.class, owner.refs[1], K::Virtual),
            Ok(numeric)
        );
        assert_eq!(
            ctx.select_virtual(owner.class, owner.refs[1], Reference::null()),
            Err(E::Context(C::UnsupportedArrayOwner))
        );
        assert_eq!(
            ctx.instance_invocation_shape(owner.class, owner.refs[2], K::Virtual),
            Err(E::UnsupportedSpecialMethod)
        );
        for index in [0, u16::MAX, owner.wrong, owner.refs[0]] {
            assert_eq!(
                ctx.instance_invocation_shape(owner.class, index, K::Virtual),
                Err(E::InvalidConstant {
                    origin: owner.class,
                    index
                })
            );
        }
        assert!(registry.class(interface.class).is_ok());
    }
}

#[test]
fn full_signature_charge_and_nominal_audit_budgets_include_duplicate_edges() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    define(
        &mut registry,
        l,
        "Top",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
    );
    define(
        &mut registry,
        l,
        "Left",
        Some("java/lang/Object"),
        &["Top"],
        0x601,
        &[],
        &[],
    );
    define(
        &mut registry,
        l,
        "Right",
        Some("java/lang/Object"),
        &["Top"],
        0x601,
        &[],
        &[],
    );
    let desc = "(Ljava/lang/Object;)V";
    let owner = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &["Left", "Right"],
        0x21,
        &[("m", desc, 1)],
        &[],
    );
    let id = method(&registry, owner.class, "m", desc);
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = SameLoaderResolver;
    for limit in [33, 34, 70, 71, 72, usize::MAX] {
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
        assert_eq!(ctx.lookup_steps(), 13);
        let result = ctx.check_method_reference_profile(id, Some(receiver));
        if limit < 72 {
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
            assert_eq!(ctx.lookup_steps(), 72);
        }
    }
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    for _ in 0..2 {
        ctx.check_method_arguments(id, &[Value::Reference(Reference::null())])
            .unwrap();
    }
    assert_eq!(ctx.lookup_steps() - before, 2 * desc.len());
}

#[test]
fn source_handle_and_pre_callback_limits_preserve_values_edges_and_successful_early_bindings() {
    let (mut registry, u) = setup();
    let first = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let target = define(
        &mut registry,
        second,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let desc = "(LTarget;LTarget;)LTarget;";
    let owner = define(
        &mut registry,
        first,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", desc, 1)],
        &[],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver.selected.insert((first, units("Target")), target);
    let mut heap = delegated_heap(&registry, Some(u), &[owner.class, target], &mut resolver);
    let value = heap.allocate_instance(target).unwrap();
    let wrong = heap.allocate_instance(owner.class).unwrap();
    let primitive = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    heap.store(primitive, 0, Value::Int(77)).unwrap();
    let retained_edges = {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let array = ctx.allocate_reference_array(T::Class(u.object), 1).unwrap();
        ctx.write_reference_array(array, 0, value).unwrap();
        array
    };
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let id = method(&registry, owner.class, "m", desc);
    let (symbols, name_units) = {
        let ctx = context(&registry, &mut heap, &mut resolver);
        (ctx.resolved_symbols(), ctx.symbol_units())
    };
    assert_eq!((symbols, name_units), (6, 83));
    for (options, expected) in [
        (
            FieldResolutionOptions {
                max_symbols: symbols,
                ..FieldResolutionOptions::default()
            },
            C::ResourceLimit {
                resource: FieldResource::Symbols,
                limit: symbols,
                required: symbols + 1,
            },
        ),
        (
            FieldResolutionOptions {
                max_symbol_units: name_units + 5,
                ..FieldResolutionOptions::default()
            },
            C::ResourceLimit {
                resource: FieldResource::SymbolUnits,
                limit: name_units + 5,
                required: name_units + 6,
            },
        ),
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
        assert_eq!(
            ctx.check_method_arguments(
                id,
                &[Value::Reference(foreign_ref), Value::Reference(value)]
            ),
            Err(E::Type(Box::new(TE::Heap(HeapError::ForeignHeap))))
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(
            ctx.check_method_arguments(id, &[Value::Reference(value), Value::Reference(value)]),
            Err(E::Type(Box::new(TE::Context(expected))))
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Int(77)));
        assert_eq!(
            ctx.heap()
                .children(retained_edges)
                .unwrap()
                .collect::<Vec<_>>(),
            [value.heap_ref().unwrap()]
        );
    }
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: 46,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        ctx.check_method_arguments(id, &[Value::Reference(value), Value::Reference(value)]),
        Err(E::Type(Box::new(TE::Context(C::ResourceLimit {
            resource: FieldResource::LookupSteps,
            limit: 46,
            required: 47
        }))))
    );
    assert_eq!(ctx.resolved_symbols(), symbols + 1);
    assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Int(77)));
    assert_eq!(
        ctx.heap()
            .children(retained_edges)
            .unwrap()
            .collect::<Vec<_>>(),
        [value.heap_ref().unwrap()]
    );
    drop(ctx);
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.check_method_arguments(id, &[Value::Reference(value), Value::Reference(wrong)]),
        Err(mismatch(
            id,
            P::Argument(1),
            T::Class(owner.class),
            T::Class(target)
        ))
    );
    assert_eq!(ctx.resolved_symbols(), symbols + 1);
    assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Int(77)));
    assert_eq!(
        ctx.heap()
            .children(retained_edges)
            .unwrap()
            .collect::<Vec<_>>(),
        [value.heap_ref().unwrap()]
    );
}

#[test]
fn long_shared_descriptor_shapes_keep_only_compact_kinds_and_charge_every_inspection() {
    let (mut registry, u) = setup();
    let descriptor = format!("(L{};)V", "X".repeat(65_530));
    assert_eq!(descriptor.len(), 65_535);
    let names: Vec<_> = (0..8).map(|index| format!("m{index}")).collect();
    let methods: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), descriptor.as_str(), 1))
        .collect();
    let owner = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &methods,
        &[],
    );
    let definition = registry.class(owner.class).unwrap();
    assert!(
        definition
            .methods
            .iter()
            .all(|method| method.descriptor_index == definition.methods[0].descriptor_index)
    );
    let mut heap = heap(&registry, &[owner.class]);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: usize::MAX,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    for name in names {
        let id = method(&registry, owner.class, &name, &descriptor);
        let before = ctx.lookup_steps();
        let shape = ctx.method_value_shape(id).unwrap();
        assert_eq!(
            shape,
            MethodValueShape {
                parameters: vec![ValueKind::Reference],
                parameter_slots: 1,
                result: RK::Void
            }
        );
        assert!(shape.parameters.capacity() <= 255);
        ctx.check_method_arguments(id, &[Value::Reference(Reference::null())])
            .unwrap();
        assert_eq!(ctx.lookup_steps() - before, 2 * descriptor.len());
    }
    assert_eq!(calls.get(), 0);
}

#[test]
fn deep_nominal_profile_audit_and_drop_use_iterative_graph_storage() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 2,
        ..RegistryOptions::default()
    })
    .unwrap();
    let l = registry.create_loader().unwrap();
    object(&mut registry, l);
    let descriptor = "()Ljava/lang/Object;";
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", descriptor, 1)],
        &[],
    )
    .class;
    let mut previous = String::from("Base");
    let mut last = base;
    for index in 0..DEPTH {
        let name = format!("C{index}");
        last = define(
            &mut registry,
            l,
            &name,
            Some(&previous),
            &[],
            0x21,
            &[],
            &[],
        )
        .class;
        previous = name;
    }
    let plan = build_hierarchy(
        &registry,
        last,
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
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let receiver = heap.allocate_instance(last).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.check_method_reference_profile(method(&registry, base, "m", descriptor), Some(receiver))
        .unwrap();
    assert_eq!(ctx.lookup_steps(), 50_050);
    drop(ctx);
    drop(heap);
    drop(plan);
    drop(registry);
}

#[test]
fn nominal_profile_receiver_option_and_handle_rules_are_separate_from_value_null_rules() {
    let (mut registry, u) = setup();
    let desc = "()Ljava/lang/Missing;";
    let owner = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", desc, 1), ("s", desc, 9)],
        &[],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let unrelated = heap.allocate_instance(u.object).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let id = method(&registry, owner.class, "m", desc);
    let static_method = method(&registry, owner.class, "s", desc);
    for (method, receiver) in [(id, None), (static_method, Some(receiver))] {
        let before = ctx.lookup_steps();
        assert!(matches!(
            ctx.check_method_reference_profile(method, receiver),
            Err(E::Context(C::InvalidState(_)))
        ));
        assert_eq!(ctx.lookup_steps() - before, desc.len());
    }
    assert_eq!(
        ctx.check_method_reference_profile(id, Some(foreign_ref)),
        Err(E::Context(C::Instance(InstanceError::Heap(
            HeapError::ForeignHeap
        ))))
    );
    assert_eq!(
        ctx.check_method_reference_profile(id, Some(unrelated)),
        Err(E::Context(C::InvalidReceiver {
            expected: owner.class,
            actual: u.object
        }))
    );
    ctx.check_method_reference_profile(id, Some(receiver))
        .unwrap();
    ctx.check_method_reference_profile(static_method, None)
        .unwrap();
    ctx.check_method_return(id, Some(Value::Reference(Reference::null())))
        .unwrap();
    assert_eq!(calls.get(), 0);
}

#[test]
fn primitive_and_reference_array_values_use_actual_component_identity_for_calls_and_returns() {
    let (mut registry, u) = setup();
    let desc = "([I)[[D";
    let owner = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", desc, 1)],
        &[],
    );
    let mut heap = bound_heap(&registry, u, &[owner.class]);
    let ints = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let booleans = heap
        .allocate_primitive_array(PrimitiveArrayKind::Boolean, 0)
        .unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let doubles = ctx
        .allocate_reference_array(
            T::Array {
                dimensions: 1,
                element: A::Primitive(PrimitiveArrayKind::Double),
                defining_loader: u.bootstrap_loader,
            },
            1,
        )
        .unwrap();
    let id = method(&registry, owner.class, "m", desc);
    ctx.check_method_arguments(id, &[Value::Reference(ints)])
        .unwrap();
    ctx.check_method_return(id, Some(Value::Reference(doubles)))
        .unwrap();
    assert_eq!(
        ctx.check_method_arguments(id, &[Value::Reference(booleans)]),
        Err(mismatch(
            id,
            P::Argument(0),
            T::Array {
                dimensions: 1,
                element: A::Primitive(PrimitiveArrayKind::Boolean),
                defining_loader: u.bootstrap_loader
            },
            T::Array {
                dimensions: 1,
                element: A::Primitive(PrimitiveArrayKind::Int),
                defining_loader: u.bootstrap_loader
            }
        ))
    );
    assert!(
        matches!(ctx.check_method_return(id,Some(Value::Reference(ints))),Err(E::IncompatibleReference(error)) if error.position==P::Return)
    );
}

#[test]
fn nulls_skip_unadmitted_targets_but_nonnull_values_check_source_then_declared_result() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let ghost = define(
        &mut registry,
        l,
        "Ghost",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let desc = "(LGhost;)LGhost;";
    let owner = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", desc, 1)],
        &[],
    );
    let mut heap = bound_heap(&registry, u, &[owner.class]);
    let value = heap.allocate_instance(owner.class).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let id = method(&registry, owner.class, "m", desc);
    ctx.check_method_arguments(id, &[Value::Reference(Reference::null())])
        .unwrap();
    ctx.check_method_return(id, Some(Value::Reference(Reference::null())))
        .unwrap();
    assert_eq!(calls.get(), 0);
    assert_eq!(
        ctx.check_method_return(id, Some(Value::Reference(foreign_ref))),
        Err(E::Type(Box::new(TE::Heap(HeapError::ForeignHeap))))
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(
        ctx.check_method_return(id, Some(Value::Reference(value))),
        Err(E::Type(Box::new(TE::Context(C::Instance(
            InstanceError::LayoutMissing { class: ghost }
        )))))
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn wide_shared_interface_audits_charge_every_edge_before_enqueue_and_every_repeat() {
    const WIDTH: usize = 128;
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    define(
        &mut registry,
        l,
        "Top",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
    );
    let branches: Vec<_> = (0..WIDTH).map(|i| format!("I{i}")).collect();
    for name in &branches {
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
    let interfaces: Vec<_> = branches.iter().map(String::as_str).collect();
    let desc = "(Ljava/lang/Object;)V";
    let owner = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &interfaces,
        0x21,
        &[("m", desc, 1)],
        &[],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let id = method(&registry, owner.class, "m", desc);
    let mut resolver = SameLoaderResolver;
    let seed = context(&registry, &mut heap, &mut resolver).lookup_steps();
    assert_eq!(seed, 4 * WIDTH + 5);
    let exact = seed + 2 * desc.len() + 4 * WIDTH + 9;
    // End halfway through the root's wide edge list, before those entries can be
    // visited. A duplicate dependency is still charged on each incoming edge.
    let during_enqueue = seed + 2 * desc.len() + 5 + WIDTH / 2;
    for limit in [during_enqueue, exact - 1, exact, usize::MAX] {
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
        let result = ctx.check_method_reference_profile(id, Some(receiver));
        if limit < exact {
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
            assert_eq!(ctx.lookup_steps(), exact);
        }
    }
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for round in 1..=2 {
        ctx.check_method_reference_profile(id, Some(receiver))
            .unwrap();
        assert_eq!(ctx.lookup_steps(), seed + round * (exact - seed));
    }
}

#[test]
fn compact_value_apis_reject_same_size_heap_replacement_before_descriptor_work() {
    let (mut registry, u) = setup();
    let desc = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let owner = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("m", desc, 1)],
        &[("C", "m", desc, 10)],
    );
    let mut first = heap(&registry, &[owner.class]);
    let second = heap(&registry, &[owner.class]);
    let receiver = first.allocate_instance(owner.class).unwrap();
    let id = method(&registry, owner.class, "m", desc);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut first, &mut resolver);
    let before = ctx.lookup_steps();
    let old = std::mem::replace(ctx.heap_mut(), second);
    assert_eq!(
        old.retained_layout_classes(),
        ctx.heap().retained_layout_classes()
    );
    assert_eq!(
        ctx.method_value_shape(id),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(
        ctx.instance_invocation_shape(owner.class, owner.refs[0], K::Virtual),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(
        ctx.check_method_reference_profile(id, Some(receiver)),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(
        ctx.check_method_arguments(id, &[Value::Reference(Reference::null())]),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(
        ctx.check_method_return(id, Some(Value::Reference(Reference::null()))),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(ctx.lookup_steps(), before);
}

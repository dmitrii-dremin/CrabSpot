//! Public checks for supplied Throwable identities, lazy catches and shared budgets.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, ExceptionAccessError as E, FieldAccessError as C,
    FieldResolutionContext, FieldResolutionOptions, FieldResource, HeapEntryKind, HeapError,
    HeapOptions, HierarchyOptions, HierarchyResolver, InstanceError, InstanceOptions, LoaderId,
    ManagedHeap, PrimitiveArrayKind, Reference, ReferenceType, ReferenceTypeUniverse as U,
    RegistryOptions, ResolveError, SameLoaderResolver, TypeAccessError as T, Value,
    VmExceptionKind, build_hierarchy,
};
use std::{cell::Cell, collections::HashMap, rc::Rc};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
#[derive(Default)]
struct Pool {
    entries: Vec<Vec<u8>>,
}
impl Pool {
    fn text(&mut self, text: &str) -> u16 {
        let mut encoded = Vec::new();
        for unit in text.encode_utf16() {
            match unit {
                1..=0x7f => encoded.push(unit as u8),
                0..=0x7ff => {
                    encoded.extend([0xc0 | ((unit >> 6) as u8), 0x80 | ((unit & 63) as u8)])
                }
                _ => encoded.extend([
                    0xe0 | ((unit >> 12) as u8),
                    0x80 | (((unit >> 6) & 63) as u8),
                    0x80 | ((unit & 63) as u8),
                ]),
            }
        }
        let mut entry = vec![1];
        entry.extend(u16::try_from(encoded.len()).unwrap().to_be_bytes());
        entry.extend(encoded);
        self.entries.push(entry);
        u16::try_from(self.entries.len()).unwrap()
    }
    fn class(&mut self, name: &str) -> u16 {
        let name = self.text(name);
        let mut entry = vec![7];
        entry.extend(name.to_be_bytes());
        self.entries.push(entry);
        u16::try_from(self.entries.len()).unwrap()
    }
}
struct Def {
    class: ClassId,
    catches: Vec<u16>,
    wrong: u16,
}
#[allow(clippy::too_many_arguments)]
fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    catches: &[&str],
) -> Def {
    let mut pool = Pool::default();
    let own = pool.class(name);
    let parent = parent.map_or(0, |name| pool.class(name));
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let catches: Vec<_> = catches.iter().map(|name| pool.class(name)).collect();
    let wrong = pool.text("not a Class entry");
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(u16::try_from(pool.entries.len() + 1).unwrap().to_be_bytes());
    for entry in pool.entries {
        bytes.extend(entry);
    }
    for word in [flags, own, parent, u16::try_from(interfaces.len()).unwrap()] {
        bytes.extend(word.to_be_bytes());
    }
    for index in interfaces {
        bytes.extend(index.to_be_bytes());
    }
    bytes.extend([0, 0, 0, 0, 0, 0]);
    Def {
        class: registry
            .define(loader, &units(name), &bytes, ParseOptions::default())
            .unwrap(),
        catches,
        wrong,
    }
}
#[derive(Default)]
struct Resolver {
    aliases: HashMap<(LoaderId, Vec<u16>), ClassId>,
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
        if let Some(class) = self.aliases.get(&(loader, name.to_vec())) {
            Ok(Some(*class))
        } else {
            registry
                .lookup_class(loader, name)
                .map_err(|error| ResolveError::new(error.to_string()))
        }
    }
}
fn registry() -> DefinitionRegistry {
    DefinitionRegistry::new(RegistryOptions::default()).unwrap()
}
fn core(registry: &mut DefinitionRegistry, loader: LoaderId) -> U {
    let object = define(registry, loader, "java/lang/Object", None, &[], 0x21, &[]).class;
    let cloneable = define(
        registry,
        loader,
        "java/lang/Cloneable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    )
    .class;
    let serializable = define(
        registry,
        loader,
        "java/io/Serializable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    )
    .class;
    U {
        bootstrap_loader: loader,
        object,
        cloneable,
        serializable,
    }
}
fn heap(
    registry: &DefinitionRegistry,
    u: U,
    roots: &[ClassId],
    resolver: &mut impl HierarchyResolver,
    bound: bool,
) -> ManagedHeap {
    let plans: Vec<_> = roots
        .iter()
        .chain([&u.cloneable, &u.serializable])
        .map(|class| {
            build_hierarchy(registry, *class, resolver, HierarchyOptions::default()).unwrap()
        })
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    if bound {
        heap.bind_reference_types(registry, u).unwrap();
    }
    heap
}
fn context<'a>(
    registry: &'a DefinitionRegistry,
    heap: &'a mut ManagedHeap,
    resolver: &'a mut impl HierarchyResolver,
) -> FieldResolutionContext<'a> {
    FieldResolutionContext::new_unnamed_modules(
        registry,
        heap,
        resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap()
}
struct Fixture {
    registry: DefinitionRegistry,
    u: U,
    throwable: ClassId,
    base: ClassId,
    child: ClassId,
    sibling: ClassId,
    origin: Def,
}
fn fixture() -> Fixture {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    let u = core(&mut registry, l);
    let throwable = define(
        &mut registry,
        l,
        "java/lang/Throwable",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Throwable"),
        &[],
        0x21,
        &[],
    )
    .class;
    let child = define(&mut registry, l, "Child", Some("Base"), &[], 0x21, &[]).class;
    let sibling = define(
        &mut registry,
        l,
        "Sibling",
        Some("java/lang/Throwable"),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define(
        &mut registry,
        l,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            "java/lang/Throwable",
            "Base",
            "Child",
            "Sibling",
            "java/lang/Object",
            "java/lang/Cloneable",
            "[I",
            "[Ljava/lang/Object;",
            "Missing",
        ],
    );
    Fixture {
        registry,
        u,
        throwable,
        base,
        child,
        sibling,
        origin,
    }
}
fn fixture_heap(f: &Fixture) -> ManagedHeap {
    heap(
        &f.registry,
        f.u,
        &[f.child, f.sibling, f.origin.class],
        &mut SameLoaderResolver,
        true,
    )
}

#[test]
fn anchor_profile_throw_and_typed_catches_have_exact_repeatable_work_and_identity() {
    let f = fixture();
    let mut heap = fixture_heap(&f);
    let value = heap.allocate_instance(f.child).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let seed = ctx.lookup_steps();
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    assert_eq!(profile.throwable(), f.throwable);
    assert_eq!(ctx.lookup_steps() - seed, 21);
    for _ in 0..2 {
        let before = ctx.lookup_steps();
        ctx.check_exception_profile(&profile).unwrap();
        assert_eq!(ctx.lookup_steps(), before);
    }
    let before = ctx.lookup_steps();
    let thrown = ctx.prepare_throw(&profile, value).unwrap();
    assert_eq!(thrown.reference(), value);
    assert_eq!(thrown.class(), f.child);
    assert_eq!(ctx.lookup_steps() - before, 3);
    for _ in 0..2 {
        let before = ctx.lookup_steps();
        assert!(
            ctx.exception_handler_matches(&profile, f.origin.class, 0, &thrown)
                .unwrap()
        );
        assert_eq!(ctx.lookup_steps(), before);
        assert!(
            ctx.exception_handler_matches(&profile, f.origin.class, f.origin.catches[1], &thrown)
                .unwrap()
        );
        assert_eq!(ctx.lookup_steps() - before, 9);
    }
    for (index, expected) in [(0, true), (2, true), (3, false)] {
        assert_eq!(
            ctx.exception_handler_matches(
                &profile,
                f.origin.class,
                f.origin.catches[index],
                &thrown
            ),
            Ok(expected)
        );
    }
    assert_eq!(calls.get(), 0);
    assert_eq!(ctx.heap().instance_class(value), Ok(f.child));
}

#[test]
fn profile_preparation_charges_name_before_comparison_and_edge_before_parent_check() {
    let f = fixture();
    let mut heap = fixture_heap(&f);
    let mut resolver = SameLoaderResolver;
    let seed = context(&f.registry, &mut heap, &mut resolver).lookup_steps();
    for allowance in [0, 1, 19, 20, 21, usize::MAX - seed] {
        let limit = seed + allowance;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let result = ctx.prepare_exception_profile(f.throwable);
        if allowance < 21 {
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
            assert_eq!(ctx.lookup_steps(), seed + 21);
        }
    }
}

#[test]
fn explicit_anchor_requires_bound_universe_registry_and_admission_before_work() {
    let f = fixture();
    let mut unbound = heap(
        &f.registry,
        f.u,
        &[f.child, f.origin.class],
        &mut SameLoaderResolver,
        false,
    );
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&f.registry, &mut unbound, &mut resolver);
    let seed = ctx.lookup_steps();
    assert_eq!(
        ctx.prepare_exception_profile(f.throwable),
        Err(E::Type(Box::new(T::MissingUniverse)))
    );
    assert_eq!(ctx.lookup_steps(), seed);
    drop(ctx);
    let mut bound = heap(&f.registry, f.u, &[f.origin.class], &mut resolver, true);
    let mut ctx = context(&f.registry, &mut bound, &mut resolver);
    let seed = ctx.lookup_steps();
    assert_eq!(
        ctx.prepare_exception_profile(f.throwable),
        Err(E::Context(C::Instance(InstanceError::LayoutMissing {
            class: f.throwable
        })))
    );
    assert_eq!(ctx.lookup_steps(), seed);
    let other = fixture();
    assert!(matches!(
        ctx.prepare_exception_profile(other.throwable),
        Err(E::Context(C::Registry(_)))
    ));
    assert_eq!(ctx.lookup_steps(), seed);
}

#[test]
fn exact_name_public_ordinary_concrete_role_and_direct_object_parent_are_required() {
    for (name, flags, parent, expected_work) in [
        ("Throwable", 0x21, "java/lang/Object", 10),
        ("java/lang/Throwable", 0x20, "java/lang/Object", 20),
        ("java/lang/Throwable", 0x421, "java/lang/Object", 20),
        ("java/lang/Throwable", 0x601, "java/lang/Object", 20),
        ("java/lang/Throwable", 0x21, "Parent", 21),
    ] {
        let mut registry = registry();
        let l = registry.create_loader().unwrap();
        let u = core(&mut registry, l);
        define(
            &mut registry,
            l,
            "Parent",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
        );
        let anchor = define(&mut registry, l, name, Some(parent), &[], flags, &[]).class;
        let mut heap = heap(&registry, u, &[anchor], &mut SameLoaderResolver, true);
        let mut resolver = Resolver::default();
        let calls = resolver.calls.clone();
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let before = ctx.lookup_steps();
        assert!(
            matches!(ctx.prepare_exception_profile(anchor),Err(E::InvalidProfile {throwable,..}) if throwable==anchor)
        );
        assert_eq!(ctx.lookup_steps() - before, expected_work);
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn same_name_throwable_from_a_different_loader_is_not_the_bootstrap_anchor() {
    let mut registry = registry();
    let l = registry.create_loader().unwrap();
    let u = core(&mut registry, l);
    let other = registry.create_loader().unwrap();
    let anchor = define(
        &mut registry,
        other,
        "java/lang/Throwable",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((other, units("java/lang/Object")), u.object);
    let mut heap = heap(&registry, u, &[anchor], &mut resolver, true);
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    assert!(matches!(
        ctx.prepare_exception_profile(anchor),
        Err(E::InvalidProfile { .. })
    ));
    assert_eq!(ctx.lookup_steps() - before, 20);
    assert_eq!(calls.get(), 0);
}

#[test]
fn supplied_throw_rejects_null_arrays_unrelated_and_foreign_before_publication() {
    let f = fixture();
    let mut heap = fixture_heap(&f);
    let unrelated = heap.allocate_instance(f.origin.class).unwrap();
    let primitive = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    heap.store(primitive, 0, Value::Int(37)).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    let array = ctx
        .allocate_reference_array(ReferenceType::Class(f.throwable), 1)
        .unwrap();
    let value = ctx.heap_mut().allocate_instance(f.child).unwrap();
    ctx.write_reference_array(array, 0, value).unwrap();
    let count = ctx.heap().allocated_arrays();
    let instances = ctx.heap().allocated_instances();
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.prepare_throw(&profile, Reference::null()),
        Err(E::NullThrowable)
    );
    assert_eq!(
        ctx.prepare_throw(&profile, foreign_ref),
        Err(E::Context(C::Instance(InstanceError::Heap(
            HeapError::ForeignHeap
        ))))
    );
    assert_eq!(ctx.lookup_steps(), before);
    for (value, kind) in [
        (primitive, HeapEntryKind::PrimitiveArray),
        (array, HeapEntryKind::ReferenceArray),
    ] {
        assert_eq!(
            ctx.prepare_throw(&profile, value),
            Err(E::Context(C::Instance(InstanceError::Heap(
                HeapError::WrongEntryKind {
                    expected: HeapEntryKind::Instance,
                    actual: kind
                }
            ))))
        );
    }
    assert_eq!(
        ctx.prepare_throw(&profile, unrelated),
        Err(E::InvalidThrownType {
            expected: f.throwable,
            actual: f.origin.class
        })
    );
    assert_eq!(ctx.heap().allocated_arrays(), count);
    assert_eq!(ctx.heap().allocated_instances(), instances);
    assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Int(37)));
    assert_eq!(
        ctx.heap().children(array).unwrap().collect::<Vec<_>>(),
        [value.heap_ref().unwrap()]
    );
}

#[test]
fn profile_and_thrown_tokens_survive_allocations_and_fresh_contexts_on_the_same_heap() {
    let f = fixture();
    let mut heap = fixture_heap(&f);
    let value = heap.allocate_instance(f.child).unwrap();
    let mut resolver = SameLoaderResolver;
    let (profile, thrown) = {
        let mut ctx = context(&f.registry, &mut heap, &mut resolver);
        let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
        let thrown = ctx.prepare_throw(&profile, value).unwrap();
        (profile, thrown)
    };
    heap.allocate_instance(f.base).unwrap();
    heap.allocate_primitive_array(PrimitiveArrayKind::Double, 0)
        .unwrap();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    ctx.check_exception_profile(&profile).unwrap();
    assert!(
        ctx.exception_handler_matches(&profile, f.origin.class, 0, &thrown)
            .unwrap()
    );
    assert_eq!(ctx.lookup_steps(), before);
    assert_eq!(thrown.reference(), value);
}

#[test]
fn foreign_profiles_and_equal_count_heap_replacement_are_detected_without_work() {
    let f = fixture();
    let mut first = fixture_heap(&f);
    let mut second = fixture_heap(&f);
    let mut resolver = SameLoaderResolver;
    let profile = context(&f.registry, &mut first, &mut resolver)
        .prepare_exception_profile(f.throwable)
        .unwrap();
    let second_profile = context(&f.registry, &mut second, &mut resolver)
        .prepare_exception_profile(f.throwable)
        .unwrap();
    let mut ctx = context(&f.registry, &mut first, &mut resolver);
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.check_exception_profile(&second_profile),
        Err(E::ProfileMismatch)
    );
    assert_eq!(
        ctx.prepare_throw(&second_profile, Reference::null()),
        Err(E::ProfileMismatch)
    );
    let old = std::mem::replace(ctx.heap_mut(), second);
    assert_eq!(
        old.retained_layout_classes(),
        ctx.heap().retained_layout_classes()
    );
    assert_eq!(
        ctx.check_exception_profile(&profile),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(ctx.lookup_steps(), before);
    drop(ctx);
    let mut ctx = context(&f.registry, &mut first, &mut resolver);
    assert_eq!(
        ctx.check_exception_profile(&profile),
        Err(E::ProfileMismatch)
    );
    ctx.check_exception_profile(&second_profile).unwrap();
    let other = fixture();
    let mut other_heap = fixture_heap(&other);
    let mut ctx = context(&other.registry, &mut other_heap, &mut resolver);
    assert_eq!(
        ctx.check_exception_profile(&profile),
        Err(E::ProfileMismatch)
    );
}

#[test]
fn new_admitted_layouts_invalidate_existing_context_and_profile_snapshots() {
    let mut f = fixture();
    let extra = define(
        &mut f.registry,
        f.u.bootstrap_loader,
        "Extra",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut heap = fixture_heap(&f);
    let mut resolver = SameLoaderResolver;
    let plan = build_hierarchy(
        &f.registry,
        extra,
        &mut resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    ctx.heap_mut()
        .admit_instance_layouts(&f.registry, &[&plan])
        .unwrap();
    assert_eq!(
        ctx.check_exception_profile(&profile),
        Err(E::Context(C::ContextChanged))
    );
    drop(ctx);
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.check_exception_profile(&profile),
        Err(E::ProfileMismatch)
    );
    ctx.prepare_exception_profile(f.throwable).unwrap();
}

#[test]
fn invalid_catch_constants_roles_and_missing_targets_are_lazy_host_failures() {
    let f = fixture();
    let mut heap = fixture_heap(&f);
    let value = heap.allocate_instance(f.child).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    let thrown = ctx.prepare_throw(&profile, value).unwrap();
    let before = ctx.lookup_steps();
    for index in [
        f.origin.wrong,
        u16::MAX,
        f.origin.catches[6],
        f.origin.catches[7],
    ] {
        assert_eq!(
            ctx.exception_handler_matches(&profile, f.origin.class, index, &thrown),
            Err(E::InvalidCatchConstant {
                origin: f.origin.class,
                index
            })
        );
    }
    assert_eq!(ctx.lookup_steps(), before);
    assert_eq!(calls.get(), 0);
    for (index, class) in [(4, f.u.object), (5, f.u.cloneable)] {
        let index = f.origin.catches[index];
        assert_eq!(
            ctx.exception_handler_matches(&profile, f.origin.class, index, &thrown),
            Err(E::InvalidCatchType {
                origin: f.origin.class,
                index,
                class
            })
        );
    }
    assert!(
        ctx.exception_handler_matches(&profile, f.origin.class, 0, &thrown)
            .unwrap()
    );
    assert_eq!(calls.get(), 0);
    assert!(
        matches!(ctx.exception_handler_matches(&profile,f.origin.class,f.origin.catches[8],&thrown),Err(E::Type(error)) if matches!(*error,T::Context(C::MissingDefinition {..})))
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn catch_all_still_requires_current_origin_admission_and_a_matching_checked_token() {
    let mut f = fixture();
    let unadmitted = define(
        &mut f.registry,
        f.u.bootstrap_loader,
        "NotAdmitted",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut first = fixture_heap(&f);
    let mut second = fixture_heap(&f);
    let value = first.allocate_instance(f.child).unwrap();
    let other_value = second.allocate_instance(f.child).unwrap();
    let mut resolver = SameLoaderResolver;
    let (profile, thrown) = {
        let mut ctx = context(&f.registry, &mut first, &mut resolver);
        let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
        (profile, ctx.prepare_throw(&profile, value).unwrap())
    };
    let other_thrown = {
        let mut ctx = context(&f.registry, &mut second, &mut resolver);
        let p = ctx.prepare_exception_profile(f.throwable).unwrap();
        ctx.prepare_throw(&p, other_value).unwrap()
    };
    let mut ctx = context(&f.registry, &mut first, &mut resolver);
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.exception_handler_matches(&profile, unadmitted, 0, &other_thrown),
        Err(E::Context(C::Instance(InstanceError::LayoutMissing {
            class: unadmitted
        })))
    );
    assert_eq!(
        ctx.exception_handler_matches(&profile, f.origin.class, 0, &other_thrown),
        Err(E::InvalidThrownToken)
    );
    let other = fixture();
    assert!(matches!(
        ctx.exception_handler_matches(&profile, other.origin.class, 0, &thrown),
        Err(E::Context(C::Registry(_)))
    ));
    assert_eq!(ctx.lookup_steps(), before);
}

#[test]
fn typed_catches_use_current_origin_loader_package_access_and_direct_java_mapping() {
    let mut f = fixture();
    let l = f.u.bootstrap_loader;
    let other = f.registry.create_loader().unwrap();
    let hidden = define(
        &mut f.registry,
        l,
        "p/Hidden",
        Some("java/lang/Throwable"),
        &[],
        0x20,
        &[],
    )
    .class;
    let same = define(
        &mut f.registry,
        l,
        "p/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["p/Hidden"],
    );
    let different_package = define(
        &mut f.registry,
        l,
        "q/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["p/Hidden"],
    );
    let different_loader = define(
        &mut f.registry,
        other,
        "p/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["p/Hidden"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((other, units("java/lang/Object")), f.u.object);
    resolver.aliases.insert((other, units("p/Hidden")), hidden);
    let mut heap = heap(
        &f.registry,
        f.u,
        &[
            hidden,
            same.class,
            different_package.class,
            different_loader.class,
        ],
        &mut resolver,
        true,
    );
    let value = heap.allocate_instance(hidden).unwrap();
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    let thrown = ctx.prepare_throw(&profile, value).unwrap();
    let before = ctx.lookup_steps();
    assert!(
        ctx.exception_handler_matches(&profile, same.class, same.catches[0], &thrown)
            .unwrap()
    );
    assert_eq!(ctx.lookup_steps() - before, "p/Hidden".len() + 4);
    for origin in [&different_package, &different_loader, &different_loader] {
        let before = ctx.lookup_steps();
        assert_eq!(
            ctx.exception_handler_matches(&profile, origin.class, origin.catches[0], &thrown),
            Err(E::Java(VmExceptionKind::IllegalAccessError))
        );
        assert_eq!(ctx.lookup_steps() - before, "p/Hidden".len() + 1);
    }
    assert_eq!(calls.get(), 1);
}

#[test]
fn delegated_throwable_subclasses_and_catches_preserve_actual_loader_qualified_identity() {
    let mut f = fixture();
    let child_loader = f.registry.create_loader().unwrap();
    let origin_loader = f.registry.create_loader().unwrap();
    let child = define(
        &mut f.registry,
        child_loader,
        "ForeignChild",
        Some("Base"),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define(
        &mut f.registry,
        origin_loader,
        "ForeignCaller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["Base"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((child_loader, units("Base")), f.base);
    resolver
        .aliases
        .insert((origin_loader, units("java/lang/Object")), f.u.object);
    resolver
        .aliases
        .insert((origin_loader, units("Base")), f.base);
    let mut heap = heap(
        &f.registry,
        f.u,
        &[child, origin.class],
        &mut resolver,
        true,
    );
    let value = heap.allocate_instance(child).unwrap();
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    let before = ctx.lookup_steps();
    let thrown = ctx.prepare_throw(&profile, value).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 3);
    assert_eq!(thrown.class(), child);
    assert_ne!(
        f.registry.defining_loader(child).unwrap(),
        f.u.bootstrap_loader
    );
    for _ in 0..2 {
        assert!(
            ctx.exception_handler_matches(&profile, origin.class, origin.catches[0], &thrown)
                .unwrap()
        );
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(thrown.reference(), value);
}

#[test]
fn typed_catch_symbol_and_unit_limits_precede_callbacks_and_late_failure_retains_binding() {
    let mut f = fixture();
    let second = f.registry.create_loader().unwrap();
    let target = define(
        &mut f.registry,
        f.u.bootstrap_loader,
        "p/Shared",
        Some("java/lang/Throwable"),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define(
        &mut f.registry,
        second,
        "OtherCaller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["p/Shared"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), f.u.object);
    resolver.aliases.insert((second, units("p/Shared")), target);
    let mut heap = heap(
        &f.registry,
        f.u,
        &[target, origin.class],
        &mut resolver,
        true,
    );
    let value = heap.allocate_instance(target).unwrap();
    let primitive = heap
        .allocate_primitive_array(PrimitiveArrayKind::Long, 1)
        .unwrap();
    heap.store(primitive, 0, Value::Long(91)).unwrap();
    let (profile, thrown, symbols, name_units, seed) = {
        let mut ctx = context(&f.registry, &mut heap, &mut resolver);
        let seed = ctx.lookup_steps();
        let symbols = ctx.resolved_symbols();
        let name_units = ctx.symbol_units();
        let p = ctx.prepare_exception_profile(f.throwable).unwrap();
        (
            p,
            ctx.prepare_throw(&p, value).unwrap(),
            symbols,
            name_units,
            seed,
        )
    };
    for (options, resource, limit, required) in [
        (
            FieldResolutionOptions {
                max_symbols: symbols,
                ..FieldResolutionOptions::default()
            },
            FieldResource::Symbols,
            symbols,
            symbols + 1,
        ),
        (
            FieldResolutionOptions {
                max_symbol_units: name_units + 7,
                ..FieldResolutionOptions::default()
            },
            FieldResource::SymbolUnits,
            name_units + 7,
            name_units + 8,
        ),
    ] {
        resolver.calls.set(0);
        let calls = resolver.calls.clone();
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut heap,
            &mut resolver,
            options,
        )
        .unwrap();
        assert_eq!(
            ctx.exception_handler_matches(&profile, origin.class, origin.catches[0], &thrown),
            Err(E::Type(Box::new(T::Context(C::ResourceLimit {
                resource,
                limit,
                required
            }))))
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(ctx.lookup_steps(), seed + 8);
        assert_eq!(ctx.resolved_symbols(), symbols);
        assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Long(91)));
    }
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let limit = seed + 8;
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &f.registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: limit,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        ctx.exception_handler_matches(&profile, origin.class, origin.catches[0], &thrown),
        Err(E::Type(Box::new(T::Context(C::ResourceLimit {
            resource: FieldResource::LookupSteps,
            limit,
            required: limit + 1
        }))))
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(ctx.resolved_symbols(), symbols + 1);
    assert_eq!(ctx.symbol_units(), name_units + 8);
    assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Long(91)));
    assert_eq!(ctx.heap().allocated_instances(), 1);
    assert_eq!(ctx.heap().allocated_arrays(), 1);
}

#[test]
fn catch_resolution_errors_are_retryable_without_publishing_bad_bindings() {
    let mut f = fixture();
    let missing = define(
        &mut f.registry,
        f.u.bootstrap_loader,
        "Missing",
        Some("java/lang/Throwable"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut heap = fixture_heap(&f);
    let value = heap.allocate_instance(f.child).unwrap();
    let other = fixture();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    let thrown = ctx.prepare_throw(&profile, value).unwrap();
    let symbols = ctx.resolved_symbols();
    drop(ctx);
    for response in [
        Err(ResolveError::new("test resolver failed")),
        Ok(None),
        Ok(Some(f.base)),
        Ok(Some(other.base)),
        Ok(Some(missing)),
    ] {
        resolver.response = Some(response.clone());
        resolver.calls.set(0);
        let mut ctx = context(&f.registry, &mut heap, &mut resolver);
        for _ in 0..2 {
            let error = ctx
                .exception_handler_matches(&profile, f.origin.class, f.origin.catches[8], &thrown)
                .unwrap_err();
            match &response {
                Err(_) => assert!(
                    matches!(error,E::Type(error) if matches!(*error,T::Context(C::Resolve {..})))
                ),
                Ok(None) => assert!(
                    matches!(error,E::Type(error) if matches!(*error,T::Context(C::MissingDefinition {..})))
                ),
                Ok(Some(class)) if *class == f.base => assert!(
                    matches!(error,E::Type(error) if matches!(*error,T::Context(C::NameMismatch {..})))
                ),
                Ok(Some(class)) if *class == other.base => assert!(
                    matches!(error,E::Type(error) if matches!(*error,T::Context(C::Registry(_))))
                ),
                _ => assert_eq!(
                    error,
                    E::Type(Box::new(T::Context(C::Instance(
                        InstanceError::LayoutMissing { class: missing }
                    ))))
                ),
            }
            assert_eq!(ctx.resolved_symbols(), symbols);
        }
        assert_eq!(calls.get(), 2);
    }
}

#[test]
fn catch_name_work_counts_utf16_units_and_repeats_for_constant_pool_aliases() {
    let mut f = fixture();
    let name = "Ω".repeat(4096);
    let target = define(
        &mut f.registry,
        f.u.bootstrap_loader,
        &name,
        Some("java/lang/Throwable"),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define(
        &mut f.registry,
        f.u.bootstrap_loader,
        "LongCaller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[&name, &name],
    );
    let mut heap = heap(
        &f.registry,
        f.u,
        &[target, origin.class],
        &mut SameLoaderResolver,
        true,
    );
    let value = heap.allocate_instance(target).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&f.registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
    let thrown = ctx.prepare_throw(&profile, value).unwrap();
    let before = ctx.lookup_steps();
    for index in &origin.catches {
        assert!(
            ctx.exception_handler_matches(&profile, origin.class, *index, &thrown)
                .unwrap()
        );
    }
    assert_eq!(ctx.lookup_steps() - before, 2 * (4096 + 4));
    assert_eq!(calls.get(), 0);
}

#[test]
fn throw_and_catch_work_limits_are_inclusive_and_cumulative_with_no_catch_all_charge() {
    let f = fixture();
    let mut heap = fixture_heap(&f);
    let value = heap.allocate_instance(f.child).unwrap();
    let mut resolver = SameLoaderResolver;
    let (profile, thrown, seed) = {
        let mut ctx = context(&f.registry, &mut heap, &mut resolver);
        let seed = ctx.lookup_steps();
        let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
        (profile, ctx.prepare_throw(&profile, value).unwrap(), seed)
    };
    for allowance in [2, 3, 5, 6] {
        let limit = seed + allowance;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let first = ctx.prepare_throw(&profile, value);
        if allowance < 3 {
            assert_eq!(
                first,
                Err(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: limit + 1
                }))
            );
        } else {
            first.unwrap();
            assert!(
                ctx.exception_handler_matches(&profile, f.origin.class, 0, &thrown)
                    .unwrap()
            );
            let next = ctx.prepare_throw(&profile, value);
            if allowance < 6 {
                assert_eq!(
                    next,
                    Err(E::Context(C::ResourceLimit {
                        resource: FieldResource::LookupSteps,
                        limit,
                        required: limit + 1
                    }))
                );
            } else {
                next.unwrap();
            }
        }
        assert_eq!(ctx.lookup_steps(), limit);
    }
    for allowance in [0, 3, 4, 5, 6, 7, 8, 9, 13, 14, 16, 17, 18] {
        let limit = seed + allowance;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        for pass in 1..=2 {
            let result = ctx.exception_handler_matches(
                &profile,
                f.origin.class,
                f.origin.catches[1],
                &thrown,
            );
            if allowance < pass * 9 {
                let failure = C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: limit + 1,
                };
                let expected = if allowance - (pass - 1) * 9 < 5 {
                    E::Type(Box::new(T::Context(failure)))
                } else {
                    E::Context(failure)
                };
                assert_eq!(result, Err(expected));
                break;
            } else {
                assert_eq!(result, Ok(true));
            }
        }
        assert_eq!(ctx.lookup_steps(), limit);
    }
}

#[test]
fn deep_throw_and_catch_ancestry_and_drop_are_iterative() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 4,
        ..RegistryOptions::default()
    })
    .unwrap();
    let l = registry.create_loader().unwrap();
    let u = core(&mut registry, l);
    let throwable = define(
        &mut registry,
        l,
        "java/lang/Throwable",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["java/lang/Throwable"],
    );
    let mut parent = String::from("java/lang/Throwable");
    let mut last = throwable.class;
    for i in 0..DEPTH {
        let name = format!("E{i}");
        last = define(&mut registry, l, &name, Some(&parent), &[], 0x21, &[]).class;
        parent = name;
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
    let marker_plans: Vec<_> = [u.cloneable, u.serializable]
        .iter()
        .map(|class| {
            build_hierarchy(
                &registry,
                *class,
                &mut SameLoaderResolver,
                HierarchyOptions::default(),
            )
            .unwrap()
        })
        .collect();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 4,
            max_layout_edges: DEPTH + 3,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    let mut plans: Vec<_> = marker_plans.iter().collect();
    plans.push(&plan);
    heap.admit_instance_layouts(&registry, &plans).unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let value = heap.allocate_instance(last).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let profile = ctx.prepare_exception_profile(throwable.class).unwrap();
    let before = ctx.lookup_steps();
    let thrown = ctx.prepare_throw(&profile, value).unwrap();
    assert_eq!(ctx.lookup_steps() - before, DEPTH + 1);
    let before = ctx.lookup_steps();
    assert!(
        ctx.exception_handler_matches(&profile, throwable.class, throwable.catches[0], &thrown)
            .unwrap()
    );
    assert_eq!(ctx.lookup_steps() - before, DEPTH + 22);
    drop(ctx);
    drop(heap);
    drop(plans);
    drop(plan);
    drop(marker_plans);
    drop(registry);
}

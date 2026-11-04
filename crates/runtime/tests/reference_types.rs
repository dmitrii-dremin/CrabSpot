//! Original classfile/type matrices independent of execution and JDK fixtures.
use crabspot_classfile::{JavaString, ParseOptions};
use crabspot_runtime::{
    ArrayElement as A, ClassId, DefinitionRegistry, FieldAccessError as C, FieldResolutionContext,
    FieldResolutionOptions, FieldResource, HeapError, HeapOptions, HierarchyOptions, HierarchyPlan,
    HierarchyResolver, InstanceError, InstanceOptions, LoaderId, ManagedHeap,
    PrimitiveArrayKind as P, Reference, ReferenceType as T, ReferenceTypeUniverse as U,
    RegistryOptions, ResolveError, SameLoaderResolver, TypeAccessError as E, TypeJavaError as J,
    Value, build_hierarchy,
};
use std::{cell::Cell, collections::HashMap, rc::Rc};
fn units(name: &str) -> Vec<u16> {
    name.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn add(&mut self, tag: u8, data: &[u8]) -> u16 {
        let mut value = vec![tag];
        value.extend(data);
        self.0.push(value);
        self.0.len() as u16
    }
    fn text(&mut self, text: &[u16]) -> u16 {
        let mut value = Vec::new();
        for &unit in text {
            match unit {
                1..=0x7f => value.push(unit as u8),
                0..=0x7ff => value.extend([0xc0 | (unit >> 6) as u8, 0x80 | (unit & 63) as u8]),
                _ => value.extend([
                    0xe0 | (unit >> 12) as u8,
                    0x80 | ((unit >> 6) & 63) as u8,
                    0x80 | (unit & 63) as u8,
                ]),
            }
        }
        let mut bytes = words(&[value.len() as u16]);
        bytes.extend(value);
        self.add(1, &bytes)
    }
    fn class(&mut self, text: &[u16]) -> u16 {
        let name = self.text(text);
        self.add(7, &words(&[name]))
    }
}
struct Def {
    class: ClassId,
    targets: Vec<u16>,
    wrong: u16,
}
fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    targets: &[&str],
) -> Def {
    define_raw(
        registry,
        loader,
        &units(name),
        parent.map(units).as_deref(),
        &interfaces
            .iter()
            .map(|name| units(name))
            .collect::<Vec<_>>(),
        flags,
        &targets.iter().map(|name| units(name)).collect::<Vec<_>>(),
    )
}
fn define_raw(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &[u16],
    parent: Option<&[u16]>,
    interfaces: &[Vec<u16>],
    flags: u16,
    targets: &[Vec<u16>],
) -> Def {
    let mut pool = Pool::default();
    let own = pool.class(name);
    let parent = parent.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let targets = targets.iter().map(|name| pool.class(name)).collect();
    let wrong = pool.text(&units("not a Class"));
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, own, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[0, 0, 0]));
    Def {
        class: registry
            .define(loader, name, &bytes, ParseOptions::default())
            .unwrap(),
        targets,
        wrong,
    }
}
fn setup() -> (DefinitionRegistry, U) {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[],
    )
    .class;
    let cloneable = define(
        &mut registry,
        loader,
        "java/lang/Cloneable",
        Some("java/lang/Object"),
        &[],
        0x601,
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
fn plan(registry: &DefinitionRegistry, class: ClassId) -> HierarchyPlan {
    build_hierarchy(
        registry,
        class,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap()
}
fn heap(registry: &DefinitionRegistry, universe: U, classes: &[ClassId]) -> ManagedHeap {
    let plans: Vec<_> = [universe.cloneable, universe.serializable]
        .into_iter()
        .chain(classes.iter().copied())
        .map(|class| plan(registry, class))
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap
}
fn array(dimensions: u8, element: A, defining_loader: LoaderId) -> T {
    T::Array {
        dimensions,
        element,
        defining_loader,
    }
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
#[derive(Default)]
struct Resolver {
    aliases: HashMap<(LoaderId, Vec<u16>), ClassId>,
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
        if let Some(class) = self.aliases.get(&(loader, name.to_vec())) {
            return Ok(Some(*class));
        }
        registry
            .lookup_class(loader, name)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

#[test]
fn universe_binding_is_explicit_idempotent_and_conflicting_bindings_are_atomic() {
    let (registry, u) = setup();
    let mut heap = heap(&registry, u, &[]);
    assert_eq!(heap.reference_type_universe(), None);
    let wrong = U {
        cloneable: u.serializable,
        ..u
    };
    assert!(matches!(
        heap.bind_reference_types(&registry, wrong),
        Err(E::InvalidUniverse(_))
    ));
    assert_eq!(heap.reference_type_universe(), None);
    heap.bind_reference_types(&registry, u).unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    assert_eq!(
        heap.bind_reference_types(&registry, wrong),
        Err(E::UniverseConflict)
    );
    assert_eq!(heap.reference_type_universe(), Some(u));
    assert_eq!(
        (heap.allocated_arrays(), heap.allocated_instances()),
        (0, 0)
    );
}

#[test]
fn binding_rejects_foreign_loader_roles_unadmitted_handles_and_disabled_storage() {
    let (mut registry, u) = setup();
    let other = registry.create_loader().unwrap();
    let missing = define(
        &mut registry,
        u.bootstrap_loader,
        "Missing",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut heap = heap(&registry, u, &[]);
    assert!(matches!(
        heap.bind_reference_types(
            &registry,
            U {
                bootstrap_loader: other,
                ..u
            }
        ),
        Err(E::InvalidUniverse(_))
    ));
    assert!(matches!(
        heap.bind_reference_types(
            &registry,
            U {
                cloneable: missing,
                ..u
            }
        ),
        Err(E::Context(C::Instance(InstanceError::LayoutMissing { .. })))
    ));
    let (foreign, foreign_u) = setup();
    assert!(matches!(
        heap.bind_reference_types(&registry, foreign_u),
        Err(E::Context(C::Registry(_)))
    ));
    assert!(matches!(
        heap.bind_reference_types(&foreign, u),
        Err(E::Context(C::Registry(_)))
    ));
    assert_eq!(heap.reference_type_universe(), None);
    let mut disabled = ManagedHeap::new(HeapOptions::default()).unwrap();
    assert_eq!(
        disabled.bind_reference_types(&registry, u),
        Err(E::Context(C::Instance(InstanceError::Disabled)))
    );
    let mut unbound =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    assert!(matches!(
        unbound.bind_reference_types(&registry, u),
        Err(E::InvalidUniverse(_))
    ));
}

#[test]
fn marker_roles_require_public_abstract_interfaces_and_no_superinterfaces() {
    for flags in [0x600, 0x21, 0x401] {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let object = define(
            &mut registry,
            loader,
            "java/lang/Object",
            None,
            &[],
            0x21,
            &[],
        )
        .class;
        let cloneable = define(
            &mut registry,
            loader,
            "java/lang/Cloneable",
            Some("java/lang/Object"),
            &[],
            flags,
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
        )
        .class;
        let u = U {
            bootstrap_loader: loader,
            object,
            cloneable,
            serializable,
        };
        let mut heap = heap(&registry, u, &[]);
        assert!(matches!(
            heap.bind_reference_types(&registry, u),
            Err(E::InvalidUniverse(_))
        ));
        assert_eq!(heap.reference_type_universe(), None);
    }
    let (mut registry, u) = setup();
    let extra = define(
        &mut registry,
        u.bootstrap_loader,
        "Extra",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    )
    .class;
    let loader = registry.create_loader().unwrap();
    // A complete second core profile whose marker has an otherwise valid superinterface.
    let object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[],
    )
    .class;
    define(
        &mut registry,
        loader,
        "Extra",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    let cloneable = define(
        &mut registry,
        loader,
        "java/lang/Cloneable",
        Some("java/lang/Object"),
        &["Extra"],
        0x601,
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
    )
    .class;
    let second = U {
        bootstrap_loader: loader,
        object,
        cloneable,
        serializable,
    };
    let mut heap = heap(&registry, second, &[]);
    assert!(matches!(
        heap.bind_reference_types(&registry, second),
        Err(E::InvalidUniverse(_))
    ));
    assert!(registry.class(extra).is_ok());
}

#[test]
fn mixed_object_roots_reject_binding_without_changing_live_arrays() {
    let (mut registry, u) = setup();
    let loader = registry.create_loader().unwrap();
    let other = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[],
    )
    .class;
    let mut heap = heap(&registry, u, &[other]);
    let reference = heap.allocate_primitive_array(P::Int, 1).unwrap();
    heap.store(reference, 0, Value::Int(77)).unwrap();
    let before = (
        heap.retained_layout_classes(),
        heap.allocated_arrays(),
        heap.payload_bytes(),
    );
    assert!(matches!(
        heap.bind_reference_types(&registry, u),
        Err(E::InvalidUniverse(_))
    ));
    assert_eq!(heap.reference_type_universe(), None);
    assert_eq!(heap.load(reference, 0).unwrap(), Value::Int(77));
    assert_eq!(
        before,
        (
            heap.retained_layout_classes(),
            heap.allocated_arrays(),
            heap.payload_bytes()
        )
    );
}

#[test]
fn post_binding_admission_preserves_atomic_batch_failure_and_allows_compatible_retry() {
    let (mut registry, u) = setup();
    let loader = registry.create_loader().unwrap();
    let bad = define(
        &mut registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[],
    )
    .class;
    let good = define(
        &mut registry,
        u.bootstrap_loader,
        "Good",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let bad_plan = plan(&registry, bad);
    let good_plan = plan(&registry, good);
    let mut heap = heap(&registry, u, &[]);
    heap.bind_reference_types(&registry, u).unwrap();
    let before = heap.retained_layout_classes();
    assert_eq!(
        heap.admit_instance_layouts(&registry, &[&good_plan, &bad_plan]),
        Err(InstanceError::ReferenceUniverseMismatch {
            class: bad,
            expected_object: u.object
        })
    );
    assert_eq!(heap.retained_layout_classes(), before);
    assert_eq!(
        heap.allocate_instance(good),
        Err(InstanceError::LayoutMissing { class: good })
    );
    heap.admit_instance_layouts(&registry, &[&good_plan])
        .unwrap();
    assert!(heap.allocate_instance(good).is_ok());
    assert_eq!(heap.reference_type_universe(), Some(u));
}

#[test]
fn binding_invalidates_old_context_but_identical_and_failed_rebinds_do_not() {
    let (registry, u) = setup();
    let mut heap = heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(ctx.reference_type(Reference::null()), Ok(None));
    ctx.heap_mut().bind_reference_types(&registry, u).unwrap();
    assert_eq!(
        ctx.reference_type(Reference::null()),
        Err(E::Context(C::ContextChanged))
    );
    drop(ctx);
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.heap_mut().bind_reference_types(&registry, u).unwrap();
    assert_eq!(
        ctx.heap_mut().bind_reference_types(
            &registry,
            U {
                cloneable: u.object,
                ..u
            }
        ),
        Err(E::UniverseConflict)
    );
    assert_eq!(ctx.reference_type(Reference::null()), Ok(None));
}

#[test]
fn arrays_allocated_before_binding_retain_contents_identity_and_independent_apis() {
    let (registry, u) = setup();
    let mut heap = heap(&registry, u, &[]);
    let kinds = [
        P::Boolean,
        P::Byte,
        P::Char,
        P::Short,
        P::Int,
        P::Long,
        P::Float,
        P::Double,
    ];
    let references: Vec<_> = kinds
        .iter()
        .map(|kind| heap.allocate_primitive_array(*kind, 1).unwrap())
        .collect();
    heap.store(references[4], 0, Value::Int(123)).unwrap();
    let before = (
        heap.allocated_arrays(),
        heap.total_elements(),
        heap.payload_bytes(),
    );
    let mut resolver = SameLoaderResolver;
    {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(ctx.reference_type(references[0]), Err(E::MissingUniverse));
        assert_eq!(ctx.reference_type(Reference::null()), Ok(None));
    }
    heap.bind_reference_types(&registry, u).unwrap();
    assert_eq!(heap.load(references[4], 0).unwrap(), Value::Int(123));
    assert_eq!(
        before,
        (
            heap.allocated_arrays(),
            heap.total_elements(),
            heap.payload_bytes()
        )
    );
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (kind, reference) in kinds.into_iter().zip(references) {
        assert_eq!(
            ctx.reference_type(reference).unwrap(),
            Some(array(1, A::Primitive(kind), u.bootstrap_loader))
        );
    }
}

#[test]
fn nominal_diamonds_targets_outside_receiver_ancestry_and_interface_object_assignment() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let top = define(
        &mut registry,
        l,
        "Top",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    )
    .class;
    define(
        &mut registry,
        l,
        "Left",
        Some("java/lang/Object"),
        &["Top"],
        0x601,
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
    );
    let root = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &["Left", "Right"],
        0x21,
        &["Top", "Other", "[I"],
    );
    let other = define(
        &mut registry,
        l,
        "Other",
        Some("java/lang/Object"),
        &[],
        0x421,
        &[],
    )
    .class;
    let mut heap = heap(&registry, u, &[root.class, other]);
    heap.bind_reference_types(&registry, u).unwrap();
    let receiver = heap.allocate_instance(root.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(ctx.reference_type(receiver), Ok(Some(T::Class(root.class))));
    for (source, target, expected) in [
        (root.class, top, true),
        (top, u.object, true),
        (top, root.class, false),
        (root.class, other, false),
        (u.object, top, false),
    ] {
        assert_eq!(
            ctx.is_assignable(T::Class(source), T::Class(target)),
            Ok(expected)
        );
    }
    assert_eq!(
        ctx.check_cast(root.class, root.targets[0], receiver),
        Ok(receiver)
    );
    assert_eq!(
        ctx.instance_of(root.class, root.targets[1], receiver),
        Ok(false)
    );
    assert_eq!(
        ctx.check_cast(root.class, root.targets[1], receiver),
        Err(E::Java(J::ClassCast))
    );
    assert_eq!(
        ctx.instance_of(root.class, root.targets[2], receiver),
        Ok(false)
    );
}

#[test]
fn every_primitive_array_kind_obeys_exact_components_and_three_nominal_supertypes() {
    let (mut registry, u) = setup();
    let other = define(
        &mut registry,
        u.bootstrap_loader,
        "Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut heap = heap(&registry, u, &[other]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let kinds = [
        P::Boolean,
        P::Byte,
        P::Char,
        P::Short,
        P::Int,
        P::Long,
        P::Float,
        P::Double,
    ];
    for source in kinds {
        let source = array(1, A::Primitive(source), u.bootstrap_loader);
        for target in kinds {
            assert_eq!(
                ctx.is_assignable(source, array(1, A::Primitive(target), u.bootstrap_loader)),
                Ok(source == array(1, A::Primitive(target), u.bootstrap_loader))
            );
        }
        for target in [u.object, u.cloneable, u.serializable] {
            assert_eq!(ctx.is_assignable(source, T::Class(target)), Ok(true));
        }
        assert_eq!(ctx.is_assignable(source, T::Class(other)), Ok(false));
        assert_eq!(ctx.is_assignable(T::Class(other), source), Ok(false));
        assert_eq!(
            ctx.is_assignable(source, array(1, A::Class(u.object), u.bootstrap_loader)),
            Ok(false)
        );
        assert_eq!(
            ctx.is_assignable(source, array(2, A::Primitive(P::Int), u.bootstrap_loader)),
            Ok(false)
        );
    }
}

#[test]
fn structural_array_covariance_reduces_unequal_ranks_through_255_without_recursion() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let interface = define(
        &mut registry,
        l,
        "I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    )
    .class;
    let class = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
    )
    .class;
    let mut heap = heap(&registry, u, &[class]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for sr in [1, 2, 3, 255] {
        for tr in [1, 2, 3, 255] {
            let ints = array(sr, A::Primitive(P::Int), l);
            let classes = array(sr, A::Class(class), l);
            assert_eq!(
                ctx.is_assignable(ints, array(tr, A::Primitive(P::Int), l)),
                Ok(sr == tr)
            );
            assert_eq!(
                ctx.is_assignable(ints, array(tr, A::Primitive(P::Boolean), l)),
                Ok(false)
            );
            assert_eq!(
                ctx.is_assignable(ints, array(tr, A::Class(u.object), l)),
                Ok(sr > tr)
            );
            assert_eq!(
                ctx.is_assignable(ints, array(tr, A::Class(u.cloneable), l)),
                Ok(sr > tr)
            );
            assert_eq!(
                ctx.is_assignable(classes, array(tr, A::Class(u.object), l)),
                Ok(sr >= tr)
            );
            assert_eq!(
                ctx.is_assignable(classes, array(tr, A::Class(interface), l)),
                Ok(sr == tr)
            );
            assert_eq!(
                ctx.is_assignable(classes, array(tr, A::Class(u.serializable), l)),
                Ok(sr > tr)
            );
        }
    }
}

#[test]
fn public_structural_values_validate_both_handles_rank_and_loader_before_identity() {
    let (mut registry, u) = setup();
    let other_loader = registry.create_loader().unwrap();
    let absent = define(
        &mut registry,
        u.bootstrap_loader,
        "Missing",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut heap = heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.is_assignable(T::Class(u.object), T::Class(u.object)),
            Err(E::MissingUniverse)
        );
    }
    heap.bind_reference_types(&registry, u).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for bad in [
        array(0, A::Primitive(P::Int), u.bootstrap_loader),
        array(1, A::Primitive(P::Int), other_loader),
        array(1, A::Class(u.object), other_loader),
    ] {
        assert!(matches!(
            ctx.is_assignable(bad, bad),
            Err(E::InvalidType(_))
        ));
    }
    assert_eq!(
        ctx.is_assignable(T::Class(absent), T::Class(absent)),
        Err(E::Context(C::Instance(InstanceError::LayoutMissing {
            class: absent
        })))
    );
    let (_, foreign) = setup();
    assert!(matches!(
        ctx.is_assignable(T::Class(u.object), T::Class(foreign.object)),
        Err(E::Context(C::Registry(_)))
    ));
    assert!(matches!(
        ctx.is_assignable(
            array(1, A::Primitive(P::Int), foreign.bootstrap_loader),
            array(1, A::Primitive(P::Int), foreign.bootstrap_loader)
        ),
        Err(E::Context(C::Registry(_)))
    ));
}

#[test]
fn null_skips_missing_inaccessible_array_leaf_targets_and_universe_but_not_cp_shape() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    define(
        &mut registry,
        l,
        "p/Hidden",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[],
    );
    let origin = define(
        &mut registry,
        l,
        "q/Origin",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["Missing", "p/Hidden", "[Lp/Hidden;", "[LMissing;", "[[I"],
    );
    let mut heap = heap(&registry, u, &[origin.class]);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = (
        ctx.lookup_steps(),
        ctx.resolved_symbols(),
        ctx.symbol_units(),
    );
    for &index in &origin.targets {
        assert_eq!(
            ctx.check_cast(origin.class, index, Reference::null()),
            Ok(Reference::null())
        );
        assert_eq!(
            ctx.instance_of(origin.class, index, Reference::null()),
            Ok(false)
        );
        assert_eq!(
            ctx.resolve_reference_type(origin.class, index),
            Err(E::MissingUniverse)
        );
    }
    assert_eq!(
        before,
        (
            ctx.lookup_steps(),
            ctx.resolved_symbols(),
            ctx.symbol_units()
        )
    );
    assert_eq!(calls.get(), 0);
    for index in [0, u16::MAX, origin.wrong] {
        assert_eq!(
            ctx.check_cast(origin.class, index, Reference::null()),
            Err(E::InvalidConstant {
                origin: origin.class,
                index
            })
        );
        assert_eq!(
            ctx.instance_of(origin.class, index, Reference::null()),
            Err(E::InvalidConstant {
                origin: origin.class,
                index
            })
        );
    }
}

#[test]
fn nonnull_source_handle_precedes_target_resolution_and_access_precedes_cast_failure() {
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
    )
    .class;
    let origin = define(
        &mut registry,
        l,
        "q/Origin",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["Missing", "p/Hidden", "[Lp/Hidden;"],
    );
    let mut heap = heap(&registry, u, &[origin.class, hidden]);
    let receiver = heap.allocate_instance(origin.class).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign.allocate_primitive_array(P::Int, 0).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.check_cast(origin.class, origin.targets[0], foreign_ref),
            Err(E::MissingUniverse)
        );
    }
    heap.bind_reference_types(&registry, u).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.check_cast(origin.class, origin.targets[0], foreign_ref),
        Err(E::Heap(HeapError::ForeignHeap))
    );
    assert_eq!(calls.get(), 0);
    for index in [1, 2] {
        assert_eq!(
            ctx.check_cast(origin.class, origin.targets[index], receiver),
            Err(E::Java(J::IllegalAccess))
        );
        assert_eq!(
            ctx.instance_of(origin.class, origin.targets[index], receiver),
            Err(E::Java(J::IllegalAccess))
        );
    }
    assert!(matches!(
        ctx.instance_of(origin.class, origin.targets[0], receiver),
        Err(E::Context(C::MissingDefinition { .. }))
    ));
    assert_eq!(calls.get(), 1);
}

#[test]
fn nominal_and_array_leaf_access_use_runtime_package_and_do_not_affect_pure_relation() {
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
    )
    .class;
    let origin = define(
        &mut registry,
        l,
        "p/Origin",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["p/Hidden", "[Lp/Hidden;"],
    );
    let mut heap = heap(&registry, u, &[origin.class, hidden]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.resolve_reference_type(origin.class, origin.targets[0]),
        Ok(T::Class(hidden))
    );
    assert_eq!(
        ctx.resolve_reference_type(origin.class, origin.targets[1]),
        Ok(array(1, A::Class(hidden), l))
    );
    assert_eq!(
        ctx.is_assignable(T::Class(hidden), T::Class(u.object)),
        Ok(true)
    );
}

#[test]
fn borrowed_array_leaf_alias_preserves_unpaired_utf16_and_shares_nominal_bindings() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let name = vec![0x70, 0x2f, 0xd800];
    let mut descriptor = units("[L");
    descriptor.extend(&name);
    descriptor.push(59);
    let target = define_raw(
        &mut registry,
        second,
        &name,
        Some(&units("java/lang/Object")),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define_raw(
        &mut registry,
        u.bootstrap_loader,
        &units("D"),
        Some(&units("java/lang/Object")),
        &[],
        0x21,
        &[descriptor, name.clone()],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .aliases
        .insert((u.bootstrap_loader, name.clone()), target);
    let target_plan = build_hierarchy(
        &registry,
        target,
        &mut resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
        plan(&registry, origin.class),
        target_plan,
    ];
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = (ctx.resolved_symbols(), ctx.symbol_units());
    assert_eq!(
        ctx.resolve_reference_type(origin.class, origin.targets[0]),
        Ok(array(1, A::Class(target), second))
    );
    assert_eq!(
        ctx.resolve_reference_type(origin.class, origin.targets[1]),
        Ok(T::Class(target))
    );
    assert_eq!(
        (ctx.resolved_symbols(), ctx.symbol_units()),
        (before.0 + 1, before.1 + name.len())
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        registry.class(target).unwrap().name().unwrap(),
        &JavaString::from_units(name)
    );
}

#[test]
fn same_named_nominal_and_array_leaves_in_distinct_loaders_do_not_alias() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let first = define(
        &mut registry,
        u.bootstrap_loader,
        "Same",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["Same"],
    );
    let other = define(
        &mut registry,
        second,
        "Same",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["Same"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), u.object);
    let other_plan = build_hierarchy(
        &registry,
        other.class,
        &mut resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
        plan(&registry, first.class),
        other_plan,
    ];
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.resolve_reference_type(first.class, first.targets[0]),
        Ok(T::Class(first.class))
    );
    assert_eq!(
        ctx.resolve_reference_type(other.class, other.targets[0]),
        Ok(T::Class(other.class))
    );
    assert_eq!(
        ctx.is_assignable(T::Class(first.class), T::Class(other.class)),
        Ok(false)
    );
    assert_eq!(
        ctx.is_assignable(
            array(1, A::Class(first.class), u.bootstrap_loader),
            array(1, A::Class(other.class), second)
        ),
        Ok(false)
    );
}

#[test]
fn registered_but_unadmitted_targets_are_not_cached_and_can_be_admitted_explicitly() {
    let (mut registry, u) = setup();
    let target = define(
        &mut registry,
        u.bootstrap_loader,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define(
        &mut registry,
        u.bootstrap_loader,
        "D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["Target"],
    );
    let mut heap = heap(&registry, u, &[origin.class]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        for _ in 0..2 {
            assert_eq!(
                ctx.resolve_reference_type(origin.class, origin.targets[0]),
                Err(E::Context(C::Instance(InstanceError::LayoutMissing {
                    class: target
                })))
            );
        }
        assert_eq!(calls.get(), 2);
    }
    heap.admit_instance_layouts(&registry, &[&plan(&registry, target)])
        .unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.resolve_reference_type(origin.class, origin.targets[0]),
        Ok(T::Class(target))
    );
}

#[test]
fn identical_types_and_rank_reductions_have_inclusive_cumulative_work_limits() {
    let (registry, u) = setup();
    let mut heap = heap(&registry, u, &[]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    for (value, exact) in [
        (T::Class(u.object), 9),
        (array(255, A::Primitive(P::Int), u.bootstrap_loader), 262),
    ] {
        for limit in [exact - 1, exact, usize::MAX] {
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
            assert_eq!(ctx.lookup_steps(), 5);
            let result = ctx.is_assignable(value, value);
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
                assert_eq!(result, Ok(true));
                assert_eq!(ctx.lookup_steps(), exact);
                if limit == exact {
                    assert_eq!(
                        ctx.is_assignable(value, value),
                        Err(E::Context(C::ResourceLimit {
                            resource: FieldResource::LookupSteps,
                            limit,
                            required: exact + 1
                        }))
                    );
                }
            }
        }
    }
}

#[test]
fn target_name_units_are_charged_on_cache_hits_and_null_does_not_scan() {
    let (mut registry, u) = setup();
    let origin = define(
        &mut registry,
        u.bootstrap_loader,
        "D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["java/lang/Object"],
    );
    let mut heap = heap(&registry, u, &[origin.class]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    for limit in [22, 24] {
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
        assert_eq!(ctx.lookup_steps(), 7);
        assert_eq!(
            ctx.instance_of(origin.class, origin.targets[0], Reference::null()),
            Ok(false)
        );
        assert_eq!(ctx.lookup_steps(), 7);
        let result = ctx.resolve_reference_type(origin.class, origin.targets[0]);
        if limit == 22 {
            assert_eq!(
                result,
                Err(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 23
                }))
            );
        } else {
            assert_eq!(result, Ok(T::Class(u.object)));
            assert_eq!(ctx.lookup_steps(), 24);
            assert_eq!(
                ctx.resolve_reference_type(origin.class, origin.targets[0]),
                Err(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 25
                }))
            );
        }
    }
}

#[test]
fn array_leaf_symbol_and_unit_limits_fail_before_the_callback_without_caching_failure() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let target = define(
        &mut registry,
        second,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define(
        &mut registry,
        u.bootstrap_loader,
        "D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["[LTarget;"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .aliases
        .insert((u.bootstrap_loader, units("Target")), target);
    let target_plan = build_hierarchy(
        &registry,
        target,
        &mut resolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
        plan(&registry, origin.class),
        target_plan,
    ];
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let (symbols, name_units) = {
        let ctx = context(&registry, &mut heap, &mut resolver);
        (ctx.resolved_symbols(), ctx.symbol_units())
    };
    for (max_symbols, max_units, error) in [
        (
            symbols,
            name_units + 6,
            Some((FieldResource::Symbols, symbols, symbols + 1)),
        ),
        (
            symbols + 1,
            name_units + 5,
            Some((FieldResource::SymbolUnits, name_units + 5, name_units + 6)),
        ),
        (symbols + 1, name_units + 6, None),
    ] {
        resolver.calls.set(0);
        let calls = resolver.calls.clone();
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_symbols,
                max_symbol_units: max_units,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let result = ctx.resolve_reference_type(origin.class, origin.targets[0]);
        if let Some((resource, limit, required)) = error {
            assert_eq!(
                result,
                Err(E::Context(C::ResourceLimit {
                    resource,
                    limit,
                    required
                }))
            );
            assert_eq!(calls.get(), 0);
            assert_eq!(ctx.resolved_symbols(), symbols);
        } else {
            assert_eq!(result, Ok(array(1, A::Class(target), second)));
            assert_eq!(calls.get(), 1);
            assert_eq!(ctx.resolved_symbols(), symbols + 1);
        }
    }
}

#[test]
fn rank_255_target_resolution_and_deep_nominal_walk_drop_are_iterative() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 3,
        ..RegistryOptions::default()
    })
    .unwrap();
    let l = registry.create_loader().unwrap();
    let object = define(&mut registry, l, "java/lang/Object", None, &[], 0x21, &[]).class;
    let cloneable = define(
        &mut registry,
        l,
        "java/lang/Cloneable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    )
    .class;
    let serializable = define(
        &mut registry,
        l,
        "java/io/Serializable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    )
    .class;
    let u = U {
        bootstrap_loader: l,
        object,
        cloneable,
        serializable,
    };
    let descriptor = format!("{}I", "[".repeat(255));
    let deep_targets = [descriptor.as_str()];
    let mut previous = String::from("java/lang/Object");
    let mut last = None;
    for index in 0..DEPTH {
        let name = format!("C{index}");
        last = Some(define(
            &mut registry,
            l,
            &name,
            Some(&previous),
            &[],
            0x21,
            if index + 1 == DEPTH {
                &deep_targets
            } else {
                &[]
            },
        ));
        previous = name;
    }
    let origin = last.unwrap();
    let deep = build_hierarchy(
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
    let plans = [
        plan(&registry, cloneable),
        plan(&registry, serializable),
        deep,
    ];
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 3,
            max_layout_edges: DEPTH + 2,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.resolve_reference_type(origin.class, origin.targets[0]),
        Ok(array(255, A::Primitive(P::Int), l))
    );
    assert_eq!(
        ctx.is_assignable(T::Class(origin.class), T::Class(object)),
        Ok(true)
    );
    assert_eq!(
        ctx.is_assignable(T::Class(origin.class), T::Class(cloneable)),
        Ok(false)
    );
    drop(ctx);
    drop(heap);
    drop(plans);
    drop(registry);
}

#[test]
fn all_primitive_class_descriptors_cast_existing_arrays_without_reference_array_allocation() {
    let (mut registry, u) = setup();
    let kinds = [
        P::Boolean,
        P::Byte,
        P::Char,
        P::Short,
        P::Int,
        P::Long,
        P::Float,
        P::Double,
    ];
    let targets = [
        "[Z",
        "[B",
        "[C",
        "[S",
        "[I",
        "[J",
        "[F",
        "[D",
        "[Ljava/lang/Object;",
    ];
    let origin = define(
        &mut registry,
        u.bootstrap_loader,
        "D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &targets,
    );
    let mut heap = heap(&registry, u, &[origin.class]);
    let references: Vec<_> = kinds
        .iter()
        .map(|kind| heap.allocate_primitive_array(*kind, 0).unwrap())
        .collect();
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (source, reference) in kinds.into_iter().zip(references) {
        for (index, target) in kinds.into_iter().enumerate() {
            assert_eq!(
                ctx.resolve_reference_type(origin.class, origin.targets[index]),
                Ok(array(1, A::Primitive(target), u.bootstrap_loader))
            );
            assert_eq!(
                ctx.instance_of(origin.class, origin.targets[index], reference),
                Ok(source == target)
            );
            assert_eq!(
                ctx.check_cast(origin.class, origin.targets[index], reference),
                if source == target {
                    Ok(reference)
                } else {
                    Err(E::Java(J::ClassCast))
                }
            );
        }
        assert_eq!(
            ctx.instance_of(origin.class, origin.targets[8], reference),
            Ok(false)
        );
    }
}

#[test]
fn nominal_fanout_exhaustion_is_checked_before_enqueuing_an_interface_edge() {
    let (mut registry, u) = setup();
    let names: Vec<_> = (0..8).map(|i| format!("I{i}")).collect();
    define(
        &mut registry,
        u.bootstrap_loader,
        "Shared",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
    );
    for name in &names {
        define(
            &mut registry,
            u.bootstrap_loader,
            name,
            Some("java/lang/Object"),
            &["Shared"],
            0x601,
            &[],
        );
    }
    let interfaces: Vec<_> = names.iter().map(String::as_str).collect();
    let root = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &interfaces,
        0x21,
        &[],
    )
    .class;
    let mut heap = heap(&registry, u, &[root]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    // 41 constructor steps, validations2 + pair1 + C node1. Limit45
    // rejects the first edge. After visiting Shared through I0, limit61
    // rejects I1's duplicate Shared edge before it can be enqueued.
    for limit in [45, 61] {
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
        assert_eq!(ctx.lookup_steps(), 41);
        assert_eq!(
            ctx.is_assignable(T::Class(root), T::Class(u.cloneable)),
            Err(E::Context(C::ResourceLimit {
                resource: FieldResource::LookupSteps,
                limit,
                required: limit + 1
            }))
        );
        assert_eq!(ctx.lookup_steps(), limit);
    }
}

#[test]
fn same_count_same_universe_heap_replacement_invalidates_reference_services() {
    let (registry, u) = setup();
    let mut replacement = heap(&registry, u, &[]);
    replacement.bind_reference_types(&registry, u).unwrap();
    let mut heap = heap(&registry, u, &[]);
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.heap().retained_layout_classes(),
        replacement.retained_layout_classes()
    );
    std::mem::swap(ctx.heap_mut(), &mut replacement);
    assert_eq!(
        ctx.reference_type(Reference::null()),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(
        ctx.is_assignable(T::Class(u.object), T::Class(u.object)),
        Err(E::Context(C::ContextChanged))
    );
}

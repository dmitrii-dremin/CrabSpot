//! Checked actual reference-array storage, quotas, covariance and edge ordering.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ArrayAccessError as AE, ArrayElement as A, ArrayJavaError as AJ, ClassId, DefinitionRegistry,
    FieldAccessError as C, FieldResolutionContext, FieldResolutionOptions, FieldResource,
    HeapEntryKind, HeapError, HeapOptions, HeapResource, HierarchyOptions, HierarchyPlan,
    HierarchyResolver, InstanceError, InstanceOptions, LoaderId, ManagedHeap,
    PrimitiveArrayKind as P, REFERENCE_ARRAY_ELEMENT_BYTES as WIDTH, Reference, ReferenceType as T,
    ReferenceTypeUniverse as U, RegistryOptions, ResolveError, SameLoaderResolver,
    TypeAccessError as E, Value, build_hierarchy,
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

fn bound_heap(registry: &DefinitionRegistry, u: U, classes: &[ClassId]) -> ManagedHeap {
    let mut heap = heap(registry, u, classes);
    heap.bind_reference_types(registry, u).unwrap();
    heap
}
fn counts(heap: &ManagedHeap) -> (usize, usize, usize, usize) {
    (
        heap.allocated_arrays(),
        heap.total_elements(),
        heap.payload_bytes(),
        heap.allocated_instances(),
    )
}
fn children(heap: &ManagedHeap, array: Reference) -> Vec<crabspot_runtime::HeapRef> {
    heap.children(array).unwrap().collect()
}

#[test]
fn defaults_empty_identities_and_actual_types_survive_allocation_growth_and_moves() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let a = ctx.allocate_reference_array(T::Class(u.object), 3).unwrap();
    let empty = ctx.allocate_reference_array(T::Class(u.object), 0).unwrap();
    let empty2 = ctx.allocate_reference_array(T::Class(u.object), 0).unwrap();
    assert_ne!(empty, empty2);
    assert_eq!(
        ctx.heap().reference_array_type(a),
        Ok(array(1, A::Class(u.object), u.bootstrap_loader))
    );
    assert_eq!(
        ctx.reference_type(a),
        Ok(Some(array(1, A::Class(u.object), u.bootstrap_loader)))
    );
    for index in 0..3 {
        assert_eq!(ctx.read_reference_array(a, index), Ok(Reference::null()));
        assert_eq!(ctx.heap().load_reference(a, index), Ok(Reference::null()));
    }
    assert_eq!(ctx.heap().length(a), Ok(3));
    assert_eq!(ctx.heap().length(empty), Ok(0));
    assert!(children(ctx.heap(), a).is_empty());
    assert_eq!(counts(ctx.heap()), (3, 3, 3 * WIDTH, 0));
    for _ in 0..512 {
        ctx.allocate_reference_array(T::Class(u.object), 0).unwrap();
    }
    drop(ctx);
    let moved = heap;
    assert_eq!(moved.length(a), Ok(3));
    moved.validate_root(a.heap_ref().unwrap()).unwrap();
}

#[test]
fn nominal_interface_abstract_and_core_components_use_exact_actual_assignability() {
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
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x421,
        &[],
    )
    .class;
    let good = define(&mut registry, l, "Good", Some("Base"), &["I"], 0x21, &[]).class;
    let other = define(
        &mut registry,
        l,
        "Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let mut heap = bound_heap(&registry, u, &[good, other]);
    let good_ref = heap.allocate_instance(good).unwrap();
    let other_ref = heap.allocate_instance(other).unwrap();
    let ints = heap.allocate_primitive_array(P::Int, 0).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for component in [interface, base, good, u.object, u.cloneable, u.serializable] {
        let destination = ctx
            .allocate_reference_array(T::Class(component), 1)
            .unwrap();
        let value = if [u.cloneable, u.serializable].contains(&component) {
            ints
        } else {
            good_ref
        };
        ctx.write_reference_array(destination, 0, value).unwrap();
        assert_eq!(ctx.read_reference_array(destination, 0), Ok(value));
        if component != u.object {
            assert_eq!(
                ctx.write_reference_array(destination, 0, other_ref),
                Err(AE::Java(AJ::ArrayStore))
            );
            assert_eq!(ctx.read_reference_array(destination, 0), Ok(value));
        }
    }
}

#[test]
fn rank_two_primitive_leaves_and_higher_covariance_retain_the_allocated_type() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
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
    let values: Vec<_> = kinds
        .iter()
        .map(|kind| heap.allocate_primitive_array(*kind, 0).unwrap())
        .collect();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (index, kind) in kinds.into_iter().enumerate() {
        let outer = ctx
            .allocate_reference_array(array(1, A::Primitive(kind), u.bootstrap_loader), 2)
            .unwrap();
        assert_eq!(
            ctx.heap().reference_array_type(outer),
            Ok(array(2, A::Primitive(kind), u.bootstrap_loader))
        );
        ctx.write_reference_array(outer, 0, values[index]).unwrap();
        assert_eq!(ctx.read_reference_array(outer, 0), Ok(values[index]));
        assert_eq!(
            ctx.write_reference_array(outer, 1, values[(index + 1) % 8]),
            Err(AE::Java(AJ::ArrayStore))
        );
        let objects = ctx.allocate_reference_array(T::Class(u.object), 1).unwrap();
        ctx.write_reference_array(objects, 0, outer).unwrap();
        let actual = ctx.heap().reference_array_type(outer).unwrap();
        assert_eq!(
            ctx.is_assignable(actual, array(1, A::Class(u.object), u.bootstrap_loader)),
            Ok(true)
        );
        let high = ctx
            .allocate_reference_array(array(2, A::Class(u.object), u.bootstrap_loader), 1)
            .unwrap();
        assert_eq!(
            ctx.write_reference_array(high, 0, outer),
            Err(AE::Java(AJ::ArrayStore))
        );
    }
}

#[test]
fn aliases_overwrite_clear_self_links_and_multiarray_cycles_preserve_index_order() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let object = heap.allocate_instance(u.object).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let a = ctx.allocate_reference_array(T::Class(u.object), 4).unwrap();
    let b = ctx.allocate_reference_array(T::Class(u.object), 2).unwrap();
    let before = counts(ctx.heap());
    for (index, value) in [(0, a), (1, object), (2, object), (3, b)] {
        ctx.write_reference_array(a, index, value).unwrap();
    }
    ctx.write_reference_array(b, 0, a).unwrap();
    assert_eq!(
        children(ctx.heap(), a),
        [
            a.heap_ref().unwrap(),
            object.heap_ref().unwrap(),
            object.heap_ref().unwrap(),
            b.heap_ref().unwrap()
        ]
    );
    assert_eq!(children(ctx.heap(), b), [a.heap_ref().unwrap()]);
    let work = ctx.lookup_steps();
    ctx.write_reference_array(a, 1, Reference::null()).unwrap();
    assert_eq!(ctx.lookup_steps(), work);
    ctx.write_reference_array(a, 2, b).unwrap();
    assert_eq!(
        children(ctx.heap(), a),
        [
            a.heap_ref().unwrap(),
            b.heap_ref().unwrap(),
            b.heap_ref().unwrap()
        ]
    );
    assert_eq!(counts(ctx.heap()), before);
}

#[test]
fn generic_length_works_but_primitive_and_instance_operations_reject_reference_payloads() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let object = heap.allocate_instance(u.object).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let reference = ctx.allocate_reference_array(T::Class(u.object), 2).unwrap();
    let error = HeapError::WrongEntryKind {
        expected: HeapEntryKind::PrimitiveArray,
        actual: HeapEntryKind::ReferenceArray,
    };
    assert_eq!(ctx.heap().kind(reference), Err(error.clone()));
    assert_eq!(ctx.heap().load(reference, -1), Err(error.clone()));
    assert_eq!(
        ctx.heap_mut().store(reference, -1, Value::Int(0)),
        Err(error)
    );
    assert_eq!(ctx.heap().length(reference), Ok(2));
    assert_eq!(
        ctx.heap().length(object),
        Err(HeapError::WrongEntryKind {
            expected: HeapEntryKind::Array,
            actual: HeapEntryKind::Instance
        })
    );
    assert_eq!(
        ctx.heap().instance_class(reference),
        Err(InstanceError::Heap(HeapError::WrongEntryKind {
            expected: HeapEntryKind::Instance,
            actual: HeapEntryKind::ReferenceArray
        }))
    );
}

#[test]
fn null_bounds_kind_and_source_handle_ordering_does_not_touch_existing_elements() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let object = heap.allocate_instance(u.object).unwrap();
    let primitive = heap.allocate_primitive_array(P::Int, 0).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign.allocate_primitive_array(P::Int, 0).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let a = ctx.allocate_reference_array(T::Class(u.object), 1).unwrap();
    ctx.write_reference_array(a, 0, object).unwrap();
    let before = (
        children(ctx.heap(), a),
        counts(ctx.heap()),
        ctx.lookup_steps(),
    );
    for index in [-1, 0, i32::MAX] {
        assert_eq!(
            ctx.read_reference_array(Reference::null(), index),
            Err(AE::Java(AJ::NullPointer))
        );
        assert_eq!(
            ctx.write_reference_array(Reference::null(), index, foreign_ref),
            Err(AE::Java(AJ::NullPointer))
        );
    }
    for index in [-1, 1, i32::MAX] {
        assert_eq!(
            ctx.write_reference_array(a, index, foreign_ref),
            Err(AE::Java(AJ::IndexOutOfBounds))
        );
        assert_eq!(
            ctx.read_reference_array(a, index),
            Err(AE::Java(AJ::IndexOutOfBounds))
        );
    }
    assert_eq!(
        ctx.write_reference_array(primitive, -1, foreign_ref),
        Err(AE::Heap(HeapError::WrongEntryKind {
            expected: HeapEntryKind::ReferenceArray,
            actual: HeapEntryKind::PrimitiveArray
        }))
    );
    assert_eq!(
        ctx.write_reference_array(foreign_ref, 0, Reference::null()),
        Err(AE::Heap(HeapError::ForeignHeap))
    );
    assert_eq!(
        ctx.write_reference_array(a, 0, foreign_ref),
        Err(AE::Type(Box::new(E::Heap(HeapError::ForeignHeap))))
    );
    assert_eq!(ctx.read_reference_array(a, 0), Ok(object));
    assert_eq!(
        (
            children(ctx.heap(), a),
            counts(ctx.heap()),
            ctx.lookup_steps()
        ),
        before
    );
}

#[test]
fn rank_boundaries_and_invalid_public_components_precede_negative_length_and_allocation() {
    let (mut registry, u) = setup();
    let wrong_loader = registry.create_loader().unwrap();
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
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for length in [-1, 0, 1] {
        for bad in [
            array(0, A::Primitive(P::Int), u.bootstrap_loader),
            array(1, A::Primitive(P::Int), wrong_loader),
            array(1, A::Class(u.object), wrong_loader),
            array(255, A::Primitive(P::Int), u.bootstrap_loader),
        ] {
            assert!(
                matches!(ctx.allocate_reference_array(bad,length),Err(AE::Type(error)) if matches!(*error,E::InvalidType(_)))
            );
        }
        assert_eq!(
            ctx.allocate_reference_array(T::Class(missing), length),
            Err(AE::Type(Box::new(E::Context(C::Instance(
                InstanceError::LayoutMissing { class: missing }
            )))))
        );
    }
    assert_eq!(counts(ctx.heap()), (0, 0, 0, 0));
    let max = ctx
        .allocate_reference_array(array(254, A::Primitive(P::Int), u.bootstrap_loader), 0)
        .unwrap();
    assert_eq!(
        ctx.heap().reference_array_type(max),
        Ok(array(255, A::Primitive(P::Int), u.bootstrap_loader))
    );
    assert_eq!(
        ctx.allocate_reference_array(T::Class(u.object), -1),
        Err(AE::Java(AJ::NegativeArraySize))
    );
}

#[test]
fn component_resolution_access_and_cp_failures_precede_negative_length() {
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
        &[
            "p/Hidden",
            "[Lp/Hidden;",
            "Missing",
            "java/lang/Object",
            "[I",
        ],
    );
    let mut heap = bound_heap(&registry, u, &[origin.class, hidden]);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for index in [0, 1] {
        assert_eq!(
            ctx.new_reference_array(origin.class, origin.targets[index], -1),
            Err(AE::Java(AJ::IllegalAccess))
        );
    }
    assert!(
        matches!(ctx.new_reference_array(origin.class,origin.targets[2],-1),Err(AE::Type(error)) if matches!(*error,E::Context(C::MissingDefinition {..})))
    );
    assert_eq!(calls.get(), 1);
    for index in [0, u16::MAX, origin.wrong] {
        assert_eq!(
            ctx.new_reference_array(origin.class, index, -1),
            Err(AE::Type(Box::new(E::InvalidConstant {
                origin: origin.class,
                index
            })))
        );
    }
    assert_eq!(
        ctx.new_reference_array(origin.class, origin.targets[3], -1),
        Err(AE::Java(AJ::NegativeArraySize))
    );
    let reference = ctx
        .new_reference_array(origin.class, origin.targets[4], 1)
        .unwrap();
    assert_eq!(
        ctx.heap().reference_array_type(reference),
        Ok(array(2, A::Primitive(P::Int), l))
    );
}

#[test]
fn covariant_aliases_use_actual_component_and_unequal_rank_assignment() {
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
    let good = define(
        &mut registry,
        l,
        "Good",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
    )
    .class;
    let other = define(
        &mut registry,
        l,
        "Other",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
    )
    .class;
    let mut heap = bound_heap(&registry, u, &[good, other]);
    let good_ref = heap.allocate_instance(good).unwrap();
    let other_ref = heap.allocate_instance(other).unwrap();
    let ints = heap.allocate_primitive_array(P::Int, 0).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let actual = ctx.allocate_reference_array(T::Class(good), 1).unwrap();
    ctx.write_reference_array(actual, 0, good_ref).unwrap();
    assert_eq!(
        ctx.is_assignable(
            array(1, A::Class(good), l),
            array(1, A::Class(interface), l)
        ),
        Ok(true)
    );
    assert_eq!(
        ctx.write_reference_array(actual, 0, other_ref),
        Err(AE::Java(AJ::ArrayStore))
    );
    assert_eq!(ctx.read_reference_array(actual, 0), Ok(good_ref));
    let int2 = ctx
        .allocate_reference_array(array(1, A::Primitive(P::Int), l), 1)
        .unwrap();
    ctx.write_reference_array(int2, 0, ints).unwrap();
    let object2 = ctx
        .allocate_reference_array(array(1, A::Class(u.object), l), 1)
        .unwrap();
    ctx.write_reference_array(object2, 0, int2).unwrap();
    assert_eq!(ctx.read_reference_array(object2, 0), Ok(int2));
}

#[test]
fn all_zero_quotas_fail_atomically_in_primitive_allocation_precedence() {
    let (registry, u) = setup();
    for (options, length, resource, limit, required) in [
        (
            HeapOptions {
                max_arrays: 0,
                ..HeapOptions::default()
            },
            0,
            HeapResource::Arrays,
            0,
            1,
        ),
        (
            HeapOptions {
                max_array_length: 0,
                ..HeapOptions::default()
            },
            1,
            HeapResource::ArrayLength,
            0,
            1,
        ),
        (
            HeapOptions {
                max_total_elements: 0,
                ..HeapOptions::default()
            },
            1,
            HeapResource::TotalElements,
            0,
            1,
        ),
        (
            HeapOptions {
                max_payload_bytes: 0,
                ..HeapOptions::default()
            },
            1,
            HeapResource::PayloadBytes,
            0,
            WIDTH,
        ),
    ] {
        let mut heap =
            ManagedHeap::new_with_instances(options, InstanceOptions::default()).unwrap();
        let plans = [
            plan(&registry, u.cloneable),
            plan(&registry, u.serializable),
        ];
        heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
            .unwrap();
        heap.bind_reference_types(&registry, u).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.allocate_reference_array(T::Class(u.object), length),
            Err(AE::Heap(HeapError::ResourceLimit {
                resource,
                limit,
                required
            }))
        );
        assert_eq!(counts(ctx.heap()), (0, 0, 0, 0));
    }
}

#[test]
fn exact_cumulative_quotas_are_shared_and_rejected_allocations_allow_smaller_retry() {
    let (registry, u) = setup();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
    ];
    let options = HeapOptions {
        max_arrays: 3,
        max_array_length: 2,
        max_total_elements: 3,
        max_payload_bytes: 8 + 2 * WIDTH,
    };
    let mut heap = ManagedHeap::new_with_instances(
        options,
        InstanceOptions {
            max_instances: 0,
            max_total_instance_slots: 0,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let primitive = heap.allocate_primitive_array(P::Long, 1).unwrap();
    heap.store(primitive, 0, Value::Long(77)).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let reference = ctx.allocate_reference_array(T::Class(u.object), 2).unwrap();
    assert_eq!(counts(ctx.heap()), (2, 3, 8 + 2 * WIDTH, 0));
    assert_eq!(
        ctx.allocate_reference_array(T::Class(u.object), 1),
        Err(AE::Heap(HeapError::ResourceLimit {
            resource: HeapResource::TotalElements,
            limit: 3,
            required: 4
        }))
    );
    assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Long(77)));
    assert!(children(ctx.heap(), reference).is_empty());
    ctx.allocate_reference_array(T::Class(u.object), 0).unwrap();
    assert_eq!(
        ctx.heap_mut().allocate_primitive_array(P::Byte, 0),
        Err(HeapError::ResourceLimit {
            resource: HeapResource::Arrays,
            limit: 3,
            required: 4
        })
    );
    assert_eq!(counts(ctx.heap()), (3, 3, 8 + 2 * WIDTH, 0));
}

#[test]
fn payload_bytes_mix_primitive_and_reference_widths_and_max_limits_are_not_allocations() {
    let (registry, u) = setup();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
    ];
    for limit in [WIDTH + 1, usize::MAX] {
        let options = HeapOptions {
            max_arrays: usize::MAX,
            max_array_length: usize::MAX,
            max_total_elements: usize::MAX,
            max_payload_bytes: limit,
        };
        let mut heap =
            ManagedHeap::new_with_instances(options, InstanceOptions::default()).unwrap();
        heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
            .unwrap();
        heap.bind_reference_types(&registry, u).unwrap();
        heap.allocate_primitive_array(P::Byte, 1).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        ctx.allocate_reference_array(T::Class(u.object), 1).unwrap();
        assert_eq!(ctx.heap().payload_bytes(), WIDTH + 1);
        if limit != usize::MAX {
            assert_eq!(
                ctx.allocate_reference_array(T::Class(u.object), 1),
                Err(AE::Heap(HeapError::ResourceLimit {
                    resource: HeapResource::PayloadBytes,
                    limit,
                    required: 2 * WIDTH + 1
                }))
            );
            ctx.allocate_reference_array(T::Class(u.object), 0).unwrap();
        } else {
            ctx.allocate_reference_array(T::Class(u.object), 2).unwrap();
        }
    }
    assert_eq!(WIDTH, core::mem::size_of::<Reference>());
}

#[test]
fn actual_loader_identity_and_typed_inaccessible_components_do_not_use_caller_access() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let first = define(
        &mut registry,
        l,
        "p/Same",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[],
    )
    .class;
    let other = define(
        &mut registry,
        second,
        "p/Same",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
    )
    .class;
    let origin = define(
        &mut registry,
        second,
        "q/D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["p/Same", "[Lp/Same;"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), u.object);
    let roots = [u.cloneable, u.serializable, first, other, origin.class];
    let plans: Vec<_> = roots
        .iter()
        .map(|class| {
            build_hierarchy(
                &registry,
                *class,
                &mut resolver,
                HierarchyOptions::default(),
            )
            .unwrap()
        })
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let first_value = heap.allocate_instance(first).unwrap();
    let other_value = heap.allocate_instance(other).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let first_array = ctx.allocate_reference_array(T::Class(first), 1).unwrap();
    ctx.write_reference_array(first_array, 0, first_value)
        .unwrap();
    assert_eq!(
        ctx.write_reference_array(first_array, 0, other_value),
        Err(AE::Java(AJ::ArrayStore))
    );
    let actual = ctx
        .new_reference_array(origin.class, origin.targets[0], 1)
        .unwrap();
    assert_eq!(
        ctx.heap().reference_array_type(actual),
        Ok(array(1, A::Class(other), second))
    );
    ctx.write_reference_array(actual, 0, other_value).unwrap();
    assert_eq!(
        ctx.write_reference_array(actual, 0, first_value),
        Err(AE::Java(AJ::ArrayStore))
    );
    let nested = ctx
        .new_reference_array(origin.class, origin.targets[1], 1)
        .unwrap();
    assert_eq!(
        ctx.heap().reference_array_type(nested),
        Ok(array(2, A::Class(other), second))
    );
    ctx.write_reference_array(nested, 0, actual).unwrap();
    assert_eq!(
        ctx.write_reference_array(nested, 0, first_array),
        Err(AE::Java(AJ::ArrayStore))
    );
}

#[test]
fn missing_universe_null_services_and_context_invalidation_preserve_error_order() {
    let (registry, u) = setup();
    let mut heap = heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.allocate_reference_array(T::Class(u.object), -1),
            Err(AE::Type(Box::new(E::MissingUniverse)))
        );
        assert_eq!(
            ctx.read_reference_array(Reference::null(), -1),
            Err(AE::Java(AJ::NullPointer))
        );
        assert_eq!(
            ctx.write_reference_array(Reference::null(), -1, Reference::null()),
            Err(AE::Java(AJ::NullPointer))
        );
        ctx.heap_mut().bind_reference_types(&registry, u).unwrap();
        assert_eq!(
            ctx.read_reference_array(Reference::null(), 0),
            Err(AE::Type(Box::new(E::Context(C::ContextChanged))))
        );
    }
    let mut replacement =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
    ];
    replacement
        .admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
        .unwrap();
    replacement.bind_reference_types(&registry, u).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let reference = ctx.allocate_reference_array(T::Class(u.object), 1).unwrap();
    ctx.write_reference_array(reference, 0, Reference::null())
        .unwrap();
    assert_eq!(
        ctx.read_reference_array(reference, 0),
        Ok(Reference::null())
    );
    std::mem::swap(ctx.heap_mut(), &mut replacement);
    assert_eq!(
        ctx.write_reference_array(reference, 0, Reference::null()),
        Err(AE::Type(Box::new(E::Context(C::ContextChanged))))
    );
}

#[test]
fn exact_typed_allocation_store_and_null_read_budgets_are_inclusive_and_atomic() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let object = heap.allocate_instance(u.object).unwrap();
    let mut resolver = SameLoaderResolver;
    for limit in [5, 6, 12, 13, 20, 21, usize::MAX] {
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
        let allocated = ctx.allocate_reference_array(T::Class(u.object), 1);
        if limit == 5 {
            assert_eq!(
                allocated,
                Err(AE::Type(Box::new(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 6
                }))))
            );
            continue;
        }
        let a = allocated.unwrap();
        assert_eq!(ctx.lookup_steps(), 6);
        let work = ctx.lookup_steps();
        ctx.write_reference_array(a, 0, Reference::null()).unwrap();
        assert_eq!(ctx.read_reference_array(a, 0), Ok(Reference::null()));
        assert_eq!(ctx.lookup_steps(), work);
        if limit == 6 {
            continue;
        }
        let store = ctx.write_reference_array(a, 0, object);
        if limit == 12 {
            assert_eq!(
                store,
                Err(AE::Type(Box::new(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 13
                }))))
            );
            assert!(children(ctx.heap(), a).is_empty());
            continue;
        }
        store.unwrap();
        assert_eq!(ctx.lookup_steps(), 13);
        if limit == 13 {
            continue;
        }
        let old = children(ctx.heap(), a);
        let store = ctx.write_reference_array(a, 0, a);
        if limit == 20 {
            assert_eq!(
                store,
                Err(AE::Type(Box::new(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 21
                }))))
            );
            assert_eq!(children(ctx.heap(), a), old);
        } else {
            store.unwrap();
            assert_eq!(ctx.lookup_steps(), 21);
            assert_eq!(children(ctx.heap(), a), [a.heap_ref().unwrap()]);
        }
    }
}

#[test]
fn component_name_cache_and_symbol_unit_quotas_use_the_existing_resolution_policy() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let remote = define(
        &mut registry,
        second,
        "Remote",
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
        &["Remote"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .aliases
        .insert((u.bootstrap_loader, units("Remote")), remote);
    let roots = [u.cloneable, u.serializable, origin.class, remote];
    let plans: Vec<_> = roots
        .iter()
        .map(|class| {
            build_hierarchy(
                &registry,
                *class,
                &mut resolver,
                HierarchyOptions::default(),
            )
            .unwrap()
        })
        .collect();
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
    for (options, error) in [
        (
            FieldResolutionOptions {
                max_symbols: symbols,
                ..FieldResolutionOptions::default()
            },
            Some(C::ResourceLimit {
                resource: FieldResource::Symbols,
                limit: symbols,
                required: symbols + 1,
            }),
        ),
        (
            FieldResolutionOptions {
                max_symbol_units: name_units + 5,
                ..FieldResolutionOptions::default()
            },
            Some(C::ResourceLimit {
                resource: FieldResource::SymbolUnits,
                limit: name_units + 5,
                required: name_units + 6,
            }),
        ),
        (FieldResolutionOptions::default(), None),
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
        if let Some(error) = error {
            assert_eq!(
                ctx.new_reference_array(origin.class, origin.targets[0], 0),
                Err(AE::Type(Box::new(E::Context(error))))
            );
            assert_eq!(calls.get(), 0);
        } else {
            for _ in 0..2 {
                let before = ctx.lookup_steps();
                let a = ctx
                    .new_reference_array(origin.class, origin.targets[0], 0)
                    .unwrap();
                assert_eq!(
                    ctx.heap().reference_array_type(a),
                    Ok(array(1, A::Class(remote), second))
                );
                assert_eq!(ctx.lookup_steps() - before, 8);
            }
            assert_eq!(calls.get(), 1);
        }
    }
}

#[test]
fn actual_reference_arrays_integrate_with_checked_fields_casts_and_cross_object_cycles() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let mut pool = Pool::default();
    let own = pool.class(&units("Holder"));
    let object = pool.class(&units("java/lang/Object"));
    let name = pool.text(&units("r"));
    let desc = pool.text(&units("[Ljava/lang/Object;"));
    let nat = pool.add(12, &words(&[name, desc]));
    let field_ref = pool.add(9, &words(&[own, nat]));
    let target = pool.class(&units("[Ljava/lang/Object;"));
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[0x21, own, object, 0, 1, 1, name, desc, 0, 0, 0]));
    let holder = registry
        .define(l, &units("Holder"), &bytes, ParseOptions::default())
        .unwrap();
    let mut heap = bound_heap(&registry, u, &[holder]);
    let receiver = heap.allocate_instance(holder).unwrap();
    let primitive = heap.allocate_primitive_array(P::Int, 0).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let array_ref = ctx.allocate_reference_array(T::Class(u.object), 1).unwrap();
    ctx.write_field(holder, field_ref, receiver, Value::Reference(array_ref))
        .unwrap();
    ctx.write_reference_array(array_ref, 0, receiver).unwrap();
    assert_eq!(
        ctx.read_field(holder, field_ref, receiver),
        Ok(Value::Reference(array_ref))
    );
    assert_eq!(ctx.check_cast(holder, target, array_ref), Ok(array_ref));
    assert_eq!(ctx.instance_of(holder, target, array_ref), Ok(true));
    assert_eq!(
        children(ctx.heap(), receiver),
        [array_ref.heap_ref().unwrap()]
    );
    assert_eq!(
        children(ctx.heap(), array_ref),
        [receiver.heap_ref().unwrap()]
    );
    assert!(matches!(
        ctx.write_field(holder, field_ref, receiver, Value::Reference(primitive)),
        Err(C::IncompatibleReference(_))
    ));
    let int2 = ctx
        .allocate_reference_array(array(1, A::Primitive(P::Int), l), 1)
        .unwrap();
    ctx.write_field(holder, field_ref, receiver, Value::Reference(int2))
        .unwrap();
    assert_eq!(ctx.check_cast(holder, target, int2), Ok(int2));
}

#[test]
fn deep_assignment_and_ten_thousand_array_links_drop_without_recursion() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 5,
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
    define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
    );
    let mut previous = String::from("Base");
    let mut last = None;
    for index in 0..DEPTH {
        let name = format!("C{index}");
        last = Some(define(&mut registry, l, &name, Some(&previous), &[], 0x21, &[]).class);
        previous = name;
    }
    let last = last.unwrap();
    let deep = build_hierarchy(
        &registry,
        last,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 3,
            max_edges: DEPTH + 3,
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
        HeapOptions {
            max_arrays: DEPTH + 1,
            max_array_length: 1,
            max_total_elements: DEPTH + 1,
            max_payload_bytes: (DEPTH + 1) * WIDTH,
        },
        InstanceOptions {
            max_layout_classes: DEPTH + 5,
            max_layout_edges: DEPTH + 5,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let value = heap.allocate_instance(last).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let typed = ctx
        .allocate_reference_array(T::Class(interface), 1)
        .unwrap();
    ctx.write_reference_array(typed, 0, value).unwrap();
    let first = ctx.allocate_reference_array(T::Class(object), 1).unwrap();
    let mut previous = first;
    for _ in 1..DEPTH {
        let next = ctx.allocate_reference_array(T::Class(object), 1).unwrap();
        ctx.write_reference_array(next, 0, previous).unwrap();
        previous = next;
    }
    ctx.write_reference_array(first, 0, previous).unwrap();
    assert_eq!(children(ctx.heap(), first), [previous.heap_ref().unwrap()]);
    assert_eq!(ctx.heap().allocated_arrays(), DEPTH + 1);
    drop(ctx);
    drop(heap);
    drop(plans);
    drop(registry);
}

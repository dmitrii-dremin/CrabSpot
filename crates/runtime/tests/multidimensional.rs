//! Complete/partial multidimensional geometry, shared quota and atomicity evidence.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ArrayAccessError as AE, ArrayElement as A, ArrayJavaError as AJ, ClassId, DefinitionRegistry,
    FieldAccessError as C, FieldResolutionContext, FieldResolutionOptions, FieldResource,
    HeapError, HeapOptions, HeapResource, HierarchyOptions, HierarchyPlan, HierarchyResolver,
    InstanceError, InstanceOptions, LoaderId, ManagedHeap, PrimitiveArrayKind as P,
    REFERENCE_ARRAY_ELEMENT_BYTES as WIDTH, Reference, ReferenceType as T,
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

fn primitive_zero(kind: P) -> Value {
    match kind {
        P::Long => Value::Long(0),
        P::Float => Value::Float(0),
        P::Double => Value::Double(0),
        _ => Value::Int(0),
    }
}
#[test]
fn all_eight_primitive_leaves_have_complete_and_partial_rank_defaults_and_actual_types() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for kind in [
        P::Boolean,
        P::Byte,
        P::Char,
        P::Short,
        P::Int,
        P::Long,
        P::Float,
        P::Double,
    ] {
        let before = counts(ctx.heap());
        let root = ctx
            .allocate_multidimensional_array(
                array(2, A::Primitive(kind), u.bootstrap_loader),
                &[2, 3],
            )
            .unwrap();
        let left = ctx.read_reference_array(root, 0).unwrap();
        let right = ctx.read_reference_array(root, 1).unwrap();
        assert_ne!(left, right);
        assert_eq!(
            ctx.reference_type(root),
            Ok(Some(array(2, A::Primitive(kind), u.bootstrap_loader)))
        );
        for leaf in [left, right] {
            assert_eq!(ctx.heap().kind(leaf), Ok(kind));
            assert_eq!(ctx.heap().length(leaf), Ok(3));
            for index in 0..3 {
                assert_eq!(ctx.heap().load(leaf, index), Ok(primitive_zero(kind)));
            }
            assert!(children(ctx.heap(), leaf).is_empty());
        }
        let after = counts(ctx.heap());
        assert_eq!(
            (after.0 - before.0, after.1 - before.1, after.2 - before.2),
            (3, 8, 2 * WIDTH + 6 * kind.element_bytes())
        );
        let partial = ctx
            .allocate_multidimensional_array(
                array(3, A::Primitive(kind), u.bootstrap_loader),
                &[2, 3],
            )
            .unwrap();
        for index in 0..2 {
            let child = ctx.read_reference_array(partial, index).unwrap();
            assert_eq!(
                ctx.heap().reference_array_type(child),
                Ok(array(2, A::Primitive(kind), u.bootstrap_loader))
            );
            assert_eq!(ctx.heap().length(child), Ok(3));
            assert!(children(ctx.heap(), child).is_empty());
            for slot in 0..3 {
                assert_eq!(ctx.read_reference_array(child, slot), Ok(Reference::null()));
            }
        }
        let leaf = ctx
            .allocate_multidimensional_array(array(1, A::Primitive(kind), u.bootstrap_loader), &[4])
            .unwrap();
        assert_eq!(ctx.heap().kind(leaf), Ok(kind));
        assert_eq!(ctx.heap().length(leaf), Ok(4));
    }
}

#[test]
fn nominal_interface_and_abstract_leaves_allocate_nulls_without_constructing_components() {
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
    let abstract_class = define(
        &mut registry,
        l,
        "Abstract",
        Some("java/lang/Object"),
        &[],
        0x421,
        &[],
    )
    .class;
    let mut heap = bound_heap(&registry, u, &[interface, abstract_class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for leaf in [u.object, interface, abstract_class] {
        let before = counts(ctx.heap());
        let root = ctx
            .allocate_multidimensional_array(array(3, A::Class(leaf), l), &[2, 2, 2])
            .unwrap();
        let mut level = vec![root];
        for rank in (1..=3).rev() {
            let mut next = Vec::new();
            for node in level {
                assert_eq!(
                    ctx.heap().reference_array_type(node),
                    Ok(array(rank, A::Class(leaf), l))
                );
                assert_eq!(ctx.heap().length(node), Ok(2));
                for index in 0..2 {
                    let value = ctx.read_reference_array(node, index).unwrap();
                    if rank > 1 {
                        assert!(!value.is_null());
                        next.push(value);
                    } else {
                        assert!(value.is_null());
                    }
                }
            }
            level = next;
        }
        let after = counts(ctx.heap());
        assert_eq!(
            (
                after.0 - before.0,
                after.1 - before.1,
                after.2 - before.2,
                after.3 - before.3
            ),
            (7, 14, 14 * WIDTH, 0)
        );
    }
}

#[test]
fn zero_at_each_level_stops_geometry_but_does_not_hide_later_negative_counts() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (sizes, nodes, elements) in [([0, 7, 9], 1, 0), ([2, 0, 7], 3, 2), ([2, 3, 0], 9, 8)] {
        let before = counts(ctx.heap());
        let root = ctx
            .allocate_multidimensional_array(
                array(3, A::Primitive(P::Int), u.bootstrap_loader),
                &sizes,
            )
            .unwrap();
        let after = counts(ctx.heap());
        assert_eq!(
            (after.0 - before.0, after.1 - before.1, after.2 - before.2),
            (nodes, elements, elements * WIDTH)
        );
        assert_eq!(ctx.heap().length(root), Ok(sizes[0]));
    }
    let before = counts(ctx.heap());
    for sizes in [[0, -1, 9], [2, 0, -1], [-1, 0, 0]] {
        assert_eq!(
            ctx.allocate_multidimensional_array(
                array(3, A::Primitive(P::Int), u.bootstrap_loader),
                &sizes
            ),
            Err(AE::Java(AJ::NegativeArraySize))
        );
        assert_eq!(counts(ctx.heap()), before);
    }
}

#[test]
fn unreachable_enormous_positive_counts_after_zero_do_not_consume_deeper_quotas() {
    let (registry, u) = setup();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
    ];
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions {
            max_arrays: 1,
            max_array_length: 0,
            max_total_elements: 0,
            max_payload_bytes: 0,
        },
        InstanceOptions::default(),
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let reference = ctx
        .allocate_multidimensional_array(
            array(3, A::Primitive(P::Long), u.bootstrap_loader),
            &[0, i32::MAX, i32::MAX],
        )
        .unwrap();
    assert_eq!(ctx.heap().length(reference), Ok(0));
    assert_eq!(counts(ctx.heap()), (1, 0, 0, 0));
    assert_eq!(ctx.lookup_steps(), 11);
}

#[test]
fn malformed_dimension_shapes_and_type_identities_precede_sign_inspection() {
    let (mut registry, u) = setup();
    let other = registry.create_loader().unwrap();
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (target, sizes) in [
        (T::Class(u.object), &[-1][..]),
        (array(1, A::Primitive(P::Int), u.bootstrap_loader), &[][..]),
        (
            array(1, A::Primitive(P::Int), u.bootstrap_loader),
            &[-1, -1][..],
        ),
        (
            array(0, A::Primitive(P::Int), u.bootstrap_loader),
            &[-1][..],
        ),
        (array(1, A::Primitive(P::Int), other), &[-1][..]),
    ] {
        assert!(
            matches!(ctx.allocate_multidimensional_array(target,sizes),Err(AE::Type(error)) if matches!(*error,E::InvalidType(_)))
        );
    }
    let huge = vec![-1; 65_536];
    let before = ctx.lookup_steps();
    assert!(
        matches!(ctx.allocate_multidimensional_array(array(255,A::Primitive(P::Int),u.bootstrap_loader),&huge),Err(AE::Type(error)) if matches!(*error,E::InvalidType(_)))
    );
    assert_eq!(ctx.lookup_steps() - before, 1);
    assert_eq!(counts(ctx.heap()), (0, 0, 0, 0));
}

#[test]
fn rank_255_full_and_partial_allocation_and_drop_are_iterative() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let root = ctx
        .allocate_multidimensional_array(
            array(255, A::Primitive(P::Int), u.bootstrap_loader),
            &[1; 255],
        )
        .unwrap();
    assert_eq!(ctx.lookup_steps(), 1025);
    let mut current = root;
    let mut seen = std::collections::HashSet::new();
    for rank in (2..=255).rev() {
        assert!(seen.insert(current));
        assert_eq!(
            ctx.heap().reference_array_type(current),
            Ok(array(rank, A::Primitive(P::Int), u.bootstrap_loader))
        );
        current = ctx.read_reference_array(current, 0).unwrap();
    }
    assert!(seen.insert(current));
    assert_eq!(seen.len(), 255);
    assert_eq!(ctx.heap().load(current, 0), Ok(Value::Int(0)));
    assert_eq!(counts(ctx.heap()), (255, 255, 254 * WIDTH + 4, 0));
    let partial = ctx
        .allocate_multidimensional_array(array(255, A::Class(u.object), u.bootstrap_loader), &[1])
        .unwrap();
    assert_eq!(ctx.read_reference_array(partial, 0), Ok(Reference::null()));
    drop(ctx);
    drop(heap);
}

#[test]
fn quota_precedence_is_per_reachable_level_and_failures_publish_nothing() {
    let (registry, u) = setup();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
    ];
    for (options, resource, limit, required) in [
        (
            HeapOptions {
                max_arrays: 0,
                max_array_length: 0,
                ..HeapOptions::default()
            },
            HeapResource::Arrays,
            0,
            1,
        ),
        (
            HeapOptions {
                max_arrays: 1,
                max_array_length: 1,
                ..HeapOptions::default()
            },
            HeapResource::ArrayLength,
            1,
            2,
        ),
        (
            HeapOptions {
                max_arrays: 2,
                max_array_length: 2,
                ..HeapOptions::default()
            },
            HeapResource::Arrays,
            2,
            3,
        ),
        (
            HeapOptions {
                max_total_elements: 1,
                max_payload_bytes: 0,
                ..HeapOptions::default()
            },
            HeapResource::TotalElements,
            1,
            2,
        ),
        (
            HeapOptions {
                max_arrays: 1,
                max_payload_bytes: WIDTH,
                ..HeapOptions::default()
            },
            HeapResource::PayloadBytes,
            WIDTH,
            2 * WIDTH,
        ),
        (
            HeapOptions {
                max_total_elements: 7,
                ..HeapOptions::default()
            },
            HeapResource::TotalElements,
            7,
            8,
        ),
        (
            HeapOptions {
                max_payload_bytes: 2 * WIDTH + 23,
                ..HeapOptions::default()
            },
            HeapResource::PayloadBytes,
            2 * WIDTH + 23,
            2 * WIDTH + 24,
        ),
    ] {
        let mut heap =
            ManagedHeap::new_with_instances(options, InstanceOptions::default()).unwrap();
        heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
            .unwrap();
        heap.bind_reference_types(&registry, u).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.allocate_multidimensional_array(
                array(2, A::Primitive(P::Int), u.bootstrap_loader),
                &[2, 3]
            ),
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
fn mixed_prior_allocations_share_exact_totals_and_inner_failure_preserves_old_graphs() {
    let (registry, u) = setup();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
    ];
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions {
            max_arrays: 5,
            max_array_length: 3,
            max_total_elements: 10,
            max_payload_bytes: 3 * WIDTH + 25,
        },
        InstanceOptions::default(),
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let primitive = heap.allocate_primitive_array(P::Byte, 1).unwrap();
    heap.store(primitive, 0, Value::Int(77)).unwrap();
    let object = heap.allocate_instance(u.object).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let existing = ctx.allocate_reference_array(T::Class(u.object), 1).unwrap();
    ctx.write_reference_array(existing, 0, object).unwrap();
    let before = counts(ctx.heap());
    let edges = children(ctx.heap(), existing);
    assert_eq!(
        ctx.allocate_multidimensional_array(
            array(2, A::Primitive(P::Int), u.bootstrap_loader),
            &[2, 4]
        ),
        Err(AE::Heap(HeapError::ResourceLimit {
            resource: HeapResource::ArrayLength,
            limit: 3,
            required: 4
        }))
    );
    assert_eq!(counts(ctx.heap()), before);
    assert_eq!(children(ctx.heap(), existing), edges);
    assert_eq!(ctx.heap().load(primitive, 0), Ok(Value::Int(77)));
    let root = ctx
        .allocate_multidimensional_array(
            array(2, A::Primitive(P::Int), u.bootstrap_loader),
            &[2, 3],
        )
        .unwrap();
    assert_eq!(counts(ctx.heap()), (5, 10, 3 * WIDTH + 25, 1));
    assert_eq!(children(ctx.heap(), root).len(), 2);
    assert_eq!(children(ctx.heap(), existing), edges);
    assert_eq!(
        ctx.allocate_multidimensional_array(
            array(1, A::Primitive(P::Byte), u.bootstrap_loader),
            &[0]
        ),
        Err(AE::Heap(HeapError::ResourceLimit {
            resource: HeapResource::Arrays,
            limit: 5,
            required: 6
        }))
    );
}

#[test]
fn exact_single_step_and_atomic_bulk_work_limits_keep_the_precharge_counter() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    for limit in [5, 6, 7, 8, 9, 10, 14, 15, usize::MAX] {
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
        let before = counts(ctx.heap());
        let result = ctx.allocate_multidimensional_array(
            array(2, A::Primitive(P::Int), u.bootstrap_loader),
            &[2, 3],
        );
        if limit < 15 {
            let required = if limit < 10 { limit + 1 } else { 15 };
            assert_eq!(
                result,
                Err(AE::Type(Box::new(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required
                }))))
            );
            assert_eq!(ctx.lookup_steps(), if limit < 10 { limit } else { 10 });
            assert_eq!(counts(ctx.heap()), before);
        } else {
            result.unwrap();
            assert_eq!(ctx.lookup_steps(), 15);
        }
    }
    for limit in [7, 8] {
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
        let result = ctx.allocate_multidimensional_array(
            array(2, A::Primitive(P::Int), u.bootstrap_loader),
            &[0, -1],
        );
        assert_eq!(
            result,
            Err(if limit == 7 {
                AE::Type(Box::new(E::Context(C::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit,
                    required: 8,
                })))
            } else {
                AE::Java(AJ::NegativeArraySize)
            })
        );
        assert_eq!(ctx.lookup_steps(), limit);
    }
}

#[test]
fn cp_resolution_access_rank_and_missing_targets_precede_negative_or_empty_counts() {
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
        "q/D",
        Some("java/lang/Object"),
        &[],
        0x21,
        &["[Lp/Hidden;", "[LMissing;", "java/lang/Object", "[[I"],
    );
    let mut heap = bound_heap(&registry, u, &[origin.class, hidden]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for sizes in [&[][..], &[-1][..]] {
        assert_eq!(
            ctx.new_multidimensional_array(origin.class, origin.targets[0], sizes),
            Err(AE::Java(AJ::IllegalAccess))
        );
        assert!(
            matches!(ctx.new_multidimensional_array(origin.class,origin.targets[1],sizes),Err(AE::Type(error)) if matches!(*error,E::Context(C::MissingDefinition {..})))
        );
        assert!(
            matches!(ctx.new_multidimensional_array(origin.class,origin.targets[2],sizes),Err(AE::Type(error)) if matches!(*error,E::InvalidType(_)))
        );
    }
    for index in [0, u16::MAX, origin.wrong] {
        assert_eq!(
            ctx.new_multidimensional_array(origin.class, index, &[-1]),
            Err(AE::Type(Box::new(E::InvalidConstant {
                origin: origin.class,
                index
            })))
        );
    }
    let root = ctx
        .new_multidimensional_array(origin.class, origin.targets[3], &[2, 1])
        .unwrap();
    assert_eq!(ctx.heap().length(root), Ok(2));
    let leaf = ctx.read_reference_array(root, 0).unwrap();
    assert_eq!(ctx.heap().kind(leaf), Ok(P::Int));
    let typed = ctx
        .allocate_multidimensional_array(array(2, A::Class(hidden), l), &[1, 1])
        .unwrap();
    assert_eq!(
        ctx.heap().reference_array_type(typed),
        Ok(array(2, A::Class(hidden), l))
    );
}

#[test]
fn delegated_nominal_leaf_binding_and_cached_name_work_are_shared_with_cp_allocation() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let target = define(
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
        &["[LRemote;"],
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .aliases
        .insert((u.bootstrap_loader, units("Remote")), target);
    let roots = [u.cloneable, u.serializable, target, origin.class];
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
                ctx.new_multidimensional_array(origin.class, origin.targets[0], &[0]),
                Err(AE::Type(Box::new(E::Context(error))))
            );
            assert_eq!(calls.get(), 0);
        } else {
            for _ in 0..2 {
                let before = ctx.lookup_steps();
                let root = ctx
                    .new_multidimensional_array(origin.class, origin.targets[0], &[0])
                    .unwrap();
                assert_eq!(
                    ctx.heap().reference_array_type(root),
                    Ok(array(1, A::Class(target), second))
                );
                assert_eq!(ctx.lookup_steps() - before, 15);
            }
            assert_eq!(calls.get(), 1);
        }
    }
}

#[test]
fn asymmetric_wide_tree_has_unique_child_order_and_usable_nested_payloads() {
    let (registry, u) = setup();
    let mut heap = bound_heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let root = ctx
        .allocate_multidimensional_array(
            array(3, A::Primitive(P::Int), u.bootstrap_loader),
            &[3, 4, 2],
        )
        .unwrap();
    let mut seen = std::collections::HashSet::from([root]);
    for outer in 0..3 {
        let middle = ctx.read_reference_array(root, outer).unwrap();
        assert!(seen.insert(middle));
        assert_eq!(ctx.heap().length(middle), Ok(4));
        for inner in 0..4 {
            let leaf = ctx.read_reference_array(middle, inner).unwrap();
            assert!(seen.insert(leaf));
            ctx.heap_mut()
                .store(leaf, 0, Value::Int(100 * outer + inner))
                .unwrap();
        }
    }
    assert_eq!(seen.len(), 16);
    for outer in 0..3 {
        let middle = ctx.read_reference_array(root, outer).unwrap();
        for inner in 0..4 {
            let leaf = ctx.read_reference_array(middle, inner).unwrap();
            assert_eq!(
                ctx.heap().load(leaf, 0),
                Ok(Value::Int(100 * outer + inner))
            );
        }
    }
    assert_eq!(counts(ctx.heap()), (16, 39, 15 * WIDTH + 24 * 4, 0));
}

#[test]
fn universe_unadmitted_identity_and_context_replacement_are_checked_before_geometry() {
    let (mut registry, u) = setup();
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
    let mut replacement = bound_heap(&registry, u, &[]);
    let mut heap = heap(&registry, u, &[]);
    let mut resolver = SameLoaderResolver;
    {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert_eq!(
            ctx.allocate_multidimensional_array(
                array(1, A::Primitive(P::Int), u.bootstrap_loader),
                &[-1]
            ),
            Err(AE::Type(Box::new(E::MissingUniverse)))
        );
        ctx.heap_mut().bind_reference_types(&registry, u).unwrap();
        assert_eq!(
            ctx.allocate_multidimensional_array(
                array(1, A::Primitive(P::Int), u.bootstrap_loader),
                &[0]
            ),
            Err(AE::Type(Box::new(E::Context(C::ContextChanged))))
        );
    }
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.allocate_multidimensional_array(array(1, A::Class(missing), u.bootstrap_loader), &[-1]),
        Err(AE::Type(Box::new(E::Context(C::Instance(
            InstanceError::LayoutMissing { class: missing }
        )))))
    );
    let (_, foreign) = setup();
    assert!(
        matches!(ctx.allocate_multidimensional_array(array(1,A::Class(foreign.object),foreign.bootstrap_loader),&[-1]),Err(AE::Type(error)) if matches!(*error,E::Context(C::Registry(_))))
    );
    let root = ctx
        .allocate_multidimensional_array(
            array(2, A::Primitive(P::Int), u.bootstrap_loader),
            &[1, 1],
        )
        .unwrap();
    assert!(ctx.read_reference_array(root, 0).is_ok());
    std::mem::swap(ctx.heap_mut(), &mut replacement);
    assert_eq!(
        ctx.allocate_multidimensional_array(
            array(1, A::Primitive(P::Int), u.bootstrap_loader),
            &[-1]
        ),
        Err(AE::Type(Box::new(E::Context(C::ContextChanged))))
    );
}

#[test]
fn maximum_policy_values_do_not_cause_eager_allocation_and_instance_quotas_are_independent() {
    let (registry, u) = setup();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
    ];
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions {
            max_arrays: usize::MAX,
            max_array_length: usize::MAX,
            max_total_elements: usize::MAX,
            max_payload_bytes: usize::MAX,
        },
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
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let root = ctx
        .allocate_multidimensional_array(
            array(2, A::Primitive(P::Double), u.bootstrap_loader),
            &[2, 2],
        )
        .unwrap();
    assert_eq!(ctx.heap().length(root), Ok(2));
    assert_eq!(counts(ctx.heap()), (3, 6, 2 * WIDTH + 4 * 8, 0));
}

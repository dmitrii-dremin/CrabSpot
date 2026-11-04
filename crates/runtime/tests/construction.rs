//! Public checked metadata/memory contract, independent of constructor execution.
mod rt {
    pub use crabspot_runtime::*;
}
#[allow(dead_code)]
#[path = "support/construction.rs"]
mod support;
use ConstructionAccessError as E;
use ConstructionJavaError as J;
use ConstructorFieldReceiverKind as F;
use ConstructorReceiverKind as K;
use rt::*;
use std::{cell::Cell, collections::HashMap, rc::Rc};
use support::*;

fn claim(ctx: &mut FieldResolutionContext<'_>, w: &World) -> InitializationClaim {
    let (statics, exceptions) = profiles(ctx, w.throwable);
    ctx.claim_initialization(&statics, &exceptions, w.error)
        .unwrap()
}
fn method(w: &World, class: ClassId, name: &str, descriptor: &str) -> MethodId {
    w.registry
        .find_declared_method(class, &units(name), &units(descriptor))
        .unwrap()
        .unwrap()
}
fn configured_heap(w: &World, roots: &[ClassId], options: InstanceOptions) -> ManagedHeap {
    let plans: Vec<_> = roots
        .iter()
        .chain([&w.u.cloneable, &w.u.serializable, &w.error])
        .map(|class| {
            build_hierarchy(
                &w.registry,
                *class,
                &mut SameLoaderResolver,
                HierarchyOptions {
                    max_classes: 20_000,
                    max_edges: 40_000,
                    ..Default::default()
                },
            )
            .unwrap()
        })
        .collect();
    let mut heap = ManagedHeap::new_with_instances(HeapOptions::default(), options).unwrap();
    heap.admit_instance_layouts(&w.registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&w.registry, w.u).unwrap();
    heap
}
fn options_context<'a>(
    w: &'a World,
    heap: &'a mut ManagedHeap,
    resolver: &'a mut impl HierarchyResolver,
    options: FieldResolutionOptions,
) -> FieldResolutionContext<'a> {
    FieldResolutionContext::new_unnamed_modules(&w.registry, heap, resolver, options).unwrap()
}
fn simple(w: &mut World, name: &str, parent: &str, flags: u16) -> Def {
    define(
        &mut w.registry,
        w.u.bootstrap_loader,
        name,
        Some(parent),
        &[],
        flags,
        &[],
        &[
            Method::new("<init>", "()V", 1),
            Method::new("body", "()V", 9),
        ],
        &[],
        &[],
        65,
    )
}

#[test]
fn new_resolves_without_allocation_and_resumption_only_charges_layout_defaults() {
    let mut w = world();
    let base = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("x", "J", 1, None)],
        &[],
        &[],
        &[],
        65,
    );
    let child = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[
            Field::new("flag", "Z", 17, None),
            Field::new("ref", "Ljava/lang/Object;", 1, None),
        ],
        &[],
        &[],
        &["Child"],
        65,
    );
    let mut heap = configured_heap(&w, &[child.class], InstanceOptions::default());
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let before = ctx.lookup_steps();
    let access = ctx
        .resolve_new(&claim, child.class, child.classes[0])
        .unwrap();
    assert_eq!(access.class(), child.class);
    assert_eq!(ctx.lookup_steps() - before, "Child".len() + 1); // name + nominal node
    assert_eq!(ctx.heap().allocated_instances(), 0);
    let resolved = ctx.lookup_steps();
    let first = ctx.allocate_new(&claim, &access).unwrap();
    let second = ctx.allocate_new(&claim, &access).unwrap();
    assert_ne!(first, second);
    assert_eq!(ctx.lookup_steps() - resolved, 12); // 3 shapes + 3 fields per allocation
    assert_eq!(ctx.heap().allocated_instances(), 2);
    assert_eq!(ctx.heap().total_instance_slots(), 6);
    assert_eq!(
        ctx.heap().get_field(first, field(&w, base.class, "x", "J")),
        Ok(Value::Long(0))
    );
    assert_eq!(
        ctx.heap()
            .get_field(first, field(&w, child.class, "ref", "Ljava/lang/Object;")),
        Ok(Value::Reference(Reference::null()))
    );
}

#[test]
fn new_class_access_precedes_instantiation_and_arrays_fail_before_name_work() {
    let mut w = world();
    let abs = simple(&mut w, "p/Abstract", "java/lang/Object", 0x421);
    let hidden = simple(&mut w, "p/Hidden", "java/lang/Object", 0x420);
    let interface = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "p/I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
        &[],
        &[],
        65,
    );
    let caller = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "q/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        &["p/Abstract", "p/Hidden", "p/I", "[I"],
        65,
    );
    let mut heap = configured_heap(
        &w,
        &[abs.class, hidden.class, interface.class, caller.class],
        InstanceOptions::default(),
    );
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    for index in [0, 2] {
        assert_eq!(
            ctx.resolve_new(&claim, caller.class, caller.classes[index])
                .unwrap_err(),
            E::Java(J::Instantiation)
        );
    }
    assert_eq!(
        ctx.resolve_new(&claim, caller.class, caller.classes[1])
            .unwrap_err(),
        E::Java(J::IllegalAccess)
    );
    let before = ctx.lookup_steps();
    for index in [0, u16::MAX, caller.classes[3]] {
        assert!(matches!(
            ctx.resolve_new(&claim, caller.class, index),
            Err(E::InvalidMetadata(_))
        ));
    }
    assert_eq!(ctx.lookup_steps(), before);
    assert_eq!(ctx.heap().allocated_instances(), 0);
}

#[test]
fn constructor_shape_is_compact_and_defers_array_owner_to_resolution() {
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("body", "()V", 9)],
        &[
            ("C", "<init>", "(IJFDLjava/lang/Object;[I)V", 10),
            ("[I", "<init>", "()V", 10),
            ("C", "body", "()V", 10),
            ("C", "body", "()V", 11),
        ],
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let before = ctx.lookup_steps();
    let shape = ctx
        .constructor_invocation_shape(own.class, own.refs[0])
        .unwrap();
    assert_eq!(
        shape.parameters,
        [
            ValueKind::Int,
            ValueKind::Long,
            ValueKind::Float,
            ValueKind::Double,
            ValueKind::Reference,
            ValueKind::Reference
        ]
    );
    assert_eq!(shape.parameter_slots, 8);
    assert_eq!(shape.result, MethodReturnKind::Void);
    assert_eq!(
        ctx.lookup_steps() - before,
        "(IJFDLjava/lang/Object;[I)V".len()
    );
    assert!(
        ctx.constructor_invocation_shape(own.class, own.refs[1])
            .is_ok()
    );
    assert!(
        matches!(ctx.resolve_constructor(&claim, own.methods[0], own.refs[1], Reference::null(), K::Allocation),
        Err(E::Method(error)) if matches!(*error, MethodAccessError::Context(FieldAccessError::UnsupportedArrayOwner)))
    );
    for index in [0, u16::MAX, own.refs[2], own.refs[3]] {
        assert!(
            matches!(ctx.constructor_invocation_shape(own.class, index), Err(E::Method(error)) if matches!(*error, MethodAccessError::InvalidConstant { .. }))
        );
    }
}

#[test]
fn inherited_constructor_access_failure_precedes_owner_mismatch_and_never_executes_ancestor() {
    for flags in [1, 2, 4, 0] {
        let mut w = world();
        let base = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "p/Base",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[Method::new("<init>", "(I)V", flags)],
            &[],
            &[],
            65,
        );
        let child = simple(&mut w, "p/Child", "p/Base", 0x21);
        let caller = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "q/Maker",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[Method::new("body", "()V", 9)],
            &[("p/Child", "<init>", "(I)V", 10)],
            &[],
            65,
        );
        let mut heap =
            configured_heap(&w, &[child.class, caller.class], InstanceOptions::default());
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let claim = claim(&mut ctx, &w);
        let error = ctx
            .resolve_constructor(
                &claim,
                caller.methods[0],
                caller.refs[0],
                Reference::null(),
                K::Allocation,
            )
            .unwrap_err();
        match flags {
            1 => assert_eq!(error, E::Java(J::NoSuchMethod)),
            2 => assert!(
                matches!(error, E::Method(error) if *error == MethodAccessError::UnsupportedNestAccess { origin: caller.class, owner: base.class })
            ),
            _ => assert_eq!(error, E::Java(J::IllegalAccess)),
        }
        assert_eq!(ctx.heap().allocated_instances(), 0);
    }
}

#[test]
fn constructor_owner_modes_use_exact_allocation_and_direct_this_super_relations() {
    for flags in [1, 0x21] {
        // no ACC_SUPER-dependent redirection
        let mut w = world();
        let base = simple(&mut w, "Base", "java/lang/Object", 0x21);
        let middle = simple(&mut w, "Middle", "Base", 0x21);
        let child = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "Child",
            Some("Middle"),
            &[],
            flags,
            &[],
            &[
                Method::new("<init>", "()V", 1),
                Method::new("body", "()V", 9),
            ],
            &[
                ("Child", "<init>", "()V", 10),
                ("Middle", "<init>", "()V", 10),
                ("Base", "<init>", "()V", 10),
            ],
            &[],
            65,
        );
        let mut heap = configured_heap(&w, &[child.class], InstanceOptions::default());
        let receiver = heap.allocate_instance(child.class).unwrap();
        let wrong = heap.allocate_instance(base.class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let claim = claim(&mut ctx, &w);
        for (index, expected) in [(0, child.methods[0]), (1, middle.methods[0])] {
            assert_eq!(
                ctx.resolve_constructor(
                    &claim,
                    child.methods[0],
                    child.refs[index],
                    receiver,
                    K::This
                )
                .unwrap()
                .method(),
                expected
            );
        }
        assert_eq!(
            ctx.resolve_constructor(&claim, child.methods[0], child.refs[2], receiver, K::This)
                .unwrap_err(),
            E::InvalidConstructorOwner {
                origin: child.methods[0],
                owner: base.class
            }
        );
        assert_eq!(
            ctx.resolve_constructor(&claim, child.methods[1], child.refs[1], receiver, K::This)
                .unwrap_err(),
            E::InvalidReceiverKind
        );
        assert_eq!(
            ctx.resolve_constructor(
                &claim,
                child.methods[1],
                child.refs[1],
                receiver,
                K::Allocation
            )
            .unwrap_err(),
            E::InvalidConstructorReceiver {
                expected: middle.class,
                actual: child.class
            }
        );
        assert_eq!(
            ctx.resolve_constructor(&claim, child.methods[0], child.refs[0], wrong, K::This)
                .unwrap_err(),
            E::InvalidConstructorReceiver {
                expected: child.class,
                actual: base.class
            }
        );
        assert_eq!(
            ctx.resolve_constructor(
                &claim,
                child.methods[0],
                child.refs[0],
                Reference::null(),
                K::Allocation
            )
            .unwrap_err(),
            E::Java(J::NullPointer)
        );
    }
}

#[test]
fn protected_constructor_allocation_is_stricter_than_class_assignment() {
    let mut w = world();
    let base = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "p/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("<init>", "()V", 4)],
        &[],
        &[],
        65,
    );
    let child = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "q/Child",
        Some("p/Base"),
        &[],
        0x21,
        &[],
        &[Method::new("<init>", "()V", 1)],
        &[("p/Base", "<init>", "()V", 10)],
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[child.class], InstanceOptions::default());
    let allocated_base = heap.allocate_instance(base.class).unwrap();
    let allocated_child = heap.allocate_instance(child.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    assert_eq!(
        ctx.resolve_constructor(
            &claim,
            child.methods[0],
            child.refs[0],
            allocated_base,
            K::Allocation
        )
        .unwrap_err(),
        E::InvalidConstructorReceiver {
            expected: child.class,
            actual: base.class
        }
    );
    assert_eq!(
        ctx.resolve_constructor(
            &claim,
            child.methods[0],
            child.refs[0],
            allocated_child,
            K::This
        )
        .unwrap()
        .method(),
        base.methods[0]
    );
}

#[test]
fn final_own_field_authority_allows_other_ordinary_receivers_and_repeated_writes() {
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("x", "I", 17, None)],
        &[Method::new("<init>", "()V", 1)],
        &[("C", "<init>", "()V", 10), ("C", "x", "I", 9)],
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let other = heap.allocate_instance(own.class).unwrap();
    let x = field(&w, own.class, "x", "I");
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let ctor = ctx
        .resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
        .unwrap();
    for value in [7, 19] {
        ctx.write_constructor_field(
            &claim,
            &ctor,
            own.refs[1],
            other,
            Value::Int(value),
            F::Ordinary,
        )
        .unwrap();
        assert_eq!(ctx.heap().get_field(other, x), Ok(Value::Int(value)));
    }
    assert_eq!(ctx.heap().get_field(receiver, x), Ok(Value::Int(0)));
    assert_eq!(
        ctx.write_field(own.class, own.refs[1], receiver, Value::Int(3)),
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    );
    assert_eq!(
        ctx.heap_mut().put_field(other, x, Value::Int(3)),
        Err(InstanceError::FinalField { field: x })
    );
}

#[test]
fn early_fields_require_bound_this_and_exact_symbolic_and_declared_owners() {
    let mut w = world();
    let base = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("inherited", "I", 1, None),
            Field::new("final", "I", 17, None),
        ],
        &[],
        &[],
        &[],
        65,
    );
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("Base"),
        &[],
        0x21,
        &[
            Field::new("x", "I", 17, None),
            Field::new("s", "I", 9, None),
        ],
        &[Method::new("<init>", "()V", 1)],
        &[
            ("C", "<init>", "()V", 10),
            ("C", "x", "I", 9),
            ("Alias", "x", "I", 9),
            ("C", "inherited", "I", 9),
            ("Base", "inherited", "I", 9),
            ("C", "final", "I", 9),
            ("C", "s", "I", 9),
        ],
        &[],
        65,
    );
    let alias = simple(&mut w, "Alias", "C", 0x21);
    let mut heap = configured_heap(&w, &[alias.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let other = heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let ctor = ctx
        .resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
        .unwrap();
    ctx.write_constructor_field(
        &claim,
        &ctor,
        own.refs[1],
        receiver,
        Value::Int(5),
        F::UninitializedThis,
    )
    .unwrap();
    for (index, owner) in [(2, alias.class), (3, base.class), (4, base.class)] {
        assert_eq!(
            ctx.write_constructor_field(
                &claim,
                &ctor,
                own.refs[index],
                receiver,
                Value::Int(8),
                F::UninitializedThis
            ),
            Err(E::InvalidConstructorOwner {
                origin: own.methods[0],
                owner
            })
        );
    }
    assert_eq!(
        ctx.write_constructor_field(
            &claim,
            &ctor,
            own.refs[1],
            other,
            Value::Int(8),
            F::UninitializedThis
        ),
        Err(E::InvalidReceiverKind)
    );
    assert_eq!(
        ctx.write_constructor_field(
            &claim,
            &ctor,
            own.refs[5],
            Reference::null(),
            Value::Int(8),
            F::UninitializedThis
        ),
        Err(E::Java(J::IllegalAccess))
    );
    assert_eq!(
        ctx.write_constructor_field(
            &claim,
            &ctor,
            own.refs[6],
            Reference::null(),
            Value::Int(8),
            F::UninitializedThis
        ),
        Err(E::Java(J::IncompatibleClassChange))
    );
    assert_eq!(
        ctx.heap()
            .get_field(receiver, field(&w, own.class, "x", "I")),
        Ok(Value::Int(5))
    );
    assert_eq!(
        ctx.heap()
            .get_field(receiver, field(&w, base.class, "inherited", "I")),
        Ok(Value::Int(0))
    );
}

#[test]
fn constructor_stores_preserve_subword_rules_raw_float_bits_and_typed_children() {
    let mut w = world();
    let specs = [
        ("z", "Z"),
        ("b", "B"),
        ("c", "C"),
        ("s", "S"),
        ("i", "I"),
        ("j", "J"),
        ("f", "F"),
        ("d", "D"),
        ("r", "Ljava/lang/Object;"),
        ("a", "[I"),
    ];
    let fields: Vec<_> = specs
        .iter()
        .map(|(n, d)| Field::new(n, d, 17, None))
        .collect();
    let mut refs = vec![("C", "<init>", "()V", 10)];
    refs.extend(specs.iter().map(|(n, d)| ("C", *n, *d, 9)));
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &[Method::new("<init>", "()V", 1)],
        &refs,
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let child = heap.allocate_instance(own.class).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    let inputs = [
        Value::Int(-3),
        Value::Int(0x180),
        Value::Int(-1),
        Value::Int(0x18000),
        Value::Int(i32::MIN),
        Value::Long(i64::MAX),
        Value::Float(0x7fa1_2345),
        Value::Double(0x8000_0000_0000_0000),
        Value::Reference(child),
        Value::Reference(array),
    ];
    let expected = [
        Value::Int(1),
        Value::Int(-128),
        Value::Int(65535),
        Value::Int(-32768),
        inputs[4],
        inputs[5],
        inputs[6],
        inputs[7],
        inputs[8],
        inputs[9],
    ];
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let ctor = ctx
        .resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
        .unwrap();
    for (index, (input, expected)) in inputs.into_iter().zip(expected).enumerate() {
        ctx.write_constructor_field(
            &claim,
            &ctor,
            own.refs[index + 1],
            receiver,
            input,
            F::UninitializedThis,
        )
        .unwrap();
        assert_eq!(
            ctx.heap().get_field(
                receiver,
                field(&w, own.class, specs[index].0, specs[index].1)
            ),
            Ok(expected)
        );
    }
    assert_eq!(
        ctx.heap().children(receiver).unwrap().collect::<Vec<_>>(),
        [child.heap_ref().unwrap(), array.heap_ref().unwrap()]
    );
    ctx.write_constructor_field(
        &claim,
        &ctor,
        own.refs[9],
        receiver,
        Value::Reference(Reference::null()),
        F::Ordinary,
    )
    .unwrap();
    assert_eq!(
        ctx.heap().children(receiver).unwrap().collect::<Vec<_>>(),
        [array.heap_ref().unwrap()]
    );
    assert_eq!(ctx.heap().allocated_instances(), 2);
    assert_eq!(ctx.heap().allocated_arrays(), 1);
}

#[test]
fn late_reference_failures_and_raw_refusal_preserve_prior_values_and_children() {
    let mut w = world();
    let other = simple(&mut w, "Other", "java/lang/Object", 0x21);
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("ref", "LC;", 17, None),
            Field::new("plain", "LC;", 1, None),
        ],
        &[Method::new("<init>", "()V", 1)],
        &[
            ("C", "<init>", "()V", 10),
            ("C", "ref", "LC;", 9),
            ("C", "plain", "LC;", 9),
        ],
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class, other.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let valid = heap.allocate_instance(own.class).unwrap();
    let wrong = heap.allocate_instance(other.class).unwrap();
    let mut foreign_heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let foreign = foreign_heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let ctor = ctx
        .resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
        .unwrap();
    ctx.write_constructor_field(
        &claim,
        &ctor,
        own.refs[1],
        receiver,
        Value::Reference(valid),
        F::Ordinary,
    )
    .unwrap();
    for input in [
        Value::Reference(wrong),
        Value::Reference(foreign),
        Value::Int(4),
    ] {
        let error = ctx
            .write_constructor_field(&claim, &ctor, own.refs[1], receiver, input, F::Ordinary)
            .unwrap_err();
        assert!(matches!(error, E::Field(_) | E::Instance(_)));
        assert_eq!(
            ctx.heap()
                .get_field(receiver, field(&w, own.class, "ref", "LC;")),
            Ok(Value::Reference(valid))
        );
        assert_eq!(
            ctx.heap().children(receiver).unwrap().collect::<Vec<_>>(),
            [valid.heap_ref().unwrap()]
        );
    }
    let plain = field(&w, own.class, "plain", "LC;");
    assert_eq!(
        ctx.heap_mut()
            .put_field(receiver, plain, Value::Reference(valid)),
        Err(InstanceError::UnsupportedReferenceStore { field: plain })
    );
    assert_eq!(
        ctx.heap().get_field(receiver, plain),
        Ok(Value::Reference(Reference::null()))
    );
    assert_eq!(ctx.heap().allocated_instances(), 3);
}

#[test]
fn layout_work_is_incremental_inclusive_and_spent_before_quota_failure() {
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("i", "I", 1, None), Field::new("j", "J", 1, None)],
        &[],
        &[],
        &["C"],
        65,
    );
    for budget in [0, 1, 2, 3, 4, usize::MAX] {
        let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let seed = ctx.lookup_steps();
        let claim = claim(&mut ctx, &w);
        let before = ctx.lookup_steps();
        ctx.resolve_new(&claim, own.class, own.classes[0]).unwrap();
        assert_eq!(ctx.lookup_steps() - before, 2);
        let baseline = seed + 2;
        drop(ctx);
        // Constructor seeding consumes identical work; choose the exact remaining
        // allowance after the already-funded name/class service.
        let mut limited = options_context(
            &w,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: if budget == usize::MAX {
                    usize::MAX
                } else {
                    baseline + budget
                },
                ..Default::default()
            },
        );
        let seed = limited.lookup_steps();
        // Work is context-local, so first replay resolve_new to the same baseline.
        let access = limited
            .resolve_new(&claim, own.class, own.classes[0])
            .unwrap();
        assert_eq!(limited.lookup_steps(), baseline);
        let result = limited.allocate_new(&claim, &access);
        if budget < 4 {
            assert!(
                matches!(result,Err(E::Field(error)) if matches!(*error,FieldAccessError::ResourceLimit{resource:FieldResource::LookupSteps,limit,required} if limit==baseline+budget && required==limit+1))
            );
            assert_eq!(limited.lookup_steps(), baseline + budget);
            assert_eq!(limited.heap().allocated_instances(), 0);
        } else {
            assert!(result.is_ok());
            assert_eq!(limited.lookup_steps() - baseline, 4);
        }
        assert!(seed <= baseline);
        let _ = access;
    }
    for (instances, per, total, expected) in [
        (0, 2, 2, InstanceResource::Instances),
        (1, 1, 2, InstanceResource::InstanceSlots),
        (1, 2, 1, InstanceResource::TotalInstanceSlots),
    ] {
        let mut heap = configured_heap(
            &w,
            &[own.class],
            InstanceOptions {
                max_instances: instances,
                max_instance_slots: per,
                max_total_instance_slots: total,
                ..Default::default()
            },
        );
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let claim = claim(&mut ctx, &w);
        let access = ctx.resolve_new(&claim, own.class, own.classes[0]).unwrap();
        let before = ctx.lookup_steps();
        assert!(
            matches!(ctx.allocate_new(&claim,&access),Err(E::Instance(error)) if matches!(*error,InstanceError::ResourceLimit{resource,..} if resource==expected))
        );
        assert_eq!(ctx.lookup_steps() - before, 4);
        assert_eq!(ctx.heap().allocated_instances(), 0);
        assert_eq!(ctx.heap().total_instance_slots(), 0);
    }
}

#[test]
fn constructor_resolution_repeats_full_signature_charge_without_override_audit() {
    let mut w = world();
    let desc = "(Ljava/lang/Object;)V";
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("<init>", desc, 1)],
        &[("C", "<init>", desc, 10)],
        &[],
        65,
    );
    let child = simple(&mut w, "Child", "C", 0x21);
    let mut heap = configured_heap(&w, &[child.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let before = ctx.lookup_steps();
    for _ in 0..2 {
        let ctor = ctx
            .resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
            .unwrap();
        assert_eq!(ctor.method(), own.methods[0]);
    }
    assert_eq!(ctx.lookup_steps() - before, 2 * (desc.len() + 2));
    assert_eq!(ctx.heap().allocated_instances(), 1);
    let before = ctx.lookup_steps();
    ctx.check_method_arguments(own.methods[0], &[Value::Reference(Reference::null())])
        .unwrap();
    ctx.check_method_return(own.methods[0], None).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 2 * desc.len());
}

#[test]
fn base_object_validation_is_lazy_and_follows_receiver_relations() {
    for object_flags in [0x21, 0x421] {
        let mut w = world_with_object(object_flags);
        let own = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[Method::new("<init>", "()V", 1)],
            &[("java/lang/Object", "<init>", "()V", 10)],
            &[],
            65,
        );
        let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
        let receiver = heap.allocate_instance(own.class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let claim = claim(&mut ctx, &w); // no eager concrete Object requirement
        assert_eq!(
            ctx.resolve_constructor(
                &claim,
                own.methods[0],
                own.refs[0],
                Reference::null(),
                K::This
            )
            .unwrap_err(),
            E::Java(J::NullPointer)
        );
        let result =
            ctx.resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::This);
        if object_flags & 0x400 != 0 {
            assert!(matches!(result, Err(E::InvalidMetadata(_))));
        } else {
            assert_eq!(
                result.unwrap().method(),
                method(&w, w.u.object, "<init>", "()V")
            );
        }
    }
}

#[derive(Default)]
struct Resolver {
    aliases: HashMap<(LoaderId, Vec<u16>), ClassId>,
    calls: Rc<Cell<usize>>,
    fail: Option<ResolveError>,
}
impl HierarchyResolver for Resolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        self.calls.set(self.calls.get() + 1);
        if let Some(error) = &self.fail {
            return Err(error.clone());
        }
        match self.aliases.get(&(loader, name.to_vec())) {
            Some(class) => Ok(Some(*class)),
            None => registry
                .lookup_class(loader, name)
                .map_err(|e| ResolveError::new(e.to_string())),
        }
    }
}
fn delegated_heap(
    w: &World,
    roots: &[ClassId],
    resolver: &mut impl HierarchyResolver,
) -> ManagedHeap {
    let plans: Vec<_> = roots
        .iter()
        .chain([&w.u.cloneable, &w.u.serializable, &w.error])
        .map(|class| {
            build_hierarchy(&w.registry, *class, resolver, HierarchyOptions::default()).unwrap()
        })
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&w.registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&w.registry, w.u).unwrap();
    heap
}

#[test]
fn delegated_new_checks_symbol_quotas_before_callback_and_caches_only_success() {
    let mut w = world();
    let second = w.registry.create_loader().unwrap();
    let target = define(
        &mut w.registry,
        second,
        "Remote",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        &[],
        65,
    );
    let caller = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Maker",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        &["Remote", "Remote", "Missing"],
        65,
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((second, units("java/lang/Object")), w.u.object);
    resolver
        .aliases
        .insert((w.u.bootstrap_loader, units("Remote")), target.class);
    let mut heap = delegated_heap(&w, &[target.class, caller.class], &mut resolver);
    let calls = resolver.calls.clone();
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let symbols = ctx.resolved_symbols();
    let name_units = ctx.symbol_units();
    drop(ctx);
    for (symbol_extra, unit_extra) in [(0, 6), (1, 5), (1, 6)] {
        calls.set(0);
        let mut ctx = options_context(
            &w,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_symbols: symbols + symbol_extra,
                max_symbol_units: name_units + unit_extra,
                ..Default::default()
            },
        );
        let result = ctx.resolve_new(&claim, caller.class, caller.classes[0]);
        if symbol_extra == 0 || unit_extra < 6 {
            assert!(
                matches!(result,Err(E::Type(error)) if matches!(*error,TypeAccessError::Context(FieldAccessError::ResourceLimit{..})))
            );
            assert_eq!(calls.get(), 0);
            assert_eq!(ctx.resolved_symbols(), symbols);
        } else {
            assert_eq!(result.unwrap().class(), target.class);
            assert_eq!(
                ctx.resolve_new(&claim, caller.class, caller.classes[1])
                    .unwrap()
                    .class(),
                target.class
            );
            assert_eq!(calls.get(), 1);
            assert_eq!(ctx.resolved_symbols(), symbols + 1);
        }
    }
    calls.set(0);
    let mut ctx = context(&w, &mut heap, &mut resolver);
    for _ in 0..2 {
        assert!(
            matches!(ctx.resolve_new(&claim,caller.class,caller.classes[2]),Err(E::Type(error)) if matches!(*error,TypeAccessError::Context(FieldAccessError::MissingDefinition{..})))
        );
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(ctx.resolved_symbols(), symbols);
    drop(ctx);
    resolver.fail = Some(ResolveError::new("definition temporarily unavailable"));
    let mut ctx = context(&w, &mut heap, &mut resolver);
    assert!(
        matches!(ctx.resolve_new(&claim,caller.class,caller.classes[0]),Err(E::Type(error)) if matches!(*error,TypeAccessError::Context(FieldAccessError::Resolve{..})))
    );
    assert_eq!(ctx.resolved_symbols(), symbols);
}

#[test]
fn nominal_constructor_loading_constraint_precedes_null_and_owner_mismatch() {
    for desc in ["(Ljava/lang/Object;)V", "([I)V"] {
        let mut w = world();
        let second = w.registry.create_loader().unwrap();
        let base = define(
            &mut w.registry,
            second,
            "Base",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[Method::new("<init>", desc, 1)],
            &[],
            &[],
            65,
        );
        let child = define(
            &mut w.registry,
            second,
            "Child",
            Some("Base"),
            &[],
            0x21,
            &[],
            &[],
            &[],
            &[],
            65,
        );
        let caller = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "Maker",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[Method::new("body", "()V", 9)],
            &[("Base", "<init>", desc, 10), ("Child", "<init>", desc, 10)],
            &[],
            65,
        );
        let mut resolver = Resolver::default();
        resolver
            .aliases
            .insert((second, units("java/lang/Object")), w.u.object);
        resolver
            .aliases
            .insert((w.u.bootstrap_loader, units("Base")), base.class);
        resolver
            .aliases
            .insert((w.u.bootstrap_loader, units("Child")), child.class);
        let mut heap = delegated_heap(&w, &[child.class, caller.class], &mut resolver);
        let receiver = heap.allocate_instance(base.class).unwrap();
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let claim = claim(&mut ctx, &w);
        for index in 0..2 {
            let result = ctx.resolve_constructor(
                &claim,
                caller.methods[0],
                caller.refs[index],
                Reference::null(),
                K::Allocation,
            );
            if desc.contains('L') {
                assert!(
                    matches!(result,Err(E::Method(error)) if *error==MethodAccessError::UnsupportedLoadingConstraint{origin:caller.class,method:base.methods[0]})
                );
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    E::Java(if index == 0 {
                        J::NullPointer
                    } else {
                        J::NoSuchMethod
                    })
                );
            }
        }
        if !desc.contains('L') {
            assert_eq!(
                ctx.resolve_constructor(
                    &claim,
                    caller.methods[0],
                    caller.refs[0],
                    receiver,
                    K::Allocation
                )
                .unwrap()
                .method(),
                base.methods[0]
            );
        }
    }
}

#[test]
fn foreign_receivers_claims_and_same_size_heap_replacement_remain_host_failures() {
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("<init>", "()V", 1)],
        &[("C", "<init>", "()V", 10)],
        &["C"],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let mut replacement = configured_heap(&w, &[own.class], InstanceOptions::default());
    let foreign = replacement.allocate_instance(own.class).unwrap();
    let mut other_resolver = SameLoaderResolver;
    let mut other_ctx = context(&w, &mut replacement, &mut other_resolver);
    let foreign_claim = claim(&mut other_ctx, &w);
    drop(other_ctx);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    assert!(matches!(
        ctx.resolve_constructor(&claim, own.methods[0], own.refs[0], foreign, K::Allocation),
        Err(E::Instance(_))
    ));
    assert!(
        matches!(ctx.resolve_new(&foreign_claim,own.class,own.classes[0]),Err(E::Initialization(error)) if *error==InitializationAccessError::ClaimMismatch)
    );
    let token = ctx.resolve_new(&claim, own.class, own.classes[0]).unwrap();
    *ctx.heap_mut() = replacement;
    assert!(
        matches!(ctx.allocate_new(&claim,&token),Err(E::Initialization(error)) if matches!(*error,InitializationAccessError::Context(FieldAccessError::ContextChanged)))
    );
    assert_eq!(ctx.heap().allocated_instances(), 1);
}

#[test]
fn abstract_parent_constructor_and_hidden_final_fields_keep_actual_declaring_identity() {
    let mut w = world();
    let base = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x421,
        &[Field::new("x", "I", 17, None)],
        &[Method::new("<init>", "()V", 1)],
        &[("Base", "x", "I", 9)],
        &[],
        65,
    );
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("Base"),
        &[],
        0x21,
        &[Field::new("x", "I", 17, None)],
        &[Method::new("<init>", "()V", 1)],
        &[
            ("C", "<init>", "()V", 10),
            ("Base", "<init>", "()V", 10),
            ("C", "x", "I", 9),
            ("Base", "x", "I", 9),
        ],
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let child = ctx
        .resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
        .unwrap();
    let parent = ctx
        .resolve_constructor(&claim, own.methods[0], own.refs[1], receiver, K::This)
        .unwrap();
    ctx.write_constructor_field(
        &claim,
        &parent,
        base.refs[0],
        receiver,
        Value::Int(17),
        F::UninitializedThis,
    )
    .unwrap();
    ctx.write_constructor_field(
        &claim,
        &child,
        own.refs[2],
        receiver,
        Value::Int(31),
        F::Ordinary,
    )
    .unwrap();
    assert_eq!(
        ctx.write_constructor_field(
            &claim,
            &child,
            own.refs[3],
            receiver,
            Value::Int(99),
            F::Ordinary
        ),
        Err(E::Java(J::IllegalAccess))
    );
    assert_eq!(
        ctx.heap()
            .get_field(receiver, field(&w, base.class, "x", "I")),
        Ok(Value::Int(17))
    );
    assert_eq!(
        ctx.heap()
            .get_field(receiver, field(&w, own.class, "x", "I")),
        Ok(Value::Int(31))
    );
}

#[test]
fn deep_layout_default_funding_and_failed_inherited_constructor_lookup_are_iterative() {
    const DEPTH: usize = 10_000;
    let mut w = world_options(
        "java/lang/Error",
        Some("java/lang/Throwable"),
        0x21,
        RegistryOptions {
            max_definitions: 20_000,
            ..Default::default()
        },
    );
    let base = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("<init>", "(I)V", 1)],
        &[],
        &[],
        65,
    );
    let mut previous = "Base".to_owned();
    let mut leaf = None;
    for index in 0..DEPTH {
        let name = format!("C{index}");
        leaf = Some(define(
            &mut w.registry,
            w.u.bootstrap_loader,
            &name,
            Some(&previous),
            &[],
            0x21,
            &[],
            &[],
            &[],
            &[],
            65,
        ));
        previous = name;
    }
    let caller = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Maker",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("body", "()V", 9)],
        &[(&previous, "<init>", "(I)V", 10)],
        &[&previous],
        65,
    );
    let mut heap = configured_heap(
        &w,
        &[leaf.unwrap().class, caller.class],
        InstanceOptions {
            max_layout_classes: 20_000,
            max_layout_edges: 40_000,
            ..Default::default()
        },
    );
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let token = ctx
        .resolve_new(&claim, caller.class, caller.classes[0])
        .unwrap();
    let before = ctx.lookup_steps();
    let receiver = ctx.allocate_new(&claim, &token).unwrap();
    assert_eq!(ctx.lookup_steps() - before, DEPTH + 2);
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.resolve_constructor(
            &claim,
            caller.methods[0],
            caller.refs[0],
            receiver,
            K::Allocation
        )
        .unwrap_err(),
        E::Java(J::NoSuchMethod)
    );
    assert_eq!(ctx.lookup_steps() - before, DEPTH + 2);
    assert_ne!(base.class, token.class());
    assert_eq!(ctx.heap().allocated_instances(), 1);
}

#[test]
fn constructor_field_cache_hits_keep_exact_descriptor_work_and_late_failure_atomicity() {
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("r", "LC;", 17, None)],
        &[Method::new("<init>", "()V", 1)],
        &[("C", "<init>", "()V", 10), ("C", "r", "LC;", 9)],
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let first = heap.allocate_instance(own.class).unwrap();
    let second = heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let seed = ctx.lookup_steps();
    let claim = claim(&mut ctx, &w);
    let ctor = ctx
        .resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
        .unwrap();
    let before = ctx.lookup_steps();
    ctx.write_constructor_field(
        &claim,
        &ctor,
        own.refs[1],
        receiver,
        Value::Reference(Reference::null()),
        F::Ordinary,
    )
    .unwrap();
    assert_eq!(ctx.lookup_steps() - before, 3);
    let before = ctx.lookup_steps();
    for _ in 0..2 {
        ctx.write_constructor_field(
            &claim,
            &ctor,
            own.refs[1],
            receiver,
            Value::Reference(first),
            F::Ordinary,
        )
        .unwrap();
    }
    assert_eq!(ctx.lookup_steps() - before, 24);
    drop(ctx);
    let field = field(&w, own.class, "r", "LC;");
    for allowance in 0..=12 {
        let mut ctx = options_context(
            &w,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: seed + allowance,
                ..Default::default()
            },
        );
        let result = ctx.write_constructor_field(
            &claim,
            &ctor,
            own.refs[1],
            receiver,
            Value::Reference(second),
            F::Ordinary,
        );
        if allowance < 12 {
            assert!(result.is_err());
            assert_eq!(ctx.lookup_steps(), seed + allowance);
            assert_eq!(
                ctx.heap().get_field(receiver, field),
                Ok(Value::Reference(first))
            );
            assert_eq!(
                ctx.heap().children(receiver).unwrap().collect::<Vec<_>>(),
                [first.heap_ref().unwrap()]
            );
        } else {
            assert!(result.is_ok());
            assert_eq!(ctx.lookup_steps(), seed + 12);
        }
    }
    assert_eq!(
        heap.get_field(receiver, field),
        Ok(Value::Reference(second))
    );
}

#[test]
fn maximum_utf16_signature_remains_borrowed_and_all_null_operations_are_charged() {
    let mut w = world();
    let desc = format!("(L{};)V", "T".repeat(65_530));
    assert_eq!(desc.len(), 65_535);
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("<init>", &desc, 1)],
        &[("C", "<init>", &desc, 10)],
        &[],
        65,
    );
    let mut heap = configured_heap(&w, &[own.class], InstanceOptions::default());
    let receiver = heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.constructor_invocation_shape(own.class, own.refs[0])
            .unwrap()
            .parameters,
        [ValueKind::Reference]
    );
    assert_eq!(
        ctx.resolve_constructor(&claim, own.methods[0], own.refs[0], receiver, K::Allocation)
            .unwrap()
            .method(),
        own.methods[0]
    );
    ctx.check_method_arguments(own.methods[0], &[Value::Reference(Reference::null())])
        .unwrap();
    ctx.check_method_return(own.methods[0], None).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 4 * desc.len() + 2);
}

#[test]
fn exact_and_maximum_instance_limits_use_actual_slots_and_failures_allow_small_retry() {
    let mut w = world();
    let heavy = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Heavy",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("i", "I", 1, None), Field::new("j", "J", 1, None)],
        &[],
        &[],
        &["Heavy", "Small"],
        65,
    );
    let small = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Small",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[Field::new("x", "I", 1, None)],
        &[],
        &[],
        &[],
        65,
    );
    for limit in [2, usize::MAX] {
        let mut heap = configured_heap(
            &w,
            &[heavy.class, small.class],
            InstanceOptions {
                max_instances: limit,
                max_instance_slots: limit,
                max_total_instance_slots: limit,
                ..Default::default()
            },
        );
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let claim = claim(&mut ctx, &w);
        let access = ctx
            .resolve_new(&claim, heavy.class, heavy.classes[0])
            .unwrap();
        ctx.allocate_new(&claim, &access).unwrap();
        assert_eq!(ctx.heap().total_instance_slots(), 2);
        if limit == 2 {
            assert!(ctx.allocate_new(&claim, &access).is_err());
            assert_eq!(ctx.heap().allocated_instances(), 1);
            assert_eq!(ctx.heap().total_instance_slots(), 2);
        }
    }
    let mut heap = configured_heap(
        &w,
        &[heavy.class, small.class],
        InstanceOptions {
            max_instance_slots: 1,
            max_total_instance_slots: 1,
            ..Default::default()
        },
    );
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let claim = claim(&mut ctx, &w);
    let rejected = ctx
        .resolve_new(&claim, heavy.class, heavy.classes[0])
        .unwrap();
    assert!(ctx.allocate_new(&claim, &rejected).is_err());
    let allowed = ctx
        .resolve_new(&claim, heavy.class, heavy.classes[1])
        .unwrap();
    let receiver = ctx.allocate_new(&claim, &allowed).unwrap();
    assert_eq!(ctx.heap().instance_class(receiver), Ok(small.class));
    assert_eq!(
        (
            ctx.heap().allocated_instances(),
            ctx.heap().total_instance_slots()
        ),
        (1, 1)
    );
}

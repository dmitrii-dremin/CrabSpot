//! Private corruption boundaries use original small classfile bytes.
use super::*;
use crate::construction::ConstructionState;
use crate::{ConstructionLimits, InitializationLimits};
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    HeapOptions, HierarchyOptions, InstanceOptions, ReferenceTypeUniverse, RegistryOptions,
    SameLoaderResolver, StaticStateOptions, build_hierarchy,
};
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn words(bytes: &mut Vec<u8>, items: &[u16]) {
    for item in items {
        bytes.extend(item.to_be_bytes());
    }
}
fn bytes(name: &str, parent: Option<&str>, flags: u16, target: bool) -> Vec<u8> {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    words(&mut bytes, &[16]);
    for text in [name, parent.unwrap_or("java/lang/Object")] {
        bytes.push(1);
        words(&mut bytes, &[text.len() as u16]);
        bytes.extend(text.bytes());
        bytes.push(7);
        words(&mut bytes, &[if text == name { 1 } else { 3 }]);
    }
    for text in ["<init>", "()V", "Code", "f", "I"] {
        bytes.push(1);
        words(&mut bytes, &[text.len() as u16]);
        bytes.extend(text.bytes());
    }
    bytes.push(12);
    words(&mut bytes, &[8, 9]);
    bytes.push(9);
    words(&mut bytes, &[2, 10]);
    bytes.push(12);
    words(&mut bytes, &[5, 6]);
    bytes.push(10);
    words(&mut bytes, &[4, 12]);
    bytes.push(10);
    words(&mut bytes, &[2, 12]);
    bytes.extend([1, 0, 1, b'm']);
    words(
        &mut bytes,
        &[
            flags,
            2,
            if parent.is_some() { 4 } else { 0 },
            0,
            u16::from(target),
        ],
    );
    if target {
        words(&mut bytes, &[0x11, 8, 9, 0]);
    }
    words(&mut bytes, &[if target { 2 } else { 0 }]);
    if target {
        for (flags, name, code, stack) in [
            (1, 5, vec![0x2a, 0x04, 0xb5, 0, 11, 0xb1], 2),
            (9, 15, vec![0xb1], 3),
        ] {
            words(&mut bytes, &[flags, name, 6, 1, 7]);
            bytes.extend((12 + code.len() as u32).to_be_bytes());
            words(&mut bytes, &[stack, 1]);
            bytes.extend((code.len() as u32).to_be_bytes());
            bytes.extend(code);
            words(&mut bytes, &[0, 0]);
        }
    }
    words(&mut bytes, &[0]);
    bytes
}
struct World {
    registry: DefinitionRegistry,
    heap: ManagedHeap,
    target: ClassId,
    ctor: MethodId,
    origin: MethodId,
    field: crabspot_runtime::FieldId,
    throwable: ClassId,
    error: ClassId,
}
fn world() -> World {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let mut classes = HashMap::new();
    for (name, parent, flags) in [
        ("java/lang/Object", None, 0x21),
        ("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
        ("java/io/Serializable", Some("java/lang/Object"), 0x601),
        ("java/lang/Throwable", Some("java/lang/Object"), 0x21),
        ("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        ("q/Target", Some("java/lang/Object"), 0x21),
    ] {
        let class = registry
            .define(
                loader,
                &units(name),
                &bytes(name, parent, flags, name == "q/Target"),
                ParseOptions::default(),
            )
            .unwrap();
        classes.insert(name, class);
    }
    let mut resolver = SameLoaderResolver;
    let plans = classes
        .values()
        .map(|class| {
            build_hierarchy(
                &registry,
                *class,
                &mut resolver,
                HierarchyOptions::default(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_instances: 1,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(
        &registry,
        ReferenceTypeUniverse {
            bootstrap_loader: loader,
            object: classes["java/lang/Object"],
            cloneable: classes["java/lang/Cloneable"],
            serializable: classes["java/io/Serializable"],
        },
    )
    .unwrap();
    let target = classes["q/Target"];
    let ctor = registry
        .find_declared_method(target, &units("<init>"), &units("()V"))
        .unwrap()
        .unwrap();
    let origin = registry
        .find_declared_method(target, &units("m"), &units("()V"))
        .unwrap()
        .unwrap();
    let field = registry
        .find_declared_field(target, &units("f"), &units("I"))
        .unwrap()
        .unwrap();
    World {
        registry,
        heap,
        target,
        ctor,
        origin,
        field,
        throwable: classes["java/lang/Throwable"],
        error: classes["java/lang/Error"],
    }
}
fn state() -> SessionState {
    SessionState {
        records: Vec::new(),
        indices: HashMap::new(),
        limits: InitializationLimits::default(),
        admitted_count: 6,
        dependencies: 0,
        constants: 0,
        evaluation_owner: 1,
        steps: 0,
        construction: ConstructionState::default(),
        equality: Default::default(),
    }
}
fn access(
    context: &mut FieldResolutionContext<'_>,
    claim: &InitializationClaim,
    origin: MethodId,
    reference: Reference,
) -> ResolvedConstructor {
    context
        .resolve_constructor(
            claim,
            origin,
            14,
            reference,
            ConstructorReceiverKind::Allocation,
        )
        .unwrap()
}
#[test]
fn mismatched_and_duplicate_constructor_authority_cannot_promote_aliases_or_write_fields() {
    for duplicate in [false, true] {
        let mut w = world();
        let receiver = w.heap.allocate_instance(w.target).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &w.registry,
            &mut w.heap,
            &mut resolver,
            crabspot_runtime::FieldResolutionOptions::default(),
        )
        .unwrap();
        let statics = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        let exceptions = context.prepare_exception_profile(w.throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &exceptions, w.error)
            .unwrap();
        let metadata = w.registry.class(w.target).unwrap();
        let caller_prepared = prepare_method(
            metadata,
            w.origin.index(),
            &mut 0,
            usize::MAX,
            Preparation::Static,
        )
        .unwrap();
        let prepared = prepare_method(
            metadata,
            w.ctor.index(),
            &mut 0,
            usize::MAX,
            Preparation::Constructor,
        )
        .unwrap();
        let alias = Uninitialized {
            reference: receiver,
            key: AliasKey::Allocation {
                method: w.origin,
                offset: 0,
            },
        };
        let this = Uninitialized {
            reference: receiver,
            key: AliasKey::This(1),
        };
        let mut caller = make_frame(
            MethodKey::Registered(w.origin),
            &caller_prepared,
            &[],
            None,
            None,
        )
        .unwrap();
        caller
            .locals
            .store_frame(0, FrameValue::Uninitialized(alias), 0)
            .unwrap();
        caller
            .stack
            .push_frame(FrameValue::Uninitialized(alias), 0)
            .unwrap();
        let mut callee = make_frame(
            MethodKey::Registered(w.ctor),
            &prepared,
            &[],
            Some((MethodKey::Registered(w.origin), 0)),
            Some(receiver),
        )
        .unwrap();
        callee.role = FrameRole::Constructor;
        callee.constructor.push(ConstructorActivation {
            access: access(&mut context, &claim, w.origin, receiver),
            this,
            caller: alias,
            chain: Chain::Complete,
        });
        if duplicate {
            callee.constructor.push(ConstructorActivation {
                access: access(&mut context, &claim, w.origin, receiver),
                this,
                caller: alias,
                chain: Chain::Complete,
            });
        } else {
            callee.method = MethodKey::Registered(w.origin);
        }
        let reserved = caller.reserved_slots + callee.reserved_slots;
        let mut java = JavaState {
            cache: HashMap::new(),
            frames: vec![caller, callee],
            reserved,
            used_code_bytes: 0,
            remaining: 10,
            monitors: None,
        };
        let mut state = state();
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: Vec::new(),
            strings: false,
        };
        assert!(matches!(
            finish_constructor_boundary(&mut java, &mut init, true, 5)
                .unwrap_err()
                .kind,
            E::ConstructionProtocol { .. }
        ));
        assert_eq!(init.state.construction.steps, 0);
        assert_eq!(java.frames.len(), 2);
        assert_eq!(java.reserved, reserved);
        assert_eq!(
            java.frames[0].locals.load_reference(0, 0).unwrap(),
            FrameValue::Uninitialized(alias)
        );
        assert_eq!(java.frames[0].stack.len(), 1);
        let frame = java.frames.last_mut().unwrap();
        frame.pc = 2;
        frame.stack.push(Value::Reference(receiver), 0).unwrap();
        frame.stack.push(Value::Int(7), 0).unwrap();
        let mut services = Services::Fields {
            context: &mut context,
            root: None,
            receiver: None,
            statics: Some(&statics),
        };
        let error = match step(
            metadata,
            &prepared,
            frame,
            &mut services,
            Some(&exceptions),
            Some(&claim),
            EngineCapabilities::default(),
        ) {
            Err(e) => e,
            Ok(_) => panic!("corrupt constructor authority wrote a field"),
        };
        assert!(matches!(error.kind, E::ConstructionProtocol { .. }));
        assert_eq!(
            context.heap().get_field(receiver, w.field).unwrap(),
            Value::Int(0)
        );
    }
}
#[test]
fn failed_new_publication_keeps_old_site_locals_and_does_not_recharge_alias_work() {
    let mut w = world();
    let receiver = w.heap.allocate_instance(w.target).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &w.registry,
        &mut w.heap,
        &mut resolver,
        crabspot_runtime::FieldResolutionOptions::default(),
    )
    .unwrap();
    let statics = context
        .allocate_static_state(StaticStateOptions::default())
        .unwrap();
    let exceptions = context.prepare_exception_profile(w.throwable).unwrap();
    let claim = context
        .claim_initialization(&statics, &exceptions, w.error)
        .unwrap();
    let prepared = prepare_method(
        w.registry.class(w.target).unwrap(),
        w.origin.index(),
        &mut 0,
        usize::MAX,
        Preparation::Static,
    )
    .unwrap();
    let alias = Uninitialized {
        reference: receiver,
        key: AliasKey::Allocation {
            method: w.origin,
            offset: 0,
        },
    };
    let mut frame =
        make_frame(MethodKey::Registered(w.origin), &prepared, &[], None, None).unwrap();
    frame
        .locals
        .store_frame(0, FrameValue::Uninitialized(alias), 0)
        .unwrap();
    frame.stack.reserve_one(0).unwrap();
    frame
        .new_access
        .push(context.resolve_new(&claim, w.target, 2).unwrap());
    frame.pending = Some(PendingUse::New { next: 3 });
    let reserved = frame.reserved_slots;
    let mut java = JavaState {
        cache: HashMap::new(),
        frames: vec![frame],
        reserved,
        used_code_bytes: 0,
        remaining: 1,
        monitors: None,
    };
    let mut state = state();
    state
        .construction
        .reset(ConstructionLimits { max_alias_steps: 2 });
    state.construction.charge(1, 0, 0).unwrap();
    let mut init = InitExecution {
        claim: &claim,
        state: &mut state,
        owner: 1,
        activations: Vec::new(),
        strings: false,
    };
    let bodies = Bodies {
        standalone: None,
        registry: Some(&w.registry),
    };
    let mut services = Services::Fields {
        context: &mut context,
        root: None,
        receiver: None,
        statics: Some(&statics),
    };
    let error = match resume_initialization(
        InitContinuation::Instruction {
            frame: 0,
            method: MethodKey::Registered(w.origin),
            pc: 0,
        },
        bodies,
        &mut java,
        &mut services,
        &mut init,
        CallLimits::default(),
    ) {
        Err(e) => e,
        Ok(_) => panic!("over-quota allocation published"),
    };
    assert!(matches!(error.kind, E::ConstructionAccess(_)));
    assert_eq!(
        java.frames[0].locals.load_reference(0, 0).unwrap(),
        FrameValue::Uninitialized(alias)
    );
    assert_eq!(java.frames[0].stack.len(), 0);
    assert_eq!(init.state.construction.steps, 2);
    assert_eq!(context.heap().allocated_instances(), 1);
}

#[test]
fn construction_mapping_preserves_all_nested_java_shaped_errors_as_host_failures() {
    for (kind, expected) in [
        (
            ConstructionJavaError::NoSuchMethod,
            VmExceptionKind::NoSuchMethodError,
        ),
        (
            ConstructionJavaError::NoSuchField,
            VmExceptionKind::NoSuchFieldError,
        ),
        (
            ConstructionJavaError::IllegalAccess,
            VmExceptionKind::IllegalAccessError,
        ),
        (
            ConstructionJavaError::IncompatibleClassChange,
            VmExceptionKind::IncompatibleClassChangeError,
        ),
        (
            ConstructionJavaError::NullPointer,
            VmExceptionKind::NullPointerException,
        ),
        (
            ConstructionJavaError::Instantiation,
            VmExceptionKind::InstantiationError,
        ),
    ] {
        assert_eq!(
            construction_failure(ConstructionAccessError::Java(kind), 7).kind,
            E::UnsupportedVmException(expected)
        );
    }
    for error in [
        ConstructionAccessError::Method(Box::new(MethodAccessError::Java(
            MethodJavaError::NoSuchMethod,
        ))),
        ConstructionAccessError::Field(Box::new(FieldAccessError::Java(
            FieldJavaError::NullPointer,
        ))),
        ConstructionAccessError::Type(Box::new(TypeAccessError::Java(TypeJavaError::ClassCast))),
        ConstructionAccessError::Initialization(Box::new(InitializationAccessError::Java(
            StaticJavaError::IllegalAccess,
        ))),
        ConstructionAccessError::Instance(Box::new(crabspot_runtime::InstanceError::Heap(
            HeapError::NullReference,
        ))),
    ] {
        let failure = construction_failure(error.clone(), 7);
        assert_eq!(failure.kind, E::ConstructionAccess(Box::new(error)));
        assert_eq!(failure.offset, Some(7));
        assert!(std::error::Error::source(&failure).is_some());
    }
}
#[test]
fn private_reference_branches_reject_null_foreign_and_non_instance_token_corruption() {
    let mut w = world();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    let array = w
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    for reference in [Reference::null(), foreign, array] {
        let value = FrameValue::Uninitialized(Uninitialized {
            reference,
            key: AliasKey::This(1),
        });
        let error = validate_frame_reference(Some(&w.heap), value, 9).unwrap_err();
        if reference.is_null() {
            assert!(matches!(error.kind, E::ConstructionProtocol { .. }));
        } else {
            assert!(matches!(error.kind, E::ConstructionAccess(_)));
        }
        assert_eq!(error.offset, Some(9));
    }
}
#[test]
fn constructor_preparation_is_distinct_from_all_ordinary_and_initializer_cache_modes() {
    let w = world();
    let class = w.registry.class(w.target).unwrap();
    let prepared = prepare_method(
        class,
        w.ctor.index(),
        &mut 0,
        usize::MAX,
        Preparation::Constructor,
    )
    .unwrap();
    assert!(prepared.constructor && prepared.instance && !prepared.initializer);
    for mode in [
        Preparation::Static,
        Preparation::Initializer,
        Preparation::DirectInstanceRoot,
        Preparation::ResolvedInstance,
    ] {
        assert!(matches!(
            prepare_method(class, w.ctor.index(), &mut 0, usize::MAX, mode)
                .err()
                .unwrap()
                .kind,
            E::UnsupportedMethod(_)
        ));
    }
    assert!(matches!(
        prepare_method(
            class,
            w.origin.index(),
            &mut 0,
            usize::MAX,
            Preparation::Constructor
        )
        .err()
        .unwrap()
        .kind,
        E::UnsupportedMethod(_)
    ));
}

#[test]
fn identity_branches_extract_both_ordinary_operands_before_foreign_handle_validation() {
    let mut foreign_heap = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign = foreign_heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    for opcode in [0xa5, 0xa6] {
        for wrong in [false, true] {
            let class = crate::tests::synthetic(&[opcode, 0, 3, 0xb1], "()V", 2, 0);
            let prepared =
                prepare_method(&class, 0, &mut 0, usize::MAX, Preparation::Static).unwrap();
            let mut frame =
                make_frame(MethodKey::Standalone(0), &prepared, &[], None, None).unwrap();
            if wrong {
                frame.stack.push(Value::Int(7), 0).unwrap();
            }
            frame.stack.push(Value::Reference(foreign), 0).unwrap();
            let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
            let mut services = Services::Heap(&mut heap);
            let error = match step(
                &class,
                &prepared,
                &mut frame,
                &mut services,
                None,
                None,
                EngineCapabilities::default(),
            ) {
                Err(error) => error,
                Ok(_) => panic!("malformed identity comparison succeeded"),
            };
            assert_eq!(
                error.kind,
                if wrong {
                    E::WrongValueType {
                        expected: ValueKind::Reference,
                        actual: ValueKind::Int,
                    }
                } else {
                    E::StackUnderflow
                }
            );
            assert_eq!(error.offset, Some(0));
        }
    }
}

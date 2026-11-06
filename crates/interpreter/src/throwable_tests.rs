use super::*;
#[allow(dead_code)]
mod world {
    pub use crate::execute::assertion_tests::world::recipes;
    include!("../tests/support/throwable_world_impl.rs");
}
fn program() -> world::Class {
    let mut c = world::app();
    c.method("fault", "(II)I", 9, &[0x1a, 0x1b, 0x6c, 0xac], 2, 2);
    c
}
fn state() -> SessionState {
    SessionState {
        records: vec![],
        indices: HashMap::new(),
        limits: Default::default(),
        admitted_count: 100,
        dependencies: 0,
        constants: 0,
        evaluation_owner: 1,
        steps: 0,
        construction: Default::default(),
        equality: Default::default(),
        vm_exceptions: Default::default(),
    }
}
fn java(context: &mut FieldResolutionContext<'_>, method: MethodId) -> JavaState {
    let class = context.registry().class(method.owner()).unwrap();
    let mut used = 0;
    let prepared = prepare_method_with_context(
        class,
        method.index(),
        &mut used,
        usize::MAX,
        Preparation::Static,
        Some((context, method, true)),
        None,
    )
    .unwrap();
    let frame = make_frame(
        MethodKey::Registered(method),
        &prepared,
        &[Value::Int(1), Value::Int(0)],
        None,
        None,
    )
    .unwrap();
    JavaState {
        cache: HashMap::from([(MethodKey::Registered(method), prepared)]),
        frames: vec![frame],
        reserved: 4,
        used_code_bytes: used,
        remaining: 100,
        monitors: Some(MonitorLedger::new(Default::default())),
        vm: Some(VmExecution::new(VmExceptionLimits {
            max_materializations: u64::MAX,
        })),
    }
}
#[test]
fn admission_overflow_and_limit_ties_do_not_partially_publish_started() {
    for case in 0..3 {
        let mut w = world::World::new(program());
        let method = w.method("fault", "(II)I");
        let error = w.error;
        let throwable = w.throwable;
        let mut context = w.context();
        let registry = context.registry();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let profile = context.prepare_exception_profile(throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &profile, error)
            .unwrap();
        let mut state = state();
        state.vm_exceptions.started = u64::MAX;
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: vec![],
            strings: true,
        };
        let mut java = java(&mut context, method);
        if case == 0 {
            java.vm.as_mut().unwrap().limits.max_materializations = 0;
        } else if case == 1 {
            java.vm.as_mut().unwrap().admitted = u64::MAX;
        }
        let mut exceptions = ExceptionState {
            profile: &profile,
            limits: Default::default(),
            prepared_handlers: 0,
            dispatch_steps: 0,
        };
        let bodies = Bodies {
            standalone: None,
            registry: Some(registry),
        };
        let mut services = Services::Fields {
            context: &mut context,
            root: None,
            receiver: None,
            statics: Some(&statics),
        };
        let error = begin_vm_fault(
            &mut java,
            &mut services,
            &mut init,
            &mut exceptions,
            bodies,
            Default::default(),
            MethodKey::Registered(method),
            2,
        )
        .err()
        .unwrap();
        match case {
            0 => assert_eq!(
                error.kind,
                E::VmExceptionLimit {
                    limit: 0,
                    required: 1
                }
            ),
            1 => assert_eq!(
                error.kind,
                E::VmExceptionAccountingOverflow {
                    counter: VmExceptionCounter::Evaluation
                }
            ),
            _ => assert_eq!(
                error.kind,
                E::VmExceptionAccountingOverflow {
                    counter: VmExceptionCounter::Started
                }
            ),
        }
        assert_eq!(init.state.vm_exceptions.started, u64::MAX);
        assert_eq!(init.state.vm_exceptions.produced, 0);
        assert!(java.vm.as_ref().unwrap().pending.is_none());
        assert_eq!(java.frames.len(), 1);
    }
}
#[test]
fn incomplete_and_corrupted_production_counts_cannot_dispatch_or_partially_increment() {
    for case in 0..3 {
        let mut w = world::World::new(program());
        let method = w.method("fault", "(II)I");
        let error = w.error;
        let throwable = w.throwable;
        let mut context = w.context();
        let registry = context.registry();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let profile = context.prepare_exception_profile(throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &profile, error)
            .unwrap();
        let mut java = java(&mut context, method);
        let spec = context
            .vm_exception_spec(&claim, VmExceptionKind::ArithmeticException)
            .unwrap();
        let (receiver, _access) = context.allocate_vm_exception(&claim, &spec).unwrap();
        let builder = context.begin_trace_snapshot(&claim, 0).unwrap();
        let snapshot = context.finish_trace_snapshot(&claim, builder).unwrap();
        let native = registry
            .find_declared_method(
                throwable,
                &world::recipes::support::units("fillInStackTrace"),
                &world::recipes::support::units("(I)Ljava/lang/Throwable;"),
            )
            .unwrap()
            .unwrap();
        let capture = context
            .prepare_throwable_capture(&claim, native, receiver)
            .unwrap();
        context
            .commit_throwable_capture(&claim, &capture, snapshot)
            .unwrap();
        let mut state = state();
        if case == 2 {
            state.vm_exceptions = crate::VmExceptionUsage {
                started: u64::MAX,
                produced: u64::MAX,
            };
        }
        let before = state.vm_exceptions;
        java.vm.as_mut().unwrap().pending = Some(VmPending {
            origin: VmFaultOrigin {
                method,
                pc: 2,
                kind: VmExceptionKind::ArithmeticException,
            },
            owner: 1,
            depth: 0,
            spec,
            snapshot: None,
            receiver: Some(receiver),
            consumed: case != 0,
        });
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: vec![],
            strings: true,
        };
        let exceptions = ExceptionState {
            profile: &profile,
            limits: Default::default(),
            prepared_handlers: 0,
            dispatch_steps: 0,
        };
        let mut services = Services::Fields {
            context: &mut context,
            root: None,
            receiver: None,
            statics: Some(&statics),
        };
        let error =
            complete_vm_fault(&mut java, &mut services, &mut init, &exceptions).unwrap_err();
        if case == 2 {
            assert_eq!(
                error.kind,
                E::VmExceptionAccountingOverflow {
                    counter: VmExceptionCounter::Produced
                }
            );
        } else {
            assert!(matches!(error.kind, E::VmExceptionProtocol { .. }));
        }
        assert_eq!(init.state.vm_exceptions, before);
        assert!(java.vm.as_ref().unwrap().pending.is_some());
        assert_eq!(java.frames.len(), 1);
    }
}
#[test]
fn snapshot_rejects_non_boundary_pc_after_paid_snapshot_admission_without_token() {
    let mut c = world::app();
    c.method("fault", "(II)I", 9, &[0x10, 7, 3, 0x6c, 0xac], 2, 2);
    let mut w = world::World::new(c);
    let method = w.method("fault", "(II)I");
    let error = w.error;
    let throwable = w.throwable;
    let mut context = w.context();
    let statics = context.allocate_static_state(Default::default()).unwrap();
    let profile = context.prepare_exception_profile(throwable).unwrap();
    let claim = context
        .claim_initialization(&statics, &profile, error)
        .unwrap();
    let java = java(&mut context, method);
    assert!(matches!(
        snapshot(&java, &mut context, &claim, 0, 1)
            .unwrap_err()
            .kind,
        E::VmExceptionProtocol { .. }
    ));
    let usage = context.heap().throwable_trace_usage();
    assert_eq!((usage.snapshots, usage.frames, usage.backtraces), (1, 1, 0));
}

#[test]
fn damaged_vm_constructor_boundary_cannot_pop_or_publish_any_caller_change() {
    for case in 0..6 {
        let mut w = world::World::new(program());
        let method = w.method("fault", "(II)I");
        let error = w.error;
        let throwable = w.throwable;
        let mut context = w.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let profile = context.prepare_exception_profile(throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &profile, error)
            .unwrap();
        let mut java = java(&mut context, method);
        let spec = context
            .vm_exception_spec(&claim, VmExceptionKind::ArithmeticException)
            .unwrap();
        let (receiver, access) = context.allocate_vm_exception(&claim, &spec).unwrap();
        let target = access.method();
        let mut used = 0;
        let prepared = prepare_method_with_context(
            context.registry().class(target.owner()).unwrap(),
            target.index(),
            &mut used,
            usize::MAX,
            Preparation::Constructor,
            Some((&mut context, target, true)),
            None,
        )
        .unwrap();
        let mut frame = make_frame(
            MethodKey::Registered(target),
            &prepared,
            &[Value::Reference(Reference::null())],
            Some((MethodKey::Registered(method), 2)),
            Some(receiver),
        )
        .unwrap();
        frame.role = FrameRole::Constructor;
        frame.constructor.push(ConstructorActivation {
            access,
            this: Uninitialized {
                reference: receiver,
                key: AliasKey::This(1),
            },
            caller: ConstructorCaller::VmFault(1),
            chain: Chain::Complete,
        });
        java.frames[0].pc = 2;
        java.frames[0].pending = Some(PendingUse::VmFault);
        java.frames[0].stack.push(Value::Int(17), 2).unwrap();
        java.reserved += frame.reserved_slots;
        java.frames.push(frame);
        java.vm.as_mut().unwrap().pending = Some(VmPending {
            origin: VmFaultOrigin {
                method,
                pc: 2,
                kind: VmExceptionKind::ArithmeticException,
            },
            owner: 1,
            depth: 0,
            spec,
            snapshot: None,
            receiver: Some(receiver),
            consumed: true,
        });
        match case {
            0 => java.frames[1].method = MethodKey::Registered(method),
            1 => java.vm.as_mut().unwrap().pending.as_mut().unwrap().owner = 2,
            2 => java.vm.as_mut().unwrap().pending.as_mut().unwrap().depth = 1,
            3 => java.frames[0].pending = None,
            4 => java.vm.as_mut().unwrap().pending.as_mut().unwrap().consumed = false,
            _ => {
                let pending = java.vm.as_ref().unwrap().pending.as_ref().unwrap();
                let (other, access) = context
                    .allocate_vm_exception(&claim, &pending.spec)
                    .unwrap();
                java.frames[1].constructor.push(ConstructorActivation {
                    access,
                    this: Uninitialized {
                        reference: other,
                        key: AliasKey::This(2),
                    },
                    caller: ConstructorCaller::VmFault(1),
                    chain: Chain::Complete,
                });
            }
        }
        let before = java.reserved;
        let mut state = state();
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: vec![],
            strings: true,
        };
        assert!(finish_constructor_boundary(&mut java, &mut init, true, 5).is_err());
        assert_eq!(java.frames.len(), 2);
        assert_eq!(java.reserved, before);
        assert_eq!(java.frames[0].pc, 2);
        assert_eq!(
            java.frames[0].stack.pop(ValueKind::Int, 2).unwrap(),
            Value::Int(17)
        );
        assert_eq!(init.state.vm_exceptions, Default::default());
        assert_eq!(context.heap().throwable_trace_usage().backtraces, 0);
    }
}

#[test]
fn nested_runtime_java_shaped_errors_stay_typed_host_sources() {
    for java in [
        FieldJavaError::NullPointer,
        FieldJavaError::IllegalAccess,
        FieldJavaError::LinkageError,
    ] {
        let source = crabspot_runtime::ThrowableAccessError {
            kind: crabspot_runtime::ThrowableErrorKind::Context(Box::new(FieldAccessError::Java(
                java,
            ))),
            discovery_usage: Default::default(),
        };
        let error = trace_error(Box::new(source.clone()));
        assert_eq!(error.kind, E::ThrowableAccess(Box::new(source)));
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.vm_exception.is_none());
    }
}

#[test]
fn pure_service_error_uses_the_pending_recursive_activation_not_oldest_method_match() {
    let mut c = program();
    c.method("outer", "()V", 9, &[0xb1], 0, 0);
    c.method("middle", "()V", 9, &[0xb1], 0, 0);
    let mut w = world::World::new(c);
    let fault = w.method("fault", "(II)I");
    let outer = w.method("outer", "()V");
    let middle = w.method("middle", "()V");
    let error = w.error;
    let throwable = w.throwable;
    let mut context = w.context();
    let statics = context.allocate_static_state(Default::default()).unwrap();
    let profile = context.prepare_exception_profile(throwable).unwrap();
    let claim = context
        .claim_initialization(&statics, &profile, error)
        .unwrap();
    let mut java = java(&mut context, fault);
    let prepared = java.cache.get(&MethodKey::Registered(fault)).unwrap();
    java.frames[0].origin = Some((MethodKey::Registered(outer), 7));
    let recent = make_frame(
        MethodKey::Registered(fault),
        prepared,
        &[Value::Int(1), Value::Int(0)],
        Some((MethodKey::Registered(middle), 19)),
        None,
    )
    .unwrap();
    java.frames.push(recent);
    let spec = context
        .vm_exception_spec(&claim, VmExceptionKind::ArithmeticException)
        .unwrap();
    let vm = java.vm.as_mut().unwrap();
    vm.context = Some(VmExceptionContext {
        origin: VmFaultOrigin {
            method: fault,
            pc: 2,
            kind: VmExceptionKind::ArithmeticException,
        },
        phase: PhaseVm::Dispatch,
    });
    vm.pending = Some(VmPending {
        origin: vm.context.unwrap().origin,
        owner: 1,
        depth: 1,
        spec,
        snapshot: None,
        receiver: None,
        consumed: true,
    });
    let bodies = Bodies {
        standalone: None,
        registry: Some(context.registry()),
    };
    let error = annotate_vm_error(
        &java,
        bodies,
        trace_error(Box::new(crabspot_runtime::ThrowableAccessError {
            kind: crabspot_runtime::ThrowableErrorKind::InvalidCapture,
            discovery_usage: Default::default(),
        })),
    );
    assert_eq!(error.method.as_ref().unwrap().index, fault.index());
    assert_eq!(error.offset, Some(2));
    let site = error.call_site.unwrap();
    assert_eq!(site.method.index, middle.index());
    assert_eq!(site.offset, 19);
}

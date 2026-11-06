use super::*;
#[path = "../tests/support/equality_world.rs"]
pub(super) mod world;
fn state(
    plan: crabspot_runtime::InitializationPlan,
    class: ClassId,
    registration: bool,
    queries: u64,
    active: bool,
) -> SessionState {
    SessionState {
        records: vec![Record {
            class,
            status: if active {
                RecordStatus::InProgress {
                    owner: 1,
                    phase: Phase::Body,
                }
            } else {
                RecordStatus::Completed
            },
            plan,
        }],
        indices: HashMap::from([(class, 0)]),
        limits: Default::default(),
        admitted_count: 100,
        dependencies: 0,
        constants: 0,
        vm_exceptions: Default::default(),
        evaluation_owner: 1,
        steps: 0,
        construction: Default::default(),
        equality: crate::StringEqualityUsage {
            natives_registered: registration,
            assertion_queries: queries,
        },
    }
}
fn frame(method: MethodId, pc: u32, role: FrameRole) -> Frame {
    Frame {
        method: MethodKey::Registered(method),
        role,
        pending: None,
        static_access: vec![],
        new_access: vec![],
        constructor: vec![],
        locals: Locals::new(4),
        stack: Stack::new(2),
        pc,
        origin: None,
        reserved_slots: 6,
        entry_receiver: None,
    }
}
#[test]
fn private_pending_target_role_caller_and_stack_damage_never_publish_query() {
    for damage in 0..8 {
        let mut w = world::World::new(world::app(), BootstrapAssertionPolicy::Enabled);
        let class = w.class;
        let throwable = w.throwable;
        let error = w.error;
        let caller = w.g.method(class, "desiredAssertionStatus", "()Z");
        let query =
            w.g.method(class, "desiredAssertionStatus0", "(Ljava/lang/Class;)Z");
        let reg = w.g.method(class, "registerNatives", "()V");
        let mirror = w.mirror(w.g.object);
        let mut context = w.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &exceptions, error)
            .unwrap();
        let plan = context
            .plan_initialization_with_strings(
                &claim,
                class,
                InitializationPlanLimits {
                    max_dependencies: 100,
                    max_constants: 100,
                },
            )
            .unwrap();
        let mut state = state(plan, class, true, 0, false);
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: vec![],
            strings: true,
        };
        let mut f = frame(caller, 10, FrameRole::Ordinary);
        f.stack.reserve_one(10).unwrap();
        let mut pending = PendingNative {
            role: ClassAssertionNative::DesiredStatus,
            target: query,
            argument: Some(mirror),
            next: 13,
            stack_len: 0,
            stack_slots: 0,
        };
        match damage {
            0 => pending.target = reg,
            1 => pending.role = ClassAssertionNative::Register,
            2 => f.method = MethodKey::Registered(reg),
            3 => f.role = FrameRole::Initializer(class),
            4 => pending.stack_len = 1,
            5 => init.owner = 2,
            6 => f.pending = Some(PendingUse::New { next: 14 }),
            _ => pending.next = 14,
        }
        let result = resume_assertion_native(pending, &mut f, &context, &mut init, 10);
        assert!(result.is_err());
        assert_eq!(init.state.equality.assertion_queries, 0);
        assert!(init.state.equality.natives_registered);
        assert_eq!(f.stack.len(), 0);
        assert_eq!(f.pc, 10);
    }
}
#[test]
fn private_registration_requires_matching_live_initializer_record_and_claim() {
    for damage in 0..6 {
        let mut w = world::World::new(world::app(), BootstrapAssertionPolicy::Disabled);
        let class = w.class;
        let throwable = w.throwable;
        let error = w.error;
        let caller = w.g.method(class, "<clinit>", "()V");
        let reg = w.g.method(class, "registerNatives", "()V");
        let mut context = w.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &exceptions, error)
            .unwrap();
        let plan = context
            .plan_initialization_with_strings(
                &claim,
                class,
                InitializationPlanLimits {
                    max_dependencies: 100,
                    max_constants: 100,
                },
            )
            .unwrap();
        let mut state = state(plan, class, false, 0, true);
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: vec![Activation {
                class,
                record: 0,
                next_dependency: 0,
                phase: Phase::Body,
                continuation: InitContinuation::RootInitialize,
                origin: None,
            }],
            strings: true,
        };
        let mut f = frame(caller, 0, FrameRole::Initializer(class));
        let mut pending = PendingNative {
            role: ClassAssertionNative::Register,
            target: reg,
            argument: None,
            next: 3,
            stack_len: 0,
            stack_slots: 0,
        };
        match damage {
            0 => init.activations[0].record = 1,
            1 => init.activations[0].phase = Phase::Dependencies,
            2 => f.role = FrameRole::Ordinary,
            3 => init.state.records[0].status = RecordStatus::Completed,
            4 => init.state.records[0].status = RecordStatus::Aborted,
            _ => pending.next = 4,
        }
        assert!(resume_assertion_native(pending, &mut f, &context, &mut init, 0).is_err());
        assert_eq!(init.state.equality, crate::StringEqualityUsage::default());
        assert_eq!(f.pc, 0);
    }
}
#[test]
fn query_overflow_or_missing_registration_has_no_result_or_counter_effect() {
    for registered in [false, true] {
        let mut w = world::World::new(world::app(), BootstrapAssertionPolicy::Enabled);
        let class = w.class;
        let throwable = w.throwable;
        let error = w.error;
        let caller = w.g.method(class, "desiredAssertionStatus", "()Z");
        let query =
            w.g.method(class, "desiredAssertionStatus0", "(Ljava/lang/Class;)Z");
        let mirror = w.mirror(w.g.object);
        let mut context = w.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &exceptions, error)
            .unwrap();
        let plan = context
            .plan_initialization_with_strings(
                &claim,
                class,
                InitializationPlanLimits {
                    max_dependencies: 100,
                    max_constants: 100,
                },
            )
            .unwrap();
        let mut state = state(plan, class, registered, u64::MAX, false);
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: vec![],
            strings: true,
        };
        let mut f = frame(caller, 10, FrameRole::Ordinary);
        f.stack.reserve_one(10).unwrap();
        let pending = PendingNative {
            role: ClassAssertionNative::DesiredStatus,
            target: query,
            argument: Some(mirror),
            next: 13,
            stack_len: 0,
            stack_slots: 0,
        };
        let error = resume_assertion_native(pending, &mut f, &context, &mut init, 10).unwrap_err();
        if registered {
            assert_eq!(error.kind, E::AssertionQueryOverflow);
        } else {
            assert!(matches!(error.kind, E::StringBody(_)));
        }
        assert_eq!(init.state.equality.assertion_queries, u64::MAX);
        assert_eq!(f.stack.len(), 0);
        assert_eq!(f.pc, 10);
    }
}

#[test]
fn null_or_invalid_native_argument_preserves_count_pc_and_reserved_result() {
    for kind in 0..3 {
        let mut w = world::World::new(world::app(), BootstrapAssertionPolicy::Enabled);
        let class = w.class;
        let throwable = w.throwable;
        let error = w.error;
        let caller = w.g.method(class, "desiredAssertionStatus", "()Z");
        let query =
            w.g.method(class, "desiredAssertionStatus0", "(Ljava/lang/Class;)Z");
        let argument = match kind {
            0 => Reference::null(),
            1 => w.h.allocate_instance(w.owner).unwrap(),
            _ => {
                let mut foreign =
                    world::World::new(world::app(), BootstrapAssertionPolicy::Disabled);
                foreign.mirror(foreign.g.object)
            }
        };
        let mut context = w.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let claim = context
            .claim_initialization(&statics, &exceptions, error)
            .unwrap();
        let plan = context
            .plan_initialization_with_strings(
                &claim,
                class,
                InitializationPlanLimits {
                    max_dependencies: 100,
                    max_constants: 100,
                },
            )
            .unwrap();
        let mut state = state(plan, class, true, 7, false);
        let mut init = InitExecution {
            claim: &claim,
            state: &mut state,
            owner: 1,
            activations: vec![],
            strings: true,
        };
        let mut f = frame(caller, 10, FrameRole::Ordinary);
        f.stack.reserve_one(10).unwrap();
        let pending = PendingNative {
            role: ClassAssertionNative::DesiredStatus,
            target: query,
            argument: Some(argument),
            next: 13,
            stack_len: 0,
            stack_slots: 0,
        };
        let e = resume_assertion_native(pending, &mut f, &context, &mut init, 10).unwrap_err();
        if kind == 0 {
            assert_eq!(
                e.kind,
                E::UnsupportedVmException(VmExceptionKind::NullPointerException)
            );
        } else {
            assert!(matches!(e.kind, E::StringBody(_)));
        }
        assert_eq!(
            init.state.equality,
            crate::StringEqualityUsage {
                natives_registered: true,
                assertion_queries: 7
            }
        );
        assert_eq!(f.pc, 10);
        assert_eq!(f.stack.len(), 0);
    }
}

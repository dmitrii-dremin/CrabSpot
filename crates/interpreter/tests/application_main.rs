//! One named entry initialization and actual main body share one engine evaluation.
#[path = "../../runtime/tests/member_support/mod.rs"]
mod support;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use support::*;
const DESC: &str = "([Ljava/lang/String;)V";
struct Fixture {
    g: Graph,
    h: ManagedHeap,
    parent: ClassId,
    child: ClassId,
    string: ClassId,
    throwable: ClassId,
    error: ClassId,
    marks: ClassId,
    code_bytes: usize,
}
fn fixture(broken: bool) -> Fixture {
    let mut g = Graph::new();
    let app = g.loader();
    let mut string = Class::plain("java/lang/String");
    string.flags = 0x31;
    let string = g.add(g.boot, string);
    g.bind(app, "java/lang/String", string);
    let throwable = g.add(g.boot, Class::plain("java/lang/Throwable"));
    let error = g.add(
        g.boot,
        Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
    );
    let mut marks = Class::plain("Marks");
    for n in ["order", "entered", "args"] {
        marks.field(n, "I", 9);
    }
    let marks = g.add(app, marks);
    let mut parent = Class::plain("Parent");
    let order = parent.reference(9, "Marks", "order", "I");
    let entered = parent.reference(9, "Marks", "entered", "I");
    let args = parent.reference(9, "Marks", "args", "I");
    let mut init = instruction(0xb2, order);
    init.extend([0x10, 10, 0x68, 4, 0x60]);
    init.extend(instruction(0xb3, order));
    init.push(0xb1);
    let parent_init = init.len();
    parent.method("<clinit>", "()V", 8, &init, 2, 0);
    let mut main = instruction(0xb2, entered);
    main.extend([4, 0x60]);
    main.extend(instruction(0xb3, entered));
    main.extend([0x2a, 0xbe]);
    main.extend(instruction(0xb3, args));
    main.push(0xb1);
    let main_bytes = main.len();
    parent.method("main", DESC, 9, &main, 2, 1);
    let parent = g.add(app, parent);
    let mut child = Class::new("Child", Some("Parent"), 0x21);
    let order = child.reference(9, "Marks", "order", "I");
    let mut init = instruction(0xb2, order);
    init.extend([0x10, 10, 0x68, 5, 0x60]);
    init.extend(instruction(0xb3, order));
    if broken {
        init.extend([1, 0xbf]);
    } else {
        init.push(0xb1);
    }
    let child_init = init.len();
    child.method("<clinit>", "()V", 8, &init, 2, 0);
    let child = g.add(app, child);
    let mut h = g.ready();
    g.bind_core(&mut h);
    Fixture {
        g,
        h,
        parent,
        child,
        string,
        throwable,
        error,
        marks,
        code_bytes: parent_init + main_bytes + child_init,
    }
}
fn run(
    f: &mut Fixture,
    named: ClassId,
    fuel: u64,
    calls: CallLimits,
    wrong_array: bool,
    old: bool,
) -> (
    Result<ExceptionOutcome, ExecutionError>,
    [i32; 3],
    InitializationStatus,
    InitializationStatus,
    InitializationStatus,
) {
    let fields: [FieldId; 3] = ["order", "entered", "args"].map(|n| f.g.field(f.marks, n, "I"));
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
            .unwrap();
    let token = c.resolve_application_main(named).unwrap();
    let statics = c.allocate_static_state(Default::default()).unwrap();
    let exceptions = c.prepare_exception_profile(f.throwable).unwrap();
    let args = c
        .allocate_reference_array(
            ReferenceType::Class(if wrong_array { f.g.object } else { f.string }),
            0,
        )
        .unwrap();
    assert_eq!(c.heap().length(args).unwrap(), 0);
    assert_eq!(
        c.heap().reference_array_type(args).unwrap(),
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Class(if wrong_array { f.g.object } else { f.string }),
            defining_loader: f.g.boot
        }
    );
    let mut session =
        InitializationSession::new(&mut c, &statics, &exceptions, f.error, Default::default())
            .unwrap();
    let result = if old {
        session.run_static(
            token.method(),
            &[Value::Reference(args)],
            ExecutionOptions {
                instruction_budget: fuel,
            },
            calls,
            Default::default(),
        )
    } else {
        session.run_application_main(
            &token,
            args,
            ExecutionOptions {
                instruction_budget: fuel,
            },
            calls,
            Default::default(),
            Default::default(),
        )
    };
    let values =
        fields.map(
            |field| match session.context().inspect_static(&statics, field).unwrap() {
                Value::Int(n) => n,
                _ => panic!("not int"),
            },
        );
    let child = session.status(f.child).unwrap();
    let parent = session.status(f.parent).unwrap();
    let string = session.status(f.string).unwrap();
    (result, values, child, parent, string)
}
#[test]
fn inherited_main_initializes_named_child_and_uses_real_nonnull_typed_array() {
    let mut f = fixture(false);
    let child = f.child;
    let (r, v, c, p, s) = run(&mut f, child, 22, Default::default(), false, false);
    assert_eq!(r.unwrap(), ExceptionOutcome::Returned(None));
    assert_eq!(v, [12, 1, 0]);
    assert_eq!(c, InitializationStatus::Completed);
    assert_eq!(p, InitializationStatus::Completed);
    assert_eq!(s, InitializationStatus::Uninitialized);
}
#[test]
fn old_run_static_still_initializes_only_actual_declarer() {
    let mut f = fixture(false);
    let parent = f.parent;
    let (r, v, c, p, _) = run(&mut f, parent, 100, Default::default(), false, true);
    assert_eq!(r.unwrap(), ExceptionOutcome::Returned(None));
    assert_eq!(v, [1, 1, 0]);
    assert_eq!(p, InitializationStatus::Completed);
    assert_eq!(c, InitializationStatus::Uninitialized);
}
#[test]
fn exact_shared_fuel_and_one_less_do_not_reset_between_child_and_main() {
    for fuel in [21, 22] {
        let mut f = fixture(false);
        let child = f.child;
        let (r, v, c, _, _) = run(&mut f, child, fuel, Default::default(), false, false);
        if fuel == 22 {
            assert_eq!(r.unwrap(), ExceptionOutcome::Returned(None));
        } else {
            assert!(matches!(
                r.unwrap_err().kind,
                ExecutionErrorKind::InstructionBudgetExhausted { limit: 21 }
            ));
        }
        assert_eq!(v, [12, 1, 0]);
        assert_eq!(c, InitializationStatus::Completed);
    }
}
#[test]
fn frame_slot_code_and_argument_preflight_fail_before_any_named_initializer_write() {
    for which in 0..4 {
        let mut f = fixture(false);
        let child = f.child;
        let limits = match which {
            0 => CallLimits {
                max_frames: 0,
                ..Default::default()
            },
            1 => CallLimits {
                max_frame_slots: 2,
                ..Default::default()
            },
            2 => CallLimits {
                max_prepared_code_bytes: 0,
                ..Default::default()
            },
            _ => Default::default(),
        };
        let (r, v, c, p, _) = run(&mut f, child, 100, limits, which == 3, false);
        let e = r.unwrap_err();
        match which {
            0 => assert!(matches!(e.kind, ExecutionErrorKind::FrameLimit { .. })),
            1 => assert!(matches!(e.kind, ExecutionErrorKind::FrameSlotLimit { .. })),
            2 => assert!(matches!(
                e.kind,
                ExecutionErrorKind::PreparedCodeLimit { .. }
            )),
            _ => assert!(matches!(e.kind, ExecutionErrorKind::MethodAccess(_))),
        }
        assert_eq!(e.initialization_class, None);
        assert_eq!(v, [0, 0, 0]);
        assert_eq!(c, InitializationStatus::Uninitialized);
        assert_eq!(p, InitializationStatus::Uninitialized);
    }
}
#[test]
fn exact_shared_code_capacity_counts_main_and_both_initializer_bodies_once() {
    for short in [false, true] {
        let mut f = fixture(false);
        let child = f.child;
        let cap = f.code_bytes - usize::from(short);
        let (r, v, c, _, _) = run(
            &mut f,
            child,
            100,
            CallLimits {
                max_prepared_code_bytes: cap,
                max_frames: 1,
                max_frame_slots: 3,
            },
            false,
            false,
        );
        if short {
            let error = r.unwrap_err();
            assert!(matches!(
                error.kind,
                ExecutionErrorKind::PreparedCodeLimit { .. }
            ));
            assert_eq!(error.initialization_class, Some(f.parent));
            assert_eq!(v, [0, 0, 0]);
            assert_eq!(c, InitializationStatus::Aborted);
        } else {
            assert_eq!(r.unwrap(), ExceptionOutcome::Returned(None));
            assert_eq!(v, [12, 1, 0]);
        }
    }
}
#[test]
fn child_initialization_error_preserves_parent_and_child_prefix_and_never_enters_main() {
    let mut f = fixture(true);
    let child = f.child;
    let (r, v, c, p, _) = run(&mut f, child, 100, Default::default(), false, false);
    let e = r.unwrap_err();
    assert_eq!(
        e.kind,
        ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException)
    );
    assert_eq!(e.initialization_class, Some(child));
    assert_eq!(v, [12, 0, 0]);
    assert_eq!(c, InitializationStatus::Aborted);
    assert_eq!(p, InitializationStatus::Completed);
}

#[test]
fn engine_token_mismatch_is_typed_and_has_no_invented_method_or_initialization() {
    let mut f = fixture(false);
    let ids = f.g.classes.clone();
    let mut other = f.g.heap(&ids);
    other
        .bind_reference_types(
            f.g.domain.registry(),
            f.h.reference_type_universe().unwrap(),
        )
        .unwrap();
    let token = {
        let mut c =
            f.g.domain
                .member_context_unnamed_modules(&mut other, Default::default(), Default::default())
                .unwrap();
        c.resolve_application_main(f.child).unwrap()
    };
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
            .unwrap();
    let statics = c.allocate_static_state(Default::default()).unwrap();
    let exceptions = c.prepare_exception_profile(f.throwable).unwrap();
    let args = c
        .allocate_reference_array(ReferenceType::Class(f.string), 0)
        .unwrap();
    let mut session =
        InitializationSession::new(&mut c, &statics, &exceptions, f.error, Default::default())
            .unwrap();
    let error = session
        .run_application_main(
            &token,
            args,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
        .unwrap_err();
    assert!(
        matches!(error.kind,ExecutionErrorKind::ApplicationMain(e)if *e==ApplicationMainError::TokenMismatch)
    );
    assert_eq!(error.method, None);
    assert_eq!(error.offset, None);
    assert_eq!(error.initialization_class, None);
    assert_eq!(
        session.status(f.child).unwrap(),
        InitializationStatus::Uninitialized
    );
    assert_eq!(session.last_initialization_steps(), 0);
    assert_eq!(session.last_construction_steps(), 0);
}

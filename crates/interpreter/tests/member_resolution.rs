//! Actual bytecodes exercise domain-backed resolution through the unchanged engine.
#[path = "../../runtime/tests/member_support/mod.rs"]
mod support;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, execute_instance_method_body, execute_instance_method_body_with_static_state,
};
use crabspot_runtime::*;
use support::*;
struct Fixture {
    g: Graph,
    h: ManagedHeap,
    d: ClassId,
    e: ClassId,
    ld: LoaderId,
    le: LoaderId,
    site: u16,
    root: MethodId,
    receiver: Reference,
}
fn fixture(opcode: u8, desc: &str, flags: u16, prefix: &[u8]) -> Fixture {
    let mut g = Graph::new();
    let ld = g.loader();
    let lc = g.loader();
    let le = g.loader();
    let tag = if matches!(opcode, 0xb2..=0xb5) { 9 } else { 10 };
    let mut e = Class::plain("e/Base");
    if tag == 9 {
        e.field("item", desc, flags);
    } else {
        let result = desc.as_bytes()[desc.find(')').unwrap() + 1];
        let code = if result == b'V' {
            vec![0xb1]
        } else if matches!(result, b'L' | b'[') {
            vec![1, 0xb0]
        } else {
            vec![0x10, 17, 0xac]
        };
        e.method("item", desc, flags, &code, 1, 8);
    }
    let e = g.add(le, e);
    g.bind(lc, "e/Base", e);
    let c = g.add(lc, Class::new("c/Sub", Some("e/Base"), 0x21));
    g.bind(ld, "c/Sub", c);
    let mut d = Class::new("d/Caller", Some("c/Sub"), 0x21);
    let site = d.reference(tag, "c/Sub", "item", desc);
    let mut code = prefix.to_vec();
    code.extend(instruction(opcode, site));
    if matches!(opcode, 0xb2 | 0xb4) || (tag == 10 && !desc.ends_with('V')) {
        code.push(0x57);
    }
    code.extend([4, 0xac]);
    d.method("run", "()I", 1, &code, 8, 1);
    let d = g.add(ld, d);
    let mut h = g.ready();
    let receiver = h.allocate_instance(d).unwrap();
    let root = g.method(d, "run", "()I");
    Fixture {
        g,
        h,
        d,
        e,
        ld,
        le,
        site,
        root,
        receiver,
    }
}
fn run(f: &mut Fixture) -> Result<O, ExecutionError> {
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(
                &mut f.h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
    execute_instance_method_body(
        &mut c,
        f.root,
        f.receiver,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
    )
}
fn site(f: &Fixture) -> Option<MemberResolutionOutcome> {
    f.g.domain.member_site(f.d, f.site).unwrap()
}
fn split(f: &mut Fixture, n: &str) {
    f.g.add(f.ld, Class::plain(n));
    f.g.add(f.le, Class::plain(n));
}
#[test]
fn direct_field_and_method_conflicts_become_symbolic_linkage_errors() {
    for (op, desc, flags, prefix) in [
        (0xb4, "Lp/K;", 1, vec![0x2a]),
        (0xb5, "Lp/K;", 1, vec![0x2a, 1]),
        (0xb8, "(Lp/K;)Lp/K;", 9, vec![1]),
        (0xb6, "(Lp/K;)Lp/K;", 1, vec![0x2a, 1]),
    ] {
        let mut f = fixture(op, desc, flags, &prefix);
        split(&mut f, "p/K");
        assert_eq!(run(&mut f).unwrap(), O::LinkageError);
        assert_eq!(
            site(&f),
            Some(MemberResolutionOutcome::Failure(
                MemberResolutionFailure::LinkageError
            ))
        );
    }
}
#[test]
fn static_field_conflicts_use_existing_static_capability_path() {
    for (op, prefix) in [(0xb2, vec![]), (0xb3, vec![1])] {
        let mut f = fixture(op, "Lp/K;", 9, &prefix);
        split(&mut f, "p/K");
        let profile =
            f.g.domain
                .member_context_unnamed_modules(
                    &mut f.h,
                    FieldResolutionOptions::default(),
                    LoadingWorkLimits::default(),
                )
                .unwrap()
                .allocate_static_state(StaticStateOptions::default())
                .unwrap();
        let mut c =
            f.g.domain
                .member_context_unnamed_modules(
                    &mut f.h,
                    FieldResolutionOptions::default(),
                    LoadingWorkLimits::default(),
                )
                .unwrap();
        assert_eq!(
            execute_instance_method_body_with_static_state(
                &mut c,
                &profile,
                f.root,
                f.receiver,
                &[],
                ExecutionOptions::default(),
                CallLimits::default()
            )
            .unwrap(),
            O::LinkageError
        );
        drop(c);
        assert!(matches!(
            site(&f),
            Some(MemberResolutionOutcome::Failure(
                MemberResolutionFailure::LinkageError
            ))
        ));
    }
}
#[test]
fn null_argument_and_result_execute_across_loaders_without_loading_leaf() {
    let mut f = fixture(0xb8, "(Lp/K;)Lp/K;", 9, &[1]);
    assert_eq!(run(&mut f).unwrap(), O::Returned(Some(Value::Int(1))));
    assert_eq!(
        site(&f),
        Some(MemberResolutionOutcome::Method(f.g.method(
            f.e,
            "item",
            "(Lp/K;)Lp/K;"
        )))
    );
    assert!(
        f.g.domain
            .context(LoadingWorkLimits::default())
            .constrained(&units("p/K"), f.ld, f.le)
            .unwrap()
    );
    assert_eq!(
        f.g.domain
            .context(LoadingWorkLimits::default())
            .lookup_initiated(f.ld, &units("p/K"))
            .unwrap(),
        None
    );
}
#[test]
fn instance_extraction_precedes_resolution_but_static_resolution_precedes_pops() {
    for (prefix, expected) in [(vec![], 0), (vec![3], 1)] {
        let mut f = fixture(0xb6, "(Lp/K;)Lp/K;", 1, &prefix);
        let e = run(&mut f).unwrap_err();
        match expected {
            0 => assert!(matches!(e.kind, E::StackUnderflow)),
            _ => assert!(matches!(e.kind, E::WrongValueType { .. })),
        };
        assert_eq!(site(&f), None);
    }
    let mut f = fixture(0xb8, "(Lp/K;)Lp/K;", 9, &[]);
    split(&mut f, "p/K");
    assert_eq!(run(&mut f).unwrap(), O::LinkageError);
    assert!(matches!(
        site(&f),
        Some(MemberResolutionOutcome::Failure(
            MemberResolutionFailure::LinkageError
        ))
    ));
    let mut f = fixture(0xb8, "(Lp/K;)Lp/K;", 9, &[]);
    assert!(matches!(run(&mut f).unwrap_err().kind, E::StackUnderflow));
    assert!(matches!(site(&f), Some(MemberResolutionOutcome::Method(_))));
}
#[test]
fn ordinary_unbacked_context_retains_conservative_loading_guard() {
    let mut f = fixture(0xb4, "Lp/K;", 1, &[0x2a]);
    let mut resolver = Resolver(&f.g.intended);
    let mut c = FieldResolutionContext::new_unnamed_modules(
        f.g.domain.registry(),
        &mut f.h,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let error = execute_instance_method_body(
        &mut c,
        f.root,
        f.receiver,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
    )
    .unwrap_err();
    assert!(
        matches!(error.kind,E::FieldAccess(e) if matches!(*e,FieldAccessError::UnsupportedLoadingConstraint{..}))
    );
    assert_eq!(site(&f), None);
}
#[test]
fn same_slot_across_two_caller_methods_and_two_evaluations_is_one_record() {
    let mut g = Graph::new();
    let mut d = Class::plain("D");
    d.field("f", "I", 1);
    let i = d.reference(9, "D", "f", "I");
    let mut body = vec![0x2a];
    body.extend(instruction(0xb4, i));
    body.push(0xac);
    d.method("first", "()I", 1, &body, 1, 1);
    d.method("second", "()I", 1, &body, 1, 1);
    let d = g.add(g.boot, d);
    let mut h = g.ready();
    let r = h.allocate_instance(d).unwrap();
    for n in ["first", "second", "first"] {
        let m = g.method(d, n, "()I");
        let mut c = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
        assert_eq!(
            execute_instance_method_body(
                &mut c,
                m,
                r,
                &[],
                ExecutionOptions::default(),
                CallLimits::default()
            )
            .unwrap(),
            O::Returned(Some(Value::Int(0)))
        );
    }
    assert_eq!(g.domain.member_resolution_usage().sites, 1);
}
#[test]
fn root_methodid_and_zero_fuel_do_not_invent_a_cp_transaction() {
    let mut f = fixture(0xb4, "Lp/K;", 1, &[0x2a]);
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(
                &mut f.h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
    let error = execute_instance_method_body(
        &mut c,
        f.root,
        f.receiver,
        &[],
        ExecutionOptions {
            instruction_budget: 0,
        },
        CallLimits::default(),
    )
    .unwrap_err();
    assert!(matches!(error.kind, E::InstructionBudgetExhausted { .. }));
    drop(c);
    assert_eq!(site(&f), None);
    assert_eq!(f.g.domain.member_resolution_usage().sites, 0);
}

#[test]
fn warm_resolution_keeps_declarer_but_each_receiver_selects_its_own_cp_and_fields() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let caller_loader = g.loader();
    let mut base = Class::plain("Base");
    base.method("m", "(Lp/K;)I", 1, &[4, 0xac], 1, 2);
    let base = g.add(a, base);
    let mut receivers = vec![];
    for (name, value) in [("First", 23), ("Second", 47)] {
        g.bind(b, "Base", base);
        let mut s = Class::new(name, Some("Base"), 0x21);
        s.field("marker", "I", 1);
        let field = s.reference(9, name, "marker", "I");
        let mut code = vec![0x2a];
        code.extend(instruction(0xb4, field));
        code.push(0xac);
        s.method("m", "(Lp/K;)I", 1, &code, 1, 2);
        let s = g.add(b, s);
        receivers.push((s, value));
    }
    g.bind(caller_loader, "Base", base);
    let mut caller = Class::plain("Caller");
    let slot = caller.reference(10, "Base", "m", "(Lp/K;)I");
    let mut code = vec![0x2b, 1];
    code.extend(instruction(0xb6, slot));
    code.push(0xac);
    caller.method("run", "(LBase;)I", 1, &code, 2, 2);
    let caller = g.add(caller_loader, caller);
    let mut h = g.ready();
    g.bind_core(&mut h);
    let root = g.method(caller, "run", "(LBase;)I");
    let r = h.allocate_instance(caller).unwrap();
    let mut actual = vec![];
    for (s, value) in receivers {
        let object = h.allocate_instance(s).unwrap();
        h.put_field(object, g.field(s, "marker", "I"), Value::Int(value))
            .unwrap();
        actual.push((object, value));
    }
    for (receiver, value) in [actual[0], actual[1], actual[0]] {
        let mut c = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
        assert_eq!(
            execute_instance_method_body(
                &mut c,
                root,
                r,
                &[Value::Reference(receiver)],
                ExecutionOptions::default(),
                CallLimits::default()
            )
            .unwrap(),
            O::Returned(Some(Value::Int(value)))
        );
    }
    assert_eq!(
        g.domain.member_site(caller, slot).unwrap(),
        Some(MemberResolutionOutcome::Method(
            g.method(base, "m", "(Lp/K;)I")
        ))
    );
}
#[test]
fn target_decode_and_frame_failures_happen_after_static_cp_success() {
    for which in 0..2 {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let mut target = Class::plain("Target");
        target.method(
            "m",
            "(Lp/K;)V",
            9,
            if which == 0 { &[0x10] } else { &[0xb1] },
            1,
            1,
        );
        let target = g.add(a, target);
        g.bind(b, "Target", target);
        let mut caller = Class::plain("Caller");
        let i = caller.reference(10, "Target", "m", "(Lp/K;)V");
        let mut code = vec![1];
        code.extend(instruction(0xb8, i));
        code.push(0xb1);
        caller.method("run", "()V", 1, &code, 1, 1);
        let caller = g.add(b, caller);
        let mut h = g.ready();
        let r = h.allocate_instance(caller).unwrap();
        let m = g.method(caller, "run", "()V");
        let mut c = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
        let limits = if which == 0 {
            CallLimits::default()
        } else {
            CallLimits {
                max_frames: 1,
                ..Default::default()
            }
        };
        let err =
            execute_instance_method_body(&mut c, m, r, &[], ExecutionOptions::default(), limits)
                .unwrap_err();
        if which == 0 {
            assert!(matches!(err.kind, E::Decode(_)));
        } else {
            assert!(matches!(err.kind, E::FrameLimit { .. }));
        }
        drop(c);
        assert_eq!(
            g.domain.member_site(caller, i).unwrap(),
            Some(MemberResolutionOutcome::Method(
                g.method(target, "m", "(Lp/K;)V")
            ))
        );
    }
}
#[test]
fn successful_field_resolution_survives_later_null_and_wrong_category_uses() {
    let mut f = fixture(0xb4, "Lp/K;", 9, &[0x2a]);
    assert_eq!(run(&mut f).unwrap(), O::IncompatibleClassChangeError);
    assert!(matches!(site(&f), Some(MemberResolutionOutcome::Field(_))));
    let mut f = fixture(0xb4, "Lp/K;", 1, &[1]);
    assert_eq!(run(&mut f).unwrap(), O::NullPointerException);
    assert!(matches!(site(&f), Some(MemberResolutionOutcome::Field(_))));
}

fn exception_classes(g: &mut Graph) -> (ClassId, ClassId) {
    let t = g.add(g.boot, Class::plain("java/lang/Throwable"));
    let e = g.add(
        g.boot,
        Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
    );
    (t, e)
}
#[test]
fn symbolic_linkage_does_not_enter_a_catch_all_in_supplied_exception_profile() {
    use crabspot_interpreter::{
        ExceptionLimits, execute_instance_method_body_with_static_state_and_exceptions,
    };
    for (opcode, desc, flags, prefix) in [
        (0xb4, "Lp/K;", 1, vec![0x2a]),
        (0xb5, "Lp/K;", 1, vec![0x2a, 1]),
        (0xb2, "Lp/K;", 9, vec![]),
        (0xb3, "Lp/K;", 9, vec![1]),
        (0xb8, "(Lp/K;)Lp/K;", 9, vec![1]),
        (0xb6, "(Lp/K;)Lp/K;", 1, vec![0x2a, 1]),
        (0xb7, "(Lp/K;)Lp/K;", 1, vec![0x2a, 1]),
    ] {
        let mut f = fixture(opcode, desc, flags, &prefix);
        split(&mut f, "p/K");
        let (throwable, _) = exception_classes(&mut f.g);
        let mut d = Class::new("CatchCaller", Some("c/Sub"), 0x21);
        let i = d.reference(
            if matches!(opcode, 0xb2..=0xb5) { 9 } else { 10 },
            "c/Sub",
            "item",
            desc,
        );
        let mut body = prefix.clone();
        body.extend(instruction(opcode, i));
        if matches!(opcode, 0xb2 | 0xb4 | 0xb6..=0xb8) {
            body.push(0x57);
        }
        body.extend([4, 0xac]);
        let handler = body.len() as u16;
        body.extend([0x4c, 5, 0xac]);
        d.method("run", "()I", 1, &body, 8, 2);
        d.handler([0, handler, handler, 0]);
        let d = f.g.add(f.ld, d);
        let mut h = f.g.ready();
        f.g.bind_core(&mut h);
        let r = h.allocate_instance(d).unwrap();
        let method = f.g.method(d, "run", "()I");
        let mut c =
            f.g.domain
                .member_context_unnamed_modules(
                    &mut h,
                    FieldResolutionOptions::default(),
                    LoadingWorkLimits::default(),
                )
                .unwrap();
        let statics = c
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        let profile = c.prepare_exception_profile(throwable).unwrap();
        let error = execute_instance_method_body_with_static_state_and_exceptions(
            &mut c,
            &statics,
            &profile,
            method,
            r,
            &[],
            ExecutionOptions::default(),
            CallLimits::default(),
            ExceptionLimits {
                max_dispatch_steps: 0,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            E::UnsupportedVmException(VmExceptionKind::LinkageError)
        );
        assert_eq!(error.method.as_ref().unwrap().owner, Some(d));
        assert_eq!(error.offset, Some(prefix.len() as u32));
        drop(c);
        assert_eq!(
            f.g.domain.member_site(d, i).unwrap(),
            Some(MemberResolutionOutcome::Failure(
                MemberResolutionFailure::LinkageError
            ))
        );
    }
}
#[test]
fn active_use_initializes_actual_field_or_static_method_declarer_only() {
    use crabspot_interpreter::{
        ExceptionLimits, ExceptionOutcome as EO, InitializationLimits, InitializationSession,
        InitializationStatus as IS,
    };
    for opcode in [0xb2, 0xb3, 0xb8] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let caller_loader = g.loader();
        let (throwable, error) = exception_classes(&mut g);
        let mut e = Class::plain("E");
        e.field("marker", "I", 9);
        e.field("item", "Lp/K;", 9);
        e.method("item", "(Lp/K;)V", 9, &[0xb1], 0, 1);
        let mark = e.reference(9, "E", "marker", "I");
        let mut cl = vec![0x10, 7];
        cl.extend(instruction(0xb3, mark));
        cl.push(0xb1);
        e.method("<clinit>", "()V", 8, &cl, 1, 0);
        let e = g.add(a, e);
        g.bind(b, "E", e);
        let mut c = Class::new("C", Some("E"), 0x21);
        c.field("marker", "I", 9);
        let mark = c.reference(9, "C", "marker", "I");
        let mut cl = vec![0x10, 9];
        cl.extend(instruction(0xb3, mark));
        cl.push(0xb1);
        c.method("<clinit>", "()V", 8, &cl, 1, 0);
        let c = g.add(b, c);
        g.bind(caller_loader, "C", c);
        let mut d = Class::plain("D");
        let i = d.reference(
            if opcode == 0xb8 { 10 } else { 9 },
            "C",
            "item",
            if opcode == 0xb8 { "(Lp/K;)V" } else { "Lp/K;" },
        );
        let mut body = if opcode == 0xb2 { vec![] } else { vec![1] };
        body.extend(instruction(opcode, i));
        if opcode == 0xb2 {
            body.push(0x57);
        }
        body.extend([4, 0xac]);
        d.method("run", "()I", 9, &body, 1, 0);
        let d = g.add(caller_loader, d);
        let mut h = g.ready();
        g.bind_core(&mut h);
        let method = g.method(d, "run", "()I");
        let ef = g.field(e, "marker", "I");
        let cf = g.field(c, "marker", "I");
        let mut context = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
        let statics = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let mut session = InitializationSession::new(
            &mut context,
            &statics,
            &exceptions,
            error,
            InitializationLimits::default(),
        )
        .unwrap();
        assert_eq!(
            session
                .run_static(
                    method,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
                .unwrap(),
            EO::Returned(Some(Value::Int(1)))
        );
        assert_eq!(session.status(e).unwrap(), IS::Completed);
        assert_eq!(session.status(c).unwrap(), IS::Uninitialized);
        assert_eq!(
            session.context().inspect_static(&statics, ef).unwrap(),
            Value::Int(7)
        );
        assert_eq!(
            session.context().inspect_static(&statics, cf).unwrap(),
            Value::Int(0)
        );
        drop(session);
        drop(context);
        assert!(matches!(
            g.domain.member_site(d, i).unwrap(),
            Some(MemberResolutionOutcome::Field(_) | MemberResolutionOutcome::Method(_))
        ));
    }
}

#[test]
fn constructor_ancestor_resolution_is_cached_before_owner_restriction() {
    use crabspot_interpreter::{ExceptionLimits, InitializationLimits, InitializationSession};
    for split_leaf in [false, true] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let dloader = g.loader();
        let (throwable, error) = exception_classes(&mut g);
        let mut base = Class::plain("E");
        let oi = base.reference(10, "java/lang/Object", "<init>", "()V");
        let mut init = vec![0x2a];
        init.extend(instruction(0xb7, oi));
        init.push(0xb1);
        base.method("<init>", "(Lp/K;)V", 1, &init, 1, 2);
        let e = g.add(a, base);
        g.bind(b, "E", e);
        let c = g.add(b, Class::new("C", Some("E"), 0x21));
        g.bind(dloader, "C", c);
        let mut d = Class::plain("D");
        let class = d.class("C");
        let init = d.reference(10, "C", "<init>", "(Lp/K;)V");
        let mut code = instruction(0xbb, class);
        code.extend([0x59, 1]);
        code.extend(instruction(0xb7, init));
        code.extend([0x57, 4, 0xac]);
        d.method("run", "()I", 9, &code, 3, 0);
        let d = g.add(dloader, d);
        if split_leaf {
            g.add(a, Class::plain("p/K"));
            g.add(dloader, Class::plain("p/K"));
        }
        let mut h = g.ready();
        g.bind_core(&mut h);
        let method = g.method(d, "run", "()I");
        let mut context = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
        let statics = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let mut session = InitializationSession::new(
            &mut context,
            &statics,
            &exceptions,
            error,
            InitializationLimits::default(),
        )
        .unwrap();
        let failure = session
            .run_static(
                method,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits::default(),
            )
            .unwrap_err();
        assert_eq!(
            failure.kind,
            E::UnsupportedVmException(if split_leaf {
                VmExceptionKind::LinkageError
            } else {
                VmExceptionKind::NoSuchMethodError
            })
        );
        assert_eq!(failure.method.as_ref().unwrap().owner, Some(d));
        assert_eq!(failure.offset, Some(5));
        drop(session);
        drop(context);
        let terminal = g.domain.member_site(d, init).unwrap();
        if split_leaf {
            assert_eq!(
                terminal,
                Some(MemberResolutionOutcome::Failure(
                    MemberResolutionFailure::LinkageError
                ))
            );
        } else {
            assert_eq!(
                terminal,
                Some(MemberResolutionOutcome::Method(
                    g.method(e, "<init>", "(Lp/K;)V")
                ))
            );
            assert!(
                g.domain
                    .context(LoadingWorkLimits::default())
                    .constrained(&units("p/K"), a, dloader)
                    .unwrap()
            );
        }
    }
}
#[test]
fn interface_resolution_and_current_default_body_use_checked_context() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let c = g.loader();
    let mut it = Class::new("I", Some("java/lang/Object"), 0x601);
    it.method("m", "(Lp/K;)I", 1, &[0x10, 37, 0xac], 1, 2);
    let it = g.add(a, it);
    g.bind(b, "I", it);
    let mut receiver = Class::plain("Receiver");
    receiver.interface("I");
    let receiver = g.add(b, receiver);
    g.bind(c, "I", it);
    g.bind(c, "Receiver", receiver);
    let mut caller = Class::plain("Caller");
    let i = caller.reference(11, "I", "m", "(Lp/K;)I");
    let mut body = vec![0x2b, 1];
    body.extend(instruction(0xb9, i));
    body.extend([2, 0, 0xac]);
    caller.method("run", "(LReceiver;)I", 1, &body, 2, 2);
    let caller = g.add(c, caller);
    let mut h = g.ready();
    g.bind_core(&mut h);
    let r = h.allocate_instance(caller).unwrap();
    let target = h.allocate_instance(receiver).unwrap();
    let root = g.method(caller, "run", "(LReceiver;)I");
    let mut context = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            root,
            r,
            &[Value::Reference(target)],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        O::Returned(Some(Value::Int(37)))
    );
    drop(context);
    assert_eq!(
        g.domain.member_site(caller, i).unwrap(),
        Some(MemberResolutionOutcome::Method(
            g.method(it, "m", "(Lp/K;)I")
        ))
    );
}

#[test]
fn earlier_effects_and_terminal_sites_survive_missing_host_fact_then_repair() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let mut e = Class::plain("Owner");
    e.field("item", "I", 1);
    let e = g.add(a, e);
    let mut d = Class::plain("Caller");
    d.field("effect", "I", 1);
    let effect = d.reference(9, "Caller", "effect", "I");
    let target = d.reference(9, "Owner", "item", "I");
    let mut code = vec![0x2a, 0x10, 31];
    code.extend(instruction(0xb5, effect));
    code.push(1);
    code.extend(instruction(0xb4, target));
    code.push(0xac);
    d.method("run", "()I", 1, &code, 2, 1);
    let d = g.add(b, d);
    let mut h = g.ready();
    let r = h.allocate_instance(d).unwrap();
    let method = g.method(d, "run", "()I");
    let field = g.field(d, "effect", "I");
    let mut context = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap();
    let error = execute_instance_method_body(
        &mut context,
        method,
        r,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
    )
    .unwrap_err();
    assert!(
        matches!(error.kind,E::FieldAccess(e) if matches!(*e,FieldAccessError::MemberResolution(_)))
    );
    drop(context);
    assert_eq!(h.get_field(r, field).unwrap(), Value::Int(31));
    assert!(matches!(
        g.domain.member_site(d, effect).unwrap(),
        Some(MemberResolutionOutcome::Field(_))
    ));
    assert_eq!(g.domain.member_site(d, target).unwrap(), None);
    g.bind(b, "Owner", e);
    let mut context = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            method,
            r,
            &[],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        O::NullPointerException
    );
    drop(context);
    assert_eq!(h.get_field(r, field).unwrap(), Value::Int(31));
    assert!(matches!(
        g.domain.member_site(d, target).unwrap(),
        Some(MemberResolutionOutcome::Field(_))
    ));
}
#[test]
fn actual_caught_and_uncaught_throw_keep_successful_method_resolution_and_prior_effects() {
    use crabspot_interpreter::{
        ExceptionLimits, ExceptionOutcome as EO, execute_instance_method_body_with_exceptions,
    };
    for catches in [false, true] {
        let mut g = Graph::new();
        let a = g.loader();
        let b = g.loader();
        let (throwable, _) = exception_classes(&mut g);
        g.bind(a, "java/lang/Throwable", throwable);
        g.bind(b, "java/lang/Throwable", throwable);
        let mut e = Class::plain("E");
        e.method("raise", "(Ljava/lang/Throwable;)V", 9, &[0x2a, 0xbf], 1, 1);
        let e = g.add(a, e);
        g.bind(b, "E", e);
        let mut d = Class::plain("D");
        d.field("effect", "I", 1);
        let effect = d.reference(9, "D", "effect", "I");
        let target = d.reference(10, "E", "raise", "(Ljava/lang/Throwable;)V");
        let mut body = vec![0x2a, 0x10, 31];
        body.extend(instruction(0xb5, effect));
        body.push(0x2b);
        body.extend(instruction(0xb8, target));
        body.extend([4, 0xac]);
        let at = body.len() as u16;
        if catches {
            body.extend([0x4d, 5, 0xac]);
        }
        d.method("run", "(Ljava/lang/Throwable;)I", 1, &body, 2, 3);
        if catches {
            d.handler([0, at, at, 0]);
        }
        let d = g.add(b, d);
        let mut h = g.ready();
        g.bind_core(&mut h);
        let receiver = h.allocate_instance(d).unwrap();
        let thrown = h.allocate_instance(throwable).unwrap();
        let method = g.method(d, "run", "(Ljava/lang/Throwable;)I");
        let field = g.field(d, "effect", "I");
        let mut context = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
        let profile = context.prepare_exception_profile(throwable).unwrap();
        let outcome = execute_instance_method_body_with_exceptions(
            &mut context,
            &profile,
            method,
            receiver,
            &[Value::Reference(thrown)],
            ExecutionOptions::default(),
            CallLimits::default(),
            ExceptionLimits::default(),
        )
        .unwrap();
        assert_eq!(
            outcome,
            if catches {
                EO::Returned(Some(Value::Int(2)))
            } else {
                EO::Uncaught(thrown)
            }
        );
        drop(context);
        assert_eq!(h.get_field(receiver, field).unwrap(), Value::Int(31));
        assert_eq!(
            g.domain.member_site(d, target).unwrap(),
            Some(MemberResolutionOutcome::Method(g.method(
                e,
                "raise",
                "(Ljava/lang/Throwable;)V"
            )))
        );
    }
}

#[test]
fn checked_context_does_not_enable_static_opcodes_without_static_profile() {
    for (op, prefix) in [(0xb2, vec![]), (0xb3, vec![1])] {
        let mut f = fixture(op, "Lp/K;", 9, &prefix);
        let error = run(&mut f).unwrap_err();
        assert!(matches!(error.kind,E::UnsupportedInstruction(x) if x==op));
        assert_eq!(site(&f), None);
        assert!(
            !f.g.domain
                .context(LoadingWorkLimits::default())
                .constrained(&units("p/K"), f.ld, f.le)
                .unwrap()
        );
    }
}
#[test]
fn host_nominal_root_without_symbolic_call_creates_no_resolution_agreement() {
    let mut g = Graph::new();
    let mut d = Class::plain("D");
    d.method("root", "(Lp/K;)I", 1, &[4, 0xac], 1, 2);
    let d = g.add(g.boot, d);
    let mut h = g.ready();
    let receiver = h.allocate_instance(d).unwrap();
    let root = g.method(d, "root", "(Lp/K;)I");
    let before = g.domain.usage();
    let mut c = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut c,
            root,
            receiver,
            &[Value::Reference(Reference::null())],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        O::Returned(Some(Value::Int(1)))
    );
    drop(c);
    assert_eq!(g.domain.usage(), before);
    assert_eq!(g.domain.member_resolution_usage().sites, 0);
}

#[test]
fn abstract_selection_failure_keeps_actual_declaration_and_constraint() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let mut base = Class::new("E", Some("java/lang/Object"), 0x421);
    base.field("entered", "I", 1);
    base.method("m", "(Lp/K;)V", 0x401, &[], 0, 2);
    let e = g.add(a, base);
    g.bind(b, "E", e);
    let mut d = Class::new("D", Some("E"), 0x21);
    let slot = d.reference(10, "E", "m", "(Lp/K;)V");
    let mut body = vec![0x2a, 1];
    body.extend(instruction(0xb6, slot));
    body.push(0xb1);
    d.method("run", "()V", 1, &body, 2, 1);
    let d = g.add(b, d);
    let mut h = g.ready();
    let r = h.allocate_instance(d).unwrap();
    let root = g.method(d, "run", "()V");
    let field = g.field(e, "entered", "I");
    let mut c = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut c,
            root,
            r,
            &[],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        O::AbstractMethodError
    );
    drop(c);
    assert_eq!(
        g.domain.member_site(d, slot).unwrap(),
        Some(MemberResolutionOutcome::Method(
            g.method(e, "m", "(Lp/K;)V")
        ))
    );
    assert!(
        g.domain
            .context(LoadingWorkLimits::default())
            .constrained(&units("p/K"), a, b)
            .unwrap()
    );
    assert_eq!(h.get_field(r, field).unwrap(), Value::Int(0));
}
#[test]
fn conflicting_defaults_leave_resolved_interface_cached_and_both_bodies_unentered() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let rloader = g.loader();
    let dloader = g.loader();
    let mut interfaces = vec![];
    for (name, loader, n) in [("I", a, 17), ("J", b, 29)] {
        let mut it = Class::new(name, Some("java/lang/Object"), 0x601);
        let field = it.reference(9, "R", "entered", "I");
        let mut body = vec![0x2a, 0x10, n];
        body.extend(instruction(0xb5, field));
        body.extend([0x10, n, 0xac]);
        it.method("m", "(Lp/K;)I", 1, &body, 2, 2);
        let id = g.add(loader, it);
        interfaces.push((id, field));
    }
    for (name, (id, _)) in ["I", "J"].into_iter().zip(&interfaces) {
        g.bind(rloader, name, *id);
    }
    let mut receiver = Class::plain("R");
    receiver.interface("I");
    receiver.interface("J");
    receiver.field("entered", "I", 1);
    let receiver = g.add(rloader, receiver);
    g.bind(a, "R", receiver);
    g.bind(b, "R", receiver);
    g.bind(dloader, "R", receiver);
    g.bind(dloader, "I", interfaces[0].0);
    let mut d = Class::plain("D");
    let slot = d.reference(11, "I", "m", "(Lp/K;)I");
    let mut code = vec![0x2b, 1];
    code.extend(instruction(0xb9, slot));
    code.extend([2, 0, 0xac]);
    d.method("run", "(LR;)I", 1, &code, 2, 2);
    let d = g.add(dloader, d);
    let mut h = g.ready();
    g.bind_core(&mut h);
    let root = g.method(d, "run", "(LR;)I");
    let r = h.allocate_instance(d).unwrap();
    let actual = h.allocate_instance(receiver).unwrap();
    let entered = g.field(receiver, "entered", "I");
    let mut c = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut c,
            root,
            r,
            &[Value::Reference(actual)],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        O::IncompatibleClassChangeError
    );
    drop(c);
    assert_eq!(
        g.domain.member_site(d, slot).unwrap(),
        Some(MemberResolutionOutcome::Method(g.method(
            interfaces[0].0,
            "m",
            "(Lp/K;)I"
        )))
    );
    assert!(
        g.domain
            .context(LoadingWorkLimits::default())
            .constrained(&units("p/K"), a, dloader)
            .unwrap()
    );
    assert_eq!(h.get_field(actual, entered).unwrap(), Value::Int(0));
    for (owner, slot) in interfaces {
        assert_eq!(g.domain.member_site(owner, slot).unwrap(), None);
    }
}
#[test]
fn failing_target_initializer_keeps_b8_success_prefix_state_and_target_body_unentered() {
    use crabspot_interpreter::{
        ExceptionLimits, InitializationLimits, InitializationSession, InitializationStatus as IS,
    };
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let dloader = g.loader();
    let (throwable, error) = exception_classes(&mut g);
    let mut e = Class::plain("E");
    e.field("started", "I", 9);
    e.field("entered", "I", 9);
    let started = e.reference(9, "E", "started", "I");
    let entered = e.reference(9, "E", "entered", "I");
    let mut cl = vec![0x10, 31];
    cl.extend(instruction(0xb3, started));
    cl.extend([4, 3, 0x6c, 0x57, 0xb1]);
    e.method("<clinit>", "()V", 8, &cl, 2, 0);
    let mut body = vec![0x10, 17];
    body.extend(instruction(0xb3, entered));
    body.push(0xb1);
    e.method("m", "(Lp/K;)V", 9, &body, 1, 1);
    let e = g.add(a, e);
    g.bind(b, "E", e);
    let c = g.add(b, Class::new("C", Some("E"), 0x21));
    g.bind(dloader, "C", c);
    let mut d = Class::plain("D");
    let slot = d.reference(10, "C", "m", "(Lp/K;)V");
    let mut body = vec![1];
    body.extend(instruction(0xb8, slot));
    body.extend([4, 0xac]);
    d.method("run", "()I", 9, &body, 1, 0);
    let d = g.add(dloader, d);
    let mut h = g.ready();
    g.bind_core(&mut h);
    let root = g.method(d, "run", "()I");
    let started = g.field(e, "started", "I");
    let entered = g.field(e, "entered", "I");
    let mut context = g
        .domain
        .member_context_unnamed_modules(
            &mut h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap();
    let statics = context
        .allocate_static_state(StaticStateOptions::default())
        .unwrap();
    let exceptions = context.prepare_exception_profile(throwable).unwrap();
    let mut session = InitializationSession::new(
        &mut context,
        &statics,
        &exceptions,
        error,
        InitializationLimits::default(),
    )
    .unwrap();
    let failure = session
        .run_static(
            root,
            &[],
            ExecutionOptions::default(),
            CallLimits::default(),
            ExceptionLimits::default(),
        )
        .unwrap_err();
    assert_eq!(
        failure.kind,
        E::UnsupportedVmException(VmExceptionKind::ArithmeticException)
    );
    assert_eq!(session.status(e).unwrap(), IS::Aborted);
    assert_eq!(session.status(c).unwrap(), IS::Uninitialized);
    assert_eq!(
        session.context().inspect_static(&statics, started).unwrap(),
        Value::Int(31)
    );
    assert_eq!(
        session.context().inspect_static(&statics, entered).unwrap(),
        Value::Int(0)
    );
    drop(session);
    drop(context);
    assert_eq!(
        g.domain.member_site(d, slot).unwrap(),
        Some(MemberResolutionOutcome::Method(
            g.method(e, "m", "(Lp/K;)V")
        ))
    );
    assert!(
        g.domain
            .context(LoadingWorkLimits::default())
            .constrained(&units("p/K"), a, dloader)
            .unwrap()
    );
}

#[test]
fn checked_nonnull_field_roundtrip_uses_declarer_identity_and_preserves_children() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let dloader = g.loader();
    let k = g.add(a, Class::plain("p/K"));
    let other = g.add(b, Class::plain("p/K"));
    assert_ne!(k, other);
    g.bind(dloader, "p/K", k);
    let mut e = Class::plain("E");
    e.field("f", "Lp/K;", 1);
    let e = g.add(a, e);
    g.bind(b, "E", e);
    let c = g.add(b, Class::new("C", Some("E"), 0x21));
    g.bind(dloader, "C", c);
    let mut d = Class::new("D", Some("C"), 0x21);
    let slot = d.reference(9, "C", "f", "Lp/K;");
    let mut body = vec![0x2a, 0x2b];
    body.extend(instruction(0xb5, slot));
    body.push(0x2a);
    body.extend(instruction(0xb4, slot));
    body.push(0xb0);
    d.method("round", "(Lp/K;)Lp/K;", 1, &body, 2, 2);
    let d = g.add(dloader, d);
    let mut h = g.ready();
    g.bind_core(&mut h);
    let receiver = h.allocate_instance(d).unwrap();
    let value = h.allocate_instance(k).unwrap();
    let root = g.method(d, "round", "(Lp/K;)Lp/K;");
    for argument in [value, Reference::null(), value] {
        let mut context = g
            .domain
            .member_context_unnamed_modules(
                &mut h,
                FieldResolutionOptions::default(),
                LoadingWorkLimits::default(),
            )
            .unwrap();
        assert_eq!(
            execute_instance_method_body(
                &mut context,
                root,
                receiver,
                &[Value::Reference(argument)],
                ExecutionOptions::default(),
                CallLimits::default()
            )
            .unwrap(),
            O::Returned(Some(Value::Reference(argument)))
        );
        drop(context);
        assert_eq!(
            h.get_field(receiver, g.field(e, "f", "Lp/K;")).unwrap(),
            Value::Reference(argument)
        );
        let children = h.children(receiver).unwrap().collect::<Vec<_>>();
        if argument.is_null() {
            assert!(children.is_empty());
        } else {
            assert_eq!(children, vec![value.heap_ref().unwrap()]);
        }
    }
    assert_eq!(g.domain.member_resolution_usage().sites, 1);
    assert!(
        !g.domain
            .context(LoadingWorkLimits::default())
            .constrained(&units("p/K"), b, dloader)
            .unwrap()
    );
}

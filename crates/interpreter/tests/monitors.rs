//! Independent monitor execution, transition ordering and per-evaluation limits.
#[path = "../../runtime/tests/member_support/mod.rs"]
mod support;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use support::*;
const OBJECT: &str = "Ljava/lang/Object;";
struct World {
    g: Graph,
    h: ManagedHeap,
    owner: ClassId,
    throwable: ClassId,
    error: ClassId,
}
impl World {
    fn new(mut c: Class) -> Self {
        c.field("mark", "I", 9);
        let mut g = Graph::new();
        let app = g.loader();
        let throwable = g.add(g.boot, Class::plain("java/lang/Throwable"));
        let error = g.add(
            g.boot,
            Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        g.bind(app, "java/lang/Throwable", throwable);
        g.bind(app, "java/lang/Error", error);
        let owner = g.add(app, c);
        let mut h = g.ready();
        g.bind_core(&mut h);
        Self {
            g,
            h,
            owner,
            throwable,
            error,
        }
    }
    fn object(&mut self) -> Reference {
        self.h.allocate_instance(self.owner).unwrap()
    }
    fn with<R>(
        &mut self,
        monitors: Option<MonitorLimits>,
        f: impl FnOnce(&mut InitializationSession<'_, '_>, StaticStateProfile) -> R,
    ) -> R {
        let mut c = self
            .g
            .domain
            .member_context_unnamed_modules(&mut self.h, Default::default(), Default::default())
            .unwrap();
        let statics = c.allocate_static_state(Default::default()).unwrap();
        let exceptions = c.prepare_exception_profile(self.throwable).unwrap();
        let mut s = if let Some(limits) = monitors {
            InitializationSession::new_with_monitors(
                &mut c,
                &statics,
                &exceptions,
                self.error,
                Default::default(),
                limits,
            )
        } else {
            InitializationSession::new(
                &mut c,
                &statics,
                &exceptions,
                self.error,
                Default::default(),
            )
        }
        .unwrap();
        f(&mut s, statics)
    }
    fn run(
        &mut self,
        name: &str,
        desc: &str,
        args: &[Value],
        limits: MonitorLimits,
    ) -> Result<ExceptionOutcome, ExecutionError> {
        let m = self.g.method(self.owner, name, desc);
        self.with(Some(limits), |s, _| {
            s.run_static(
                m,
                args,
                Default::default(),
                Default::default(),
                Default::default(),
            )
        })
    }
}
fn class_c() -> Class {
    Class::plain("C")
}
fn pair() -> Class {
    let mut c = class_c();
    c.method(
        "run",
        "(Ljava/lang/Object;Ljava/lang/Object;)I",
        9,
        &[0x2a, 0xc2, 0x2b, 0xc2, 0x2a, 0xc3, 0x2b, 0xc3, 4, 0xac],
        1,
        2,
    );
    c
}
fn cap(resource: MonitorResource, n: u64) -> MonitorLimits {
    let mut l = MonitorLimits::default();
    match resource {
        MonitorResource::Steps => l.max_steps = n,
        MonitorResource::ActiveMonitors => l.max_active_monitors = n,
        MonitorResource::ActiveClaims => l.max_active_claims = n,
        MonitorResource::Recursion => l.max_recursion = n,
    }
    l
}
fn limit(e: &ExecutionError, resource: MonitorResource, n: u64, required: u64) {
    assert_eq!(
        e.kind,
        ExecutionErrorKind::Monitor(MonitorError::Limit {
            resource,
            limit: n,
            required
        })
    );
}
fn imse(e: &ExecutionError, pc: u32) {
    assert_eq!(
        e.kind,
        ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::IllegalMonitorStateException)
    );
    assert_eq!(e.offset, Some(pc));
}
#[test]
fn crossed_pairs_are_balanced_per_identity_and_all_four_exact_ceilings_are_real() {
    for (resource, exact, short, pc) in [
        (MonitorResource::Steps, 4, 3, 7),
        (MonitorResource::ActiveMonitors, 2, 1, 3),
        (MonitorResource::ActiveClaims, 2, 1, 3),
    ] {
        let mut w = World::new(pair());
        let a = w.object();
        let b = w.object();
        assert_eq!(
            w.run(
                "run",
                "(Ljava/lang/Object;Ljava/lang/Object;)I",
                &[Value::Reference(a), Value::Reference(b)],
                cap(resource, exact)
            )
            .unwrap(),
            ExceptionOutcome::Returned(Some(Value::Int(1)))
        );
        let mut w = World::new(pair());
        let a = w.object();
        let b = w.object();
        let e = w
            .run(
                "run",
                "(Ljava/lang/Object;Ljava/lang/Object;)I",
                &[Value::Reference(a), Value::Reference(b)],
                cap(resource, short),
            )
            .unwrap_err();
        limit(&e, resource, short, exact);
        assert_eq!(e.offset, Some(pc));
    }
    // Same identity has one live key and claim but requires recursion depth two.
    let mut w = World::new(pair());
    let a = w.object();
    let l = MonitorLimits {
        max_steps: 4,
        max_active_monitors: 1,
        max_active_claims: 1,
        max_recursion: 2,
    };
    assert_eq!(
        w.run(
            "run",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
            &[Value::Reference(a); 2],
            l
        )
        .unwrap(),
        ExceptionOutcome::Returned(Some(Value::Int(1)))
    );
    let mut w = World::new(pair());
    let a = w.object();
    let e = w
        .run(
            "run",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
            &[Value::Reference(a); 2],
            MonitorLimits {
                max_recursion: 1,
                ..l
            },
        )
        .unwrap_err();
    limit(&e, MonitorResource::Recursion, 1, 2);
    assert_eq!(e.offset, Some(3));
}
#[test]
fn null_type_and_underflow_precede_steps_and_nonnull_exit_pays_before_ownership_error() {
    for opcode in [0xc2, 0xc3] {
        for (prefix, expected, pc) in [
            (
                vec![1],
                ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException),
                1,
            ),
            (
                vec![3],
                ExecutionErrorKind::WrongValueType {
                    expected: ValueKind::Reference,
                    actual: ValueKind::Int,
                },
                1,
            ),
            (vec![], ExecutionErrorKind::StackUnderflow, 0),
        ] {
            let mut c = class_c();
            let mut body = prefix;
            body.extend([opcode, 0xb1]);
            c.method("run", "()V", 9, &body, 1, 0);
            let mut w = World::new(c);
            let e = w
                .run("run", "()V", &[], cap(MonitorResource::Steps, 0))
                .unwrap_err();
            assert_eq!(e.kind, expected);
            assert_eq!(e.offset, Some(pc));
        }
    }
    for n in [0, 1] {
        let mut c = class_c();
        c.method("run", "(Ljava/lang/Object;)V", 9, &[0x2a, 0xc3, 0xb1], 1, 1);
        let mut w = World::new(c);
        let r = w.object();
        let e = w
            .run(
                "run",
                "(Ljava/lang/Object;)V",
                &[Value::Reference(r)],
                cap(MonitorResource::Steps, n),
            )
            .unwrap_err();
        if n == 0 {
            limit(&e, MonitorResource::Steps, 0, 1)
        } else {
            imse(&e, 1)
        }
    }
}
#[test]
fn explicit_frame_claim_cannot_be_consumed_by_a_callee_and_implicit_receipt_is_not_explicit() {
    for implicit in [false, true] {
        let mut c = class_c();
        let exit = c.reference(10, "C", "exit", "(Ljava/lang/Object;)V");
        c.method(
            "exit",
            "(Ljava/lang/Object;)V",
            if implicit { 0x29 } else { 9 },
            &[0x2a, 0xc3, 0xb1],
            1,
            1,
        );
        let mut body = if implicit { vec![] } else { vec![0x2a, 0xc2] };
        body.push(0x2a);
        body.extend(instruction(0xb8, exit));
        body.push(0xb1);
        c.method("run", "(Ljava/lang/Object;)V", 9, &body, 1, 1);
        let mut w = World::new(c);
        let r = w.object();
        let e = w
            .run(
                "run",
                "(Ljava/lang/Object;)V",
                &[Value::Reference(r)],
                Default::default(),
            )
            .unwrap_err();
        imse(&e, 1);
        assert_eq!(e.method.as_ref().unwrap().name.escaped(), "exit");
    }
    // Receiver, unlike a static class key, exactly identifies the implicit lock.
    let mut c = class_c();
    let m = c.reference(10, "C", "m", "()V");
    c.method("m", "()V", 0x21, &[0x2a, 0xc3, 0xb1], 1, 1);
    let mut body = vec![0x2a];
    body.extend(instruction(0xb6, m));
    body.push(0xb1);
    c.method("run", "(LC;)V", 9, &body, 1, 1);
    let mut w = World::new(c);
    let r = w.object();
    let e = w
        .run("run", "(LC;)V", &[Value::Reference(r)], Default::default())
        .unwrap_err();
    imse(&e, 1);
}
#[test]
fn six_returns_preserve_values_and_unbalanced_returns_fail_at_the_real_return() {
    for (desc, load, ret, arg, width) in [
        ("I", 0x1b, 0xac, Value::Int(-17), 1),
        ("J", 0x1f, 0xad, Value::Long(i64::MIN), 2),
        ("F", 0x23, 0xae, Value::Float(0xff812345), 1),
        ("D", 0x27, 0xaf, Value::Double(0xfff0123456789abc), 2),
        (OBJECT, 0x2b, 0xb0, Value::Reference(Reference::null()), 1),
    ] {
        for balanced in [true, false] {
            let mut c = class_c();
            let mut body = vec![0x2a, 0xc2];
            if balanced {
                body.extend([0x2a, 0xc3]);
            }
            body.extend([load, ret]);
            c.method(
                "run",
                &format!("(Ljava/lang/Object;{desc}){desc}"),
                9,
                &body,
                2,
                width + 1,
            );
            let mut w = World::new(c);
            let r = w.object();
            let result = w.run(
                "run",
                &format!("(Ljava/lang/Object;{desc}){desc}"),
                &[Value::Reference(r), arg],
                Default::default(),
            );
            if balanced {
                assert_eq!(result.unwrap(), ExceptionOutcome::Returned(Some(arg)))
            } else {
                imse(&result.unwrap_err(), 3)
            }
        }
    }
    for balanced in [true, false] {
        let mut c = class_c();
        let body = if balanced {
            vec![0x2a, 0xc2, 0x2a, 0xc3, 0xb1]
        } else {
            vec![0x2a, 0xc2, 0xb1]
        };
        c.method("run", "(Ljava/lang/Object;)V", 9, &body, 1, 1);
        let mut w = World::new(c);
        let r = w.object();
        let result = w.run(
            "run",
            "(Ljava/lang/Object;)V",
            &[Value::Reference(r)],
            Default::default(),
        );
        if balanced {
            assert_eq!(result.unwrap(), ExceptionOutcome::Returned(None))
        } else {
            imse(&result.unwrap_err(), 2)
        }
    }
}
#[test]
fn arrays_are_real_lock_keys_and_balanced_completion_needs_no_extra_step() {
    for reference_array in [false, true] {
        let mut c = class_c();
        c.method(
            "run",
            "(Ljava/lang/Object;)V",
            9,
            &[0x2a, 0xc2, 0x2a, 0xc3, 0xb1],
            1,
            1,
        );
        let mut w = World::new(c);
        let r = if reference_array {
            w.g.domain
                .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
                .unwrap()
                .allocate_reference_array(ReferenceType::Class(w.g.object), 0)
                .unwrap()
        } else {
            w.h.allocate_primitive_array(PrimitiveArrayKind::Long, 0)
                .unwrap()
        };
        assert_eq!(
            w.run(
                "run",
                "(Ljava/lang/Object;)V",
                &[Value::Reference(r)],
                cap(MonitorResource::Steps, 2)
            )
            .unwrap(),
            ExceptionOutcome::Returned(None)
        );
    }
}

#[test]
fn synchronized_six_return_kinds_pay_exactly_entry_and_exit_and_do_not_change_bits() {
    for (t, load, ret, arg, width) in [
        ("I", 0x1a, 0xac, Value::Int(-19), 1),
        ("J", 0x1e, 0xad, Value::Long(i64::MAX), 2),
        ("F", 0x22, 0xae, Value::Float(0x80000000), 1),
        ("D", 0x26, 0xaf, Value::Double(0x7ff0123456789abc), 2),
        (OBJECT, 0x2a, 0xb0, Value::Reference(Reference::null()), 1),
    ] {
        let desc = format!("({t}){t}");
        let mut c = class_c();
        c.method("run", &desc, 0x29, &[load, ret], width, width);
        let mut w = World::new(c);
        assert_eq!(
            w.run("run", &desc, &[arg], cap(MonitorResource::Steps, 2))
                .unwrap(),
            ExceptionOutcome::Returned(Some(arg))
        );
    }
    let mut c = class_c();
    c.method("run", "()V", 0x29, &[0xb1], 0, 0);
    let mut w = World::new(c);
    let e = w
        .run("run", "()V", &[], cap(MonitorResource::Steps, 1))
        .unwrap_err();
    limit(&e, MonitorResource::Steps, 1, 2);
    assert_eq!(e.offset, Some(0));
    let mut c = class_c();
    c.method(
        "run",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        0x29,
        &[0x2a, 0xb0],
        1,
        1,
    );
    let mut w = World::new(c);
    let r = w.object();
    assert_eq!(
        w.run(
            "run",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            &[Value::Reference(r)],
            Default::default()
        )
        .unwrap(),
        ExceptionOutcome::Returned(Some(Value::Reference(r)))
    );
}
#[test]
fn roots_and_nested_acquisition_failures_keep_the_real_method_and_invocation_pc() {
    let mut c = class_c();
    c.method("target", "()V", 0x29, &[0xb1], 0, 0);
    let target = c.reference(10, "C", "target", "()V");
    let mut body = vec![0];
    body.extend(instruction(0xb8, target));
    body.push(0xb1);
    c.method("run", "()V", 9, &body, 0, 0);
    let mut w = World::new(c);
    let run = w.g.method(w.owner, "run", "()V");
    let target = w.g.method(w.owner, "target", "()V");
    let owner = w.owner;
    w.with(Some(cap(MonitorResource::Steps, 0)), |s, _| {
        let e = s
            .run_static(
                target,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        limit(&e, MonitorResource::Steps, 0, 1);
        assert_eq!(e.offset, None);
        assert_eq!(e.method.as_ref().unwrap().name.escaped(), "target");
        assert!(e.call_site.is_none());
        assert_eq!(s.status(owner).unwrap(), InitializationStatus::Completed);
        let e = s
            .run_static(
                run,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        limit(&e, MonitorResource::Steps, 0, 1);
        assert_eq!(e.offset, Some(1));
        assert_eq!(e.method.as_ref().unwrap().name.escaped(), "run");
        let site = e.call_site.as_ref().unwrap();
        assert_eq!(site.offset, 1);
        assert_eq!(site.method.name.escaped(), "run");
    });
}
#[test]
fn selected_instance_flags_and_actual_receiver_determine_the_implicit_lock() {
    for (base_sync, derived_sync) in [(true, false), (false, true)] {
        let mut g = Graph::new();
        let app = g.loader();
        let throwable = g.add(g.boot, Class::plain("java/lang/Throwable"));
        let error = g.add(
            g.boot,
            Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        let mut base = Class::plain("Base");
        base.method("m", "()V", if base_sync { 0x21 } else { 1 }, &[0xb1], 0, 1);
        g.add(app, base);
        let mut derived = Class::new("Derived", Some("Base"), 0x21);
        derived.method(
            "m",
            "()V",
            if derived_sync { 0x21 } else { 1 },
            &[0xb1],
            0,
            1,
        );
        let derived = g.add(app, derived);
        let mut root = Class::plain("Root");
        let m = root.reference(10, "Base", "m", "()V");
        let mut body = vec![0x2a, 0xc2, 0x2a];
        body.extend(instruction(0xb6, m));
        body.extend([0x2a, 0xc3, 0xb1]);
        root.method("run", "(LBase;)V", 9, &body, 1, 1);
        let root = g.add(app, root);
        let mut h = g.ready();
        g.bind_core(&mut h);
        let receiver = h.allocate_instance(derived).unwrap();
        let method = g.method(root, "run", "(LBase;)V");
        let mut w = World {
            g,
            h,
            owner: root,
            throwable,
            error,
        };
        let result = w.with(Some(cap(MonitorResource::Recursion, 1)), |s, _| {
            s.run_static(
                method,
                &[Value::Reference(receiver)],
                Default::default(),
                Default::default(),
                Default::default(),
            )
        });
        if derived_sync {
            let e = result.unwrap_err();
            limit(&e, MonitorResource::Recursion, 1, 2);
            assert_eq!(e.offset, Some(3))
        } else {
            assert_eq!(result.unwrap(), ExceptionOutcome::Returned(None))
        }
    }
}
#[test]
fn inherited_static_call_uses_actual_declarer_not_symbolic_subclass() {
    let mut g = Graph::new();
    let app = g.loader();
    let throwable = g.add(g.boot, Class::plain("java/lang/Throwable"));
    let error = g.add(
        g.boot,
        Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
    );
    let mut base = Class::plain("Base");
    let target = base.reference(10, "Child", "run", "(I)V");
    let mut code = vec![0x1a, 0x99, 0, 10, 0x1a, 4, 0x64];
    code.extend(instruction(0xb8, target));
    code.extend([0xb1, 0xb1]);
    base.method("run", "(I)V", 0x29, &code, 2, 1);
    let base = g.add(app, base);
    g.add(app, Class::new("Child", Some("Base"), 0x21));
    let method = g.method(base, "run", "(I)V");
    let mut h = g.ready();
    g.bind_core(&mut h);
    let mut w = World {
        g,
        h,
        owner: base,
        throwable,
        error,
    };
    let e = w
        .with(Some(cap(MonitorResource::Recursion, 1)), |s, _| {
            s.run_static(
                method,
                &[Value::Int(1)],
                Default::default(),
                Default::default(),
                Default::default(),
            )
        })
        .unwrap_err();
    limit(&e, MonitorResource::Recursion, 1, 2);
    assert_eq!(e.offset, Some(7));
}
#[test]
fn ten_thousand_recursive_synchronized_invocations_and_sequential_claim_retirement_are_bounded() {
    let mut c = class_c();
    let target = c.reference(10, "C", "run", "(I)V");
    let mut body = vec![0x1a, 0x99, 0, 10, 0x1a, 4, 0x64];
    body.extend(instruction(0xb8, target));
    body.extend([0xb1, 0xb1]);
    c.method("run", "(I)V", 0x29, &body, 2, 1);
    let mut w = World::new(c);
    let method = w.g.method(w.owner, "run", "(I)V");
    w.with(
        Some(MonitorLimits {
            max_steps: 20000,
            max_active_monitors: 1,
            max_active_claims: 10000,
            max_recursion: 10000,
        }),
        |s, _| {
            assert_eq!(
                s.run_static(
                    method,
                    &[Value::Int(9999)],
                    ExecutionOptions {
                        instruction_budget: 100000
                    },
                    CallLimits {
                        max_frames: 10000,
                        max_frame_slots: 30000,
                        max_prepared_code_bytes: 12
                    },
                    Default::default()
                )
                .unwrap(),
                ExceptionOutcome::Returned(None)
            );
        },
    );
    let mut c = class_c();
    c.method(
        "run",
        "(ILjava/lang/Object;)V",
        9,
        &[
            0x2b, 0xc2, 0x2b, 0xc3, 0x84, 0, 0xff, 0x1a, 0x9d, 0xff, 0xf8, 0xb1,
        ],
        1,
        2,
    );
    let mut w = World::new(c);
    let r = w.object();
    assert_eq!(
        w.run(
            "run",
            "(ILjava/lang/Object;)V",
            &[Value::Int(10000), Value::Reference(r)],
            MonitorLimits {
                max_steps: 20000,
                max_active_monitors: 1,
                max_active_claims: 1,
                max_recursion: 1
            }
        )
        .unwrap(),
        ExceptionOutcome::Returned(None)
    );
}
#[test]
fn caught_supplied_throw_keeps_claim_but_escape_releases_implicit_once() {
    for synchronized in [false, true] {
        let mut c = class_c();
        let code = if synchronized {
            vec![0x2b, 0xbf, 0x4d, 0x2a, 0xc2, 0x2a, 0xc3, 0x2c, 0xb0]
        } else {
            vec![0x2a, 0xc2, 0x2b, 0xbf, 0x4d, 0x2a, 0xc3, 0x2c, 0xb0]
        };
        c.method(
            "m",
            "(Ljava/lang/Throwable;)Ljava/lang/Throwable;",
            if synchronized { 0x21 } else { 1 },
            &code,
            1,
            3,
        );
        c.handler(if synchronized {
            [0, 2, 2, 0]
        } else {
            [2, 4, 4, 0]
        });
        let m = c.reference(10, "C", "m", "(Ljava/lang/Throwable;)Ljava/lang/Throwable;");
        let mut body = vec![0x2a, 0x2b];
        body.extend(instruction(0xb6, m));
        body.push(0xb0);
        c.method(
            "run",
            "(LC;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
            9,
            &body,
            2,
            2,
        );
        let mut w = World::new(c);
        let r = w.object();
        let thrown = w.h.allocate_instance(w.throwable).unwrap();
        let result = w
            .run(
                "run",
                "(LC;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
                &[Value::Reference(r), Value::Reference(thrown)],
                MonitorLimits {
                    max_steps: if synchronized { 4 } else { 2 },
                    max_active_monitors: 1,
                    max_active_claims: if synchronized { 2 } else { 1 },
                    max_recursion: if synchronized { 2 } else { 1 },
                },
            )
            .unwrap();
        assert_eq!(
            result,
            ExceptionOutcome::Returned(Some(Value::Reference(thrown)))
        );
    }
    // The caller catches two escapes and invokes the same synchronized target again.
    // With recursion1, a leaked first receipt makes the second invocation fail.
    let mut c = class_c();
    c.method(
        "thrower",
        "(Ljava/lang/Throwable;)V",
        0x29,
        &[0x2a, 0xbf],
        1,
        1,
    );
    let target = c.reference(10, "C", "thrower", "(Ljava/lang/Throwable;)V");
    let mut body = vec![0x2a];
    body.extend(instruction(0xb8, target));
    body.extend([0xb1, 0x57, 0x2a]);
    body.extend(instruction(0xb8, target));
    body.extend([0xb1, 0x57, 0xb1]);
    c.method("run", "(Ljava/lang/Throwable;)V", 9, &body, 1, 1);
    c.handler([0, 4, 5, 0]);
    c.handler([6, 10, 11, 0]);
    let mut w = World::new(c);
    let thrown = w.h.allocate_instance(w.throwable).unwrap();
    assert_eq!(
        w.run(
            "run",
            "(Ljava/lang/Throwable;)V",
            &[Value::Reference(thrown)],
            MonitorLimits {
                max_steps: 4,
                max_active_monitors: 1,
                max_active_claims: 1,
                max_recursion: 1
            }
        )
        .unwrap(),
        ExceptionOutcome::Returned(None)
    );
}
#[test]
fn unbalanced_escaping_throw_is_replaced_before_caller_handler_or_initializer_cause() {
    let mut c = class_c();
    c.method(
        "bad",
        "(Ljava/lang/Object;Ljava/lang/Throwable;)V",
        9,
        &[0x2a, 0xc2, 0x2b, 0xbf],
        1,
        2,
    );
    let target = c.reference(10, "C", "bad", "(Ljava/lang/Object;Ljava/lang/Throwable;)V");
    let mark = c.reference(9, "C", "mark", "I");
    let mut body = vec![0x2a, 0x2b];
    body.extend(instruction(0xb8, target));
    body.push(0xb1);
    let handler = body.len() as u16;
    body.extend([0x57, 4]);
    body.extend(instruction(0xb3, mark));
    body.push(0xb1);
    c.method(
        "run",
        "(Ljava/lang/Object;Ljava/lang/Throwable;)V",
        9,
        &body,
        2,
        2,
    );
    c.handler([0, 5, handler, 0]);
    let mut w = World::new(c);
    let r = w.object();
    let thrown = w.h.allocate_instance(w.throwable).unwrap();
    let method =
        w.g.method(w.owner, "run", "(Ljava/lang/Object;Ljava/lang/Throwable;)V");
    let field = w.g.field(w.owner, "mark", "I");
    w.with(Some(Default::default()), |s, p| {
        let e = s
            .run_static(
                method,
                &[Value::Reference(r), Value::Reference(thrown)],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        imse(&e, 3);
        assert_eq!(e.method.as_ref().unwrap().name.escaped(), "bad");
        assert_eq!(
            s.context().inspect_static(&p, field).unwrap(),
            Value::Int(0)
        );
    });
}

#[test]
fn constructor_private_aliases_lock_without_promotion_and_return_protocol_stays_first() {
    for before_chain in [false, true] {
        let mut c = class_c();
        let parent = c.reference(10, "java/lang/Object", "<init>", "()V");
        let init = c.reference(10, "C", "<init>", "()V");
        let class = c.class("C");
        let mut ctor = vec![];
        if before_chain {
            ctor.extend([0x2a, 0xc2, 0x2a, 0xc3]);
        }
        ctor.push(0x2a);
        ctor.extend(instruction(0xb7, parent));
        ctor.push(0xb1);
        c.method("<init>", "()V", 1, &ctor, 1, 1);
        let mut body = instruction(0xbb, class);
        body.extend([0x59, 0xc2, 0x59, 0xc3, 0x59]);
        body.extend(instruction(0xb7, init));
        body.push(0xb0);
        c.method("run", "()LC;", 9, &body, 2, 0);
        let mut w = World::new(c);
        let r = w
            .run(
                "run",
                "()LC;",
                &[],
                cap(MonitorResource::Steps, if before_chain { 4 } else { 2 }),
            )
            .unwrap();
        let ExceptionOutcome::Returned(Some(Value::Reference(r))) = r else {
            panic!("reference result required")
        };
        assert_eq!(w.h.instance_class(r).unwrap(), w.owner);
    }
    let mut c = class_c();
    let init = c.reference(10, "C", "<init>", "()V");
    let class = c.class("C");
    c.method("<init>", "()V", 1, &[0x2a, 0xc2, 0xb1], 1, 1);
    let mut body = instruction(0xbb, class);
    body.push(0x59);
    body.extend(instruction(0xb7, init));
    body.extend([0x57, 0xb1]);
    c.method("run", "()V", 9, &body, 2, 0);
    let mut w = World::new(c);
    let e = w.run("run", "()V", &[], Default::default()).unwrap_err();
    assert!(matches!(
        e.kind,
        ExecutionErrorKind::ConstructionProtocol { .. }
    ));
    assert_eq!(e.offset, Some(2));
}
#[test]
fn initializer_flags_do_not_acquire_but_initializer_explicit_balance_precedes_completion() {
    let mut c = class_c();
    c.method("<clinit>", "()V", 0x28, &[0xb1], 0, 0);
    c.method("run", "()V", 9, &[0xb1], 0, 0);
    let mut w = World::new(c);
    assert_eq!(
        w.run("run", "()V", &[], cap(MonitorResource::Steps, 0))
            .unwrap(),
        ExceptionOutcome::Returned(None)
    );
    let mut c = class_c();
    let class = c.class("C");
    let init = c.reference(10, "C", "<init>", "()V");
    let parent = c.reference(10, "java/lang/Object", "<init>", "()V");
    let mut ctor = vec![0x2a];
    ctor.extend(instruction(0xb7, parent));
    ctor.push(0xb1);
    c.method("<init>", "()V", 1, &ctor, 1, 1);
    let mut body = instruction(0xbb, class);
    body.push(0x59);
    body.extend(instruction(0xb7, init));
    body.extend([0xc2, 0xb1]);
    c.method("<clinit>", "()V", 0x28, &body, 2, 0);
    c.method("run", "()V", 9, &[0xb1], 0, 0);
    let mut w = World::new(c);
    let owner = w.owner;
    let m = w.g.method(owner, "run", "()V");
    w.with(Some(Default::default()), |s, _| {
        let e = s
            .run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        imse(&e, 8);
        assert_eq!(e.initialization_class, Some(owner));
        assert_eq!(s.status(owner).unwrap(), InitializationStatus::Aborted);
        assert_eq!(s.retained_roots().count(), 0);
    });
}
#[test]
fn host_vm_dispatch_and_fuel_interruptions_drop_claims_without_erasing_prior_writes() {
    for failure in 0..4 {
        let mut c = class_c();
        let mark = c.reference(9, "C", "mark", "I");
        let class = c.class("C");
        let mut code = vec![0x2a, 0xc2, 0x10, 7];
        code.extend(instruction(0xb3, mark));
        match failure {
            0 => {
                code.extend([1, 0xbf]);
            }
            1 => {
                code.extend([0x12, u8::try_from(class).unwrap(), 0x57, 0xb1]);
            }
            2 => {
                code.extend([0x2b, 0xbf]);
            }
            _ => {
                code.extend([0x2a, 0xc3, 0xb1]);
            }
        }
        c.method(
            "bad",
            "(Ljava/lang/Object;Ljava/lang/Throwable;)V",
            9,
            &code,
            1,
            2,
        );
        c.method(
            "good",
            "(Ljava/lang/Object;)V",
            9,
            &[0x2a, 0xc2, 0x2a, 0xc3, 0xb1],
            1,
            1,
        );
        let mut w = World::new(c);
        let lock = w.object();
        let thrown = w.h.allocate_instance(w.throwable).unwrap();
        let bad =
            w.g.method(w.owner, "bad", "(Ljava/lang/Object;Ljava/lang/Throwable;)V");
        let good = w.g.method(w.owner, "good", "(Ljava/lang/Object;)V");
        let field = w.g.field(w.owner, "mark", "I");
        w.with(
            Some(MonitorLimits {
                max_recursion: 1,
                ..Default::default()
            }),
            |s, p| {
                let e = s
                    .run_static(
                        bad,
                        &[Value::Reference(lock), Value::Reference(thrown)],
                        ExecutionOptions {
                            instruction_budget: if failure == 3 { 4 } else { 100 },
                        },
                        Default::default(),
                        ExceptionLimits {
                            max_dispatch_steps: if failure == 2 { 0 } else { 100 },
                            ..Default::default()
                        },
                    )
                    .unwrap_err();
                match failure {
                    0 => assert_eq!(
                        e.kind,
                        ExecutionErrorKind::UnsupportedVmException(
                            VmExceptionKind::NullPointerException
                        )
                    ),
                    1 => assert!(matches!(
                        e.kind,
                        ExecutionErrorKind::UnsupportedConstant { .. }
                    )),
                    2 => assert!(matches!(
                        e.kind,
                        ExecutionErrorKind::ExceptionDispatchLimit {
                            limit: 0,
                            required: 1
                        }
                    )),
                    _ => assert!(matches!(
                        e.kind,
                        ExecutionErrorKind::InstructionBudgetExhausted { limit: 4 }
                    )),
                }
                assert_eq!(
                    s.context().inspect_static(&p, field).unwrap(),
                    Value::Int(7)
                );
                assert_eq!(
                    s.run_static(
                        good,
                        &[Value::Reference(lock)],
                        Default::default(),
                        Default::default(),
                        Default::default()
                    )
                    .unwrap(),
                    ExceptionOutcome::Returned(None)
                );
            },
        );
    }
}
#[test]
fn legacy_modes_reject_both_monitor_opcodes_and_synchronized_code_even_with_the_same_heap() {
    for body in [vec![1, 0xc2, 0xb1], vec![1, 0xc3, 0xb1]] {
        let mut c = class_c();
        c.method("run", "()V", 9, &body, 1, 0);
        let mut w = World::new(c);
        let m = w.g.method(w.owner, "run", "()V");
        w.with(None, |s, _| {
            let e = s
                .run_static(
                    m,
                    &[],
                    Default::default(),
                    Default::default(),
                    Default::default(),
                )
                .unwrap_err();
            assert_eq!(e.kind, ExecutionErrorKind::UnsupportedInstruction(body[1]));
            assert_eq!(e.offset, Some(1));
        });
    }
    let mut c = class_c();
    c.method("run", "()V", 0x29, &[0xb1], 0, 0);
    let mut w = World::new(c);
    let m = w.g.method(w.owner, "run", "()V");
    w.with(None, |s, _| {
        assert!(matches!(
            s.run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default()
            )
            .unwrap_err()
            .kind,
            ExecutionErrorKind::UnsupportedMethod(_)
        ))
    });
}
#[test]
fn unsupported_native_abstract_decode_and_frame_preflight_never_pay_implicit_steps() {
    for (flags, body, expected) in [(0x129, vec![], "native"), (0x29, vec![0x10], "decode")] {
        let mut c = class_c();
        if flags & 0x400 != 0 {
            c.flags = 0x421;
        }
        c.method("run", "()V", flags, &body, 0, 0);
        let mut w = World::new(c);
        let e = w
            .run("run", "()V", &[], cap(MonitorResource::Steps, 0))
            .unwrap_err();
        match expected {
            "decode" => assert!(matches!(e.kind, ExecutionErrorKind::Decode(_))),
            "native" => assert!(
                matches!(e.kind,ExecutionErrorKind::UnsupportedMethod(s) if s.contains("native"))
            ),
            _ => assert!(
                matches!(e.kind,ExecutionErrorKind::UnsupportedMethod(s) if s.contains("abstract"))
            ),
        }
    }
    let mut c = class_c();
    c.method("run", "()V", 0x29, &[0xb1], 0, 0);
    let mut w = World::new(c);
    let m = w.g.method(w.owner, "run", "()V");
    let owner = w.owner;
    w.with(Some(cap(MonitorResource::Steps, 0)), |s, _| {
        let e = s
            .run_static(
                m,
                &[],
                Default::default(),
                CallLimits {
                    max_frames: 0,
                    ..Default::default()
                },
                Default::default(),
            )
            .unwrap_err();
        assert!(matches!(
            e.kind,
            ExecutionErrorKind::FrameLimit {
                limit: 0,
                required: 1
            }
        ));
        assert_eq!(
            s.status(owner).unwrap(),
            InitializationStatus::Uninitialized
        );
    });
}
#[test]
fn all_max_policies_allocate_only_actual_small_claims_and_zero_capacity_precedence_is_ordered() {
    let mut w = World::new(pair());
    let a = w.object();
    let b = w.object();
    assert_eq!(
        w.run(
            "run",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
            &[Value::Reference(a), Value::Reference(b)],
            MonitorLimits {
                max_steps: u64::MAX,
                max_active_monitors: u64::MAX,
                max_active_claims: u64::MAX,
                max_recursion: u64::MAX
            }
        )
        .unwrap(),
        ExceptionOutcome::Returned(Some(Value::Int(1)))
    );
    for which in 0..3 {
        let mut c = class_c();
        c.method("run", "()V", 0x29, &[0xb1], 0, 0);
        let mut w = World::new(c);
        let e = w
            .run(
                "run",
                "()V",
                &[],
                MonitorLimits {
                    max_steps: 1,
                    max_active_monitors: if which == 0 { 0 } else { 1 },
                    max_active_claims: if which <= 1 { 0 } else { 1 },
                    max_recursion: 0,
                },
            )
            .unwrap_err();
        limit(
            &e,
            [
                MonitorResource::ActiveMonitors,
                MonitorResource::ActiveClaims,
                MonitorResource::Recursion,
            ][which],
            0,
            1,
        );
    }
}

#[test]
fn parser_rejects_synchronized_constructor_or_abstract_method_before_any_monitor_authority() {
    for (name, flags) in [("<init>", 0x21), ("abstract", 0x421)] {
        let mut c = class_c();
        c.flags = 0x421;
        c.method(name, "()V", flags, &[0xb1], 0, 1);
        assert!(crabspot_classfile::parse(&c.bytes()).is_err());
    }
}

#[test]
fn cache_hits_enter_again_without_double_charging_prepared_code_and_deferred_calls_keep_one_fuel() {
    let mut c = class_c();
    c.method("m", "()V", 0x29, &[0xb1], 0, 0);
    let m = c.reference(10, "C", "m", "()V");
    let mut code = instruction(0xb8, m);
    code.extend(instruction(0xb8, m));
    code.push(0xb1);
    c.method("run", "()V", 9, &code, 0, 0);
    let mut w = World::new(c);
    let method = w.g.method(w.owner, "run", "()V");
    w.with(
        Some(MonitorLimits {
            max_steps: 4,
            max_active_monitors: 1,
            max_active_claims: 1,
            max_recursion: 1,
        }),
        |s, _| {
            assert_eq!(
                s.run_static(
                    method,
                    &[],
                    ExecutionOptions {
                        instruction_budget: 5
                    },
                    CallLimits {
                        max_frames: 2,
                        max_frame_slots: 0,
                        max_prepared_code_bytes: 8
                    },
                    Default::default()
                )
                .unwrap(),
                ExceptionOutcome::Returned(None)
            )
        },
    );
}
#[test]
fn static_initialization_failure_happens_before_any_implicit_acquisition() {
    let mut c = class_c();
    let mark = c.reference(9, "C", "mark", "I");
    let mut code = vec![0x10, 7];
    code.extend(instruction(0xb3, mark));
    code.extend([1, 0xbf]);
    c.method("<clinit>", "()V", 8, &code, 1, 0);
    c.method("run", "()V", 0x29, &[0xb1], 0, 0);
    let mut w = World::new(c);
    let owner = w.owner;
    let m = w.g.method(owner, "run", "()V");
    let field = w.g.field(owner, "mark", "I");
    w.with(Some(cap(MonitorResource::Steps, 0)), |s, p| {
        let e = s
            .run_static(
                m,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException)
        );
        assert_eq!(e.initialization_class, Some(owner));
        assert_eq!(s.status(owner).unwrap(), InitializationStatus::Aborted);
        assert_eq!(
            s.context().inspect_static(&p, field).unwrap(),
            Value::Int(7)
        );
    });
}
#[test]
fn same_spelling_static_declarers_from_distinct_defining_loaders_have_distinct_keys() {
    let mut g = Graph::new();
    let a = g.loader();
    let b = g.loader();
    let throwable = g.add(g.boot, Class::plain("java/lang/Throwable"));
    let error = g.add(
        g.boot,
        Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
    );
    let mut second = Class::plain("q/Base");
    second.method("m", "()V", 0x29, &[0xb1], 0, 0);
    g.add(b, second);
    let mut bridge = Class::plain("q/Bridge");
    let m = bridge.reference(10, "q/Base", "m", "()V");
    let mut code = instruction(0xb8, m);
    code.push(0xb1);
    bridge.method("call", "()V", 9, &code, 0, 0);
    let bridge = g.add(b, bridge);
    g.bind(a, "q/Bridge", bridge);
    let mut first = Class::plain("q/Base");
    let target = first.reference(10, "q/Bridge", "call", "()V");
    let mut code = instruction(0xb8, target);
    code.push(0xb1);
    first.method("run", "()V", 0x29, &code, 0, 0);
    let first = g.add(a, first);
    let m = g.method(first, "run", "()V");
    let mut h = g.ready();
    g.bind_core(&mut h);
    let mut w = World {
        g,
        h,
        owner: first,
        throwable,
        error,
    };
    w.with(
        Some(MonitorLimits {
            max_steps: 4,
            max_active_monitors: 2,
            max_active_claims: 2,
            max_recursion: 1,
        }),
        |s, _| {
            assert_eq!(
                s.run_static(
                    m,
                    &[],
                    Default::default(),
                    Default::default(),
                    Default::default()
                )
                .unwrap(),
                ExceptionOutcome::Returned(None)
            )
        },
    );
}
#[test]
fn foreign_monitor_operand_is_host_error_before_zero_monitor_budget() {
    let mut c = class_c();
    c.field("foreign", "Ljava/lang/Object;", 9);
    let field = c.reference(9, "C", "foreign", "Ljava/lang/Object;");
    let mut body = instruction(0xb2, field);
    body.extend([0xc2, 0xb1]);
    c.method("run", "()V", 9, &body, 1, 0);
    // Ordinary checked stores reject foreign handles. Low-level private boundary
    // validation is tested in execute.rs, while public argument validation must
    // reject the foreign reference even before activation/initialization.
    c.method(
        "argument",
        "(Ljava/lang/Object;)V",
        9,
        &[0x2a, 0xc2, 0x2a, 0xc3, 0xb1],
        1,
        1,
    );
    let mut w = World::new(c);
    let mut other = ManagedHeap::new(Default::default()).unwrap();
    let foreign = other
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let e = w
        .run(
            "argument",
            "(Ljava/lang/Object;)V",
            &[Value::Reference(foreign)],
            cap(MonitorResource::Steps, 0),
        )
        .unwrap_err();
    assert!(!matches!(
        e.kind,
        ExecutionErrorKind::Monitor(_) | ExecutionErrorKind::UnsupportedVmException(_)
    ));
    assert!(e.method.is_some());
}
#[test]
fn real_resolver_panic_discards_monitor_state_and_keeps_prior_effects_in_same_session() {
    struct Panics;
    impl HierarchyResolver for Panics {
        fn resolve(
            &mut self,
            r: &DefinitionRegistry,
            l: LoaderId,
            n: &[u16],
        ) -> Result<Option<ClassId>, ResolveError> {
            if n == units("Missing") {
                panic!("intentional monitored resolver panic")
            };
            r.lookup_class(l, n)
                .map_err(|e| ResolveError::new(e.to_string()))
        }
    }
    let mut c = class_c();
    let field = c.reference(9, "C", "mark", "I");
    let missing = c.reference(9, "Missing", "value", "I");
    let mut code = vec![0x2a, 0xc2, 0x10, 7];
    code.extend(instruction(0xb3, field));
    code.extend(instruction(0xb2, missing));
    code.extend([0x57, 0xb1]);
    c.method("bad", "(Ljava/lang/Object;)V", 9, &code, 1, 1);
    c.method(
        "good",
        "(Ljava/lang/Object;)V",
        9,
        &[0x2a, 0xc2, 0x2a, 0xc3, 0xb1],
        1,
        1,
    );
    let mut w = World::new(c);
    let lock = w.object();
    let bad = w.g.method(w.owner, "bad", "(Ljava/lang/Object;)V");
    let good = w.g.method(w.owner, "good", "(Ljava/lang/Object;)V");
    let field = w.g.field(w.owner, "mark", "I");
    let mut resolver = Panics;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        w.g.domain.registry(),
        &mut w.h,
        &mut resolver,
        Default::default(),
    )
    .unwrap();
    let statics = context.allocate_static_state(Default::default()).unwrap();
    let exceptions = context.prepare_exception_profile(w.throwable).unwrap();
    let mut session = InitializationSession::new_with_monitors(
        &mut context,
        &statics,
        &exceptions,
        w.error,
        Default::default(),
        MonitorLimits {
            max_recursion: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        session.run_static(
            bad,
            &[Value::Reference(lock)],
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }));
    assert!(result.is_err());
    assert_eq!(
        session.context().inspect_static(&statics, field).unwrap(),
        Value::Int(7)
    );
    assert_eq!(
        session
            .run_static(
                good,
                &[Value::Reference(lock)],
                Default::default(),
                Default::default(),
                Default::default()
            )
            .unwrap(),
        ExceptionOutcome::Returned(None)
    );
}

#[test]
fn synchronized_super_and_interface_calls_use_the_selected_receiver_and_real_caller_pc() {
    for interface in [false, true] {
        for short in [false, true] {
            let mut g = Graph::new();
            let app = g.loader();
            let throwable = g.add(g.boot, Class::plain("java/lang/Throwable"));
            let error = g.add(
                g.boot,
                Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
            );
            let mut base = Class::new(
                "Base",
                Some("java/lang/Object"),
                if interface { 0x601 } else { 0x21 },
            );
            base.method(
                "m",
                "()V",
                if interface { 0x401 } else { 0x21 },
                &[0xb1],
                0,
                1,
            );
            g.add(app, base);
            let mut child = Class::new(
                "Child",
                Some(if interface {
                    "java/lang/Object"
                } else {
                    "Base"
                }),
                0x21,
            );
            if interface {
                child.interface("Base");
                child.method("m", "()V", 0x21, &[0xb1], 0, 1);
            } else {
                let m = child.reference(10, "Base", "m", "()V");
                let mut code = vec![0x2a];
                code.extend(instruction(0xb7, m));
                code.push(0xb1);
                child.method("call", "()V", 1, &code, 1, 1);
            }
            let target = child.reference(
                if interface { 11 } else { 10 },
                if interface { "Base" } else { "Child" },
                if interface { "m" } else { "call" },
                "()V",
            );
            let mut code = vec![0x2a, 0xc2, 0x2a];
            code.extend(instruction(if interface { 0xb9 } else { 0xb6 }, target));
            if interface {
                code.extend([1, 0]);
            }
            code.extend([0x2a, 0xc3, 0xb1]);
            child.method("run", "(LChild;)V", 9, &code, 1, 1);
            let child = g.add(app, child);
            let method = g.method(child, "run", "(LChild;)V");
            let mut h = g.ready();
            g.bind_core(&mut h);
            let receiver = h.allocate_instance(child).unwrap();
            let mut w = World {
                g,
                h,
                owner: child,
                throwable,
                error,
            };
            let result = w.with(
                Some(MonitorLimits {
                    max_steps: 4,
                    max_active_monitors: 1,
                    max_active_claims: 2,
                    max_recursion: if short { 1 } else { 2 },
                }),
                |s, _| {
                    s.run_static(
                        method,
                        &[Value::Reference(receiver)],
                        Default::default(),
                        Default::default(),
                        Default::default(),
                    )
                },
            );
            if short {
                let e = result.unwrap_err();
                limit(&e, MonitorResource::Recursion, 1, 2);
                assert_eq!(e.offset, Some(if interface { 3 } else { 1 }));
                assert_eq!(e.method.as_ref().unwrap().owner, Some(child));
                assert_eq!(
                    e.method.as_ref().unwrap().name.escaped(),
                    if interface { "run" } else { "call" }
                );
                assert_eq!(
                    e.call_site.as_ref().unwrap().offset,
                    if interface { 3 } else { 1 }
                );
            } else {
                assert_eq!(result.unwrap(), ExceptionOutcome::Returned(None));
            }
        }
    }
}
#[test]
fn protected_receiver_validation_precedes_zero_step_implicit_acquisition() {
    for legal in [false, true] {
        let mut g = Graph::new();
        let app = g.loader();
        let throwable = g.add(g.boot, Class::plain("java/lang/Throwable"));
        let error = g.add(
            g.boot,
            Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        let mut base = Class::plain("p/Base");
        base.method("m", "()V", 0x24, &[0xb1], 0, 1);
        let base = g.add(app, base);
        let mut child = Class::new("q/Child", Some("p/Base"), 0x21);
        let target = child.reference(10, "p/Base", "m", "()V");
        let mut code = vec![0x2a];
        code.extend(instruction(0xb6, target));
        code.push(0xb1);
        child.method("run", "(Lp/Base;)V", 9, &code, 1, 1);
        let child = g.add(app, child);
        let method = g.method(child, "run", "(Lp/Base;)V");
        let mut h = g.ready();
        g.bind_core(&mut h);
        let receiver = h
            .allocate_instance(if legal { child } else { base })
            .unwrap();
        let mut w = World {
            g,
            h,
            owner: child,
            throwable,
            error,
        };
        let e = w
            .with(Some(cap(MonitorResource::Steps, 0)), |s, _| {
                s.run_static(
                    method,
                    &[Value::Reference(receiver)],
                    Default::default(),
                    Default::default(),
                    Default::default(),
                )
            })
            .unwrap_err();
        if legal {
            limit(&e, MonitorResource::Steps, 0, 1);
        } else {
            assert_eq!(
                e.kind,
                ExecutionErrorKind::MethodAccess(Box::new(MethodAccessError::Context(
                    FieldAccessError::InvalidReceiver {
                        expected: child,
                        actual: base
                    }
                )))
            );
        }
        assert_eq!(e.offset, Some(1));
    }
}
#[test]
fn constructor_escaping_imbalance_preserves_escaped_heap_state_but_never_runs_caller_catch() {
    let mut c = class_c();
    c.field("leak", "LC;", 9);
    c.field("value", "I", 1);
    let leak = c.reference(9, "C", "leak", "LC;");
    let value = c.reference(9, "C", "value", "I");
    let mark = c.reference(9, "C", "mark", "I");
    let parent = c.reference(10, "java/lang/Object", "<init>", "()V");
    let init = c.reference(10, "C", "<init>", "(Ljava/lang/Throwable;)V");
    let class = c.class("C");
    let mut ctor = vec![0x2a];
    ctor.extend(instruction(0xb7, parent));
    ctor.extend([0x2a, 0x10, 7]);
    ctor.extend(instruction(0xb5, value));
    ctor.push(0x2a);
    ctor.extend(instruction(0xb3, leak));
    ctor.extend([0x2a, 0xc2, 0x2b, 0xbf]);
    let throw_pc = (ctor.len() - 1) as u32;
    c.method("<init>", "(Ljava/lang/Throwable;)V", 1, &ctor, 2, 2);
    let mut body = instruction(0xbb, class);
    body.extend([0x59, 0x2a]);
    body.extend(instruction(0xb7, init));
    body.extend([0x57, 0xb1]);
    let handler = body.len() as u16;
    body.extend([0x57, 4]);
    body.extend(instruction(0xb3, mark));
    body.push(0xb1);
    c.method("run", "(Ljava/lang/Throwable;)V", 9, &body, 3, 1);
    c.handler([0, 8, handler, 0]);
    let mut w = World::new(c);
    let thrown = w.h.allocate_instance(w.error).unwrap();
    let m = w.g.method(w.owner, "run", "(Ljava/lang/Throwable;)V");
    let leaked = w.g.field(w.owner, "leak", "LC;");
    let value = w.g.field(w.owner, "value", "I");
    let mark = w.g.field(w.owner, "mark", "I");
    w.with(Some(Default::default()), |s, p| {
        let e = s
            .run_static(
                m,
                &[Value::Reference(thrown)],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        imse(&e, throw_pc);
        assert_eq!(e.method.as_ref().unwrap().name.escaped(), "<init>");
        assert_eq!(e.call_site.as_ref().unwrap().offset, 5);
        assert_eq!(s.context().inspect_static(&p, mark).unwrap(), Value::Int(0));
        let Value::Reference(r) = s.context().inspect_static(&p, leaked).unwrap() else {
            panic!("escaped reference missing")
        };
        assert!(!r.is_null());
        assert_eq!(
            s.context().heap().get_field(r, value).unwrap(),
            Value::Int(7)
        );
    });
}
#[test]
fn actual_initializer_escaping_unbalanced_error_is_aborted_without_retained_cause() {
    let mut c = class_c();
    c.field("supplied", "Ljava/lang/Throwable;", 9);
    let supplied = c.reference(9, "C", "supplied", "Ljava/lang/Throwable;");
    let mark = c.reference(9, "C", "mark", "I");
    let object = c.class("java/lang/Object");
    let init = c.reference(10, "java/lang/Object", "<init>", "()V");
    let mut code = vec![0x10, 7];
    code.extend(instruction(0xb3, mark));
    code.extend(instruction(0xbb, object));
    code.push(0x59);
    code.extend(instruction(0xb7, init));
    code.push(0xc2);
    code.extend(instruction(0xb2, supplied));
    code.push(0xbf);
    let throw_pc = (code.len() - 1) as u32;
    c.method("<clinit>", "()V", 8, &code, 2, 0);
    let mut w = World::new(c);
    let owner = w.owner;
    let thrown = w.h.allocate_instance(w.error).unwrap();
    let field = w.g.field(owner, "mark", "I");
    let mut context =
        w.g.domain
            .member_context_unnamed_modules(&mut w.h, Default::default(), Default::default())
            .unwrap();
    let statics = context.allocate_static_state(Default::default()).unwrap();
    context
        .write_static(&statics, owner, supplied, Value::Reference(thrown))
        .unwrap();
    let exceptions = context.prepare_exception_profile(w.throwable).unwrap();
    let mut session = InitializationSession::new_with_monitors(
        &mut context,
        &statics,
        &exceptions,
        w.error,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let e = session
        .initialize(
            owner,
            Default::default(),
            Default::default(),
            Default::default(),
        )
        .unwrap_err();
    imse(&e, throw_pc);
    assert_eq!(e.initialization_class, Some(owner));
    assert_eq!(e.method.as_ref().unwrap().name.escaped(), "<clinit>");
    assert_eq!(
        session.status(owner).unwrap(),
        InitializationStatus::Aborted
    );
    assert_eq!(session.retained_roots().count(), 0);
    assert_eq!(
        session.context().inspect_static(&statics, field).unwrap(),
        Value::Int(7)
    );
}

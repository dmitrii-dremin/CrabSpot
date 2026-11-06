//! ABI and private publication tests, sharing only the prior synthetic compact
//! String emitter; no Java execution oracle or historical stack claims.
use super::*;
use crate::mirror_support::{Class, Graph};
use crate::string_bodies::equality::tests::image;
use crate::throwable::{ThrowableErrorKind as E, ThrowableResource as R};
const T: &str = "java/lang/Throwable";
fn throwable(flags: u16, fields: bool, ctors: bool) -> Class {
    throwable_extra(flags, fields, ctors, 0)
}
fn throwable_extra(flags: u16, fields: bool, ctors: bool, extra: u8) -> Class {
    let mut t = Class::new(T, Some("java/lang/Object"), 0x21);
    if extra == 1 {
        t.method("z", "()V", 1, &[0xb1], 0, 1);
    }
    if extra == 2 {
        t.field("z", "I", 1);
    }
    // Same names with other descriptors are unrelated ABI members.
    t.field("backtrace", "J", 2);
    if fields {
        t.field("backtrace", "Ljava/lang/Object;", 0x82);
        t.field("depth", "I", 0x82);
    }
    t.field("user", "I", 1);
    t.field("alternate", "Ljava/lang/Object;", 1);
    if flags != 0 {
        t.method(
            "fillInStackTrace",
            "(I)Ljava/lang/Throwable;",
            flags,
            &[],
            0,
            2,
        );
    }
    if ctors {
        for (d, a) in [
            ("()V", 1),
            ("(Ljava/lang/String;)V", 1),
            ("(Ljava/lang/String;Ljava/lang/Throwable;)V", 1),
            ("(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V", 4),
        ] {
            t.method("<init>", d, a, &[0xb1], 0, 5);
        }
    }
    t.method(
        "fillInStackTrace",
        "()Ljava/lang/Throwable;",
        0x21,
        &[1, 0xb0],
        1,
        1,
    );
    t.method("fillInStackTrace", "(J)V", 9, &[0xb1], 0, 2);
    t
}
fn graph(damage: u8) -> (Graph, ClassId, CompactStringBodyPlan, ThrowablePlan) {
    let (mut g, _, body) = image(false, 0);
    let b = g.boot;
    let t = g.add(b, throwable_extra(0x102, true, true, damage));
    if damage == 3 {
        let foreign = g.loader();
        g.add(foreign, Class::plain("foreign/X"));
    }
    for (n, parent) in [
        ("java/lang/Exception", T),
        ("java/lang/RuntimeException", "java/lang/Exception"),
        (
            "java/lang/ArithmeticException",
            "java/lang/RuntimeException",
        ),
    ] {
        let mut c = Class::new(n, Some(parent), 0x21);
        c.method("<init>", "(Ljava/lang/String;)V", 1, &[0xb1], 0, 2);
        if n.ends_with("ArithmeticException") {
            c.method("<init>", "()V", 1, &[0xb1], 0, 1);
        }
        g.add(b, c);
    }
    for (n, parent) in [
        ("java/lang/Error", T),
        (
            "java/lang/NullPointerException",
            "java/lang/RuntimeException",
        ),
        ("java/lang/AssertionError", "java/lang/Error"),
        ("java/lang/StackTraceElement", "java/lang/Object"),
        ("java/util/Collections", "java/lang/Object"),
        ("java/util/Collections$EmptySet", "java/lang/Object"),
        ("java/util/Collections$EmptyList", "java/lang/Object"),
        ("java/util/Collections$EmptyMap", "java/lang/Object"),
    ] {
        g.add(b, Class::new(n, Some(parent), 0x21));
    }
    let p = probe_throwable_support(g.domain.registry(), b, t, Default::default())
        .unwrap()
        .plan
        .unwrap();
    (g, t, body, p)
}
struct F {
    g: Graph,
    h: ManagedHeap,
    p: ThrowablePlan,
    t: ClassId,
    error: ClassId,
}
fn fixture(limits: TraceLimits) -> F {
    let (mut g, t, body, p) = graph(0);
    let mut h = g.ready();
    g.bind_core(&mut h);
    g.domain.enable_class_mirror_sites(1000).unwrap();
    let e = probe_compact_string_equality(g.domain.registry(), &body, Default::default())
        .unwrap()
        .plan
        .unwrap();
    let class = g
        .domain
        .registry()
        .lookup_class(
            g.boot,
            &"java/lang/Class".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    h.bind_class_mirrors(g.domain.registry(), class, 1000)
        .unwrap();
    let string = g
        .domain
        .registry()
        .lookup_class(
            g.boot,
            &"java/lang/String".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    h.bind_strings(g.domain.registry(), string, Default::default())
        .unwrap();
    h.bind_compact_string_bodies(g.domain.registry(), &body, Default::default())
        .unwrap();
    h.bind_compact_string_equality(g.domain.registry(), &e, BootstrapAssertionPolicy::Disabled)
        .unwrap();
    h.bind_throwable_support(g.domain.registry(), &p, limits)
        .unwrap();
    let error = g
        .domain
        .registry()
        .lookup_class(
            g.boot,
            &"java/lang/Error".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    F { g, h, p, t, error }
}
fn context(f: &mut F) -> FieldResolutionContext<'_> {
    f.g.domain
        .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
        .unwrap()
}
fn claim(
    c: &mut FieldResolutionContext<'_>,
    t: ClassId,
    error: ClassId,
) -> (InitializationClaim, ExceptionProfile) {
    let s = c.allocate_static_state(Default::default()).unwrap();
    let e = c.prepare_exception_profile(t).unwrap();
    (c.claim_initialization(&s, &e, error).unwrap(), e)
}
fn snapshot(
    c: &mut FieldResolutionContext<'_>,
    claim: &InitializationClaim,
    method: MethodId,
    n: usize,
) -> TraceSnapshot {
    let mut b = c.begin_trace_snapshot(claim, n).unwrap();
    for pc in 0..n {
        c.append_trace_frame(
            claim,
            &mut b,
            TraceFrame {
                method,
                pc: pc as u32,
            },
        )
        .unwrap();
    }
    c.finish_trace_snapshot(claim, b).unwrap()
}
fn capture(
    c: &mut FieldResolutionContext<'_>,
    claim: &InitializationClaim,
    p: &ThrowablePlan,
    r: Reference,
    n: usize,
) {
    let a = c.prepare_throwable_capture(claim, p.native, r).unwrap();
    let s = snapshot(c, claim, p.ctors[0], n);
    assert_eq!(c.commit_throwable_capture(claim, &a, s).unwrap(), r);
}
#[test]
fn discovery_empty_native_absence_exact22_and_funded_failure_prefix() {
    let mut g = Graph::new();
    let t = g.add(g.boot, Class::new(T, Some("java/lang/Object"), 0x21));
    for (limit, ok, paid) in [
        (0, false, 0),
        (21, false, 3),
        (22, true, 22),
        (u64::MAX, true, 22),
    ] {
        let r = probe_throwable_support(
            g.domain.registry(),
            g.boot,
            t,
            ThrowableDiscoveryLimits {
                max_work_units: limit,
            },
        );
        if ok {
            let r = r.unwrap();
            assert!(r.plan.is_none());
            assert_eq!(r.usage.work_units, paid)
        } else {
            let e = r.unwrap_err();
            assert_eq!(e.discovery_usage.work_units, paid);
            assert!(matches!(
                e.kind,
                E::Limit {
                    resource: R::DiscoveryWork,
                    ..
                }
            ))
        }
    }
}
#[test]
fn discovery_abi_only_roles_and_exact_total_quota() {
    let (g, t, _, p) = graph(0);
    let d = probe_throwable_support(g.domain.registry(), g.boot, t, Default::default()).unwrap();
    assert_eq!(d.plan.as_ref(), Some(&p));
    assert_eq!(p.private_roots().len(), 4);
    assert_eq!(p.public_roles().arithmetic_exception, p.ctors[6].owner());
    let count = d.usage.work_units;
    assert_eq!(
        probe_throwable_support(
            g.domain.registry(),
            g.boot,
            t,
            ThrowableDiscoveryLimits {
                max_work_units: count
            }
        )
        .unwrap(),
        d
    );
    let e = probe_throwable_support(
        g.domain.registry(),
        g.boot,
        t,
        ThrowableDiscoveryLimits {
            max_work_units: count - 1,
        },
    )
    .unwrap_err();
    assert!(matches!(
        e.kind,
        E::Limit {
            resource: R::DiscoveryWork,
            ..
        }
    ));
}
#[test]
fn discovery_wrong_native_and_missing_fields_or_constructors_reject_before_roles() {
    for (flags, fields, ctors) in [
        (0x101, true, true),
        (0x102, false, true),
        (0x102, true, false),
    ] {
        let mut g = Graph::new();
        let t = g.add(g.boot, throwable(flags, fields, ctors));
        assert!(matches!(
            probe_throwable_support(g.domain.registry(), g.boot, t, Default::default())
                .unwrap_err()
                .kind,
            E::InvalidBinding(_)
        ));
    }
}
#[test]
fn binding_is_pristine_idempotent_and_conflicting_policy_is_atomic() {
    let mut f = fixture(Default::default());
    f.h.bind_throwable_support(f.g.domain.registry(), &f.p, Default::default())
        .unwrap();
    let r = f.h.allocate_instance(f.t).unwrap();
    assert!(f.h.throwable_trace_usage() == TraceUsage::default());
    f.h.bind_throwable_support(f.g.domain.registry(), &f.p, Default::default())
        .unwrap();
    assert_eq!(f.h.instance_class(r).unwrap(), f.t);
    assert!(matches!(
        f.h.bind_throwable_support(
            f.g.domain.registry(),
            &f.p,
            TraceLimits {
                max_snapshots: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        E::PolicyConflict
    ));
}
#[test]
fn captured_empty_is_distinct_and_tokens_are_truthful_object_children() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    assert_eq!(c.throwable_trace(r).unwrap(), None);
    capture(&mut c, &k, &p, r, 0);
    assert_eq!(c.throwable_trace(r).unwrap(), Some([].as_slice()));
    let token = match c.heap().get_field(r, p.fields[0]).unwrap() {
        Value::Reference(r) => r,
        _ => panic!(),
    };
    assert_eq!(
        c.reference_type(token).unwrap(),
        Some(ReferenceType::Class(p.object))
    );
    assert_eq!(
        c.heap().children(token).unwrap().collect::<Vec<_>>(),
        [r.heap_ref().unwrap()]
    );
    assert!(
        c.heap()
            .children(r)
            .unwrap()
            .any(|h| Some(h) == token.heap_ref())
    );
    assert!(c.heap().reject_opaque(token).is_err());
    assert!(
        c.heap_mut()
            .put_field(token, p.fields[1], Value::Int(3))
            .is_err()
    );
    assert_eq!(c.heap().throwable_trace_usage().backtraces, 1);
}
#[test]
fn refill_preserves_old_token_and_accepted_snapshot_counters() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    capture(&mut c, &k, &p, r, 2);
    let old = c.heap().get_field(r, p.fields[0]).unwrap();
    capture(&mut c, &k, &p, r, 1);
    assert_ne!(old, c.heap().get_field(r, p.fields[0]).unwrap());
    let Value::Reference(old) = old else { panic!() };
    assert_eq!(
        c.heap().children(old).unwrap().collect::<Vec<_>>(),
        [r.heap_ref().unwrap()]
    );
    assert_eq!(
        c.throwable_trace(r).unwrap().unwrap(),
        [TraceFrame {
            method: p.ctors[0],
            pc: 0
        }]
    );
    assert_eq!(c.heap().throwable_trace_usage().frames, 3);
    assert_eq!(c.heap().throwable_trace_usage().backtraces, 2);
}
#[test]
fn ordinary_native_field_writes_are_refused_after_kind_and_source_checks() {
    let mut f = fixture(Default::default());
    let r = f.h.allocate_instance(f.t).unwrap();
    for (field, v) in [
        (f.p.fields[0], Value::Reference(Reference::null())),
        (f.p.fields[1], Value::Int(3)),
    ] {
        assert!(
            matches!(f.h.put_field(r,field,v),Err(InstanceError::ReadOnlyThrowableState{field:f})if f==field)
        );
    }
    assert!(matches!(
        f.h.put_field(r, f.p.fields[1], Value::Long(3)),
        Err(InstanceError::Heap(HeapError::ValueKindMismatch { .. }))
    ));
    assert_eq!(f.h.get_field(r, f.p.fields[1]).unwrap(), Value::Int(0));
}
#[test]
fn snapshot_zero_exact_and_cumulative_limits_are_separate_from_publication() {
    let mut f = fixture(TraceLimits {
        max_snapshots: 2,
        max_frames: 2,
        max_frames_per_snapshot: 1,
        max_work_units: u64::MAX,
    });
    let t = f.t;
    let e = f.error;
    let m = f.p.ctors[0];
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    assert!(matches!(
        c.begin_trace_snapshot(&k, 2).unwrap_err().kind,
        E::Limit {
            resource: R::FramesPerSnapshot,
            ..
        }
    ));
    let s = snapshot(&mut c, &k, m, 1);
    drop(s);
    let s = snapshot(&mut c, &k, m, 1);
    drop(s);
    assert!(matches!(
        c.begin_trace_snapshot(&k, 0).unwrap_err().kind,
        E::Limit {
            resource: R::Snapshots,
            ..
        }
    ));
    assert_eq!(c.heap().throwable_trace_usage().frames, 2);
    assert_eq!(c.heap().throwable_trace_usage().backtraces, 0);
}
#[test]
fn work_failure_before_begin_preserves_counters_and_old_pair() {
    let mut f = fixture(TraceLimits {
        max_work_units: 0,
        ..Default::default()
    });
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    assert!(matches!(
        c.begin_trace_snapshot(&k, 0).unwrap_err().kind,
        E::Limit {
            resource: R::TraceWork,
            limit: 0,
            required: 1
        }
    ));
    assert_eq!(c.heap().throwable_trace_usage(), TraceUsage::default());
    assert_eq!(c.throwable_trace(r).unwrap(), None);
}
#[test]
fn reserve_failure_retains_paid_creation_and_never_replaces_old_pair() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    capture(&mut c, &k, &p, r, 1);
    let old = c.heap().get_field(r, p.fields[0]).unwrap();
    c.heap_mut().throwables.as_mut().unwrap().fail_reserve_after = Some(0);
    assert!(matches!(
        c.begin_trace_snapshot(&k, 3).unwrap_err().kind,
        E::AllocationFailure
    ));
    assert_eq!(c.heap().throwable_trace_usage().snapshots, 2);
    assert_eq!(c.heap().throwable_trace_usage().frames, 4);
    assert_eq!(c.heap().get_field(r, p.fields[0]).unwrap(), old);
    c.heap_mut().throwables.as_mut().unwrap().fail_reserve_after = None;
    let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    let s = snapshot(&mut c, &k, p.ctors[0], 2);
    c.heap_mut().throwables.as_mut().unwrap().fail_reserve_after = Some(0);
    assert!(matches!(
        c.commit_throwable_capture(&k, &a, s).unwrap_err().kind,
        E::AllocationFailure
    ));
    assert_eq!(c.heap().get_field(r, p.fields[0]).unwrap(), old);
    assert_eq!(c.heap().throwable_trace_usage().backtraces, 1);
}
#[test]
fn builder_exact_count_invalid_pc_and_name_filter_have_funded_work() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    let mut b = c.begin_trace_snapshot(&k, 1).unwrap();
    assert!(matches!(
        c.append_trace_frame(
            &k,
            &mut b,
            TraceFrame {
                method: p.ctors[0],
                pc: 65535
            }
        )
        .unwrap_err()
        .kind,
        E::InvalidFrame { .. }
    ));
    c.append_trace_frame(
        &k,
        &mut b,
        TraceFrame {
            method: p.ctors[0],
            pc: 65534,
        },
    )
    .unwrap();
    assert!(matches!(
        c.append_trace_frame(
            &k,
            &mut b,
            TraceFrame {
                method: p.ctors[0],
                pc: 0
            }
        )
        .unwrap_err()
        .kind,
        E::InvalidSnapshot
    ));
    c.finish_trace_snapshot(&k, b).unwrap();
    assert_eq!(
        c.trace_method_kind(&k, p.ctors[0]).unwrap(),
        TraceMethodKind::Constructor
    );
    assert_eq!(
        c.trace_method_kind(&k, p.native).unwrap(),
        TraceMethodKind::FillInStackTrace
    );
}
#[test]
fn vm_bridge_has_exact_message_class_constructor_and_requires_current_capture() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let mut c = context(&mut f);
    let (k, exceptions) = claim(&mut c, t, e);
    assert!(matches!(
        c.vm_exception_spec(&k, VmExceptionKind::NullPointerException)
            .unwrap_err()
            .kind,
        E::UnsupportedProducer(VmExceptionKind::NullPointerException)
    ));
    let s = c
        .vm_exception_spec(&k, VmExceptionKind::ArithmeticException)
        .unwrap();
    assert_eq!(
        s.message_units(),
        "/ by zero".encode_utf16().collect::<Vec<_>>()
    );
    let (r, ctor) = c.allocate_vm_exception(&k, &s).unwrap();
    assert_eq!(ctor.method(), p.ctors[6]);
    assert_eq!(c.heap().instance_class(r).unwrap(), s.class());
    assert!(matches!(
        c.authenticate_vm_exception(&k, &exceptions, &s, r)
            .unwrap_err()
            .kind,
        E::InvalidState(_)
    ));
    capture(&mut c, &k, &p, r, 1);
    let thrown = c.authenticate_vm_exception(&k, &exceptions, &s, r).unwrap();
    assert_eq!(thrown.reference(), r);
}
#[test]
fn all_nested_errors_preserve_sources_and_clone_equality() {
    use std::error::Error;
    let e: Box<ThrowableAccessError> = HeapError::NullReference.into();
    assert_eq!(e, e.clone());
    assert!(e.source().is_some());
}
#[test]
fn forged_alternate_native_slots_fail_before_publication() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let alternate = f.g.field(t, "alternate", "Ljava/lang/Object;");
    let user = f.g.field(t, "user", "I");
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    capture(&mut c, &k, &p, r, 1);
    let old = c.heap().get_field(r, p.fields[0]).unwrap();
    let old_count = c.heap().allocated_instances();
    let mut a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    let s = c.heap().throwables.as_ref().unwrap();
    a.slots = [s.slots[1] + 2, s.slots[1] + 1];
    let snap = snapshot(&mut c, &k, p.ctors[0], 0);
    let before = c.heap().throwable_trace_usage();
    assert!(matches!(
        c.commit_throwable_capture(&k, &a, snap).unwrap_err().kind,
        E::InvalidCapture
    ));
    assert_eq!(c.heap().get_field(r, p.fields[0]).unwrap(), old);
    assert_eq!(
        c.heap().get_field(r, alternate).unwrap(),
        Value::Reference(Reference::null())
    );
    assert_eq!(c.heap().get_field(r, user).unwrap(), Value::Int(0));
    assert_eq!(c.heap().allocated_instances(), old_count);
    assert_eq!(
        c.heap().throwable_trace_usage().backtraces,
        before.backtraces
    );
}
#[test]
fn zero_length_buffer_does_not_consume_reservation_failpoint() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    c.heap_mut().throwables.as_mut().unwrap().fail_reserve_after = Some(0);
    let s = snapshot(&mut c, &k, p.ctors[0], 0);
    assert_eq!(
        c.heap().throwables.as_ref().unwrap().fail_reserve_after,
        Some(0)
    );
    assert!(matches!(
        c.commit_throwable_capture(&k, &a, s).unwrap_err().kind,
        E::AllocationFailure
    ));
    assert_eq!(c.throwable_trace(r).unwrap(), None);
    assert_eq!(c.heap().throwable_trace_usage().backtraces, 0);
}
#[test]
fn foreign_stale_and_wrong_payload_receivers_keep_typed_failures() {
    let mut f = fixture(Default::default());
    let mut other = fixture(Default::default());
    let t = f.t;
    let e = f.error;
    let native = f.p.native;
    let foreign = other.h.allocate_instance(other.t).unwrap();
    let array =
        f.h.allocate_primitive_array(PrimitiveArrayKind::Byte, 0)
            .unwrap();
    let r = f.h.allocate_instance(t).unwrap();
    let (h, i, g) = r.heap_ref().unwrap().components();
    let stale = Reference::issued(h, i, g + 1);
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    for x in [Reference::null(), foreign, stale] {
        assert!(matches!(c.throwable_trace(x).unwrap_err().kind, E::Heap(_)));
        assert!(matches!(
            c.prepare_throwable_capture(&k, native, x).unwrap_err().kind,
            E::Heap(_)
        ));
    }
    assert!(matches!(
        c.throwable_trace(array).unwrap_err().kind,
        E::InvalidReceiver
    ));
    assert_eq!(c.heap().throwable_trace_usage(), TraceUsage::default());
}
#[test]
fn reciprocal_token_corruption_never_changes_current_trace_or_fields() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let other = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    capture(&mut c, &k, &p, r, 1);
    capture(&mut c, &k, &p, other, 1);
    let Value::Reference(token) = c.heap().get_field(r, p.fields[0]).unwrap() else {
        panic!()
    };
    let index = token.heap_ref().unwrap().components().1;
    let before = c.heap().throwable_trace_usage();
    for damage in 0..4 {
        let crate::heap::EntryPayload::Backtrace(bt) = &mut c.heap_mut().entries[index].payload
        else {
            panic!()
        };
        let owner = bt.owner;
        let actual = bt.actual;
        let fp = bt.fingerprint;
        match damage {
            0 => bt.owner = other.heap_ref().unwrap(),
            1 => bt.actual = p.object,
            2 => bt.fingerprint.heap += 1,
            _ => bt.frames.push(TraceFrame {
                method: p.ctors[0],
                pc: 0,
            }),
        };
        assert!(matches!(
            c.throwable_trace(r).unwrap_err().kind,
            E::InvalidState(_)
        ));
        let crate::heap::EntryPayload::Backtrace(bt) = &mut c.heap_mut().entries[index].payload
        else {
            panic!()
        };
        bt.owner = owner;
        bt.actual = actual;
        bt.fingerprint = fp;
        if damage == 3 {
            bt.frames.pop();
        }
    }
    assert_eq!(c.heap().throwable_trace_usage(), before);
    assert_eq!(c.throwable_trace(r).unwrap().unwrap().len(), 1);
}
#[test]
fn forged_snapshot_claim_receiver_and_vm_spec_authority_are_rejected() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    capture(&mut c, &k, &p, r, 1);
    let old = c.heap().get_field(r, p.fields[0]).unwrap();
    let mut a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    a.actual = p.object;
    let s = snapshot(&mut c, &k, p.ctors[0], 0);
    assert!(c.commit_throwable_capture(&k, &a, s).is_err());
    let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    let mut s = snapshot(&mut c, &k, p.ctors[0], 0);
    s.fingerprint.heap += 1;
    assert!(matches!(
        c.commit_throwable_capture(&k, &a, s).unwrap_err().kind,
        E::InvalidCapture
    ));
    let mut spec = c
        .vm_exception_spec(&k, VmExceptionKind::ArithmeticException)
        .unwrap();
    spec.constructor = p.ctors[0];
    assert!(matches!(
        c.allocate_vm_exception(&k, &spec).unwrap_err().kind,
        E::InvalidBinding(_)
    ));
    assert_eq!(c.heap().get_field(r, p.fields[0]).unwrap(), old);
}
#[test]
fn instance_then_payload_commit_caps_preserve_old_pair_and_paid_frames() {
    for instances in [true, false] {
        let mut f = fixture(Default::default());
        let p = f.p.clone();
        let t = f.t;
        let e = f.error;
        let r = f.h.allocate_instance(t).unwrap();
        let mut c = context(&mut f);
        let (k, _) = claim(&mut c, t, e);
        capture(&mut c, &k, &p, r, 1);
        let old = c.heap().get_field(r, p.fields[0]).unwrap();
        let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
        let s = snapshot(&mut c, &k, p.ctors[0], 2);
        if instances {
            let h = c.heap_mut();
            h.instances.as_mut().unwrap().options.max_instances = h.allocated_instances();
            h.options.max_payload_bytes = h.payload_bytes();
        } else {
            let h = c.heap_mut();
            h.options.max_payload_bytes = h.payload_bytes();
        }
        let before = c.heap().throwable_trace_usage();
        let er = c.commit_throwable_capture(&k, &a, s).unwrap_err();
        if instances {
            assert!(
                matches!(er.kind,E::Instance(ref x)if matches!(x.as_ref(),InstanceError::ResourceLimit{resource:InstanceResource::Instances,..}))
            )
        } else {
            assert!(
                matches!(er.kind,E::Heap(ref x)if matches!(x.as_ref(),HeapError::ResourceLimit{resource:HeapResource::PayloadBytes,..}))
            )
        }
        assert_eq!(c.heap().get_field(r, p.fields[0]).unwrap(), old);
        assert_eq!(c.heap().throwable_trace_usage().frames, before.frames);
        assert_eq!(c.heap().throwable_trace_usage().backtraces, 1);
    }
}
#[test]
fn snapshot_hard_depth_and_frames_preflight_have_no_creation_debit() {
    let mut f = fixture(TraceLimits {
        max_frames: 0,
        max_frames_per_snapshot: u64::MAX,
        max_snapshots: u64::MAX,
        max_work_units: u64::MAX,
    });
    let t = f.t;
    let e = f.error;
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    assert!(matches!(
        c.begin_trace_snapshot(&k, i32::MAX as usize + 1)
            .unwrap_err()
            .kind,
        E::UnrepresentableDepth { .. }
    ));
    assert!(matches!(
        c.begin_trace_snapshot(&k, 1).unwrap_err().kind,
        E::Limit {
            resource: R::Frames,
            ..
        }
    ));
    assert_eq!(c.heap().throwable_trace_usage().snapshots, 0);
    assert_eq!(c.heap().throwable_trace_usage().frames, 0);
    assert_eq!(c.heap().throwable_trace_usage().work_units, 2);
}
#[test]
fn immutable_trace_buffer_moves_and_exact_work_is_not_frame_redebited() {
    let mut f = fixture(TraceLimits {
        max_work_units: 6,
        ..Default::default()
    });
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    let mut b = c.begin_trace_snapshot(&k, 3).unwrap();
    for pc in [2, 1, 0] {
        c.append_trace_frame(
            &k,
            &mut b,
            TraceFrame {
                method: p.ctors[0],
                pc,
            },
        )
        .unwrap();
    }
    let ptr = b.frames.as_ptr();
    let s = c.finish_trace_snapshot(&k, b).unwrap();
    assert_eq!(s.frames.as_ptr(), ptr);
    c.commit_throwable_capture(&k, &a, s).unwrap();
    let trace = c.throwable_trace(r).unwrap().unwrap();
    assert_eq!(trace.as_ptr(), ptr);
    assert_eq!(trace.iter().map(|f| f.pc).collect::<Vec<_>>(), [2, 1, 0]);
    assert_eq!(
        c.heap().throwable_trace_usage(),
        TraceUsage {
            snapshots: 1,
            frames: 3,
            backtraces: 1,
            work_units: 6
        }
    );
}
#[test]
fn no_binding_or_unrelated_native_classifier_adds_zero_field_work() {
    let mut f = fixture(Default::default());
    let object = f.g.method(f.g.object, "<init>", "()V");
    let native = f.p.native;
    let other = f.p.ctors[0];
    let mut c = context(&mut f);
    let start = c.lookup_steps();
    assert!(!c.is_throwable_fill_in_stack_trace(object).unwrap());
    assert_eq!(c.lookup_steps(), start);
    assert!(!c.is_throwable_fill_in_stack_trace(other).unwrap());
    assert_eq!(c.lookup_steps(), start + 1);
    assert!(c.is_throwable_fill_in_stack_trace(native).unwrap());
    assert_eq!(c.lookup_steps(), start + 2);
    c.heap_mut().throwables = None;
    let start = c.lookup_steps();
    assert!(!c.is_throwable_fill_in_stack_trace(native).unwrap());
    assert_eq!(c.lookup_steps(), start);
}

#[test]
fn checked_and_constructor_commits_cannot_write_native_pair() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    let spec = c
        .vm_exception_spec(&k, VmExceptionKind::ArithmeticException)
        .unwrap();
    let (r, ctor) = c.allocate_vm_exception(&k, &spec).unwrap();
    let proof = c
        .heap()
        .constructor_field_proof(c.registry(), &ctor, r, p.fields[1])
        .unwrap();
    let reg = c.registry();
    assert!(matches!(
        c.heap_mut()
            .commit_constructor_field(reg, proof, Value::Int(5)),
        Err(InstanceError::ReadOnlyThrowableState { .. })
    ));
    assert!(matches!(
        c.heap_mut()
            .commit_reference_field(r, p.fields[0], Reference::null()),
        Err(InstanceError::ReadOnlyThrowableState { .. })
    ));
    assert_eq!(c.heap().get_field(r, p.fields[1]).unwrap(), Value::Int(0));
    assert_eq!(c.throwable_trace(r).unwrap(), None);
}
#[test]
fn scalar_counter_overflow_has_ordered_errors_and_no_partial_debit() {
    for resource in [R::TraceWork, R::Snapshots, R::Frames] {
        let mut f = fixture(TraceLimits {
            max_snapshots: u64::MAX,
            max_frames: u64::MAX,
            max_frames_per_snapshot: u64::MAX,
            max_work_units: u64::MAX,
        });
        let t = f.t;
        let e = f.error;
        let mut c = context(&mut f);
        let (k, _) = claim(&mut c, t, e);
        let s = c.heap_mut().throwables.as_mut().unwrap();
        match resource {
            R::TraceWork => s.usage.work_units = u64::MAX,
            R::Snapshots => s.usage.snapshots = u64::MAX,
            R::Frames => s.usage.frames = u64::MAX,
            _ => unreachable!(),
        };
        let before = c.heap().throwable_trace_usage();
        let err = c.begin_trace_snapshot(&k, 1).unwrap_err();
        assert!(matches!(err.kind,E::Overflow{resource:r}if r==resource));
        let after = c.heap().throwable_trace_usage();
        assert_eq!(after.snapshots, before.snapshots);
        assert_eq!(after.frames, before.frames);
        assert_eq!(
            after.work_units,
            if resource == R::TraceWork {
                before.work_units
            } else {
                before.work_units + 1
            }
        );
    }
}
#[test]
fn published_count_corruption_blocks_commit_and_preserves_token() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    capture(&mut c, &k, &p, r, 1);
    let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    let s = snapshot(&mut c, &k, p.ctors[0], 0);
    let old = c.heap().get_field(r, p.fields[0]).unwrap();
    c.heap_mut().throwables.as_mut().unwrap().usage.backtraces = 2;
    assert!(matches!(
        c.commit_throwable_capture(&k, &a, s).unwrap_err().kind,
        E::InvalidState(_)
    ));
    assert_eq!(c.heap().get_field(r, p.fields[0]).unwrap(), old);
}
#[test]
fn binding_wrong_plan_fingerprints_and_pristine_state_fail_without_publication() {
    for damage in 0..5 {
        let mut f = fixture(Default::default());
        f.h.throwables = None;
        let mut p = f.p.clone();
        match damage {
            0 => p.registry += 1,
            1 => p.ctor_keys[0].2 ^= 1,
            2 => p.native_key.2 ^= 1,
            3 => p.ordinals[0] = usize::MAX,
            _ => {
                f.h.allocate_primitive_array(PrimitiveArrayKind::Byte, 0)
                    .unwrap();
            }
        }
        assert!(
            f.h.bind_throwable_support(f.g.domain.registry(), &p, Default::default())
                .is_err()
        );
        assert!(f.h.throwables.is_none());
        assert_eq!(f.h.throwable_trace_usage(), TraceUsage::default());
    }
}
#[test]
fn legacy_context_and_same_size_heap_replacement_do_not_inherit_capability() {
    let mut f = fixture(Default::default());
    let mut resolver = crate::mirror_support::Resolver(&f.g.intended);
    let legacy = FieldResolutionContext::new_unnamed_modules(
        f.g.domain.registry(),
        &mut f.h,
        &mut resolver,
        Default::default(),
    )
    .unwrap();
    assert!(legacy.check_throwable_profile().is_err());
    drop(legacy);
    let mut replacement = f.g.heap(&f.g.classes);
    replacement
        .bind_reference_types(
            f.g.domain.registry(),
            f.h.reference_type_universe().unwrap(),
        )
        .unwrap();
    assert_eq!(
        replacement.retained_layout_classes(),
        f.h.retained_layout_classes()
    );
    let mut c = context(&mut f);
    let old = std::mem::replace(c.heap_mut(), replacement);
    assert!(matches!(
        c.check_throwable_profile().unwrap_err().kind,
        E::Context(_)
    ));
    *c.heap_mut() = old;
    c.check_throwable_profile().unwrap();
}
#[test]
fn begin_payload_preflight_does_not_debit_snapshot_or_frames() {
    let mut f = fixture(Default::default());
    let t = f.t;
    let e = f.error;
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    c.heap_mut().options.max_payload_bytes = 0;
    let err = c.begin_trace_snapshot(&k, 1).unwrap_err();
    assert!(
        matches!(err.kind,E::Heap(ref e)if matches!(e.as_ref(),HeapError::ResourceLimit{resource:HeapResource::PayloadBytes,limit:0,..}))
    );
    assert_eq!(
        c.heap().throwable_trace_usage(),
        TraceUsage {
            work_units: 1,
            ..Default::default()
        }
    );
    let b = c.begin_trace_snapshot(&k, 0).unwrap();
    c.finish_trace_snapshot(&k, b).unwrap();
}
#[test]
fn ten_thousand_frame_buffer_and_deep_observer_are_iterative() {
    let (mut g, t, body, p) = graph(0);
    let mut ids = g.classes.clone();
    let mut parent = T.to_owned();
    let mut leaf = t;
    for i in 0..10000 {
        let name = format!("deep/C{i}");
        leaf = g.add(g.boot, Class::new(&name, Some(&parent), 0x21));
        parent = name;
    }
    ids.push(leaf);
    g.enable(MemberResolutionLimits::default());
    g.prepare(&ids);
    let mut h = g.heap(&ids);
    g.bind_core(&mut h);
    g.domain.enable_class_mirror_sites(1000).unwrap();
    let eq = probe_compact_string_equality(g.domain.registry(), &body, Default::default())
        .unwrap()
        .plan
        .unwrap();
    let class = g
        .domain
        .registry()
        .lookup_class(
            g.boot,
            &"java/lang/Class".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    let string = g
        .domain
        .registry()
        .lookup_class(
            g.boot,
            &"java/lang/String".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    h.bind_class_mirrors(g.domain.registry(), class, 1000)
        .unwrap();
    h.bind_strings(g.domain.registry(), string, Default::default())
        .unwrap();
    h.bind_compact_string_bodies(g.domain.registry(), &body, Default::default())
        .unwrap();
    h.bind_compact_string_equality(g.domain.registry(), &eq, Default::default())
        .unwrap();
    h.bind_throwable_support(g.domain.registry(), &p, Default::default())
        .unwrap();
    let r = h.allocate_instance(leaf).unwrap();
    let mut steps = 0;
    let (actual, slots) = h
        .throwable_pair_slots(r, p.fields, &mut steps, Default::default())
        .unwrap();
    assert_eq!(actual, leaf);
    assert_eq!(steps, 10000 + 1 + 3);
    assert_eq!(
        h.trace_from_pair(r, actual, h.throwable_pair(r, actual, slots).unwrap())
            .unwrap(),
        None
    );
    let before = steps;
    assert!(
        h.throwable_pair_slots(
            r,
            p.fields,
            &mut steps,
            FieldResolutionOptions {
                max_lookup_steps: before + 10000,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(steps, before + 10000);
    drop((g, h));
    let mut f = fixture(Default::default());
    let t = f.t;
    let error = f.error;
    let p = f.p.clone();
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, error);
    capture(&mut c, &k, &p, r, 10000);
    assert_eq!(c.throwable_trace(r).unwrap().unwrap().len(), 10000);
    assert_eq!(c.heap().throwable_trace_usage().frames, 10000);
}
#[test]
fn discovery_actual_row_deltas_are_independently_funded() {
    let (g, t, _, _) = graph(0);
    let baseline = probe_throwable_support(g.domain.registry(), g.boot, t, Default::default())
        .unwrap()
        .usage
        .work_units;
    for (damage, extra) in [(1, 4), (2, 2), (3, 1)] {
        let (g, t, _, _) = graph(damage);
        let n = probe_throwable_support(g.domain.registry(), g.boot, t, Default::default())
            .unwrap()
            .usage
            .work_units;
        assert_eq!(n, baseline + extra);
    }
}
#[test]
fn same_domain_heaps_have_independent_trace_roots_and_builder_affinity() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let mut second = f.g.heap(&f.g.classes);
    second
        .bind_reference_types(
            f.g.domain.registry(),
            f.h.reference_type_universe().unwrap(),
        )
        .unwrap();
    let r = f.g.domain.registry();
    let string = r
        .lookup_class(
            f.g.boot,
            &"java/lang/String".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    let class = r
        .lookup_class(
            f.g.boot,
            &"java/lang/Class".encode_utf16().collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    let body = probe_compact_string_bodies(r, f.g.boot, string, Default::default())
        .unwrap()
        .plan
        .unwrap();
    let eq = probe_compact_string_equality(r, &body, Default::default())
        .unwrap()
        .plan
        .unwrap();
    second.bind_class_mirrors(r, class, 1000).unwrap();
    second.bind_strings(r, string, Default::default()).unwrap();
    second
        .bind_compact_string_bodies(r, &body, Default::default())
        .unwrap();
    second
        .bind_compact_string_equality(r, &eq, Default::default())
        .unwrap();
    second
        .bind_throwable_support(r, &p, Default::default())
        .unwrap();
    let first = f.h.allocate_instance(t).unwrap();
    let other = second.allocate_instance(t).unwrap();
    assert_ne!(first, other);
    let pending = {
        let mut c = context(&mut f);
        let (k, _) = claim(&mut c, t, e);
        capture(&mut c, &k, &p, first, 1);
        snapshot(&mut c, &k, p.ctors[0], 2)
    };
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(&mut second, Default::default(), Default::default())
            .unwrap();
    let (k, _) = claim(&mut c, t, e);
    let a = c.prepare_throwable_capture(&k, p.native, other).unwrap();
    assert!(matches!(
        c.commit_throwable_capture(&k, &a, pending)
            .unwrap_err()
            .kind,
        E::InvalidCapture
    ));
    assert_eq!(c.throwable_trace(other).unwrap(), None);
    capture(&mut c, &k, &p, other, 0);
    let first_token = f.h.children(first).unwrap().next().unwrap();
    let second_token = c.heap().children(other).unwrap().next().unwrap();
    assert_ne!(first_token, second_token);
    assert_eq!(f.h.throwable_trace_usage().snapshots, 2);
    assert_eq!(c.heap().throwable_trace_usage().snapshots, 1);
    assert_eq!(
        c.heap().children(other).unwrap().collect::<Vec<_>>(),
        [second_token]
    );
}
#[test]
fn names_classify_without_descriptor_or_static_override_rules() {
    let mut f = fixture(Default::default());
    let t = f.t;
    let e = f.error;
    let m = f.g.method(t, "fillInStackTrace", "(J)V");
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    let before = c.heap().throwable_trace_usage().work_units;
    assert_eq!(
        c.trace_method_kind(&k, m).unwrap(),
        TraceMethodKind::FillInStackTrace
    );
    assert_eq!(c.heap().throwable_trace_usage().work_units - before, 18);
}
#[test]
fn post_arena_reservation_failure_preserves_nonempty_pair_counters_and_edges() {
    let mut f = fixture(Default::default());
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let r = f.h.allocate_instance(t).unwrap();
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    capture(&mut c, &k, &p, r, 1);
    let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
    let s = snapshot(&mut c, &k, p.ctors[0], 2);
    let fields = [
        c.heap().get_field(r, p.fields[0]).unwrap(),
        c.heap().get_field(r, p.fields[1]).unwrap(),
    ];
    let edges = c.heap().children(r).unwrap().collect::<Vec<_>>();
    let count = c.heap().allocated_instances();
    let bytes = c.heap().payload_bytes();
    let entries = c.heap().entries.len();
    c.heap_mut().throwables.as_mut().unwrap().fail_reserve_after = Some(1);
    assert!(matches!(
        c.commit_throwable_capture(&k, &a, s).unwrap_err().kind,
        E::AllocationFailure
    ));
    assert_eq!(c.heap().entries.len(), entries);
    assert_eq!(c.heap().allocated_instances(), count);
    assert_eq!(c.heap().payload_bytes(), bytes);
    assert_eq!(c.heap().get_field(r, p.fields[0]).unwrap(), fields[0]);
    assert_eq!(c.heap().get_field(r, p.fields[1]).unwrap(), fields[1]);
    assert_eq!(c.heap().children(r).unwrap().collect::<Vec<_>>(), edges);
    assert_eq!(c.heap().throwable_trace_usage().backtraces, 1);
}
#[test]
fn sealed_snapshot_length_and_one_less_commit_work_cannot_publish() {
    for forged in [false, true] {
        let mut f = fixture(TraceLimits {
            max_work_units: if forged { u64::MAX } else { 2 },
            ..Default::default()
        });
        let p = f.p.clone();
        let t = f.t;
        let e = f.error;
        let r = f.h.allocate_instance(t).unwrap();
        let mut c = context(&mut f);
        let (k, _) = claim(&mut c, t, e);
        let a = c.prepare_throwable_capture(&k, p.native, r).unwrap();
        let mut s = snapshot(&mut c, &k, p.ctors[0], 0);
        if forged {
            s.frames.push(TraceFrame {
                method: p.ctors[0],
                pc: 0,
            });
        }
        let error = c.commit_throwable_capture(&k, &a, s).unwrap_err();
        if forged {
            assert!(matches!(error.kind, E::InvalidCapture));
        } else {
            assert!(matches!(
                error.kind,
                E::Limit {
                    resource: R::TraceWork,
                    limit: 2,
                    required: 3
                }
            ));
        }
        assert_eq!(c.throwable_trace(r).unwrap(), None);
        assert_eq!(c.heap().throwable_trace_usage().backtraces, 0);
    }
}
#[test]
fn field_role_protection_is_binding_scoped_and_same_name_different_descriptor_stays_ordinary() {
    let mut f = fixture(Default::default());
    let r = f.h.allocate_instance(f.t).unwrap();
    let unrelated = f.g.field(f.t, "backtrace", "J");
    f.h.put_field(r, unrelated, Value::Long(123)).unwrap();
    assert_eq!(f.h.get_field(r, unrelated).unwrap(), Value::Long(123));
    let field = f.p.fields[1];
    f.h.throwables = None;
    f.h.put_field(r, field, Value::Int(7)).unwrap();
    assert_eq!(f.h.get_field(r, field).unwrap(), Value::Int(7));
}
#[test]
fn classifier_full_name_debit_is_atomic_after_two_scalar_units() {
    let mut f = fixture(TraceLimits {
        max_work_units: 17,
        ..Default::default()
    });
    let p = f.p.clone();
    let t = f.t;
    let e = f.error;
    let mut c = context(&mut f);
    let (k, _) = claim(&mut c, t, e);
    let e = c.trace_method_kind(&k, p.native).unwrap_err();
    assert!(matches!(
        e.kind,
        E::Limit {
            resource: R::TraceWork,
            limit: 17,
            required: 18
        }
    ));
    assert_eq!(c.heap().throwable_trace_usage().work_units, 2);
}

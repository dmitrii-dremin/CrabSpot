//! Public-service behavior plus private fixed-spec and counter guards. No engine
//! history or constructor-completion truth is asserted by these low-level tests.
use super::*;
use crate::mirror_support::{Class, Graph};
use crate::throwable::tests::graph;
use OperandExceptionErrorKind as E;
use OperandExceptionResource as Q;
const NAMES: [&str; 5] = [
    "java/lang/IndexOutOfBoundsException",
    "java/lang/ArrayIndexOutOfBoundsException",
    "java/lang/NegativeArraySizeException",
    "java/lang/ArrayStoreException",
    "java/lang/ClassCastException",
];
struct F {
    g: Graph,
    h: ManagedHeap,
    tp: ThrowablePlan,
    body: CompactStringBodyPlan,
    op: OperandExceptionPlan,
    t: ClassId,
    error: ClassId,
    caller: ClassId,
    sites: Vec<u16>,
    a: ClassId,
    b: ClassId,
    other: ClassId,
    unicode: ClassId,
    extra: Option<ClassId>,
}
fn fixture(limits: OperandDiagnosticLimits) -> F {
    fixture_options(limits, Default::default(), true)
}
fn fixture_options(limits: OperandDiagnosticLimits, strings: StringLimits, bind: bool) -> F {
    fixture_extra(limits, strings, bind, None)
}
fn fixture_extra(
    limits: OperandDiagnosticLimits,
    strings: StringLimits,
    bind: bool,
    extra: Option<Class>,
) -> F {
    let (mut g, t, body, tp) = graph(0);
    let boot = g.boot;
    let extra = extra.map(|c| g.add(boot, c));
    for (i, n) in NAMES.iter().enumerate() {
        let mut c = Class::new(
            n,
            Some(if i == 1 {
                NAMES[0]
            } else {
                "java/lang/RuntimeException"
            }),
            0x21,
        );
        c.method("<init>", "(Ljava/lang/String;)V", 1, &[0xb1], 0, 2);
        g.add(boot, c);
    }
    let a = g.add(boot, Class::plain("p/A"));
    let b = g.add(boot, Class::plain("p/LongerB"));
    let other_loader = g.loader();
    let other = g.add(other_loader, Class::plain("p/A"));
    let unicode = g.add(boot, Class::plain("p/Élément名"));
    for n in ["I", "V"] {
        g.add(boot, Class::plain(n));
    }
    let mut caller = Class::plain("p/Caller");
    let sites = [
        "p/A",
        "p/LongerB",
        "[I",
        "[[[I",
        "[Lp/A;",
        "missing/Type",
        "java/lang/Class",
    ]
    .iter()
    .map(|n| caller.class(n))
    .collect();
    caller.method("root", "()V", 9, &[0xb1], 0, 0);
    let caller = g.add(boot, caller);
    let op = probe_operand_exceptions(g.domain.registry(), &tp, Default::default())
        .unwrap()
        .plan;
    let mut h = g.ready();
    g.bind_core(&mut h);
    g.domain.enable_class_mirror_sites(1000).unwrap();
    let get = |n: &str| {
        g.domain
            .registry()
            .lookup_class(boot, &n.encode_utf16().collect::<Vec<_>>())
            .unwrap()
            .unwrap()
    };
    h.bind_class_mirrors(g.domain.registry(), get("java/lang/Class"), 1000)
        .unwrap();
    h.bind_strings(g.domain.registry(), get("java/lang/String"), strings)
        .unwrap();
    h.bind_compact_string_bodies(g.domain.registry(), &body, Default::default())
        .unwrap();
    let eq = probe_compact_string_equality(g.domain.registry(), &body, Default::default())
        .unwrap()
        .plan
        .unwrap();
    h.bind_compact_string_equality(g.domain.registry(), &eq, BootstrapAssertionPolicy::Disabled)
        .unwrap();
    h.bind_throwable_support(g.domain.registry(), &tp, Default::default())
        .unwrap();
    if bind {
        h.bind_operand_exceptions(g.domain.registry(), &op, limits)
            .unwrap();
    }
    let error = get("java/lang/Error");
    F {
        g,
        h,
        tp,
        body,
        op,
        t,
        error,
        caller,
        sites,
        a,
        b,
        other,
        unicode,
        extra,
    }
}
fn ctx(f: &mut F) -> FieldResolutionContext<'_> {
    f.g.domain
        .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
        .unwrap()
}
fn claim(
    c: &mut FieldResolutionContext<'_>,
    t: ClassId,
    e: ClassId,
) -> (InitializationClaim, ExceptionProfile) {
    let s = c.allocate_static_state(Default::default()).unwrap();
    let p = c.prepare_exception_profile(t).unwrap();
    (c.claim_initialization(&s, &p, e).unwrap(), p)
}
fn msg(
    c: &mut FieldResolutionContext<'_>,
    cl: &InitializationClaim,
    fault: OperandExceptionFault,
) -> (Reference, String) {
    let s = c.operand_exception_spec(cl, fault).unwrap();
    let r = c.new_operand_exception_message(cl, &s).unwrap();
    (
        r,
        String::from_utf16(c.heap().string_units(r).unwrap().unwrap()).unwrap(),
    )
}
fn array(rank: u8, leaf: ArrayElement, l: LoaderId) -> ReferenceType {
    ReferenceType::Array {
        dimensions: rank,
        element: leaf,
        defining_loader: l,
    }
}
fn limit(e: Box<OperandExceptionError>, q: Q, n: u64) {
    assert!(
        matches!(e.kind,E::Limit{resource,required,..}if resource==q&&required==n),
        "{e:?}"
    );
}
#[test]
fn detailed_bounds_null_and_store_have_single_original_path() {
    let mut f = fixture(Default::default());
    let a = f.a;
    let b = f.b;
    let mut c = ctx(&mut f);
    let r = c
        .allocate_reference_array(ReferenceType::Class(a), 2)
        .unwrap();
    let value = c.heap_mut().allocate_instance(a).unwrap();
    let wrong = c.heap_mut().allocate_instance(b).unwrap();
    c.write_reference_array(r, 0, value).unwrap();
    let before = c.lookup_steps();
    assert_eq!(
        c.read_reference_array_detailed(r, -1).unwrap(),
        OperandOutcome::Fault(ArrayOperandFault::Bounds {
            index: -1,
            length: 2
        })
    );
    assert_eq!(c.lookup_steps(), before);
    assert_eq!(
        c.read_reference_array(r, -1),
        Err(ArrayAccessError::Java(ArrayJavaError::IndexOutOfBounds))
    );
    assert_eq!(
        c.write_reference_array_detailed(r, 2, Reference::issued(u64::MAX, 0, 1))
            .unwrap(),
        OperandOutcome::Fault(ArrayOperandFault::Bounds {
            index: 2,
            length: 2
        })
    );
    assert_eq!(
        c.read_reference_array_detailed(Reference::null(), -1),
        Err(ArrayAccessError::Java(ArrayJavaError::NullPointer))
    );
    assert_eq!(
        c.write_reference_array_detailed(r, 0, wrong).unwrap(),
        OperandOutcome::Fault(ArrayOperandFault::Store {
            source: ReferenceType::Class(b),
            component: ReferenceType::Class(a)
        })
    );
    assert_eq!(
        c.write_reference_array(r, 0, wrong),
        Err(ArrayAccessError::Java(ArrayJavaError::ArrayStore))
    );
    assert_eq!(
        c.read_reference_array_detailed(r, 0).unwrap(),
        OperandOutcome::Value(value)
    );
    assert_eq!(
        c.heap().children(r).unwrap().collect::<Vec<_>>(),
        [value.heap_ref().unwrap()]
    );
    assert_eq!(c.heap().operand_diagnostic_usage(), Default::default());
}
#[test]
fn detailed_host_relation_and_late_load_stay_host() {
    let mut f = fixture(Default::default());
    let a = f.a;
    let mut c = ctx(&mut f);
    let r = c
        .allocate_reference_array(ReferenceType::Class(a), 1)
        .unwrap();
    let foreign = Reference::issued(u64::MAX, 0, 1);
    assert!(matches!(
        c.write_reference_array_detailed(r, 0, foreign),
        Err(ArrayAccessError::Type(_))
    ));
    let slot = c
        .heap_mut()
        .entries
        .get_mut(r.heap_ref().unwrap().components().1)
        .unwrap();
    let crate::heap::EntryPayload::ReferenceArray(p) = &mut slot.payload else {
        panic!()
    };
    p.values[0] = foreign;
    assert!(matches!(
        c.read_reference_array_detailed(r, 0),
        Err(ArrayAccessError::Heap(HeapError::ForeignHeap))
    ));
    assert!(matches!(
        c.read_reference_array(r, 0),
        Err(ArrayAccessError::Heap(HeapError::ForeignHeap))
    ));
}
#[test]
fn detailed_negative_rank_and_cp_order_and_c5_logical_positions() {
    let mut f = fixture(Default::default());
    let l = f.g.boot;
    let a = f.a;
    let caller = f.caller;
    let sites = f.sites.clone();
    let mut c = ctx(&mut f);
    assert_eq!(
        c.allocate_reference_array_detailed(ReferenceType::Class(a), -4)
            .unwrap(),
        OperandOutcome::Fault(ArrayOperandFault::NegativeSize {
            length: -4,
            dimension: 0
        })
    );
    assert_eq!(
        c.new_reference_array_detailed(caller, sites[0], -4)
            .unwrap(),
        OperandOutcome::Fault(ArrayOperandFault::NegativeSize {
            length: -4,
            dimension: 0
        })
    );
    assert!(c.new_reference_array_detailed(caller, 0, -4).is_err());
    assert!(matches!(
        c.allocate_reference_array_detailed(
            array(255, ArrayElement::Primitive(PrimitiveArrayKind::Int), l),
            -4
        ),
        Err(ArrayAccessError::Type(_))
    ));
    for (counts, dimension, n) in [
        (vec![-3, -4, -5], 0, -3),
        (vec![0, -4, 2], 1, -4),
        (
            {
                let mut v = vec![0; 255];
                v[254] = -9;
                v
            },
            254,
            -9,
        ),
    ] {
        let before = c.lookup_steps();
        let rank = counts.len() as u8;
        assert_eq!(
            c.allocate_multidimensional_array_detailed(
                array(rank, ArrayElement::Primitive(PrimitiveArrayKind::Int), l),
                &counts
            )
            .unwrap(),
            OperandOutcome::Fault(ArrayOperandFault::NegativeSize {
                length: n,
                dimension
            })
        );
        assert_eq!(c.lookup_steps() - before, 2 + usize::from(dimension));
    }
    assert_eq!(
        c.new_multidimensional_array_detailed(caller, sites[3], &[0, -5])
            .unwrap(),
        OperandOutcome::Fault(ArrayOperandFault::NegativeSize {
            length: -5,
            dimension: 1
        })
    );
    assert_eq!(
        c.new_multidimensional_array(caller, sites[3], &[0, -5]),
        Err(ArrayAccessError::Java(ArrayJavaError::NegativeArraySize))
    );
    assert_eq!(c.heap().allocated_arrays(), 0);
}
#[test]
fn detailed_cast_preserves_null_cp_and_actual_source_identity() {
    let mut f = fixture(Default::default());
    let caller = f.caller;
    let sites = f.sites.clone();
    let a = f.a;
    let b = f.b;
    let mut c = ctx(&mut f);
    assert_eq!(
        c.check_cast_detailed(caller, sites[5], Reference::null())
            .unwrap(),
        OperandOutcome::Value(Reference::null())
    );
    assert!(c.check_cast_detailed(caller, 0, Reference::null()).is_err());
    let r = c.heap_mut().allocate_instance(a).unwrap();
    assert_eq!(
        c.check_cast_detailed(caller, sites[1], r).unwrap(),
        OperandOutcome::Fault(CastOperandFault::Mismatch {
            source: ReferenceType::Class(a),
            target: ReferenceType::Class(b)
        })
    );
    assert_eq!(
        c.check_cast(caller, sites[1], r),
        Err(TypeAccessError::Java(TypeJavaError::ClassCast))
    );
    assert_eq!(
        c.check_cast_detailed(caller, sites[0], r).unwrap(),
        OperandOutcome::Value(r)
    );
}
#[test]
fn numeric_messages_and_independent_exact_work() {
    for (fault, text, total) in [
        (
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
            "-1",
            6,
        ),
        (
            OperandExceptionFault::NegativeSize {
                length: i32::MIN,
                dimension: 254,
            },
            "-2147483648",
            24,
        ),
        (
            OperandExceptionFault::Bounds {
                index: -1,
                length: 2,
            },
            "Index -1 out of bounds for length 2",
            40,
        ),
        (
            OperandExceptionFault::Bounds {
                index: i32::MIN,
                length: 0,
            },
            "Index -2147483648 out of bounds for length 0",
            58,
        ),
        (
            OperandExceptionFault::Bounds {
                index: i32::MAX,
                length: 2,
            },
            "Index 2147483647 out of bounds for length 2",
            56,
        ),
    ] {
        let mut f = fixture(Default::default());
        let (t, e) = (f.t, f.error);
        let mut c = ctx(&mut f);
        let (cl, _) = claim(&mut c, t, e);
        let before = c.lookup_steps();
        let (_, actual) = msg(&mut c, &cl, fault);
        assert_eq!(actual, text);
        assert_eq!(c.lookup_steps(), before);
        assert_eq!(
            c.heap().operand_diagnostic_usage(),
            OperandDiagnosticUsage {
                work_units: total,
                message_units: text.len() as u64,
                messages: 1
            }
        );
        assert_eq!(c.heap().string_usage().work_units, text.len() as u64 + 1);
        assert_eq!(c.heap().allocated_instances(), 1);
    }
}
#[test]
fn diagnostic_limits_are_ordered_before_creation_transaction() {
    for (q, limits, required) in [
        (
            Q::MessageUnits,
            OperandDiagnosticLimits {
                max_message_units: 34,
                ..Default::default()
            },
            35,
        ),
        (
            Q::TotalMessageUnits,
            OperandDiagnosticLimits {
                max_total_message_units: 34,
                ..Default::default()
            },
            35,
        ),
        (
            Q::TemporaryBytes,
            OperandDiagnosticLimits {
                max_temporary_bytes: 69,
                ..Default::default()
            },
            70,
        ),
        (
            Q::DiagnosticWork,
            OperandDiagnosticLimits {
                max_work_units: 39,
                ..Default::default()
            },
            40,
        ),
    ] {
        let mut f = fixture(limits);
        let (t, e) = (f.t, f.error);
        let mut c = ctx(&mut f);
        let (cl, _) = claim(&mut c, t, e);
        let s = c
            .operand_exception_spec(
                &cl,
                OperandExceptionFault::Bounds {
                    index: -1,
                    length: 2,
                },
            )
            .unwrap();
        limit(
            c.new_operand_exception_message(&cl, &s).unwrap_err(),
            q,
            required,
        );
        assert_eq!(
            c.heap().operand_diagnostic_usage(),
            OperandDiagnosticUsage {
                work_units: 5,
                ..Default::default()
            }
        );
        assert_eq!(c.heap().string_usage(), StringUsage::default());
        assert_eq!(c.heap().allocated_instances(), 0);
    }
    let mut f = fixture(OperandDiagnosticLimits {
        max_work_units: 40,
        max_message_units: 35,
        max_total_message_units: 35,
        max_temporary_bytes: 70,
    });
    let (t, e) = (f.t, f.error);
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    assert_eq!(
        msg(
            &mut c,
            &cl,
            OperandExceptionFault::Bounds {
                index: -1,
                length: 2
            }
        )
        .1,
        "Index -1 out of bounds for length 2"
    );
}
#[test]
fn temporary_and_string_failures_retain_accepted_units_without_message() {
    for string_failure in [false, true] {
        let mut f = fixture_options(
            Default::default(),
            StringLimits {
                max_objects: if string_failure { 0 } else { 1000 },
                ..Default::default()
            },
            true,
        );
        if !string_failure {
            f.h.operand_exceptions.as_mut().unwrap().fail_reserve_after = Some(0);
        }
        let (t, e) = (f.t, f.error);
        let mut c = ctx(&mut f);
        let (cl, _) = claim(&mut c, t, e);
        let s = c
            .operand_exception_spec(
                &cl,
                OperandExceptionFault::Bounds {
                    index: -1,
                    length: 2,
                },
            )
            .unwrap();
        let err = c.new_operand_exception_message(&cl, &s).unwrap_err();
        assert!(if string_failure {
            matches!(err.kind, E::String(_))
        } else {
            matches!(err.kind, E::AllocationFailure)
        });
        assert_eq!(
            c.heap().operand_diagnostic_usage(),
            OperandDiagnosticUsage {
                work_units: 40,
                message_units: 35,
                messages: 0
            }
        );
        assert_eq!(c.heap().allocated_instances(), 0);
        assert_eq!(c.heap().string_usage(), Default::default());
        assert_eq!(c.heap().throwable_trace_usage(), Default::default());
    }
}
#[test]
fn repeated_messages_are_fresh_and_charge_cumulative_units() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    let s = c
        .operand_exception_spec(
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        )
        .unwrap();
    let a = c.new_operand_exception_message(&cl, &s).unwrap();
    let b = c.new_operand_exception_message(&cl, &s).unwrap();
    assert_ne!(a, b);
    assert_eq!(
        c.heap().operand_diagnostic_usage(),
        OperandDiagnosticUsage {
            work_units: 11,
            message_units: 4,
            messages: 2
        }
    );
    assert_eq!(c.heap().string_usage().objects, 2);
    assert_eq!(c.heap().string_usage().work_units, 6);
}
#[test]
fn spec_scalar_invalidity_and_zero_cap_do_not_create_buffers() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    for fault in [
        OperandExceptionFault::Bounds {
            index: 0,
            length: 1,
        },
        OperandExceptionFault::Bounds {
            index: -1,
            length: -1,
        },
        OperandExceptionFault::NegativeSize {
            length: 0,
            dimension: 0,
        },
        OperandExceptionFault::NegativeSize {
            length: -1,
            dimension: 255,
        },
    ] {
        assert!(matches!(
            c.operand_exception_spec(&cl, fault).unwrap_err().kind,
            E::InvalidFault
        ));
    }
    assert_eq!(c.heap().operand_diagnostic_usage().work_units, 4);
    c.heap
        .operand_exceptions
        .as_mut()
        .unwrap()
        .limits
        .max_work_units = 4;
    limit(
        c.operand_exception_spec(
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        )
        .unwrap_err(),
        Q::DiagnosticWork,
        5,
    );
    assert_eq!(c.heap().operand_diagnostic_usage().work_units, 4);
    assert_eq!(c.heap().allocated_instances(), 0);
}
#[test]
fn nominal_and_array_recipes_have_independently_derived_costs() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let (a, b, u) = (f.a, f.b, f.unicode);
    let l = f.g.boot;
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    for (source, target, text, work) in [
        (
            ReferenceType::Class(a),
            ReferenceType::Class(b),
            "class p.A cannot be cast to class p.LongerB",
            9 + 43,
        ),
        (
            array(2, ArrayElement::Primitive(PrimitiveArrayKind::Int), l),
            array(1, ArrayElement::Primitive(PrimitiveArrayKind::Long), l),
            "class [[I cannot be cast to class [J",
            3 + 36,
        ),
    ] {
        let before = c.heap().operand_diagnostic_usage();
        let (_, actual) = msg(&mut c, &cl, OperandExceptionFault::Cast { source, target });
        assert_eq!(actual, text);
        assert_eq!(
            c.heap().operand_diagnostic_usage().work_units - before.work_units,
            1 + work
        );
    }
    let before = c.heap().operand_diagnostic_usage();
    let (_, text) = msg(
        &mut c,
        &cl,
        OperandExceptionFault::Store {
            source: ReferenceType::Class(u),
            component: ReferenceType::Class(a),
        },
    );
    assert_eq!(text, "p.Élément名");
    assert_eq!(
        c.heap().operand_diagnostic_usage().work_units - before.work_units,
        8 + text.encode_utf16().count() as u64
    );
}
#[test]
fn all_primitive_tags_and_rank255_are_iterative() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let l = f.g.boot;
    let a = f.a;
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    for (p, tag) in [
        (PrimitiveArrayKind::Boolean, 'Z'),
        (PrimitiveArrayKind::Byte, 'B'),
        (PrimitiveArrayKind::Char, 'C'),
        (PrimitiveArrayKind::Short, 'S'),
        (PrimitiveArrayKind::Int, 'I'),
        (PrimitiveArrayKind::Long, 'J'),
        (PrimitiveArrayKind::Float, 'F'),
        (PrimitiveArrayKind::Double, 'D'),
    ] {
        let before = c.heap().operand_diagnostic_usage();
        let (_, s) = msg(
            &mut c,
            &cl,
            OperandExceptionFault::Store {
                source: array(255, ArrayElement::Primitive(p), l),
                component: ReferenceType::Class(a),
            },
        );
        assert_eq!(s, format!("{}{tag}", "[".repeat(255)));
        assert_eq!(
            c.heap().operand_diagnostic_usage().work_units - before.work_units,
            1 + 1 + 1 + 2 + 256
        );
    }
}
#[test]
fn equal_names_compare_full_leaf_and_only_different_loaders_add_suffix() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let (a, other) = (f.a, f.other);
    let l = f.g.boot;
    let ol = f.g.domain.registry().defining_loader(other).unwrap();
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    for (source, target, text, work) in [
        (
            ReferenceType::Class(a),
            ReferenceType::Class(other),
            "class p.A cannot be cast to class p.A (different defining loaders)",
            1 + 9 + 3 + 66,
        ),
        (
            array(2, ArrayElement::Class(a), l),
            array(2, ArrayElement::Class(other), ol),
            "class [[Lp.A; cannot be cast to class [[Lp.A; (different defining loaders)",
            1 + 9 + 3 + 74,
        ),
    ] {
        let before = c.heap().operand_diagnostic_usage();
        let (_, actual) = msg(&mut c, &cl, OperandExceptionFault::Cast { source, target });
        assert_eq!(actual, text);
        assert_eq!(
            c.heap().operand_diagnostic_usage().work_units - before.work_units,
            work
        );
    }
}
#[test]
fn malformed_type_is_deferred_to_message_without_relation_replay() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let l = f.g.boot;
    let a = f.a;
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    let before = c.lookup_steps();
    let s = c
        .operand_exception_spec(
            &cl,
            OperandExceptionFault::Store {
                source: array(0, ArrayElement::Primitive(PrimitiveArrayKind::Int), l),
                component: ReferenceType::Class(a),
            },
        )
        .unwrap();
    assert_eq!(c.heap().operand_diagnostic_usage().work_units, 1);
    assert!(matches!(
        c.new_operand_exception_message(&cl, &s).unwrap_err().kind,
        E::InvalidFault
    ));
    assert_eq!(c.lookup_steps(), before);
    assert_eq!(c.heap().operand_diagnostic_usage().work_units, 3);
    assert_eq!(c.heap().allocated_instances(), 0);
}
#[test]
fn spec_damage_cannot_change_fixed_class_ctor_kind_claim_or_fault() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    for damage in 0..7 {
        let mut s = c
            .operand_exception_spec(
                &cl,
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 0,
                },
            )
            .unwrap();
        match damage {
            0 => s.class = t,
            1 => s.constructor.owner = t,
            2 => s.kind = VmExceptionKind::ArithmeticException,
            3 => s.fingerprint.heap += 1,
            4 => {
                s.key = Key::of(
                    c.registry
                        .method(c.heap.operand_state().unwrap().plan.ctors[1])
                        .unwrap(),
                )
            }
            5 => {
                s.fault = OperandExceptionFault::Bounds {
                    index: -1,
                    length: 0,
                }
            }
            _ => {
                s.fault = OperandExceptionFault::NegativeSize {
                    length: 0,
                    dimension: 0,
                }
            }
        };
        // Keys may coincide across exact identical constructor rows: use scalar damage directly.
        if damage == 4 {
            s.key = Key::of(
                c.registry
                    .method(c.heap.throwables.as_ref().unwrap().plan.native)
                    .unwrap(),
            );
        }
        let before = c.heap().operand_diagnostic_usage();
        assert!(c.new_operand_exception_message(&cl, &s).is_err());
        assert!(c.allocate_operand_exception(&cl, &s).is_err());
        assert_eq!(c.heap().operand_diagnostic_usage(), before);
        assert_eq!(c.heap().allocated_instances(), 0);
    }
}
#[test]
fn four_fixed_allocations_authenticate_only_exact_current_capture() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let native = f.tp.native;
    let mut c = ctx(&mut f);
    let (cl, exceptions) = claim(&mut c, t, e);
    for fault in [
        OperandExceptionFault::Bounds {
            index: -1,
            length: 0,
        },
        OperandExceptionFault::NegativeSize {
            length: -3,
            dimension: 1,
        },
        OperandExceptionFault::Store {
            source: ReferenceType::Class(t),
            component: ReferenceType::Class(e),
        },
        OperandExceptionFault::Cast {
            source: ReferenceType::Class(t),
            target: ReferenceType::Class(e),
        },
    ] {
        let s = c.operand_exception_spec(&cl, fault).unwrap();
        let (r, ctor) = c.allocate_operand_exception(&cl, &s).unwrap();
        assert_eq!(ctor.method(), s.constructor());
        assert_eq!(c.heap().instance_class(r).unwrap(), s.class());
        assert!(
            c.authenticate_operand_exception(&cl, &exceptions, &s, r)
                .is_err()
        );
        let authority = c.prepare_throwable_capture(&cl, native, r).unwrap();
        let b = c.begin_trace_snapshot(&cl, 0).unwrap();
        let snap = c.finish_trace_snapshot(&cl, b).unwrap();
        c.commit_throwable_capture(&cl, &authority, snap).unwrap();
        let thrown = c
            .authenticate_operand_exception(&cl, &exceptions, &s, r)
            .unwrap();
        assert_eq!(thrown.reference(), r);
    }
    assert_eq!(c.heap().throwable_trace_usage().backtraces, 4);
    assert_eq!(c.heap().operand_diagnostic_usage().messages, 0);
}
#[test]
fn binding_is_idempotent_after_effects_but_changed_policy_is_not() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    {
        let mut c = ctx(&mut f);
        let (cl, _) = claim(&mut c, t, e);
        msg(
            &mut c,
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        );
    }
    let old = f.h.operand_diagnostic_usage();
    f.h.bind_operand_exceptions(f.g.domain.registry(), &f.op, Default::default())
        .unwrap();
    assert_eq!(f.h.operand_diagnostic_usage(), old);
    assert!(matches!(
        f.h.bind_operand_exceptions(
            f.g.domain.registry(),
            &f.op,
            OperandDiagnosticLimits {
                max_work_units: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        E::PolicyConflict
    ));
    assert_eq!(f.h.operand_diagnostic_usage(), old);
}
#[test]
fn first_binding_rejects_any_prior_array_or_paid_failed_string_work() {
    for damage in 0..3 {
        let mut f = fixture_options(Default::default(), Default::default(), false);
        match damage {
            0 => {
                f.h.allocate_primitive_array(PrimitiveArrayKind::Int, 0)
                    .unwrap();
            }
            1 => {
                let a = f.a;
                ctx(&mut f)
                    .allocate_reference_array(ReferenceType::Class(a), 0)
                    .unwrap();
            }
            _ => {
                let (t, e) = (f.t, f.error);
                let mut c = ctx(&mut f);
                let _ = claim(&mut c, t, e);
            }
        };
        assert!(matches!(
            f.h.bind_operand_exceptions(f.g.domain.registry(), &f.op, Default::default())
                .unwrap_err()
                .kind,
            E::InvalidBinding(_)
        ));
        assert!(f.h.operand_exceptions.is_none());
    }
}
#[test]
fn diagnostic_counter_overflow_transaction_preserves_last_representable_prefix() {
    for resource in [Q::TotalMessageUnits, Q::DiagnosticWork, Q::Messages] {
        let mut f = fixture(OperandDiagnosticLimits {
            max_work_units: u64::MAX,
            max_total_message_units: u64::MAX,
            ..Default::default()
        });
        let (t, e) = (f.t, f.error);
        let mut c = ctx(&mut f);
        let (cl, _) = claim(&mut c, t, e);
        let s = c
            .operand_exception_spec(
                &cl,
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 0,
                },
            )
            .unwrap();
        let state = c.heap.operand_exceptions.as_mut().unwrap();
        match resource {
            Q::TotalMessageUnits => state.usage.message_units = u64::MAX,
            Q::DiagnosticWork => state.usage.work_units = u64::MAX - 3,
            Q::Messages => state.usage.messages = u64::MAX,
            _ => unreachable!(),
        };
        let old = state.usage;
        let err = c.new_operand_exception_message(&cl, &s).unwrap_err();
        assert!(matches!(err.kind,E::Overflow{resource:r}if r==resource));
        let now = c.heap().operand_diagnostic_usage();
        assert_eq!(now.message_units, old.message_units);
        assert_eq!(now.messages, old.messages);
        assert_eq!(now.work_units, old.work_units + 3);
        assert_eq!(c.heap().allocated_instances(), 0);
    }
}
#[test]
fn discovery_limits_and_fixed_role_identity_are_paid_without_heap_effects() {
    let f = fixture(Default::default());
    let reg = f.g.domain.registry();
    let full = probe_operand_exceptions(reg, &f.tp, Default::default()).unwrap();
    assert_eq!(full.plan, f.op);
    let exact = probe_operand_exceptions(
        reg,
        &f.tp,
        OperandExceptionDiscoveryLimits {
            max_work_units: full.usage.work_units,
        },
    )
    .unwrap();
    assert_eq!(exact, full);
    let err = probe_operand_exceptions(
        reg,
        &f.tp,
        OperandExceptionDiscoveryLimits {
            max_work_units: full.usage.work_units - 1,
        },
    )
    .unwrap_err();
    assert!(err.discovery_usage.work_units < full.usage.work_units);
    assert!(matches!(
        err.kind,
        E::Limit {
            resource: Q::DiscoveryWork,
            ..
        }
    ));
    let zero = probe_operand_exceptions(
        reg,
        &f.tp,
        OperandExceptionDiscoveryLimits { max_work_units: 0 },
    )
    .unwrap_err();
    assert_eq!(zero.discovery_usage.work_units, 0);
    assert_eq!(f.h.operand_diagnostic_usage(), Default::default());
    let roles = f.op.public_roles();
    assert_ne!(roles.index_out_of_bounds, roles.array_index_out_of_bounds);
    assert_eq!(
        f.op.ids,
        [
            roles.index_out_of_bounds,
            roles.array_index_out_of_bounds,
            roles.negative_array_size,
            roles.array_store,
            roles.class_cast
        ]
    );
}
#[test]
fn string_copy_reservation_failure_keeps_accepted_diagnostics_and_string_work() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    let s = c
        .operand_exception_spec(
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        )
        .unwrap();
    c.heap.strings.as_mut().unwrap().fail_reserve_after = Some(0);
    assert!(
        matches!(c.new_operand_exception_message(&cl,&s).unwrap_err().kind,E::String(e)if matches!(*e,StringError::AllocationFailure))
    );
    assert_eq!(
        c.heap().operand_diagnostic_usage(),
        OperandDiagnosticUsage {
            work_units: 6,
            message_units: 2,
            messages: 0
        }
    );
    assert_eq!(c.heap().string_usage().objects, 0);
    assert_eq!(c.heap().string_usage().work_units, 3);
    assert_eq!(c.heap().allocated_instances(), 0);
}
#[test]
fn impossible_length_overflows_and_zero_buffer_do_not_reserve() {
    let mut f = fixture(OperandDiagnosticLimits {
        max_work_units: u64::MAX,
        max_message_units: u64::MAX,
        max_total_message_units: u64::MAX,
        max_temporary_bytes: u64::MAX,
    });
    let s = f.h.operand_exceptions.as_mut().unwrap();
    s.fail_reserve_after = Some(0);
    let before = s.usage;
    assert!(matches!(
        s.message_buffer(u64::MAX).unwrap_err().kind,
        E::Overflow {
            resource: Q::TemporaryBytes
        }
    ));
    assert_eq!(s.usage, before);
    assert!(s.message_buffer(0).unwrap().0.is_empty());
    assert_eq!(s.fail_reserve_after, Some(0));
    assert_eq!(s.usage, before);
    assert!(matches!(
        s.message_buffer(1).unwrap_err().kind,
        E::AllocationFailure
    ));
    assert_eq!(s.usage.message_units, 1);
    assert_eq!(s.usage.work_units, 1);
    assert_eq!(s.usage.messages, 0);
}
#[test]
fn direct_error_sources_remain_typed_and_diagnostic_errors_never_become_java() {
    use std::error::Error;
    let e = Box::<OperandExceptionError>::from(TypeAccessError::Java(TypeJavaError::ClassCast));
    assert!(matches!(e.kind, E::Type(_)));
    assert!(
        e.source()
            .unwrap()
            .downcast_ref::<TypeAccessError>()
            .is_some()
    );
    let e =
        Box::<OperandExceptionError>::from(FieldAccessError::Java(FieldJavaError::IllegalAccess));
    assert!(
        e.source()
            .unwrap()
            .downcast_ref::<FieldAccessError>()
            .is_some()
    );
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    let mut spec = c
        .operand_exception_spec(
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        )
        .unwrap();
    spec.constructor.index = usize::MAX;
    assert!(matches!(
        c.allocate_operand_exception(&cl, &spec).unwrap_err().kind,
        E::InvalidBinding(_)
    ));
    assert_eq!(c.heap().allocated_instances(), 0);
}
#[test]
fn profile_affinity_rejects_legacy_context_replacement_and_foreign_claim() {
    let mut f = fixture(Default::default());
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
    let (t, e) = (f.t, f.error);
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    let spec = c
        .operand_exception_spec(
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        )
        .unwrap();
    let mut other = fixture(Default::default());
    let (ot, oe) = (other.t, other.error);
    let mut oc = ctx(&mut other);
    let (other_claim, _) = claim(&mut oc, ot, oe);
    let before = c.heap().operand_diagnostic_usage();
    assert!(
        c.new_operand_exception_message(&other_claim, &spec)
            .is_err()
    );
    assert_eq!(c.heap().operand_diagnostic_usage(), before);
    let old = std::mem::replace(c.heap_mut(), replacement);
    assert!(c.check_operand_exception_profile().is_err());
    assert!(c.new_operand_exception_message(&cl, &spec).is_err());
    assert_eq!(old.operand_diagnostic_usage(), before);
}
#[test]
fn same_domain_two_heaps_keep_spec_and_usage_local() {
    let mut f = fixture(Default::default());
    let ids = f.g.classes.clone();
    let mut second = f.g.heap(&ids);
    let u = f.h.reference_type_universe().unwrap();
    second
        .bind_reference_types(f.g.domain.registry(), u)
        .unwrap();
    let r = f.g.domain.registry();
    let class = r
        .lookup_class(f.g.boot, &crate::mirror_support::units("java/lang/Class"))
        .unwrap()
        .unwrap();
    let string = r
        .lookup_class(f.g.boot, &crate::mirror_support::units("java/lang/String"))
        .unwrap()
        .unwrap();
    second.bind_class_mirrors(r, class, 1000).unwrap();
    second.bind_strings(r, string, Default::default()).unwrap();
    let body = &f.body;
    second
        .bind_compact_string_bodies(r, body, Default::default())
        .unwrap();
    // Equality/Throwable plans are immutable fixed metadata, not heap authority.
    let eq = probe_compact_string_equality(r, body, Default::default())
        .unwrap()
        .plan
        .unwrap();
    second
        .bind_compact_string_equality(r, &eq, BootstrapAssertionPolicy::Disabled)
        .unwrap();
    second
        .bind_throwable_support(r, &f.tp, Default::default())
        .unwrap();
    second
        .bind_operand_exceptions(r, &f.op, Default::default())
        .unwrap();
    let (t, e) = (f.t, f.error);
    let mut first =
        f.g.domain
            .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
            .unwrap();
    let (cl, _) = claim(&mut first, t, e);
    let s = first
        .operand_exception_spec(
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        )
        .unwrap();
    let one = first.new_operand_exception_message(&cl, &s).unwrap();
    let mut second_context =
        f.g.domain
            .member_context_unnamed_modules(&mut second, Default::default(), Default::default())
            .unwrap();
    let (other_claim, _) = claim(&mut second_context, t, e);
    assert!(
        second_context
            .new_operand_exception_message(&other_claim, &s)
            .is_err()
    );
    assert_eq!(
        second_context.heap().operand_diagnostic_usage(),
        Default::default()
    );
    let (two, text) = msg(
        &mut second_context,
        &other_claim,
        OperandExceptionFault::NegativeSize {
            length: -1,
            dimension: 0,
        },
    );
    assert_eq!(text, "-1");
    assert_ne!(one, two);
    assert_eq!(
        second_context.heap().operand_diagnostic_usage(),
        f.h.operand_diagnostic_usage()
    );
}
fn discovery_graph(damage: u8, noise: u8) -> (Graph, ThrowablePlan) {
    let (mut g, _, _, tp) = graph(0);
    let boot = g.boot;
    if noise == 2 {
        let l = g.loader();
        g.add(l, Class::plain("foreign/Noise"));
    }
    for (i, n) in NAMES.iter().enumerate() {
        if damage == 1 && i == 4 {
            continue;
        }
        let parent = if damage == 2 && i == 2 {
            "java/lang/Exception"
        } else if i == 1 {
            NAMES[0]
        } else {
            "java/lang/RuntimeException"
        };
        let flags = if damage == 3 && i == 2 {
            0x20
        } else if damage == 4 && i == 2 {
            0x421
        } else {
            0x21
        };
        let mut c = Class::new(n, Some(parent), flags);
        if noise == 1 && i == 0 {
            c.method("x", "()V", 1, &[0xb1], 0, 1);
        }
        c.method(
            "<init>",
            if damage == 5 && i == 2 {
                "()V"
            } else {
                "(Ljava/lang/String;)V"
            },
            if damage == 6 && i == 2 { 2 } else { 1 },
            &[0xb1],
            0,
            2,
        );
        if damage == 7 && i == 4 {
            let l = g.loader();
            g.add(l, c);
        } else {
            g.add(boot, c);
        }
    }
    (g, tp)
}
#[test]
fn discovery_rejects_missing_foreign_parent_nonpublic_abstract_and_private_ctor() {
    for damage in 1..=7 {
        let (g, tp) = discovery_graph(damage, 0);
        let e = probe_operand_exceptions(g.domain.registry(), &tp, Default::default()).unwrap_err();
        assert!(
            matches!(e.kind, E::InvalidBinding(_)),
            "damage{damage}: {e:?}"
        );
        assert!(e.discovery_usage.work_units > 0);
    }
    let (g, tp) = discovery_graph(0, 0);
    let other = DefinitionRegistry::new(Default::default()).unwrap();
    assert!(matches!(
        probe_operand_exceptions(&other, &tp, Default::default())
            .unwrap_err()
            .kind,
        E::InvalidBinding(_)
    ));
    assert!(
        probe_operand_exceptions(
            g.domain.registry(),
            &tp,
            OperandExceptionDiscoveryLimits {
                max_work_units: u64::MAX
            }
        )
        .is_ok()
    );
}
#[test]
fn discovery_noise_rows_have_independent_hand_deltas() {
    let mut counts = Vec::new();
    for noise in 0..=2 {
        let (g, tp) = discovery_graph(0, noise);
        counts.push(
            probe_operand_exceptions(g.domain.registry(), &tp, Default::default())
                .unwrap()
                .usage
                .work_units,
        );
    }
    assert_eq!(counts[1] - counts[0], 2); // own short method row+name, unequal length skips comparison
    assert_eq!(counts[2] - counts[0], 1); // nonbootstrap registry row only
}
#[test]
fn discovery_full_u16_method_count_is_iterative_and_pays_each_actual_row() {
    fn run(count: usize) -> u64 {
        let (mut g, _, _, tp) = graph(0);
        let boot = g.boot;
        let mut c = Class::new(NAMES[0], Some("java/lang/RuntimeException"), 0x21);
        let names: Vec<_> = (0..256).map(|i| c.utf(&format!("m{i}"))).collect();
        let descriptors: Vec<_> = (0..256).map(|i| c.utf(&format!("()Lx{i};"))).collect();
        c.method("<init>", "(Ljava/lang/String;)V", 1, &[0xb1], 0, 2);
        let b = c.bytes();
        // Constructor row27, then class attributes_count2. Each unrelated native
        // row has a distinct name/descriptor pair from a fixed 256x256 CP grid.
        let start = b.len() - 2 - 27;
        assert_eq!(&b[start - 2..start], &1u16.to_be_bytes());
        let mut bytes = b[..start - 2].to_vec();
        bytes.extend((count as u16).to_be_bytes());
        for i in 0..count - 1 {
            for value in [0x101, names[i % 256], descriptors[i / 256], 0] {
                bytes.extend(value.to_be_bytes());
            }
        }
        bytes.extend(&b[start..]);
        g.domain
            .context(Default::default())
            .define(
                boot,
                &crate::mirror_support::units(NAMES[0]),
                &bytes,
                Default::default(),
            )
            .unwrap();
        for (i, n) in NAMES.iter().enumerate().skip(1) {
            let mut c = Class::new(
                n,
                Some(if i == 1 {
                    NAMES[0]
                } else {
                    "java/lang/RuntimeException"
                }),
                0x21,
            );
            c.method("<init>", "(Ljava/lang/String;)V", 1, &[0xb1], 0, 2);
            g.add(boot, c);
        }
        probe_operand_exceptions(g.domain.registry(), &tp, Default::default())
            .unwrap()
            .usage
            .work_units
    }
    assert_eq!(run(65535) - run(2), 2 * (65535 - 2));
}
#[test]
fn detailed_and_legacy_same_work_for_relation_false_quota_and_success() {
    fn run(detailed: bool, limit_steps: bool) -> (usize, usize, usize) {
        let mut f = fixture(Default::default());
        let (a, b) = (f.a, f.b);
        let mut c = ctx(&mut f);
        let r = c
            .allocate_reference_array(ReferenceType::Class(a), 1)
            .unwrap();
        let wrong = c.heap_mut().allocate_instance(b).unwrap();
        let good = c.heap_mut().allocate_instance(a).unwrap();
        let before = c.lookup_steps();
        if limit_steps {
            c.options.max_lookup_steps = before + 1;
        }
        if detailed {
            let result = c.write_reference_array_detailed(r, 0, wrong);
            if limit_steps {
                assert!(matches!(result, Err(ArrayAccessError::Type(_))));
            } else {
                assert!(matches!(
                    result,
                    Ok(OperandOutcome::Fault(ArrayOperandFault::Store { .. }))
                ));
                c.write_reference_array_detailed(r, 0, good).unwrap();
            }
        } else {
            let result = c.write_reference_array(r, 0, wrong);
            if limit_steps {
                assert!(matches!(result, Err(ArrayAccessError::Type(_))));
            } else {
                assert_eq!(
                    result,
                    Err(ArrayAccessError::Java(ArrayJavaError::ArrayStore))
                );
                c.write_reference_array(r, 0, good).unwrap();
            }
        }
        (
            c.lookup_steps() - before,
            c.heap().allocated_arrays(),
            c.heap().children(r).unwrap().count(),
        )
    }
    for limited in [false, true] {
        assert_eq!(run(true, limited), run(false, limited));
    }
}
#[test]
fn malformed_and_missing_destination_facts_preserve_host_and_old_field_contents() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let a = f.a;
    let l = f.g.boot;
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    for ty in [
        ReferenceType::Class(ClassId {
            registry: u64::MAX,
            index: 0,
        }),
        array(
            1,
            ArrayElement::Class(a),
            LoaderId {
                registry: u64::MAX,
                index: 0,
            },
        ),
        array(
            2,
            ArrayElement::Primitive(PrimitiveArrayKind::Int),
            LoaderId {
                registry: l.registry,
                index: usize::MAX,
            },
        ),
    ] {
        let s = c
            .operand_exception_spec(
                &cl,
                OperandExceptionFault::Store {
                    source: ReferenceType::Class(a),
                    component: ty,
                },
            )
            .unwrap();
        assert!(c.new_operand_exception_message(&cl, &s).is_err());
    }
    assert_eq!(c.heap().operand_diagnostic_usage().message_units, 0);
    assert_eq!(c.heap().allocated_instances(), 0);
}
#[test]
fn long_nominal_names_have_funded_copy_and_no_symbol_cache_or_relation_work() {
    let name = format!("p/{}", "x".repeat(60000));
    let mut f = fixture_extra(
        Default::default(),
        Default::default(),
        true,
        Some(Class::plain(&name)),
    );
    let (t, e) = (f.t, f.error);
    let source = f.extra.unwrap();
    let component = f.a;
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    let before = c.lookup_steps();
    let (_, actual) = msg(
        &mut c,
        &cl,
        OperandExceptionFault::Store {
            source: ReferenceType::Class(source),
            component: ReferenceType::Class(component),
        },
    );
    assert_eq!(actual, name.replace('/', "."));
    assert_eq!(c.lookup_steps(), before);
    assert_eq!(c.heap().operand_diagnostic_usage().work_units, 8 + 60002);
    assert_eq!(c.heap().operand_diagnostic_usage().message_units, 60002);
}
#[test]
fn actual_mirror_string_and_backtrace_types_produce_truthful_source_names() {
    let mut f = fixture(Default::default());
    let (t, e, a) = (f.t, f.error, f.a);
    let native = f.tp.native;
    let fields = f.tp.fields;
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    let dest = c
        .allocate_reference_array(ReferenceType::Class(a), 1)
        .unwrap();
    let mirror = c.class_mirror_for_type(ReferenceType::Class(a)).unwrap();
    let string = c.fresh_string(&[65]).unwrap();
    let s = c
        .operand_exception_spec(
            &cl,
            OperandExceptionFault::NegativeSize {
                length: -1,
                dimension: 0,
            },
        )
        .unwrap();
    let (r, _) = c.allocate_operand_exception(&cl, &s).unwrap();
    let authority = c.prepare_throwable_capture(&cl, native, r).unwrap();
    let b = c.begin_trace_snapshot(&cl, 0).unwrap();
    let snap = c.finish_trace_snapshot(&cl, b).unwrap();
    c.commit_throwable_capture(&cl, &authority, snap).unwrap();
    let Value::Reference(token) = c.heap().get_field(r, fields[0]).unwrap() else {
        panic!()
    };
    for (value, expected) in [
        (mirror, "java.lang.Class"),
        (string, "java.lang.String"),
        (token, "java.lang.Object"),
    ] {
        let OperandOutcome::Fault(ArrayOperandFault::Store { source, component }) =
            c.write_reference_array_detailed(dest, 0, value).unwrap()
        else {
            panic!()
        };
        let (_, actual) = msg(
            &mut c,
            &cl,
            OperandExceptionFault::Store { source, component },
        );
        assert_eq!(actual, expected);
    }
    assert_eq!(c.read_reference_array(dest, 0).unwrap(), Reference::null());
}
#[test]
fn nominal_i_and_v_are_not_primitive_array_tags_and_same_loader_has_no_suffix() {
    let mut f = fixture(Default::default());
    let (t, e) = (f.t, f.error);
    let l = f.g.boot;
    let r = f.g.domain.registry();
    let i = r.lookup_class(l, &[73]).unwrap().unwrap();
    let v = r.lookup_class(l, &[86]).unwrap().unwrap();
    let mut c = ctx(&mut f);
    let (cl, _) = claim(&mut c, t, e);
    for (source, target, expected, work) in [
        (
            ReferenceType::Class(i),
            ReferenceType::Class(i),
            "class I cannot be cast to class I",
            44,
        ),
        (
            ReferenceType::Class(v),
            ReferenceType::Class(i),
            "class V cannot be cast to class I",
            44,
        ),
        (
            array(1, ArrayElement::Primitive(PrimitiveArrayKind::Int), l),
            ReferenceType::Class(i),
            "class [I cannot be cast to class I",
            41,
        ),
    ] {
        let before = c.heap().operand_diagnostic_usage();
        assert_eq!(
            msg(&mut c, &cl, OperandExceptionFault::Cast { source, target }).1,
            expected
        );
        assert_eq!(
            c.heap().operand_diagnostic_usage().work_units - before.work_units,
            work
        );
    }
}
#[test]
fn pure_class_site_work_allows_first_binding_but_failed_string_work_does_not() {
    let mut f = fixture_options(Default::default(), Default::default(), false);
    let (caller, site) = (f.caller, f.sites[0]);
    ctx(&mut f).resolve_class_mirror_type(caller, site).unwrap();
    assert_eq!(f.h.class_mirror_count(), 0);
    f.h.bind_operand_exceptions(f.g.domain.registry(), &f.op, Default::default())
        .unwrap();
    let mut f = fixture_options(Default::default(), Default::default(), false);
    f.h.strings.as_mut().unwrap().fail_reserve_after = Some(0);
    assert!(ctx(&mut f).fresh_string(&[65]).is_err());
    assert_eq!(f.h.string_usage().objects, 0);
    assert_eq!(f.h.string_usage().work_units, 2);
    assert!(f.h.entries.is_empty());
    assert!(matches!(
        f.h.bind_operand_exceptions(f.g.domain.registry(), &f.op, Default::default())
            .unwrap_err()
            .kind,
        E::InvalidBinding(_)
    ));
}
#[test]
fn changed_plan_and_layout_shape_fail_binding_without_mutating_old_state() {
    let mut f = fixture_options(Default::default(), Default::default(), false);
    let mut wrong = f.op.clone();
    wrong.ctors[3] = f.tp.native;
    assert!(
        f.h.bind_operand_exceptions(f.g.domain.registry(), &wrong, Default::default())
            .is_err()
    );
    assert!(f.h.operand_exceptions.is_none());
    wrong = f.op.clone();
    wrong.ids[3] = f.a;
    assert!(
        f.h.bind_operand_exceptions(f.g.domain.registry(), &wrong, Default::default())
            .is_err()
    );
    assert!(f.h.operand_exceptions.is_none());
    f.h.bind_operand_exceptions(f.g.domain.registry(), &f.op, Default::default())
        .unwrap();
    let old = f.h.operand_diagnostic_usage();
    let boot = f.g.boot;
    let extra = f.g.add(boot, Class::plain("p/Late"));
    let plan = f.g.plan(extra);
    let before = f.h.retained_layout_classes();
    assert!(matches!(
        f.h.admit_instance_layouts(f.g.domain.registry(), &[&plan]),
        Err(InstanceError::MirrorLayoutSealed)
    ));
    assert_eq!(f.h.retained_layout_classes(), before);
    assert_eq!(f.h.operand_diagnostic_usage(), old);
    ctx(&mut f).check_operand_exception_profile().unwrap();
    f.h.operand_exceptions.as_mut().unwrap().fingerprint.classes += 1;
    assert!(ctx(&mut f).check_operand_exception_profile().is_err());
}

#[test]
fn full_string_affinity_rejects_before_spec_work_or_publication() {
    for field in 0..4 {
        let mut f = fixture(Default::default());
        let (t, e) = (f.t, f.error);
        let mut c = ctx(&mut f);
        let (cl, _) = claim(&mut c, t, e);
        c.heap
            .strings
            .as_mut()
            .unwrap()
            .corrupt_profile_for_operand_test(field);
        let usage = c.heap().operand_diagnostic_usage();
        let strings = c.heap().string_usage();
        let steps = c.lookup_steps();
        let instances = c.heap().allocated_instances();
        assert!(matches!(
            c.check_operand_exception_profile().unwrap_err().kind,
            E::String(_)
        ));
        assert!(matches!(
            c.operand_exception_spec(
                &cl,
                OperandExceptionFault::NegativeSize {
                    length: -1,
                    dimension: 0
                }
            )
            .unwrap_err()
            .kind,
            E::String(_)
        ));
        assert_eq!(c.heap().operand_diagnostic_usage(), usage);
        assert_eq!(c.heap().string_usage(), strings);
        assert_eq!(c.lookup_steps(), steps);
        assert_eq!(c.heap().allocated_instances(), instances);
        assert_eq!(c.heap().throwable_trace_usage(), TraceUsage::default());
    }
}

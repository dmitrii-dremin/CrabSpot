//! Genuine normalized Java instruction paths, never replacement length results.
#[path = "support/string_body_world.rs"]
mod world;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use world::*;
fn program() -> Class {
    let mut c = app();
    call(&mut c, "len", "length", "()I");
    call(&mut c, "empty", "isEmpty", "()Z");
    c
}
#[test]
fn normalized_code_and_shifted_cp_indices_use_real_java_bodies() {
    for padding in [0, 300] {
        let mut classes = platform();
        classes[0] = string_class(padding);
        let mut w = World::build(
            program(),
            classes,
            Default::default(),
            Default::default(),
            true,
            true,
        );
        let inputs = [
            vec![],
            vec![65, 0, 255],
            vec![65, 0x20ac, 0xd83d, 0xde80],
            vec![0xdfff],
            vec![0xd83d, 0xde80],
        ];
        let refs = inputs.iter().map(|u| w.fresh(u)).collect::<Vec<_>>();
        let len = w.method("len");
        let empty = w.method("empty");
        let string = w.string;
        let comparator = w.comparator;
        let case = w.case;
        let stream = w.stream;
        let flag = w.g.field(string, "COMPACT_STRINGS", "Z");
        let cmp =
            w.g.field(string, "CASE_INSENSITIVE_ORDER", "Ljava/util/Comparator;");
        let serial = w.g.field(
            string,
            "serialPersistentFields",
            "[Ljava/io/ObjectStreamField;",
        );
        w.with(|s,statics|{
            for (u,r) in inputs.iter().zip(&refs) {assert_eq!(int(run(s,len,*r)),u.len() as i32);assert_eq!(int(run(s,empty,*r)),i32::from(u.is_empty()));}
            assert_eq!(s.status(string).unwrap(),InitializationStatus::Completed);
            assert_eq!(s.status(comparator).unwrap(),InitializationStatus::Completed);
            assert_eq!(s.status(case).unwrap(),InitializationStatus::Completed);
            assert_eq!(s.status(stream).unwrap(),InitializationStatus::Uninitialized);
            assert_eq!(s.context().inspect_static(&statics,flag).unwrap(),Value::Int(1));
            let Value::Reference(cmp)=s.context().inspect_static(&statics,cmp).unwrap() else{panic!()};
            assert_eq!(s.context().heap().instance_class(cmp).unwrap(),case);
            let Value::Reference(serial)=s.context().inspect_static(&statics,serial).unwrap() else{panic!()};
            assert_eq!(s.context().heap().length(serial).unwrap(),0);
            assert!(matches!(s.context().heap().reference_array_type(serial).ok(),Some(ReferenceType::Array{dimensions:1,element:ArrayElement::Class(c),..}) if c==stream));
        });
        for (u, r) in inputs.iter().zip(refs) {
            let (handle, coder) = w.h.string_backing(r).unwrap().unwrap();
            let backing = value_reference(&mut w, r);
            assert_eq!(backing.heap_ref(), Some(handle));
            let expected = if u.iter().all(|u| *u <= 255) {
                assert_eq!(coder, 0);
                u.iter().map(|u| *u as u8).collect::<Vec<_>>()
            } else {
                assert_eq!(coder, 1);
                u.iter().flat_map(|u| u.to_le_bytes()).collect()
            };
            assert_eq!(w.h.length(backing).unwrap(), expected.len() as i32);
            for (i, b) in expected.iter().enumerate() {
                let Value::Int(actual) = w.h.load(backing, i as i32).unwrap() else {
                    panic!()
                };
                assert_eq!(actual as u8, *b);
            }
            assert_eq!(
                w.h.children(r).unwrap().collect::<Vec<_>>(),
                [backing.heap_ref().unwrap()]
            );
        }
    }
}
#[test]
fn is_empty_is_passive_and_independent_equal_objects_keep_distinct_stable_backings() {
    let mut w = World::new(program());
    let first = w.fresh(&[120]);
    let second = w.fresh(&[120]);
    let empty = w.fresh(&[]);
    let method = w.method("empty");
    let string = w.string;
    assert_eq!(w.h.string_body_usage(), StringBodyUsage::default());
    assert_eq!(w.h.string_backing(first).unwrap(), None);
    w.with(|s, _| {
        for (r, n) in [(first, 0), (second, 0), (empty, 1), (first, 0)] {
            assert_eq!(int(run(s, method, r)), n);
            assert_eq!(
                s.status(string).unwrap(),
                InitializationStatus::Uninitialized
            );
        }
    });
    let a = w.h.string_backing(first).unwrap().unwrap();
    let b = w.h.string_backing(second).unwrap().unwrap();
    assert_ne!(a.0, b.0);
    let usage = w.h.string_body_usage();
    assert_eq!(usage.backings, 3);
    assert_eq!(usage.backing_bytes, 2);
    assert_eq!(usage.work_units, 3 + 3 + 1 + 1);
    let before_a = a;
    let canonical = w.context().intern_string(first).unwrap();
    assert_eq!(canonical, first);
    assert_eq!(w.context().intern_string(second).unwrap(), first);
    assert_eq!(w.h.string_backing(first).unwrap(), Some(before_a));
    assert_eq!(w.h.string_backing(second).unwrap(), Some(b));
}
#[test]
fn exact_real_frames_code_and_fuel_are_shared_and_one_short_retains_backing() {
    for (frames, code, fuel, success, backing, status) in [
        (6, 59, 30, true, true, InitializationStatus::Completed),
        (1, 59, 30, false, false, InitializationStatus::Uninitialized),
        (2, 59, 30, false, true, InitializationStatus::Uninitialized),
        (5, 59, 30, false, true, InitializationStatus::Aborted),
        (6, 4, 30, false, false, InitializationStatus::Uninitialized),
        (6, 16, 30, false, true, InitializationStatus::Uninitialized),
        (6, 58, 30, false, true, InitializationStatus::Aborted),
        (6, 59, 3, false, false, InitializationStatus::Uninitialized),
        (6, 59, 4, false, true, InitializationStatus::Uninitialized),
        (6, 59, 29, false, true, InitializationStatus::Completed),
    ] {
        let mut w = World::new(program());
        let r = w.fresh(&[120]);
        let method = w.method("len");
        let string = w.string;
        w.with(|s, _| {
            let result = s.run_static(
                method,
                &[Value::Reference(r)],
                ExecutionOptions {
                    instruction_budget: fuel,
                },
                CallLimits {
                    max_frames: frames,
                    max_prepared_code_bytes: code,
                    ..Default::default()
                },
                Default::default(),
            );
            if success {
                assert_eq!(
                    result.unwrap(),
                    ExceptionOutcome::Returned(Some(Value::Int(1)))
                );
            } else {
                let error = result.unwrap_err();
                assert!(
                    matches!(
                        error.kind,
                        ExecutionErrorKind::FrameLimit { .. }
                            | ExecutionErrorKind::PreparedCodeLimit { .. }
                            | ExecutionErrorKind::InstructionBudgetExhausted { .. }
                    ),
                    "{frames}/{code}/{fuel}: {error:?}"
                );
                let (name, offset, site) = if frames == 1 {
                    ("length", None, Some(1))
                } else if frames == 2 {
                    ("coder", None, Some(6))
                } else if frames == 5 {
                    ("<init>", None, Some(1))
                } else if code == 4 {
                    ("len", None, None)
                } else if code == 16 {
                    ("coder", None, Some(6))
                } else if code == 58 {
                    ("<init>", None, Some(1))
                } else if fuel == 3 {
                    ("length", Some(1), Some(1))
                } else if fuel == 4 {
                    ("length", Some(4), Some(1))
                } else {
                    ("len", Some(4), None)
                };
                assert!(
                    error.method.as_ref().unwrap().name.equals_ascii(name),
                    "{error:?}"
                );
                assert_eq!(error.offset, offset, "{frames}/{code}/{fuel}");
                assert_eq!(
                    error.call_site.as_ref().map(|s| s.offset),
                    site,
                    "{frames}/{code}/{fuel}"
                );
                assert_eq!(
                    error.initialization_class,
                    if status == InitializationStatus::Aborted {
                        Some(string)
                    } else {
                        None
                    }
                );
                if frames < 6 {
                    assert_eq!(
                        error.kind,
                        ExecutionErrorKind::FrameLimit {
                            limit: frames,
                            required: frames + 1
                        }
                    );
                } else if code < 59 {
                    assert_eq!(
                        error.kind,
                        ExecutionErrorKind::PreparedCodeLimit {
                            limit: code,
                            required: match code {
                                4 => 5,
                                16 => 31,
                                _ => 59,
                            }
                        }
                    );
                } else {
                    assert_eq!(
                        error.kind,
                        ExecutionErrorKind::InstructionBudgetExhausted { limit: fuel }
                    );
                }
            }
            assert_eq!(
                s.context().heap().string_backing(r).unwrap().is_some(),
                backing,
                "{frames}/{code}/{fuel}"
            );
            assert_eq!(s.status(string).unwrap(), status, "{frames}/{code}/{fuel}");
        });
    }
}
#[test]
fn body_work_and_heap_array_caps_fail_without_losing_string_or_pool() {
    for (units, work, bytes, success, required) in [
        (vec![], 1, 0, true, 1),
        (vec![120], 2, 1, false, 3),
        (vec![120], 3, 1, true, 3),
        (vec![0x20ac], 3, 2, false, 4),
        (vec![0x20ac], 4, 2, true, 4),
        (vec![120], u64::MAX, 0, false, 0),
    ] {
        let mut w = World::build(
            program(),
            platform(),
            HeapOptions {
                max_payload_bytes: bytes,
                ..Default::default()
            },
            StringBodyLimits {
                max_work_units: work,
            },
            true,
            true,
        );
        let r = w.fresh(&units);
        let canonical = w.context().intern_string(r).unwrap();
        let m = w.method("empty");
        let result = w.with(|s, _| run(s, m, r));
        if success {
            assert!(result.is_ok());
            assert_eq!(w.h.string_body_usage().work_units, required);
            assert!(w.h.string_backing(r).unwrap().is_some());
        } else {
            let error = result.unwrap_err();
            assert!(
                matches!(error.kind, ExecutionErrorKind::FieldAccess(_)),
                "{error:?}"
            );
            assert_eq!(w.h.string_backing(r).unwrap(), None);
            assert_eq!(w.h.string_body_usage().backings, 0);
        }
        assert_eq!(w.h.string_units(r).unwrap(), Some(units.as_slice()));
        assert_eq!(w.context().intern_string(r).unwrap(), canonical);
    }
}
#[test]
fn denied_warm_read_preserves_already_materialized_pair() {
    let mut w = World::build(
        program(),
        platform(),
        Default::default(),
        StringBodyLimits { max_work_units: 3 },
        true,
        true,
    );
    let r = w.fresh(&[120]);
    let m = w.method("empty");
    w.with(|s, _| {
        assert_eq!(int(run(s, m, r)), 0);
        let before = s.context().heap().string_backing(r).unwrap();
        let e = run(s, m, r).unwrap_err();
        assert_eq!(e.offset, Some(1));
        assert!(matches!(e.kind, ExecutionErrorKind::FieldAccess(_)));
        assert_eq!(s.context().heap().string_backing(r).unwrap(), before);
        assert_eq!(s.context().heap().string_body_usage().work_units, 3);
    });
}
#[test]
fn frozen_backing_rejects_bytecode_and_raw_writes_after_ordinary_checks() {
    let mut c = program();
    c.method("store", "([BII)V", 1, &[0x2b, 0x1c, 0x1d, 0x54, 0xb1], 3, 4);
    let mut w = World::new(c);
    let r = w.fresh(&[255]);
    let m = w.method("empty");
    let store = w.g.method(w.owner, "store", "([BII)V");
    w.with(|s, _| {
        int(run(s, m, r));
    });
    let (handle, _) = w.h.string_backing(r).unwrap().unwrap();
    let a = value_reference(&mut w, r);
    assert_eq!(a.heap_ref(), Some(handle));
    assert!(matches!(
        w.h.store(a, -1, Value::Long(0)),
        Err(HeapError::IndexOutOfBounds { .. })
    ));
    assert!(matches!(
        w.h.store(a, 0, Value::Long(0)),
        Err(HeapError::ValueKindMismatch { .. })
    ));
    assert_eq!(
        w.h.store(a, 0, Value::Int(0)).unwrap_err(),
        HeapError::ReadOnlyStringBacking
    );
    // An existing registered body evaluator shares the same raw array guard.
    let receiver = w.h.allocate_instance(w.owner).unwrap();
    let error = execute_instance_method_body(
        &mut w.context(),
        store,
        receiver,
        &[Value::Reference(a), Value::Int(0), Value::Int(0)],
        Default::default(),
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ExecutionErrorKind::Heap(HeapError::ReadOnlyStringBacking)
    );
    assert_eq!(error.offset, Some(3));
    assert_eq!(w.h.load(a, 0).unwrap(), Value::Int(-1));
    assert_eq!(w.context().intern_string(r).unwrap(), r);
}
#[test]
fn old_profiles_gain_only_bound_code_and_existing_no_init_gates_remain() {
    for bind in [false, true] {
        let mut w = World::build(
            program(),
            platform(),
            Default::default(),
            Default::default(),
            bind,
            true,
        );
        let r = w.fresh(&[120]);
        let empty = w.g.method(w.string, "isEmpty", "()Z");
        let length = w.g.method(w.string, "length", "()I");
        let mut context = w.context();
        let result = execute_instance_method_body(
            &mut context,
            empty,
            r,
            &[],
            Default::default(),
            Default::default(),
        );
        if bind {
            assert_eq!(
                result.unwrap(),
                ExecutionOutcome::Returned(Some(Value::Int(0)))
            );
            let e = execute_instance_method_body(
                &mut context,
                length,
                r,
                &[],
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
            assert!(matches!(
                e.kind,
                ExecutionErrorKind::UnsupportedInstruction(0xb2)
            ));
            assert_eq!(e.offset, Some(0));
        } else {
            assert!(matches!(
                result.unwrap_err().kind,
                ExecutionErrorKind::MethodAccess(_)
            ));
        }
    }
}
#[test]
fn unavailable_reciprocal_nest_denies_actual_comparator_constructor_after_prior_writes() {
    for nest in [false, true] {
        let mut p = platform();
        p[1] = case_class(nest);
        let mut w = World::build(
            program(),
            p,
            Default::default(),
            Default::default(),
            true,
            true,
        );
        let r = w.fresh(&[120]);
        let m = w.method("len");
        let string = w.string;
        let flag = w.g.field(string, "COMPACT_STRINGS", "Z");
        w.with(|s, statics| {
            let result = run(s, m, r);
            if nest {
                assert_eq!(int(result), 1);
            } else {
                let e = result.unwrap_err();
                assert_eq!(
                    e.kind,
                    ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::IllegalAccessError)
                );
                assert_eq!(e.offset, Some(15));
                assert_eq!(e.method.as_ref().unwrap().owner, Some(string));
                assert_eq!(s.status(string).unwrap(), InitializationStatus::Aborted);
                assert_eq!(
                    s.context().inspect_static(&statics, flag).unwrap(),
                    Value::Int(1)
                );
            }
        });
    }
}
#[test]
fn unsupported_string_java_methods_still_refuse_real_selected_body() {
    let mut c = program();
    call(&mut c, "hash", "hashCode", "()I");
    let mut w = World::new(c);
    let r = w.fresh(&[120]);
    let m = w.method("hash");
    let e = w.with(|s, _| run(s, m, r)).unwrap_err();
    assert_eq!(e.offset, Some(1));
    assert!(matches!(e.kind, ExecutionErrorKind::MethodAccess(_)));
    assert_eq!(w.h.string_backing(r).unwrap(), None);
}

#[test]
fn heap_resource_order_precedes_encoding_charge_and_work_precedes_heap() {
    for (arrays, length, elements, bytes, resource) in [
        (0, 0, 0, 0, HeapResource::Arrays),
        (1, 0, 0, 0, HeapResource::ArrayLength),
        (1, 1, 0, 0, HeapResource::TotalElements),
        (1, 1, 1, 0, HeapResource::PayloadBytes),
    ] {
        let mut w = World::build(
            program(),
            platform(),
            HeapOptions {
                max_arrays: arrays,
                max_array_length: length,
                max_total_elements: elements,
                max_payload_bytes: bytes,
            },
            StringBodyLimits { max_work_units: 2 },
            true,
            true,
        );
        let r = w.fresh(&[120]);
        let m = w.method("empty");
        let e = w.with(|s, _| run(s, m, r)).unwrap_err();
        assert!(
            matches!(e.kind,ExecutionErrorKind::FieldAccess(ref e) if matches!(e.as_ref(),FieldAccessError::StringBody(e) if matches!(e.kind,StringBodyErrorKind::Heap(ref h) if matches!(h.as_ref(),HeapError::ResourceLimit{resource:r,..} if *r==resource))))
        );
        assert_eq!(w.h.string_body_usage().work_units, 2);
        assert_eq!(w.h.allocated_arrays(), 0);
        assert_eq!(w.h.string_backing(r).unwrap(), None);
    }
    let mut w = World::build(
        program(),
        platform(),
        HeapOptions {
            max_arrays: 0,
            ..Default::default()
        },
        StringBodyLimits { max_work_units: 0 },
        true,
        true,
    );
    let r = w.fresh(&[120]);
    let m = w.method("empty");
    let e = w.with(|s, _| run(s, m, r)).unwrap_err();
    assert!(
        matches!(e.kind,ExecutionErrorKind::FieldAccess(ref e) if matches!(e.as_ref(),FieldAccessError::StringBody(e) if matches!(e.kind,StringBodyErrorKind::Limit{resource:StringBodyResource::BackingWork,limit:0,required:1})))
    );
}
#[test]
fn repeated_selected_calls_reuse_real_code_cache_and_only_constant_backing_reads() {
    let mut c = program();
    let length = c.reference(10, STRING, "length", "()I");
    c.method(
        "twice",
        "(Ljava/lang/String;)I",
        9,
        &[
            vec![0x2a],
            instruction(0xb6, length),
            vec![0x2a],
            instruction(0xb6, length),
            vec![0x60, 0xac],
        ]
        .concat(),
        2,
        1,
    );
    let mut w = World::new(c);
    let r = w.fresh(&[120]);
    let m = w.method("twice");
    w.with(|s, _| {
        let result = s.run_static(
            m,
            &[Value::Reference(r)],
            Default::default(),
            CallLimits {
                max_prepared_code_bytes: 64,
                ..Default::default()
            },
            Default::default(),
        );
        assert_eq!(int(result), 2);
    });
    assert_eq!(
        w.h.string_body_usage(),
        StringBodyUsage {
            work_units: 6,
            backings: 1,
            backing_bytes: 1
        }
    );
}
#[test]
fn readonly_peek_and_raw_field_reads_never_mint_or_grant_private_access() {
    let mut w = World::new(program());
    let r = w.fresh(&[120]);
    let value = w.g.field(w.string, "value", "[B");
    assert_eq!(w.h.string_backing(r).unwrap(), None);
    assert!(matches!(
        w.h.get_field(r, value),
        Err(InstanceError::OpaqueStringObject { .. })
    ));
    assert_eq!(w.h.string_backing(r).unwrap(), None);
    let m = w.method("empty");
    w.with(|s, _| {
        int(run(s, m, r));
    });
    let before = w.h.string_body_usage();
    let pair = w.h.string_backing(r).unwrap();
    assert!(pair.is_some());
    assert!(matches!(
        w.h.get_field(r, value),
        Err(InstanceError::OpaqueStringObject { .. })
    ));
    assert_eq!(w.h.string_body_usage(), before);
    assert!(
        matches!(w.h.string_backing(Reference::null()).unwrap_err().kind,StringBodyErrorKind::Heap(ref e) if matches!(e.as_ref(),HeapError::NullReference))
    );
    let ordinary = w.h.allocate_instance(w.owner).unwrap();
    assert!(matches!(
        w.h.string_backing(ordinary).unwrap_err().kind,
        StringBodyErrorKind::InvalidReceiver
    ));
    let mut other = World::new(program());
    let foreign = other.fresh(&[]);
    assert!(
        matches!(w.h.string_backing(foreign).unwrap_err().kind,StringBodyErrorKind::Heap(ref e) if matches!(e.as_ref(),HeapError::ForeignHeap))
    );
}
#[test]
fn foreign_string_designation_and_interleaved_registry_rows_are_checked() {
    fn discover(extra: bool) -> (StringBodyDiscovery, ClassId, Vec<ClassId>) {
        let mut g = support::Graph::new();
        let boot = g.boot;
        let string = g.add(boot, string_class(0));
        if extra {
            let app = g.loader();
            g.add(app, Class::new(STRING, Some("java/lang/Object"), 0x31));
        }
        let mut roots = Vec::new();
        for c in platform().into_iter().skip(1) {
            roots.push(g.add(boot, c));
        }
        let discovered =
            probe_compact_string_bodies(g.domain.registry(), boot, string, Default::default())
                .unwrap();
        let wrong = g.loader();
        let error =
            probe_compact_string_bodies(g.domain.registry(), wrong, string, Default::default())
                .err()
                .unwrap();
        assert!(matches!(error.kind, StringBodyErrorKind::InvalidBinding(_)));
        assert_eq!(error.discovery_usage.work_units, 1);
        (discovered, string, roots)
    }
    let (a, _, roots) = discover(false);
    assert_eq!(
        a.plan.as_ref().unwrap().private_roots(),
        [roots[0], roots[1], roots[2]]
    );
    let (b, _, _) = discover(true);
    assert_eq!(b.usage.work_units, a.usage.work_units + 1);
}

#[test]
fn null_selection_precedes_body_access_or_backing_work() {
    let mut w = World::new(program());
    let m = w.method("empty");
    let string = w.string;
    w.with(|s, _| {
        let e = run(s, m, Reference::null()).unwrap_err();
        assert_eq!(
            e.kind,
            ExecutionErrorKind::UnsupportedVmException(VmExceptionKind::NullPointerException)
        );
        assert_eq!(e.offset, Some(1));
        assert_eq!(
            s.status(string).unwrap(),
            InitializationStatus::Uninitialized
        );
        assert_eq!(
            s.context().heap().string_body_usage(),
            StringBodyUsage::default()
        );
    });
}

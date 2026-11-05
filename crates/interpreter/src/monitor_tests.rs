use super::*;
use crabspot_runtime::{
    DefinitionRegistry, HeapError, HeapOptions, InstanceOptions, ManagedHeap, PrimitiveArrayKind,
    SameLoaderResolver, Value, build_hierarchy,
};
#[derive(Debug, PartialEq)]
struct State {
    claims: u64,
    keys: HashMap<MonitorKey, u64>,
    frames: HashMap<usize, (HashMap<MonitorKey, u64>, Option<MonitorKey>)>,
}
fn state(m: &MonitorLedger) -> State {
    State {
        claims: m.claims,
        keys: m.keys.clone(),
        frames: m
            .frames
            .iter()
            .map(|(d, f)| (*d, (f.explicit.clone(), f.implicit)))
            .collect(),
    }
}
fn keys() -> (ManagedHeap, MonitorKey, MonitorKey, MonitorKey) {
    let mut heap = ManagedHeap::new(Default::default()).unwrap();
    let mut key = || {
        MonitorKey::Object(
            heap.allocate_primitive_array(PrimitiveArrayKind::Int, 1)
                .unwrap()
                .heap_ref()
                .unwrap(),
        )
    };
    let a = key();
    let b = key();
    let c = key();
    (heap, a, b, c)
}
fn host(e: MonitorFailure) -> MonitorError {
    match e {
        MonitorFailure::Host(e) => e,
        other => panic!("expected host error, got {other:?}"),
    }
}
fn assert_limit(e: MonitorFailure, r: R, limit: u64, required: u64) {
    assert_eq!(
        host(e),
        MonitorError::Limit {
            resource: r,
            limit,
            required
        }
    )
}
fn assert_host(e: MonitorFailure) {
    assert!(matches!(
        e,
        MonitorFailure::Host(MonitorError::InvalidState(_))
    ))
}
fn audit(m: &MonitorLedger) {
    let mut keys = HashMap::<MonitorKey, u64>::new();
    let mut claims = 0;
    for f in m.frames.values() {
        assert!(!f.explicit.is_empty() || f.implicit.is_some());
        for (k, n) in &f.explicit {
            assert!(*n > 0);
            *keys.entry(*k).or_default() += n;
            claims += 1
        }
        if let Some(k) = f.implicit {
            *keys.entry(k).or_default() += 1;
            claims += 1
        }
    }
    assert_eq!(keys, m.keys);
    assert_eq!(claims, m.claims);
    m.coherent().unwrap()
}
fn object_bytes() -> Vec<u8> {
    let n = b"java/lang/Object";
    let mut b = vec![
        0xca,
        0xfe,
        0xba,
        0xbe,
        0,
        0,
        0,
        65,
        0,
        3,
        1,
        0,
        n.len() as u8,
    ];
    b.extend(n);
    b.extend([7, 0, 1]);
    for n in [0x21u16, 2, 0, 0, 0, 0, 0] {
        b.extend(n.to_be_bytes())
    }
    b
}
#[test]
fn crossed_explicit_keys_are_independent_and_empty_finish_is_free() {
    let (_heap, a, b, _) = keys();
    let mut m = MonitorLedger::new(MonitorLimits {
        max_steps: 4,
        ..Default::default()
    });
    m.check_new_frame(0).unwrap();
    m.enter_explicit(0, a).unwrap();
    m.enter_explicit(0, b).unwrap();
    audit(&m);
    m.exit_explicit(0, a).unwrap();
    assert_eq!(m.keys.get(&b), Some(&1));
    m.exit_explicit(0, b).unwrap();
    audit(&m);
    assert_eq!(m.steps, 4);
    m.finish(0).unwrap();
    m.check_new_frame(0).unwrap();
    assert_eq!(m.steps, 4);
    assert!(m.frames.is_empty());
}
#[test]
fn reentry_counts_depth_but_not_claims_and_key_recursion_is_thread_wide() {
    let (_heap, a, _, _) = keys();
    let mut m = MonitorLedger::new(MonitorLimits {
        max_active_monitors: 1,
        max_active_claims: 2,
        max_recursion: 3,
        ..Default::default()
    });
    m.enter_explicit(0, a).unwrap();
    m.enter_explicit(0, a).unwrap();
    assert_eq!(m.claims, 1);
    m.enter_implicit(1, a).unwrap();
    assert_eq!(m.claims, 2);
    assert_eq!(m.keys[&a], 3);
    let old = state(&m);
    assert_limit(m.enter_explicit(0, a).unwrap_err(), R::Recursion, 3, 4);
    assert_eq!(state(&m), old);
    m.finish(1).unwrap();
    m.exit_explicit(0, a).unwrap();
    m.exit_explicit(0, a).unwrap();
    audit(&m);
}
#[test]
fn implicit_receipt_cannot_be_consumed_by_same_or_other_frame_explicit_exit() {
    let (_heap, a, _, _) = keys();
    let mut m = MonitorLedger::new(Default::default());
    m.enter_implicit(0, a).unwrap();
    let old = state(&m);
    assert_eq!(m.exit_explicit(0, a), Err(imse()));
    assert_eq!(state(&m), old);
    assert_eq!(m.exit_explicit(1, a), Err(imse()));
    assert_eq!(state(&m), old);
    m.enter_explicit(0, a).unwrap();
    m.exit_explicit(0, a).unwrap();
    assert_eq!(state(&m), old);
    m.finish(0).unwrap();
    assert!(m.keys.is_empty());
    assert_eq!(m.steps, 6);
}
#[test]
fn child_receipt_release_preserves_callers_explicit_count() {
    let (_heap, a, _, _) = keys();
    let mut m = MonitorLedger::new(Default::default());
    m.enter_explicit(0, a).unwrap();
    m.enter_implicit(1, a).unwrap();
    m.finish(1).unwrap();
    assert_eq!(m.keys[&a], 1);
    assert_eq!(m.frames[&0].explicit[&a], 1);
    let old = state(&m);
    assert_eq!(m.finish(0), Err(imse()));
    assert_eq!(state(&m), old);
    m.exit_explicit(0, a).unwrap();
    m.finish(0).unwrap();
    audit(&m);
}
#[test]
fn wrong_key_exit_and_unbalanced_finish_are_java_and_preserve_counts() {
    let (_heap, a, b, _) = keys();
    let mut m = MonitorLedger::new(Default::default());
    assert_eq!(m.exit_explicit(0, a), Err(imse()));
    assert_eq!(m.steps, 1);
    m.enter_explicit(0, a).unwrap();
    let old = state(&m);
    assert_eq!(m.exit_explicit(0, b), Err(imse()));
    assert_eq!(m.finish(0), Err(imse()));
    assert_eq!(state(&m), old);
    assert_eq!(m.steps, 4);
    m.exit_explicit(0, a).unwrap();
}
#[test]
fn quota_precedence_is_steps_keys_claims_then_recursion() {
    let (_heap, a, _, _) = keys();
    for failing in 0..4 {
        let mut p = MonitorLimits {
            max_steps: 1,
            max_active_monitors: 1,
            max_active_claims: 1,
            max_recursion: 1,
        };
        match failing {
            0 => p.max_steps = 0,
            1 => {
                p.max_active_monitors = 0;
                p.max_active_claims = 0;
                p.max_recursion = 0
            }
            2 => {
                p.max_active_claims = 0;
                p.max_recursion = 0
            }
            _ => p.max_recursion = 0,
        }
        let mut m = MonitorLedger::new(p);
        let resource = [R::Steps, R::ActiveMonitors, R::ActiveClaims, R::Recursion][failing];
        assert_limit(m.enter_explicit(0, a).unwrap_err(), resource, 0, 1);
        assert_eq!(m.steps, u64::from(failing != 0));
        assert!(m.keys.is_empty());
        assert!(m.frames.is_empty());
        assert_eq!(m.claims, 0);
    }
}
#[test]
fn inclusive_caps_allow_reentry_when_no_new_claim_is_needed() {
    let (_heap, a, b, _) = keys();
    let mut m = MonitorLedger::new(MonitorLimits {
        max_steps: 5,
        max_active_monitors: 1,
        max_active_claims: 1,
        max_recursion: 2,
    });
    m.enter_explicit(0, a).unwrap();
    m.enter_explicit(0, a).unwrap();
    let old = state(&m);
    assert_limit(m.enter_explicit(0, b).unwrap_err(), R::ActiveMonitors, 1, 2);
    assert_eq!(state(&m), old);
    m.exit_explicit(0, a).unwrap();
    m.exit_explicit(0, a).unwrap();
    assert_eq!(m.steps, 5);
    m.finish(0).unwrap();
}
#[test]
fn exhausted_step_before_exit_or_finish_keeps_all_ownership() {
    let (_heap, a, _, _) = keys();
    for implicit in [false, true] {
        let mut m = MonitorLedger::new(MonitorLimits {
            max_steps: 1,
            ..Default::default()
        });
        if implicit {
            m.enter_implicit(0, a).unwrap()
        } else {
            m.enter_explicit(0, a).unwrap()
        }
        let old = state(&m);
        let e = if implicit {
            m.finish(0)
        } else {
            m.exit_explicit(0, a)
        }
        .unwrap_err();
        assert_limit(e, R::Steps, 1, 2);
        assert_eq!(state(&m), old);
        assert_eq!(m.steps, 1);
    }
}
#[test]
fn new_identity_frame_and_local_reservation_failpoints_leave_existing_rows_intact() {
    let (_heap, a, b, _) = keys();
    for failure in 0..3 {
        let mut m = MonitorLedger::new(Default::default());
        m.enter_explicit(0, a).unwrap();
        let old = state(&m);
        m.fail_reserve_after = Some(failure);
        assert_eq!(
            host(m.enter_explicit(1, b).unwrap_err()),
            MonitorError::AllocationFailure
        );
        assert_eq!(state(&m), old);
        assert_eq!(m.steps, 2);
        audit(&m);
    }
    let mut m = MonitorLedger::new(Default::default());
    m.enter_explicit(0, a).unwrap();
    let old = state(&m);
    m.fail_reserve_after = Some(0);
    assert_eq!(
        host(m.enter_explicit(0, b).unwrap_err()),
        MonitorError::AllocationFailure
    );
    assert_eq!(state(&m), old);
}
#[test]
fn existing_local_row_reservation_and_implicit_frame_reservation_are_atomic() {
    let (_heap, a, b, _) = keys();
    let mut m = MonitorLedger::new(Default::default());
    m.enter_explicit(0, a).unwrap();
    m.enter_explicit(1, b).unwrap();
    let old = state(&m);
    m.fail_reserve_after = Some(0);
    assert_eq!(
        host(m.enter_explicit(0, b).unwrap_err()),
        MonitorError::AllocationFailure
    );
    assert_eq!(state(&m), old);
    for failure in 0..2 {
        let mut m = MonitorLedger::new(Default::default());
        m.enter_explicit(0, a).unwrap();
        let old = state(&m);
        m.fail_reserve_after = Some(failure);
        assert_eq!(
            host(m.enter_implicit(1, b).unwrap_err()),
            MonitorError::AllocationFailure
        );
        assert_eq!(state(&m), old);
    }
}
#[test]
fn duplicate_activation_and_stale_depth_check_do_not_steal_receipt() {
    let (_heap, a, _, _) = keys();
    let mut m = MonitorLedger::new(Default::default());
    m.enter_implicit(0, a).unwrap();
    let old = state(&m);
    let steps = m.steps;
    assert_host(m.check_new_frame(0).unwrap_err());
    assert_eq!(m.steps, steps);
    assert_host(m.enter_implicit(0, a).unwrap_err());
    assert_eq!(state(&m), old);
    m.finish(0).unwrap();
    m.check_new_frame(0).unwrap();
    m.enter_explicit(0, a).unwrap();
    assert_host(m.check_new_frame(0).unwrap_err());
}
#[test]
fn corrupted_zero_missing_and_oversized_local_counts_are_host_errors() {
    let (_heap, a, _, _) = keys();
    for damage in 0..4 {
        let mut m = MonitorLedger::new(Default::default());
        m.enter_explicit(0, a).unwrap();
        match damage {
            0 => {
                m.frames.get_mut(&0).unwrap().explicit.insert(a, 0);
            }
            1 => {
                m.keys.remove(&a);
            }
            2 => {
                m.frames.get_mut(&0).unwrap().explicit.insert(a, 2);
            }
            _ => m.claims = 0,
        }
        let old = state(&m);
        assert_host(m.exit_explicit(0, a).unwrap_err());
        assert_eq!(state(&m), old);
    }
}
#[test]
fn finish_rejects_damaged_summary_or_receipt_before_java_balance_error() {
    let (_heap, a, b, _) = keys();
    for claims in [1, 2] {
        let mut m = MonitorLedger::new(Default::default());
        m.enter_explicit(0, a).unwrap();
        m.frames.get_mut(&0).unwrap().implicit = Some(b);
        m.claims = claims;
        let old = state(&m);
        assert_host(m.finish(0).unwrap_err());
        assert_eq!(state(&m), old);
    }
    let mut m = MonitorLedger::new(Default::default());
    m.frames.insert(0, FrameMonitors::default());
    assert_host(m.finish(0).unwrap_err());
    assert_eq!(m.steps, 0);
}
#[test]
fn staged_corruption_or_missing_reservation_cannot_publish_over_old_state() {
    let (_heap, a, b, _) = keys();
    for damage in 0..6 {
        let mut m = MonitorLedger::new(Default::default());
        m.enter_explicit(0, a).unwrap();
        let old = state(&m);
        let mut stage = m.stage(1, b, false).unwrap();
        m.keys.try_reserve(1).unwrap();
        m.frames.try_reserve(1).unwrap();
        let mut fresh = FrameMonitors::default();
        fresh.explicit.try_reserve(1).unwrap();
        match damage {
            0 => stage.new_global += 1,
            1 => stage.new_local += 1,
            2 => stage.new_claims += 1,
            3 => stage.before_frames += 1,
            4 => m.limits.max_active_monitors = 1,
            _ => fresh.explicit = HashMap::new(),
        };
        assert_host(m.commit(stage, Some(fresh)).unwrap_err());
        assert_eq!(state(&m), old);
    }
}
#[test]
fn replacement_updates_never_grow_full_tables_or_invoke_reservation() {
    let (_heap, a, b, c) = keys();
    let mut m = MonitorLedger::new(Default::default());
    for key in [a, b, c] {
        m.enter_explicit(0, key).unwrap();
    }
    m.enter_implicit(1, a).unwrap();
    m.enter_implicit(2, a).unwrap();
    m.keys.shrink_to_fit();
    m.frames.shrink_to_fit();
    m.frames.get_mut(&0).unwrap().explicit.shrink_to_fit();
    let capacities = (
        m.keys.capacity(),
        m.frames.capacity(),
        m.frames[&0].explicit.capacity(),
    );
    m.fail_reserve_after = Some(0);
    m.enter_explicit(0, a).unwrap();
    m.exit_explicit(0, a).unwrap();
    assert_eq!(
        (
            m.keys.capacity(),
            m.frames.capacity(),
            m.frames[&0].explicit.capacity()
        ),
        capacities
    );
    m.finish(2).unwrap();
    m.finish(1).unwrap();
    for key in [a, b, c] {
        m.exit_explicit(0, key).unwrap();
    }
    audit(&m);
}
#[test]
fn checked_counter_overflow_keeps_last_representable_state() {
    let (_heap, a, b, _) = keys();
    let mut m = MonitorLedger::new(MonitorLimits {
        max_steps: u64::MAX,
        max_active_claims: u64::MAX,
        max_recursion: u64::MAX,
        ..Default::default()
    });
    m.steps = u64::MAX;
    assert_eq!(
        host(m.enter_explicit(0, a).unwrap_err()),
        MonitorError::AccountingOverflow { resource: R::Steps }
    );
    assert_eq!(m.steps, u64::MAX);
    m.steps = 0;
    m.enter_explicit(0, a).unwrap();
    m.claims = u64::MAX;
    let old = state(&m);
    assert_eq!(
        host(m.enter_explicit(1, b).unwrap_err()),
        MonitorError::AccountingOverflow {
            resource: R::ActiveClaims
        }
    );
    assert_eq!(state(&m), old);
    m.claims = 1;
    m.keys.insert(a, u64::MAX);
    m.frames.get_mut(&0).unwrap().explicit.insert(a, u64::MAX);
    let old = state(&m);
    assert_eq!(
        host(m.enter_explicit(0, a).unwrap_err()),
        MonitorError::AccountingOverflow {
            resource: R::Recursion
        }
    );
    assert_eq!(state(&m), old);
    assert_eq!(
        host(increment(u64::MAX, 1, u64::MAX, R::ActiveMonitors).unwrap_err()),
        MonitorError::AccountingOverflow {
            resource: R::ActiveMonitors
        }
    );
}
#[test]
fn ten_thousand_nested_receipts_and_sequential_pairs_are_iterative() {
    let (_heap, a, _, _) = keys();
    let n = 10_000;
    let mut m = MonitorLedger::new(MonitorLimits {
        max_steps: 2 * n,
        ..Default::default()
    });
    for depth in 0..n as usize {
        m.check_new_frame(depth).unwrap();
        m.enter_implicit(depth, a).unwrap()
    }
    assert_eq!(m.keys[&a], n);
    assert_eq!(m.claims, n);
    m.fail_reserve_after = Some(0);
    for depth in (0..n as usize).rev() {
        m.finish(depth).unwrap()
    }
    assert!(m.keys.is_empty());
    assert!(m.frames.is_empty());
    assert_eq!(m.steps, 2 * n);
    let mut m = MonitorLedger::new(MonitorLimits {
        max_steps: 2 * n,
        max_active_claims: 1,
        max_active_monitors: 1,
        max_recursion: 1,
    });
    for _ in 0..n {
        m.check_new_frame(0).unwrap();
        m.enter_explicit(0, a).unwrap();
        m.exit_explicit(0, a).unwrap();
        m.finish(0).unwrap()
    }
    assert_eq!(m.steps, 2 * n);
    assert_eq!(m.claims, 0);
}
#[test]
fn max_policy_uses_only_actual_state_and_drop_retains_heap_effects() {
    let (mut heap, a, _, _) = keys();
    let MonitorKey::Object(handle) = a else {
        unreachable!()
    };
    heap.validate_root(handle).unwrap();
    let reference = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut m = MonitorLedger::new(MonitorLimits {
            max_steps: u64::MAX,
            max_active_monitors: u64::MAX,
            max_active_claims: u64::MAX,
            max_recursion: u64::MAX,
        });
        m.enter_explicit(0, a).unwrap();
        heap.store(reference, 0, Value::Int(42)).unwrap();
        m.fail_reserve_after = Some(0);
        panic!("simulated host unwind")
    }));
    assert!(result.is_err());
    assert_eq!(heap.load(reference, 0).unwrap(), Value::Int(42));
    let mut next = MonitorLedger::new(Default::default());
    next.enter_explicit(0, a).unwrap();
    next.exit_explicit(0, a).unwrap();
}
#[test]
fn class_keys_preserve_actual_defining_identity_and_are_separate_from_heap_keys() {
    let (mut heap, a, _, _) = keys();
    let mut registry = DefinitionRegistry::new(Default::default()).unwrap();
    let left = registry.create_loader().unwrap();
    let right = registry.create_loader().unwrap();
    let n: Vec<_> = "java/lang/Object".encode_utf16().collect();
    let c = registry
        .define(left, &n, &object_bytes(), Default::default())
        .unwrap();
    let d = registry
        .define(right, &n, &object_bytes(), Default::default())
        .unwrap();
    assert_ne!(c, d);
    let mut m = MonitorLedger::new(Default::default());
    for (depth, key) in [MonitorKey::Class(c), MonitorKey::Class(d), a]
        .into_iter()
        .enumerate()
    {
        m.enter_implicit(depth, key).unwrap()
    }
    assert_eq!(m.keys.len(), 3);
    for depth in (0..3).rev() {
        m.finish(depth).unwrap()
    }
    let plan = build_hierarchy(&registry, c, &mut SameLoaderResolver, Default::default()).unwrap();
    let mut instances =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    instances
        .admit_instance_layouts(&registry, &[&plan])
        .unwrap();
    let r = instances.allocate_instance(c).unwrap();
    let before = (
        instances.allocated_instances(),
        instances.allocated_arrays(),
    );
    instances.validate_root(r.heap_ref().unwrap()).unwrap();
    assert_eq!(
        (
            instances.allocated_instances(),
            instances.allocated_arrays()
        ),
        before
    );
    assert_eq!(
        heap.validate_root(r.heap_ref().unwrap()),
        Err(HeapError::ForeignHeap)
    );
    let r = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    assert_eq!(
        instances.validate_root(r.heap_ref().unwrap()),
        Err(HeapError::ForeignHeap)
    );
}

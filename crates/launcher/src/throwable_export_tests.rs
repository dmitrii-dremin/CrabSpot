use super::*;
#[path = "../../interpreter/tests/support/throwable_world.rs"]
mod world;
fn program() -> world::Class {
    let mut c = world::app();
    c.method("fault", "(II)I", 9, &[0x1a, 0x1b, 0x6c, 0xac], 2, 2);
    c
}
#[test]
fn hand_derived_head_and_class_prices_and_atomic_copy_phase_rejections() {
    let n = world::ARITHMETIC.encode_utf16().count() as u64;
    let l = 1 + 5 + 5; // C + fault + (II)I
    for ceiling in 0..5 {
        let mut w = world::World::new(program());
        let method = w.method("fault", "(II)I");
        w.with(|s, _| {
            let crabspot_interpreter::ExceptionOutcome::Uncaught(r) = world::run(
                s,
                method,
                &[
                    crabspot_runtime::Value::Int(1),
                    crabspot_runtime::Value::Int(0),
                ],
            )
            .unwrap() else {
                panic!()
            };
            let mut limits = crate::ImageLimits::default();
            match ceiling {
                0 => {}
                1 => limits.max_utf16_units = n + l - 1,
                2 => limits.max_inventory_work = n + l + 14,
                3 => limits.max_scratch_entries = 4,
                _ => limits.max_utf16_units = 0,
            }
            let mut meter = Meter::new(limits);
            let usage = ApplicationUsage {
                vm_exceptions: s.vm_exception_usage(),
                ..Default::default()
            };
            let result = uncaught(s, r, EntryEvaluation::MainEvaluation, &mut meter, usage);
            if ceiling == 0 {
                let ApplicationCompletion::Uncaught {
                    location: Some(head),
                    ..
                } = result.unwrap()
                else {
                    panic!()
                };
                assert_eq!(head.method_name.escaped(), "fault");
                assert_eq!(head.offset, 2);
                assert_eq!(
                    (
                        meter.usage.utf16_units,
                        meter.usage.work_units,
                        meter.usage.scratch_entries
                    ),
                    (n + l, n + l + 15, 5)
                );
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.stage, S::Setup);
                assert_eq!(
                    error.usage.vm_exceptions,
                    crate::VmExceptionUsage {
                        started: 1,
                        produced: 1
                    }
                );
                if ceiling == 4 {
                    assert_eq!(
                        (
                            meter.usage.utf16_units,
                            meter.usage.work_units,
                            meter.usage.scratch_entries
                        ),
                        (0, 4, 0)
                    );
                } else {
                    assert_eq!(
                        (
                            meter.usage.utf16_units,
                            meter.usage.work_units,
                            meter.usage.scratch_entries
                        ),
                        (n, n + 12, 2)
                    );
                }
            }
        });
    }
}
#[test]
fn genuinely_absent_trace_exports_none_without_head_copy_funding() {
    let mut w = world::World::new(program());
    let t = w.throwable;
    let r = w.context().heap_mut().allocate_instance(t).unwrap();
    let n = world::THROWABLE.encode_utf16().count() as u64;
    w.with(|s, _| {
        let mut meter = Meter::new(Default::default());
        let result = uncaught(
            s,
            r,
            EntryEvaluation::MainEvaluation,
            &mut meter,
            Default::default(),
        )
        .unwrap();
        assert!(matches!(
            result,
            ApplicationCompletion::Uncaught { location: None, .. }
        ));
        assert_eq!(
            (
                meter.usage.utf16_units,
                meter.usage.work_units,
                meter.usage.scratch_entries
            ),
            (n, n + 5, 1)
        );
    });
}

use super::super::tests::{Spec, World};
use super::*;
use std::error::Error;

fn totals(v: [u64; 9]) -> ClassTypeVerificationLimits {
    ClassTypeVerificationLimits {
        max_methods: u64::MAX,
        max_code_bytes: v[0],
        max_code_metadata_entries: v[1],
        max_code_utf16_units: v[2],
        max_code_work_units: v[3],
        max_type_symbols: v[4],
        max_state_cells: v[5],
        max_edge_checks: v[6],
        max_type_utf16_units: v[7],
        max_type_work_units: v[8],
    }
}
fn options(v: [u64; 9]) -> TypeVerificationOptions {
    clamp(
        TypeVerificationOptions {
            code: CodeValidationOptions {
                allow_preview: true,
                max_code_bytes: u64::MAX,
                max_metadata_entries: u64::MAX,
                max_utf16_units: u64::MAX,
                max_work_units: u64::MAX,
            },
            max_type_symbols: u64::MAX,
            max_state_cells: u64::MAX,
            max_edge_checks: u64::MAX,
            max_utf16_units: u64::MAX,
            max_work_units: u64::MAX,
        },
        v,
    )
}
fn diagnostic(
    class: ClassId,
    resource: R,
    limit: u64,
    required: u64,
) -> Box<TypeVerificationError> {
    let location = CodeValidationLocation::ExceptionHandler(3);
    let kind = match resource {
        R::Code(resource) => TypeVerificationErrorKind::Code(Box::new(CodeValidationError {
            method_index: 7,
            location,
            usage: CodeValidationUsage::default(),
            kind: CodeValidationErrorKind::ResourceLimit {
                resource,
                limit,
                required,
            },
        })),
        R::Type(resource) => TypeVerificationErrorKind::ResourceLimit {
            resource,
            limit,
            required,
        },
        R::Methods => panic!("no child Methods resource"),
    };
    Box::new(TypeVerificationError {
        class,
        method_index: 7,
        location,
        usage: TypeVerificationUsage::default(),
        kind,
    })
}

#[test]
fn scalar_header_preserves_version_preview_and_history_order() {
    for (major, minor) in [(44, 0), (66, 0), (56, 1), (64, u16::MAX), (65, 1)] {
        assert!(
            matches!(header(major,minor,true),Err(E::UnsupportedVersion{major:m,minor:n}) if m==major&&n==minor)
        );
    }
    for (major, minor) in [(45, 0), (45, u16::MAX), (50, 0), (50, 999)] {
        assert!(
            matches!(header(major,minor,false),Err(E::UnsupportedHistory{major:m}) if m==major)
        );
    }
    for (major, minor) in [
        (51, 0),
        (51, 999),
        (55, u16::MAX),
        (56, 0),
        (64, 0),
        (65, 0),
    ] {
        header(major, minor, false).unwrap();
    }
    assert!(matches!(
        header(65, u16::MAX, false),
        Err(E::PreviewDisabled)
    ));
    header(65, u16::MAX, true).unwrap();
}
#[test]
fn all_nine_cap_mappings_and_remainders_preserve_preview_and_order() {
    let available = [9, 8, 7, 6, 5, 4, 3, 2, 1];
    let prefix = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    assert_eq!(
        remainder(verification(prefix), totals([10; 9])).unwrap(),
        available
    );
    let configured = options([5; 9]);
    let clamped = clamp(configured, available);
    assert!(clamped.code.allow_preview);
    assert_eq!(caps(clamped), [5, 5, 5, 5, 5, 4, 3, 2, 1]);
    for index in 0..9 {
        let mut paid = [0; 9];
        paid[index] = 2;
        assert_eq!(
            remainder(verification(paid), totals([1; 9])).unwrap_err(),
            RESOURCES[index]
        );
    }
    assert_eq!(
        remainder(verification([2; 9]), totals([1; 9])).unwrap_err(),
        RESOURCES[0]
    );
}
#[test]
fn methods_zero_exact_limit_and_overflow_do_not_publish_failed_count() {
    let mut usage = ClassTypeVerificationUsage::default();
    assert!(matches!(
        usage.start(0),
        Err(E::AggregateLimit {
            resource: R::Methods,
            limit: 0,
            required: 1,
            source: None
        })
    ));
    assert_eq!(usage, ClassTypeVerificationUsage::default());
    usage.start(1).unwrap();
    let prior = usage;
    assert!(matches!(
        usage.start(1),
        Err(E::AggregateLimit {
            resource: R::Methods,
            required: 2,
            ..
        })
    ));
    assert_eq!(usage, prior);
    usage.methods_started = u64::MAX;
    let prior = usage;
    assert!(matches!(
        usage.start(u64::MAX),
        Err(E::AccountingOverflow {
            resource: R::Methods,
            source: None
        })
    ));
    assert_eq!(usage, prior);
}
#[test]
fn late_paid_sum_overflow_keeps_all_prior_counters_and_funded_entry() {
    for index in 0..9 {
        let mut paid = [10; 9];
        paid[index] = u64::MAX;
        let mut usage = ClassTypeVerificationUsage {
            methods_started: 2,
            code_bodies: 1,
            bodyless: 0,
            verification: verification(paid),
        };
        let prior = usage;
        assert_eq!(
            usage
                .accept(verification([1; 9]), None, totals([u64::MAX; 9]))
                .unwrap_err(),
            RESOURCES[index]
        );
        assert_eq!(usage, prior);
        assert_eq!(
            usage
                .accept(verification([1; 9]), Some(true), totals([u64::MAX; 9]))
                .unwrap_err(),
            RESOURCES[index]
        );
        assert_eq!(usage, prior);
    }
}
#[test]
fn inconsistent_completed_counts_and_double_none_arithmetic_are_rejected_atomically() {
    for (started, bodies, bodyless) in [
        (0, u64::MAX, 1),
        (2, u64::MAX, 1),
        (1, 1, 0),
        (2, 0, 0),
        (u64::MAX, u64::MAX, 0),
    ] {
        let mut usage = ClassTypeVerificationUsage {
            methods_started: started,
            code_bodies: bodies,
            bodyless,
            verification: TypeVerificationUsage::default(),
        };
        let prior = usage;
        assert_eq!(usage.prior_completed(), Err(R::Methods));
        assert_eq!(
            usage.accept(verification([1; 9]), Some(true), totals([u64::MAX; 9])),
            Err(R::Methods)
        );
        assert_eq!(usage, prior);
    }
}
#[test]
fn completed_and_failed_child_transactions_keep_exact_prefix_semantics() {
    let mut usage = ClassTypeVerificationUsage::default();
    usage.start(3).unwrap();
    usage
        .accept(verification([2; 9]), Some(true), totals([u64::MAX; 9]))
        .unwrap();
    usage.start(3).unwrap();
    usage
        .accept(verification([3; 9]), Some(false), totals([u64::MAX; 9]))
        .unwrap();
    usage.start(3).unwrap();
    usage
        .accept(verification([4; 9]), None, totals([u64::MAX; 9]))
        .unwrap();
    assert_eq!(usage.methods_started, 3);
    assert_eq!(usage.code_bodies, 1);
    assert_eq!(usage.bodyless, 1);
    assert_eq!(counters(usage.verification), [9; 9]);
}
#[test]
fn direct_limit_translation_for_each_resource_preserves_original_source_and_absolute_required() {
    let world = World::new(&[], &[]);
    let configured = ClassTypeVerificationOptions {
        method: options([100; 9]),
        total: totals([50; 9]),
    };
    for resource in RESOURCES {
        let error = diagnostic(world.core.object, resource, 40, 42);
        let pointer = error.as_ref() as *const TypeVerificationError;
        let translated = classify(error, configured, [40; 9], verification([10; 9]));
        assert!(
            matches!(translated,E::AggregateLimit{resource:r,limit:50,required:52,source:Some(_)} if r==resource)
        );
        let nested = translated
            .source()
            .unwrap()
            .downcast_ref::<TypeVerificationError>()
            .unwrap();
        assert_eq!(nested as *const _, pointer);
        assert_eq!(nested.method_index, 7);
        assert_eq!(nested.location, CodeValidationLocation::ExceptionHandler(3));
    }
}
#[test]
fn absolute_required_overflow_keeps_exact_child_and_acceptance_is_independent() {
    let world = World::new(&[], &[]);
    for resource in RESOURCES {
        let error = diagnostic(world.core.object, resource, 1, 2);
        let pointer = error.as_ref() as *const TypeVerificationError;
        let translated = classify(
            error,
            ClassTypeVerificationOptions {
                method: options([2; 9]),
                total: totals([u64::MAX; 9]),
            },
            [1; 9],
            verification([u64::MAX; 9]),
        );
        assert!(
            matches!(translated,E::AccountingOverflow{resource:r,source:Some(_)} if r==resource)
        );
        assert_eq!(
            translated
                .source()
                .unwrap()
                .downcast_ref::<TypeVerificationError>()
                .unwrap() as *const _,
            pointer
        );
    }
}
#[test]
fn equal_tighter_or_mismatched_local_caps_remain_method_errors() {
    let world = World::new(&[], &[]);
    for (per, remaining, reported) in [(40, 40, 40), (30, 40, 30), (100, 40, 39)] {
        for resource in RESOURCES {
            let error = diagnostic(world.core.object, resource, reported, reported + 1);
            let pointer = error.as_ref() as *const TypeVerificationError;
            let kind = classify(
                error,
                ClassTypeVerificationOptions {
                    method: options([per; 9]),
                    total: totals([50; 9]),
                },
                [remaining; 9],
                verification([10; 9]),
            );
            assert!(matches!(kind, E::Method(_)));
            assert_eq!(
                kind.source()
                    .unwrap()
                    .downcast_ref::<TypeVerificationError>()
                    .unwrap() as *const _,
                pointer
            );
        }
    }
}
#[test]
fn nonlimit_and_child_overflow_failures_are_never_reclassified() {
    let world = World::new(&[], &[]);
    let kinds = [
        TypeVerificationErrorKind::TypeRule {
            reason: "independent type failure",
        },
        TypeVerificationErrorKind::AccountingOverflow {
            resource: TypeVerificationResource::WorkUnits,
        },
        TypeVerificationErrorKind::Registry(RegistryError::ResourceLimit {
            resource: crabspot_runtime::RegistryResource::Definitions,
            limit: 0,
            required: 1,
        }),
        TypeVerificationErrorKind::Code(Box::new(CodeValidationError {
            method_index: 7,
            location: CodeValidationLocation::Method,
            usage: CodeValidationUsage::default(),
            kind: CodeValidationErrorKind::AccountingOverflow {
                resource: CodeValidationResource::WorkUnits,
            },
        })),
    ];
    for kind in kinds {
        let error = Box::new(TypeVerificationError {
            class: world.core.object,
            method_index: 7,
            location: CodeValidationLocation::Method,
            usage: TypeVerificationUsage::default(),
            kind,
        });
        assert!(matches!(
            classify(
                error,
                ClassTypeVerificationOptions::default(),
                [0; 9],
                TypeVerificationUsage::default()
            ),
            E::Method(_)
        ));
    }
}
#[test]
fn aggregate_borrows_canonical_environment_and_sums_bodyless_method_usage() {
    let world = World::new(
        &[Spec {
            name: "test/C",
            parent: Some("java/lang/Object"),
            interfaces: &[],
            flags: 0x21,
            fields: &[],
            methods: &[("first", "()V", 0x101), ("second", "()V", 0x109)],
        }],
        &[],
    );
    let env = world.environment();
    let class = world.extra[0];
    let first = verify_method_types(&env, class, 0, TypeVerificationOptions::default())
        .unwrap()
        .usage();
    let second = verify_method_types(&env, class, 1, TypeVerificationOptions::default())
        .unwrap()
        .usage();
    let result = verify_class_types(&env, class, ClassTypeVerificationOptions::default()).unwrap();
    assert!(std::ptr::eq(result.environment(), &env));
    assert!(std::ptr::eq(
        result.class(),
        world.registry.class(class).unwrap()
    ));
    assert_eq!(result.class_id(), class);
    assert_eq!(result.usage().methods_started, 2);
    assert_eq!(result.usage().bodyless, 2);
    assert_eq!(result.usage().code_bodies, 0);
    for ((actual, a), b) in counters(result.usage().verification)
        .into_iter()
        .zip(counters(first))
        .zip(counters(second))
    {
        assert_eq!(actual, a + b);
    }
    let zero = ClassTypeVerificationOptions {
        method: options([0; 9]),
        total: totals([0; 9]),
    };
    assert_eq!(
        verify_class_types(&env, world.core.object, zero)
            .unwrap()
            .usage(),
        ClassTypeVerificationUsage::default()
    );
}
#[test]
fn outer_error_source_is_direct_registry_or_original_method_error() {
    let first = World::new(&[], &[]);
    let second = World::new(&[], &[]);
    let env = first.environment();
    let error = verify_class_types(
        &env,
        second.core.object,
        ClassTypeVerificationOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error.method_index, None);
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<RegistryError>()
            .is_some()
    );
    let nested = diagnostic(first.core.object, RESOURCES[0], 0, 1);
    let pointer = nested.as_ref() as *const TypeVerificationError;
    let error = ClassTypeVerificationError {
        class: first.core.object,
        method_index: Some(7),
        usage: ClassTypeVerificationUsage::default(),
        kind: E::Method(nested),
    };
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<TypeVerificationError>()
            .unwrap() as *const _,
        pointer
    );
}

#[test]
fn representable_over_cap_sums_fail_in_declared_order_before_completed_counts() {
    for first in 0..9 {
        let mut bounds = [10; 9];
        bounds[first] = 1;
        bounds[8] = 1;
        let mut usage = ClassTypeVerificationUsage {
            methods_started: 1,
            verification: verification([1; 9]),
            ..Default::default()
        };
        let prior = usage;
        assert_eq!(
            usage.accept(verification([1; 9]), Some(true), totals(bounds)),
            Err(RESOURCES[first])
        );
        assert_eq!(usage, prior);
    }
    // Even a separate bad completed count cannot hide the earlier Code bound.
    let mut usage = ClassTypeVerificationUsage {
        methods_started: 1,
        bodyless: 7,
        verification: verification([1; 9]),
        ..Default::default()
    };
    let prior = usage;
    assert_eq!(
        usage.accept(verification([1; 9]), None, totals([1; 9])),
        Err(RESOURCES[0])
    );
    assert_eq!(usage, prior);
}
#[test]
fn paid_sum_guard_precedes_limit_translation_and_keeps_real_failure_source() {
    let world = World::new(&[], &[]);
    let mut usage = ClassTypeVerificationUsage {
        methods_started: 2,
        code_bodies: 1,
        verification: verification([1; 9]),
        ..Default::default()
    };
    let prior = usage;
    let mut error = diagnostic(world.core.object, RESOURCES[8], 1, 2);
    error.usage = verification([2; 9]);
    let pointer = error.as_ref() as *const TypeVerificationError;
    let kind = usage.failed(
        error,
        ClassTypeVerificationOptions {
            method: options([3; 9]),
            total: totals([2; 9]),
        },
        [1; 9],
        prior.verification,
    );
    assert!(matches!(
        kind,
        E::AccountingOverflow {
            resource: R::Code(CodeValidationResource::CodeBytes),
            source: Some(_)
        }
    ));
    assert_eq!(usage, prior);
    assert_eq!(
        kind.source()
            .unwrap()
            .downcast_ref::<TypeVerificationError>()
            .unwrap() as *const _,
        pointer
    );
}
#[test]
fn absolute_required_translation_overflow_retains_paid_current_work_for_every_resource() {
    let world = World::new(&[], &[]);
    for (index, resource) in RESOURCES.into_iter().enumerate() {
        let mut prefix = [0; 9];
        prefix[index] = u64::MAX - 1;
        let mut paid = [0; 9];
        paid[index] = 1;
        let mut remaining = [u64::MAX; 9];
        remaining[index] = 1;
        let mut usage = ClassTypeVerificationUsage {
            methods_started: 2,
            code_bodies: 1,
            verification: verification(prefix),
            ..Default::default()
        };
        let mut error = diagnostic(world.core.object, resource, 1, 2);
        error.usage = verification(paid);
        let pointer = error.as_ref() as *const TypeVerificationError;
        let kind = usage.failed(
            error,
            ClassTypeVerificationOptions {
                method: options([2; 9]),
                total: totals([u64::MAX; 9]),
            },
            remaining,
            verification(prefix),
        );
        assert!(matches!(kind,E::AccountingOverflow{resource:r,source:Some(_)} if r==resource));
        assert_eq!(counters(usage.verification)[index], u64::MAX);
        assert_eq!(usage.methods_started, 2);
        assert_eq!(usage.code_bodies, 1);
        assert_eq!(usage.bodyless, 0);
        assert_eq!(
            kind.source()
                .unwrap()
                .downcast_ref::<TypeVerificationError>()
                .unwrap() as *const _,
            pointer
        );
    }
}

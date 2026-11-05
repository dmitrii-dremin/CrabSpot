//! Independent aggregate tests. Existing method fixtures and methods remain unchanged.
pub mod type_support;
use crabspot_classfile::{ParseOptions, StackMapFrameKind as K};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, HierarchyPlan, RegistryError, RegistryOptions,
};
use crabspot_verifier::{
    ClassTypeVerificationError as Error, ClassTypeVerificationErrorKind as E,
    ClassTypeVerificationLimits as Limits, ClassTypeVerificationOptions as Options,
    ClassTypeVerificationResource as R, ClassTypeVerificationUsage as Usage,
    CodeValidationLocation as L, CodeValidationOptions, CodeValidationResource as CR,
    TypeVerificationError as MethodError, TypeVerificationErrorKind as ME,
    TypeVerificationOptions as MethodOptions, TypeVerificationResource as TR,
    TypeVerificationUsage as MethodUsage, VerificationCore, VerificationEnvironment as Environment,
    VerificationEnvironmentOptions, verify_class_types as verify_class,
    verify_method_types as verify_method,
};
use type_support::*;
impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
fn core(w: &World) -> VerificationCore {
    let [
        object,
        cloneable,
        serializable,
        throwable,
        string,
        class,
        method_type,
        method_handle,
    ] = w.core;
    VerificationCore {
        bootstrap_loader: w.loader,
        object,
        cloneable,
        serializable,
        throwable,
        string,
        class,
        method_type,
        method_handle,
    }
}
fn env<'a>(w: &'a World, plans: &'a [HierarchyPlan]) -> Environment<'a> {
    Environment::new(
        &w.registry,
        &plans.iter().collect::<Vec<_>>(),
        &[],
        core(w),
        VerificationEnvironmentOptions::default(),
    )
    .unwrap()
}
fn empty_world() -> World {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let mut ids = Vec::new();
    for (i, name) in CORE_NAMES.iter().enumerate() {
        let mut c = Class::new(name, if i == 0 { None } else { Some(CORE_NAMES[0]) });
        if matches!(i, 1 | 2) {
            c.flags = 0x601;
        }
        ids.push(
            registry
                .define(
                    loader,
                    &name.encode_utf16().collect::<Vec<_>>(),
                    &c.bytes(),
                    ParseOptions::default(),
                )
                .unwrap(),
        );
    }
    World {
        registry,
        loader,
        core: ids.try_into().unwrap(),
        classes: vec![],
    }
}
fn zero_limits() -> Limits {
    Limits {
        max_methods: 0,
        max_code_bytes: 0,
        max_code_metadata_entries: 0,
        max_code_utf16_units: 0,
        max_code_work_units: 0,
        max_type_symbols: 0,
        max_state_cells: 0,
        max_edge_checks: 0,
        max_type_utf16_units: 0,
        max_type_work_units: 0,
    }
}
fn maximum() -> Options {
    Options {
        method: MethodOptions {
            code: CodeValidationOptions {
                max_code_bytes: u64::MAX,
                max_metadata_entries: u64::MAX,
                max_utf16_units: u64::MAX,
                max_work_units: u64::MAX,
                ..Default::default()
            },
            max_type_symbols: u64::MAX,
            max_state_cells: u64::MAX,
            max_edge_checks: u64::MAX,
            max_utf16_units: u64::MAX,
            max_work_units: u64::MAX,
        },
        total: Limits {
            max_methods: u64::MAX,
            max_code_bytes: u64::MAX,
            max_code_metadata_entries: u64::MAX,
            max_code_utf16_units: u64::MAX,
            max_code_work_units: u64::MAX,
            max_type_symbols: u64::MAX,
            max_state_cells: u64::MAX,
            max_edge_checks: u64::MAX,
            max_type_utf16_units: u64::MAX,
            max_type_work_units: u64::MAX,
        },
    }
}
const RESOURCES: [R; 9] = [
    R::Code(CR::CodeBytes),
    R::Code(CR::MetadataEntries),
    R::Code(CR::Utf16Units),
    R::Code(CR::WorkUnits),
    R::Type(TR::TypeSymbols),
    R::Type(TR::StateCells),
    R::Type(TR::EdgeChecks),
    R::Type(TR::Utf16Units),
    R::Type(TR::WorkUnits),
];
fn counters(u: MethodUsage) -> [u64; 9] {
    [
        u.code.code_bytes,
        u.code.metadata_entries,
        u.code.utf16_units,
        u.code.work_units,
        u.type_symbols,
        u.state_cells,
        u.edge_checks,
        u.utf16_units,
        u.work_units,
    ]
}
fn add(a: MethodUsage, b: MethodUsage) -> MethodUsage {
    let a = counters(a);
    let b = counters(b);
    let s = std::array::from_fn::<_, 9, _>(|i| a[i].checked_add(b[i]).unwrap());
    MethodUsage {
        code: crabspot_verifier::CodeValidationUsage {
            code_bytes: s[0],
            metadata_entries: s[1],
            utf16_units: s[2],
            work_units: s[3],
        },
        type_symbols: s[4],
        state_cells: s[5],
        edge_checks: s[6],
        utf16_units: s[7],
        work_units: s[8],
    }
}
fn set_total(o: &mut Options, r: R, limit: u64) {
    match r {
        R::Methods => o.total.max_methods = limit,
        R::Code(CR::CodeBytes) => o.total.max_code_bytes = limit,
        R::Code(CR::MetadataEntries) => o.total.max_code_metadata_entries = limit,
        R::Code(CR::Utf16Units) => o.total.max_code_utf16_units = limit,
        R::Code(CR::WorkUnits) => o.total.max_code_work_units = limit,
        R::Type(TR::TypeSymbols) => o.total.max_type_symbols = limit,
        R::Type(TR::StateCells) => o.total.max_state_cells = limit,
        R::Type(TR::EdgeChecks) => o.total.max_edge_checks = limit,
        R::Type(TR::Utf16Units) => o.total.max_type_utf16_units = limit,
        R::Type(TR::WorkUnits) => o.total.max_type_work_units = limit,
    }
}
fn set_method(o: &mut MethodOptions, r: R, limit: u64) {
    match r {
        R::Methods => panic!("method-count is aggregate only"),
        R::Code(CR::CodeBytes) => o.code.max_code_bytes = limit,
        R::Code(CR::MetadataEntries) => o.code.max_metadata_entries = limit,
        R::Code(CR::Utf16Units) => o.code.max_utf16_units = limit,
        R::Code(CR::WorkUnits) => o.code.max_work_units = limit,
        R::Type(TR::TypeSymbols) => o.max_type_symbols = limit,
        R::Type(TR::StateCells) => o.max_state_cells = limit,
        R::Type(TR::EdgeChecks) => o.max_edge_checks = limit,
        R::Type(TR::Utf16Units) => o.max_utf16_units = limit,
        R::Type(TR::WorkUnits) => o.max_work_units = limit,
    }
}
fn resource(e: &MethodError) -> (R, u64, u64) {
    match &e.kind {
        ME::ResourceLimit {
            resource,
            limit,
            required,
        } => (R::Type(*resource), *limit, *required),
        ME::Code(e) => match e.kind {
            crabspot_verifier::CodeValidationErrorKind::ResourceLimit {
                resource,
                limit,
                required,
            } => (R::Code(resource), limit, required),
            _ => panic!("{e}"),
        },
        _ => panic!("{e}"),
    }
}
fn direct_source(error: &Error, child: &MethodError) {
    let source = std::error::Error::source(error).unwrap();
    assert!(std::ptr::eq(
        source.downcast_ref::<MethodError>().unwrap(),
        child
    ));
}
fn rich_class(w: &mut World, count: usize) -> ClassId {
    let mut c = Class::ordinary("t/Rich");
    for i in 0..count {
        c.method(
            &format!("m{i}"),
            "(I)I",
            9,
            Some(Body::new(&[0x1a, 4, 0x60, 0xac], 2, 1).maps(vec![(0, K::Same)])),
        );
    }
    w.add(c)
}
#[test]
fn defaults_and_zero_usage_are_explicit_host_policies() {
    assert_eq!(Options::default().method, MethodOptions::default());
    let l = Limits::default();
    assert_eq!(
        [
            l.max_methods,
            l.max_code_bytes,
            l.max_code_metadata_entries,
            l.max_code_utf16_units,
            l.max_code_work_units,
            l.max_type_symbols,
            l.max_state_cells,
            l.max_edge_checks,
            l.max_type_utf16_units,
            l.max_type_work_units
        ],
        [
            65535, 8388608, 4194304, 16777216, 67108864, 1048576, 16777216, 16777216, 16777216,
            268435456
        ]
    );
    assert_eq!(Usage::default().verification, MethodUsage::default());
}
#[test]
fn empty_modern_class_interface_and_designated_object_succeed_with_every_limit_zero() {
    let mut w = empty_world();
    let ordinary = w.add(Class::ordinary("t/Empty"));
    let mut c = Class::ordinary("t/Interface");
    c.flags = 0x601;
    let interface = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    for id in [ordinary, interface, w.core[0]] {
        let proof = verify_class(
            &env,
            id,
            Options {
                total: zero_limits(),
                method: MethodOptions {
                    code: CodeValidationOptions {
                        max_code_bytes: 0,
                        max_metadata_entries: 0,
                        max_utf16_units: 0,
                        max_work_units: 0,
                        ..Default::default()
                    },
                    max_type_symbols: 0,
                    max_state_cells: 0,
                    max_edge_checks: 0,
                    max_utf16_units: 0,
                    max_work_units: 0,
                },
            },
        )
        .unwrap();
        assert_eq!(proof.usage(), Usage::default());
        assert!(std::ptr::eq(proof.environment(), &env));
        assert!(std::ptr::eq(proof.class(), w.registry.class(id).unwrap()));
        assert_eq!(proof.class_id(), id);
    }
}
#[test]
fn historical_and_preview_headers_are_checked_even_without_declarations() {
    for historical in [true, false] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/Empty");
        if historical {
            c.major = 50;
        } else {
            c.minor = 65535;
        }
        let id = w.add(c);
        let plans = w.plans();
        let env = env(&w, &plans);
        let e = verify_class(
            &env,
            id,
            Options {
                total: zero_limits(),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(e.method_index, None);
        assert_eq!(e.usage, Usage::default());
        if historical {
            assert!(matches!(e.kind, E::UnsupportedHistory { major: 50 }));
        } else {
            assert!(matches!(e.kind, E::PreviewDisabled));
            let mut options = maximum();
            options.method.code.allow_preview = true;
            assert_eq!(
                verify_class(&env, id, options).unwrap().usage(),
                Usage::default()
            );
        }
    }
}
#[test]
fn identity_and_retention_fail_before_header_or_declaration_budgets() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/Preview");
    c.minor = 65535;
    let id = w.add(c);
    let plans = w.plans_from(&[]);
    let env = env(&w, &plans);
    let e = verify_class(
        &env,
        id,
        Options {
            total: zero_limits(),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(e.kind,E::UnretainedClass{class}if class==id));
    assert_eq!(e.method_index, None);
    assert_eq!(e.usage, Usage::default());
    let foreign = World::new();
    let e = verify_class(&env, foreign.core[0], Options::default()).unwrap_err();
    assert!(matches!(e.kind, E::Registry(_)));
    assert_eq!(e.method_index, None);
    assert_eq!(e.usage, Usage::default());
    assert!(
        std::error::Error::source(e.as_ref())
            .unwrap()
            .downcast_ref::<RegistryError>()
            .is_some()
    );
}
#[test]
fn mixed_code_bodyless_constructor_and_ignored_initializer_flags_sum_exactly() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/Mixed");
    c.flags |= 0x400;
    let super_init = c.pool.member(10, CORE_NAMES[0], "<init>", "()V");
    c.method(
        "<init>",
        "()V",
        1,
        Some(Body::new(
            &[vec![0x2a], op(0xb7, super_init), vec![0xb1]].concat(),
            1,
            1,
        )),
    );
    c.method("<clinit>", "()V", 0x508, Some(Body::new(&[0xb1], 0, 0)));
    c.method("ordinary", "()I", 9, Some(Body::new(&[4, 0xac], 1, 0)));
    c.method("native", "()V", 0x109, None);
    c.method("abstract", "()V", 0x401, None);
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let mut sum = MethodUsage::default();
    for i in 0..5 {
        sum = add(
            sum,
            verify_method(&env, id, i, maximum().method)
                .unwrap()
                .usage(),
        );
    }
    let proof = verify_class(&env, id, maximum()).unwrap();
    assert_eq!(
        proof.usage(),
        Usage {
            methods_started: 5,
            code_bodies: 3,
            bodyless: 2,
            verification: sum
        }
    );
    assert_eq!(
        verify_class(&env, id, maximum()).unwrap().usage(),
        proof.usage()
    );
}
#[test]
fn unused_bad_sibling_rejects_the_class_in_original_declaration_order() {
    for bad_first in [true, false] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/Siblings");
        let good = Body::new(&[4, 0xac], 1, 0);
        let bad = Body::new(&[1, 0xac], 1, 0);
        if bad_first {
            c.method("bad", "()I", 9, Some(bad));
            c.method("good", "()I", 9, Some(good));
        } else {
            c.method("good", "()I", 9, Some(good));
            c.method("bad", "()I", 9, Some(bad));
        }
        let id = w.add(c);
        let plans = w.plans();
        let env = env(&w, &plans);
        let failing = usize::from(!bad_first);
        verify_method(&env, id, 1 - failing, maximum().method).unwrap();
        let child = verify_method(&env, id, failing, maximum().method).unwrap_err();
        let e = verify_class(&env, id, maximum()).unwrap_err();
        assert_eq!(e.method_index, Some(failing));
        assert_eq!(e.usage.methods_started, (failing + 1) as u64);
        assert_eq!(e.usage.code_bodies, failing as u64);
        assert_eq!(e.usage.bodyless, 0);
        let E::Method(source) = &e.kind else {
            panic!("{e}")
        };
        assert!(matches!(source.kind, ME::TypeRule { .. }));
        assert_eq!(source.location, L::Instruction(1));
        assert_eq!(source.usage, child.usage);
        direct_source(&e, source);
        let mut sum = child.usage;
        if !bad_first {
            sum = add(
                verify_method(&env, id, 0, maximum().method)
                    .unwrap()
                    .usage(),
                sum,
            );
        }
        assert_eq!(e.usage.verification, sum);
    }
}
#[test]
fn bodyless_success_before_bad_code_counts_only_completed_declarations() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.method("native", "()V", 0x109, None);
    c.method("bad", "()I", 9, Some(Body::new(&[0x1a, 0xac], 1, 1)));
    c.method("later", "()V", 9, Some(Body::new(&[0xff], 0, 0)));
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let e = verify_class(&env, id, maximum()).unwrap_err();
    assert_eq!(e.method_index, Some(1));
    assert_eq!(
        (
            e.usage.methods_started,
            e.usage.code_bodies,
            e.usage.bodyless
        ),
        (2, 0, 1)
    );
    assert!(
        matches!(e.kind,E::Method(ref e)if matches!(e.kind,ME::TypeRule{..})&&e.location==L::Instruction(0))
    );
}
#[test]
fn bodyless_final_override_is_a_method_failure_not_an_empty_code_success() {
    let mut w = World::new();
    let mut base = Class::ordinary("p/Base");
    base.method("m", "()V", 0x111, None);
    let ancestor = w.add(base);
    let mut c = Class::new("q/Child", Some("p/Base"));
    c.method("m", "()V", 0x101, None);
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let e = verify_class(&env, id, maximum()).unwrap_err();
    assert_eq!(e.method_index, Some(0));
    assert_eq!(
        (
            e.usage.methods_started,
            e.usage.code_bodies,
            e.usage.bodyless
        ),
        (1, 0, 0)
    );
    assert!(
        matches!(e.kind,E::Method(ref e)if matches!(e.kind,ME::FinalOverride{ancestor:a,..}if a==ancestor))
    );
}
#[test]
fn selected_child_proof_does_not_certify_or_visit_an_ill_typed_parent_body() {
    let mut w = World::new();
    let mut parent = Class::ordinary("t/BadParent");
    parent.method("bad", "()I", 9, Some(Body::new(&[1, 0xac], 1, 0)));
    let parent = w.add(parent);
    let mut child = Class::new("t/Child", Some("t/BadParent"));
    child.method("good", "()I", 9, Some(Body::new(&[4, 0xac], 1, 0)));
    let child = w.add(child);
    let plans = w.plans();
    let env = env(&w, &plans);
    let proof = verify_class(&env, child, maximum()).unwrap();
    assert_eq!(proof.usage().methods_started, 1);
    let e = verify_class(&env, parent, maximum()).unwrap_err();
    assert!(matches!(e.kind,E::Method(ref e)if matches!(e.kind,ME::TypeRule{..})));
    assert_eq!(
        verify_class(&env, child, maximum()).unwrap().usage(),
        proof.usage()
    );
}
#[test]
fn class_header_history_precedes_bad_body_but_method_api_keeps_its_old_representation_order() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/Historical");
    c.major = 50;
    c.method("bad", "()V", 9, Some(Body::new(&[0xff], 0, 0)));
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    assert!(matches!(
        verify_method(&env, id, 0, maximum().method)
            .unwrap_err()
            .kind,
        ME::Code(_)
    ));
    let e = verify_class(&env, id, maximum()).unwrap_err();
    assert!(matches!(e.kind, E::UnsupportedHistory { major: 50 }));
    assert_eq!(e.method_index, None);
    assert_eq!(e.usage, Usage::default());
}
#[test]
fn representation_decode_errors_remain_nested_method_failures_with_paid_work() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.method("good", "()V", 9, Some(Body::new(&[0xb1], 0, 0)));
    c.method("bad", "()V", 9, Some(Body::new(&[0xff], 0, 0)));
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let e = verify_class(&env, id, maximum()).unwrap_err();
    assert_eq!(e.method_index, Some(1));
    assert_eq!(
        (
            e.usage.methods_started,
            e.usage.code_bodies,
            e.usage.bodyless
        ),
        (2, 1, 0)
    );
    let E::Method(child) = &e.kind else {
        panic!("{e}")
    };
    assert!(
        matches!(child.kind,ME::Code(ref e)if matches!(e.kind,crabspot_verifier::CodeValidationErrorKind::Decode(_)))
    );
    assert_eq!(child.location, L::Instruction(0));
    direct_source(&e, child);
    assert_eq!(
        e.usage.verification,
        add(
            verify_method(&env, id, 0, maximum().method)
                .unwrap()
                .usage(),
            child.usage
        )
    );
}
#[test]
fn module_native_with_body_and_invalid_versions_are_rejected_at_actual_registry_admission() {
    let mut w = World::new();
    let before = w.registry.definition_count();
    let error = w
        .registry
        .define(
            w.loader,
            &"module-info".encode_utf16().collect::<Vec<_>>(),
            include_bytes!("../../../tests/fixtures/classes/module-info.class"),
            ParseOptions::default(),
        )
        .unwrap_err();
    assert!(matches!(error, RegistryError::UnsupportedDefinition(_)));
    let mut native = Class::ordinary("t/NativeBody");
    native.method("m", "()V", 0x109, Some(Body::new(&[0xb1], 0, 0)));
    let error = w
        .registry
        .define(
            w.loader,
            &native.name.encode_utf16().collect::<Vec<_>>(),
            &native.bytes(),
            ParseOptions::default(),
        )
        .unwrap_err();
    assert!(matches!(error, RegistryError::Parse(_)));
    let mut version = Class::ordinary("t/Version");
    version.major = 66;
    let name = version.name.encode_utf16().collect::<Vec<_>>();
    assert!(matches!(
        w.registry
            .define(w.loader, &name, &version.bytes(), ParseOptions::default())
            .unwrap_err(),
        RegistryError::Parse(_)
    ));
    assert_eq!(w.registry.definition_count(), before);
}
#[test]
fn method_limit_stops_before_entry_and_counts_only_completed_prefix() {
    let mut w = World::new();
    let id = rich_class(&mut w, 2);
    let plans = w.plans();
    let env = env(&w, &plans);
    let first = verify_method(&env, id, 0, maximum().method)
        .unwrap()
        .usage();
    for limit in [0, 1] {
        let mut o = maximum();
        o.total.max_methods = limit;
        let e = verify_class(&env, id, o).unwrap_err();
        assert_eq!(e.method_index, Some(limit as usize));
        assert_eq!(
            (
                e.usage.methods_started,
                e.usage.code_bodies,
                e.usage.bodyless
            ),
            (limit, limit, 0)
        );
        assert_eq!(
            e.usage.verification,
            if limit == 0 {
                MethodUsage::default()
            } else {
                first
            }
        );
        assert!(
            matches!(e.kind,E::AggregateLimit{resource:R::Methods,limit:l,required,source:None}if l==limit&&required==limit+1)
        );
        assert!(std::error::Error::source(e.as_ref()).is_none());
    }
    let mut o = maximum();
    o.total.max_methods = 2;
    assert_eq!(
        verify_class(&env, id, o).unwrap().usage().methods_started,
        2
    );
}
#[test]
fn all_nine_aggregate_limits_preserve_paid_current_work_and_absolute_required_amounts() {
    let mut w = World::new();
    let id = rich_class(&mut w, 2);
    let plans = w.plans();
    let env = env(&w, &plans);
    let first = verify_method(&env, id, 0, maximum().method)
        .unwrap()
        .usage();
    let second = verify_method(&env, id, 1, maximum().method)
        .unwrap()
        .usage();
    let sum = add(first, second);
    for (i, r) in RESOURCES.into_iter().enumerate() {
        assert!(counters(first)[i] > 0 && counters(second)[i] > 0);
        let cap = counters(sum)[i] - 1;
        let mut o = maximum();
        set_total(&mut o, r, cap);
        let e = verify_class(&env, id, o).unwrap_err();
        assert_eq!(e.method_index, Some(1), "{r:?}: {e}");
        assert_eq!(
            (
                e.usage.methods_started,
                e.usage.code_bodies,
                e.usage.bodyless
            ),
            (2, 1, 0)
        );
        let E::AggregateLimit {
            resource: actual,
            limit,
            required,
            source: Some(child),
        } = &e.kind
        else {
            panic!("{r:?} {e}")
        };
        assert_eq!(*actual, r);
        assert_eq!(*limit, cap);
        assert_eq!(*required, counters(sum)[i]);
        let (child_r, child_l, child_q) = resource(child);
        assert_eq!(child_r, r);
        assert_eq!(child_l, cap - counters(first)[i]);
        assert_eq!(child_q, counters(second)[i]);
        assert_eq!(e.usage.verification, add(first, child.usage));
        assert!(counters(child.usage)[i] <= child_l);
        direct_source(&e, child);
        let mut retry = maximum();
        set_total(&mut retry, r, counters(sum)[i]);
        assert_eq!(
            verify_class(&env, id, retry).unwrap().usage().verification,
            sum
        );
        assert_eq!(verify_class(&env, id, o).unwrap_err().usage, e.usage);
    }
}
#[test]
fn ties_and_tighter_per_method_caps_remain_method_errors_for_every_resource() {
    let mut w = World::new();
    let id = rich_class(&mut w, 1);
    let plans = w.plans();
    let env = env(&w, &plans);
    let full = verify_method(&env, id, 0, maximum().method)
        .unwrap()
        .usage();
    for (i, r) in RESOURCES.into_iter().enumerate() {
        let cap = counters(full)[i] - 1;
        for tie in [false, true] {
            let mut o = maximum();
            set_method(&mut o.method, r, cap);
            if tie {
                set_total(&mut o, r, cap);
            }
            let expected = verify_method(&env, id, 0, o.method).unwrap_err();
            let e = verify_class(&env, id, o).unwrap_err();
            let E::Method(child) = &e.kind else {
                panic!("{r:?}/{tie}: {e}")
            };
            assert_eq!(e.method_index, Some(0));
            assert_eq!(
                (
                    e.usage.methods_started,
                    e.usage.code_bodies,
                    e.usage.bodyless
                ),
                (1, 0, 0)
            );
            assert_eq!(resource(child), resource(&expected));
            assert_eq!(child.location, expected.location);
            assert_eq!(child.usage, expected.usage);
            assert_eq!(e.usage.verification, expected.usage);
            direct_source(&e, child);
        }
    }
}
#[test]
fn simultaneous_code_byte_and_work_limit_failure_is_atomic_and_keeps_child_precedence() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    for name in ["run", "foo"] {
        c.method(name, "()V", 9, Some(Body::new(&[0xb1], 0, 0)));
    }
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let first = verify_method(&env, id, 0, maximum().method)
        .unwrap()
        .usage();
    assert_eq!(
        first.code,
        crabspot_verifier::CodeValidationUsage {
            code_bytes: 1,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 16
        }
    );
    let mut o = maximum();
    o.total.max_code_bytes = 1;
    o.total.max_code_work_units = 30;
    let e = verify_class(&env, id, o).unwrap_err();
    let E::AggregateLimit {
        resource: R::Code(CR::CodeBytes),
        limit: 1,
        required: 2,
        source: Some(child),
    } = &e.kind
    else {
        panic!("{e}")
    };
    assert_eq!(resource(child), (R::Code(CR::CodeBytes), 0, 1));
    assert_eq!(
        child.usage.code,
        crabspot_verifier::CodeValidationUsage {
            code_bytes: 0,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 14
        }
    );
    assert_eq!(child.usage.work_units, 0);
    assert_eq!(e.usage.verification, add(first, child.usage));
    assert_eq!(e.usage.verification.code.work_units, 30);
    assert_eq!(e.usage.verification.code.code_bytes, 1);
    assert_eq!((e.usage.methods_started, e.usage.code_bodies), (2, 1));
}
#[test]
fn all_zero_declared_method_ties_are_method_failures_but_methods_zero_wins_before_checker() {
    let mut w = World::new();
    let id = rich_class(&mut w, 1);
    let plans = w.plans();
    let env = env(&w, &plans);
    let zero_method = MethodOptions {
        code: CodeValidationOptions {
            max_code_bytes: 0,
            max_metadata_entries: 0,
            max_utf16_units: 0,
            max_work_units: 0,
            ..Default::default()
        },
        max_type_symbols: 0,
        max_state_cells: 0,
        max_edge_checks: 0,
        max_utf16_units: 0,
        max_work_units: 0,
    };
    let mut o = Options {
        method: zero_method,
        total: zero_limits(),
    };
    let e = verify_class(&env, id, o).unwrap_err();
    assert!(matches!(
        e.kind,
        E::AggregateLimit {
            resource: R::Methods,
            source: None,
            ..
        }
    ));
    assert_eq!(e.usage, Usage::default());
    o.total.max_methods = 1;
    let e = verify_class(&env, id, o).unwrap_err();
    assert!(matches!(e.kind, E::Method(_)));
    assert_eq!(e.usage.methods_started, 1);
    assert_eq!(e.usage.verification, MethodUsage::default());
}
#[test]
fn generous_class_usage_equals_sum_of_method_usages_for_each_resource_and_maximum_policies() {
    let mut w = World::new();
    let id = rich_class(&mut w, 4);
    let plans = w.plans();
    let env = env(&w, &plans);
    let mut sum = MethodUsage::default();
    for i in 0..4 {
        sum = add(
            sum,
            verify_method(&env, id, i, maximum().method)
                .unwrap()
                .usage(),
        );
    }
    let mut exact = maximum();
    exact.total.max_methods = 4;
    for (i, r) in RESOURCES.into_iter().enumerate() {
        set_total(&mut exact, r, counters(sum)[i]);
    }
    for options in [maximum(), exact] {
        let proof = verify_class(&env, id, options).unwrap();
        assert_eq!(
            proof.usage(),
            Usage {
                methods_started: 4,
                code_bodies: 4,
                bodyless: 0,
                verification: sum
            }
        );
    }
}
#[test]
fn many_tiny_methods_stop_at_known_declaration_without_returning_partial_class_evidence() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/Many");
    for i in 0..1000 {
        c.method(&format!("m{i}"), "()V", 9, Some(Body::new(&[0xb1], 0, 0)));
    }
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let mut o = maximum();
    o.total.max_code_bytes = 5;
    let e = verify_class(&env, id, o).unwrap_err();
    assert_eq!(e.method_index, Some(5));
    assert_eq!(
        (
            e.usage.methods_started,
            e.usage.code_bodies,
            e.usage.bodyless
        ),
        (6, 5, 0)
    );
    assert_eq!(e.usage.verification.code.code_bytes, 5);
    assert!(matches!(
        e.kind,
        E::AggregateLimit {
            resource: R::Code(CR::CodeBytes),
            limit: 5,
            required: 6,
            ..
        }
    ));
    o.total.max_methods = 5;
    let e = verify_class(&env, id, o).unwrap_err();
    assert_eq!(e.method_index, Some(5));
    assert_eq!(e.usage.methods_started, 5);
    assert!(matches!(
        e.kind,
        E::AggregateLimit {
            resource: R::Methods,
            limit: 5,
            required: 6,
            source: None
        }
    ));
}
#[test]
fn bodyless_declarations_need_no_code_bytes_or_attribute_quota_but_still_start_methods() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/Bodyless");
    c.flags |= 0x400;
    c.method("native", "()V", 0x109, None);
    c.method("abstract", "()V", 0x401, None);
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let mut o = maximum();
    o.total.max_methods = 2;
    o.total.max_code_bytes = 0;
    o.total.max_code_metadata_entries = 0;
    let u = verify_class(&env, id, o).unwrap().usage();
    assert_eq!((u.methods_started, u.code_bodies, u.bodyless), (2, 0, 2));
    assert_eq!(u.verification.code.code_bytes, 0);
    assert_eq!(u.verification.code.metadata_entries, 0);
    o.total.max_methods = 1;
    let e = verify_class(&env, id, o).unwrap_err();
    assert_eq!(
        (
            e.usage.methods_started,
            e.usage.code_bodies,
            e.usage.bodyless
        ),
        (1, 0, 1)
    );
    assert_eq!(e.method_index, Some(1));
    assert!(matches!(
        e.kind,
        E::AggregateLimit {
            resource: R::Methods,
            source: None,
            ..
        }
    ));
}
#[test]
fn simultaneous_type_utf16_and_work_charges_keep_existing_child_order_and_atomicity() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    for name in ["run", "foo"] {
        c.method(name, "()V", 9, Some(Body::new(&[0xb1], 0, 0)));
    }
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let first = verify_method(&env, id, 0, maximum().method)
        .unwrap()
        .usage();
    let mut probe = maximum().method;
    probe.max_utf16_units = 2;
    let child = verify_method(&env, id, 1, probe).unwrap_err();
    assert_eq!(resource(&child), (R::Type(TR::Utf16Units), 2, 3));
    assert_eq!(child.usage.utf16_units, 0);
    let paid = child.usage.work_units;
    let mut o = maximum();
    o.total.max_type_utf16_units = first.utf16_units + 2;
    o.total.max_type_work_units = first.work_units + paid + 2;
    let e = verify_class(&env, id, o).unwrap_err();
    let E::AggregateLimit {
        resource: R::Type(TR::Utf16Units),
        limit,
        required,
        source: Some(source),
    } = &e.kind
    else {
        panic!("{e}")
    };
    assert_eq!(
        (*limit, *required),
        (first.utf16_units + 2, first.utf16_units + 3)
    );
    assert_eq!(source.usage, child.usage);
    assert_eq!(e.usage.verification, add(first, child.usage));
    assert_eq!(e.usage.verification.utf16_units, first.utf16_units);
    assert_eq!(e.usage.verification.work_units, first.work_units + paid);
    direct_source(&e, source);
    o.total.max_type_utf16_units += 1;
    let e = verify_class(&env, id, o).unwrap_err();
    let E::AggregateLimit {
        resource: R::Type(TR::WorkUnits),
        limit,
        required,
        source: Some(source),
    } = &e.kind
    else {
        panic!("{e}")
    };
    assert_eq!(
        (*limit, *required),
        (first.work_units + paid + 2, first.work_units + paid + 3)
    );
    assert_eq!(source.usage.utf16_units, 0);
    assert_eq!(source.usage.work_units, paid);
    assert_eq!(e.usage.verification.utf16_units, first.utf16_units);
}
#[test]
fn preview_empty_class_succeeds_with_all_zero_caps_when_the_only_switch_is_enabled() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/PreviewEmpty");
    c.minor = 65535;
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let method = MethodOptions {
        code: CodeValidationOptions {
            allow_preview: true,
            max_code_bytes: 0,
            max_metadata_entries: 0,
            max_utf16_units: 0,
            max_work_units: 0,
        },
        max_type_symbols: 0,
        max_state_cells: 0,
        max_edge_checks: 0,
        max_utf16_units: 0,
        max_work_units: 0,
    };
    let proof = verify_class(
        &env,
        id,
        Options {
            method,
            total: zero_limits(),
        },
    )
    .unwrap();
    assert_eq!(proof.usage(), Usage::default());
}
#[test]
fn final_parent_is_rejected_by_hierarchy_before_an_environment_can_authorize_the_child() {
    use crabspot_runtime::{HierarchyError, HierarchyOptions, SameLoaderResolver, build_hierarchy};
    let mut w = World::new();
    let mut c = Class::ordinary("t/FinalParent");
    c.flags |= 0x10;
    let parent = w.add(c);
    let child = w.add(Class::new("t/Child", Some("t/FinalParent")));
    let e = build_hierarchy(
        &w.registry,
        child,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(e,HierarchyError::FinalSuperclass{class,superclass}if class==child&&superclass==parent)
    );
}
#[test]
fn declaration_budget_prevents_even_decoding_an_invalid_later_sibling() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/Stop");
    c.method("good", "()V", 9, Some(Body::new(&[0xb1], 0, 0)));
    c.method("bad", "()V", 9, Some(Body::new(&[0xff], 0, 0)));
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let mut o = maximum();
    o.total.max_methods = 1;
    let e = verify_class(&env, id, o).unwrap_err();
    assert!(matches!(
        e.kind,
        E::AggregateLimit {
            resource: R::Methods,
            source: None,
            ..
        }
    ));
    assert_eq!(e.method_index, Some(1));
    assert_eq!(
        e.usage.verification,
        verify_method(&env, id, 0, maximum().method)
            .unwrap()
            .usage()
    );
    assert_eq!((e.usage.methods_started, e.usage.code_bodies), (1, 1));
}

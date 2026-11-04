//! Runtime initialization plans certify checked operations, not completion.
mod rt {
    pub use crabspot_runtime::*;
}
mod fixtures {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/initialization.rs"
    ));
}
use InitializationAccessError as E;
use InitializationStaticKind as K;
use fixtures::*;
use rt::*;
use std::{cell::Cell, collections::HashMap, rc::Rc};

#[derive(Default)]
struct Resolver {
    aliases: HashMap<(LoaderId, Vec<u16>), ClassId>,
    calls: Rc<Cell<usize>>,
}
impl HierarchyResolver for Resolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        self.calls.set(self.calls.get() + 1);
        if let Some(class) = self.aliases.get(&(loader, name.to_vec())) {
            Ok(Some(*class))
        } else {
            registry
                .lookup_class(loader, name)
                .map_err(|error| ResolveError::new(error.to_string()))
        }
    }
}

#[test]
fn claim_anchor_cost_permanence_and_rechecks_are_exact_without_lifecycle_state() {
    let w = world();
    let mut heap = heap(&w, &[w.error], &mut SameLoaderResolver);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    {
        let before = ctx.lookup_steps();
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        assert_eq!(ctx.lookup_steps() - before, 17);
        let before = ctx.lookup_steps();
        ctx.check_initialization_claim(&claim).unwrap();
        assert_eq!(ctx.lookup_steps(), before);
        assert!(matches!(
            ctx.claim_initialization(&s, &e, w.u.object),
            Err(E::AlreadyClaimed)
        ));
        assert_eq!(ctx.lookup_steps(), before);
    }
    assert!(matches!(
        ctx.claim_initialization(&s, &e, w.error),
        Err(E::AlreadyClaimed)
    ));
    drop(ctx);
    let mut ctx = context(&w, &mut heap, &mut resolver);
    assert!(matches!(
        ctx.claim_initialization(&s, &e, w.error),
        Err(E::AlreadyClaimed)
    ));
    assert_eq!(calls.get(), 0);
}

#[test]
fn all_numeric_constants_include_nonfinal_and_ignore_instance_attributes() {
    let mut w = world();
    let values = [
        ("z", "Z", Cv::Int(2), Value::Int(0)),
        ("b", "B", Cv::Int(128), Value::Int(-128)),
        ("c", "C", Cv::Int(-1), Value::Int(65535)),
        ("s", "S", Cv::Int(32768), Value::Int(-32768)),
        ("i", "I", Cv::Int(i32::MIN), Value::Int(i32::MIN)),
        ("j", "J", Cv::Long(i64::MAX), Value::Long(i64::MAX)),
        ("f", "F", Cv::Float(0xff800123), Value::Float(0xff800123)),
        (
            "d",
            "D",
            Cv::Double(0x8000000000000000),
            Value::Double(0x8000000000000000),
        ),
    ];
    let mut fields: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, (name, desc, value, _))| {
            Field::new(name, desc, if i % 2 == 0 { 9 } else { 25 }, Some(*value))
        })
        .collect();
    fields.push(Field::new(
        "instance",
        "Ljava/lang/String;",
        1,
        Some(Cv::String("ignored")),
    ));
    fields.push(Field::new("plain", "I", 9, None));
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Constants",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &[Method::new("<clinit>", "()V", 8)],
        &[],
        65,
    );
    let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let before = ctx.lookup_steps();
    let plan = ctx
        .plan_initialization(&claim, own.class, limits())
        .unwrap();
    assert_eq!(plan.class(), own.class);
    assert_eq!(plan.initializer(), Some(own.methods[0]));
    assert_eq!(plan.constant_count(), 8);
    assert_eq!(plan.dependencies(), [w.u.object]);
    assert_eq!(ctx.lookup_steps() - before, 1 + 10 + 9 + 8 + 1 + 1);
    for (name, desc, _, _) in values {
        assert!(matches!(
            ctx.inspect_static(&s, field(&w, own.class, name, desc)),
            Ok(Value::Int(0) | Value::Long(0) | Value::Float(0) | Value::Double(0))
        ));
    }
    let before = ctx.lookup_steps();
    ctx.install_initialization_constants(&claim, &plan).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 9);
    for (name, desc, _, value) in values {
        assert_eq!(
            ctx.inspect_static(&s, field(&w, own.class, name, desc)),
            Ok(value)
        );
    }
    assert_eq!(
        ctx.inspect_static(&s, field(&w, own.class, "plain", "I")),
        Ok(Value::Int(0))
    );
    assert_eq!(
        (
            ctx.heap().allocated_arrays(),
            ctx.heap().allocated_instances()
        ),
        (0, 0)
    );
}

#[test]
fn string_constant_rejects_whole_plan_before_any_numeric_installation() {
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("first", "I", 9, Some(Cv::Int(93))),
            Field::new(
                "text",
                "Ljava/lang/String;",
                25,
                Some(Cv::String("unsupported")),
            ),
        ],
        &[],
        &[],
        65,
    );
    let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let text = field(&w, own.class, "text", "Ljava/lang/String;");
    assert!(
        matches!(ctx.plan_initialization(&claim,own.class,limits()),Err(E::UnsupportedStringConstant {field}) if field==text)
    );
    assert_eq!(
        ctx.inspect_static(&s, field(&w, own.class, "first", "I")),
        Ok(Value::Int(0))
    );
    assert_eq!(ctx.static_roots(&s).unwrap().count(), 0);
}

#[test]
fn static_tokens_preserve_declared_owner_and_final_authority_without_selector_replay() {
    let mut w = world();
    let base = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("final", "I", 25, None),
            Field::new("value", "I", 9, None),
        ],
        &[
            Method::new("<clinit>", "()V", 8),
            Method::new("helper", "()V", 9),
        ],
        &[("Base", "final", "I"), ("Base", "value", "I")],
        65,
    );
    let child = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[],
        &[Method::new("<clinit>", "()V", 8)],
        &[("Child", "value", "I"), ("Child", "final", "I")],
        65,
    );
    let mut heap = heap(&w, &[w.error, child.class], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let access = ctx
        .resolve_initialization_static(&claim, child.methods[0], child.refs[0], K::Write)
        .unwrap();
    assert_eq!(access.field().owner(), base.class);
    assert_eq!(access.kind(), K::Write);
    let before = ctx.lookup_steps();
    ctx.write_initialization_static(&claim, &access, Value::Int(41))
        .unwrap();
    assert_eq!(ctx.lookup_steps(), before);
    assert!(matches!(
        ctx.read_initialization_static(&claim, &access),
        Err(E::InvalidMetadata(_))
    ));
    let final_access = ctx
        .resolve_initialization_static(&claim, base.methods[0], base.refs[0], K::Write)
        .unwrap();
    ctx.write_initialization_static(&claim, &final_access, Value::Int(79))
        .unwrap();
    assert_eq!(
        ctx.inspect_static(&s, field(&w, base.class, "final", "I")),
        Ok(Value::Int(79))
    );
    assert_eq!(
        ctx.write_static(&s, base.class, base.refs[0], Value::Int(11)),
        Err(StaticAccessError::Java(StaticJavaError::IllegalAccess))
    );
    for (method, index) in [
        (base.methods[1], base.refs[0]),
        (child.methods[0], child.refs[1]),
    ] {
        assert!(matches!(
            ctx.resolve_initialization_static(&claim, method, index, K::Write),
            Err(E::Java(StaticJavaError::IllegalAccess))
        ));
    }
    let read = ctx
        .resolve_initialization_static(&claim, child.methods[0], child.refs[0], K::Read)
        .unwrap();
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.read_initialization_static(&claim, &read).unwrap(),
        Value::Int(41)
    );
    assert_eq!(ctx.lookup_steps(), before);
}

#[test]
fn error_anchor_roles_root_name_loader_and_failed_claims_are_checked_before_publication() {
    for (name, parent, flags, work) in [
        ("Wrong", Some("java/lang/Throwable"), 0x21, 6),
        ("java/lang/Error", Some("java/lang/Throwable"), 0x20, 16),
        ("java/lang/Error", Some("java/lang/Throwable"), 0x421, 16),
        ("java/lang/Error", Some("java/lang/Object"), 0x601, 16),
        ("java/lang/Error", Some("java/lang/Object"), 0x21, 17),
    ] {
        let w = world_with_error(name, parent, flags);
        let mut heap = heap(&w, &[w.error], &mut SameLoaderResolver);
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        for _ in 0..2 {
            let before = ctx.lookup_steps();
            assert!(
                matches!(ctx.claim_initialization(&s,&e,w.error),Err(E::InvalidErrorAnchor {class,..}) if class==w.error)
            );
            assert_eq!(ctx.lookup_steps() - before, work);
        }
    }
    let mut w = world();
    let l = w.registry.create_loader().unwrap();
    let wrong = define(
        &mut w.registry,
        l,
        "java/lang/Error",
        Some("java/lang/Throwable"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        65,
    )
    .class;
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((l, units("java/lang/Throwable")), w.throwable);
    let mut heap = heap(&w, &[w.error, wrong], &mut resolver);
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let before = ctx.lookup_steps();
    assert!(matches!(
        ctx.claim_initialization(&s, &e, wrong),
        Err(E::InvalidErrorAnchor { .. })
    ));
    assert_eq!(ctx.lookup_steps() - before, 16);
    ctx.claim_initialization(&s, &e, w.error).unwrap();
}

#[test]
fn claim_checks_static_then_exception_then_claim_then_anchor_and_ownership() {
    let w = world();
    let mut first = heap(&w, &[w.error], &mut SameLoaderResolver);
    let mut other = heap(&w, &[w.error], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let (os, oe) = {
        let mut ctx = context(&w, &mut other, &mut resolver);
        profiles(&mut ctx, w.throwable)
    };
    let mut ctx = context(&w, &mut first, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    assert!(
        matches!(ctx.claim_initialization(&os,&e,w.u.object),Err(E::Static(error)) if *error==StaticAccessError::ProfileMismatch)
    );
    assert!(
        matches!(ctx.claim_initialization(&s,&oe,w.u.object),Err(E::Exception(error)) if *error==ExceptionAccessError::ProfileMismatch)
    );
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    assert!(matches!(
        ctx.claim_initialization(&s, &oe, w.error),
        Err(E::Exception(_))
    ));
    assert!(matches!(
        ctx.claim_initialization(&s, &e, w.u.object),
        Err(E::AlreadyClaimed)
    ));
    drop(ctx);
    let mut ctx = context(&w, &mut other, &mut resolver);
    assert_eq!(
        ctx.check_initialization_claim(&claim),
        Err(E::ClaimMismatch)
    );
    let second_claim = ctx.claim_initialization(&os, &oe, w.error).unwrap();
    assert_eq!(ctx.check_initialization_claim(&second_claim), Ok(()));
    let old = std::mem::replace(ctx.heap_mut(), first);
    assert_eq!(
        old.retained_layout_classes(),
        ctx.heap().retained_layout_classes()
    );
    assert!(matches!(
        ctx.check_initialization_claim(&second_claim),
        Err(E::Context(FieldAccessError::ContextChanged))
    ));
    drop(ctx);
    let mut ctx = context(&w, &mut other, &mut resolver);
    ctx.check_initialization_claim(&claim).unwrap();
}

#[test]
fn anchor_claim_work_limit_is_inclusive_and_failed_anchor_does_not_claim_the_heap() {
    let w = world();
    let mut heap = heap(&w, &[w.error], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let (s, e) = {
        let mut ctx = context(&w, &mut heap, &mut resolver);
        profiles(&mut ctx, w.throwable)
    };
    let seed = context(&w, &mut heap, &mut resolver).lookup_steps();
    for extra in [0, 1, 15, 16] {
        let limit = seed + extra;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &w.registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        assert!(
            matches!(ctx.claim_initialization(&s,&e,w.error),Err(E::Context(FieldAccessError::ResourceLimit {limit:l,required,..})) if l==limit&&required==limit+1)
        );
        assert_eq!(ctx.lookup_steps(), limit);
    }
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &w.registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: seed + 17,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    ctx.claim_initialization(&s, &e, w.error).unwrap();
    assert_eq!(ctx.lookup_steps(), seed + 17);
}

#[test]
fn default_interface_postorder_includes_private_bridge_and_synthetic_but_not_static_or_abstract() {
    let mut w = world();
    let l = w.u.bootstrap_loader;
    let base = define(
        &mut w.registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        65,
    );
    let k = define(
        &mut w.registry,
        l,
        "K",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[Method::new("private", "()V", 2)],
        &[],
        65,
    );
    let i = define(
        &mut w.registry,
        l,
        "I",
        Some("java/lang/Object"),
        &["K"],
        0x601,
        &[],
        &[Method::new("abstract", "()V", 0x401)],
        &[],
        65,
    );
    let j = define(
        &mut w.registry,
        l,
        "J",
        Some("java/lang/Object"),
        &["K"],
        0x601,
        &[],
        &[Method::new("static", "()V", 9)],
        &[],
        65,
    );
    let bridge = define(
        &mut w.registry,
        l,
        "Bridge",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[Method::new("bridge", "()V", 0x1041)],
        &[],
        65,
    );
    let old = define(
        &mut w.registry,
        l,
        "Old",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[Method::new("<clinit>", "()V", 0)],
        &[],
        45,
    );
    let own = define(
        &mut w.registry,
        l,
        "C",
        Some("Base"),
        &["I", "J", "Bridge", "Old"],
        0x21,
        &[],
        &[],
        &[],
        65,
    );
    let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let direct = ctx.plan_initialization(&claim, i.class, limits()).unwrap();
    assert!(direct.dependencies().is_empty());
    assert!(
        ctx.plan_initialization(&claim, j.class, limits())
            .unwrap()
            .dependencies()
            .is_empty()
    );
    for _ in 0..2 {
        let before = ctx.lookup_steps();
        let plan = ctx
            .plan_initialization(&claim, own.class, limits())
            .unwrap();
        assert_eq!(plan.dependencies(), [base.class, k.class, bridge.class]);
        assert_eq!(ctx.lookup_steps() - before, 1 + 1 + 4 + 5 + 5 + 2);
    }
    assert_eq!(calls.get(), 0);
    let plan = ctx
        .plan_initialization(&claim, old.class, limits())
        .unwrap();
    assert!(plan.dependencies().is_empty());
    assert_eq!(plan.initializer(), Some(old.methods[0]));
}

#[test]
fn historical_parameterized_initializer_is_explicitly_unsupported_and_ignored_flags_are_metadata() {
    for (major, flags) in [(45, 0), (45, 0x520), (51, 0x128), (65, 0x428)] {
        let mut w = world();
        let own = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[Method::new("<clinit>", "()V", flags)],
            &[],
            major,
        );
        let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        assert_eq!(
            ctx.plan_initialization(&claim, own.class, limits())
                .unwrap()
                .initializer(),
            Some(own.methods[0])
        );
    }
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[Method::new("<clinit>", "(I)V", 0)],
        &[],
        45,
    );
    let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    assert!(
        matches!(ctx.plan_initialization(&claim,own.class,limits()),Err(E::UnsupportedInitializerDescriptor {method}) if method==own.methods[0])
    );
}

#[test]
fn remaining_plan_quotas_fail_before_growth_and_plans_can_be_retried_without_value_changes() {
    let mut w = world();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("a", "I", 9, Some(Cv::Int(1))),
            Field::new("b", "I", 25, Some(Cv::Int(2))),
        ],
        &[],
        &[],
        65,
    );
    let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    for allowance in [0, 1] {
        assert!(
            matches!(ctx.plan_initialization(&claim,own.class,InitializationPlanLimits {max_constants:allowance,max_dependencies:usize::MAX}),Err(E::PlanLimit {resource:InitializationPlanResource::Constants,limit,required}) if limit==allowance&&required==allowance+1)
        );
    }
    assert!(matches!(
        ctx.plan_initialization(
            &claim,
            own.class,
            InitializationPlanLimits {
                max_constants: 2,
                max_dependencies: 0
            }
        ),
        Err(E::PlanLimit {
            resource: InitializationPlanResource::Dependencies,
            limit: 0,
            required: 1
        })
    ));
    assert_eq!(
        ctx.inspect_static(&s, field(&w, own.class, "a", "I")),
        Ok(Value::Int(0))
    );
    let plan = ctx
        .plan_initialization(
            &claim,
            own.class,
            InitializationPlanLimits {
                max_constants: 2,
                max_dependencies: 1,
            },
        )
        .unwrap();
    ctx.install_initialization_constants(&claim, &plan).unwrap();
    assert_eq!(
        ctx.inspect_static(&s, field(&w, own.class, "a", "I")),
        Ok(Value::Int(1))
    );
    assert_eq!(
        ctx.inspect_static(&s, field(&w, own.class, "b", "I")),
        Ok(Value::Int(2))
    );
}

#[test]
fn readonly_class_validation_and_failed_anchor_metadata_checks_spend_no_work_or_claim() {
    let mut w = world();
    let missing = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Unadmitted",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        65,
    )
    .class;
    let foreign = world();
    let mut heap = heap(&w, &[w.error], &mut SameLoaderResolver);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let before = ctx.lookup_steps();
    assert!(
        matches!(ctx.claim_initialization(&s,&e,missing),Err(E::Context(FieldAccessError::Instance(InstanceError::LayoutMissing {class}))) if class==missing)
    );
    assert!(matches!(
        ctx.claim_initialization(&s, &e, foreign.error),
        Err(E::Context(FieldAccessError::Registry(_)))
    ));
    assert_eq!(ctx.lookup_steps(), before);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let before = ctx.lookup_steps();
    let readonly = &ctx;
    readonly
        .check_initialization_class(&claim, w.u.object)
        .unwrap();
    assert!(matches!(
        readonly.check_initialization_class(&claim, missing),
        Err(E::Context(FieldAccessError::Instance(
            InstanceError::LayoutMissing { .. }
        )))
    ));
    assert!(matches!(
        readonly.check_initialization_class(&claim, foreign.error),
        Err(E::Context(FieldAccessError::Registry(_)))
    ));
    assert_eq!(ctx.lookup_steps(), before);
}

#[test]
fn constants_installation_bulk_work_is_atomic_and_scans_only_the_own_class_range() {
    let mut w = world();
    let many_names: Vec<_> = (0..100).map(|i| format!("unrelated{i}")).collect();
    let many_fields: Vec<_> = many_names
        .iter()
        .map(|name| Field::new(name, "I", 9, None))
        .collect();
    let other = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &many_fields,
        &[],
        &[],
        65,
    );
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("a", "I", 9, Some(Cv::Int(43))),
            Field::new("b", "I", 25, Some(Cv::Int(59))),
        ],
        &[],
        &[],
        65,
    );
    let mut heap = heap(
        &w,
        &[w.error, other.class, own.class],
        &mut SameLoaderResolver,
    );
    let mut resolver = SameLoaderResolver;
    let (s, claim, plan) = {
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        let plan = ctx
            .plan_initialization(&claim, own.class, limits())
            .unwrap();
        (s, claim, plan)
    };
    let seed = context(&w, &mut heap, &mut resolver).lookup_steps();
    let limit = seed + 1;
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &w.registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: limit,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert!(
        matches!(ctx.install_initialization_constants(&claim,&plan),Err(E::Context(FieldAccessError::ResourceLimit {limit:l,required,..})) if l==limit&&required==seed+2)
    );
    assert_eq!(ctx.lookup_steps(), seed);
    for (name, desc) in [("a", "I"), ("b", "I")] {
        assert_eq!(
            ctx.inspect_static(&s, field(&w, own.class, name, desc)),
            Ok(Value::Int(0))
        );
    }
    drop(ctx);
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &w.registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: seed + 2,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    ctx.install_initialization_constants(&claim, &plan).unwrap();
    assert_eq!(ctx.lookup_steps(), seed + 2);
    assert_eq!(
        ctx.inspect_static(&s, field(&w, own.class, "a", "I")),
        Ok(Value::Int(43))
    );
    assert_eq!(
        ctx.inspect_static(&s, field(&w, own.class, "b", "I")),
        Ok(Value::Int(59))
    );
}

#[test]
fn error_classification_uses_actual_class_ancestry_including_foreign_loader_subclasses() {
    let mut w = world();
    let l = w.registry.create_loader().unwrap();
    let child = define(
        &mut w.registry,
        l,
        "ChildError",
        Some("java/lang/Error"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        65,
    );
    let ordinary = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "OtherThrowable",
        Some("java/lang/Throwable"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        65,
    );
    let mut resolver = Resolver::default();
    resolver
        .aliases
        .insert((l, units("java/lang/Error")), w.error);
    let mut heap = heap(&w, &[child.class, ordinary.class], &mut resolver);
    let error = heap.allocate_instance(w.error).unwrap();
    let child_ref = heap.allocate_instance(child.class).unwrap();
    let other_ref = heap.allocate_instance(ordinary.class).unwrap();
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    for (value, expected, work) in [
        (error, true, 1),
        (child_ref, true, 2),
        (other_ref, false, 3),
    ] {
        let thrown = ctx.prepare_throw(&e, value).unwrap();
        let before = ctx.lookup_steps();
        assert_eq!(ctx.is_initialization_error(&claim, &thrown), Ok(expected));
        assert_eq!(ctx.lookup_steps() - before, work);
    }
}

#[test]
fn reference_continuations_defer_value_resolution_and_allow_only_the_declaring_initializer_to_write_finals()
 {
    let mut w = world();
    let target = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
        &[],
        65,
    );
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            Field::new("final", "LTarget;", 25, None),
            Field::new("missing", "LMissing;", 9, None),
        ],
        &[
            Method::new("<clinit>", "()V", 8),
            Method::new("helper", "()V", 9),
        ],
        &[("C", "final", "LTarget;"), ("C", "missing", "LMissing;")],
        65,
    );
    let mut heap = heap(
        &w,
        &[w.error, target.class, own.class],
        &mut SameLoaderResolver,
    );
    let value = heap.allocate_instance(target.class).unwrap();
    let wrong = heap.allocate_instance(own.class).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let final_access = ctx
        .resolve_initialization_static(&claim, own.methods[0], own.refs[0], K::Write)
        .unwrap();
    ctx.write_initialization_static(&claim, &final_access, Value::Reference(value))
        .unwrap();
    assert!(
        matches!(ctx.write_initialization_static(&claim,&final_access,Value::Reference(wrong)),Err(E::Static(error)) if matches!(*error,StaticAccessError::IncompatibleReference(_)))
    );
    assert_eq!(
        ctx.static_roots(&s).unwrap().collect::<Vec<_>>(),
        [value.heap_ref().unwrap()]
    );
    let missing = ctx
        .resolve_initialization_static(&claim, own.methods[1], own.refs[1], K::Write)
        .unwrap();
    let before = ctx.lookup_steps();
    ctx.write_initialization_static(&claim, &missing, Value::Reference(Reference::null()))
        .unwrap();
    assert_eq!(ctx.lookup_steps(), before);
    assert_eq!(calls.get(), 0);
    assert!(
        matches!(ctx.write_initialization_static(&claim,&missing,Value::Int(7)),Err(E::Static(error)) if matches!(*error,StaticAccessError::ValueKindMismatch {..}))
    );
    assert_eq!(calls.get(), 0);
    assert!(
        matches!(ctx.write_initialization_static(&claim,&missing,Value::Reference(value)),Err(E::Static(error)) if matches!(*error,StaticAccessError::Type(_)))
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        ctx.static_roots(&s).unwrap().collect::<Vec<_>>(),
        [value.heap_ref().unwrap()]
    );
}

#[test]
fn long_shared_plain_descriptors_are_not_copied_or_scanned_by_a_constant_free_plan() {
    let mut w = world();
    let descriptor = format!("L{};", "X".repeat(65_533));
    let names: Vec<_> = (0..100).map(|i| format!("r{i}")).collect();
    let fields: Vec<_> = names
        .iter()
        .map(|name| Field::new(name, &descriptor, 9, None))
        .collect();
    let own = define(
        &mut w.registry,
        w.u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &[Method::new("helper", "()V", 9)],
        &[("C", &names[0], &descriptor)],
        65,
    );
    let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let before = ctx.lookup_steps();
    let plan = ctx
        .plan_initialization(
            &claim,
            own.class,
            InitializationPlanLimits {
                max_constants: 0,
                max_dependencies: 1,
            },
        )
        .unwrap();
    assert_eq!(ctx.lookup_steps() - before, 103);
    assert_eq!(plan.constant_count(), 0);
    let before = ctx.lookup_steps();
    ctx.install_initialization_constants(&claim, &plan).unwrap();
    assert_eq!(ctx.lookup_steps(), before);
    let access = ctx
        .resolve_initialization_static(&claim, own.methods[0], own.refs[0], K::Read)
        .unwrap();
    let before = ctx.lookup_steps();
    for _ in 0..2 {
        assert_eq!(
            ctx.read_initialization_static(&claim, &access).unwrap(),
            Value::Reference(Reference::null())
        );
    }
    assert_eq!(ctx.lookup_steps(), before);
    assert!(format!("{access:?}").len() < 2048);
    assert_eq!(calls.get(), 0);
}

#[test]
fn deep_default_interface_planning_and_drop_use_iterative_bounded_storage() {
    const DEPTH: usize = 10_000;
    let mut w = world_options(
        "java/lang/Error",
        Some("java/lang/Throwable"),
        0x21,
        RegistryOptions {
            max_definitions: DEPTH + 6,
            ..RegistryOptions::default()
        },
    );
    let l = w.u.bootstrap_loader;
    let mut previous = None;
    let mut first = None;
    for i in 0..DEPTH {
        let name = format!("I{i}");
        let parents: Vec<_> = previous.as_deref().into_iter().collect();
        let id = define(
            &mut w.registry,
            l,
            &name,
            Some("java/lang/Object"),
            &parents,
            0x601,
            &[],
            &[Method::new("default", "()V", 1)],
            &[],
            65,
        )
        .class;
        if first.is_none() {
            first = Some(id);
        }
        previous = Some(name);
    }
    let root = define(
        &mut w.registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[previous.as_deref().unwrap()],
        0x21,
        &[],
        &[],
        &[],
        65,
    );
    let hierarchy = HierarchyOptions {
        max_classes: DEPTH + 6,
        max_edges: 2 * DEPTH + 5,
        max_declared_fields: 0,
        max_instance_slots: 0,
    };
    let plans: Vec<_> = [root.class, w.error, w.u.cloneable, w.u.serializable]
        .iter()
        .map(|class| {
            build_hierarchy(&w.registry, *class, &mut SameLoaderResolver, hierarchy).unwrap()
        })
        .collect();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 6,
            max_layout_edges: 2 * DEPTH + 5,
            max_layout_fields: 0,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&w.registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&w.registry, w.u).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&w, &mut heap, &mut resolver);
    let (s, e) = profiles(&mut ctx, w.throwable);
    let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
    let before = ctx.lookup_steps();
    let plan = ctx
        .plan_initialization(
            &claim,
            root.class,
            InitializationPlanLimits {
                max_dependencies: DEPTH + 1,
                max_constants: 0,
            },
        )
        .unwrap();
    assert_eq!(ctx.lookup_steps() - before, 3 * DEPTH + 2);
    assert_eq!(plan.dependencies().len(), DEPTH + 1);
    assert_eq!(plan.dependencies()[0], w.u.object);
    assert_eq!(plan.dependencies()[1], first.unwrap());
    drop(plan);
    drop(ctx);
    drop(heap);
    drop(plans);
    drop(w);
}

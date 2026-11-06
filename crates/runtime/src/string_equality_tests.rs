//! Private authority tests use synthetic metadata, not an execution oracle.
//! Independent public tests and the coordinator's original JDK replay cover
//! recipe semantics separately; this local emitter exercises sealed invariants.
use super::*;
use crate::mirror_support::{Class, Graph};
fn render(c: &mut Class, spec: &[O], wide: bool) -> Vec<u8> {
    let mut parts = Vec::new();
    let mut pcs = Vec::new();
    let mut pc = 0usize;
    for &O(op, arg) in spec {
        pcs.push(pc);
        let mut part = vec![op];
        match arg {
            A::None => {}
            A::Branch(_) => part.extend([0, 0]),
            A::Inc(i, d) => part.extend([i, d as u8]),
            A::Class(name) => part.extend(c.class(name).to_be_bytes()),
            A::Ref(tag, owner, name, desc) => {
                part.extend(c.reference(tag, owner, name, desc).to_be_bytes())
            }
            A::Ldc(name) => {
                let cp = c.class(name);
                if wide || cp > 255 {
                    part[0] = 0x13;
                    part.extend(cp.to_be_bytes())
                } else {
                    part.push(cp as u8)
                }
            }
        };
        pc += part.len();
        parts.push(part);
    }
    for (i, op) in spec.iter().enumerate() {
        if let A::Branch(target) = op.1 {
            let d = i16::try_from(pcs[target] as isize - pcs[i] as isize).unwrap();
            parts[i][1..3].copy_from_slice(&d.to_be_bytes());
        }
    }
    parts.concat()
}
fn push(c: &mut Class, n: &str, d: &str, flags: u16, spec: &[O], wide: bool) {
    let code = render(c, spec, wide);
    c.method(n, d, flags, &code, 4, 4);
}
pub(crate) fn image(wide: bool, damage: u8) -> (Graph, ClassId, CompactStringBodyPlan) {
    let mut g = Graph::new();
    let boot = g.boot;
    g.add(
        boot,
        Class::new("java/util/Comparator", Some(OBJECT), 0x601),
    );
    g.add(boot, Class::plain(OSF));
    let mut comparator = Class::new(
        "java/lang/String$CaseInsensitiveComparator",
        Some(OBJECT),
        0x20,
    );
    comparator.interface("java/util/Comparator");
    push(
        &mut comparator,
        "<init>",
        "()V",
        2,
        &[o(0x2a), m(0xb7, OBJECT, "<init>", "()V"), o(0xb1)],
        false,
    );
    g.add(boot, comparator);
    let mut string = Class::new(STRING, Some(OBJECT), 0x31);
    for (n, d, flags) in [
        ("value", "[B", 18),
        ("coder", "B", 18),
        ("COMPACT_STRINGS", "Z", 24),
        ("serialPersistentFields", "[Ljava/io/ObjectStreamField;", 26),
        ("CASE_INSENSITIVE_ORDER", "Ljava/util/Comparator;", 25),
    ] {
        string.field(n, d, flags);
    }
    push(
        &mut string,
        "length",
        "()I",
        1,
        &[
            o(0x2a),
            f(0xb4, STRING, "value", "[B"),
            o(0xbe),
            o(0x2a),
            m(0xb6, STRING, "coder", "()B"),
            o(0x7a),
            o(0xac),
        ],
        false,
    );
    push(
        &mut string,
        "isEmpty",
        "()Z",
        1,
        &[
            o(0x2a),
            f(0xb4, STRING, "value", "[B"),
            o(0xbe),
            b(0x9a, 6),
            o(4),
            b(0xa7, 7),
            o(3),
            o(0xac),
        ],
        false,
    );
    push(
        &mut string,
        "coder",
        "()B",
        0,
        &[
            f(0xb2, STRING, "COMPACT_STRINGS", "Z"),
            b(0x99, 5),
            o(0x2a),
            f(0xb4, STRING, "coder", "B"),
            b(0xa7, 6),
            o(4),
            o(0xac),
        ],
        false,
    );
    push(
        &mut string,
        "<clinit>",
        "()V",
        8,
        &[
            o(4),
            f(0xb3, STRING, "COMPACT_STRINGS", "Z"),
            o(3),
            c(0xbd, OSF),
            f(
                0xb3,
                STRING,
                "serialPersistentFields",
                "[Ljava/io/ObjectStreamField;",
            ),
            c(0xbb, "java/lang/String$CaseInsensitiveComparator"),
            o(0x59),
            m(
                0xb7,
                "java/lang/String$CaseInsensitiveComparator",
                "<init>",
                "()V",
            ),
            f(
                0xb3,
                STRING,
                "CASE_INSENSITIVE_ORDER",
                "Ljava/util/Comparator;",
            ),
            o(0xb1),
        ],
        false,
    );
    string.method("intern", "()Ljava/lang/String;", 0x101, &[], 0, 1);
    string.method("charAt", "(I)C", 1, &[3, 0xac], 1, 2);
    let mut equals = EQUALS.to_vec();
    if damage == 1 {
        equals[0] = o(0x2b)
    }
    if damage != 11 {
        push(
            &mut string,
            "equals",
            "(Ljava/lang/Object;)Z",
            if damage == 2 {
                0
            } else if damage == 12 {
                0x101
            } else {
                1
            },
            &equals,
            false,
        );
    }
    let s = g.add(boot, string);
    if damage != 10 {
        let mut helper = Class::new(HELPER, Some(OBJECT), if damage == 9 { 0x20 } else { 0x30 });
        helper.field("$assertionsDisabled", "Z", 0x1018);
        let mut loop_body = HELPER_EQUALS.to_vec();
        if damage == 3 {
            loop_body[20] = O(0x84, A::Inc(2, 2))
        }
        push(&mut helper, "equals", "([B[B)Z", 9, &loop_body, false);
        push(&mut helper, "<clinit>", "()V", 8, HELPER_INIT, wide);
        g.add(boot, helper);
    }
    let mut class = Class::new(CLASS, Some(OBJECT), 0x31);
    class.field(
        "classLoader",
        "Ljava/lang/ClassLoader;",
        if damage == 6 { 0 } else { 0x12 },
    );
    class.field("EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;", 0x1a);
    class.field(
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
        0x1a,
    );
    class.field("opaque", "I", 1);
    let mut status = STATUS.to_vec();
    if damage == 4 {
        status[12] = o(0)
    }
    push(
        &mut class,
        "desiredAssertionStatus",
        "()Z",
        1,
        &status,
        false,
    );
    for row in [
        [21, 38, if damage == 5 { 45 } else { 44 }, 0],
        [39, 41, 44, 0],
        [44, 47, 44, 0],
    ] {
        class.handler(row);
    }
    let mut init = CLASS_INIT.to_vec();
    if damage == 8 {
        init[2] = c(0xbd, OBJECT)
    }
    push(&mut class, "<clinit>", "()V", 8, &init, false);
    class.method(
        "registerNatives",
        "()V",
        if damage == 7 { 0xa } else { 0x10a },
        &[0xb1],
        0,
        0,
    );
    class.method(
        "desiredAssertionStatus0",
        "(Ljava/lang/Class;)Z",
        0x10a,
        &[],
        0,
        1,
    );
    class.method("other", "()I", 1, &[3, 0xac], 1, 1);
    // A genuine own CP field reference is needed for checked direct reads.
    class.reference(9, CLASS, "classLoader", "Ljava/lang/ClassLoader;");
    g.add(boot, class);
    let body = probe_compact_string_bodies(g.domain.registry(), boot, s, Default::default())
        .unwrap()
        .plan
        .unwrap();
    (g, s, body)
}
struct F {
    g: Graph,
    h: ManagedHeap,
    p: CompactStringEqualityPlan,
    site: u16,
    app: ClassId,
    marker: ClassId,
}
fn fixture(policy: BootstrapAssertionPolicy) -> F {
    let (mut g, _, body) = image(false, 0);
    let boot = g.boot;
    let loader = g.loader();
    let app = g.add(loader, Class::plain("app/Guest"));
    let mut h = g.ready();
    let u = g.bind_core(&mut h);
    g.domain.enable_class_mirror_sites(100).unwrap();
    let p = probe_compact_string_equality(g.domain.registry(), &body, Default::default())
        .unwrap()
        .plan
        .unwrap();
    h.bind_class_mirrors(g.domain.registry(), p.class, 100)
        .unwrap();
    h.bind_strings(g.domain.registry(), body.string, Default::default())
        .unwrap();
    h.bind_compact_string_bodies(g.domain.registry(), &body, Default::default())
        .unwrap();
    h.bind_compact_string_equality(g.domain.registry(), &p, policy)
        .unwrap();
    let class = g.domain.registry().class(p.class).unwrap();
    let site = class
        .constant_pool
        .entries()
        .iter()
        .enumerate()
        .find_map(|(i, c)| match c {
            Constant::Fieldref { class_index, .. }
                if class
                    .constant_pool
                    .class_name(*class_index)
                    .is_ok_and(|n| n.equals_ascii(CLASS)) =>
            {
                Some(i as u16)
            }
            _ => None,
        })
        .unwrap();
    let _ = boot;
    F {
        g,
        h,
        p,
        site,
        app,
        marker: u.cloneable,
    }
}
fn ctx(f: &mut F) -> FieldResolutionContext<'_> {
    f.g.domain
        .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
        .unwrap()
}
#[test]
fn narrow_and_wide_class_literal_recipes_match_and_have_identical_logical_prices() {
    let mut prices = Vec::new();
    for wide in [false, true] {
        let (g, _, body) = image(wide, 0);
        let d =
            probe_compact_string_equality(g.domain.registry(), &body, Default::default()).unwrap();
        assert!(d.plan.is_some());
        let count = d.usage.work_units;
        prices.push(count);
        let e = probe_compact_string_equality(
            g.domain.registry(),
            &body,
            StringBodyDiscoveryLimits {
                max_work_units: count - 1,
            },
        )
        .unwrap_err();
        assert!(
            matches!(e.kind,StringBodyErrorKind::Limit{resource:StringBodyResource::DiscoveryWork,limit,required}if limit==count-1&&required==count)
        );
        assert!(e.discovery_usage.work_units < count);
        assert!(
            probe_compact_string_equality(
                g.domain.registry(),
                &body,
                StringBodyDiscoveryLimits {
                    max_work_units: count
                }
            )
            .unwrap()
            .plan
            .is_some()
        );
    }
    assert_eq!(prices[0], prices[1]);
}
#[test]
fn first_recipe_difference_is_clean_but_every_dependent_claimed_recipe_error_is_strict() {
    for damage in 1..=10 {
        let (g, _, body) = image(false, damage);
        let result = probe_compact_string_equality(g.domain.registry(), &body, Default::default());
        if damage == 1 {
            assert!(result.unwrap().plan.is_none())
        } else {
            let e = result.unwrap_err();
            assert!(matches!(e.kind, StringBodyErrorKind::InvalidBinding(_)));
            assert!(e.discovery_usage.work_units > 0);
        }
    }
}
#[test]
fn native_classification_and_site_guards_have_exact_prices_without_mutating_effects() {
    let mut f = fixture(BootstrapAssertionPolicy::Enabled);
    let p = f.p.clone();
    let other = f.g.method(p.class, "other", "()I");
    let mut c = ctx(&mut f);
    let start = c.lookup_steps();
    assert_eq!(c.class_assertion_native(p.methods[0]).unwrap(), None);
    assert_eq!(c.lookup_steps(), start);
    for (role, id) in [
        (ClassAssertionNative::Register, p.methods[5]),
        (ClassAssertionNative::DesiredStatus, p.methods[6]),
    ] {
        assert_eq!(c.class_assertion_native(id).unwrap(), Some(role));
    }
    assert_eq!(c.class_assertion_native(other).unwrap(), None);
    assert_eq!(c.lookup_steps(), start + 3);
    for (role, caller, pc) in [
        (ClassAssertionNative::Register, p.methods[4], 0),
        (ClassAssertionNative::DesiredStatus, p.methods[3], 10),
        (ClassAssertionNative::DesiredStatus, p.methods[3], 50),
    ] {
        c.check_class_assertion_site(role, caller, pc).unwrap();
        assert!(c.check_class_assertion_site(role, caller, pc + 1).is_err());
        assert!(c.check_class_assertion_site(role, other, pc).is_err());
    }
    assert_eq!(c.lookup_steps(), start + 3);
    assert_eq!(c.heap().class_mirror_count(), 0);
    assert!(c.heap().initialization_claim.is_none());
}
#[test]
fn supported_canonical_bootstrap_class_and_interface_targets_observe_fixed_policy_without_work() {
    for policy in [
        BootstrapAssertionPolicy::Disabled,
        BootstrapAssertionPolicy::Enabled,
    ] {
        let mut f = fixture(policy);
        let object = f.g.object;
        let marker = f.marker;
        let p = f.p.clone();
        let site = f.site;
        let mut c = ctx(&mut f);
        for id in [object, marker, p.body.string, p.helper, p.class] {
            let r = c.class_mirror_for_type(ReferenceType::Class(id)).unwrap();
            let before = c.lookup_steps();
            assert_eq!(c.bootstrap_assertion_policy(r).unwrap(), policy);
            assert_eq!(c.lookup_steps(), before);
            assert_eq!(
                c.read_field(p.class, site, r).unwrap(),
                Value::Reference(Reference::null())
            );
        }
        assert!(c.heap().initialization_claim.is_none());
        assert_eq!(c.heap().allocated_arrays(), 0);
    }
}
#[test]
fn app_and_all_array_targets_enter_the_java_role_but_fail_at_the_real_checked_field() {
    let mut f = fixture(BootstrapAssertionPolicy::Disabled);
    let p = f.p.clone();
    let app = f.app;
    let site = f.site;
    let boot = f.g.boot;
    let mut c = ctx(&mut f);
    for ty in [
        ReferenceType::Class(app),
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
            defining_loader: boot,
        },
        ReferenceType::Array {
            dimensions: 2,
            element: ArrayElement::Class(p.body.string),
            defining_loader: boot,
        },
    ] {
        let r = c.class_mirror_for_type(ty).unwrap();
        c.check_method_reference_profile(p.methods[3], Some(r))
            .unwrap();
        assert!(
            matches!(c.read_field(p.class,site,r),Err(FieldAccessError::StringBody(e))if e.kind==StringBodyErrorKind::UnsupportedAssertionTarget{represented:ty})
        );
    }
    assert!(c.heap().initialization_claim.is_none());
}
#[test]
fn raw_storage_and_unrelated_class_and_string_code_stay_opaque() {
    let mut f = fixture(BootstrapAssertionPolicy::Disabled);
    let p = f.p.clone();
    let other = f.g.method(p.class, "other", "()I");
    let field = p.fields[1];
    let char_at = f.g.method(p.body.string, "charAt", "(I)C");
    let mut c = ctx(&mut f);
    let mirror = c
        .class_mirror_for_type(ReferenceType::Class(p.class))
        .unwrap();
    assert!(matches!(
        c.heap().get_field(mirror, field),
        Err(InstanceError::OpaqueClassObject { .. })
    ));
    assert!(
        c.check_method_reference_profile(other, Some(mirror))
            .is_err()
    );
    let string = c.fresh_string(&[65]).unwrap();
    c.check_method_reference_profile(p.methods[0], Some(string))
        .unwrap();
    assert!(
        c.check_method_reference_profile(char_at, Some(string))
            .is_err()
    );
    assert_eq!(c.heap().allocated_arrays(), 0);
}
#[test]
fn missing_binding_preserves_legacy_opacity_and_native_classifier_zero_price() {
    let mut f = fixture(BootstrapAssertionPolicy::Disabled);
    let p = f.p.clone();
    f.h.string_equality = None;
    let mut c = ctx(&mut f);
    let before = c.lookup_steps();
    assert_eq!(c.class_assertion_native(p.methods[5]).unwrap(), None);
    assert_eq!(c.lookup_steps(), before);
    let r = c
        .class_mirror_for_type(ReferenceType::Class(p.class))
        .unwrap();
    assert!(
        c.check_method_reference_profile(p.methods[3], Some(r))
            .is_err()
    );
    assert!(c.bootstrap_assertion_policy(r).is_err());
}
#[test]
fn identical_repeat_preserves_issued_mirrors_and_work_but_changed_policy_is_conflict() {
    let mut f = fixture(BootstrapAssertionPolicy::Disabled);
    let p = f.p.clone();
    let r = ctx(&mut f)
        .class_mirror_for_type(ReferenceType::Class(p.helper))
        .unwrap();
    let count = f.h.class_mirror_count();
    f.h.bind_compact_string_equality(
        f.g.domain.registry(),
        &p,
        BootstrapAssertionPolicy::Disabled,
    )
    .unwrap();
    assert_eq!(f.h.class_mirror_count(), count);
    assert!(matches!(
        f.h.bind_compact_string_equality(
            f.g.domain.registry(),
            &p,
            BootstrapAssertionPolicy::Enabled
        )
        .unwrap_err()
        .kind,
        StringBodyErrorKind::PolicyConflict
    ));
    assert_eq!(
        ctx(&mut f).bootstrap_assertion_policy(r).unwrap(),
        BootstrapAssertionPolicy::Disabled
    );
}
#[test]
fn first_binding_rejects_prior_entries_or_charged_work_without_partial_state() {
    for variant in 0..3 {
        let mut f = fixture(BootstrapAssertionPolicy::Disabled);
        let p = f.p.clone();
        f.h.string_equality = None;
        match variant {
            0 => {
                ctx(&mut f).fresh_string(&[65]).unwrap();
            }
            1 => {
                ctx(&mut f)
                    .class_mirror_for_type(ReferenceType::Class(p.helper))
                    .unwrap();
            }
            _ => {
                f.h.string_bodies.as_mut().unwrap().usage.work_units = 1;
            }
        }
        assert!(
            f.h.bind_compact_string_equality(
                f.g.domain.registry(),
                &p,
                BootstrapAssertionPolicy::Disabled
            )
            .is_err()
        );
        assert!(f.h.string_equality.is_none());
    }
}
#[test]
fn damaged_plan_keys_owners_sites_and_registry_cannot_publish_or_replace_authority() {
    for damage in 0..6 {
        let mut f = fixture(BootstrapAssertionPolicy::Disabled);
        let mut p = f.p.clone();
        match damage {
            0 => p.body.registry = u64::MAX,
            1 => p.methods[5] = p.methods[6],
            2 => p.fields[1] = p.fields[2],
            3 => p.sites[1] = 9,
            4 => p.class = p.helper,
            _ => p.body.methods[0] = p.body.methods[1],
        };
        assert!(
            f.h.bind_compact_string_equality(
                f.g.domain.registry(),
                &p,
                BootstrapAssertionPolicy::Disabled
            )
            .is_err()
        );
        assert_eq!(f.h.class_mirror_count(), 0);
        assert_eq!(f.h.string_body_usage(), StringBodyUsage::default());
    }
}

#[test]
fn absent_shape_has_hand_counted_thirty_work_and_funds_full_equal_length_comparisons() {
    let (g, _, body) = image(false, 11);
    // Six headers + six UTF handles; length, intern, charAt each have six
    // units and therefore pay six for a failed comparison against "equals".
    let d = probe_compact_string_equality(
        g.domain.registry(),
        &body,
        StringBodyDiscoveryLimits { max_work_units: 30 },
    )
    .unwrap();
    assert!(d.plan.is_none());
    assert_eq!(d.usage.work_units, 30);
    for (limit, accepted, required) in [(0, 0, 1), (1, 1, 2), (2, 2, 8), (7, 2, 8)] {
        let e = probe_compact_string_equality(
            g.domain.registry(),
            &body,
            StringBodyDiscoveryLimits {
                max_work_units: limit,
            },
        )
        .unwrap_err();
        assert_eq!(e.discovery_usage.work_units, accepted);
        assert_eq!(
            e.kind,
            StringBodyErrorKind::Limit {
                resource: StringBodyResource::DiscoveryWork,
                limit,
                required
            }
        );
    }
    let (g, _, body) = image(false, 12);
    assert!(
        probe_compact_string_equality(g.domain.registry(), &body, Default::default())
            .unwrap()
            .plan
            .is_none()
    );
    let wrong = Graph::new();
    let e = probe_compact_string_equality(
        wrong.domain.registry(),
        &body,
        StringBodyDiscoveryLimits { max_work_units: 0 },
    )
    .unwrap_err();
    assert!(matches!(e.kind, StringBodyErrorKind::InvalidBinding(_)));
    assert_eq!(e.discovery_usage.work_units, 0);
}
#[test]
fn unrelated_primitive_array_before_first_equality_bind_is_preserved_and_not_a_conflict() {
    let mut f = fixture(BootstrapAssertionPolicy::Disabled);
    let p = f.p.clone();
    f.h.string_equality = None;
    let array =
        f.h.allocate_primitive_array(PrimitiveArrayKind::Int, 1)
            .unwrap();
    f.h.store(array, 0, Value::Int(99)).unwrap();
    f.h.bind_compact_string_equality(
        f.g.domain.registry(),
        &p,
        BootstrapAssertionPolicy::Disabled,
    )
    .unwrap();
    assert_eq!(f.h.load(array, 0).unwrap(), Value::Int(99));
    assert_eq!(f.h.allocated_arrays(), 1);
    assert_eq!(f.h.string_usage(), StringUsage::default());
}
#[test]
fn null_foreign_stale_nonmirror_and_forward_map_damage_keep_typed_host_causes() {
    let mut f = fixture(BootstrapAssertionPolicy::Disabled);
    let mut foreign = fixture(BootstrapAssertionPolicy::Disabled);
    let foreign_type = foreign.p.helper;
    let foreign_ref = ctx(&mut foreign)
        .class_mirror_for_type(ReferenceType::Class(foreign_type))
        .unwrap();
    let p = f.p.clone();
    let mut c = ctx(&mut f);
    let r = c
        .class_mirror_for_type(ReferenceType::Class(p.helper))
        .unwrap();
    let (_, i, generation) = r.heap_ref().unwrap().components();
    let stale = Reference::issued(c.heap().identity, i, generation + 1);
    for (input, wanted) in [
        (Reference::null(), HeapError::NullReference),
        (foreign_ref, HeapError::ForeignHeap),
        (
            stale,
            HeapError::StaleGeneration {
                expected: generation,
                actual: generation + 1,
            },
        ),
    ] {
        let before = c.lookup_steps();
        let e = c.bootstrap_assertion_policy(input).unwrap_err();
        assert!(matches!(e.kind,StringBodyErrorKind::Heap(ref e) if **e==wanted));
        assert_eq!(c.lookup_steps(), before);
    }
    let ordinary = c
        .heap_mut()
        .allocate_primitive_array(PrimitiveArrayKind::Byte, 0)
        .unwrap();
    assert_eq!(
        c.bootstrap_assertion_policy(ordinary).unwrap_err().kind,
        StringBodyErrorKind::InvalidReceiver
    );
    assert!(
        c.check_method_reference_profile(p.methods[3], Some(ordinary))
            .is_err()
    );
    let other = c
        .class_mirror_for_type(ReferenceType::Class(p.body.string))
        .unwrap();
    let before = c.lookup_steps();
    let count = c.heap().class_mirror_count();
    c.heap_mut().entries[i].payload =
        crate::heap::EntryPayload::Mirror(ReferenceType::Class(p.body.string));
    assert!(matches!(
        c.bootstrap_assertion_policy(r).unwrap_err().kind,
        StringBodyErrorKind::InvalidState(_)
    ));
    assert!(
        c.check_method_reference_profile(p.methods[3], Some(r))
            .is_err()
    );
    assert_eq!(c.heap().class_mirror_count(), count);
    assert_eq!(
        c.bootstrap_assertion_policy(other).unwrap(),
        BootstrapAssertionPolicy::Disabled
    );
    assert_eq!(c.lookup_steps(), before);
    assert!(c.heap().initialization_claim.is_none());
}
#[test]
fn classifier_zero_and_exact_one_step_limits_do_not_change_heap_or_native_authority() {
    for limit in [0, 1, usize::MAX] {
        let mut f = fixture(BootstrapAssertionPolicy::Enabled);
        let id = f.p.methods[5];
        let mut c = ctx(&mut f);
        let before = c.lookup_steps();
        // Context opening has its own funded namespace work. Isolate the
        // classifier's remaining allowance after that accepted prefix.
        c.options.max_lookup_steps = before.saturating_add(limit);
        let result = c.class_assertion_native(id);
        if limit == 0 {
            assert!(
                matches!(result.unwrap_err().kind,StringBodyErrorKind::Context(ref e)if matches!(**e,FieldAccessError::ResourceLimit{..}))
            );
            assert_eq!(c.lookup_steps(), before);
        } else {
            assert_eq!(result.unwrap(), Some(ClassAssertionNative::Register));
            assert_eq!(c.lookup_steps(), before + 1);
        }
        assert_eq!(c.heap().class_mirror_count(), 0);
        assert_eq!(c.heap().allocated_arrays(), 0);
        assert!(c.heap().initialization_claim.is_none());
    }
}
#[test]
fn shared_namespace_two_heaps_retain_independent_immutable_policies_and_canonical_roots() {
    let mut f = fixture(BootstrapAssertionPolicy::Disabled);
    let p = f.p.clone();
    let mut second = f.g.heap(&f.g.classes);
    second
        .bind_reference_types(
            f.g.domain.registry(),
            f.h.reference_type_universe().unwrap(),
        )
        .unwrap();
    second
        .bind_class_mirrors(f.g.domain.registry(), p.class, 100)
        .unwrap();
    second
        .bind_strings(f.g.domain.registry(), p.body.string, Default::default())
        .unwrap();
    second
        .bind_compact_string_bodies(f.g.domain.registry(), &p.body, Default::default())
        .unwrap();
    second
        .bind_compact_string_equality(f.g.domain.registry(), &p, BootstrapAssertionPolicy::Enabled)
        .unwrap();
    let a = ctx(&mut f)
        .class_mirror_for_type(ReferenceType::Class(p.helper))
        .unwrap();
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(&mut second, Default::default(), Default::default())
            .unwrap();
    let b = c
        .class_mirror_for_type(ReferenceType::Class(p.helper))
        .unwrap();
    assert_ne!(a, b);
    assert_eq!(
        c.bootstrap_assertion_policy(b).unwrap(),
        BootstrapAssertionPolicy::Enabled
    );
    assert!(
        matches!(c.bootstrap_assertion_policy(a).unwrap_err().kind,StringBodyErrorKind::Heap(ref e)if **e==HeapError::ForeignHeap)
    );
    assert_eq!(
        ctx(&mut f).bootstrap_assertion_policy(a).unwrap(),
        BootstrapAssertionPolicy::Disabled
    );
    assert_eq!(
        f.h.class_mirror_roots().collect::<Vec<_>>(),
        vec![a.heap_ref().unwrap()]
    );
    assert_eq!(
        second.class_mirror_roots().collect::<Vec<_>>(),
        vec![b.heap_ref().unwrap()]
    );
}

#[test]
fn legacy_context_cannot_supply_checked_native_or_site_namespace_authority() {
    let mut f = fixture(BootstrapAssertionPolicy::Enabled);
    let p = f.p.clone();
    let mut resolver = SameLoaderResolver;
    let mut c = FieldResolutionContext::new_unnamed_modules(
        f.g.domain.registry(),
        &mut f.h,
        &mut resolver,
        Default::default(),
    )
    .unwrap();
    let before = c.lookup_steps();
    assert_eq!(c.class_assertion_native(p.methods[0]).unwrap(), None);
    assert_eq!(c.lookup_steps(), before);
    let e = c.class_assertion_native(p.methods[5]).unwrap_err();
    assert!(
        matches!(e.kind,StringBodyErrorKind::Context(ref e)if matches!(**e,FieldAccessError::ClassMirror(ref e)if **e==ClassMirrorError::Disabled))
    );
    assert_eq!(c.lookup_steps(), before + 1);
    let e = c
        .check_class_assertion_site(ClassAssertionNative::Register, p.methods[4], 0)
        .unwrap_err();
    assert!(
        matches!(e.kind,StringBodyErrorKind::Context(ref e)if matches!(**e,FieldAccessError::ClassMirror(ref e)if **e==ClassMirrorError::Disabled))
    );
    assert_eq!(c.lookup_steps(), before + 1);
    assert!(c.heap().initialization_claim.is_none());
}

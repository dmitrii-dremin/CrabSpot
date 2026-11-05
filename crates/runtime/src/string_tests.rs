use super::*;
use crate::mirror_support::{Class, Graph, units, world};
use crabspot_classfile::ParseOptions;

pub(crate) struct Fixture {
    pub(crate) g: Graph,
    pub(crate) h: ManagedHeap,
    pub(crate) string: ClassId,
    pub(crate) caller: ClassId,
    pub(crate) sites: Vec<u16>,
    pub(crate) constants: ClassId,
    pub(crate) fields: [FieldId; 3],
    pub(crate) native: MethodId,
    pub(crate) body: MethodId,
    pub(crate) cv_sites: [u16; 2],
}
fn text_cp(c: &mut Class, units: &[u16]) -> u16 {
    let mut bytes = Vec::new();
    for &u in units {
        match u {
            1..=127 => bytes.push(u as u8),
            0..=2047 => bytes.extend([0xc0 | (u >> 6) as u8, 0x80 | (u & 63) as u8]),
            _ => bytes.extend([
                0xe0 | (u >> 12) as u8,
                0x80 | ((u >> 6) & 63) as u8,
                0x80 | (u & 63) as u8,
            ]),
        }
    }
    let mut data = (u16::try_from(bytes.len()).unwrap()).to_be_bytes().to_vec();
    data.extend(bytes);
    let utf = c.cp(1, &data);
    c.cp(8, &utf.to_be_bytes())
}
fn with_values(c: &mut Class, values: &[u16]) -> Vec<u8> {
    let name = c.utf("ConstantValue");
    let bytes = c.bytes();
    let cp = u16::from_be_bytes([bytes[8], bytes[9]]);
    let mut p = 10;
    for _ in 1..cp {
        let tag = bytes[p];
        p += 1;
        p += match tag {
            1 => {
                let n = u16::from_be_bytes([bytes[p], bytes[p + 1]]) as usize;
                2 + n
            }
            7 | 8 => 2,
            3 | 9 | 10 | 11 | 12 => 4,
            _ => panic!("unexpected fixture CP tag"),
        };
    }
    p += 6;
    let n = u16::from_be_bytes([bytes[p], bytes[p + 1]]) as usize;
    p += 2 + 2 * n;
    let fields = u16::from_be_bytes([bytes[p], bytes[p + 1]]) as usize;
    p += 2;
    assert_eq!(fields, values.len());
    let mut out = bytes[..p].to_vec();
    for &cv in values {
        out.extend(&bytes[p..p + 6]);
        out.extend(1_u16.to_be_bytes());
        out.extend(name.to_be_bytes());
        out.extend(2_u32.to_be_bytes());
        out.extend(cv.to_be_bytes());
        p += 8;
    }
    out.extend(&bytes[p..]);
    out
}
fn fixture_with(limits: StringLimits, texts: &[&[u16]], native_flags: u16) -> Fixture {
    let w = world(ClassMirrorLimits::default());
    let mut g = w.g;
    let boot = g.boot;
    let mut string = Class::new("java/lang/String", Some("java/lang/Object"), 0x31);
    string.field("value", "I", 1);
    string.method(
        "intern",
        "()Ljava/lang/String;",
        native_flags,
        &[1, 0xb0],
        1,
        1,
    );
    string.method("length", "()I", 1, &[3, 0xac], 1, 1);
    let string = g.add(boot, string);
    let native = g.method(string, "intern", "()Ljava/lang/String;");
    let body = g.method(string, "length", "()I");
    let mut caller = Class::plain("s/Caller");
    let sites = texts.iter().map(|t| text_cp(&mut caller, t)).collect();
    let caller = g.add(boot, caller);
    let mut constants = Class::plain("s/Constants");
    constants.field("number", "I", 9);
    constants.field("first", "Ljava/lang/String;", 25);
    constants.field("second", "Ljava/lang/String;", 9);
    let number = constants.cp(3, &7_i32.to_be_bytes());
    let a = text_cp(&mut constants, &[65]);
    let b = text_cp(&mut constants, &[66]);
    let bytes = with_values(&mut constants, &[number, a, b]);
    let id = g
        .domain
        .context(LoadingWorkLimits::default())
        .define(
            boot,
            &units(&constants.name),
            &bytes,
            ParseOptions::default(),
        )
        .unwrap();
    g.classes.push(id);
    g.intended.insert((boot, units(&constants.name)), id);
    g.prepare(&[string, caller, id]);
    let mut h = g.heap(&g.classes);
    h.bind_reference_types(g.domain.registry(), w.u).unwrap();
    h.bind_class_mirrors(g.domain.registry(), w.class, 16_384)
        .unwrap();
    h.bind_strings(g.domain.registry(), string, limits).unwrap();
    let fields = [
        g.field(id, "number", "I"),
        g.field(id, "first", "Ljava/lang/String;"),
        g.field(id, "second", "Ljava/lang/String;"),
    ];
    Fixture {
        g,
        h,
        string,
        caller,
        sites,
        constants: id,
        fields,
        native,
        body,
        cv_sites: [a, b],
    }
}
pub(crate) fn fixture(limits: StringLimits) -> Fixture {
    fixture_with(limits, &[&[65], &[65], &[66], &[]], 0x101)
}
pub(crate) fn context(f: &mut Fixture) -> FieldResolutionContext<'_> {
    f.g.domain
        .member_context_unnamed_modules(
            &mut f.h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap()
}
pub(crate) fn claim(
    c: &mut FieldResolutionContext<'_>,
    gboot: LoaderId,
) -> (InitializationClaim, StaticStateProfile) {
    let registry = c.registry();
    let throwable = registry
        .lookup_class(gboot, &units("java/lang/Throwable"))
        .unwrap()
        .unwrap();
    let error = registry
        .lookup_class(gboot, &units("java/lang/Error"))
        .unwrap()
        .unwrap();
    let statics = c.allocate_static_state(Default::default()).unwrap();
    let exceptions = c.prepare_exception_profile(throwable).unwrap();
    let claim = c
        .claim_initialization(&statics, &exceptions, error)
        .unwrap();
    (claim, statics)
}
#[test]
fn fresh_values_preserve_every_unit_and_never_pool_or_fake_backing_fields() {
    let text = [0, 0x20ac, 0xd83d, 0xde80, 0xd800, 65, 0xdc00];
    let mut f = fixture(Default::default());
    let string = f.string;
    let mut c = context(&mut f);
    let a = c.fresh_string(&text).unwrap();
    let b = c.fresh_string(&text).unwrap();
    let e = c.fresh_string(&[]).unwrap();
    let e2 = c.fresh_string(&[]).unwrap();
    assert_ne!(a, b);
    assert_ne!(e, e2);
    assert_eq!(c.heap().string_units(a).unwrap(), Some(text.as_slice()));
    assert_eq!(
        c.reference_type(a).unwrap(),
        Some(ReferenceType::Class(string))
    );
    assert_eq!(c.heap().children(a).unwrap().count(), 0);
    assert_eq!(
        c.heap().string_usage(),
        StringUsage {
            objects: 4,
            retained_units: 14,
            interned: 0,
            literal_sites: 0,
            work_units: 18
        }
    );
    assert_eq!(c.heap().string_roots().len(), 0);
    assert_eq!(c.heap().allocated_instances(), 4);
    assert_eq!(c.heap().total_instance_slots(), 0);
    assert_eq!(c.heap().allocated_arrays(), 0);
}
#[test]
fn exact_work_formulas_and_receiver_first_canonicality_are_observable() {
    let mut f = fixture(Default::default());
    let d = f.caller;
    let sites = f.sites.clone();
    let mut c = context(&mut f);
    let a = c.fresh_string(&[65]).unwrap();
    assert_eq!(c.heap().string_usage().work_units, 2);
    let old = c.heap().string_usage();
    assert_eq!(c.intern_string(a).unwrap(), a);
    assert_eq!(c.heap().string_usage().work_units - old.work_units, 4);
    let old = c.heap().string_usage();
    assert_eq!(c.intern_string(a).unwrap(), a);
    assert_eq!(c.heap().string_usage().work_units - old.work_units, 1);
    let b = c.fresh_string(&[65]).unwrap();
    assert_ne!(a, b);
    let old = c.heap().string_usage();
    assert_eq!(c.intern_string(b).unwrap(), a);
    assert_eq!(c.heap().string_usage().work_units - old.work_units, 5);
    assert_eq!(
        payload(&c.heap().entries, c.heap().identity, b)
            .unwrap()
            .canonical,
        None
    );
    let old = c.heap().string_usage();
    assert_eq!(c.string_literal(d, sites[0]).unwrap(), a);
    assert_eq!(c.heap().string_usage().work_units - old.work_units, 6);
    let old = c.heap().string_usage();
    assert_eq!(c.string_literal(d, sites[0]).unwrap(), a);
    assert_eq!(c.heap().string_usage().work_units - old.work_units, 1);
    let old = c.heap().string_usage();
    let different = c.string_literal(d, sites[2]).unwrap();
    assert_ne!(a, different);
    assert_eq!(c.heap().string_usage().work_units - old.work_units, 7);
    assert_eq!(
        c.heap().string_roots().collect::<Vec<_>>(),
        [a.heap_ref().unwrap(), different.heap_ref().unwrap()]
    );
}
#[test]
fn empty_and_long_modified_utf8_literals_are_heap_local_and_warm_cost_one() {
    let long = vec![65; 65_535];
    let mixed = [0, 0x20ac, 0xd800, 0xdc00, 0xdfff];
    let mut f = fixture_with(Default::default(), &[&long, &long, &mixed, &[]], 0x101);
    let d = f.caller;
    let sites = f.sites.clone();
    let mut c = context(&mut f);
    let a = c.string_literal(d, sites[0]).unwrap();
    assert_eq!(c.heap().string_units(a).unwrap().unwrap().len(), 65_535);
    let old = c.heap().string_usage().work_units;
    assert_eq!(c.string_literal(d, sites[1]).unwrap(), a);
    assert_eq!(c.heap().string_usage().work_units - old, 2 * 65_535 + 4);
    let old = c.heap().string_usage().work_units;
    for _ in 0..10_000 {
        assert_eq!(c.string_literal(d, sites[0]).unwrap(), a);
    }
    assert_eq!(c.heap().string_usage().work_units - old, 10_000);
    let r = c.string_literal(d, sites[2]).unwrap();
    assert_eq!(c.heap().string_units(r).unwrap(), Some(mixed.as_slice()));
    let e = c.string_literal(d, sites[3]).unwrap();
    assert_eq!(c.heap().string_units(e).unwrap(), Some(&[][..]));
}
#[test]
fn collisions_compare_all_equal_length_contents_and_keep_ordered_roots() {
    let mut f = fixture(Default::default());
    let d = f.caller;
    let sites = f.sites.clone();
    f.h.strings.as_mut().unwrap().force_fingerprint = Some(0);
    let mut c = context(&mut f);
    let a = c.string_literal(d, sites[0]).unwrap();
    let old = c.heap().string_usage().work_units;
    let b = c.string_literal(d, sites[2]).unwrap();
    assert_ne!(a, b);
    assert_eq!(c.heap().string_usage().work_units - old, 9);
    assert_eq!(c.string_literal(d, sites[1]).unwrap(), a);
    assert_eq!(
        c.heap().string_roots().collect::<Vec<_>>(),
        [a.heap_ref().unwrap(), b.heap_ref().unwrap()]
    );
}
#[test]
fn all_fresh_and_literal_reservation_failpoints_preserve_nonempty_pool_and_recover() {
    // Cold insertion into an empty pool has six distinct reserve points.
    for fail in 0..6 {
        let mut f = fixture(Default::default());
        let d = f.caller;
        let i = f.sites[0];
        f.h.strings.as_mut().unwrap().fail_reserve_after = Some(fail);
        let mut c = context(&mut f);
        assert_eq!(
            *c.string_literal(d, i).unwrap_err(),
            StringError::AllocationFailure
        );
        let u = c.heap().string_usage();
        assert_eq!(
            (u.objects, u.retained_units, u.interned, u.literal_sites),
            (0, 0, 0, 0)
        );
        assert_eq!(c.heap().entries.len(), 0);
        c.heap_mut().strings.as_mut().unwrap().fail_reserve_after = None;
        c.string_literal(d, i).unwrap();
    }
    // Forced collision uses an existing bucket and a nonempty canonical prefix.
    for fail in 0..5 {
        let mut f = fixture(Default::default());
        f.h.strings.as_mut().unwrap().force_fingerprint = Some(1);
        let d = f.caller;
        let sites = f.sites.clone();
        let mut c = context(&mut f);
        let a = c.string_literal(d, sites[0]).unwrap();
        let old = c.heap().string_usage();
        let roots = c.heap().string_roots().collect::<Vec<_>>();
        c.heap_mut().strings.as_mut().unwrap().fail_reserve_after = Some(fail);
        assert_eq!(
            *c.string_literal(d, sites[2]).unwrap_err(),
            StringError::AllocationFailure
        );
        let u = c.heap().string_usage();
        assert_eq!(
            (u.objects, u.retained_units, u.interned, u.literal_sites),
            (
                old.objects,
                old.retained_units,
                old.interned,
                old.literal_sites
            )
        );
        assert_eq!(c.heap().string_roots().collect::<Vec<_>>(), roots);
        assert_eq!(c.string_literal(d, sites[0]).unwrap(), a);
        c.heap_mut().strings.as_mut().unwrap().fail_reserve_after = None;
        c.string_literal(d, sites[2]).unwrap();
    }
}
#[test]
fn limits_zero_exact_one_less_and_max_keep_counts_and_accepted_work() {
    for (limits, resource) in [
        (
            StringLimits {
                max_objects: 0,
                ..Default::default()
            },
            StringResource::Objects,
        ),
        (
            StringLimits {
                max_retained_units: 0,
                ..Default::default()
            },
            StringResource::RetainedUnits,
        ),
        (
            StringLimits {
                max_interned: 0,
                ..Default::default()
            },
            StringResource::Interned,
        ),
        (
            StringLimits {
                max_literal_sites: 0,
                ..Default::default()
            },
            StringResource::LiteralSites,
        ),
        (
            StringLimits {
                max_work_units: 6,
                ..Default::default()
            },
            StringResource::WorkUnits,
        ),
    ] {
        let mut f = fixture(limits);
        let d = f.caller;
        let site = f.sites[0];
        assert!(
            matches!(*context(&mut f).string_literal(d,site).unwrap_err(),StringError::Limit{resource:r,..} if r==resource)
        );
        let u = f.h.string_usage();
        assert_eq!(
            (u.objects, u.retained_units, u.interned, u.literal_sites),
            (0, 0, 0, 0)
        );
    }
    let mut f = fixture(StringLimits {
        max_objects: 1,
        max_retained_units: 1,
        max_interned: 1,
        max_literal_sites: 1,
        max_work_units: 8,
    });
    let d = f.caller;
    let i = f.sites[0];
    let mut c = context(&mut f);
    let r = c.string_literal(d, i).unwrap();
    assert_eq!(c.heap().string_usage().work_units, 7);
    assert_eq!(c.string_literal(d, i).unwrap(), r);
    assert_eq!(c.heap().string_usage().work_units, 8);
    assert!(c.string_literal(d, i).is_err());
    let mut f = fixture(StringLimits {
        max_objects: u64::MAX,
        max_retained_units: u64::MAX,
        max_interned: u64::MAX,
        max_literal_sites: u64::MAX,
        max_work_units: u64::MAX,
    });
    assert!(context(&mut f).fresh_string(&[0]).is_ok());
}
#[test]
fn native_and_opaque_guards_have_no_heap_work_and_exact_classifier_field_cost() {
    let mut f = fixture(Default::default());
    let native = f.native;
    let body = f.body;
    let string = f.string;
    let value = f.g.field(string, "value", "I");
    let mut c = context(&mut f);
    let r = c.fresh_string(&[65]).unwrap();
    let old = c.lookup_steps();
    assert!(c.is_string_intern(native).unwrap());
    assert_eq!(c.lookup_steps() - old, 1);
    let old = c.lookup_steps();
    c.check_string_method_entry(native, r).unwrap();
    assert_eq!(c.lookup_steps(), old);
    assert_eq!(
        c.check_method_reference_profile(body, Some(r)),
        Err(MethodAccessError::Context(FieldAccessError::Instance(
            InstanceError::OpaqueStringObject { class: string }
        )))
    );
    assert_eq!(
        c.heap().get_field(r, value),
        Err(InstanceError::OpaqueStringObject { class: string })
    );
    assert_eq!(
        c.heap_mut().allocate_instance(string),
        Err(InstanceError::ReservedStringObject { class: string })
    );
    assert_eq!(c.heap().string_usage().work_units, 2);
    for flags in [1, 0x109, 0x121, 0x104] {
        let mut f = fixture_with(Default::default(), &[], flags);
        let m = f.native;
        let mut c = context(&mut f);
        let r = c.fresh_string(&[]).unwrap();
        assert!(
            matches!(c.check_string_method_entry(m,r),Err(MethodAccessError::String(e)) if matches!(*e,StringError::InvalidBinding(_)))
        );
    }
}
#[test]
fn class_constant_install_stages_all_fields_but_retains_paid_string_prefix_on_failure() {
    let mut f = fixture(StringLimits {
        max_interned: 1,
        ..Default::default()
    });
    let boot = f.g.boot;
    let class = f.constants;
    let fields = f.fields;
    let mut c = context(&mut f);
    let (claim, statics) = claim(&mut c, boot);
    assert!(matches!(
        c.plan_initialization(
            &claim,
            class,
            InitializationPlanLimits {
                max_dependencies: 100,
                max_constants: 100
            }
        ),
        Err(InitializationAccessError::UnsupportedStringConstant { .. })
    ));
    let plan = c
        .plan_initialization_with_strings(
            &claim,
            class,
            InitializationPlanLimits {
                max_dependencies: 100,
                max_constants: 100,
            },
        )
        .unwrap();
    assert_eq!(c.heap().string_usage(), StringUsage::default());
    assert!(
        matches!(c.install_initialization_constants(&claim,&plan),Err(InitializationAccessError::String(e)) if matches!(*e,StringError::Limit{resource:StringResource::Interned,..}))
    );
    assert_eq!(c.heap().string_usage().interned, 1);
    assert_eq!(c.heap().string_usage().literal_sites, 1);
    assert_eq!(
        c.inspect_static(&statics, fields[0]).unwrap(),
        Value::Int(0)
    );
    for field in &fields[1..] {
        assert_eq!(
            c.inspect_static(&statics, *field).unwrap(),
            Value::Reference(Reference::null())
        );
    }
}
#[test]
fn successful_constant_install_uses_canonical_refs_and_nonfinal_constant_values() {
    let mut f = fixture(Default::default());
    let boot = f.g.boot;
    let class = f.constants;
    let fields = f.fields;
    let cv = f.cv_sites;
    let mut c = context(&mut f);
    let (claim, statics) = claim(&mut c, boot);
    let plan = c
        .plan_initialization_with_strings(
            &claim,
            class,
            InitializationPlanLimits {
                max_dependencies: 100,
                max_constants: 100,
            },
        )
        .unwrap();
    c.install_initialization_constants(&claim, &plan).unwrap();
    assert_eq!(
        c.inspect_static(&statics, fields[0]).unwrap(),
        Value::Int(7)
    );
    for (field, site) in fields[1..].iter().zip(cv) {
        let r = c.string_literal(class, site).unwrap();
        assert_eq!(
            c.inspect_static(&statics, *field).unwrap(),
            Value::Reference(r)
        );
    }
    assert_eq!(c.heap().string_usage().objects, 2);
    assert_eq!(c.static_roots(&statics).unwrap().count(), 2);
}

#[test]
fn foreign_stale_and_wrong_payload_handles_are_never_interned_or_retyped() {
    let mut f = fixture(Default::default());
    let mut other = fixture(Default::default());
    let foreign = context(&mut other).fresh_string(&[65]).unwrap();
    let caller = f.caller;
    let mut c = context(&mut f);
    let object = c.heap_mut().allocate_instance(caller).unwrap();
    assert_eq!(c.heap().string_units(Reference::null()).unwrap(), None);
    assert_eq!(c.heap().string_units(object).unwrap(), None);
    for r in [Reference::null(), object] {
        assert_eq!(
            *c.intern_string(r).unwrap_err(),
            StringError::InvalidReceiver
        );
    }
    assert!(
        matches!(*c.intern_string(foreign).unwrap_err(),StringError::Heap(e) if *e==HeapError::ForeignHeap)
    );
    let good = c.fresh_string(&[65]).unwrap();
    let (heap, index, generation) = good.heap_ref().unwrap().components();
    let before = c.heap().string_usage();
    for r in [
        Reference::issued(heap, index + 100, generation),
        Reference::issued(heap, index, generation + 1),
        Reference::issued(heap + 1, index, generation),
    ] {
        assert!(c.intern_string(r).is_err());
        assert!(c.heap().string_units(r).is_err());
    }
    assert_eq!(c.heap().string_usage(), before);
}
#[test]
fn private_canonical_and_literal_corruption_fails_without_mutating_unrelated_state() {
    for damage in 0..3 {
        let mut f = fixture(Default::default());
        let d = f.caller;
        let site = f.sites[0];
        let mut c = context(&mut f);
        let r = c.string_literal(d, site).unwrap();
        let (_, index, _) = r.heap_ref().unwrap().components();
        let before = c.heap().string_usage();
        match damage {
            0 => {
                let EntryPayload::String(p) = &mut c.heap_mut().entries[index].payload else {
                    panic!()
                };
                p.canonical = Some(usize::MAX);
            }
            1 => {
                c.heap_mut()
                    .strings
                    .as_mut()
                    .unwrap()
                    .sites
                    .get_mut(&(d, site))
                    .unwrap()
                    .handle = Reference::issued(999, 0, 1).heap_ref().unwrap();
            }
            _ => {
                c.heap_mut().strings.as_mut().unwrap().usage.literal_sites = 999;
            }
        }
        assert!(c.string_literal(d, site).is_err());
        assert_eq!(c.heap().string_usage().objects, before.objects);
        assert_eq!(c.heap().string_usage().interned, before.interned);
        assert_eq!(c.heap().entries.len(), 1);
    }
}
#[test]
fn overflow_funding_is_atomic_and_machine_max_is_not_an_allocation_request() {
    for (old, add, resource) in [
        (u64::MAX, 1, StringResource::Objects),
        (u64::MAX - 1, 2, StringResource::WorkUnits),
    ] {
        assert!(
            matches!(*increased(old,add,u64::MAX,resource).unwrap_err(),StringError::Overflow{resource:r} if r==resource)
        );
    }
    let mut f = fixture(StringLimits {
        max_work_units: u64::MAX,
        ..Default::default()
    });
    f.h.strings.as_mut().unwrap().usage.work_units = u64::MAX;
    let d = f.caller;
    let site = f.sites[0];
    assert!(matches!(
        *context(&mut f).string_literal(d, site).unwrap_err(),
        StringError::Overflow {
            resource: StringResource::WorkUnits
        }
    ));
    assert_eq!(f.h.string_usage().work_units, u64::MAX);
    assert_eq!(f.h.entries.len(), 0);
}
#[test]
fn wrong_constant_tag_and_foreign_origins_do_not_spend_string_work() {
    let mut f = fixture(Default::default());
    let d = f.caller;
    let mut other = fixture(Default::default());
    let foreign = other.caller;
    let mut c = context(&mut f);
    for i in [0, u16::MAX] {
        assert_eq!(
            *c.string_literal(d, i).unwrap_err(),
            StringError::InvalidLiteral {
                origin: d,
                index: i
            }
        );
    }
    assert!(matches!(
        *c.string_literal(foreign, other.sites[0]).unwrap_err(),
        StringError::Registry(_)
    ));
    assert_eq!(c.heap().string_usage(), StringUsage::default());
    // Mere text and another defining loader never grant a String role.
    let boot = other.g.boot;
    assert!(
        other
            .h
            .bind_strings(other.g.domain.registry(), other.caller, Default::default())
            .is_err()
    );
    assert!(boot != f.g.boot);
}
#[test]
fn binding_is_pristine_idempotent_and_does_not_open_legacy_representation() {
    let mut f = fixture(Default::default());
    f.h.bind_strings(f.g.domain.registry(), f.string, Default::default())
        .unwrap();
    let string = f.string;
    let r = context(&mut f).fresh_string(&[]).unwrap();
    f.h.bind_strings(f.g.domain.registry(), string, Default::default())
        .unwrap();
    assert_eq!(
        *f.h.bind_strings(
            f.g.domain.registry(),
            string,
            StringLimits {
                max_objects: 1,
                ..Default::default()
            }
        )
        .unwrap_err(),
        StringError::PolicyConflict
    );
    let mut legacy = f.g.heap(&f.g.classes);
    let u = f.h.reference_type_universe().unwrap();
    legacy
        .bind_reference_types(f.g.domain.registry(), u)
        .unwrap();
    legacy
        .bind_class_mirrors(
            f.g.domain.registry(),
            f.h.mirrors.as_ref().unwrap().class,
            16_384,
        )
        .unwrap();
    let ordinary = legacy.allocate_instance(string).unwrap();
    assert_eq!(legacy.string_units(ordinary).unwrap(), None);
    assert!(matches!(
        *legacy
            .bind_strings(f.g.domain.registry(), string, Default::default())
            .unwrap_err(),
        StringError::InvalidBinding(_)
    ));
    assert_eq!(legacy.string_usage(), StringUsage::default());
    assert!(legacy.string_units(r).is_err());
}
#[test]
fn another_loader_same_text_joins_pool_but_wrong_field_string_identity_fails_preflight() {
    let mut f = fixture(Default::default());
    let loader = f.g.loader();
    let mut c = Class::plain("other/Caller");
    let site = text_cp(&mut c, &[65]);
    let other = f.g.add(loader, c);
    let fake = f.g.add(
        loader,
        Class::new("java/lang/String", Some("java/lang/Object"), 0x31),
    );
    let mut constants = Class::plain("other/Constants");
    constants.field("s", "Ljava/lang/String;", 25);
    let cv = text_cp(&mut constants, &[66]);
    let bytes = with_values(&mut constants, &[cv]);
    let bad =
        f.g.domain
            .context(LoadingWorkLimits::default())
            .define(
                loader,
                &units(&constants.name),
                &bytes,
                ParseOptions::default(),
            )
            .unwrap();
    f.g.classes.push(bad);
    f.g.intended.insert((loader, units(&constants.name)), bad);
    f.g.prepare(&[other, fake, bad]);
    let u = f.h.reference_type_universe().unwrap();
    let mirror = f.h.mirrors.as_ref().unwrap().class;
    f.h = f.g.heap(&f.g.classes);
    f.h.bind_reference_types(f.g.domain.registry(), u).unwrap();
    f.h.bind_class_mirrors(f.g.domain.registry(), mirror, 16_384)
        .unwrap();
    f.h.bind_strings(f.g.domain.registry(), f.string, Default::default())
        .unwrap();
    let own = f.caller;
    let ownsite = f.sites[0];
    let boot = f.g.boot;
    let mut c = context(&mut f);
    let a = c.string_literal(own, ownsite).unwrap();
    assert_eq!(c.string_literal(other, site).unwrap(), a);
    let old = c.heap().string_usage();
    let (claim, _) = claim(&mut c, boot);
    assert!(
        matches!(c.plan_initialization_with_strings(&claim,bad,InitializationPlanLimits{max_dependencies:100,max_constants:100}),Err(InitializationAccessError::String(e)) if matches!(*e,StringError::InvalidBinding(_)))
    );
    assert_eq!(c.heap().string_usage(), old);
}
#[test]
fn wrapped_diagnostics_preserve_all_sources_and_never_synthesize_java_categories() {
    let errors = [
        StringError::Registry(RegistryError::InvalidName("x")),
        StringError::Instance(InstanceError::Disabled),
        StringError::Heap(Box::new(HeapError::ForeignHeap)),
        StringError::Context(Box::new(FieldAccessError::ContextChanged)),
    ];
    for e in errors {
        assert!(std::error::Error::source(&e).is_some());
        assert!(!e.to_string().is_empty());
        let wrapped = MethodAccessError::String(Box::new(e));
        assert!(std::error::Error::source(&wrapped).is_some());
    }
}

#[test]
fn intern_reservations_never_replace_or_prematurely_canonicalize_the_receiver() {
    for fail in 0..3 {
        let mut f = fixture(Default::default());
        let mut c = context(&mut f);
        let r = c.fresh_string(&[65]).unwrap();
        let usage = c.heap().string_usage();
        c.heap_mut().strings.as_mut().unwrap().fail_reserve_after = Some(fail);
        assert_eq!(
            *c.intern_string(r).unwrap_err(),
            StringError::AllocationFailure
        );
        let now = c.heap().string_usage();
        assert_eq!(
            (now.objects, now.retained_units, now.interned),
            (usage.objects, usage.retained_units, 0)
        );
        assert_eq!(
            payload(&c.heap().entries, c.heap().identity, r)
                .unwrap()
                .canonical,
            None
        );
        c.heap_mut().strings.as_mut().unwrap().fail_reserve_after = None;
        assert_eq!(c.intern_string(r).unwrap(), r);
        assert_eq!(c.heap().string_roots().next(), r.heap_ref());
    }
    for fail in 0..2 {
        let mut f = fixture(Default::default());
        f.h.strings.as_mut().unwrap().force_fingerprint = Some(1);
        let mut c = context(&mut f);
        let old = c.fresh_string(&[65]).unwrap();
        c.intern_string(old).unwrap();
        let r = c.fresh_string(&[66]).unwrap();
        c.heap_mut().strings.as_mut().unwrap().fail_reserve_after = Some(fail);
        assert_eq!(
            *c.intern_string(r).unwrap_err(),
            StringError::AllocationFailure
        );
        assert_eq!(
            c.heap().string_roots().collect::<Vec<_>>(),
            [old.heap_ref().unwrap()]
        );
        assert_eq!(
            payload(&c.heap().entries, c.heap().identity, r)
                .unwrap()
                .canonical,
            None
        );
        c.heap_mut().strings.as_mut().unwrap().fail_reserve_after = None;
        assert_eq!(c.intern_string(r).unwrap(), r);
    }
}
#[test]
fn context_replacement_with_the_same_layout_count_cannot_reuse_string_authority() {
    let mut f = fixture(Default::default());
    let mut replacement = f.g.heap(&f.g.classes);
    let u = f.h.reference_type_universe().unwrap();
    replacement
        .bind_reference_types(f.g.domain.registry(), u)
        .unwrap();
    replacement
        .bind_class_mirrors(
            f.g.domain.registry(),
            f.h.mirrors.as_ref().unwrap().class,
            16_384,
        )
        .unwrap();
    replacement
        .bind_strings(f.g.domain.registry(), f.string, Default::default())
        .unwrap();
    assert_eq!(
        replacement.retained_layout_classes(),
        f.h.retained_layout_classes()
    );
    let mut c = context(&mut f);
    *c.heap_mut() = replacement;
    assert!(
        matches!(*c.fresh_string(&[65]).unwrap_err(),StringError::Context(e) if *e==FieldAccessError::ContextChanged)
    );
    assert_eq!(c.heap().string_usage(), StringUsage::default());
}

#[test]
fn two_compatible_heaps_share_metadata_but_never_literal_handles_roots_or_work() {
    let mut f = fixture(Default::default());
    let d = f.caller;
    let site = f.sites[0];
    let a = context(&mut f).string_literal(d, site).unwrap();
    let first = f.h.string_usage();
    let u = f.h.reference_type_universe().unwrap();
    let mirror = f.h.mirrors.as_ref().unwrap().class;
    let mut second = f.g.heap(&f.g.classes);
    second
        .bind_reference_types(f.g.domain.registry(), u)
        .unwrap();
    second
        .bind_class_mirrors(f.g.domain.registry(), mirror, 16_384)
        .unwrap();
    second
        .bind_strings(f.g.domain.registry(), f.string, Default::default())
        .unwrap();
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(&mut second, Default::default(), Default::default())
            .unwrap();
    let b = c.string_literal(d, site).unwrap();
    assert_ne!(a, b);
    assert_eq!(c.heap().string_usage(), first);
    assert_eq!(
        c.heap().string_roots().collect::<Vec<_>>(),
        [b.heap_ref().unwrap()]
    );
    assert!(c.heap().string_units(a).is_err());
    assert_eq!(
        f.h.string_roots().collect::<Vec<_>>(),
        [a.heap_ref().unwrap()]
    );
    assert_eq!(f.h.string_usage(), first);
}
#[test]
fn missing_owner_loader_string_initiation_fails_planning_without_derivation() {
    let mut f = fixture(Default::default());
    let loader = f.g.loader();
    let mut constants = Class::plain("missing/Constants");
    constants.field("s", "Ljava/lang/String;", 25);
    let cv = text_cp(&mut constants, &[65]);
    let bytes = with_values(&mut constants, &[cv]);
    let id =
        f.g.domain
            .context(LoadingWorkLimits::default())
            .define(
                loader,
                &units(&constants.name),
                &bytes,
                ParseOptions::default(),
            )
            .unwrap();
    f.g.classes.push(id);
    f.g.intended.insert((loader, units(&constants.name)), id);
    f.g.prepare(&[id]);
    let u = f.h.reference_type_universe().unwrap();
    let mirror = f.h.mirrors.as_ref().unwrap().class;
    f.h = f.g.heap(&f.g.classes);
    f.h.bind_reference_types(f.g.domain.registry(), u).unwrap();
    f.h.bind_class_mirrors(f.g.domain.registry(), mirror, 16_384)
        .unwrap();
    f.h.bind_strings(f.g.domain.registry(), f.string, Default::default())
        .unwrap();
    let boot = f.g.boot;
    let mut c = context(&mut f);
    let (claim, _) = claim(&mut c, boot);
    assert!(matches!(
        c.plan_initialization_with_strings(
            &claim,
            id,
            InitializationPlanLimits {
                max_dependencies: 100,
                max_constants: 100
            }
        ),
        Err(InitializationAccessError::String(_))
    ));
    assert_eq!(c.heap().string_usage(), StringUsage::default());
}
#[test]
fn literal_site_scalar_poison_cannot_substitute_a_valid_different_canonical_value() {
    for damage in 0..5 {
        let mut f = fixture(Default::default());
        let d = f.caller;
        let sites = f.sites.clone();
        let mut c = context(&mut f);
        let a = c.string_literal(d, sites[0]).unwrap();
        let b = c.string_literal(d, sites[2]).unwrap();
        let row = c
            .heap_mut()
            .strings
            .as_mut()
            .unwrap()
            .sites
            .get_mut(&(d, sites[0]))
            .unwrap();
        match damage {
            0 => row.handle = b.heap_ref().unwrap(),
            1 => row.root = usize::MAX,
            2 => row.utf8 = 0,
            3 => row.index = sites[2],
            _ => {
                row.origin = ClassId {
                    registry: d.registry,
                    index: usize::MAX,
                }
            }
        };
        let old = c.heap().string_usage();
        assert!(c.string_literal(d, sites[0]).is_err());
        let now = c.heap().string_usage();
        assert_eq!(now.work_units - old.work_units, 1);
        assert_eq!(
            (now.objects, now.interned, now.literal_sites),
            (old.objects, old.interned, old.literal_sites)
        );
        assert_eq!(c.heap().string_units(a).unwrap(), Some(&[65][..]));
    }
}

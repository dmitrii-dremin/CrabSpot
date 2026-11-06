use super::*;
use crate::mirror_support::{Class, Graph, units, world};
use crabspot_classfile::ParseOptions;
fn add(g: &mut Graph, mut c: Class, host: Option<&str>, members: Option<&[&str]>) -> ClassId {
    let mut attrs = Vec::<(u16, Vec<u8>)>::new();
    if let Some(h) = host {
        let n = c.utf("NestHost");
        let i = c.class(h);
        attrs.push((n, i.to_be_bytes().to_vec()));
    }
    if let Some(names) = members {
        let n = c.utf("NestMembers");
        let mut b = (names.len() as u16).to_be_bytes().to_vec();
        for name in names {
            b.extend(c.class(name).to_be_bytes());
        }
        attrs.push((n, b));
    }
    let mut bytes = c.bytes();
    bytes.truncate(bytes.len() - 2);
    bytes.extend((attrs.len() as u16).to_be_bytes());
    for (n, b) in attrs {
        bytes.extend(n.to_be_bytes());
        bytes.extend((b.len() as u32).to_be_bytes());
        bytes.extend(b);
    }
    let id = g
        .domain
        .context(Default::default())
        .define(g.boot, &units(&c.name), &bytes, ParseOptions::default())
        .unwrap();
    g.classes.push(id);
    g.intended.insert((g.boot, units(&c.name)), id);
    id
}
fn instr(op: u8, index: u16) -> Vec<u8> {
    vec![op, (index >> 8) as u8, index as u8]
}
struct Fixture {
    g: Graph,
    h: ManagedHeap,
    string: ClassId,
    plan: CompactStringBodyPlan,
    value: u16,
    coder: u16,
    plain: FieldId,
}
fn fixture(limits: StringBodyLimits, shift: usize) -> Fixture {
    let w = world(Default::default());
    let mut g = w.g;
    let boot = g.boot;
    let interface = g.add(
        boot,
        Class::new("java/util/Comparator", Some("java/lang/Object"), 0x601),
    );
    let stream = g.add(boot, Class::plain("java/io/ObjectStreamField"));
    let mut cmp = Class::new(
        "java/lang/String$CaseInsensitiveComparator",
        Some("java/lang/Object"),
        0x20,
    );
    cmp.interface("java/util/Comparator");
    let call = cmp.reference(10, "java/lang/Object", "<init>", "()V");
    let mut code = vec![0x2a];
    code.extend(instr(0xb7, call));
    code.push(0xb1);
    cmp.method("<init>", "()V", 2, &code, 1, 1);
    let cmp = add(&mut g, cmp, Some("java/lang/String"), None);
    let mut s = Class::new("java/lang/String", Some("java/lang/Object"), 0x31);
    for _ in 0..shift {
        s.utf("shift");
    }
    for (n, d, f) in [
        ("value", "[B", 18),
        ("coder", "B", 18),
        ("COMPACT_STRINGS", "Z", 24),
        ("serialPersistentFields", "[Ljava/io/ObjectStreamField;", 26),
        ("CASE_INSENSITIVE_ORDER", "Ljava/util/Comparator;", 25),
    ] {
        s.field(n, d, f);
    }
    let value = s.reference(9, "java/lang/String", "value", "[B");
    let coder = s.reference(9, "java/lang/String", "coder", "B");
    let compact = s.reference(9, "java/lang/String", "COMPACT_STRINGS", "Z");
    let serial = s.reference(
        9,
        "java/lang/String",
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
    );
    let order = s.reference(
        9,
        "java/lang/String",
        "CASE_INSENSITIVE_ORDER",
        "Ljava/util/Comparator;",
    );
    let coder_method = s.reference(10, "java/lang/String", "coder", "()B");
    let constructor = s.reference(
        10,
        "java/lang/String$CaseInsensitiveComparator",
        "<init>",
        "()V",
    );
    let cmp_class = s.class("java/lang/String$CaseInsensitiveComparator");
    let stream_class = s.class("java/io/ObjectStreamField");
    let mut length = vec![0x2a];
    length.extend(instr(0xb4, value));
    length.extend([0xbe, 0x2a]);
    length.extend(instr(0xb6, coder_method));
    length.extend([0x7a, 0xac]);
    s.method("length", "()I", 1, &length, 2, 1);
    let mut empty = vec![0x2a];
    empty.extend(instr(0xb4, value));
    empty.extend([0xbe, 0x9a, 0, 7, 4, 0xa7, 0, 4, 3, 0xac]);
    s.method("isEmpty", "()Z", 1, &empty, 1, 1);
    let mut code = instr(0xb2, compact);
    code.extend([0x99, 0, 10, 0x2a]);
    code.extend(instr(0xb4, coder));
    code.extend([0xa7, 0, 4, 4, 0xac]);
    s.method("coder", "()B", 0, &code, 1, 1);
    let mut init = vec![4];
    init.extend(instr(0xb3, compact));
    init.push(3);
    init.extend(instr(0xbd, stream_class));
    init.extend(instr(0xb3, serial));
    init.extend(instr(0xbb, cmp_class));
    init.push(0x59);
    init.extend(instr(0xb7, constructor));
    init.extend(instr(0xb3, order));
    init.push(0xb1);
    s.method("<clinit>", "()V", 8, &init, 2, 0);
    s.method("intern", "()Ljava/lang/String;", 0x101, &[], 0, 1);
    s.method("charAt", "(I)C", 1, &[3, 0xac], 1, 2);
    let string = add(
        &mut g,
        s,
        None,
        Some(&["java/lang/String$CaseInsensitiveComparator"]),
    );
    g.prepare(&[interface, stream, cmp, string]);
    let probe =
        probe_compact_string_bodies(g.domain.registry(), boot, string, Default::default()).unwrap();
    let plan = probe.plan.unwrap();
    let mut h = g.heap(&g.classes);
    h.bind_reference_types(g.domain.registry(), w.u).unwrap();
    h.bind_class_mirrors(g.domain.registry(), w.class, 16_384)
        .unwrap();
    h.bind_strings(g.domain.registry(), string, Default::default())
        .unwrap();
    h.bind_compact_string_bodies(g.domain.registry(), &plan, limits)
        .unwrap();
    g.domain.enable_nest_access().unwrap();
    let plain = g.field(string, "value", "[B");
    Fixture {
        g,
        h,
        string,
        plan,
        value,
        coder,
        plain,
    }
}
fn ctx(f: &mut Fixture) -> FieldResolutionContext<'_> {
    f.g.domain
        .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
        .unwrap()
}
#[test]
fn no_initializer_discovery_has_hand_derived_nineteen_work_and_atomic_name_charge() {
    let mut g = Graph::new();
    let s = g.add(
        g.boot,
        Class::new("java/lang/String", Some("java/lang/Object"), 0x31),
    );
    let d = probe_compact_string_bodies(
        g.domain.registry(),
        g.boot,
        s,
        StringBodyDiscoveryLimits { max_work_units: 19 },
    )
    .unwrap();
    assert!(d.plan.is_none());
    assert_eq!(d.usage.work_units, 19);
    let e = probe_compact_string_bodies(
        g.domain.registry(),
        g.boot,
        s,
        StringBodyDiscoveryLimits { max_work_units: 18 },
    )
    .unwrap_err();
    assert_eq!(e.discovery_usage.work_units, 3);
    assert_eq!(
        e.kind,
        StringBodyErrorKind::Limit {
            resource: StringBodyResource::DiscoveryWork,
            limit: 18,
            required: 19
        }
    );
}
#[test]
fn shifted_cp_recipe_binds_exact_roles_without_body_or_string_work() {
    for shift in [0, 300] {
        let f = fixture(Default::default(), shift);
        assert_eq!(f.plan.private_roots().len(), 3);
        assert_eq!(f.h.string_body_usage(), StringBodyUsage::default());
        assert_eq!(f.h.string_usage(), StringUsage::default());
        assert_eq!(f.h.allocated_arrays(), 0);
        let usage = probe_compact_string_bodies(
            f.g.domain.registry(),
            f.g.boot,
            f.string,
            Default::default(),
        )
        .unwrap()
        .usage;
        let e = probe_compact_string_bodies(
            f.g.domain.registry(),
            f.g.boot,
            f.string,
            StringBodyDiscoveryLimits {
                max_work_units: usage.work_units - 1,
            },
        )
        .unwrap_err();
        assert!(matches!(
            e.kind,
            StringBodyErrorKind::Limit {
                resource: StringBodyResource::DiscoveryWork,
                ..
            }
        ));
    }
}
#[test]
fn actual_byte_arrays_encode_latin_utf16_and_empty_with_exact_cold_and_warm_cost() {
    let mut f = fixture(Default::default(), 0);
    let string = f.string;
    let value = f.value;
    let coder = f.coder;
    let plain = f.plain;
    let mut c = ctx(&mut f);
    for (text, bytes, code) in [
        (vec![], vec![], 0),
        (vec![65, 0, 255], vec![65, 0, 255], 0),
        (
            vec![65, 0x20ac, 0xd83d, 0xde80],
            vec![65, 0, 0xac, 0x20, 0x3d, 0xd8, 0x80, 0xde],
            1,
        ),
        (vec![0xdfff], vec![255, 223], 1),
    ] {
        let r = c.fresh_string(&text).unwrap();
        assert_eq!(c.heap().string_backing(r).unwrap(), None);
        let initial = c.heap().string_usage();
        let before = c.heap().string_body_usage();
        let result = c.read_field(string, value, r).unwrap();
        let Value::Reference(array) = result else {
            panic!()
        };
        assert_eq!(c.heap().string_usage(), initial);
        assert_eq!(c.heap().length(array).unwrap(), bytes.len() as i32);
        assert_eq!(c.heap().kind(array).unwrap(), PrimitiveArrayKind::Byte);
        let actual = (0..bytes.len())
            .map(|i| match c.heap().load(array, i as i32).unwrap() {
                Value::Int(x) => x as u8,
                _ => panic!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, bytes);
        assert_eq!(
            c.heap().string_body_usage().work_units - before.work_units,
            1 + text.len() as u64 + bytes.len() as u64
        );
        assert_eq!(
            c.heap().children(r).unwrap().collect::<Vec<_>>(),
            [array.heap_ref().unwrap()]
        );
        assert_eq!(
            c.heap().string_backing(r).unwrap(),
            Some((array.heap_ref().unwrap(), code))
        );
        let old = c.heap().string_body_usage();
        assert_eq!(
            c.read_field(string, coder, r).unwrap(),
            Value::Int(code.into())
        );
        assert_eq!(c.heap().string_body_usage().work_units - old.work_units, 1);
        assert_eq!(
            c.heap().get_field(r, plain),
            Err(InstanceError::OpaqueStringObject { class: string })
        );
    }
    assert_eq!(c.heap().allocated_arrays(), 4);
    assert_eq!(c.heap().string_body_usage().backings, 4);
    assert_eq!(c.heap().string_body_usage().backing_bytes, 13);
}
#[test]
fn readonly_storage_preserves_kind_bounds_and_value_precedence_and_ordinary_bytes() {
    let mut f = fixture(Default::default(), 0);
    let string = f.string;
    let value = f.value;
    let mut c = ctx(&mut f);
    let r = c.fresh_string(&[65]).unwrap();
    let Value::Reference(array) = c.read_field(string, value, r).unwrap() else {
        panic!()
    };
    assert!(matches!(
        c.heap_mut().store(array, -1, Value::Int(7)),
        Err(HeapError::IndexOutOfBounds { .. })
    ));
    assert!(matches!(
        c.heap_mut().store(array, 0, Value::Long(7)),
        Err(HeapError::ValueKindMismatch { .. })
    ));
    assert_eq!(
        c.heap_mut().store(array, 0, Value::Int(7)),
        Err(HeapError::ReadOnlyStringBacking)
    );
    assert_eq!(c.heap().load(array, 0).unwrap(), Value::Int(65));
    let plain = c
        .heap_mut()
        .allocate_primitive_array(PrimitiveArrayKind::Byte, 1)
        .unwrap();
    c.heap_mut().store(plain, 0, Value::Int(7)).unwrap();
    assert_eq!(c.heap().load(plain, 0).unwrap(), Value::Int(7));
    assert_eq!(c.intern_string(r).unwrap(), r);
}
#[test]
fn body_work_and_array_failures_preserve_string_then_retry_under_new_context() {
    for limit in [0, 1, 2] {
        let mut f = fixture(
            StringBodyLimits {
                max_work_units: limit,
            },
            0,
        );
        let string = f.string;
        let value = f.value;
        let mut c = ctx(&mut f);
        let r = c.fresh_string(&[65]).unwrap();
        assert!(c.read_field(string, value, r).is_err());
        assert_eq!(c.heap().string_backing(r).unwrap(), None);
        assert_eq!(c.heap().allocated_arrays(), 0);
        assert_eq!(c.heap().string_units(r).unwrap(), Some(&[65][..]));
    }
    let mut f = fixture(Default::default(), 0);
    f.h.options.max_arrays = 0;
    let string = f.string;
    let value = f.value;
    let mut c = ctx(&mut f);
    let r = c.fresh_string(&[0x20ac]).unwrap();
    assert!(c.read_field(string, value, r).is_err());
    assert_eq!(c.heap().string_body_usage().work_units, 2);
    assert_eq!(c.heap().allocated_arrays(), 0);
    assert_eq!(c.heap().string_backing(r).unwrap(), None);
    c.heap_mut().options.max_arrays = 1;
    assert!(c.read_field(string, value, r).is_ok());
}
#[test]
fn both_allocation_failpoints_keep_old_backing_pool_and_receiver_unmaterialized() {
    for fail in 0..2 {
        let mut f = fixture(Default::default(), 0);
        let string = f.string;
        let value = f.value;
        let mut c = ctx(&mut f);
        let old = c.fresh_string(&[65]).unwrap();
        c.intern_string(old).unwrap();
        c.read_field(string, value, old).unwrap();
        let previous = c.heap().string_backing(old).unwrap();
        let r = c.fresh_string(&[66]).unwrap();
        let counts = (
            c.heap().allocated_arrays(),
            c.heap().payload_bytes(),
            c.heap().entries.len(),
        );
        c.heap_mut()
            .string_bodies
            .as_mut()
            .unwrap()
            .fail_reserve_after = Some(fail);
        assert!(c.read_field(string, value, r).is_err());
        assert_eq!(c.heap().string_backing(r).unwrap(), None);
        assert_eq!(c.heap().string_backing(old).unwrap(), previous);
        assert_eq!(
            (
                c.heap().allocated_arrays(),
                c.heap().payload_bytes(),
                c.heap().entries.len()
            ),
            counts
        );
        c.heap_mut()
            .string_bodies
            .as_mut()
            .unwrap()
            .fail_reserve_after = None;
        c.read_field(string, value, r).unwrap();
        assert_eq!(c.intern_string(old).unwrap(), old);
    }
}
#[test]
fn cross_backing_coder_length_and_reverse_owner_poison_are_o1_host_failures() {
    for damage in 0..4 {
        let mut f = fixture(Default::default(), 0);
        let string = f.string;
        let value = f.value;
        let mut c = ctx(&mut f);
        let a = c.fresh_string(&[0x20ac]).unwrap();
        let b = c.fresh_string(&[0x20ad]).unwrap();
        c.read_field(string, value, a).unwrap();
        c.read_field(string, value, b).unwrap();
        let pair_a = c.heap().string_backing(a).unwrap().unwrap();
        let pair_b = c.heap().string_backing(b).unwrap().unwrap();
        let (_, owner, _) = a.heap_ref().unwrap().components();
        let (_, array, _) = pair_a.0.components();
        match damage {
            0 => {
                let EntryPayload::String(p) = &mut c.heap_mut().entries[owner].payload else {
                    panic!()
                };
                p.backing = Some((pair_b.0, 1));
            }
            1 => {
                let EntryPayload::String(p) = &mut c.heap_mut().entries[owner].payload else {
                    panic!()
                };
                p.backing = Some((pair_a.0, 0));
            }
            2 => {
                c.heap_mut().entries[array].string_backing =
                    Some(crate::heap::StringBackingOwner {
                        owner: usize::MAX,
                        coder: 1,
                    });
            }
            _ => {
                let EntryPayload::Array(Payload::Byte(bytes)) =
                    &mut c.heap_mut().entries[array].payload
                else {
                    panic!()
                };
                bytes.pop();
            }
        }
        let old = c.heap().string_body_usage();
        assert!(c.heap().string_backing(a).is_err());
        assert_eq!(c.heap().string_body_usage(), old);
        assert!(c.read_field(string, value, a).is_err());
        assert_eq!(c.heap().string_body_usage().work_units - old.work_units, 1);
        assert_eq!(c.heap().allocated_arrays(), 2);
        assert_eq!(c.heap().string_units(a).unwrap(), Some(&[0x20ac][..]));
    }
}

struct NestFixture {
    g: Graph,
    h: ManagedHeap,
    d: ClassId,
    owner: ClassId,
    field: u16,
    method: u16,
    public: u16,
    ctor: u16,
    origin: MethodId,
}
fn nest_fixture(mode: u8, enabled: bool) -> NestFixture {
    let mut g = Graph::new();
    let boot = g.boot;
    g.add(boot, Class::plain("java/lang/Throwable"));
    g.add(
        boot,
        Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
    );
    let mut h = Class::plain("p/H");
    h.field("secret", "I", 2);
    h.field("publicValue", "I", 1);
    h.method("secret", "()I", 2, &[4, 0xac], 1, 1);
    let call = h.reference(10, "java/lang/Object", "<init>", "()V");
    let mut body = vec![0x2a];
    body.extend(instr(0xb7, call));
    body.push(0xb1);
    h.method("<init>", "()V", 2, &body, 1, 1);
    let owner = if mode == 5 {
        add(&mut g, h, Some("p/Missing"), None)
    } else {
        add(
            &mut g,
            h,
            None,
            if [0, 2, 4, 6].contains(&mode) {
                Some(&["unrelated/Absent", "p/D"][..])
            } else {
                None
            },
        )
    };
    if mode == 4 {
        add(&mut g, Class::plain("p/Other"), None, Some(&["p/D"]));
    }
    if mode == 7 {
        let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
        add(&mut g, hidden, None, Some(&["p/D"]));
    }
    let mut d = Class::plain("p/D");
    let field = d.reference(9, "p/H", "secret", "I");
    let method = d.reference(10, "p/H", "secret", "()I");
    let public = d.reference(9, "p/H", "publicValue", "I");
    let ctor = d.reference(10, "p/H", "<init>", "()V");
    d.method("run", "()V", 9, &[0xb1], 0, 0);
    let host = match mode {
        0 | 1 => Some("p/H"),
        4 => Some("p/Other"),
        5 => Some("p/Missing"),
        6 => Some("[I"),
        7 => Some("q/Hidden"),
        _ => None,
    };
    let d = add(&mut g, d, host, None);
    let origin = g.method(d, "run", "()V");
    let mut heap = g.ready();
    g.bind_core(&mut heap);
    g.domain.enable_class_mirror_sites(262_144).unwrap();
    if enabled {
        g.domain.enable_nest_access().unwrap();
    }
    NestFixture {
        g,
        h: heap,
        d,
        owner,
        field,
        method,
        public,
        ctor,
        origin,
    }
}
fn nest_ctx(f: &mut NestFixture) -> FieldResolutionContext<'_> {
    f.g.domain
        .member_context_unnamed_modules(&mut f.h, Default::default(), Default::default())
        .unwrap()
}
#[test]
fn reciprocal_nests_enable_private_fields_methods_and_actual_constructor_resolution() {
    let mut f = nest_fixture(0, true);
    let d = f.d;
    let owner = f.owner;
    let field = f.field;
    let method = f.method;
    let ctor = f.ctor;
    let origin = f.origin;
    let boot = f.g.boot;
    let receiver = f.h.allocate_instance(owner).unwrap();
    let mut c = nest_ctx(&mut f);
    assert_eq!(c.read_field(d, field, receiver).unwrap(), Value::Int(0));
    assert_eq!(
        c.select_virtual(d, method, receiver).unwrap().owner(),
        owner
    );
    let throwable = c
        .registry()
        .lookup_class(boot, &units("java/lang/Throwable"))
        .unwrap()
        .unwrap();
    let error_class = c
        .registry()
        .lookup_class(boot, &units("java/lang/Error"))
        .unwrap()
        .unwrap();
    let statics = c.allocate_static_state(Default::default()).unwrap();
    let exceptions = c.prepare_exception_profile(throwable).unwrap();
    let claim = c
        .claim_initialization(&statics, &exceptions, error_class)
        .unwrap();
    assert_eq!(
        c.resolve_constructor(
            &claim,
            origin,
            ctor,
            receiver,
            ConstructorReceiverKind::Allocation
        )
        .unwrap()
        .method()
        .owner(),
        owner
    );
    let work = c.lookup_steps();
    assert_eq!(c.read_field(d, field, receiver).unwrap(), Value::Int(0));
    assert_eq!(c.lookup_steps() - work, 1); // direct receiver ancestry only; loading probe is independently metered
}
#[test]
fn false_missing_reciprocal_other_host_and_array_host_do_not_grant_private_access() {
    for mode in [1, 2, 3, 4, 6, 7] {
        let mut f = nest_fixture(mode, true);
        let d = f.d;
        let field = f.field;
        let method = f.method;
        let r = f.h.allocate_instance(f.owner).unwrap();
        assert_eq!(
            nest_ctx(&mut f).read_field(d, field, r),
            Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
        );
        assert_eq!(
            nest_ctx(&mut f).select_virtual(d, method, r),
            Err(MethodAccessError::Java(MethodJavaError::IllegalAccess))
        );
        assert!(matches!(
            f.g.domain.member_site(d, field).unwrap(),
            Some(MemberResolutionOutcome::Failure(
                MemberResolutionFailure::IllegalAccess
            ))
        ));
    }
}
#[test]
fn absent_host_facts_leave_member_vacant_then_repair_without_resetting_policy() {
    let mut f = nest_fixture(5, true);
    let d = f.d;
    let site = f.field;
    let receiver = f.h.allocate_instance(f.owner).unwrap();
    assert!(matches!(
        nest_ctx(&mut f).read_field(d, site, receiver),
        Err(FieldAccessError::ClassMirror(_))
    ));
    assert_eq!(f.g.domain.member_site(d, site).unwrap(), None);
    let host = add(
        &mut f.g,
        Class::plain("p/Missing"),
        None,
        Some(&["p/D", "p/H"]),
    );
    f.g.prepare(&[host]);
    let plan = f.g.plan(host);
    f.h.admit_instance_layouts(f.g.domain.registry(), &[&plan])
        .unwrap();
    assert_eq!(
        nest_ctx(&mut f).read_field(d, site, receiver).unwrap(),
        Value::Int(0)
    );
    assert!(matches!(
        f.g.domain.member_site(d, site).unwrap(),
        Some(MemberResolutionOutcome::Field(_))
    ));
}
#[test]
fn legacy_and_enablement_order_keep_their_distinct_host_and_terminal_semantics() {
    let mut f = nest_fixture(0, false);
    let d = f.d;
    let site = f.field;
    let receiver = f.h.allocate_instance(f.owner).unwrap();
    assert!(matches!(
        nest_ctx(&mut f).read_field(d, site, receiver),
        Err(FieldAccessError::UnsupportedNestAccess { .. })
    ));
    assert_eq!(f.g.domain.member_site(d, site).unwrap(), None);
    f.g.domain.enable_nest_access().unwrap();
    assert!(nest_ctx(&mut f).read_field(d, site, receiver).is_ok());
    f.g.domain.enable_nest_access().unwrap();
    let mut f = nest_fixture(0, false);
    let d = f.d;
    let site = f.public;
    let r = f.h.allocate_instance(f.owner).unwrap();
    nest_ctx(&mut f).read_field(d, site, r).unwrap();
    assert!(f.g.domain.enable_nest_access().is_err());
}
#[test]
fn domain_terminal_private_denial_replays_without_host_or_declaring_layout() {
    let mut f = nest_fixture(1, true);
    let d = f.d;
    let site = f.field;
    let r = f.h.allocate_instance(f.owner).unwrap();
    assert!(matches!(
        nest_ctx(&mut f).read_field(d, site, r),
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    ));
    let mut smaller = f.g.heap(&[d]);
    let mut c =
        f.g.domain
            .member_context_unnamed_modules(&mut smaller, Default::default(), Default::default())
            .unwrap();
    assert_eq!(
        c.read_field(d, site, Reference::null()),
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    );
}

#[test]
fn disabled_class_sites_remain_a_typed_retryable_nest_prerequisite() {
    let mut f = nest_fixture(0, true);
    let saved = f.g.domain.members.as_mut().unwrap().class_sites.take();
    let d = f.d;
    let site = f.field;
    let r = f.h.allocate_instance(f.owner).unwrap();
    assert!(
        matches!(nest_ctx(&mut f).read_field(d,site,r),Err(FieldAccessError::ClassMirror(e)) if *e==ClassMirrorError::Disabled)
    );
    assert_eq!(f.g.domain.member_site(d, site).unwrap(), None);
    f.g.domain.members.as_mut().unwrap().class_sites = saved;
    assert!(nest_ctx(&mut f).read_field(d, site, r).is_ok());
}
#[test]
fn identical_package_text_with_different_defining_loaders_never_forms_a_nest() {
    let mut g = Graph::new();
    let loader = g.loader();
    let mut h = Class::plain("p/H");
    h.field("secret", "I", 2);
    let owner = g.add(loader, h);
    g.bind(g.boot, "p/H", owner);
    let mut d = Class::plain("p/D");
    let field = d.reference(9, "p/H", "secret", "I");
    let d = g.add(g.boot, d);
    let mut heap = g.ready();
    g.bind_core(&mut heap);
    g.domain.enable_class_mirror_sites(100).unwrap();
    g.domain.enable_nest_access().unwrap();
    let r = heap.allocate_instance(owner).unwrap();
    let mut c = g
        .domain
        .member_context_unnamed_modules(&mut heap, Default::default(), Default::default())
        .unwrap();
    assert_eq!(
        c.read_field(d, field, r),
        Err(FieldAccessError::Java(FieldJavaError::IllegalAccess))
    );
    assert_eq!(g.domain.class_site_count(), 0);
}
#[test]
fn malformed_private_plan_scalars_cannot_rebind_and_matching_rebind_preserves_usage() {
    for damage in 0..5 {
        let mut f = fixture(Default::default(), 0);
        let mut plan = f.plan.clone();
        match damage {
            0 => plan.fields[0] = plan.fields[1],
            1 => plan.methods[0] = plan.methods[1],
            2 => plan.constructor = plan.methods[0],
            3 => plan.comparator_edge = usize::MAX,
            _ => plan.registry = u64::MAX,
        };
        assert!(
            f.h.bind_compact_string_bodies(f.g.domain.registry(), &plan, Default::default())
                .is_err()
        );
        assert_eq!(f.h.string_body_usage(), StringBodyUsage::default());
        assert_eq!(f.h.entries.len(), 0);
    }
    let mut f = fixture(Default::default(), 0);
    let s = f.string;
    let value = f.value;
    let r = ctx(&mut f).fresh_string(&[65]).unwrap();
    ctx(&mut f).read_field(s, value, r).unwrap();
    let usage = f.h.string_body_usage();
    f.h.bind_compact_string_bodies(f.g.domain.registry(), &f.plan, Default::default())
        .unwrap();
    assert_eq!(f.h.string_body_usage(), usage);
    assert!(
        f.h.bind_compact_string_bodies(
            f.g.domain.registry(),
            &f.plan,
            StringBodyLimits { max_work_units: 0 }
        )
        .is_err()
    );
    assert_eq!(f.h.string_body_usage(), usage);
}
#[test]
fn binder_never_promotes_a_heap_with_existing_kernel_objects_or_array_entries() {
    for string in [false, true] {
        let mut f = fixture(Default::default(), 0);
        f.h.string_bodies = None;
        if string {
            ctx(&mut f).fresh_string(&[65]).unwrap();
        } else {
            f.h.allocate_primitive_array(PrimitiveArrayKind::Byte, 0)
                .unwrap();
        }
        assert!(matches!(
            f.h.bind_compact_string_bodies(f.g.domain.registry(), &f.plan, Default::default())
                .unwrap_err()
                .kind,
            StringBodyErrorKind::InvalidBinding(_)
        ));
        assert!(f.h.string_bodies.is_none());
    }
}
#[test]
fn only_bound_code_roles_open_and_new_raw_body_entry_does_not_initialize_or_allocate() {
    let mut f = fixture(Default::default(), 0);
    let method = f.plan.methods[0];
    let other = f.g.method(f.string, "charAt", "(I)C");
    let s = f.string;
    let mut c = ctx(&mut f);
    let r = c.fresh_string(&[65]).unwrap();
    c.check_method_reference_profile(method, Some(r)).unwrap();
    assert_eq!(c.heap().string_body_usage(), StringBodyUsage::default());
    assert!(c.heap().initialization_claim.is_none());
    assert!(
        matches!(c.check_method_reference_profile(other,Some(r)),Err(MethodAccessError::Context(FieldAccessError::Instance(InstanceError::OpaqueStringObject{class}))) if class==s)
    );
    assert_eq!(c.heap().allocated_arrays(), 0);
}
#[test]
fn all_four_heap_limits_follow_width_work_and_precede_encoding_work() {
    for resource in [
        HeapResource::Arrays,
        HeapResource::ArrayLength,
        HeapResource::TotalElements,
        HeapResource::PayloadBytes,
    ] {
        let mut f = fixture(Default::default(), 0);
        match resource {
            HeapResource::Arrays => f.h.options.max_arrays = 0,
            HeapResource::ArrayLength => f.h.options.max_array_length = 1,
            HeapResource::TotalElements => f.h.options.max_total_elements = 1,
            HeapResource::PayloadBytes => f.h.options.max_payload_bytes = 1,
        };
        let s = f.string;
        let field = f.value;
        let mut c = ctx(&mut f);
        let r = c.fresh_string(&[0x20ac]).unwrap();
        let e = c.read_field(s, field, r).unwrap_err();
        assert!(
            matches!(e,FieldAccessError::StringBody(e) if matches!(e.kind,StringBodyErrorKind::Heap(ref h) if matches!(**h,HeapError::ResourceLimit{resource:actual,..} if actual==resource)))
        );
        assert_eq!(c.heap().string_body_usage().work_units, 2);
        assert_eq!(c.heap().allocated_arrays(), 0);
        assert_eq!(c.heap().string_backing(r).unwrap(), None);
    }
}

#[test]
fn scalar_overflows_keep_the_old_backing_graph_and_do_not_publish_a_new_array() {
    for counter in 0..3 {
        let mut f = fixture(
            StringBodyLimits {
                max_work_units: u64::MAX,
            },
            0,
        );
        let s = f.string;
        let value = f.value;
        let old = ctx(&mut f).fresh_string(&[65]).unwrap();
        ctx(&mut f).read_field(s, value, old).unwrap();
        let pair = f.h.string_backing(old).unwrap();
        let next = ctx(&mut f).fresh_string(&[66]).unwrap();
        let before_arrays = f.h.allocated_arrays();
        let b = f.h.string_bodies.as_mut().unwrap();
        match counter {
            0 => b.usage.work_units = u64::MAX - 1,
            1 => b.usage.backings = u64::MAX,
            _ => b.usage.backing_bytes = u64::MAX,
        }
        let before = f.h.string_body_usage();
        let e = ctx(&mut f).read_field(s, value, next).unwrap_err();
        assert!(
            matches!(e, FieldAccessError::StringBody(e) if match e.kind {
                StringBodyErrorKind::Overflow { resource: StringBodyResource::BackingWork } => counter == 0,
                StringBodyErrorKind::InvalidState(_) => counter != 0,
                _ => false,
            })
        );
        assert_eq!(f.h.allocated_arrays(), before_arrays);
        assert_eq!(f.h.string_backing(old).unwrap(), pair);
        assert_eq!(f.h.string_backing(next).unwrap(), None);
        let after = f.h.string_body_usage();
        assert_eq!(after.backings, before.backings);
        assert_eq!(after.backing_bytes, before.backing_bytes);
        assert_eq!(
            after.work_units,
            if counter == 0 {
                u64::MAX
            } else {
                before.work_units + 2
            }
        );
    }
}

#[test]
fn large_nest_member_list_is_funded_before_visiting_unrelated_entries_and_is_retryable() {
    let mut g = Graph::new();
    let mut h = Class::plain("p/H");
    h.field("secret", "I", 2);
    let names: Vec<String> = (0..2048).map(|i| format!("absent/Member{i}")).collect();
    let mut borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    borrowed.push("p/D");
    let owner = add(&mut g, h, None, Some(&borrowed));
    let mut d = Class::plain("p/D");
    let site = d.reference(9, "p/H", "secret", "I");
    let d = add(&mut g, d, Some("p/H"), None);
    let mut heap = g.ready();
    g.bind_core(&mut heap);
    g.domain.enable_class_mirror_sites(100).unwrap();
    g.domain.enable_nest_access().unwrap();
    let receiver = heap.allocate_instance(owner).unwrap();
    let mut c = g
        .domain
        .member_context_unnamed_modules(
            &mut heap,
            FieldResolutionOptions {
                max_lookup_steps: 256,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap();
    assert!(c.read_field(d, site, receiver).is_err());
    assert!(c.lookup_steps() <= 256);
    assert_eq!(g.domain.member_site(d, site).unwrap(), None);
    let mut c = g
        .domain
        .member_context_unnamed_modules(&mut heap, Default::default(), Default::default())
        .unwrap();
    assert_eq!(c.read_field(d, site, receiver).unwrap(), Value::Int(0));
    assert!(c.lookup_steps() >= 3 * 2048);
}

//! Immutable equality recipe authority. Native execution effects belong only to
//! the interpreter's permanently heap-claiming initialization session.
use super::probe::Probe;
use super::*;
use crabspot_classfile::{ClassFile, Constant};
const STRING: &str = "java/lang/String";
const HELPER: &str = "java/lang/StringLatin1";
const CLASS: &str = "java/lang/Class";
const LOADER: &str = "java/lang/ClassLoader";
const OBJECT: &str = "java/lang/Object";
const OSF: &str = "java/io/ObjectStreamField";
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BootstrapAssertionPolicy {
    #[default]
    Disabled,
    Enabled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassAssertionNative {
    Register,
    DesiredStatus,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactStringEqualityPlan {
    body: CompactStringBodyPlan,
    class: ClassId,
    helper: ClassId,
    // String.equals, helper.equals/clinit, Class.query/clinit/register/query0.
    methods: [MethodId; 7],
    method_keys: [MemberKey; 7],
    // helper flag, Class loader/empty Class array/serial fields.
    fields: [FieldId; 4],
    field_keys: [MemberKey; 4],
    sites: [u32; 3],
}
impl CompactStringEqualityPlan {
    pub fn private_root(&self) -> ClassId {
        self.helper
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringEqualityDiscovery {
    pub plan: Option<CompactStringEqualityPlan>,
    pub usage: StringBodyDiscoveryUsage,
}
#[derive(Debug)]
pub(crate) struct EqualityState {
    plan: CompactStringEqualityPlan,
    policy: BootstrapAssertionPolicy,
    heap: u64,
    universe: ReferenceTypeUniverse,
    classes: usize,
}
impl EqualityState {
    pub(crate) fn string_method(&self, method: MethodId) -> bool {
        method == self.plan.methods[0]
    }
    pub(crate) fn class_method(&self, method: MethodId) -> bool {
        method == self.plan.methods[3]
    }
}
fn invalid(s: &'static str) -> Box<StringBodyError> {
    error(StringBodyErrorKind::InvalidBinding(s))
}
fn validate_body(registry: &DefinitionRegistry, body: &CompactStringBodyPlan) -> R<()> {
    if registry.identity() != body.registry {
        return Err(invalid("equality compact registry differs"));
    }
    for (id, key) in body.methods.iter().zip(body.method_keys) {
        if id.owner() != body.string || MemberKey::of(registry.method(*id)?) != key {
            return Err(invalid("equality compact method proof differs"));
        }
    }
    for (id, key) in body.fields.iter().zip(body.field_keys) {
        if id.owner() != body.string || MemberKey::of(registry.field(*id)?) != key {
            return Err(invalid("equality compact field proof differs"));
        }
    }
    if body.constructor.owner() != body.roots[0]
        || MemberKey::of(registry.method(body.constructor)?) != body.constructor_key
    {
        return Err(invalid("equality compact constructor proof differs"));
    }
    Ok(())
}
impl ManagedHeap {
    pub fn bind_compact_string_equality(
        &mut self,
        registry: &DefinitionRegistry,
        plan: &CompactStringEqualityPlan,
        policy: BootstrapAssertionPolicy,
    ) -> R<()> {
        validate_body(registry, &plan.body)?;
        let limits = self
            .string_bodies
            .as_ref()
            .ok_or_else(|| invalid("equality requires compact bodies"))?
            .limits;
        self.bind_compact_string_bodies(registry, &plan.body, limits)?;
        let universe = self
            .reference_type_universe()
            .ok_or_else(|| invalid("equality universe absent"))?;
        if self.mirrors.as_ref().is_none_or(|m| m.class != plan.class) {
            return Err(invalid("equality Class differs from mirror role"));
        }
        for id in [plan.class, plan.helper] {
            registry.class(id)?;
            self.instance_graph_node(id)?;
            if registry.defining_loader(id)? != plan.body.bootstrap
                || self.instance_graph_node(id)?.superclass != Some(universe.object)
            {
                return Err(invalid("equality role hierarchy or loader differs"));
            }
        }
        let owners = [
            plan.body.string,
            plan.helper,
            plan.helper,
            plan.class,
            plan.class,
            plan.class,
            plan.class,
        ];
        for ((id, key), owner) in plan.methods.iter().zip(plan.method_keys).zip(owners) {
            if id.owner() != owner || MemberKey::of(registry.method(*id)?) != key {
                return Err(invalid("equality method fingerprint differs"));
            }
        }
        for ((id, key), owner) in plan.fields.iter().zip(plan.field_keys).zip([
            plan.helper,
            plan.class,
            plan.class,
            plan.class,
        ]) {
            if id.owner() != owner || MemberKey::of(registry.field(*id)?) != key {
                return Err(invalid("equality field fingerprint differs"));
            }
        }
        if plan.sites != [0, 10, 50] {
            return Err(invalid("equality native call locations differ"));
        }
        if let Some(old) = &self.string_equality {
            if old.plan == *plan
                && old.policy == policy
                && old.heap == self.identity
                && old.universe == universe
                && old.classes == self.retained_layout_classes()
            {
                return Ok(());
            }
            return Err(error(StringBodyErrorKind::PolicyConflict));
        }
        if self.class_mirror_count() != 0
            || self.initialization_claim.is_some()
            || self.string_usage() != StringUsage::default()
            || self.string_body_usage() != StringBodyUsage::default()
        {
            return Err(invalid(
                "equality binding requires no String work, mirrors or initialization claim",
            ));
        }
        self.string_equality = Some(EqualityState {
            plan: plan.clone(),
            policy,
            heap: self.identity,
            universe,
            classes: self.retained_layout_classes(),
        });
        Ok(())
    }
    pub(crate) fn equality_affinity(&self, registry: &DefinitionRegistry) -> R<&EqualityState> {
        self.body_affinity()?;
        let e = self
            .string_equality
            .as_ref()
            .ok_or_else(|| invalid("equality profile is absent"))?;
        if e.heap != self.identity
            || e.classes != self.retained_layout_classes()
            || Some(e.universe) != self.reference_type_universe()
            || e.plan.body.registry != registry.identity()
            || self
                .string_bodies
                .as_ref()
                .is_none_or(|b| b.plan != e.plan.body)
            || self
                .mirrors
                .as_ref()
                .is_none_or(|m| m.class != e.plan.class)
        {
            return Err(error(StringBodyErrorKind::InvalidState(
                "equality context affinity differs",
            )));
        }
        Ok(e)
    }
}
impl FieldResolutionContext<'_> {
    pub fn class_assertion_native(&mut self, actual: MethodId) -> R<Option<ClassAssertionNative>> {
        self.unchanged()?;
        self.registry.method(actual)?;
        let Some(e) = &self.heap.string_equality else {
            return Ok(None);
        };
        if actual.owner() != e.plan.class {
            return Ok(None);
        }
        let e = self.heap.equality_affinity(self.registry)?;
        let found = if actual == e.plan.methods[5] {
            Some(ClassAssertionNative::Register)
        } else if actual == e.plan.methods[6] {
            Some(ClassAssertionNative::DesiredStatus)
        } else {
            None
        };
        self.step()?;
        self.check_class_mirror_profile()
            .map_err(|e| Box::<StringBodyError>::from(FieldAccessError::ClassMirror(e)))?;
        Ok(found)
    }
    pub fn check_class_assertion_site(
        &self,
        role: ClassAssertionNative,
        caller: MethodId,
        pc: u32,
    ) -> R<MethodId> {
        self.unchanged()?;
        self.registry.method(caller)?;
        let e = self.heap.equality_affinity(self.registry)?;
        self.check_class_mirror_profile()
            .map_err(|e| Box::<StringBodyError>::from(FieldAccessError::ClassMirror(e)))?;
        let valid = match role {
            ClassAssertionNative::Register => caller == e.plan.methods[4] && pc == e.plan.sites[0],
            ClassAssertionNative::DesiredStatus => {
                caller == e.plan.methods[3] && (pc == e.plan.sites[1] || pc == e.plan.sites[2])
            }
        };
        if !valid {
            return Err(invalid("native caller role or instruction differs"));
        }
        Ok(match role {
            ClassAssertionNative::Register => e.plan.methods[5],
            ClassAssertionNative::DesiredStatus => e.plan.methods[6],
        })
    }
    pub fn bootstrap_assertion_policy(&self, mirror: Reference) -> R<BootstrapAssertionPolicy> {
        self.unchanged()?;
        let e = self.heap.equality_affinity(self.registry)?;
        self.check_class_mirror_profile()
            .map_err(|e| Box::<StringBodyError>::from(FieldAccessError::ClassMirror(e)))?;
        let crate::heap::EntryPayload::Mirror(represented) = self.heap.entry(mirror)?.payload
        else {
            return Err(error(StringBodyErrorKind::InvalidReceiver));
        };
        if !self.heap.canonical_mirror(mirror, represented)? {
            return Err(error(StringBodyErrorKind::InvalidState(
                "mirror forward map differs",
            )));
        }
        if let ReferenceType::Class(id) = represented {
            self.registry.class(id)?;
            self.heap.instance_graph_node(id)?;
            if self.registry.defining_loader(id)? == e.plan.body.bootstrap {
                return Ok(e.policy);
            }
        }
        Err(error(StringBodyErrorKind::UnsupportedAssertionTarget {
            represented,
        }))
    }
    pub(crate) fn read_class_assertion_field(
        &self,
        field: FieldId,
        receiver: Reference,
    ) -> R<Option<Value>> {
        let Some(e) = &self.heap.string_equality else {
            return Ok(None);
        };
        if field != e.plan.fields[1]
            || !matches!(
                self.heap.entry(receiver)?.payload,
                crate::heap::EntryPayload::Mirror(_)
            )
        {
            return Ok(None);
        }
        self.bootstrap_assertion_policy(receiver)?;
        Ok(Some(Value::Reference(Reference::null())))
    }
}

#[derive(Clone, Copy)]
enum A {
    None,
    Branch(usize),
    Class(&'static str),
    Ref(u8, &'static str, &'static str, &'static str),
    Ldc(&'static str),
    Inc(u8, i8),
}
#[derive(Clone, Copy)]
struct O(u8, A);
const fn o(op: u8) -> O {
    O(op, A::None)
}
const fn b(op: u8, to: usize) -> O {
    O(op, A::Branch(to))
}
const fn c(op: u8, name: &'static str) -> O {
    O(op, A::Class(name))
}
const fn f(op: u8, owner: &'static str, name: &'static str, desc: &'static str) -> O {
    O(op, A::Ref(9, owner, name, desc))
}
const fn m(op: u8, owner: &'static str, name: &'static str, desc: &'static str) -> O {
    O(op, A::Ref(10, owner, name, desc))
}
const EQUALS: &[O] = &[
    o(0x2a),
    o(0x2b),
    b(0xa6, 5),
    o(4),
    o(0xac),
    o(0x2b),
    c(0xc1, STRING),
    b(0x99, 26),
    o(0x2b),
    c(0xc0, STRING),
    o(0x4d),
    f(0xb2, STRING, "COMPACT_STRINGS", "Z"),
    b(0x99, 18),
    o(0x2a),
    f(0xb4, STRING, "coder", "B"),
    o(0x2c),
    f(0xb4, STRING, "coder", "B"),
    b(0xa0, 26),
    o(0x2a),
    f(0xb4, STRING, "value", "[B"),
    o(0x2c),
    f(0xb4, STRING, "value", "[B"),
    m(0xb8, HELPER, "equals", "([B[B)Z"),
    b(0x99, 26),
    o(4),
    b(0xa7, 27),
    o(3),
    o(0xac),
];
const HELPER_EQUALS: &[O] = &[
    o(0x2a),
    o(0xbe),
    o(0x2b),
    o(0xbe),
    b(0xa0, 24),
    o(3),
    o(0x3d),
    o(0x1c),
    o(0x2a),
    o(0xbe),
    b(0xa2, 22),
    o(0x2a),
    o(0x1c),
    o(0x33),
    o(0x2b),
    o(0x1c),
    o(0x33),
    b(0x9f, 20),
    o(3),
    o(0xac),
    O(0x84, A::Inc(2, 1)),
    b(0xa7, 7),
    o(4),
    o(0xac),
    o(3),
    o(0xac),
];
const HELPER_INIT: &[O] = &[
    O(0x12, A::Ldc(HELPER)),
    m(0xb6, CLASS, "desiredAssertionStatus", "()Z"),
    b(0x9a, 5),
    o(4),
    b(0xa7, 6),
    o(3),
    f(0xb3, HELPER, "$assertionsDisabled", "Z"),
    o(0xb1),
];
const CLASS_INIT: &[O] = &[
    m(0xb8, CLASS, "registerNatives", "()V"),
    o(3),
    c(0xbd, CLASS),
    f(0xb3, CLASS, "EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;"),
    o(3),
    c(0xbd, OSF),
    f(
        0xb3,
        CLASS,
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
    ),
    o(0xb1),
];
const STATUS: &[O] = &[
    o(0x2a),
    f(0xb4, CLASS, "classLoader", "Ljava/lang/ClassLoader;"),
    o(0x4c),
    o(0x2b),
    b(0xc7, 8),
    o(0x2a),
    m(
        0xb8,
        CLASS,
        "desiredAssertionStatus0",
        "(Ljava/lang/Class;)Z",
    ),
    o(0xac),
    o(0x2b),
    f(0xb4, LOADER, "assertionLock", "Ljava/lang/Object;"),
    o(0x59),
    o(0x4d),
    o(0xc2),
    o(0x2b),
    f(0xb4, LOADER, "classAssertionStatus", "Ljava/util/Map;"),
    b(0xc6, 23),
    o(0x2b),
    o(0x2a),
    m(0xb6, CLASS, "getName", "()Ljava/lang/String;"),
    m(
        0xb6,
        LOADER,
        "desiredAssertionStatus",
        "(Ljava/lang/String;)Z",
    ),
    o(0x2c),
    o(0xc3),
    o(0xac),
    o(0x2c),
    o(0xc3),
    b(0xa7, 31),
    o(0x4e),
    o(0x2c),
    o(0xc3),
    o(0x2d),
    o(0xbf),
    o(0x2a),
    m(
        0xb8,
        CLASS,
        "desiredAssertionStatus0",
        "(Ljava/lang/Class;)Z",
    ),
    o(0xac),
];
fn recipe(
    p: &mut Probe,
    c: &ClassFile,
    member: &Member,
    length: usize,
    spec: &[O],
    handlers: &[[usize; 3]],
) -> R<bool> {
    let Some(code) = p.code(member)? else {
        return Ok(false);
    };
    let wide = spec.iter().any(|op| matches!(op.1, A::Ldc(_)));
    if (code.code.len() != length && !(wide && code.code.len() == length + 1))
        || code.exception_table.len() != handlers.len()
    {
        return Ok(false);
    }
    let mut pcs = [0usize; 65];
    let mut branches = [None; 64];
    let mut pc = 0usize;
    for (ordinal, &O(expected, arg)) in spec.iter().enumerate() {
        p.pay(1)?;
        let actual = code.code.get(pc).copied();
        if actual != Some(expected) && !(matches!(arg, A::Ldc(_)) && actual == Some(0x13)) {
            return Ok(false);
        }
        *pcs.get_mut(ordinal)
            .ok_or_else(|| p.fail(StringBodyErrorKind::InvalidState("recipe too long")))? = pc;
        let start = pc;
        pc += 1;
        match arg {
            A::None => {}
            A::Inc(index, delta) => {
                p.pay(1)?;
                if code.code.get(pc).copied() != Some(index) {
                    return Ok(false);
                }
                pc += 1;
                p.pay(1)?;
                if code.code.get(pc).copied() != Some(delta as u8) {
                    return Ok(false);
                }
                pc += 1;
            }
            _ => {
                p.pay(1)?;
                let word = if matches!(arg, A::Ldc(_)) && actual == Some(0x12) {
                    let Some(&v) = code.code.get(pc) else {
                        return Ok(false);
                    };
                    pc += 1;
                    u16::from(v)
                } else {
                    let Some(data) = code.code.get(pc..pc + 2) else {
                        return Ok(false);
                    };
                    pc += 2;
                    u16::from_be_bytes([data[0], data[1]])
                };
                let valid = match arg {
                    A::Branch(_) => {
                        *branches.get_mut(ordinal).ok_or_else(|| {
                            p.fail(StringBodyErrorKind::InvalidState("branch slot missing"))
                        })? = Some(start as i64 + i64::from(word as i16));
                        true
                    }
                    A::Class(name) | A::Ldc(name) => p.class_name(c, word, name)?,
                    A::Ref(tag, owner, name, desc) => p.member(c, word, tag, owner, name, desc)?,
                    _ => false,
                };
                if !valid {
                    return Ok(false);
                }
            }
        }
    }
    if pc != code.code.len() {
        return Ok(false);
    }
    *pcs.get_mut(spec.len())
        .ok_or_else(|| p.fail(StringBodyErrorKind::InvalidState("recipe end missing")))? = pc;
    for (ordinal, op) in spec.iter().enumerate() {
        if let A::Branch(target) = op.1 {
            if branches.get(ordinal).copied().flatten() != pcs.get(target).map(|x| *x as i64) {
                return Ok(false);
            }
        }
    }
    for (actual, expected) in code.exception_table.iter().zip(handlers) {
        p.pay(1)?;
        if actual.catch_type != 0
            || Some(usize::from(actual.start_pc)) != pcs.get(expected[0]).copied()
            || Some(usize::from(actual.end_pc)) != pcs.get(expected[1]).copied()
            || Some(usize::from(actual.handler_pc)) != pcs.get(expected[2]).copied()
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn require(p: &Probe, value: Option<MethodId>) -> R<MethodId> {
    value.ok_or_else(|| {
        p.fail(StringBodyErrorKind::InvalidBinding(
            "equality method missing",
        ))
    })
}
fn fields<const N: usize>(
    p: &mut Probe,
    r: &DefinitionRegistry,
    id: ClassId,
    specs: &[(&str, &str); N],
    flags: [u16; N],
) -> R<[FieldId; N]> {
    let found = p.find_fields(r, id, specs)?;
    let mut out = [FieldId {
        owner: id,
        index: 0,
    }; N];
    for (i, entry) in found.iter().enumerate() {
        let value = entry.ok_or_else(|| {
            p.fail(StringBodyErrorKind::InvalidBinding(
                "equality field missing",
            ))
        })?;
        if p.registry(r.field(value))?.access_flags != flags[i] {
            return Err(p.fail(StringBodyErrorKind::InvalidBinding(
                "equality field flags differ",
            )));
        }
        out[i] = value;
    }
    Ok(out)
}
pub fn probe_compact_string_equality(
    registry: &DefinitionRegistry,
    body: &CompactStringBodyPlan,
    limits: StringBodyDiscoveryLimits,
) -> R<StringEqualityDiscovery> {
    validate_body(registry, body)?;
    let mut p = Probe {
        limit: limits.max_work_units,
        usage: Default::default(),
    };
    let initial = p.find_methods(
        registry,
        body.string,
        &[("equals", "(Ljava/lang/Object;)Z")],
    )?[0];
    let Some(eq) = initial else {
        return Ok(StringEqualityDiscovery {
            plan: None,
            usage: p.usage,
        });
    };
    let string = p.registry(registry.class(body.string))?;
    let eq_member = p.registry(registry.method(eq))?;
    if !recipe(&mut p, string, eq_member, 56, EQUALS, &[])? {
        return Ok(StringEqualityDiscovery {
            plan: None,
            usage: p.usage,
        });
    }
    if eq_member.access_flags != 1 {
        return Err(p.fail(StringBodyErrorKind::InvalidBinding(
            "String.equals flags differ",
        )));
    }
    let mut roots = [None; 2];
    for (id, c) in registry.definition_rows() {
        p.pay(1)?;
        if p.registry(registry.defining_loader(id))? != body.bootstrap {
            continue;
        }
        let mut name = None;
        for (slot, wanted) in [HELPER, CLASS].iter().enumerate() {
            if roots[slot].is_some() {
                continue;
            }
            let text = if let Some(n) = name {
                n
            } else {
                let Constant::Class { name_index } = p.cp(c, c.this_class)? else {
                    return Err(p.fail(StringBodyErrorKind::InvalidBinding(
                        "equality definition name differs",
                    )));
                };
                let n = p.utf(c, *name_index)?;
                name = Some(n);
                n
            };
            if p.eq(text, wanted)? {
                roots[slot] = Some(id)
            }
        }
        if roots.iter().all(Option::is_some) {
            break;
        }
    }
    let helper = roots[0]
        .ok_or_else(|| p.fail(StringBodyErrorKind::InvalidBinding("StringLatin1 missing")))?;
    let class =
        roots[1].ok_or_else(|| p.fail(StringBodyErrorKind::InvalidBinding("Class missing")))?;
    let h = p.registry(registry.class(helper))?;
    let c = p.registry(registry.class(class))?;
    if h.access_flags != 0x30
        || !p.class_name(h, h.super_class, OBJECT)?
        || c.access_flags & (1 | 16 | 0x200 | 0x400) != 17
        || !p.class_name(c, c.super_class, OBJECT)?
    {
        return Err(p.fail(StringBodyErrorKind::InvalidBinding(
            "equality class role differs",
        )));
    }
    let hm = p.find_methods(
        registry,
        helper,
        &[("equals", "([B[B)Z"), ("<clinit>", "()V")],
    )?;
    let hf = fields(
        &mut p,
        registry,
        helper,
        &[("$assertionsDisabled", "Z")],
        [0x1018],
    )?;
    let hm = [require(&p, hm[0])?, require(&p, hm[1])?];
    for (id, flags, len, spec) in [(hm[0], 9, 36, HELPER_EQUALS), (hm[1], 8, 17, HELPER_INIT)] {
        let member = p.registry(registry.method(id))?;
        if member.access_flags != flags || !recipe(&mut p, h, member, len, spec, &[])? {
            return Err(p.fail(StringBodyErrorKind::InvalidBinding(
                "StringLatin1 method differs",
            )));
        }
    }
    let cm = p.find_methods(
        registry,
        class,
        &[
            ("desiredAssertionStatus", "()Z"),
            ("<clinit>", "()V"),
            ("registerNatives", "()V"),
            ("desiredAssertionStatus0", "(Ljava/lang/Class;)Z"),
        ],
    )?;
    let cf = fields(
        &mut p,
        registry,
        class,
        &[
            ("classLoader", "Ljava/lang/ClassLoader;"),
            ("EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;"),
            ("serialPersistentFields", "[Ljava/io/ObjectStreamField;"),
        ],
        [0x12, 0x1a, 0x1a],
    )?;
    let cm = [
        require(&p, cm[0])?,
        require(&p, cm[1])?,
        require(&p, cm[2])?,
        require(&p, cm[3])?,
    ];
    for (i, id) in cm.iter().enumerate() {
        let member = p.registry(registry.method(*id))?;
        if member.access_flags != [1, 8, 0x10a, 0x10a][i] {
            return Err(p.fail(StringBodyErrorKind::InvalidBinding(
                "Class method flags differ",
            )));
        }
        let valid = match i {
            0 => recipe(
                &mut p,
                c,
                member,
                54,
                STATUS,
                &[[13, 22, 26], [23, 25, 26], [26, 29, 26]],
            )?,
            1 => recipe(&mut p, c, member, 18, CLASS_INIT, &[])?,
            _ => p.code(member)?.is_none(),
        };
        if !valid {
            return Err(p.fail(StringBodyErrorKind::InvalidBinding(
                "Class method recipe differs",
            )));
        }
    }
    let methods = [eq, hm[0], hm[1], cm[0], cm[1], cm[2], cm[3]];
    let fields = [hf[0], cf[0], cf[1], cf[2]];
    let mut method_keys = [MemberKey::of(eq_member); 7];
    for (i, id) in methods.iter().enumerate() {
        method_keys[i] = MemberKey::of(p.registry(registry.method(*id))?);
    }
    let mut field_keys = [MemberKey::of(p.registry(registry.field(fields[0]))?); 4];
    for (i, id) in fields.iter().enumerate() {
        field_keys[i] = MemberKey::of(p.registry(registry.field(*id))?);
    }
    Ok(StringEqualityDiscovery {
        plan: Some(CompactStringEqualityPlan {
            body: body.clone(),
            class,
            helper,
            methods,
            method_keys,
            fields,
            field_keys,
            sites: [0, 10, 50],
        }),
        usage: p.usage,
    })
}

#[cfg(test)]
#[path = "string_equality_tests.rs"]
pub(crate) mod tests;

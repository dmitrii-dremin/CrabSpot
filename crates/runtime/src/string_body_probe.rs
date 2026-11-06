//! A bounded, deterministic semantic recipe probe. Only immutable registry facts
//! are inspected; no heap, class initialization or preparation authority is made.
use super::*;
use crabspot_classfile::{AttributeBody, ClassFile, Code, Constant, JavaString};
const STRING: &str = "java/lang/String";
const CMP: &str = "java/lang/String$CaseInsensitiveComparator";
const COMPARATOR: &str = "java/util/Comparator";
const OSF: &str = "java/io/ObjectStreamField";
const OBJECT: &str = "java/lang/Object";
struct Probe {
    limit: u64,
    usage: StringBodyDiscoveryUsage,
}
impl Probe {
    fn fail(&self, kind: StringBodyErrorKind) -> Box<StringBodyError> {
        Box::new(StringBodyError {
            kind,
            discovery_usage: self.usage,
        })
    }
    fn registry<T>(&self, r: Result<T, RegistryError>) -> R<T> {
        r.map_err(|e| self.fail(StringBodyErrorKind::Registry(Box::new(e))))
    }
    fn parse<T>(&self, r: Result<T, crabspot_classfile::ParseError>) -> R<T> {
        self.registry(r.map_err(RegistryError::Parse))
    }
    fn pay(&mut self, n: u64) -> R<()> {
        let resource = StringBodyResource::DiscoveryWork;
        let required = self
            .usage
            .work_units
            .checked_add(n)
            .ok_or_else(|| self.fail(StringBodyErrorKind::Overflow { resource }))?;
        if required > self.limit {
            return Err(self.fail(StringBodyErrorKind::Limit {
                resource,
                limit: self.limit,
                required,
            }));
        }
        self.usage.work_units = required;
        Ok(())
    }
    fn eq(&mut self, a: &JavaString, b: &str) -> R<bool> {
        if a.units().len() != b.len() {
            return Ok(false);
        }
        self.pay(a.units().len() as u64)?;
        Ok(a.equals_ascii(b))
    }
    fn cp<'a>(&mut self, c: &'a ClassFile, index: u16) -> R<&'a Constant> {
        self.pay(1)?;
        self.parse(c.constant_pool.get(index))
    }
    fn utf<'a>(&mut self, c: &'a ClassFile, index: u16) -> R<&'a JavaString> {
        self.pay(1)?;
        self.parse(c.constant_pool.utf8(index))
    }
    fn class_name(&mut self, c: &ClassFile, index: u16, name: &str) -> R<bool> {
        let Constant::Class { name_index } = self.cp(c, index)? else {
            return Err(self.fail(StringBodyErrorKind::InvalidBinding(
                "recipe class operand has wrong tag",
            )));
        };
        let text = self.utf(c, *name_index)?;
        self.eq(text, name)
    }
    fn member(
        &mut self,
        c: &ClassFile,
        index: u16,
        tag: u8,
        owner: &str,
        name: &str,
        desc: &str,
    ) -> R<bool> {
        let (class, nat) = match self.cp(c, index)? {
            Constant::Fieldref {
                class_index,
                name_and_type_index,
            } if tag == 9 => (*class_index, *name_and_type_index),
            Constant::Methodref {
                class_index,
                name_and_type_index,
            } if tag == 10 => (*class_index, *name_and_type_index),
            _ => return Ok(false),
        };
        if !self.class_name(c, class, owner)? {
            return Ok(false);
        }
        let Constant::NameAndType {
            name_index,
            descriptor_index,
        } = self.cp(c, nat)?
        else {
            return Err(self.fail(StringBodyErrorKind::InvalidBinding(
                "recipe member has no NameAndType",
            )));
        };
        let n = self.utf(c, *name_index)?;
        if !self.eq(n, name)? {
            return Ok(false);
        }
        let d = self.utf(c, *descriptor_index)?;
        self.eq(d, desc)
    }
    fn code<'a>(&mut self, m: &'a Member) -> R<Option<&'a Code>> {
        for a in &m.attributes {
            self.pay(1)?;
            if let AttributeBody::Code(c) = &a.body {
                return Ok(Some(c));
            }
        }
        Ok(None)
    }
    fn find_methods<const N: usize>(
        &mut self,
        r: &DefinitionRegistry,
        class: ClassId,
        specs: &[(&str, &str); N],
    ) -> R<[Option<MethodId>; N]> {
        let c = self.registry(r.class(class))?;
        let mut found = [None; N];
        for (index, m) in c.methods.iter().enumerate() {
            self.pay(1)?;
            let name = self.utf(c, m.name_index)?;
            for (slot, (n, d)) in specs.iter().enumerate() {
                if found.get(slot).is_some_and(Option::is_some) {
                    continue;
                }
                if !self.eq(name, n)? {
                    continue;
                }
                let desc = self.utf(c, m.descriptor_index)?;
                if self.eq(desc, d)? {
                    let value = found.get_mut(slot).ok_or_else(|| {
                        self.fail(StringBodyErrorKind::InvalidState("method slot absent"))
                    })?;
                    *value = Some(MethodId {
                        owner: class,
                        index,
                    });
                }
            }
            if found.iter().all(Option::is_some) {
                break;
            }
        }
        Ok(found)
    }
    fn find_fields<const N: usize>(
        &mut self,
        r: &DefinitionRegistry,
        class: ClassId,
        specs: &[(&str, &str); N],
    ) -> R<[Option<FieldId>; N]> {
        let c = self.registry(r.class(class))?;
        let mut found = [None; N];
        for (index, m) in c.fields.iter().enumerate() {
            self.pay(1)?;
            let name = self.utf(c, m.name_index)?;
            for (slot, (n, d)) in specs.iter().enumerate() {
                if found.get(slot).is_some_and(Option::is_some) {
                    continue;
                }
                if !self.eq(name, n)? {
                    continue;
                }
                let desc = self.utf(c, m.descriptor_index)?;
                if self.eq(desc, d)? {
                    let value = found.get_mut(slot).ok_or_else(|| {
                        self.fail(StringBodyErrorKind::InvalidState("field slot absent"))
                    })?;
                    *value = Some(FieldId {
                        owner: class,
                        index,
                    });
                }
            }
            if found.iter().all(Option::is_some) {
                break;
            }
        }
        Ok(found)
    }
    fn matched(&mut self, c: &ClassFile, m: &Member, length: usize, spec: &[Op]) -> R<bool> {
        let Some(code) = self.code(m)? else {
            return Ok(false);
        };
        if code.code.len() != length || !code.exception_table.is_empty() {
            return Ok(false);
        }
        let mut pc = 0usize;
        for op in spec {
            self.pay(1)?;
            if code.code.get(pc).copied() != Some(op.code) {
                return Ok(false);
            }
            pc += 1;
            match op.arg {
                Arg::None => {}
                arg => {
                    self.pay(1)?;
                    let data = code.code.get(pc..pc + 2).ok_or_else(|| {
                        self.fail(StringBodyErrorKind::InvalidBinding("short recipe operand"))
                    })?;
                    let word = u16::from_be_bytes([data[0], data[1]]);
                    pc += 2;
                    let okay = match arg {
                        Arg::Imm(n) => word as i16 == n,
                        Arg::Class(name) => self.class_name(c, word, name)?,
                        Arg::Ref(tag, owner, name, desc) => {
                            self.member(c, word, tag, owner, name, desc)?
                        }
                        Arg::None => true,
                    };
                    if !okay {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(pc == length)
    }
}
#[derive(Clone, Copy)]
enum Arg {
    None,
    Imm(i16),
    Class(&'static str),
    Ref(u8, &'static str, &'static str, &'static str),
}
#[derive(Clone, Copy)]
struct Op {
    code: u8,
    arg: Arg,
}
const fn op(code: u8) -> Op {
    Op {
        code,
        arg: Arg::None,
    }
}
const fn imm(code: u8, n: i16) -> Op {
    Op {
        code,
        arg: Arg::Imm(n),
    }
}
const fn class(code: u8, name: &'static str) -> Op {
    Op {
        code,
        arg: Arg::Class(name),
    }
}
const fn field(code: u8, name: &'static str, desc: &'static str) -> Op {
    Op {
        code,
        arg: Arg::Ref(9, STRING, name, desc),
    }
}
const fn method(code: u8, owner: &'static str, name: &'static str, desc: &'static str) -> Op {
    Op {
        code,
        arg: Arg::Ref(10, owner, name, desc),
    }
}
const INIT: [Op; 10] = [
    op(4),
    field(0xb3, "COMPACT_STRINGS", "Z"),
    op(3),
    class(0xbd, OSF),
    field(
        0xb3,
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
    ),
    class(0xbb, CMP),
    op(0x59),
    method(0xb7, CMP, "<init>", "()V"),
    field(0xb3, "CASE_INSENSITIVE_ORDER", "Ljava/util/Comparator;"),
    op(0xb1),
];
const LENGTH: [Op; 7] = [
    op(0x2a),
    field(0xb4, "value", "[B"),
    op(0xbe),
    op(0x2a),
    method(0xb6, STRING, "coder", "()B"),
    op(0x7a),
    op(0xac),
];
const EMPTY: [Op; 8] = [
    op(0x2a),
    field(0xb4, "value", "[B"),
    op(0xbe),
    imm(0x9a, 7),
    op(4),
    imm(0xa7, 4),
    op(3),
    op(0xac),
];
const CODER: [Op; 7] = [
    field(0xb2, "COMPACT_STRINGS", "Z"),
    imm(0x99, 10),
    op(0x2a),
    field(0xb4, "coder", "B"),
    imm(0xa7, 4),
    op(4),
    op(0xac),
];
const CTOR: [Op; 3] = [op(0x2a), method(0xb7, OBJECT, "<init>", "()V"), op(0xb1)];
fn missing(p: &Probe, s: &'static str) -> Box<StringBodyError> {
    p.fail(StringBodyErrorKind::InvalidBinding(s))
}
pub fn probe_compact_string_bodies(
    registry: &DefinitionRegistry,
    bootstrap_loader: LoaderId,
    string: ClassId,
    limits: StringBodyDiscoveryLimits,
) -> R<StringBodyDiscovery> {
    let mut p = Probe {
        limit: limits.max_work_units,
        usage: Default::default(),
    };
    p.pay(1)?;
    let c = p.registry(registry.class(string))?;
    if p.registry(registry.defining_loader(string))? != bootstrap_loader
        || !p.class_name(c, c.this_class, STRING)?
    {
        return Err(missing(
            &p,
            "probe must designate canonical bootstrap String",
        ));
    }
    let init = p
        .find_methods(registry, string, &[("<clinit>", "()V")])?
        .first()
        .copied()
        .flatten();
    let Some(init) = init else {
        return Ok(StringBodyDiscovery {
            plan: None,
            usage: p.usage,
        });
    };
    if !p.matched(c, p.registry(registry.method(init))?, 22, &INIT)? {
        return Ok(StringBodyDiscovery {
            plan: None,
            usage: p.usage,
        });
    }
    if c.access_flags & (1 | 16 | 0x200 | 0x400) != 17 || !p.class_name(c, c.super_class, OBJECT)? {
        return Err(missing(&p, "String class role differs"));
    }
    let mut roots = [None; 3];
    for (id, definition) in registry.definition_rows() {
        p.pay(1)?;
        if p.registry(registry.defining_loader(id))? != bootstrap_loader {
            continue;
        }
        let mut name = None;
        for (slot, wanted) in [CMP, COMPARATOR, OSF].iter().enumerate() {
            if roots.get(slot).is_some_and(Option::is_some) {
                continue;
            }
            let actual = if let Some(n) = name {
                n
            } else {
                let Constant::Class { name_index } = p.cp(definition, definition.this_class)?
                else {
                    return Err(missing(&p, "definition self class missing"));
                };
                let n = p.utf(definition, *name_index)?;
                name = Some(n);
                n
            };
            if p.eq(actual, wanted)? {
                *roots
                    .get_mut(slot)
                    .ok_or_else(|| missing(&p, "root slot absent"))? = Some(id);
            }
        }
        if roots.iter().all(Option::is_some) {
            break;
        }
    }
    let roots = [
        roots[0].ok_or_else(|| missing(&p, "comparator implementation missing"))?,
        roots[1].ok_or_else(|| missing(&p, "Comparator interface missing"))?,
        roots[2].ok_or_else(|| missing(&p, "ObjectStreamField missing"))?,
    ];
    let comparator = p.registry(registry.class(roots[0]))?;
    let interface = p.registry(registry.class(roots[1]))?;
    let stream = p.registry(registry.class(roots[2]))?;
    if comparator.access_flags & (0x200 | 0x400) != 0
        || interface.access_flags & 0x201 != 0x201
        || stream.access_flags & (1 | 0x200 | 0x400) != 1
        || !p.class_name(comparator, comparator.super_class, OBJECT)?
    {
        return Err(missing(&p, "private type kind differs"));
    }
    let mut edge = None;
    for (i, &cp) in comparator.interfaces.iter().enumerate() {
        p.pay(1)?;
        if p.class_name(comparator, cp, COMPARATOR)? {
            edge = Some(i);
            break;
        }
    }
    let edge = edge.ok_or_else(|| missing(&p, "comparator has no canonical direct interface"))?;
    let methods = p.find_methods(
        registry,
        string,
        &[("length", "()I"), ("isEmpty", "()Z"), ("coder", "()B")],
    )?;
    let methods = [
        methods[0].ok_or_else(|| missing(&p, "length missing"))?,
        methods[1].ok_or_else(|| missing(&p, "isEmpty missing"))?,
        methods[2].ok_or_else(|| missing(&p, "coder missing"))?,
        init,
    ];
    for (slot, id) in methods.iter().enumerate() {
        let m = p.registry(registry.method(*id))?;
        let visible = if slot == 2 {
            0
        } else if slot == 3 {
            8
        } else {
            1
        };
        let mask = if slot == 3 {
            8 | 0x100 | 0x400 | 32
        } else {
            7 | 8 | 0x100 | 0x400 | 32
        };
        if m.access_flags & mask != visible {
            return Err(missing(&p, "String method flags differ"));
        }
    }
    for (id, len, body) in [
        (methods[0], 11, LENGTH.as_slice()),
        (methods[1], 14, EMPTY.as_slice()),
        (methods[2], 15, CODER.as_slice()),
    ] {
        if !p.matched(c, p.registry(registry.method(id))?, len, body)? {
            return Err(missing(&p, "String instance Code recipe differs"));
        }
    }
    let constructor = p.find_methods(registry, roots[0], &[("<init>", "()V")])?[0]
        .ok_or_else(|| missing(&p, "private comparator constructor missing"))?;
    let ctor = p.registry(registry.method(constructor))?;
    if ctor.access_flags & (7 | 8 | 0x100 | 0x400 | 32) != 2
        || !p.matched(comparator, ctor, 5, &CTOR)?
    {
        return Err(missing(&p, "private comparator constructor differs"));
    }
    let fields = p.find_fields(
        registry,
        string,
        &[
            ("value", "[B"),
            ("coder", "B"),
            ("COMPACT_STRINGS", "Z"),
            ("serialPersistentFields", "[Ljava/io/ObjectStreamField;"),
            ("CASE_INSENSITIVE_ORDER", "Ljava/util/Comparator;"),
        ],
    )?;
    let fields = [
        fields[0].ok_or_else(|| missing(&p, "value field missing"))?,
        fields[1].ok_or_else(|| missing(&p, "coder field missing"))?,
        fields[2].ok_or_else(|| missing(&p, "compact flag missing"))?,
        fields[3].ok_or_else(|| missing(&p, "serial fields missing"))?,
        fields[4].ok_or_else(|| missing(&p, "comparator field missing"))?,
    ];
    for (i, id) in fields.iter().enumerate() {
        let flags = p.registry(registry.field(*id))?.access_flags;
        if flags & (7 | 8 | 16) != [18, 18, 24, 26, 25][i] {
            return Err(missing(&p, "String field flags differ"));
        }
    }
    let method_keys = [
        MemberKey::of(p.registry(registry.method(methods[0]))?),
        MemberKey::of(p.registry(registry.method(methods[1]))?),
        MemberKey::of(p.registry(registry.method(methods[2]))?),
        MemberKey::of(p.registry(registry.method(methods[3]))?),
    ];
    let field_keys = [
        MemberKey::of(p.registry(registry.field(fields[0]))?),
        MemberKey::of(p.registry(registry.field(fields[1]))?),
        MemberKey::of(p.registry(registry.field(fields[2]))?),
        MemberKey::of(p.registry(registry.field(fields[3]))?),
        MemberKey::of(p.registry(registry.field(fields[4]))?),
    ];
    let plan = CompactStringBodyPlan {
        registry: registry.identity(),
        bootstrap: bootstrap_loader,
        string,
        roots,
        methods,
        method_keys,
        constructor,
        constructor_key: MemberKey::of(ctor),
        fields,
        field_keys,
        comparator_edge: edge,
    };
    Ok(StringBodyDiscovery {
        plan: Some(plan),
        usage: p.usage,
    })
}

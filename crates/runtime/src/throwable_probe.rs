//! Allocation-free paid discovery of the immutable native/field ABI.
use super::*;
use crabspot_classfile::{ClassFile, Constant, JavaString};
const THROWABLE: &str = "java/lang/Throwable";
const ROLES: [&str; 11] = [
    "java/lang/Object",
    "java/lang/Exception",
    "java/lang/RuntimeException",
    "java/lang/ArithmeticException",
    "java/lang/NullPointerException",
    "java/lang/AssertionError",
    "java/lang/StackTraceElement",
    "java/util/Collections",
    "java/util/Collections$EmptySet",
    "java/util/Collections$EmptyList",
    "java/util/Collections$EmptyMap",
];
const CTORS: [&str; 4] = [
    "()V",
    "(Ljava/lang/String;)V",
    "(Ljava/lang/String;Ljava/lang/Throwable;)V",
    "(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V",
];
struct Probe {
    limit: u64,
    usage: ThrowableDiscoveryUsage,
}
impl Probe {
    fn fail(&self, k: ThrowableErrorKind) -> Box<ThrowableAccessError> {
        Box::new(ThrowableAccessError {
            kind: k,
            discovery_usage: self.usage,
        })
    }
    fn pay(&mut self, n: u64) -> R<()> {
        let required = self.usage.work_units.checked_add(n).ok_or_else(|| {
            self.fail(ThrowableErrorKind::Overflow {
                resource: ThrowableResource::DiscoveryWork,
            })
        })?;
        if required > self.limit {
            return Err(self.fail(ThrowableErrorKind::Limit {
                resource: ThrowableResource::DiscoveryWork,
                limit: self.limit,
                required,
            }));
        }
        self.usage.work_units = required;
        Ok(())
    }
    fn registry<T>(&self, r: Result<T, RegistryError>) -> R<T> {
        r.map_err(|e| self.fail(ThrowableErrorKind::Registry(Box::new(e))))
    }
    fn utf<'a>(&mut self, c: &'a ClassFile, i: u16) -> R<&'a JavaString> {
        self.pay(1)?;
        self.registry(c.constant_pool.utf8(i).map_err(RegistryError::Parse))
    }
    fn eq(&mut self, a: &JavaString, b: &str) -> R<bool> {
        if a.units().len() != b.len() {
            return Ok(false);
        }
        self.pay(b.len() as u64)?;
        Ok(a.equals_ascii(b))
    }
    fn name<'a>(&mut self, c: &'a ClassFile, i: u16) -> R<&'a JavaString> {
        self.pay(1)?;
        let Constant::Class { name_index } =
            self.registry(c.constant_pool.get(i).map_err(RegistryError::Parse))?
        else {
            return Err(self.fail(ThrowableErrorKind::InvalidBinding(
                "class role has invalid Class constant",
            )));
        };
        self.utf(c, *name_index)
    }
    fn ordinary(&self, c: &ClassFile, public: bool, concrete: bool) -> R<()> {
        if c.access_flags & 0x8200 != 0
            || (public && c.access_flags & 1 == 0)
            || (concrete && c.access_flags & 0x400 != 0)
        {
            return Err(self.fail(ThrowableErrorKind::InvalidBinding(
                "role class flags differ",
            )));
        }
        Ok(())
    }
    fn constructors(
        &mut self,
        c: &ClassFile,
        id: ClassId,
        wanted: &[&str],
        access: &[u16],
    ) -> R<[Option<(MethodId, Key)>; 4]> {
        let mut found = [None; 4];
        for (index, m) in c.methods.iter().enumerate() {
            self.pay(1)?;
            let n = self.utf(c, m.name_index)?;
            if !self.eq(n, "<init>")? {
                continue;
            }
            let d = self.utf(c, m.descriptor_index)?;
            for (j, target) in wanted.iter().enumerate() {
                if found[j].is_none() && self.eq(d, target)? {
                    if m.access_flags & 7 != access[j] || m.access_flags & (8 | 0x100 | 0x400) != 0
                    {
                        return Err(self.fail(ThrowableErrorKind::InvalidBinding(
                            "constructor role flags differ",
                        )));
                    }
                    found[j] = Some((MethodId { owner: id, index }, Key::of(m)));
                    break;
                }
            }
            if found[..wanted.len()].iter().all(Option::is_some) {
                break;
            }
        }
        if found[..wanted.len()].iter().any(Option::is_none) {
            return Err(self.fail(ThrowableErrorKind::InvalidBinding(
                "required own constructor absent",
            )));
        }
        Ok(found)
    }
    fn parent(
        &mut self,
        r: &DefinitionRegistry,
        id: ClassId,
        parent: Option<(ClassId, &str)>,
    ) -> R<()> {
        self.pay(1)?;
        let c = self.registry(r.class(id))?;
        match parent {
            None => {
                if c.super_class != 0 {
                    return Err(self.fail(ThrowableErrorKind::InvalidBinding("Object is not root")));
                }
            }
            Some((expected, name)) => {
                let n = self.name(c, c.super_class)?;
                if !self.eq(n, name)?
                    || self.registry(r.defining_loader(expected))?
                        != self.registry(r.defining_loader(id))?
                {
                    return Err(self.fail(ThrowableErrorKind::InvalidBinding(
                        "role superclass differs",
                    )));
                }
            }
        }
        Ok(())
    }
}
pub fn probe_throwable_support(
    r: &DefinitionRegistry,
    boot: LoaderId,
    throwable: ClassId,
    limits: ThrowableDiscoveryLimits,
) -> R<ThrowableDiscovery> {
    r.loader_index(boot)?;
    let c = r.class(throwable)?;
    let mut p = Probe {
        limit: limits.max_work_units,
        usage: Default::default(),
    };
    p.pay(1)?;
    let n = p.name(c, c.this_class)?;
    if !p.eq(n, THROWABLE)? || p.registry(r.defining_loader(throwable))? != boot {
        return Err(p.fail(ThrowableErrorKind::InvalidBinding(
            "designated Throwable name/loader differs",
        )));
    }
    p.ordinary(c, true, true)?;
    let mut native = None;
    for (index, m) in c.methods.iter().enumerate() {
        p.pay(1)?;
        let n = p.utf(c, m.name_index)?;
        if !p.eq(n, "fillInStackTrace")? {
            continue;
        }
        let d = p.utf(c, m.descriptor_index)?;
        if !p.eq(d, "(I)Ljava/lang/Throwable;")? {
            continue;
        }
        if m.access_flags != 0x0102 {
            return Err(p.fail(ThrowableErrorKind::InvalidBinding(
                "native capture role flags differ",
            )));
        }
        native = Some((
            MethodId {
                owner: throwable,
                index,
            },
            Key::of(m),
        ));
        break;
    }
    let Some((native, native_key)) = native else {
        return Ok(ThrowableDiscovery {
            plan: None,
            usage: p.usage,
        });
    };
    let mut fields = [None; 2];
    let mut ordinals = [0; 2];
    let mut ordinal = 0;
    for (index, f) in c.fields.iter().enumerate() {
        p.pay(1)?;
        let n = p.utf(c, f.name_index)?;
        for (j, (name, desc)) in [("backtrace", "Ljava/lang/Object;"), ("depth", "I")]
            .iter()
            .enumerate()
        {
            if fields[j].is_some() || !p.eq(n, name)? {
                continue;
            }
            let d = p.utf(c, f.descriptor_index)?;
            if p.eq(d, desc)? {
                if f.access_flags != 0x82 {
                    return Err(p.fail(ThrowableErrorKind::InvalidBinding(
                        "native field flags differ",
                    )));
                }
                fields[j] = Some((
                    FieldId {
                        owner: throwable,
                        index,
                    },
                    Key::of(f),
                ));
                ordinals[j] = ordinal;
                break;
            }
        }
        if f.access_flags & 8 == 0 {
            ordinal += 1
        }
        if fields.iter().all(Option::is_some) {
            break;
        }
    }
    let fields = [
        fields[0]
            .ok_or_else(|| p.fail(ThrowableErrorKind::InvalidBinding("backtrace field absent")))?,
        fields[1]
            .ok_or_else(|| p.fail(ThrowableErrorKind::InvalidBinding("depth field absent")))?,
    ];
    let field_keys = [fields[0].1, fields[1].1];
    let fields = [fields[0].0, fields[1].0];
    let tc = p.constructors(c, throwable, &CTORS, &[1, 1, 1, 4])?;
    let mut roles = [None; 11];
    for (id, definition) in r.definition_rows() {
        p.pay(1)?;
        if p.registry(r.defining_loader(id))? != boot {
            continue;
        }
        let n = p.name(definition, definition.this_class)?;
        for (j, name) in ROLES.iter().enumerate() {
            if roles[j].is_none() && p.eq(n, name)? {
                roles[j] = Some(id);
                break;
            }
        }
        if roles.iter().all(Option::is_some) {
            break;
        }
    }
    let mut ids = [throwable; 11];
    for (j, id) in roles.into_iter().enumerate() {
        ids[j] = id.ok_or_else(|| {
            p.fail(ThrowableErrorKind::InvalidBinding(
                "required bootstrap role absent",
            ))
        })?
    }
    for (j, id) in ids.iter().copied().enumerate() {
        p.pay(1)?;
        let d = p.registry(r.class(id))?;
        p.ordinary(d, j <= 6, j <= 3)?;
    }
    p.parent(r, ids[0], None)?;
    p.parent(r, throwable, Some((ids[0], ROLES[0])))?;
    p.parent(r, ids[1], Some((throwable, THROWABLE)))?;
    p.parent(r, ids[2], Some((ids[1], ROLES[1])))?;
    p.parent(r, ids[3], Some((ids[2], ROLES[2])))?;
    p.pay(1)?;
    let ec = p.constructors(p.registry(r.class(ids[1]))?, ids[1], &CTORS[1..2], &[1])?;
    p.pay(1)?;
    let rc = p.constructors(p.registry(r.class(ids[2]))?, ids[2], &CTORS[1..2], &[1])?;
    p.pay(1)?;
    let ac = p.constructors(
        p.registry(r.class(ids[3]))?,
        ids[3],
        &[CTORS[1], CTORS[0]],
        &[1, 1],
    )?;
    let missing = || {
        p.fail(ThrowableErrorKind::InvalidBinding(
            "constructor discovery incomplete",
        ))
    };
    let ctors = [
        tc[0].ok_or_else(missing)?,
        tc[1].ok_or_else(missing)?,
        tc[2].ok_or_else(missing)?,
        tc[3].ok_or_else(missing)?,
        ec[0].ok_or_else(missing)?,
        rc[0].ok_or_else(missing)?,
        ac[0].ok_or_else(missing)?,
        ac[1].ok_or_else(missing)?,
    ];
    let ctor_keys = ctors.map(|(_, key)| key);
    let ctors = ctors.map(|(id, _)| id);
    Ok(ThrowableDiscovery {
        plan: Some(ThrowablePlan {
            registry: r.identity(),
            boot,
            throwable,
            object: ids[0],
            public: ThrowablePublicRoles {
                exception: ids[1],
                runtime_exception: ids[2],
                arithmetic_exception: ids[3],
                null_pointer_exception: ids[4],
                assertion_error: ids[5],
                stack_trace_element: ids[6],
            },
            roots: [ids[7], ids[8], ids[9], ids[10]],
            native,
            native_key,
            fields,
            field_keys,
            ordinals,
            ctors,
            ctor_keys,
        }),
        usage: p.usage,
    })
}

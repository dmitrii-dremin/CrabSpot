//! Allocation-free paid discovery, independent of executable verification.
use super::*;
use crabspot_classfile::{ClassFile, Constant, JavaString};
const NAMES: [&str; 5] = [
    "java/lang/IndexOutOfBoundsException",
    "java/lang/ArrayIndexOutOfBoundsException",
    "java/lang/NegativeArraySizeException",
    "java/lang/ArrayStoreException",
    "java/lang/ClassCastException",
];
struct Probe {
    limit: u64,
    usage: OperandExceptionDiscoveryUsage,
}
impl Probe {
    fn fail(&self, kind: OperandExceptionErrorKind) -> Box<OperandExceptionError> {
        Box::new(OperandExceptionError {
            kind,
            discovery_usage: self.usage,
        })
    }
    fn pay(&mut self, n: u64) -> R<()> {
        self.usage.work_units = checked(
            self.usage.work_units,
            n,
            self.limit,
            OperandExceptionResource::DiscoveryWork,
        )
        .map_err(|mut e| {
            e.discovery_usage = self.usage;
            e
        })?;
        Ok(())
    }
    fn reg<T>(&self, r: Result<T, RegistryError>) -> R<T> {
        r.map_err(|e| self.fail(OperandExceptionErrorKind::Registry(Box::new(e))))
    }
    fn utf<'a>(&mut self, c: &'a ClassFile, i: u16) -> R<&'a JavaString> {
        self.pay(1)?;
        self.reg(c.constant_pool.utf8(i).map_err(RegistryError::Parse))
    }
    fn name<'a>(&mut self, c: &'a ClassFile, i: u16) -> R<&'a JavaString> {
        self.pay(1)?;
        let Constant::Class { name_index } =
            self.reg(c.constant_pool.get(i).map_err(RegistryError::Parse))?
        else {
            return Err(self.fail(OperandExceptionErrorKind::InvalidBinding(
                "invalid role Class",
            )));
        };
        self.utf(c, *name_index)
    }
    fn eq(&mut self, a: &JavaString, b: &str) -> R<bool> {
        if a.units().len() != b.len() {
            return Ok(false);
        }
        self.pay(b.len() as u64)?;
        Ok(a.equals_ascii(b))
    }
}
pub fn probe_operand_exceptions(
    registry: &DefinitionRegistry,
    throwable: &ThrowablePlan,
    limits: OperandExceptionDiscoveryLimits,
) -> R<OperandExceptionDiscovery> {
    if registry.identity() != throwable.registry {
        return Err(invalid("operand discovery registry differs"));
    }
    registry.loader_index(throwable.boot)?;
    let mut p = Probe {
        limit: limits.max_work_units,
        usage: Default::default(),
    };
    let mut found = [None; 5];
    for (id, c) in registry.definition_rows() {
        p.pay(1)?;
        if p.reg(registry.defining_loader(id))? != throwable.boot {
            continue;
        }
        let n = p.name(c, c.this_class)?;
        for (j, wanted) in NAMES.iter().enumerate() {
            if found[j].is_none() && p.eq(n, wanted)? {
                found[j] = Some(id);
                break;
            }
        }
        if found.iter().all(Option::is_some) {
            break;
        }
    }
    let mut ids = [throwable.throwable; 5];
    for (j, id) in found.into_iter().enumerate() {
        ids[j] = id.ok_or_else(|| {
            p.fail(OperandExceptionErrorKind::InvalidBinding(
                "required operand class absent",
            ))
        })?;
    }
    let mut ctors = [throwable.ctors[0]; 5];
    let mut keys = [throwable.ctor_keys[0]; 5];
    for (j, id) in ids.iter().copied().enumerate() {
        p.pay(1)?;
        let c = p.reg(registry.class(id))?;
        if c.access_flags & 0x8600 != 0 || c.access_flags & 1 == 0 {
            return Err(p.fail(OperandExceptionErrorKind::InvalidBinding(
                "operand class flags differ",
            )));
        }
        let n = p.name(c, c.super_class)?;
        let parent = if j == 1 {
            NAMES[0]
        } else {
            "java/lang/RuntimeException"
        };
        if !p.eq(n, parent)? {
            return Err(p.fail(OperandExceptionErrorKind::InvalidBinding(
                "operand superclass differs",
            )));
        }
        let mut ctor = None;
        for (index, m) in c.methods.iter().enumerate() {
            p.pay(1)?;
            let n = p.utf(c, m.name_index)?;
            if !p.eq(n, "<init>")? {
                continue;
            }
            let d = p.utf(c, m.descriptor_index)?;
            if !p.eq(d, "(Ljava/lang/String;)V")? {
                continue;
            }
            if m.access_flags & 7 != 1 || m.access_flags & (8 | 0x100 | 0x400) != 0 {
                return Err(p.fail(OperandExceptionErrorKind::InvalidBinding(
                    "operand constructor flags differ",
                )));
            }
            ctor = Some((MethodId { owner: id, index }, Key::of(m)));
            break;
        }
        let (method, key) = ctor.ok_or_else(|| {
            p.fail(OperandExceptionErrorKind::InvalidBinding(
                "operand String constructor absent",
            ))
        })?;
        ctors[j] = method;
        keys[j] = key;
    }
    Ok(OperandExceptionDiscovery {
        plan: OperandExceptionPlan {
            registry: registry.identity(),
            throwable: throwable.clone(),
            ids,
            ctors,
            keys,
        },
        usage: p.usage,
    })
}

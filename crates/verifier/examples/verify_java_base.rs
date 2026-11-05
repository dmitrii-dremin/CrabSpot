//! Reference corpus tool. The Python driver supplies every class from a local JDK 21 java.base.jmod.
use crabspot_classfile::{AttributeBody, Constant, parse};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, RegistryOptions, SameLoaderResolver, build_hierarchy,
};
use crabspot_verifier::{
    VerificationCore, VerificationEnvironment, VerificationEnvironmentOptions, verify_class_types,
};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_CLASSES: usize = 16_384;
const MAX_TOTAL: usize = 128 << 20;

fn bytes<const N: usize>(reader: &mut impl Read) -> Result<[u8; N]> {
    let mut result = [0; N];
    reader.read_exact(&mut result)?;
    Ok(result)
}

fn read_entry(reader: &mut impl Read, length: usize) -> Result<Vec<u8>> {
    let mut value = Vec::new();
    reader.take(length as u64).read_to_end(&mut value)?;
    if value.len() != length {
        return Err("truncated corpus entry".into());
    }
    Ok(value)
}

fn check_module(bytes: &[u8]) -> Result<()> {
    let class = parse(bytes)?;
    let module = class
        .attributes
        .iter()
        .find_map(|a| match &a.body {
            AttributeBody::Module(m) => Some(m),
            _ => None,
        })
        .ok_or("missing module attribute")?;
    let Constant::Module { name_index } = class.constant_pool.get(module.module_name_index)? else {
        return Err("missing module name".into());
    };
    if !class.name()?.equals_ascii("module-info")
        || class.access_flags != 0x8000
        || !class
            .constant_pool
            .utf8(*name_index)?
            .equals_ascii("java.base")
    {
        return Err("expected the java.base module descriptor".into());
    }
    Ok(())
}

fn run(reader: &mut impl Read) -> Result<()> {
    let count = u32::from_be_bytes(bytes(reader)?) as usize;
    if !(2..=MAX_CLASSES).contains(&count) {
        return Err("invalid corpus class count".into());
    }
    let options = RegistryOptions {
        max_definitions: MAX_CLASSES,
        max_total_definition_bytes: MAX_TOTAL,
        ..Default::default()
    };
    let mut registry = DefinitionRegistry::new(options)?;
    let boot = registry.create_loader()?;
    let mut previous = String::new();
    let mut targets = Vec::new();
    let mut module = false;
    let mut total = 0usize;
    for _ in 0..count {
        let name_size = u16::from_be_bytes(bytes(reader)?) as usize;
        let size = u32::from_be_bytes(bytes(reader)?) as usize;
        total = total.checked_add(size).ok_or("class bytes overflow")?;
        if !(1..=4096).contains(&name_size)
            || !(1..=options.max_definition_bytes).contains(&size)
            || total > MAX_TOTAL
        {
            return Err("name or class bytes exceed corpus limit".into());
        }
        let name = String::from_utf8(read_entry(reader, name_size)?)?;
        if name <= previous {
            return Err(format!("duplicate or out-of-order class {name}").into());
        }
        let data = read_entry(reader, size)?;
        if name == "module-info" {
            check_module(&data)?;
            module = true;
        } else {
            let id = registry
                .define(
                    boot,
                    &name.encode_utf16().collect::<Vec<_>>(),
                    &data,
                    Default::default(),
                )
                .map_err(|error| format!("admission of {name}: {error}"))?;
            targets.push(id);
        }
        previous = name;
    }
    if reader.read(&mut [0])? != 0 || !module || targets.len() + 1 != count {
        return Err("incomplete java.base inventory or trailing bytes".into());
    }
    let lookup = |name: &str| -> Result<ClassId> {
        registry
            .lookup_class(boot, &name.encode_utf16().collect::<Vec<_>>())?
            .ok_or_else(|| format!("missing bootstrap definition {name}").into())
    };
    let core = VerificationCore {
        bootstrap_loader: boot,
        object: lookup("java/lang/Object")?,
        class: lookup("java/lang/Class")?,
        string: lookup("java/lang/String")?,
        throwable: lookup("java/lang/Throwable")?,
        cloneable: lookup("java/lang/Cloneable")?,
        serializable: lookup("java/io/Serializable")?,
        method_type: lookup("java/lang/invoke/MethodType")?,
        method_handle: lookup("java/lang/invoke/MethodHandle")?,
    };
    let limits = VerificationEnvironmentOptions::default();
    let mut plans = Vec::new();
    let (mut rows, mut edges) = (0u64, 0u64);
    for &id in &targets {
        let plan = build_hierarchy(&registry, id, &mut SameLoaderResolver, Default::default())
            .map_err(|error| format!("hierarchy for {id:?}: {error}"))?;
        rows += plan.classes().len() as u64;
        if rows > limits.max_class_rows {
            return Err("retained hierarchy row limit".into());
        }
        for &class in plan.classes() {
            let layout = plan.layout(class).ok_or("missing layout")?;
            edges += layout.direct_interfaces().len() as u64
                + u64::from(layout.direct_superclass().is_some());
            if edges > limits.max_edges {
                return Err("retained hierarchy edge limit".into());
            }
        }
        plans.push(plan);
    }
    let refs = plans.iter().collect::<Vec<_>>();
    let env = VerificationEnvironment::new(&registry, &refs, &[], core, limits)?;
    let (mut checked, mut bodies, mut bodyless) = (0, 0u64, 0u64);
    for id in targets {
        let class = registry.class(id)?;
        let name = class.name()?.escaped();
        let proof = verify_class_types(&env, id, Default::default())
            .map_err(|error| format!("verification of {name}: {error}"))?;
        let usage = proof.usage();
        let expected_bodies = class
            .methods
            .iter()
            .filter(|m| {
                m.attributes
                    .iter()
                    .any(|a| matches!(a.body, AttributeBody::Code(_)))
            })
            .count() as u64;
        let declarations = class.methods.len() as u64;
        if proof.class_id() != id
            || usage.methods_started != declarations
            || usage.code_bodies != expected_bodies
            || usage.bodyless != declarations - expected_bodies
        {
            return Err(format!("incomplete proof of {name}").into());
        }
        checked += 1;
        bodies += expected_bodies;
        bodyless += declarations - expected_bodies;
    }
    if checked + 1 != count {
        return Err("incomplete java.base proof set".into());
    }
    println!(
        "java.base proved_classes={checked} code_bodies={bodies} bodyless={bodyless} declarations={} module_descriptors=1",
        bodies + bodyless
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let file = File::open(args.next().ok_or("use tools/verify_java_base.py")?)?;
    if args.next().is_some() || file.metadata()?.len() > 144 << 20 {
        return Err("expected one bounded packed corpus".into());
    }
    run(&mut BufReader::new(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_counts_lengths_and_short_input() {
        for count in [0u32, 1, MAX_CLASSES as u32 + 1] {
            assert!(run(&mut &count.to_be_bytes()[..]).is_err());
        }
        for (name_len, size) in [(0u16, 1u32), (4097, 1), (1, 0), (1, 1 << 25)] {
            let input = [
                &2u32.to_be_bytes()[..],
                &name_len.to_be_bytes(),
                &size.to_be_bytes(),
            ]
            .concat();
            assert!(run(&mut input.as_slice()).is_err());
        }
        assert!(read_entry(&mut &b"A"[..], 2).is_err());
        assert_eq!(read_entry(&mut &b"abc"[..], 3).unwrap(), b"abc");
    }
}

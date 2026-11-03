//! Symbolic module metadata and file-local checks from JVMS 4.7.25.
use std::collections::BTreeSet;

use crate::{
    Constant, ConstantPool, ErrorKind, JavaString, ParseError, descriptor, reader::Reader,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub module_name_index: u16,
    pub module_flags: u16,
    pub module_version_index: u16,
    pub requires: Vec<ModuleRequires>,
    pub exports: Vec<ModuleExports>,
    pub opens: Vec<ModuleOpens>,
    pub uses: Vec<u16>,
    pub provides: Vec<ModuleProvides>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRequires {
    pub requires_index: u16,
    pub requires_flags: u16,
    pub requires_version_index: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleExports {
    pub exports_index: u16,
    pub exports_flags: u16,
    pub exports_to_index: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleOpens {
    pub opens_index: u16,
    pub opens_flags: u16,
    pub opens_to_index: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleProvides {
    pub provides_index: u16,
    pub provides_with_index: Vec<u16>,
}

/// JVMS 4.2.3 module encoding, rather than Java source identifier grammar.
pub(crate) fn valid_module_name(name: &[u16]) -> bool {
    let mut units = name.iter().copied();
    while let Some(unit) = units.next() {
        match unit {
            0..=31 | 58 | 64 => return false,
            92 if !matches!(units.next(), Some(92 | 58 | 64)) => return false,
            _ => {}
        }
    }
    true
}

pub(crate) fn valid_package_name(name: &[u16]) -> bool {
    descriptor::valid_class_name(name)
}

fn module_name(pool: &ConstantPool, index: u16) -> Result<&JavaString, ParseError> {
    let Constant::Module { name_index } = pool.get(index)? else {
        return Err(pool.index_error(index, "Module"));
    };
    let name = pool.utf8(*name_index)?;
    if !valid_module_name(name.units()) {
        return Err(pool.index_error(index, "a valid module name"));
    }
    Ok(name)
}

fn package_name(pool: &ConstantPool, index: u16) -> Result<&JavaString, ParseError> {
    let Constant::Package { name_index } = pool.get(index)? else {
        return Err(pool.index_error(index, "Package"));
    };
    let name = pool.utf8(*name_index)?;
    if !valid_package_name(name.units()) {
        return Err(pool.index_error(index, "a valid package name"));
    }
    Ok(name)
}

fn service_name(pool: &ConstantPool, index: u16) -> Result<&JavaString, ParseError> {
    let name = pool.class_name(index)?;
    if !descriptor::valid_class_name(name.units()) {
        return Err(pool.index_error(index, "a non-array class name"));
    }
    Ok(name)
}

fn optional_version(pool: &ConstantPool, index: u16) -> Result<(), ParseError> {
    if index != 0 {
        pool.utf8(index)?;
    }
    Ok(())
}

fn unique<'a>(
    names: &mut BTreeSet<&'a [u16]>,
    name: &'a JavaString,
    reader: &Reader<'_>,
) -> Result<(), ParseError> {
    if !names.insert(name.units()) {
        return Err(reader.invalid("duplicate name in module metadata table"));
    }
    Ok(())
}

fn names(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    get_name: fn(&ConstantPool, u16) -> Result<&JavaString, ParseError>,
) -> Result<Vec<u16>, ParseError> {
    let count = reader.u2()?;
    let mut indices = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..count {
        let index = reader.u2()?;
        unique(&mut seen, get_name(pool, index)?, reader)?;
        indices.push(index);
    }
    Ok(indices)
}

/// Decode one bounded payload. The enclosing attribute parser checks consumption.
/// Resolution, package membership, and the module graph are later-stage concerns.
pub(crate) fn parse_module(
    reader: &mut Reader<'_>,
    pool: &ConstantPool,
    major: u16,
) -> Result<Module, ParseError> {
    let module_name_index = reader.u2()?;
    let current_name = module_name(pool, module_name_index)?;
    let is_base = current_name.equals_ascii("java.base");
    let module_flags = reader.u2()?;
    let module_version_index = reader.u2()?;
    optional_version(pool, module_version_index)?;

    let requires_count = reader.u2()?;
    if is_base && requires_count != 0 {
        return Err(reader.invalid("java.base must have an empty requires table"));
    }
    let mut requires = Vec::new();
    let mut seen = BTreeSet::new();
    let mut requires_base = false;
    for _ in 0..requires_count {
        let requires_index = reader.u2()?;
        let name = module_name(pool, requires_index)?;
        unique(&mut seen, name, reader)?;
        let requires_flags = reader.u2()?;
        let requires_version_index = reader.u2()?;
        optional_version(pool, requires_version_index)?;
        if name.equals_ascii("java.base") {
            if requires_flags & 0x1000 != 0 || (major >= 54 && requires_flags & 0x0060 != 0) {
                return Err(reader.invalid("invalid java.base requires flags"));
            }
            requires_base = true;
        }
        requires.push(ModuleRequires {
            requires_index,
            requires_flags,
            requires_version_index,
        });
    }
    if !is_base && !requires_base {
        return Err(reader.invalid("a module other than java.base must require java.base"));
    }

    let exports_count = reader.u2()?;
    let mut exports = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..exports_count {
        let exports_index = reader.u2()?;
        unique(&mut seen, package_name(pool, exports_index)?, reader)?;
        let exports_flags = reader.u2()?;
        let exports_to_index = names(reader, pool, module_name)?;
        exports.push(ModuleExports {
            exports_index,
            exports_flags,
            exports_to_index,
        });
    }

    let opens_count = reader.u2()?;
    if module_flags & 0x0020 != 0 && opens_count != 0 {
        return Err(reader.invalid("an open module must have an empty opens table"));
    }
    let mut opens = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..opens_count {
        let opens_index = reader.u2()?;
        unique(&mut seen, package_name(pool, opens_index)?, reader)?;
        let opens_flags = reader.u2()?;
        let opens_to_index = names(reader, pool, module_name)?;
        opens.push(ModuleOpens {
            opens_index,
            opens_flags,
            opens_to_index,
        });
    }

    let uses = names(reader, pool, service_name)?;
    let provides_count = reader.u2()?;
    let mut provides = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..provides_count {
        let provides_index = reader.u2()?;
        unique(&mut seen, service_name(pool, provides_index)?, reader)?;
        let provides_with_index = names(reader, pool, service_name)?;
        if provides_with_index.is_empty() {
            return Err(reader.invalid("provides_with_count must be nonzero"));
        }
        provides.push(ModuleProvides {
            provides_index,
            provides_with_index,
        });
    }

    Ok(Module {
        module_name_index,
        module_flags,
        module_version_index,
        requires,
        exports,
        opens,
        uses,
        provides,
    })
}

/// Check the explicitly declared ModulePackages inventory without resolving classes.
pub(crate) fn validate_module_packages(
    module: &Module,
    pool: &ConstantPool,
    packages: &[u16],
    offset: usize,
) -> Result<(), ParseError> {
    let invalid = |reason| {
        ParseError::new(
            offset,
            "ModulePackages",
            ErrorKind::InvalidStructure(reason),
        )
    };
    let mut inventory = BTreeSet::new();
    for index in packages {
        if !inventory.insert(package_name(pool, *index)?.units()) {
            return Err(invalid("duplicate package name in ModulePackages"));
        }
    }
    for index in module
        .exports
        .iter()
        .map(|entry| entry.exports_index)
        .chain(module.opens.iter().map(|entry| entry.opens_index))
    {
        if !inventory.contains(package_name(pool, index)?.units()) {
            return Err(invalid(
                "ModulePackages omits an exported or opened package",
            ));
        }
    }
    for index in module
        .provides
        .iter()
        .flat_map(|entry| entry.provides_with_index.iter())
    {
        let name = service_name(pool, *index)?.units();
        let package = name
            .iter()
            .rposition(|unit| *unit == 47)
            .and_then(|position| name.get(..position))
            .unwrap_or(&[]);
        if !inventory.contains(package) {
            return Err(invalid(
                "ModulePackages omits a service implementation package",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constant_pool::parse_pool;

    #[derive(Default)]
    struct PoolBuilder(Vec<Vec<u8>>);

    impl PoolBuilder {
        fn utf8(&mut self, text: &str) -> u16 {
            assert!(text.is_ascii());
            let mut bytes = vec![1];
            bytes.extend_from_slice(&(text.len() as u16).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
            self.0.push(bytes);
            self.0.len() as u16
        }

        fn named(&mut self, tag: u8, text: &str) -> u16 {
            let index = self.utf8(text);
            let mut bytes = vec![tag];
            bytes.extend_from_slice(&index.to_be_bytes());
            self.0.push(bytes);
            self.0.len() as u16
        }

        fn finish(self) -> ConstantPool {
            let mut bytes = ((self.0.len() + 1) as u16).to_be_bytes().to_vec();
            bytes.extend(self.0.into_iter().flatten());
            parse_pool(&mut Reader::new(&bytes, 0, "test pool"), 65).unwrap()
        }
    }

    struct Fixture {
        pool: ConstantPool,
        module: u16,
        base: u16,
        friend: u16,
        duplicate_base: u16,
        duplicate_friend: u16,
        package: u16,
        duplicate_package: u16,
        other_package: u16,
        service: u16,
        duplicate_service: u16,
        implementation: u16,
        duplicate_implementation: u16,
        array: u16,
        version: u16,
    }

    impl Fixture {
        fn new() -> Self {
            let mut builder = PoolBuilder::default();
            let module = builder.named(19, "example.module");
            let base = builder.named(19, "java.base");
            let friend = builder.named(19, "friend");
            let duplicate_base = builder.named(19, "java.base");
            let duplicate_friend = builder.named(19, "friend");
            let package = builder.named(20, "example/api");
            let duplicate_package = builder.named(20, "example/api");
            let other_package = builder.named(20, "other");
            let service = builder.named(7, "example/api/Service");
            let duplicate_service = builder.named(7, "example/api/Service");
            let implementation = builder.named(7, "example/api/Implementation");
            let duplicate_implementation = builder.named(7, "example/api/Implementation");
            let array = builder.named(7, "[Ljava/lang/String;");
            let version = builder.utf8("21-ea");
            Self {
                pool: builder.finish(),
                module,
                base,
                friend,
                duplicate_base,
                duplicate_friend,
                package,
                duplicate_package,
                other_package,
                service,
                duplicate_service,
                implementation,
                duplicate_implementation,
                array,
                version,
            }
        }

        fn payload(&self) -> Payload {
            Payload {
                module: self.module,
                requires: vec![[self.base, 0x8000, 0]],
                ..Payload::default()
            }
        }

        fn decode(&self, payload: &Payload, major: u16) -> Result<Module, ParseError> {
            let bytes = payload.bytes();
            let mut reader = Reader::new(&bytes, 100, "Module");
            let module = parse_module(&mut reader, &self.pool, major)?;
            reader.finish()?;
            Ok(module)
        }
    }

    #[derive(Default)]
    struct Payload {
        module: u16,
        flags: u16,
        version: u16,
        requires: Vec<[u16; 3]>,
        exports: Vec<(u16, u16, Vec<u16>)>,
        opens: Vec<(u16, u16, Vec<u16>)>,
        uses: Vec<u16>,
        provides: Vec<(u16, Vec<u16>)>,
    }

    impl Payload {
        fn bytes(&self) -> Vec<u8> {
            let mut words = vec![
                self.module,
                self.flags,
                self.version,
                self.requires.len() as u16,
            ];
            words.extend(self.requires.iter().flatten());
            for table in [&self.exports, &self.opens] {
                words.push(table.len() as u16);
                for (index, flags, targets) in table {
                    words.extend([*index, *flags, targets.len() as u16]);
                    words.extend(targets);
                }
            }
            words.push(self.uses.len() as u16);
            words.extend(&self.uses);
            words.push(self.provides.len() as u16);
            for (index, implementations) in &self.provides {
                words.extend([*index, implementations.len() as u16]);
                words.extend(implementations);
            }
            words.into_iter().flat_map(u16::to_be_bytes).collect()
        }
    }

    #[test]
    fn module_and_package_name_encodings_follow_jvms_not_source_identifiers() {
        for valid in [
            "",
            "example.module",
            "a/b",
            "1 weird-name",
            "a\\:b\\@c\\\\d",
        ] {
            assert!(
                valid_module_name(&valid.encode_utf16().collect::<Vec<_>>()),
                "{valid}"
            );
        }
        assert!(valid_module_name(&[0xd800, 0xffff]));
        for control in 0..=31 {
            assert!(!valid_module_name(&[65, control, 66]));
        }
        assert!(valid_module_name(&[32]));
        for invalid in ["a:b", "a@b", "a\\b", "a\\", "\\.", "\0", "\u{1f}"] {
            assert!(
                !valid_module_name(&invalid.encode_utf16().collect::<Vec<_>>()),
                "{invalid}"
            );
        }
        for valid in ["a", "a/b", "1 odd-name"] {
            assert!(valid_package_name(
                &valid.encode_utf16().collect::<Vec<_>>()
            ));
        }
        for invalid in ["", "/a", "a/", "a//b", "a.b", "a;b", "[I"] {
            assert!(!valid_package_name(
                &invalid.encode_utf16().collect::<Vec<_>>()
            ));
        }
    }

    #[test]
    fn decodes_all_module_tables_and_preserves_symbolic_indices_and_reserved_flags() {
        let fixture = Fixture::new();
        let mut payload = fixture.payload();
        payload.flags = 0x8001;
        payload.version = fixture.version;
        payload.requires[0][1] = 0x8001;
        payload.requires[0][2] = fixture.version;
        payload.exports = vec![(fixture.package, 0x9001, vec![fixture.friend])];
        payload.opens = vec![(fixture.package, 0x1001, vec![])];
        payload.uses = vec![fixture.service];
        payload.provides = vec![(fixture.service, vec![fixture.implementation])];
        let module = fixture.decode(&payload, 65).unwrap();
        assert_eq!(module.module_flags, 0x8001);
        assert_eq!(module.module_version_index, fixture.version);
        assert_eq!(module.requires[0].requires_version_index, fixture.version);
        assert_eq!(module.requires[0].requires_flags, 0x8001);
        assert_eq!(module.exports[0].exports_flags, 0x9001);
        assert_eq!(module.exports[0].exports_to_index, [fixture.friend]);
        assert_eq!(module.opens[0].opens_flags, 0x1001);
        assert!(module.opens[0].opens_to_index.is_empty());
        assert_eq!(module.uses, [fixture.service]);
        assert_eq!(
            module.provides[0].provides_with_index,
            [fixture.implementation]
        );
    }

    #[test]
    fn base_requires_rules_cover_java_nine_and_ten_boundary() {
        let fixture = Fixture::new();
        let mut payload = fixture.payload();
        for flags in [0, 0x8000, 0x0001] {
            payload.requires[0][1] = flags;
            assert!(fixture.decode(&payload, 65).is_ok());
        }
        for flags in [0x20, 0x40, 0x60] {
            payload.requires[0][1] = flags;
            assert!(fixture.decode(&payload, 53).is_ok());
            assert!(fixture.decode(&payload, 54).is_err());
        }
        payload.requires[0][1] = 0x1000;
        assert!(fixture.decode(&payload, 53).is_err());
        payload.requires.clear();
        assert!(fixture.decode(&payload, 65).is_err());
        payload.module = fixture.base;
        assert!(fixture.decode(&payload, 65).is_ok());
        payload.requires = vec![[fixture.friend, 0, 0]];
        assert!(fixture.decode(&payload, 65).is_err());
    }

    #[test]
    fn open_module_rejects_explicit_opens() {
        let fixture = Fixture::new();
        let mut payload = fixture.payload();
        payload.flags = 0x20;
        assert!(fixture.decode(&payload, 65).is_ok());
        payload.opens = vec![(fixture.package, 0, vec![])];
        assert!(fixture.decode(&payload, 65).is_err());
    }

    #[test]
    fn duplicate_names_are_rejected_even_through_distinct_constant_pool_entries() {
        let fixture = Fixture::new();
        let mut cases = Vec::new();
        let mut payload = fixture.payload();
        payload.requires.push([fixture.duplicate_base, 0, 0]);
        cases.push(payload);
        for opens in [false, true] {
            let mut payload = fixture.payload();
            let table = if opens {
                &mut payload.opens
            } else {
                &mut payload.exports
            };
            *table = vec![
                (fixture.package, 0, vec![]),
                (fixture.duplicate_package, 0, vec![]),
            ];
            cases.push(payload);
            let mut payload = fixture.payload();
            let table = if opens {
                &mut payload.opens
            } else {
                &mut payload.exports
            };
            *table = vec![(
                fixture.package,
                0,
                vec![fixture.friend, fixture.duplicate_friend],
            )];
            cases.push(payload);
        }
        let mut payload = fixture.payload();
        payload.uses = vec![fixture.service, fixture.duplicate_service];
        cases.push(payload);
        let mut payload = fixture.payload();
        payload.provides = vec![
            (fixture.service, vec![fixture.implementation]),
            (fixture.duplicate_service, vec![fixture.implementation]),
        ];
        cases.push(payload);
        let mut payload = fixture.payload();
        payload.provides = vec![(
            fixture.service,
            vec![fixture.implementation, fixture.duplicate_implementation],
        )];
        cases.push(payload);
        for payload in cases {
            assert!(fixture.decode(&payload, 65).is_err());
        }
    }

    #[test]
    fn checks_typed_indices_versions_and_non_array_services() {
        let fixture = Fixture::new();
        let mut cases = Vec::new();
        for index in [
            0,
            u16::MAX,
            fixture.version,
            fixture.package,
            fixture.service,
        ] {
            let mut payload = fixture.payload();
            payload.module = index;
            cases.push(payload);
            let mut payload = fixture.payload();
            payload.requires[0][0] = index;
            cases.push(payload);
        }
        let mut payload = fixture.payload();
        payload.version = fixture.base;
        cases.push(payload);
        let mut payload = fixture.payload();
        payload.requires[0][2] = fixture.base;
        cases.push(payload);
        for opens in [false, true] {
            for (index, target) in [
                (fixture.module, fixture.friend),
                (fixture.package, fixture.package),
            ] {
                let mut payload = fixture.payload();
                let table = if opens {
                    &mut payload.opens
                } else {
                    &mut payload.exports
                };
                *table = vec![(index, 0, vec![target])];
                cases.push(payload);
            }
        }
        for index in [0, u16::MAX, fixture.base, fixture.array] {
            let mut payload = fixture.payload();
            payload.uses = vec![index];
            cases.push(payload);
            let mut payload = fixture.payload();
            payload.provides = vec![(index, vec![fixture.implementation])];
            cases.push(payload);
            let mut payload = fixture.payload();
            payload.provides = vec![(fixture.service, vec![index])];
            cases.push(payload);
        }
        let mut payload = fixture.payload();
        payload.provides = vec![(fixture.service, vec![])];
        cases.push(payload);
        for payload in cases {
            assert!(fixture.decode(&payload, 65).is_err());
        }
    }

    #[test]
    fn empty_and_escaped_module_names_are_valid_decoded_references() {
        for name in ["", "a\\:b\\@c\\\\d", "a/b"] {
            let mut builder = PoolBuilder::default();
            let module = builder.named(19, name);
            let base = builder.named(19, "java.base");
            let pool = builder.finish();
            let bytes = Payload {
                module,
                requires: vec![[base, 0, 0]],
                ..Payload::default()
            }
            .bytes();
            assert!(parse_module(&mut Reader::new(&bytes, 0, "Module"), &pool, 65).is_ok());
        }
    }

    #[test]
    fn all_payload_truncations_fail_and_outer_reader_enforces_exact_length() {
        let fixture = Fixture::new();
        let mut payload = fixture.payload();
        payload.exports = vec![(fixture.package, 0, vec![fixture.friend])];
        payload.opens = vec![(fixture.package, 0, vec![fixture.friend])];
        payload.uses = vec![fixture.service];
        payload.provides = vec![(fixture.service, vec![fixture.implementation])];
        let mut bytes = payload.bytes();
        for length in 0..bytes.len() {
            let mut reader = Reader::new(&bytes[..length], 100, "Module");
            assert!(
                parse_module(&mut reader, &fixture.pool, 65).is_err(),
                "length={length}"
            );
        }
        bytes.push(0);
        let mut reader = Reader::new(&bytes, 100, "Module");
        assert!(parse_module(&mut reader, &fixture.pool, 65).is_ok());
        assert!(matches!(
            reader.finish().unwrap_err().kind,
            ErrorKind::TrailingBytes(1)
        ));
        for offset in [6, 14, 16, 18, 20] {
            let mut bytes = fixture.payload().bytes();
            bytes[offset..offset + 2].copy_from_slice(&u16::MAX.to_be_bytes());
            assert!(
                parse_module(&mut Reader::new(&bytes, 0, "Module"), &fixture.pool, 65).is_err()
            );
        }
    }

    #[test]
    fn module_packages_checks_semantic_duplicates_and_all_required_packages() {
        let fixture = Fixture::new();
        let mut payload = fixture.payload();
        let empty = fixture.decode(&payload, 65).unwrap();
        assert!(validate_module_packages(&empty, &fixture.pool, &[], 50).is_ok());
        assert!(
            validate_module_packages(
                &empty,
                &fixture.pool,
                &[fixture.package, fixture.duplicate_package],
                50
            )
            .is_err()
        );
        for table in [0, 1, 2] {
            match table {
                0 => payload.exports = vec![(fixture.package, 0, vec![])],
                1 => payload.opens = vec![(fixture.package, 0, vec![])],
                _ => payload.provides = vec![(fixture.service, vec![fixture.implementation])],
            }
            let module = fixture.decode(&payload, 65).unwrap();
            assert!(validate_module_packages(&module, &fixture.pool, &[], 50).is_err());
            assert!(
                validate_module_packages(&module, &fixture.pool, &[fixture.other_package], 50)
                    .is_err()
            );
            assert!(
                validate_module_packages(&module, &fixture.pool, &[fixture.duplicate_package], 50)
                    .is_ok()
            );
        }
    }
}

use crate::{ErrorKind, ParseError, descriptor, reader::Reader};

/// Modified UTF-8 represents UTF-16 code units, including unpaired surrogates.
/// Conversion to Rust `String` is deliberately explicit and potentially lossy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaString(Vec<u16>);

impl JavaString {
    pub fn from_units(units: Vec<u16>) -> Self {
        Self(units)
    }
    pub fn units(&self) -> &[u16] {
        &self.0
    }
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(&self.0)
    }
    pub fn equals_ascii(&self, text: &str) -> bool {
        text.is_ascii() && self.0.iter().copied().eq(text.bytes().map(u16::from))
    }
    /// An unambiguous, single-line form for diagnostic and inspection output.
    pub fn escaped(&self) -> String {
        let mut result = String::new();
        for value in char::decode_utf16(self.0.iter().copied()) {
            match value {
                Ok(ch) => result.extend(ch.escape_debug()),
                Err(error) => result.push_str(&format!("\\u{:04x}", error.unpaired_surrogate())),
            }
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Unusable,
    Utf8(JavaString),
    Integer(i32),
    Float(u32),
    Long(i64),
    Double(u64),
    Class {
        name_index: u16,
    },
    String {
        string_index: u16,
    },
    Fieldref {
        class_index: u16,
        name_and_type_index: u16,
    },
    Methodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    InterfaceMethodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    NameAndType {
        name_index: u16,
        descriptor_index: u16,
    },
    MethodHandle {
        reference_kind: u8,
        reference_index: u16,
    },
    MethodType {
        descriptor_index: u16,
    },
    Dynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    InvokeDynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    Module {
        name_index: u16,
    },
    Package {
        name_index: u16,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstantPool {
    entries: Vec<Constant>,
    offsets: Vec<usize>,
}

impl ConstantPool {
    /// Includes the unusable zero slot and wide-constant continuation slots.
    pub fn entries(&self) -> &[Constant] {
        &self.entries
    }
    pub fn get(&self, index: u16) -> Result<&Constant, ParseError> {
        self.entries
            .get(usize::from(index))
            .filter(|entry| !matches!(entry, Constant::Unusable))
            .ok_or_else(|| self.index_error(index, "a usable constant"))
    }
    pub fn utf8(&self, index: u16) -> Result<&JavaString, ParseError> {
        match self.get(index)? {
            Constant::Utf8(value) => Ok(value),
            _ => Err(self.index_error(index, "Utf8")),
        }
    }
    pub fn class_name(&self, index: u16) -> Result<&JavaString, ParseError> {
        match self.get(index)? {
            Constant::Class { name_index } => self.utf8(*name_index),
            _ => Err(self.index_error(index, "Class")),
        }
    }
    pub(crate) fn index_error(&self, index: u16, expected: &'static str) -> ParseError {
        ParseError::new(
            self.offsets.get(usize::from(index)).copied().unwrap_or(0),
            "constant pool",
            ErrorKind::InvalidIndex { index, expected },
        )
    }
    pub(crate) fn require(
        &self,
        index: u16,
        expected: &'static str,
        predicate: impl FnOnce(&Constant) -> bool,
    ) -> Result<(), ParseError> {
        if predicate(self.get(index)?) {
            Ok(())
        } else {
            Err(self.index_error(index, expected))
        }
    }
    pub(crate) fn name_and_type(
        &self,
        index: u16,
        method: bool,
    ) -> Result<(&JavaString, &JavaString), ParseError> {
        let Constant::NameAndType {
            name_index,
            descriptor_index,
        } = self.get(index)?
        else {
            return Err(self.index_error(index, "NameAndType"));
        };
        let name = self.utf8(*name_index)?;
        let descriptor = self.utf8(*descriptor_index)?;
        let valid = if method {
            descriptor::parse_method_descriptor(descriptor.units()).is_ok()
        } else {
            descriptor::parse_field_descriptor(descriptor.units()).is_ok()
        };
        if !valid {
            return Err(ParseError::new(
                self.offsets[usize::from(*descriptor_index)],
                "constant-pool descriptor",
                ErrorKind::InvalidDescriptor,
            ));
        }
        if !(descriptor::valid_unqualified_name(name.units(), method)
            || (method && name.equals_ascii("<init>")))
        {
            return Err(self.index_error(*name_index, "a valid member name"));
        }
        Ok((name, descriptor))
    }
}

pub(crate) fn decode_modified_utf8(bytes: &[u8], offset: usize) -> Result<JavaString, ParseError> {
    let mut units = Vec::new();
    let mut position = 0;
    while let Some(&first) = bytes.get(position) {
        let bad = || {
            ParseError::new(
                offset + position,
                "CONSTANT_Utf8",
                ErrorKind::InvalidModifiedUtf8,
            )
        };
        let (unit, width) = match first {
            1..=0x7f => (u16::from(first), 1),
            0xc0..=0xdf => {
                let second = *bytes.get(position + 1).ok_or_else(bad)?;
                if second & 0xc0 != 0x80 {
                    return Err(bad());
                }
                let value = (u16::from(first & 0x1f) << 6) | u16::from(second & 0x3f);
                if value < 0x80 && !(first == 0xc0 && second == 0x80) {
                    return Err(bad());
                }
                (value, 2)
            }
            0xe0..=0xef => {
                let second = *bytes.get(position + 1).ok_or_else(bad)?;
                let third = *bytes.get(position + 2).ok_or_else(bad)?;
                if second & 0xc0 != 0x80 || third & 0xc0 != 0x80 {
                    return Err(bad());
                }
                let value = (u16::from(first & 0xf) << 12)
                    | (u16::from(second & 0x3f) << 6)
                    | u16::from(third & 0x3f);
                if value < 0x800 {
                    return Err(bad());
                }
                (value, 3)
            }
            _ => return Err(bad()),
        };
        units.push(unit);
        position += width;
    }
    Ok(JavaString(units))
}

pub(crate) fn parse_pool(reader: &mut Reader<'_>, major: u16) -> Result<ConstantPool, ParseError> {
    let count = reader.u2()?;
    if count == 0 {
        return Err(reader.invalid("constant_pool_count must be nonzero"));
    }
    let mut entries = vec![Constant::Unusable];
    let mut offsets = vec![reader.offset() - 2];
    while entries.len() < usize::from(count) {
        let offset = reader.offset();
        let tag = reader.u1()?;
        let introduced = match tag {
            15 | 16 | 18 => 51,
            19 | 20 => 53,
            17 => 55,
            _ => 45,
        };
        if major < introduced {
            return Err(
                reader.invalid("constant-pool tag is not defined for this class-file version")
            );
        }
        let constant = match tag {
            1 => {
                let length = usize::from(reader.u2()?);
                let offset = reader.offset();
                Constant::Utf8(decode_modified_utf8(reader.take(length)?, offset)?)
            }
            3 => Constant::Integer(reader.u4()? as i32),
            4 => Constant::Float(reader.u4()?),
            5 | 6 => {
                let bits = (u64::from(reader.u4()?) << 32) | u64::from(reader.u4()?);
                if entries.len() + 1 >= usize::from(count) {
                    return Err(reader
                        .invalid("Long/Double requires a reserved following constant-pool slot"));
                }
                if tag == 5 {
                    Constant::Long(bits as i64)
                } else {
                    Constant::Double(bits)
                }
            }
            7 => Constant::Class {
                name_index: reader.u2()?,
            },
            8 => Constant::String {
                string_index: reader.u2()?,
            },
            9 => Constant::Fieldref {
                class_index: reader.u2()?,
                name_and_type_index: reader.u2()?,
            },
            10 => Constant::Methodref {
                class_index: reader.u2()?,
                name_and_type_index: reader.u2()?,
            },
            11 => Constant::InterfaceMethodref {
                class_index: reader.u2()?,
                name_and_type_index: reader.u2()?,
            },
            12 => Constant::NameAndType {
                name_index: reader.u2()?,
                descriptor_index: reader.u2()?,
            },
            15 => Constant::MethodHandle {
                reference_kind: reader.u1()?,
                reference_index: reader.u2()?,
            },
            16 => Constant::MethodType {
                descriptor_index: reader.u2()?,
            },
            17 => Constant::Dynamic {
                bootstrap_method_attr_index: reader.u2()?,
                name_and_type_index: reader.u2()?,
            },
            18 => Constant::InvokeDynamic {
                bootstrap_method_attr_index: reader.u2()?,
                name_and_type_index: reader.u2()?,
            },
            19 => Constant::Module {
                name_index: reader.u2()?,
            },
            20 => Constant::Package {
                name_index: reader.u2()?,
            },
            _ => {
                return Err(ParseError::new(
                    offset,
                    "constant pool",
                    ErrorKind::InvalidConstantPoolTag(tag),
                ));
            }
        };
        entries.push(constant);
        offsets.push(offset);
        if matches!(tag, 5 | 6) {
            entries.push(Constant::Unusable);
            offsets.push(offset);
        }
    }
    let pool = ConstantPool { entries, offsets };
    validate_pool(&pool, major)?;
    Ok(pool)
}

fn validate_pool(pool: &ConstantPool, major: u16) -> Result<(), ParseError> {
    for (index, entry) in pool.entries.iter().enumerate() {
        let invalid = |reason| {
            ParseError::new(
                pool.offsets[index],
                "constant pool",
                ErrorKind::InvalidStructure(reason),
            )
        };
        match entry {
            Constant::Class { name_index } => {
                let name = pool.utf8(*name_index)?;
                let valid = if name.units().first() == Some(&91) {
                    descriptor::parse_field_descriptor(name.units()).is_ok()
                } else {
                    descriptor::valid_class_name(name.units())
                };
                if !valid {
                    return Err(invalid("invalid class name or array descriptor"));
                }
            }
            Constant::String { string_index } => {
                pool.utf8(*string_index)?;
            }
            Constant::NameAndType {
                name_index,
                descriptor_index,
            } => {
                let name = pool.utf8(*name_index)?;
                let desc = pool.utf8(*descriptor_index)?;
                let method = desc.units().first() == Some(&40);
                pool.name_and_type(index as u16, method)?;
                if name.equals_ascii("<init>")
                    && descriptor::parse_method_descriptor(desc.units())?
                        .return_type
                        .is_some()
                {
                    return Err(invalid("constructor descriptor must return void"));
                }
            }
            Constant::Fieldref {
                class_index,
                name_and_type_index,
            } => {
                pool.class_name(*class_index)?;
                pool.name_and_type(*name_and_type_index, false)?;
            }
            Constant::Methodref {
                class_index,
                name_and_type_index,
            }
            | Constant::InterfaceMethodref {
                class_index,
                name_and_type_index,
            } => {
                pool.class_name(*class_index)?;
                let (name, _) = pool.name_and_type(*name_and_type_index, true)?;
                if matches!(entry, Constant::InterfaceMethodref { .. })
                    && name.equals_ascii("<init>")
                {
                    return Err(invalid(
                        "interface method reference cannot name a constructor",
                    ));
                }
            }
            Constant::MethodType { descriptor_index } => {
                if descriptor::parse_method_descriptor(pool.utf8(*descriptor_index)?.units())
                    .is_err()
                {
                    return Err(invalid("invalid MethodType descriptor"));
                }
            }
            Constant::Dynamic {
                name_and_type_index,
                ..
            } => {
                pool.name_and_type(*name_and_type_index, false)?;
            }
            Constant::InvokeDynamic {
                name_and_type_index,
                ..
            } => {
                let (name, _) = pool.name_and_type(*name_and_type_index, true)?;
                if name.equals_ascii("<init>") {
                    return Err(invalid("InvokeDynamic cannot name a constructor"));
                }
            }
            Constant::Module { name_index } => {
                if !crate::module::valid_module_name(pool.utf8(*name_index)?.units()) {
                    return Err(invalid("invalid module name"));
                }
            }
            Constant::Package { name_index } => {
                if !crate::module::valid_package_name(pool.utf8(*name_index)?.units()) {
                    return Err(invalid("invalid package name"));
                }
            }
            Constant::MethodHandle {
                reference_kind,
                reference_index,
            } => {
                let target = pool.get(*reference_index)?;
                let valid = match reference_kind {
                    1..=4 => matches!(target, Constant::Fieldref { .. }),
                    5 | 8 => matches!(target, Constant::Methodref { .. }),
                    6 | 7 => {
                        matches!(target, Constant::Methodref { .. })
                            || (major >= 52
                                && matches!(target, Constant::InterfaceMethodref { .. }))
                    }
                    9 => matches!(target, Constant::InterfaceMethodref { .. }),
                    _ => false,
                };
                if !valid {
                    return Err(invalid("invalid MethodHandle reference kind or target tag"));
                }
                if let Constant::Methodref {
                    name_and_type_index,
                    ..
                }
                | Constant::InterfaceMethodref {
                    name_and_type_index,
                    ..
                } = target
                {
                    let (name, _) = pool.name_and_type(*name_and_type_index, true)?;
                    if (*reference_kind == 8) != name.equals_ascii("<init>") {
                        return Err(invalid(
                            "MethodHandle constructor name does not match reference kind",
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modified_utf8_preserves_utf16() {
        let string = decode_modified_utf8(
            &[
                0x41, 0xc0, 0x80, 0xed, 0xa0, 0x80, 0xed, 0xb0, 0x80, 0xed, 0xa0, 0x81,
            ],
            0,
        )
        .unwrap();
        assert_eq!(string.units(), &[65, 0, 0xd800, 0xdc00, 0xd801]);
        assert_eq!(string.escaped(), "A\\0𐀀\\ud801");
        for bytes in [
            &[0][..],
            &[0xc0, 0x81],
            &[0xc1, 0xbf],
            &[0xe0, 0x80, 0x80],
            &[0xf0, 0x90, 0x80, 0x80],
            &[0x80],
            &[0xe1, 0x80],
            &[0xc2, 0x41],
        ] {
            assert!(decode_modified_utf8(bytes, 10).is_err(), "{bytes:?}");
        }
    }
    #[test]
    fn wide_constants_reserve_two_slots() {
        let bytes = [0, 3, 5, 0, 0, 0, 0, 0, 0, 0, 42];
        let pool = parse_pool(&mut Reader::new(&bytes, 0, "test"), 65).unwrap();
        assert_eq!(pool.get(1).unwrap(), &Constant::Long(42));
        assert!(pool.get(0).is_err());
        assert!(pool.get(2).is_err());
        assert!(pool.get(3).is_err());
        let mut invalid = bytes;
        invalid[1] = 2;
        assert!(parse_pool(&mut Reader::new(&invalid, 0, "test"), 65).is_err());
    }
}

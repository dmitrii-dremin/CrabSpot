//! Descriptor grammar from JVMS 4.3; generic signatures use a different grammar.
use crate::{ErrorKind, JavaString, ParseError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldType {
    Byte,
    Char,
    Double,
    Float,
    Int,
    Long,
    Short,
    Boolean,
    Object(JavaString),
    Array {
        dimensions: u8,
        component: Box<FieldType>,
    },
}

impl FieldType {
    pub fn slots(&self) -> u16 {
        if matches!(self, Self::Long | Self::Double) {
            2
        } else {
            1
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodDescriptor {
    pub parameters: Vec<FieldType>,
    pub return_type: Option<FieldType>,
    pub parameter_slots: u16,
}

fn error(position: usize) -> ParseError {
    ParseError::new(
        position,
        "descriptor (UTF-16 unit offset)",
        ErrorKind::InvalidDescriptor,
    )
}

pub(crate) fn valid_unqualified_name(name: &[u16], method: bool) -> bool {
    !name.is_empty()
        && !name
            .iter()
            .any(|unit| matches!(*unit, 46 | 59 | 91 | 47) || (method && matches!(*unit, 60 | 62)))
}

pub(crate) fn valid_class_name(name: &[u16]) -> bool {
    !name.is_empty()
        && name
            .split(|unit| *unit == 47)
            .all(|part| valid_unqualified_name(part, false))
}

fn field_at(units: &[u16], position: &mut usize) -> Result<FieldType, ParseError> {
    let start = *position;
    let tag = *units.get(*position).ok_or_else(|| error(*position))?;
    *position += 1;
    Ok(match tag {
        66 => FieldType::Byte,
        67 => FieldType::Char,
        68 => FieldType::Double,
        70 => FieldType::Float,
        73 => FieldType::Int,
        74 => FieldType::Long,
        83 => FieldType::Short,
        90 => FieldType::Boolean,
        76 => {
            let name_start = *position;
            while units.get(*position).is_some_and(|unit| *unit != 59) {
                *position += 1;
            }
            let name = &units[name_start..*position];
            if !valid_class_name(name) || units.get(*position) != Some(&59) {
                return Err(error(start));
            }
            *position += 1;
            FieldType::Object(JavaString::from_units(name.to_vec()))
        }
        91 => {
            let mut dimensions: u16 = 1;
            while units.get(*position) == Some(&91) {
                dimensions += 1;
                *position += 1;
                if dimensions > 255 {
                    return Err(error(start));
                }
            }
            let component = field_at(units, position)?;
            FieldType::Array {
                dimensions: dimensions as u8,
                component: Box::new(component),
            }
        }
        _ => return Err(error(start)),
    })
}

pub fn parse_field_descriptor(units: &[u16]) -> Result<FieldType, ParseError> {
    let mut position = 0;
    let field = field_at(units, &mut position)?;
    if position != units.len() {
        return Err(error(position));
    }
    Ok(field)
}

pub fn parse_method_descriptor(units: &[u16]) -> Result<MethodDescriptor, ParseError> {
    if units.first() != Some(&40) {
        return Err(error(0));
    }
    let mut position = 1;
    let mut parameters = Vec::new();
    let mut parameter_slots = 0;
    while units.get(position) != Some(&41) {
        let field = field_at(units, &mut position)?;
        parameter_slots += field.slots();
        if parameter_slots > 255 {
            return Err(error(position));
        }
        parameters.push(field);
    }
    position += 1;
    let return_type = if units.get(position) == Some(&86) {
        position += 1;
        None
    } else {
        Some(field_at(units, &mut position)?)
    };
    if position != units.len() {
        return Err(error(position));
    }
    Ok(MethodDescriptor {
        parameters,
        return_type,
        parameter_slots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    #[test]
    fn descriptors_accept_exact_grammar_and_slot_widths() {
        let method = parse_method_descriptor(&units("(IJ[DLjava/lang/String;)V")).unwrap();
        assert_eq!(method.parameter_slots, 5);
        assert_eq!(method.return_type, None);
        for bad in ["V", "L;", "L/foo;", "La//b;", "[V", "II", "La.b;", ""] {
            assert!(parse_field_descriptor(&units(bad)).is_err(), "{bad}");
        }
        for bad in ["", "()", "(V)V", "(I", "()VI", "(I)Q"] {
            assert!(parse_method_descriptor(&units(bad)).is_err(), "{bad}");
        }
    }
    #[test]
    fn descriptor_limits() {
        assert!(parse_field_descriptor(&units(&("[".repeat(255) + "I"))).is_ok());
        assert!(parse_field_descriptor(&units(&("[".repeat(256) + "I"))).is_err());
        assert!(parse_method_descriptor(&units(&format!("({})V", "I".repeat(255)))).is_ok());
        assert!(parse_method_descriptor(&units(&format!("({})V", "J".repeat(128)))).is_err());
    }
}

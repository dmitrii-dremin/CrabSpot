//! Allocation-free summaries; only the selected initial locals may be emitted.
use crate::{Checked, CodeValidationErrorKind as E};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Name {
    #[default]
    Ordinary,
    Init,
    Clinit,
    Code,
    StackMap,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Summary {
    pub width: u8,
    pub rank: u8,
    pub slots: u16,
    pub void: bool,
    pub name: Name,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum Role {
    Attribute,
    MethodName,
    FieldName,
    Class,
    Field,
    Method,
}

fn ascii(units: &[u16], text: &str) -> bool {
    units.iter().copied().eq(text.bytes().map(u16::from))
}
fn unqualified(units: &[u16], method: bool) -> bool {
    !units.is_empty()
        && !units
            .iter()
            .any(|unit| matches!(*unit, 46 | 59 | 91 | 47) || (method && matches!(*unit, 60 | 62)))
}
fn class(units: &[u16]) -> bool {
    !units.is_empty()
        && units
            .split(|unit| *unit == 47)
            .all(|part| unqualified(part, false))
}
fn field(units: &[u16], position: &mut usize) -> Option<Summary> {
    let mut rank = 0_u16;
    while units.get(*position) == Some(&91) {
        rank = rank.checked_add(1)?;
        if rank > 255 {
            return None;
        }
        *position = position.checked_add(1)?;
    }
    let tag = *units.get(*position)?;
    *position = position.checked_add(1)?;
    let width = match tag {
        66 | 67 | 70 | 73 | 83 | 90 => 1,
        68 | 74 => 2,
        76 => {
            let start = *position;
            while units.get(*position).is_some_and(|unit| *unit != 59) {
                *position = position.checked_add(1)?;
            }
            if !class(units.get(start..*position)?) || units.get(*position) != Some(&59) {
                return None;
            }
            *position = position.checked_add(1)?;
            1
        }
        _ => return None,
    };
    Some(Summary {
        width: if rank > 0 { 1 } else { width },
        rank: u8::try_from(rank).ok()?,
        ..Summary::default()
    })
}
pub(crate) fn reserve<T>(
    values: &mut Vec<T>,
    amount: usize,
    resource: &'static str,
) -> Checked<()> {
    values
        .try_reserve(amount)
        .map_err(|_| E::AllocationFailure { resource })
}
pub(crate) fn inspect(
    units: &[u16],
    index: u16,
    role: Role,
    mut locals: Option<&mut Vec<u8>>,
) -> Checked<Summary> {
    let invalid = || E::InvalidDescriptor { index };
    match role {
        Role::Attribute => Ok(Summary {
            name: if ascii(units, "Code") {
                Name::Code
            } else if ascii(units, "StackMapTable") {
                Name::StackMap
            } else {
                Name::Ordinary
            },
            ..Summary::default()
        }),
        Role::MethodName | Role::FieldName => {
            let name = if role == Role::MethodName && ascii(units, "<init>") {
                Name::Init
            } else if role == Role::MethodName && ascii(units, "<clinit>") {
                Name::Clinit
            } else if unqualified(units, role == Role::MethodName) {
                Name::Ordinary
            } else {
                return Err(E::InvalidName { index });
            };
            Ok(Summary {
                name,
                ..Summary::default()
            })
        }
        Role::Class => {
            if units.first() == Some(&91) {
                let mut p = 0;
                let summary = field(units, &mut p).ok_or(E::InvalidName { index })?;
                if p != units.len() {
                    return Err(E::InvalidName { index });
                }
                Ok(summary)
            } else if class(units) {
                Ok(Summary::default())
            } else {
                Err(E::InvalidName { index })
            }
        }
        Role::Field => {
            let mut p = 0;
            let summary = field(units, &mut p).ok_or_else(invalid)?;
            if p != units.len() {
                return Err(invalid());
            }
            Ok(summary)
        }
        Role::Method => {
            if units.first() != Some(&40) {
                return Err(invalid());
            }
            let mut p = 1;
            let mut slots = 0_u16;
            while units.get(p) != Some(&41) {
                let next = field(units, &mut p).ok_or_else(invalid)?;
                slots = slots
                    .checked_add(u16::from(next.width))
                    .ok_or_else(invalid)?;
                if slots > 255 {
                    return Err(invalid());
                }
                if let Some(widths) = locals.as_deref_mut() {
                    reserve(widths, 1, "initial local widths")?;
                    widths.push(next.width);
                }
            }
            p = p.checked_add(1).ok_or_else(invalid)?;
            let void = units.get(p) == Some(&86);
            if void {
                p = p.checked_add(1).ok_or_else(invalid)?;
            } else {
                field(units, &mut p).ok_or_else(invalid)?;
            }
            if p != units.len() {
                return Err(invalid());
            }
            Ok(Summary {
                slots,
                void,
                ..Summary::default()
            })
        }
    }
}

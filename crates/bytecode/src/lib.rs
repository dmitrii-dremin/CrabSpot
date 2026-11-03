//! Bounded JVM instruction decoding, independent of verification and execution.
//!
//! The decoder checks instruction formats and static branch destinations. It
//! does not resolve constants, check frame types, or establish executable code.
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operands {
    None,
    Byte(i8),
    Short(i16),
    Local(u16),
    Constant(u16),
    Iinc {
        index: u16,
        increment: i16,
    },
    Branch(i32),
    TableSwitch {
        default: i32,
        low: i32,
        offsets: Vec<i32>,
    },
    LookupSwitch {
        default: i32,
        pairs: Vec<(i32, i32)>,
    },
    InvokeInterface {
        index: u16,
        count: u8,
    },
    InvokeDynamic {
        index: u16,
    },
    NewArray(u8),
    MultiANewArray {
        index: u16,
        dimensions: u8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub offset: u32,
    pub length: u32,
    /// For wide encodings, this is the embedded opcode.
    pub opcode: u8,
    /// The offset and length include the wide prefix, when present.
    pub wide: bool,
    pub operands: Operands,
}

/// Only `decode` constructs successful instruction sequences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedCode {
    instructions: Vec<Instruction>,
    code_len: u32,
}

impl DecodedCode {
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub fn instruction_at(&self, offset: u32) -> Option<&Instruction> {
        self.instructions
            .binary_search_by_key(&offset, |instruction| instruction.offset)
            .ok()
            .and_then(|index| self.instructions.get(index))
    }

    pub fn code_len(&self) -> u32 {
        self.code_len
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodeError {
    pub offset: u32,
    pub kind: DecodeErrorKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeErrorKind {
    EmptyCode,
    CodeTooLong { length: usize },
    UnsupportedVersion { major: u16 },
    Truncated { needed: usize, remaining: usize },
    InvalidOpcode(u8),
    InvalidWideOpcode(u8),
    InvalidOperand(&'static str),
    InvalidSwitch(&'static str),
    InvalidTarget { displacement: i32 },
    OpcodeVersion { opcode: u8, major: u16 },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bytecode at {}: ", self.offset)?;
        match &self.kind {
            DecodeErrorKind::EmptyCode => f.write_str("code must not be empty"),
            DecodeErrorKind::CodeTooLong { length } => {
                write!(f, "code length {length} exceeds 65535")
            }
            DecodeErrorKind::UnsupportedVersion { major } => {
                write!(f, "unsupported class-file major version {major}")
            }
            DecodeErrorKind::Truncated { needed, remaining } => {
                write!(f, "need {needed} bytes, only {remaining} remain")
            }
            DecodeErrorKind::InvalidOpcode(opcode) => {
                write!(f, "reserved or undefined opcode 0x{opcode:02x}")
            }
            DecodeErrorKind::InvalidWideOpcode(opcode) => {
                write!(f, "wide cannot modify opcode 0x{opcode:02x}")
            }
            DecodeErrorKind::InvalidOperand(reason) | DecodeErrorKind::InvalidSwitch(reason) => {
                f.write_str(reason)
            }
            DecodeErrorKind::InvalidTarget { displacement } => write!(
                f,
                "branch displacement {displacement} does not target an instruction"
            ),
            DecodeErrorKind::OpcodeVersion { opcode, major } => write!(
                f,
                "opcode 0x{opcode:02x} is not allowed in class-file major version {major}"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    fn take(&mut self, count: usize) -> Result<&[u8], DecodeError> {
        if count > self.remaining() {
            return Err(error(
                self.position,
                DecodeErrorKind::Truncated {
                    needed: count,
                    remaining: self.remaining(),
                },
            ));
        }
        let end = self.position + count;
        let bytes = self.bytes.get(self.position..end).ok_or_else(|| {
            error(
                self.position,
                DecodeErrorKind::Truncated {
                    needed: count,
                    remaining: self.remaining(),
                },
            )
        })?;
        self.position = end;
        Ok(bytes)
    }

    fn u1(&mut self) -> Result<u8, DecodeError> {
        let bytes = self.take(1)?;
        Ok(bytes[0])
    }

    fn u2(&mut self) -> Result<u16, DecodeError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn i4(&mut self) -> Result<i32, DecodeError> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

fn error(offset: usize, kind: DecodeErrorKind) -> DecodeError {
    // All reader positions are bounded by the validated 65535-byte input.
    DecodeError {
        offset: offset as u32,
        kind,
    }
}

fn constant(index: u16, offset: usize) -> Result<u16, DecodeError> {
    if index == 0 {
        return Err(error(
            offset,
            DecodeErrorKind::InvalidOperand("constant-pool index must be nonzero"),
        ));
    }
    Ok(index)
}

fn check_opcode_version(opcode: u8, major: u16, offset: usize) -> Result<(), DecodeError> {
    if (major >= 51 && matches!(opcode, 0xa8 | 0xa9 | 0xc9)) || (major < 51 && opcode == 0xba) {
        return Err(error(
            offset,
            DecodeErrorKind::OpcodeVersion { opcode, major },
        ));
    }
    Ok(())
}

fn switch(reader: &mut Reader<'_>, offset: usize, lookup: bool) -> Result<Operands, DecodeError> {
    let padding = (4 - reader.position % 4) % 4;
    reader.take(padding)?;
    let default = reader.i4()?;
    if lookup {
        let count = reader.i4()?;
        if count < 0 || i64::from(count) > (reader.remaining() / 8) as i64 {
            return Err(error(
                offset,
                DecodeErrorKind::InvalidSwitch("invalid lookupswitch pair count"),
            ));
        }
        let mut pairs = Vec::new();
        let mut previous = None;
        for _ in 0..count {
            let key = reader.i4()?;
            let displacement = reader.i4()?;
            if previous.is_some_and(|previous| key <= previous) {
                return Err(error(
                    offset,
                    DecodeErrorKind::InvalidSwitch("lookupswitch keys must be strictly increasing"),
                ));
            }
            previous = Some(key);
            pairs.push((key, displacement));
        }
        Ok(Operands::LookupSwitch { default, pairs })
    } else {
        let low = reader.i4()?;
        let high = reader.i4()?;
        let count = i64::from(high) - i64::from(low) + 1;
        if high < low || count > (reader.remaining() / 4) as i64 {
            return Err(error(
                offset,
                DecodeErrorKind::InvalidSwitch("invalid tableswitch low/high range or entry count"),
            ));
        }
        let mut offsets = Vec::new();
        for _ in 0..count {
            offsets.push(reader.i4()?);
        }
        Ok(Operands::TableSwitch {
            default,
            low,
            offsets,
        })
    }
}

fn read_instruction(reader: &mut Reader<'_>, major: u16) -> Result<Instruction, DecodeError> {
    let offset = reader.position;
    let first = reader.u1()?;
    let wide = first == 0xc4;
    let opcode = if wide { reader.u1()? } else { first };
    if wide && !matches!(opcode, 0x15..=0x19 | 0x36..=0x3a | 0x84 | 0xa9) {
        return Err(error(offset, DecodeErrorKind::InvalidWideOpcode(opcode)));
    }
    check_opcode_version(opcode, major, offset)?;
    let operands = if wide {
        match opcode {
            0x15..=0x19 | 0x36..=0x3a | 0xa9 => Operands::Local(reader.u2()?),
            0x84 => Operands::Iinc {
                index: reader.u2()?,
                increment: reader.u2()? as i16,
            },
            _ => return Err(error(offset, DecodeErrorKind::InvalidWideOpcode(opcode))),
        }
    } else {
        match opcode {
            0x10 => Operands::Byte(reader.u1()? as i8),
            0x11 => Operands::Short(reader.u2()? as i16),
            0x12 => Operands::Constant(constant(u16::from(reader.u1()?), offset)?),
            0x13 | 0x14 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 => {
                Operands::Constant(constant(reader.u2()?, offset)?)
            }
            0x15..=0x19 | 0x36..=0x3a | 0xa9 => Operands::Local(u16::from(reader.u1()?)),
            0x84 => Operands::Iinc {
                index: u16::from(reader.u1()?),
                increment: i16::from(reader.u1()? as i8),
            },
            0x99..=0xa8 | 0xc6 | 0xc7 => Operands::Branch(i32::from(reader.u2()? as i16)),
            0xaa | 0xab => switch(reader, offset, opcode == 0xab)?,
            0xb9 => {
                let index = constant(reader.u2()?, offset)?;
                let count = reader.u1()?;
                let reserved = reader.u1()?;
                if count == 0 || reserved != 0 {
                    return Err(error(
                        offset,
                        DecodeErrorKind::InvalidOperand(
                            "invokeinterface requires nonzero count and zero reserved byte",
                        ),
                    ));
                }
                Operands::InvokeInterface { index, count }
            }
            0xba => {
                let index = constant(reader.u2()?, offset)?;
                if reader.u2()? != 0 {
                    return Err(error(
                        offset,
                        DecodeErrorKind::InvalidOperand(
                            "invokedynamic reserved bytes must be zero",
                        ),
                    ));
                }
                Operands::InvokeDynamic { index }
            }
            0xbc => {
                let atype = reader.u1()?;
                if !(4..=11).contains(&atype) {
                    return Err(error(
                        offset,
                        DecodeErrorKind::InvalidOperand("newarray type must be in 4..=11"),
                    ));
                }
                Operands::NewArray(atype)
            }
            0xc5 => {
                let index = constant(reader.u2()?, offset)?;
                let dimensions = reader.u1()?;
                if dimensions == 0 {
                    return Err(error(
                        offset,
                        DecodeErrorKind::InvalidOperand(
                            "multianewarray dimensions must be nonzero",
                        ),
                    ));
                }
                Operands::MultiANewArray { index, dimensions }
            }
            0xc8 | 0xc9 => Operands::Branch(reader.i4()?),
            // All remaining defined opcodes have no encoded operands.
            0x00..=0x0f
            | 0x1a..=0x35
            | 0x3b..=0x83
            | 0x85..=0x98
            | 0xac..=0xb1
            | 0xbe
            | 0xbf
            | 0xc2
            | 0xc3 => Operands::None,
            _ => return Err(error(offset, DecodeErrorKind::InvalidOpcode(opcode))),
        }
    };
    Ok(Instruction {
        offset: offset as u32,
        length: (reader.position - offset) as u32,
        opcode,
        wide,
        operands,
    })
}

fn validate_target(
    code: &DecodedCode,
    instruction: &Instruction,
    displacement: i32,
) -> Result<(), DecodeError> {
    let target = i64::from(instruction.offset) + i64::from(displacement);
    if target < 0
        || target >= i64::from(code.code_len)
        || code.instruction_at(target as u32).is_none()
    {
        return Err(DecodeError {
            offset: instruction.offset,
            kind: DecodeErrorKind::InvalidTarget { displacement },
        });
    }
    Ok(())
}

/// Decode all Java SE 21 instruction formats and validate static destinations.
/// `code` must have the Code attribute's length in 1..=65535 bytes.
pub fn decode(code: &[u8], major: u16) -> Result<DecodedCode, DecodeError> {
    if code.is_empty() {
        return Err(error(0, DecodeErrorKind::EmptyCode));
    }
    if code.len() > 65535 {
        return Err(error(
            0,
            DecodeErrorKind::CodeTooLong { length: code.len() },
        ));
    }
    if !(45..=65).contains(&major) {
        return Err(error(0, DecodeErrorKind::UnsupportedVersion { major }));
    }
    let mut reader = Reader {
        bytes: code,
        position: 0,
    };
    let mut instructions = Vec::new();
    while reader.remaining() != 0 {
        instructions.push(read_instruction(&mut reader, major)?);
    }
    let decoded = DecodedCode {
        instructions,
        code_len: code.len() as u32,
    };
    for instruction in decoded.instructions() {
        match &instruction.operands {
            Operands::Branch(displacement) => {
                validate_target(&decoded, instruction, *displacement)?
            }
            Operands::TableSwitch {
                default, offsets, ..
            } => {
                validate_target(&decoded, instruction, *default)?;
                for displacement in offsets {
                    validate_target(&decoded, instruction, *displacement)?;
                }
            }
            Operands::LookupSwitch { default, pairs } => {
                validate_target(&decoded, instruction, *default)?;
                for (_, displacement) in pairs {
                    validate_target(&decoded, instruction, *displacement)?;
                }
            }
            _ => {}
        }
    }
    Ok(decoded)
}

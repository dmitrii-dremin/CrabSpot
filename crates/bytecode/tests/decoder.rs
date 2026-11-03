use crabspot_bytecode::{DecodeErrorKind, Operands, decode};

fn table(prefix: usize, default: i32, low: i32, high: i32, offsets: &[i32]) -> Vec<u8> {
    let mut bytes = vec![0; prefix];
    bytes.push(0xaa);
    while bytes.len() % 4 != 0 {
        bytes.push(0xca);
    }
    for value in [default, low, high].iter().chain(offsets) {
        bytes.extend(value.to_be_bytes());
    }
    bytes
}

fn lookup(prefix: usize, default: i32, count: i32, pairs: &[(i32, i32)]) -> Vec<u8> {
    let mut bytes = vec![0; prefix];
    bytes.push(0xab);
    while bytes.len() % 4 != 0 {
        bytes.push(0xff);
    }
    bytes.extend(default.to_be_bytes());
    bytes.extend(count.to_be_bytes());
    for (key, offset) in pairs {
        bytes.extend(key.to_be_bytes());
        bytes.extend(offset.to_be_bytes());
    }
    bytes
}

fn encoding(opcode: u8) -> (Vec<u8>, Operands) {
    let (suffix, operands) = match opcode {
        0x10 => (vec![0x80], Operands::Byte(-128)),
        0x11 => (vec![0x80, 0], Operands::Short(-32768)),
        0x12 => (vec![255], Operands::Constant(255)),
        0x13 | 0x14 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 => {
            (vec![255, 255], Operands::Constant(65535))
        }
        0x15..=0x19 | 0x36..=0x3a | 0xa9 => (vec![255], Operands::Local(255)),
        0x84 => (
            vec![255, 0x80],
            Operands::Iinc {
                index: 255,
                increment: -128,
            },
        ),
        0x99..=0xa8 | 0xc6 | 0xc7 => (vec![0, 0], Operands::Branch(0)),
        0xaa => {
            return (
                table(0, 0, i32::MIN, i32::MIN, &[0]),
                Operands::TableSwitch {
                    default: 0,
                    low: i32::MIN,
                    offsets: vec![0],
                },
            );
        }
        0xab => {
            return (
                lookup(0, 0, 0, &[]),
                Operands::LookupSwitch {
                    default: 0,
                    pairs: vec![],
                },
            );
        }
        0xb9 => (
            vec![255, 255, 255, 0],
            Operands::InvokeInterface {
                index: 65535,
                count: 255,
            },
        ),
        0xba => (
            vec![255, 255, 0, 0],
            Operands::InvokeDynamic { index: 65535 },
        ),
        0xbc => (vec![4], Operands::NewArray(4)),
        0xc4 => (
            vec![0x84, 255, 255, 0x80, 0],
            Operands::Iinc {
                index: 65535,
                increment: -32768,
            },
        ),
        0xc5 => (
            vec![255, 255, 255],
            Operands::MultiANewArray {
                index: 65535,
                dimensions: 255,
            },
        ),
        0xc8 | 0xc9 => (vec![0, 0, 0, 0], Operands::Branch(0)),
        _ => (vec![], Operands::None),
    };
    let mut bytes = vec![opcode];
    bytes.extend(suffix);
    (bytes, operands)
}

#[test]
fn every_defined_opcode_decodes_with_its_operand_format_and_exact_length() {
    for opcode in 0..=0xc9 {
        let (bytes, expected) = encoding(opcode);
        // No single class-file version permits both subroutines and invokedynamic.
        let major = if opcode == 0xba { 51 } else { 50 };
        let decoded =
            decode(&bytes, major).unwrap_or_else(|error| panic!("opcode {opcode:02x}: {error}"));
        assert_eq!(decoded.code_len(), bytes.len() as u32);
        assert_eq!(decoded.instructions().len(), 1);
        let instruction = decoded.instruction_at(0).unwrap();
        assert_eq!(instruction.offset, 0);
        assert_eq!(instruction.length, bytes.len() as u32);
        assert_eq!(
            instruction.opcode,
            if opcode == 0xc4 { 0x84 } else { opcode }
        );
        assert_eq!(instruction.wide, opcode == 0xc4);
        assert_eq!(instruction.operands, expected, "opcode {opcode:02x}");
        for offset in 1..=bytes.len() {
            assert!(decoded.instruction_at(offset as u32).is_none());
        }
        assert!(decoded.instruction_at(u32::MAX).is_none());
    }
}

#[test]
fn every_multi_byte_encoding_rejects_each_truncated_prefix() {
    for opcode in 0..=0xc9 {
        let (bytes, _) = encoding(opcode);
        let major = if opcode == 0xba { 51 } else { 50 };
        for end in 0..bytes.len() {
            assert!(
                decode(&bytes[..end], major).is_err(),
                "opcode={opcode:02x}, end={end}"
            );
        }
    }
}

#[test]
fn signed_immediates_and_increment_operands_cover_boundaries() {
    for value in [i8::MIN, -1, 0, i8::MAX] {
        assert_eq!(
            decode(&[0x10, value as u8], 65).unwrap().instructions()[0].operands,
            Operands::Byte(value)
        );
        assert_eq!(
            decode(&[0x84, 0, value as u8], 65).unwrap().instructions()[0].operands,
            Operands::Iinc {
                index: 0,
                increment: i16::from(value)
            }
        );
    }
    for value in [i16::MIN, -1, 0, i16::MAX] {
        let mut bytes = vec![0x11];
        bytes.extend(value.to_be_bytes());
        assert_eq!(
            decode(&bytes, 65).unwrap().instructions()[0].operands,
            Operands::Short(value)
        );
        let mut bytes = vec![0xc4, 0x84, 255, 255];
        bytes.extend(value.to_be_bytes());
        assert_eq!(
            decode(&bytes, 65).unwrap().instructions()[0].operands,
            Operands::Iinc {
                index: 65535,
                increment: value
            }
        );
    }
}

#[test]
fn wide_accepts_only_legal_followers_and_preserves_prefix_boundaries() {
    for opcode in [
        0x15, 0x16, 0x17, 0x18, 0x19, 0x36, 0x37, 0x38, 0x39, 0x3a, 0xa9,
    ] {
        for index in [0_u16, 255, 256, 65535] {
            let mut bytes = vec![0xc4, opcode];
            bytes.extend(index.to_be_bytes());
            let decoded = decode(&bytes, 50).unwrap();
            let instruction = &decoded.instructions()[0];
            assert_eq!(instruction.opcode, opcode);
            assert!(instruction.wide);
            assert_eq!(instruction.length, 4);
            assert_eq!(instruction.operands, Operands::Local(index));
            assert!(decoded.instruction_at(1).is_none());
            for end in 1..bytes.len() {
                assert!(decode(&bytes[..end], 50).is_err());
            }
        }
    }
    for opcode in 0..=255 {
        if matches!(opcode, 0x15..=0x19 | 0x36..=0x3a | 0x84 | 0xa9) {
            continue;
        }
        let error = decode(&[0xc4, opcode, 0, 0, 0, 0], 50).unwrap_err();
        assert_eq!(error.offset, 0);
        assert!(matches!(error.kind, DecodeErrorKind::InvalidWideOpcode(value) if value == opcode));
    }
    let valid = decode(&[0xa7, 0, 3, 0xc4, 0x15, 0, 0], 65).unwrap();
    assert_eq!(valid.instruction_at(3).unwrap().opcode, 0x15);
    assert!(matches!(
        decode(&[0xa7, 0, 4, 0xc4, 0x15, 0, 0], 65)
            .unwrap_err()
            .kind,
        DecodeErrorKind::InvalidTarget { .. }
    ));
}

#[test]
fn switches_cover_all_alignments_nonzero_padding_and_signed_key_bounds() {
    for prefix in 0..4 {
        let bytes = table(prefix, 0, i32::MAX, i32::MAX, &[0]);
        let decoded = decode(&bytes, 65).unwrap();
        let instruction = decoded.instruction_at(prefix as u32).unwrap();
        assert_eq!(instruction.length, (bytes.len() - prefix) as u32);
        assert_eq!(
            instruction.operands,
            Operands::TableSwitch {
                default: 0,
                low: i32::MAX,
                offsets: vec![0]
            }
        );
        let pairs = [(i32::MIN, 0), (-1, 0), (0, 0), (i32::MAX, 0)];
        let bytes = lookup(prefix, 0, 4, &pairs);
        let decoded = decode(&bytes, 65).unwrap();
        assert_eq!(
            decoded.instruction_at(prefix as u32).unwrap().operands,
            Operands::LookupSwitch {
                default: 0,
                pairs: pairs.to_vec()
            }
        );
        assert!(decode(&lookup(prefix, 0, 0, &[]), 65).is_ok());
    }
}

#[test]
fn malformed_switch_ranges_counts_keys_and_targets_are_rejected() {
    for bytes in [
        table(0, 0, 1, 0, &[]),
        table(0, 0, i32::MIN, i32::MAX, &[]),
        table(0, 0, 0, 1, &[0]),
        lookup(0, 0, -1, &[]),
        lookup(0, 0, i32::MAX, &[]),
        lookup(0, 0, 1, &[]),
        lookup(0, 0, 2, &[(0, 0), (0, 0)]),
        lookup(0, 0, 2, &[(1, 0), (-1, 0)]),
    ] {
        assert!(matches!(
            decode(&bytes, 65).unwrap_err().kind,
            DecodeErrorKind::InvalidSwitch(_)
        ));
    }
    for displacement in [i32::MIN, -1, 1, 4, 20, i32::MAX] {
        for bytes in [
            table(0, displacement, 0, 0, &[0]),
            table(0, 0, 0, 0, &[displacement]),
            lookup(0, displacement, 0, &[]),
            lookup(0, 0, 1, &[(0, displacement)]),
        ] {
            assert!(matches!(
                decode(&bytes, 65).unwrap_err().kind,
                DecodeErrorKind::InvalidTarget { .. }
            ));
        }
    }
    let mut bytes = table(0, 20, -1, 0, &[20, 0]);
    // Two-entry table is 24 bytes, so its forward target is byte 24.
    bytes[4..8].copy_from_slice(&24_i32.to_be_bytes());
    bytes[16..20].copy_from_slice(&24_i32.to_be_bytes());
    bytes.push(0xb1);
    assert!(decode(&bytes, 65).is_ok());
}

#[test]
fn all_static_branch_formats_check_forward_backward_and_invalid_destinations() {
    for opcode in (0x99..=0xa8).chain([0xc6, 0xc7, 0xc8, 0xc9]) {
        let wide = opcode >= 0xc8;
        let length: i32 = if wide { 5 } else { 3 };
        let mut bytes = vec![opcode];
        if wide {
            bytes.extend(length.to_be_bytes());
        } else {
            bytes.extend((length as i16).to_be_bytes());
        }
        bytes.push(0xb1);
        assert!(decode(&bytes, 50).is_ok(), "opcode={opcode:02x}");
        let mut backward = vec![0, opcode];
        if wide {
            backward.extend((-1_i32).to_be_bytes());
        } else {
            backward.extend((-1_i16).to_be_bytes());
        }
        let decoded = decode(&backward, 50).unwrap();
        assert_eq!(decoded.instructions()[1].operands, Operands::Branch(-1));
        for displacement in [-1, 1, 2, length + 1] {
            let mut bytes = vec![opcode];
            if wide {
                bytes.extend(displacement.to_be_bytes());
            } else {
                bytes.extend((displacement as i16).to_be_bytes());
            }
            bytes.push(0xb1);
            let error = decode(&bytes, 50).unwrap_err();
            assert_eq!(error.offset, 0);
            assert!(matches!(error.kind, DecodeErrorKind::InvalidTarget { .. }));
        }
    }
    for displacement in [i32::MIN, i32::MAX] {
        let mut bytes = vec![0xc8];
        bytes.extend(displacement.to_be_bytes());
        assert!(decode(&bytes, 65).is_err());
    }
    // A target can be a multi-byte instruction start but never its operand.
    assert!(decode(&[0x10, 1, 0xa7, 0xff, 0xfe], 65).is_ok());
    assert!(decode(&[0x10, 1, 0xa7, 0xff, 0xff], 65).is_err());
}

#[test]
fn invocation_reserved_bytes_counts_and_constant_indices_are_checked() {
    for bytes in [
        vec![0xb9, 0, 1, 0, 0],
        vec![0xb9, 0, 1, 1, 1],
        vec![0xba, 0, 1, 1, 0],
        vec![0xba, 0, 1, 0, 1],
        vec![0xc5, 0, 1, 0],
    ] {
        assert!(matches!(
            decode(&bytes, 65).unwrap_err().kind,
            DecodeErrorKind::InvalidOperand(_)
        ));
    }
    for opcode in [
        0x12, 0x13, 0x14, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbd, 0xc0,
        0xc1, 0xc5,
    ] {
        let (mut bytes, _) = encoding(opcode);
        bytes[1] = 0;
        if opcode != 0x12 {
            bytes[2] = 0;
        }
        assert!(
            matches!(
                decode(&bytes, 65).unwrap_err().kind,
                DecodeErrorKind::InvalidOperand(_)
            ),
            "opcode={opcode:02x}"
        );
    }
    for atype in 0..=255 {
        assert_eq!(
            decode(&[0xbc, atype], 65).is_ok(),
            (4..=11).contains(&atype)
        );
    }
}

#[test]
fn versions_code_sizes_and_reserved_opcodes_are_structured_errors() {
    for opcode in [0xa8, 0xa9, 0xc9] {
        let (bytes, _) = encoding(opcode);
        assert!(decode(&bytes, 50).is_ok());
        assert!(matches!(
            decode(&bytes, 51).unwrap_err().kind,
            DecodeErrorKind::OpcodeVersion { .. }
        ));
    }
    assert!(decode(&[0xc4, 0xa9, 0, 0], 50).is_ok());
    assert!(matches!(
        decode(&[0xc4, 0xa9, 0, 0], 51).unwrap_err().kind,
        DecodeErrorKind::OpcodeVersion { .. }
    ));
    assert!(matches!(
        decode(&[0xba, 0, 1, 0, 0], 50).unwrap_err().kind,
        DecodeErrorKind::OpcodeVersion { .. }
    ));
    assert!(decode(&[0xba, 0, 1, 0, 0], 51).is_ok());
    for major in [0, 44, 66, u16::MAX] {
        assert!(matches!(
            decode(&[0], major).unwrap_err().kind,
            DecodeErrorKind::UnsupportedVersion { .. }
        ));
    }
    assert!(decode(&[0], 45).is_ok());
    assert!(decode(&[0], 65).is_ok());
    assert_eq!(
        decode(&[], 65).unwrap_err().kind,
        DecodeErrorKind::EmptyCode
    );
    assert_eq!(
        decode(&vec![0; 65535], 65).unwrap().instructions().len(),
        65535
    );
    assert!(matches!(
        decode(&vec![0; 65536], 65).unwrap_err().kind,
        DecodeErrorKind::CodeTooLong { length: 65536 }
    ));
    for opcode in 0xca..=255 {
        assert_eq!(
            decode(&[0, opcode], 65).unwrap_err(),
            crabspot_bytecode::DecodeError {
                offset: 1,
                kind: DecodeErrorKind::InvalidOpcode(opcode)
            }
        );
    }
}

#[test]
fn malformed_and_structured_mutation_corpus_never_panics() {
    let mut state = 0x4352_4142_4445_434f_u64;
    for case in 0..12_000 {
        let length = case % 1024;
        let mut bytes = Vec::new();
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.push(state as u8);
        }
        for major in [50, 65] {
            assert!(
                std::panic::catch_unwind(|| decode(&bytes, major)).is_ok(),
                "case={case}, major={major}"
            );
        }
    }
    for opcode in 0..=0xc9 {
        let (bytes, _) = encoding(opcode);
        for position in 0..bytes.len() {
            for replacement in [0, 0x7f, 0x80, 0xff] {
                let mut mutated = bytes.clone();
                mutated[position] = replacement;
                assert!(std::panic::catch_unwind(|| decode(&mutated, 50)).is_ok());
            }
        }
    }
}

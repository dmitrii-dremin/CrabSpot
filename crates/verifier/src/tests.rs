use super::*;
use crate::{
    inspect::Context,
    scan::{self, Role},
};
use crabspot_classfile::{
    Attribute, AttributeBody, StackMapFrame, StackMapFrameKind as F, VerificationType as V,
};
use std::error::Error;

fn fixture() -> ClassFile {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 8];
    for (tag, data) in [
        (1, b"java/lang/Object".as_slice()),
        (7, &[0, 1]),
        (1, b"run".as_slice()),
        (1, b"()V".as_slice()),
        (1, b"Code".as_slice()),
        (1, b"StackMapTable".as_slice()),
        (1, b"opaque".as_slice()),
    ] {
        bytes.push(tag);
        if tag == 1 {
            bytes.extend((data.len() as u16).to_be_bytes());
        }
        bytes.extend(data);
    }
    for word in [0x21_u16, 2, 0, 0, 0, 1, 9, 3, 4, 1, 5] {
        bytes.extend(word.to_be_bytes());
    }
    bytes.extend(13_u32.to_be_bytes());
    for word in [0_u16, 0] {
        bytes.extend(word.to_be_bytes());
    }
    bytes.extend(1_u32.to_be_bytes());
    bytes.push(0xb1);
    for word in [0_u16, 0, 0] {
        bytes.extend(word.to_be_bytes());
    }
    crabspot_classfile::parse(&bytes).unwrap()
}
fn code(class: &mut ClassFile) -> &mut Code {
    let AttributeBody::Code(code) = &mut class.methods[0].attributes[0].body else {
        panic!("fixture Code")
    };
    code
}
fn options() -> CodeValidationOptions {
    CodeValidationOptions {
        max_code_bytes: u64::MAX,
        max_metadata_entries: u64::MAX,
        max_utf16_units: u64::MAX,
        max_work_units: u64::MAX,
        ..Default::default()
    }
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn simple_method_and_bodyless_results_have_hand_derived_exact_usage_and_borrows() {
    let mut class = fixture();
    let result = validate_method_code(&class, 0, CodeValidationOptions::default()).unwrap();
    assert!(std::ptr::eq(result.class(), &class));
    assert!(std::ptr::eq(result.method(), &class.methods[0]));
    assert_eq!(result.method_index(), 0);
    assert_eq!(result.decoded().unwrap().code_len(), 1);
    assert_eq!(
        result.usage(),
        CodeValidationUsage {
            code_bytes: 1,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 16
        }
    );
    drop(result);
    class.methods[0].access_flags = 0x109;
    class.methods[0].attributes.clear();
    let result = validate_method_code(&class, 0, options()).unwrap();
    assert!(result.code().is_none() && result.decoded().is_none());
    assert_eq!(
        result.usage(),
        CodeValidationUsage {
            code_bytes: 0,
            metadata_entries: 0,
            utf16_units: 6,
            work_units: 8
        }
    );
}
#[test]
fn atomic_resource_funding_and_overflow_never_advance_any_counter_on_failure() {
    let class = fixture();
    let mut ctx = Context::new(&class, 0, options());
    ctx.usage = CodeValidationUsage {
        code_bytes: u64::MAX - 1,
        metadata_entries: u64::MAX - 1,
        utf16_units: u64::MAX - 1,
        work_units: u64::MAX - 1,
    };
    ctx.charge(1, 1, 1, 1).unwrap();
    let before = ctx.usage;
    for amounts in [(1, 0, 0, 0), (0, 1, 0, 0), (0, 0, 1, 0), (0, 0, 0, 1)] {
        assert!(matches!(
            ctx.charge(amounts.0, amounts.1, amounts.2, amounts.3),
            Err(CodeValidationErrorKind::AccountingOverflow { .. })
        ));
        assert_eq!(ctx.usage, before);
    }
    let mut ctx = Context::new(
        &class,
        0,
        CodeValidationOptions {
            max_work_units: 0,
            ..Default::default()
        },
    );
    assert_eq!(
        ctx.charge(1, 1, 1, 1),
        Err(CodeValidationErrorKind::ResourceLimit {
            resource: CodeValidationResource::WorkUnits,
            limit: 0,
            required: 1
        })
    );
    assert_eq!(ctx.usage, CodeValidationUsage::default());
}
#[test]
fn zero_exact_and_maximum_quotas_fund_only_actual_inputs() {
    let class = fixture();
    let exact = CodeValidationOptions {
        max_code_bytes: 1,
        max_metadata_entries: 1,
        max_utf16_units: 10,
        max_work_units: 16,
        ..Default::default()
    };
    assert_eq!(
        validate_method_code(&class, 0, exact)
            .unwrap()
            .usage()
            .work_units,
        16
    );
    assert!(validate_method_code(&class, 0, options()).is_ok());
    for resource in 0..4 {
        let mut limited = exact;
        match resource {
            0 => limited.max_code_bytes = 0,
            1 => limited.max_metadata_entries = 0,
            2 => limited.max_utf16_units = 0,
            _ => limited.max_work_units = 0,
        }
        let error = validate_method_code(&class, 0, limited).unwrap_err();
        assert!(matches!(
            error.kind,
            CodeValidationErrorKind::ResourceLimit { .. }
        ));
        assert!(
            error.usage.code_bytes <= limited.max_code_bytes
                && error.usage.metadata_entries <= limited.max_metadata_entries
                && error.usage.utf16_units <= limited.max_utf16_units
                && error.usage.work_units <= limited.max_work_units
        );
    }
    let error = validate_method_code(
        &class,
        0,
        CodeValidationOptions {
            max_work_units: 14,
            ..exact
        },
    )
    .unwrap_err();
    // Attribute name funding completes at14; the byte charge needs15 and leaves
    // BOTH its CodeBytes and WorkUnits untouched.
    assert_eq!(
        error.usage,
        CodeValidationUsage {
            code_bytes: 0,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 14
        }
    );
}
#[test]
fn allocation_failure_keeps_width_buffer_and_invalid_scans_are_never_cached() {
    let mut widths = vec![1_u8, 2];
    assert!(matches!(
        scan::reserve(&mut widths, usize::MAX, "test widths"),
        Err(CodeValidationErrorKind::AllocationFailure { .. })
    ));
    assert_eq!(widths, [1, 2]);
    let class = fixture();
    let mut ctx = Context::new(&class, 0, options());
    for _ in 0..2 {
        assert!(matches!(
            ctx.utf(3, Role::Method, None),
            Err(CodeValidationErrorKind::InvalidDescriptor { index: 3 })
        ));
    }
    assert_eq!(ctx.usage.utf16_units, 6);
    assert_eq!(ctx.usage.work_units, 8);
    assert!(ctx.utf(4, Role::Method, None).is_ok());
    assert!(ctx.utf(4, Role::Method, None).is_ok());
    assert_eq!(ctx.usage.utf16_units, 9);
    assert_eq!(ctx.usage.work_units, 13);
    assert!(ctx.utf(4, Role::Class, None).is_ok()); // Same CP index, different role.
    assert_eq!(ctx.usage.utf16_units, 12);
    let before = ctx.usage;
    assert!(matches!(
        ctx.reserve_cache(usize::MAX),
        Err(CodeValidationErrorKind::AllocationFailure {
            resource: "UTF-16 summary cache"
        })
    ));
    assert_eq!(ctx.usage, before);
    ctx.utf(4, Role::Method, None).unwrap();
    assert_eq!(ctx.usage.utf16_units, before.utf16_units);
}
#[test]
fn descriptor_scanner_preserves_widths_ranks_grammar_and_utf16_names() {
    let mut widths = vec![];
    let summary =
        scan::inspect(&units("(J[DILp/Ω;)D"), 4, Role::Method, Some(&mut widths)).unwrap();
    assert_eq!(summary.slots, 5);
    assert!(!summary.void);
    assert_eq!(widths, [2, 1, 1, 1]);
    for (desc, width, rank) in [
        ("J", 2, 0),
        ("D", 2, 0),
        ("[J", 1, 1),
        ("[[I", 1, 2),
        ("Ljava/lang/Object;", 1, 0),
    ] {
        let summary = scan::inspect(&units(desc), 4, Role::Field, None).unwrap();
        assert_eq!((summary.width, summary.rank), (width, rank));
    }
    assert_eq!(
        scan::inspect(&units(&("[".repeat(255) + "I")), 4, Role::Class, None)
            .unwrap()
            .rank,
        255
    );
    for bad in ["", "V", "[V", "L;", "La//b;", "La.b;", "Lx", "II"] {
        assert!(
            scan::inspect(&units(bad), 4, Role::Field, None).is_err(),
            "{bad}"
        );
    }
    assert!(scan::inspect(&units(&("[".repeat(256) + "I")), 4, Role::Class, None).is_err());
    let surrogate = [76, 0xd800, 59];
    assert!(scan::inspect(&surrogate, 4, Role::Field, None).is_ok());
    assert_eq!(
        scan::inspect(
            &units(&("(".to_owned() + &"I".repeat(255) + ")V")),
            4,
            Role::Method,
            None
        )
        .unwrap()
        .slots,
        255
    );
    assert!(
        scan::inspect(
            &units(&("(".to_owned() + &"I".repeat(256) + ")V")),
            4,
            Role::Method,
            None
        )
        .is_err()
    );
}
#[test]
fn normalized_stackmaps_track_entries_and_slots_without_expanding_same_frames() {
    let mut class = fixture();
    let body = code(&mut class);
    body.max_locals = 5;
    body.max_stack = 2;
    body.code = vec![0, 0, 0, 0, 0xb1];
    body.attributes.push(Attribute {
        offset: 0,
        name_index: 6,
        body: AttributeBody::StackMapTable(vec![
            StackMapFrame {
                offset_delta: 0,
                bytecode_offset: 0,
                kind: F::Full {
                    locals: vec![V::Long, V::Double, V::Top],
                    stack: vec![V::Long],
                },
            },
            StackMapFrame {
                offset_delta: 0,
                bytecode_offset: 1,
                kind: F::Same,
            },
            StackMapFrame {
                offset_delta: 0,
                bytecode_offset: 2,
                kind: F::Chop(2),
            },
            StackMapFrame {
                offset_delta: 0,
                bytecode_offset: 3,
                kind: F::Append(vec![V::Integer]),
            },
        ]),
    });
    let usage = validate_method_code(&class, 0, options()).unwrap().usage();
    assert_eq!(
        usage,
        CodeValidationUsage {
            code_bytes: 5,
            metadata_entries: 11,
            utf16_units: 23,
            work_units: 52
        }
    );
    code(&mut class).max_stack = 1;
    assert!(matches!(
        validate_method_code(&class, 0, options()).unwrap_err().kind,
        CodeValidationErrorKind::InvalidStackMap(_)
    ));
    code(&mut class).max_stack = 2;
    code(&mut class).max_locals = 4;
    assert!(matches!(
        validate_method_code(&class, 0, options()).unwrap_err().kind,
        CodeValidationErrorKind::InvalidStackMap(_)
    ));
}
#[test]
fn error_sources_and_metadata_count_guards_are_structured() {
    let mut class = fixture();
    code(&mut class).code = vec![0x10];
    let error = validate_method_code(&class, 0, options()).unwrap_err();
    assert!(error.source().is_some());
    assert!(error.to_string().contains("bytecode"));
    assert!(Context::count(usize::from(u16::MAX)).is_ok());
    assert!(Context::count(usize::from(u16::MAX) + 1).is_err());
    let error = validate_method_code(&class, usize::MAX, options()).unwrap_err();
    assert_eq!(error.method_index, usize::MAX);
    assert!(error.source().is_none());
}

#[test]
fn wrong_fixed_ldc_category_stops_before_following_its_class_name_link() {
    let mut class = fixture();
    code(&mut class).code = vec![0x14, 0, 2, 0xb1];
    let error = validate_method_code(&class, 0, options()).unwrap_err();
    assert!(matches!(
        error.kind,
        CodeValidationErrorKind::InvalidConstant { index: 2, .. }
    ));
    assert_eq!(error.location, CodeValidationLocation::Instruction(0));
    assert_eq!(
        error.usage,
        CodeValidationUsage {
            code_bytes: 4,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 20
        }
    );
}
#[test]
fn deterministic_bounded_mutations_never_panic_or_mutate_input() {
    let mut state = 0x9e37_79b9_u32;
    for case in 0..2048 {
        let mut class = fixture();
        let mut bytes = Vec::new();
        for _ in 0..case % 96 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            bytes.push((state >> 24) as u8);
        }
        code(&mut class).code = bytes;
        code(&mut class).max_locals = (case % 8) as u16;
        class.version.major = 45 + (case % 21) as u16;
        let before = class.clone();
        let first = validate_method_code(&class, 0, options());
        let second = validate_method_code(&class, 0, options());
        match (first, second) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.usage(), b.usage());
                assert_eq!(a.decoded(), b.decoded());
            }
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("nondeterministic validation"),
        }
        assert_eq!(class, before);
    }
}

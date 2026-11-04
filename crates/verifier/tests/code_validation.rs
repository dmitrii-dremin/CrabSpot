//! Independent public API checks: partial whole-Code validation, never type verification.
mod support;
use crabspot_classfile::{
    Attribute, AttributeBody, ClassVersion, ExceptionHandler, StackMapFrame,
    StackMapFrameKind as K, VerificationType as V,
};
use crabspot_verifier::{
    CodeValidationError as Error, CodeValidationErrorKind as E, CodeValidationLocation as L,
    CodeValidationOptions as Options, CodeValidationResource as R, CodeValidationUsage as Usage,
    validate_method_code as validate,
};
use support::*;
fn good(class: &crabspot_classfile::ClassFile) -> crabspot_verifier::MethodCodeValidation<'_> {
    match validate(class, 0, Options::default()) {
        Ok(value) => value,
        Err(e) => panic!("unexpected rejection: {e}"),
    }
}
fn bad(class: &crabspot_classfile::ClassFile) -> Error {
    match validate(class, 0, Options::default()) {
        Err(e) => e,
        Ok(_) => panic!("unexpected partial validation success"),
    }
}
fn limited(class: &crabspot_classfile::ClassFile, options: Options) -> Error {
    match validate(class, 0, options) {
        Err(e) => e,
        Ok(_) => panic!("expected configured failure"),
    }
}
fn with_frames(class: &mut crabspot_classfile::ClassFile, name: u16, frames: Vec<StackMapFrame>) {
    code_mut(class).attributes.push(Attribute {
        offset: usize::MAX,
        name_index: name,
        body: AttributeBody::StackMapTable(frames),
    });
}
#[test]
fn result_borrows_original_metadata_owns_one_decode_and_repeats_deterministically_without_mutation()
{
    let class = simple();
    let before = class.clone();
    let result = good(&class);
    assert!(std::ptr::eq(result.class(), &class));
    assert!(std::ptr::eq(result.method(), member(&class)));
    assert!(std::ptr::eq(result.code().unwrap(), code(&class)));
    assert_eq!(result.method_index(), 0);
    assert_eq!(result.decoded().unwrap().code_len(), 1);
    assert_eq!(result.decoded().unwrap().instructions()[0].opcode, 0xb1);
    assert_eq!(result.usage(), good(&class).usage());
    assert_eq!(class, before);
}
#[test]
fn bodyless_native_and_abstract_have_both_optional_getters_absent() {
    for flags in [0x109, 0x401] {
        let mut b = Builder::new("()V", &[0xb1], 0, 1);
        b.flags = flags;
        b.code = None;
        let class = b.finish();
        let result = good(&class);
        assert!(result.code().is_none());
        assert!(result.decoded().is_none());
        assert_eq!(result.usage().code_bytes, 0);
        assert_eq!(result.usage().metadata_entries, 0);
    }
}
#[test]
fn method_index_and_version_policy_precede_all_selected_metadata() {
    let mut class = simple();
    class.version = ClassVersion {
        major: 66,
        minor: 0,
    };
    class.methods.clear();
    let e = match validate(&class, usize::MAX, Options::default()) {
        Err(e) => e,
        Ok(_) => panic!(),
    };
    assert_eq!(
        e.kind,
        E::UnsupportedVersion {
            major: 66,
            minor: 0
        }
    );
    assert_eq!(
        e.usage,
        Usage {
            code_bytes: 0,
            metadata_entries: 0,
            utf16_units: 0,
            work_units: 0
        }
    );
    class.version = ClassVersion {
        major: 65,
        minor: 0,
    };
    let e = match validate(&class, 7, Options::default()) {
        Err(e) => e,
        Ok(_) => panic!(),
    };
    assert_eq!(e.method_index, 7);
    assert_eq!(e.kind, E::MethodIndexOutOfBounds { index: 7, count: 0 });
    assert_eq!(e.location, L::Method);
}
#[test]
fn historical_minors_preview_and_current_release_policy_are_explicit() {
    for (major, minor, okay) in [
        (45, 65535, true),
        (55, 7, true),
        (56, 0, true),
        (64, 0, true),
        (56, 65535, false),
        (64, 65535, false),
        (65, 1, false),
        (44, 0, false),
    ] {
        let mut class = simple();
        class.version = ClassVersion { major, minor };
        assert_eq!(
            validate(&class, 0, Options::default()).is_ok(),
            okay,
            "{major}.{minor}"
        );
    }
    let mut class = simple();
    class.version.minor = 65535;
    assert_eq!(bad(&class).kind, E::PreviewDisabled);
    assert!(
        validate(
            &class,
            0,
            Options {
                allow_preview: true,
                ..Options::default()
            }
        )
        .is_ok()
    );
}
#[test]
fn code_presence_and_typed_header_agreement_reject_mutable_model_damage() {
    for mode in 0..6 {
        let mut class = simple();
        match mode {
            0 => class.methods[0].attributes.clear(),
            1 => {
                let duplicate = class.methods[0].attributes[0].clone();
                class.methods[0].attributes.push(duplicate);
            }
            2 => class.methods[0].access_flags |= 0x100,
            3 => class.methods[0].attributes[0].name_index = class.methods[0].name_index,
            4 => {
                class.methods[0].attributes[0].body = AttributeBody::Opaque {
                    bytes: vec![],
                    standard: false,
                }
            }
            _ => class.methods[0].attributes[0].name_index = 0,
        };
        assert!(
            matches!(
                bad(&class).kind,
                E::InvalidMetadata(_) | E::InvalidConstant { .. }
            ),
            "{mode}"
        );
    }
}
#[test]
fn malformed_instruction_bytes_decode_before_dead_operand_or_handler_semantics() {
    let mut class = simple();
    code_mut(&mut class).code = vec![0xb1, 0xb2, 0, 1, 0xc4, 0x00];
    code_mut(&mut class).exception_table.push(ExceptionHandler {
        start_pc: 0,
        end_pc: 1,
        handler_pc: 0,
        catch_type: 0xffff,
    });
    let e = bad(&class);
    assert!(matches!(e.kind, E::Decode(_)));
    assert_eq!(e.location, L::Instruction(4));
    assert!(std::error::Error::source(&e).is_some());
}
#[test]
fn dead_encoded_operand_defects_fail_while_deferred_stack_local_flow_and_falloff_can_pass() {
    for code in [
        vec![0x04, 0xac, 0xb2, 0, 2, 0xb1],
        vec![0x04, 0xac, 0x1a, 0xac],
    ] {
        let class = Builder::new("()I", &code, 1, 0).finish();
        assert_eq!(bad(&class).location, L::Instruction(2));
    }
    for (code, locals) in [
        (vec![0x04, 0xac, 0x01, 0xac], 0),
        (vec![0x04, 0xac, 0x1a, 0xac], 1),
        (vec![0x00], 0),
        (vec![0x09, 0xac], 0),
    ] {
        let class = Builder::new("()I", &code, 2, locals).finish();
        good(&class);
    }
}
#[test]
fn syntactically_valid_unsupported_execution_opcodes_are_not_filtered_by_interpreter_coverage() {
    for code in [vec![0xc2, 0xc3, 0xb1], vec![0xbf], vec![0x5f, 0xb1]] {
        good(&Builder::new("()V", &code, 0, 0).finish());
    }
}
#[test]
fn simple_exact_usage_and_atomic_resource_failures_are_independently_counted() {
    let class = simple();
    assert_eq!(
        good(&class).usage(),
        Usage {
            code_bytes: 1,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 16
        }
    );
    for (resource, options, prefix, required) in [
        (
            R::WorkUnits,
            Options {
                max_work_units: 0,
                ..Options::default()
            },
            Usage {
                code_bytes: 0,
                metadata_entries: 0,
                utf16_units: 0,
                work_units: 0,
            },
            1,
        ),
        (
            R::Utf16Units,
            Options {
                max_utf16_units: 0,
                ..Options::default()
            },
            Usage {
                code_bytes: 0,
                metadata_entries: 0,
                utf16_units: 0,
                work_units: 1,
            },
            3,
        ),
        (
            R::MetadataEntries,
            Options {
                max_metadata_entries: 0,
                ..Options::default()
            },
            Usage {
                code_bytes: 0,
                metadata_entries: 0,
                utf16_units: 6,
                work_units: 8,
            },
            1,
        ),
        (
            R::CodeBytes,
            Options {
                max_code_bytes: 0,
                ..Options::default()
            },
            Usage {
                code_bytes: 0,
                metadata_entries: 1,
                utf16_units: 10,
                work_units: 14,
            },
            1,
        ),
        (
            R::WorkUnits,
            Options {
                max_work_units: 14,
                ..Options::default()
            },
            Usage {
                code_bytes: 0,
                metadata_entries: 1,
                utf16_units: 10,
                work_units: 14,
            },
            15,
        ),
    ] {
        let e = limited(&class, options);
        assert_eq!(e.usage, prefix);
        assert!(
            matches!(e.kind,E::ResourceLimit{resource:r,required:n,..}if r==resource&&n==required)
        );
    }
    assert!(
        validate(
            &class,
            0,
            Options {
                max_code_bytes: 1,
                max_metadata_entries: 1,
                max_utf16_units: 10,
                max_work_units: 16,
                allow_preview: false
            }
        )
        .is_ok()
    );
    assert!(
        validate(
            &class,
            0,
            Options {
                max_code_bytes: u64::MAX,
                max_metadata_entries: u64::MAX,
                max_utf16_units: u64::MAX,
                max_work_units: u64::MAX,
                allow_preview: false
            }
        )
        .is_ok()
    );
}

#[test]
fn every_explicit_and_implicit_local_opcode_checks_its_actual_width_even_in_dead_code() {
    for (opcode, width) in [
        (0x15, 1),
        (0x16, 2),
        (0x17, 1),
        (0x18, 2),
        (0x19, 1),
        (0x36, 1),
        (0x37, 2),
        (0x38, 1),
        (0x39, 2),
        (0x3a, 1),
    ] {
        for wide in [false, true] {
            let index = if wide { 65534u16 } else { 255 };
            let mut bytes = vec![0xb1];
            if wide {
                bytes.extend([0xc4, opcode]);
                bytes.extend(index.to_be_bytes());
            } else {
                bytes.extend([opcode, index as u8]);
            }
            bytes.push(0xb1);
            let capacity = u32::from(index) + u32::from(width);
            if capacity <= 65535 {
                good(&Builder::new("()V", &bytes, 0, capacity as u16).finish());
            }
            let class = Builder::new("()V", &bytes, 0, (capacity - 1) as u16).finish();
            let e = bad(&class);
            assert_eq!(e.location, L::Instruction(1));
            assert_eq!(
                e.kind,
                E::LocalOutOfBounds {
                    index,
                    width,
                    max_locals: (capacity - 1) as u16
                }
            );
        }
    }
    for (start, width) in [
        (0x1a, 1),
        (0x1e, 2),
        (0x22, 1),
        (0x26, 2),
        (0x2a, 1),
        (0x3b, 1),
        (0x3f, 2),
        (0x43, 1),
        (0x47, 2),
        (0x4b, 1),
    ] {
        for index in 0..4u16 {
            let bytes = [0xb1, start + index as u8, 0xb1];
            good(&Builder::new("()V", &bytes, 0, index + u16::from(width)).finish());
            assert_eq!(
                bad(&Builder::new("()V", &bytes, 0, index + u16::from(width) - 1).finish()).kind,
                E::LocalOutOfBounds {
                    index,
                    width,
                    max_locals: index + u16::from(width) - 1
                }
            );
        }
    }
}
#[test]
fn iinc_ret_and_jsr_widths_and_historical_version_restrictions_are_preserved() {
    for code in [
        vec![0x84, 255, 0x80, 0xb1],
        vec![0xc4, 0x84, 0xff, 0xfe, 0x80, 0, 0xb1],
    ] {
        let index = if code[0] == 0xc4 { 65534 } else { 255 };
        good(&Builder::new("()V", &code, 0, index + 1).finish());
        assert_eq!(
            bad(&Builder::new("()V", &code, 0, index).finish()).kind,
            E::LocalOutOfBounds {
                index,
                width: 1,
                max_locals: index
            }
        );
    }
    for code in [
        vec![0xa9, 0, 0xb1],
        vec![0xc4, 0xa9, 0, 0, 0xb1],
        vec![0xa8, 0, 3, 0xb1],
        vec![0xc9, 0, 0, 0, 5, 0xb1],
    ] {
        let mut class = Builder::new("()V", &code, 0, 1).finish();
        class.version.major = 50;
        good(&class);
        class.version.major = 51;
        assert!(matches!(bad(&class).kind, E::Decode(_)));
    }
}
#[test]
fn field_and_method_families_check_tags_throughout_unreachable_instructions() {
    for opcode in 0xb2..=0xb5 {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let field = b.pool.member(9, "q/Absent", "field", "J");
        let method = b.pool.member(10, "q/Absent", "method", "()V");
        let mut class = b.finish();
        for (index, valid) in [(field, true), (method, false)] {
            let mut bytes = vec![0xb1];
            bytes.extend(op(opcode, index));
            bytes.push(0xb1);
            code_mut(&mut class).code = bytes;
            if valid {
                good(&class);
            } else {
                let e = bad(&class);
                assert_eq!(e.location, L::Instruction(1));
                assert!(matches!(e.kind,E::InvalidConstant{index:i,..}if i==index));
            }
        }
    }
    for opcode in [0xb6, 0xb7, 0xb8, 0xb9, 0xba] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let target = if opcode == 0xb9 {
            b.pool.member(11, "q/I", "m", "()V")
        } else if opcode == 0xba {
            b.pool.dynamic(18, "m", "()V")
        } else {
            b.pool.member(10, "q/C", "m", "()V")
        };
        let bad_target = b.pool.class("q/C");
        let mut class = b.finish();
        for (index, valid) in [(target, true), (bad_target, false)] {
            let mut bytes = op(opcode, index);
            if opcode == 0xb9 {
                bytes.extend([1, 0]);
            }
            if opcode == 0xba {
                bytes.extend([0, 0]);
            }
            bytes.push(0xb1);
            code_mut(&mut class).code = bytes;
            if valid {
                good(&class);
            } else {
                assert!(matches!(bad(&class).kind,E::InvalidConstant{index:i,..}if i==index));
            }
        }
    }
}
#[test]
fn interface_methodref_b7_b8_version_edges_and_constructor_name_restrictions_are_local() {
    for opcode in [0xb7, 0xb8] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let reference = b.pool.member(11, "q/I", "v", "()V");
        let mut class = b.finish();
        let mut bytes = op(opcode, reference);
        bytes.push(0xb1);
        code_mut(&mut class).code = bytes;
        class.version.major = 52;
        good(&class);
        class.version.major = 51;
        assert!(matches!(
            bad(&class).kind,
            E::InvalidConstant { .. } | E::InvalidOperand(_)
        ));
    }
    for opcode in [0xb6, 0xb7, 0xb8] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let reference = b.pool.member(10, "q/C", "<init>", "()V");
        let mut class = b.finish();
        let mut bytes = op(opcode, reference);
        bytes.push(0xb1);
        code_mut(&mut class).code = bytes;
        if opcode == 0xb7 {
            good(&class);
        } else {
            assert!(matches!(
                bad(&class).kind,
                E::InvalidName { .. } | E::InvalidOperand(_)
            ));
        }
    }
}
#[test]
fn invokeinterface_counts_and_all_invocation_receiver_slot_bounds_include_category_two_values() {
    for (params, count, valid) in [
        ("JDI".to_string(), 6, true),
        ("JDI".to_string(), 5, false),
        ("I".repeat(254), 255, true),
        ("I".repeat(255), 255, false),
    ] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let reference = b.pool.member(11, "q/I", "v", &format!("({params})V"));
        let mut class = b.finish();
        let mut bytes = op(0xb9, reference);
        bytes.extend([count, 0, 0xb1]);
        code_mut(&mut class).code = bytes;
        assert_eq!(validate(&class, 0, Options::default()).is_ok(), valid);
    }
    for opcode in [0xb6, 0xb7, 0xb8, 0xba] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let desc = format!("({})V", "I".repeat(255));
        let reference = if opcode == 0xba {
            b.pool.dynamic(18, "v", &desc)
        } else {
            b.pool.member(10, "q/C", "v", &desc)
        };
        let mut class = b.finish();
        let mut bytes = op(opcode, reference);
        if opcode == 0xba {
            bytes.extend([0, 0]);
        }
        bytes.push(0xb1);
        code_mut(&mut class).code = bytes;
        assert_eq!(
            validate(&class, 0, Options::default()).is_ok(),
            matches!(opcode, 0xb8 | 0xba)
        );
    }
}
#[test]
fn class_operand_families_rank_edges_and_method_owner_arrays_follow_representation_not_execution_policy()
 {
    for (opcode, name, dimensions, valid) in [
        (0xbb, "q/C".to_string(), 0, true),
        (0xbb, "[I".to_string(), 0, false),
        (0xbd, "q/C".to_string(), 0, true),
        (0xbd, format!("{}I", "[".repeat(254)), 0, true),
        (0xbd, format!("{}I", "[".repeat(255)), 0, false),
        (0xc0, format!("{}I", "[".repeat(255)), 0, true),
        (0xc1, "[Lq/C;".to_string(), 0, true),
        (0xc5, "[[I".to_string(), 2, true),
        (0xc5, "[[I".to_string(), 3, false),
        (0xc5, "q/C".to_string(), 1, false),
    ] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let reference = b.pool.class(&name);
        let mut class = b.finish();
        let mut bytes = op(opcode, reference);
        if opcode == 0xc5 {
            bytes.push(dimensions);
        }
        bytes.push(0xb1);
        code_mut(&mut class).code = bytes;
        assert_eq!(
            validate(&class, 0, Options::default()).is_ok(),
            valid,
            "{opcode:x} {name}"
        );
    }
    let mut b = Builder::new("()V", &[0xb1], 0, 0);
    let reference = b.pool.member(10, "[I", "clone", "()Ljava/lang/Object;");
    let mut class = b.finish();
    let mut bytes = op(0xb6, reference);
    bytes.push(0xb1);
    code_mut(&mut class).code = bytes;
    good(&class);
}
#[test]
fn ldc_categories_dynamic_widths_and_real_wide_continuations_are_checked_without_bootstrap_certification()
 {
    for category in 0..10 {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let (index, wide) = match category {
            0 => (b.pool.add(3, &7i32.to_be_bytes()), false),
            1 => (b.pool.add(4, &0xffc00042u32.to_be_bytes()), false),
            2 => (b.pool.add(5, &i64::MIN.to_be_bytes()), true),
            3 => (b.pool.add(6, &0x8000000000000000u64.to_be_bytes()), true),
            4 => (b.pool.class("[I"), false),
            5 => (b.pool.string(&[0, 0xd800, 0x41]), false),
            6 => (b.pool.method_type("(J)D"), false),
            7 => {
                let member = b.pool.member(10, "q/C", "m", "()V");
                (b.pool.handle(6, member), false)
            }
            8 => (b.pool.dynamic(17, "value", "J"), true),
            _ => (b.pool.dynamic(17, "value", "Ljava/lang/Object;"), false),
        };
        let mut class = b.finish();
        class.attributes.clear();
        for opcode in [0x12, 0x13, 0x14] {
            let mut bytes = if opcode == 0x12 {
                vec![opcode, index as u8]
            } else {
                op(opcode, index)
            };
            bytes.push(0xb1);
            code_mut(&mut class).code = bytes;
            assert_eq!(
                validate(&class, 0, Options::default()).is_ok(),
                (opcode == 0x14) == wide,
                "category{category} opcode{opcode:x}"
            );
        }
        if matches!(category, 2 | 3) {
            code_mut(&mut class).code = [op(0x14, index + 1), vec![0xb1]].concat();
            assert!(matches!(bad(&class).kind,E::InvalidConstant{index:i,..}if i==index+1));
        }
    }
}
#[test]
fn loadable_constant_version_checks_use_current_class_not_the_pool_source_version() {
    for (kind, major) in [(0, 49), (1, 51), (2, 51), (3, 55)] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let index = match kind {
            0 => b.pool.class("q/C"),
            1 => b.pool.method_type("()V"),
            2 => {
                let member = b.pool.member(10, "q/C", "m", "()V");
                b.pool.handle(6, member)
            }
            _ => b.pool.dynamic(17, "x", "I"),
        };
        let mut class = b.finish();
        code_mut(&mut class).code = vec![0x12, index as u8, 0xb1];
        class.version.major = major;
        good(&class);
        class.version.major = major - 1;
        assert!(validate(&class, 0, Options::default()).is_err());
        code_mut(&mut class).code = vec![0xb1];
        good(&class);
    }
}
#[test]
fn all_methodhandle_kinds_and_current_version_interface_target_rules_are_inspected() {
    for kind in 1..=9 {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let target = if kind <= 4 {
            b.pool.member(9, "q/C", "field", "I")
        } else if kind == 9 {
            b.pool.member(11, "q/I", "m", "()V")
        } else {
            b.pool
                .member(10, "q/C", if kind == 8 { "<init>" } else { "m" }, "()V")
        };
        let handle = b.pool.handle(kind, target);
        let mut class = b.finish();
        code_mut(&mut class).code = vec![0x12, handle as u8, 0xb1];
        good(&class);
    }
    for kind in [6, 7] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let target = b.pool.member(11, "q/I", "m", "()V");
        let handle = b.pool.handle(kind, target);
        let mut class = b.finish();
        code_mut(&mut class).code = vec![0x12, handle as u8, 0xb1];
        class.version.major = 52;
        good(&class);
        class.version.major = 51;
        assert!(validate(&class, 0, Options::default()).is_err());
    }
}
#[test]
fn string_literal_contents_are_not_semantically_scanned_or_charged() {
    let mut usages = Vec::new();
    for count in [1, 65000] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let index = b.pool.string(&vec![120; count]);
        let mut class = b.finish();
        code_mut(&mut class).code = vec![0x12, index as u8, 0xb1];
        usages.push(good(&class).usage());
    }
    assert_eq!(usages[0], usages[1]);
    assert_eq!(usages[0].utf16_units, 10);
}
#[test]
fn selected_roles_descriptor_grammar_receiver_slots_and_parameter_locals_are_checked() {
    let mut c = Builder::new("(JD)V", &[0xb1], 0, 4).finish();
    good(&c);
    code_mut(&mut c).max_locals = 3;
    assert!(matches!(bad(&c).kind, E::InvalidMetadata(_)));
    let mut c = Builder::new(&format!("({})V", "I".repeat(255)), &[0xb1], 0, 255).finish();
    good(&c);
    c.methods[0].access_flags = 1;
    assert!(matches!(
        bad(&c).kind,
        E::InvalidDescriptor { .. } | E::InvalidMetadata(_)
    ));
    for (name, desc, flags, class_flags, valid) in [
        ("<init>", "()V", 1, 0x21, true),
        ("<init>", "()V", 9, 0x21, false),
        ("<init>", "()I", 1, 0x21, false),
        ("<init>", "()V", 1, 0x601, false),
        ("bad/name", "()V", 9, 0x21, false),
        ("run", "(I", 9, 0x21, false),
    ] {
        let mut b = Builder::new("()V", &[0xb1], 0, 1);
        let n = b.pool.text(name);
        let d = b.pool.text(desc);
        let mut c = b.finish();
        c.methods[0].name_index = n;
        c.methods[0].descriptor_index = d;
        c.methods[0].access_flags = flags;
        c.access_flags = class_flags;
        assert_eq!(
            validate(&c, 0, Options::default()).is_ok(),
            valid,
            "{name} {desc} flags {flags}"
        );
    }
}
#[test]
fn historical_initializer_policy_preserves_ignored_flags_but_orders_grammar_and_void_first() {
    for (major, flags) in [(50, 0x500), (65, 0x508)] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let n = b.pool.text("<clinit>");
        let mut c = b.finish();
        c.methods[0].name_index = n;
        c.methods[0].access_flags = flags;
        c.version.major = major;
        good(&c);
    }
    for (desc, expected) in [("(I)V", 0), ("(I)I", 1), ("(I", 2)] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let n = b.pool.text("<clinit>");
        let d = b.pool.text(desc);
        let mut c = b.finish();
        c.methods[0].name_index = n;
        c.methods[0].descriptor_index = d;
        c.version.major = 50;
        code_mut(&mut c).code = vec![0xff];
        let e = bad(&c);
        assert_eq!(e.location, L::Method);
        match expected {
            0 => assert_eq!(e.kind, E::UnsupportedHistoricalInitializer),
            1 => assert!(matches!(e.kind, E::InvalidMetadata(_))),
            _ => assert!(matches!(e.kind, E::InvalidDescriptor { .. })),
        }
    }
}
#[test]
fn code_length_limits_precede_precharge_and_wide_embedded_targets_are_not_boundaries() {
    for len in [0, 65536] {
        let mut c = simple();
        code_mut(&mut c).code = vec![0; len];
        let e = bad(&c);
        assert!(matches!(e.kind, E::InvalidMetadata(_)));
        assert_eq!(e.usage.code_bytes, 0);
    }
    let mut c = simple();
    code_mut(&mut c).code = vec![0; 65535];
    assert_eq!(good(&c).usage().code_bytes, 65535);
    // goto target 4 is the embedded iload, not the wide-prefix start at 3.
    code_mut(&mut c).code = vec![0xa7, 0, 4, 0xc4, 0x15, 0, 0, 0xb1];
    assert!(matches!(bad(&c).kind, E::Decode(_)));
}
#[test]
fn handler_boundaries_types_and_pass_order_are_file_local() {
    let mut b = Builder::new("()V", &[0x10, 1, 0xb1, 0xb1], 0, 0);
    let throwable = b.pool.class("absent/Throwable");
    let array = b.pool.class("[I");
    let wrong = b.pool.text("not-a-Class");
    let c = b.finish();
    for catch_type in [0, throwable] {
        let mut c = c.clone();
        code_mut(&mut c).exception_table = vec![ExceptionHandler {
            start_pc: 0,
            end_pc: 4,
            handler_pc: 3,
            catch_type,
        }];
        good(&c); // No stack-depth or Throwable ancestry proof is claimed.
    }
    for (start_pc, end_pc, handler_pc, catch_type) in [
        (1, 2, 3, 0),
        (0, 1, 3, 0),
        (0, 2, 1, 0),
        (2, 2, 3, 0),
        (3, 2, 3, 0),
        (0, 5, 3, 0),
        (0, 2, 4, 0),
        (0, 2, 3, array),
        (0, 2, 3, wrong),
    ] {
        let mut c = c.clone();
        code_mut(&mut c).exception_table = vec![ExceptionHandler {
            start_pc,
            end_pc,
            handler_pc,
            catch_type,
        }];
        let e = bad(&c);
        assert_eq!(e.location, L::ExceptionHandler(0));
        assert!(matches!(
            e.kind,
            E::InvalidBoundary { .. }
                | E::InvalidMetadata(_)
                | E::InvalidConstant { .. }
                | E::InvalidOperand(_)
        ));
    }
    let mut c = c;
    code_mut(&mut c).exception_table = vec![ExceptionHandler {
        start_pc: 1,
        end_pc: 2,
        handler_pc: 3,
        catch_type: 0,
    }];
    code_mut(&mut c).attributes.push(Attribute {
        offset: 0,
        name_index: 0,
        body: AttributeBody::Deprecated,
    });
    assert_eq!(bad(&c).location, L::ExceptionHandler(0));
}
fn frame(offset: u32, kind: K) -> StackMapFrame {
    StackMapFrame {
        offset_delta: 0,
        bytecode_offset: offset,
        kind,
    }
}
#[test]
fn all_normalized_stackmap_forms_track_logical_entries_and_category_two_slots() {
    let mut b = Builder::new("(JD)V", &[0, 0, 0, 0, 0xb1], 2, 5);
    let name = b.pool.text("StackMapTable");
    let mut c = b.finish();
    with_frames(
        &mut c,
        name,
        vec![
            frame(
                0,
                K::Full {
                    locals: vec![V::Long, V::Double, V::Top],
                    stack: vec![],
                },
            ),
            frame(1, K::Same),
            frame(2, K::Chop(2)),
            frame(3, K::Append(vec![V::Integer])),
            frame(4, K::SameLocalsOneStack(V::Double)),
        ],
    );
    good(&c);
    code_mut(&mut c).max_locals = 4;
    assert!(matches!(bad(&c).kind, E::InvalidStackMap(_)));
    code_mut(&mut c).max_locals = 5;
    code_mut(&mut c).max_stack = 1;
    let e = bad(&c);
    assert_eq!(
        e.location,
        L::StackMapFrame {
            attribute: 0,
            frame: 4
        }
    );
    assert!(matches!(e.kind, E::InvalidStackMap(_)));
    // Initial locals contain two logical entries, not max_locals padded Tops.
    let mut b = Builder::new("(JD)V", &[0xb1], 0, 100);
    let name = b.pool.text("StackMapTable");
    let mut c = b.finish();
    with_frames(&mut c, name, vec![frame(0, K::Chop(3))]);
    assert!(matches!(bad(&c).kind, E::InvalidStackMap(_)));
}
#[test]
fn stackmap_offsets_shapes_and_explicit_types_reject_mutated_metadata() {
    for kind in [
        K::Append(vec![]),
        K::Append(vec![V::Top; 4]),
        K::Chop(0),
        K::Chop(4),
        K::SameLocalsOneStack(V::UninitializedThis),
    ] {
        let mut b = Builder::new("()V", &[0xb1], 10, 10);
        let name = b.pool.text("StackMapTable");
        let mut c = b.finish();
        with_frames(&mut c, name, vec![frame(0, kind)]);
        assert!(matches!(bad(&c).kind, E::InvalidStackMap(_)));
    }
    for (delta, stored) in [(0, 1), (1, 1), (65535, 65535)] {
        let mut b = Builder::new("()V", &[0x10, 1, 0xb1], 1, 0);
        let name = b.pool.text("StackMapTable");
        let mut c = b.finish();
        with_frames(
            &mut c,
            name,
            vec![StackMapFrame {
                offset_delta: delta,
                bytecode_offset: stored,
                kind: K::Same,
            }],
        );
        assert!(matches!(
            bad(&c).kind,
            E::InvalidBoundary { .. } | E::InvalidStackMap(_)
        ));
    }
    for offset in [0, 1, 3] {
        let mut b = Builder::new("()V", &[0xb1], 1, 0);
        let name = b.pool.text("StackMapTable");
        let class = b.pool.class("q/Unresolved");
        let mut c = b.finish();
        code_mut(&mut c).code = [op(0xbb, class), vec![0xb1]].concat();
        with_frames(
            &mut c,
            name,
            vec![frame(0, K::SameLocalsOneStack(V::Uninitialized(offset)))],
        );
        assert_eq!(validate(&c, 0, Options::default()).is_ok(), offset == 0);
    }
}
#[test]
fn normalized_extended_offsets_and_constructor_uninitialized_this_are_supported() {
    let mut b = Builder::new("()V", &[0xb1], 1, 1);
    let name = b.pool.text("StackMapTable");
    let init = b.pool.text("<init>");
    let array = b.pool.class(&format!("{}I", "[".repeat(255)));
    let mut c = b.finish();
    c.methods[0].name_index = init;
    c.methods[0].access_flags = 1;
    code_mut(&mut c).code = vec![0; 101];
    with_frames(
        &mut c,
        name,
        vec![StackMapFrame {
            offset_delta: 100,
            bytecode_offset: 100,
            kind: K::Full {
                locals: vec![V::UninitializedThis],
                stack: vec![V::Object(array)],
            },
        }],
    );
    good(&c);
}
#[test]
fn handler_bulk_funding_and_each_of_three_boundary_predicates_have_exact_charges() {
    let mut c = simple();
    code_mut(&mut c).exception_table.push(ExceptionHandler {
        start_pc: 0,
        end_pc: 1,
        handler_pc: 0,
        catch_type: 0,
    });
    // Baseline 16 + one row + start/end/handler predicates, including end==CodeLen.
    assert_eq!(
        good(&c).usage(),
        Usage {
            code_bytes: 1,
            metadata_entries: 2,
            utf16_units: 10,
            work_units: 20
        }
    );
    let e = limited(
        &c,
        Options {
            max_metadata_entries: 1,
            ..Options::default()
        },
    );
    assert_eq!(e.location, L::ExceptionHandler(0));
    assert_eq!(
        e.usage,
        Usage {
            code_bytes: 1,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 16
        }
    );
    assert_eq!(
        e.kind,
        E::ResourceLimit {
            resource: R::MetadataEntries,
            limit: 1,
            required: 2
        }
    );
    let mut options = Options {
        max_work_units: 19,
        ..Options::default()
    };
    assert_eq!(limited(&c, options).usage.work_units, 19);
    options.max_work_units = 20;
    assert!(validate(&c, 0, options).is_ok());
}
#[test]
fn recognized_attribute_bodies_are_checked_without_reinterpreting_opaque_extensions() {
    for standard in [false, true] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let sm = b.pool.text("StackMapTable");
        let mut c = b.finish();
        c.methods[0].attributes.push(Attribute {
            offset: 0,
            name_index: sm,
            body: AttributeBody::Opaque {
                bytes: vec![0xff; 100],
                standard,
            },
        });
        code_mut(&mut c).attributes.push(Attribute {
            offset: 0,
            name_index: 7,
            body: AttributeBody::Opaque {
                bytes: vec![0xff; 100],
                standard,
            },
        });
        good(&c); // StackMap at method level and Code inside Code are extensions.
        code_mut(&mut c).attributes.push(Attribute {
            offset: 0,
            name_index: sm,
            body: AttributeBody::Opaque {
                bytes: vec![0xff],
                standard,
            },
        });
        assert!(matches!(bad(&c).kind, E::InvalidMetadata(_)));
        c.version.major = 49;
        good(&c); // Pre50 Code-level StackMap name is also an opaque extension.
    }
    let mut b = Builder::new("()V", &[0xb1], 0, 0);
    let sm = b.pool.text("StackMapTable");
    let other = b.pool.text("ignored");
    let c = b.finish();
    for location in 0..4 {
        let mut c = c.clone();
        let attr = Attribute {
            offset: 0,
            name_index: if location == 3 { other } else { sm },
            body: AttributeBody::StackMapTable(vec![]),
        };
        match location {
            0 => c.methods[0].attributes.push(attr),
            1 => {
                c.version.major = 49;
                code_mut(&mut c).attributes.push(attr);
            }
            2 => {
                code_mut(&mut c).attributes.push(attr.clone());
                code_mut(&mut c).attributes.push(attr);
            }
            _ => code_mut(&mut c).attributes.push(attr),
        }
        assert!(matches!(bad(&c).kind, E::InvalidMetadata(_)));
    }
    let mut c = c;
    let nested = code(&c).clone();
    code_mut(&mut c).attributes.push(Attribute {
        offset: 0,
        name_index: 7,
        body: AttributeBody::Code(nested),
    });
    assert!(matches!(bad(&c).kind, E::InvalidMetadata(_)));
}
#[test]
fn ignored_annotation_trees_and_debug_content_do_not_claim_validation_or_add_tree_charges() {
    use crabspot_classfile::{Annotation, ElementValue, ElementValuePair};
    let mut b = Builder::new("()V", &[0xb1], 0, 0);
    let ann = b.pool.text("RuntimeVisibleAnnotations");
    let dbg = b.pool.text("SourceDebugExtension");
    let mut c = b.finish();
    c.methods[0].attributes.push(Attribute {
        offset: 0,
        name_index: ann,
        body: AttributeBody::RuntimeVisibleAnnotations(vec![]),
    });
    code_mut(&mut c).attributes.push(Attribute {
        offset: 0,
        name_index: dbg,
        body: AttributeBody::SourceDebugExtension(vec![]),
    });
    let usage = good(&c).usage();
    c.methods[0].attributes.last_mut().unwrap().body =
        AttributeBody::RuntimeVisibleAnnotations(vec![Annotation {
            type_index: 0,
            elements: vec![ElementValuePair {
                element_name_index: 0,
                value: ElementValue::Array(vec![
                    ElementValue::Class {
                        class_info_index: 0
                    };
                    10000
                ]),
            }],
        }]);
    code_mut(&mut c).attributes[0].body = AttributeBody::SourceDebugExtension(vec![0xff; 100000]);
    assert_eq!(good(&c).usage(), usage);
    c.methods[0].attributes.last_mut().unwrap().name_index = 0;
    assert!(matches!(bad(&c).kind, E::InvalidConstant { .. }));
}
#[test]
fn public_vector_wire_count_overflows_fail_without_traversing_nonexistent_wire_entries() {
    let mut b = Builder::new("()V", &[0xb1], 1, 1);
    let sm = b.pool.text("StackMapTable");
    let c = b.finish();
    for target in 0..5 {
        let mut c = c.clone();
        match target {
            0 => c.methods[0].attributes.resize(
                65536,
                Attribute {
                    offset: 0,
                    name_index: 0,
                    body: AttributeBody::Deprecated,
                },
            ),
            1 => code_mut(&mut c).attributes.resize(
                65536,
                Attribute {
                    offset: 0,
                    name_index: 0,
                    body: AttributeBody::Deprecated,
                },
            ),
            2 => code_mut(&mut c).exception_table.resize(
                65536,
                ExceptionHandler {
                    start_pc: 0,
                    end_pc: 1,
                    handler_pc: 0,
                    catch_type: 0,
                },
            ),
            3 => with_frames(&mut c, sm, vec![frame(0, K::Same); 65536]),
            _ => with_frames(
                &mut c,
                sm,
                vec![frame(
                    0,
                    K::Full {
                        locals: vec![V::Top; 65536],
                        stack: vec![],
                    },
                )],
            ),
        }
        assert!(
            matches!(bad(&c).kind, E::InvalidMetadata(_) | E::InvalidStackMap(_)),
            "target {target}"
        );
    }
}
#[test]
fn exact_stackmap_bulk_and_type_list_accounting_is_independent_of_retained_same_frame_widths() {
    let mut b = Builder::new("()V", &[0, 0, 0, 0, 0xb1], 2, 5);
    let sm = b.pool.text("StackMapTable");
    let mut c = b.finish();
    with_frames(
        &mut c,
        sm,
        vec![
            frame(
                0,
                K::Full {
                    locals: vec![V::Long, V::Double, V::Top],
                    stack: vec![],
                },
            ),
            frame(1, K::Same),
            frame(2, K::Chop(2)),
            frame(3, K::Append(vec![V::Integer])),
        ],
    );
    // Baseline Code5: W24/UTF10/Meta1. Header: W15/UTF13/Meta1.
    // Four frame rows + four offset queries + four explicit types: W12/Meta8.
    assert_eq!(
        good(&c).usage(),
        Usage {
            code_bytes: 5,
            metadata_entries: 10,
            utf16_units: 23,
            work_units: 51
        }
    );
    let full = good(&c).usage();
    for (resource, limit) in [
        (R::MetadataEntries, full.metadata_entries - 1),
        (R::Utf16Units, full.utf16_units - 1),
        (R::WorkUnits, full.work_units - 1),
    ] {
        let mut options = Options::default();
        match resource {
            R::MetadataEntries => options.max_metadata_entries = limit,
            R::Utf16Units => options.max_utf16_units = limit,
            R::WorkUnits => options.max_work_units = limit,
            _ => unreachable!(),
        }
        let e = limited(&c, options);
        assert!(matches!(e.kind,E::ResourceLimit{resource:r,..}if r==resource));
        assert_eq!(e, bad_with_options(&c, options));
    }
    // No Top padding or repeated scan of the 30k retained locals for each Same.
    let mut b = Builder::new("()V", &vec![0; 10001], 0, 30000);
    let sm = b.pool.text("StackMapTable");
    let mut c = b.finish();
    let mut frames = vec![frame(
        0,
        K::Full {
            locals: vec![V::Top; 30000],
            stack: vec![],
        },
    )];
    frames.extend((1..10001).map(|i| frame(i, K::Same)));
    with_frames(&mut c, sm, frames);
    let u = good(&c).usage();
    assert_eq!(u.metadata_entries, 40003); // two headers,10001 rows,30000 explicit types.
    assert!(u.work_units < 100000);
}
fn bad_with_options(c: &crabspot_classfile::ClassFile, options: Options) -> Error {
    limited(c, options)
}
#[test]
fn uninitialized_type_obtains_new_with_one_boundary_query_not_two() {
    let mut b = Builder::new("()V", &[0xb1], 1, 0);
    let sm = b.pool.text("StackMapTable");
    let target = b.pool.class("q/X");
    let mut c = b.finish();
    code_mut(&mut c).code = [op(0xbb, target), vec![0xb1]].concat();
    let baseline = good(&c).usage();
    with_frames(
        &mut c,
        sm,
        vec![frame(0, K::SameLocalsOneStack(V::Uninitialized(0)))],
    );
    let u = good(&c).usage();
    // Header 1+Utf8link1+13units, frame1+offsetquery1, type1+newquery1.
    assert_eq!(u.work_units - baseline.work_units, 19);
    assert_eq!(u.metadata_entries - baseline.metadata_entries, 3);
    assert_eq!(u.utf16_units - baseline.utf16_units, 13);
}
#[test]
fn utf16_summary_cache_keys_actual_utf8_index_and_role_not_outer_alias_or_text() {
    let mut usages = Vec::new();
    for shared in [true, false] {
        let mut b = Builder::new("()V", &[0xb1], 0, 0);
        let text = b.pool.text("q/Repeated");
        let first = b.pool.class_index(text);
        let second_text = if shared {
            text
        } else {
            b.pool.text("q/Repeated")
        };
        let second = b.pool.class_index(second_text);
        let mut c = b.finish();
        code_mut(&mut c).code = [op(0x13, first), op(0x13, second), vec![0xb1]].concat();
        usages.push(good(&c).usage());
    }
    assert_eq!(
        usages[0],
        Usage {
            code_bytes: 7,
            metadata_entries: 1,
            utf16_units: 20,
            work_units: 38
        }
    );
    assert_eq!(usages[1].utf16_units - usages[0].utf16_units, 10);
    assert_eq!(usages[1].work_units - usages[0].work_units, 10);
    let mut b = Builder::new("()V", &[0xb1], 0, 0);
    let owner = b.pool.class("q/X");
    let desc = b.pool.text("I");
    let nat = b.pool.nat_indices(b.name, desc); // selected method name UTF "run" reused as FIELD name.
    let field = b.pool.member_indices(9, owner, nat);
    let mut c = b.finish();
    code_mut(&mut c).code = [op(0xb2, field), vec![0xb1]].concat();
    assert_eq!(good(&c).usage().utf16_units, 17); // base10 + class3 + separate-role run3 + I1.
}
#[test]
fn repeated_long_invocation_descriptors_are_scanned_once_with_scalar_summary_reuse() {
    let descriptor = format!("()L{};", "x".repeat(65000));
    assert_eq!(descriptor.encode_utf16().count(), 65004);
    let mut b = Builder::new("()V", &[0xb1], 0, 0);
    let target = b.pool.member(10, "q/X", "m", &descriptor);
    let mut c = b.finish();
    code_mut(&mut c).code = [op(0xb8, target), vec![0xb1]].concat();
    let one = good(&c).usage();
    let mut bytes = Vec::new();
    for _ in 0..1000 {
        bytes.extend(op(0xb8, target));
    }
    bytes.push(0xb1);
    code_mut(&mut c).code = bytes;
    let before = c.clone();
    let many = good(&c).usage();
    assert_eq!(many.utf16_units, 65018);
    assert_eq!(one.utf16_units, many.utf16_units);
    // Each repeated call adds Code3 + instruction1 + six CP-link lookups.
    assert_eq!(many.work_units - one.work_units, 9990);
    let options = Options {
        max_utf16_units: 65018,
        ..Options::default()
    };
    assert!(validate(&c, 0, options).is_ok());
    let e = limited(
        &c,
        Options {
            max_utf16_units: 65017,
            ..options
        },
    );
    assert_eq!(e.location, L::Instruction(0));
    assert_eq!(e.usage.utf16_units, 14);
    assert_eq!(
        e.kind,
        E::ResourceLimit {
            resource: R::Utf16Units,
            limit: 65017,
            required: 65018
        }
    );
    assert_eq!(c, before);
}
#[test]
fn exact_bodyless_and_atomic_utf16_work_charges_do_not_fund_partial_scans() {
    let mut c = simple();
    c.methods[0].access_flags = 0x109;
    c.methods[0].attributes.clear();
    assert_eq!(
        good(&c).usage(),
        Usage {
            code_bytes: 0,
            metadata_entries: 0,
            utf16_units: 6,
            work_units: 8
        }
    );
    let c = simple();
    let prefix = Usage {
        code_bytes: 0,
        metadata_entries: 1,
        utf16_units: 6,
        work_units: 10,
    };
    let e = limited(
        &c,
        Options {
            max_utf16_units: 9,
            ..Options::default()
        },
    );
    assert_eq!(e.usage, prefix);
    assert_eq!(e.location, L::MethodAttribute(0));
    let e = limited(
        &c,
        Options {
            max_work_units: 13,
            ..Options::default()
        },
    );
    assert_eq!(e.usage, prefix); // Four-unit header scan updates neither UTF nor Work.
    assert_eq!(
        e.kind,
        E::ResourceLimit {
            resource: R::WorkUnits,
            limit: 13,
            required: 14
        }
    );
}
#[test]
fn lossless_utf16_names_and_unrelated_mutable_metadata_are_outside_whole_class_certification() {
    let mut b = Builder::new("()V", &[0xb1], 0, 0);
    let name = b.pool.units(&[0xd800, 0x78]);
    let class_name = b.pool.units(&[0x71, 0x2f, 0xdfff]);
    let class_index = b.pool.class_index(class_name);
    let mut c = b.finish();
    c.methods[0].name_index = name;
    code_mut(&mut c).code = [op(0xbb, class_index), vec![0xb1]].concat();
    good(&c); // UTF-16 units are preserved, not converted lossily to Unicode scalar values.
    let mut unrelated = c.methods[0].clone();
    unrelated.name_index = 0;
    unrelated.descriptor_index = 0;
    c.methods.push(unrelated);
    c.this_class = 0;
    c.super_class = 0xffff;
    c.attributes.push(Attribute {
        offset: 0,
        name_index: 0,
        body: AttributeBody::Deprecated,
    });
    good(&c); // Selected method service does not certify unused class/member metadata.
    let e = match validate(&c, 1, Options::default()) {
        Err(e) => e,
        Ok(_) => panic!(),
    };
    assert_eq!(e.method_index, 1);
    assert!(matches!(e.kind, E::InvalidConstant { index: 0, .. }));
}
#[test]
fn fixed_wrong_ldc_category_fails_before_followed_class_name_scan() {
    let mut b = Builder::new("()V", &[0xb1], 0, 0);
    let class = b.pool.class(&format!("q/{}", "x".repeat(60000)));
    let mut c = b.finish();
    code_mut(&mut c).code = [op(0x14, class), vec![0xb1]].concat();
    let e = bad(&c);
    assert!(matches!(e.kind,E::InvalidConstant{index,..} if index==class));
    assert_eq!(e.location, L::Instruction(0));
    assert_eq!(
        e.usage,
        Usage {
            code_bytes: 4,
            metadata_entries: 1,
            utf16_units: 10,
            work_units: 20
        }
    );
    // The malformed operand's long Class spelling consumes no UTF budget.
    assert_eq!(
        limited(
            &c,
            Options {
                max_utf16_units: 10,
                ..Options::default()
            }
        ),
        e
    );
}
#[test]
fn each_legal_append_chop_length_and_all_scalar_verification_types_have_width_checks() {
    for count in 1..=3 {
        for append in [false, true] {
            let mut b = Builder::new("(III)V", &[0xb1], 0, 6);
            let sm = b.pool.text("StackMapTable");
            let mut c = b.finish();
            with_frames(
                &mut c,
                sm,
                vec![frame(
                    0,
                    if append {
                        K::Append(vec![V::Top; usize::from(count)])
                    } else {
                        K::Chop(count)
                    },
                )],
            );
            good(&c);
        }
    }
    for (ty, width) in [
        (V::Top, 1),
        (V::Integer, 1),
        (V::Float, 1),
        (V::Null, 1),
        (V::Long, 2),
        (V::Double, 2),
    ] {
        let mut b = Builder::new("()V", &[0xb1], width, 0);
        let sm = b.pool.text("StackMapTable");
        let mut c = b.finish();
        with_frames(&mut c, sm, vec![frame(0, K::SameLocalsOneStack(ty))]);
        good(&c);
        code_mut(&mut c).max_stack = width - 1;
        assert!(matches!(bad(&c).kind, E::InvalidStackMap(_)));
    }
    let mut b = Builder::new("()V", &[0xb1], 1, 0);
    let sm = b.pool.text("StackMapTable");
    let mut c = b.finish();
    with_frames(
        &mut c,
        sm,
        vec![frame(0, K::SameLocalsOneStack(V::Object(0)))],
    );
    assert!(matches!(bad(&c).kind, E::InvalidConstant { index: 0, .. }));
}
#[test]
fn switches_are_decoded_whole_and_targets_exclude_code_end_or_embedded_operands() {
    for table in [false, true] {
        let mut bytes = vec![if table { 0xaa } else { 0xab }, 0, 0, 0];
        let words: Vec<i32> = if table {
            vec![0, 0, 1, 0, 0]
        } else {
            vec![0, 2, -1, 0, 3, 0]
        };
        for word in words {
            bytes.extend(word.to_be_bytes());
        }
        bytes.push(0xb1);
        let mut c = simple();
        code_mut(&mut c).code = bytes;
        good(&c); // No key stack value is required by this representation-only pass.
        let mut broken = c.clone();
        code_mut(&mut broken).code[4..8].copy_from_slice(&1i32.to_be_bytes());
        assert!(matches!(bad(&broken).kind, E::Decode(_))); // Padding is not an instruction start.
        let end = i32::try_from(code(&c).code.len()).unwrap();
        code_mut(&mut c).code[4..8].copy_from_slice(&end.to_be_bytes());
        assert!(matches!(bad(&c).kind, E::Decode(_)));
    }
}
#[test]
fn methodhandle_and_methodtype_slot_limits_use_their_own_receiver_policy() {
    for slots in [254, 255] {
        let ordinary = format!("({})V", "I".repeat(slots));
        for kind in [5, 6, 7, 8, 9, 0] {
            let mut b = Builder::new("()V", &[0xb1], 0, 0);
            let loadable = if kind == 0 {
                b.pool.method_type(&ordinary)
            } else {
                let target = b.pool.member(
                    if kind == 9 { 11 } else { 10 },
                    "q/C",
                    if kind == 8 { "<init>" } else { "m" },
                    &ordinary,
                );
                b.pool.handle(kind, target)
            };
            let mut c = b.finish();
            code_mut(&mut c).code = [op(0x13, loadable), vec![0xb1]].concat();
            if slots == 254 || matches!(kind, 0 | 6) {
                good(&c);
            } else {
                let e = bad(&c);
                assert_eq!(e.location, L::Instruction(0));
                assert!(
                    matches!(e.kind, E::InvalidDescriptor { .. } | E::InvalidOperand(_)),
                    "kind{kind} {e}"
                );
            }
        }
    }
}

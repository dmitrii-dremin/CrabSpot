//! Independent whole-modern-method proof tests; no interpreter or reference JVM is called here.
mod type_support;
use crabspot_classfile::{StackMapFrameKind as K, VerificationType as V};
use crabspot_runtime::{ClassId, HierarchyPlan};
use crabspot_verifier::{
    CodeValidationLocation as L, TypeVerificationError as Error, TypeVerificationErrorKind as E,
    TypeVerificationOptions as Options, TypeVerificationResource as R, VerificationBinding,
    VerificationCore, VerificationEnvironment as Environment,
    VerificationEnvironmentErrorKind as EE, VerificationEnvironmentOptions as EnvOptions,
    VerificationEnvironmentResource as ER, verify_method_types as verify,
};
use std::{cell::RefCell, collections::BTreeSet};
use type_support::*;
thread_local! {static OPCODE_COVERAGE:RefCell<Option<BTreeSet<(u8,bool)>>> = const {RefCell::new(None)};}
fn core(w: &World) -> VerificationCore {
    let [
        object,
        cloneable,
        serializable,
        throwable,
        string,
        class,
        method_type,
        method_handle,
    ] = w.core;
    VerificationCore {
        bootstrap_loader: w.loader,
        object,
        cloneable,
        serializable,
        throwable,
        string,
        class,
        method_type,
        method_handle,
    }
}
fn env<'a>(w: &'a World, plans: &'a [HierarchyPlan]) -> Environment<'a> {
    Environment::new(
        &w.registry,
        &plans.iter().collect::<Vec<_>>(),
        &[],
        core(w),
        EnvOptions::default(),
    )
    .unwrap()
}
fn check(
    w: &World,
    id: ClassId,
    index: usize,
) -> Result<crabspot_verifier::TypeVerificationUsage, Box<Error>> {
    let plans = w.plans();
    let env = env(w, &plans);
    let result = verify(&env, id, index, Options::default());
    if let Ok(r) = &result {
        if let Some(decoded) = r.code_validation().decoded() {
            OPCODE_COVERAGE.with(|seen| {
                if let Some(seen) = seen.borrow_mut().as_mut() {
                    for instruction in decoded.instructions() {
                        seen.insert((instruction.opcode, instruction.wide));
                    }
                }
            });
        }
    }
    result.map(|r| r.usage())
}
fn accepted(desc: &str, bytes: &[u8], stack: u16, locals: u16) {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run(desc, Body::new(bytes, stack, locals));
    let id = w.add(c);
    if let Err(e) = check(&w, id, 0) {
        panic!("{desc} {bytes:02x?}: {e}");
    }
}
fn rejected(desc: &str, bytes: &[u8], stack: u16, locals: u16) {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run(desc, Body::new(bytes, stack, locals));
    let id = w.add(c);
    let e = check(&w, id, 0).unwrap_err();
    assert!(
        matches!(e.kind, E::TypeRule { .. }),
        "{desc} {bytes:02x?}: {e}"
    );
}
#[test]
fn numeric_constants_arithmetic_conversions_comparisons_and_returns_cover_all_types() {
    for (prefix, opcodes, ret, width) in [
        (
            vec![3, 4],
            vec![0x60, 0x64, 0x68, 0x6c, 0x70, 0x7e, 0x80, 0x82],
            0xac,
            1,
        ),
        (
            vec![9, 10],
            vec![0x61, 0x65, 0x69, 0x6d, 0x71, 0x7f, 0x81, 0x83],
            0xad,
            2,
        ),
        (
            vec![0x0b, 0x0c],
            vec![0x62, 0x66, 0x6a, 0x6e, 0x72],
            0xae,
            1,
        ),
        (
            vec![0x0e, 0x0f],
            vec![0x63, 0x67, 0x6b, 0x6f, 0x73],
            0xaf,
            2,
        ),
    ] {
        for opcode in opcodes {
            let desc = match ret {
                0xac => "()I",
                0xad => "()J",
                0xae => "()F",
                _ => "()D",
            };
            accepted(
                desc,
                &[prefix.clone(), vec![opcode, ret]].concat(),
                width * 2,
                0,
            );
            rejected(desc, &[vec![1], vec![opcode, ret]].concat(), width * 2, 0);
        }
    }
    for (constant, neg, ret, desc, width) in [
        (3, 0x74, 0xac, "()I", 1),
        (9, 0x75, 0xad, "()J", 2),
        (0x0b, 0x76, 0xae, "()F", 1),
        (0x0e, 0x77, 0xaf, "()D", 2),
    ] {
        accepted(desc, &[constant, neg, ret], width, 0);
    }
    for (opcode, input, ret, desc) in [
        (0x85, 3, 0xad, "()J"),
        (0x86, 3, 0xae, "()F"),
        (0x87, 3, 0xaf, "()D"),
        (0x88, 9, 0xac, "()I"),
        (0x89, 9, 0xae, "()F"),
        (0x8a, 9, 0xaf, "()D"),
        (0x8b, 0x0b, 0xac, "()I"),
        (0x8c, 0x0b, 0xad, "()J"),
        (0x8d, 0x0b, 0xaf, "()D"),
        (0x8e, 0x0e, 0xac, "()I"),
        (0x8f, 0x0e, 0xad, "()J"),
        (0x90, 0x0e, 0xae, "()F"),
        (0x91, 3, 0xac, "()I"),
        (0x92, 3, 0xac, "()I"),
        (0x93, 3, 0xac, "()I"),
    ] {
        accepted(desc, &[input, opcode, ret], 2, 0);
        rejected(desc, &[1, opcode, ret], 2, 0);
    }
    for opcode in [0x78, 0x7a, 0x7c] {
        accepted("()I", &[3, 4, opcode, 0xac], 2, 0);
        rejected("()I", &[3, 9, opcode, 0xac], 3, 0);
    }
    for opcode in [0x79, 0x7b, 0x7d] {
        accepted("()J", &[9, 4, opcode, 0xad], 3, 0);
        rejected("()J", &[9, 9, opcode, 0xad], 4, 0);
    }
    for (op, constant, width) in [
        (0x94, 9, 4),
        (0x95, 0x0b, 2),
        (0x96, 0x0b, 2),
        (0x97, 0x0e, 4),
        (0x98, 0x0e, 4),
    ] {
        accepted("()I", &[constant, constant, op, 0xac], width, 0);
        rejected("()I", &[1, 1, op, 0xac], 2, 0);
    }
    for constant in [2, 3, 4, 5, 6, 7, 8] {
        accepted("()I", &[constant, 0xac], 1, 0);
    }
    accepted("()I", &[0x10, 0x80, 0xac], 1, 0);
    accepted("()I", &[0x11, 0x80, 0, 0xac], 1, 0);
    accepted("()F", &[0x0d, 0xae], 1, 0);
    accepted("()V", &[0, 0xb1], 0, 0);
}
#[test]
fn local_family_explicit_implicit_and_wide_forms_require_initialized_types() {
    for (desc, constant, load, store, base_load, base_store, ret, width) in [
        ("()I", 3, 0x15, 0x36, 0x1a, 0x3b, 0xac, 1),
        ("()J", 9, 0x16, 0x37, 0x1e, 0x3f, 0xad, 2),
        ("()F", 0x0b, 0x17, 0x38, 0x22, 0x43, 0xae, 1),
        ("()D", 0x0e, 0x18, 0x39, 0x26, 0x47, 0xaf, 2),
        ("()Ljava/lang/Object;", 1, 0x19, 0x3a, 0x2a, 0x4b, 0xb0, 1),
    ] {
        for index in 0..4 {
            accepted(
                desc,
                &[constant, base_store + index, base_load + index, ret],
                width,
                u16::from(index) + width,
            );
        }
        accepted(desc, &[constant, store, 7, load, 7, ret], width, 7 + width);
        accepted(
            desc,
            &[constant, 0xc4, store, 1, 0, 0xc4, load, 1, 0, ret],
            width,
            256 + width,
        );
        rejected(desc, &[load, 0, ret], width, width);
    }
    accepted(
        "()V",
        &[3, 0x3b, 0x84, 0, 0xff, 0xc4, 0x84, 0, 0, 0x7f, 0xff, 0xb1],
        1,
        1,
    );
    rejected("()V", &[1, 0x4b, 0x84, 0, 1, 0xb1], 1, 1);
}
#[test]
fn overlapping_category_two_local_stores_invalidate_both_possible_prior_heads() {
    for wide in [9, 0x0e] {
        let store = if wide == 9 { 0x3f } else { 0x47 };
        let load = if wide == 9 { 0x1e } else { 0x26 };
        let ret = if wide == 9 { 0xad } else { 0xaf };
        let desc = if wide == 9 { "()J" } else { "()D" };
        rejected(desc, &[wide, store, 3, 0x3c, load, ret], 2, 3);
        rejected(desc, &[wide, store, 3, 0x3b, load, ret], 2, 3);
        accepted("()I", &[wide, store, 3, 0x3c, 0x1b, 0xac], 2, 3);
    }
    // Store long at1 overwrites tails of long0 and head of long2.
    rejected("()J", &[9, 0x3f, 9, 0x41, 9, 0x40, 0x1e, 0xad], 2, 4);
    rejected("()J", &[9, 0x3f, 9, 0x41, 9, 0x40, 0x20, 0xad], 2, 4);
    accepted("()J", &[9, 0x3f, 9, 0x41, 9, 0x40, 0x1f, 0xad], 2, 4);
}
#[test]
fn every_stack_manipulation_category_form_and_invalid_split_is_independent() {
    for (prefix, opcode, discards) in [
        (vec![3], 0x57, vec![]),
        (vec![3, 4], 0x58, vec![]),
        (vec![9], 0x58, vec![]),
        (vec![3], 0x59, vec![0x58]),
        (vec![3, 4], 0x5a, vec![0x58, 0x57]),
        (vec![3, 4, 5], 0x5b, vec![0x58, 0x58]),
        (vec![9, 3], 0x5b, vec![0x57, 0x58, 0x57]),
        (vec![3, 4], 0x5c, vec![0x58, 0x58]),
        (vec![9], 0x5c, vec![0x58, 0x58]),
        (vec![3, 4, 5], 0x5d, vec![0x58, 0x58, 0x57]),
        (vec![3, 9], 0x5d, vec![0x58, 0x57, 0x58]),
        (vec![3, 4, 5, 6], 0x5e, vec![0x58, 0x58, 0x58]),
        (vec![9, 3, 4], 0x5e, vec![0x58, 0x58, 0x58]),
        (vec![3, 4, 9], 0x5e, vec![0x58, 0x58, 0x58]),
        (vec![9, 10], 0x5e, vec![0x58, 0x58, 0x58]),
        (vec![3, 4], 0x5f, vec![0x58]),
    ] {
        accepted(
            "()V",
            &[prefix, vec![opcode], discards, vec![0xb1]].concat(),
            6,
            0,
        );
    }
    for (prefix, opcode) in [
        (vec![9], 0x57),
        (vec![9], 0x59),
        (vec![3], 0x58),
        (vec![9, 3], 0x5a),
        (vec![3, 9], 0x5f),
        (vec![3], 0x5e),
    ] {
        rejected("()V", &[prefix, vec![opcode, 0xb1]].concat(), 6, 0);
    }
}
fn body_result(
    desc: &str,
    body: Body,
) -> Result<crabspot_verifier::TypeVerificationUsage, Box<Error>> {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run(desc, body);
    let id = w.add(c);
    check(&w, id, 0)
}
#[test]
fn conditional_branch_families_and_wide_goto_check_every_declared_target() {
    for (prefix, opcodes) in [
        (vec![3], (0x99..=0x9e).collect::<Vec<_>>()),
        (vec![3, 4], (0x9f..=0xa4).collect()),
        (vec![1, 1], vec![0xa5, 0xa6]),
        (vec![1], vec![0xc6, 0xc7]),
    ] {
        for opcode in opcodes {
            let target = u16::try_from(prefix.len() + 4).unwrap();
            let bytes = [prefix.clone(), vec![opcode, 0, 4, 0xb1, 0xb1]].concat();
            let body = Body::new(&bytes, 2, 0).maps(vec![(target, K::Same)]);
            body_result("()V", body).unwrap();
            let body = Body::new(&bytes, 2, 0);
            assert!(matches!(
                body_result("()V", body).unwrap_err().kind,
                E::TypeRule { .. }
            ));
        }
    }
    for bytes in [vec![0xa7, 0, 3, 0xb1], vec![0xc8, 0, 0, 0, 5, 0xb1]] {
        let target = u16::try_from(bytes.len() - 1).unwrap();
        body_result("()V", Body::new(&bytes, 0, 0).maps(vec![(target, K::Same)])).unwrap();
    }
    for (prefix, opcode) in [
        (vec![1], 0x99),
        (vec![3], 0xc6),
        (vec![3, 1], 0xa5),
        (vec![1, 1], 0x9f),
    ] {
        let target = u16::try_from(prefix.len() + 4).unwrap();
        let b = [prefix, vec![opcode, 0, 4, 0xb1, 0xb1]].concat();
        assert!(matches!(
            body_result("()V", Body::new(&b, 2, 0).maps(vec![(target, K::Same)]))
                .unwrap_err()
                .kind,
            E::TypeRule { .. }
        ));
    }
}
#[test]
fn switch_targets_including_duplicates_are_proved_from_the_post_key_state() {
    for table in [true, false] {
        let mut bytes = vec![3, if table { 0xaa } else { 0xab }, 0, 0];
        let target = if table { 24 } else { 28 };
        let words: Vec<i32> = if table {
            vec![target - 1, 0, 1, target - 1, target - 1]
        } else {
            vec![target - 1, 2, -1, target - 1, 4, target - 1]
        };
        for x in words {
            bytes.extend(x.to_be_bytes());
        }
        assert_eq!(bytes.len(), target as usize);
        bytes.push(0xb1);
        let usage = body_result(
            "()V",
            Body::new(&bytes, 1, 0).maps(vec![(target as u16, K::Same)]),
        )
        .unwrap();
        assert_eq!(usage.edge_checks, 3); // Three explicit targets; no incoming comparison after terminal switch.
    }
}
#[test]
fn missing_dead_frames_falloff_bad_joins_and_zero_target_policy_are_not_reachability_shortcuts() {
    accepted("()I", &[3, 4, 0xac], 2, 0); // A return discards lower operands.
    rejected("()I", &[9, 0xad], 2, 0);
    rejected("()V", &[3, 0xac], 1, 0);
    rejected("()V", &[0], 0, 0);
    assert!(matches!(
        body_result("()V", Body::new(&[0xb1, 0xb1], 0, 0))
            .unwrap_err()
            .kind,
        E::TypeRule { .. }
    ));
    body_result(
        "()V",
        Body::new(&[0xb1, 0xb1], 0, 0).maps(vec![(1, K::Same)]),
    )
    .unwrap();
    let mut w = World::new();
    let mut c = Class::ordinary("t/Dead");
    c.run(
        "()V",
        Body::new(&[0xb1, 0x1a, 0x57, 0xb1], 1, 1).maps(vec![(1, K::Same)]),
    );
    let id = w.add(c);
    crabspot_verifier::validate_method_code(w.registry.class(id).unwrap(), 0, Default::default())
        .unwrap();
    assert!(matches!(
        check(&w, id, 0).unwrap_err().kind,
        E::TypeRule { .. }
    ));
    body_result(
        "()V",
        Body::new(&[0xa7, 0, 0], 0, 0).maps(vec![(0, K::Same)]),
    )
    .unwrap();
    assert!(matches!(
        body_result("()V", Body::new(&[0xa7, 0, 0], 0, 0))
            .unwrap_err()
            .kind,
        E::TypeRule { .. }
    ));
    // Empty incoming stack cannot match a prescribed Int stack even on fallthrough.
    assert!(matches!(
        body_result(
            "()V",
            Body::new(&[0, 0x57, 0xb1], 1, 0).maps(vec![(1, K::SameLocalsOneStack(V::Integer))])
        )
        .unwrap_err()
        .kind,
        E::TypeRule { .. }
    ));
}
#[test]
fn every_array_load_store_kind_preserves_exact_primitive_and_reference_shape() {
    for (leaf, load, store, ret, val_load, width) in [
        ("I", 0x2e, 0x4f, 0xac, 0x1c, 1),
        ("J", 0x2f, 0x50, 0xad, 0x20, 2),
        ("F", 0x30, 0x51, 0xae, 0x24, 1),
        ("D", 0x31, 0x52, 0xaf, 0x28, 2),
        ("Ljava/lang/Object;", 0x32, 0x53, 0xb0, 0x2c, 1),
        ("B", 0x33, 0x54, 0xac, 0x1c, 1),
        ("Z", 0x33, 0x54, 0xac, 0x1c, 1),
        ("C", 0x34, 0x55, 0xac, 0x1c, 1),
        ("S", 0x35, 0x56, 0xac, 0x1c, 1),
    ] {
        let result = if matches!(leaf, "B" | "Z" | "C" | "S") {
            "I"
        } else {
            leaf
        };
        accepted(
            &format!("([{leaf}){result}"),
            &[0x2a, 3, load, ret],
            2.max(width),
            1,
        );
        accepted(
            &format!("([{leaf}I{result})V"),
            &[0x2a, 0x1b, val_load, store, 0xb1],
            2 + width,
            2 + width,
        );
        let wrong = if leaf == "D" { "I" } else { "D" };
        rejected(
            &format!("([{wrong}){result}"),
            &[0x2a, 3, load, ret],
            2.max(width),
            1,
        );
    }
    accepted(
        "([Ljava/lang/String;Ljava/lang/Object;)V",
        &[0x2a, 3, 0x2b, 0x53, 0xb1],
        3,
        2,
    );
    accepted("()Ljava/lang/Object;", &[1, 3, 0x32, 0xb0], 2, 0);
    accepted("()I", &[1, 0xbe, 0xac], 1, 0);
    rejected("(Ljava/lang/Object;)I", &[0x2a, 0xbe, 0xac], 1, 1);
}
#[test]
fn allocation_cast_instanceof_throw_and_monitors_distinguish_generic_and_ordinary_references() {
    for (atype, leaf) in [
        (4, "Z"),
        (5, "C"),
        (6, "F"),
        (7, "D"),
        (8, "B"),
        (9, "S"),
        (10, "I"),
        (11, "J"),
    ] {
        accepted(&format!("()[{leaf}"), &[3, 0xbc, atype, 0xb0], 1, 0);
    }
    accepted("()V", &[1, 0xc2, 1, 0xc3, 0xb1], 1, 0);
    accepted("()V", &[1, 0xbf], 1, 0);
    accepted("(Ljava/lang/Throwable;)V", &[0x2a, 0xbf], 1, 1);
    rejected("(Ljava/lang/Object;)V", &[0x2a, 0xbf], 1, 1);
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    let owner = c.pool.class("t/C");
    let arr = c.pool.class("[[I");
    let unknown = c.pool.class("not/Loaded");
    c.run(
        "()V",
        Body::new(
            &[op(0xbb, owner), vec![0x59, 0xc2, 0xc3, 0xb1]].concat(),
            2,
            0,
        ),
    );
    c.method(
        "casts",
        "()I",
        9,
        Some(Body::new(
            &[vec![1], op(0xc0, unknown), op(0xc1, unknown), vec![0xac]].concat(),
            1,
            0,
        )),
    );
    c.method(
        "arrays",
        "()[[I",
        9,
        Some(Body::new(
            &[vec![3, 4, 0xc5], arr.to_be_bytes().to_vec(), vec![2, 0xb0]].concat(),
            2,
            0,
        )),
    );
    c.method(
        "anew",
        "()[Lt/C;",
        9,
        Some(Body::new(
            &[vec![3], op(0xbd, owner), vec![0xb0]].concat(),
            1,
            0,
        )),
    );
    let id = w.add(c);
    for index in 0..4 {
        check(&w, id, index).unwrap();
    }
}
#[test]
fn all_loadable_constant_types_use_computational_or_designated_core_types_without_linking() {
    for kind in 0..10 {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let (index, desc, ret, width) = match kind {
            0 => (c.pool.add(3, &1i32.to_be_bytes()), "()I", 0xac, 1),
            1 => (c.pool.add(4, &1f32.to_bits().to_be_bytes()), "()F", 0xae, 1),
            2 => (c.pool.add(5, &1i64.to_be_bytes()), "()J", 0xad, 2),
            3 => (c.pool.add(6, &1f64.to_bits().to_be_bytes()), "()D", 0xaf, 2),
            4 => (c.pool.string("literal"), "()Ljava/lang/String;", 0xb0, 1),
            5 => (c.pool.class("not/Loaded"), "()Ljava/lang/Class;", 0xb0, 1),
            6 => (
                c.pool.method_type("()V"),
                "()Ljava/lang/invoke/MethodType;",
                0xb0,
                1,
            ),
            7 => {
                let target = c.pool.member(10, "absent/C", "m", "()V");
                (
                    c.pool.handle(6, target),
                    "()Ljava/lang/invoke/MethodHandle;",
                    0xb0,
                    1,
                )
            }
            8 => (c.pool.dynamic(17, "value", "J"), "()J", 0xad, 2),
            _ => (
                c.pool.dynamic(17, "value", "Ljava/lang/String;"),
                "()Ljava/lang/String;",
                0xb0,
                1,
            ),
        };
        c.run(
            desc,
            Body::new(
                &[op(if width == 2 { 0x14 } else { 0x13 }, index), vec![ret]].concat(),
                width,
                0,
            ),
        );
        let id = w.add(c);
        check(&w, id, 0).unwrap();
    }
    // Both one-byte and two-byte indices are valid independent encoding paths.
    let mut w = World::new();
    let mut c = Class::ordinary("t/ShortLdc");
    let i = c.pool.add(3, &1i32.to_be_bytes());
    c.run(
        "()I",
        Body::new(&[0x12, u8::try_from(i).unwrap(), 0xac], 1, 0),
    );
    let id = w.add(c);
    check(&w, id, 0).unwrap();
}
#[test]
fn all_field_opcodes_transfer_descriptor_types_without_requiring_runtime_staticness_or_final_writes()
 {
    for (desc, const_op, ret, width) in [
        ("I", 3, 0xac, 1),
        ("J", 9, 0xad, 2),
        ("F", 0x0b, 0xae, 1),
        ("D", 0x0e, 0xaf, 2),
        ("Ljava/lang/Object;", 1, 0xb0, 1),
    ] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        c.field("value", desc, 0x11);
        let field = c.pool.member(9, "t/C", "value", desc);
        c.run(
            &format!("(){desc}"),
            Body::new(&[op(0xb2, field), vec![ret]].concat(), width, 0),
        );
        c.method(
            "putstatic",
            "()V",
            9,
            Some(Body::new(
                &[vec![const_op], op(0xb3, field), vec![0xb1]].concat(),
                width,
                0,
            )),
        );
        c.method(
            "getfield",
            &format!("(Lt/C;){desc}"),
            9,
            Some(Body::new(
                &[vec![0x2a], op(0xb4, field), vec![ret]].concat(),
                width,
                1,
            )),
        );
        c.method(
            "putfield",
            "(Lt/C;)V",
            9,
            Some(Body::new(
                &[vec![0x2a, const_op], op(0xb5, field), vec![0xb1]].concat(),
                1 + width,
                1,
            )),
        );
        let id = w.add(c);
        for i in 0..4 {
            check(&w, id, i).unwrap();
        }
    }
}
#[test]
fn every_invocation_family_checks_mixed_argument_types_and_declared_results() {
    for opcode in [0xb6, 0xb7, 0xb8, 0xb9, 0xba] {
        let mut w = World::new();
        let mut intf = Class::ordinary("t/I");
        intf.flags = 0x601;
        intf.method(
            "m",
            "(IJDFLjava/lang/Object;)Ljava/lang/Object;",
            0x401,
            None,
        );
        w.add(intf);
        let mut c = Class::ordinary("t/C");
        c.interface("t/I");
        let target_desc = "(IJDFLjava/lang/Object;)Ljava/lang/Object;";
        let target = if opcode == 0xba {
            c.pool.dynamic(18, "m", target_desc)
        } else {
            c.pool.member(
                if opcode == 0xb9 { 11 } else { 10 },
                if opcode == 0xb9 { "t/I" } else { "t/C" },
                "m",
                target_desc,
            )
        };
        c.method(
            "m",
            target_desc,
            if opcode == 0xb8 { 0x109 } else { 0x101 },
            None,
        );
        let invocation = match opcode {
            0xb9 => [op(opcode, target), vec![8, 0]].concat(),
            0xba => [op(opcode, target), vec![0, 0]].concat(),
            _ => op(opcode, target),
        };
        let mut prefix = if matches!(opcode, 0xb8 | 0xba) {
            vec![]
        } else {
            vec![0x2a]
        };
        prefix.extend([0x1b, 0x20, 0x18, 4, 0x17, 6, 0x19, 7]);
        c.run(
            "(Lt/C;IJDFLjava/lang/Object;)Ljava/lang/Object;",
            Body::new(
                &[prefix.clone(), invocation.clone(), vec![0xb0]].concat(),
                8,
                8,
            ),
        );
        let mut wrong = prefix;
        let argument = if matches!(opcode, 0xb8 | 0xba) { 0 } else { 1 };
        wrong[argument] = 0x0b;
        c.method(
            "bad",
            "(Lt/C;IJDFLjava/lang/Object;)Ljava/lang/Object;",
            9,
            Some(Body::new(&[wrong, invocation, vec![0xb0]].concat(), 8, 8)),
        );
        let id = w.add(c);
        check(&w, id, 1).unwrap();
        assert!(matches!(
            check(&w, id, 2).unwrap_err().kind,
            E::TypeRule { .. }
        ));
    }
}
#[test]
fn constructor_base_chain_aliases_and_early_exact_own_fields_have_distinct_rules() {
    let mut w = World::new();
    check(&w, w.core[0], 0).unwrap();
    let mut c = Class::ordinary("t/C");
    c.field("x", "I", 0x11);
    let field = c.pool.member(9, "t/C", "x", "I");
    let super_init = c.pool.member(10, CORE_NAMES[0], "<init>", "()V");
    c.method(
        "<init>",
        "()V",
        1,
        Some(Body::new(
            &[
                vec![0x2a, 4],
                op(0xb5, field),
                vec![0x2a, 0x59, 0x4c],
                op(0xb7, super_init),
                vec![0x2b],
                op(0xb4, field),
                vec![0x57, 0xb1],
            ]
            .concat(),
            2,
            2,
        )),
    );
    let this_init = c.pool.member(10, "t/C", "<init>", "()V");
    c.method(
        "<init>",
        "(I)V",
        1,
        Some(Body::new(
            &[vec![0x2a], op(0xb7, this_init), vec![0xb1]].concat(),
            1,
            2,
        )),
    );
    let id = w.add(c);
    check(&w, id, 0).unwrap();
    check(&w, id, 1).unwrap();
    for invalid in 0..4 {
        let mut w = World::new();
        let mut parent = Class::ordinary("t/P");
        parent.field("x", "I", 1);
        w.add(parent);
        let mut c = Class::new("t/C", Some("t/P"));
        let init = c.pool.member(10, "t/P", "<init>", "()V");
        let field = c
            .pool
            .member(9, if invalid == 0 { "t/C" } else { "t/P" }, "x", "I");
        let bytes = match invalid {
            0 | 1 => [
                vec![0x2a, 3],
                op(0xb5, field),
                vec![0x2a],
                op(0xb7, init),
                vec![0xb1],
            ]
            .concat(),
            2 => vec![3, 0x3b, 0xb1],
            _ => vec![0xb1],
        };
        c.method("<init>", "()V", 1, Some(Body::new(&bytes, 2, 1)));
        let id = w.add(c);
        assert!(matches!(
            check(&w, id, 0).unwrap_err().kind,
            E::TypeRule { .. }
        ));
    }
}
#[test]
fn unfinished_references_allow_generic_branches_monitors_and_discard_but_not_ordinary_escape() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    let class = c.pool.class("t/C");
    let init = c.pool.member(10, "t/C", "<init>", "()V");
    let super_init = c.pool.member(10, CORE_NAMES[0], "<init>", "()V");
    c.method(
        "<init>",
        "()V",
        1,
        Some(Body::new(
            &[vec![0x2a], op(0xb7, super_init), vec![0xb1]].concat(),
            1,
            1,
        )),
    );
    c.method(
        "complete",
        "()Lt/C;",
        9,
        Some(Body::new(
            &[op(0xbb, class), vec![0x59], op(0xb7, init), vec![0xb0]].concat(),
            2,
            0,
        )),
    );
    c.method(
        "discard",
        "()V",
        9,
        Some(Body::new(
            &[op(0xbb, class), vec![0x57, 0xb1]].concat(),
            1,
            0,
        )),
    );
    c.method(
        "monitor",
        "()V",
        9,
        Some(Body::new(
            &[op(0xbb, class), vec![0x59, 0xc2, 0xc3, 0xb1]].concat(),
            2,
            0,
        )),
    );
    for opcode in [0xc6, 0xc7] {
        let bytes = [op(0xbb, class), vec![opcode, 0, 4, 0xb1, 0xb1]].concat();
        c.method(
            &format!("branch{opcode}"),
            "()V",
            9,
            Some(Body::new(&bytes, 1, 0).maps(vec![(7, K::Same)])),
        );
    }
    for opcode in [0xa5, 0xa6] {
        let bytes = [op(0xbb, class), vec![1, opcode, 0, 4, 0xb1, 0xb1]].concat();
        c.method(
            &format!("identity{opcode}"),
            "()V",
            9,
            Some(Body::new(&bytes, 2, 0).maps(vec![(8, K::Same)])),
        );
    }
    let count = 8;
    let id = w.add(c);
    for i in 0..count {
        check(&w, id, i).unwrap();
    }
    for opcode in [0xb0, 0xbf, 0xc0, 0xc1] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let class = c.pool.class("t/C");
        let bytes = if matches!(opcode, 0xc0 | 0xc1) {
            [op(0xbb, class), op(opcode, class), vec![0x57, 0xb1]].concat()
        } else {
            [op(0xbb, class), vec![opcode]].concat()
        };
        c.run(
            if opcode == 0xb0 { "()Lt/C;" } else { "()V" },
            Body::new(&bytes, 1, 0),
        );
        let id = w.add(c);
        assert!(matches!(
            check(&w, id, 0).unwrap_err().kind,
            E::TypeRule { .. }
        ));
    }
}
#[test]
fn same_site_unfinished_locals_are_invalidated_while_live_stack_aliases_are_rejected() {
    for location in 0..3 {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let class = c.pool.class("t/C");
        let tail = if location == 0 {
            vec![0x4b, 0xb1]
        } else {
            vec![0x57, 0x2a, 0x57, 0xb1]
        };
        let bytes = [vec![0xb1], op(0xbb, class), tail].concat();
        let map = if location == 2 {
            K::Full {
                locals: vec![],
                stack: vec![V::Uninitialized(1)],
            }
        } else {
            K::Full {
                locals: vec![V::Uninitialized(1)],
                stack: vec![],
            }
        };
        c.run("()V", Body::new(&bytes, 2, 1).maps(vec![(1, map)]));
        let id = w.add(c);
        let result = check(&w, id, 0);
        assert_eq!(
            result.is_ok(),
            location == 0,
            "location{location} {result:?}"
        );
    }
}
#[test]
fn constructor_allocation_handlers_must_accept_both_unfinished_and_initialized_local_views() {
    for variant in 0..4 {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let class = c.pool.class("t/C");
        let init = c.pool.member(10, "t/C", "<init>", "()V");
        let super_init = c.pool.member(10, CORE_NAMES[0], "<init>", "()V");
        c.method(
            "<init>",
            "()V",
            1,
            Some(Body::new(
                &[vec![0x2a], op(0xb7, super_init), vec![0xb1]].concat(),
                1,
                1,
            )),
        );
        let retry = if variant == 3 {
            [vec![0x4c, 0x2a], op(0xb7, init), vec![0xb1]].concat()
        } else {
            vec![0x4c, 0xb1]
        };
        let bytes = [
            op(0xbb, class),
            vec![0x59, 0x4b],
            op(0xb7, init),
            vec![0xb1],
            retry,
        ]
        .concat();
        let mut body = Body::new(&bytes, 2, 2).maps(vec![(
            9,
            K::Full {
                locals: vec![match variant {
                    1 => V::Uninitialized(0),
                    2 => V::Object(class),
                    _ => V::Top,
                }],
                stack: vec![V::Object(c.pool.class(CORE_NAMES[3]))],
            },
        )]);
        body.handlers.push([5, 8, 9, 0]);
        c.run("()V", body);
        let id = w.add(c);
        let result = check(&w, id, 1);
        assert_eq!(result.is_ok(), variant == 0, "variant{variant} {result:?}");
    }
}
#[test]
fn failed_this_super_handler_maps_cannot_erase_the_live_constructor_flag() {
    for variant in 0..3 {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let class = c.pool.class("t/C");
        let throwable = c.pool.class(CORE_NAMES[3]);
        let init = c.pool.member(10, CORE_NAMES[0], "<init>", "()V");
        let mut b = Body::new(
            &[vec![0x2a], op(0xb7, init), vec![0xb1, 0xbf]].concat(),
            1,
            1,
        )
        .maps(vec![(
            5,
            K::Full {
                locals: vec![match variant {
                    0 => V::Top,
                    1 => V::UninitializedThis,
                    _ => V::Object(class),
                }],
                stack: vec![V::Object(throwable)],
            },
        )]);
        b.handlers.push([1, 4, 5, 0]);
        c.method("<init>", "()V", 1, Some(b));
        let id = w.add(c);
        assert!(matches!(
            check(&w, id, 0).unwrap_err().kind,
            E::TypeRule { .. }
        ));
    }
}
#[test]
fn caught_ordinary_helper_and_failed_separate_allocation_before_chain_preserve_pending_this() {
    for allocation in [false, true] {
        let mut w = World::new();
        let mut other = Class::ordinary("t/Other");
        let object = other.pool.member(10, CORE_NAMES[0], "<init>", "()V");
        other.method(
            "<init>",
            "()V",
            1,
            Some(Body::new(
                &[vec![0x2a], op(0xb7, object), vec![0xb1]].concat(),
                1,
                1,
            )),
        );
        w.add(other);
        let mut c = Class::ordinary("t/C");
        let throwable = c.pool.class(CORE_NAMES[3]);
        let super_init = c.pool.member(10, CORE_NAMES[0], "<init>", "()V");
        let body = if allocation {
            let other = c.pool.class("t/Other");
            let init = c.pool.member(10, "t/Other", "<init>", "()V");
            let bytes = [
                op(0xbb, other),
                vec![0x59, 0x4c],
                op(0xb7, init),
                vec![0xa7, 0, 5, 0x4d, 0, 0x2a],
                op(0xb7, super_init),
                vec![0xb1],
            ]
            .concat();
            let mut b = Body::new(&bytes, 2, 3).maps(vec![
                (
                    11,
                    K::Full {
                        locals: vec![V::UninitializedThis, V::Top],
                        stack: vec![V::Object(throwable)],
                    },
                ),
                (
                    13,
                    K::Full {
                        locals: vec![V::UninitializedThis],
                        stack: vec![],
                    },
                ),
            ]);
            b.handlers.push([5, 8, 11, 0]);
            b
        } else {
            let helper = c.pool.member(10, "t/C", "helper", "()V");
            let bytes = [
                op(0xb8, helper),
                vec![0xa7, 0, 5, 0x4c, 0, 0x2a],
                op(0xb7, super_init),
                vec![0xb1],
            ]
            .concat();
            let mut b = Body::new(&bytes, 1, 2).maps(vec![
                (
                    6,
                    K::Full {
                        locals: vec![V::UninitializedThis],
                        stack: vec![V::Object(throwable)],
                    },
                ),
                (
                    8,
                    K::Full {
                        locals: vec![V::UninitializedThis],
                        stack: vec![],
                    },
                ),
            ]);
            b.handlers.push([0, 3, 6, 0]);
            b
        };
        c.method("<init>", "()V", 1, Some(body));
        c.method("helper", "()V", 0x109, None);
        let id = w.add(c);
        check(&w, id, 0).unwrap();
    }
}
#[test]
fn canonical_borrowed_result_bodyless_guarantees_and_old_profile_are_independent() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run("()V", Body::new(&[0xb1], 0, 0));
    c.method("native", "()V", 0x109, None);
    c.method("abstract", "()V", 0x401, None);
    c.flags |= 0x400;
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    for index in 0..3 {
        let r = verify(&env, id, index, Options::default()).unwrap();
        assert!(std::ptr::eq(r.environment(), &env));
        assert_eq!(r.class_id(), id);
        assert_eq!(r.method_index(), index);
        assert!(std::ptr::eq(
            r.code_validation().class(),
            w.registry.class(id).unwrap()
        ));
        assert_eq!(r.code_validation().code().is_none(), index != 0);
        assert_eq!(
            r.usage(),
            verify(&env, id, index, Options::default()).unwrap().usage()
        );
    }
}
#[test]
fn environment_duplicate_physical_rows_pay_before_deduplication_but_seed_bindings_once() {
    let w = World::new();
    let plans = w.plans();
    let first = env(&w, &plans);
    let mut refs = plans.iter().collect::<Vec<_>>();
    refs.push(&plans[4]);
    let duplicated =
        Environment::new(&w.registry, &refs, &[], core(&w), EnvOptions::default()).unwrap();
    let a = first.usage();
    let b = duplicated.usage();
    assert_eq!(b.class_rows - a.class_rows, 2);
    assert_eq!(b.edges - a.edges, 1);
    assert_eq!(b.bindings, a.bindings);
    assert_eq!(b.utf16_units, a.utf16_units);
    // Header1 + rows2 + edge1 + two shape comparisons (including absent root parent).
    assert_eq!(b.work_units - a.work_units, 6);
    let binding = VerificationBinding {
        initiating_loader: w.loader,
        definition: w.core[4],
    };
    let duplicate = Environment::new(
        &w.registry,
        &plans.iter().collect::<Vec<_>>(),
        &[binding],
        core(&w),
        EnvOptions::default(),
    )
    .unwrap()
    .usage();
    assert_eq!(duplicate.bindings - a.bindings, 1);
    assert_eq!(duplicate.utf16_units - a.utf16_units, 16);
    assert_eq!(duplicate.work_units - a.work_units, 17);
}
#[test]
fn environment_rejects_foreign_facts_core_lookalikes_and_loader_name_conflicts_atomically() {
    let mut w = World::new();
    let foreign = World::new();
    let plans = w.plans();
    let bad = VerificationBinding {
        initiating_loader: w.loader,
        definition: foreign.core[0],
    };
    let e = Environment::new(
        &w.registry,
        &plans.iter().collect::<Vec<_>>(),
        &[bad],
        core(&w),
        EnvOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(e.kind, EE::Registry(_)));
    let local = w.add(Class::ordinary("q/C"));
    let second = w.registry.create_loader().unwrap();
    let delegated = w.add_in(second, Class::ordinary("q/C"));
    let plans = w.plans_from(&[delegated]);
    let e = Environment::new(
        &w.registry,
        &plans.iter().collect::<Vec<_>>(),
        &[VerificationBinding {
            initiating_loader: w.loader,
            definition: delegated,
        }],
        core(&w),
        EnvOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(e.kind,EE::BindingConflict{existing,incoming,..}if existing==local&&incoming==delegated)
    );
    let fake = w.add_in(second, Class::ordinary(CORE_NAMES[4]));
    let plans = w.plans();
    let mut wrong = core(&w);
    wrong.string = fake;
    assert!(matches!(
        Environment::new(
            &w.registry,
            &plans.iter().collect::<Vec<_>>(),
            &[],
            wrong,
            EnvOptions::default()
        )
        .unwrap_err()
        .kind,
        EE::InvalidCore { .. }
    ));
    // All prior immutable facts remain usable after independent construction failures.
    env(&w, &plans);
}
#[test]
fn two_environments_with_different_delegations_do_not_share_proof_authority() {
    let mut w = World::new();
    let second = w.registry.create_loader().unwrap();
    let mut a = Class::ordinary("q/A");
    a.flags = 0x601;
    let iface = w.add(a);
    let other = w.add_in(second, Class::ordinary("q/A"));
    let third = w.registry.create_loader().unwrap();
    let mut c = Class::ordinary("p/C");
    c.run("(Ljava/lang/Object;)Lq/A;", Body::new(&[0x2a, 0xb0], 1, 1));
    let selected = w.add_in(third, c);
    let plans = w.plans();
    let refs = plans.iter().collect::<Vec<_>>();
    let object = VerificationBinding {
        initiating_loader: third,
        definition: w.core[0],
    };
    let first = Environment::new(
        &w.registry,
        &refs,
        &[
            object,
            VerificationBinding {
                initiating_loader: third,
                definition: iface,
            },
        ],
        core(&w),
        EnvOptions::default(),
    )
    .unwrap();
    let second = Environment::new(
        &w.registry,
        &refs,
        &[
            object,
            VerificationBinding {
                initiating_loader: third,
                definition: other,
            },
        ],
        core(&w),
        EnvOptions::default(),
    )
    .unwrap();
    verify(&first, selected, 0, Options::default()).unwrap();
    assert!(matches!(
        verify(&second, selected, 0, Options::default())
            .unwrap_err()
            .kind,
        E::TypeRule { .. }
    ));
}
#[test]
fn necessary_facts_fail_structurally_while_identical_symbols_and_null_need_no_binding() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run("(Lmissing/T;)Lmissing/T;", Body::new(&[0x2a, 0xb0], 1, 1));
    c.method(
        "null",
        "()Lmissing/T;",
        9,
        Some(Body::new(&[1, 0xb0], 1, 0)),
    );
    c.method(
        "need",
        "(Ljava/lang/Object;)Lmissing/T;",
        9,
        Some(Body::new(&[0x2a, 0xb0], 1, 1)),
    );
    let id = w.add(c);
    check(&w, id, 0).unwrap();
    check(&w, id, 1).unwrap();
    let e = check(&w, id, 2).unwrap_err();
    assert!(matches!(e.kind, E::UnavailableFact { .. }));
    assert_eq!(e.location, L::Instruction(1));
}
#[test]
fn primitive_arrays_do_not_gain_arbitrary_interface_relaxation_but_nominal_classes_do() {
    let mut w = World::new();
    let mut i = Class::ordinary("q/I");
    i.flags = 0x601;
    w.add(i);
    let mut c = Class::ordinary("t/C");
    c.run("(Ljava/lang/Object;)Lq/I;", Body::new(&[0x2a, 0xb0], 1, 1));
    c.method("bad", "([I)Lq/I;", 9, Some(Body::new(&[0x2a, 0xb0], 1, 1)));
    c.method(
        "arrays",
        "([Ljava/lang/Object;)[Lq/I;",
        9,
        Some(Body::new(&[0x2a, 0xb0], 1, 1)),
    );
    let id = w.add(c);
    check(&w, id, 0).unwrap();
    assert!(matches!(
        check(&w, id, 1).unwrap_err().kind,
        E::TypeRule { .. }
    ));
    check(&w, id, 2).unwrap();
}
#[test]
fn handler_checks_use_input_locals_even_for_nonthrowing_store_instructions() {
    for post_state in [false, true] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let throwable = c.pool.class(CORE_NAMES[3]);
        let mut b = Body::new(&[1, 0x4b, 0xb1, 0x4c, 0xb1], 1, 2).maps(vec![(
            3,
            K::Full {
                locals: vec![if post_state { V::Null } else { V::Integer }],
                stack: vec![V::Object(throwable)],
            },
        )]);
        b.handlers.push([1, 2, 3, 0]);
        c.run("(I)V", b);
        let id = w.add(c);
        let r = check(&w, id, 0);
        assert_eq!(r.is_ok(), !post_state, "{r:?}");
        if let Ok(u) = r {
            assert_eq!(u.edge_checks, 7);
        }
    }
}
#[test]
fn initial_handler_legality_precedes_instruction_errors_and_accepts_reference_supertypes() {
    for invalid in [false, true] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let object = c.pool.class(CORE_NAMES[0]);
        let mut b = Body::new(&[0x57, 0xb1, 0x57, 0xb1], 1, 0).maps(vec![(
            2,
            K::SameLocalsOneStack(if invalid {
                V::Integer
            } else {
                V::Object(object)
            }),
        )]);
        b.handlers.push([0, 1, 2, 0]);
        c.run("()V", b);
        let id = w.add(c);
        let e = check(&w, id, 0).unwrap_err();
        assert!(matches!(e.kind, E::TypeRule { .. }));
        assert_eq!(
            e.location,
            if invalid {
                L::ExceptionHandler(0)
            } else {
                L::Instruction(0)
            }
        );
    }
}
#[test]
fn constructor_generic_monitor_and_branch_rules_do_not_prematurely_promote_this() {
    for monitors in [false, true] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let init = c.pool.member(10, CORE_NAMES[0], "<init>", "()V");
        let b = if monitors {
            Body::new(
                &[
                    vec![0x2a, 0x59, 0xc2, 0xc3, 0x2a],
                    op(0xb7, init),
                    vec![0xb1],
                ]
                .concat(),
                2,
                1,
            )
        } else {
            Body::new(
                &[vec![0x2a, 0xc6, 0, 4, 0, 0x2a], op(0xb7, init), vec![0xb1]].concat(),
                1,
                1,
            )
            .maps(vec![(5, K::Same)])
        };
        c.method("<init>", "()V", 1, Some(b));
        let id = w.add(c);
        check(&w, id, 0).unwrap();
    }
}
#[test]
fn frame_expansion_preserves_encoded_entries_and_implicit_wide_continuations() {
    let mut c = Class::ordinary("t/C");
    c.run(
        "(J)V",
        Body::new(&[0, 0, 0, 0xb1], 0, 3).maps(vec![
            (
                0,
                K::Full {
                    locals: vec![V::Long, V::Top],
                    stack: vec![],
                },
            ),
            (1, K::Chop(1)),
            (2, K::Append(vec![V::Top])),
        ]),
    );
    let mut w = World::new();
    let id = w.add(c);
    check(&w, id, 0).unwrap();
    // Same map sharing cannot turn an explicit Top into a usable Long tail value.
    let b = Body::new(&[0, 0x1c, 0xac], 1, 3).maps(vec![(
        0,
        K::Full {
            locals: vec![V::Long, V::Top],
            stack: vec![],
        },
    )]);
    assert!(matches!(
        body_result("(J)I", b).unwrap_err().kind,
        E::TypeRule { .. }
    ));
}
#[test]
fn modern_history_and_representation_phase_order_preserve_partial_diagnostics() {
    for major in [45, 50, 51, 65] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        c.major = major;
        c.run("()V", Body::new(&[0xb1], 0, 0));
        let id = w.add(c);
        let result = check(&w, id, 0);
        if major < 51 {
            let e = result.unwrap_err();
            assert!(matches!(e.kind,E::UnsupportedHistory{major:m}if m==major));
            assert_eq!(e.usage.work_units, 0);
            assert_eq!(e.usage.code.code_bytes, 1);
        } else {
            result.unwrap();
        }
    }
    let mut w = World::new();
    let mut c = Class::ordinary("t/Bad");
    c.major = 50;
    c.run("()V", Body::new(&[0xff], 0, 0));
    let id = w.add(c);
    let e = check(&w, id, 0).unwrap_err();
    assert!(matches!(e.kind, E::Code(_)));
    assert_eq!(e.usage.work_units, 0);
    assert_eq!(e.location, L::Instruction(0));
    assert!(std::error::Error::source(e.as_ref()).is_some());
}
#[test]
fn foreign_and_unretained_selected_class_fail_before_partial_method_lookup() {
    let mut w = World::new();
    let id = w.add(Class::ordinary("t/C"));
    let plans = w.plans_from(&[]);
    let environment = env(&w, &plans);
    let e = verify(&environment, id, usize::MAX, Options::default()).unwrap_err();
    assert!(matches!(e.kind,E::UnretainedClass{class}if class==id));
    assert_eq!(e.usage.code.code_bytes, 0);
    let foreign = World::new();
    assert!(matches!(
        verify(
            &environment,
            foreign.core[0],
            usize::MAX,
            Options::default()
        )
        .unwrap_err()
        .kind,
        E::Registry(_)
    ));
}
#[test]
fn selected_final_override_checks_include_bodyless_methods_and_do_not_stop_at_private_ancestors() {
    for (ancestor_flags, selected_flags, abstract_class, invalid) in [
        (0x111, 0x101, false, true),
        (0x111, 0x401, true, true),
        (0x111, 0x102, false, false),
        (0x111, 0x109, false, false),
        (0x112, 0x101, false, false),
        (0x119, 0x101, false, false),
    ] {
        let mut w = World::new();
        let mut a = Class::ordinary("p/A");
        a.method("m", "()V", ancestor_flags, None);
        let ancestor = w.add(a);
        let mut c = Class::new("q/C", Some("p/A"));
        if abstract_class {
            c.flags |= 0x400;
        }
        c.method("m", "()V", selected_flags, None);
        let id = w.add(c);
        let result = check(&w, id, 0);
        assert_eq!(
            result.is_err(),
            invalid,
            "flags{ancestor_flags:x}/{selected_flags:x} {result:?}"
        );
        if let Err(e) = result {
            assert!(
                matches!(e.kind,E::FinalOverride{ancestor:a,ancestor_method_index:0}if a==ancestor)
            );
            assert_eq!(e.location, L::Method);
        }
    }
    let mut w = World::new();
    let mut a = Class::ordinary("p/A");
    a.method("m", "()V", 0x111, None);
    let ancestor = w.add(a);
    let mut b = Class::new("q/B", Some("p/A"));
    b.method("m", "()V", 0x102, None);
    w.add(b);
    let mut c = Class::new("r/C", Some("q/B"));
    c.method("m", "()V", 0x101, None);
    let id = w.add(c);
    assert!(
        matches!(check(&w,id,0).unwrap_err().kind,E::FinalOverride{ancestor:a,..}if a==ancestor)
    );
}
#[test]
fn package_final_overrides_use_defining_loader_and_transitive_package_reachability() {
    for (package, second_loader, invalid) in
        [("p", false, true), ("q", false, false), ("p", true, false)]
    {
        let mut w = World::new();
        let mut a = Class::ordinary("p/A");
        a.method("m", "()V", 0x110, None);
        w.add(a);
        let mut c = Class::new(&format!("{package}/C"), Some("p/A"));
        c.method("m", "()V", 0x101, None);
        let loader = if second_loader {
            w.registry.create_loader().unwrap()
        } else {
            w.loader
        };
        let id = w.add_in(loader, c);
        assert_eq!(check(&w, id, 0).is_err(), invalid);
    }
    for package in ["p", "r"] {
        let mut w = World::new();
        let mut a = Class::ordinary("p/A");
        a.method("m", "()V", 0x110, None);
        w.add(a);
        let mut middle = Class::new("q/Middle", Some("p/A"));
        middle.method("m", "()V", 0x101, None);
        w.add(middle);
        let mut c = Class::new(&format!("{package}/C"), Some("q/Middle"));
        c.method("m", "()V", 0x101, None);
        let id = w.add(c);
        assert_eq!(check(&w, id, 0).is_err(), package == "p");
    }
}
#[test]
fn protected_inherited_members_use_actual_declarer_package_and_receiver_type() {
    for method in [false, true] {
        for (child_package, receiver, hides, valid) in [
            ("q", "q/Middle", false, false),
            ("p", "q/Middle", false, true),
            ("q", "q/Child", false, true),
            ("q", "q/Middle", true, true),
        ] {
            let mut w = World::new();
            let mut base = Class::ordinary("p/Base");
            if method {
                base.method("m", "()I", 0x104, None);
            } else {
                base.field("x", "I", 4);
            }
            w.add(base);
            let mut middle = Class::new("q/Middle", Some("p/Base"));
            if hides {
                if method {
                    middle.method("m", "()I", 0x101, None);
                } else {
                    middle.field("x", "I", 1);
                }
            }
            w.add(middle);
            let child = format!("{child_package}/Child");
            let mut c = Class::new(&child, Some("q/Middle"));
            let target = c.pool.member(
                if method { 10 } else { 9 },
                "q/Middle",
                if method { "m" } else { "x" },
                if method { "()I" } else { "I" },
            );
            c.run(
                &format!("(L{receiver};)I"),
                Body::new(
                    &[
                        vec![0x2a],
                        op(if method { 0xb6 } else { 0xb4 }, target),
                        vec![0xac],
                    ]
                    .concat(),
                    1,
                    1,
                ),
            );
            let id = w.add(c);
            let result = check(&w, id, 0);
            assert_eq!(
                result.is_ok(),
                valid,
                "{method}/{child_package}/{receiver}/{hides} {result:?}"
            );
        }
    }
}
#[test]
fn protected_runtime_package_is_loader_sensitive_and_array_clone_is_an_exception() {
    let mut w = World::new();
    let mut base = Class::ordinary("p/Base");
    base.field("x", "I", 4);
    w.add(base);
    let second = w.registry.create_loader().unwrap();
    let mut c = Class::new("p/Child", Some("p/Base"));
    let field = c.pool.member(9, "p/Base", "x", "I");
    c.run(
        "(Lp/Base;)I",
        Body::new(&[vec![0x2a], op(0xb4, field), vec![0xac]].concat(), 1, 1),
    );
    let id = w.add_in(second, c);
    assert!(matches!(
        check(&w, id, 0).unwrap_err().kind,
        E::TypeRule { .. }
    ));
    let mut c = Class::ordinary("t/Clone");
    let target = c
        .pool
        .member(10, CORE_NAMES[0], "clone", "()Ljava/lang/Object;");
    c.run(
        "([I)Ljava/lang/Object;",
        Body::new(&[vec![0x2a], op(0xb6, target), vec![0xb0]].concat(), 1, 1),
    );
    let id = w.add(c);
    check(&w, id, 0).unwrap();
}
#[test]
fn large_shared_same_map_prefix_does_not_allocate_or_recompare_locals_per_map() {
    let mut frames = vec![(
        0,
        K::Full {
            locals: vec![V::Top; 30000],
            stack: vec![],
        },
    )];
    frames.extend((1..10000).map(|i| (i, K::Same)));
    let mut bytes = vec![0; 10000];
    bytes.push(0xb1);
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run("()V", Body::new(&bytes, 0, 30000).maps(frames));
    let id = w.add(c);
    let plans = w.plans();
    let env = env(&w, &plans);
    let result = verify(
        &env,
        id,
        0,
        Options {
            max_state_cells: 30000,
            max_work_units: 1_000_000,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(result.usage().state_cells, 30000);
}
#[test]
fn compact_signature_lists_and_expanded_local_prefixes_have_distinct_cell_counts() {
    for (desc, cells) in [("()V", 0), ("(I)V", 2), ("(J)V", 3), ("(JD)V", 6)] {
        let locals = match desc {
            "()V" => 0,
            "(I)V" => 1,
            "(J)V" => 2,
            _ => 4,
        };
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        c.run(desc, Body::new(&[0xb1], 0, locals));
        let id = w.add(c);
        let usage = check(&w, id, 0).unwrap();
        assert_eq!(usage.state_cells, cells, "{desc}");
        assert_eq!(usage.edge_checks, 0);
    }
}
#[test]
fn each_proof_quota_has_zero_exact_and_maximum_boundaries_without_partial_funding() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run("(Lt/C;)Lt/C;", Body::new(&[0x2a, 0xb0], 1, 1));
    let id = w.add(c);
    let plans = w.plans();
    let environment = env(&w, &plans);
    let usage = verify(&environment, id, 0, Options::default())
        .unwrap()
        .usage();
    for (resource, required) in [
        (R::TypeSymbols, usage.type_symbols),
        (R::StateCells, usage.state_cells),
        (R::Utf16Units, usage.utf16_units),
        (R::WorkUnits, usage.work_units),
    ] {
        assert!(required > 0);
        for limit in [0, required - 1] {
            let mut options = Options::default();
            match resource {
                R::TypeSymbols => options.max_type_symbols = limit,
                R::StateCells => options.max_state_cells = limit,
                R::Utf16Units => options.max_utf16_units = limit,
                R::WorkUnits => options.max_work_units = limit,
                _ => unreachable!(),
            };
            let e = verify(&environment, id, 0, options).unwrap_err();
            assert!(
                matches!(e.kind,E::ResourceLimit{resource:r,limit:l,..}if r==resource&&l==limit)
            );
            assert_eq!(e.usage.code, usage.code);
        }
    }
    let exact = Options {
        max_type_symbols: usage.type_symbols,
        max_state_cells: usage.state_cells,
        max_edge_checks: usage.edge_checks,
        max_utf16_units: usage.utf16_units,
        max_work_units: usage.work_units,
        ..Options::default()
    };
    assert_eq!(verify(&environment, id, 0, exact).unwrap().usage(), usage);
    let maximum = Options {
        max_type_symbols: u64::MAX,
        max_state_cells: u64::MAX,
        max_edge_checks: u64::MAX,
        max_utf16_units: u64::MAX,
        max_work_units: u64::MAX,
        ..Options::default()
    };
    assert_eq!(verify(&environment, id, 0, maximum).unwrap().usage(), usage);
    let mut c = Class::ordinary("t/Loop");
    c.run(
        "()V",
        Body::new(&[0xa7, 0, 0], 0, 0).maps(vec![(0, K::Same)]),
    );
    drop(environment);
    let id = w.add(c);
    let plans = w.plans();
    let environment = env(&w, &plans);
    let e = verify(
        &environment,
        id,
        0,
        Options {
            max_edge_checks: 0,
            ..Options::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::EdgeChecks,
            limit: 0,
            required: 1
        }
    ));
    assert_eq!(e.usage.edge_checks, 0);
}
#[test]
fn environment_quota_zero_exact_and_maximum_boundaries_leave_registry_unchanged() {
    let w = World::new();
    let plans = w.plans();
    let refs = plans.iter().collect::<Vec<_>>();
    let baseline = env(&w, &plans).usage();
    let before = (
        w.registry.definition_count(),
        w.registry.total_definition_bytes(),
    );
    for resource in [
        ER::ClassRows,
        ER::Edges,
        ER::Bindings,
        ER::Utf16Units,
        ER::WorkUnits,
    ] {
        let mut options = EnvOptions::default();
        match resource {
            ER::ClassRows => options.max_class_rows = 0,
            ER::Edges => options.max_edges = 0,
            ER::Bindings => options.max_bindings = 0,
            ER::Utf16Units => options.max_utf16_units = 0,
            ER::WorkUnits => options.max_work_units = 0,
        };
        let e = Environment::new(&w.registry, &refs, &[], core(&w), options).unwrap_err();
        assert!(matches!(e.kind,EE::ResourceLimit{resource:r,limit:0,..}if r==resource));
        if resource == ER::ClassRows {
            assert_eq!(e.usage.work_units, 1);
            assert_eq!(e.usage.class_rows, 0);
        }
        if resource == ER::WorkUnits {
            assert_eq!(e.usage.work_units, 0);
        }
    }
    let exact = EnvOptions {
        max_class_rows: baseline.class_rows,
        max_edges: baseline.edges,
        max_bindings: baseline.bindings,
        max_utf16_units: baseline.utf16_units,
        max_work_units: baseline.work_units,
    };
    assert_eq!(
        Environment::new(&w.registry, &refs, &[], core(&w), exact)
            .unwrap()
            .usage(),
        baseline
    );
    let maximum = EnvOptions {
        max_class_rows: u64::MAX,
        max_edges: u64::MAX,
        max_bindings: u64::MAX,
        max_utf16_units: u64::MAX,
        max_work_units: u64::MAX,
    };
    assert_eq!(
        Environment::new(&w.registry, &refs, &[], core(&w), maximum)
            .unwrap()
            .usage(),
        baseline
    );
    assert_eq!(
        (
            w.registry.definition_count(),
            w.registry.total_definition_bytes()
        ),
        before
    );
}
#[test]
fn ten_thousand_class_environment_and_reference_ancestry_are_iterative() {
    let mut w = World::with_options(crabspot_runtime::RegistryOptions {
        max_definitions: 10020,
        ..Default::default()
    });
    let mut last = w.core[0];
    for i in 0..10000 {
        let parent = if i == 0 {
            CORE_NAMES[0].into()
        } else {
            format!("deep/C{}", i - 1)
        };
        let mut c = Class::new(&format!("deep/C{i}"), Some(&parent));
        if i == 9999 {
            c.run("(Ldeep/C9999;)Ldeep/C0;", Body::new(&[0x2a, 0xb0], 1, 1));
        }
        last = w.add(c);
    }
    let plans = w.plans_from(&[last]);
    let environment = env(&w, &plans);
    verify(&environment, last, 0, Options::default()).unwrap();
}
#[test]
fn protected_constructor_allocation_receiver_differs_from_legal_super_chaining() {
    for package in ["p", "q"] {
        let mut w = World::new();
        let mut base = Class::ordinary("p/Base");
        let object = base.pool.member(10, CORE_NAMES[0], "<init>", "()V");
        base.method(
            "<init>",
            "()V",
            4,
            Some(Body::new(
                &[vec![0x2a], op(0xb7, object), vec![0xb1]].concat(),
                1,
                1,
            )),
        );
        w.add(base);
        let mut c = Class::new(&format!("{package}/C"), Some("p/Base"));
        let class = c.pool.class("p/Base");
        let init = c.pool.member(10, "p/Base", "<init>", "()V");
        c.run(
            "()Lp/Base;",
            Body::new(
                &[op(0xbb, class), vec![0x59], op(0xb7, init), vec![0xb0]].concat(),
                2,
                0,
            ),
        );
        c.method(
            "<init>",
            "()V",
            1,
            Some(Body::new(
                &[vec![0x2a], op(0xb7, init), vec![0xb1]].concat(),
                1,
                1,
            )),
        );
        let id = w.add(c);
        assert_eq!(check(&w, id, 0).is_ok(), package == "p");
        check(&w, id, 1).unwrap();
    }
}
#[test]
fn protected_field_fact_lookup_observes_direct_interface_hiding_before_superclass() {
    let mut w = World::new();
    let mut base = Class::ordinary("p/Base");
    base.field("x", "I", 4);
    w.add(base);
    let mut interface = Class::ordinary("t/I");
    interface.flags = 0x601;
    interface.field("x", "I", 0x19);
    w.add(interface);
    let mut middle = Class::new("q/Middle", Some("p/Base"));
    middle.interface("t/I");
    w.add(middle);
    let mut c = Class::new("q/C", Some("q/Middle"));
    let field = c.pool.member(9, "q/Middle", "x", "I");
    c.run(
        "(Lq/Middle;)I",
        Body::new(&[vec![0x2a], op(0xb4, field), vec![0xac]].concat(), 1, 1),
    );
    let id = w.add(c);
    check(&w, id, 0).unwrap(); // Actual field staticness is a later linkage obligation.
}

#[test]
fn every_modern_effective_opcode_and_all_wide_forms_have_checked_positive_fixtures() {
    OPCODE_COVERAGE.with(|c| *c.borrow_mut() = Some(BTreeSet::new()));
    numeric_constants_arithmetic_conversions_comparisons_and_returns_cover_all_types();
    local_family_explicit_implicit_and_wide_forms_require_initialized_types();
    overlapping_category_two_local_stores_invalidate_both_possible_prior_heads();
    every_stack_manipulation_category_form_and_invalid_split_is_independent();
    conditional_branch_families_and_wide_goto_check_every_declared_target();
    switch_targets_including_duplicates_are_proved_from_the_post_key_state();
    missing_dead_frames_falloff_bad_joins_and_zero_target_policy_are_not_reachability_shortcuts();
    every_array_load_store_kind_preserves_exact_primitive_and_reference_shape();
    allocation_cast_instanceof_throw_and_monitors_distinguish_generic_and_ordinary_references();
    all_loadable_constant_types_use_computational_or_designated_core_types_without_linking();
    all_field_opcodes_transfer_descriptor_types_without_requiring_runtime_staticness_or_final_writes();
    every_invocation_family_checks_mixed_argument_types_and_declared_results();
    constructor_base_chain_aliases_and_early_exact_own_fields_have_distinct_rules();
    let observed = OPCODE_COVERAGE.with(|c| c.borrow_mut().take().unwrap());
    let expected = (0u8..=0xc8)
        .filter(|op| !matches!(op, 0xa8 | 0xa9 | 0xc4))
        .map(|op| (op, false))
        .chain(
            [
                0x15, 0x16, 0x17, 0x18, 0x19, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x84,
            ]
            .into_iter()
            .map(|op| (op, true)),
        )
        .collect::<BTreeSet<_>>();
    assert_eq!(
        observed, expected,
        "Every decoder-valid modern effective opcode must be positively proved; jsr/ret/jsr_w historical and wide is an encoding prefix"
    );
}
#[test]
fn ignored_initializer_flags_do_not_make_the_actual_code_body_disappear() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.method("<clinit>", "()V", 0x508, Some(Body::new(&[0xb1], 0, 0)));
    let id = w.add(c);
    let plans = w.plans();
    let environment = env(&w, &plans);
    let result = verify(&environment, id, 0, Options::default()).unwrap();
    assert!(result.code_validation().code().is_some());
    assert_eq!(result.usage().state_cells, 0);
}
#[test]
fn every_core_role_checks_publicness_and_marker_interfaces_check_kind() {
    for role in 0..8 {
        let mut flags = [0x21, 0x601, 0x601, 0x21, 0x21, 0x21, 0x21, 0x421];
        flags[role] &= !1;
        let w = World::with_core(Default::default(), flags);
        let plans = w.plans();
        let error = Environment::new(
            &w.registry,
            &plans.iter().collect::<Vec<_>>(),
            &[],
            core(&w),
            EnvOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(error.kind, EE::InvalidCore { .. }));
    }
    for role in [1, 2] {
        let mut flags = [0x21, 0x601, 0x601, 0x21, 0x21, 0x21, 0x21, 0x421];
        flags[role] = 0x21;
        let w = World::with_core(Default::default(), flags);
        let plans = w.plans();
        assert!(matches!(
            Environment::new(
                &w.registry,
                &plans.iter().collect::<Vec<_>>(),
                &[],
                core(&w),
                EnvOptions::default()
            )
            .unwrap_err()
            .kind,
            EE::InvalidCore { .. }
        ));
    }
}
#[test]
fn every_local_encoding_rejects_wrong_value_types_at_the_actual_load_or_store() {
    for (load, store, base_load, base_store, ret, desc, width, reference) in [
        (0x15, 0x36, 0x1a, 0x3b, 0xac, "()I", 1, false),
        (0x16, 0x37, 0x1e, 0x3f, 0xad, "()J", 2, false),
        (0x17, 0x38, 0x22, 0x43, 0xae, "()F", 1, false),
        (0x18, 0x39, 0x26, 0x47, 0xaf, "()D", 2, false),
        (
            0x19,
            0x3a,
            0x2a,
            0x4b,
            0xb0,
            "()Ljava/lang/Object;",
            1,
            true,
        ),
    ] {
        let wrong = if reference { 3 } else { 1 };
        let init = if reference { 0x36 } else { 0x3a };
        let init_base = if reference { 0x3b } else { 0x4b };
        for i in 0..4 {
            rejected(
                desc,
                &[wrong, init_base + i, base_load + i, ret],
                2,
                u16::from(i) + width,
            );
            rejected(
                "()V",
                &[wrong, base_store + i, 0xb1],
                2,
                u16::from(i) + width,
            );
        }
        rejected(desc, &[wrong, init, 7, load, 7, ret], 2, 7 + width);
        rejected("()V", &[wrong, store, 7, 0xb1], 2, 7 + width);
        rejected(
            desc,
            &[wrong, 0xc4, init, 1, 0, 0xc4, load, 1, 0, ret],
            2,
            256 + width,
        );
        rejected("()V", &[wrong, 0xc4, store, 1, 0, 0xb1], 2, 256 + width);
    }
    rejected("()V", &[1, 0x4b, 0xc4, 0x84, 0, 0, 0, 1, 0xb1], 1, 1);
}
#[test]
fn constant_pushes_negation_monitors_and_every_stack_opcode_have_negative_capacity_or_category_cases()
 {
    for constant in 1..=0x0f {
        rejected("()V", &[constant, 0xb1], 0, 0);
    }
    rejected("()V", &[0x10, 0, 0xb1], 0, 0);
    rejected("()V", &[0x11, 0, 0, 0xb1], 0, 0);
    for opcode in [0x74, 0x75, 0x76, 0x77, 0xc2, 0xc3] {
        rejected(
            "()V",
            &[
                if matches!(opcode, 0xc2 | 0xc3) { 3 } else { 1 },
                opcode,
                0xb1,
            ],
            2,
            0,
        );
    }
    for opcode in 0x57..=0x5f {
        rejected(
            "()V",
            &[
                if matches!(opcode, 0x58 | 0x5c) { 3 } else { 9 },
                opcode,
                0xb1,
            ],
            2,
            0,
        );
    }
    for (constant, ret, desc) in [
        (3, 0xac, "()F"),
        (9, 0xad, "()D"),
        (0x0b, 0xae, "()I"),
        (0x0e, 0xaf, "()J"),
        (1, 0xb0, "()I"),
    ] {
        rejected(desc, &[constant, ret], 2, 0);
    }
    rejected("()I", &[0xb1], 0, 0);
}
#[test]
fn every_conditional_family_and_unconditional_target_rejects_incompatible_type_state() {
    for opcode in 0x99..=0xa6 {
        let prefix = match opcode {
            0x99..=0x9e => vec![1],
            0x9f..=0xa4 => vec![1, 1],
            _ => vec![3, 4],
        };
        let target = u16::try_from(prefix.len() + 4).unwrap();
        let bytes = [prefix, vec![opcode, 0, 4, 0xb1, 0xb1]].concat();
        assert!(matches!(
            body_result("()V", Body::new(&bytes, 2, 0).maps(vec![(target, K::Same)]))
                .unwrap_err()
                .kind,
            E::TypeRule { .. }
        ));
    }
    for opcode in [0xc6, 0xc7] {
        assert!(matches!(
            body_result(
                "()V",
                Body::new(&[3, opcode, 0, 4, 0xb1, 0xb1], 1, 0).maps(vec![(5, K::Same)])
            )
            .unwrap_err()
            .kind,
            E::TypeRule { .. }
        ));
    }
    for wide in [false, true] {
        let bytes = if wide {
            vec![3, 0xc8, 0, 0, 0, 5, 0xb1]
        } else {
            vec![3, 0xa7, 0, 3, 0xb1]
        };
        let target = u16::try_from(bytes.len() - 1).unwrap();
        assert!(matches!(
            body_result("()V", Body::new(&bytes, 1, 0).maps(vec![(target, K::Same)]))
                .unwrap_err()
                .kind,
            E::TypeRule { .. }
        ));
    }
}
#[test]
fn conflicting_ordered_hierarchy_facts_for_the_same_class_are_not_merged_by_equal_names() {
    use crabspot_runtime::{
        DefinitionRegistry, HierarchyOptions, HierarchyResolver, LoaderId, ResolveError,
        build_hierarchy,
    };
    struct Choice {
        parent: ClassId,
        bootstrap: LoaderId,
    }
    impl HierarchyResolver for Choice {
        fn resolve(
            &mut self,
            registry: &DefinitionRegistry,
            loader: LoaderId,
            name: &[u16],
        ) -> Result<Option<ClassId>, ResolveError> {
            if name == "q/P".encode_utf16().collect::<Vec<_>>() {
                Ok(Some(self.parent))
            } else {
                registry
                    .lookup_class(loader, name)
                    .and_then(|id| {
                        if id.is_some() {
                            Ok(id)
                        } else {
                            registry.lookup_class(self.bootstrap, name)
                        }
                    })
                    .map_err(|e| ResolveError::new(e.to_string()))
            }
        }
    }
    let mut w = World::new();
    let first = w.registry.create_loader().unwrap();
    let second = w.registry.create_loader().unwrap();
    let third = w.registry.create_loader().unwrap();
    let a = w.add_in(first, Class::ordinary("q/P"));
    let b = w.add_in(second, Class::ordinary("q/P"));
    let selected = w.add_in(third, Class::new("t/C", Some("q/P")));
    let mut plans = w.plans_from(&[]);
    for parent in [a, b] {
        plans.push(
            build_hierarchy(
                &w.registry,
                selected,
                &mut Choice {
                    parent,
                    bootstrap: w.loader,
                },
                HierarchyOptions::default(),
            )
            .unwrap(),
        );
    }
    let error = Environment::new(
        &w.registry,
        &plans.iter().collect::<Vec<_>>(),
        &[],
        core(&w),
        EnvOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error.kind,EE::ConflictingHierarchy{class}if class==selected));
}
#[test]
fn field_array_allocation_and_switch_negative_transfers_reject_at_their_own_opcodes() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.field("x", "I", 1);
    let field = c.pool.member(9, "t/C", "x", "I");
    let element = c.pool.class("t/C");
    let array = c.pool.class("[[I");
    let mut expected = Vec::new();
    for (name, desc, bytes, stack, locals, offset) in [
        (
            "gs",
            "()V",
            [op(0xb2, field), vec![0x57, 0xb1]].concat(),
            0,
            0,
            0,
        ),
        (
            "ps",
            "()V",
            [vec![1], op(0xb3, field), vec![0xb1]].concat(),
            1,
            0,
            1,
        ),
        (
            "gf",
            "()V",
            [vec![3], op(0xb4, field), vec![0x57, 0xb1]].concat(),
            1,
            0,
            1,
        ),
        (
            "pf",
            "(Lt/C;)V",
            [vec![0x2a, 1], op(0xb5, field), vec![0xb1]].concat(),
            2,
            1,
            2,
        ),
        ("newp", "()V", vec![1, 0xbc, 10, 0x57, 0xb1], 1, 0, 1),
        (
            "newr",
            "()V",
            [vec![1], op(0xbd, element), vec![0x57, 0xb1]].concat(),
            1,
            0,
            1,
        ),
        (
            "newmulti",
            "()V",
            [vec![3, 0x0b], op(0xc5, array), vec![2, 0x57, 0xb1]].concat(),
            2,
            0,
            2,
        ),
    ] {
        let index = c.method(name, desc, 9, Some(Body::new(&bytes, stack, locals)));
        expected.push((index, offset));
    }
    let id = w.add(c);
    for (index, offset) in expected {
        let e = check(&w, id, index).unwrap_err();
        assert!(matches!(e.kind, E::TypeRule { .. }));
        assert_eq!(e.location, L::Instruction(offset));
    }
    for table in [true, false] {
        let mut bytes = vec![1, if table { 0xaa } else { 0xab }, 0, 0];
        let target = if table { 24 } else { 28 };
        let words: Vec<i32> = if table {
            vec![target - 1, 0, 1, target - 1, target - 1]
        } else {
            vec![target - 1, 2, 0, target - 1, 1, target - 1]
        };
        for word in words {
            bytes.extend(word.to_be_bytes());
        }
        bytes.push(0xb1);
        let e = body_result(
            "()V",
            Body::new(&bytes, 1, 0).maps(vec![(target as u16, K::Same)]),
        )
        .unwrap_err();
        assert!(matches!(e.kind, E::TypeRule { .. }));
        assert_eq!(e.location, L::Instruction(1));
    }
}
#[test]
fn maximum_utf16_descriptor_reuse_has_bounded_symbol_state_and_scan_accounting() {
    fn usage(count: usize) -> crabspot_verifier::TypeVerificationUsage {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let descriptor = format!("()L{};", "x".repeat(65531));
        assert_eq!(descriptor.encode_utf16().count(), 65535);
        let target = c.pool.member(10, "t/C", "missing", &descriptor);
        let mut bytes = Vec::new();
        for _ in 0..count {
            bytes.extend(op(0xb8, target));
            bytes.push(0x57);
        }
        bytes.push(0xb1);
        c.run("()V", Body::new(&bytes, 1, 0));
        let id = w.add(c);
        let plans = w.plans();
        let environment = env(&w, &plans);
        verify(
            &environment,
            id,
            0,
            Options {
                max_type_symbols: 64,
                max_utf16_units: 150000,
                max_state_cells: u64::try_from(count).unwrap(),
                max_work_units: 1000000,
                ..Options::default()
            },
        )
        .unwrap()
        .usage()
    }
    let one = usage(1);
    let many = usage(1000);
    assert_eq!(one.utf16_units, many.utf16_units);
    assert_eq!(one.type_symbols, many.type_symbols);
    assert_eq!(many.state_cells, 1000);
    assert_eq!(one.state_cells, 1);
    assert_eq!(many.edge_checks, 0);
}
#[test]
fn handler_top_slot_can_be_ignored_but_cannot_be_used_as_a_category_one_value() {
    for (kind, tail, valid) in [
        (V::Top, vec![0xb1], true),
        (V::Top, vec![0x57, 0xb1], false),
        (V::Top, vec![0x4b, 0xb1], false),
    ] {
        let mut w = World::new();
        let mut c = Class::ordinary("t/C");
        let bytes = [vec![0, 0xb1], tail].concat();
        let mut body = Body::new(&bytes, 1, 1).maps(vec![(2, K::SameLocalsOneStack(kind))]);
        body.handlers.push([0, 1, 2, 0]);
        c.run("()V", body);
        let id = w.add(c);
        let result = check(&w, id, 0);
        assert_eq!(result.is_ok(), valid, "{result:?}");
        if let Err(e) = result {
            assert!(matches!(e.kind, E::TypeRule { .. }));
            assert_eq!(e.location, L::Instruction(2));
        }
    }
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    let throwable = c.pool.class(CORE_NAMES[3]);
    let mut body = Body::new(&[0, 0xb1, 0x57, 0xb1], 1, 0)
        .maps(vec![(2, K::SameLocalsOneStack(V::Object(throwable)))]);
    body.handlers.push([0, 1, 2, 0]);
    c.run("()V", body);
    let id = w.add(c);
    check(&w, id, 0).unwrap();
}
#[test]
fn interface_object_metadata_edge_does_not_shadow_a_real_superclass_field_fact() {
    let mut w = World::new();
    let mut base = Class::ordinary("p/Base");
    base.field("rootField", "I", 1);
    w.add(base);
    let mut interface = Class::ordinary("t/I");
    interface.flags = 0x601;
    w.add(interface);
    let mut middle = Class::new("q/Middle", Some("p/Base"));
    middle.interface("t/I");
    w.add(middle);
    let mut c = Class::new("q/C", Some("q/Middle"));
    let field = c.pool.member(9, "q/Middle", "rootField", "I");
    c.run(
        "(Lq/Middle;)I",
        Body::new(&[vec![0x2a], op(0xb4, field), vec![0xac]].concat(), 1, 1),
    );
    let id = w.add(c);
    check(&w, id, 0).unwrap();
}
#[test]
fn first_mandatory_semantic_scan_fails_atomically_before_reading_its_three_units() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/C");
    c.run("()V", Body::new(&[0xb1], 0, 0));
    let id = w.add(c);
    let plans = w.plans();
    let environment = env(&w, &plans);
    // Both selected class spelling t/C and selected descriptor ()V are3units; cache composition does not affect the first required span.
    for limit in [0, 2] {
        let e = verify(
            &environment,
            id,
            0,
            Options {
                max_utf16_units: limit,
                ..Options::default()
            },
        )
        .unwrap_err();
        assert!(
            matches!(e.kind,E::ResourceLimit{resource:R::Utf16Units,limit:l,required:3}if l==limit)
        );
        assert_eq!(e.usage.utf16_units, 0);
        assert_eq!(e.usage.code.code_bytes, 1);
    }
    let e = verify(
        &environment,
        id,
        0,
        Options {
            max_work_units: 0,
            ..Options::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        e.kind,
        E::ResourceLimit {
            resource: R::WorkUnits,
            limit: 0,
            required: 1
        }
    ));
    assert_eq!(e.usage.work_units, 0);
}
#[test]
fn preview_policy_is_explicit_in_nested_code_options_and_preserves_zero_type_phase_on_failure() {
    let mut w = World::new();
    let mut c = Class::ordinary("t/Preview");
    c.minor = 65535;
    c.run("()V", Body::new(&[0xb1], 0, 0));
    c.method("native", "()V", 0x109, None);
    let id = w.add(c);
    let plans = w.plans();
    let environment = env(&w, &plans);
    for index in 0..2 {
        let error = verify(&environment, id, index, Options::default()).unwrap_err();
        assert!(
            matches!(error.kind,E::Code(ref code)if code.kind==crabspot_verifier::CodeValidationErrorKind::PreviewDisabled)
        );
        assert_eq!(error.usage.work_units, 0);
        verify(
            &environment,
            id,
            index,
            Options {
                code: crabspot_verifier::CodeValidationOptions {
                    allow_preview: true,
                    ..Default::default()
                },
                ..Options::default()
            },
        )
        .unwrap();
    }
}

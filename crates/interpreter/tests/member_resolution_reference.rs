//! Execute the original caller bytes and inspect real heap values and domain-owned CP records.
#[path = "support/construction_core.rs"]
mod construction_core;
#[path = "support/member_resolution_world.rs"]
mod world;

use crabspot_interpreter::InitializationStatus;
use crabspot_runtime::{
    MemberResolutionFailure as Failure, MemberResolutionOutcome as Site, Value,
};
use world::{A, B, C, D, E, World, call, owner_name};

const ORDINARY: &str = include_str!("../../../tests/fixtures/member-resolution/ordinary/zulu.tsv");
const DISPATCH: &str = include_str!("../../../tests/fixtures/member-resolution/dispatch/zulu.tsv");
const LEAVES: &str =
    include_str!("../../../tests/fixtures/member-resolution/signature-order/zulu.tsv");
const ORDER: &str =
    include_str!("../../../tests/fixtures/member-resolution/field-method-order/zulu.tsv");

fn assert_missing_initiation(
    error: &crabspot_interpreter::ExecutionError,
    origin: crabspot_runtime::ClassId,
    loader: crabspot_runtime::LoaderId,
) {
    use crabspot_runtime::{MemberResolutionError, MemberResolutionErrorKind};
    use std::error::Error;
    let mut source = error.source();
    loop {
        let current = source.unwrap_or_else(|| panic!("missing member cause in {error:?}"));
        if let Some(detail) = current.downcast_ref::<MemberResolutionError>() {
            assert_eq!(
                detail.kind,
                MemberResolutionErrorKind::MissingInitiation { origin, loader }
            );
            return;
        }
        source = current.source();
    }
}

#[test]
fn missing_host_initiation_can_be_supplied_before_retrying_the_original_site() {
    let mut w = World::new("missing-class");
    let owner = w.define(A, "owner/Late");
    let sticky = w.define(C, "caller/Sticky");
    let fresh = w.define(C, "caller/Fresh");
    let loader = w.loaders[C];
    let cases = [
        (
            "field1",
            "count",
            "I",
            "I:51",
            Site::Field(w.field(owner, "count", "I")),
        ),
        (
            "field2",
            "count",
            "I",
            "I:51",
            Site::Field(w.field(owner, "count", "I")),
        ),
        (
            "method",
            "value",
            "()I",
            "I:52",
            Site::Method(w.method(owner, "value", "()I")),
        ),
    ];
    for (entry, name, desc, _, _) in cases {
        let cp = w.member_cp(sticky, "owner/Late", name, desc);
        let method = w.method(sticky, entry, "()I");
        let error = w.with_session(|session, _| {
            session
                .run_static(
                    method,
                    &[],
                    Default::default(),
                    Default::default(),
                    Default::default(),
                )
                .unwrap_err()
        });
        assert_missing_initiation(&error, sticky, loader);
        assert_eq!(w.domain.member_site(sticky, cp).unwrap(), None);
    }

    w.alias(C, "owner/Late", owner);
    for caller in [sticky, fresh] {
        for (entry, name, desc, expected, resolved) in cases {
            let method = w.method(caller, entry, "()I");
            let cp = w.member_cp(caller, "owner/Late", name, desc);
            assert_eq!(w.with_session(|s, _| call(s, method, &[])), expected);
            assert_eq!(w.domain.member_site(caller, cp).unwrap(), Some(resolved));
        }
    }
}

#[test]
fn missing_value_type_after_field_resolution_preserves_the_site_and_earlier_effects() {
    let mut w = World::new("ordinary");
    let base = w.define(A, "base/Declarer");
    w.alias(B, "base/Declarer", base);
    let sub = w.define(B, "symbolic/Sub");
    w.alias(C, "symbolic/Sub", sub);
    let caller = w.define(C, "caller/Probe");
    let token = w.define(C, "value/K");
    let entry = w.method(caller, "combined", "()I");
    let cp = w.member_cp(caller, "symbolic/Sub", "slot", "Lvalue/K;");
    let slot = w.field(base, "slot", "Lvalue/K;");
    let entered = w.field(caller, "entered", "I");
    let returned = w.field(caller, "returned", "I");
    let loader = w.loaders[A];
    w.with_session(|s, statics| {
        let error = s
            .run_static(
                entry,
                &[],
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
        assert_missing_initiation(&error, base, loader);
        let ctx = s.context();
        assert_eq!(ctx.inspect_static(statics, entered).unwrap(), Value::Int(1));
        assert_eq!(
            ctx.inspect_static(statics, returned).unwrap(),
            Value::Int(0)
        );
        assert!(
            matches!(ctx.inspect_static(statics, slot).unwrap(), Value::Reference(r) if r.is_null())
        );
    });
    assert_eq!(
        w.domain.member_site(caller, cp).unwrap(),
        Some(Site::Field(slot))
    );
    assert!(w.requires(A, C, "K"));
    assert_eq!(w.local(A, "K"), "LinkageError");
    w.alias(A, "value/K", token);
    assert_eq!(w.with_session(|s, _| call(s, entry, &[])), "I:44");
    assert_eq!(
        w.domain.member_site(caller, cp).unwrap(),
        Some(Site::Field(slot))
    );
}

#[test]
fn late_delegation_between_executions_keeps_the_resolved_field() {
    let mut w = World::new("ordinary");
    let base = w.define(A, "base/Declarer");
    w.alias(B, "base/Declarer", base);
    let sub = w.define(B, "symbolic/Sub");
    w.alias(C, "symbolic/Sub", sub);
    let caller = w.define(C, "caller/Probe");
    let read = w.method(caller, "read", "()I");
    let cp = w.member_cp(caller, "symbolic/Sub", "slot", "Lvalue/K;");
    let slot = w.field(base, "slot", "Lvalue/K;");
    assert_eq!(w.with_session(|s, _| call(s, read, &[])), "I:11");
    assert_eq!(
        w.domain.member_site(caller, cp).unwrap(),
        Some(Site::Field(slot))
    );
    assert_eq!(w.local(A, "K"), "A");
    assert_eq!(w.local(C, "K"), "LinkageError");
    w.alias(C, "value/K", w.class(A, "value/K"));
    assert_eq!(w.local(B, "K"), "B");
    assert_eq!(w.with_session(|s, _| call(s, read, &[])), "I:11");
    assert_eq!(
        w.domain.member_site(caller, cp).unwrap(),
        Some(Site::Field(slot))
    );
}

#[test]
fn genuine_caller_preserves_values_effects_and_resolved_declarer() {
    for line in ORDINARY.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        let [
            method,
            timing,
            first,
            post,
            repair,
            second,
            counts,
            retained,
        ] = parts.as_slice()
        else {
            panic!("{line}")
        };
        let mut w = World::new("ordinary");
        let base = w.define(A, "base/Declarer");
        w.alias(B, "base/Declarer", base);
        let sub = w.define(B, "symbolic/Sub");
        w.alias(C, "symbolic/Sub", sub);
        let caller = w.define(C, "caller/Probe");
        if matches!(*timing, "same" | "symbolic-only" | "split" | "left-only") {
            assert_eq!(w.local(A, "K"), "A");
        }
        if matches!(*timing, "same" | "symbolic-only") {
            w.alias(C, "value/K", w.class(A, "value/K"));
        }
        if *timing == "symbolic-only" {
            assert_eq!(w.local(B, "K"), "B");
        }
        if *method == "combined" || matches!(*timing, "split" | "right-only") {
            w.local(C, "K"); // The combined caller executes new before touching its inherited field.
        }
        let needs_host_binding = *method == "combined" && matches!(*timing, "late" | "right-only");
        if needs_host_binding {
            // Non-null value checking requires E's actual type, supplied by delegation in this closed domain.
            w.alias(A, "value/K", w.class(C, "value/K"));
        }
        let entry = w.method(caller, method, "()I");
        let entered = w.field(caller, "entered", "I");
        let returned = w.field(caller, "returned", "I");
        let calls = w.field(base, "calls", "I");
        let slot = w.field(base, "slot", "Lvalue/K;");
        let echo = w.method(base, "echo", "(Lvalue/K;)Lvalue/K;");
        let field_cp = w.member_cp(caller, "symbolic/Sub", "slot", "Lvalue/K;");
        let call_cp = w.member_cp(caller, "symbolic/Sub", "echo", "(Lvalue/K;)Lvalue/K;");
        let loaders = w.loaders;
        w.with_session(|session, statics| {
            assert_eq!(call(session, entry, &[]), *first, "{line}");
            assert_eq!(call(session, entry, &[]), *second, "{line}");
            let ctx = session.context();
            let effects =
                [entered, returned, calls].map(|f| match ctx.inspect_static(statics, f).unwrap() {
                    Value::Int(n) => n.to_string(),
                    other => panic!("{other:?}"),
                });
            assert_eq!(effects.join("/"), *counts, "{line}");
            let Value::Reference(value) = ctx.inspect_static(statics, slot).unwrap() else {
                panic!("expected reference")
            };
            let token = if value.is_null() {
                "null"
            } else {
                owner_name(
                    &loaders,
                    ctx.registry(),
                    ctx.heap().instance_class(value).unwrap(),
                )
            };
            assert_eq!(token, *retained, "{line}");
            if first.starts_with("I:") {
                assert_eq!(
                    session.status(base).unwrap(),
                    InitializationStatus::Completed
                );
                assert_eq!(
                    session.status(sub).unwrap(),
                    InitializationStatus::Uninitialized
                );
            }
        });
        let field_site = w.domain.member_site(caller, field_cp).unwrap();
        let method_site = w.domain.member_site(caller, call_cp).unwrap();
        let failed = *first == "LinkageError";
        if *method != "call" {
            assert_eq!(
                field_site,
                Some(if failed {
                    Site::Failure(Failure::LinkageError)
                } else {
                    Site::Field(slot)
                }),
                "{line}"
            );
        } else {
            assert_eq!(field_site, None);
        }
        if *method == "call" || (*method == "combined" && !failed) {
            assert_eq!(
                method_site,
                Some(if failed {
                    Site::Failure(Failure::LinkageError)
                } else {
                    Site::Method(echo)
                }),
                "{line}"
            );
        } else {
            assert_eq!(method_site, None);
        }
        assert_eq!(w.requires(A, C, "K"), !failed);
        assert!(!w.requires(B, C, "K"));
        let aa = w.local(A, "K");
        let cc = w.local(C, "K");
        let bb = w.local(B, "K");
        if needs_host_binding {
            assert_eq!((aa, cc, bb), ("C", "C", "B"));
            assert_eq!((*post, *repair), ("LinkageError/C/B", "C"));
        } else {
            assert_eq!(format!("{aa}/{cc}/{bb}"), *post, "{line}");
            if *repair != "-" {
                let (client, owner) = if aa == "LinkageError" { (A, C) } else { (C, A) };
                w.alias(client, "value/K", w.class(owner, "value/K"));
                assert_eq!(w.local(client, "K"), *repair);
            }
        }
    }
}

#[test]
fn genuine_virtual_and_interface_sites_select_per_receiver() {
    let rows: Vec<Vec<_>> = DISPATCH.lines().map(|l| l.split('\t').collect()).collect();
    for group in rows.chunks_exact(4) {
        let mode = group[0][0];
        let interface: i32 = group[0][1].parse().unwrap();
        let mut w = World::new("dispatch");
        let port = w.define(A, "api/Port");
        let owner = w.define(A, "base/Owner");
        w.alias(B, "base/Owner", owner);
        let sub = w.define(B, "symbolic/Sub");
        for client in [C, D, E] {
            w.alias(client, "symbolic/Sub", sub);
            w.alias(client, "api/Port", port);
        }
        w.alias(C, "base/Owner", owner);
        let first = w.define(D, "dyn/First");
        let second = w.define(D, "dyn/Second");
        let plain = w.define(E, "dyn/Plain");
        for (name, id) in [
            ("dyn/First", first),
            ("dyn/Second", second),
            ("dyn/Plain", plain),
        ] {
            w.alias(C, name, id);
        }
        let caller = w.define(C, "caller/Dispatch");
        w.local(A, "K");
        w.local(B, "K");
        let k = w.class(A, "value/K");
        for (client, split) in [
            (C, "caller-split"),
            (D, "override-split"),
            (E, "plain-split"),
        ] {
            if mode == split {
                w.local(client, "K");
            } else {
                w.alias(client, "value/K", k);
            }
        }
        let cp = if interface == 0 {
            w.member_cp(caller, "symbolic/Sub", "hit", "(Lvalue/K;)I")
        } else {
            w.member_cp(caller, "api/Port", "hit", "(Lvalue/K;)I")
        };
        let actual = w.method(
            if interface == 0 { owner } else { port },
            "hit",
            "(Lvalue/K;)I",
        );
        if mode == "override-split" {
            // A host cannot open a heap containing either conflicting override without preparation.
            assert!(matches!(
                w.prepare_failure(first),
                crabspot_runtime::LoadingConstraintErrorKind::ConstraintConflict { .. }
            ));
            assert!(matches!(
                w.prepare_failure(second),
                crabspot_runtime::LoadingConstraintErrorKind::ConstraintConflict { .. }
            ));
            assert_eq!(w.domain.member_site(caller, cp).unwrap(), None);
            let entry = w.method(caller, "run", "(II)I");
            let held = w.field(caller, "held", "Lsymbolic/Sub;");
            w.with_session_excluding(&[first, second], |s, statics| {
                assert_eq!(
                    call(s, entry, &[Value::Int(3), Value::Int(interface)]),
                    group[2][5]
                );
                let ctx = s.context();
                let Value::Reference(receiver) = ctx.inspect_static(statics, held).unwrap() else {
                    panic!("expected the plain receiver");
                };
                assert_eq!(ctx.heap().instance_class(receiver).unwrap(), plain);
            });
            assert_eq!(
                w.domain.member_site(caller, cp).unwrap(),
                Some(Site::Method(actual))
            );
            continue;
        }
        let entry = w.method(caller, "run", "(II)I");
        let held = w.field(caller, "held", "Lsymbolic/Sub;");
        let slot = w.field(owner, "slot", "Lvalue/K;");
        let entered = w.field(caller, "entered", "I");
        let returned = w.field(caller, "returned", "I");
        let calls = w.field(owner, "calls", "I");
        let loaders = w.loaders;
        w.with_session(|session, statics| {
            for row in group {
                let receiver: i32 = row[2].parse().unwrap();
                assert_eq!(row[4], "invoke");
                assert_eq!(
                    call(
                        session,
                        entry,
                        &[Value::Int(receiver), Value::Int(interface)]
                    ),
                    row[5],
                    "{row:?}"
                );
                let ctx = session.context();
                let counts = [entered, returned, calls].map(|f| {
                    match ctx.inspect_static(statics, f).unwrap() {
                        Value::Int(n) => n.to_string(),
                        v => panic!("{v:?}"),
                    }
                });
                assert_eq!(counts.join("/"), row[8], "{row:?}");
                let Value::Reference(r) = ctx.inspect_static(statics, held).unwrap() else {
                    panic!("expected held receiver")
                };
                let class = ctx.heap().instance_class(r).unwrap();
                assert_eq!(
                    class,
                    match receiver {
                        1 => first,
                        2 => second,
                        _ => plain,
                    }
                );
                assert_eq!(
                    ctx.registry()
                        .class(class)
                        .unwrap()
                        .name()
                        .unwrap()
                        .escaped()
                        .rsplit('/')
                        .next()
                        .unwrap(),
                    row[6]
                );
                let Value::Reference(token) = ctx.heap().get_field(r, slot).unwrap() else {
                    panic!("expected slot")
                };
                let token_owner = if token.is_null() {
                    "null"
                } else {
                    owner_name(
                        &loaders,
                        ctx.registry(),
                        ctx.heap().instance_class(token).unwrap(),
                    )
                };
                assert_eq!(token_owner, row[7]);
            }
        });
        assert_eq!(
            w.domain.member_site(caller, cp).unwrap(),
            Some(if mode == "caller-split" {
                Site::Failure(Failure::LinkageError)
            } else {
                Site::Method(actual)
            })
        );
        assert!(!w.requires(B, C, "K"));
        assert!(!w.requires(E, A, "K"));
    }
}

fn order_world(family: &'static str) -> (World, crabspot_runtime::ClassId) {
    let mut w = World::new(family);
    let base = w.define(A, "owner/Base");
    w.alias(B, "owner/Base", base);
    let sub = w.define(B, "sym/Sub");
    w.alias(C, "sym/Sub", sub);
    let caller = w.define(C, "call/Steps");
    (w, caller)
}

#[test]
fn multi_leaf_resolution_preserves_the_successful_prefix_on_retry() {
    for line in LEAVES.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        let [kind, mode, first, p, q, second, independent] = parts.as_slice() else {
            panic!("{line}")
        };
        let (mut w, caller) = order_world("signature-order");
        if let Some(leaf) = mode.strip_prefix("split-") {
            w.local(A, leaf);
            w.local(C, leaf);
        }
        if *mode == "shared" {
            for leaf in ["P", "Q"] {
                w.local(A, leaf);
                let name = format!("value/{leaf}");
                w.alias(C, &name, w.class(A, &name));
            }
        }
        let entry = w.method(caller, kind, "()I");
        assert_eq!(w.with_session(|s, _| call(s, entry, &[])), *first);
        for (leaf, result) in [("P", p), ("Q", q)] {
            assert_eq!(
                format!("{}/{}", w.local(A, leaf), w.local(C, leaf)),
                **result,
                "{line}"
            );
        }
        assert_eq!(w.with_session(|s, _| call(s, entry, &[])), *second);
        assert_eq!(
            format!("{}/{}", w.local(B, "P"), w.local(B, "Q")),
            *independent
        );
    }
}

#[test]
fn access_is_checked_before_agreement_and_instruction_category_after_resolution() {
    for line in ORDER.lines() {
        let row: Vec<_> = line.split('\t').collect();
        let (name, mode) = (row[0], row[1]);
        let (mut w, caller) = order_world("field-method-order");
        if mode != "late" {
            w.local(A, "K");
            if mode == "same" {
                w.alias(C, "value/K", w.class(A, "value/K"));
            } else {
                w.local(C, "K");
            }
        }
        let entry = w.method(caller, name, "()I");
        let access_failure = matches!(
            name,
            "privateField" | "privateCall" | "missingField" | "missingCall"
        );
        let expected = if mode == "split" && !access_failure {
            "LinkageError"
        } else {
            row[2]
        };
        assert_eq!(
            w.with_session(|s, _| call(s, entry, &[])),
            expected,
            "{line}"
        );
        let (member, desc) = match name.trim_end_matches("Wrong") {
            "getStatic" => ("sf", "Lvalue/K;"),
            "getInstance" => ("instanceField", "Lvalue/K;"),
            "callStatic" => ("publicCall", "(Lvalue/K;)V"),
            "callInstance" => ("instanceCall", "(Lvalue/K;)V"),
            "finalWrite" => ("finalField", "Lvalue/K;"),
            "missingField" => ("absent", "Lvalue/K;"),
            "missingCall" => ("absent", "(Lvalue/K;)V"),
            "privateCall" => (name, "(Lvalue/K;)V"),
            "privateField" => (name, "Lvalue/K;"),
            other => panic!("unknown caller {other}"),
        };
        let cp = w.member_cp(caller, "sym/Sub", member, desc);
        let base = w.class(A, "owner/Base");
        let resolved = match name {
            "privateField" | "privateCall" => Site::Failure(Failure::IllegalAccess),
            "missingField" => Site::Failure(Failure::NoSuchField),
            "missingCall" => Site::Failure(Failure::NoSuchMethod),
            _ if mode == "split" => Site::Failure(Failure::LinkageError),
            _ if desc.starts_with('(') => Site::Method(w.method(base, member, desc)),
            _ => Site::Field(w.field(base, member, desc)),
        };
        assert_eq!(
            w.domain.member_site(caller, cp).unwrap(),
            Some(resolved),
            "{line}"
        );
        let agreement = !access_failure && mode != "split";
        assert_eq!(w.requires(A, C, "K"), agreement, "{line}");
        assert!(!w.requires(B, C, "K"));
        assert_eq!(w.local(A, "K"), "A");
        assert_eq!(
            w.local(C, "K"),
            match mode {
                "same" => "A",
                "late" if agreement => "LinkageError",
                _ => "C",
            }
        );
        assert_eq!(
            w.with_session(|s, _| call(s, entry, &[])),
            expected,
            "{line}"
        );
        if let Some(valid) = name.strip_suffix("Wrong") {
            let right = w.method(caller, valid, "()I");
            let expected = if mode == "split" {
                "LinkageError"
            } else if valid.ends_with("Instance") {
                "NullPointerException"
            } else {
                "I:1"
            };
            assert_eq!(
                w.with_session(|s, _| call(s, right, &[])),
                expected,
                "{line}"
            );
        }
        assert_eq!(
            w.domain.member_site(caller, cp).unwrap(),
            Some(resolved),
            "{line}"
        );
    }
}

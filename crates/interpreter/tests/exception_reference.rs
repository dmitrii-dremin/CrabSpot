//! Independent JDK supplied-Throwable outcomes and source-derived visible state.
//! Original core metadata and default host objects do not certify Throwable construction.
use crabspot_classfile::{AttributeBody, ClassFile, Constant, ParseOptions};
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome, ExecutionOptions,
    execute_instance_method_body_with_exceptions,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldResolutionContext, FieldResolutionOptions, HeapOptions,
    InstanceOptions, LoaderId, ManagedHeap, Reference, ReferenceType, ReferenceTypeUniverse,
    RegistryOptions, SameLoaderResolver, Value, build_hierarchy,
};
use std::collections::{BTreeMap, BTreeSet};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn core(name: &str, parent: Option<&str>, interfaces: &[&str], interface: bool) -> Vec<u8> {
    let mut names = vec![name];
    names.extend(parent);
    names.extend_from_slice(interfaces);
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend_from_slice(&(1 + names.len() as u16 * 2).to_be_bytes());
    for (index, name) in names.into_iter().enumerate() {
        bytes.push(1);
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(7);
        bytes.extend_from_slice(&(1 + index as u16 * 2).to_be_bytes());
    }
    bytes.extend_from_slice(&(if interface { 0x601u16 } else { 0x21u16 }).to_be_bytes());
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&(if parent.is_some() { 4u16 } else { 0u16 }).to_be_bytes());
    bytes.extend_from_slice(&(interfaces.len() as u16).to_be_bytes());
    for index in 0..interfaces.len() {
        let slot = 1 + usize::from(parent.is_some()) + index;
        bytes.extend_from_slice(&(2 + slot as u16 * 2).to_be_bytes());
    }
    bytes.extend_from_slice(&[0; 6]);
    bytes
}
fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    bytes: &[u8],
) -> ClassId {
    registry
        .define(loader, &units(name), bytes, ParseOptions::default())
        .unwrap()
}
fn field_reference(class: &ClassFile, name: &str) -> u16 {
    class
        .constant_pool
        .entries()
        .iter()
        .enumerate()
        .find_map(|(index, constant)| {
            let Constant::Fieldref {
                class_index,
                name_and_type_index,
            } = constant
            else {
                return None;
            };
            let Constant::NameAndType {
                name_index,
                descriptor_index,
            } = class.constant_pool.get(*name_and_type_index).unwrap()
            else {
                return None;
            };
            (class.constant_pool.class_name(*class_index).unwrap() == class.name().unwrap()
                && class
                    .constant_pool
                    .utf8(*name_index)
                    .unwrap()
                    .equals_ascii(name)
                && class
                    .constant_pool
                    .utf8(*descriptor_index)
                    .unwrap()
                    .equals_ascii("Ljava/lang/Throwable;"))
            .then_some(index as u16)
        })
        .unwrap()
}
fn observed(outcome: ExceptionOutcome, first: Reference, second: Reference) -> String {
    match outcome {
        ExceptionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExceptionOutcome::Uncaught(value) if value == first => "T:first".into(),
        ExceptionOutcome::Uncaught(value) if value == second => "T:second".into(),
        other => panic!("unexpected supplied-exception outcome {other:?}"),
    }
}
fn expected_state(
    name: &str,
    args: &[i32],
    is_base: bool,
    is_child: bool,
    selected: Reference,
    second: Reference,
) -> (i32, Reference) {
    let raised = |initial: i32| {
        assert!((-1..=7).contains(&args[1]));
        (initial + args[1].max(0) + 1) * 10 + 1
    };
    match name {
        "localOrder" => (
            if is_child {
                31
            } else if is_base {
                41
            } else {
                51
            },
            selected,
        ),
        "crossFrame" => (
            if is_base {
                raised(2) * 3 + 1
            } else {
                raised(2) * 5 + 2
            },
            selected,
        ),
        "nestedFinally" => {
            let state = raised(1) * 3 + 2;
            (
                if is_base {
                    state * 5 + 3
                } else {
                    state * 7 + 4
                },
                selected,
            )
        }
        "rethrow" => (
            if is_child {
                11
            } else if is_base {
                13
            } else {
                17
            },
            selected,
        ),
        "replacement" => (18, second),
        "finallyOverride" => (19, selected),
        "uncaught" => (raised(0), Reference::null()),
        "outsideRange" => (241, Reference::null()),
        "callSite" => (if is_base { 29 } else { 23 }, selected),
        "handlerCalls" => (29, selected),
        "handlerThrows" => {
            if is_base {
                (31, selected)
            } else {
                (0, Reference::null())
            }
        }
        "allCalls" => (args[1], selected),
        _ => panic!("unknown oracle method {name}"),
    }
}
#[test]
fn executes_336_supplied_exception_graphs_against_jdk21() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        &core("java/lang/Object", None, &[], false),
    );
    let cloneable = define(
        &mut registry,
        loader,
        "java/lang/Cloneable",
        &core("java/lang/Cloneable", Some("java/lang/Object"), &[], true),
    );
    let serializable = define(
        &mut registry,
        loader,
        "java/io/Serializable",
        &core("java/io/Serializable", Some("java/lang/Object"), &[], true),
    );
    let throwable = define(
        &mut registry,
        loader,
        "java/lang/Throwable",
        &core(
            "java/lang/Throwable",
            Some("java/lang/Object"),
            &["java/io/Serializable"],
            false,
        ),
    );
    define(
        &mut registry,
        loader,
        "java/lang/Exception",
        &core(
            "java/lang/Exception",
            Some("java/lang/Throwable"),
            &[],
            false,
        ),
    );
    let universe = ReferenceTypeUniverse {
        bootstrap_loader: loader,
        object,
        cloneable,
        serializable,
    };
    for (name, bytes) in [
        (
            "DispatchThrownBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchThrownBase.class")
                .as_slice(),
        ),
        (
            "DispatchThrownChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchThrownChild.class")
                .as_slice(),
        ),
        (
            "DispatchThrownOther",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchThrownOther.class")
                .as_slice(),
        ),
        (
            "DispatchThrowsContract",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchThrowsContract.class")
                .as_slice(),
        ),
        (
            "DispatchCallBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/DispatchCallBase.class")
                .as_slice(),
        ),
        (
            "ExceptionDispatchCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ExceptionDispatchCases.class")
                .as_slice(),
        ),
    ] {
        define(&mut registry, loader, &format!("fixtures/{name}"), bytes);
    }
    let class = |name: &str| {
        registry
            .lookup_class(loader, &units(&format!("fixtures/{name}")))
            .unwrap()
            .unwrap()
    };
    let owner = class("ExceptionDispatchCases");
    let base = class("DispatchThrownBase");
    let child = class("DispatchThrownChild");
    let other = class("DispatchThrownOther");
    let plans = [owner, child, other, cloneable].map(|class| {
        build_hierarchy(
            &registry,
            class,
            &mut SameLoaderResolver,
            Default::default(),
        )
        .unwrap()
    });
    let field = |name: &str, descriptor: &str| {
        registry
            .find_declared_field(owner, &units(name), &units(descriptor))
            .unwrap()
            .unwrap()
    };
    let state_field = field("state", "I");
    let first_field = field("first", "Ljava/lang/Throwable;");
    let second_field = field("second", "Ljava/lang/Throwable;");
    let saved_field = field("saved", "Ljava/lang/Throwable;");
    let definition = registry.class(owner).unwrap();
    let first_cp = field_reference(definition, "first");
    let second_cp = field_reference(definition, "second");
    let mut opcodes = BTreeSet::new();
    let mut catch_types = BTreeSet::new();
    let mut catch_all = 0;
    let mut invocation_boundary = false;
    for owner in [
        owner,
        class("DispatchCallBase"),
        class("DispatchThrowsContract"),
    ] {
        let definition = registry.class(owner).unwrap();
        for method in &definition.methods {
            let name = definition.constant_pool.utf8(method.name_index).unwrap();
            if ["<init>", "main", "emit"]
                .iter()
                .any(|n| name.equals_ascii(n))
            {
                continue;
            }
            for attribute in &method.attributes {
                let AttributeBody::Code(code) = &attribute.body else {
                    continue;
                };
                let decoded = crabspot_bytecode::decode(&code.code, 65).unwrap();
                for instruction in decoded.instructions() {
                    opcodes.insert(instruction.opcode);
                    if name.equals_ascii("callSite") && instruction.opcode == 0xb6 {
                        let crabspot_bytecode::Operands::Constant(index) = instruction.operands
                        else {
                            panic!("invoke operand");
                        };
                        let Constant::Methodref {
                            name_and_type_index,
                            ..
                        } = definition.constant_pool.get(index).unwrap()
                        else {
                            panic!("invoke constant");
                        };
                        let Constant::NameAndType { name_index, .. } =
                            definition.constant_pool.get(*name_and_type_index).unwrap()
                        else {
                            panic!("invoke name");
                        };
                        if definition
                            .constant_pool
                            .utf8(*name_index)
                            .unwrap()
                            .equals_ascii("hop")
                        {
                            assert!(!code.exception_table.is_empty());
                            for row in &code.exception_table {
                                assert!(u32::from(row.start_pc) <= instruction.offset);
                                assert_eq!(
                                    u32::from(row.end_pc),
                                    instruction.offset + instruction.length
                                );
                            }
                            invocation_boundary = true;
                        }
                    }
                }
                for handler in &code.exception_table {
                    if handler.catch_type == 0 {
                        catch_all += 1;
                    } else {
                        catch_types.insert(
                            definition
                                .constant_pool
                                .class_name(handler.catch_type)
                                .unwrap()
                                .units()
                                .to_vec(),
                        );
                    }
                }
                if name.equals_ascii("localOrder") {
                    let names: Vec<_> = code
                        .exception_table
                        .iter()
                        .map(|h| {
                            definition
                                .constant_pool
                                .class_name(h.catch_type)
                                .unwrap()
                                .units()
                                .to_vec()
                        })
                        .collect();
                    assert_eq!(
                        names,
                        [
                            units("fixtures/DispatchThrownChild"),
                            units("fixtures/DispatchThrownBase"),
                            units("java/lang/Throwable")
                        ]
                    );
                }
            }
        }
    }
    for opcode in [0xbf, 0xb6, 0xb7, 0xb8, 0xb9] {
        assert!(opcodes.contains(&opcode));
    }
    for name in [
        "java/lang/Throwable",
        "fixtures/DispatchThrownBase",
        "fixtures/DispatchThrownChild",
        "fixtures/DispatchThrownOther",
    ] {
        assert!(catch_types.contains(&units(name)));
    }
    assert!(catch_all > 0);
    assert!(invocation_boundary);
    let mut identities = BTreeSet::new();
    let mut outcomes = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/exceptions.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (kind, name) = columns[0].split_once('/').unwrap();
        let first_class = match kind {
            "base" => base,
            "child" => child,
            "other" => other,
            _ => panic!("unknown supplied type"),
        };
        let ints: Vec<i32> = columns[2]
            .split(',')
            .map(|v| v.strip_prefix("I:").unwrap().parse().unwrap())
            .collect();
        let args: Vec<_> = ints.iter().copied().map(Value::Int).collect();
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(&registry, universe).unwrap();
        let receiver = heap.allocate_instance(owner).unwrap();
        let first = heap.allocate_instance(first_class).unwrap();
        let second = heap.allocate_instance(other).unwrap();
        assert_ne!(first, second);
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        context
            .write_field(owner, first_cp, receiver, Value::Reference(first))
            .unwrap();
        context
            .write_field(owner, second_cp, receiver, Value::Reference(second))
            .unwrap();
        let profile = context.prepare_exception_profile(throwable).unwrap();
        let outcome = execute_instance_method_body_with_exceptions(
            &mut context,
            &profile,
            method,
            receiver,
            &args,
            ExecutionOptions::default(),
            CallLimits::default(),
            ExceptionLimits::default(),
        )
        .unwrap_or_else(|e| panic!("{line}: {e}"));
        let output = observed(outcome, first, second);
        assert_eq!(output, columns[3], "{line}");
        *outcomes
            .entry(if output.starts_with("I:") {
                "I".to_owned()
            } else {
                output
            })
            .or_insert(0) += 1;
        let selected = if ints[0] == 0 { first } else { second };
        let selected_base = ints[0] == 0 && kind != "other";
        let selected_child = ints[0] == 0 && kind == "child";
        let (state, saved) =
            expected_state(name, &ints, selected_base, selected_child, selected, second);
        for (field, value) in [
            (state_field, Value::Int(state)),
            (first_field, Value::Reference(first)),
            (second_field, Value::Reference(second)),
            (saved_field, Value::Reference(saved)),
        ] {
            assert_eq!(
                context.heap().get_field(receiver, field).unwrap(),
                value,
                "{line}"
            );
        }
        assert_eq!(
            context.reference_type(first).unwrap(),
            Some(ReferenceType::Class(first_class))
        );
        assert_eq!(
            context.reference_type(second).unwrap(),
            Some(ReferenceType::Class(other))
        );
        assert_eq!(
            context
                .heap()
                .children(receiver)
                .unwrap()
                .collect::<Vec<_>>(),
            [first, second, saved]
                .into_iter()
                .filter_map(Reference::heap_ref)
                .collect::<Vec<_>>(),
            "{line}"
        );
        assert_eq!(context.heap().allocated_instances(), 3, "{line}");
        assert_eq!(context.heap().allocated_arrays(), 0, "{line}");
        assert_eq!(context.heap().total_elements(), 0, "{line}");
        assert_eq!(context.heap().payload_bytes(), 0, "{line}");
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 336);
    assert_eq!(identities.len(), 36);
    assert_eq!(
        outcomes,
        BTreeMap::from([
            ("I".to_owned(), 240),
            ("T:first".to_owned(), 19),
            ("T:second".to_owned(), 77)
        ])
    );
}

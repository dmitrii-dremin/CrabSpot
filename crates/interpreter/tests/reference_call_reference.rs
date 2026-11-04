//! Independent Java 21 outcomes plus source-derived retained-state checks.
use crabspot_classfile::{AttributeBody, ParseOptions};
use crabspot_interpreter::{
    CallLimits, ExecutionOptions, ExecutionOutcome, execute_instance_method_body,
};
use crabspot_runtime::{
    ArrayElement, ClassId, DefinitionRegistry, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, InstanceOptions, LoaderId, ManagedHeap, PrimitiveArrayKind as P,
    REFERENCE_ARRAY_ELEMENT_BYTES, Reference, ReferenceType as T, ReferenceTypeUniverse,
    RegistryOptions, SameLoaderResolver, Value, build_hierarchy,
};
use std::collections::{BTreeMap, BTreeSet};
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn core(name: &str, interface: bool) -> Vec<u8> {
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend_from_slice(&(if interface { 5u16 } else { 3u16 }).to_be_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&[7, 0, 1]);
    if interface {
        bytes.extend_from_slice(&[1, 0, 16]);
        bytes.extend_from_slice(b"java/lang/Object");
        bytes.extend_from_slice(&[7, 0, 3]);
    }
    bytes.extend_from_slice(&(if interface { 0x601u16 } else { 0x21u16 }).to_be_bytes());
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&(if interface { 4u16 } else { 0u16 }).to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes
}

fn observed(result: ExecutionOutcome) -> String {
    match result {
        ExecutionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExecutionOutcome::ArrayStoreException => "E:java/lang/ArrayStoreException".into(),
        ExecutionOutcome::ArrayIndexOutOfBoundsException => {
            "E:java/lang/ArrayIndexOutOfBoundsException".into()
        }
        ExecutionOutcome::NullPointerException => "E:java/lang/NullPointerException".into(),
        other => panic!("unexpected oracle outcome {other:?}"),
    }
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
fn array_type(rank: u8, element: ArrayElement, loader: LoaderId) -> T {
    T::Array {
        dimensions: rank,
        element,
        defining_loader: loader,
    }
}
#[derive(Clone, Copy)]
enum Input {
    Null,
    Receiver,
    Ints,
    Objects,
    Matrix,
    Nominal,
    PartialMatrix,
}
fn input_kind(name: &str, args: &[i32]) -> Input {
    match name {
        "typedTrip" => match args[0] {
            0 => Input::Null,
            1 => Input::Receiver,
            2 => Input::Nominal,
            _ => Input::Matrix,
        },
        "storeTrip" => match args[0] {
            0 => Input::Objects,
            1 => Input::Nominal,
            _ => Input::PartialMatrix,
        },
        "cycleTrip" => Input::Objects,
        _ => match args[0] {
            0 => Input::Null,
            1 => Input::Receiver,
            2 => Input::Ints,
            3 => Input::Objects,
            4 => Input::Matrix,
            _ => Input::Nominal,
        },
    }
}
fn state(name: &str, args: &[i32], child: bool) -> i32 {
    match name {
        "virtualTrip" | "interfaceTrip" => args[2]
            .wrapping_add((f64::from(args[2]) + 0.75) as i32)
            .wrapping_add(args[1].wrapping_mul(7))
            .wrapping_add(if child { 100 } else { 0 }),
        "staticTrip" => args[2],
        "specialTrip" => args[1]
            .wrapping_add((f64::from(args[1]) + 0.75) as i32)
            .wrapping_add(17),
        "defaultTrip" => 0,
        "recursiveTrip" => args[1],
        _ => args[0],
    }
}
fn result_reference(
    name: &str,
    args: &[i32],
    child: bool,
    input: Reference,
    receiver: Reference,
) -> Reference {
    match name {
        "virtualTrip" | "interfaceTrip" | "staticTrip" => match args[1] {
            0 if child && name != "staticTrip" => receiver,
            0 => input,
            1 if child && name != "staticTrip" => input,
            1 => receiver,
            _ => Reference::null(),
        },
        "recursiveTrip" if args[1] > 0 && args[1] % 2 == 1 => receiver,
        "storeTrip" => receiver,
        _ => input,
    }
}
#[test]
fn executes_904_reference_call_graphs_against_jdk21() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let object = define(
        &mut registry,
        loader,
        "java/lang/Object",
        &core("java/lang/Object", false),
    );
    let cloneable = define(
        &mut registry,
        loader,
        "java/lang/Cloneable",
        &core("java/lang/Cloneable", true),
    );
    let serializable = define(
        &mut registry,
        loader,
        "java/io/Serializable",
        &core("java/io/Serializable", true),
    );
    let universe = ReferenceTypeUniverse {
        bootstrap_loader: loader,
        object,
        cloneable,
        serializable,
    };
    for (name, bytes) in [
        (
            "ReferenceCallContract",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceCallContract.class")
                .as_slice(),
        ),
        (
            "ReferenceCallBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceCallBase.class")
                .as_slice(),
        ),
        (
            "ReferenceCallCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceCallCases.class")
                .as_slice(),
        ),
        (
            "ReferenceCallChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceCallChild.class")
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
    let base = class("ReferenceCallBase");
    let owner = class("ReferenceCallCases");
    let child = class("ReferenceCallChild");
    let mut opcodes = BTreeSet::new();
    let mut signatures = BTreeSet::new();
    for owner in [base, owner, child, class("ReferenceCallContract")] {
        let definition = registry.class(owner).unwrap();
        for method in &definition.methods {
            let name = definition.constant_pool.utf8(method.name_index).unwrap();
            if ["<init>", "main", "emit", "value"]
                .iter()
                .any(|n| name.equals_ascii(n))
            {
                continue;
            }
            signatures.insert((
                name.units().to_vec(),
                definition
                    .constant_pool
                    .utf8(method.descriptor_index)
                    .unwrap()
                    .units()
                    .to_vec(),
            ));
            for attribute in &method.attributes {
                if let AttributeBody::Code(code) = &attribute.body {
                    for instruction in crabspot_bytecode::decode(&code.code, 65)
                        .unwrap()
                        .instructions()
                    {
                        opcodes.insert(instruction.opcode);
                    }
                }
            }
        }
    }
    for (name, descriptor) in [
        (
            "route",
            "(Ljava/lang/Object;JLjava/lang/Object;DI)Ljava/lang/Object;",
        ),
        (
            "choose",
            "(Ljava/lang/Object;JLjava/lang/Object;DI)Ljava/lang/Object;",
        ),
        ("special", "(Ljava/lang/Object;JD)Ljava/lang/Object;"),
        ("defaultEcho", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        ("privateEcho", "(Ljava/lang/Object;)Ljava/lang/Object;"),
        (
            "self",
            "(Lfixtures/ReferenceCallCases;)Lfixtures/ReferenceCallCases;",
        ),
        (
            "contract",
            "(Lfixtures/ReferenceCallContract;)Lfixtures/ReferenceCallContract;",
        ),
        ("objects", "([Ljava/lang/Object;)[Ljava/lang/Object;"),
        ("integers", "([[I)[[I"),
        (
            "recurse",
            "(ILjava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        ),
        (
            "store",
            "([Ljava/lang/Object;Ljava/lang/Object;I)Ljava/lang/Object;",
        ),
    ] {
        assert!(
            signatures.contains(&(units(name), units(descriptor))),
            "{name}: {descriptor}"
        );
    }
    for opcode in [0xb0, 0xb6, 0xb7, 0xb8, 0xb9] {
        assert!(opcodes.contains(&opcode));
    }
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let cloneable_plan = build_hierarchy(
        &registry,
        cloneable,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let serializable_plan = build_hierarchy(
        &registry,
        serializable,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let field = |owner, name: &str, desc: &str| {
        registry
            .find_declared_field(owner, &units(name), &units(desc))
            .unwrap()
            .unwrap()
    };
    let state_field = field(base, "state", "I");
    let saved_field = field(base, "saved", "Ljava/lang/Object;");
    let input_field = field(owner, "input", "Ljava/lang/Object;");
    let alternate_field = field(owner, "alternate", "Ljava/lang/Object;");
    let mut identities = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/reference_calls.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (receiver_name, name) = columns[0].split_once('/').unwrap();
        let is_child = receiver_name == "child";
        assert!(is_child || receiver_name == "root");
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let ints: Vec<i32> = columns[2]
            .split(',')
            .map(|v| v.strip_prefix("I:").unwrap().parse().unwrap())
            .collect();
        let args: Vec<_> = ints.iter().copied().map(Value::Int).collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan, &cloneable_plan, &serializable_plan])
            .unwrap();
        heap.bind_reference_types(&registry, universe).unwrap();
        let receiver = heap
            .allocate_instance(if is_child { child } else { owner })
            .unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let result = execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &args,
            ExecutionOptions::default(),
            CallLimits::default(),
        )
        .unwrap_or_else(|e| panic!("{line}: {e}"));
        let output = observed(result);
        assert_eq!(output, columns[3], "{line}");
        if output.starts_with("E:") {
            *exceptions.entry(output).or_insert(0) += 1;
        }
        assert_eq!(
            context.heap().get_field(receiver, state_field).unwrap(),
            Value::Int(state(name, &ints, is_child)),
            "{line}"
        );
        let read_ref = |field| match context.heap().get_field(receiver, field).unwrap() {
            Value::Reference(value) => value,
            other => panic!("unexpected field {other:?}"),
        };
        let input = read_ref(input_field);
        let saved = read_ref(saved_field);
        let alternate = read_ref(alternate_field);
        assert_eq!(
            saved,
            result_reference(name, &ints, is_child, input, receiver),
            "{line}"
        );
        assert_eq!(
            alternate,
            if name == "typedTrip" {
                Reference::null()
            } else if name == "cycleTrip" {
                input
            } else {
                receiver
            },
            "{line}"
        );
        let kind = input_kind(name, &ints);
        let (arrays, elements, bytes) = match kind {
            Input::Null => {
                assert!(input.is_null(), "{line}");
                (0, 0, 0)
            }
            Input::Receiver => {
                assert_eq!(input, receiver, "{line}");
                (0, 0, 0)
            }
            Input::Ints => {
                assert_ne!(input, receiver);
                assert_eq!(
                    context.reference_type(input).unwrap(),
                    Some(array_type(1, ArrayElement::Primitive(P::Int), loader))
                );
                assert_eq!(context.heap().length(input).unwrap(), 2);
                for i in 0..2 {
                    assert_eq!(context.heap().load(input, i).unwrap(), Value::Int(0));
                }
                assert_eq!(context.heap().children(input).unwrap().count(), 0);
                (1, 2, 8)
            }
            Input::Matrix => {
                assert_ne!(input, receiver);
                assert_eq!(
                    context.reference_type(input).unwrap(),
                    Some(array_type(2, ArrayElement::Primitive(P::Int), loader))
                );
                assert_eq!(context.heap().length(input).unwrap(), 1);
                let inner = context.heap().load_reference(input, 0).unwrap();
                assert_ne!(inner, input);
                assert_ne!(inner, receiver);
                assert_eq!(
                    context.reference_type(inner).unwrap(),
                    Some(array_type(1, ArrayElement::Primitive(P::Int), loader))
                );
                assert_eq!(context.heap().length(inner).unwrap(), 2);
                for i in 0..2 {
                    assert_eq!(context.heap().load(inner, i).unwrap(), Value::Int(0));
                }
                assert_eq!(
                    context.heap().children(input).unwrap().collect::<Vec<_>>(),
                    vec![inner.heap_ref().unwrap()]
                );
                assert_eq!(context.heap().children(inner).unwrap().count(), 0);
                (2, 3, REFERENCE_ARRAY_ELEMENT_BYTES + 8)
            }
            Input::Objects | Input::Nominal | Input::PartialMatrix => {
                let ty = match kind {
                    Input::Objects => array_type(1, ArrayElement::Class(object), loader),
                    Input::Nominal => array_type(1, ArrayElement::Class(owner), loader),
                    _ => array_type(2, ArrayElement::Primitive(P::Int), loader),
                };
                assert_ne!(input, receiver);
                assert_eq!(context.reference_type(input).unwrap(), Some(ty));
                assert_eq!(context.heap().length(input).unwrap(), 2);
                let mut children = Vec::new();
                for i in 0..2 {
                    let value = if name == "cycleTrip" && ints[0] == i {
                        input
                    } else if name == "storeTrip" && ints[0] < 2 && ints[1] == i {
                        receiver
                    } else {
                        Reference::null()
                    };
                    assert_eq!(
                        context.heap().load_reference(input, i).unwrap(),
                        value,
                        "{line}"
                    );
                    if let Some(reference) = value.heap_ref() {
                        children.push(reference);
                    }
                }
                assert_eq!(
                    context.heap().children(input).unwrap().collect::<Vec<_>>(),
                    children,
                    "{line}"
                );
                (1, 2, REFERENCE_ARRAY_ELEMENT_BYTES * 2)
            }
        };
        assert_eq!(context.heap().allocated_instances(), 1);
        assert_eq!(context.heap().allocated_arrays(), arrays, "{line}");
        assert_eq!(context.heap().total_elements(), elements, "{line}");
        assert_eq!(context.heap().payload_bytes(), bytes, "{line}");
        assert_eq!(
            context
                .heap()
                .children(receiver)
                .unwrap()
                .collect::<Vec<_>>(),
            [saved, input, alternate]
                .into_iter()
                .filter_map(Reference::heap_ref)
                .collect::<Vec<_>>(),
            "{line}"
        );
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 904);
    assert_eq!(identities.len(), 20);
    for (kind, count) in [
        ("NullPointerException", 12),
        ("ArrayIndexOutOfBoundsException", 16),
        ("ArrayStoreException", 4),
    ] {
        assert_eq!(exceptions.get(&format!("E:java/lang/{kind}")), Some(&count));
    }
    assert_eq!(exceptions.len(), 3);
}

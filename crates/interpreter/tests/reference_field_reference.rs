//! JDK21 results for checked non-null instance-field body graphs.
//! Host-supplied default receivers bypass constructors. State/edge assertions
//! derive independently from the fixture source, not recorded JDK heap snapshots.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionOptions, ExecutionOutcome, execute_instance_method_body,
};
use crabspot_runtime::{
    DefinitionRegistry, FieldResolutionContext, FieldResolutionOptions, HeapOptions,
    InstanceOptions, ManagedHeap, PrimitiveArrayKind, Reference, ReferenceTypeUniverse,
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
        ExecutionOutcome::ClassCastException => "E:java/lang/ClassCastException".to_owned(),
        ExecutionOutcome::NegativeArraySizeException => {
            "E:java/lang/NegativeArraySizeException".to_owned()
        }
        ExecutionOutcome::ArrayIndexOutOfBoundsException => {
            "E:java/lang/ArrayIndexOutOfBoundsException".to_owned()
        }
        ExecutionOutcome::ArithmeticException => "E:java/lang/ArithmeticException".to_owned(),
        ExecutionOutcome::NullPointerException => "E:java/lang/NullPointerException".to_owned(),
        other => panic!("unexpected oracle outcome {other:?}"),
    }
}
#[derive(Clone, Copy)]
enum Stored {
    Null,
    Receiver,
    Array,
}
#[test]
fn executes_690_reference_field_body_graphs_against_jdk21() {
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    let mut define_core = |name, interface| {
        registry
            .define(
                loader,
                &units(name),
                &core(name, interface),
                ParseOptions::default(),
            )
            .unwrap()
    };
    let object = define_core("java/lang/Object", false);
    let cloneable = define_core("java/lang/Cloneable", true);
    let serializable = define_core("java/io/Serializable", true);
    let universe = ReferenceTypeUniverse {
        bootstrap_loader: loader,
        object,
        cloneable,
        serializable,
    };
    for (name, bytes) in [
        (
            "fixtures/ReferenceFieldBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceFieldBase.class")
                .as_slice(),
        ),
        (
            "fixtures/ReferenceFieldCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceFieldCases.class")
                .as_slice(),
        ),
        (
            "fixtures/ReferenceFieldChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceFieldChild.class")
                .as_slice(),
        ),
        (
            "fixtures/ReferenceFieldMarker",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceFieldMarker.class")
                .as_slice(),
        ),
        (
            "referencep/ReferenceOwner",
            include_bytes!("../../../tests/fixtures/classes/referencep/ReferenceOwner.class")
                .as_slice(),
        ),
        (
            "referencep/HiddenReferenceBase",
            include_bytes!("../../../tests/fixtures/classes/referencep/HiddenReferenceBase.class")
                .as_slice(),
        ),
    ] {
        let id = registry
            .define(loader, &units(name), bytes, ParseOptions::default())
            .unwrap();
        assert_eq!(registry.class(id).unwrap().version.major, 65);
    }
    let class = |name: &str| {
        registry
            .lookup_class(loader, &units(name))
            .unwrap()
            .unwrap()
    };
    let owner = class("fixtures/ReferenceFieldCases");
    let child = class("fixtures/ReferenceFieldChild");
    let base = class("fixtures/ReferenceFieldBase");
    let hidden_owner = class("referencep/ReferenceOwner");
    let hidden_type = class("referencep/HiddenReferenceBase");
    assert_eq!(registry.class(hidden_type).unwrap().access_flags & 1, 0);
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let field = |owner, name: &str, descriptor: &str| {
        registry
            .find_declared_field(owner, &units(name), &units(descriptor))
            .unwrap()
            .unwrap()
    };
    let state = field(owner, "state", "I");
    let touched = field(base, "touched", "I");
    // Independent declaration order, with inherited slots first. Numeric slots
    // are absent from this expected child sequence.
    let fields = [
        (
            "hidden",
            field(hidden_owner, "hidden", "Lreferencep/HiddenReferenceBase;"),
        ),
        (
            "base_link",
            field(base, "link", "Lfixtures/ReferenceFieldBase;"),
        ),
        ("payload", field(base, "payload", "Ljava/lang/Object;")),
        (
            "link",
            field(owner, "link", "Lfixtures/ReferenceFieldCases;"),
        ),
        ("object", field(owner, "object", "Ljava/lang/Object;")),
        (
            "marker",
            field(owner, "marker", "Lfixtures/ReferenceFieldMarker;"),
        ),
        (
            "cloneable",
            field(owner, "cloneable", "Ljava/lang/Cloneable;"),
        ),
        (
            "serializable",
            field(owner, "serializable", "Ljava/io/Serializable;"),
        ),
        ("booleans", field(owner, "booleans", "[Z")),
        ("bytes", field(owner, "bytes", "[B")),
        ("chars", field(owner, "chars", "[C")),
        ("shorts", field(owner, "shorts", "[S")),
        ("ints", field(owner, "ints", "[I")),
        ("longs", field(owner, "longs", "[J")),
        ("floats", field(owner, "floats", "[F")),
        ("doubles", field(owner, "doubles", "[D")),
    ];
    let arrays = [
        ("booleans", PrimitiveArrayKind::Boolean),
        ("bytes", PrimitiveArrayKind::Byte),
        ("chars", PrimitiveArrayKind::Char),
        ("shorts", PrimitiveArrayKind::Short),
        ("ints", PrimitiveArrayKind::Int),
        ("longs", PrimitiveArrayKind::Long),
        ("floats", PrimitiveArrayKind::Float),
        ("doubles", PrimitiveArrayKind::Double),
    ];
    let mut identities = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/reference_fields.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (receiver_name, name) = columns[0].split_once('/').unwrap();
        let actual = match receiver_name {
            "root" => owner,
            "child" => child,
            _ => panic!("unknown receiver"),
        };
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let integers: Vec<i32> = columns[2]
            .split(',')
            .map(|text| text.strip_prefix("I:").unwrap().parse().unwrap())
            .collect();
        let args: Vec<_> = integers.iter().copied().map(Value::Int).collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        heap.bind_reference_types(&registry, universe).unwrap();
        let receiver = heap.allocate_instance(actual).unwrap();
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
        .unwrap_or_else(|error| panic!("{line}: {error}"));
        let output = observed(result);
        assert_eq!(output, columns[3], "{line}");
        if output.starts_with("E:") {
            *exceptions.entry(output).or_insert(0) += 1;
        }
        assert_eq!(
            context.heap().get_field(receiver, state).unwrap(),
            args[0],
            "{line}"
        );
        let expected_touched = match name {
            "baseSelf" | "hiddenLinks" => integers[0].wrapping_add(1),
            "specialChain" => integers[0],
            _ => 0,
        };
        assert_eq!(
            context.heap().get_field(receiver, touched).unwrap(),
            Value::Int(expected_touched),
            "{line}"
        );
        let mut expected = BTreeMap::new();
        let mut assign = |labels: &[&str], value| {
            for label in labels {
                expected.insert((*label).to_owned(), value);
            }
        };
        let mut allocated = false;
        let mut array_label = None;
        let mut array_kind = PrimitiveArrayKind::Int;
        match name {
            "self" | "storedVirtual" | "specialChain" => assign(&["link"], Stored::Receiver),
            "aliases" => assign(
                &["link", "object", "marker", "serializable"],
                Stored::Receiver,
            ),
            "baseSelf" => assign(&["base_link"], Stored::Receiver),
            "hiddenLinks" | "cycle" => assign(&["base_link", "link"], Stored::Receiver),
            "clear" | "nullAfterStore" | "clearHidden" => assign(&["object"], Stored::Receiver),
            "replaceWithNull" => {}
            "storedInterface" => assign(&["marker"], Stored::Receiver),
            "castAfterStore" => {
                assign(&["link", "object"], Stored::Receiver);
                if receiver_name == "child" {
                    assign(&["cloneable"], Stored::Receiver);
                }
            }
            "divideAfterStore" => assign(&["link", "object"], Stored::Receiver),
            "hiddenType" => assign(&["hidden", "object"], Stored::Receiver),
            "overwrite" | "arrayAfterStore" | "replaceArrayWithSelf" => {
                if name == "overwrite" {
                    assign(&["object"], Stored::Receiver);
                }
                if name == "arrayAfterStore" {
                    assign(&["link"], Stored::Receiver);
                }
                if integers[0] >= 0 {
                    allocated = true;
                    array_label = Some("ints");
                    assign(&["ints"], Stored::Array);
                    assign(
                        &["object"],
                        if name == "replaceArrayWithSelf" {
                            Stored::Receiver
                        } else {
                            Stored::Array
                        },
                    );
                }
            }
            "storeArray" | "arrayClear" | "arrayAlias" => {
                if integers[1] >= 0 {
                    allocated = true;
                    if name != "arrayClear" {
                        let (label, kind) = arrays[integers[0] as usize];
                        array_label = Some(label);
                        array_kind = kind;
                        assign(&[label, "cloneable", "serializable"], Stored::Array);
                        if name == "storeArray" {
                            assign(&["object"], Stored::Array);
                        }
                    }
                }
            }
            _ => panic!("unknown fixture method {name}"),
        }
        let array_reference = array_label.map(|label| {
            let id = fields.iter().find(|(name, _)| *name == label).unwrap().1;
            let Value::Reference(reference) = context.heap().get_field(receiver, id).unwrap()
            else {
                panic!("array field kind")
            };
            assert!(!reference.is_null(), "{line}");
            assert_ne!(reference, receiver, "{line}");
            assert_eq!(
                context.heap().kind(reference).unwrap(),
                array_kind,
                "{line}"
            );
            assert_eq!(
                context.heap().length(reference).unwrap(),
                *integers.last().unwrap(),
                "{line}"
            );
            assert_eq!(
                context.heap().children(reference).unwrap().count(),
                0,
                "{line}"
            );
            reference
        });
        let mut children = Vec::new();
        for (label, field) in fields {
            let reference = match expected.get(label).copied().unwrap_or(Stored::Null) {
                Stored::Null => Reference::null(),
                Stored::Receiver => receiver,
                Stored::Array => array_reference.unwrap(),
            };
            assert_eq!(
                context.heap().get_field(receiver, field).unwrap(),
                Value::Reference(reference),
                "{line}: {label}"
            );
            if let Some(handle) = reference.heap_ref() {
                children.push(handle);
            }
        }
        assert_eq!(
            context
                .heap()
                .children(receiver)
                .unwrap()
                .collect::<Vec<_>>(),
            children,
            "{line}"
        );
        assert_eq!(
            context.heap().allocated_arrays(),
            usize::from(allocated),
            "{line}"
        );
        assert_eq!(context.heap().allocated_instances(), 1, "{line}");
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 690);
    assert_eq!(identities.len(), 42);
    for (kind, count) in [
        ("ClassCastException", 14),
        ("NegativeArraySizeException", 54),
        ("NullPointerException", 28),
        ("ArithmeticException", 2),
        ("ArrayIndexOutOfBoundsException", 2),
    ] {
        assert_eq!(exceptions.get(&format!("E:java/lang/{kind}")), Some(&count));
    }
    assert_eq!(exceptions.len(), 5);
}

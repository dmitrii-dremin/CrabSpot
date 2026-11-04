//! Original JDK reset-state outcomes and independent retained static-memory checks.
//! This profile does not execute initialization or constructors.
use crabspot_classfile::{AttributeBody, ClassFile, Constant, ParseOptions};
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome, ExecutionOptions, ExecutionOutcome,
    execute_instance_method_body_with_static_state,
    execute_instance_method_body_with_static_state_and_exceptions,
};
use crabspot_runtime::{
    ArrayElement, ClassId, DefinitionRegistry, FieldId, FieldResolutionContext,
    FieldResolutionOptions, HeapOptions, InstanceOptions, LoaderId, ManagedHeap,
    PrimitiveArrayKind, REFERENCE_ARRAY_ELEMENT_BYTES, Reference, ReferenceType,
    ReferenceTypeUniverse, RegistryOptions, STATIC_STATE_SLOT_BYTES, SameLoaderResolver,
    StaticStateOptions, Value, build_hierarchy,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
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

fn input(text: &str) -> Value {
    let (kind, value) = text.split_once(':').unwrap();
    match kind {
        "I" => Value::Int(value.parse().unwrap()),
        "L" => Value::Long(value.parse().unwrap()),
        "F" => Value::Float(u32::from_str_radix(value, 16).unwrap()),
        "D" => Value::Double(u64::from_str_radix(value, 16).unwrap()),
        _ => panic!("unexpected input {text}"),
    }
}
fn returned(value: Option<Value>) -> String {
    match value.unwrap() {
        Value::Int(v) => format!("I:{v}"),
        Value::Long(v) => format!("L:{v}"),
        Value::Float(v) if f32::from_bits(v).is_nan() => "F:NaN".into(),
        Value::Float(v) => format!("F:{v:08x}"),
        Value::Double(v) if f64::from_bits(v).is_nan() => "D:NaN".into(),
        Value::Double(v) => format!("D:{v:016x}"),
        other => panic!("unexpected result {other:?}"),
    }
}
fn field(registry: &DefinitionRegistry, owner: ClassId, name: &str, descriptor: &str) -> FieldId {
    registry
        .find_declared_field(owner, &units(name), &units(descriptor))
        .unwrap()
        .unwrap()
}
fn reference(value: Value) -> Reference {
    let Value::Reference(reference) = value else {
        panic!("expected reference");
    };
    reference
}
#[test]
fn genuine_static_bodies_match_jdk_and_preserve_checked_locations() {
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
            "StaticBodyBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/StaticBodyBase.class")
                .as_slice(),
        ),
        (
            "StaticBodyContract",
            include_bytes!("../../../tests/fixtures/classes/fixtures/StaticBodyContract.class")
                .as_slice(),
        ),
        (
            "StaticBodyCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/StaticBodyCases.class")
                .as_slice(),
        ),
        (
            "StaticBodyChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/StaticBodyChild.class")
                .as_slice(),
        ),
        (
            "StaticBodyThrown",
            include_bytes!("../../../tests/fixtures/classes/fixtures/StaticBodyThrown.class")
                .as_slice(),
        ),
        (
            "StaticUnapplied",
            include_bytes!("../../../tests/fixtures/classes/fixtures/StaticUnapplied.class")
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
    let owner = class("StaticBodyCases");
    let base = class("StaticBodyBase");
    let child = class("StaticBodyChild");
    let thrown = class("StaticBodyThrown");
    let unapplied = class("StaticUnapplied");
    let plans = [child, thrown, unapplied].map(|id| {
        build_hierarchy(&registry, id, &mut SameLoaderResolver, Default::default()).unwrap()
    });
    let definition = registry.class(owner).unwrap();
    let first_cp = field_reference(definition, "first");
    let second_cp = field_reference(definition, "second");
    let first_field = field(&registry, owner, "first", "Ljava/lang/Throwable;");
    let second_field = field(&registry, owner, "second", "Ljava/lang/Throwable;");
    let descriptors = [
        ("flag", "Z"),
        ("tiny", "B"),
        ("small", "S"),
        ("letter", "C"),
        ("number", "I"),
        ("wide", "J"),
        ("single", "F"),
        ("precise", "D"),
        ("hidden", "I"),
        ("object", "Ljava/lang/Object;"),
        ("link", "Lfixtures/StaticBodyCases;"),
        ("contract", "Lfixtures/StaticBodyContract;"),
        ("cloneable", "Ljava/lang/Cloneable;"),
        ("serializable", "Ljava/io/Serializable;"),
        ("ints", "[I"),
        ("objects", "[Ljava/lang/Object;"),
        ("pending", "Ljava/lang/Throwable;"),
        ("saved", "Ljava/lang/Throwable;"),
    ];
    let fields: BTreeMap<_, _> = descriptors
        .iter()
        .map(|(name, descriptor)| (*name, field(&registry, owner, name, descriptor)))
        .collect();
    let base_hidden = field(&registry, base, "hidden", "I");
    let base_object = field(&registry, base, "baseObject", "Ljava/lang/Object;");
    let unapplied_fields = [
        field(&registry, unapplied, "CONSTANT", "I"),
        field(&registry, unapplied, "TEXT", "Ljava/lang/String;"),
        field(&registry, unapplied, "initialized", "I"),
        field(&registry, unapplied, "fresh", "Ljava/lang/Object;"),
    ];
    let mut order = vec![base_hidden, base_object];
    order.extend(descriptors.iter().map(|(name, _)| fields[name]));
    order.extend(unapplied_fields);
    assert_eq!(order.len(), 24);
    let unapplied_metadata = registry.class(unapplied).unwrap();
    assert_eq!(
        unapplied_metadata
            .fields
            .iter()
            .flat_map(|f| &f.attributes)
            .filter(|a| matches!(a.body, AttributeBody::ConstantValue(_)))
            .count(),
        2
    );
    assert!(
        registry
            .find_declared_method(unapplied, &units("<clinit>"), &units("()V"))
            .unwrap()
            .is_some()
    );
    let mut opcodes = BTreeSet::new();
    for class in [owner, base, child, class("StaticBodyContract")] {
        let metadata = registry.class(class).unwrap();
        for method in &metadata.methods {
            let name = metadata.constant_pool.utf8(method.name_index).unwrap();
            if ["<init>", "main", "emit", "reset", "value"]
                .iter()
                .any(|n| name.equals_ascii(n))
            {
                continue;
            }
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
    for opcode in [0xb2, 0xb3, 0xb6, 0xb7, 0xb8, 0xb9, 0xbf] {
        assert!(opcodes.contains(&opcode));
    }
    let mut identities = BTreeSet::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/static_bodies.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (kind, name) = columns[0].split_once('/').unwrap();
        let receiver_class = match kind {
            "root" => owner,
            "child" => child,
            _ => panic!("receiver kind"),
        };
        let argument = input(columns[2]);
        let integer = if let Value::Int(v) = argument { v } else { 0 };
        let method = registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plans[0], &plans[1]])
            .unwrap();
        heap.bind_reference_types(&registry, universe).unwrap();
        let receiver = heap.allocate_instance(receiver_class).unwrap();
        let first = heap.allocate_instance(thrown).unwrap();
        let second = heap.allocate_instance(thrown).unwrap();
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
        let statics = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        assert_eq!(statics.class_count(), 10);
        assert_eq!(statics.slot_count(), 20);
        assert_eq!(statics.payload_bytes(), 20 * STATIC_STATE_SLOT_BYTES);
        assert_eq!(context.static_fields(&statics).unwrap(), &order[..20]);
        assert_eq!(context.static_roots(&statics).unwrap().count(), 0);
        let output = if matches!(name, "caught" | "uncaught" | "replacement") {
            let exceptions = context.prepare_exception_profile(throwable).unwrap();
            match execute_instance_method_body_with_static_state_and_exceptions(
                &mut context,
                &statics,
                &exceptions,
                method,
                receiver,
                &[argument],
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits::default(),
            )
            .unwrap_or_else(|e| panic!("{line}: {e}"))
            {
                ExceptionOutcome::Returned(value) => returned(value),
                ExceptionOutcome::Uncaught(value) if value == first => "T:first".into(),
                ExceptionOutcome::Uncaught(value) if value == second => "T:second".into(),
                other => panic!("unexpected {other:?}: {line}"),
            }
        } else {
            match execute_instance_method_body_with_static_state(
                &mut context,
                &statics,
                method,
                receiver,
                &[argument],
                ExecutionOptions::default(),
                CallLimits::default(),
            )
            .unwrap_or_else(|e| panic!("{line}: {e}"))
            {
                ExecutionOutcome::Returned(value) => returned(value),
                ExecutionOutcome::ArithmeticException => "E:java/lang/ArithmeticException".into(),
                ExecutionOutcome::NegativeArraySizeException => {
                    "E:java/lang/NegativeArraySizeException".into()
                }
                other => panic!("unexpected {other:?}: {line}"),
            }
        };
        assert_eq!(output, columns[3], "{line}");
        let mut expected = HashMap::new();
        for (name, descriptor) in descriptors {
            let value = match descriptor {
                "J" => Value::Long(0),
                "F" => Value::Float(0),
                "D" => Value::Double(0),
                d if d.starts_with(['L', '[']) => Value::Reference(Reference::null()),
                _ => Value::Int(0),
            };
            expected.insert(fields[name], value);
        }
        expected.insert(base_hidden, Value::Int(0));
        expected.insert(base_object, Value::Reference(Reference::null()));
        for (id, value) in unapplied_fields.into_iter().zip([
            Value::Int(0),
            Value::Reference(Reference::null()),
            Value::Int(0),
            Value::Reference(Reference::null()),
        ]) {
            expected.insert(id, value);
        }
        let mut set = |name: &str, value| {
            expected.insert(fields[name], value);
        };
        match name {
            "integral" => {
                set("flag", Value::Int(integer & 1));
                set("tiny", Value::Int(i32::from(integer as i8)));
                set("small", Value::Int(i32::from(integer as i16)));
                set("letter", Value::Int(i32::from(integer as u16)));
                set("number", argument);
            }
            "longStore" => set("wide", argument),
            "floatStore" => set("single", argument),
            "doubleStore" => set("precise", argument),
            "hiddenSlots" => {
                set("hidden", Value::Int(integer.wrapping_add(1)));
                expected.insert(base_hidden, argument);
            }
            "self" => {
                set("number", argument);
                set("object", Value::Reference(receiver));
                set("link", Value::Reference(receiver));
            }
            "aliases" => {
                set("number", argument);
                for name in ["object", "link", "contract", "cloneable", "serializable"] {
                    set(name, Value::Reference(receiver));
                }
                expected.insert(base_object, Value::Reference(receiver));
            }
            "clear" | "divideAfterStore" => {
                set("number", argument);
                set("object", Value::Reference(receiver));
            }
            "special" => {
                set("number", Value::Int(integer.wrapping_add(1)));
                expected.insert(base_hidden, argument);
            }
            "recursion" => set("number", Value::Int(8 + integer.max(0))),
            "mixedCall" => {
                set("wide", Value::Long(i64::from(integer)));
                set("precise", Value::from_f64(f64::from(integer)));
                set("link", Value::Reference(receiver));
            }
            "overwrite" | "arrays" => {
                set("number", argument);
                if integer < 0 {
                    if name == "overwrite" {
                        set("object", Value::Reference(receiver));
                    }
                } else {
                    let ints = reference(context.inspect_static(&statics, fields["ints"]).unwrap());
                    assert_ne!(ints, receiver);
                    assert!(!ints.is_null());
                    assert_eq!(
                        context.reference_type(ints).unwrap(),
                        Some(ReferenceType::Array {
                            dimensions: 1,
                            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
                            defining_loader: loader
                        })
                    );
                    assert_eq!(context.heap().length(ints).unwrap(), integer);
                    for i in 0..integer {
                        assert_eq!(
                            context.heap().load(ints, i).unwrap(),
                            Value::Int(if name == "arrays" && i == 0 {
                                integer + 3
                            } else {
                                0
                            })
                        );
                    }
                    assert_eq!(context.heap().children(ints).unwrap().count(), 0);
                    set("ints", Value::Reference(ints));
                    set("object", Value::Reference(ints));
                    if name == "arrays" {
                        set("cloneable", Value::Reference(ints));
                        set("serializable", Value::Reference(ints));
                        let objects =
                            reference(context.inspect_static(&statics, fields["objects"]).unwrap());
                        assert_ne!(objects, ints);
                        assert_ne!(objects, receiver);
                        assert!(!objects.is_null());
                        assert_eq!(
                            context.reference_type(objects).unwrap(),
                            Some(ReferenceType::Array {
                                dimensions: 1,
                                element: ArrayElement::Class(owner),
                                defining_loader: loader
                            })
                        );
                        assert_eq!(context.heap().length(objects).unwrap(), integer);
                        for i in 0..integer {
                            assert_eq!(
                                context.heap().load_reference(objects, i).unwrap(),
                                if i == 0 { receiver } else { Reference::null() }
                            );
                        }
                        assert_eq!(
                            context
                                .heap()
                                .children(objects)
                                .unwrap()
                                .collect::<Vec<_>>(),
                            if integer > 0 {
                                vec![receiver.heap_ref().unwrap()]
                            } else {
                                vec![]
                            }
                        );
                        set("objects", Value::Reference(objects));
                    }
                }
            }
            "caught" | "uncaught" | "replacement" => {
                let selected = if integer == 0 { first } else { second };
                set(
                    "number",
                    Value::Int(match name {
                        "caught" => 15,
                        "uncaught" => 20,
                        _ => 37,
                    }),
                );
                set(
                    "pending",
                    Value::Reference(if name == "replacement" {
                        second
                    } else {
                        selected
                    }),
                );
                if name != "uncaught" {
                    set("saved", Value::Reference(selected));
                }
            }
            _ => panic!("unmodeled case {name}"),
        }
        for id in &order[..20] {
            assert_eq!(
                context.inspect_static(&statics, *id).unwrap(),
                expected[id],
                "{line}: {id:?}"
            );
        }
        let roots: Vec<_> = order
            .iter()
            .filter_map(|id| {
                if let Value::Reference(r) = expected[id] {
                    r.heap_ref()
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            context.static_roots(&statics).unwrap().collect::<Vec<_>>(),
            roots,
            "{line}"
        );
        let steps = context.lookup_steps();
        assert_eq!(
            context
                .allocate_static_state(StaticStateOptions::default())
                .unwrap(),
            statics
        );
        assert_eq!(context.lookup_steps(), steps);
        assert_eq!(
            context.static_roots(&statics).unwrap().collect::<Vec<_>>(),
            roots
        );
        for id in &order[..20] {
            assert_eq!(context.inspect_static(&statics, *id).unwrap(), expected[id]);
        }
        assert_eq!(
            context.heap().get_field(receiver, first_field).unwrap(),
            Value::Reference(first)
        );
        assert_eq!(
            context.heap().get_field(receiver, second_field).unwrap(),
            Value::Reference(second)
        );
        assert_eq!(
            context
                .heap()
                .children(receiver)
                .unwrap()
                .collect::<Vec<_>>(),
            vec![first.heap_ref().unwrap(), second.heap_ref().unwrap()]
        );
        assert_eq!(context.heap().allocated_instances(), 3);
        let has_arrays = integer >= 0 && matches!(name, "arrays" | "overwrite");
        let arrays = if has_arrays {
            if name == "arrays" { 2 } else { 1 }
        } else {
            0
        };
        let length = if has_arrays { integer as usize } else { 0 };
        let bytes = if name == "arrays" {
            length * (4 + REFERENCE_ARRAY_ELEMENT_BYTES)
        } else {
            length * 4
        };
        assert_eq!(context.heap().allocated_arrays(), arrays, "{line}");
        assert_eq!(context.heap().total_elements(), length * arrays, "{line}");
        assert_eq!(context.heap().payload_bytes(), bytes, "{line}");
        // Extend genuine metadata after completed effects, without resetting old storage.
        context
            .heap_mut()
            .admit_instance_layouts(&registry, &[&plans[2]])
            .unwrap();
        assert!(matches!(
            context.check_static_profile(&statics),
            Err(crabspot_runtime::StaticAccessError::Context(
                crabspot_runtime::FieldAccessError::ContextChanged
            ))
        ));
        drop(context);
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert!(matches!(
            context.check_static_profile(&statics),
            Err(crabspot_runtime::StaticAccessError::ProfileMismatch)
        ));
        let extended = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        assert_eq!(extended.class_count(), 11);
        assert_eq!(extended.slot_count(), 24);
        assert_eq!(extended.payload_bytes(), 24 * STATIC_STATE_SLOT_BYTES);
        assert_eq!(context.static_fields(&extended).unwrap(), order);
        for id in &order {
            assert_eq!(
                context.inspect_static(&extended, *id).unwrap(),
                expected[id],
                "extension: {line}"
            );
        }
        assert_eq!(
            context.static_roots(&extended).unwrap().collect::<Vec<_>>(),
            roots,
            "extension: {line}"
        );
        assert_eq!(context.heap().allocated_instances(), 3);
        assert_eq!(context.heap().allocated_arrays(), arrays);
        assert_eq!(context.heap().total_elements(), length * arrays);
        assert_eq!(context.heap().payload_bytes(), bytes);
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 364);
    assert_eq!(identities.len(), 34);
}

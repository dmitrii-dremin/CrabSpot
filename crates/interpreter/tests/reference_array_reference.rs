//! Java 21 reference-array outcomes with source-derived heap graph checks.
//! Host receiver storage is default-only; no constructor or multianewarray runs.
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
        ExecutionOutcome::ArrayStoreException => "E:java/lang/ArrayStoreException".to_owned(),
        ExecutionOutcome::NegativeArraySizeException => {
            "E:java/lang/NegativeArraySizeException".to_owned()
        }
        ExecutionOutcome::ArrayIndexOutOfBoundsException => {
            "E:java/lang/ArrayIndexOutOfBoundsException".to_owned()
        }
        ExecutionOutcome::NullPointerException => "E:java/lang/NullPointerException".to_owned(),
        other => panic!("unexpected oracle outcome {other:?}"),
    }
}
const PRIMITIVES: [P; 8] = [
    P::Boolean,
    P::Byte,
    P::Char,
    P::Short,
    P::Int,
    P::Long,
    P::Float,
    P::Double,
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Node {
    Receiver,
    Array(usize),
}
struct ExpectedArray {
    ty: T,
    primitive: Option<P>,
    length: i32,
    elements: Vec<Option<Node>>,
}
#[derive(Default)]
struct Graph {
    saved: Option<Node>,
    item: Option<Node>,
    auxiliary: Option<Node>,
    arrays: Vec<ExpectedArray>,
}
impl Graph {
    fn allocate(&mut self, ty: T, primitive: Option<P>, length: i32) -> Option<Node> {
        if length < 0 {
            return None;
        }
        let node = Node::Array(self.arrays.len());
        self.arrays.push(ExpectedArray {
            ty,
            primitive,
            length,
            elements: if primitive.is_some() {
                Vec::new()
            } else {
                vec![None; length as usize]
            },
        });
        Some(node)
    }
    fn store(&mut self, array: Node, index: i32, value: Option<Node>, compatible: bool) -> bool {
        let Node::Array(id) = array else {
            panic!("expected array")
        };
        if index < 0 || index >= self.arrays[id].length || !compatible {
            return false;
        }
        self.arrays[id].elements[index as usize] = value;
        true
    }
}
fn nominal(rank: u8, class: ClassId, loader: LoaderId) -> T {
    T::Array {
        dimensions: rank,
        element: ArrayElement::Class(class),
        defining_loader: loader,
    }
}
fn primitive(rank: u8, kind: P, loader: LoaderId) -> T {
    T::Array {
        dimensions: rank,
        element: ArrayElement::Primitive(kind),
        defining_loader: loader,
    }
}
// This model follows the fixture's explicit allocation/write sequence. It does
// not calculate return values or implement general JVM assignment: those results
// come independently from the JDK, and compatibility here is a fixture-specific table.
fn graph(name: &str, a: &[i32], child: bool, classes: &[ClassId; 9], loader: LoaderId) -> Graph {
    let mut g = Graph::default();
    match name {
        "allocateNominal" | "storeSelf" | "storeNull" | "storePrimitive" | "storeArray" => {
            let kind = a[0] as usize;
            let length = if name == "storePrimitive" { a[2] } else { a[1] };
            let Some(array) = g.allocate(nominal(1, classes[kind], loader), None, length) else {
                return g;
            };
            g.saved = Some(array);
            if name == "allocateNominal" {
                return g;
            }
            let index = if name == "storePrimitive" { a[3] } else { a[2] };
            let (value, compatible) = match name {
                "storeNull" => (None, true),
                "storeSelf" => (
                    Some(Node::Receiver),
                    matches!(kind, 0 | 1 | 2 | 4 | 8) || (child && matches!(kind, 3 | 5 | 7)),
                ),
                "storePrimitive" => {
                    let kind_value = PRIMITIVES[a[1] as usize];
                    let Some(value) =
                        g.allocate(primitive(1, kind_value, loader), Some(kind_value), a[4])
                    else {
                        return g;
                    };
                    g.auxiliary = Some(value);
                    (Some(value), matches!(kind, 0 | 7 | 8))
                }
                "storeArray" => {
                    let Some(value) = g.allocate(nominal(1, classes[0], loader), None, a[3]) else {
                        return g;
                    };
                    g.auxiliary = Some(value);
                    (Some(value), matches!(kind, 0 | 7 | 8))
                }
                _ => unreachable!(),
            };
            if g.store(array, index, value, compatible) {
                g.item = value;
            }
        }
        "allocateNested" | "nestedPrimitive" => {
            let kind = PRIMITIVES[a[0] as usize];
            let length = if name == "allocateNested" { a[1] } else { a[2] };
            let Some(array) = g.allocate(primitive(2, kind, loader), None, length) else {
                return g;
            };
            g.saved = Some(array);
            if name == "nestedPrimitive" {
                let actual = PRIMITIVES[a[1] as usize];
                let Some(value) = g.allocate(primitive(1, actual, loader), Some(actual), a[4])
                else {
                    return g;
                };
                g.auxiliary = Some(value);
                if g.store(array, a[3], Some(value), kind == actual) {
                    g.item = Some(value);
                }
            }
        }
        "nullLoad" => {}
        "nullStore" => {
            g.auxiliary = g.allocate(primitive(1, P::Int, loader), Some(P::Int), 1);
        }
        "negativeAfterStore" => {
            let array = g.allocate(nominal(1, classes[0], loader), None, 1).unwrap();
            g.saved = Some(array);
            assert!(g.store(array, 0, Some(Node::Receiver), true));
            g.item = Some(Node::Receiver);
            g.auxiliary = g.allocate(nominal(1, classes[0], loader), None, a[0]);
        }
        "nestedObjects" | "primitiveIntoObjects" | "deepArray" => {
            let ty = if name == "deepArray" {
                primitive(3, P::Int, loader)
            } else {
                nominal(2, classes[0], loader)
            };
            let Some(array) = g.allocate(ty, None, a[0]) else {
                return g;
            };
            g.saved = Some(array);
            let (ty, kind) = match name {
                "deepArray" => (primitive(2, P::Int, loader), None),
                "primitiveIntoObjects" => (primitive(1, P::Int, loader), Some(P::Int)),
                _ => (nominal(1, classes[0], loader), None),
            };
            let Some(value) = g.allocate(ty, kind, a[2]) else {
                return g;
            };
            g.auxiliary = Some(value);
            if g.store(array, a[1], Some(value), name != "primitiveIntoObjects") {
                g.item = Some(value);
            }
        }
        "cycle" | "aliases" | "overwrite" | "failedOverwrite" | "callInterface" | "callVirtual"
        | "callSpecial" | "clearAfterCast" => {
            let class = match name {
                "callInterface" => classes[4],
                "failedOverwrite" | "callVirtual" | "callSpecial" | "clearAfterCast" => classes[2],
                _ => classes[0],
            };
            let Some(array) = g.allocate(nominal(1, class, loader), None, a[0]) else {
                return g;
            };
            g.saved = Some(array);
            let index = if name == "aliases" { 0 } else { a[1] };
            let value = if name == "cycle" {
                array
            } else {
                Node::Receiver
            };
            if !g.store(array, index, Some(value), true) {
                return g;
            }
            if name == "aliases" && !g.store(array, 1, Some(Node::Receiver), true) {
                return g;
            }
            if name == "overwrite" || name == "failedOverwrite" {
                if name == "failedOverwrite" {
                    g.item = Some(Node::Receiver);
                }
                let aux = g
                    .allocate(primitive(1, P::Int, loader), Some(P::Int), 1)
                    .unwrap();
                g.auxiliary = Some(aux);
                if !g.store(array, index, Some(aux), name == "overwrite") {
                    return g;
                }
                g.item = Some(aux);
                assert!(g.store(array, index, None, true));
            } else {
                g.item = Some(value);
                if name == "aliases" || name == "clearAfterCast" {
                    assert!(g.store(array, index, None, true));
                }
            }
        }
        _ => panic!("unknown method {name}"),
    }
    g
}
fn actual(node: Option<Node>, receiver: Reference, arrays: &[Option<Reference>]) -> Reference {
    match node {
        None => Reference::null(),
        Some(Node::Receiver) => receiver,
        Some(Node::Array(id)) => arrays[id].unwrap(),
    }
}
#[test]
fn executes_3948_reference_array_body_graphs_against_jdk21() {
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
            "ReferenceArrayBase",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceArrayBase.class")
                .as_slice(),
        ),
        (
            "ReferenceArrayCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceArrayCases.class")
                .as_slice(),
        ),
        (
            "ReferenceArrayChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceArrayChild.class")
                .as_slice(),
        ),
        (
            "ReferenceArrayMarker",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceArrayMarker.class")
                .as_slice(),
        ),
        (
            "ReferenceArrayOther",
            include_bytes!("../../../tests/fixtures/classes/fixtures/ReferenceArrayOther.class")
                .as_slice(),
        ),
        (
            "ReferenceArrayUnrelated",
            include_bytes!(
                "../../../tests/fixtures/classes/fixtures/ReferenceArrayUnrelated.class"
            )
            .as_slice(),
        ),
    ] {
        let id = registry
            .define(
                loader,
                &units(&format!("fixtures/{name}")),
                bytes,
                ParseOptions::default(),
            )
            .unwrap();
        assert_eq!(registry.class(id).unwrap().version.major, 65);
    }
    let class = |name: &str| {
        registry
            .lookup_class(loader, &units(&format!("fixtures/{name}")))
            .unwrap()
            .unwrap()
    };
    let owner = class("ReferenceArrayCases");
    let child = class("ReferenceArrayChild");
    let classes = [
        object,
        class("ReferenceArrayBase"),
        owner,
        child,
        class("ReferenceArrayMarker"),
        class("ReferenceArrayOther"),
        class("ReferenceArrayUnrelated"),
        cloneable,
        serializable,
    ];
    let plan = build_hierarchy(
        &registry,
        child,
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let extra = build_hierarchy(
        &registry,
        classes[6],
        &mut SameLoaderResolver,
        Default::default(),
    )
    .unwrap();
    let field = |name: &str, descriptor: &str| {
        registry
            .find_declared_field(owner, &units(name), &units(descriptor))
            .unwrap()
            .unwrap()
    };
    let state = field("state", "I");
    let fields = [
        field("saved", "[Ljava/lang/Object;"),
        field("item", "Ljava/lang/Object;"),
        field("auxiliary", "Ljava/lang/Object;"),
    ];
    let definition = registry.class(owner).unwrap();
    let mut emitted = BTreeSet::new();
    for method in &definition.methods {
        let name = definition.constant_pool.utf8(method.name_index).unwrap();
        if ["<init>", "main", "emit", "value"]
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
                    assert_ne!(
                        instruction.opcode, 0xc5,
                        "selected body must not need multianewarray"
                    );
                    emitted.insert(instruction.opcode);
                }
            }
        }
    }
    for opcode in [0xbd, 0x32, 0x53] {
        assert!(emitted.contains(&opcode));
    }
    let mut identities = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/reference_arrays.tsv").lines() {
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
        let expected = graph(name, &ints, is_child, &classes, loader);
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan, &extra])
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
            context.heap().get_field(receiver, state).unwrap(),
            args[0],
            "{line}"
        );
        let nodes = [expected.saved, expected.item, expected.auxiliary];
        let mut arrays = vec![None; expected.arrays.len()];
        for (field, node) in fields.into_iter().zip(nodes) {
            let Value::Reference(reference) = context.heap().get_field(receiver, field).unwrap()
            else {
                panic!("reference field kind")
            };
            if let Some(Node::Array(id)) = node {
                assert!(!reference.is_null(), "{line}");
                assert_ne!(reference, receiver, "{line}");
                if let Some(previous) = arrays[id] {
                    assert_eq!(reference, previous, "{line}");
                } else {
                    arrays[id] = Some(reference);
                }
            } else {
                assert_eq!(reference, actual(node, receiver, &arrays), "{line}");
            }
        }
        assert!(
            arrays.iter().all(Option::is_some),
            "fixture must retain each allocated array: {line}"
        );
        for (index, reference) in arrays.iter().enumerate() {
            assert!(
                !arrays[..index].contains(reference),
                "distinct allocated identities: {line}"
            );
        }
        let child_handles = |nodes: &[Option<Node>]| {
            nodes
                .iter()
                .filter_map(|node| actual(*node, receiver, &arrays).heap_ref())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            context
                .heap()
                .children(receiver)
                .unwrap()
                .collect::<Vec<_>>(),
            child_handles(&nodes),
            "{line}"
        );
        for (id, array) in expected.arrays.iter().enumerate() {
            let reference = arrays[id].unwrap();
            assert_eq!(
                context.heap().length(reference).unwrap(),
                array.length,
                "{line}"
            );
            assert_eq!(
                context.reference_type(reference).unwrap(),
                Some(array.ty),
                "{line}"
            );
            if let Some(kind) = array.primitive {
                assert_eq!(context.heap().kind(reference).unwrap(), kind, "{line}");
            } else {
                assert_eq!(
                    context.heap().reference_array_type(reference).unwrap(),
                    array.ty,
                    "{line}"
                );
                for (index, node) in array.elements.iter().enumerate() {
                    assert_eq!(
                        context
                            .heap()
                            .load_reference(reference, index as i32)
                            .unwrap(),
                        actual(*node, receiver, &arrays),
                        "{line}: element{index}"
                    );
                }
            }
            assert_eq!(
                context
                    .heap()
                    .children(reference)
                    .unwrap()
                    .collect::<Vec<_>>(),
                child_handles(&array.elements),
                "{line}"
            );
        }
        assert_eq!(
            context.heap().allocated_arrays(),
            expected.arrays.len(),
            "{line}"
        );
        assert_eq!(
            context.heap().total_elements(),
            expected
                .arrays
                .iter()
                .map(|a| a.length as usize)
                .sum::<usize>(),
            "{line}"
        );
        assert_eq!(
            context.heap().payload_bytes(),
            expected
                .arrays
                .iter()
                .map(|a| a.length as usize
                    * a.primitive
                        .map_or(REFERENCE_ARRAY_ELEMENT_BYTES, P::element_bytes))
                .sum::<usize>(),
            "{line}"
        );
        assert_eq!(context.heap().allocated_instances(), 1, "{line}");
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 3948);
    assert_eq!(identities.len(), 42);
    for (kind, count) in [
        ("ArrayStoreException", 313),
        ("NegativeArraySizeException", 1406),
        ("ArrayIndexOutOfBoundsException", 1810),
        ("NullPointerException", 20),
    ] {
        assert_eq!(exceptions.get(&format!("E:java/lang/{kind}")), Some(&count));
    }
    assert_eq!(exceptions.len(), 4);
}

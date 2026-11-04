//! JDK21 multidimensional results and independent source-derived reachable graph checks.
//! A bounded depth-first fixture model differs from runtime breadth-first staging.
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
struct Array {
    ty: T,
    primitive: Option<P>,
    length: i32,
    children: Vec<Option<Node>>,
    values: Vec<Value>,
}
#[derive(Default)]
struct Graph {
    graph: Option<Node>,
    leaf: Option<Node>,
    extra: Option<Node>,
    arrays: Vec<Array>,
}
fn stored(kind: P, value: i32) -> Value {
    match kind {
        P::Boolean => Value::Int(value & 1),
        P::Byte => Value::Int(i32::from(value as i8)),
        P::Char => Value::Int(i32::from(value as u16)),
        P::Short => Value::Int(i32::from(value as i16)),
        P::Int => Value::Int(value),
        P::Long => Value::Long(i64::from(value)),
        P::Float => Value::Float((value as f32).to_bits()),
        P::Double => Value::Double(f64::from(value).to_bits()),
    }
}
impl Graph {
    fn tree(
        &mut self,
        rank: u8,
        element: ArrayElement,
        loader: LoaderId,
        counts: &[i32],
    ) -> Option<Node> {
        assert!(!counts.is_empty() && counts.len() <= 4 && usize::from(rank) >= counts.len());
        if counts.iter().any(|value| *value < 0) {
            return None;
        }
        Some(self.tree_inner(rank, element, loader, counts))
    }
    // Trusted fixture geometry has at most four levels. Recursion is deliberately
    // bounded here; production allocation and graph drop remain iterative.
    fn tree_inner(
        &mut self,
        rank: u8,
        element: ArrayElement,
        loader: LoaderId,
        counts: &[i32],
    ) -> Node {
        assert!(!counts.is_empty() && counts.len() <= 4);
        let length = counts[0];
        assert!(
            (0..=3).contains(&length),
            "huge positive dimensions must be unreachable"
        );
        let primitive = match (rank, element) {
            (1, ArrayElement::Primitive(kind)) => Some(kind),
            _ => None,
        };
        let id = self.arrays.len();
        self.arrays.push(Array {
            ty: T::Array {
                dimensions: rank,
                element,
                defining_loader: loader,
            },
            primitive,
            length,
            children: if primitive.is_none() {
                vec![None; length as usize]
            } else {
                Vec::new()
            },
            values: primitive.map_or_else(Vec::new, |kind| vec![stored(kind, 0); length as usize]),
        });
        if counts.len() > 1 {
            for index in 0..length as usize {
                let child = self.tree_inner(rank - 1, element, loader, &counts[1..]);
                self.arrays[id].children[index] = Some(child);
            }
        }
        Node::Array(id)
    }
    fn child(&self, node: Node, index: i32) -> Option<Node> {
        let Node::Array(id) = node else {
            panic!("array expected")
        };
        if index < 0 {
            return None;
        }
        self.arrays[id]
            .children
            .get(index as usize)
            .copied()
            .flatten()
    }
    fn last_at_depth(&self, mut node: Node, depth: usize) -> Option<Node> {
        for _ in 0..depth {
            let Node::Array(id) = node else {
                panic!("array expected")
            };
            node = self.arrays[id].children.last().copied().flatten()?;
        }
        Some(node)
    }
}
fn expected(name: &str, a: &[i32], child: bool, classes: &[ClassId; 9], loader: LoaderId) -> Graph {
    let mut g = Graph::default();
    let (rank, element, counts, inspect_depth) = match name {
        "full2" => (
            2,
            ArrayElement::Primitive(PRIMITIVES[a[0] as usize]),
            &a[1..3],
            Some(1),
        ),
        "partial2" => (
            3,
            ArrayElement::Primitive(PRIMITIVES[a[0] as usize]),
            &a[1..3],
            Some(1),
        ),
        "full3" => (
            3,
            ArrayElement::Primitive(PRIMITIVES[a[0] as usize]),
            &a[1..4],
            Some(2),
        ),
        "partial3" => (
            4,
            ArrayElement::Primitive(PRIMITIVES[a[0] as usize]),
            &a[1..4],
            Some(2),
        ),
        "full4" => (4, ArrayElement::Primitive(P::Int), a, Some(3)),
        "objects4" => (4, ArrayElement::Class(classes[0]), a, Some(3)),
        "references2" => (
            2,
            ArrayElement::Class(classes[a[0] as usize]),
            &a[1..3],
            Some(1),
        ),
        "storeReference" => (
            2,
            ArrayElement::Class(classes[a[0] as usize]),
            &a[1..3],
            None,
        ),
        "touch2" => (
            2,
            ArrayElement::Primitive(PRIMITIVES[a[0] as usize]),
            &a[1..3],
            None,
        ),
        "negativeAfterPrior" => {
            let old = g
                .tree(1, ArrayElement::Class(classes[0]), loader, &[1])
                .unwrap();
            let Node::Array(id) = old else { unreachable!() };
            g.arrays[id].children[0] = Some(Node::Receiver);
            g.graph = Some(old);
            g.extra = Some(old);
            g.leaf = Some(Node::Receiver);
            (2, ArrayElement::Primitive(P::Int), a, None)
        }
        _ => panic!("unknown fixture {name}"),
    };
    let Some(root) = g.tree(rank, element, loader, counts) else {
        return g;
    };
    g.graph = Some(root);
    if let Some(depth) = inspect_depth {
        g.leaf = g.last_at_depth(root, depth);
    }
    if name == "touch2" || name == "storeReference" {
        let Some(row) = g.child(root, a[3]) else {
            return g;
        };
        let Node::Array(id) = row else { unreachable!() };
        if name == "touch2" {
            g.leaf = Some(row);
        }
        let index = a[4];
        if index < 0 || index >= g.arrays[id].length {
            return g;
        }
        if name == "touch2" {
            g.arrays[id].values[index as usize] = stored(PRIMITIVES[a[0] as usize], a[5]);
        } else {
            let kind = a[0];
            let compatible =
                matches!(kind, 0 | 1 | 2 | 4 | 8) || (child && matches!(kind, 3 | 5 | 7));
            if compatible {
                g.arrays[id].children[index as usize] = Some(Node::Receiver);
                g.leaf = Some(Node::Receiver);
            }
        }
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
fn executes_7388_multidimensional_body_graphs_against_jdk21() {
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
        (
            "MultiArrayCases",
            include_bytes!("../../../tests/fixtures/classes/fixtures/MultiArrayCases.class")
                .as_slice(),
        ),
        (
            "MultiArrayChild",
            include_bytes!("../../../tests/fixtures/classes/fixtures/MultiArrayChild.class")
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
    let owner = class("MultiArrayCases");
    let child = class("MultiArrayChild");
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
    let field = |name: &str, desc: &str| {
        registry
            .find_declared_field(owner, &units(name), &units(desc))
            .unwrap()
            .unwrap()
    };
    let state = field("state", "I");
    let fields = [
        field("graph", "Ljava/lang/Object;"),
        field("leaf", "Ljava/lang/Object;"),
        field("extra", "Ljava/lang/Object;"),
    ];
    let definition = registry.class(owner).unwrap();
    let mut dimensions = BTreeSet::new();
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
                    if let crabspot_bytecode::Operands::MultiANewArray {
                        index,
                        dimensions: count,
                    } = instruction.operands
                    {
                        let rank = definition
                            .constant_pool
                            .class_name(index)
                            .unwrap()
                            .units()
                            .iter()
                            .take_while(|v| **v == 91)
                            .count();
                        dimensions.insert((rank, count));
                    }
                }
            }
        }
    }
    for pair in [(2, 2), (3, 2), (3, 3), (4, 3), (4, 4)] {
        assert!(dimensions.contains(&pair));
    }
    let mut identities = BTreeSet::new();
    let mut exceptions = BTreeMap::new();
    let mut count = 0;
    for line in
        include_str!("../../../tests/fixtures/reference/multidimensional_arrays.tsv").lines()
    {
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
        let expected = expected(name, &ints, is_child, &classes, loader);
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
        let nodes = [expected.graph, expected.leaf, expected.extra];
        let mut arrays = vec![None; expected.arrays.len()];
        let mut pending = Vec::new();
        for (field, node) in fields.into_iter().zip(nodes) {
            let Value::Reference(reference) = context.heap().get_field(receiver, field).unwrap()
            else {
                panic!("reference field")
            };
            if let Some(Node::Array(id)) = node {
                pending.push((id, reference));
            } else {
                assert_eq!(reference, actual(node, receiver, &arrays), "{line}");
            }
        }
        while let Some((id, reference)) = pending.pop() {
            if let Some(previous) = arrays[id] {
                assert_eq!(reference, previous, "{line}");
                continue;
            }
            assert!(!reference.is_null(), "{line}");
            assert_ne!(reference, receiver, "{line}");
            arrays[id] = Some(reference);
            let array = &expected.arrays[id];
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
                for (index, value) in array.values.iter().enumerate() {
                    assert_eq!(
                        context.heap().load(reference, index as i32).unwrap(),
                        *value,
                        "{line}: primitive{index}"
                    );
                }
            } else {
                assert_eq!(
                    context.heap().reference_array_type(reference).unwrap(),
                    array.ty,
                    "{line}"
                );
                for (index, node) in array.children.iter().enumerate() {
                    let value = context
                        .heap()
                        .load_reference(reference, index as i32)
                        .unwrap();
                    if let Some(Node::Array(id)) = node {
                        pending.push((*id, value));
                    } else {
                        assert_eq!(
                            value,
                            actual(*node, receiver, &arrays),
                            "{line}: child{index}"
                        );
                    }
                }
            }
        }
        assert!(
            arrays.iter().all(Option::is_some),
            "all allocations retained: {line}"
        );
        for (index, reference) in arrays.iter().enumerate() {
            assert!(
                !arrays[..index].contains(reference),
                "unique identity: {line}"
            );
        }
        let children = |nodes: &[Option<Node>]| {
            nodes
                .iter()
                .filter_map(|n| actual(*n, receiver, &arrays).heap_ref())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            context
                .heap()
                .children(receiver)
                .unwrap()
                .collect::<Vec<_>>(),
            children(&nodes),
            "{line}"
        );
        for (id, array) in expected.arrays.iter().enumerate() {
            assert_eq!(
                context
                    .heap()
                    .children(arrays[id].unwrap())
                    .unwrap()
                    .collect::<Vec<_>>(),
                children(&array.children),
                "{line}"
            );
        }
        assert_eq!(
            context.heap().allocated_arrays(),
            expected.arrays.len(),
            "{line}"
        );
        assert_eq!(context.heap().allocated_instances(), 1, "{line}");
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
        identities.insert(columns[0]);
        count += 1;
    }
    assert_eq!(count, 7388);
    assert_eq!(identities.len(), 20);
    for (kind, count) in [
        ("NegativeArraySizeException", 3364),
        ("ArrayIndexOutOfBoundsException", 1666),
        ("ArrayStoreException", 15),
    ] {
        assert_eq!(exceptions.get(&format!("E:java/lang/{kind}")), Some(&count));
    }
    assert_eq!(exceptions.len(), 3);
}

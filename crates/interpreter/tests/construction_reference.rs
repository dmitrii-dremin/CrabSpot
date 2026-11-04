//! Genuine Java21 construction results plus independently checked retained object graphs.
//! The reference driver supplies Error instances; those constructors are outside evaluated Code.
//! Rust admits one defining loader. The JDK's original raw classes use a child loader;
//! these rows compare construction/value behavior, not cross-loader constraints.
use crabspot_classfile::{ClassFile, Constant, ParseOptions};
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome, ExecutionOptions, InitializationLimits,
    InitializationSession, InitializationStatus,
};
use crabspot_runtime::{
    ArrayElement, ClassId, DefinitionRegistry, FieldId, FieldResolutionContext,
    FieldResolutionOptions, HeapOptions, HierarchyPlan, InstanceOptions, LoaderId, ManagedHeap,
    PrimitiveArrayKind, REFERENCE_ARRAY_ELEMENT_BYTES, Reference, ReferenceType,
    ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver, StaticStateOptions, Value,
    build_hierarchy,
};
use std::collections::{BTreeMap, HashMap, HashSet};
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn core(name: &str, parent: Option<&str>, interfaces: &[&str], interface: bool) -> Vec<u8> {
    let mut names = vec![name];
    names.extend(parent);
    names.extend_from_slice(interfaces);
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend_from_slice(
        &(1 + names.len() as u16 * 2 + if parent.is_none() { 3 } else { 0 }).to_be_bytes(),
    );
    for (index, name) in names.into_iter().enumerate() {
        bytes.push(1);
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(7);
        bytes.extend_from_slice(&(1 + index as u16 * 2).to_be_bytes());
    }
    if parent.is_none() {
        for text in ["<init>", "()V", "Code"] {
            bytes.push(1);
            bytes.extend_from_slice(&(text.len() as u16).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
    }
    bytes.extend_from_slice(&(if interface { 0x601u16 } else { 0x21u16 }).to_be_bytes());
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&(if parent.is_some() { 4u16 } else { 0u16 }).to_be_bytes());
    bytes.extend_from_slice(&(interfaces.len() as u16).to_be_bytes());
    for index in 0..interfaces.len() {
        let slot = 1 + usize::from(parent.is_some()) + index;
        bytes.extend_from_slice(&(2 + slot as u16 * 2).to_be_bytes());
    }
    if parent.is_none() {
        // Original Object constructor Code: return, with initialized local0.
        bytes.extend_from_slice(&[0, 0, 0, 1, 0, 1, 0, 3, 0, 4, 0, 1, 0, 5]);
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 1, 0xb1, 0, 0, 0, 0, 0, 0]);
    } else {
        bytes.extend_from_slice(&[0; 6]);
    }
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

struct Definitions {
    registry: DefinitionRegistry,
    loader: LoaderId,
    classes: BTreeMap<&'static str, ClassId>,
    order: Vec<ClassId>,
    universe: ReferenceTypeUniverse,
}
impl Definitions {
    fn new() -> Self {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut classes = BTreeMap::new();
        let mut order = Vec::new();
        for (name, parent, interfaces, interface) in [
            ("java/lang/Object", None, &[][..], false),
            (
                "java/lang/Cloneable",
                Some("java/lang/Object"),
                &[][..],
                true,
            ),
            (
                "java/io/Serializable",
                Some("java/lang/Object"),
                &[][..],
                true,
            ),
            (
                "java/lang/Throwable",
                Some("java/lang/Object"),
                &["java/io/Serializable"][..],
                false,
            ),
            (
                "java/lang/Error",
                Some("java/lang/Throwable"),
                &[][..],
                false,
            ),
        ] {
            let id = define(
                &mut registry,
                loader,
                name,
                &core(name, parent, interfaces, interface),
            );
            classes.insert(name, id);
            order.push(id);
        }
        let universe = ReferenceTypeUniverse {
            bootstrap_loader: loader,
            object: classes["java/lang/Object"],
            cloneable: classes["java/lang/Cloneable"],
            serializable: classes["java/io/Serializable"],
        };
        Self {
            registry,
            loader,
            classes,
            order,
            universe,
        }
    }
    fn add(&mut self, name: &'static str, bytes: &[u8]) -> ClassId {
        let id = define(
            &mut self.registry,
            self.loader,
            &format!("fixtures/{name}"),
            bytes,
        );
        assert!(self.classes.insert(name, id).is_none());
        self.order.push(id);
        id
    }
    fn genuine() -> Self {
        let mut definitions = Self::new();
        macro_rules! add {
            ($name:literal) => {
                definitions.add(
                    $name,
                    include_bytes!(concat!(
                        "../../../tests/fixtures/classes/fixtures/",
                        $name,
                        ".class"
                    )),
                );
            };
        }
        add!("ConstructionTrace");
        add!("ConstructionBase");
        add!("ConstructionChild");
        add!("ConstructionValues");
        add!("ConstructionNode");
        add!("ConstructionInner");
        add!("ConstructionOuter");
        add!("ConstructionActive");
        add!("ConstructionFailBefore");
        add!("ConstructionBadInitialization");
        add!("ConstructionError");
        add!("ConstructionCases");
        definitions
    }
    fn plans(&self) -> Vec<HierarchyPlan> {
        self.order
            .iter()
            .map(|id| {
                build_hierarchy(
                    &self.registry,
                    *id,
                    &mut SameLoaderResolver,
                    Default::default(),
                )
                .unwrap()
            })
            .collect()
    }
    fn class(&self, name: &str) -> ClassId {
        self.classes[name]
    }
    fn field(&self, class: &str, name: &str) -> FieldId {
        let owner = self.class(class);
        let definition = self.registry.class(owner).unwrap();
        let member = definition
            .fields
            .iter()
            .find(|f| {
                definition
                    .constant_pool
                    .utf8(f.name_index)
                    .unwrap()
                    .equals_ascii(name)
            })
            .unwrap();
        self.registry
            .find_declared_field(
                owner,
                &units(name),
                definition
                    .constant_pool
                    .utf8(member.descriptor_index)
                    .unwrap()
                    .units(),
            )
            .unwrap()
            .unwrap()
    }
    fn slots(&self) -> Vec<(FieldId, Value)> {
        let mut slots = Vec::new();
        for owner in &self.order {
            let definition = self.registry.class(*owner).unwrap();
            for member in &definition.fields {
                if member.access_flags & 8 == 0 {
                    continue;
                }
                let name = definition.constant_pool.utf8(member.name_index).unwrap();
                let descriptor = definition
                    .constant_pool
                    .utf8(member.descriptor_index)
                    .unwrap();
                let id = self
                    .registry
                    .find_declared_field(*owner, name.units(), descriptor.units())
                    .unwrap()
                    .unwrap();
                let value = match descriptor.units()[0] {
                    74 => Value::Long(0),
                    70 => Value::Float(0),
                    68 => Value::Double(0),
                    76 | 91 => Value::Reference(Reference::null()),
                    _ => Value::Int(0),
                };
                slots.push((id, value));
            }
        }
        slots
    }
}
fn field_reference(class: &ClassFile, owner: &str, name: &str) -> u16 {
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
            let Constant::NameAndType { name_index, .. } =
                class.constant_pool.get(*name_and_type_index).unwrap()
            else {
                return None;
            };
            (class
                .constant_pool
                .class_name(*class_index)
                .unwrap()
                .equals_ascii(owner)
                && class
                    .constant_pool
                    .utf8(*name_index)
                    .unwrap()
                    .equals_ascii(name))
            .then_some(index as u16)
        })
        .unwrap()
}

fn reference(value: Value) -> Reference {
    let Value::Reference(reference) = value else {
        panic!("expected reference, got {value:?}")
    };
    reference
}
fn outcome(value: ExceptionOutcome, first: Reference, escaped: Reference) -> String {
    match value {
        ExceptionOutcome::Returned(Some(Value::Reference(value))) if value.is_null() => {
            "R:null".into()
        }
        ExceptionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExceptionOutcome::Returned(Some(Value::Long(value))) => format!("L:{value}"),
        ExceptionOutcome::Returned(Some(Value::Float(bits))) => format!("F:{bits:08x}"),
        ExceptionOutcome::Returned(Some(Value::Double(bits))) => format!("D:{bits:016x}"),
        ExceptionOutcome::Returned(Some(Value::Reference(value)))
            if value == escaped && !value.is_null() =>
        {
            "R:escaped".into()
        }
        ExceptionOutcome::Uncaught(value) if value == first => "T:first".into(),
        other => panic!("unexpected construction outcome {other:?}"),
    }
}
fn inspect_graph(
    defs: &Definitions,
    heap: &ManagedHeap,
    name: &str,
    input: i32,
    first: Reference,
    escaped: Reference,
    saved: Reference,
) -> (i32, i32, HashSet<ClassId>) {
    let mut completed = HashSet::new();
    let mut count = 0;
    let mut trace = 0;
    let mut allocations = 1; // Explicitly supplied Error, not executed construction.
    let mut slots = 0;
    let mut arrays = 0;
    let mut elements = 0;
    let mut payload = 0;
    let check = |receiver, class: &str, field: &str, value| {
        assert_eq!(
            heap.get_field(receiver, defs.field(class, field)).unwrap(),
            value,
            "{name} {input}: {class}.{field}"
        );
    };
    let children = |receiver, expected: &[Reference]| {
        assert_eq!(
            heap.children(receiver).unwrap().collect::<Vec<_>>(),
            expected
                .iter()
                .filter_map(|value| value.heap_ref())
                .collect::<Vec<_>>()
        );
    };
    let class = |receiver: Reference, name: &str| {
        assert!(!receiver.is_null());
        assert_ne!(receiver, first);
        assert_eq!(heap.instance_class(receiver).unwrap(), defs.class(name));
    };
    macro_rules! done { ($($name:literal),+ $(,)?) => { $(completed.insert(defs.class(concat!("Construction", $name)));)+ }; }
    match name {
        "delegated" | "caught" | "uncaught" => {
            done!("Base", "Child");
            let value = if name == "delegated" { 4 } else { input };
            class(escaped, "ConstructionChild");
            check(escaped, "ConstructionBase", "base", Value::Int(value + 5));
            check(escaped, "ConstructionBase", "hidden", Value::Int(value + 6));
            check(escaped, "ConstructionChild", "own", Value::Int(value + 2));
            check(escaped, "ConstructionChild", "hidden", Value::Int(value));
            assert_eq!(
                saved,
                if name != "delegated" && value >= 0 {
                    escaped
                } else {
                    Reference::null()
                }
            );
            children(escaped, &[]);
            trace = if name == "delegated" { 123 } else { 12 };
            count = 1;
            allocations += 1;
            slots = 4;
        }
        "primitives" | "single" | "floating" | "nullArguments" => {
            done!("Values");
            class(escaped, "ConstructionValues");
            assert!(saved.is_null());
            let null = name == "nullArguments";
            check(
                escaped,
                "ConstructionValues",
                "flag",
                Value::Int(i32::from(!null && input & 1 != 0)),
            );
            check(
                escaped,
                "ConstructionValues",
                "tiny",
                Value::Int(if null { 0 } else { (input * 129) as i8 as i32 }),
            );
            check(
                escaped,
                "ConstructionValues",
                "character",
                Value::Int(if null { 0 } else { (input - 1) as u16 as i32 }),
            );
            check(
                escaped,
                "ConstructionValues",
                "small",
                Value::Int(if null {
                    0
                } else {
                    (input * 16385) as i16 as i32
                }),
            );
            check(escaped, "ConstructionValues", "integer", Value::Int(input));
            check(
                escaped,
                "ConstructionValues",
                "wide",
                Value::Long(if null {
                    0
                } else {
                    (i64::from(input) << 60) | 0x12345
                }),
            );
            check(
                escaped,
                "ConstructionValues",
                "single",
                Value::Float(if null {
                    (-0.0f32).to_bits()
                } else {
                    (-(input as f32)).to_bits()
                }),
            );
            check(
                escaped,
                "ConstructionValues",
                "precise",
                Value::Double(if null {
                    (-0.0f64).to_bits()
                } else {
                    (-f64::from(input)).to_bits()
                }),
            );
            check(
                escaped,
                "ConstructionValues",
                "object",
                Value::Reference(if null { Reference::null() } else { first }),
            );
            let array = reference(
                heap.get_field(escaped, defs.field("ConstructionValues", "array"))
                    .unwrap(),
            );
            if null {
                assert!(array.is_null());
                children(escaped, &[]);
            } else {
                assert_eq!(heap.length(array).unwrap(), 2);
                assert_eq!(heap.kind(array).unwrap(), PrimitiveArrayKind::Int);
                assert_eq!(heap.load(array, 0).unwrap(), Value::Int(input));
                assert_eq!(heap.load(array, 1).unwrap(), Value::Int(input + 1));
                children(array, &[]);
                children(escaped, &[first, array]);
                arrays = 1;
                elements = 2;
                payload = 8;
            }
            count = 1;
            allocations += 1;
            slots = 10;
        }
        "chain" | "loop" | "cycle" | "sameSite" => {
            done!("Node");
            let length = if matches!(name, "chain" | "loop") {
                (input & 3) as usize + 1
            } else {
                2
            };
            let mut nodes = Vec::new();
            let mut cursor = escaped;
            let mut link_array = Reference::null();
            for index in 0..length {
                class(cursor, "ConstructionNode");
                assert!(!nodes.contains(&cursor));
                nodes.push(cursor);
                check(
                    cursor,
                    "ConstructionNode",
                    "value",
                    Value::Int(input + index as i32),
                );
                let next = reference(
                    heap.get_field(cursor, defs.field("ConstructionNode", "next"))
                        .unwrap(),
                );
                let links = reference(
                    heap.get_field(cursor, defs.field("ConstructionNode", "links"))
                        .unwrap(),
                );
                if name == "cycle" {
                    if index == 0 {
                        link_array = links;
                        assert_eq!(next, saved);
                    } else {
                        assert_eq!(links, link_array);
                        assert_eq!(next, escaped);
                    }
                    children(cursor, &[next, links]);
                } else {
                    assert!(links.is_null());
                    children(cursor, &[next]);
                    if name == "sameSite" || index + 1 == length {
                        assert!(next.is_null());
                    }
                }
                cursor = if name == "sameSite" { saved } else { next };
            }
            if matches!(name, "chain" | "loop") {
                assert!(saved.is_null());
                assert!(cursor.is_null());
            } else {
                assert_eq!(saved, nodes[1]);
            }
            if name == "cycle" {
                assert_eq!(heap.length(link_array).unwrap(), 3);
                assert_eq!(
                    heap.reference_array_type(link_array).unwrap(),
                    ReferenceType::Array {
                        dimensions: 1,
                        element: ArrayElement::Class(defs.class("java/lang/Object")),
                        defining_loader: defs.loader,
                    }
                );
                for (index, expected) in [escaped, saved, escaped].into_iter().enumerate() {
                    assert_eq!(
                        heap.load_reference(link_array, index as i32).unwrap(),
                        expected
                    );
                }
                children(link_array, &[escaped, saved, escaped]);
                arrays = 1;
                elements = 3;
                payload = 3 * REFERENCE_ARRAY_ELEMENT_BYTES;
            }
            count = length as i32;
            allocations += length;
            slots = length * 3;
        }
        "beforeDelegation" => {
            done!("Base", "Outer", "Inner");
            class(escaped, "ConstructionOuter");
            assert!(saved.is_null());
            check(escaped, "ConstructionBase", "base", Value::Int(input + 5));
            check(escaped, "ConstructionBase", "hidden", Value::Int(input + 6));
            let inner = reference(
                heap.get_field(escaped, defs.field("ConstructionOuter", "inner"))
                    .unwrap(),
            );
            class(inner, "ConstructionInner");
            assert_ne!(inner, escaped);
            check(inner, "ConstructionInner", "value", Value::Int(input));
            children(inner, &[]);
            children(escaped, &[inner]);
            count = 2;
            allocations += 2;
            slots = 4;
            trace = 1;
        }
        "activeInitialization" => {
            done!("Active");
            class(escaped, "ConstructionActive");
            class(saved, "ConstructionActive");
            assert_ne!(escaped, saved);
            for (receiver, value, seen) in [(escaped, input + 1, 13), (saved, input, 0)] {
                check(receiver, "ConstructionActive", "value", Value::Int(value));
                check(receiver, "ConstructionActive", "seen", Value::Int(seen));
                children(receiver, &[]);
            }
            count = 2;
            allocations += 2;
            slots = 4;
            trace = 44;
        }
        "caughtBefore" => {
            done!("Base", "FailBefore");
            allocations += 1;
            slots = 2;
            if input < 0 {
                assert!(escaped.is_null());
                assert!(saved.is_null());
                trace = 5;
            } else {
                class(escaped, "ConstructionFailBefore");
                assert_eq!(saved, escaped);
                check(escaped, "ConstructionBase", "base", Value::Int(input + 5));
                check(escaped, "ConstructionBase", "hidden", Value::Int(input + 6));
                children(escaped, &[]);
                count = 1;
                trace = 51;
            }
        }
        "caughtInitialization" => {
            assert!(escaped.is_null());
            trace = 6;
            if input < 0 {
                assert!(saved.is_null());
            } else {
                done!("BadInitialization");
                class(saved, "ConstructionBadInitialization");
                children(saved, &[]);
                count = 1;
                allocations += 1;
            }
        }
        "object" => {
            class(escaped, "java/lang/Object");
            assert!(saved.is_null());
            children(escaped, &[]);
            allocations += 1;
        }
        other => panic!("unmodeled construction case {other}"),
    }
    assert_eq!(heap.allocated_instances(), allocations, "{name} {input}");
    assert_eq!(heap.total_instance_slots(), slots, "{name} {input}");
    assert_eq!(heap.allocated_arrays(), arrays);
    assert_eq!(heap.total_elements(), elements);
    assert_eq!(heap.payload_bytes(), payload);
    children(first, &[]);
    (trace, count, completed)
}

#[test]
fn genuine_construction_matches_112_jdk_rows_and_complete_retained_effects() {
    let defs = Definitions::genuine();
    assert_eq!(defs.order.len(), 17);
    let plans = defs.plans();
    let root = defs.class("ConstructionCases");
    let trace = defs.class("ConstructionTrace");
    let first_cp = field_reference(
        defs.registry.class(root).unwrap(),
        "fixtures/ConstructionTrace",
        "first",
    );
    let input_owner = defs.class("ConstructionActive");
    let input_cp = field_reference(
        defs.registry.class(input_owner).unwrap(),
        "fixtures/ConstructionTrace",
        "input",
    );
    let slots = defs.slots();
    assert_eq!(slots.len(), 10);
    let mut methods = HashSet::new();
    let mut rows = 0;
    for line in include_str!("../../../tests/fixtures/reference/construction.tsv")
        .lines()
        .filter(|line| !line.starts_with("raw"))
    {
        let columns: Vec<_> = line.split('\t').collect();
        let name = columns[0];
        let input: i32 = columns[2].strip_prefix("I:").unwrap().parse().unwrap();
        let method = defs
            .registry
            .find_declared_method(root, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&defs.registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(&defs.registry, defs.universe)
            .unwrap();
        let first = heap
            .allocate_instance(defs.class("ConstructionError"))
            .unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &defs.registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let statics = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        for (field, value) in &slots {
            assert_eq!(context.inspect_static(&statics, *field).unwrap(), *value);
        }
        context
            .write_static(&statics, root, first_cp, Value::Reference(first))
            .unwrap();
        context
            .write_static(&statics, input_owner, input_cp, Value::Int(input))
            .unwrap();
        let exceptions = context
            .prepare_exception_profile(defs.class("java/lang/Throwable"))
            .unwrap();
        let mut session = InitializationSession::new(
            &mut context,
            &statics,
            &exceptions,
            defs.class("java/lang/Error"),
            InitializationLimits::default(),
        )
        .unwrap();
        let result = session
            .run_static(
                method,
                &[Value::Int(input)],
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("{line}: {error:?}"));
        let escaped = reference(
            session
                .context()
                .inspect_static(&statics, defs.field("ConstructionTrace", "escaped"))
                .unwrap(),
        );
        let saved = reference(
            session
                .context()
                .inspect_static(&statics, defs.field("ConstructionTrace", "saved"))
                .unwrap(),
        );
        assert_eq!(outcome(result, first, escaped), columns[3], "{line}");
        let (expected_trace, count, mut completed) = inspect_graph(
            &defs,
            session.context().heap(),
            name,
            input,
            first,
            escaped,
            saved,
        );
        completed.extend([root, trace, defs.class("java/lang/Object")]);
        let erroneous = name == "caughtInitialization" && input < 0;
        for id in &defs.order {
            let expected = if erroneous && *id == defs.class("ConstructionBadInitialization") {
                InitializationStatus::Erroneous { cause: first }
            } else if completed.contains(id) {
                InitializationStatus::Completed
            } else {
                InitializationStatus::Uninitialized
            };
            assert_eq!(session.status(*id).unwrap(), expected, "{line}: {id:?}");
        }
        let mut expected: HashMap<_, _> = slots.iter().copied().collect();
        for (field, value) in [
            ("input", Value::Int(input)),
            ("trace", Value::Int(expected_trace)),
            ("count", Value::Int(count)),
            ("observed", Value::Int(0)),
            ("first", Value::Reference(first)),
            ("escaped", Value::Reference(escaped)),
            ("saved", Value::Reference(saved)),
        ] {
            expected.insert(defs.field("ConstructionTrace", field), value);
        }
        if name == "activeInitialization" {
            for (field, value) in [
                ("early", Value::Int(input)),
                ("late", Value::Int(13)),
                ("singleton", Value::Reference(saved)),
            ] {
                expected.insert(defs.field("ConstructionActive", field), value);
            }
        }
        for (field, _) in &slots {
            assert_eq!(
                session.context().inspect_static(&statics, *field).unwrap(),
                expected[field],
                "{line}: {field:?}"
            );
        }
        let roots = slots
            .iter()
            .filter_map(|(field, _)| match expected[field] {
                Value::Reference(value) => value.heap_ref(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            session
                .context()
                .static_roots(&statics)
                .unwrap()
                .collect::<Vec<_>>(),
            roots
        );
        assert_eq!(
            session.retained_roots().collect::<Vec<_>>(),
            vec![first.heap_ref().unwrap(); usize::from(erroneous)]
        );
        rows += 1;
        methods.insert(name);
    }
    assert_eq!(rows, 112);
    assert_eq!(methods.len(), 16);
}

#[test]
fn original_lawful_encodings_match_72_jdk_rows_and_field_bits() {
    let mut defs = Definitions::genuine();
    macro_rules! raw {
        ($name:literal) => {
            defs.add(
                concat!("Construction", $name),
                include_bytes!(concat!(
                    "../../../tests/fixtures/construction/",
                    $name,
                    ".class"
                )),
            );
        };
    }
    raw!("rawOwn");
    raw!("rawLong");
    raw!("rawNull");
    raw!("rawNarrow");
    raw!("rawCaughtBefore");
    raw!("rawTokenNull");
    raw!("rawTokenNonNull");
    raw!("rawTokenEqual");
    raw!("rawTokenNotEqual");
    let plans = defs.plans();
    let slots = defs.slots();
    let fixture = defs.class("ConstructionCases");
    let first_cp = field_reference(
        defs.registry.class(fixture).unwrap(),
        "fixtures/ConstructionTrace",
        "first",
    );
    let input_owner = defs.class("ConstructionActive");
    let input_cp = field_reference(
        defs.registry.class(input_owner).unwrap(),
        "fixtures/ConstructionTrace",
        "input",
    );
    let mut rows = 0;
    let mut methods = HashSet::new();
    for line in include_str!("../../../tests/fixtures/reference/construction.tsv")
        .lines()
        .filter(|line| line.starts_with("raw"))
    {
        let columns: Vec<_> = line.split('\t').collect();
        let name = columns[0];
        let input: i32 = columns[2].strip_prefix("I:").unwrap().parse().unwrap();
        let owner_name = format!("Construction{name}");
        let owner = defs.class(&owner_name);
        let method = defs
            .registry
            .find_declared_method(owner, &units("run"), &units(columns[1]))
            .unwrap()
            .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&defs.registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(&defs.registry, defs.universe)
            .unwrap();
        let first = heap
            .allocate_instance(defs.class("ConstructionError"))
            .unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &defs.registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let statics = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        context
            .write_static(&statics, fixture, first_cp, Value::Reference(first))
            .unwrap();
        context
            .write_static(&statics, input_owner, input_cp, Value::Int(input))
            .unwrap();
        let exceptions = context
            .prepare_exception_profile(defs.class("java/lang/Throwable"))
            .unwrap();
        let mut session = InitializationSession::new(
            &mut context,
            &statics,
            &exceptions,
            defs.class("java/lang/Error"),
            InitializationLimits::default(),
        )
        .unwrap();
        let result = session
            .run_static(
                method,
                &[Value::Int(input)],
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits::default(),
            )
            .unwrap_or_else(|error| panic!("{line}: {error:?}"));
        let escaped = reference(
            session
                .context()
                .inspect_static(&statics, defs.field("ConstructionTrace", "escaped"))
                .unwrap(),
        );
        assert_eq!(outcome(result, first, escaped), columns[3], "{line}");
        let token = name.starts_with("rawToken");
        if token {
            assert!(escaped.is_null());
        } else {
            assert_ne!(escaped, first);
            assert!(!escaped.is_null());
            let heap = session.context().heap();
            assert_eq!(heap.instance_class(escaped).unwrap(), owner);
            let expected = match name {
                "rawLong" => vec![Value::Long(i64::from(input))],
                "rawNull" => vec![Value::Reference(Reference::null())],
                "rawNarrow" => vec![
                    Value::Int(input as i8 as i32),
                    Value::Int(input as u16 as i32),
                    Value::Int(input as i16 as i32),
                    Value::Int(input & 1),
                ],
                _ => vec![Value::Int(input)],
            };
            for (index, value) in expected.into_iter().enumerate() {
                assert_eq!(
                    heap.get_field(escaped, defs.field(&owner_name, &format!("value{index}")))
                        .unwrap(),
                    value,
                    "{line}"
                );
            }
            assert_eq!(heap.children(escaped).unwrap().count(), 0);
        }
        assert_eq!(session.context().heap().allocated_instances(), 2);
        assert_eq!(
            session.context().heap().total_instance_slots(),
            if token {
                0
            } else if name == "rawNarrow" {
                4
            } else {
                1
            }
        );
        assert_eq!(session.context().heap().allocated_arrays(), 0);
        for id in &defs.order {
            let completed = *id == owner
                || *id == defs.class("java/lang/Object")
                || (!token && [fixture, defs.class("ConstructionTrace")].contains(id));
            assert_eq!(
                session.status(*id).unwrap(),
                if completed {
                    InitializationStatus::Completed
                } else {
                    InitializationStatus::Uninitialized
                },
                "{line}: {id:?}"
            );
        }
        let mut expected: HashMap<_, _> = slots.iter().copied().collect();
        for (field, value) in [
            ("input", Value::Int(input)),
            ("first", Value::Reference(first)),
            ("escaped", Value::Reference(escaped)),
            (
                "trace",
                Value::Int(if name == "rawCaughtBefore" { 8 } else { 0 }),
            ),
        ] {
            expected.insert(defs.field("ConstructionTrace", field), value);
        }
        for (field, _) in &slots {
            assert_eq!(
                session.context().inspect_static(&statics, *field).unwrap(),
                expected[field],
                "{line}: {field:?}"
            );
        }
        assert_eq!(
            session
                .context()
                .static_roots(&statics)
                .unwrap()
                .collect::<Vec<_>>(),
            [first, escaped]
                .into_iter()
                .filter_map(|value| value.heap_ref())
                .collect::<Vec<_>>()
        );
        assert_eq!(session.retained_roots().count(), 0);
        rows += 1;
        methods.insert(name);
    }
    assert_eq!(rows, 72);
    assert_eq!(methods.len(), 9);
}

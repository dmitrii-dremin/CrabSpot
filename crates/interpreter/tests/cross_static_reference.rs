//! Cross-class static calls against fresh-loader JDK outcomes and independent effects.
//! Original core metadata and supplied Error storage do not certify Java construction.
use crabspot_classfile::{AttributeBody, ClassFile, Constant, ParseOptions};
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome, ExecutionOptions, ExecutionOutcome,
    InitializationLimits, InitializationOutcome, InitializationSession, InitializationStatus,
    execute_instance_method_body, execute_instance_method_body_with_exceptions,
    execute_instance_method_body_with_static_state,
    execute_instance_method_body_with_static_state_and_exceptions,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldId, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, HierarchyPlan, InstanceOptions, LoaderId, ManagedHeap, Reference,
    ReferenceTypeUniverse, RegistryOptions, STATIC_STATE_SLOT_BYTES, SameLoaderResolver,
    StaticStateOptions, Value, build_hierarchy,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
        add!("CrossStaticTrace");
        add!("CrossStaticMath");
        add!("CrossStaticParent");
        add!("CrossStaticChild");
        add!("CrossStaticSuperContract");
        add!("CrossStaticContract");
        add!("CrossStaticEntry");
        add!("CrossStaticCycleA");
        add!("CrossStaticCycleB");
        add!("CrossStaticReference");
        add!("CrossStaticError");
        add!("CrossStaticThrower");
        add!("CrossStaticBad");
        add!("CrossStaticRecover");
        add!("CrossStaticPlain");
        add!("CrossStaticPlainChild");
        add!("CrossStaticPlainTwo");
        add!("CrossStaticPlainInterface");
        add!("CrossStaticCases");
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

fn assert_method_reference(
    class: &ClassFile,
    owner: &str,
    name: &str,
    descriptor: &str,
    interface: bool,
) {
    assert_eq!(class.version.major, 65);
    assert!(
        class.constant_pool.entries().iter().any(|constant| {
            let (class_index, name_and_type_index, actual_interface) = match constant {
                Constant::Methodref {
                    class_index,
                    name_and_type_index,
                } => (*class_index, *name_and_type_index, false),
                Constant::InterfaceMethodref {
                    class_index,
                    name_and_type_index,
                } => (*class_index, *name_and_type_index, true),
                _ => return false,
            };
            let Constant::NameAndType {
                name_index,
                descriptor_index,
            } = class.constant_pool.get(name_and_type_index).unwrap()
            else {
                return false;
            };
            actual_interface == interface
                && class
                    .constant_pool
                    .class_name(class_index)
                    .unwrap()
                    .equals_ascii(owner)
                && class
                    .constant_pool
                    .utf8(*name_index)
                    .unwrap()
                    .equals_ascii(name)
                && class
                    .constant_pool
                    .utf8(*descriptor_index)
                    .unwrap()
                    .equals_ascii(descriptor)
        }),
        "missing method reference {owner}.{name}{descriptor}"
    );
}
fn output(outcome: ExceptionOutcome, supplied: [Reference; 2]) -> String {
    match outcome {
        ExceptionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExceptionOutcome::Returned(Some(Value::Long(value))) => format!("L:{value}"),
        ExceptionOutcome::Returned(Some(Value::Float(bits))) => format!("F:{bits:08x}"),
        ExceptionOutcome::Returned(Some(Value::Double(bits))) => format!("D:{bits:016x}"),
        ExceptionOutcome::Uncaught(reference) if reference == supplied[0] => "T:first".into(),
        ExceptionOutcome::Uncaught(reference) if reference == supplied[1] => "T:second".into(),
        other => panic!("unexpected cross-class result {other:?}"),
    }
}
struct Expected {
    values: HashMap<FieldId, Value>,
    completed: HashSet<ClassId>,
    erroneous: HashSet<ClassId>,
}
fn model(
    defs: &Definitions,
    method: &str,
    input: i32,
    supplied: [Reference; 2],
    repetition: usize,
    array: Option<Reference>,
) -> Expected {
    let [first, second] = supplied;
    let mut expected = Expected {
        values: defs.slots().into_iter().collect(),
        completed: HashSet::new(),
        erroneous: HashSet::new(),
    };
    macro_rules! put {
        ($class:literal, $field:literal, $value:expr) => {
            expected
                .values
                .insert(defs.field(concat!("CrossStatic", $class), $field), $value);
        };
    }
    macro_rules! int {
        ($class:literal, $field:literal, $value:expr) => {
            put!($class, $field, Value::Int($value));
        };
    }
    macro_rules! done {
        ($($class:literal),+ $(,)?) => { $(expected.completed.insert(defs.class(concat!("CrossStatic", $class)));)+ };
    }
    expected.completed.insert(defs.class("java/lang/Object"));
    expected.completed.insert(defs.class(if method == "entry" {
        "CrossStaticEntry"
    } else {
        "CrossStaticCases"
    }));
    done!("Trace");
    int!("Trace", "input", input);
    put!("Trace", "first", Value::Reference(first));
    put!("Trace", "second", Value::Reference(second));
    match method {
        "arithmetic" | "wide" | "single" | "floating" | "entry" => {
            done!("Math");
            int!("Trace", "trace", 1);
            int!("Math", "bias", input + 7);
            match method {
                "arithmetic" => {
                    int!("Trace", "calls", repetition as i32 * 2);
                }
                "wide" => {
                    put!(
                        "Trace",
                        "wide",
                        Value::Long((i64::from(input) << 60) | 0x12345)
                    );
                }
                "single" => {
                    put!("Trace", "single", Value::Float((input as f32).to_bits()));
                }
                "floating" => {
                    put!(
                        "Trace",
                        "precise",
                        Value::Double(f64::from(input).to_bits())
                    );
                }
                "entry" => {
                    int!("Entry", "value", input * 4 + 7);
                    int!("Trace", "calls", 1);
                }
                _ => unreachable!(),
            }
        }
        "inherited" | "hidden" => {
            done!("Parent");
            int!("Parent", "base", input + 11);
            int!("Trace", "trace", if method == "hidden" { 23 } else { 2 });
            if method == "hidden" {
                done!("Child");
                int!("Child", "own", input + 13);
            }
        }
        "interfaces" => {
            done!("Contract", "Math");
            int!("Contract", "MARK", 4);
            int!("Math", "bias", input + 7);
            int!("Trace", "trace", 41);
            int!("Trace", "calls", repetition as i32);
        }
        "recursiveInitialization" | "recursion" => {
            done!("CycleA", "CycleB");
            int!("CycleA", "before", input);
            int!("CycleA", "value", input + 5);
            int!("CycleB", "value", input + 2);
            int!("Trace", "trace", 56);
            if method == "recursion" {
                int!("Trace", "calls", ((input & 3) + 1) * repetition as i32);
            }
        }
        "mixed" | "nullMixed" | "arrays" => {
            done!("Reference");
            int!("Trace", "trace", 7);
            if method == "arrays" {
                put!("Trace", "object", Value::Reference(array.unwrap()));
            } else {
                let chosen = if input & 1 != 0 {
                    second
                } else if method == "mixed" {
                    first
                } else {
                    Reference::null()
                };
                put!("Trace", "object", Value::Reference(chosen));
                put!(
                    "Trace",
                    "wide",
                    Value::Long(
                        i64::from(input) - if method == "mixed" { 4_294_967_296 } else { 0 }
                    )
                );
                put!(
                    "Trace",
                    "precise",
                    Value::Double((f64::from(input) / 2.0).to_bits())
                );
            }
        }
        "caughtBody" | "uncaughtBody" | "replacement" => {
            done!("Thrower");
            int!("Trace", "trace", 8);
            int!(
                "Trace",
                "calls",
                repetition as i32 * if method == "replacement" { 2 } else { 1 }
            );
            let thrown = if method == "replacement" || input & 1 != 0 {
                second
            } else {
                first
            };
            put!("Trace", "saved", Value::Reference(thrown));
        }
        "caughtInitialization" | "uncaughtInitialization" | "recoveredInitialization" => {
            int!("Trace", "trace", 8);
            if input == 0 {
                done!("Bad");
                int!("Bad", "before", 43);
                int!(
                    "Trace",
                    "calls",
                    if method == "recoveredInitialization" {
                        1
                    } else {
                        repetition as i32
                    }
                );
            } else {
                expected.erroneous.insert(defs.class("CrossStaticBad"));
                int!("Bad", "before", 41);
                if method != "uncaughtInitialization" {
                    put!("Trace", "saved", Value::Reference(first));
                }
            }
            if method == "recoveredInitialization" {
                done!("Recover");
                int!("Recover", "value", if input == 0 { 43 } else { 53 });
            }
        }
        other => panic!("unmodeled cross-static method {other}"),
    }
    expected
}
fn heap(defs: &Definitions, plans: &[HierarchyPlan]) -> ManagedHeap {
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(&defs.registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&defs.registry, defs.universe)
        .unwrap();
    heap
}
#[test]
fn cross_class_initialization_and_calls_match_133_fresh_loader_jdk_rows_and_retained_effects() {
    let defs = Definitions::genuine();
    assert_eq!(defs.order.len(), 24);
    let plans = defs.plans();
    let slots = defs.slots();
    assert_eq!(slots.len(), 21);
    let fields: Vec<_> = slots.iter().map(|(field, _)| *field).collect();
    let root = defs.class("CrossStaticCases");
    let input_owner = defs.class("CrossStaticMath");
    let input_cp = field_reference(
        defs.registry.class(input_owner).unwrap(),
        "fixtures/CrossStaticTrace",
        "input",
    );
    let first_cp = field_reference(
        defs.registry.class(root).unwrap(),
        "fixtures/CrossStaticTrace",
        "first",
    );
    let second_cp = field_reference(
        defs.registry.class(root).unwrap(),
        "fixtures/CrossStaticTrace",
        "second",
    );
    for (origin, owner, name, descriptor, interface) in [
        (
            "CrossStaticCases",
            "CrossStaticChild",
            "inherited",
            "(I)I",
            false,
        ),
        (
            "CrossStaticCases",
            "CrossStaticContract",
            "value",
            "(I)I",
            true,
        ),
        (
            "CrossStaticCases",
            "CrossStaticReference",
            "select",
            "(JLjava/lang/Object;DLjava/lang/Object;I)Ljava/lang/Object;",
            false,
        ),
        (
            "CrossStaticCases",
            "CrossStaticPlainChild",
            "value",
            "(I)I",
            false,
        ),
        (
            "CrossStaticContract",
            "CrossStaticContract",
            "helper",
            "(I)I",
            true,
        ),
        (
            "CrossStaticEntry",
            "CrossStaticMath",
            "value",
            "(I)I",
            false,
        ),
        (
            "CrossStaticCycleA",
            "CrossStaticCycleB",
            "read",
            "()I",
            false,
        ),
        (
            "CrossStaticCycleB",
            "CrossStaticCycleA",
            "seed",
            "()I",
            false,
        ),
    ] {
        assert_method_reference(
            defs.registry.class(defs.class(origin)).unwrap(),
            &format!("fixtures/{owner}"),
            name,
            descriptor,
            interface,
        );
    }
    let mut opcodes = BTreeSet::new();
    let mut initializers = 0;
    for id in &defs.order {
        let class = defs.registry.class(*id).unwrap();
        for method in &class.methods {
            let name = class.constant_pool.utf8(method.name_index).unwrap();
            if name.equals_ascii("<clinit>") {
                initializers += 1;
            }
            if ["<init>", "main", "emit", "result"]
                .iter()
                .any(|excluded| name.equals_ascii(excluded))
            {
                continue;
            }
            for attribute in &method.attributes {
                if let AttributeBody::Code(code) = &attribute.body {
                    for instruction in crabspot_bytecode::decode(&code.code, class.version.major)
                        .unwrap()
                        .instructions()
                    {
                        opcodes.insert(instruction.opcode);
                    }
                }
            }
        }
    }
    assert_eq!(initializers, 13);
    for opcode in [0xb8, 0xb2, 0xb3, 0xbf, 0xb0] {
        assert!(opcodes.contains(&opcode));
    }
    let mut count = 0;
    let mut methods = BTreeSet::new();
    for line in include_str!("../../../tests/fixtures/reference/cross_static.tsv")
        .lines()
        .filter(|line| !line.starts_with("body"))
    {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let name = columns[0];
        let input: i32 = columns[2].strip_prefix("I:").unwrap().parse().unwrap();
        let owner = defs.class(if name == "entry" {
            "CrossStaticEntry"
        } else {
            "CrossStaticCases"
        });
        let method = defs
            .registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
            .unwrap()
            .unwrap();
        let mut heap = heap(&defs, &plans);
        let first = heap
            .allocate_instance(defs.class("CrossStaticError"))
            .unwrap();
        let second = heap
            .allocate_instance(defs.class("CrossStaticError"))
            .unwrap();
        let supplied = [first, second];
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
        assert_eq!(statics.class_count(), 24);
        assert_eq!(statics.slot_count(), 21);
        assert_eq!(statics.payload_bytes(), 21 * STATIC_STATE_SLOT_BYTES);
        assert_eq!(context.static_fields(&statics).unwrap(), fields);
        for (field, value) in &slots {
            assert_eq!(context.inspect_static(&statics, *field).unwrap(), *value);
        }
        context
            .write_static(&statics, input_owner, input_cp, Value::Int(input))
            .unwrap();
        context
            .write_static(&statics, root, first_cp, Value::Reference(first))
            .unwrap();
        context
            .write_static(&statics, root, second_cp, Value::Reference(second))
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
        for id in &defs.order {
            assert_eq!(
                session.status(*id).unwrap(),
                InitializationStatus::Uninitialized
            );
        }
        if input == -7 {
            assert_eq!(
                session
                    .initialize(
                        owner,
                        ExecutionOptions::default(),
                        CallLimits::default(),
                        ExceptionLimits::default()
                    )
                    .unwrap(),
                InitializationOutcome::Completed
            );
        }
        let repeats = if columns[3].starts_with("T:") || name == "caughtInitialization" {
            1
        } else {
            2
        };
        let mut arrays = Vec::new();
        for repetition in 1..=repeats {
            let outcome = session
                .run_static(
                    method,
                    &[Value::Int(input)],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap();
            assert_eq!(
                output(outcome, supplied),
                columns[3],
                "{line} repetition {repetition}"
            );
            let array = if name == "arrays" {
                let Value::Reference(reference) = session
                    .context()
                    .inspect_static(&statics, defs.field("CrossStaticTrace", "object"))
                    .unwrap()
                else {
                    panic!("array reference lost")
                };
                assert!(reference.heap_ref().is_some());
                assert!(!arrays.contains(&reference));
                arrays.push(reference);
                Some(reference)
            } else {
                None
            };
            let expected = model(&defs, name, input, supplied, repetition, array);
            for id in &defs.order {
                let status = if expected.erroneous.contains(id) {
                    InitializationStatus::Erroneous { cause: first }
                } else if expected.completed.contains(id) {
                    InitializationStatus::Completed
                } else {
                    InitializationStatus::Uninitialized
                };
                assert_eq!(session.status(*id).unwrap(), status, "{line}: {id:?}");
            }
            for field in &fields {
                assert_eq!(
                    session.context().inspect_static(&statics, *field).unwrap(),
                    expected.values[field],
                    "{line}: {field:?} repetition {repetition}"
                );
            }
            let roots: Vec<_> = fields
                .iter()
                .filter_map(|field| {
                    if let Value::Reference(reference) = expected.values[field] {
                        reference.heap_ref()
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                session
                    .context()
                    .static_roots(&statics)
                    .unwrap()
                    .collect::<Vec<_>>(),
                roots,
                "{line}"
            );
            assert_eq!(
                session.retained_roots().collect::<Vec<_>>(),
                vec![first.heap_ref().unwrap(); expected.erroneous.len()],
                "{line}"
            );
            let heap = session.context().heap();
            assert_eq!(heap.allocated_instances(), 2);
            assert_eq!(heap.allocated_arrays(), arrays.len());
            let length = (input & 3) as usize;
            assert_eq!(heap.total_elements(), arrays.len() * length);
            assert_eq!(heap.payload_bytes(), arrays.len() * length * 4);
            for reference in &arrays {
                assert_eq!(
                    heap.kind(*reference).unwrap(),
                    crabspot_runtime::PrimitiveArrayKind::Int
                );
                assert_eq!(heap.length(*reference).unwrap(), length as i32);
                for index in 0..length {
                    assert_eq!(
                        heap.load(*reference, index as i32).unwrap(),
                        Value::Int(if index == 0 { input } else { 0 })
                    );
                }
            }
            for reference in supplied {
                assert_eq!(
                    heap.instance_class(reference).unwrap(),
                    defs.class("CrossStaticError")
                );
                assert_eq!(heap.children(reference).unwrap().count(), 0);
            }
            assert!(session.last_initialization_steps() > 0);
        }
        count += 1;
        methods.insert(name);
    }
    assert_eq!(count, 133);
    assert_eq!(methods.len(), 19);
}
#[test]
fn cross_class_calls_match_21_jdk_rows_in_all_four_host_controlled_profiles() {
    let defs = Definitions::genuine();
    let plans = defs.plans();
    let root = defs.class("CrossStaticCases");
    let slots = defs.slots();
    let mut count = 0;
    let mut methods = BTreeSet::new();
    for line in include_str!("../../../tests/fixtures/reference/cross_static.tsv")
        .lines()
        .filter(|line| line.starts_with("body"))
    {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let input: i32 = columns[2].strip_prefix("I:").unwrap().parse().unwrap();
        let method = defs
            .registry
            .find_declared_method(root, &units(columns[0]), &units(columns[1]))
            .unwrap()
            .unwrap();
        for profile in 0..4 {
            let mut heap = heap(&defs, &plans);
            let receiver = heap.allocate_instance(root).unwrap();
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
            let exceptions = context
                .prepare_exception_profile(defs.class("java/lang/Throwable"))
                .unwrap();
            let before_work = context.lookup_steps();
            let args = &[Value::Int(input)];
            let result = match profile {
                0 | 2 => {
                    let outcome = if profile == 0 {
                        execute_instance_method_body(
                            &mut context,
                            method,
                            receiver,
                            args,
                            ExecutionOptions::default(),
                            CallLimits::default(),
                        )
                        .unwrap()
                    } else {
                        execute_instance_method_body_with_static_state(
                            &mut context,
                            &statics,
                            method,
                            receiver,
                            args,
                            ExecutionOptions::default(),
                            CallLimits::default(),
                        )
                        .unwrap()
                    };
                    let ExecutionOutcome::Returned(value) = outcome else {
                        panic!("unexpected symbolic outcome {outcome:?}")
                    };
                    ExceptionOutcome::Returned(value)
                }
                1 => execute_instance_method_body_with_exceptions(
                    &mut context,
                    &exceptions,
                    method,
                    receiver,
                    args,
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap(),
                3 => execute_instance_method_body_with_static_state_and_exceptions(
                    &mut context,
                    &statics,
                    &exceptions,
                    method,
                    receiver,
                    args,
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(
                output(result, [Reference::null(); 2]),
                columns[3],
                "{line} profile {profile}"
            );
            assert!(context.lookup_steps() > before_work);
            for (field, value) in &slots {
                assert_eq!(
                    context.inspect_static(&statics, *field).unwrap(),
                    *value,
                    "{line} profile {profile}"
                );
            }
            assert_eq!(context.static_roots(&statics).unwrap().count(), 0);
            assert_eq!(context.heap().allocated_instances(), 1);
            assert_eq!(context.heap().allocated_arrays(), 0);
            assert_eq!(context.heap().total_elements(), 0);
            assert_eq!(context.heap().payload_bytes(), 0);
            assert_eq!(context.heap().children(receiver).unwrap().count(), 0);
            let session = InitializationSession::new(
                &mut context,
                &statics,
                &exceptions,
                defs.class("java/lang/Error"),
                InitializationLimits::default(),
            )
            .unwrap();
            for id in &defs.order {
                assert_eq!(
                    session.status(*id).unwrap(),
                    InitializationStatus::Uninitialized
                );
            }

            count += 1;
        }
        methods.insert(columns[0]);
    }
    assert_eq!(count, 84);
    assert_eq!(methods.len(), 3);
}

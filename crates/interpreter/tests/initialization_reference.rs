//! Fresh-identity JDK initialization results plus independent state/effect models.
//! Original core metadata and supplied Error storage do not certify Java construction.
use crabspot_classfile::{AttributeBody, ClassFile, Constant, ParseOptions};
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome, ExecutionOptions, InitializationLimits,
    InitializationOutcome, InitializationSession, InitializationStatus,
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
        add!("InitProbeChild");
        add!("InitProbeCycleA");
        add!("InitProbeCycleB");
        add!("InitProbeDefaultA");
        add!("InitProbeDefaultB");
        add!("InitProbeDefaultC");
        add!("InitProbeDiamond");
        add!("InitProbeEntry");
        add!("InitProbeError");
        add!("InitProbeFailure");
        add!("InitProbeFailureChild");
        add!("InitProbeFailureSecond");
        add!("InitProbeInherited");
        add!("InitProbeInheritedChild");
        add!("InitProbeNumeric");
        add!("InitProbeOnce");
        add!("InitProbeOwner");
        add!("InitProbeParent");
        add!("InitProbePlain");
        add!("InitProbePrivate");
        add!("InitProbePrivateChild");
        add!("InitProbeRecover");
        add!("InitProbeStaticOnly");
        add!("InitProbeSymbolic");
        add!("InitProbeTrace");
        add!("InitProbeWrite");
        add!("InitializationCases");
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
fn output(outcome: ExceptionOutcome, supplied: Reference) -> String {
    match outcome {
        ExceptionOutcome::Returned(Some(Value::Int(value))) => format!("I:{value}"),
        ExceptionOutcome::Uncaught(reference) if reference == supplied => "T:first".into(),
        other => panic!("unexpected initialization oracle result {other:?}"),
    }
}
struct Expected {
    values: HashMap<FieldId, Value>,
    completed: HashSet<ClassId>,
    erroneous: HashMap<ClassId, Reference>,
}
fn model(
    defs: &Definitions,
    method: &str,
    input: i32,
    first: Reference,
    second: Reference,
) -> Expected {
    let mut expected = Expected {
        values: defs.slots().into_iter().collect(),
        completed: HashSet::new(),
        erroneous: HashMap::new(),
    };
    macro_rules! set {
        ($class:expr, $field:expr, $value:expr) => {
            expected.values.insert(defs.field($class, $field), $value);
        };
    }
    macro_rules! int {
        ($class:expr, $field:expr, $value:expr) => {
            set!($class, $field, Value::Int($value));
        };
    }
    macro_rules! done { ($($name:expr),+ $(,)?) => { $(expected.completed.insert(defs.class($name));)+ }; }
    done!(
        "java/lang/Object",
        if method == "entry" {
            "InitProbeEntry"
        } else {
            "InitializationCases"
        }
    );
    int!("InitProbeTrace", "input", input);
    set!("InitProbeTrace", "first", Value::Reference(first));
    set!("InitProbeTrace", "second", Value::Reference(second));
    if method != "once" {
        done!("InitProbeTrace");
    }
    let trace = match method {
        "constants" => {
            done!("InitProbeNumeric");
            int!("InitProbeNumeric", "FIXED", 17);
            int!("InitProbeNumeric", "SMALL", -2);
            set!("InitProbeNumeric", "WIDE", Value::Long(-4_294_967_297));
            set!("InitProbeNumeric", "SINGLE", Value::Float(0x8000_0000));
            set!(
                "InitProbeNumeric",
                "DOUBLE",
                Value::Double(0x7ff8_0000_0000_0000)
            );
            int!("InitProbeNumeric", "dynamic", input + 31);
            int!("InitProbeNumeric", "value", input + 17);
            1
        }
        "order" | "interfaceThenClass" | "classThenInterface" => {
            done!(
                "InitProbeParent",
                "InitProbeDefaultA",
                "InitProbeDefaultB",
                "InitProbeChild"
            );
            if method == "interfaceThenClass" {
                int!("InitProbeDefaultB", "MARK", 2);
                int!("InitProbeParent", "value", 23);
                int!("InitProbeDefaultA", "MARK", 231);
                int!("InitProbeChild", "value", 2314);
                2314
            } else {
                int!("InitProbeParent", "value", 3);
                int!("InitProbeDefaultA", "MARK", 31);
                int!("InitProbeDefaultB", "MARK", 312);
                int!("InitProbeChild", "value", 3124);
                if method == "classThenInterface" {
                    done!("InitProbePlain");
                    int!("InitProbePlain", "MARK", 31249);
                    31249
                } else {
                    3124
                }
            }
        }
        "interfaceOnly" => {
            done!("InitProbeDefaultB");
            int!("InitProbeDefaultB", "MARK", 2);
            2
        }
        "diamond" => {
            done!(
                "InitProbeDefaultA",
                "InitProbeDefaultB",
                "InitProbeDefaultC",
                "InitProbeDiamond"
            );
            int!("InitProbeDefaultA", "MARK", 1);
            int!("InitProbeDefaultB", "MARK", 12);
            int!("InitProbeDefaultC", "MARK", 125);
            int!("InitProbeDiamond", "value", 1256);
            1256
        }
        "inheritedInterface" => {
            done!("InitProbeDefaultA", "InitProbeInheritedChild");
            int!("InitProbeDefaultA", "MARK", 1);
            int!("InitProbeInheritedChild", "value", 16);
            16
        }
        "privateInterface" => {
            done!("InitProbePrivate", "InitProbePrivateChild");
            int!("InitProbePrivate", "MARK", 5);
            int!("InitProbePrivateChild", "value", 57);
            57
        }
        "declaringOwner" => {
            done!("InitProbeOwner");
            int!("InitProbeOwner", "value", 1);
            1
        }
        "recursive" => {
            done!("InitProbeCycleA", "InitProbeCycleB");
            int!("InitProbeCycleA", "before", input);
            int!("InitProbeCycleB", "value", input + 2);
            int!("InitProbeCycleA", "value", input + 5);
            123
        }
        "once" => {
            done!("InitProbeOnce");
            int!("InitProbeOnce", "count", 1);
            int!("InitProbeOnce", "value", 1);
            0
        }
        "writeTrigger" => {
            done!("InitProbeWrite");
            int!("InitProbeWrite", "value", input);
            8
        }
        "entry" => {
            int!("InitProbeEntry", "value", input + 9);
            9
        }
        "twoFailures" => {
            if input == 0 {
                done!("InitProbeFailureSecond", "InitProbeFailure");
                int!("InitProbeFailureSecond", "value", 29);
                int!("InitProbeFailure", "value", 23);
            } else {
                expected
                    .erroneous
                    .insert(defs.class("InitProbeFailureSecond"), second);
                expected
                    .erroneous
                    .insert(defs.class("InitProbeFailure"), first);
                int!("InitProbeFailureSecond", "value", 27);
                int!("InitProbeFailure", "value", 19);
                set!("InitProbeTrace", "savedSecond", Value::Reference(second));
                set!("InitProbeTrace", "saved", Value::Reference(first));
            }
            64
        }
        "caughtError" | "uncaughtError" | "dependencyError" | "recoveredError" => {
            if input == 0 {
                done!("InitProbeFailure");
                int!("InitProbeFailure", "value", 23);
                if method == "dependencyError" {
                    done!("InitProbeFailureChild");
                    int!("InitProbeFailureChild", "childValue", 45);
                }
                if method == "recoveredError" {
                    done!("InitProbeRecover");
                    int!("InitProbeRecover", "value", 23);
                }
            } else {
                expected
                    .erroneous
                    .insert(defs.class("InitProbeFailure"), first);
                int!("InitProbeFailure", "value", 19);
                if method == "dependencyError" {
                    expected
                        .erroneous
                        .insert(defs.class("InitProbeFailureChild"), first);
                }
                if method != "uncaughtError" {
                    set!("InitProbeTrace", "saved", Value::Reference(first));
                }
                if method == "recoveredError" {
                    done!("InitProbeRecover");
                    int!("InitProbeRecover", "value", 43);
                }
            }
            if method == "dependencyError" && input == 0 {
                45
            } else {
                4
            }
        }
        _ => panic!("unmodeled method {method}"),
    };
    int!("InitProbeTrace", "trace", trace);
    expected
}
#[test]
fn initializes_90_genuine_scenarios_against_fresh_jdk_identities() {
    let defs = Definitions::genuine();
    let plans = defs.plans();
    assert_eq!(defs.order.len(), 32);
    let slots = defs.slots();
    assert_eq!(slots.len(), 38);
    let fields: Vec<_> = slots.iter().map(|(id, _)| *id).collect();
    let numeric = defs.class("InitProbeNumeric");
    let failure = defs.class("InitProbeFailure");
    let input_cp = field_reference(
        defs.registry.class(numeric).unwrap(),
        "fixtures/InitProbeTrace",
        "input",
    );
    let first_cp = field_reference(
        defs.registry.class(failure).unwrap(),
        "fixtures/InitProbeTrace",
        "first",
    );
    let second_owner = defs.class("InitProbeFailureSecond");
    let second_cp = field_reference(
        defs.registry.class(second_owner).unwrap(),
        "fixtures/InitProbeTrace",
        "second",
    );
    let mut observed_opcodes = BTreeSet::new();
    let mut initializer_count = 0;
    for id in &defs.order {
        let class = defs.registry.class(*id).unwrap();
        for method in &class.methods {
            let name = class.constant_pool.utf8(method.name_index).unwrap();
            if name.equals_ascii("<clinit>") {
                initializer_count += 1;
            }
            if [
                "<init>",
                "main",
                "emit",
                "emitRaw",
                "rawConstant",
                "utf",
                "pair",
            ]
            .iter()
            .any(|n| name.equals_ascii(n))
            {
                continue;
            }
            for attribute in &method.attributes {
                if let AttributeBody::Code(code) = &attribute.body {
                    for instruction in crabspot_bytecode::decode(&code.code, class.version.major)
                        .unwrap()
                        .instructions()
                    {
                        observed_opcodes.insert(instruction.opcode);
                    }
                }
            }
        }
    }
    assert_eq!(initializer_count, 24);
    for opcode in [0xb2, 0xb3, 0xbf] {
        assert!(observed_opcodes.contains(&opcode));
    }
    let mut count = 0;
    let mut methods = BTreeSet::new();
    for line in include_str!("../../../tests/fixtures/reference/initialization.tsv")
        .lines()
        .filter(|line| !line.starts_with("raw/"))
    {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let name = columns[0];
        let input: i32 = columns[2].strip_prefix("I:").unwrap().parse().unwrap();
        let owner = defs.class(if name == "entry" {
            "InitProbeEntry"
        } else {
            "InitializationCases"
        });
        let method = defs
            .registry
            .find_declared_method(owner, &units(name), &units(columns[1]))
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
            .allocate_instance(defs.class("InitProbeError"))
            .unwrap();
        let second = heap
            .allocate_instance(defs.class("InitProbeError"))
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
        assert_eq!(statics.class_count(), 32);
        assert_eq!(statics.slot_count(), 38);
        assert_eq!(statics.payload_bytes(), 38 * STATIC_STATE_SLOT_BYTES);
        assert_eq!(context.static_fields(&statics).unwrap(), fields);
        for (field, value) in &slots {
            assert_eq!(context.inspect_static(&statics, *field).unwrap(), *value);
        }
        context
            .write_static(&statics, numeric, input_cp, Value::Int(input))
            .unwrap();
        context
            .write_static(&statics, failure, first_cp, Value::Reference(first))
            .unwrap();
        context
            .write_static(&statics, second_owner, second_cp, Value::Reference(second))
            .unwrap();
        let exceptions = context
            .prepare_exception_profile(defs.class("java/lang/Throwable"))
            .unwrap();
        let expected = model(&defs, name, input, first, second);
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
        let repeatable = !matches!(
            name,
            "caughtError" | "uncaughtError" | "dependencyError" | "twoFailures"
        );
        for _ in 0..if repeatable { 2 } else { 1 } {
            let result = session
                .run_static(
                    method,
                    &[Value::Int(input)],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(output(result, first), columns[3], "{line}");
            for id in &defs.order {
                let status = if expected.erroneous.contains_key(id) {
                    InitializationStatus::Erroneous {
                        cause: expected.erroneous[id],
                    }
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
                    "{line}: {field:?}"
                );
            }
            let roots: Vec<_> = fields
                .iter()
                .filter_map(|field| {
                    if let Value::Reference(r) = expected.values[field] {
                        r.heap_ref()
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
                if name == "twoFailures" && input != 0 {
                    vec![second.heap_ref().unwrap(), first.heap_ref().unwrap()]
                } else {
                    vec![first.heap_ref().unwrap(); expected.erroneous.len()]
                },
                "{line}"
            );
            assert_eq!(session.context().heap().allocated_instances(), 2);
            assert_eq!(session.context().heap().allocated_arrays(), 0);
            assert_eq!(session.context().heap().total_elements(), 0);
            assert_eq!(session.context().heap().payload_bytes(), 0);
            assert_eq!(
                session.context().heap().instance_class(first).unwrap(),
                defs.class("InitProbeError")
            );
            assert_eq!(session.context().heap().children(first).unwrap().count(), 0);
        }
        methods.insert(name);
        count += 1;
    }
    assert_eq!(count, 90);
    assert_eq!(methods.len(), 18);
}
fn raw_constant(descriptor: &str, value: i32) -> Vec<u8> {
    fn word(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend(value.to_be_bytes());
    }
    fn utf(bytes: &mut Vec<u8>, text: &str) {
        bytes.push(1);
        word(bytes, text.len() as u16);
        bytes.extend(text.as_bytes());
    }
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    word(&mut bytes, 14);
    utf(&mut bytes, "fixtures/InitProbeRawConstant");
    bytes.extend([7, 0, 1]);
    utf(&mut bytes, "java/lang/Object");
    bytes.extend([7, 0, 3]);
    utf(&mut bytes, "value");
    utf(&mut bytes, descriptor);
    utf(&mut bytes, "ConstantValue");
    bytes.push(3);
    bytes.extend(value.to_be_bytes());
    utf(&mut bytes, "get");
    utf(&mut bytes, "(I)I");
    utf(&mut bytes, "Code");
    bytes.extend([12, 0, 5, 0, 6, 9, 0, 2, 0, 12]);
    for value in [0x21, 2, 4, 0, 1, 9, 5, 6, 1, 7] {
        word(&mut bytes, value);
    }
    bytes.extend(2u32.to_be_bytes());
    word(&mut bytes, 8);
    for value in [1, 9, 9, 10, 1, 11] {
        word(&mut bytes, value);
    }
    bytes.extend(16u32.to_be_bytes());
    word(&mut bytes, 1);
    word(&mut bytes, 1);
    bytes.extend(4u32.to_be_bytes());
    bytes.extend([0xb2, 0, 13, 0xac]);
    bytes.extend([0; 6]);
    bytes
}
#[test]
fn initializes_55_original_nonfinal_constant_encodings_against_jdk() {
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/reference/initialization.tsv")
        .lines()
        .filter(|line| line.starts_with("raw/"))
    {
        let columns: Vec<_> = line.split('\t').collect();
        let descriptor = columns[0].strip_prefix("raw/").unwrap();
        let input: i32 = columns[2].strip_prefix("I:").unwrap().parse().unwrap();
        let mut defs = Definitions::new();
        let owner = defs.add("InitProbeRawConstant", &raw_constant(descriptor, input));
        let field = defs.field("InitProbeRawConstant", "value");
        assert_eq!(defs.registry.field(field).unwrap().access_flags, 9);
        let method = defs
            .registry
            .find_declared_method(owner, &units("get"), &units("(I)I"))
            .unwrap()
            .unwrap();
        let plans = defs.plans();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&defs.registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(&defs.registry, defs.universe)
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
        assert_eq!(statics.slot_count(), 1);
        assert_eq!(
            context.inspect_static(&statics, field).unwrap(),
            Value::Int(0)
        );
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
        for _ in 0..2 {
            let outcome = session
                .run_static(
                    method,
                    &[Value::Int(input)],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(output(outcome, Reference::null()), columns[3], "{line}");
            let expected = match descriptor {
                "Z" => input & 1,
                "B" => i32::from(input as i8),
                "C" => i32::from(input as u16),
                "S" => i32::from(input as i16),
                "I" => input,
                _ => panic!("constant kind"),
            };
            assert_eq!(
                session.context().inspect_static(&statics, field).unwrap(),
                Value::Int(expected)
            );
            for id in &defs.order {
                assert_eq!(
                    session.status(*id).unwrap(),
                    if *id == owner || *id == defs.universe.object {
                        InitializationStatus::Completed
                    } else {
                        InitializationStatus::Uninitialized
                    }
                );
            }
            assert_eq!(session.retained_roots().count(), 0);
            assert_eq!(session.context().static_roots(&statics).unwrap().count(), 0);
            assert_eq!(session.context().heap().allocated_instances(), 0);
            assert_eq!(session.context().heap().allocated_arrays(), 0);
            assert_eq!(session.context().heap().payload_bytes(), 0);
        }
        count += 1;
    }
    assert_eq!(count, 55);
}

//! Actual initializer scheduling with retained active-use continuations and failures.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome as EO, ExecutionError, ExecutionErrorKind as E,
    ExecutionOptions, InitializationLimits, InitializationOutcome as IO, InitializationSession,
    InitializationStatus as IS, Value,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, ExceptionProfile, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, HierarchyOptions, InitializationAccessError, InstanceOptions, ManagedHeap,
    MethodId, Reference, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver,
    StaticStateOptions, StaticStateProfile,
};
use std::collections::HashMap;
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn u2(bytes: &mut Vec<u8>, word: u16) {
    bytes.extend(word.to_be_bytes());
}
fn u4(bytes: &mut Vec<u8>, word: u32) {
    bytes.extend(word.to_be_bytes());
}
struct Method {
    flags: u16,
    name: u16,
    descriptor: u16,
    code_name: u16,
    code: Option<Vec<u8>>,
    stack: u16,
    locals: u16,
    handlers: Vec<[u16; 4]>,
}
struct Class {
    name: String,
    pool: Vec<Vec<u8>>,
    this: u16,
    parent: u16,
    major: u16,
    flags: u16,
    interfaces: Vec<u16>,
    fields: Vec<[u16; 3]>,
    constants: HashMap<u16, (u16, u16)>,
    methods: Vec<Method>,
}
impl Class {
    fn new(name: &str, parent: Option<&str>, flags: u16) -> Self {
        let mut class = Self {
            name: name.into(),
            pool: Vec::new(),
            this: 0,
            parent: 0,
            major: 65,
            flags,
            interfaces: Vec::new(),
            fields: Vec::new(),
            constants: HashMap::new(),
            methods: Vec::new(),
        };
        class.this = class.class(name);
        class.parent = parent.map_or(0, |name| class.class(name));
        class
    }
    fn cp(&mut self, tag: u8, data: &[u8]) -> u16 {
        let mut entry = vec![tag];
        entry.extend(data);
        self.pool.push(entry);
        let index = self.pool.len() as u16;
        if matches!(tag, 5 | 6) {
            self.pool.push(Vec::new());
        }
        index
    }
    fn utf8(&mut self, text: &str) -> u16 {
        let mut data = Vec::new();
        u2(&mut data, text.len() as u16);
        data.extend(text.bytes());
        self.cp(1, &data)
    }
    fn class(&mut self, name: &str) -> u16 {
        let name = self.utf8(name);
        self.cp(7, &name.to_be_bytes())
    }
    fn reference(&mut self, tag: u8, owner: &str, name: &str, descriptor: &str) -> u16 {
        let owner = self.class(owner);
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let mut data = Vec::new();
        u2(&mut data, name);
        u2(&mut data, descriptor);
        let nat = self.cp(12, &data);
        let mut data = Vec::new();
        u2(&mut data, owner);
        u2(&mut data, nat);
        self.cp(tag, &data)
    }
    fn field(&mut self, name: &str, descriptor: &str, flags: u16) {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        self.fields.push([flags, name, descriptor]);
    }
    fn constant(&mut self, name: &str, descriptor: &str, flags: u16, tag: u8, data: &[u8]) {
        self.field(name, descriptor, flags);
        let field = self.fields.last().unwrap()[1];
        let attribute = self.utf8("ConstantValue");
        let index = self.cp(tag, data);
        self.constants.insert(field, (attribute, index));
    }
    fn constant_int(&mut self, name: &str, descriptor: &str, flags: u16, value: i32) {
        self.constant(name, descriptor, flags, 3, &value.to_be_bytes());
    }
    fn method(
        &mut self,
        name: &str,
        descriptor: &str,
        flags: u16,
        code: &[u8],
        stack: u16,
        locals: u16,
    ) {
        let initializer = name == "<clinit>";
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let code_name = self.utf8("Code");
        self.methods.push(Method {
            flags,
            name,
            descriptor,
            code_name,
            code: if flags & 0x500 != 0 && !initializer {
                None
            } else {
                Some(code.to_vec())
            },
            stack,
            locals,
            handlers: Vec::new(),
        });
    }
    fn bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0];
        u2(&mut bytes, self.major);
        u2(&mut bytes, self.pool.len() as u16 + 1);
        for entry in &self.pool {
            bytes.extend(entry);
        }
        for word in [
            self.flags,
            self.this,
            self.parent,
            self.interfaces.len() as u16,
        ] {
            u2(&mut bytes, word);
        }
        for interface in &self.interfaces {
            u2(&mut bytes, *interface);
        }
        u2(&mut bytes, self.fields.len() as u16);
        for field in &self.fields {
            for word in field {
                u2(&mut bytes, *word);
            }
            if let Some(&(attribute, index)) = self.constants.get(&field[1]) {
                u2(&mut bytes, 1);
                u2(&mut bytes, attribute);
                u4(&mut bytes, 2);
                u2(&mut bytes, index);
            } else {
                u2(&mut bytes, 0);
            }
        }
        u2(&mut bytes, self.methods.len() as u16);
        for method in &self.methods {
            for word in [
                method.flags,
                method.name,
                method.descriptor,
                u16::from(method.code.is_some()),
            ] {
                u2(&mut bytes, word);
            }
            if let Some(code) = &method.code {
                u2(&mut bytes, method.code_name);
                u4(
                    &mut bytes,
                    12 + code.len() as u32 + method.handlers.len() as u32 * 8,
                );
                u2(&mut bytes, method.stack);
                u2(&mut bytes, method.locals);
                u4(&mut bytes, code.len() as u32);
                bytes.extend(code);
                u2(&mut bytes, method.handlers.len() as u16);
                for row in &method.handlers {
                    for word in row {
                        u2(&mut bytes, *word);
                    }
                }
                u2(&mut bytes, 0);
            }
        }
        u2(&mut bytes, 0);
        bytes
    }
}
fn op(opcode: u8, index: u16) -> Vec<u8> {
    let mut code = vec![opcode];
    u2(&mut code, index);
    code
}

struct Fixture {
    registry: DefinitionRegistry,
    heap: ManagedHeap,
    classes: HashMap<String, ClassId>,
}
impl Fixture {
    fn new(classes: Vec<Class>) -> Self {
        Self::selected(classes, &[])
    }
    fn selected(mut classes: Vec<Class>, roots: &[&str]) -> Self {
        for (name, parent, flags) in [
            ("java/lang/Object", None, 0x21),
            ("java/lang/Throwable", Some("java/lang/Object"), 0x21),
            ("java/lang/Error", Some("java/lang/Throwable"), 0x21),
            ("q/Error", Some("java/lang/Error"), 0x21),
            ("q/Exception", Some("java/lang/Throwable"), 0x21),
            ("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
            ("java/io/Serializable", Some("java/lang/Object"), 0x601),
        ] {
            if !classes.iter().any(|class| class.name == name) {
                classes.push(Class::new(name, parent, flags));
            }
        }
        let mut registry = DefinitionRegistry::new(RegistryOptions {
            max_definitions: classes.len().max(1024),
            ..RegistryOptions::default()
        })
        .unwrap();
        let loader = registry.create_loader().unwrap();
        let mut handles = HashMap::new();
        for class in classes {
            let id = registry
                .define(
                    loader,
                    &units(&class.name),
                    &class.bytes(),
                    ParseOptions::default(),
                )
                .unwrap();
            handles.insert(class.name, id);
        }
        let mut heap = ManagedHeap::new_with_instances(
            HeapOptions::default(),
            InstanceOptions {
                max_layout_classes: handles.len().max(1024),
                max_layout_edges: handles.len().max(8192),
                ..InstanceOptions::default()
            },
        )
        .unwrap();
        let selected: Vec<_> = if roots.is_empty() {
            handles.values().copied().collect()
        } else {
            roots.iter().map(|name| handles[*name]).collect()
        };
        let plans: Vec<_> = selected
            .iter()
            .map(|id| {
                crabspot_runtime::build_hierarchy(
                    &registry,
                    *id,
                    &mut SameLoaderResolver,
                    HierarchyOptions {
                        max_classes: handles.len().max(1024),
                        max_edges: handles.len().max(8192),
                        ..HierarchyOptions::default()
                    },
                )
                .unwrap()
            })
            .collect();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(
            &registry,
            ReferenceTypeUniverse {
                bootstrap_loader: loader,
                object: handles["java/lang/Object"],
                cloneable: handles["java/lang/Cloneable"],
                serializable: handles["java/io/Serializable"],
            },
        )
        .unwrap();
        Self {
            registry,
            heap,
            classes: handles,
        }
    }
    fn method(&self, class: &str, name: &str, descriptor: &str) -> MethodId {
        self.registry
            .find_declared_method(self.classes[class], &units(name), &units(descriptor))
            .unwrap()
            .unwrap()
    }
    fn profiles(&mut self) -> (StaticStateProfile, ExceptionProfile) {
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let statics = context
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        let exceptions = context
            .prepare_exception_profile(self.classes["java/lang/Throwable"])
            .unwrap();
        (statics, exceptions)
    }
    fn run<T>(
        &mut self,
        limits: InitializationLimits,
        run: impl FnOnce(&mut InitializationSession<'_, '_>, StaticStateProfile) -> T,
    ) -> T {
        let (statics, exceptions) = self.profiles();
        let error = self.classes["java/lang/Error"];
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: 30_000_000,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let mut session =
            InitializationSession::new(&mut context, &statics, &exceptions, error, limits).unwrap();
        run(&mut session, statics)
    }
}
fn inspect(
    session: &InitializationSession<'_, '_>,
    profile: &StaticStateProfile,
    class: ClassId,
    name: &str,
    descriptor: &str,
) -> Value {
    let context = session.context();
    let field = context
        .registry()
        .find_declared_field(class, &units(name), &units(descriptor))
        .unwrap()
        .unwrap();
    context.inspect_static(profile, field).unwrap()
}
fn initialize(
    session: &mut InitializationSession<'_, '_>,
    class: ClassId,
) -> Result<IO, ExecutionError> {
    session.initialize(
        class,
        ExecutionOptions::default(),
        CallLimits::default(),
        ExceptionLimits::default(),
    )
}
fn ordinary(name: &str) -> Class {
    Class::new(name, Some("java/lang/Object"), 0x21)
}
fn clinit(class: &mut Class, code: &[u8], stack: u16) {
    class.method("<clinit>", "()V", 8, code, stack, 0);
}
fn set_initializer(class: &mut Class, name: &str, value: i8) {
    let field = class.reference(9, &class.name.clone(), name, "I");
    let mut code = vec![0x10, value as u8];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    clinit(class, &code, 1);
}

#[test]
fn no_body_superclass_and_previously_completed_dependencies_are_driven_without_fake_frames() {
    let parent = ordinary("q/Parent");
    let child = Class::new("q/Child", Some("q/Parent"), 0x21);
    let mut fixture = Fixture::new(vec![parent, child]);
    let parent = fixture.classes["q/Parent"];
    let child = fixture.classes["q/Child"];
    let object = fixture.classes["java/lang/Object"];
    fixture.run(InitializationLimits::default(), |session, _| {
        assert_eq!(
            session
                .initialize(
                    parent,
                    ExecutionOptions {
                        instruction_budget: 0
                    },
                    CallLimits {
                        max_frames: 0,
                        ..CallLimits::default()
                    },
                    ExceptionLimits::default()
                )
                .unwrap(),
            IO::Completed
        );
        assert_eq!(session.status(parent).unwrap(), IS::Completed);
        assert_eq!(session.status(object).unwrap(), IS::Completed);
        assert_eq!(
            session
                .initialize(
                    child,
                    ExecutionOptions {
                        instruction_budget: 0
                    },
                    CallLimits {
                        max_frames: 0,
                        ..CallLimits::default()
                    },
                    ExceptionLimits::default()
                )
                .unwrap(),
            IO::Completed
        );
        assert_eq!(session.status(child).unwrap(), IS::Completed);
        assert_eq!(session.last_initialization_steps(), 6);
        assert_eq!(initialize(session, child).unwrap(), IO::Completed);
        assert_eq!(session.last_initialization_steps(), 1);
    });
}
#[test]
fn real_clinit_runs_once_and_static_root_is_activated_only_after_its_owner_completes() {
    let mut root = ordinary("p/T");
    root.field("value", "I", 9);
    set_initializer(&mut root, "value", 7);
    let field = root.reference(9, "p/T", "value", "I");
    let mut code = op(0xb2, field);
    code.push(0xac);
    root.method("read", "()I", 9, &code, 1, 0);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.classes["p/T"];
    let read = fixture.method("p/T", "read", "()I");
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(
            session
                .run_static(
                    read,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
                .unwrap(),
            EO::Returned(Some(Value::Int(7)))
        );
        assert_eq!(session.status(class).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, class, "value", "I"),
            Value::Int(7)
        );
        assert_eq!(initialize(session, class).unwrap(), IO::Completed);
        assert_eq!(session.last_initialization_steps(), 1);
    });
}
#[test]
fn final_numeric_constants_include_nonfinal_fields_and_own_constants_precede_dependencies() {
    let mut parent = ordinary("q/Parent");
    parent.field("seen", "I", 9);
    let child_value = parent.reference(9, "q/Child", "value", "I");
    let seen = parent.reference(9, "q/Parent", "seen", "I");
    let mut code = op(0xb2, child_value);
    code.extend(op(0xb3, seen));
    code.push(0xb1);
    clinit(&mut parent, &code, 1);
    let mut child = Class::new("q/Child", Some("q/Parent"), 0x21);
    child.constant_int("value", "I", 9, 23);
    let mut fixture = Fixture::new(vec![parent, child]);
    let parent = fixture.classes["q/Parent"];
    let child = fixture.classes["q/Child"];
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(initialize(session, child).unwrap(), IO::Completed);
        assert_eq!(
            inspect(session, &statics, parent, "seen", "I"),
            Value::Int(23)
        );
        assert_eq!(
            inspect(session, &statics, child, "value", "I"),
            Value::Int(23)
        );
        assert_eq!(session.status(parent).unwrap(), IS::Completed);
        assert_eq!(session.status(child).unwrap(), IS::Completed);
    });
}

fn iface(name: &str) -> Class {
    Class::new(name, Some("java/lang/Object"), 0x601)
}
fn implement(class: &mut Class, name: &str) {
    let index = class.class(name);
    class.interfaces.push(index);
}
fn append_trace(class: &mut Class, digit: i8) {
    let trace = class.reference(9, "q/Trace", "value", "I");
    let mut code = op(0xb2, trace);
    code.extend([0x10, 10, 0x68, 0x10, digit as u8, 0x60]);
    code.extend(op(0xb3, trace));
    code.push(0xb1);
    clinit(class, &code, 2);
}
#[test]
fn recursive_c_to_d_to_c_observes_partial_state_without_reinitializing_or_resetting_constants() {
    let mut c = ordinary("q/C");
    c.field("value", "I", 9);
    c.field("seen", "I", 9);
    let cv = c.reference(9, "q/C", "value", "I");
    let cs = c.reference(9, "q/C", "seen", "I");
    let dv = c.reference(9, "q/D", "value", "I");
    let mut code = vec![0x04];
    code.extend(op(0xb3, cv));
    code.extend(op(0xb2, dv));
    code.extend(op(0xb3, cs));
    code.push(0x06);
    code.extend(op(0xb3, cv));
    code.push(0xb1);
    clinit(&mut c, &code, 1);
    let mut d = ordinary("q/D");
    d.field("value", "I", 9);
    d.field("seen", "I", 9);
    let cv = d.reference(9, "q/C", "value", "I");
    let ds = d.reference(9, "q/D", "seen", "I");
    let dv = d.reference(9, "q/D", "value", "I");
    let mut code = op(0xb2, cv);
    code.extend(op(0xb3, ds));
    code.push(0x05);
    code.extend(op(0xb3, dv));
    code.push(0xb1);
    clinit(&mut d, &code, 1);
    let mut fixture = Fixture::new(vec![c, d]);
    let c = fixture.classes["q/C"];
    let d = fixture.classes["q/D"];
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(initialize(session, c).unwrap(), IO::Completed);
        for (class, field, expected) in [
            (c, "value", 3),
            (c, "seen", 2),
            (d, "value", 2),
            (d, "seen", 1),
        ] {
            assert_eq!(
                inspect(session, &statics, class, field, "I"),
                Value::Int(expected)
            );
        }
        assert_eq!(session.status(c).unwrap(), IS::Completed);
        assert_eq!(session.status(d).unwrap(), IS::Completed);
        assert_eq!(initialize(session, d).unwrap(), IO::Completed);
        assert_eq!(session.last_initialization_steps(), 1);
    });
}
#[test]
fn superclass_then_postorder_default_interfaces_include_private_but_exclude_static_and_abstract() {
    let mut trace = ordinary("q/Trace");
    trace.field("value", "I", 9);
    let mut parent = ordinary("q/Parent");
    append_trace(&mut parent, 1);
    let mut i = iface("q/I");
    i.method("value", "()V", 1, &[0xb1], 0, 1);
    append_trace(&mut i, 2);
    let mut j = iface("q/J");
    implement(&mut j, "q/I");
    j.method("privateValue", "()V", 2, &[0xb1], 0, 1);
    append_trace(&mut j, 3);
    let mut abstract_only = iface("q/Abstract");
    abstract_only.method("value", "()V", 0x401, &[], 0, 1);
    append_trace(&mut abstract_only, 8);
    let mut static_only = iface("q/Static");
    static_only.method("value", "()V", 9, &[0xb1], 0, 0);
    append_trace(&mut static_only, 9);
    let mut child = Class::new("q/Child", Some("q/Parent"), 0x21);
    for name in ["q/J", "q/I", "q/Abstract", "q/Static"] {
        implement(&mut child, name);
    }
    append_trace(&mut child, 4);
    let mut fixture = Fixture::new(vec![trace, parent, i, j, abstract_only, static_only, child]);
    let ids = fixture.classes.clone();
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(initialize(session, ids["q/Child"]).unwrap(), IO::Completed);
        assert_eq!(
            inspect(session, &statics, ids["q/Trace"], "value", "I"),
            Value::Int(1234)
        );
        for name in ["q/Parent", "q/I", "q/J", "q/Child"] {
            assert_eq!(session.status(ids[name]).unwrap(), IS::Completed);
        }
        for name in ["q/Abstract", "q/Static"] {
            assert_eq!(session.status(ids[name]).unwrap(), IS::Uninitialized);
        }
    });
}
#[test]
fn direct_interface_skips_ancestors_but_later_class_plans_do_not_prune_completed_child_interfaces()
{
    let mut i = iface("q/I");
    i.field("value", "I", 0x19);
    i.method("value", "()V", 1, &[0xb1], 0, 1);
    set_initializer(&mut i, "value", 2);
    let mut j = iface("q/J");
    j.field("value", "I", 0x19);
    implement(&mut j, "q/I");
    j.method("value", "()V", 1, &[0xb1], 0, 1);
    set_initializer(&mut j, "value", 3);
    let mut root = ordinary("p/T");
    implement(&mut root, "q/J");
    let mut fixture = Fixture::new(vec![i, j, root]);
    let ids = fixture.classes.clone();
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(initialize(session, ids["q/J"]).unwrap(), IO::Completed);
        assert_eq!(session.status(ids["q/I"]).unwrap(), IS::Uninitialized);
        assert_eq!(
            session.status(ids["java/lang/Object"]).unwrap(),
            IS::Uninitialized
        );
        assert_eq!(
            inspect(session, &statics, ids["q/J"], "value", "I"),
            Value::Int(3)
        );
        assert_eq!(initialize(session, ids["p/T"]).unwrap(), IO::Completed);
        assert_eq!(session.status(ids["q/I"]).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, ids["q/I"], "value", "I"),
            Value::Int(2)
        );
        assert_eq!(
            inspect(session, &statics, ids["q/J"], "value", "I"),
            Value::Int(3)
        );
    });
}
#[test]
fn active_use_initializes_actual_field_declaration_and_b3_resumes_retained_value_once() {
    let mut parent = ordinary("q/Parent");
    parent.field("value", "I", 9);
    set_initializer(&mut parent, "value", 7);
    let mut child = Class::new("q/Child", Some("q/Parent"), 0x21);
    child.field("marker", "I", 9);
    set_initializer(&mut child, "marker", 9);
    let mut root = ordinary("p/T");
    let field = root.reference(9, "q/Child", "value", "I");
    let mut code = op(0xb2, field);
    code.push(0xac);
    root.method("read", "()I", 9, &code, 1, 0);
    let mut code = vec![0x1a];
    code.extend(op(0xb3, field));
    code.extend(op(0xb2, field));
    code.push(0xac);
    root.method("write", "(I)I", 9, &code, 1, 1);
    let mut fixture = Fixture::new(vec![parent, child, root]);
    let parent = fixture.classes["q/Parent"];
    let child = fixture.classes["q/Child"];
    let read = fixture.method("p/T", "read", "()I");
    let write = fixture.method("p/T", "write", "(I)I");
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(
            session
                .run_static(
                    write,
                    &[Value::Int(19)],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
                .unwrap(),
            EO::Returned(Some(Value::Int(19)))
        );
        assert_eq!(
            inspect(session, &statics, parent, "value", "I"),
            Value::Int(19)
        );
        assert_eq!(session.status(parent).unwrap(), IS::Completed);
        assert_eq!(session.status(child).unwrap(), IS::Uninitialized);
        assert_eq!(
            session
                .run_static(
                    read,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
                .unwrap(),
            EO::Returned(Some(Value::Int(19)))
        );
        assert_eq!(
            inspect(session, &statics, child, "marker", "I"),
            Value::Int(0)
        );
    });
}
#[test]
fn all_raw_numeric_constants_apply_without_code_or_frames_and_instance_attributes_are_ignored() {
    let mut root = ordinary("p/T");
    for (name, descriptor, value, flags) in [
        ("z", "Z", 6, 9),
        ("b", "B", 255, 0x19),
        ("c", "C", -1, 9),
        ("s", "S", 65534, 0x19),
        ("i", "I", i32::MIN, 9),
    ] {
        root.constant_int(name, descriptor, flags, value);
    }
    root.constant("j", "J", 9, 5, &i64::MIN.to_be_bytes());
    root.constant("f", "F", 9, 4, &0xffc00042u32.to_be_bytes());
    root.constant("d", "D", 0x19, 6, &0x8000000000000000u64.to_be_bytes());
    root.constant_int("instance", "I", 1, 99);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.classes["p/T"];
    let object = fixture.heap.allocate_instance(class).unwrap();
    let instance = fixture
        .registry
        .find_declared_field(class, &units("instance"), &units("I"))
        .unwrap()
        .unwrap();
    fixture.run(
        InitializationLimits {
            max_constant_values: 8,
            ..InitializationLimits::default()
        },
        |session, statics| {
            assert_eq!(
                session
                    .initialize(
                        class,
                        ExecutionOptions {
                            instruction_budget: 0
                        },
                        CallLimits {
                            max_frames: 0,
                            ..CallLimits::default()
                        },
                        ExceptionLimits::default()
                    )
                    .unwrap(),
                IO::Completed
            );
            for (name, descriptor, expected) in [
                ("z", "Z", Value::Int(0)),
                ("b", "B", Value::Int(-1)),
                ("c", "C", Value::Int(65535)),
                ("s", "S", Value::Int(-2)),
                ("i", "I", Value::Int(i32::MIN)),
                ("j", "J", Value::Long(i64::MIN)),
                ("f", "F", Value::Float(0xffc00042)),
                ("d", "D", Value::Double(0x8000000000000000)),
            ] {
                assert_eq!(
                    inspect(session, &statics, class, name, descriptor),
                    expected
                );
            }
            assert_eq!(
                session
                    .context()
                    .heap()
                    .get_field(object, instance)
                    .unwrap(),
                Value::Int(0)
            );
        },
    );
}
#[test]
fn historical_ignored_initializer_flags_are_semantic_static_and_parameterized_clinit_is_explicit() {
    for major in [50, 65] {
        let mut root = ordinary("p/T");
        root.major = major;
        root.field("value", "I", 9);
        let field = root.reference(9, "p/T", "value", "I");
        let mut code = vec![0x10, 7];
        code.extend(op(0xb3, field));
        code.push(0xb1);
        root.method(
            "<clinit>",
            "()V",
            if major == 50 { 0x520 } else { 0x528 },
            &code,
            1,
            0,
        );
        let mut fixture = Fixture::new(vec![root]);
        let class = fixture.classes["p/T"];
        fixture.run(InitializationLimits::default(), |session, statics| {
            assert_eq!(initialize(session, class).unwrap(), IO::Completed);
            assert_eq!(
                inspect(session, &statics, class, "value", "I"),
                Value::Int(7)
            );
        });
    }
    let mut root = ordinary("p/T");
    root.major = 50;
    root.method("<clinit>", "(I)V", 0, &[0xb1], 0, 1);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.classes["p/T"];
    fixture.run(InitializationLimits::default(),|session,_|{let error=initialize(session,class).unwrap_err();assert!(matches!(error.kind,E::InitializationAccess(error) if matches!(error.as_ref(),InitializationAccessError::UnsupportedInitializerDescriptor{..})));assert_eq!(error.initialization_class,Some(class));assert_eq!(session.status(class).unwrap(),IS::Uninitialized);});
}
#[test]
fn mixed_b8_arguments_are_retained_and_helpers_never_inherit_initializer_final_authority() {
    let mut root = ordinary("p/T");
    for (name, desc) in [
        ("j", "J"),
        ("r", "Ljava/lang/Object;"),
        ("d", "D"),
        ("i", "I"),
    ] {
        root.field(name, desc, 9);
    }
    let descriptor = "(JLjava/lang/Object;DI)J";
    let mut helper = Vec::new();
    for (load, name, desc) in [
        (vec![0x1e], "j", "J"),
        (vec![0x2c], "r", "Ljava/lang/Object;"),
        (vec![0x29], "d", "D"),
        (vec![0x15, 5], "i", "I"),
    ] {
        helper.extend(load);
        let field = root.reference(9, "p/T", name, desc);
        helper.extend(op(0xb3, field));
    }
    helper.extend([0x1e, 0xad]);
    root.method("helper", descriptor, 9, &helper, 2, 6);
    let call = root.reference(10, "p/T", "helper", descriptor);
    let mut init = vec![0x0a, 0x01, 0x0f, 0x06];
    init.extend(op(0xb8, call));
    init.extend([0x58, 0xb1]);
    clinit(&mut root, &init, 6);
    let mut code = vec![0x1e, 0x2c, 0x29, 0x15, 5];
    code.extend(op(0xb8, call));
    code.push(0xad);
    root.method("run", descriptor, 9, &code, 6, 6);
    let mut fixture = Fixture::new(vec![root]);
    let object = fixture
        .heap
        .allocate_instance(fixture.classes["q/Error"])
        .unwrap();
    let class = fixture.classes["p/T"];
    let method = fixture.method("p/T", "run", descriptor);
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(
            session
                .run_static(
                    method,
                    &[
                        Value::Long(i64::MIN),
                        Value::Reference(object),
                        Value::Double(0xfff8000000000042),
                        Value::Int(99)
                    ],
                    ExecutionOptions::default(),
                    CallLimits {
                        max_frames: 2,
                        max_frame_slots: 20,
                        ..CallLimits::default()
                    },
                    ExceptionLimits::default()
                )
                .unwrap(),
            EO::Returned(Some(Value::Long(i64::MIN)))
        );
        assert_eq!(
            inspect(session, &statics, class, "j", "J"),
            Value::Long(i64::MIN)
        );
        assert_eq!(
            inspect(session, &statics, class, "r", "Ljava/lang/Object;"),
            Value::Reference(object)
        );
        assert_eq!(
            inspect(session, &statics, class, "d", "D"),
            Value::Double(0xfff8000000000042)
        );
        assert_eq!(inspect(session, &statics, class, "i", "I"), Value::Int(99));
    });
    let mut root = ordinary("p/T");
    root.field("final", "I", 0x19);
    let field = root.reference(9, "p/T", "final", "I");
    let mut code = vec![0x04];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    root.method("helper", "()V", 9, &code, 1, 0);
    let call = root.reference(10, "p/T", "helper", "()V");
    let mut code = op(0xb8, call);
    code.push(0xb1);
    clinit(&mut root, &code, 0);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.classes["p/T"];
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(
            initialize(session, class).unwrap_err().kind,
            E::UnsupportedVmException(crabspot_runtime::VmExceptionKind::IllegalAccessError)
        );
        assert_eq!(session.status(class).unwrap(), IS::Aborted);
        assert_eq!(
            inspect(session, &statics, class, "final", "I"),
            Value::Int(0)
        );
    });
}

fn supplied_fixture(mut classes: Vec<Class>, thrown_class: &str) -> (Fixture, Reference) {
    let mut seed = ordinary("q/Seed");
    seed.field("cause", "Ljava/lang/Throwable;", 9);
    let field = seed.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
    classes.push(seed);
    let mut fixture = Fixture::new(classes);
    let thrown = fixture
        .heap
        .allocate_instance(fixture.classes[thrown_class])
        .unwrap();
    let (statics, _) = fixture.profiles();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    context
        .write_static(
            &statics,
            fixture.classes["q/Seed"],
            field,
            Value::Reference(thrown),
        )
        .unwrap();
    drop(context);
    (fixture, thrown)
}
fn throwing_initializer(class: &mut Class) {
    let cause = class.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
    let mut code = op(0xb2, cause);
    code.push(0xbf);
    clinit(class, &code, 1);
}
#[test]
fn escaping_error_marks_dependency_waiting_parents_erroneous_and_retains_duplicate_causes() {
    let mut failing = ordinary("q/Fail");
    throwing_initializer(&mut failing);
    let parent = Class::new("q/Parent", Some("q/Fail"), 0x21);
    let healthy = ordinary("q/Healthy");
    let (mut fixture, thrown) = supplied_fixture(vec![failing, parent, healthy], "q/Error");
    let ids = fixture.classes.clone();
    fixture.run(InitializationLimits::default(), |session, _| {
        assert_eq!(
            initialize(session, ids["q/Parent"]).unwrap(),
            IO::Uncaught(thrown)
        );
        for name in ["q/Fail", "q/Parent"] {
            assert_eq!(
                session.status(ids[name]).unwrap(),
                IS::Erroneous { cause: thrown }
            );
        }
        assert_eq!(
            session.retained_roots().collect::<Vec<_>>(),
            vec![thrown.heap_ref().unwrap(), thrown.heap_ref().unwrap()]
        );
        let error = initialize(session, ids["q/Parent"]).unwrap_err();
        assert_eq!(
            error.kind,
            E::UnsupportedNoClassDefFoundError {
                class: ids["q/Parent"]
            }
        );
        assert_eq!(error.initialization_class, Some(ids["q/Parent"]));
        assert_eq!(
            initialize(session, ids["q/Healthy"]).unwrap(),
            IO::Completed
        );
        assert_eq!(session.context().heap().allocated_instances(), 1);
    });
}
#[test]
fn non_error_initializer_throw_requires_wrapping_and_aborts_all_pending_classes_without_retry() {
    let mut failing = ordinary("q/Fail");
    throwing_initializer(&mut failing);
    let parent = Class::new("q/Parent", Some("q/Fail"), 0x21);
    let healthy = ordinary("q/Healthy");
    let (mut fixture, thrown) = supplied_fixture(vec![failing, parent, healthy], "q/Exception");
    let ids = fixture.classes.clone();
    fixture.run(InitializationLimits::default(), |session, _| {
        let error = initialize(session, ids["q/Parent"]).unwrap_err();
        assert_eq!(
            error.kind,
            E::UnsupportedInitializerWrapping {
                class: ids["q/Fail"],
                thrown
            }
        );
        assert_eq!(error.initialization_class, Some(ids["q/Fail"]));
        for name in ["q/Fail", "q/Parent"] {
            assert_eq!(session.status(ids[name]).unwrap(), IS::Aborted);
        }
        assert_eq!(session.retained_roots().count(), 0);
        assert_eq!(
            initialize(session, ids["q/Fail"]).unwrap_err().kind,
            E::InitializationAborted {
                class: ids["q/Fail"]
            }
        );
        assert_eq!(
            initialize(session, ids["q/Healthy"]).unwrap(),
            IO::Completed
        );
    });
}
#[test]
fn dependent_error_can_be_caught_inside_initializer_at_the_original_active_use_pc() {
    let mut failing = ordinary("q/Fail");
    failing.field("value", "I", 9);
    throwing_initializer(&mut failing);
    let mut parent = ordinary("q/Parent");
    parent.field("caught", "I", 9);
    let value = parent.reference(9, "q/Fail", "value", "I");
    let caught = parent.reference(9, "q/Parent", "caught", "I");
    let mut code = op(0xb2, value);
    code.extend([0x57, 0xb1, 0x57, 0x10, 7]);
    code.extend(op(0xb3, caught));
    code.push(0xb1);
    clinit(&mut parent, &code, 1);
    parent.methods.last_mut().unwrap().handlers = vec![[0, 3, 5, 0]];
    let (mut fixture, thrown) = supplied_fixture(vec![parent, failing], "q/Error");
    let ids = fixture.classes.clone();
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(initialize(session, ids["q/Parent"]).unwrap(), IO::Completed);
        assert_eq!(session.status(ids["q/Parent"]).unwrap(), IS::Completed);
        assert_eq!(
            session.status(ids["q/Fail"]).unwrap(),
            IS::Erroneous { cause: thrown }
        );
        assert_eq!(
            inspect(session, &statics, ids["q/Parent"], "caught", "I"),
            Value::Int(7)
        );
        assert_eq!(
            session.retained_roots().collect::<Vec<_>>(),
            vec![thrown.heap_ref().unwrap()]
        );
    });
}
#[test]
fn caught_non_error_inside_own_clinit_does_not_require_wrapping_and_can_complete() {
    let mut root = ordinary("p/T");
    let cause = root.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
    let mut code = op(0xb2, cause);
    code.extend([0xbf, 0x57, 0xb1]);
    clinit(&mut root, &code, 1);
    root.methods.last_mut().unwrap().handlers = vec![[0, 4, 4, 0]];
    let (mut fixture, _) = supplied_fixture(vec![root], "q/Exception");
    let class = fixture.classes["p/T"];
    fixture.run(InitializationLimits::default(), |session, _| {
        assert_eq!(initialize(session, class).unwrap(), IO::Completed);
        assert_eq!(session.status(class).unwrap(), IS::Completed);
        assert_eq!(session.retained_roots().count(), 0);
    });
}
#[test]
fn clinit_and_helper_host_failures_attach_actual_class_without_rewriting_method_or_pc() {
    for helper in [false, true] {
        let mut root = ordinary("p/T");
        root.field("touch", "I", 9);
        let touch = root.reference(9, "p/T", "touch", "I");
        let mut code = vec![0x10, 7];
        code.extend(op(0xb3, touch));
        if helper {
            root.method("helper", "()V", 0x109, &[], 0, 0);
            let call = root.reference(10, "p/T", "helper", "()V");
            code.extend(op(0xb8, call));
        } else {
            code.push(0xc2);
        }
        code.push(0xb1);
        clinit(&mut root, &code, 1);
        let healthy = ordinary("q/Healthy");
        let mut fixture = Fixture::new(vec![root, healthy]);
        let class = fixture.classes["p/T"];
        let healthy = fixture.classes["q/Healthy"];
        let expected = fixture.method("p/T", if helper { "helper" } else { "<clinit>" }, "()V");
        fixture.run(InitializationLimits::default(), |session, statics| {
            let error = initialize(session, class).unwrap_err();
            assert_eq!(error.initialization_class, Some(class));
            assert_eq!(error.method.as_ref().unwrap().index, expected.index());
            if helper {
                assert_eq!(
                    error.kind,
                    E::UnsupportedMethod("native methods require runtime services")
                );
                assert_eq!(error.call_site.as_ref().unwrap().offset, 5);
            } else {
                assert_eq!(error.kind, E::UnsupportedInstruction(0xc2));
                assert_eq!(error.offset, Some(5));
            }
            assert_eq!(session.status(class).unwrap(), IS::Aborted);
            assert_eq!(
                inspect(session, &statics, class, "touch", "I"),
                Value::Int(7)
            );
            assert_eq!(initialize(session, healthy).unwrap(), IO::Completed);
        });
    }
}
#[test]
fn fuel_interrupt_after_constants_preserves_values_aborts_only_active_records_and_never_retries() {
    let mut root = ordinary("p/T");
    root.constant_int("value", "I", 9, 23);
    clinit(&mut root, &[0xb1], 0);
    let healthy = ordinary("q/Healthy");
    let mut fixture = Fixture::new(vec![root, healthy]);
    let ids = fixture.classes.clone();
    fixture.run(InitializationLimits::default(), |session, statics| {
        let error = session
            .initialize(
                ids["p/T"],
                ExecutionOptions {
                    instruction_budget: 0,
                },
                CallLimits::default(),
                ExceptionLimits::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind, E::InstructionBudgetExhausted { limit: 0 });
        assert_eq!(error.initialization_class, Some(ids["p/T"]));
        assert_eq!(error.offset, Some(0));
        assert_eq!(session.status(ids["p/T"]).unwrap(), IS::Aborted);
        assert_eq!(
            session.status(ids["java/lang/Object"]).unwrap(),
            IS::Completed
        );
        assert_eq!(
            inspect(session, &statics, ids["p/T"], "value", "I"),
            Value::Int(23)
        );
        assert_eq!(
            initialize(session, ids["p/T"]).unwrap_err().kind,
            E::InitializationAborted { class: ids["p/T"] }
        );
        assert_eq!(
            session
                .initialize(
                    ids["q/Healthy"],
                    ExecutionOptions {
                        instruction_budget: 0
                    },
                    CallLimits {
                        max_frames: 0,
                        ..CallLimits::default()
                    },
                    ExceptionLimits::default()
                )
                .unwrap(),
            IO::Completed
        );
    });
}
#[test]
fn ordinary_root_failures_after_successful_initialization_are_not_labeled_initializer_failures() {
    let mut root = ordinary("p/T");
    root.method("run", "()I", 9, &[0x04, 0x03, 0x6c, 0xac], 2, 0);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.classes["p/T"];
    let method = fixture.method("p/T", "run", "()I");
    fixture.run(InitializationLimits::default(), |session, _| {
        let error = session
            .run_static(
                method,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits::default(),
            )
            .unwrap_err();
        assert_eq!(
            error.kind,
            E::UnsupportedVmException(crabspot_runtime::VmExceptionKind::ArithmeticException)
        );
        assert_eq!(error.initialization_class, None);
        assert_eq!(session.status(class).unwrap(), IS::Completed);
    });
}

#[test]
fn every_vm_production_route_aborts_initialization_even_with_a_catch_all() {
    use crabspot_runtime::VmExceptionKind as VM;
    for failure in 0..12 {
        let mut other = ordinary("q/Other");
        other.field("package", "I", 8);
        let mut i = iface("q/I");
        i.method("absent", "()I", 0x401, &[], 0, 1);
        let mut error_class = Class::new("q/Error", Some("java/lang/Error"), 0x21);
        implement(&mut error_class, "q/I");
        let mut root = ordinary("p/T");
        root.field("touch", "I", 9);
        root.field("instance", "I", 1);
        let touch = root.reference(9, "p/T", "touch", "I");
        let cause = root.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
        let array = root.class("[I");
        let component = root.class("q/Exception");
        let missing = root.reference(9, "p/T", "missing", "I");
        let hidden = root.reference(9, "q/Other", "package", "I");
        let instance = root.reference(9, "p/T", "instance", "I");
        let method = root.reference(10, "java/lang/Throwable", "missing", "()I");
        let abstract_method = root.reference(11, "q/I", "absent", "()I");
        let mut code = vec![0x10, 7];
        code.extend(op(0xb3, touch));
        let expected = match failure {
            0 => {
                code.extend([0x04, 0x03, 0x6c, 0x57]);
                VM::ArithmeticException
            }
            1 => {
                code.extend(op(0xb2, cause));
                code.extend(op(0xc0, array));
                code.push(0x57);
                VM::ClassCastException
            }
            2 => {
                code.push(0x04);
                code.extend(op(0xbd, component));
                code.push(0x03);
                code.extend(op(0xb2, cause));
                code.push(0x53);
                VM::ArrayStoreException
            }
            3 => {
                code.extend([0x01, 0xbe, 0x57]);
                VM::NullPointerException
            }
            4 => {
                code.extend([0x03, 0xbc, 10, 0x03, 0x2e, 0x57]);
                VM::ArrayIndexOutOfBoundsException
            }
            5 => {
                code.extend([0x02, 0xbc, 10, 0x57]);
                VM::NegativeArraySizeException
            }
            6 => {
                code.extend(op(0xb2, missing));
                code.push(0x57);
                VM::NoSuchFieldError
            }
            7 => {
                code.extend(op(0xb2, hidden));
                code.push(0x57);
                VM::IllegalAccessError
            }
            8 => {
                code.extend(op(0xb2, instance));
                code.push(0x57);
                VM::IncompatibleClassChangeError
            }
            9 => {
                code.extend(op(0xb2, cause));
                code.extend(op(0xb6, method));
                code.push(0x57);
                VM::NoSuchMethodError
            }
            10 => {
                code.extend(op(0xb2, cause));
                code.extend(op(0xb9, abstract_method));
                code.extend([1, 0, 0x57]);
                VM::AbstractMethodError
            }
            _ => {
                code.extend([0x01, 0xbf]);
                VM::NullPointerException
            }
        };
        let end = code.len() as u16;
        code.extend([0xb1, 0x57, 0xb1]);
        clinit(&mut root, &code, 3);
        root.methods.last_mut().unwrap().handlers = vec![[0, end, end + 1, 0]];
        let (mut fixture, _) = supplied_fixture(vec![root, other, i, error_class], "q/Error");
        let ids = fixture.classes.clone();
        fixture.run(InitializationLimits::default(), |session, statics| {
            let error = session
                .initialize(
                    ids["p/T"],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits {
                        max_dispatch_steps: 0,
                        ..ExceptionLimits::default()
                    },
                )
                .unwrap_err();
            assert_eq!(
                error.kind,
                E::UnsupportedVmException(expected),
                "case{failure}"
            );
            assert_eq!(error.initialization_class, Some(ids["p/T"]));
            assert_eq!(session.status(ids["p/T"]).unwrap(), IS::Aborted);
            assert_eq!(
                inspect(session, &statics, ids["p/T"], "touch", "I"),
                Value::Int(7)
            );
            for id in ids.values() {
                assert_ne!(session.status(*id).unwrap(), IS::InProgress);
            }
            assert_eq!(session.retained_roots().count(), 0);
        });
    }
}
#[test]
fn host_callback_panic_aborts_active_records_without_translating_or_stranding_state() {
    struct Panics;
    impl crabspot_runtime::HierarchyResolver for Panics {
        fn resolve(
            &mut self,
            registry: &DefinitionRegistry,
            loader: crabspot_runtime::LoaderId,
            name: &[u16],
        ) -> Result<Option<ClassId>, crabspot_runtime::ResolveError> {
            if name == units("q/Missing") {
                panic!("intentional initialization resolver panic")
            };
            registry
                .lookup_class(loader, name)
                .map_err(|error| crabspot_runtime::ResolveError::new(error.to_string()))
        }
    }
    let mut root = ordinary("p/T");
    root.field("touch", "I", 9);
    let touch = root.reference(9, "p/T", "touch", "I");
    let missing = root.reference(9, "q/Missing", "value", "I");
    let mut code = vec![0x10, 7];
    code.extend(op(0xb3, touch));
    code.extend(op(0xb2, missing));
    code.extend([0x57, 0xb1]);
    clinit(&mut root, &code, 1);
    let healthy = ordinary("q/Healthy");
    let mut fixture = Fixture::new(vec![root, healthy]);
    let class = fixture.classes["p/T"];
    let healthy = fixture.classes["q/Healthy"];
    let (statics, exceptions) = fixture.profiles();
    let mut resolver = Panics;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let mut session = InitializationSession::new(
        &mut context,
        &statics,
        &exceptions,
        fixture.classes["java/lang/Error"],
        InitializationLimits::default(),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        initialize(&mut session, class)
    }));
    assert!(result.is_err());
    assert_eq!(session.status(class).unwrap(), IS::Aborted);
    assert_eq!(
        inspect(&session, &statics, class, "touch", "I"),
        Value::Int(7)
    );
    assert!(session.last_initialization_steps() > 0);
    assert_eq!(initialize(&mut session, healthy).unwrap(), IO::Completed);
    assert_eq!(
        initialize(&mut session, class).unwrap_err().kind,
        E::InitializationAborted { class }
    );
}
#[test]
fn exact_fuel_does_not_replay_active_use_and_completed_initialization_survives_root_failure() {
    for budget in [0, 2, 6, 7] {
        let mut target = ordinary("q/Target");
        target.field("value", "I", 9);
        set_initializer(&mut target, "value", 7);
        let mut root = ordinary("p/T");
        let value = root.reference(9, "q/Target", "value", "I");
        let mut code = vec![0x10, 9];
        code.extend(op(0xb3, value));
        code.extend(op(0xb2, value));
        code.push(0xac);
        root.method("run", "()I", 9, &code, 1, 0);
        let mut fixture = Fixture::new(vec![root, target]);
        let method = fixture.method("p/T", "run", "()I");
        let root = fixture.classes["p/T"];
        let target = fixture.classes["q/Target"];
        fixture.run(InitializationLimits::default(), |session, statics| {
            let result = session.run_static(
                method,
                &[],
                ExecutionOptions {
                    instruction_budget: budget,
                },
                CallLimits::default(),
                ExceptionLimits::default(),
            );
            if budget == 7 {
                assert_eq!(result.unwrap(), EO::Returned(Some(Value::Int(9))));
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind, E::InstructionBudgetExhausted { limit: budget });
                assert_eq!(
                    error.initialization_class,
                    if budget == 2 { Some(target) } else { None }
                );
            }
            assert_eq!(session.status(root).unwrap(), IS::Completed);
            assert_eq!(
                session.status(target).unwrap(),
                match budget {
                    0 => IS::Uninitialized,
                    2 => IS::Aborted,
                    _ => IS::Completed,
                }
            );
            assert_eq!(
                inspect(session, &statics, target, "value", "I"),
                Value::Int(if budget >= 6 { 9 } else { 0 })
            );
        });
    }
}
#[test]
fn own_clinit_final_write_is_allowed_but_a_later_ordinary_static_body_has_no_authority() {
    let mut root = ordinary("p/T");
    root.field("final", "I", 0x19);
    set_initializer(&mut root, "final", 7);
    let field = root.reference(9, "p/T", "final", "I");
    let mut code = vec![0x10, 9];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    root.method("write", "()V", 9, &code, 1, 0);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.classes["p/T"];
    let write = fixture.method("p/T", "write", "()V");
    let initializer = fixture.method("p/T", "<clinit>", "()V");
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(initialize(session, class).unwrap(), IO::Completed);
        assert_eq!(
            inspect(session, &statics, class, "final", "I"),
            Value::Int(7)
        );
        assert_eq!(
            session
                .run_static(
                    write,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
                .unwrap_err()
                .kind,
            E::UnsupportedVmException(crabspot_runtime::VmExceptionKind::IllegalAccessError)
        );
        assert_eq!(session.status(class).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, class, "final", "I"),
            Value::Int(7)
        );
        assert!(matches!(
            session
                .run_static(
                    initializer,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
                .unwrap_err()
                .kind,
            E::UnsupportedMethod(_)
        ));
    });
}

#[test]
fn reference_store_validation_is_deferred_until_target_initialization_and_preserves_completed_effects()
 {
    let mut target = ordinary("q/Target");
    target.field("reference", "Lq/Error;", 9);
    let target_ref = target.reference(9, "q/Target", "reference", "Lq/Error;");
    let source = target.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
    let mut code = op(0xb2, source);
    code.extend(op(0xb3, target_ref));
    code.push(0xb1);
    clinit(&mut target, &code, 1);
    let mut root = ordinary("p/T");
    let field = root.reference(9, "q/Target", "reference", "Lq/Error;");
    let mut code = vec![0x2a];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    root.method("run", "(Ljava/lang/Object;)V", 9, &code, 1, 1);
    let (mut fixture, supplied) = supplied_fixture(vec![target, root], "q/Error");
    let wrong = fixture
        .heap
        .allocate_instance(fixture.classes["q/Exception"])
        .unwrap();
    let target = fixture.classes["q/Target"];
    let root = fixture.classes["p/T"];
    let method = fixture.method("p/T", "run", "(Ljava/lang/Object;)V");
    fixture.run(InitializationLimits::default(),|session,statics|{let error=session.run_static(method,&[Value::Reference(wrong)],ExecutionOptions::default(),CallLimits::default(),ExceptionLimits::default()).unwrap_err();assert!(matches!(error.kind,E::InitializationAccess(error) if matches!(error.as_ref(),InitializationAccessError::Static(error) if matches!(error.as_ref(),crabspot_runtime::StaticAccessError::IncompatibleReference(_)))));assert_eq!(error.initialization_class,Some(target));assert_eq!(error.offset,Some(1));assert_eq!(session.status(target).unwrap(),IS::Completed);assert_eq!(session.status(root).unwrap(),IS::Completed);assert_eq!(inspect(session,&statics,target,"reference","Lq/Error;"),Value::Reference(supplied));});
}
#[test]
fn read_push_overflow_after_initialization_does_not_abort_the_completed_target() {
    let mut target = ordinary("q/Target");
    target.field("wide", "J", 9);
    let field = target.reference(9, "q/Target", "wide", "J");
    let mut code = vec![0x0a];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    clinit(&mut target, &code, 2);
    let mut root = ordinary("p/T");
    let field = root.reference(9, "q/Target", "wide", "J");
    let mut code = op(0xb2, field);
    code.push(0xad);
    root.method("run", "()J", 9, &code, 1, 0);
    let mut fixture = Fixture::new(vec![target, root]);
    let target = fixture.classes["q/Target"];
    let method = fixture.method("p/T", "run", "()J");
    fixture.run(InitializationLimits::default(), |session, statics| {
        let error = session
            .run_static(
                method,
                &[],
                ExecutionOptions::default(),
                CallLimits {
                    max_frames: 2,
                    max_frame_slots: 3,
                    ..CallLimits::default()
                },
                ExceptionLimits::default(),
            )
            .unwrap_err();
        assert_eq!(
            error.kind,
            E::StackOverflow {
                required: 2,
                capacity: 1
            }
        );
        assert_eq!(error.method.as_ref().unwrap().index, method.index());
        assert_eq!(error.offset, Some(0));
        assert_eq!(error.initialization_class, Some(target));
        assert_eq!(session.status(target).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, target, "wide", "J"),
            Value::Long(1)
        );
    });
}
#[test]
fn abrupt_b3_discards_saved_store_and_reuses_token_buffer_after_an_ordinary_caller_catches_error() {
    let mut failing = ordinary("q/Fail");
    failing.field("value", "I", 9);
    let value = failing.reference(9, "q/Fail", "value", "I");
    let cause = failing.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
    let mut code = vec![0x10, 9];
    code.extend(op(0xb3, value));
    code.extend(op(0xb2, cause));
    code.push(0xbf);
    clinit(&mut failing, &code, 1);
    let mut good = ordinary("q/Good");
    good.constant_int("value", "I", 9, 5);
    let mut root = ordinary("p/T");
    let bad = root.reference(9, "q/Fail", "value", "I");
    let good_value = root.reference(9, "q/Good", "value", "I");
    let mut code = vec![0x1a];
    code.extend(op(0xb3, bad));
    code.extend([0x03, 0xac, 0x57, 0x1a]);
    code.extend(op(0xb2, good_value));
    code.extend([0x60, 0xac]);
    root.method("run", "(I)I", 9, &code, 2, 1);
    root.methods.last_mut().unwrap().handlers = vec![[1, 4, 6, 0]];
    let (mut fixture, thrown) = supplied_fixture(vec![failing, good, root], "q/Error");
    let ids = fixture.classes.clone();
    let method = fixture.method("p/T", "run", "(I)I");
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(
            session
                .run_static(
                    method,
                    &[Value::Int(2)],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
                .unwrap(),
            EO::Returned(Some(Value::Int(7)))
        );
        assert_eq!(
            session.status(ids["q/Fail"]).unwrap(),
            IS::Erroneous { cause: thrown }
        );
        assert_eq!(session.status(ids["p/T"]).unwrap(), IS::Completed);
        assert_eq!(session.status(ids["q/Good"]).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, ids["q/Fail"], "value", "I"),
            Value::Int(9)
        );
    });
}
#[test]
fn exact_scheduler_begin_body_and_completion_step_failures_leave_distinct_states() {
    for (has_body, limit) in [
        (false, 0),
        (false, 1),
        (false, 2),
        (false, 3),
        (false, 4),
        (true, 3),
        (true, 4),
    ] {
        let mut root = iface("q/I");
        if has_body {
            clinit(&mut root, &[0xb1], 0);
        }
        let mut fixture = Fixture::new(vec![root]);
        let class = fixture.classes["q/I"];
        fixture.run(
            InitializationLimits {
                max_steps: limit,
                ..InitializationLimits::default()
            },
            |session, _| {
                let result = initialize(session, class);
                if limit == 4 {
                    assert_eq!(result.unwrap(), IO::Completed);
                    assert_eq!(session.status(class).unwrap(), IS::Completed);
                    assert_eq!(session.last_initialization_steps(), 4);
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(
                        error.kind,
                        E::InitializationStepLimit {
                            limit,
                            required: limit + 1
                        }
                    );
                    assert_eq!(error.initialization_class, Some(class));
                    assert_eq!(error.offset, None);
                    assert_eq!(error.method, None);
                    assert_eq!(
                        session.status(class).unwrap(),
                        if limit < 2 {
                            IS::Uninitialized
                        } else {
                            IS::Aborted
                        }
                    );
                    assert_eq!(session.last_initialization_steps(), limit);
                }
            },
        );
    }
}
#[test]
fn ten_thousand_no_body_dependencies_schedule_and_drop_iteratively_without_java_frames() {
    let count = 10_000usize;
    let mut classes = Vec::new();
    for index in 0..count {
        let name = format!("q/C{index}");
        let parent = if index == 0 {
            "java/lang/Object".to_string()
        } else {
            format!("q/C{}", index - 1)
        };
        classes.push(Class::new(&name, Some(&parent), 0x21));
    }
    let root_name = format!("q/C{}", count - 1);
    let mut fixture = Fixture::selected(
        classes,
        &[
            &root_name,
            "java/lang/Error",
            "java/lang/Cloneable",
            "java/io/Serializable",
        ],
    );
    let root = fixture.classes[&root_name];
    let limits = InitializationLimits {
        max_classes: count + 5,
        max_plan_dependencies: count,
        max_constant_values: 0,
        max_pending: count + 1,
        max_steps: (count as u64) * 5 + 4,
    };
    fixture.run(limits, |session, _| {
        assert_eq!(
            session
                .initialize(
                    root,
                    ExecutionOptions {
                        instruction_budget: 0
                    },
                    CallLimits {
                        max_frames: 0,
                        max_frame_slots: 0,
                        max_prepared_code_bytes: 0
                    },
                    ExceptionLimits {
                        max_prepared_handlers: 0,
                        max_dispatch_steps: 0
                    }
                )
                .unwrap(),
            IO::Completed
        );
        assert_eq!(session.last_initialization_steps(), (count as u64) * 5 + 4);
        assert_eq!(session.status(root).unwrap(), IS::Completed);
        assert_eq!(initialize(session, root).unwrap(), IO::Completed);
        assert_eq!(session.last_initialization_steps(), 1);
        assert_eq!(session.context().heap().allocated_instances(), 0);
        assert_eq!(session.context().heap().allocated_arrays(), 0);
    });
}

#[test]
fn string_constant_and_malformed_initializer_preflight_publish_neither_constants_nor_record() {
    for malformed in [false, true] {
        let mut root = ordinary("p/T");
        root.constant_int("number", "I", 9, 23);
        if malformed {
            clinit(&mut root, &[0xff], 0);
        } else {
            let text = root.utf8("unmaterialized");
            root.constant("text", "Ljava/lang/String;", 0x19, 8, &text.to_be_bytes());
        }
        let mut fixture = Fixture::new(vec![root, ordinary("q/Healthy")]);
        let class = fixture.classes["p/T"];
        let healthy = fixture.classes["q/Healthy"];
        fixture.run(InitializationLimits::default(), |session, statics| {
            let error = initialize(session, class).unwrap_err();
            if malformed {
                assert!(matches!(error.kind, E::Decode(_)));
                assert!(error.method.as_ref().unwrap().name.equals_ascii("<clinit>"));
            } else {
                assert!(matches!(error.kind, E::InitializationAccess(ref error) if matches!(**error, InitializationAccessError::UnsupportedStringConstant { .. })));
                assert_eq!(error.method, None);
                assert_eq!(error.offset, None);
            }
            assert_eq!(error.initialization_class, Some(class));
            assert_eq!(session.status(class).unwrap(), IS::Uninitialized);
            assert_eq!(inspect(session, &statics, class, "number", "I"), Value::Int(0));
            assert_eq!(initialize(session, healthy).unwrap(), IO::Completed);
        });
    }
}
#[test]
fn a_different_initializer_cannot_write_final_or_trigger_target_initialization() {
    let mut target = ordinary("q/Target");
    target.constant_int("value", "I", 0x19, 23);
    let mut root = ordinary("p/T");
    let field = root.reference(9, "q/Target", "value", "I");
    let mut code = vec![0x10, 9];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    clinit(&mut root, &code, 1);
    let mut fixture = Fixture::new(vec![root, target]);
    let class = fixture.classes["p/T"];
    let target = fixture.classes["q/Target"];
    fixture.run(InitializationLimits::default(), |session, statics| {
        let error = initialize(session, class).unwrap_err();
        assert_eq!(
            error.kind,
            E::UnsupportedVmException(crabspot_runtime::VmExceptionKind::IllegalAccessError)
        );
        assert_eq!(error.initialization_class, Some(class));
        assert_eq!(error.offset, Some(2));
        assert_eq!(session.status(class).unwrap(), IS::Aborted);
        assert_eq!(session.status(target).unwrap(), IS::Uninitialized);
        assert_eq!(
            inspect(session, &statics, target, "value", "I"),
            Value::Int(0)
        );
    });
}
#[test]
fn propagation_budget_failure_preserves_child_error_and_aborts_waiting_parent() {
    fn setup() -> (Fixture, Reference) {
        let mut failing = ordinary("q/Fail");
        let cause = failing.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
        let mut code = op(0xb2, cause);
        code.push(0xbf);
        clinit(&mut failing, &code, 1);
        let parent = Class::new("p/T", Some("q/Fail"), 0x21);
        supplied_fixture(vec![failing, parent], "q/Error")
    }
    let (mut probe, _) = setup();
    let parent = probe.classes["p/T"];
    let completed_steps = probe.run(InitializationLimits::default(), |session, _| {
        assert!(matches!(
            initialize(session, parent).unwrap(),
            IO::Uncaught(_)
        ));
        session.last_initialization_steps()
    });
    let (mut fixture, thrown) = setup();
    let parent = fixture.classes["p/T"];
    let child = fixture.classes["q/Fail"];
    fixture.run(
        InitializationLimits {
            max_steps: completed_steps - 1,
            ..InitializationLimits::default()
        },
        |session, _| {
            let error = initialize(session, parent).unwrap_err();
            assert!(matches!(error.kind, E::InitializationStepLimit { .. }));
            assert_eq!(session.status(parent).unwrap(), IS::Aborted);
            assert_eq!(
                session.status(child).unwrap(),
                IS::Erroneous { cause: thrown }
            );
            assert_eq!(
                session.retained_roots().collect::<Vec<_>>(),
                vec![thrown.heap_ref().unwrap()]
            );
        },
    );
}

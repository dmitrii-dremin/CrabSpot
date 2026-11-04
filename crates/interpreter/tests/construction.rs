//! Dynamic session construction protocol; fixtures supply actual Object Code.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ConstructionLimits, ExceptionLimits, ExceptionOutcome as EO, ExecutionError,
    ExecutionErrorKind as E, ExecutionOptions, InitializationLimits, InitializationOutcome as IO,
    InitializationSession, InitializationStatus as IS, Value,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, ExceptionProfile, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap, MethodId, Reference,
    ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver, StaticStateOptions,
    StaticStateProfile,
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

fn object_class() -> Class {
    let mut object = Class::new("java/lang/Object", None, 0x21);
    object.method("<init>", "()V", 1, &[0xb1], 0, 1);
    object
}
#[test]
fn actual_constructor_code_chains_and_promotes_factory_alias_under_exact_limits() {
    let mut target = ordinary("q/Target");
    target.field("value", "I", 0x11);
    let object = target.reference(10, "java/lang/Object", "<init>", "()V");
    let field = target.reference(9, "q/Target", "value", "I");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, object));
    code.extend([0x2a, 0x1b]);
    code.extend(op(0xb5, field));
    code.push(0xb1);
    target.method("<init>", "(I)V", 1, &code, 2, 2);
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let constructor = root.reference(10, "q/Target", "<init>", "(I)V");
    let mut code = op(0xbb, class);
    code.extend([0x59, 0x1a]);
    code.extend(op(0xb7, constructor));
    code.push(0xb0);
    root.method("make", "(I)Lq/Target;", 9, &code, 3, 1);
    let mut fixture = Fixture::new(vec![root, target, object_class()]);
    let target = fixture.classes["q/Target"];
    let method = fixture.method("p/T", "make", "(I)Lq/Target;");
    let field = fixture
        .registry
        .find_declared_field(target, &units("value"), &units("I"))
        .unwrap()
        .unwrap();
    fixture.run(InitializationLimits::default(), |session, _| {
        let outcome = session
            .run_static_with_construction_limits(
                method,
                &[Value::Int(7)],
                ExecutionOptions {
                    instruction_budget: 12,
                },
                CallLimits {
                    max_frames: 3,
                    max_frame_slots: 9,
                    max_prepared_code_bytes: 20,
                },
                ExceptionLimits::default(),
                ConstructionLimits { max_alias_steps: 8 },
            )
            .unwrap();
        let EO::Returned(Some(Value::Reference(reference))) = outcome else {
            panic!("expected constructed reference")
        };
        assert_eq!(
            session
                .context()
                .heap()
                .get_field(reference, field)
                .unwrap(),
            Value::Int(7)
        );
        assert_eq!(session.context().heap().allocated_instances(), 1);
        assert_eq!(session.last_construction_steps(), 8);
        assert_eq!(session.status(target).unwrap(), IS::Completed);
    });
}

fn simple_constructor(class: &mut Class, parent: &str) {
    let call = class.reference(10, parent, "<init>", "()V");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, call));
    code.push(0xb1);
    class.method("<init>", "()V", 1, &code, 1, 1);
}
fn run(
    session: &mut InitializationSession<'_, '_>,
    method: MethodId,
    args: &[Value],
) -> Result<EO, ExecutionError> {
    session.run_static(
        method,
        args,
        ExecutionOptions::default(),
        CallLimits::default(),
        ExceptionLimits::default(),
    )
}
#[test]
fn constructor_chain_obligation_survives_lost_local_zero_and_other_completed_allocations() {
    for scenario in 0..3 {
        let mut target = ordinary("q/Target");
        let mut other = ordinary("q/Other");
        simple_constructor(&mut other, "java/lang/Object");
        let code = match scenario {
            0 => vec![0xb1],
            1 => vec![0x01, 0x4b, 0xb1],
            _ => {
                let class = target.class("q/Other");
                let ctor = target.reference(10, "q/Other", "<init>", "()V");
                let mut code = op(0xbb, class);
                code.push(0x59);
                code.extend(op(0xb7, ctor));
                code.extend([0x57, 0xb1]);
                code
            }
        };
        target.method("<init>", "()V", 1, &code, 2, 1);
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let ctor = root.reference(10, "q/Target", "<init>", "()V");
        let mut code = op(0xbb, class);
        code.push(0x59);
        code.extend(op(0xb7, ctor));
        code.push(0xb0);
        root.method("make", "()Lq/Target;", 9, &code, 2, 0);
        let mut fixture = Fixture::new(vec![root, target, other, object_class()]);
        let method = fixture.method("p/T", "make", "()Lq/Target;");
        let target = fixture.classes["q/Target"];
        fixture.run(InitializationLimits::default(), |session, _| {
            let error = run(session, method, &[]).unwrap_err();
            assert!(matches!(error.kind, E::ConstructionProtocol { .. }));
            assert_eq!(error.method.unwrap().owner, Some(target));
            assert_eq!(session.status(target).unwrap(), IS::Completed);
            assert_eq!(
                session.context().heap().allocated_instances(),
                if scenario == 2 { 2 } else { 1 }
            );
        });
    }
}
#[test]
fn this_delegation_and_early_final_write_promote_only_the_callers_actual_this_key() {
    let mut target = ordinary("q/Target");
    target.field("value", "I", 0x11);
    let field = target.reference(9, "q/Target", "value", "I");
    let object = target.reference(10, "java/lang/Object", "<init>", "()V");
    let this = target.reference(10, "q/Target", "<init>", "(I)V");
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb7, this));
    code.push(0xb1);
    target.method("<init>", "()V", 1, &code, 2, 1);
    let mut code = vec![0x2a, 0x1b];
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb7, object));
    code.push(0xb1);
    target.method("<init>", "(I)V", 1, &code, 2, 2);
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Lq/Target;", 9, &code, 2, 0);
    let mut fixture = Fixture::new(vec![root, target, object_class()]);
    let method = fixture.method("p/T", "make", "()Lq/Target;");
    let field = fixture
        .registry
        .find_declared_field(fixture.classes["q/Target"], &units("value"), &units("I"))
        .unwrap()
        .unwrap();
    fixture.run(InitializationLimits::default(), |session, _| {
        let EO::Returned(Some(Value::Reference(reference))) = run(session, method, &[]).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            session
                .context()
                .heap()
                .get_field(reference, field)
                .unwrap(),
            Value::Int(7)
        );
        assert_eq!(session.context().heap().allocated_instances(), 1);
    });
}
#[test]
fn unfinished_values_cannot_escape_through_ordinary_services_or_be_used_as_numeric_values() {
    for scenario in 0..13 {
        let mut target = ordinary("q/Target");
        target.field("field", "I", 1);
        target.method("value", "()I", 1, &[0x04, 0xac], 1, 1);
        target.method("accept", "(Ljava/lang/Object;)I", 9, &[0x04, 0xac], 1, 1);
        let mut interface = Class::new("q/I", Some("java/lang/Object"), 0x601);
        interface.method("value", "()I", 1, &[0x04, 0xac], 1, 1);
        let interface_index = target.class("q/I");
        target.interfaces.push(interface_index);
        let mut root = ordinary("p/T");
        root.field("reference", "Ljava/lang/Object;", 9);
        let class = root.class("q/Target");
        let field = root.reference(9, "q/Target", "field", "I");
        let static_field = root.reference(9, "p/T", "reference", "Ljava/lang/Object;");
        let virtual_call = root.reference(10, "q/Target", "value", "()I");
        let interface_call = root.reference(11, "q/I", "value", "()I");
        let accept = root.reference(10, "q/Target", "accept", "(Ljava/lang/Object;)I");
        let mut code = if scenario == 5 {
            vec![0x01]
        } else if scenario == 6 {
            vec![0x01, 0x03]
        } else {
            vec![]
        };
        code.extend(op(0xbb, class));
        let operation = code.len();
        match scenario {
            0 => code.push(0xb0),
            1 => code.extend(op(0xc0, class)),
            2 => code.extend(op(0xc1, class)),
            3 => code.extend(op(0xb4, field)),
            4 => code.extend(op(0xb3, static_field)),
            5 => code.extend(op(0xb5, static_field)),
            6 => code.push(0x53),
            7 => code.push(0xbf),
            8 => code.extend(op(0xb6, virtual_call)),
            9 => code.extend(op(0xb8, accept)),
            10 => code.extend([
                0xb9,
                (interface_call >> 8) as u8,
                interface_call as u8,
                1,
                0,
            ]),
            11 => code.extend(op(0xb7, virtual_call)),
            _ => code.push(0x74),
        }
        code.extend([0x03, 0xac]);
        let handler = code.len();
        code.extend(if scenario == 0 {
            vec![0x57, 0x01, 0xb0]
        } else {
            vec![0x57, 0x04, 0xac]
        });
        let descriptor = if scenario == 0 {
            "()Ljava/lang/Object;"
        } else {
            "()I"
        };
        root.method("run", descriptor, 9, &code, 3, 0);
        root.methods.last_mut().unwrap().handlers =
            vec![[operation as u16, (operation + 1) as u16, handler as u16, 0]];
        // End points must be instruction boundaries even though the host error is uncaught.
        root.methods.last_mut().unwrap().handlers[0][1] = handler as u16;
        let mut fixture = Fixture::new(vec![root, target, interface, object_class()]);
        let method = fixture.method("p/T", "run", descriptor);
        fixture.run(InitializationLimits::default(), |session, _| {
            let error = run(session, method, &[]).unwrap_err();
            if scenario == 12 {
                assert_eq!(
                    error.kind,
                    E::WrongValueType {
                        expected: crabspot_interpreter::ValueKind::Int,
                        actual: crabspot_interpreter::ValueKind::Reference
                    }
                );
            } else {
                assert!(
                    matches!(error.kind, E::ConstructionProtocol { .. }),
                    "{scenario}: {error}"
                );
            }
            assert_eq!(error.offset, Some(operation as u32));
            assert_eq!(session.context().heap().allocated_instances(), 1);
        });
    }
}
#[test]
fn fixed_indexed_and_wide_reference_moves_preserve_unfinished_aliases_and_promote_all_copies() {
    let mut target = ordinary("q/Target");
    simple_constructor(&mut target, "java/lang/Object");
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.extend([0x59, 0x4c, 0xc4, 0x3a, 1, 44, 0x2b]);
    code.extend(op(0xb7, ctor));
    code.extend([0xc4, 0x19, 1, 44, 0x2b, 0xa6, 0, 5, 0x04, 0xac, 0x03, 0xac]);
    root.method("run", "()I", 9, &code, 2, 301);
    let mut fixture = Fixture::new(vec![root, target, object_class()]);
    let method = fixture.method("p/T", "run", "()I");
    fixture.run(InitializationLimits::default(), |session, _| {
        assert_eq!(
            run(session, method, &[]).unwrap(),
            EO::Returned(Some(Value::Int(1)))
        );
        assert_eq!(session.context().heap().allocated_instances(), 1);
    });
}
#[test]
fn token_local_writes_invalidate_both_halves_of_category_two_values() {
    for index in [0u8, 1] {
        let target = ordinary("q/Target");
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let mut code = vec![0x09, 0x3f];
        code.extend(op(0xbb, class));
        code.extend([0x3a, index, 0x1e, 0xad]);
        root.method("run", "()J", 9, &code, 2, 2);
        let mut fixture = Fixture::new(vec![root, target, object_class()]);
        let method = fixture.method("p/T", "run", "()J");
        fixture.run(InitializationLimits::default(), |session, _| {
            assert_eq!(
                run(session, method, &[]).unwrap_err().kind,
                if index == 0 {
                    E::WrongValueType {
                        expected: crabspot_interpreter::ValueKind::Long,
                        actual: crabspot_interpreter::ValueKind::Reference,
                    }
                } else {
                    E::LocalNotValue { index: 0 }
                }
            );
        });
    }
}
#[test]
fn repeated_new_site_rejects_live_stack_alias_and_invalidates_old_locals_only_after_new_publication()
 {
    for stack_alias in [true, false] {
        let target = ordinary("q/Target");
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let mut code = op(0xbb, class);
        let (descriptor, stack, locals, args) = if stack_alias {
            code.extend([0xa7, 0xff, 0xfd]);
            ("()I", 2, 0, vec![])
        } else {
            code.extend([
                0x4c, 0x1a, 0x05, 0xa0, 0, 5, 0x2b, 0x4d, 0x84, 0, 0xff, 0x1a, 0x9d, 0xff, 0xf1,
                0x2c, 0x57, 0x03, 0xac,
            ]);
            ("(I)I", 2, 3, vec![Value::Int(2)])
        };
        root.method("run", descriptor, 9, &code, stack, locals);
        let mut fixture = Fixture::new(vec![root, target, object_class()]);
        let method = fixture.method("p/T", "run", descriptor);
        fixture.run(InitializationLimits::default(), |session, _| {
            let error = run(session, method, &args).unwrap_err();
            if stack_alias {
                assert!(matches!(error.kind, E::ConstructionProtocol { .. }));
                assert_eq!(error.offset, Some(0));
                assert_eq!(session.context().heap().allocated_instances(), 1);
                assert_eq!(session.last_construction_steps(), 3);
            } else {
                assert_eq!(error.kind, E::LocalNotValue { index: 2 });
                assert_eq!(session.context().heap().allocated_instances(), 2);
                assert_eq!(session.last_construction_steps(), 8);
            }
        });
    }
}

#[test]
fn all_generic_reference_branches_compare_private_actual_identity_without_promotion() {
    for (opcode, other) in [
        (0xc6, 0),
        (0xc7, 0),
        (0xa5, 0),
        (0xa6, 0),
        (0xa5, 1),
        (0xa6, 1),
        (0xa5, 2),
        (0xa6, 2),
        (0xa5, 3),
        (0xa6, 3),
    ] {
        let target = ordinary("q/Target");
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let mut code = op(0xbb, class);
        if matches!(opcode, 0xa5 | 0xa6) {
            match other {
                0 => code.push(0x59),
                1 => code.extend(op(0xbb, class)),
                2 => code.push(0x01),
                _ => code.push(0x2a),
            }
        }
        code.extend([opcode, 0, 5, 0x03, 0xac, 0x04, 0xac]);
        root.method("run", "(Ljava/lang/Object;)I", 9, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root, target, object_class()]);
        let ordinary = fixture
            .heap
            .allocate_instance(fixture.classes["q/Error"])
            .unwrap();
        let method = fixture.method("p/T", "run", "(Ljava/lang/Object;)I");
        fixture.run(InitializationLimits::default(), |session, _| {
            let truth = match opcode {
                0xc6 => false,
                0xc7 => true,
                0xa5 => other == 0,
                _ => other != 0,
            };
            assert_eq!(
                run(session, method, &[Value::Reference(ordinary)]).unwrap(),
                EO::Returned(Some(Value::Int(i32::from(truth))))
            );
            assert_eq!(
                session.context().heap().allocated_instances(),
                if other == 1 { 3 } else { 2 }
            );
        });
    }
}
#[test]
fn unused_private_allocations_may_be_discarded_or_left_in_an_ordinary_returning_frame() {
    for scenario in 0..5 {
        let target = ordinary("q/Target");
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let mut code = op(0xbb, class);
        match scenario {
            0 => {}
            1 => code.push(0x57),
            2 => code.push(0x4b),
            3 => code.extend([0x04, 0x58]),
            _ => code.extend([0x09, 0x58]),
        };
        code.extend([0x10, 7, 0xac]);
        root.method("run", "()I", 9, &code, 3, 1);
        let mut fixture = Fixture::new(vec![root, target, object_class()]);
        let method = fixture.method("p/T", "run", "()I");
        fixture.run(InitializationLimits::default(), |session, _| {
            assert_eq!(
                run(session, method, &[]).unwrap(),
                EO::Returned(Some(Value::Int(7)))
            );
            assert_eq!(session.context().heap().allocated_instances(), 1);
            assert_eq!(session.retained_roots().count(), 0);
        });
    }
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
fn caught_helper_or_separate_allocation_failure_before_chain_preserves_pending_this() {
    for separate in [false, true] {
        let mut throwing = ordinary("q/Throwing");
        let cause = throwing.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
        let mut code = op(0xb2, cause);
        code.push(0xbf);
        throwing.method(
            if separate { "<init>" } else { "helper" },
            "()V",
            if separate { 1 } else { 9 },
            &code,
            1,
            if separate { 1 } else { 0 },
        );
        let mut target = ordinary("q/Target");
        let object = target.reference(10, "java/lang/Object", "<init>", "()V");
        let mut code = Vec::new();
        let start;
        if separate {
            let class = target.class("q/Throwing");
            let call = target.reference(10, "q/Throwing", "<init>", "()V");
            code.extend(op(0xbb, class));
            code.extend([0x59, 0x4c]);
            start = code.len();
            code.extend(op(0xb7, call));
        } else {
            let call = target.reference(10, "q/Throwing", "helper", "()V");
            start = 0;
            code.extend(op(0xb8, call));
        }
        let end = code.len();
        code.extend([0xa7, 0, 4]);
        let handler = code.len();
        code.push(0x57);
        code.push(0x2a);
        code.extend(op(0xb7, object));
        code.push(0xb1);
        target.method("<init>", "()V", 1, &code, 2, 2);
        target.methods.last_mut().unwrap().handlers =
            vec![[start as u16, end as u16, handler as u16, 0]];
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let call = root.reference(10, "q/Target", "<init>", "()V");
        let mut code = op(0xbb, class);
        code.push(0x59);
        code.extend(op(0xb7, call));
        code.push(0xb0);
        root.method("make", "()Lq/Target;", 9, &code, 2, 0);
        let (mut fixture, _) =
            supplied_fixture(vec![root, target, throwing, object_class()], "q/Exception");
        let method = fixture.method("p/T", "make", "()Lq/Target;");
        let ids = fixture.classes.clone();
        fixture.run(InitializationLimits::default(), |session, _| {
            assert!(matches!(
                run(session, method, &[]).unwrap(),
                EO::Returned(Some(Value::Reference(_)))
            ));
            assert_eq!(
                session.context().heap().allocated_instances(),
                if separate { 3 } else { 2 }
            );
            assert_eq!(session.retained_roots().count(), 0);
            assert_eq!(session.status(ids["q/Throwing"]).unwrap(), IS::Completed);
        });
    }
}
#[test]
fn failed_this_or_super_chain_invalidates_aliases_before_handlers_and_never_permits_return() {
    for reload in [false, true] {
        let mut base = ordinary("q/Base");
        let cause = base.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
        let mut code = op(0xb2, cause);
        code.push(0xbf);
        base.method("<init>", "()V", 1, &code, 1, 1);
        let mut target = Class::new("q/Target", Some("q/Base"), 0x21);
        let call = target.reference(10, "q/Base", "<init>", "()V");
        let mut code = vec![0x2a];
        code.extend(op(0xb7, call));
        code.extend([0xb1, 0x57]);
        if reload {
            code.extend([0x2a, 0x57]);
        }
        code.push(0xb1);
        target.method("<init>", "()V", 1, &code, 1, 1);
        target.methods.last_mut().unwrap().handlers = vec![[1, 4, 5, 0]];
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let ctor = root.reference(10, "q/Target", "<init>", "()V");
        let mut code = op(0xbb, class);
        code.push(0x59);
        code.extend(op(0xb7, ctor));
        code.push(0xb0);
        root.method("make", "()Lq/Target;", 9, &code, 2, 0);
        let (mut fixture, _) =
            supplied_fixture(vec![root, target, base, object_class()], "q/Exception");
        let method = fixture.method("p/T", "make", "()Lq/Target;");
        let target = fixture.classes["q/Target"];
        fixture.run(InitializationLimits::default(), |session, _| {
            let error = run(session, method, &[]).unwrap_err();
            if reload {
                assert_eq!(error.kind, E::LocalNotValue { index: 0 });
            } else {
                assert!(matches!(error.kind, E::ConstructionProtocol { .. }));
            }
            assert_eq!(error.method.as_ref().unwrap().owner, Some(target));
            assert_eq!(session.status(target).unwrap(), IS::Completed);
            assert_eq!(session.retained_roots().count(), 0);
        });
    }
}
#[test]
fn ordinary_this_published_after_inner_completion_remains_usable_after_outer_constructor_throws() {
    let mut target = ordinary("q/Target");
    target.field("value", "I", 0x11);
    let field = target.reference(9, "q/Target", "value", "I");
    let object = target.reference(10, "java/lang/Object", "<init>", "()V");
    let leaked = target.reference(9, "q/Holder", "leaked", "Ljava/lang/Object;");
    let cause = target.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, object));
    code.extend([0x2a, 0x10, 7]);
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb3, leaked));
    code.extend(op(0xb2, cause));
    code.push(0xbf);
    target.method("<init>", "()V", 1, &code, 2, 1);
    let mut holder = ordinary("q/Holder");
    holder.field("leaked", "Ljava/lang/Object;", 9);
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let leaked = root.reference(9, "q/Holder", "leaked", "Ljava/lang/Object;");
    let mut code = op(0xbb, class);
    code.extend([0x59, 0x4b]);
    code.extend(op(0xb7, ctor));
    code.extend([0x2a, 0xb0, 0x57]);
    code.extend(op(0xb2, leaked));
    code.push(0xb0);
    root.method("make", "()Ljava/lang/Object;", 9, &code, 2, 1);
    root.methods.last_mut().unwrap().handlers = vec![[5, 8, 10, 0]];
    let (mut fixture, _) =
        supplied_fixture(vec![root, target, holder, object_class()], "q/Exception");
    let method = fixture.method("p/T", "make", "()Ljava/lang/Object;");
    let target = fixture.classes["q/Target"];
    let field = fixture
        .registry
        .find_declared_field(target, &units("value"), &units("I"))
        .unwrap()
        .unwrap();
    fixture.run(InitializationLimits::default(), |session, statics| {
        let EO::Returned(Some(Value::Reference(reference))) = run(session, method, &[]).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            session.context().heap().instance_class(reference).unwrap(),
            target
        );
        assert_eq!(
            session
                .context()
                .heap()
                .get_field(reference, field)
                .unwrap(),
            Value::Int(7)
        );
        assert_eq!(session.context().heap().allocated_instances(), 2);
        assert_eq!(
            inspect(
                session,
                &statics,
                session
                    .context()
                    .registry()
                    .lookup_class(
                        session
                            .context()
                            .registry()
                            .defining_loader(target)
                            .unwrap(),
                        &units("q/Holder")
                    )
                    .unwrap()
                    .unwrap(),
                "leaked",
                "Ljava/lang/Object;"
            ),
            Value::Reference(reference)
        );
        assert_eq!(session.retained_roots().count(), 0);
    });
}
#[test]
fn a_thousand_constructor_super_frames_complete_and_drop_iteratively_with_exact_shared_limits() {
    let count = 1000usize;
    let mut classes = vec![object_class()];
    for index in 0..count {
        let name = format!("q/C{index}");
        let parent = if index == 0 {
            "java/lang/Object".to_string()
        } else {
            format!("q/C{}", index - 1)
        };
        let mut class = Class::new(&name, Some(&parent), 0x21);
        simple_constructor(&mut class, &parent);
        classes.push(class);
    }
    let name = format!("q/C{}", count - 1);
    let mut root = ordinary("p/T");
    let class = root.class(&name);
    let ctor = root.reference(10, &name, "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Ljava/lang/Object;", 9, &code, 2, 0);
    classes.push(root);
    let mut fixture = Fixture::selected(
        classes,
        &[
            "p/T",
            &name,
            "java/lang/Error",
            "java/lang/Cloneable",
            "java/io/Serializable",
        ],
    );
    let method = fixture.method("p/T", "make", "()Ljava/lang/Object;");
    let target = fixture.classes[&name];
    fixture.run(InitializationLimits::default(), |session, _| {
        let outcome = session
            .run_static_with_construction_limits(
                method,
                &[],
                ExecutionOptions {
                    instruction_budget: (count as u64) * 3 + 5,
                },
                CallLimits {
                    max_frames: count + 2,
                    max_frame_slots: count * 2 + 3,
                    max_prepared_code_bytes: count * 5 + 9,
                },
                ExceptionLimits::default(),
                ConstructionLimits {
                    max_alias_steps: (count as u64) * 2 + 3,
                },
            )
            .unwrap();
        let EO::Returned(Some(Value::Reference(reference))) = outcome else {
            panic!()
        };
        assert_eq!(
            session.context().heap().instance_class(reference).unwrap(),
            target
        );
        assert_eq!(session.last_construction_steps(), (count as u64) * 2 + 3);
        assert_eq!(session.context().heap().allocated_instances(), 1);
    });
}

#[test]
fn object_constructor_executes_actual_code_and_published_this_survives_its_escaping_failure() {
    let mut object = Class::new("java/lang/Object", None, 0x21);
    let leaked = object.reference(9, "q/Holder", "leaked", "Ljava/lang/Object;");
    let cause = object.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
    let mut code = vec![0x2a];
    code.extend(op(0xb3, leaked));
    code.extend(op(0xb2, cause));
    code.push(0xbf);
    object.method("<init>", "()V", 1, &code, 1, 1);
    let mut target = ordinary("q/Target");
    simple_constructor(&mut target, "java/lang/Object");
    let mut holder = ordinary("q/Holder");
    holder.field("leaked", "Ljava/lang/Object;", 9);
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let leaked = root.reference(9, "q/Holder", "leaked", "Ljava/lang/Object;");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.extend([0xb0, 0x57]);
    code.extend(op(0xb2, leaked));
    code.push(0xb0);
    root.method("make", "()Ljava/lang/Object;", 9, &code, 2, 0);
    root.methods.last_mut().unwrap().handlers = vec![[4, 7, 8, 0]];
    let (mut fixture, _) = supplied_fixture(vec![root, target, object, holder], "q/Exception");
    let method = fixture.method("p/T", "make", "()Ljava/lang/Object;");
    let target = fixture.classes["q/Target"];
    fixture.run(InitializationLimits::default(), |session, _| {
        let EO::Returned(Some(Value::Reference(reference))) = run(session, method, &[]).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            session.context().heap().instance_class(reference).unwrap(),
            target
        );
        assert_eq!(session.status(target).unwrap(), IS::Completed);
        assert_eq!(session.context().heap().allocated_instances(), 2);
        assert_eq!(session.retained_roots().count(), 0);
    });
}
#[test]
fn invalid_object_base_is_rejected_lazily_without_rejecting_nonconstruction_evaluations() {
    let mut object = object_class();
    object.flags |= 0x400;
    let mut target = ordinary("q/Target");
    simple_constructor(&mut target, "java/lang/Object");
    let mut root = ordinary("p/T");
    root.method("number", "()I", 9, &[0x04, 0xac], 1, 0);
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Lq/Target;", 9, &code, 2, 0);
    let mut fixture = Fixture::new(vec![root, target, object]);
    let number = fixture.method("p/T", "number", "()I");
    let make = fixture.method("p/T", "make", "()Lq/Target;");
    fixture.run(InitializationLimits::default(),|session,_|{assert_eq!(run(session,number,&[]).unwrap(),EO::Returned(Some(Value::Int(1))));assert_eq!(session.last_construction_steps(),0);let error=run(session,make,&[]).unwrap_err();assert!(matches!(error.kind,E::ConstructionAccess(ref e) if matches!(**e,crabspot_runtime::ConstructionAccessError::InvalidMetadata(_))));assert_eq!(session.context().heap().allocated_instances(),1);});
}
#[test]
fn new_preflights_instantiation_and_stack_room_before_lifecycle_or_alias_publication() {
    for scenario in 0..4 {
        let target = Class::new(
            "q/Target",
            Some("java/lang/Object"),
            match scenario {
                0 => 0x601,
                1 => 0x421,
                2 => 0x420,
                _ => 0x21,
            },
        );
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let mut code = op(0xbb, class);
        code.extend([0x03, 0xac]);
        root.method(
            "make",
            "()I",
            9,
            &code,
            if scenario == 3 { 0 } else { 2 },
            0,
        );
        let mut fixture = Fixture::new(vec![root, target, object_class()]);
        let method = fixture.method("p/T", "make", "()I");
        let target = fixture.classes["q/Target"];
        fixture.run(InitializationLimits::default(), |session, _| {
            let error = run(session, method, &[]).unwrap_err();
            match scenario {
                0 | 1 => assert_eq!(
                    error.kind,
                    E::UnsupportedVmException(
                        crabspot_runtime::VmExceptionKind::InstantiationError
                    )
                ),
                2 => assert_eq!(
                    error.kind,
                    E::UnsupportedVmException(
                        crabspot_runtime::VmExceptionKind::IllegalAccessError
                    )
                ),
                _ => assert_eq!(
                    error.kind,
                    E::StackOverflow {
                        required: 1,
                        capacity: 0
                    }
                ),
            };
            assert_eq!(error.offset, Some(0));
            assert_eq!(session.last_construction_steps(), 0);
            assert_eq!(session.status(target).unwrap(), IS::Uninitialized);
            assert_eq!(session.context().heap().allocated_instances(), 0);
        });
    }
}
#[test]
fn ordinary_references_cannot_be_constructed_again_and_constructors_cannot_be_host_roots() {
    let mut target = ordinary("q/Target");
    target.field("count", "I", 9);
    let count = target.reference(9, "q/Target", "count", "I");
    let object = target.reference(10, "java/lang/Object", "<init>", "()V");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, object));
    code.extend(op(0xb2, count));
    code.push(0x04);
    code.push(0x60);
    code.extend(op(0xb3, count));
    code.push(0xb1);
    target.method("<init>", "()V", 1, &code, 2, 1);
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Lq/Target;", 9, &code, 2, 0);
    let mut fixture = Fixture::new(vec![root, target, object_class()]);
    let method = fixture.method("p/T", "make", "()Lq/Target;");
    let ctor = fixture.method("q/Target", "<init>", "()V");
    let target = fixture.classes["q/Target"];
    fixture.run(InitializationLimits::default(), |session, statics| {
        let error = run(session, method, &[]).unwrap_err();
        assert!(matches!(error.kind, E::ConstructionProtocol { .. }));
        assert_eq!(
            inspect(session, &statics, target, "count", "I"),
            Value::Int(1)
        );
        assert_eq!(session.context().heap().allocated_instances(), 1);
        assert!(matches!(
            run(session, ctor, &[]).unwrap_err().kind,
            E::UnsupportedMethod(_)
        ));
        assert_eq!(session.last_construction_steps(), 0);
    });
}
#[test]
fn recursive_this_delegation_reuses_one_method_cache_but_keeps_activation_keys_distinct() {
    let count = 1000usize;
    let mut target = ordinary("q/Target");
    let object = target.reference(10, "java/lang/Object", "<init>", "()V");
    let this = target.reference(10, "q/Target", "<init>", "(I)V");
    let mut code = vec![0x1b, 0x9a, 0, 8, 0x2a];
    code.extend(op(0xb7, object));
    code.push(0xb1);
    code.extend([0x2a, 0x1b, 0x04, 0x64]);
    code.extend(op(0xb7, this));
    code.push(0xb1);
    let ctorbytes = code.len();
    target.method("<init>", "(I)V", 1, &code, 3, 2);
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "(I)V");
    let mut code = op(0xbb, class);
    code.extend([0x59, 0x1a]);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    let bytes = code.len() + ctorbytes + 1;
    root.method("make", "(I)Lq/Target;", 9, &code, 3, 1);
    let mut fixture = Fixture::new(vec![root, target, object_class()]);
    let method = fixture.method("p/T", "make", "(I)Lq/Target;");
    fixture.run(InitializationLimits::default(), |session, _| {
        let result = session
            .run_static_with_construction_limits(
                method,
                &[Value::Int(count as i32)],
                ExecutionOptions::default(),
                CallLimits {
                    max_frames: count + 3,
                    max_frame_slots: (count + 1) * 5 + 5,
                    max_prepared_code_bytes: bytes,
                },
                ExceptionLimits::default(),
                ConstructionLimits {
                    max_alias_steps: (count as u64) * 3 + 8,
                },
            )
            .unwrap();
        assert!(matches!(result, EO::Returned(Some(Value::Reference(_)))));
        assert_eq!(session.last_construction_steps(), (count as u64) * 3 + 8);
        assert_eq!(session.context().heap().allocated_instances(), 1);
    });
}

fn basic_factory() -> (Fixture, MethodId, ClassId) {
    let mut target = ordinary("q/Target");
    simple_constructor(&mut target, "java/lang/Object");
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Lq/Target;", 9, &code, 2, 0);
    let fixture = Fixture::new(vec![root, target, object_class()]);
    let method = fixture.method("p/T", "make", "()Lq/Target;");
    let target = fixture.classes["q/Target"];
    (fixture, method, target)
}
#[test]
fn alias_work_limits_are_atomic_reset_each_evaluation_and_attribute_completion_to_constructor_pc() {
    for limit in [0, 1, 2, 3, 4, 5, u64::MAX] {
        let (mut fixture, method, target) = basic_factory();
        let object = fixture.classes["java/lang/Object"];
        fixture.run(InitializationLimits::default(), |session, _| {
            let result = session.run_static_with_construction_limits(
                method,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits::default(),
                ConstructionLimits {
                    max_alias_steps: limit,
                },
            );
            if limit >= 5 {
                assert!(matches!(
                    result.unwrap(),
                    EO::Returned(Some(Value::Reference(_)))
                ));
                assert_eq!(session.last_construction_steps(), 5);
            } else {
                let error = result.unwrap_err();
                let (required, used, owner, pc) = if limit == 0 {
                    (1, 0, method.owner(), 0)
                } else if limit < 3 {
                    (3, 1, object, 0)
                } else {
                    (5, 3, target, 4)
                };
                assert_eq!(error.kind, E::ConstructionStepLimit { limit, required });
                assert_eq!(session.last_construction_steps(), used);
                assert_eq!(error.method.as_ref().unwrap().owner, Some(owner));
                assert_eq!(error.offset, Some(pc));
                assert_eq!(
                    session.context().heap().allocated_instances(),
                    usize::from(limit > 0)
                );
                assert_eq!(
                    session.status(target).unwrap(),
                    if limit == 0 {
                        IS::Uninitialized
                    } else {
                        IS::Completed
                    }
                );
            }
            // A later eager argument failure is a new evaluation and clears scan work.
            let error = session
                .run_static_with_construction_limits(
                    method,
                    &[Value::Int(0)],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                    ConstructionLimits {
                        max_alias_steps: u64::MAX,
                    },
                )
                .unwrap_err();
            assert!(matches!(error.kind, E::ArgumentCount { .. }));
            assert_eq!(session.last_construction_steps(), 0);
        });
    }
}
#[test]
fn constructor_frames_slots_code_and_fuel_share_ordinary_limits_without_replaying_new() {
    for scenario in 0..5 {
        let (mut fixture, method, target) = basic_factory();
        fixture.run(InitializationLimits::default(), |session, _| {
            let limits = CallLimits {
                max_frames: if scenario == 0 { 2 } else { 3 },
                max_frame_slots: if scenario == 1 { 4 } else { 5 },
                max_prepared_code_bytes: if scenario == 2 { 13 } else { 14 },
            };
            let result = session.run_static_with_construction_limits(
                method,
                &[],
                ExecutionOptions {
                    instruction_budget: if scenario == 3 { 7 } else { 8 },
                },
                limits,
                ExceptionLimits::default(),
                ConstructionLimits { max_alias_steps: 5 },
            );
            if scenario == 4 {
                assert!(matches!(
                    result.unwrap(),
                    EO::Returned(Some(Value::Reference(_)))
                ));
                assert_eq!(session.last_construction_steps(), 5);
            } else {
                let error = result.unwrap_err();
                match scenario {
                    0 => assert_eq!(
                        error.kind,
                        E::FrameLimit {
                            limit: 2,
                            required: 3
                        }
                    ),
                    1 => assert_eq!(
                        error.kind,
                        E::FrameSlotLimit {
                            limit: 4,
                            required: 5
                        }
                    ),
                    2 => assert_eq!(
                        error.kind,
                        E::PreparedCodeLimit {
                            limit: 13,
                            required: 14
                        }
                    ),
                    _ => assert_eq!(error.kind, E::InstructionBudgetExhausted { limit: 7 }),
                };
            }
            assert_eq!(session.status(target).unwrap(), IS::Completed);
            assert_eq!(session.context().heap().allocated_instances(), 1);
        });
    }
}
#[test]
fn recursive_new_during_own_clinit_allocates_once_per_instruction_and_resumes_original_new_once() {
    let mut target = ordinary("q/Target");
    target.constant_int("flag", "I", 9, 23);
    let field = target.reference(9, "q/Target", "flag", "I");
    let class = target.class("q/Target");
    let mut code = op(0xbb, class);
    code.extend([0x57, 0x10, 7]);
    code.extend(op(0xb3, field));
    code.push(0xb1);
    clinit(&mut target, &code, 1);
    simple_constructor(&mut target, "java/lang/Object");
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Lq/Target;", 9, &code, 2, 0);
    let mut fixture = Fixture::new(vec![root, target, object_class()]);
    let method = fixture.method("p/T", "make", "()Lq/Target;");
    let target = fixture.classes["q/Target"];
    fixture.run(InitializationLimits::default(), |session, statics| {
        let outcome = session
            .run_static_with_construction_limits(
                method,
                &[],
                ExecutionOptions {
                    instruction_budget: 13,
                },
                CallLimits {
                    max_frames: 3,
                    max_frame_slots: 5,
                    max_prepared_code_bytes: 24,
                },
                ExceptionLimits::default(),
                ConstructionLimits { max_alias_steps: 6 },
            )
            .unwrap();
        assert!(matches!(outcome, EO::Returned(Some(Value::Reference(_)))));
        assert_eq!(session.last_construction_steps(), 6);
        assert_eq!(session.context().heap().allocated_instances(), 2);
        assert_eq!(
            inspect(session, &statics, target, "flag", "I"),
            Value::Int(7)
        );
        assert_eq!(session.status(target).unwrap(), IS::Completed);
    });
}
#[test]
fn new_initialization_error_or_abort_allocates_nothing_and_later_requests_keep_sticky_state() {
    for error_kind in [false, true] {
        let mut target = ordinary("q/Target");
        if error_kind {
            throwing_initializer(&mut target);
        } else {
            clinit(&mut target, &[0x01, 0xc2, 0xb1], 1);
        }
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let mut code = op(0xbb, class);
        code.extend([0x57, 0x04, 0xac, 0x57, 0x10, 7, 0xac]);
        root.method("make", "()I", 9, &code, 1, 0);
        root.methods.last_mut().unwrap().handlers = vec![[0, 3, 6, 0]];
        let (mut fixture, thrown) = supplied_fixture(vec![root, target, object_class()], "q/Error");
        let method = fixture.method("p/T", "make", "()I");
        let target = fixture.classes["q/Target"];
        fixture.run(InitializationLimits::default(), |session, _| {
            let first = run(session, method, &[]);
            if error_kind {
                assert_eq!(first.unwrap(), EO::Returned(Some(Value::Int(7))));
                assert_eq!(
                    session.status(target).unwrap(),
                    IS::Erroneous { cause: thrown }
                );
            } else {
                assert_eq!(first.unwrap_err().kind, E::UnsupportedInstruction(0xc2));
                assert_eq!(session.status(target).unwrap(), IS::Aborted);
            }
            assert_eq!(session.last_construction_steps(), 1);
            assert_eq!(session.context().heap().allocated_instances(), 1);
            let error = run(session, method, &[]).unwrap_err();
            assert_eq!(error.offset, Some(0));
            assert_eq!(
                error.kind,
                if error_kind {
                    E::UnsupportedNoClassDefFoundError { class: target }
                } else {
                    E::InitializationAborted { class: target }
                }
            );
            assert_eq!(session.last_construction_steps(), 1);
            assert_eq!(session.context().heap().allocated_instances(), 1);
        });
    }
}
#[test]
fn allocation_does_not_resolve_a_constructor_and_missing_constructor_is_a_later_host_vm_boundary() {
    let target = ordinary("q/Target");
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Lq/Target;", 9, &code, 2, 0);
    let mut fixture = Fixture::new(vec![root, target, object_class()]);
    let method = fixture.method("p/T", "make", "()Lq/Target;");
    let target = fixture.classes["q/Target"];
    fixture.run(InitializationLimits::default(), |session, _| {
        assert_eq!(
            run(session, method, &[]).unwrap_err().kind,
            E::UnsupportedVmException(crabspot_runtime::VmExceptionKind::NoSuchMethodError)
        );
        assert_eq!(session.context().heap().allocated_instances(), 1);
        assert_eq!(session.status(target).unwrap(), IS::Completed);
    });
}
#[test]
fn explicit_initialize_without_construction_does_not_charge_alias_steps_even_with_zero_limit() {
    let mut root = ordinary("p/T");
    root.field("flag", "I", 9);
    set_initializer(&mut root, "flag", 7);
    let mut fixture = Fixture::new(vec![root, object_class()]);
    let root = fixture.classes["p/T"];
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(
            session
                .initialize_with_construction_limits(
                    root,
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                    ConstructionLimits { max_alias_steps: 0 }
                )
                .unwrap(),
            IO::Completed
        );
        assert_eq!(session.last_construction_steps(), 0);
        assert_eq!(initialize(session, root).unwrap(), IO::Completed);
        assert_eq!(session.last_construction_steps(), 0);
        assert_eq!(inspect(session, &statics, root, "flag", "I"), Value::Int(7));
    });
}

#[test]
fn every_old_runtime_body_profile_keeps_new_and_constructor_calls_disabled() {
    use crabspot_interpreter::{
        execute_instance_method_body, execute_instance_method_body_with_exceptions,
        execute_instance_method_body_with_static_state,
        execute_instance_method_body_with_static_state_and_exceptions,
    };
    for profile in 0..4 {
        for constructor_call in [false, true] {
            let mut target = ordinary("q/Target");
            simple_constructor(&mut target, "java/lang/Object");
            let mut root = ordinary("p/T");
            let class = root.class("q/Target");
            let ctor = root.reference(10, "q/Target", "<init>", "()V");
            let mut code = if constructor_call {
                let mut code = vec![0x01];
                code.extend(op(0xb7, ctor));
                code
            } else {
                let mut code = op(0xbb, class);
                code.push(0x57);
                code
            };
            code.push(0xb1);
            root.method("run", "()V", 1, &code, 1, 1);
            let mut fixture = Fixture::new(vec![root, target, object_class()]);
            let receiver = fixture
                .heap
                .allocate_instance(fixture.classes["p/T"])
                .unwrap();
            let method = fixture.method("p/T", "run", "()V");
            let (statics, exceptions) = fixture.profiles();
            let mut resolver = SameLoaderResolver;
            let mut context = FieldResolutionContext::new_unnamed_modules(
                &fixture.registry,
                &mut fixture.heap,
                &mut resolver,
                FieldResolutionOptions::default(),
            )
            .unwrap();
            let error = match profile {
                0 => execute_instance_method_body(
                    &mut context,
                    method,
                    receiver,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                )
                .unwrap_err(),
                1 => execute_instance_method_body_with_exceptions(
                    &mut context,
                    &exceptions,
                    method,
                    receiver,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap_err(),
                2 => execute_instance_method_body_with_static_state(
                    &mut context,
                    &statics,
                    method,
                    receiver,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                )
                .unwrap_err(),
                _ => execute_instance_method_body_with_static_state_and_exceptions(
                    &mut context,
                    &statics,
                    &exceptions,
                    method,
                    receiver,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap_err(),
            };
            if constructor_call {
                assert_eq!(
                    error.kind,
                    E::MethodAccess(Box::new(
                        crabspot_runtime::MethodAccessError::UnsupportedSpecialMethod
                    ))
                );
                assert_eq!(error.offset, Some(1));
            } else {
                assert_eq!(error.kind, E::UnsupportedInstruction(0xbb));
                assert_eq!(error.offset, Some(0));
            }
            assert_eq!(context.heap().allocated_instances(), 1);
        }
    }
}
#[test]
fn constructor_handler_rows_use_the_same_prepared_handler_quota() {
    for allowed in [false, true] {
        let mut target = ordinary("q/Target");
        simple_constructor(&mut target, "java/lang/Object");
        target.methods[0].handlers = vec![[1, 4, 4, 0]];
        let mut root = ordinary("p/T");
        let class = root.class("q/Target");
        let ctor = root.reference(10, "q/Target", "<init>", "()V");
        let mut code = op(0xbb, class);
        code.push(0x59);
        code.extend(op(0xb7, ctor));
        code.push(0xb0);
        root.method("make", "()Lq/Target;", 9, &code, 2, 0);
        let mut fixture = Fixture::new(vec![root, target, object_class()]);
        let method = fixture.method("p/T", "make", "()Lq/Target;");
        fixture.run(InitializationLimits::default(), |session, _| {
            let result = session.run_static(
                method,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits {
                    max_prepared_handlers: usize::from(allowed),
                    ..ExceptionLimits::default()
                },
            );
            if allowed {
                assert!(matches!(
                    result.unwrap(),
                    EO::Returned(Some(Value::Reference(_)))
                ));
            } else {
                assert_eq!(
                    result.unwrap_err().kind,
                    E::ExceptionHandlerLimit {
                        limit: 0,
                        required: 1
                    }
                );
            }
            assert_eq!(session.context().heap().allocated_instances(), 1);
        });
    }
}

#[test]
fn abstract_superclass_constructor_executes_for_a_concrete_descendant() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x421);
    simple_constructor(&mut base, "java/lang/Object");
    let mut target = Class::new("q/Target", Some("q/Base"), 0x21);
    simple_constructor(&mut target, "q/Base");
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.push(0xb0);
    root.method("make", "()Lq/Target;", 9, &code, 2, 0);
    let mut fixture = Fixture::new(vec![root, target, base, object_class()]);
    let method = fixture.method("p/T", "make", "()Lq/Target;");
    fixture.run(InitializationLimits::default(), |session, _| {
        assert!(matches!(
            run(session, method, &[]).unwrap(),
            EO::Returned(Some(Value::Reference(_)))
        ))
    });
}
#[test]
fn resolver_panic_drops_constructor_frames_aborts_active_initializers_and_preserves_escaped_ordinary_this()
 {
    struct Panics;
    impl crabspot_runtime::HierarchyResolver for Panics {
        fn resolve(
            &mut self,
            registry: &DefinitionRegistry,
            loader: crabspot_runtime::LoaderId,
            name: &[u16],
        ) -> Result<Option<ClassId>, crabspot_runtime::ResolveError> {
            if name == units("q/Missing") {
                panic!("intentional constructor resolver panic")
            };
            registry
                .lookup_class(loader, name)
                .map_err(|e| crabspot_runtime::ResolveError::new(e.to_string()))
        }
    }
    let mut holder = ordinary("q/Holder");
    holder.field("leaked", "Ljava/lang/Object;", 9);
    let mut target = ordinary("q/Target");
    let object = target.reference(10, "java/lang/Object", "<init>", "()V");
    let leaked = target.reference(9, "q/Holder", "leaked", "Ljava/lang/Object;");
    let missing = target.reference(9, "q/Missing", "value", "I");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, object));
    code.push(0x2a);
    code.extend(op(0xb3, leaked));
    code.extend(op(0xb2, missing));
    code.extend([0x57, 0xb1]);
    target.method("<init>", "()V", 1, &code, 1, 1);
    let mut root = ordinary("p/T");
    let class = root.class("q/Target");
    let ctor = root.reference(10, "q/Target", "<init>", "()V");
    let mut code = op(0xbb, class);
    code.push(0x59);
    code.extend(op(0xb7, ctor));
    code.extend([0x57, 0xb1]);
    clinit(&mut root, &code, 2);
    let healthy = ordinary("q/Healthy");
    let mut fixture = Fixture::new(vec![root, target, holder, healthy, object_class()]);
    let ids = fixture.classes.clone();
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
        ids["java/lang/Error"],
        InitializationLimits::default(),
    )
    .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        initialize(&mut session, ids["p/T"])
    }));
    assert!(result.is_err());
    assert_eq!(session.status(ids["p/T"]).unwrap(), IS::Aborted);
    assert_eq!(session.status(ids["q/Target"]).unwrap(), IS::Completed);
    let Value::Reference(reference) = inspect(
        &session,
        &statics,
        ids["q/Holder"],
        "leaked",
        "Ljava/lang/Object;",
    ) else {
        panic!()
    };
    assert!(!reference.is_null());
    assert_eq!(
        session.context().heap().instance_class(reference).unwrap(),
        ids["q/Target"]
    );
    assert_eq!(session.context().heap().allocated_instances(), 1);
    assert_eq!(session.retained_roots().count(), 0);
    assert_eq!(
        initialize(&mut session, ids["q/Healthy"]).unwrap(),
        IO::Completed
    );
    assert_eq!(session.last_construction_steps(), 0);
}

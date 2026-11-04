//! Registered static-call resolution uses the shared engine in every runtime profile.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome as EO, ExecutionError, ExecutionErrorKind as E,
    ExecutionOptions, ExecutionOutcome as O, InitializationLimits, InitializationOutcome as IO,
    InitializationSession, InitializationStatus as IS, Value, execute_instance_method_body,
    execute_instance_method_body_with_exceptions, execute_instance_method_body_with_static_state,
    execute_instance_method_body_with_static_state_and_exceptions, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, ExceptionProfile, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap, MethodAccessError, MethodId,
    Reference, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver, StaticStateOptions,
    StaticStateProfile, VmExceptionKind,
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

#[derive(Clone, Copy, Debug)]
enum Profile {
    Body,
    Exceptions,
    Static,
    Composed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum Outcome {
    Legacy(O),
    Throwing(EO),
}
#[derive(Default)]
struct Options {
    execution: ExecutionOptions,
    calls: CallLimits,
    fields: FieldResolutionOptions,
    exceptions: ExceptionLimits,
}
impl Fixture {
    fn object(&mut self, name: &str) -> Reference {
        self.heap.allocate_instance(self.classes[name]).unwrap()
    }
    fn eval(
        &mut self,
        profile: Profile,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
        options: Options,
    ) -> Result<Outcome, ExecutionError> {
        let (statics, exceptions) = self.profiles();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            options.fields,
        )
        .unwrap();
        match profile {
            Profile::Body => execute_instance_method_body(
                &mut context,
                method,
                receiver,
                args,
                options.execution,
                options.calls,
            )
            .map(Outcome::Legacy),
            Profile::Exceptions => execute_instance_method_body_with_exceptions(
                &mut context,
                &exceptions,
                method,
                receiver,
                args,
                options.execution,
                options.calls,
                options.exceptions,
            )
            .map(Outcome::Throwing),
            Profile::Static => execute_instance_method_body_with_static_state(
                &mut context,
                &statics,
                method,
                receiver,
                args,
                options.execution,
                options.calls,
            )
            .map(Outcome::Legacy),
            Profile::Composed => execute_instance_method_body_with_static_state_and_exceptions(
                &mut context,
                &statics,
                &exceptions,
                method,
                receiver,
                args,
                options.execution,
                options.calls,
                options.exceptions,
            )
            .map(Outcome::Throwing),
        }
    }
    fn stored_static(&mut self, class: &str, name: &str, descriptor: &str) -> Value {
        let (profile, _) = self.profiles();
        let field = self
            .registry
            .find_declared_field(self.classes[class], &units(name), &units(descriptor))
            .unwrap()
            .unwrap();
        let mut resolver = SameLoaderResolver;
        let context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        context.inspect_static(&profile, field).unwrap()
    }
}
fn returned(profile: Profile, value: Value) -> Outcome {
    match profile {
        Profile::Body | Profile::Static => Outcome::Legacy(O::Returned(Some(value))),
        _ => Outcome::Throwing(EO::Returned(Some(value))),
    }
}
fn body_root(
    owner: &str,
    name: &str,
    descriptor: &str,
    prefix: &[u8],
    stack: u16,
    locals: u16,
) -> Class {
    let mut root = ordinary("p/T");
    let call = root.reference(10, owner, name, descriptor);
    let mut code = prefix.to_vec();
    code.extend(op(0xb8, call));
    code.push(0xac);
    root.method("run", "()I", 1, &code, stack, locals);
    root
}
#[test]
fn all_runtime_profiles_resolve_cross_class_without_implicitly_running_initializers() {
    for profile in [
        Profile::Body,
        Profile::Exceptions,
        Profile::Static,
        Profile::Composed,
    ] {
        let mut target = ordinary("q/Target");
        target.field("flag", "I", 9);
        set_initializer(&mut target, "flag", 7);
        target.method("leaf", "()I", 9, &[0x10, 42, 0xac], 1, 0);
        let root = body_root("q/Target", "leaf", "()I", &[], 1, 1);
        let mut fixture = Fixture::new(vec![target, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture
                .eval(profile, method, receiver, &[], Options::default())
                .unwrap(),
            returned(profile, Value::Int(42))
        );
        assert_eq!(
            fixture.stored_static("q/Target", "flag", "I"),
            Value::Int(0)
        );
    }
}
#[test]
fn resolved_calls_do_not_enable_static_fields_in_profiles_without_that_capability() {
    for profile in [
        Profile::Body,
        Profile::Exceptions,
        Profile::Static,
        Profile::Composed,
    ] {
        let mut target = ordinary("q/Target");
        target.field("flag", "I", 9);
        set_initializer(&mut target, "flag", 7);
        let field = target.reference(9, "q/Target", "flag", "I");
        let mut code = op(0xb2, field);
        code.push(0xac);
        target.method("leaf", "()I", 9, &code, 1, 0);
        let root = body_root("q/Target", "leaf", "()I", &[], 1, 1);
        let mut fixture = Fixture::new(vec![target, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.eval(profile, method, receiver, &[], Options::default());
        if matches!(profile, Profile::Body | Profile::Exceptions) {
            let error = result.unwrap_err();
            assert_eq!(error.kind, E::UnsupportedInstruction(0xb2));
            assert_eq!(
                error.method.as_ref().unwrap().owner,
                Some(fixture.classes["q/Target"])
            );
            assert_eq!(
                error.call_site.as_ref().unwrap().method.owner,
                Some(fixture.classes["p/T"])
            );
        } else {
            assert_eq!(result.unwrap(), returned(profile, Value::Int(0)));
        }
    }
}
#[test]
fn standalone_and_heap_only_keep_same_class_resolution_and_diagnostics() {
    let mut root = body_root("q/Target", "leaf", "()I", &[], 1, 1);
    root.methods[0].flags = 9;
    root.methods[0].locals = 0;
    let class = crabspot_classfile::parse(&root.bytes()).unwrap();
    let expected = E::UnsupportedCall(
        "cross-class owner requires runtime class identity, resolution and initialization",
    );
    assert_eq!(
        execute_static_method_body(&class, 0, &[], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        expected
    );
    let mut heap = ManagedHeap::new(HeapOptions::default()).unwrap();
    assert_eq!(
        execute_static_method_body_with_heap(
            &class,
            0,
            &[],
            ExecutionOptions::default(),
            CallLimits::default(),
            &mut heap
        )
        .unwrap_err()
        .kind,
        expected
    );
}
#[test]
fn target_preparation_and_reservations_precede_argument_underflow_or_wrong_kind() {
    for scenario in 0..8 {
        let mut target = ordinary("q/Target");
        let flags = match scenario {
            0 => 0x109,
            1 => 0x29,
            _ => 9,
        };
        let code = if scenario == 2 {
            vec![0xff]
        } else {
            vec![0x1a, 0xac]
        };
        target.method("leaf", "(I)I", flags, &code, 1, 1);
        let prefix = if scenario == 7 { vec![0x01] } else { vec![] };
        let root = body_root("q/Target", "leaf", "(I)I", &prefix, 1, 1);
        let mut fixture = Fixture::new(vec![target, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let mut options = Options::default();
        match scenario {
            3 => options.calls.max_prepared_code_bytes = 4,
            4 => options.calls.max_frames = 1,
            5 => options.calls.max_frame_slots = 2,
            _ => {}
        }
        let error = fixture
            .eval(Profile::Body, method, receiver, &[], options)
            .unwrap_err();
        match scenario {
            0 | 1 => assert!(matches!(error.kind, E::UnsupportedMethod(_))),
            2 => assert!(matches!(error.kind, E::Decode(_))),
            3 => assert!(matches!(error.kind, E::PreparedCodeLimit { .. })),
            4 => assert!(matches!(error.kind, E::FrameLimit { .. })),
            5 => assert!(matches!(error.kind, E::FrameSlotLimit { .. })),
            6 => assert_eq!(error.kind, E::StackUnderflow),
            _ => assert_eq!(
                error.kind,
                E::WrongValueType {
                    expected: crabspot_interpreter::ValueKind::Int,
                    actual: crabspot_interpreter::ValueKind::Reference
                }
            ),
        }
        assert_eq!(
            error.method.as_ref().unwrap().owner,
            Some(fixture.classes[if scenario < 6 { "q/Target" } else { "p/T" }])
        );
        assert_eq!(
            error.call_site.as_ref().unwrap().method.owner,
            Some(fixture.classes["p/T"])
        );
    }
}

fn iface(name: &str) -> Class {
    Class::new(name, Some("java/lang/Object"), 0x601)
}
fn implement(class: &mut Class, name: &str) {
    let index = class.class(name);
    class.interfaces.push(index);
}
#[test]
fn resolution_categories_precede_empty_argument_stacks_and_map_only_in_ordinary_profiles() {
    for scenario in 0..5 {
        for profile in [Profile::Body, Profile::Composed] {
            let mut target = ordinary("q/Target");
            let (flags, name) = match scenario {
                0 => (9, "missing"),
                1 => (1, "leaf"),
                2 => (8, "leaf"),
                3 => (10, "leaf"),
                _ => (9, "leaf"),
            };
            target.method(
                "leaf",
                "(I)I",
                flags,
                &[0x04, 0xac],
                1,
                if flags & 8 != 0 { 1 } else { 2 },
            );
            let mut root = ordinary("p/T");
            let reference = root.reference(
                if scenario == 4 { 11 } else { 10 },
                "q/Target",
                name,
                "(I)I",
            );
            let mut code = op(0xb8, reference);
            code.extend([0xac, 0x57, 0x03, 0xac]);
            root.method("run", "()I", 1, &code, 1, 1);
            root.methods.last_mut().unwrap().handlers = if matches!(profile, Profile::Composed) {
                vec![[0, 3, 4, 0]]
            } else {
                vec![]
            };
            let mut fixture = Fixture::new(vec![root, target]);
            let receiver = fixture.object("p/T");
            let method = fixture.method("p/T", "run", "()I");
            let result = fixture.eval(profile, method, receiver, &[], Options::default());
            if scenario == 3 {
                let error = result.unwrap_err();
                assert!(
                    matches!(error.kind,E::MethodAccess(ref e) if matches!(**e,MethodAccessError::UnsupportedNestAccess { .. }))
                );
                assert_eq!(
                    error.method.as_ref().unwrap().owner,
                    Some(fixture.classes["p/T"])
                );
                assert_eq!(error.offset, Some(0));
            } else if matches!(profile, Profile::Body) {
                let expected = match scenario {
                    0 => O::NoSuchMethodError,
                    2 => O::IllegalAccessError,
                    _ => O::IncompatibleClassChangeError,
                };
                assert_eq!(result.unwrap(), Outcome::Legacy(expected));
            } else {
                let expected = match scenario {
                    0 => VmExceptionKind::NoSuchMethodError,
                    2 => VmExceptionKind::IllegalAccessError,
                    _ => VmExceptionKind::IncompatibleClassChangeError,
                };
                let error = result.unwrap_err();
                assert_eq!(error.kind, E::UnsupportedVmException(expected));
                assert_eq!(error.offset, Some(0));
                assert_eq!(error.call_site.unwrap().offset, 0);
            }
        }
    }
}
#[test]
fn tag_version_array_and_special_boundaries_do_not_pop_arguments_or_resolve_bodies() {
    for scenario in 0..4 {
        let mut target = if scenario == 1 {
            iface("q/Target")
        } else {
            ordinary("q/Target")
        };
        target.method("leaf", "(I)I", 9, &[0x04, 0xac], 1, 1);
        let mut root = ordinary("p/T");
        if scenario == 1 {
            root.major = 51;
        }
        let index = root.reference(
            if scenario == 0 {
                9
            } else if scenario == 1 {
                11
            } else {
                10
            },
            if scenario == 2 { "[I" } else { "q/Target" },
            if scenario == 3 { "<init>" } else { "leaf" },
            if scenario == 0 {
                "I"
            } else if scenario == 3 {
                "(I)V"
            } else {
                "(I)I"
            },
        );
        let mut code = op(0xb8, index);
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root, target]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let error = fixture
            .eval(Profile::Body, method, receiver, &[], Options::default())
            .unwrap_err();
        assert_eq!(error.offset, Some(0));
        assert_eq!(
            error.method.as_ref().unwrap().owner,
            Some(fixture.classes["p/T"])
        );
        match scenario {
            0 | 1 => assert!(
                matches!(error.kind,E::MethodAccess(ref e) if matches!(**e,MethodAccessError::InvalidConstant{..}))
            ),
            2 => assert!(
                matches!(error.kind,E::MethodAccess(ref e) if matches!(**e,MethodAccessError::Context(crabspot_runtime::FieldAccessError::UnsupportedArrayOwner)))
            ),
            _ => assert_eq!(
                error.kind,
                E::MethodAccess(Box::new(MethodAccessError::UnsupportedSpecialMethod))
            ),
        }
    }
}
#[test]
fn inherited_aliases_share_code_but_repeat_origin_sensitive_resolution_with_exact_work_limits() {
    fn fixture() -> (Fixture, MethodId, Reference, usize) {
        let mut base = ordinary("q/Base");
        base.method("leaf", "()I", 9, &[0x04, 0xac], 1, 0);
        let child = Class::new("q/Child", Some("q/Base"), 0x21);
        let mut root = ordinary("p/T");
        let a = root.reference(10, "q/Base", "leaf", "()I");
        let b = root.reference(10, "q/Child", "leaf", "()I");
        let mut code = op(0xb8, a);
        code.push(0x57);
        code.extend(op(0xb8, b));
        code.push(0xac);
        let bytes = code.len() + 2;
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root, base, child]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        (fixture, method, receiver, bytes)
    }
    let (mut probe, method, receiver, bytes) = fixture();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &probe.registry,
        &mut probe.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &[],
            ExecutionOptions::default(),
            CallLimits {
                max_prepared_code_bytes: bytes,
                ..CallLimits::default()
            }
        )
        .unwrap(),
        O::Returned(Some(Value::Int(1)))
    );
    let total = context.lookup_steps();
    drop(context);
    for (limit, success) in [(total, true), (total - 1, false)] {
        let (mut fixture, method, receiver, bytes) = fixture();
        let result = fixture.eval(
            Profile::Body,
            method,
            receiver,
            &[],
            Options {
                calls: CallLimits {
                    max_prepared_code_bytes: bytes,
                    ..CallLimits::default()
                },
                fields: FieldResolutionOptions {
                    max_lookup_steps: limit,
                    ..FieldResolutionOptions::default()
                },
                ..Options::default()
            },
        );
        if success {
            assert_eq!(result.unwrap(), returned(Profile::Body, Value::Int(1)));
        } else {
            let error = result.unwrap_err();
            assert!(matches!(error.kind, E::MethodAccess(_)));
            assert_eq!(error.offset, Some(4));
            assert_eq!(error.call_site.unwrap().offset, 4);
        }
    }
    let (mut fixture, method, receiver, bytes) = fixture();
    assert!(
        matches!(fixture.eval(Profile::Body,method,receiver,&[],Options {calls:CallLimits {max_prepared_code_bytes:bytes-1,..CallLimits::default()},..Options::default()}).unwrap_err().kind,E::PreparedCodeLimit{required,..} if required==bytes)
    );
}
#[test]
fn actual_declarer_validates_reference_arguments_and_results_without_receiver_narrowing() {
    for wrong in [false, true] {
        let mut base = ordinary("q/Base");
        base.method("identity", "(Lq/Base;)Lq/Base;", 9, &[0x2a, 0xb0], 1, 1);
        let child = Class::new("q/Child", Some("q/Base"), 0x21);
        let mut root = ordinary("p/T");
        let call = root.reference(10, "q/Child", "identity", "(Lq/Base;)Lq/Base;");
        let mut code = vec![0x2b];
        code.extend(op(0xb8, call));
        code.push(0xb0);
        root.method(
            "run",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            1,
            &code,
            1,
            2,
        );
        let mut fixture = Fixture::new(vec![root, base, child]);
        let receiver = fixture.object("p/T");
        let value = fixture.object(if wrong { "p/T" } else { "q/Base" });
        let method = fixture.method("p/T", "run", "(Ljava/lang/Object;)Ljava/lang/Object;");
        let result = fixture.eval(
            Profile::Body,
            method,
            receiver,
            &[Value::Reference(value)],
            Options::default(),
        );
        if wrong {
            let error = result.unwrap_err();
            assert!(
                matches!(error.kind,E::MethodAccess(ref e) if matches!(**e,MethodAccessError::IncompatibleReference(_)))
            );
            assert_eq!(
                error.method.as_ref().unwrap().owner,
                Some(fixture.classes["q/Base"])
            );
            assert_eq!(error.offset, Some(1));
        } else {
            assert_eq!(
                result.unwrap(),
                returned(Profile::Body, Value::Reference(value))
            );
        }
    }
}
#[test]
fn cross_class_static_body_uses_its_own_cp_for_nested_virtual_special_interface_and_static_calls() {
    let mut base = ordinary("q/Base");
    base.method("base", "()I", 1, &[0x05, 0xac], 1, 1);
    let mut interface = iface("q/I");
    interface.method("default", "()I", 1, &[0x06, 0xac], 1, 1);
    let mut target = Class::new("q/Target", Some("q/Base"), 0x21);
    implement(&mut target, "q/I");
    target.method("virtual", "()I", 1, &[0x04, 0xac], 1, 1);
    target.method("helper", "()I", 10, &[0x07, 0xac], 1, 0);
    let virtual_call = target.reference(10, "q/Target", "virtual", "()I");
    let special = target.reference(10, "q/Base", "base", "()I");
    let default = target.reference(11, "q/I", "default", "()I");
    let helper = target.reference(10, "q/Target", "helper", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, virtual_call));
    code.push(0x2a);
    code.extend(op(0xb7, special));
    code.extend([0x60, 0x2a]);
    code.extend([0xb9, (default >> 8) as u8, default as u8, 1, 0]);
    code.push(0x60);
    code.extend(op(0xb8, helper));
    code.extend([0x60, 0xac]);
    target.method("calculate", "(Lq/Target;)I", 9, &code, 3, 1);
    let mut root = Class::new("p/T", Some("q/Target"), 0x21);
    let call = root.reference(10, "q/Target", "calculate", "(Lq/Target;)I");
    let mut code = vec![0x2a];
    code.extend(op(0xb8, call));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root, target, base, interface]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture
            .eval(Profile::Body, method, receiver, &[], Options::default())
            .unwrap(),
        returned(Profile::Body, Value::Int(10))
    );
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
fn session_initializes_actual_static_declarer_and_not_the_symbolic_subclass() {
    let mut base = ordinary("q/Base");
    base.field("flag", "I", 9);
    set_initializer(&mut base, "flag", 7);
    let field = base.reference(9, "q/Base", "flag", "I");
    let mut code = op(0xb2, field);
    code.push(0xac);
    base.method("leaf", "()I", 9, &code, 1, 0);
    let mut child = Class::new("q/Child", Some("q/Base"), 0x21);
    child.field("flag", "I", 9);
    set_initializer(&mut child, "flag", 9);
    let mut root = body_root("q/Child", "leaf", "()I", &[], 1, 0);
    root.methods[0].flags = 9;
    let mut fixture = Fixture::new(vec![root, base, child]);
    let ids = fixture.classes.clone();
    let method = fixture.method("p/T", "run", "()I");
    fixture.run(InitializationLimits::default(), |session, statics| {
        for _ in 0..2 {
            assert_eq!(
                session
                    .run_static(
                        method,
                        &[],
                        ExecutionOptions::default(),
                        CallLimits::default(),
                        ExceptionLimits::default()
                    )
                    .unwrap(),
                EO::Returned(Some(Value::Int(7)))
            );
        }
        assert_eq!(session.status(ids["q/Base"]).unwrap(), IS::Completed);
        assert_eq!(session.status(ids["q/Child"]).unwrap(), IS::Uninitialized);
        assert_eq!(
            inspect(session, &statics, ids["q/Child"], "flag", "I"),
            Value::Int(0)
        );
    });
}
#[test]
fn cross_class_recursive_initializers_retain_mixed_arguments_once_under_exact_fuel() {
    let mut c = ordinary("p/T");
    for name in ["value", "answer"] {
        c.field(name, "I", 9);
    }
    let value = c.reference(9, "p/T", "value", "I");
    let answer = c.reference(9, "p/T", "answer", "I");
    let compute = c.reference(10, "q/D", "compute", "()I");
    let mut code = vec![0x04];
    code.extend(op(0xb3, value));
    code.extend(op(0xb8, compute));
    code.extend(op(0xb3, answer));
    code.push(0xb1);
    clinit(&mut c, &code, 1);
    let mut code = op(0xb2, value);
    code.push(0xac);
    c.method("peek", "()I", 9, &code, 1, 0);
    let descriptor = "(JLjava/lang/Object;DI)J";
    let capture = c.reference(10, "q/D", "capture", descriptor);
    let mut code = vec![0x1e, 0x2c, 0x29, 0x15, 5];
    code.extend(op(0xb8, capture));
    code.push(0xad);
    c.method("run", descriptor, 9, &code, 6, 6);
    let mut d = ordinary("q/D");
    for (name, desc) in [
        ("seen", "I"),
        ("j", "J"),
        ("r", "Ljava/lang/Object;"),
        ("d", "D"),
        ("i", "I"),
    ] {
        d.field(name, desc, 9);
    }
    let seen = d.reference(9, "q/D", "seen", "I");
    let peek = d.reference(10, "p/T", "peek", "()I");
    let mut code = op(0xb8, peek);
    code.extend(op(0xb3, seen));
    code.push(0xb1);
    clinit(&mut d, &code, 1);
    let mut code = op(0xb2, seen);
    code.push(0xac);
    d.method("compute", "()I", 9, &code, 1, 0);
    let mut code = Vec::new();
    for (load, name, desc) in [
        (vec![0x1e], "j", "J"),
        (vec![0x2c], "r", "Ljava/lang/Object;"),
        (vec![0x29], "d", "D"),
        (vec![0x15, 5], "i", "I"),
    ] {
        code.extend(load);
        let field = d.reference(9, "q/D", name, desc);
        code.extend(op(0xb3, field));
    }
    code.extend([0x1e, 0xad]);
    d.method("capture", descriptor, 9, &code, 2, 6);
    let mut fixture = Fixture::new(vec![c, d]);
    let reference = fixture.object("q/Error");
    let ids = fixture.classes.clone();
    let method = fixture.method("p/T", "run", descriptor);
    fixture.run(InitializationLimits::default(), |session, statics| {
        let args = [
            Value::Long(i64::MIN),
            Value::Reference(reference),
            Value::Double(0xfff8000000000042),
            Value::Int(99),
        ];
        for fuel in [28, 16] {
            assert_eq!(
                session
                    .run_static(
                        method,
                        &args,
                        ExecutionOptions {
                            instruction_budget: fuel
                        },
                        CallLimits::default(),
                        ExceptionLimits::default()
                    )
                    .unwrap(),
                EO::Returned(Some(Value::Long(i64::MIN)))
            );
        }
        assert_eq!(
            inspect(session, &statics, ids["p/T"], "answer", "I"),
            Value::Int(1)
        );
        assert_eq!(
            inspect(session, &statics, ids["q/D"], "seen", "I"),
            Value::Int(1)
        );
        for (name, desc, value) in [
            ("j", "J", args[0]),
            ("r", "Ljava/lang/Object;", args[1]),
            ("d", "D", args[2]),
            ("i", "I", args[3]),
        ] {
            assert_eq!(inspect(session, &statics, ids["q/D"], name, desc), value);
        }
        let error = session
            .run_static(
                method,
                &args,
                ExecutionOptions {
                    instruction_budget: 15,
                },
                CallLimits::default(),
                ExceptionLimits::default(),
            )
            .unwrap_err();
        assert!(matches!(error.kind, E::InstructionBudgetExhausted { .. }));
        assert_eq!(error.initialization_class, None);
        for name in ["p/T", "q/D"] {
            assert_eq!(session.status(ids[name]).unwrap(), IS::Completed);
        }
    });
}
#[test]
fn initializer_error_is_caught_at_the_original_cross_class_b8_pc_but_body_throw_is_ordinary() {
    for body_throw in [false, true] {
        let mut target = ordinary("q/Target");
        target.field("flag", "I", 9);
        if body_throw {
            set_initializer(&mut target, "flag", 7);
            let cause = target.reference(9, "q/Seed", "cause", "Ljava/lang/Throwable;");
            let mut code = op(0xb2, cause);
            code.push(0xbf);
            target.method("leaf", "()V", 9, &code, 1, 0);
        } else {
            throwing_initializer(&mut target);
            target.method("leaf", "()V", 9, &[0xb1], 0, 0);
        }
        let mut root = ordinary("p/T");
        root.field("caught", "I", 9);
        let field = root.reference(9, "p/T", "caught", "I");
        let call = root.reference(10, "q/Target", "leaf", "()V");
        let mut code = op(0xb8, call);
        code.extend([0xb1, 0x57, 0x10, 7]);
        code.extend(op(0xb3, field));
        code.push(0xb1);
        clinit(&mut root, &code, 1);
        root.methods.last_mut().unwrap().handlers = vec![[0, 3, 4, 0]];
        let (mut fixture, thrown) = supplied_fixture(
            vec![root, target],
            if body_throw { "q/Exception" } else { "q/Error" },
        );
        let ids = fixture.classes.clone();
        fixture.run(InitializationLimits::default(), |session, statics| {
            assert_eq!(initialize(session, ids["p/T"]).unwrap(), IO::Completed);
            assert_eq!(
                inspect(session, &statics, ids["p/T"], "caught", "I"),
                Value::Int(7)
            );
            assert_eq!(session.status(ids["p/T"]).unwrap(), IS::Completed);
            if body_throw {
                assert_eq!(session.status(ids["q/Target"]).unwrap(), IS::Completed);
                assert_eq!(session.retained_roots().count(), 0);
            } else {
                assert_eq!(
                    session.status(ids["q/Target"]).unwrap(),
                    IS::Erroneous { cause: thrown }
                );
                assert_eq!(
                    session.retained_roots().collect::<Vec<_>>(),
                    vec![thrown.heap_ref().unwrap()]
                );
                assert_eq!(
                    initialize(session, ids["q/Target"]).unwrap_err().kind,
                    E::UnsupportedNoClassDefFoundError {
                        class: ids["q/Target"]
                    }
                );
            }
        });
    }
}
#[test]
fn target_host_preflight_and_reference_argument_failures_precede_initialization_and_stay_retryable()
{
    for native in [false, true] {
        let mut target = ordinary("q/Target");
        target.constant_int("flag", "I", 9, 23);
        target.method(
            "leaf",
            "(Lq/Error;)I",
            if native { 0x109 } else { 9 },
            &[0x04, 0xac],
            1,
            1,
        );
        let mut root = ordinary("p/T");
        let call = root.reference(10, "q/Target", "leaf", "(Lq/Error;)I");
        let mut code = vec![0x2a];
        code.extend(op(0xb8, call));
        code.push(0xac);
        root.method("run", "(Ljava/lang/Object;)I", 9, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root, target]);
        let reference = fixture.object("q/Exception");
        let ids = fixture.classes.clone();
        let method = fixture.method("p/T", "run", "(Ljava/lang/Object;)I");
        fixture.run(InitializationLimits::default(),|session,statics|{let error=session.run_static(method,&[Value::Reference(reference)],ExecutionOptions::default(),CallLimits::default(),ExceptionLimits::default()).unwrap_err();if native {assert!(matches!(error.kind,E::UnsupportedMethod(_)));}else{assert!(matches!(error.kind,E::MethodAccess(ref e) if matches!(**e,MethodAccessError::IncompatibleReference(_))));}assert_eq!(error.initialization_class,None);assert_eq!(session.status(ids["p/T"]).unwrap(),IS::Completed);assert_eq!(session.status(ids["q/Target"]).unwrap(),IS::Uninitialized);assert_eq!(inspect(session,&statics,ids["q/Target"],"flag","I"),Value::Int(0));});
    }
}
#[test]
fn aborted_cross_class_target_is_sticky_and_preserves_completed_root_and_prior_effects() {
    let mut target = ordinary("q/Target");
    target.field("flag", "I", 9);
    let field = target.reference(9, "q/Target", "flag", "I");
    let mut code = vec![0x10, 7];
    code.extend(op(0xb3, field));
    code.extend([0x01, 0xc2, 0xb1]);
    clinit(&mut target, &code, 1);
    target.method("leaf", "()I", 9, &[0x04, 0xac], 1, 0);
    let mut root = body_root("q/Target", "leaf", "()I", &[], 1, 0);
    root.methods[0].flags = 9;
    let mut fixture = Fixture::new(vec![root, target]);
    let ids = fixture.classes.clone();
    let method = fixture.method("p/T", "run", "()I");
    fixture.run(InitializationLimits::default(), |session, statics| {
        for repeat in [false, true] {
            let error = session
                .run_static(
                    method,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default(),
                )
                .unwrap_err();
            if repeat {
                assert_eq!(
                    error.kind,
                    E::InitializationAborted {
                        class: ids["q/Target"]
                    }
                );
                assert_eq!(error.offset, Some(0));
                assert_eq!(error.method.as_ref().unwrap().owner, Some(ids["p/T"]));
            } else {
                assert_eq!(error.kind, E::UnsupportedInstruction(0xc2));
                assert_eq!(error.method.as_ref().unwrap().owner, Some(ids["q/Target"]));
            }
            assert_eq!(error.initialization_class, Some(ids["q/Target"]));
            assert_eq!(session.status(ids["q/Target"]).unwrap(), IS::Aborted);
            assert_eq!(session.status(ids["p/T"]).unwrap(), IS::Completed);
            assert_eq!(
                inspect(session, &statics, ids["q/Target"], "flag", "I"),
                Value::Int(7)
            );
        }
    });
}

#[test]
fn protected_static_uses_declarer_ancestry_without_symbolic_owner_comparability() {
    let mut base = ordinary("q/Base");
    base.method("leaf", "()I", 12, &[0x04, 0xac], 1, 0);
    let sibling = Class::new("q/Sibling", Some("q/Base"), 0x21);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Sibling", "leaf", "()I");
    let mut code = op(0xb8, call);
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root, base, sibling]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture
            .eval(Profile::Body, method, receiver, &[], Options::default())
            .unwrap(),
        returned(Profile::Body, Value::Int(1))
    );
}
#[test]
fn interface_own_private_static_helpers_work_but_parent_interface_statics_are_not_inherited() {
    for inherited in [false, true] {
        let mut parent = iface("q/I");
        parent.method("helper", "()I", 10, &[0x04, 0xac], 1, 0);
        let helper = parent.reference(11, "q/I", "helper", "()I");
        let mut code = op(0xb8, helper);
        code.push(0xac);
        parent.method("leaf", "()I", 9, &code, 1, 0);
        let mut child = iface("q/J");
        implement(&mut child, "q/I");
        let mut root = ordinary("p/T");
        let call = root.reference(11, if inherited { "q/J" } else { "q/I" }, "leaf", "()I");
        let mut code = op(0xb8, call);
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root, parent, child]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture
            .eval(Profile::Body, method, receiver, &[], Options::default())
            .unwrap();
        assert_eq!(
            result,
            if inherited {
                Outcome::Legacy(O::NoSuchMethodError)
            } else {
                returned(Profile::Body, Value::Int(1))
            }
        );
    }
}
#[test]
fn selected_static_result_failure_keeps_callee_owner_and_real_return_pc() {
    let mut target = ordinary("q/Target");
    target.method(
        "identity",
        "(Ljava/lang/Object;)Lq/Base;",
        9,
        &[0x2a, 0xb0],
        1,
        1,
    );
    let mut root = ordinary("p/T");
    let call = root.reference(10, "q/Target", "identity", "(Ljava/lang/Object;)Lq/Base;");
    let mut code = vec![0x2b];
    code.extend(op(0xb8, call));
    code.push(0xb0);
    root.method(
        "run",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        1,
        &code,
        1,
        2,
    );
    let base = ordinary("q/Base");
    let mut fixture = Fixture::new(vec![root, target, base]);
    let receiver = fixture.object("p/T");
    let base = fixture.object("q/Base");
    let method = fixture.method("p/T", "run", "(Ljava/lang/Object;)Ljava/lang/Object;");
    assert_eq!(
        fixture
            .eval(
                Profile::Body,
                method,
                receiver,
                &[Value::Reference(base)],
                Options::default()
            )
            .unwrap(),
        returned(Profile::Body, Value::Reference(base))
    );
    let error = fixture
        .eval(
            Profile::Body,
            method,
            receiver,
            &[Value::Reference(receiver)],
            Options::default(),
        )
        .unwrap_err();
    assert!(
        matches!(error.kind,E::MethodAccess(ref error) if matches!(**error,MethodAccessError::IncompatibleReference(_)))
    );
    assert_eq!(
        error.method.as_ref().unwrap().owner,
        Some(fixture.classes["q/Target"])
    );
    assert_eq!(error.offset, Some(1));
    let site = error.call_site.unwrap();
    assert_eq!(site.method.owner, Some(fixture.classes["p/T"]));
    assert_eq!(site.offset, 1);
}
#[test]
fn cross_class_transfer_preserves_raw_float_double_bits_and_narrows_all_int_return_kinds() {
    for double in [false, true] {
        let descriptor = if double {
            "(FDJLjava/lang/Object;)D"
        } else {
            "(FDJLjava/lang/Object;)F"
        };
        let mut target = ordinary("q/Target");
        target.method(
            "mixed",
            descriptor,
            9,
            if double { &[0x27, 0xaf] } else { &[0x22, 0xae] },
            2,
            6,
        );
        let mut root = ordinary("p/T");
        let call = root.reference(10, "q/Target", "mixed", descriptor);
        let mut code = vec![0x23, 0x28, 0x16, 4, 0x19, 6];
        code.extend(op(0xb8, call));
        code.push(if double { 0xaf } else { 0xae });
        root.method("run", descriptor, 1, &code, 6, 7);
        let mut fixture = Fixture::new(vec![root, target]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", descriptor);
        let float = Value::Float(0xffc00042);
        let wide = Value::Double(0x8000000000000000);
        assert_eq!(
            fixture
                .eval(
                    Profile::Body,
                    method,
                    receiver,
                    &[
                        float,
                        wide,
                        Value::Long(i64::MIN),
                        Value::Reference(receiver)
                    ],
                    Options::default()
                )
                .unwrap(),
            returned(Profile::Body, if double { wide } else { float })
        );
    }
    for (kind, input, expected) in [
        ("B", 0x1234ffff, -1),
        ("C", -1, 65535),
        ("S", 0x1234ffff, -1),
        ("Z", -3, 1),
    ] {
        let descriptor = format!("(I){kind}");
        let mut target = ordinary("q/Target");
        target.method("narrow", &descriptor, 9, &[0x1a, 0xac], 1, 1);
        let mut root = ordinary("p/T");
        let call = root.reference(10, "q/Target", "narrow", &descriptor);
        let mut code = vec![0x1b];
        code.extend(op(0xb8, call));
        code.push(0xac);
        root.method("run", "(I)I", 1, &code, 1, 2);
        let mut fixture = Fixture::new(vec![root, target]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "(I)I");
        assert_eq!(
            fixture
                .eval(
                    Profile::Body,
                    method,
                    receiver,
                    &[Value::Int(input)],
                    Options::default()
                )
                .unwrap(),
            returned(Profile::Body, Value::Int(expected))
        );
    }
}
#[test]
fn nominal_cross_loader_resolution_precedes_even_null_or_missing_arguments_but_primitive_arrays_are_allowed()
 {
    use crabspot_runtime::{HierarchyResolver, LoaderId, ResolveError};
    struct Delegate {
        local: LoaderId,
        remote: LoaderId,
    }
    impl HierarchyResolver for Delegate {
        fn resolve(
            &mut self,
            registry: &DefinitionRegistry,
            initiating: LoaderId,
            name: &[u16],
        ) -> Result<Option<ClassId>, ResolveError> {
            for loader in [initiating, self.local, self.remote] {
                if let Some(class) = registry
                    .lookup_class(loader, name)
                    .map_err(|e| ResolveError::new(e.to_string()))?
                {
                    return Ok(Some(class));
                }
            }
            Ok(None)
        }
    }
    for (nominal, empty) in [(true, false), (true, true), (false, false)] {
        let descriptor = if nominal {
            "(Ljava/lang/Object;)I"
        } else {
            "([I)I"
        };
        let mut target = ordinary("q/Target");
        target.method("leaf", descriptor, 9, &[0x04, 0xac], 1, 1);
        let mut root = Class::new("p/T", Some("q/Target"), 0x21);
        let call = root.reference(10, "q/Target", "leaf", descriptor);
        let mut code = if empty { vec![] } else { vec![0x01] };
        code.extend(op(0xb8, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut classes = vec![target, root];
        for (name, parent, flags) in [
            ("java/lang/Object", None, 0x21),
            ("java/lang/Throwable", Some("java/lang/Object"), 0x21),
            ("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
            ("java/io/Serializable", Some("java/lang/Object"), 0x601),
        ] {
            classes.push(Class::new(name, parent, flags));
        }
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let local = registry.create_loader().unwrap();
        let remote = registry.create_loader().unwrap();
        let mut handles = HashMap::new();
        for class in classes {
            let id = registry
                .define(
                    if class.name == "q/Target" {
                        remote
                    } else {
                        local
                    },
                    &units(&class.name),
                    &class.bytes(),
                    ParseOptions::default(),
                )
                .unwrap();
            handles.insert(class.name, id);
        }
        let mut resolver = Delegate { local, remote };
        let plans = handles
            .values()
            .map(|id| {
                crabspot_runtime::build_hierarchy(
                    &registry,
                    *id,
                    &mut resolver,
                    HierarchyOptions::default(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(
            &registry,
            ReferenceTypeUniverse {
                bootstrap_loader: local,
                object: handles["java/lang/Object"],
                cloneable: handles["java/lang/Cloneable"],
                serializable: handles["java/io/Serializable"],
            },
        )
        .unwrap();
        let mut fixture = Fixture {
            registry,
            heap,
            classes: handles,
        };
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.eval(Profile::Body, method, receiver, &[], Options::default());
        if nominal {
            let error = result.unwrap_err();
            assert!(
                matches!(error.kind,E::MethodAccess(ref error) if matches!(**error,MethodAccessError::UnsupportedLoadingConstraint{origin,method} if origin==fixture.classes["p/T"]&&method.owner()==fixture.classes["q/Target"]))
            );
            assert_eq!(error.offset, Some(u32::from(!empty)));
        } else {
            assert_eq!(result.unwrap(), returned(Profile::Body, Value::Int(1)));
        }
    }
}
#[test]
fn ten_thousand_cross_class_recursive_calls_share_one_iterative_engine_and_checked_limits() {
    let count = 10_000usize;
    let mut a = ordinary("q/A");
    let mut b = ordinary("q/B");
    for (class, other) in [(&mut a, "q/B"), (&mut b, "q/A")] {
        let call = class.reference(10, other, "recur", "(I)I");
        let mut code = vec![0x1a, 0x9e, 0, 10, 0x1a, 0x04, 0x64];
        code.extend(op(0xb8, call));
        code.extend([0xac, 0x03, 0xac]);
        class.method("recur", "(I)I", 9, &code, 2, 1);
    }
    let mut root = ordinary("p/T");
    let call = root.reference(10, "q/A", "recur", "(I)I");
    let mut code = vec![0x1b];
    code.extend(op(0xb8, call));
    code.push(0xac);
    root.method("run", "(I)I", 1, &code, 1, 2);
    let mut fixture = Fixture::new(vec![root, a, b]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "(I)I");
    assert_eq!(
        fixture
            .eval(
                Profile::Body,
                method,
                receiver,
                &[Value::Int(count as i32)],
                Options {
                    execution: ExecutionOptions {
                        instruction_budget: (count as u64) * 7 + 7
                    },
                    calls: CallLimits {
                        max_frames: count + 2,
                        max_frame_slots: (count + 2) * 3,
                        max_prepared_code_bytes: 31
                    },
                    ..Options::default()
                }
            )
            .unwrap(),
        returned(Profile::Body, Value::Int(0))
    );
}

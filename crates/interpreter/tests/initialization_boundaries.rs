//! Actual initializer scheduling with retained active-use continuations and failures.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome as EO, ExecutionError, ExecutionErrorKind as E,
    ExecutionOptions, InitializationLimits, InitializationOutcome as IO,
    InitializationResource as IR, InitializationSession, InitializationStatus as IS, Value,
    ValueKind,
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
    fn new(mut classes: Vec<Class>) -> Self {
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
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
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
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        let plans: Vec<_> = handles
            .values()
            .map(|id| {
                crabspot_runtime::build_hierarchy(
                    &registry,
                    *id,
                    &mut SameLoaderResolver,
                    HierarchyOptions::default(),
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

fn run_root(
    session: &mut InitializationSession<'_, '_>,
    method: MethodId,
    args: &[Value],
    limits: CallLimits,
) -> Result<EO, ExecutionError> {
    session.run_static(
        method,
        args,
        ExecutionOptions::default(),
        limits,
        ExceptionLimits::default(),
    )
}
fn no_frames() -> CallLimits {
    CallLimits {
        max_frames: 0,
        max_frame_slots: 0,
        max_prepared_code_bytes: 0,
    }
}

#[test]
fn constructor_profiles_precede_class_limit_anchor_and_permanent_claim_checks() {
    let mut fixture = Fixture::new(vec![ordinary("C")]);
    let mut foreign = Fixture::new(vec![ordinary("C")]);
    let (statics, exceptions) = fixture.profiles();
    let (other_statics, other_exceptions) = foreign.profiles();
    let error = fixture.classes["java/lang/Error"];
    let wrong_anchor = fixture.classes["java/lang/Object"];
    let count = fixture.heap.retained_layout_classes();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let before = context.lookup_steps();
    let limits = InitializationLimits {
        max_classes: 0,
        ..InitializationLimits::default()
    };
    let error_kind = InitializationSession::new(
        &mut context,
        &other_statics,
        &exceptions,
        wrong_anchor,
        limits,
    )
    .err()
    .unwrap()
    .kind;
    assert!(
        matches!(error_kind,E::StaticAccess(error) if *error==crabspot_runtime::StaticAccessError::ProfileMismatch)
    );
    let error_kind = InitializationSession::new(
        &mut context,
        &statics,
        &other_exceptions,
        wrong_anchor,
        limits,
    )
    .err()
    .unwrap()
    .kind;
    assert!(
        matches!(error_kind,E::ExceptionAccess(error) if *error==crabspot_runtime::ExceptionAccessError::ProfileMismatch)
    );
    assert_eq!(
        InitializationSession::new(&mut context, &statics, &exceptions, wrong_anchor, limits)
            .err()
            .unwrap()
            .kind,
        E::InitializationLimit {
            resource: IR::Classes,
            limit: 0,
            required: count
        }
    );
    assert_eq!(context.lookup_steps(), before);
    let exact = InitializationLimits {
        max_classes: count,
        ..InitializationLimits::default()
    };
    assert!(
        matches!(InitializationSession::new(&mut context,&statics,&exceptions,wrong_anchor,exact).err().unwrap().kind,E::InitializationAccess(error) if matches!(*error,InitializationAccessError::InvalidErrorAnchor {class,..} if class==wrong_anchor))
    );
    {
        let session =
            InitializationSession::new(&mut context, &statics, &exceptions, error, exact).unwrap();
        assert_eq!(session.last_initialization_steps(), 0);
        assert_eq!(session.retained_roots().count(), 0);
    }
    assert!(
        matches!(InitializationSession::new(&mut context,&statics,&exceptions,error,exact).err().unwrap().kind,E::InitializationAccess(error) if *error==InitializationAccessError::AlreadyClaimed)
    );
}

#[test]
fn class_limit_exact_and_maximum_allow_only_actual_small_constructor_reservations() {
    for generous in [false, true] {
        let mut fixture = Fixture::new(vec![ordinary("C")]);
        let (statics, exceptions) = fixture.profiles();
        let error = fixture.classes["java/lang/Error"];
        let count = fixture.heap.retained_layout_classes();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &fixture.registry,
            &mut fixture.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let session = InitializationSession::new(
            &mut context,
            &statics,
            &exceptions,
            error,
            InitializationLimits {
                max_classes: if generous { usize::MAX } else { count },
                max_pending: usize::MAX,
                max_plan_dependencies: usize::MAX,
                max_constant_values: usize::MAX,
                max_steps: u64::MAX,
            },
        )
        .unwrap();
        assert_eq!(session.retained_roots().count(), 0);
        assert_eq!(session.last_initialization_steps(), 0);
    }
}

#[test]
fn ordinary_static_root_eligibility_fails_before_owner_initialization_and_remains_retryable() {
    let mut class = ordinary("C");
    class.field("state", "I", 9);
    set_initializer(&mut class, "state", 61);
    class.method("instance", "()V", 1, &[0xb1], 0, 1);
    class.method("native", "()V", 0x109, &[], 0, 0);
    class.method("synchronized", "()V", 0x29, &[0xb1], 0, 0);
    class.method("<init>", "()V", 1, &[0xb1], 0, 1);
    class.method("valid", "()I", 9, &[0x07, 0xac], 1, 0);
    let mut fixture = Fixture::new(vec![class]);
    let owner = fixture.classes["C"];
    let bad: Vec<_> = ["instance", "native", "synchronized", "<init>", "<clinit>"]
        .iter()
        .map(|name| fixture.method("C", name, "()V"))
        .collect();
    let good = fixture.method("C", "valid", "()I");
    fixture.run(InitializationLimits::default(), |session, statics| {
        for method in bad {
            let error = run_root(session, method, &[], CallLimits::default()).unwrap_err();
            assert!(matches!(error.kind, E::UnsupportedMethod(_)));
            assert_eq!(session.status(owner).unwrap(), IS::Uninitialized);
            assert_eq!(session.last_initialization_steps(), 0);
            assert_eq!(
                inspect(session, &statics, owner, "state", "I"),
                Value::Int(0)
            );
        }
        assert_eq!(
            run_root(session, good, &[], CallLimits::default()).unwrap(),
            EO::Returned(Some(Value::Int(4)))
        );
        assert_eq!(session.status(owner).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, owner, "state", "I"),
            Value::Int(61)
        );
    });
}

#[test]
fn root_computational_frame_slot_and_code_preflight_precedes_owner_initialization() {
    let mut class = ordinary("C");
    class.field("state", "I", 9);
    set_initializer(&mut class, "state", 73);
    class.method("root", "(JLjava/lang/Object;D)I", 9, &[0x06, 0xac], 1, 5);
    let mut fixture = Fixture::new(vec![class]);
    let owner = fixture.classes["C"];
    let root = fixture.method("C", "root", "(JLjava/lang/Object;D)I");
    let args = [
        Value::Long(9),
        Value::Reference(Reference::null()),
        Value::Double(0),
    ];
    fixture.run(InitializationLimits::default(), |session, statics| {
        assert_eq!(
            run_root(session, root, &args[..1], CallLimits::default())
                .unwrap_err()
                .kind,
            E::ArgumentCount {
                expected: 3,
                actual: 1
            }
        );
        let wrong = [Value::Long(9), Value::Int(0), Value::Double(0)];
        assert_eq!(
            run_root(session, root, &wrong, CallLimits::default())
                .unwrap_err()
                .kind,
            E::ArgumentType {
                index: 1,
                expected: ValueKind::Reference,
                actual: ValueKind::Int
            }
        );
        for (limits, expected) in [
            (
                CallLimits {
                    max_frames: 0,
                    ..CallLimits::default()
                },
                E::FrameLimit {
                    limit: 0,
                    required: 1,
                },
            ),
            (
                CallLimits {
                    max_frame_slots: 5,
                    ..CallLimits::default()
                },
                E::FrameSlotLimit {
                    limit: 5,
                    required: 6,
                },
            ),
            (
                CallLimits {
                    max_prepared_code_bytes: 1,
                    ..CallLimits::default()
                },
                E::PreparedCodeLimit {
                    limit: 1,
                    required: 2,
                },
            ),
        ] {
            assert_eq!(
                run_root(session, root, &args, limits).unwrap_err().kind,
                expected
            );
            assert_eq!(session.status(owner).unwrap(), IS::Uninitialized);
            assert_eq!(session.last_initialization_steps(), 0);
            assert_eq!(
                inspect(session, &statics, owner, "state", "I"),
                Value::Int(0)
            );
        }
        assert_eq!(
            run_root(
                session,
                root,
                &args,
                CallLimits {
                    max_frames: 1,
                    max_frame_slots: 6,
                    max_prepared_code_bytes: 8
                }
            )
            .unwrap(),
            EO::Returned(Some(Value::Int(3)))
        );
        assert_eq!(
            inspect(session, &statics, owner, "state", "I"),
            Value::Int(73)
        );
    });
}

#[test]
fn malformed_root_code_is_eagerly_rejected_without_beginning_its_owner() {
    let mut class = ordinary("C");
    class.field("state", "I", 9);
    set_initializer(&mut class, "state", 49);
    class.method("broken", "()V", 9, &[0x10], 1, 0);
    let mut fixture = Fixture::new(vec![class]);
    let owner = fixture.classes["C"];
    let root = fixture.method("C", "broken", "()V");
    fixture.run(InitializationLimits::default(), |session, statics| {
        let error = run_root(session, root, &[], CallLimits::default()).unwrap_err();
        assert!(matches!(error.kind, E::Decode(_)));
        assert_eq!(error.offset, Some(1));
        assert_eq!(error.method.as_ref().unwrap().owner, Some(owner));
        assert!(error.call_site.is_none());
        assert_eq!(session.status(owner).unwrap(), IS::Uninitialized);
        assert_eq!(session.last_initialization_steps(), 0);
        assert_eq!(
            inspect(session, &statics, owner, "state", "I"),
            Value::Int(0)
        );
    });
}

#[test]
fn status_checks_foreign_and_defined_unadmitted_classes_without_work_or_resolution() {
    let mut fixture = Fixture::new(vec![ordinary("C")]);
    let owner = fixture.classes["C"];
    let loader = fixture.registry.defining_loader(owner).unwrap();
    let class = ordinary("DefinedOnly");
    let unadmitted = fixture
        .registry
        .define(
            loader,
            &units("DefinedOnly"),
            &class.bytes(),
            ParseOptions::default(),
        )
        .unwrap();
    let other = Fixture::new(vec![ordinary("C")]);
    let foreign = other.classes["C"];
    fixture.run(InitializationLimits::default(),|session,_| {let before=session.context().lookup_steps();assert_eq!(session.status(owner).unwrap(),IS::Uninitialized);assert!(matches!(session.status(foreign).unwrap_err().kind,E::InitializationAccess(error) if matches!(*error,InitializationAccessError::Context(crabspot_runtime::FieldAccessError::Registry(_)))));assert!(matches!(session.status(unadmitted).unwrap_err().kind,E::InitializationAccess(error) if matches!(*error,InitializationAccessError::Context(crabspot_runtime::FieldAccessError::Instance(crabspot_runtime::InstanceError::LayoutMissing {class})) if class==unadmitted)));assert_eq!(session.context().lookup_steps(),before);assert_eq!(session.last_initialization_steps(),0);assert_eq!(session.retained_roots().count(),0);});
}

#[test]
fn pending_limit_zero_and_nested_exact_boundary_keep_unstarted_children_uninitialized() {
    for maximum in [0, 1, 2] {
        let mut fixture = Fixture::new(vec![ordinary("C")]);
        let owner = fixture.classes["C"];
        let object = fixture.classes["java/lang/Object"];
        fixture.run(
            InitializationLimits {
                max_pending: maximum,
                ..InitializationLimits::default()
            },
            |session, _| {
                let result = session.initialize(
                    owner,
                    ExecutionOptions {
                        instruction_budget: 0,
                    },
                    no_frames(),
                    ExceptionLimits::default(),
                );
                match maximum {
                    0 => {
                        let error = result.unwrap_err();
                        assert_eq!(
                            error.kind,
                            E::InitializationLimit {
                                resource: IR::Pending,
                                limit: 0,
                                required: 1
                            }
                        );
                        assert_eq!(error.initialization_class, Some(owner));
                        assert_eq!(session.status(owner).unwrap(), IS::Uninitialized);
                    }
                    1 => {
                        let error = result.unwrap_err();
                        assert_eq!(
                            error.kind,
                            E::InitializationLimit {
                                resource: IR::Pending,
                                limit: 1,
                                required: 2
                            }
                        );
                        assert_eq!(error.initialization_class, Some(object));
                        assert_eq!(session.status(owner).unwrap(), IS::Aborted);
                        assert_eq!(session.status(object).unwrap(), IS::Uninitialized);
                    }
                    _ => {
                        assert_eq!(result.unwrap(), IO::Completed);
                        assert_eq!(session.status(owner).unwrap(), IS::Completed);
                        assert_eq!(session.status(object).unwrap(), IS::Completed);
                    }
                }
            },
        );
    }
}

#[test]
fn no_body_scheduler_steps_are_inclusive_report_the_actual_class_and_need_no_java_frame() {
    for maximum in [0, 1, 2, 4, 6, 7, 8, 9, u64::MAX] {
        let mut fixture = Fixture::new(vec![ordinary("C")]);
        let owner = fixture.classes["C"];
        let object = fixture.classes["java/lang/Object"];
        fixture.run(
            InitializationLimits {
                max_steps: maximum,
                ..InitializationLimits::default()
            },
            |session, _| {
                let result = session.initialize(
                    owner,
                    ExecutionOptions {
                        instruction_budget: 0,
                    },
                    no_frames(),
                    ExceptionLimits::default(),
                );
                if maximum < 9 {
                    let error = result.unwrap_err();
                    assert_eq!(
                        error.kind,
                        E::InitializationStepLimit {
                            limit: maximum,
                            required: maximum + 1
                        }
                    );
                    assert_eq!(
                        error.initialization_class,
                        Some(if matches!(maximum, 4 | 6) {
                            object
                        } else {
                            owner
                        })
                    );
                    assert!(error.method.is_none());
                    assert!(error.offset.is_none());
                    assert_eq!(session.last_initialization_steps(), maximum);
                    assert_eq!(
                        session.status(owner).unwrap(),
                        if maximum < 2 {
                            IS::Uninitialized
                        } else {
                            IS::Aborted
                        }
                    );
                    assert_eq!(
                        session.status(object).unwrap(),
                        if maximum < 5 {
                            IS::Uninitialized
                        } else if maximum < 7 {
                            IS::Aborted
                        } else {
                            IS::Completed
                        }
                    );
                } else {
                    assert_eq!(result.unwrap(), IO::Completed);
                    assert_eq!(session.last_initialization_steps(), 9);
                    assert_eq!(
                        session
                            .initialize(
                                owner,
                                ExecutionOptions {
                                    instruction_budget: 0
                                },
                                no_frames(),
                                ExceptionLimits::default()
                            )
                            .unwrap(),
                        IO::Completed
                    );
                    assert_eq!(session.last_initialization_steps(), 1);
                }
            },
        );
    }
}

#[test]
fn aggregate_dependency_allowance_is_remaining_and_completed_records_do_not_recharge_it() {
    let mut fixture = Fixture::new(vec![ordinary("C"), ordinary("D")]);
    let c = fixture.classes["C"];
    let d = fixture.classes["D"];
    let object = fixture.classes["java/lang/Object"];
    fixture.run(InitializationLimits {max_plan_dependencies:1,..InitializationLimits::default()},|session,_| {assert_eq!(initialize(session,c).unwrap(),IO::Completed);assert_eq!(initialize(session,c).unwrap(),IO::Completed);assert_eq!(session.last_initialization_steps(),1);for _ in 0..2 {let error=initialize(session,d).unwrap_err();assert!(matches!(error.kind,E::InitializationAccess(error) if *error==InitializationAccessError::PlanLimit {resource:crabspot_runtime::InitializationPlanResource::Dependencies,limit:0,required:1}));assert_eq!(error.initialization_class,Some(d));assert_eq!(session.status(d).unwrap(),IS::Uninitialized);assert_eq!(session.status(c).unwrap(),IS::Completed);assert_eq!(session.status(object).unwrap(),IS::Completed);assert_eq!(session.last_initialization_steps(),1);}});
}

#[test]
fn failed_constant_plan_does_not_consume_remaining_allowance_or_publish_values() {
    let mut c = ordinary("C");
    c.constant_int("value", "I", 9, 31);
    let mut d = ordinary("D");
    d.constant_int("a", "I", 9, 41);
    d.constant_int("b", "I", 25, 59);
    let mut other = ordinary("Other");
    other.constant_int("value", "I", 9, 73);
    let mut fixture = Fixture::new(vec![c, d, other]);
    let c = fixture.classes["C"];
    let d = fixture.classes["D"];
    let other = fixture.classes["Other"];
    fixture.run(InitializationLimits {max_constant_values:2,..InitializationLimits::default()},|session,statics| {initialize(session,c).unwrap();let error=initialize(session,d).unwrap_err();assert!(matches!(error.kind,E::InitializationAccess(error) if *error==InitializationAccessError::PlanLimit {resource:crabspot_runtime::InitializationPlanResource::Constants,limit:1,required:2}));assert_eq!(error.initialization_class,Some(d));assert_eq!(session.status(d).unwrap(),IS::Uninitialized);assert_eq!(inspect(session,&statics,d,"a","I"),Value::Int(0));assert_eq!(inspect(session,&statics,d,"b","I"),Value::Int(0));initialize(session,other).unwrap();assert_eq!(inspect(session,&statics,c,"value","I"),Value::Int(31));assert_eq!(inspect(session,&statics,other,"value","I"),Value::Int(73));assert!(matches!(initialize(session,d).unwrap_err().kind,E::InitializationAccess(error) if *error==InitializationAccessError::PlanLimit {resource:crabspot_runtime::InitializationPlanResource::Constants,limit:0,required:1}));assert_eq!(session.status(d).unwrap(),IS::Uninitialized);});
}

#[test]
fn maximal_policies_on_small_no_body_inputs_do_not_allocate_policy_sized_state() {
    let mut fixture = Fixture::new(vec![ordinary("C")]);
    let c = fixture.classes["C"];
    fixture.run(
        InitializationLimits {
            max_classes: usize::MAX,
            max_plan_dependencies: usize::MAX,
            max_constant_values: usize::MAX,
            max_pending: usize::MAX,
            max_steps: u64::MAX,
        },
        |session, _| {
            assert_eq!(
                session
                    .initialize(
                        c,
                        ExecutionOptions {
                            instruction_budget: 0
                        },
                        no_frames(),
                        ExceptionLimits {
                            max_prepared_handlers: usize::MAX,
                            max_dispatch_steps: u64::MAX
                        }
                    )
                    .unwrap(),
                IO::Completed
            );
            assert_eq!(session.last_initialization_steps(), 9);
            assert_eq!(session.retained_roots().count(), 0);
        },
    );
}

#[test]
fn ordinary_calls_reject_initializer_names_and_do_not_inherit_cached_final_authority() {
    // The public registry rejects <clinit> Methodrefs before execution. A legal
    // constructor Methodref exercises the ordinary b8 special-name gate below.
    let mut forbidden = ordinary("Forbidden");
    let index = forbidden.reference(10, "Forbidden", "<clinit>", "()V");
    let mut code = op(0xb8, index);
    code.push(0xb1);
    forbidden.method("root", "()V", 9, &code, 0, 0);
    let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
    let loader = registry.create_loader().unwrap();
    assert!(matches!(
        registry.define(
            loader,
            &units("Forbidden"),
            &forbidden.bytes(),
            ParseOptions::default()
        ),
        Err(crabspot_runtime::RegistryError::Parse(
            crabspot_classfile::ParseError {
                context: "constant pool",
                ..
            }
        ))
    ));
    let mut class = ordinary("C");
    class.field("final", "I", 25);
    set_initializer(&mut class, "final", 47);
    let initializer = class.reference(10, "C", "<init>", "()V");
    let mut bad = op(0xb8, initializer);
    bad.push(0xb1);
    class.method("callInitializer", "()V", 9, &bad, 0, 0);
    let field = class.reference(9, "C", "final", "I");
    let mut helper = vec![0x04];
    helper.extend(op(0xb3, field));
    helper.push(0xb1);
    class.method("helper", "()V", 9, &helper, 1, 0);
    let helper_ref = class.reference(10, "C", "helper", "()V");
    let mut root = op(0xb8, helper_ref);
    root.push(0xb1);
    class.method("callHelper", "()V", 9, &root, 0, 0);
    let mut fixture = Fixture::new(vec![class]);
    let owner = fixture.classes["C"];
    let bad = fixture.method("C", "callInitializer", "()V");
    let root = fixture.method("C", "callHelper", "()V");
    let helper = fixture.method("C", "helper", "()V");
    fixture.run(InitializationLimits::default(), |session, statics| {
        let error = run_root(session, bad, &[], CallLimits::default()).unwrap_err();
        assert!(matches!(error.kind, E::MethodAccess(ref error) if matches!(**error, crabspot_runtime::MethodAccessError::UnsupportedSpecialMethod)));
        assert_eq!(error.offset, Some(0));
        assert_eq!(session.status(owner).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, owner, "final", "I"),
            Value::Int(47)
        );
        let error = run_root(session, root, &[], CallLimits::default()).unwrap_err();
        assert_eq!(
            error.kind,
            E::UnsupportedVmException(crabspot_runtime::VmExceptionKind::IllegalAccessError)
        );
        assert_eq!(error.offset, Some(1));
        assert_eq!(error.method.as_ref().unwrap().index, helper.index());
        assert_eq!(error.call_site.as_ref().unwrap().offset, 0);
        assert_eq!(session.status(owner).unwrap(), IS::Completed);
        assert_eq!(
            inspect(session, &statics, owner, "final", "I"),
            Value::Int(47)
        );
    });
}

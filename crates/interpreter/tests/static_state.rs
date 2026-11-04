//! Explicit host-controlled static memory composed with the existing body engine.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome as EO, ExecutionError, ExecutionErrorKind as E,
    ExecutionOptions, ExecutionOutcome as O, Value, ValueKind, execute_instance_method_body,
    execute_instance_method_body_with_exceptions, execute_static_method_body,
};
use crabspot_interpreter::{
    execute_instance_method_body_with_static_state,
    execute_instance_method_body_with_static_state_and_exceptions,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap, MethodId, PrimitiveArrayKind,
    Reference, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver, build_hierarchy,
};
use crabspot_runtime::{ExceptionAccessError, ExceptionProfile, VmExceptionKind};
use crabspot_runtime::{StaticAccessError, StaticStateOptions, StaticStateProfile};
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
        self.pool.len() as u16
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
    fn method(
        &mut self,
        name: &str,
        descriptor: &str,
        flags: u16,
        code: &[u8],
        stack: u16,
        locals: u16,
    ) {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let code_name = self.utf8("Code");
        self.methods.push(Method {
            flags,
            name,
            descriptor,
            code_name,
            code: if flags & 0x500 != 0 {
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
            u2(&mut bytes, 0);
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
        Self::configured(classes, true)
    }
    fn configured(classes: Vec<Class>, bind: bool) -> Self {
        Self::configured_arrays(classes, bind, HeapOptions::default())
    }
    fn configured_arrays(mut classes: Vec<Class>, bind: bool, array_options: HeapOptions) -> Self {
        for (name, parent) in [
            ("java/lang/Throwable", "java/lang/Object"),
            ("q/BaseThrow", "java/lang/Throwable"),
            ("q/ChildThrow", "q/BaseThrow"),
        ] {
            if !classes.iter().any(|class| class.name == name) {
                classes.push(Class::new(name, Some(parent), 0x21));
            }
        }
        for name in ["java/lang/Cloneable", "java/io/Serializable"] {
            if !classes.iter().any(|class| class.name == name) {
                classes.push(Class::new(name, Some("java/lang/Object"), 0x601));
            }
        }
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut handles = HashMap::new();
        let explicit_object = classes.iter().any(|class| class.name == "java/lang/Object");
        for class in std::iter::once(Class::new("java/lang/Object", None, 0x21))
            .filter(|_| !explicit_object)
            .chain(classes)
        {
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
            ManagedHeap::new_with_instances(array_options, InstanceOptions::default()).unwrap();
        let plans: Vec<_> = handles
            .values()
            .map(|class| {
                build_hierarchy(
                    &registry,
                    *class,
                    &mut SameLoaderResolver,
                    HierarchyOptions::default(),
                )
                .unwrap()
            })
            .collect();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        if bind {
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
        }
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
    fn object(&mut self, class: &str) -> Reference {
        self.heap.allocate_instance(self.classes[class]).unwrap()
    }
    fn run(
        &mut self,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
    ) -> Result<O, ExecutionError> {
        self.run_limits(
            method,
            receiver,
            args,
            ExecutionOptions::default(),
            CallLimits::default(),
        )
    }
    fn run_limits(
        &mut self,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
        options: ExecutionOptions,
        limits: CallLimits,
    ) -> Result<O, ExecutionError> {
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        execute_instance_method_body(&mut context, method, receiver, args, options, limits)
    }
    fn statics(&mut self) -> StaticStateProfile {
        let mut resolver = SameLoaderResolver;
        FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap()
        .allocate_static_state(StaticStateOptions::default())
        .unwrap()
    }
    fn s_run(
        &mut self,
        profile: &StaticStateProfile,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
    ) -> Result<O, ExecutionError> {
        self.s_limits(
            profile,
            method,
            receiver,
            args,
            (ExecutionOptions::default(), CallLimits::default()),
        )
    }
    fn s_limits(
        &mut self,
        profile: &StaticStateProfile,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
        policies: (ExecutionOptions, CallLimits),
    ) -> Result<O, ExecutionError> {
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        execute_instance_method_body_with_static_state(
            &mut context,
            profile,
            method,
            receiver,
            args,
            policies.0,
            policies.1,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn both(
        &mut self,
        statics: &StaticStateProfile,
        exceptions: &ExceptionProfile,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
        policies: (ExecutionOptions, CallLimits, ExceptionLimits),
    ) -> Result<EO, ExecutionError> {
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        execute_instance_method_body_with_static_state_and_exceptions(
            &mut context,
            statics,
            exceptions,
            method,
            receiver,
            args,
            policies.0,
            policies.1,
            policies.2,
        )
    }
    fn inspect(
        &mut self,
        statics: &StaticStateProfile,
        class: &str,
        name: &str,
        descriptor: &str,
    ) -> Value {
        let field = self
            .registry
            .find_declared_field(self.classes[class], &units(name), &units(descriptor))
            .unwrap()
            .unwrap();
        let mut resolver = SameLoaderResolver;
        FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap()
        .inspect_static(statics, field)
        .unwrap()
    }
    fn profile(&mut self) -> ExceptionProfile {
        let mut resolver = SameLoaderResolver;
        FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap()
        .prepare_exception_profile(self.classes["java/lang/Throwable"])
        .unwrap()
    }
    fn e_run(
        &mut self,
        profile: &ExceptionProfile,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
    ) -> Result<EO, ExecutionError> {
        self.e_limits(
            profile,
            method,
            receiver,
            args,
            (
                ExecutionOptions::default(),
                CallLimits::default(),
                ExceptionLimits::default(),
            ),
        )
    }
    fn e_limits(
        &mut self,
        profile: &ExceptionProfile,
        method: MethodId,
        receiver: Reference,
        args: &[Value],
        policies: (ExecutionOptions, CallLimits, ExceptionLimits),
    ) -> Result<EO, ExecutionError> {
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &self.registry,
            &mut self.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        execute_instance_method_body_with_exceptions(
            &mut context,
            profile,
            method,
            receiver,
            args,
            policies.0,
            policies.1,
            policies.2,
        )
    }
    fn stored(&self, class: &str, name: &str, descriptor: &str, receiver: Reference) -> Value {
        let field = self
            .registry
            .find_declared_field(self.classes[class], &units(name), &units(descriptor))
            .unwrap()
            .unwrap();
        self.heap.get_field(receiver, field).unwrap()
    }
}
fn ordinary() -> Class {
    Class::new("p/T", Some("java/lang/Object"), 0x21)
}
fn iface(name: &str) -> Class {
    Class::new(name, Some("java/lang/Object"), 0x601)
}
fn implement(class: &mut Class, name: &str) {
    let index = class.class(name);
    class.interfaces.push(index);
}
fn handlers(class: &mut Class, rows: &[[u16; 4]]) {
    class.methods.last_mut().unwrap().handlers = rows.to_vec();
}
fn caught_int(value: i32) -> EO {
    EO::Returned(Some(Value::Int(value)))
}

#[test]
fn all_primitive_static_stores_use_existing_subword_rules_widths_and_raw_bits() {
    for (descriptor, load, ret, value, expected) in [
        ("Z", 0x1b, 0xac, Value::Int(6), Value::Int(0)),
        ("B", 0x1b, 0xac, Value::Int(255), Value::Int(-1)),
        ("C", 0x1b, 0xac, Value::Int(-1), Value::Int(65535)),
        ("S", 0x1b, 0xac, Value::Int(65534), Value::Int(-2)),
        ("I", 0x1b, 0xac, Value::Int(i32::MIN), Value::Int(i32::MIN)),
        (
            "J",
            0x1f,
            0xad,
            Value::Long(i64::MIN),
            Value::Long(i64::MIN),
        ),
        (
            "F",
            0x23,
            0xae,
            Value::Float(0xffc00042),
            Value::Float(0xffc00042),
        ),
        (
            "D",
            0x27,
            0xaf,
            Value::Double(0x8000000000000000),
            Value::Double(0x8000000000000000),
        ),
    ] {
        let mut root = ordinary();
        root.field("value", descriptor, 9);
        let field = root.reference(9, "p/T", "value", descriptor);
        let mut code = vec![load];
        code.extend(op(0xb3, field));
        code.extend(op(0xb2, field));
        code.push(ret);
        let method_descriptor = format!("({descriptor}){descriptor}");
        root.method("run", &method_descriptor, 1, &code, 2, 3);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let statics = fixture.statics();
        let method = fixture.method("p/T", "run", &method_descriptor);
        assert_eq!(
            fixture.s_run(&statics, method, receiver, &[value]).unwrap(),
            O::Returned(Some(expected))
        );
        assert_eq!(
            fixture.inspect(&statics, "p/T", "value", descriptor),
            expected
        );
        assert_eq!(fixture.heap.allocated_instances(), 1);
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
}
#[test]
fn hidden_and_inherited_symbolic_static_owners_share_only_the_declaring_location() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("value", "I", 9);
    let field = base.reference(9, "q/Base", "value", "I");
    let mut code = vec![0x1b];
    code.extend(op(0xb3, field));
    code.extend(op(0xb2, field));
    code.push(0xac);
    base.method("body", "(I)I", 1, &code, 1, 2);
    let child = Class::new("q/Child", Some("q/Base"), 0x21);
    let mut root = Class::new("p/T", Some("q/Child"), 0x21);
    root.field("value", "I", 9);
    let inherited = root.reference(9, "q/Child", "value", "I");
    let own = root.reference(9, "p/T", "value", "I");
    let mut code = vec![0x1b];
    code.extend(op(0xb3, inherited));
    code.extend(op(0xb2, own));
    code.push(0xac);
    root.method("run", "(I)I", 1, &code, 1, 2);
    let mut fixture = Fixture::new(vec![base, child, root]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    let method = fixture.method("p/T", "run", "(I)I");
    assert_eq!(
        fixture
            .s_run(&statics, method, receiver, &[Value::Int(9)])
            .unwrap(),
        O::Returned(Some(Value::Int(0)))
    );
    assert_eq!(
        fixture.inspect(&statics, "q/Base", "value", "I"),
        Value::Int(9)
    );
    assert_eq!(
        fixture.inspect(&statics, "p/T", "value", "I"),
        Value::Int(0)
    );
    let method = fixture.method("q/Base", "body", "(I)I");
    assert_eq!(
        fixture
            .s_run(&statics, method, receiver, &[Value::Int(7)])
            .unwrap(),
        O::Returned(Some(Value::Int(7)))
    );
    assert_eq!(statics.slot_count(), 2);
}
#[test]
fn current_declaring_origin_and_static_capability_follow_all_nested_call_families() {
    let mut other = Class::new("q/Other", Some("java/lang/Object"), 0x21);
    other.field("package", "I", 8);
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let field = base.reference(9, "q/Other", "package", "I");
    let helper = base.reference(10, "q/Base", "helper", "()I");
    let mut code = vec![0x1b];
    code.extend(op(0xb3, field));
    code.extend(op(0xb8, helper));
    code.push(0xac);
    base.method("body", "(I)I", 1, &code, 1, 2);
    let mut code = op(0xb2, field);
    code.push(0xac);
    base.method("helper", "()I", 9, &code, 1, 0);
    let mut interface = iface("q/I");
    let field = interface.reference(9, "q/Other", "package", "I");
    let helper = interface.reference(11, "q/I", "helper", "()I");
    let mut code = vec![0x1b];
    code.extend(op(0xb3, field));
    code.extend(op(0xb8, helper));
    code.push(0xac);
    interface.method("defaultBody", "(I)I", 1, &code, 1, 2);
    let mut code = op(0xb2, field);
    code.push(0xac);
    interface.method("helper", "()I", 9, &code, 1, 0);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    for (name, opcode, owner, body) in [
        ("virtual", 0xb6, "q/Base", "body"),
        ("special", 0xb7, "q/Base", "body"),
        ("interface", 0xb9, "q/I", "defaultBody"),
    ] {
        let call = root.reference(if opcode == 0xb9 { 11 } else { 10 }, owner, body, "(I)I");
        let mut code = vec![0x2a, 0x1b];
        code.extend(op(opcode, call));
        if opcode == 0xb9 {
            code.extend([2, 0]);
        }
        code.push(0xac);
        root.method(name, "(I)I", 1, &code, 2, 2);
    }
    let denied = root.reference(9, "q/Other", "package", "I");
    let mut code = op(0xb2, denied);
    code.push(0xac);
    root.method("denied", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root, base, interface, other]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    for (index, name) in ["virtual", "special", "interface"].iter().enumerate() {
        let method = fixture.method("p/T", name, "(I)I");
        assert_eq!(
            fixture
                .s_run(&statics, method, receiver, &[Value::Int(index as i32 + 3)])
                .unwrap(),
            O::Returned(Some(Value::Int(index as i32 + 3)))
        );
    }
    let method = fixture.method("p/T", "denied", "()I");
    assert_eq!(
        fixture.s_run(&statics, method, receiver, &[]).unwrap(),
        O::IllegalAccessError
    );
    assert_eq!(
        fixture.inspect(&statics, "q/Other", "package", "I"),
        Value::Int(5)
    );
}
#[test]
fn checked_static_reference_aliases_return_identity_and_preserve_duplicate_root_locations() {
    let mut root = ordinary();
    root.field("first", "Ljava/lang/Object;", 9);
    root.field("second", "Ljava/lang/Object;", 9);
    let first = root.reference(9, "p/T", "first", "Ljava/lang/Object;");
    let second = root.reference(9, "p/T", "second", "Ljava/lang/Object;");
    let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let mut code = vec![0x2b];
    code.extend(op(0xb3, first));
    code.push(0x2b);
    code.extend(op(0xb3, second));
    code.extend(op(0xb2, first));
    code.push(0xb0);
    root.method("run", descriptor, 1, &code, 1, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let array = fixture
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Double, 1)
        .unwrap();
    let statics = fixture.statics();
    let method = fixture.method("p/T", "run", descriptor);
    for value in [receiver, array, Reference::null()] {
        assert_eq!(
            fixture
                .s_run(&statics, method, receiver, &[Value::Reference(value)])
                .unwrap(),
            O::Returned(Some(Value::Reference(value)))
        );
        let mut resolver = SameLoaderResolver;
        let context = FieldResolutionContext::new_unnamed_modules(
            &fixture.registry,
            &mut fixture.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let roots: Vec<_> = context.static_roots(&statics).unwrap().collect();
        assert_eq!(
            roots,
            if value.is_null() {
                vec![]
            } else {
                vec![value.heap_ref().unwrap(), value.heap_ref().unwrap()]
            }
        );
        assert_eq!(context.heap().children(receiver).unwrap().count(), 0);
    }
    assert_eq!(statics.slot_count(), 2);
    assert_eq!(fixture.heap.allocated_arrays(), 1);
}

#[test]
fn putstatic_extracts_before_selection_but_wrong_kind_follows_access_category_and_finality() {
    for failure in 0..7 {
        let mut other = Class::new("q/Other", Some("java/lang/Object"), 0x21);
        other.field("package", "I", 8);
        let mut root = ordinary();
        root.field("value", "I", 9);
        root.field("finalValue", "I", 0x19);
        root.field("instance", "I", 1);
        let field = match failure {
            0 => root.utf8("wrong CP kind"),
            1 => root.reference(9, "p/T", "missing", "I"),
            2 => root.reference(9, "q/Other", "package", "I"),
            3 => root.reference(9, "p/T", "instance", "I"),
            4 => root.reference(9, "p/T", "finalValue", "I"),
            5 => root.reference(9, "p/T", "value", "I"),
            _ => root.utf8("wrong CP kind"),
        };
        let mut code = if failure == 0 { Vec::new() } else { vec![0x01] };
        code.extend(op(0xb3, field));
        code.push(0xb1);
        root.method("run", "()V", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root, other]);
        let receiver = fixture.object("p/T");
        let statics = fixture.statics();
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.s_run(&statics, method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap_err().kind, E::StackUnderflow),
            1 => assert_eq!(result.unwrap(), O::NoSuchFieldError),
            2 | 4 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            3 => assert_eq!(result.unwrap(), O::IncompatibleClassChangeError),
            5 => assert!(
                matches!(result.unwrap_err().kind,E::StaticAccess(error) if matches!(error.as_ref(),StaticAccessError::ValueKindMismatch{expected:ValueKind::Int,actual:ValueKind::Reference,..}))
            ),
            _ => assert!(
                matches!(result.unwrap_err().kind,E::StaticAccess(error) if matches!(error.as_ref(),StaticAccessError::Context(FieldAccessError::InvalidConstant{..})))
            ),
        }
        assert_eq!(
            fixture.inspect(&statics, "p/T", "value", "I"),
            Value::Int(0)
        );
        assert_eq!(
            fixture.inspect(&statics, "p/T", "finalValue", "I"),
            Value::Int(0)
        );
    }
}
#[test]
fn old_profiles_reject_static_opcodes_even_with_allocated_static_memory() {
    for opcode in [0xb2, 0xb3] {
        let mut root = ordinary();
        root.field("value", "I", 9);
        let field = root.reference(9, "p/T", "value", "I");
        let mut code = op(opcode, field);
        code.push(0xb1);
        root.method("run", "()V", 1, &code, 1, 1);
        root.method("staticBody", "()V", 9, &code, 1, 0);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let _statics = fixture.statics();
        let exceptions = fixture.profile();
        let method = fixture.method("p/T", "run", "()V");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::UnsupportedInstruction(opcode)
        );
        assert_eq!(
            fixture
                .e_run(&exceptions, method, receiver, &[])
                .unwrap_err()
                .kind,
            E::UnsupportedInstruction(opcode)
        );
        let class = fixture.registry.class(fixture.classes["p/T"]).unwrap();
        assert_eq!(
            execute_static_method_body(class, 1, &[], ExecutionOptions::default())
                .unwrap_err()
                .kind,
            E::UnsupportedInstruction(opcode)
        );
        assert_eq!(
            crabspot_interpreter::execute_static_method_body_with_heap(
                class,
                1,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                &mut fixture.heap
            )
            .unwrap_err()
            .kind,
            E::UnsupportedInstruction(opcode)
        );
    }
}
#[test]
fn static_and_throwable_profiles_compose_without_losing_writes_after_catches_or_uncaught_exits() {
    let mut root = ordinary();
    root.field("value", "I", 9);
    let field = root.reference(9, "p/T", "value", "I");
    let mut code = vec![0x10, 7];
    code.extend(op(0xb3, field));
    code.extend([0x2b, 0xbf, 0x57]);
    code.extend(op(0xb2, field));
    code.push(0xac);
    root.method("caught", "(Ljava/lang/Throwable;)I", 1, &code, 1, 2);
    handlers(&mut root, &[[0, 7, 7, 0]]);
    let mut code = vec![0x10, 9];
    code.extend(op(0xb3, field));
    code.extend([0x2b, 0xbf]);
    root.method("uncaught", "(Ljava/lang/Throwable;)V", 1, &code, 1, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("q/ChildThrow");
    let exceptions = fixture.profile();
    let statics = fixture.statics();
    let method = fixture.method("p/T", "caught", "(Ljava/lang/Throwable;)I");
    assert_eq!(
        fixture
            .s_run(&statics, method, receiver, &[Value::Reference(thrown)])
            .unwrap_err()
            .kind,
        E::UnsupportedMethod("exception handlers require exception dispatch")
    );
    assert_eq!(
        fixture
            .both(
                &statics,
                &exceptions,
                method,
                receiver,
                &[Value::Reference(thrown)],
                (
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
            )
            .unwrap(),
        caught_int(7)
    );
    assert_eq!(
        fixture.inspect(&statics, "p/T", "value", "I"),
        Value::Int(7)
    );
    let method = fixture.method("p/T", "uncaught", "(Ljava/lang/Throwable;)V");
    assert_eq!(
        fixture
            .both(
                &statics,
                &exceptions,
                method,
                receiver,
                &[Value::Reference(thrown)],
                (
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
            )
            .unwrap(),
        EO::Uncaught(thrown)
    );
    assert_eq!(
        fixture.inspect(&statics, "p/T", "value", "I"),
        Value::Int(9)
    );
    assert_eq!(fixture.heap.allocated_instances(), 2);
    assert_eq!(fixture.heap.allocated_arrays(), 0);
}
#[test]
fn composed_static_java_failures_are_unsupported_before_any_catchall_search() {
    for failure in 0..3 {
        let mut root = ordinary();
        root.field("value", "I", 9);
        root.field("finalValue", "I", 0x19);
        root.field("instance", "I", 1);
        let value = root.reference(9, "p/T", "value", "I");
        let bad = root.reference(
            9,
            "p/T",
            if failure == 0 {
                "missing"
            } else if failure == 1 {
                "finalValue"
            } else {
                "instance"
            },
            "I",
        );
        let mut code = vec![0x10, 7];
        code.extend(op(0xb3, value));
        code.push(0x03);
        code.extend(op(0xb3, bad));
        let end = code.len() as u16;
        code.push(0xb1);
        let handler = code.len() as u16;
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 1, 1);
        handlers(&mut root, &[[0, end, handler, 0]]);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let statics = fixture.statics();
        let exceptions = fixture.profile();
        let method = fixture.method("p/T", "run", "()V");
        let error = fixture
            .both(
                &statics,
                &exceptions,
                method,
                receiver,
                &[],
                (
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits {
                        max_dispatch_steps: 0,
                        ..ExceptionLimits::default()
                    },
                ),
            )
            .unwrap_err();
        assert_eq!(
            error.kind,
            E::UnsupportedVmException(match failure {
                0 => VmExceptionKind::NoSuchFieldError,
                1 => VmExceptionKind::IllegalAccessError,
                _ => VmExceptionKind::IncompatibleClassChangeError,
            })
        );
        assert_eq!(error.offset, Some(6));
        assert_eq!(
            fixture.inspect(&statics, "p/T", "value", "I"),
            Value::Int(7)
        );
    }
}
#[test]
fn incompatible_static_references_keep_previous_value_and_root_without_java_outcome() {
    let mut root = ordinary();
    root.field("value", "Lq/BaseThrow;", 9);
    let field = root.reference(9, "p/T", "value", "Lq/BaseThrow;");
    let mut code = vec![0x2b];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    root.method("run", "(Ljava/lang/Object;)V", 1, &code, 1, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let child = fixture.object("q/ChildThrow");
    let statics = fixture.statics();
    let method = fixture.method("p/T", "run", "(Ljava/lang/Object;)V");
    assert_eq!(
        fixture
            .s_run(&statics, method, receiver, &[Value::Reference(child)])
            .unwrap(),
        O::Returned(None)
    );
    let error = fixture
        .s_run(&statics, method, receiver, &[Value::Reference(receiver)])
        .unwrap_err();
    assert!(
        matches!(error.kind,E::StaticAccess(error) if matches!(error.as_ref(),StaticAccessError::IncompatibleReference(_)))
    );
    assert_eq!(
        fixture.inspect(&statics, "p/T", "value", "Lq/BaseThrow;"),
        Value::Reference(child)
    );
    let mut resolver = SameLoaderResolver;
    let context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        context.static_roots(&statics).unwrap().collect::<Vec<_>>(),
        vec![child.heap_ref().unwrap()]
    );
}
#[test]
fn eager_static_profile_then_exception_profile_checks_precede_the_root_frame_gate() {
    let mut root = ordinary();
    root.method("run", "()V", 1, &[0xb1], 0, 1);
    let mut first = Fixture::new(vec![root]);
    let first_static = first.statics();
    let first_exceptions = first.profile();
    let method = first.method("p/T", "run", "()V");
    let receiver = first.object("p/T");
    let mut other = Fixture::new(vec![ordinary()]);
    let foreign_static = other.statics();
    let foreign_exceptions = other.profile();
    let no_frames = CallLimits {
        max_frames: 0,
        ..CallLimits::default()
    };
    assert!(
        matches!(first.both(&foreign_static,&foreign_exceptions,method,receiver,&[],(ExecutionOptions::default(),no_frames,ExceptionLimits::default())).unwrap_err().kind,E::StaticAccess(error) if matches!(error.as_ref(),StaticAccessError::ProfileMismatch))
    );
    assert!(
        matches!(first.both(&first_static,&foreign_exceptions,method,receiver,&[],(ExecutionOptions::default(),no_frames,ExceptionLimits::default())).unwrap_err().kind,E::ExceptionAccess(error) if matches!(error.as_ref(),ExceptionAccessError::ProfileMismatch))
    );
    assert_eq!(
        first
            .both(
                &first_static,
                &first_exceptions,
                method,
                receiver,
                &[],
                (
                    ExecutionOptions::default(),
                    no_frames,
                    ExceptionLimits::default()
                )
            )
            .unwrap_err()
            .kind,
        E::FrameLimit {
            limit: 0,
            required: 1
        }
    );
}

#[test]
fn retrieved_static_references_feed_instance_fields_casts_virtual_calls_and_nested_arrays() {
    let mut root = ordinary();
    root.field("reference", "Ljava/lang/Object;", 9);
    root.field("instance", "I", 1);
    let reference = root.reference(9, "p/T", "reference", "Ljava/lang/Object;");
    let instance = root.reference(9, "p/T", "instance", "I");
    let target = root.class("p/T");
    let value = root.reference(10, "p/T", "value", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb3, reference));
    code.extend(op(0xb2, reference));
    code.extend(op(0xc0, target));
    code.extend([0x59, 0x10, 9]);
    code.extend(op(0xb5, instance));
    code.extend(op(0xb6, value));
    code.push(0xac);
    root.method("object", "()I", 1, &code, 3, 1);
    let mut code = vec![0x2a];
    code.extend(op(0xb4, instance));
    code.push(0xac);
    root.method("value", "()I", 1, &code, 1, 1);
    let array = root.class("[[I");
    let mut code = vec![0x05, 0x06];
    code.extend(op(0xc5, array));
    code.push(2);
    code.extend(op(0xb3, reference));
    code.extend(op(0xb2, reference));
    code.extend(op(0xc0, array));
    code.extend([0x04, 0x32, 0x05, 0x10, 17, 0x4f]);
    code.extend(op(0xb2, reference));
    code.extend(op(0xc0, array));
    code.extend([0x04, 0x32, 0x05, 0x2e, 0xac]);
    root.method("array", "()I", 1, &code, 3, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    for (name, expected) in [("object", 9), ("array", 17)] {
        let method = fixture.method("p/T", name, "()I");
        assert_eq!(
            fixture.s_run(&statics, method, receiver, &[]).unwrap(),
            O::Returned(Some(Value::Int(expected)))
        );
    }
    assert_eq!(
        fixture.stored("p/T", "instance", "I", receiver),
        Value::Int(9)
    );
    let Value::Reference(array) =
        fixture.inspect(&statics, "p/T", "reference", "Ljava/lang/Object;")
    else {
        panic!("static reference lost")
    };
    assert_eq!(fixture.heap.length(array).unwrap(), 2);
    let last = fixture.heap.load_reference(array, 1).unwrap();
    assert_eq!(fixture.heap.load(last, 2).unwrap(), Value::Int(17));
    assert_eq!(fixture.heap.allocated_arrays(), 3);
}
#[test]
fn static_defaults_ignore_clinit_and_every_final_write_remains_rejected() {
    let mut root = ordinary();
    root.field("value", "I", 9);
    root.field("finalValue", "I", 0x19);
    let value = root.reference(9, "p/T", "value", "I");
    let final_value = root.reference(9, "p/T", "finalValue", "I");
    let mut code = vec![0x04];
    code.extend(op(0xb3, value));
    code.push(0xb1);
    root.method("<clinit>", "()V", 8, &code, 1, 0);
    let mut code = op(0xb2, value);
    code.push(0xac);
    root.method("read", "()I", 1, &code, 1, 1);
    let mut code = vec![0x04];
    code.extend(op(0xb3, final_value));
    code.push(0xb1);
    root.method("writeFinal", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    let method = fixture.method("p/T", "read", "()I");
    assert_eq!(
        fixture.s_run(&statics, method, receiver, &[]).unwrap(),
        O::Returned(Some(Value::Int(0)))
    );
    let method = fixture.method("p/T", "writeFinal", "()V");
    assert_eq!(
        fixture.s_run(&statics, method, receiver, &[]).unwrap(),
        O::IllegalAccessError
    );
    assert_eq!(
        fixture.inspect(&statics, "p/T", "finalValue", "I"),
        Value::Int(0)
    );
    assert_eq!(
        fixture.inspect(&statics, "p/T", "value", "I"),
        Value::Int(0)
    );
}
#[test]
fn static_profile_survives_first_universe_binding_and_idempotent_allocation_preserves_values() {
    let mut root = ordinary();
    root.field("reference", "Ljava/lang/Object;", 9);
    let field = root.reference(9, "p/T", "reference", "Ljava/lang/Object;");
    let mut code = op(0xb2, field);
    code.push(0xb0);
    root.method("read", "()Ljava/lang/Object;", 1, &code, 1, 1);
    let mut code = vec![0x2a];
    code.extend(op(0xb3, field));
    code.extend(op(0xb2, field));
    code.push(0xb0);
    root.method("write", "()Ljava/lang/Object;", 1, &code, 1, 1);
    let mut fixture = Fixture::configured(vec![root], false);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    let read = fixture.method("p/T", "read", "()Ljava/lang/Object;");
    assert_eq!(
        fixture.s_run(&statics, read, receiver, &[]).unwrap(),
        O::Returned(Some(Value::Reference(Reference::null())))
    );
    let write = fixture.method("p/T", "write", "()Ljava/lang/Object;");
    assert!(
        matches!(fixture.s_run(&statics,write,receiver,&[]).unwrap_err().kind,E::StaticAccess(error) if matches!(error.as_ref(),StaticAccessError::Type(error) if matches!(error.as_ref(),crabspot_runtime::TypeAccessError::MissingUniverse)))
    );
    let loader = fixture
        .registry
        .defining_loader(fixture.classes["p/T"])
        .unwrap();
    fixture
        .heap
        .bind_reference_types(
            &fixture.registry,
            ReferenceTypeUniverse {
                bootstrap_loader: loader,
                object: fixture.classes["java/lang/Object"],
                cloneable: fixture.classes["java/lang/Cloneable"],
                serializable: fixture.classes["java/io/Serializable"],
            },
        )
        .unwrap();
    assert_eq!(
        fixture.s_run(&statics, write, receiver, &[]).unwrap(),
        O::Returned(Some(Value::Reference(receiver)))
    );
    assert_eq!(fixture.statics(), statics);
    assert_eq!(
        fixture.inspect(&statics, "p/T", "reference", "Ljava/lang/Object;"),
        Value::Reference(receiver)
    );
}
#[test]
fn exact_fuel_frame_code_and_category_two_policies_preserve_completed_static_writes() {
    let mut root = ordinary();
    root.field("value", "I", 9);
    let field = root.reference(9, "p/T", "value", "I");
    let mut code = vec![0x10, 7];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    let bytes = code.len();
    root.method("run", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    let method = fixture.method("p/T", "run", "()V");
    let limits = CallLimits {
        max_frames: 1,
        max_frame_slots: 2,
        max_prepared_code_bytes: bytes,
    };
    for (budget, limit, expected) in [
        (0, limits, E::InstructionBudgetExhausted { limit: 0 }),
        (1, limits, E::InstructionBudgetExhausted { limit: 1 }),
        (
            3,
            CallLimits {
                max_frames: 0,
                ..limits
            },
            E::FrameLimit {
                limit: 0,
                required: 1,
            },
        ),
        (
            3,
            CallLimits {
                max_frame_slots: 1,
                ..limits
            },
            E::FrameSlotLimit {
                limit: 1,
                required: 2,
            },
        ),
        (
            3,
            CallLimits {
                max_prepared_code_bytes: bytes - 1,
                ..limits
            },
            E::PreparedCodeLimit {
                limit: bytes - 1,
                required: bytes,
            },
        ),
    ] {
        assert_eq!(
            fixture
                .s_limits(
                    &statics,
                    method,
                    receiver,
                    &[],
                    (
                        ExecutionOptions {
                            instruction_budget: budget
                        },
                        limit
                    )
                )
                .unwrap_err()
                .kind,
            expected
        );
        assert_eq!(
            fixture.inspect(&statics, "p/T", "value", "I"),
            Value::Int(0)
        );
    }
    assert_eq!(
        fixture
            .s_limits(
                &statics,
                method,
                receiver,
                &[],
                (
                    ExecutionOptions {
                        instruction_budget: 2
                    },
                    limits
                )
            )
            .unwrap_err()
            .kind,
        E::InstructionBudgetExhausted { limit: 2 }
    );
    assert_eq!(
        fixture.inspect(&statics, "p/T", "value", "I"),
        Value::Int(7)
    );
    assert_eq!(
        fixture
            .s_limits(
                &statics,
                method,
                receiver,
                &[],
                (
                    ExecutionOptions {
                        instruction_budget: 3
                    },
                    limits
                )
            )
            .unwrap(),
        O::Returned(None)
    );
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("wide", "J", 9);
    let field = base.reference(9, "q/Base", "wide", "J");
    let mut code = op(0xb2, field);
    code.push(0xad);
    base.method("body", "()J", 1, &code, 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "body", "()J");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, call));
    code.push(0xad);
    root.method("run", "()J", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    let method = fixture.method("p/T", "run", "()J");
    let error = fixture.s_run(&statics, method, receiver, &[]).unwrap_err();
    assert_eq!(
        error.kind,
        E::StackOverflow {
            required: 2,
            capacity: 1
        }
    );
    assert_eq!(error.offset, Some(0));
    assert_eq!(
        error.method.as_ref().unwrap().owner,
        Some(fixture.classes["q/Base"])
    );
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(fixture.classes["p/T"])
    );
    assert_eq!(error.call_site.as_ref().unwrap().offset, 1);
}
#[test]
fn earlier_static_changes_survive_symbolic_java_errors_and_later_host_failures() {
    for failure in 0..9 {
        let mut root = ordinary();
        root.field("value", "I", 9);
        root.field("finalValue", "I", 0x19);
        root.field("instance", "I", 1);
        let value = root.reference(9, "p/T", "value", "I");
        let missing = root.reference(9, "p/T", "missing", "I");
        let final_value = root.reference(9, "p/T", "finalValue", "I");
        let instance = root.reference(9, "p/T", "instance", "I");
        let array = root.class("[I");
        let mut code = vec![0x10, 7];
        code.extend(op(0xb3, value));
        let expected = match failure {
            0 => {
                code.extend([0x04, 0x03, 0x6c, 0x57, 0xb1]);
                Some(O::ArithmeticException)
            }
            1 => {
                code.extend([0x01, 0xbe, 0x57, 0xb1]);
                Some(O::NullPointerException)
            }
            2 => {
                code.extend([0x03, 0xbc, 10, 0x03, 0x2e, 0x57, 0xb1]);
                Some(O::ArrayIndexOutOfBoundsException)
            }
            3 => {
                code.extend(op(0xb2, missing));
                code.extend([0x57, 0xb1]);
                Some(O::NoSuchFieldError)
            }
            4 => {
                code.push(0x03);
                code.extend(op(0xb3, final_value));
                code.push(0xb1);
                Some(O::IllegalAccessError)
            }
            5 => {
                code.extend(op(0xb2, instance));
                code.extend([0x57, 0xb1]);
                Some(O::IncompatibleClassChangeError)
            }
            6 => {
                code.push(0x01);
                code.extend(op(0xb3, value));
                code.push(0xb1);
                None
            }
            7 => {
                code.push(0x2a);
                code.extend(op(0xc0, array));
                code.extend([0x57, 0xb1]);
                Some(O::ClassCastException)
            }
            _ => {
                code.extend([0xc2, 0xb1]);
                None
            }
        };
        root.method("run", "()V", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let statics = fixture.statics();
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.s_run(&statics, method, receiver, &[]);
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(
            fixture.inspect(&statics, "p/T", "value", "I"),
            Value::Int(7)
        );
        assert_eq!(
            fixture.inspect(&statics, "p/T", "finalValue", "I"),
            Value::Int(0)
        );
        assert_eq!(
            fixture.heap.allocated_arrays(),
            if failure == 2 { 1 } else { 0 }
        );
    }
}
#[test]
fn late_static_selector_work_failure_keeps_provenance_and_all_prior_values() {
    let mut root = ordinary();
    root.field("value", "I", 9);
    let field = root.reference(9, "p/T", "value", "I");
    let mut code = vec![0x10, 7];
    code.extend(op(0xb3, field));
    code.extend(op(0xb2, field));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    let method = fixture.method("p/T", "run", "()I");
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        execute_instance_method_body_with_static_state(
            &mut context,
            &statics,
            method,
            receiver,
            &[],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        O::Returned(Some(Value::Int(7)))
    );
    let exact = context.lookup_steps();
    drop(context);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: exact - 1,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    let error = execute_instance_method_body_with_static_state(
        &mut context,
        &statics,
        method,
        receiver,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.offset, Some(5));
    assert_eq!(error.method.as_ref().unwrap().owner, Some(method.owner()));
    assert!(
        matches!(error.kind,E::StaticAccess(error) if matches!(error.as_ref(),StaticAccessError::Context(FieldAccessError::ResourceLimit{..})))
    );
    drop(context);
    assert_eq!(
        fixture.inspect(&statics, "p/T", "value", "I"),
        Value::Int(7)
    );
}

#[test]
fn fresh_context_extension_keeps_values_but_old_static_and_exception_profiles_stay_rejected() {
    let mut root = ordinary();
    root.field("value", "I", 9);
    let field = root.reference(9, "p/T", "value", "I");
    let mut code = vec![0x10, 7];
    code.extend(op(0xb3, field));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let statics = fixture.statics();
    let exceptions = fixture.profile();
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.s_run(&statics, method, receiver, &[]).unwrap(),
        O::Returned(None)
    );
    let loader = fixture.registry.defining_loader(method.owner()).unwrap();
    let mut extra = Class::new("q/Extra", Some("java/lang/Object"), 0x21);
    extra.field("later", "J", 9);
    let added = fixture
        .registry
        .define(
            loader,
            &units("q/Extra"),
            &extra.bytes(),
            ParseOptions::default(),
        )
        .unwrap();
    let plan = build_hierarchy(
        &fixture.registry,
        added,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    fixture
        .heap
        .admit_instance_layouts(&fixture.registry, &[&plan])
        .unwrap();
    assert!(
        matches!(fixture.s_run(&statics,method,receiver,&[]).unwrap_err().kind,E::StaticAccess(error) if matches!(error.as_ref(),StaticAccessError::ProfileMismatch))
    );
    let fresh = fixture.statics();
    assert_eq!(fresh.slot_count(), statics.slot_count() + 1);
    assert_eq!(fixture.inspect(&fresh, "p/T", "value", "I"), Value::Int(7));
    assert!(
        matches!(fixture.both(&fresh,&exceptions,method,receiver,&[],(ExecutionOptions::default(),CallLimits::default(),ExceptionLimits::default())).unwrap_err().kind,E::ExceptionAccess(error) if matches!(error.as_ref(),ExceptionAccessError::ProfileMismatch))
    );
    let exceptions = fixture.profile();
    assert_eq!(
        fixture
            .both(
                &fresh,
                &exceptions,
                method,
                receiver,
                &[],
                (
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits::default()
                )
            )
            .unwrap(),
        EO::Returned(None)
    );
}

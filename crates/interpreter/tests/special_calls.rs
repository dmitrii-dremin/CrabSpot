//! Ordinary nonconstructor special calls through the shared execution engine.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_instance_method_body, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext, FieldResolutionOptions,
    HeapError, HeapOptions, HierarchyOptions, InstanceError, InstanceOptions, ManagedHeap,
    MethodAccessError, MethodId, Reference, RegistryOptions, SameLoaderResolver, build_hierarchy,
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
    handler: bool,
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
            handler: false,
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
                    12 + code.len() as u32 + if method.handler { 8 } else { 0 },
                );
                u2(&mut bytes, method.stack);
                u2(&mut bytes, method.locals);
                u4(&mut bytes, code.len() as u32);
                bytes.extend(code);
                u2(&mut bytes, u16::from(method.handler));
                if method.handler {
                    for word in [0, 1, 0, 0] {
                        u2(&mut bytes, word);
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
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
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
fn returned(value: Value) -> O {
    O::Returned(Some(value))
}

fn iface(name: &str) -> Class {
    Class::new(name, Some("java/lang/Object"), 0x601)
}
fn implement(class: &mut Class, name: &str) {
    let index = class.class(name);
    class.interfaces.push(index);
}
fn interface_call(index: u16) -> Vec<u8> {
    let mut code = op(0xb9, index);
    code.extend([1, 0]);
    code
}
#[test]
fn old_and_current_acc_super_clear_files_start_at_direct_parent_not_named_ancestor_or_receiver() {
    for major in [45, 51, 65] {
        let mut grand = Class::new("q/Grand", Some("java/lang/Object"), 0x21);
        grand.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut parent = Class::new("q/Parent", Some("q/Grand"), 0x21);
        parent.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
        let mut root = Class::new("p/T", Some("q/Parent"), 1);
        root.major = major;
        let call = root.reference(10, "q/Grand", "v", "()I");
        let mut code = vec![0x2a];
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        root.method("v", "()I", 1, &[0x06, 0xac], 1, 1);
        let mut leaf = Class::new("p/Leaf", Some("p/T"), 0x21);
        leaf.method("v", "()I", 1, &[0x07, 0xac], 1, 1);
        let mut fixture = Fixture::new(vec![grand, parent, root, leaf]);
        let receiver = fixture.object("p/Leaf");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.registry.class(method.owner()).unwrap().access_flags & 0x20,
            0
        );
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(2)),
            "major {major}"
        );
    }
}
#[test]
fn private_special_body_keeps_hidden_fields_and_subsequent_virtual_and_static_origins() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("f", "I", 2);
    let private = base.reference(10, "q/Base", "privateBody", "()I");
    let virtual_call = base.reference(10, "q/Base", "v", "()I");
    let helper = base.reference(10, "q/Base", "helper", "()I");
    let field = base.reference(9, "q/Base", "f", "I");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, private));
    code.push(0xac);
    base.method("run", "()I", 1, &code, 1, 1);
    let mut code = vec![0x2a, 0x10, 17];
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb6, virtual_call));
    code.extend(op(0xb8, helper));
    code.extend([0x60, 0xac]);
    base.method("privateBody", "()I", 2, &code, 2, 1);
    base.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
    base.method("helper", "()I", 9, &[0x10, 7, 0xac], 1, 0);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    root.field("f", "I", 2);
    root.method("privateBody", "()I", 1, &[0x10, 99, 0xac], 1, 1);
    root.method("v", "()I", 1, &[0x08, 0xac], 1, 1);
    root.method("helper", "()I", 9, &[0x10, 100, 0xac], 1, 0);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("q/Base", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(12))
    );
    assert_eq!(fixture.stored("q/Base", "f", "I", receiver), Value::Int(17));
    assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(0));
}
#[test]
fn direct_interface_and_class_super_defaults_ignore_actual_receiver_overrides_and_other_defaults() {
    for class_owner in [false, true] {
        let mut top = iface("q/Top");
        top.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut direct = iface("q/Direct");
        implement(&mut direct, "q/Top");
        let mut other = iface("q/Other");
        other.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        implement(&mut base, "q/Top");
        let mut root = Class::new(
            "p/T",
            Some(if class_owner {
                "q/Base"
            } else {
                "java/lang/Object"
            }),
            0x21,
        );
        if !class_owner {
            implement(&mut root, "q/Direct");
        }
        implement(&mut root, "q/Other");
        let call = root.reference(
            if class_owner { 10 } else { 11 },
            if class_owner { "q/Base" } else { "q/Direct" },
            "v",
            "()I",
        );
        let mut code = vec![0x2a];
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        root.method("v", "()I", 1, &[0x06, 0xac], 1, 1);
        let mut fixture = Fixture::new(vec![top, direct, other, base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(1))
        );
    }
}
#[test]
fn special_interface_private_and_interface_calls_share_one_method_cache_and_keep_root_guard() {
    let mut interface = iface("q/I");
    let helper = interface.reference(11, "q/I", "helper", "()I");
    let mut code = vec![0x2a];
    code.extend(interface_call(helper));
    code.push(0x2a);
    code.extend(op(0xb7, helper));
    code.extend([0x60, 0xac]);
    let interface_bytes = code.len();
    interface.method("v", "()I", 1, &code, 2, 1);
    interface.method("helper", "()I", 2, &[0x10, 13, 0xac], 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(interface_call(call));
    code.push(0xac);
    let code_bytes = code.len() + interface_bytes + 3;
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: 13
                },
                CallLimits {
                    max_frames: 3,
                    max_frame_slots: 7,
                    max_prepared_code_bytes: code_bytes
                }
            )
            .unwrap(),
        returned(Value::Int(26))
    );
    let private = fixture.method("q/I", "helper", "()I");
    assert_eq!(
        fixture.run(private, receiver, &[]).unwrap_err().kind,
        E::UnsupportedMethod("interface instance bodies are unsupported")
    );
}
#[test]
fn special_and_virtual_paths_share_exact_prepared_code_frames_slots_and_instruction_fuel() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("v", "()I", 1, &[0x10, 7, 0xac], 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, call));
    code.push(0x2a);
    code.extend(op(0xb6, call));
    code.extend([0x60, 0xac]);
    let code_bytes = code.len() + 3;
    root.method("run", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    let limits = CallLimits {
        max_frames: 2,
        max_frame_slots: 5,
        max_prepared_code_bytes: code_bytes,
    };
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: 10
                },
                limits
            )
            .unwrap(),
        returned(Value::Int(14))
    );
    for (limits, budget, expected) in [
        (
            CallLimits {
                max_frames: 1,
                ..limits
            },
            10,
            E::FrameLimit {
                limit: 1,
                required: 2,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 4,
                ..limits
            },
            10,
            E::FrameSlotLimit {
                limit: 4,
                required: 5,
            },
        ),
        (
            CallLimits {
                max_prepared_code_bytes: code_bytes - 1,
                ..limits
            },
            10,
            E::PreparedCodeLimit {
                limit: code_bytes - 1,
                required: code_bytes,
            },
        ),
        (limits, 9, E::InstructionBudgetExhausted { limit: 9 }),
    ] {
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[],
                    ExecutionOptions {
                        instruction_budget: budget
                    },
                    limits
                )
                .unwrap_err()
                .kind,
            expected
        );
    }
}
#[test]
fn special_mixed_width_numeric_returns_and_narrowing_keep_original_argument_layout() {
    for (result, load, ret, expected) in [
        ("J", vec![0x1f], 0xad, Value::Long(42)),
        ("D", vec![0x29], 0xaf, Value::Double(0x8000000000000000)),
        ("F", vec![0x17, 5], 0xae, Value::Float(0x7fc12345)),
        ("I", vec![0x15, 6], 0xac, Value::Int(-7)),
    ] {
        let descriptor = format!("(JDFI){result}");
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        let mut code = load;
        code.push(ret);
        base.method("v", &descriptor, 1, &code, 2, 7);
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        let call = root.reference(10, "q/Base", "v", &descriptor);
        let mut code = vec![0x2a, 0x1f, 0x29, 0x17, 5, 0x15, 6];
        code.extend(op(0xb7, call));
        code.push(ret);
        root.method("run", &descriptor, 1, &code, 7, 7);
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", &descriptor);
        assert_eq!(
            fixture
                .run(
                    method,
                    receiver,
                    &[
                        Value::Long(42),
                        Value::Double(0x8000000000000000),
                        Value::Float(0x7fc12345),
                        Value::Int(-7)
                    ]
                )
                .unwrap(),
            returned(expected)
        );
    }
    for (result, value, expected) in [
        ("Z", 3, 1),
        ("B", 128, -128),
        ("C", -1, 65535),
        ("S", 32768, -32768),
    ] {
        let descriptor = format!("(I){result}");
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        base.method("v", &descriptor, 1, &[0x1b, 0xac], 1, 2);
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        let call = root.reference(10, "q/Base", "v", &descriptor);
        let mut code = vec![0x2a, 0x1b];
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "(I)I", 1, &code, 2, 2);
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "(I)I");
        assert_eq!(
            fixture.run(method, receiver, &[Value::Int(value)]).unwrap(),
            returned(Value::Int(expected))
        );
    }
}
#[test]
fn special_void_return_preserves_caller_operands_and_public_field_effects() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("f", "I", 1);
    let field = base.reference(9, "q/Base", "f", "I");
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    base.method("v", "()V", 1, &code, 2, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "v", "()V");
    let mut code = vec![0x10, 42, 0x2a];
    code.extend(op(0xb7, call));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(42))
    );
    assert_eq!(fixture.stored("q/Base", "f", "I", receiver), Value::Int(7));
}
#[test]
fn constructor_names_and_invalid_cp_tags_precede_typed_reference_extraction() {
    for (tag, name, descriptor, expected_special) in
        [(10, "<init>", "()V", true), (10, "v", "([I)I", false)]
    {
        let mut root = ordinary();
        let call = root.reference(tag, "java/lang/Object", name, descriptor);
        let mut code = op(0xb7, call);
        code.push(0xb1);
        root.method("run", "()V", 1, &code, 0, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let before = fixture.heap.allocated_instances();
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert_eq!(
            error.kind,
            if expected_special {
                E::MethodAccess(Box::new(MethodAccessError::UnsupportedSpecialMethod))
            } else {
                E::StackUnderflow
            }
        );
        assert_eq!(fixture.heap.allocated_instances(), before);
    }
    let mut root = ordinary();
    let index = root.class("p/T");
    let mut code = op(0xb7, index);
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 0, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::InvalidConstant {..}))
    );
}
#[test]
fn interface_methodref_special_calls_enforce_the_51_52_version_boundary() {
    for major in [51, 52, 65] {
        let mut interface = iface("q/I");
        interface.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut root = ordinary();
        root.major = major;
        implement(&mut root, "q/I");
        let call = root.reference(11, "q/I", "v", "()I");
        let mut code = if major < 52 { Vec::new() } else { vec![0x2a] };
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.run(method, receiver, &[]);
        if major < 52 {
            assert!(
                matches!(result.unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::InvalidConstant {..}))
            );
        } else {
            assert_eq!(result.unwrap(), returned(Value::Int(1)));
        }
    }
}
#[test]
fn old_entry_points_keep_b7_disabled_and_runtime_stack_or_encoding_errors_stay_structured() {
    let mut root = ordinary();
    root.method("staticBody", "()V", 9, &[0xb7, 0, 1, 0xb1], 0, 0);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.registry.class(fixture.classes["p/T"]).unwrap();
    for error in [
        execute_static_method_body(class, 0, &[], ExecutionOptions::default()).unwrap_err(),
        execute_static_method_body_with_heap(
            class,
            0,
            &[],
            ExecutionOptions::default(),
            CallLimits::default(),
            &mut fixture.heap,
        )
        .unwrap_err(),
    ] {
        assert_eq!(error.kind, E::UnsupportedInstruction(0xb7));
        assert_eq!(error.offset, Some(0));
        assert_eq!(error.method.unwrap().owner, None);
    }
    for prefix in [vec![], vec![0x03]] {
        let mut root = ordinary();
        let call = root.reference(10, "p/T", "v", "()I");
        let mut code = prefix;
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        root.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert!(matches!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::StackUnderflow
                | E::WrongValueType {
                    expected: ValueKind::Reference,
                    ..
                }
        ));
    }
    let mut root = ordinary();
    root.method("run", "()V", 1, &[0xb1, 0xb7, 0], 0, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert!(matches!(
        fixture.run(method, receiver, &[]).unwrap_err().kind,
        E::Decode(_)
    ));
}

#[test]
fn interface_object_public_control_and_protected_access_keep_class_only_subtyping() {
    for protected in [false, true] {
        let mut object = Class::new("java/lang/Object", None, 0x21);
        object.method(
            "ping",
            "()I",
            if protected { 4 } else { 1 },
            &[0x10, 17, 0xac],
            1,
            1,
        );
        let mut interface = iface("q/I");
        let call = interface.reference(10, "java/lang/Object", "ping", "()I");
        let mut code = vec![if protected { 0x01 } else { 0x2a }];
        code.extend(op(0xb7, call));
        code.push(0xac);
        interface.method("v", "()I", 1, &code, 1, 1);
        let mut root = ordinary();
        implement(&mut root, "q/I");
        let call = root.reference(11, "q/I", "v", "()I");
        let mut code = vec![0x2a];
        code.extend(interface_call(call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![object, interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            if protected {
                O::IllegalAccessError
            } else {
                returned(Value::Int(17))
            }
        );
    }
}
#[test]
fn indirect_interface_and_sibling_special_owners_are_host_failures_before_null() {
    for (owner, tag, invalid_owner) in [
        ("q/I", 11, true),
        ("q/Sibling", 10, true),
        ("q/I", 10, false),
        ("q/Base", 11, false),
    ] {
        let mut interface = iface("q/I");
        interface.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut direct = iface("q/Direct");
        implement(&mut direct, "q/I");
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        base.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut sibling = Class::new("q/Sibling", Some("java/lang/Object"), 0x21);
        sibling.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        implement(&mut root, "q/Direct");
        let call = root.reference(tag, owner, "v", "()I");
        let mut code = vec![0x01];
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![interface, direct, base, sibling, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.run(method, receiver, &[]);
        if invalid_owner {
            assert_eq!(
                result.unwrap_err().kind,
                E::MethodAccess(Box::new(MethodAccessError::InvalidSpecialOwner {
                    origin: fixture.classes["p/T"],
                    owner: fixture.classes[owner]
                }))
            );
        } else {
            assert_eq!(result.unwrap(), O::IncompatibleClassChangeError);
        }
    }
}
#[test]
fn private_selection_candidates_are_not_virtual_filtered_and_static_candidates_are_skipped() {
    for flags in [2, 9] {
        let mut grand = Class::new("q/Grand", Some("java/lang/Object"), 0x21);
        grand.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut parent = Class::new("q/Parent", Some("q/Grand"), 0x21);
        parent.method("v", "()I", flags, &[0x05, 0xac], 1, 1);
        let mut root = Class::new("p/T", Some("q/Parent"), 0x21);
        let call = root.reference(10, "q/Grand", "v", "()I");
        let mut code = vec![0x2a];
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![grand, parent, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(if flags == 2 { 2 } else { 1 }))
        );
    }
}
#[test]
fn special_and_static_hits_never_bypass_cached_invocation_modes() {
    for static_target in [false, true] {
        let mut root = ordinary();
        let call = root.reference(10, "p/T", "v", "()I");
        let mut code = if static_target {
            op(0xb8, call)
        } else {
            let mut c = vec![0x2a];
            c.extend(op(0xb7, call));
            c
        };
        code.push(0x57);
        if static_target {
            code.push(0x2a);
            code.extend(op(0xb7, call));
        } else {
            code.extend(op(0xb8, call));
        }
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        root.method(
            "v",
            "()I",
            if static_target { 9 } else { 2 },
            &[0x04, 0xac],
            1,
            1,
        );
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.run(method, receiver, &[]);
        if static_target {
            assert_eq!(result.unwrap(), O::IncompatibleClassChangeError);
        } else {
            assert_eq!(
                result.unwrap_err().kind,
                E::UnsupportedMethod("method must be static")
            );
        }
    }
}
#[test]
fn selected_special_callee_decode_and_return_transfer_failures_keep_transition_site_owners() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("v", "()V", 1, &[0xb1, 0xff], 0, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "v", "()V");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, call));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert!(matches!(error.kind, E::Decode(_)));
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.method.unwrap().owner, Some(fixture.classes["q/Base"]));
    assert_eq!(
        error.call_site.unwrap().method.owner,
        Some(fixture.classes["p/T"])
    );
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("v", "()J", 1, &[0x09, 0xad], 2, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "v", "()J");
    let mut code = vec![0x09, 0x2a];
    code.extend(op(0xb7, call));
    code.push(0xad);
    root.method("run", "()J", 1, &code, 3, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()J");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(
        error.kind,
        E::StackOverflow {
            required: 4,
            capacity: 3
        }
    );
    assert_eq!(error.offset, Some(2));
    assert_eq!(error.method.unwrap().owner, Some(method.owner()));
    assert_eq!(error.call_site.unwrap().method.owner, Some(method.owner()));
}
#[test]
fn deep_private_special_recursion_reuses_one_body_without_rust_recursion() {
    let mut root = ordinary();
    let call = root.reference(10, "p/T", "v", "(I)I");
    let mut entry = vec![0x2a, 0x1b];
    entry.extend(op(0xb7, call));
    entry.push(0xac);
    root.method("run", "(I)I", 1, &entry, 2, 2);
    let mut body = vec![0x1b, 0x99, 0, 13, 0x2a, 0x1b, 0x04, 0x64];
    body.extend(op(0xb7, call));
    body.extend([0x04, 0x60, 0xac, 0x03, 0xac]);
    let code_bytes = entry.len() + body.len();
    root.method("v", "(I)I", 2, &body, 3, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "(I)I");
    let count = 10_000;
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[Value::Int(count)],
                ExecutionOptions {
                    instruction_budget: 10 * count as u64 + 8
                },
                CallLimits {
                    max_frames: usize::MAX,
                    max_frame_slots: usize::MAX,
                    max_prepared_code_bytes: code_bytes
                }
            )
            .unwrap(),
        returned(Value::Int(count))
    );
}
#[test]
fn special_selection_keeps_cumulative_work_budget_on_a_prepared_target_hit() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("f", "I", 1);
    let field = base.reference(9, "q/Base", "f", "I");
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, field));
    code.extend([0x04, 0xac]);
    base.method("v", "()I", 1, &code, 2, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, call));
    code.extend([0x57, 0x2a]);
    code.extend(op(0xb7, call));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
    let base = fixture.classes["q/Base"];
    let method = fixture.method("p/T", "run", "()I");
    let mut resolver = SameLoaderResolver;
    let budget = {
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &fixture.registry,
            &mut fixture.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        context.check_receiver(owner, receiver).unwrap();
        context.select_special(owner, call, receiver).unwrap();
        context
            .write_field(base, field, receiver, Value::Int(0))
            .unwrap();
        context.lookup_steps()
    };
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: budget,
            ..Default::default()
        },
    )
    .unwrap();
    let error = execute_instance_method_body(
        &mut context,
        method,
        receiver,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
    )
    .unwrap_err();
    assert!(
        matches!(error.kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::Context(FieldAccessError::ResourceLimit {resource:crabspot_runtime::FieldResource::LookupSteps,..})))
    );
    let field = fixture
        .registry
        .find_declared_field(base, &units("f"), &units("I"))
        .unwrap()
        .unwrap();
    assert_eq!(
        context.heap().get_field(receiver, field).unwrap(),
        Value::Int(7)
    );
}
#[test]
fn java_and_host_failures_after_special_resolution_preserve_completed_heap_effects() {
    for failure in 0..15 {
        let flags = match failure {
            1 => 9,
            2 => 0,
            3 | 4 => 0x401,
            5 => 0x101,
            6 => 0x21,
            11 => 2,
            _ => 1,
        };
        let mut base = Class::new(
            "q/Base",
            Some("java/lang/Object"),
            if flags & 0x400 != 0 { 0x421 } else { 0x21 },
        );
        base.method("v", "()I", flags, &[0x04, 0xac], 1, 1);
        if failure == 7 {
            base.methods[0].handler = true;
        }
        if failure == 8 {
            base.methods[0].code = Some(vec![0xb1, 0xff]);
        }
        if failure == 12 {
            base.methods[0].code = Some(vec![0xc2, 0x03, 0xac]);
        }
        if failure == 13 {
            base.methods[0].code = Some(vec![0x04, 0x03, 0x6c, 0xac]);
            base.methods[0].stack = 2;
        }
        if failure == 14 {
            base.methods[0].code = Some(vec![0xb1]);
        }
        let mut sibling = Class::new("q/Sibling", Some("java/lang/Object"), 0x21);
        sibling.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        root.field("f", "I", 1);
        let field = root.reference(9, "p/T", "f", "I");
        let call = root.reference(
            10,
            if failure == 10 { "q/Sibling" } else { "q/Base" },
            if failure == 0 { "missing" } else { "v" },
            "()I",
        );
        let mut code = vec![0x2a, 0x10, 7];
        code.extend(op(0xb5, field));
        code.extend([
            0x03,
            0xbc,
            10,
            0x57,
            if matches!(failure, 0 | 1 | 2 | 4 | 9 | 10 | 11) {
                0x01
            } else {
                0x2a
            },
        ]);
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![base, sibling, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap(), O::NoSuchMethodError),
            1 => assert_eq!(result.unwrap(), O::IncompatibleClassChangeError),
            2 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            3 => assert_eq!(result.unwrap(), O::AbstractMethodError),
            4 | 9 => assert_eq!(result.unwrap(), O::NullPointerException),
            10 => assert!(
                matches!(result.unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::InvalidSpecialOwner {..}))
            ),
            11 => assert!(
                matches!(result.unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::UnsupportedNestAccess {..}))
            ),
            13 => assert_eq!(result.unwrap(), O::ArithmeticException),
            _ => assert!(result.is_err(), "case {failure}"),
        };
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(7));
    }
}
#[test]
fn special_conflicts_are_computed_from_start_type_after_null_and_not_receiver_overrides() {
    for null in [false, true] {
        let mut a = iface("q/A");
        a.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut b = iface("q/B");
        b.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        implement(&mut base, "q/A");
        implement(&mut base, "q/B");
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        let call = root.reference(10, "q/Base", "v", "()I");
        let mut code = vec![if null { 0x01 } else { 0x2a }];
        code.extend(op(0xb7, call));
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        root.method("v", "()I", 1, &[0x06, 0xac], 1, 1);
        let mut fixture = Fixture::new(vec![a, b, base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            if null {
                O::NullPointerException
            } else {
                O::IncompatibleClassChangeError
            }
        );
    }
}
#[test]
fn array_receivers_and_wrong_numeric_arguments_remain_host_errors() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "v", "()I");
    let mut code = vec![0x03, 0xbc, 10];
    code.extend(op(0xb7, call));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::Context(FieldAccessError::Instance(InstanceError::Heap(HeapError::WrongEntryKind {..})))))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    let mut root = ordinary();
    let call = root.reference(10, "p/T", "v", "(I)I");
    let mut code = vec![0x2a, 0x09];
    code.extend(op(0xb7, call));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 3, 1);
    root.method("v", "(I)I", 2, &[0x1b, 0xac], 1, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(
        error.kind,
        E::WrongValueType {
            expected: ValueKind::Int,
            actual: ValueKind::Long
        }
    );
    assert_eq!(error.offset, Some(2));
}

//! Synthetic interface/default calls through the shared execution engine.
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
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
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
fn invoke(index: u16, count: u8) -> Vec<u8> {
    let mut bytes = op(0xb9, index);
    bytes.extend([count, 0]);
    bytes
}
#[test]
fn virtual_fallback_and_interface_call_share_one_default_preparation_and_exact_limits() {
    let mut interface = iface("q/I");
    interface.method("v", "()I", 1, &[0x08, 0xac], 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let virtual_call = root.reference(10, "p/T", "v", "()I");
    let interface_call = root.reference(11, "q/I", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, virtual_call));
    code.push(0x2a);
    code.extend(invoke(interface_call, 1));
    code.extend([0x60, 0xac]);
    let code_bytes = code.len() + 2;
    root.method("m", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
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
        returned(Value::Int(10))
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
fn different_interfaces_with_equal_method_indices_use_their_own_constants_and_call_origins() {
    let mut a = iface("q/A");
    let number = a.cp(3, &42i32.to_be_bytes());
    let mut code = op(0x13, number);
    code.push(0xac);
    a.method("a", "()I", 1, &code, 1, 1);
    let mut b = iface("q/B");
    let number = b.cp(3, &100i32.to_be_bytes());
    let mut code = op(0x13, number);
    code.push(0xac);
    b.method("b", "()I", 1, &code, 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/A");
    implement(&mut root, "q/B");
    let a_call = root.reference(11, "q/A", "a", "()I");
    let b_call = root.reference(11, "q/B", "b", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(a_call, 1));
    code.push(0x2a);
    code.extend(invoke(b_call, 1));
    code.extend([0x60, 0xac]);
    root.method("m", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![a, b, root]);
    assert_eq!(
        fixture.method("q/A", "a", "()I").index(),
        fixture.method("q/B", "b", "()I").index()
    );
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(142))
    );
}
#[test]
fn interface_default_calls_private_and_static_own_helpers_but_direct_roots_stay_excluded() {
    let mut interface = iface("q/I");
    let private = interface.reference(11, "q/I", "privateHelper", "()I");
    let helper = interface.reference(11, "q/I", "staticHelper", "()I");
    let mut code = op(0xb8, helper);
    code.push(0x2a);
    code.extend(invoke(private, 1));
    code.extend([0x60, 0xac]);
    interface.method("v", "()I", 1, &code, 2, 1);
    interface.method("privateHelper", "()I", 2, &[0x10, 13, 0xac], 1, 1);
    interface.method("staticHelper", "()I", 9, &[0x10, 7, 0xac], 1, 0);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(20))
    );
    for name in ["v", "privateHelper"] {
        let method = fixture.method("q/I", name, "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::UnsupportedMethod("interface instance bodies are unsupported")
        );
    }
}
#[test]
fn default_bodies_can_call_other_interfaces_and_class_methods_with_original_receivers() {
    let mut a = iface("q/A");
    let b_call = a.reference(11, "q/B", "b", "()I");
    let class_call = a.reference(10, "p/T", "concrete", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(b_call, 1));
    code.push(0x2a);
    code.extend(op(0xb6, class_call));
    code.extend([0x60, 0xac]);
    a.method("a", "()I", 1, &code, 2, 1);
    let mut b = iface("q/B");
    b.method("b", "()I", 1, &[0x10, 17, 0xac], 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/A");
    implement(&mut root, "q/B");
    let call = root.reference(11, "q/A", "a", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    root.method("concrete", "()I", 1, &[0x10, 23, 0xac], 1, 1);
    let mut fixture = Fixture::new(vec![a, b, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(40))
    );
}
#[test]
fn all_numeric_returns_keep_width_narrowing_raw_bits_and_descriptor_counts() {
    for (result, parameter, value, expected, load, ret, width) in [
        ("Z", "I", Value::Int(3), Value::Int(1), 0x1b, 0xac, 1),
        ("B", "I", Value::Int(128), Value::Int(-128), 0x1b, 0xac, 1),
        ("C", "I", Value::Int(-1), Value::Int(65535), 0x1b, 0xac, 1),
        (
            "S",
            "I",
            Value::Int(32768),
            Value::Int(-32768),
            0x1b,
            0xac,
            1,
        ),
        (
            "I",
            "I",
            Value::Int(i32::MIN),
            Value::Int(i32::MIN),
            0x1b,
            0xac,
            1,
        ),
        (
            "J",
            "J",
            Value::Long(i64::MIN),
            Value::Long(i64::MIN),
            0x1f,
            0xad,
            2,
        ),
        (
            "F",
            "F",
            Value::Float(0x7fc12345),
            Value::Float(0x7fc12345),
            0x23,
            0xae,
            1,
        ),
        (
            "D",
            "D",
            Value::Double(0x8000000000000000),
            Value::Double(0x8000000000000000),
            0x27,
            0xaf,
            2,
        ),
    ] {
        let target_descriptor = format!("({parameter}){result}");
        let root_descriptor = format!("({parameter}){parameter}");
        let mut interface = iface("q/I");
        interface.method("v", &target_descriptor, 1, &[load, ret], 2, 1 + width);
        let mut root = ordinary();
        implement(&mut root, "q/I");
        let call = root.reference(11, "q/I", "v", &target_descriptor);
        let mut code = vec![0x2a, load];
        code.extend(invoke(call, 1 + width as u8));
        code.push(ret);
        root.method("m", &root_descriptor, 1, &code, 3, 1 + width);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", &root_descriptor);
        assert_eq!(
            fixture.run(method, receiver, &[value]).unwrap(),
            returned(expected)
        );
    }
}
#[test]
fn mixed_width_arguments_require_receiver_in_count_and_preserve_void_caller_operands() {
    let descriptor = "(JDFI)J";
    let mut interface = iface("q/I");
    interface.method("v", descriptor, 1, &[0x1f, 0xad], 2, 7);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "v", descriptor);
    let mut code = vec![0x2a, 0x1f, 0x29, 0x17, 5, 0x15, 6];
    code.extend(invoke(call, 7));
    code.push(0xad);
    root.method("m", descriptor, 1, &code, 7, 7);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", descriptor);
    assert_eq!(
        fixture
            .run(
                method,
                receiver,
                &[
                    Value::Long(42),
                    Value::Double(0),
                    Value::Float(0),
                    Value::Int(0)
                ]
            )
            .unwrap(),
        returned(Value::Long(42))
    );
    let mut interface = iface("q/I");
    interface.method("v", "()V", 1, &[0xb1], 0, 1);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "v", "()V");
    let mut code = vec![0x10, 42, 0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(42))
    );
}
#[test]
fn exact_255_count_executes_with_category_one_or_two_arguments_and_inclusive_resources() {
    for (parameter, arguments, load, last_local, ret, stack, expected) in [
        ("I", 254, 0x15, 254, 0xac, 1, Value::Int(42)),
        ("J", 127, 0x16, 253, 0xad, 2, Value::Long(1)),
    ] {
        let descriptor = format!("({}){}", parameter.repeat(arguments), parameter);
        let mut interface = iface("q/I");
        interface.method("v", &descriptor, 1, &[load, last_local, ret], stack, 255);
        let mut root = ordinary();
        implement(&mut root, "q/I");
        let call = root.reference(11, "q/I", "v", &descriptor);
        let mut code = vec![0x2a];
        if parameter == "I" {
            code.extend(std::iter::repeat_n(0x03, arguments - 1));
            code.extend([0x10, 42]);
        } else {
            code.extend(std::iter::repeat_n(0x0a, arguments));
        }
        code.extend(invoke(call, 255));
        code.push(ret);
        let code_bytes = code.len() + 3;
        let root_descriptor = format!("(){parameter}");
        root.method("m", &root_descriptor, 1, &code, 255, 1);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", &root_descriptor);
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[],
                    ExecutionOptions {
                        instruction_budget: arguments as u64 + 5
                    },
                    CallLimits {
                        max_frames: 2,
                        max_frame_slots: 511 + usize::from(stack),
                        max_prepared_code_bytes: code_bytes
                    }
                )
                .unwrap(),
            returned(expected)
        );
    }
}

#[test]
fn count_mismatch_and_overflow_win_over_stack_extraction_and_runtime_resolution() {
    for (descriptor, count) in [
        ("()I".to_owned(), 2),
        ("(JDI)I".to_owned(), 4),
        (format!("({})I", "I".repeat(255)), 255),
    ] {
        let mut interface = iface("q/I");
        interface.method("other", "()V", 1, &[0xb1], 0, 1);
        let mut root = ordinary();
        let call = root.reference(11, "q/I", "missing", &descriptor);
        let mut code = invoke(call, count);
        code.push(0xb1);
        root.method("m", "()V", 1, &code, 0, 1);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert_eq!(error.kind, E::InvalidOperands);
        assert_eq!(error.offset, Some(0));
    }
}
#[test]
fn wrong_tags_precede_count_and_reference_descriptors_use_slot_counts_while_decoding_is_eager() {
    for (tag, descriptor, expected_descriptor) in [
        (10, "()I", false),
        (11, "([I)I", true),
        (11, "()Ljava/lang/Object;", true),
    ] {
        let interface = iface("q/I");
        let mut root = ordinary();
        let call = root.reference(tag, "q/I", "v", descriptor);
        let mut code = invoke(call, 255);
        code.push(0xb1);
        root.method("m", "()V", 1, &code, 0, 1);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        if expected_descriptor {
            assert_eq!(error.kind, E::InvalidOperands);
        } else {
            assert!(
                matches!(error.kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::InvalidConstant {..}))
            );
        }
    }
    for tail in [
        vec![0xb9, 0, 1, 0, 0],
        vec![0xb9, 0, 1, 1, 1],
        vec![0xb9, 0, 1, 1],
    ] {
        let mut root = ordinary();
        let mut code = vec![0xb1];
        code.extend(tail);
        root.method("m", "()V", 1, &code, 0, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        assert!(matches!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::Decode(_)
        ));
    }
    // Nonzero count/CP semantics are checked only when reached, unlike encoding.
    let mut root = ordinary();
    root.method("m", "()I", 1, &[0x03, 0xac, 0xb9, 0, 1, 2, 0], 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(0))
    );
}
#[test]
fn standalone_and_heap_modes_reject_b9_and_reached_bad_stack_states_stay_structured() {
    let mut root = ordinary();
    root.method("staticBody", "()V", 9, &[0xb9, 0, 1, 1, 0, 0xb1], 0, 0);
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
        assert_eq!(error.kind, E::UnsupportedInstruction(0xb9));
        assert_eq!(error.offset, Some(0));
        assert_eq!(error.method.unwrap().owner, None);
    }
    for prefix in [vec![], vec![0x03]] {
        let mut interface = iface("q/I");
        interface.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut root = ordinary();
        implement(&mut root, "q/I");
        let call = root.reference(11, "q/I", "v", "()I");
        let mut code = prefix;
        code.extend(invoke(call, 1));
        code.push(0xac);
        root.method("m", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        assert!(matches!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::StackUnderflow
                | E::WrongValueType {
                    expected: ValueKind::Reference,
                    ..
                }
        ));
    }
}

#[test]
fn class_and_inherited_implementations_mask_defaults_and_diamonds_reuse_one_declaration() {
    for inherited in [false, true] {
        let mut interface = iface("q/I");
        interface.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x20);
        if inherited {
            base.method("v", "()I", 1, &[0x08, 0xac], 1, 1);
        }
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        implement(&mut root, "q/I");
        if !inherited {
            root.method("v", "()I", 1, &[0x08, 0xac], 1, 1);
        }
        let call = root.reference(11, "q/I", "v", "()I");
        let mut code = vec![0x2a];
        code.extend(invoke(call, 1));
        code.push(0xac);
        root.method("m", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![interface, base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(5))
        );
    }
    let mut top = iface("q/Top");
    top.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
    let mut left = iface("q/Left");
    implement(&mut left, "q/Top");
    let mut right = iface("q/Right");
    implement(&mut right, "q/Top");
    let mut root = ordinary();
    implement(&mut root, "q/Left");
    implement(&mut root, "q/Right");
    let call = root.reference(11, "q/Top", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![top, left, right, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(2))
    );
}
#[test]
fn explicit_original_object_metadata_can_supply_a_public_instance_fallback_body() {
    let mut object = Class::new("java/lang/Object", None, 0x21);
    object.method("ping", "()I", 1, &[0x10, 17, 0xac], 1, 1);
    let interface = iface("q/I");
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "ping", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![object, interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(17))
    );
}
#[test]
fn same_interface_static_and_instance_cache_hits_never_bypass_mode_checks() {
    for static_target in [false, true] {
        let mut interface = iface("q/I");
        let target = interface.reference(11, "q/I", "helper", "()I");
        let mut code = if static_target {
            op(0xb8, target)
        } else {
            let mut c = vec![0x2a];
            c.extend(invoke(target, 1));
            c
        };
        code.push(0x57);
        if static_target {
            code.push(0x2a);
            code.extend(invoke(target, 1));
        } else {
            code.extend(op(0xb8, target));
        }
        code.push(0xac);
        interface.method("v", "()I", 1, &code, 1, 1);
        interface.method(
            "helper",
            "()I",
            if static_target { 9 } else { 2 },
            &[0x04, 0xac],
            1,
            1,
        );
        let mut root = ordinary();
        implement(&mut root, "q/I");
        let call = root.reference(11, "q/I", "v", "()I");
        let mut code = vec![0x2a];
        code.extend(invoke(call, 1));
        code.push(0xac);
        root.method("m", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        let result = fixture.run(method, receiver, &[]);
        // Both mismatched invocation modes are rejected by resolution before
        // a previously prepared target can grant the wrong mode.
        assert_eq!(result.unwrap(), O::IncompatibleClassChangeError);
    }
}
#[test]
fn interface_body_field_and_nested_call_errors_preserve_nonroot_owner_provenance() {
    let mut a = iface("q/A");
    let call = a.reference(11, "q/B", "b", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    a.method("a", "()I", 1, &code, 1, 1);
    let mut b = iface("q/B");
    b.method("b", "()I", 1, &[0x03, 0xac, 0xff], 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/A");
    implement(&mut root, "q/B");
    let call = root.reference(11, "q/A", "a", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![a, b, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert!(matches!(error.kind, E::Decode(_)));
    assert_eq!(error.offset, Some(2));
    assert_eq!(error.method.unwrap().owner, Some(fixture.classes["q/B"]));
    assert_eq!(
        error.call_site.unwrap().method.owner,
        Some(fixture.classes["q/A"])
    );
    let mut interface = iface("q/I");
    let field = interface.reference(9, "p/T", "private", "I");
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.push(0xac);
    interface.method("v", "()I", 1, &code, 1, 1);
    let mut root = ordinary();
    root.field("private", "I", 2);
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert!(
        matches!(error.kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::UnsupportedNestAccess {origin,..} if origin==fixture.classes["q/I"]))
    );
    assert_eq!(error.method.unwrap().owner, Some(fixture.classes["q/I"]));
    assert_eq!(
        error.call_site.unwrap().method.owner,
        Some(fixture.classes["p/T"])
    );
}
#[test]
fn deep_default_recursion_uses_shared_explicit_frames_and_prepares_the_interface_once() {
    let mut interface = iface("q/I");
    let recursive = interface.reference(11, "q/I", "v", "(I)I");
    let mut body = vec![0x1b, 0x99, 0, 15, 0x2a, 0x1b, 0x04, 0x64];
    body.extend(invoke(recursive, 2));
    body.extend([0x04, 0x60, 0xac, 0x03, 0xac]);
    let body_bytes = body.len();
    interface.method("v", "(I)I", 1, &body, 3, 2);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "v", "(I)I");
    let mut code = vec![0x2a, 0x1b];
    code.extend(invoke(call, 2));
    code.push(0xac);
    let code_bytes = code.len() + body_bytes;
    root.method("m", "(I)I", 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "(I)I");
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
fn shared_lookup_budget_is_not_bypassed_by_a_b9_hit_after_b6_cached_the_default() {
    let mut interface = iface("q/I");
    interface.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let first = root.reference(10, "p/T", "v", "()I");
    let second = root.reference(11, "q/I", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, first));
    code.extend([0x57, 0x2a]);
    code.extend(invoke(second, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
    let method = fixture.method("p/T", "m", "()I");
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
        context.select_virtual(owner, first, receiver).unwrap();
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
}
#[test]
fn java_failure_order_and_host_failures_preserve_prior_field_and_array_effects() {
    for failure in 0..13 {
        let mut interface = Class::new(
            "q/I",
            Some("java/lang/Object"),
            if failure == 2 { 0x600 } else { 0x601 },
        );
        if failure != 0 {
            interface.method(
                "v",
                "()I",
                if failure == 1 {
                    9
                } else if matches!(failure, 4 | 5) {
                    0x401
                } else {
                    1
                },
                &[0x04, 0xac],
                1,
                1,
            );
        }
        if failure == 11 {
            interface.methods[0].handler = true;
        }
        if failure == 12 {
            interface.methods[0].code = Some(vec![0xb1, 0xff]);
        }
        let mut other = iface("q/J");
        if matches!(failure, 6 | 7) {
            other.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
        }
        let mut root = ordinary();
        root.field("f", "I", 1);
        if failure != 3 {
            implement(&mut root, "q/I");
        }
        if matches!(failure, 6 | 7) {
            implement(&mut root, "q/J");
        }
        if failure == 8 {
            root.method("v", "()I", 0, &[0x05, 0xac], 1, 1);
        }
        if matches!(failure, 9 | 10) {
            root.method(
                "v",
                "()I",
                if failure == 9 { 0x101 } else { 0x21 },
                &[0x05, 0xac],
                1,
                1,
            );
        }
        let field = root.reference(9, "p/T", "f", "I");
        let call = root.reference(11, "q/I", "v", "()I");
        let mut code = vec![0x2a, 0x10, 7];
        code.extend(op(0xb5, field));
        code.extend([
            0x03,
            0xbc,
            10,
            0x57,
            if matches!(failure, 0 | 1 | 2 | 4 | 7) {
                0x01
            } else {
                0x2a
            },
        ]);
        code.extend(invoke(call, 1));
        code.push(0xac);
        root.method("m", "()I", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![interface, other, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap(), O::NoSuchMethodError),
            1 | 3 | 6 => assert_eq!(result.unwrap(), O::IncompatibleClassChangeError),
            2 | 8 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            4 | 7 => assert_eq!(result.unwrap(), O::NullPointerException),
            5 => assert_eq!(result.unwrap(), O::AbstractMethodError),
            _ => assert!(result.is_err(), "case {failure}"),
        };
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(7));
    }
}
#[test]
fn wrong_array_receiver_is_a_host_error_and_private_interface_nests_stay_unsupported() {
    let mut interface = iface("q/I");
    interface.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "v", "()I");
    let mut code = vec![0x03, 0xbc, 10];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::Context(FieldAccessError::Instance(InstanceError::Heap(HeapError::WrongEntryKind {..})))))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    let mut interface = iface("q/I");
    interface.method("privateHelper", "()I", 2, &[0x04, 0xac], 1, 1);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let call = root.reference(11, "q/I", "privateHelper", "()I");
    let mut code = vec![0x01];
    code.extend(invoke(call, 1));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::UnsupportedNestAccess {..}))
    );
}

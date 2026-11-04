//! Synthetic virtual-call body graphs and per-frame identity boundaries.
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
        for class in std::iter::once(Class::new("java/lang/Object", None, 0x21)).chain(classes) {
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

#[test]
fn equal_indices_use_callee_constants_hidden_fields_and_current_class_static_helpers() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("f", "I", 2);
    let field = base.reference(9, "q/Base", "f", "I");
    let helper = base.reference(10, "q/Base", "helper", "()I");
    let number = base.cp(3, &123456i32.to_be_bytes());
    let mut code = vec![0x2a, 0x10, 17];
    code.extend(op(0xb5, field));
    code.extend(op(0xb8, helper));
    code.push(0xac);
    base.method("value", "()I", 1, &code, 2, 1);
    let mut code = op(0x13, number);
    code.push(0xac);
    base.method("helper", "()I", 9, &code, 1, 0);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    root.field("f", "I", 2);
    let helper = root.reference(10, "p/T", "helper", "()I");
    let call = root.reference(10, "q/Base", "value", "()I");
    let number = root.cp(3, &(-789i32).to_be_bytes());
    let mut code = op(0xb8, helper);
    code.extend([0x57, 0x05, 0x2a]);
    code.extend(op(0xb6, call));
    code.extend([0x60, 0xac]);
    root.method("m", "()I", 1, &code, 2, 1);
    let mut code = op(0x13, number);
    code.push(0xac);
    root.method("helper", "()I", 9, &code, 1, 0);
    let code_bytes: usize = base
        .methods
        .iter()
        .chain(&root.methods)
        .map(|method| method.code.as_ref().unwrap().len())
        .sum();
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        method.index(),
        fixture.method("q/Base", "value", "()I").index()
    );
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(123458))
    );
    assert_eq!(fixture.stored("q/Base", "f", "I", receiver), Value::Int(17));
    assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(0));
    let limits = CallLimits {
        max_frames: 3,
        max_frame_slots: 7,
        max_prepared_code_bytes: code_bytes,
    };
    assert_eq!(
        fixture
            .run_limits(method, receiver, &[], ExecutionOptions::default(), limits)
            .unwrap(),
        returned(Value::Int(123458))
    );
    let error = fixture
        .run_limits(
            method,
            receiver,
            &[],
            ExecutionOptions::default(),
            CallLimits {
                max_prepared_code_bytes: code_bytes - 1,
                ..limits
            },
        )
        .unwrap_err();
    assert_eq!(
        error.kind,
        E::PreparedCodeLimit {
            limit: code_bytes - 1,
            required: code_bytes
        }
    );
    assert_eq!(error.method.unwrap().owner, Some(fixture.classes["q/Base"]));
}
#[test]
fn virtual_arguments_keep_mixed_widths_and_every_numeric_return_kind() {
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
        code.extend(op(0xb6, call));
        code.push(ret);
        root.method("m", &descriptor, 1, &code, 7, 7);
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", &descriptor);
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
    for (result, arg, expected) in [
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
        code.extend(op(0xb6, call));
        code.push(0xac);
        root.method("m", "(I)I", 1, &code, 2, 2);
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "(I)I");
        assert_eq!(
            fixture.run(method, receiver, &[Value::Int(arg)]).unwrap(),
            returned(Value::Int(expected))
        );
    }
}
#[test]
fn void_virtual_return_preserves_caller_operands_and_receiver_field_effects() {
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
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(42))
    );
    assert_eq!(fixture.stored("q/Base", "f", "I", receiver), Value::Int(7));
}
fn recursive_code(index: u16) -> Vec<u8> {
    let mut code = vec![0x1b, 0x99, 0, 13, 0x2a, 0x1b, 0x04, 0x64];
    code.extend(op(0xb6, index));
    code.extend([0x04, 0x60, 0xac, 0x03, 0xac]);
    code
}
#[test]
fn direct_recursion_reuses_instance_root_cache_without_recursive_host_execution() {
    let mut root = ordinary();
    let call = root.reference(10, "p/T", "m", "(I)I");
    let code = recursive_code(call);
    root.method("m", "(I)I", 1, &code, 3, 2);
    let mut fixture = Fixture::new(vec![root]);
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
                    instruction_budget: 10 * count as u64 + 4
                },
                CallLimits {
                    max_frames: usize::MAX,
                    max_frame_slots: usize::MAX,
                    max_prepared_code_bytes: code.len()
                }
            )
            .unwrap(),
        returned(Value::Int(count))
    );
}
#[test]
fn mutual_crossclass_recursion_shares_exact_frame_slot_code_and_instruction_limits() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let call = base.reference(10, "p/T", "b", "(I)I");
    let code_a = recursive_code(call);
    base.method("a", "(I)I", 1, &code_a, 3, 2);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "a", "(I)I");
    let code_b = recursive_code(call);
    root.method("b", "(I)I", 1, &code_b, 3, 2);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("q/Base", "a", "(I)I");
    let limits = CallLimits {
        max_frames: 4,
        max_frame_slots: 20,
        max_prepared_code_bytes: code_a.len() + code_b.len(),
    };
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[Value::Int(3)],
                ExecutionOptions {
                    instruction_budget: 34
                },
                limits
            )
            .unwrap(),
        returned(Value::Int(3))
    );
    for (limits, budget, expected) in [
        (
            CallLimits {
                max_frames: 3,
                ..limits
            },
            34,
            E::FrameLimit {
                limit: 3,
                required: 4,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 19,
                ..limits
            },
            34,
            E::FrameSlotLimit {
                limit: 19,
                required: 20,
            },
        ),
        (
            CallLimits {
                max_prepared_code_bytes: 31,
                ..limits
            },
            34,
            E::PreparedCodeLimit {
                limit: 31,
                required: 32,
            },
        ),
        (limits, 33, E::InstructionBudgetExhausted { limit: 33 }),
    ] {
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[Value::Int(3)],
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
fn cache_hits_never_convert_static_and_instance_invocation_modes() {
    for static_target in [true, false] {
        let mut root = ordinary();
        let call = root.reference(10, "p/T", "v", "()I");
        let mut code = if static_target {
            op(0xb8, call)
        } else {
            let mut c = vec![0x2a];
            c.extend(op(0xb6, call));
            c
        };
        code.push(0x57);
        if static_target {
            code.push(0x2a);
            code.extend(op(0xb6, call));
        } else {
            code.extend(op(0xb8, call));
        }
        code.push(0xac);
        root.method("m", "()I", 1, &code, 1, 1);
        root.method(
            "v",
            "()I",
            if static_target { 9 } else { 1 },
            &[0x04, 0xac],
            1,
            1,
        );
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
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
fn selected_callee_errors_report_distinct_equal_index_owners_and_actual_call_site() {
    for code in [vec![0xb1, 0xff], vec![0x2a, 0xb4, 0, 1, 0x57, 0xb1]] {
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        base.method("v", "()V", 1, &code, 1, 1);
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        let call = root.reference(10, "q/Base", "v", "()V");
        let mut code = vec![0x2a];
        code.extend(op(0xb6, call));
        code.push(0xb1);
        root.method("m", "()V", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        let site = error.call_site.as_ref().unwrap();
        assert_eq!(error.offset, Some(1));
        assert_eq!(
            error.method.as_ref().unwrap().owner,
            Some(fixture.classes["q/Base"])
        );
        assert_eq!(site.method.owner, Some(fixture.classes["p/T"]));
        assert_eq!(error.method.as_ref().unwrap().index, site.method.index);
        assert_eq!(site.offset, 1);
        assert!(error.to_string().contains("owner"));
        assert!(matches!(error.kind, E::Decode(_) | E::FieldAccess(_)));
    }
}
#[test]
fn nested_method_host_errors_use_innermost_constant_pool_and_nonroot_caller_identity() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let call = base.reference(10, "p/T", "v", "()V");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, call));
    code.push(0xb1);
    base.method("m", "()V", 1, &code, 1, 1);
    let mut middle = Class::new("p/T", Some("q/Base"), 0x21);
    let call = middle.reference(10, "r/Leaf", "w", "()V");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, call));
    code.push(0xb1);
    middle.method("v", "()V", 1, &code, 1, 1);
    let mut leaf = Class::new("r/Leaf", Some("p/T"), 0x21);
    leaf.method("w", "()V", 1, &[0x2a, 0xb6, 0, 1, 0xb1], 1, 1);
    let mut fixture = Fixture::new(vec![base, middle, leaf]);
    let receiver = fixture.object("r/Leaf");
    let method = fixture.method("q/Base", "m", "()V");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(
        error.method.as_ref().unwrap().owner,
        Some(fixture.classes["r/Leaf"])
    );
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(fixture.classes["r/Leaf"])
    );
    assert_eq!(error.offset, Some(1));
    assert!(
        matches!(error.kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::InvalidConstant {origin,index:1} if origin==fixture.classes["r/Leaf"]))
    );
}
#[test]
fn category_two_return_transfer_checks_actual_caller_capacity() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("v", "()J", 1, &[0x09, 0xad], 2, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "v", "()J");
    let mut code = vec![0x09, 0x2a];
    code.extend(op(0xb6, call));
    code.push(0xad);
    root.method("m", "()J", 1, &code, 3, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()J");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(
        error.kind,
        E::StackOverflow {
            required: 4,
            capacity: 3
        }
    );
    assert_eq!(error.method.as_ref().unwrap().owner, Some(method.owner()));
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(method.owner())
    );
    assert_eq!(error.offset, Some(2));
}
#[test]
fn selection_failures_and_target_preparation_order_are_symbolic_or_explicit_hosts() {
    for (flags, null, name, expected) in [
        (1, true, "missing", Some(O::NoSuchMethodError)),
        (9, true, "v", Some(O::IncompatibleClassChangeError)),
        (0, true, "v", Some(O::IllegalAccessError)),
        (0x401, true, "v", Some(O::NullPointerException)),
        (0x401, false, "v", Some(O::AbstractMethodError)),
        (0x101, true, "v", Some(O::NullPointerException)),
        (0x101, false, "v", None),
        (0x21, false, "v", None),
        (2, true, "v", None),
    ] {
        let mut base = Class::new(
            "q/Base",
            Some("java/lang/Object"),
            if flags & 0x400 != 0 { 0x421 } else { 0x21 },
        );
        base.method("v", "()I", flags, &[0x04, 0xac], 1, 1);
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        let call = root.reference(10, "q/Base", name, "()I");
        let mut code = vec![if null { 0x01 } else { 0x2a }];
        code.extend(op(0xb6, call));
        code.push(0xac);
        root.method("m", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        let result = fixture.run(method, receiver, &[]);
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected, "flags {flags:x}");
        } else if flags == 2 {
            assert!(
                matches!(result.unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::UnsupportedNestAccess {..}))
            );
        } else {
            assert!(matches!(result.unwrap_err().kind, E::UnsupportedMethod(_)));
        }
    }
}
#[test]
fn old_entry_points_reject_virtual_calls_and_malformed_runtime_calls_are_checked() {
    for heap in [false, true] {
        let mut root = ordinary();
        root.method("m", "()V", 9, &[0xb6, 0, 1, 0xb1], 0, 0);
        let mut fixture = Fixture::new(vec![root]);
        let class = fixture.registry.class(fixture.classes["p/T"]).unwrap();
        let error = if heap {
            execute_static_method_body_with_heap(
                class,
                0,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                &mut fixture.heap,
            )
        } else {
            execute_static_method_body(class, 0, &[], ExecutionOptions::default())
        }
        .unwrap_err();
        assert_eq!(error.kind, E::UnsupportedInstruction(0xb6));
        assert_eq!(error.method.unwrap().owner, None);
    }
    for (tag, prefix, descriptor) in [
        (9, vec![0x2a], "I"),
        (11, vec![0x2a], "()I"),
        (10, vec![0x03], "()I"),
        (10, vec![], "()I"),
        (10, vec![0x2a, 0x09], "(I)I"),
    ] {
        let mut root = ordinary();
        let call = root.reference(tag, "p/T", "v", descriptor);
        let mut code = prefix;
        code.extend(op(0xb6, call));
        code.push(0xac);
        root.method("m", "()I", 1, &code, 3, 1);
        root.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        if descriptor == "(I)I" {
            assert_eq!(
                error.kind,
                E::WrongValueType {
                    expected: ValueKind::Int,
                    actual: ValueKind::Long
                }
            );
        } else {
            assert!(matches!(
                error.kind,
                E::MethodAccess(_) | E::WrongValueType { .. } | E::StackUnderflow
            ));
        }
    }
}

#[test]
fn private_and_final_targets_dispatch_correctly_and_invalid_overrides_stay_host_errors() {
    for final_method in [false, true] {
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        let call = base.reference(10, "q/Base", "v", "()I");
        let mut code = vec![0x2a];
        code.extend(op(0xb6, call));
        code.push(0xac);
        base.method("m", "()I", 1, &code, 1, 1);
        base.method(
            "v",
            "()I",
            if final_method { 0x11 } else { 2 },
            &[0x04, 0xac],
            1,
            1,
        );
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        if !final_method {
            root.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
        }
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("q/Base", "m", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(1))
        );
    }
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("v", "()I", 0x11, &[0x04, 0xac], 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "p/T", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    root.method("v", "()I", 1, &[0x05, 0xac], 1, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::InvalidOverride {..}))
    );
}
#[test]
fn interface_fallback_signature_polymorphism_and_array_owners_remain_explicit() {
    let mut interface = Class::new("q/I", Some("java/lang/Object"), 0x601);
    interface.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
    let mut root = ordinary();
    let interface_index = root.class("q/I");
    root.interfaces.push(interface_index);
    let call = root.reference(10, "p/T", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    let mut handle = Class::new(
        "java/lang/invoke/MethodHandle",
        Some("java/lang/Object"),
        0x21,
    );
    handle.method(
        "invoke",
        "([Ljava/lang/Object;)Ljava/lang/Object;",
        0x181,
        &[],
        0,
        2,
    );
    let mut root = Class::new("p/T", Some("java/lang/invoke/MethodHandle"), 0x21);
    let call = root.reference(10, "java/lang/invoke/MethodHandle", "invoke", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![handle, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::UnsupportedSignaturePolymorphic {..}))
    );
    let mut root = ordinary();
    let call = root.reference(10, "[I", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::Context(FieldAccessError::UnsupportedArrayOwner)))
    );
}
#[test]
fn wrong_receiver_kinds_and_reference_descriptors_are_structured_before_target_execution() {
    let mut root = ordinary();
    let call = root.reference(10, "p/T", "v", "()I");
    let mut code = vec![0x03, 0xbc, 10];
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    root.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,MethodAccessError::Context(FieldAccessError::Instance(InstanceError::Heap(HeapError::WrongEntryKind {..})))))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    for descriptor in ["()Ljava/lang/Object;", "([I)I"] {
        let mut root = ordinary();
        let call = root.reference(10, "p/T", "v", descriptor);
        let mut code = vec![0x2a];
        code.extend(op(0xb6, call));
        code.push(0xb1);
        root.method("m", "()V", 1, &code, 1, 1);
        root.method("v", descriptor, 1, &[0x01, 0xb0], 1, 2);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        if descriptor.starts_with("()") {
            assert_eq!(
                fixture.run(method, receiver, &[]).unwrap(),
                O::Returned(None)
            );
        } else {
            assert_eq!(
                fixture.run(method, receiver, &[]).unwrap_err().kind,
                E::StackUnderflow
            );
        }
    }
}
#[test]
fn cumulative_resolution_budget_is_shared_across_cached_virtual_calls_and_field_effects() {
    let mut root = ordinary();
    root.field("f", "I", 1);
    let field = root.reference(9, "p/T", "f", "I");
    let call = root.reference(10, "p/T", "v", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, call));
    code.extend([0x57, 0x2a]);
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, field));
    code.extend([0x04, 0xac]);
    root.method("v", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
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
        context.select_virtual(owner, call, receiver).unwrap();
        context
            .write_field(owner, field, receiver, Value::Int(0))
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
        .find_declared_field(owner, &units("f"), &units("I"))
        .unwrap()
        .unwrap();
    assert_eq!(
        context.heap().get_field(receiver, field).unwrap(),
        Value::Int(7)
    );
}
#[test]
fn completed_effects_survive_virtual_java_outcomes_and_target_or_host_failures() {
    for failure in 0..8 {
        let mut base = Class::new(
            "q/Base",
            Some("java/lang/Object"),
            if failure == 1 { 0x421 } else { 0x21 },
        );
        let flags = match failure {
            1 => 0x401,
            2 => 9,
            3 => 0,
            5 => 0x101,
            _ => 1,
        };
        base.method("v", "()I", flags, &[0x04, 0xac], 1, 1);
        if failure == 6 {
            base.methods[0].handler = true;
        }
        if failure == 7 {
            base.methods[0].code = Some(vec![0xb1, 0xff]);
        }
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        root.field("f", "I", 1);
        let field = root.reference(9, "p/T", "f", "I");
        let call = root.reference(
            10,
            "q/Base",
            if failure == 0 { "missing" } else { "v" },
            "()I",
        );
        let mut code = vec![0x2a, 0x10, 7];
        code.extend(op(0xb5, field));
        code.extend([0x03, 0xbc, 10, 0x57, if failure == 4 { 0x01 } else { 0x2a }]);
        code.extend(op(0xb6, call));
        code.push(0xac);
        root.method("m", "()I", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![base, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap(), O::NoSuchMethodError),
            1 => assert_eq!(result.unwrap(), O::AbstractMethodError),
            2 => assert_eq!(result.unwrap(), O::IncompatibleClassChangeError),
            3 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            4 => assert_eq!(result.unwrap(), O::NullPointerException),
            _ => assert!(result.is_err()),
        };
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(7));
    }
}

#[test]
fn static_helpers_after_crossing_classes_cannot_use_a_root_owner_cache_shortcut() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let call = base.reference(10, "p/T", "helper", "()I");
    let mut code = op(0xb8, call);
    code.push(0xac);
    base.method("v", "()I", 1, &code, 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let helper = root.reference(10, "p/T", "helper", "()I");
    let call = root.reference(10, "q/Base", "v", "()I");
    let mut code = op(0xb8, helper);
    code.extend([0x57, 0x2a]);
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("m", "()I", 1, &code, 1, 1);
    root.method("helper", "()I", 9, &[0x04, 0xac], 1, 0);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert!(matches!(error.kind, E::UnsupportedCall(_)));
    assert_eq!(
        error.method.as_ref().unwrap().owner,
        Some(fixture.classes["q/Base"])
    );
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(fixture.classes["q/Base"])
    );
}

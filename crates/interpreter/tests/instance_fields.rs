//! Synthetic acceptance for the explicit instance-body/field bridge.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_instance_method_body, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext, FieldResolutionOptions,
    HeapError, HeapOptions, HierarchyOptions, InstanceError, InstanceOptions, ManagedHeap,
    MethodId, PrimitiveArrayKind, Reference, RegistryOptions, SameLoaderResolver, build_hierarchy,
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
            0,
            self.fields.len() as u16,
        ] {
            u2(&mut bytes, word);
        }
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
fn all_numeric_field_kinds_narrow_or_preserve_raw_bits_with_category_two_receiver_layout() {
    for (descriptor, args, expected, load, ret, locals) in [
        (
            "Z",
            vec![Value::Int(-2), Value::Int(3)],
            vec![Value::Int(0), Value::Int(1)],
            0x1b,
            0xac,
            2,
        ),
        (
            "B",
            vec![Value::Int(128), Value::Int(0xffff_ffffu32 as i32)],
            vec![Value::Int(-128), Value::Int(-1)],
            0x1b,
            0xac,
            2,
        ),
        (
            "C",
            vec![Value::Int(-1), Value::Int(65536)],
            vec![Value::Int(65535), Value::Int(0)],
            0x1b,
            0xac,
            2,
        ),
        (
            "S",
            vec![Value::Int(32768), Value::Int(65535)],
            vec![Value::Int(-32768), Value::Int(-1)],
            0x1b,
            0xac,
            2,
        ),
        (
            "I",
            vec![Value::Int(i32::MIN), Value::Int(i32::MAX)],
            vec![Value::Int(i32::MIN), Value::Int(i32::MAX)],
            0x1b,
            0xac,
            2,
        ),
        (
            "J",
            vec![Value::Long(i64::MIN), Value::Long(i64::MAX)],
            vec![Value::Long(i64::MIN), Value::Long(i64::MAX)],
            0x1f,
            0xad,
            3,
        ),
        (
            "F",
            vec![Value::Float(0x80000000), Value::Float(0x7fc12345)],
            vec![Value::Float(0x80000000), Value::Float(0x7fc12345)],
            0x23,
            0xae,
            2,
        ),
        (
            "D",
            vec![
                Value::Double(0x8000000000000000),
                Value::Double(0x7ff8123456789abc),
            ],
            vec![
                Value::Double(0x8000000000000000),
                Value::Double(0x7ff8123456789abc),
            ],
            0x27,
            0xaf,
            3,
        ),
    ] {
        let mut class = ordinary();
        class.field("f", descriptor, 1);
        let field = class.reference(9, "p/T", "f", descriptor);
        let mut code = vec![0x2a, load];
        code.extend(op(0xb5, field));
        code.push(0x2a);
        code.extend(op(0xb4, field));
        code.push(ret);
        let computational = if matches!(descriptor, "Z" | "B" | "C" | "S") {
            "I"
        } else {
            descriptor
        };
        let method_descriptor = format!("({computational}){computational}");
        class.method("m", &method_descriptor, 1, &code, 3, locals);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", &method_descriptor);
        for (value, expected) in args.into_iter().zip(expected) {
            assert_eq!(
                fixture.run(method, receiver, &[value]).unwrap(),
                returned(expected)
            );
            assert_eq!(fixture.stored("p/T", "f", descriptor, receiver), expected);
        }
    }
}
#[test]
fn null_reference_fields_support_reads_stores_branches_and_receiver_aliases() {
    for descriptor in ["Ljava/lang/Object;", "[I", "[[Ljava/lang/Object;"] {
        let mut class = ordinary();
        class.field("f", descriptor, 1);
        let field = class.reference(9, "p/T", "f", descriptor);
        let mut code = vec![0x2a, 0x4c, 0x2b, 0x01];
        code.extend(op(0xb5, field));
        code.push(0x2b);
        code.extend(op(0xb4, field));
        code.extend([0xc6, 0, 5, 0x03, 0xac, 0x04, 0xac]);
        class.method("m", "()I", 1, &code, 2, 2);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(1))
        );
        assert_eq!(
            fixture.stored("p/T", "f", descriptor, receiver),
            Value::Reference(Reference::null())
        );
    }
}
#[test]
fn root_receiver_precedes_mixed_width_arguments_and_original_argument_indices_remain_stable() {
    let mut class = ordinary();
    class.method("m", "(JDFI)J", 1, &[0x1f, 0xad], 2, 7);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "(JDFI)J");
    let args = [
        Value::Long(42),
        Value::Double(0),
        Value::Float(0),
        Value::Int(0),
    ];
    assert_eq!(
        fixture.run(method, receiver, &args).unwrap(),
        returned(Value::Long(42))
    );
    let mut wrong = args;
    wrong[2] = Value::Int(0);
    assert_eq!(
        fixture.run(method, receiver, &wrong).unwrap_err().kind,
        E::ArgumentType {
            index: 2,
            expected: ValueKind::Float,
            actual: ValueKind::Int
        }
    );
    assert!(matches!(
        fixture.run(method, receiver, &args[..3]).unwrap_err().kind,
        E::ArgumentCount {
            expected: 4,
            actual: 3
        }
    ));
}
#[test]
fn inherited_body_uses_declaring_constant_pool_and_preserves_hidden_fields() {
    let mut parent = ordinary();
    parent.field("f", "I", 1);
    let field = parent.reference(9, "p/T", "f", "I");
    let mut code = vec![0x2a, 0x10, 41];
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb4, field));
    code.push(0xac);
    parent.method("m", "()I", 1, &code, 2, 1);
    let mut child = Class::new("p/Child", Some("p/T"), 0x21);
    child.field("f", "I", 1);
    let mut fixture = Fixture::new(vec![parent, child]);
    let receiver = fixture.object("p/Child");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(41))
    );
    assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(41));
    assert_eq!(fixture.stored("p/Child", "f", "I", receiver), Value::Int(0));
}
#[test]
fn resolution_staticness_finality_and_access_failures_precede_null() {
    for (name, flags, owner_public, opcode, expected) in [
        ("missing", 1, true, 0xb4, O::NoSuchFieldError),
        ("f", 9, true, 0xb4, O::IncompatibleClassChangeError),
        ("f", 9, true, 0xb5, O::IncompatibleClassChangeError),
        ("f", 0x11, true, 0xb5, O::IllegalAccessError),
        ("f", 1, false, 0xb4, O::IllegalAccessError),
        ("f", 1, true, 0xb4, O::NullPointerException),
    ] {
        let mut class = ordinary();
        let field = class.reference(9, "q/Owner", name, "I");
        let mut code = vec![0x01];
        if opcode == 0xb5 {
            code.push(0x04);
        }
        code.extend(op(opcode, field));
        code.extend(if opcode == 0xb4 {
            vec![0xac]
        } else {
            vec![0x03, 0xac]
        });
        class.method("m", "()I", 1, &code, 2, 1);
        let mut owner = Class::new(
            "q/Owner",
            Some("java/lang/Object"),
            if owner_public { 0x21 } else { 0x20 },
        );
        owner.field("f", "I", flags);
        let mut fixture = Fixture::new(vec![class, owner]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        assert_eq!(fixture.run(method, receiver, &[]).unwrap(), expected);
    }
}
#[test]
fn malformed_constant_indices_and_stack_types_are_explicit_with_instruction_identity() {
    for (index, code_prefix) in [(u16::MAX, vec![0x2a]), (1, vec![0x2a]), (1, vec![0x03])] {
        let mut class = ordinary();
        let mut code = code_prefix;
        code.extend(op(0xb4, index));
        code.push(0xac);
        class.method("m", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert_eq!(error.offset, Some(1));
        assert_eq!(error.method.as_ref().unwrap().index, method.index());
        if code[0] == 0x03 {
            assert!(matches!(
                error.kind,
                E::WrongValueType {
                    expected: ValueKind::Reference,
                    actual: ValueKind::Int
                }
            ));
        } else {
            assert!(
                matches!(error.kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::InvalidConstant {..}))
            );
        }
    }
    for code in [vec![0xb4, 0, 1, 0xac], vec![0x2a, 0xb5, 0, 1, 0xb1]] {
        let mut class = ordinary();
        class.method("m", "()V", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        assert!(matches!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::StackUnderflow | E::WrongValueType { .. }
        ));
    }
}
#[test]
fn incorrect_field_value_kinds_and_unbound_nonnull_stores_preserve_defaults() {
    for (descriptor, code_tail, expected_kind) in [
        ("I", vec![0x01], ValueKind::Int),
        ("J", vec![0x03], ValueKind::Long),
        ("F", vec![0x09], ValueKind::Float),
        ("D", vec![0x0b], ValueKind::Double),
    ] {
        let mut class = ordinary();
        class.field("f", descriptor, 1);
        let field = class.reference(9, "p/T", "f", descriptor);
        let mut code = vec![0x2a];
        code.extend(code_tail);
        code.extend(op(0xb5, field));
        code.push(0xb1);
        class.method("m", "()V", 1, &code, 3, 1);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        assert!(
            matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::Instance(InstanceError::Heap(HeapError::ValueKindMismatch {expected,..})) if expected==expected_kind))
        );
    }
    let mut class = ordinary();
    class.field("f", "Ljava/lang/Object;", 1);
    let field = class.reference(9, "p/T", "f", "Ljava/lang/Object;");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    class.method("m", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()V");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::FieldAccess(error) if matches!(error.as_ref(),FieldAccessError::ReferenceType(error) if matches!(error.as_ref(),crabspot_runtime::TypeAccessError::MissingUniverse)))
    );
    assert_eq!(
        fixture.stored("p/T", "f", "Ljava/lang/Object;", receiver),
        Value::Reference(Reference::null())
    );
}
#[test]
fn old_entry_points_never_enable_fields_even_with_a_configured_instance_heap() {
    for opcode in [0xb4, 0xb5] {
        let mut class = ordinary();
        let mut code = op(opcode, 1);
        code.push(0xb1);
        class.method("m", "()V", 9, &code, 0, 0);
        let mut fixture = Fixture::new(vec![class]);
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
            assert_eq!(error.kind, E::UnsupportedInstruction(opcode));
            assert_eq!(error.offset, Some(0));
        }
    }
}

#[test]
fn host_receiver_checks_reject_null_arrays_foreign_heaps_and_unrelated_instances() {
    let mut class = ordinary();
    class.method("m", "()V", 1, &[0xb1], 0, 1);
    let unrelated = Class::new("q/U", Some("java/lang/Object"), 0x21);
    let mut fixture = Fixture::new(vec![class, unrelated]);
    let method = fixture.method("p/T", "m", "()V");
    let array = fixture
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut foreign_heap = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign = foreign_heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    for (receiver, expected) in [
        (Reference::null(), HeapError::NullReference),
        (foreign, HeapError::ForeignHeap),
    ] {
        assert!(
            matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::Instance(InstanceError::Heap(ref actual)) if *actual==expected))
        );
    }
    assert!(
        matches!(fixture.run(method,array,&[]).unwrap_err().kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::Instance(InstanceError::Heap(HeapError::WrongEntryKind {..}))))
    );
    let unrelated = fixture.object("q/U");
    assert!(
        matches!(fixture.run(method,unrelated,&[]).unwrap_err().kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::InvalidReceiver {..}))
    );
    let mut other_class = ordinary();
    other_class.method("m", "()V", 1, &[0xb1], 0, 1);
    let other = Fixture::new(vec![other_class]);
    let foreign_method = other.method("p/T", "m", "()V");
    let receiver = fixture.object("p/T");
    assert!(
        matches!(fixture.run(foreign_method,receiver,&[]).unwrap_err().kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::Registry(_)))
    );
}
#[test]
fn excluded_root_declarations_and_reference_descriptors_fail_before_execution() {
    for (class_flags, method_flags, name, descriptor, handler, expected) in [
        (
            0x21,
            9,
            "m",
            "()V",
            false,
            "instance body must be non-static",
        ),
        (
            0x21,
            0x101,
            "m",
            "()V",
            false,
            "native methods require runtime services",
        ),
        (
            0x421,
            0x401,
            "m",
            "()V",
            false,
            "abstract methods have no executable body",
        ),
        (
            0x21,
            0x21,
            "m",
            "()V",
            false,
            "synchronized methods require monitor services",
        ),
        (
            0x601,
            1,
            "m",
            "()V",
            false,
            "interface instance bodies are unsupported",
        ),
        (
            0x21,
            1,
            "<init>",
            "()V",
            false,
            "special initialization methods are not ordinary method bodies",
        ),
        (
            0x21,
            8,
            "<clinit>",
            "()V",
            false,
            "special initialization methods are not ordinary method bodies",
        ),
        (
            0x21,
            1,
            "m",
            "()V",
            true,
            "exception handlers require exception dispatch",
        ),
    ] {
        let mut class = Class::new("p/T", Some("java/lang/Object"), class_flags);
        class.method(name, descriptor, method_flags, &[0xb1], 1, 1);
        class.methods[0].handler = handler;
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("java/lang/Object");
        let method = fixture.method("p/T", name, descriptor);
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::UnsupportedMethod(expected)
        );
    }
    for descriptor in [
        "(Ljava/lang/Object;)V",
        "([I)V",
        "()Ljava/lang/Object;",
        "()[I",
    ] {
        let mut class = ordinary();
        class.method("m", descriptor, 1, &[0xb1], 0, 2);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", descriptor);
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::UnsupportedDescriptor
        );
    }
    let mut class = ordinary();
    class.method("m", "()I", 2, &[0x04, 0xac], 1, 1);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
}
#[test]
fn root_local_and_operand_capacities_include_receiver_and_category_two_values() {
    let mut class = ordinary();
    class.field("f", "J", 1);
    let field = class.reference(9, "p/T", "f", "J");
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.push(0xad);
    class.method("m", "()J", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()J");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(error.offset, Some(1));
    assert_eq!(
        error.kind,
        E::StackOverflow {
            required: 2,
            capacity: 1
        }
    );
    let mut class = ordinary();
    class.method("m", "(J)J", 1, &[0x2a, 0x4d, 0x1f, 0xad], 2, 3);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "(J)J");
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Long(1)])
            .unwrap_err()
            .kind,
        E::LocalNotValue { index: 1 }
    );
}
#[test]
fn instance_root_cannot_be_reentered_through_invokestatic_even_when_prepared_cache_hits() {
    let mut class = ordinary();
    let call = class.reference(10, "p/T", "m", "(I)I");
    let mut code = vec![0x1b];
    code.extend(op(0xb8, call));
    code.push(0xac);
    class.method("m", "(I)I", 1, &code, 1, 2);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "(I)I");
    let error = fixture.run(method, receiver, &[Value::Int(7)]).unwrap_err();
    assert_eq!(error.kind, E::UnsupportedMethod("method must be static"));
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.method.unwrap().index, method.index());
    let site = error.call_site.unwrap();
    assert_eq!(site.offset, 1);
    assert_eq!(site.method.index, method.index());
}
#[test]
fn numeric_static_helpers_share_array_services_fuel_slots_and_code_cache_with_instance_root() {
    let mut class = ordinary();
    class.field("f", "I", 1);
    let field = class.reference(9, "p/T", "f", "I");
    let call = class.reference(10, "p/T", "helper", "(I)I");
    let mut code = vec![0x2a, 0x06];
    code.extend(op(0xb5, field));
    code.extend([0x05, 0xbc, 10, 0x59, 0x03, 0x10, 9, 0x4f, 0xbe]);
    code.extend(op(0xb8, call));
    code.push(0xac);
    let code_bytes = code.len() + 4;
    class.method("m", "()I", 1, &code, 4, 1);
    class.method("helper", "(I)I", 9, &[0x1a, 0x04, 0x60, 0xac], 2, 1);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    let limits = CallLimits {
        max_frames: 2,
        max_frame_slots: 8,
        max_prepared_code_bytes: code_bytes,
    };
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: 16
                },
                limits
            )
            .unwrap(),
        returned(Value::Int(3))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    for (limits, budget, expected) in [
        (
            CallLimits {
                max_frames: 1,
                ..limits
            },
            100,
            E::FrameLimit {
                limit: 1,
                required: 2,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 7,
                ..limits
            },
            100,
            E::FrameSlotLimit {
                limit: 7,
                required: 8,
            },
        ),
        (
            CallLimits {
                max_prepared_code_bytes: code_bytes - 1,
                ..limits
            },
            100,
            E::PreparedCodeLimit {
                limit: code_bytes - 1,
                required: code_bytes,
            },
        ),
        (limits, 15, E::InstructionBudgetExhausted { limit: 15 }),
    ] {
        let before = fixture.heap.allocated_arrays();
        let error = fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: budget,
                },
                limits,
            )
            .unwrap_err();
        assert_eq!(error.kind, expected);
        assert_eq!(fixture.heap.allocated_arrays(), before + 1);
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(3));
    }
}
#[test]
fn root_limits_and_zero_fuel_are_inclusive_and_do_not_execute_the_first_field_store() {
    let mut class = ordinary();
    class.field("f", "I", 1);
    let field = class.reference(9, "p/T", "f", "I");
    let mut code = vec![0x2a, 0x1b];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    class.method("m", "(I)V", 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "(I)V");
    let exact = CallLimits {
        max_frames: 1,
        max_frame_slots: 4,
        max_prepared_code_bytes: 6,
    };
    for (limits, budget, expected) in [
        (
            CallLimits {
                max_frames: 0,
                ..exact
            },
            4,
            E::FrameLimit {
                limit: 0,
                required: 1,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 3,
                ..exact
            },
            4,
            E::FrameSlotLimit {
                limit: 3,
                required: 4,
            },
        ),
        (
            CallLimits {
                max_prepared_code_bytes: 5,
                ..exact
            },
            4,
            E::PreparedCodeLimit {
                limit: 5,
                required: 6,
            },
        ),
        (exact, 0, E::InstructionBudgetExhausted { limit: 0 }),
    ] {
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[Value::Int(9)],
                    ExecutionOptions {
                        instruction_budget: budget
                    },
                    limits
                )
                .unwrap_err()
                .kind,
            expected
        );
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(0));
    }
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[Value::Int(9)],
                ExecutionOptions {
                    instruction_budget: 4
                },
                exact
            )
            .unwrap(),
        O::Returned(None)
    );
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[Value::Int(8)],
                ExecutionOptions {
                    instruction_budget: 3
                },
                exact
            )
            .unwrap_err()
            .kind,
        E::InstructionBudgetExhausted { limit: 3 }
    );
    assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(8));
}
#[test]
fn static_helper_field_host_failures_keep_callee_and_caller_provenance() {
    let mut class = ordinary();
    let call = class.reference(10, "p/T", "helper", "()I");
    let mut code = op(0xb8, call);
    code.push(0xac);
    class.method("m", "()I", 1, &code, 1, 1);
    class.method("helper", "()I", 9, &[0x01, 0xb4, 0, 1, 0xac], 1, 0);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    let helper = fixture.method("p/T", "helper", "()I");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(error.offset, Some(1));
    assert_eq!(error.method.as_ref().unwrap().index, helper.index());
    let site = error.call_site.as_ref().unwrap();
    assert_eq!(site.method.index, method.index());
    assert_eq!(site.offset, 0);
    assert!(matches!(error.kind, E::FieldAccess(_)));
    assert!(std::error::Error::source(&error).is_some());
    assert!(error.to_string().contains("field service"));
}
#[test]
fn completed_field_and_array_effects_persist_across_every_symbolic_outcome_and_host_failure() {
    for failure in 0..10 {
        let mut class = ordinary();
        class.field("f", "I", 1);
        class.field("final", "I", 0x11);
        class.field("static", "I", 9);
        let field = class.reference(9, "p/T", "f", "I");
        let missing = class.reference(9, "p/T", "missing", "I");
        let final_field = class.reference(9, "p/T", "final", "I");
        let static_field = class.reference(9, "p/T", "static", "I");
        let mut code = vec![0x2a, 0x10, 7];
        code.extend(op(0xb5, field));
        code.extend([0x03, 0xbc, 10, 0x57]);
        let expected = match failure {
            0 => {
                code.extend([0x04, 0x03, 0x6c, 0xac]);
                Some(O::ArithmeticException)
            }
            1 => {
                code.push(0x01);
                code.extend(op(0xb4, field));
                code.push(0xac);
                Some(O::NullPointerException)
            }
            2 => {
                code.push(0x01);
                code.extend(op(0xb4, missing));
                code.push(0xac);
                Some(O::NoSuchFieldError)
            }
            3 => {
                code.extend([0x01, 0x03]);
                code.extend(op(0xb5, final_field));
                code.push(0xb1);
                Some(O::IllegalAccessError)
            }
            4 => {
                code.push(0x01);
                code.extend(op(0xb4, static_field));
                code.push(0xac);
                Some(O::IncompatibleClassChangeError)
            }
            5 => {
                code.extend([0x03, 0xbc, 10, 0x03, 0x2e, 0xac]);
                Some(O::ArrayIndexOutOfBoundsException)
            }
            6 => {
                code.extend([0x02, 0xbc, 10, 0x57, 0xb1]);
                Some(O::NegativeArraySizeException)
            }
            7 => {
                code.extend([0xc2, 0xb1]);
                None
            }
            8 => {
                code.push(0x01);
                code.extend(op(0xb4, 1));
                code.push(0xac);
                None
            }
            _ => {
                code.extend([0x03, 0xbe, 0xac]);
                None
            }
        };
        class.method("m", "()I", 1, &code, 3, 1);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()I");
        let result = fixture.run(method, receiver, &[]);
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected, "failure {failure}");
        } else {
            assert!(result.is_err(), "failure {failure}");
        }
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(7));
        assert_eq!(
            fixture.heap.allocated_arrays(),
            if failure == 5 { 2 } else { 1 }
        );
        assert_eq!(fixture.heap.allocated_instances(), 1);
    }
}
#[test]
fn unsupported_instruction_families_stay_explicit_and_all_code_is_eagerly_decoded() {
    for opcode in [0xb2, 0xb3, 0xbb, 0xbd, 0xb0, 0xbf, 0xc2, 0xc3] {
        let mut class = ordinary();
        let index = class.class("p/T");
        let mut code = if matches!(opcode, 0xb2 | 0xb3 | 0xb6 | 0xb7 | 0xbb | 0xbd) {
            op(opcode, index)
        } else {
            vec![opcode]
        };
        code.push(0xb1);
        class.method("m", "()V", 1, &code, 0, 1);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::UnsupportedInstruction(opcode)
        );
    }
    // invokevirtual is now enabled in runtime mode, so a Class constant operand
    // reaches its structural Methodref guard instead of the old opcode gate.
    let mut class = ordinary();
    let index = class.class("p/T");
    let mut code = vec![0x2a];
    code.extend(op(0xb6, index));
    code.push(0xb1);
    class.method("m", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()V");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,crabspot_runtime::MethodAccessError::InvalidConstant {..}))
    );
    // Ordinary invokespecial is enabled only in runtime mode; an invalid CP kind
    // now reaches its Methodref guard rather than the old opcode boundary.
    let mut class = ordinary();
    let index = class.class("p/T");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, index));
    code.push(0xb1);
    class.method("special", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "special", "()V");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::MethodAccess(error) if matches!(*error,crabspot_runtime::MethodAccessError::InvalidConstant {..}))
    );
    for code in [
        vec![0xb1, 0xff],
        vec![0xb1, 0xb4, 0, 0],
        vec![0xb1, 0xb5, 0],
    ] {
        let mut class = ordinary();
        class.method("m", "()V", 1, &code, 0, 1);
        let mut fixture = Fixture::new(vec![class]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "m", "()V");
        assert!(matches!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::Decode(_)
        ));
    }
}

#[test]
fn wide_reference_locals_and_static_helper_descriptor_restrictions_share_existing_rules() {
    let mut class = ordinary();
    class.field("f", "I", 1);
    let field = class.reference(9, "p/T", "f", "I");
    let mut code = vec![0x2a, 0xc4, 0x3a, 1, 44, 0xc4, 0x19, 1, 44];
    code.extend(op(0xb4, field));
    code.push(0xac);
    class.method("wide", "()I", 1, &code, 1, 301);
    let call = class.reference(10, "p/T", "helper", "()Ljava/lang/Object;");
    let mut code = op(0xb8, call);
    code.push(0xb1);
    class.method("m", "()V", 1, &code, 1, 1);
    class.method("helper", "()Ljava/lang/Object;", 9, &[0x01, 0xb0], 1, 0);
    let mut fixture = Fixture::new(vec![class]);
    let receiver = fixture.object("p/T");
    let wide = fixture.method("p/T", "wide", "()I");
    assert_eq!(
        fixture.run(wide, receiver, &[]).unwrap(),
        returned(Value::Int(0))
    );
    let method = fixture.method("p/T", "m", "()V");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(error.kind, E::UnsupportedDescriptor);
    assert_eq!(
        error.call_site.as_ref().unwrap().method.index,
        method.index()
    );
}
#[test]
fn field_resolution_policy_exhaustion_remains_host_error_and_preserves_completed_store() {
    let mut class = ordinary();
    class.field("f", "I", 1);
    let field = class.reference(9, "p/T", "f", "I");
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb4, field));
    code.push(0xac);
    class.method("m", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![class]);
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
        matches!(error.kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::ResourceLimit {resource:crabspot_runtime::FieldResource::LookupSteps,limit,..} if limit==budget))
    );
    let field_id = fixture
        .registry
        .find_declared_field(owner, &units("f"), &units("I"))
        .unwrap()
        .unwrap();
    assert_eq!(
        context.heap().get_field(receiver, field_id).unwrap(),
        Value::Int(7)
    );
}
#[test]
fn context_layout_extension_and_unsupported_private_resolution_do_not_become_java_outcomes() {
    let mut class = ordinary();
    let field = class.reference(9, "q/Owner", "f", "I");
    let mut code = vec![0x01];
    code.extend(op(0xb4, field));
    code.push(0xac);
    class.method("m", "()I", 1, &code, 1, 1);
    let mut owner = Class::new("q/Owner", Some("java/lang/Object"), 0x21);
    owner.field("f", "I", 2);
    let mut fixture = Fixture::new(vec![class, owner]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "m", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::UnsupportedNestAccess {..}))
    );
    let root = build_hierarchy(
        &fixture.registry,
        fixture.classes["p/T"],
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    let extra = build_hierarchy(
        &fixture.registry,
        fixture.classes["q/Owner"],
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap();
    fixture.heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    fixture
        .heap
        .admit_instance_layouts(&fixture.registry, &[&root])
        .unwrap();
    let receiver = fixture.object("p/T");
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    context
        .heap_mut()
        .admit_instance_layouts(&fixture.registry, &[&extra])
        .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &[],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap_err()
        .kind,
        E::FieldAccess(Box::new(FieldAccessError::ContextChanged))
    );
}

#[test]
fn field_backed_loop_branches_and_instruction_budget_count_each_get_and_put_once() {
    let mut class = ordinary();
    class.field("f", "I", 1);
    let field = class.reference(9, "p/T", "f", "I");
    let mut code = vec![0x2a, 0x59];
    code.extend(op(0xb4, field));
    code.extend([0x04, 0x60]);
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb4, field));
    code.extend([0x1b, 0xa1, 0xff, 0xf1, 0x2a]);
    code.extend(op(0xb4, field));
    code.push(0xac);
    class.method("m", "(I)I", 1, &code, 3, 2);
    let mut fixture = Fixture::new(vec![class]);
    let method = fixture.method("p/T", "m", "(I)I");
    for count in [1, 2, 50] {
        let receiver = fixture.object("p/T");
        let budget = 10 * count as u64 + 3;
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[Value::Int(count)],
                    ExecutionOptions {
                        instruction_budget: budget
                    },
                    CallLimits::default()
                )
                .unwrap(),
            returned(Value::Int(count))
        );
        let receiver = fixture.object("p/T");
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[Value::Int(count)],
                    ExecutionOptions {
                        instruction_budget: budget - 1
                    },
                    CallLimits::default()
                )
                .unwrap_err()
                .kind,
            E::InstructionBudgetExhausted { limit: budget - 1 }
        );
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(count));
    }
}

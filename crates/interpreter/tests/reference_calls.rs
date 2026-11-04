//! Checked reference parameters/results through the existing body graph.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_instance_method_body, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ArrayElement, ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext,
    FieldResolutionOptions, HeapError, HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap,
    MethodAccessError, MethodId, MethodValuePosition, PrimitiveArrayKind, Reference, ReferenceType,
    ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver, TypeAccessError, build_hierarchy,
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
        Self::configured(classes, true)
    }
    fn unbound(classes: Vec<Class>) -> Self {
        Self::configured(classes, false)
    }
    fn configured(classes: Vec<Class>, bind: bool) -> Self {
        Self::configured_arrays(classes, bind, HeapOptions::default())
    }
    fn configured_arrays(mut classes: Vec<Class>, bind: bool, array_options: HeapOptions) -> Self {
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
#[test]
fn host_roots_transfer_null_self_other_objects_and_arrays_without_changing_identity() {
    let mut root = ordinary();
    root.method(
        "identity",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        1,
        &[0x2b, 0xb0],
        1,
        2,
    );
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("p/T");
    let array = fixture
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Double, 2)
        .unwrap();
    let method = fixture.method("p/T", "identity", "(Ljava/lang/Object;)Ljava/lang/Object;");
    for reference in [Reference::null(), receiver, other, array] {
        assert_eq!(
            fixture
                .run(method, receiver, &[Value::Reference(reference)])
                .unwrap(),
            returned(Value::Reference(reference))
        );
    }
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    assert_eq!(fixture.heap.allocated_instances(), 2);
}
#[test]
fn all_null_missing_nominal_types_work_without_universe_but_full_signatures_are_charged() {
    let mut root = ordinary();
    root.method(
        "identity",
        "(Lq/Missing;)Lq/OtherMissing;",
        1,
        &[0x2b, 0xb0],
        1,
        2,
    );
    let mut fixture = Fixture::unbound(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "identity", "(Lq/Missing;)Lq/OtherMissing;");
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let symbols = context.resolved_symbols();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &[Value::Reference(Reference::null())],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        returned(Value::Reference(Reference::null()))
    );
    assert!(context.lookup_steps() >= 4 * "(Lq/Missing;)Lq/OtherMissing;".len());
    assert_eq!(context.resolved_symbols(), symbols);
}
#[test]
fn mixed_width_reference_arguments_keep_receiver_slot_zero_and_original_indices() {
    let mut root = ordinary();
    let descriptor = "(JLjava/lang/Object;DLjava/lang/Object;I)Ljava/lang/Object;";
    root.method("first", descriptor, 1, &[0x2d, 0xb0], 1, 8);
    root.method("second", descriptor, 1, &[0x19, 6, 0xb0], 1, 8);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("p/T");
    let args = [
        Value::Long(i64::MIN),
        Value::Reference(other),
        Value::Double(0xfff8000000000042),
        Value::Reference(receiver),
        Value::Int(99),
    ];
    for (name, reference) in [("first", other), ("second", receiver)] {
        let method = fixture.method("p/T", name, descriptor);
        assert_eq!(
            fixture.run(method, receiver, &args).unwrap(),
            returned(Value::Reference(reference))
        );
    }
    let method = fixture.method("p/T", "first", descriptor);
    let mut bad = args;
    bad[1] = Value::Int(1);
    assert_eq!(
        fixture.run(method, receiver, &bad).unwrap_err().kind,
        E::ArgumentType {
            index: 1,
            expected: ValueKind::Reference,
            actual: ValueKind::Int
        }
    );
}
#[test]
fn same_class_static_virtual_interface_private_and_special_paths_share_actual_handles() {
    let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let mut interface = iface("q/I");
    interface.method("defaultIdentity", descriptor, 1, &[0x2b, 0xb0], 1, 2);
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("baseIdentity", descriptor, 1, &[0x2b, 0xb0], 1, 2);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    root.method("virtualIdentity", descriptor, 1, &[0x2b, 0xb0], 1, 2);
    root.method("privateIdentity", descriptor, 2, &[0x2b, 0xb0], 1, 2);
    root.method("staticIdentity", descriptor, 9, &[0x2a, 0xb0], 1, 1);
    for (name, opcode, owner, body) in [
        ("virtual", 0xb6, "p/T", "virtualIdentity"),
        ("private", 0xb6, "p/T", "privateIdentity"),
        ("special", 0xb7, "q/Base", "baseIdentity"),
        ("interface", 0xb9, "q/I", "defaultIdentity"),
        ("static", 0xb8, "p/T", "staticIdentity"),
    ] {
        let call = root.reference(
            if opcode == 0xb9 { 11 } else { 10 },
            owner,
            body,
            descriptor,
        );
        let mut code = if opcode == 0xb8 {
            vec![0x2b]
        } else {
            vec![0x2a, 0x2b]
        };
        if opcode == 0xb9 {
            code.extend(op(0xb9, call));
            code.extend([2, 0]);
        } else {
            code.extend(op(opcode, call));
        }
        code.push(0xb0);
        root.method(name, descriptor, 1, &code, 2, 2);
    }
    let mut fixture = Fixture::new(vec![root, base, interface]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("p/T");
    for name in ["virtual", "private", "special", "interface", "static"] {
        let method = fixture.method("p/T", name, descriptor);
        for reference in [Reference::null(), receiver, other] {
            assert_eq!(
                fixture
                    .run(method, receiver, &[Value::Reference(reference)])
                    .unwrap(),
                returned(Value::Reference(reference))
            );
        }
    }
}
#[test]
fn actual_covariant_array_arguments_and_results_keep_complete_nominal_and_primitive_ranks() {
    let mut root = ordinary();
    for (name, descriptor) in [
        ("objects", "([Ljava/lang/Object;)[Ljava/lang/Object;"),
        ("nested", "([[I)[[I"),
        ("marker", "(Ljava/lang/Cloneable;)Ljava/io/Serializable;"),
    ] {
        root.method(name, descriptor, 1, &[0x2b, 0xb0], 1, 2);
    }
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
    let loader = fixture.registry.defining_loader(owner).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let object_array = context
        .allocate_reference_array(ReferenceType::Class(owner), 2)
        .unwrap();
    let nested = context
        .allocate_reference_array(
            ReferenceType::Array {
                dimensions: 1,
                element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
                defining_loader: loader,
            },
            2,
        )
        .unwrap();
    drop(context);
    for (name, descriptor, array) in [
        (
            "objects",
            "([Ljava/lang/Object;)[Ljava/lang/Object;",
            object_array,
        ),
        ("nested", "([[I)[[I", nested),
        (
            "marker",
            "(Ljava/lang/Cloneable;)Ljava/io/Serializable;",
            nested,
        ),
    ] {
        let method = fixture.method("p/T", name, descriptor);
        assert_eq!(
            fixture
                .run(method, receiver, &[Value::Reference(array)])
                .unwrap(),
            returned(Value::Reference(array))
        );
    }
}
#[test]
fn argument_mismatch_missing_universe_foreign_handle_and_kind_errors_never_become_java() {
    let mut root = ordinary();
    root.method("typed", "(Lp/T;)Lp/T;", 1, &[0x2b, 0xb0], 1, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("java/lang/Object");
    let array = fixture
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let method = fixture.method("p/T", "typed", "(Lp/T;)Lp/T;");
    for reference in [other, array] {
        let error = fixture
            .run(method, receiver, &[Value::Reference(reference)])
            .unwrap_err();
        assert!(
            matches!(error.kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::IncompatibleReference(mismatch) if mismatch.method==method&&mismatch.position==MethodValuePosition::Argument(0)))
        );
    }
    assert!(
        matches!(fixture.run(method,receiver,&[Value::Reference(foreign)]).unwrap_err().kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::Heap(HeapError::ForeignHeap))))
    );
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Int(1)])
            .unwrap_err()
            .kind,
        E::ArgumentType {
            index: 0,
            expected: ValueKind::Reference,
            actual: ValueKind::Int
        }
    );
    let mut root = ordinary();
    root.method("typed", "(Lp/T;)Lp/T;", 1, &[0x2b, 0xb0], 1, 2);
    let mut unbound = Fixture::unbound(vec![root]);
    let receiver = unbound.object("p/T");
    let method = unbound.method("p/T", "typed", "(Lp/T;)Lp/T;");
    assert!(
        matches!(unbound.run(method,receiver,&[Value::Reference(receiver)]).unwrap_err().kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::MissingUniverse)))
    );
}

#[test]
fn areturn_uses_selected_result_identity_and_reports_failures_before_frame_removal() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let descriptor = "(Ljava/lang/Object;)Lq/Base;";
    base.field("touched", "I", 1);
    let field = base.reference(9, "q/Base", "touched", "I");
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, field));
    code.extend([0x2b, 0xb0]);
    base.method("body", descriptor, 1, &code, 2, 2);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "body", descriptor);
    let mut code = vec![0x2a, 0x2b];
    code.extend(op(0xb7, call));
    code.push(0xb0);
    root.method("run", descriptor, 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![root, base]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("java/lang/Object");
    let method = fixture.method("p/T", "run", descriptor);
    let error = fixture
        .run(method, receiver, &[Value::Reference(other)])
        .unwrap_err();
    assert_eq!(error.offset, Some(7));
    assert_eq!(
        error.method.as_ref().unwrap().owner,
        Some(fixture.classes["q/Base"])
    );
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(fixture.classes["p/T"])
    );
    assert_eq!(error.call_site.as_ref().unwrap().offset, 2);
    assert!(
        matches!(error.kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::IncompatibleReference(mismatch) if mismatch.position==MethodValuePosition::Return))
    );
    assert_eq!(
        fixture.stored("q/Base", "touched", "I", receiver),
        Value::Int(7)
    );
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Reference(receiver)])
            .unwrap(),
        returned(Value::Reference(receiver))
    );
    let separate_base = fixture.object("q/Base");
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Reference(separate_base)])
            .unwrap(),
        returned(Value::Reference(separate_base))
    );
}
#[test]
fn primitive_and_reference_return_opcodes_keep_descriptor_and_computational_guards() {
    for (descriptor, code, expected) in [
        (
            "()Ljava/lang/Object;",
            vec![0x03, 0xac],
            E::ReturnTypeMismatch,
        ),
        ("()Ljava/lang/Object;", vec![0xb1], E::ReturnTypeMismatch),
        ("()I", vec![0x01, 0xb0], E::ReturnTypeMismatch),
        ("()V", vec![0x01, 0xb0], E::ReturnTypeMismatch),
        (
            "()Ljava/lang/Object;",
            vec![0x03, 0xb0],
            E::WrongValueType {
                expected: ValueKind::Reference,
                actual: ValueKind::Int,
            },
        ),
        ("()Ljava/lang/Object;", vec![0xb0], E::StackUnderflow),
    ] {
        let mut root = ordinary();
        root.method("run", descriptor, 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", descriptor);
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            expected
        );
    }
    let mut root = ordinary();
    root.method("run", "()Lp/T;", 1, &[0x04, 0x2a, 0xb0], 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()Lp/T;");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Reference(receiver))
    );
}
#[test]
fn returned_reference_transfer_overflow_identifies_receiving_caller_and_invocation() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let helper = base.reference(10, "q/Base", "helper", "()Ljava/lang/Object;");
    let mut code = vec![0x03];
    code.extend(op(0xb8, helper));
    code.push(0xb0);
    base.method("body", "()Ljava/lang/Object;", 1, &code, 1, 1);
    base.method("helper", "()Ljava/lang/Object;", 9, &[0x01, 0xb0], 1, 0);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "body", "()Ljava/lang/Object;");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, call));
    code.push(0xb0);
    root.method("run", "()Ljava/lang/Object;", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root, base]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()Ljava/lang/Object;");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(
        error.kind,
        E::StackOverflow {
            required: 2,
            capacity: 1
        }
    );
    assert_eq!(error.offset, Some(1));
    let body = fixture.method("q/Base", "body", "()Ljava/lang/Object;");
    assert_eq!(error.method.as_ref().unwrap().owner, Some(body.owner()));
    assert_eq!(error.method.as_ref().unwrap().index, body.index());
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(body.owner())
    );
    assert_eq!(error.call_site.as_ref().unwrap().method.index, body.index());
    assert_eq!(error.call_site.as_ref().unwrap().offset, 1);
}
#[test]
fn recursive_reference_identity_uses_shared_frames_cache_and_exact_limits() {
    let mut root = ordinary();
    let descriptor = "(ILjava/lang/Object;)Ljava/lang/Object;";
    let call = root.reference(10, "p/T", "recurse", descriptor);
    let mut code = vec![0x1b, 0x99, 0, 13, 0x2a, 0x1b, 0x04, 0x64, 0x2c];
    code.extend(op(0xb6, call));
    code.extend([0xb0, 0x00, 0x2c, 0xb0]);
    root.method("recurse", descriptor, 1, &code, 3, 3);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("p/T");
    let method = fixture.method("p/T", "recurse", descriptor);
    let args = [Value::Int(3), Value::Reference(other)];
    let limits = CallLimits {
        max_frames: 4,
        max_frame_slots: 24,
        max_prepared_code_bytes: code.len(),
    };
    assert_eq!(
        fixture
            .run_limits(method, receiver, &args, ExecutionOptions::default(), limits)
            .unwrap(),
        returned(Value::Reference(other))
    );
    assert!(matches!(
        fixture
            .run_limits(
                method,
                receiver,
                &args,
                ExecutionOptions::default(),
                CallLimits {
                    max_frames: 3,
                    ..limits
                }
            )
            .unwrap_err()
            .kind,
        E::FrameLimit { required: 4, .. }
    ));
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &args,
                ExecutionOptions::default(),
                CallLimits {
                    max_frame_slots: 23,
                    ..limits
                }
            )
            .unwrap_err()
            .kind,
        E::FrameSlotLimit {
            limit: 23,
            required: 24
        }
    );
}
#[test]
fn long_shared_descriptor_small_bodies_cache_only_compact_shapes() {
    let mut root = ordinary();
    let descriptor = format!("()L{};", "x".repeat(65000));
    let shared = root.utf8(&descriptor);
    let mut code = Vec::new();
    for index in 0..4 {
        let name = root.utf8(&format!("helper{index}"));
        let code_name = root.utf8("Code");
        root.methods.push(Method {
            flags: 9,
            name,
            descriptor: shared,
            code_name,
            code: Some(vec![0x01, 0xb0]),
            stack: 1,
            locals: 0,
            handler: false,
        });
        let mut data = Vec::new();
        u2(&mut data, name);
        u2(&mut data, shared);
        let nat = root.cp(12, &data);
        let mut data = Vec::new();
        u2(&mut data, root.this);
        u2(&mut data, nat);
        let call = root.cp(10, &data);
        code.extend(op(0xb8, call));
        code.push(if index == 3 { 0xb0 } else { 0x57 });
    }
    root.method("run", "()Ljava/lang/Object;", 1, &code, 1, 1);
    let mut fixture = Fixture::unbound(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()Ljava/lang/Object;");
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: 2_000_000,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    let symbols = context.resolved_symbols();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &[],
            ExecutionOptions::default(),
            CallLimits {
                max_frames: 2,
                max_frame_slots: 3,
                max_prepared_code_bytes: 24
            }
        )
        .unwrap(),
        returned(Value::Reference(Reference::null()))
    );
    assert!(context.lookup_steps() >= 16 * descriptor.len());
    assert_eq!(context.resolved_symbols(), symbols);
}
#[test]
fn old_modes_reject_reference_signatures_and_areturn_without_reference_validation() {
    let mut root = ordinary();
    root.method(
        "reference",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
        9,
        &[0x2a, 0xb0],
        1,
        1,
    );
    root.method("numeric", "()V", 9, &[0xb0], 0, 0);
    let mut fixture = Fixture::new(vec![root]);
    let class = fixture.registry.class(fixture.classes["p/T"]).unwrap();
    for index in 0..2 {
        for error in [
            execute_static_method_body(class, index, &[], ExecutionOptions::default()).unwrap_err(),
            execute_static_method_body_with_heap(
                class,
                index,
                &[],
                ExecutionOptions::default(),
                CallLimits::default(),
                &mut fixture.heap,
            )
            .unwrap_err(),
        ] {
            assert_eq!(
                error.kind,
                if index == 0 {
                    E::UnsupportedDescriptor
                } else {
                    E::UnsupportedInstruction(0xb0)
                }
            );
        }
    }
}

#[test]
fn array_owner_rejection_follows_exact_typed_extraction_errors() {
    for (prefix, name, expected_stack) in [
        (vec![], "v", Some(E::StackUnderflow)),
        (
            vec![0x01, 0x03],
            "v",
            Some(E::WrongValueType {
                expected: ValueKind::Reference,
                actual: ValueKind::Int,
            }),
        ),
        (vec![0x01, 0x01], "v", None),
    ] {
        let mut root = ordinary();
        let call = root.reference(10, "[I", name, "(Ljava/lang/Object;)V");
        let mut code = prefix;
        code.extend(op(0xb6, call));
        code.push(0xb1);
        root.method("run", "()V", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        if let Some(expected) = expected_stack {
            assert_eq!(error.kind, expected);
        } else {
            assert!(
                matches!(error.kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::Context(FieldAccessError::UnsupportedArrayOwner)))
            );
        }
    }
}
#[test]
fn inaccessible_parameter_and_result_types_use_declarer_identity_without_caller_access() {
    let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let descriptor = "(Lq/Hidden;)Lq/Hidden;";
    base.method("identity", descriptor, 1, &[0x2b, 0xb0], 1, 2);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "identity", descriptor);
    let mut code = vec![0x2a, 0x2b];
    code.extend(op(0xb6, call));
    code.push(0xb0);
    root.method("run", descriptor, 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![hidden, base, root]);
    let receiver = fixture.object("p/T");
    let value = fixture.object("q/Hidden");
    let method = fixture.method("p/T", "run", descriptor);
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Reference(value)])
            .unwrap(),
        returned(Value::Reference(value))
    );
}

#[test]
fn one_selected_interface_body_reuses_code_across_b6_b7_b9_and_preserves_cache_mode_guards() {
    let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let mut interface = iface("q/I");
    interface.method("identity", descriptor, 1, &[0x2b, 0xb0], 1, 2);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    let mut code = Vec::new();
    for (opcode, owner) in [(0xb6, "p/T"), (0xb7, "q/I"), (0xb9, "q/I")] {
        let call = root.reference(
            if opcode == 0xb6 { 10 } else { 11 },
            owner,
            "identity",
            descriptor,
        );
        code.extend([0x2a, 0x2b]);
        code.extend(op(opcode, call));
        if opcode == 0xb9 {
            code.extend([2, 0]);
        }
        code.push(if opcode == 0xb9 { 0xb0 } else { 0x57 });
    }
    let bytes = code.len() + 2;
    root.method("run", descriptor, 1, &code, 2, 2);
    let root_call = root.reference(10, "p/T", "mode", descriptor);
    let mut mode = vec![0x2b];
    mode.extend(op(0xb8, root_call));
    mode.push(0xb0);
    root.method("mode", descriptor, 1, &mode, 1, 2);
    let mut fixture = Fixture::new(vec![interface, root]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("p/T");
    let method = fixture.method("p/T", "run", descriptor);
    let limits = CallLimits {
        max_frames: 2,
        max_frame_slots: 7,
        max_prepared_code_bytes: bytes,
    };
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[Value::Reference(other)],
                ExecutionOptions::default(),
                limits
            )
            .unwrap(),
        returned(Value::Reference(other))
    );
    assert!(
        matches!(fixture.run_limits(method,receiver,&[Value::Reference(other)],ExecutionOptions::default(),CallLimits{max_prepared_code_bytes:bytes-1,..limits}).unwrap_err().kind,E::PreparedCodeLimit{required,..} if required==bytes)
    );
    let method = fixture.method("p/T", "mode", descriptor);
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Reference(other)])
            .unwrap_err()
            .kind,
        E::UnsupportedMethod("method must be static")
    );
}
#[test]
fn root_computational_frame_and_receiver_prefix_precedes_reference_values_and_body_effects() {
    let mut root = ordinary();
    root.field("touch", "I", 1);
    let field = root.reference(9, "p/T", "touch", "I");
    let mut code = vec![0x2a, 0x04];
    code.extend(op(0xb5, field));
    code.extend([0x2b, 0xb0]);
    let descriptor = "(Lp/T;)Lp/T;";
    root.method("run", descriptor, 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("java/lang/Object");
    let method = fixture.method("p/T", "run", descriptor);
    assert_eq!(
        fixture
            .run(method, Reference::null(), &[])
            .unwrap_err()
            .kind,
        E::ArgumentCount {
            expected: 1,
            actual: 0
        }
    );
    let error = fixture
        .run_limits(
            method,
            Reference::null(),
            &[Value::Reference(other)],
            ExecutionOptions::default(),
            CallLimits {
                max_frame_slots: 0,
                ..CallLimits::default()
            },
        )
        .unwrap_err();
    assert!(matches!(error.kind, E::FrameSlotLimit { .. }));
    assert!(matches!(
        fixture
            .run(method, Reference::null(), &[Value::Reference(other)])
            .unwrap_err()
            .kind,
        E::FieldAccess(_)
    ));
    assert!(
        matches!(fixture.run(method,receiver,&[Value::Reference(other)]).unwrap_err().kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::IncompatibleReference(_)))
    );
    assert_eq!(fixture.stored("p/T", "touch", "I", receiver), Value::Int(0));
}
#[test]
fn callsite_kinds_and_interface_count_precede_resolution_and_selected_argument_validation() {
    for failure in 0..3 {
        let mut interface = iface("q/I");
        interface.method(
            "identity",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            1,
            &[0x2b, 0xb0],
            1,
            2,
        );
        let mut root = ordinary();
        implement(&mut root, "q/I");
        let call = root.reference(
            11,
            "q/I",
            "identity",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
        );
        let mut code = match failure {
            0 => Vec::new(),
            1 => vec![0x2a, 0x03],
            _ => vec![0x01, 0x01],
        };
        code.extend(op(0xb9, call));
        code.extend([if failure == 0 { 1 } else { 2 }, 0, 0xb0]);
        root.method("run", "()Ljava/lang/Object;", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()Ljava/lang/Object;");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap_err().kind, E::InvalidOperands),
            1 => assert_eq!(
                result.unwrap_err().kind,
                E::WrongValueType {
                    expected: ValueKind::Reference,
                    actual: ValueKind::Int
                }
            ),
            _ => assert_eq!(result.unwrap(), O::NullPointerException),
        }
    }
}

#[test]
fn return_signature_work_failure_is_at_return_offset_and_keeps_prior_field_effects() {
    let mut root = ordinary();
    root.field("touch", "I", 1);
    let field = root.reference(9, "p/T", "touch", "I");
    let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, field));
    code.extend([0x2b, 0xb0]);
    root.method("run", descriptor, 1, &code, 2, 2);
    let mut fixture = Fixture::unbound(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", descriptor);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &[Value::Reference(Reference::null())],
            ExecutionOptions::default(),
            CallLimits::default()
        )
        .unwrap(),
        returned(Value::Reference(Reference::null()))
    );
    let exact = context.lookup_steps();
    drop(context);
    let touch = fixture
        .registry
        .find_declared_field(fixture.classes["p/T"], &units("touch"), &units("I"))
        .unwrap()
        .unwrap();
    fixture
        .heap
        .put_field(receiver, touch, Value::Int(0))
        .unwrap();
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
    let error = execute_instance_method_body(
        &mut context,
        method,
        receiver,
        &[Value::Reference(Reference::null())],
        ExecutionOptions::default(),
        CallLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.offset, Some(7));
    assert_eq!(error.method.as_ref().unwrap().owner, Some(method.owner()));
    assert!(
        matches!(error.kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::Context(FieldAccessError::ResourceLimit{resource:crabspot_runtime::FieldResource::LookupSteps,..})))
    );
    assert_eq!(
        context.heap().get_field(receiver, touch).unwrap(),
        Value::Int(7)
    );
}
#[test]
fn deep_primitive_array_reference_recursion_remains_iterative_and_uses_one_prepared_body() {
    let mut root = ordinary();
    let descriptor = "(I[I)[I";
    let call = root.reference(10, "p/T", "recurse", descriptor);
    let mut code = vec![0x1b, 0x99, 0, 13, 0x2a, 0x1b, 0x04, 0x64, 0x2c];
    code.extend(op(0xb6, call));
    code.extend([0xb0, 0x00, 0x2c, 0xb0]);
    root.method("recurse", descriptor, 1, &code, 3, 3);
    let mut fixture = Fixture::unbound(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "recurse", descriptor);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: 5_000_000,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &[Value::Int(10_000), Value::Reference(Reference::null())],
            ExecutionOptions::default(),
            CallLimits {
                max_frames: 10_001,
                max_frame_slots: 60_006,
                max_prepared_code_bytes: code.len()
            }
        )
        .unwrap(),
        returned(Value::Reference(Reference::null()))
    );
    assert_eq!(context.heap().allocated_arrays(), 0);
}

#[test]
fn nominal_host_profile_and_symbolic_loader_boundaries_apply_even_for_null_values() {
    struct Delegation(HashMap<Vec<u16>, ClassId>);
    impl crabspot_runtime::HierarchyResolver for Delegation {
        fn resolve(
            &mut self,
            _: &DefinitionRegistry,
            _: crabspot_runtime::LoaderId,
            name: &[u16],
        ) -> Result<Option<ClassId>, crabspot_runtime::ResolveError> {
            Ok(self.0.get(name).copied())
        }
    }
    for host_profile in [true, false] {
        let descriptor = "(Ljava/lang/Object;)V";
        let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        base.method("accept", descriptor, 1, &[0xb1], 0, 2);
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        let method_name = if host_profile {
            root.method("accept", descriptor, 1, &[0xb1], 0, 2);
            "accept"
        } else {
            let call = root.reference(10, "q/Base", "accept", descriptor);
            let mut code = vec![0x01, 0x2b];
            code.extend(op(0xb6, call));
            code.push(0xb1);
            root.method("run", descriptor, 1, &code, 2, 2);
            "run"
        };
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let first = registry.create_loader().unwrap();
        let second = registry.create_loader().unwrap();
        let object = registry
            .define(
                first,
                &units("java/lang/Object"),
                &Class::new("java/lang/Object", None, 0x21).bytes(),
                ParseOptions::default(),
            )
            .unwrap();
        let base = registry
            .define(
                second,
                &units("q/Base"),
                &base.bytes(),
                ParseOptions::default(),
            )
            .unwrap();
        let root = registry
            .define(first, &units("p/T"), &root.bytes(), ParseOptions::default())
            .unwrap();
        let classes = HashMap::from([
            ("java/lang/Object".to_string(), object),
            ("q/Base".to_string(), base),
            ("p/T".to_string(), root),
        ]);
        let mut resolver = Delegation(
            classes
                .iter()
                .map(|(name, id)| (units(name), *id))
                .collect(),
        );
        let plan =
            build_hierarchy(&registry, root, &mut resolver, HierarchyOptions::default()).unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        let mut fixture = Fixture {
            registry,
            heap,
            classes,
        };
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", method_name, descriptor);
        let error = fixture
            .run(method, receiver, &[Value::Reference(Reference::null())])
            .unwrap_err();
        if host_profile {
            assert!(
                matches!(error.kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::UnsupportedOverrideConstraint{method:selected,declaration} if *selected==method&&declaration.owner()==base))
            );
        } else {
            assert!(
                matches!(error.kind,E::MethodAccess(error) if matches!(error.as_ref(),MethodAccessError::UnsupportedLoadingConstraint{origin,method:resolved} if *origin==root&&resolved.owner()==base))
            );
            assert_eq!(error.offset, Some(2));
        }
    }
}

#[test]
fn returned_aliases_and_cycles_keep_exact_edges_with_inclusive_fuel_and_frame_policies() {
    let mut root = ordinary();
    root.field("link", "Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "link", "Ljava/lang/Object;");
    let descriptor = "(Ljava/lang/Object;)Ljava/lang/Object;";
    let mut code = vec![0x2a, 0x2b];
    code.extend(op(0xb5, field));
    code.extend([0x2b, 0xb0]);
    root.method("link", descriptor, 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![root]);
    let first = fixture.object("p/T");
    let second = fixture.object("p/T");
    let method = fixture.method("p/T", "link", descriptor);
    let limits = CallLimits {
        max_frames: 1,
        max_frame_slots: 4,
        max_prepared_code_bytes: code.len(),
    };
    assert_eq!(
        fixture
            .run_limits(
                method,
                first,
                &[Value::Reference(second)],
                ExecutionOptions {
                    instruction_budget: 5
                },
                limits
            )
            .unwrap(),
        returned(Value::Reference(second))
    );
    assert_eq!(
        fixture
            .run_limits(
                method,
                second,
                &[Value::Reference(first)],
                ExecutionOptions {
                    instruction_budget: 3
                },
                limits
            )
            .unwrap_err()
            .kind,
        E::InstructionBudgetExhausted { limit: 3 }
    );
    assert_eq!(
        fixture.heap.children(first).unwrap().collect::<Vec<_>>(),
        vec![second.heap_ref().unwrap()]
    );
    assert_eq!(
        fixture.heap.children(second).unwrap().collect::<Vec<_>>(),
        vec![first.heap_ref().unwrap()]
    );
    assert_eq!(
        fixture
            .run(method, first, &[Value::Reference(Reference::null())])
            .unwrap(),
        returned(Value::Reference(Reference::null()))
    );
    assert_eq!(fixture.heap.children(first).unwrap().count(), 0);
    assert_eq!(fixture.heap.allocated_instances(), 2);
    assert_eq!(fixture.heap.allocated_arrays(), 0);
}

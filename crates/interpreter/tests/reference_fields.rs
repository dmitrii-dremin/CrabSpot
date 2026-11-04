//! Checked non-null reference fields through the existing putfield engine.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_instance_method_body, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ArrayElement, ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext,
    FieldResolutionOptions, HeapError, HeapOptions, HierarchyOptions, InstanceError,
    InstanceOptions, ManagedHeap, MethodId, PrimitiveArrayKind, Reference, ReferenceFieldMismatch,
    ReferenceType, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver, TypeAccessError,
    build_hierarchy,
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
    fn configured(mut classes: Vec<Class>, bind: bool) -> Self {
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
fn interface_call(index: u16) -> Vec<u8> {
    let mut code = op(0xb9, index);
    code.extend([1, 0]);
    code
}
fn field_id(
    fixture: &Fixture,
    class: &str,
    name: &str,
    descriptor: &str,
) -> crabspot_runtime::FieldId {
    fixture
        .registry
        .find_declared_field(fixture.classes[class], &units(name), &units(descriptor))
        .unwrap()
        .unwrap()
}
fn seed(fixture: &mut Fixture, owner: &str, index: u16, receiver: Reference, value: Value) {
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    context
        .write_field(fixture.classes[owner], index, receiver, value)
        .unwrap();
}
fn mismatch(error: &ExecutionError) -> &ReferenceFieldMismatch {
    let E::FieldAccess(field) = &error.kind else {
        panic!("not a field failure: {error:?}")
    };
    let FieldAccessError::IncompatibleReference(mismatch) = field.as_ref() else {
        panic!("not an assignment failure: {error:?}")
    };
    mismatch
}
#[test]
fn self_links_assign_to_current_super_interface_and_object_types_without_raw_store_bypass() {
    for descriptor in ["Lp/T;", "Lq/Base;", "Lq/I;", "Ljava/lang/Object;"] {
        let base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
        let interface = iface("q/I");
        let mut root = Class::new("p/T", Some("q/Base"), 0x21);
        implement(&mut root, "q/I");
        root.field("link", descriptor, 1);
        let field = root.reference(9, "p/T", "link", descriptor);
        let mut code = vec![0x2a, 0x2a];
        code.extend(op(0xb5, field));
        code.push(0x2a);
        code.extend(op(0xb4, field));
        code.extend([0x2a, 0xa5, 0, 5, 0x03, 0xac, 0x04, 0xac]);
        root.method("run", "()I", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![base, interface, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(1))
        );
        assert_eq!(
            fixture.stored("p/T", "link", descriptor, receiver),
            Value::Reference(receiver)
        );
        assert_eq!(
            fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
            vec![receiver.heap_ref().unwrap()]
        );
        let field = field_id(&fixture, "p/T", "link", descriptor);
        assert_eq!(
            fixture
                .heap
                .put_field(receiver, field, Value::Reference(receiver))
                .unwrap_err(),
            InstanceError::UnsupportedReferenceStore { field }
        );
        assert_eq!(fixture.heap.children(receiver).unwrap().count(), 1);
    }
}
#[test]
fn aliases_cycles_overwrite_and_null_clearing_preserve_slot_order_without_tracing() {
    let mut root = ordinary();
    root.field("first", "Lp/T;", 1);
    root.field("number", "I", 1);
    root.field("second", "Lp/T;", 1);
    let first = root.reference(9, "p/T", "first", "Lp/T;");
    let second = root.reference(9, "p/T", "second", "Lp/T;");
    let number = root.reference(9, "p/T", "number", "I");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, first));
    code.extend([0x2a, 0x10, 7]);
    code.extend(op(0xb5, number));
    code.extend([0x2a, 0x2a]);
    code.extend(op(0xb5, second));
    code.extend([0x04, 0xac]);
    root.method("selfAlias", "()I", 1, &code, 2, 1);
    let mut code = vec![0x2a];
    code.extend(op(0xb4, first));
    code.push(0x2a);
    code.extend(op(0xb5, first));
    code.push(0x2a);
    code.extend(op(0xb4, first));
    code.extend(op(0xb4, first));
    code.extend([0x2a, 0xa5, 0, 5, 0x03, 0xac, 0x04, 0xac]);
    root.method("cycle", "()I", 1, &code, 2, 1);
    for (name, index) in [("clearFirst", first), ("clearSecond", second)] {
        let mut code = vec![0x2a, 0x01];
        code.extend(op(0xb5, index));
        code.push(0xb1);
        root.method(name, "()V", 1, &code, 2, 1);
    }
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let other = fixture.object("p/T");
    let method = fixture.method("p/T", "selfAlias", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    let root_handle = receiver.heap_ref().unwrap();
    let other_handle = other.heap_ref().unwrap();
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![root_handle, root_handle]
    );
    seed(
        &mut fixture,
        "p/T",
        first,
        receiver,
        Value::Reference(other),
    );
    seed(
        &mut fixture,
        "p/T",
        second,
        receiver,
        Value::Reference(other),
    );
    let cycle = fixture.method("p/T", "cycle", "()I");
    assert_eq!(
        fixture.run(cycle, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![other_handle, other_handle]
    );
    assert_eq!(
        fixture.heap.children(other).unwrap().collect::<Vec<_>>(),
        vec![root_handle]
    );
    for (name, remaining) in [("clearFirst", 1), ("clearSecond", 0)] {
        let method = fixture.method("p/T", name, "()V");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            O::Returned(None)
        );
        assert_eq!(fixture.heap.children(receiver).unwrap().count(), remaining);
    }
    assert_eq!(
        fixture.heap.children(other).unwrap().collect::<Vec<_>>(),
        vec![root_handle]
    );
    assert_eq!(fixture.heap.allocated_instances(), 2);
    assert_eq!(fixture.heap.allocated_arrays(), 0);
    assert_eq!(fixture.heap.total_instance_slots(), 6);
}
#[test]
fn inherited_hidden_references_keep_owner_slots_and_child_order_across_numeric_fields() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("link", "Ljava/lang/Object;", 1);
    base.field("n", "J", 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    root.field("link", "Lp/T;", 1);
    let base_field = root.reference(9, "q/Base", "link", "Ljava/lang/Object;");
    let own_field = root.reference(9, "p/T", "link", "Lp/T;");
    let mut code = vec![0x03, 0xbc, 10, 0x4c, 0x2a, 0x2b];
    code.extend(op(0xb5, base_field));
    code.extend([0x2a, 0x2a]);
    code.extend(op(0xb5, own_field));
    code.extend([0x04, 0xac]);
    root.method("run", "()I", 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    let Value::Reference(array) = fixture.stored("q/Base", "link", "Ljava/lang/Object;", receiver)
    else {
        panic!()
    };
    assert_eq!(fixture.heap.kind(array).unwrap(), PrimitiveArrayKind::Int);
    assert_eq!(
        fixture.stored("p/T", "link", "Lp/T;", receiver),
        Value::Reference(receiver)
    );
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![array.heap_ref().unwrap(), receiver.heap_ref().unwrap()]
    );
    assert_eq!(fixture.heap.total_instance_slots(), 3);
}
#[test]
fn all_primitive_arrays_store_to_exact_and_core_fields_then_round_trip_payload_through_retrieved_alias()
 {
    for (
        atype,
        array_descriptor,
        parameter,
        input,
        expected,
        load,
        store,
        array_load,
        ret,
        width,
    ) in [
        (
            4,
            "[Z",
            "I",
            Value::Int(3),
            Value::Int(1),
            0x1b,
            0x54,
            0x33,
            0xac,
            1,
        ),
        (
            8,
            "[B",
            "I",
            Value::Int(128),
            Value::Int(-128),
            0x1b,
            0x54,
            0x33,
            0xac,
            1,
        ),
        (
            5,
            "[C",
            "I",
            Value::Int(-1),
            Value::Int(65535),
            0x1b,
            0x55,
            0x34,
            0xac,
            1,
        ),
        (
            9,
            "[S",
            "I",
            Value::Int(32768),
            Value::Int(-32768),
            0x1b,
            0x56,
            0x35,
            0xac,
            1,
        ),
        (
            10,
            "[I",
            "I",
            Value::Int(i32::MIN),
            Value::Int(i32::MIN),
            0x1b,
            0x4f,
            0x2e,
            0xac,
            1,
        ),
        (
            11,
            "[J",
            "J",
            Value::Long(i64::MIN),
            Value::Long(i64::MIN),
            0x1f,
            0x50,
            0x2f,
            0xad,
            2,
        ),
        (
            6,
            "[F",
            "F",
            Value::Float(0x7fc12345),
            Value::Float(0x7fc12345),
            0x23,
            0x51,
            0x30,
            0xae,
            1,
        ),
        (
            7,
            "[D",
            "D",
            Value::Double(0x8000000000000000),
            Value::Double(0x8000000000000000),
            0x27,
            0x52,
            0x31,
            0xaf,
            2,
        ),
    ] {
        let mut root = ordinary();
        let fields = [
            ("exact", array_descriptor),
            ("object", "Ljava/lang/Object;"),
            ("cloneable", "Ljava/lang/Cloneable;"),
            ("serial", "Ljava/io/Serializable;"),
        ];
        let mut indices = Vec::new();
        for (name, descriptor) in fields {
            root.field(name, descriptor, 1);
            indices.push(root.reference(9, "p/T", name, descriptor));
        }
        let target = root.class(array_descriptor);
        let array_local = 1 + width;
        let mut code = vec![0x04, 0xbc, atype, 0x3a, array_local as u8];
        for &field in &indices {
            code.extend([0x2a, 0x19, array_local as u8]);
            code.extend(op(0xb5, field));
        }
        code.push(0x2a);
        code.extend(op(0xb4, indices[0]));
        code.extend([0x03, load, store, 0x2a]);
        code.extend(op(0xb4, indices[1]));
        code.extend(op(0xc0, target));
        code.extend([0x03, array_load, ret]);
        let descriptor = format!("({parameter}){parameter}");
        root.method("run", &descriptor, 1, &code, 4, array_local + 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", &descriptor);
        assert_eq!(
            fixture.run(method, receiver, &[input]).unwrap(),
            returned(expected)
        );
        let children = fixture.heap.children(receiver).unwrap().collect::<Vec<_>>();
        assert_eq!(children.len(), 4);
        assert!(children.iter().all(|child| *child == children[0]));
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.heap.total_elements(), 1);
        assert_eq!(fixture.heap.allocated_instances(), 1);
    }
}
#[test]
fn incompatible_nominal_and_array_stores_are_host_diagnostics_and_preserve_old_edges() {
    let other = Class::new("q/Other", Some("java/lang/Object"), 0x21);
    let mut root = ordinary();
    root.field("link", "Lq/Other;", 1);
    let field = root.reference(9, "p/T", "link", "Lq/Other;");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![other, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    let diagnostic = mismatch(&error);
    assert_eq!(
        diagnostic.field,
        field_id(&fixture, "p/T", "link", "Lq/Other;")
    );
    assert_eq!(
        diagnostic.source,
        ReferenceType::Class(fixture.classes["p/T"])
    );
    assert_eq!(
        diagnostic.target,
        ReferenceType::Class(fixture.classes["q/Other"])
    );
    assert_eq!(
        fixture.stored("p/T", "link", "Lq/Other;", receiver),
        Value::Reference(Reference::null())
    );
    assert_eq!(fixture.heap.children(receiver).unwrap().count(), 0);
    assert_eq!(error.offset, Some(2));
    let mut root = ordinary();
    root.field("array", "[I", 1);
    let field = root.reference(9, "p/T", "array", "[I");
    let mut code = vec![0x2a, 0x03, 0xbc, 4];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let old = fixture
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 2)
        .unwrap();
    seed(&mut fixture, "p/T", field, receiver, Value::Reference(old));
    let method = fixture.method("p/T", "run", "()V");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    let diagnostic = mismatch(&error);
    let bootstrap = fixture
        .heap
        .reference_type_universe()
        .unwrap()
        .bootstrap_loader;
    assert_eq!(
        diagnostic.source,
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Boolean),
            defining_loader: bootstrap
        }
    );
    assert_eq!(
        diagnostic.target,
        ReferenceType::Array {
            dimensions: 1,
            element: ArrayElement::Primitive(PrimitiveArrayKind::Int),
            defining_loader: bootstrap
        }
    );
    assert_eq!(
        fixture.stored("p/T", "array", "[I", receiver),
        Value::Reference(old)
    );
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![old.heap_ref().unwrap()]
    );
    assert_eq!(fixture.heap.allocated_arrays(), 2);
}
#[test]
fn public_field_type_access_is_not_cast_target_access_and_inherited_field_uses_declaring_descriptor()
 {
    let mut hidden = iface("q/Hidden");
    hidden.flags = 0x600;
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    implement(&mut base, "q/Hidden");
    base.field("link", "Lq/Hidden;", 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let field = root.reference(9, "p/T", "link", "Lq/Hidden;");
    let target = root.class("q/Hidden");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, field));
    code.extend([0x04, 0xac]);
    root.method("store", "()I", 1, &code, 2, 1);
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.extend(op(0xc0, target));
    code.extend([0x57, 0x04, 0xac]);
    root.method("cast", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![hidden, base, root]);
    let receiver = fixture.object("p/T");
    let store = fixture.method("p/T", "store", "()I");
    assert_eq!(
        fixture.run(store, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(
        fixture.stored("q/Base", "link", "Lq/Hidden;", receiver),
        Value::Reference(receiver)
    );
    let cast = fixture.method("p/T", "cast", "()I");
    assert_eq!(
        fixture.run(cast, receiver, &[]).unwrap(),
        O::IllegalAccessError
    );
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![receiver.heap_ref().unwrap()]
    );
}
#[test]
fn numeric_and_null_stores_keep_old_universe_free_behavior_even_for_missing_descriptor_targets() {
    let mut root = ordinary();
    root.field("number", "I", 1);
    root.field("link", "Lq/Missing;", 1);
    let number = root.reference(9, "p/T", "number", "I");
    let field = root.reference(9, "p/T", "link", "Lq/Missing;");
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, number));
    code.extend([0x2a, 0x01]);
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb4, number));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::unbound(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(7))
    );
    assert!(fixture.heap.reference_type_universe().is_none());
    assert_eq!(fixture.heap.children(receiver).unwrap().count(), 0);
    for (descriptor, value, expected_kind) in [
        ("J", 0x2a, ValueKind::Long),
        ("Ljava/lang/Object;", 0x03, ValueKind::Reference),
    ] {
        let mut root = ordinary();
        root.field("f", descriptor, 1);
        let field = root.reference(9, "p/T", "f", descriptor);
        let mut code = vec![0x2a, value];
        code.extend(op(0xb5, field));
        code.push(0xb1);
        root.method("run", "()V", 1, &code, 2, 1);
        let mut fixture = Fixture::unbound(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert!(
            matches!(error.kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::Instance(InstanceError::Heap(HeapError::ValueKindMismatch {expected,..})) if expected==expected_kind))
        );
    }
}

#[test]
fn retrieved_references_feed_virtual_interface_and_special_calls_with_current_frame_access() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("privateLink", "Lq/Base;", 2);
    base.field("publicLink", "Lq/Base;", 1);
    let link = base.reference(9, "q/Base", "privateLink", "Lq/Base;");
    let call = base.reference(10, "q/Base", "v", "()I");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, link));
    code.push(0x2a);
    code.extend(op(0xb4, link));
    code.extend(op(0xb6, call));
    code.push(0xac);
    base.method("storeAndVirtual", "()I", 1, &code, 2, 1);
    let mut code = vec![0x2a];
    code.extend(op(0xb4, link));
    code.extend(op(0xb7, call));
    code.push(0xac);
    base.method("specialUse", "()I", 1, &code, 1, 1);
    base.method("v", "()I", 1, &[0x04, 0xac], 1, 1);
    let mut interface = iface("q/I");
    let link = interface.reference(9, "q/Base", "publicLink", "Lq/Base;");
    let call = interface.reference(11, "q/I", "value", "()I");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, link));
    code.push(0x2a);
    code.extend(op(0xb4, link));
    code.extend(interface_call(call));
    code.push(0xac);
    interface.method("body", "()I", 1, &code, 2, 1);
    interface.method("value", "()I", 0x401, &[], 0, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    root.field("privateLink", "Lp/T;", 2);
    for (name, opcode, target) in [
        ("virtual", 0xb6, "storeAndVirtual"),
        ("special", 0xb7, "storeAndVirtual"),
        ("specialUseRun", 0xb6, "specialUse"),
    ] {
        let call = root.reference(10, "q/Base", target, "()I");
        let mut code = vec![0x2a];
        code.extend(op(opcode, call));
        code.push(0xac);
        root.method(name, "()I", 1, &code, 1, 1);
    }
    let call = root.reference(11, "q/I", "body", "()I");
    let mut code = vec![0x2a];
    code.extend(interface_call(call));
    code.push(0xac);
    root.method("interface", "()I", 1, &code, 1, 1);
    root.method("v", "()I", 1, &[0x08, 0xac], 1, 1);
    root.method("value", "()I", 1, &[0x10, 7, 0xac], 1, 1);
    let mut fixture = Fixture::new(vec![base, interface, root]);
    let receiver = fixture.object("p/T");
    for (name, expected) in [
        ("virtual", 5),
        ("special", 5),
        ("specialUseRun", 1),
        ("interface", 7),
    ] {
        let method = fixture.method("p/T", name, "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(expected))
        );
    }
    assert_eq!(
        fixture.stored("p/T", "privateLink", "Lp/T;", receiver),
        Value::Reference(Reference::null())
    );
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![receiver.heap_ref().unwrap(), receiver.heap_ref().unwrap()]
    );
}
#[test]
fn failure_prefix_static_final_access_and_null_win_before_reference_assignment() {
    for failure in 0..6 {
        let mut owner = Class::new(
            "q/Owner",
            Some("java/lang/Object"),
            if failure == 3 { 0x20 } else { 0x21 },
        );
        owner.field("link", "Lq/Other;", if failure == 4 { 2 } else { 1 });
        let other = Class::new("q/Other", Some("java/lang/Object"), 0x21);
        let mut root = Class::new("p/T", Some("q/Owner"), 0x21);
        root.field(
            "link",
            "Lq/Other;",
            if failure == 0 {
                9
            } else if failure == 1 {
                0x11
            } else {
                1
            },
        );
        let field = root.reference(
            9,
            if matches!(failure, 3 | 4) {
                "q/Owner"
            } else {
                "p/T"
            },
            "link",
            "Lq/Other;",
        );
        let mut code = vec![if failure == 5 { 0x2a } else { 0x01 }, 0x2a];
        code.extend(op(0xb5, field));
        code.push(0xb1);
        root.method("run", "()V", 1, &code, 2, 1);
        let mut fixture = Fixture::unbound(vec![owner, other, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap(), O::IncompatibleClassChangeError),
            1 | 3 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            2 => assert_eq!(result.unwrap(), O::NullPointerException),
            4 => assert!(
                matches!(result.unwrap_err().kind,E::FieldAccess(error) if matches!(*error,FieldAccessError::UnsupportedNestAccess {..}))
            ),
            _ => assert!(
                matches!(result.unwrap_err().kind,E::FieldAccess(error) if matches!(error.as_ref(),FieldAccessError::ReferenceType(error) if matches!(error.as_ref(),TypeAccessError::MissingUniverse)))
            ),
        };
        assert_eq!(fixture.heap.children(receiver).unwrap().count(), 0);
    }
}
#[test]
fn late_assignment_work_exhaustion_preserves_existing_reference_and_its_child_edge() {
    let mut root = ordinary();
    root.field("link", "Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "link", "Ljava/lang/Object;");
    let mut code = vec![0x2a, 0x03, 0xbc, 10];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    let owner = fixture.classes["p/T"];
    let probe = fixture
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut resolver = SameLoaderResolver;
    let full = {
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &fixture.registry,
            &mut fixture.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        context.check_receiver(owner, receiver).unwrap();
        context
            .write_field(owner, field, receiver, Value::Reference(probe))
            .unwrap();
        context.lookup_steps()
    };
    seed(
        &mut fixture,
        "p/T",
        field,
        receiver,
        Value::Reference(receiver),
    );
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: full - 1,
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
        matches!(error.kind,E::FieldAccess(error) if matches!(error.as_ref(),FieldAccessError::ReferenceType(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::ResourceLimit {resource:crabspot_runtime::FieldResource::LookupSteps,..}))))
    );
    let field = fixture
        .registry
        .find_declared_field(owner, &units("link"), &units("Ljava/lang/Object;"))
        .unwrap()
        .unwrap();
    assert_eq!(
        context.heap().get_field(receiver, field).unwrap(),
        Value::Reference(receiver)
    );
    assert_eq!(
        context
            .heap()
            .children(receiver)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![receiver.heap_ref().unwrap()]
    );
    assert_eq!(context.heap().allocated_arrays(), 2);
}
#[test]
fn reference_store_uses_one_fuel_unit_and_existing_frame_slot_code_limits() {
    let mut root = ordinary();
    root.field("link", "Lp/T;", 1);
    let field = root.reference(9, "p/T", "link", "Lp/T;");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    let field = field_id(&fixture, "p/T", "link", "Lp/T;");
    let limits = CallLimits {
        max_frames: 1,
        max_frame_slots: 3,
        max_prepared_code_bytes: 6,
    };
    for (limits, budget, expected) in [
        (
            CallLimits {
                max_frames: 0,
                ..limits
            },
            4,
            E::FrameLimit {
                limit: 0,
                required: 1,
            },
        ),
        (
            CallLimits {
                max_frame_slots: 2,
                ..limits
            },
            4,
            E::FrameSlotLimit {
                limit: 2,
                required: 3,
            },
        ),
        (
            CallLimits {
                max_prepared_code_bytes: 5,
                ..limits
            },
            4,
            E::PreparedCodeLimit {
                limit: 5,
                required: 6,
            },
        ),
        (limits, 0, E::InstructionBudgetExhausted { limit: 0 }),
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
        assert_eq!(fixture.heap.children(receiver).unwrap().count(), 0);
    }
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: 4
                },
                limits
            )
            .unwrap(),
        O::Returned(None)
    );
    fixture
        .heap
        .put_field(receiver, field, Value::Reference(Reference::null()))
        .unwrap();
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
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
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![receiver.heap_ref().unwrap()]
    );
    assert_eq!(fixture.heap.allocated_instances(), 1);
    assert_eq!(fixture.heap.allocated_arrays(), 0);
    assert_eq!(fixture.heap.total_instance_slots(), 1);
}
#[test]
fn earlier_reference_edges_and_allocations_persist_after_later_java_or_host_failures() {
    for failure in 0..7 {
        let mut root = ordinary();
        root.field("good", "Lp/T;", 1);
        root.field("bad", "Lp/T;", 1);
        let good = root.reference(9, "p/T", "good", "Lp/T;");
        let bad = root.reference(9, "p/T", "bad", "Lp/T;");
        let array = root.class("[I");
        let missing = root.class("q/Missing");
        let method_ref = root.reference(10, "p/T", "missing", "()I");
        let mut code = vec![0x2a, 0x2a];
        code.extend(op(0xb5, good));
        code.extend([0x03, 0xbc, 10, 0x57]);
        let expected = match failure {
            0 => {
                code.extend([0x04, 0x03, 0x6c, 0xac]);
                Some(O::ArithmeticException)
            }
            1 => {
                code.push(0x01);
                code.extend(op(0xb4, good));
                code.extend([0x57, 0x03, 0xac]);
                Some(O::NullPointerException)
            }
            2 => {
                code.push(0x2a);
                code.extend(op(0xc0, array));
                code.extend([0x57, 0x03, 0xac]);
                Some(O::ClassCastException)
            }
            3 => {
                code.push(0x2a);
                code.extend(op(0xb6, method_ref));
                code.push(0xac);
                Some(O::NoSuchMethodError)
            }
            4 => {
                code.extend([0xc2, 0x03, 0xac]);
                None
            }
            5 => {
                code.push(0x2a);
                code.extend(op(0xc0, missing));
                code.extend([0x57, 0x03, 0xac]);
                None
            }
            _ => {
                code.extend([0x2a, 0x03, 0xbc, 10]);
                code.extend(op(0xb5, bad));
                code.extend([0x03, 0xac]);
                None
            }
        };
        root.method("run", "()I", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.run(method, receiver, &[]);
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(
            fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
            vec![receiver.heap_ref().unwrap()]
        );
        assert_eq!(
            fixture.stored("p/T", "bad", "Lp/T;", receiver),
            Value::Reference(Reference::null())
        );
        assert_eq!(
            fixture.heap.allocated_arrays(),
            if failure == 6 { 2 } else { 1 }
        );
    }
}
#[test]
fn stored_reference_local_overlap_retains_category_two_invalidation_and_prior_edge() {
    let mut root = ordinary();
    root.field("link", "Lp/T;", 1);
    let field = root.reference(9, "p/T", "link", "Lp/T;");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb4, field));
    code.extend([0x4d, 0x1f, 0x58, 0x04, 0xac]);
    root.method("run", "(J)I", 1, &code, 2, 3);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "(J)I");
    let error = fixture
        .run(method, receiver, &[Value::Long(42)])
        .unwrap_err();
    assert_eq!(error.kind, E::LocalNotValue { index: 1 });
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![receiver.heap_ref().unwrap()]
    );
}
#[test]
fn descriptor_resolution_failures_keep_field_error_provenance_and_old_edge_state() {
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.field("missing", "Lq/Missing;", 1);
    let field = base.reference(9, "q/Base", "missing", "Lq/Missing;");
    let mut code = vec![0x2a, 0x2a];
    code.extend(op(0xb5, field));
    code.push(0xb1);
    base.method("body", "()V", 1, &code, 2, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "body", "()V");
    let mut code = vec![0x2a];
    code.extend(op(0xb7, call));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    let error = fixture.run(method, receiver, &[]).unwrap_err();
    assert_eq!(error.offset, Some(2));
    assert_eq!(
        error.method.as_ref().unwrap().owner,
        Some(fixture.classes["q/Base"])
    );
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(fixture.classes["p/T"])
    );
    assert!(
        matches!(&error.kind,E::FieldAccess(error) if matches!(error.as_ref(),FieldAccessError::ReferenceType(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::MissingDefinition {origin,..}) if *origin==fixture.classes["q/Base"])))
    );
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(fixture.heap.children(receiver).unwrap().count(), 0);
}
#[test]
fn checked_edges_do_not_enable_old_field_modes_or_reference_method_signatures() {
    let mut root = ordinary();
    root.field("link", "Lp/T;", 1);
    let field = root.reference(9, "p/T", "link", "Lp/T;");
    let mut code = op(0xb5, field);
    code.push(0xb1);
    root.method("staticBody", "()V", 9, &code, 0, 0);
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.push(0xb0);
    root.method("referenceReturn", "()Lp/T;", 1, &code, 1, 1);
    root.method("numeric", "(I)I", 1, &[0x1b, 0xac], 1, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
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
        assert_eq!(error.kind, E::UnsupportedInstruction(0xb5));
    }
    let method = fixture.method("p/T", "referenceReturn", "()Lp/T;");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap_err().kind,
        E::UnsupportedDescriptor
    );
    let method = fixture.method("p/T", "numeric", "(I)I");
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Reference(receiver)])
            .unwrap_err()
            .kind,
        E::ArgumentType {
            index: 0,
            expected: ValueKind::Int,
            actual: ValueKind::Reference
        }
    );
}

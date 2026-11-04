//! Actual reference-array values through the shared runtime-aware body engine.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_instance_method_body, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ArrayAccessError, ArrayElement, ClassId, DefinitionRegistry, FieldAccessError,
    FieldResolutionContext, FieldResolutionOptions, HeapEntryKind, HeapError, HeapOptions,
    HierarchyOptions, InstanceOptions, ManagedHeap, MethodId, PrimitiveArrayKind,
    REFERENCE_ARRAY_ELEMENT_BYTES, Reference, ReferenceType, ReferenceTypeUniverse,
    RegistryOptions, SameLoaderResolver, TypeAccessError, build_hierarchy,
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
    fn limited(classes: Vec<Class>, options: HeapOptions) -> Self {
        Self::configured_arrays(classes, true, options)
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
fn interface_call(index: u16) -> Vec<u8> {
    let mut code = op(0xb9, index);
    code.extend([1, 0]);
    code
}
fn field_id(fixture: &Fixture, name: &str, descriptor: &str) -> crabspot_runtime::FieldId {
    fixture
        .registry
        .find_declared_field(fixture.classes["p/T"], &units(name), &units(descriptor))
        .unwrap()
        .unwrap()
}
#[test]
fn nominal_interface_and_abstract_components_allocate_null_defaults_and_generic_lengths() {
    for component in ["p/T", "q/I", "q/Abstract", "java/lang/Object"] {
        let interface = iface("q/I");
        let abstract_class = Class::new("q/Abstract", Some("java/lang/Object"), 0x421);
        let mut root = ordinary();
        root.field("array", "Ljava/lang/Object;", 1);
        let field = root.reference(9, "p/T", "array", "Ljava/lang/Object;");
        let target = root.class(component);
        let mut code = vec![0x06];
        code.extend(op(0xbd, target));
        code.extend([0x4c, 0x2a, 0x2b]);
        code.extend(op(0xb5, field));
        code.extend([0x2b, 0xbe, 0xac]);
        root.method("run", "()I", 1, &code, 2, 2);
        let mut fixture = Fixture::new(vec![interface, abstract_class, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(3))
        );
        let Value::Reference(array) =
            fixture.stored("p/T", "array", "Ljava/lang/Object;", receiver)
        else {
            panic!()
        };
        let loader = fixture
            .registry
            .defining_loader(fixture.classes[component])
            .unwrap();
        assert_eq!(
            fixture.heap.reference_array_type(array).unwrap(),
            ReferenceType::Array {
                dimensions: 1,
                element: ArrayElement::Class(fixture.classes[component]),
                defining_loader: loader
            }
        );
        for index in 0..3 {
            assert_eq!(
                fixture.heap.load_reference(array, index).unwrap(),
                Reference::null()
            );
        }
        assert_eq!(fixture.heap.length(array).unwrap(), 3);
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.heap.total_elements(), 3);
        assert_eq!(
            fixture.heap.payload_bytes(),
            3 * REFERENCE_ARRAY_ELEMENT_BYTES
        );
        assert_eq!(fixture.heap.allocated_instances(), 1);
        assert_eq!(fixture.heap.children(array).unwrap().count(), 0);
    }
}
#[test]
fn null_elements_and_empty_arrays_have_correct_load_identity_and_distinct_allocations() {
    let mut root = ordinary();
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let component = root.class("java/lang/Object");
    let mut code = vec![0x04];
    code.extend(op(0xbd, component));
    code.extend([0x59, 0x4c, 0x03, 0x32, 0xc6, 0, 5, 0x03, 0xac, 0x2a, 0x2b]);
    code.extend(op(0xb5, field));
    code.extend([0x04, 0xac]);
    root.method("defaults", "()I", 1, &code, 2, 2);
    let mut code = vec![0x03];
    code.extend(op(0xbd, component));
    code.push(0x4c);
    code.push(0x03);
    code.extend(op(0xbd, component));
    code.extend([0x2b, 0xa6, 0, 5, 0x03, 0xac, 0x04, 0xac]);
    root.method("empty", "()I", 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    for name in ["defaults", "empty"] {
        let method = fixture.method("p/T", name, "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(1))
        );
    }
    assert_eq!(fixture.heap.allocated_arrays(), 3);
    assert_eq!(fixture.heap.total_elements(), 1);
    assert_eq!(fixture.heap.payload_bytes(), REFERENCE_ARRAY_ELEMENT_BYTES);
}
#[test]
fn aliases_self_cycles_and_object_edges_keep_per_element_order_and_unchanged_loaded_identity() {
    let mut root = ordinary();
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let component = root.class("java/lang/Object");
    let mut code = vec![0x06];
    code.extend(op(0xbd, component));
    code.extend([
        0x4c, 0x2b, 0x03, 0x2b, 0x53, 0x2b, 0x04, 0x2a, 0x53, 0x2b, 0x05, 0x2a, 0x53, 0x2a, 0x2b,
    ]);
    code.extend(op(0xb5, field));
    code.extend([0x2b, 0x03, 0x32, 0x2b, 0xa5, 0, 5, 0x03, 0xac, 0x04, 0xac]);
    root.method("run", "()I", 1, &code, 3, 2);
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.extend([0x05, 0x01, 0x53, 0xb1]);
    root.method("clear", "()V", 1, &code, 3, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    let Value::Reference(array) = fixture.stored("p/T", "array", "[Ljava/lang/Object;", receiver)
    else {
        panic!()
    };
    let handle = array.heap_ref().unwrap();
    let receiver_handle = receiver.heap_ref().unwrap();
    assert_eq!(
        fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
        vec![handle, receiver_handle, receiver_handle]
    );
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![handle]
    );
    let method = fixture.method("p/T", "clear", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        O::Returned(None)
    );
    assert_eq!(
        fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
        vec![handle, receiver_handle]
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    assert_eq!(fixture.heap.allocated_instances(), 1);
}
#[test]
fn nested_arrays_for_all_primitive_components_store_retrieve_and_cast_by_actual_type() {
    for (atype, component, kind) in [
        (4, "[Z", PrimitiveArrayKind::Boolean),
        (5, "[C", PrimitiveArrayKind::Char),
        (6, "[F", PrimitiveArrayKind::Float),
        (7, "[D", PrimitiveArrayKind::Double),
        (8, "[B", PrimitiveArrayKind::Byte),
        (9, "[S", PrimitiveArrayKind::Short),
        (10, "[I", PrimitiveArrayKind::Int),
        (11, "[J", PrimitiveArrayKind::Long),
    ] {
        let mut root = ordinary();
        root.field("array", "Ljava/lang/Object;", 1);
        let field = root.reference(9, "p/T", "array", "Ljava/lang/Object;");
        let component_index = root.class(component);
        let cast = root.class("[Ljava/lang/Object;");
        let mut code = vec![0x04];
        code.extend(op(0xbd, component_index));
        code.extend([0x4c, 0x2b, 0x03, 0x05, 0xbc, atype, 0x53, 0x2a, 0x2b]);
        code.extend(op(0xb5, field));
        code.push(0x2b);
        code.extend(op(0xc0, cast));
        code.extend([0x03, 0x32, 0xbe, 0xac]);
        root.method("run", "()I", 1, &code, 3, 2);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(2))
        );
        let Value::Reference(array) =
            fixture.stored("p/T", "array", "Ljava/lang/Object;", receiver)
        else {
            panic!()
        };
        let bootstrap = fixture
            .heap
            .reference_type_universe()
            .unwrap()
            .bootstrap_loader;
        assert_eq!(
            fixture.heap.reference_array_type(array).unwrap(),
            ReferenceType::Array {
                dimensions: 2,
                element: ArrayElement::Primitive(kind),
                defining_loader: bootstrap
            }
        );
        let inner = fixture.heap.load_reference(array, 0).unwrap();
        assert_eq!(fixture.heap.kind(inner).unwrap(), kind);
        assert_eq!(
            fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
            vec![inner.heap_ref().unwrap()]
        );
        assert_eq!(fixture.heap.allocated_arrays(), 2);
        assert_eq!(fixture.heap.total_elements(), 3);
        assert_eq!(
            fixture.heap.payload_bytes(),
            REFERENCE_ARRAY_ELEMENT_BYTES + 2 * kind.element_bytes()
        );
    }
}
#[test]
fn covariant_alias_store_checks_actual_component_and_preserves_previous_element_and_children() {
    let mut root = ordinary();
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.extend([0x03, 0x2a, 0x53, 0xb1]);
    root.method("run", "()V", 1, &code, 3, 1);
    let child = Class::new("p/Child", Some("p/T"), 0x21);
    let mut fixture = Fixture::new(vec![root, child]);
    let receiver = fixture.object("p/T");
    let old = fixture.object("p/Child");
    let mut resolver = SameLoaderResolver;
    let array = {
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &fixture.registry,
            &mut fixture.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let array = context
            .allocate_reference_array(ReferenceType::Class(fixture.classes["p/Child"]), 1)
            .unwrap();
        context.write_reference_array(array, 0, old).unwrap();
        context
            .write_field(
                fixture.classes["p/T"],
                field,
                receiver,
                Value::Reference(array),
            )
            .unwrap();
        array
    };
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        O::ArrayStoreException
    );
    assert_eq!(fixture.heap.load_reference(array, 0).unwrap(), old);
    assert_eq!(
        fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
        vec![old.heap_ref().unwrap()]
    );
    assert_eq!(
        fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
        vec![array.heap_ref().unwrap()]
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
}
#[test]
fn primitive_component_mismatch_is_array_store_and_never_field_type_exception() {
    let mut root = ordinary();
    let component = root.class("[I");
    let mut code = vec![0x04];
    code.extend(op(0xbd, component));
    code.extend([0x4c, 0x2b, 0x03, 0x04, 0xbc, 4, 0x53, 0xb1]);
    root.method("run", "()V", 1, &code, 3, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        O::ArrayStoreException
    );
    assert_eq!(fixture.heap.allocated_arrays(), 2);
}
#[test]
fn null_bounds_type_and_malformed_computational_kinds_have_explicit_order() {
    for failure in 0..6 {
        let other = Class::new("q/Other", Some("java/lang/Object"), 0x21);
        let mut root = ordinary();
        let component = root.class("q/Other");
        let mut code = if failure == 0 {
            vec![0x01]
        } else {
            let mut c = vec![if failure == 1 { 0x03 } else { 0x04 }];
            c.extend(op(0xbd, component));
            c
        };
        code.push(if failure == 3 {
            0x01
        } else if failure == 0 {
            0x02
        } else {
            0x03
        });
        code.push(if failure == 4 { 0x03 } else { 0x2a });
        code.extend([0x53, 0xb1]);
        root.method("run", "()V", 1, &code, 3, 1);
        let mut fixture = Fixture::new(vec![other, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap(), O::NullPointerException),
            1 => assert_eq!(result.unwrap(), O::ArrayIndexOutOfBoundsException),
            2 | 5 => assert_eq!(result.unwrap(), O::ArrayStoreException),
            3 => assert!(matches!(
                result.unwrap_err().kind,
                E::WrongValueType {
                    expected: ValueKind::Int,
                    actual: ValueKind::Reference
                }
            )),
            _ => assert!(matches!(
                result.unwrap_err().kind,
                E::WrongValueType {
                    expected: ValueKind::Reference,
                    actual: ValueKind::Int
                }
            )),
        };
    }
    for null in [true, false] {
        let mut root = ordinary();
        let component = root.class("java/lang/Object");
        let mut code = if null {
            vec![0x01]
        } else {
            let mut c = vec![0x03];
            c.extend(op(0xbd, component));
            c
        };
        code.extend([0x02, 0x32, 0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            if null {
                O::NullPointerException
            } else {
                O::ArrayIndexOutOfBoundsException
            }
        );
    }
}
#[test]
fn anewarray_resolves_access_and_rank_before_negative_length_and_leaves_counters_atomic() {
    for failure in 0..6 {
        let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
        let mut root = ordinary();
        let component = root.class(match failure {
            0 => "p/T",
            1 => "q/Hidden",
            2 => "q/Missing",
            _ => "p/T",
        });
        let mut code = vec![0x02];
        code.extend(op(0xbd, if failure == 3 { 1 } else { component }));
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 1, 1);
        let mut fixture = if failure == 4 {
            Fixture::unbound(vec![hidden, root])
        } else if failure == 5 {
            Fixture::limited(
                vec![hidden, root],
                HeapOptions {
                    max_arrays: 0,
                    ..HeapOptions::default()
                },
            )
        } else {
            Fixture::new(vec![hidden, root])
        };
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 | 5 => assert_eq!(result.unwrap(), O::NegativeArraySizeException),
            1 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            _ => assert!(matches!(result.unwrap_err().kind, E::ArrayAccess(_))),
        }
        assert_eq!(fixture.heap.allocated_arrays(), 0);
        assert_eq!(fixture.heap.payload_bytes(), 0);
        assert_eq!(fixture.heap.total_elements(), 0);
    }
    for length in [0x02, 0x03] {
        let mut root = ordinary();
        let component = root.class(&format!("{}I", "[".repeat(255)));
        let mut code = vec![length];
        code.extend(op(0xbd, component));
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        assert!(
            matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::InvalidType(_))))
        );
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
}

#[test]
fn highest_allocatable_component_rank_produces_rank_255_without_recursive_components() {
    let mut root = ordinary();
    let component = root.class(&format!("{}I", "[".repeat(254)));
    let target = root.class(&format!("{}I", "[".repeat(255)));
    let mut code = vec![0x03];
    code.extend(op(0xbd, component));
    code.extend(op(0xc1, target));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    assert_eq!(fixture.heap.payload_bytes(), 0);
}
#[test]
fn primitive_and_reference_allocations_share_exact_quota_totals_and_generous_limits_are_not_capacity()
 {
    let mut root = ordinary();
    let component = root.class("java/lang/Object");
    let mut code = vec![0x04, 0xbc, 10, 0x57, 0x05];
    code.extend(op(0xbd, component));
    code.extend([0xbe, 0xac]);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut code = vec![0x03];
    code.extend(op(0xbd, component));
    code.extend([0xbe, 0xac]);
    root.method("more", "()I", 1, &code, 1, 1);
    let options = HeapOptions {
        max_arrays: 2,
        max_array_length: 2,
        max_total_elements: 3,
        max_payload_bytes: 4 + 2 * REFERENCE_ARRAY_ELEMENT_BYTES,
    };
    let mut fixture = Fixture::limited(vec![root], options);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(2))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 2);
    assert_eq!(fixture.heap.total_elements(), 3);
    assert_eq!(fixture.heap.payload_bytes(), options.max_payload_bytes);
    let method = fixture.method("p/T", "more", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::ArrayAccess(error) if matches!(*error,ArrayAccessError::Heap(HeapError::ResourceLimit {resource:crabspot_runtime::HeapResource::Arrays,limit:2,required:3})))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 2);
    assert!(matches!(
        fixture
            .heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0),
        Err(HeapError::ResourceLimit {
            resource: crabspot_runtime::HeapResource::Arrays,
            ..
        })
    ));
    for options in [
        HeapOptions {
            max_arrays: 1,
            max_array_length: 0,
            max_total_elements: 0,
            max_payload_bytes: 0,
        },
        HeapOptions {
            max_arrays: usize::MAX,
            max_array_length: usize::MAX,
            max_total_elements: usize::MAX,
            max_payload_bytes: usize::MAX,
        },
    ] {
        let mut root = ordinary();
        let component = root.class("java/lang/Object");
        let mut code = vec![0x03];
        code.extend(op(0xbd, component));
        code.extend([0xbe, 0xac]);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::limited(vec![root], options);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(0))
        );
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.heap.payload_bytes(), 0);
    }
    let mut root = ordinary();
    let component = root.class("java/lang/Object");
    let mut code = vec![0x04];
    code.extend(op(0xbd, component));
    code.extend([0xbe, 0xac]);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::limited(
        vec![root],
        HeapOptions {
            max_payload_bytes: REFERENCE_ARRAY_ELEMENT_BYTES - 1,
            ..Default::default()
        },
    );
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert!(
        matches!(fixture.run(method,receiver,&[]).unwrap_err().kind,E::ArrayAccess(error) if matches!(*error,ArrayAccessError::Heap(HeapError::ResourceLimit {resource:crabspot_runtime::HeapResource::PayloadBytes,required,..}) if required==REFERENCE_ARRAY_ELEMENT_BYTES))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 0);
}
#[test]
fn exact_fuel_frame_stack_and_code_limits_keep_completed_allocations_and_stores() {
    let mut root = ordinary();
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let component = root.class("java/lang/Object");
    let mut code = vec![0x04];
    code.extend(op(0xbd, component));
    code.extend([0x4c, 0x2a, 0x2b]);
    code.extend(op(0xb5, field));
    code.extend([
        0x2b, 0x03, 0x2a, 0x53, 0x2b, 0x03, 0x32, 0x2a, 0xa5, 0, 5, 0x03, 0xac, 0x04, 0xac,
    ]);
    let bytes = code.len();
    root.method("run", "()I", 1, &code, 3, 2);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    let limits = CallLimits {
        max_frames: 1,
        max_frame_slots: 5,
        max_prepared_code_bytes: bytes,
    };
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: 17
                },
                limits
            )
            .unwrap(),
        returned(Value::Int(1))
    );
    for (limits, expected) in [
        (
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
            CallLimits {
                max_frame_slots: 4,
                ..limits
            },
            E::FrameSlotLimit {
                limit: 4,
                required: 5,
            },
        ),
        (
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
                .run_limits(method, receiver, &[], ExecutionOptions::default(), limits)
                .unwrap_err()
                .kind,
            expected
        );
        assert_eq!(fixture.heap.allocated_arrays(), 1);
    }
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
            .unwrap_err()
            .kind,
        E::InstructionBudgetExhausted { limit: 10 }
    );
    let Value::Reference(array) = fixture.stored("p/T", "array", "[Ljava/lang/Object;", receiver)
    else {
        panic!()
    };
    assert_eq!(fixture.heap.load_reference(array, 0).unwrap(), receiver);
    assert_eq!(
        fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
        vec![receiver.heap_ref().unwrap()]
    );
    assert_eq!(fixture.heap.allocated_arrays(), 2);
}
#[test]
fn current_declaring_origin_controls_component_access_after_every_call_kind() {
    let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let component = base.class("q/Hidden");
    let helper = base.reference(10, "q/Base", "helper", "()I");
    let mut code = vec![0x04];
    code.extend(op(0xbd, component));
    code.push(0xbe);
    code.extend(op(0xb8, helper));
    code.extend([0x60, 0xac]);
    base.method("body", "()I", 1, &code, 2, 1);
    let mut code = vec![0x04];
    code.extend(op(0xbd, component));
    code.extend([0xbe, 0xac]);
    base.method("helper", "()I", 9, &code, 1, 0);
    let mut interface = iface("q/I");
    let component = interface.class("q/Hidden");
    let mut code = vec![0x04];
    code.extend(op(0xbd, component));
    code.extend([0xbe, 0xac]);
    interface.method("interfaceBody", "()I", 1, &code, 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    let call = root.reference(10, "q/Base", "body", "()I");
    for (name, opcode) in [("virtual", 0xb6), ("special", 0xb7)] {
        let mut code = vec![0x2a];
        code.extend(op(opcode, call));
        code.push(0xac);
        root.method(name, "()I", 1, &code, 1, 1);
    }
    let call = root.reference(11, "q/I", "interfaceBody", "()I");
    let mut code = vec![0x2a];
    code.extend(interface_call(call));
    code.push(0xac);
    root.method("interface", "()I", 1, &code, 1, 1);
    let component = root.class("q/Hidden");
    let mut code = vec![0x03];
    code.extend(op(0xbd, component));
    code.extend([0xbe, 0xac]);
    root.method("direct", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![hidden, base, interface, root]);
    let receiver = fixture.object("p/T");
    for (name, expected) in [("virtual", 2), ("special", 2), ("interface", 1)] {
        let method = fixture.method("p/T", name, "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(expected))
        );
    }
    let method = fixture.method("p/T", "direct", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        O::IllegalAccessError
    );
    assert_eq!(fixture.heap.allocated_arrays(), 5);
}
#[test]
fn retained_inaccessible_component_allows_compatible_store_without_new_caller_access_checks() {
    let mut hidden = iface("q/Hidden");
    hidden.flags = 0x600;
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    implement(&mut base, "q/Hidden");
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.extend([0x03, 0x2a, 0x53, 0x04, 0xac]);
    root.method("run", "()I", 1, &code, 3, 1);
    let mut fixture = Fixture::new(vec![hidden, base, root]);
    let receiver = fixture.object("p/T");
    let mut resolver = SameLoaderResolver;
    let array = {
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &fixture.registry,
            &mut fixture.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let array = context
            .allocate_reference_array(ReferenceType::Class(fixture.classes["q/Hidden"]), 1)
            .unwrap();
        context
            .write_field(
                fixture.classes["p/T"],
                field,
                receiver,
                Value::Reference(array),
            )
            .unwrap();
        array
    };
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(fixture.heap.load_reference(array, 0).unwrap(), receiver);
    assert_eq!(
        fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
        vec![receiver.heap_ref().unwrap()]
    );
}
#[test]
fn primitive_only_operations_and_generic_length_use_the_exact_new_entry_diagnostics() {
    for primitive_target in [false, true] {
        let mut root = ordinary();
        let component = root.class("java/lang/Object");
        let mut code = vec![0x03];
        if primitive_target {
            code.extend([0xbc, 10, 0x03, 0x32, 0x57, 0xb1]);
        } else {
            code.extend(op(0xbd, component));
            code.extend([0x03, 0x2e, 0x57, 0xb1]);
        }
        root.method("run", "()V", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        let expected = HeapError::WrongEntryKind {
            expected: if primitive_target {
                HeapEntryKind::ReferenceArray
            } else {
                HeapEntryKind::PrimitiveArray
            },
            actual: if primitive_target {
                HeapEntryKind::PrimitiveArray
            } else {
                HeapEntryKind::ReferenceArray
            },
        };
        assert_eq!(
            error.kind,
            if primitive_target {
                E::ArrayAccess(Box::new(ArrayAccessError::Heap(expected)))
            } else {
                E::Heap(expected)
            }
        );
    }
    let mut root = ordinary();
    root.method("run", "()I", 1, &[0x2a, 0xbe, 0xac], 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap_err().kind,
        E::Heap(HeapError::WrongEntryKind {
            expected: HeapEntryKind::Array,
            actual: HeapEntryKind::Instance
        })
    );
}
#[test]
fn old_modes_reject_new_reference_opcodes_and_runtime_multidimensional_arrays_use_actual_payloads()
{
    for opcode in [0x32, 0x53, 0xbd] {
        let mut root = ordinary();
        let mut code = if opcode == 0xbd {
            op(opcode, 1)
        } else {
            vec![opcode]
        };
        code.push(0xb1);
        root.method("staticBody", "()V", 9, &code, 0, 0);
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
            assert_eq!(error.kind, E::UnsupportedInstruction(opcode));
        }
    }
    let mut root = ordinary();
    let component = root.class("[[Ljava/lang/Object;");
    let mut code = vec![0x03, 0x03];
    code.extend(op(0xc5, component));
    code.extend([2, 0x57, 0xb1]);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        O::Returned(None)
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    assert_eq!(fixture.heap.total_elements(), 0);
    let mut root = ordinary();
    let mut code = vec![0x01];
    code.extend(op(0xbd, 1));
    code.extend([0x57, 0xb1]);
    root.method("badLength", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "badLength", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap_err().kind,
        E::WrongValueType {
            expected: ValueKind::Int,
            actual: ValueKind::Reference
        }
    );
}

#[test]
fn reads_and_null_stores_consume_no_relation_work_after_field_selection() {
    let mut root = ordinary();
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.extend([0x03, 0x01, 0x53, 0x2a]);
    code.extend(op(0xb4, field));
    code.extend([0x03, 0x32, 0xc6, 0, 5, 0x03, 0xac, 0x04, 0xac]);
    root.method("run", "()I", 1, &code, 3, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
    let method = fixture.method("p/T", "run", "()I");
    let field_id = field_id(&fixture, "array", "[Ljava/lang/Object;");
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let array = context
        .allocate_reference_array(ReferenceType::Class(owner), 1)
        .unwrap();
    context.write_reference_array(array, 0, receiver).unwrap();
    context
        .write_field(owner, field, receiver, Value::Reference(array))
        .unwrap();
    drop(context);
    let mut resolver = SameLoaderResolver;
    let mut probe = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    probe.check_receiver(owner, receiver).unwrap();
    probe.read_field(owner, field, receiver).unwrap();
    let before = probe.lookup_steps();
    assert_eq!(probe.read_reference_array(array, 0).unwrap(), receiver);
    probe
        .write_reference_array(array, 0, Reference::null())
        .unwrap();
    assert_eq!(
        probe.read_reference_array(array, 0).unwrap(),
        Reference::null()
    );
    assert_eq!(probe.lookup_steps(), before);
    probe.read_field(owner, field, receiver).unwrap();
    let exact = probe.lookup_steps();
    drop(probe);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: exact,
            ..FieldResolutionOptions::default()
        },
    )
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
        .unwrap(),
        returned(Value::Int(1))
    );
    assert_eq!(context.lookup_steps(), exact);
    assert_eq!(
        context.heap().get_field(receiver, field_id).unwrap(),
        Value::Reference(array)
    );
    assert_eq!(context.heap().children(array).unwrap().count(), 0);
    assert_eq!(
        context
            .heap()
            .children(receiver)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![array.heap_ref().unwrap()]
    );
}

#[test]
fn late_assignment_work_exhaustion_preserves_old_element_and_all_prior_publications() {
    let mut root = ordinary();
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.extend([0x03, 0x2a, 0x53, 0xb1]);
    root.method("run", "()V", 1, &code, 3, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let previous = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
    let method = fixture.method("p/T", "run", "()V");
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    let array = context
        .allocate_reference_array(ReferenceType::Class(owner), 1)
        .unwrap();
    context.write_reference_array(array, 0, previous).unwrap();
    context
        .write_field(owner, field, receiver, Value::Reference(array))
        .unwrap();
    drop(context);
    let mut resolver = SameLoaderResolver;
    let mut probe = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    probe.check_receiver(owner, receiver).unwrap();
    probe.read_field(owner, field, receiver).unwrap();
    let prefix = probe.lookup_steps();
    probe.write_reference_array(array, 0, receiver).unwrap();
    let exact = probe.lookup_steps();
    assert!(exact > prefix);
    drop(probe);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    context.write_reference_array(array, 0, previous).unwrap();
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
    let error = execute_instance_method_body(
        &mut context,
        method,
        receiver,
        &[],
        ExecutionOptions::default(),
        CallLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.offset, Some(6));
    assert!(
        matches!(error.kind, E::ArrayAccess(error) if matches!(error.as_ref(), ArrayAccessError::Type(error) if matches!(error.as_ref(), TypeAccessError::Context(FieldAccessError::ResourceLimit { .. }))))
    );
    assert_eq!(context.heap().load_reference(array, 0).unwrap(), previous);
    assert_eq!(
        context.heap().children(array).unwrap().collect::<Vec<_>>(),
        vec![previous.heap_ref().unwrap()]
    );
    assert_eq!(context.heap().allocated_arrays(), 1);
    assert_eq!(context.heap().allocated_instances(), 2);
}

#[test]
fn component_resolution_failures_have_selected_method_identity_and_invocation_provenance() {
    for interface in [false, true] {
        let mut body = if interface {
            iface("q/I")
        } else {
            Class::new("q/Base", Some("java/lang/Object"), 0x21)
        };
        let missing = body.class("q/Missing");
        let mut code = vec![0x03];
        code.extend(op(0xbd, missing));
        code.extend([0xbe, 0xac]);
        body.method("body", "()I", 1, &code, 1, 1);
        let owner_name = body.name.clone();
        let mut root = Class::new(
            "p/T",
            Some(if interface {
                "java/lang/Object"
            } else {
                "q/Base"
            }),
            0x21,
        );
        if interface {
            implement(&mut root, "q/I");
        }
        let call = root.reference(if interface { 11 } else { 10 }, &owner_name, "body", "()I");
        let mut code = vec![0x2a];
        code.extend(if interface {
            interface_call(call)
        } else {
            op(0xb7, call)
        });
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![body, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert_eq!(error.offset, Some(1));
        assert_eq!(
            error.method.as_ref().unwrap().owner,
            Some(fixture.classes[&owner_name])
        );
        assert_eq!(error.call_site.as_ref().unwrap().offset, 1);
        assert_eq!(
            error.call_site.as_ref().unwrap().method.owner,
            Some(fixture.classes["p/T"])
        );
        assert!(
            matches!(error.kind, E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::MissingDefinition { origin, .. }) if *origin==fixture.classes[&owner_name])))
        );
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
}

#[test]
fn earlier_reference_array_publications_and_edges_persist_after_every_failure_family() {
    for failure in 0..8 {
        let mut root = ordinary();
        root.field("array", "[Ljava/lang/Object;", 1);
        let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
        let component = root.class("p/T");
        let int_array = root.class("[I");
        let missing = root.class("q/Missing");
        let mut code = vec![0x2a, 0x04];
        code.extend(op(0xbd, component));
        code.extend(op(0xb5, field));
        code.push(0x2a);
        code.extend(op(0xb4, field));
        code.extend([0x03, 0x2a, 0x53]);
        let expected = match failure {
            0 => {
                code.push(0x2a);
                code.extend(op(0xb4, field));
                code.extend([0x03, 0x03, 0xbc, 10, 0x53, 0xb1]);
                Some(O::ArrayStoreException)
            }
            1 => {
                code.extend([0x01, 0x03, 0x32, 0x57, 0xb1]);
                Some(O::NullPointerException)
            }
            2 => {
                code.push(0x2a);
                code.extend(op(0xb4, field));
                code.extend([0x04, 0x32, 0x57, 0xb1]);
                Some(O::ArrayIndexOutOfBoundsException)
            }
            3 => {
                code.push(0x02);
                code.extend(op(0xbd, component));
                code.extend([0x57, 0xb1]);
                Some(O::NegativeArraySizeException)
            }
            4 => {
                code.push(0x2a);
                code.extend(op(0xc0, int_array));
                code.extend([0x57, 0xb1]);
                Some(O::ClassCastException)
            }
            5 => {
                code.extend([0x04, 0x03, 0x6c, 0x57, 0xb1]);
                Some(O::ArithmeticException)
            }
            6 => {
                code.push(0x03);
                code.extend(op(0xbd, missing));
                code.extend([0x57, 0xb1]);
                None
            }
            _ => {
                code.extend([0xc2, 0xb1]);
                None
            }
        };
        root.method("run", "()V", 1, &code, 3, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.run(method, receiver, &[]);
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(result.is_err());
        }
        let Value::Reference(array) =
            fixture.stored("p/T", "array", "[Ljava/lang/Object;", receiver)
        else {
            panic!("array field lost")
        };
        assert_eq!(fixture.heap.load_reference(array, 0).unwrap(), receiver);
        assert_eq!(
            fixture.heap.children(receiver).unwrap().collect::<Vec<_>>(),
            vec![array.heap_ref().unwrap()]
        );
        assert_eq!(
            fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
            vec![receiver.heap_ref().unwrap()]
        );
        assert_eq!(
            fixture.heap.allocated_arrays(),
            if failure == 0 { 2 } else { 1 }
        );
    }
}

#[test]
fn loaded_elements_keep_identity_through_wide_locals_casts_fields_and_all_instance_calls() {
    let mut interface = iface("q/I");
    interface.method("interfaceBody", "()I", 1, &[0x06, 0xac], 1, 1);
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("baseBody", "()I", 1, &[0x05, 0xac], 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    root.field("array", "[Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "[Ljava/lang/Object;");
    let component = root.class("p/T");
    let virtual_call = root.reference(10, "p/T", "virtualBody", "()I");
    let special_call = root.reference(10, "q/Base", "baseBody", "()I");
    let iface_call = root.reference(11, "q/I", "interfaceBody", "()I");
    root.method("virtualBody", "()I", 1, &[0x04, 0xac], 1, 1);
    let mut code = vec![0x2a, 0x04];
    code.extend(op(0xbd, component));
    code.extend(op(0xb5, field));
    code.push(0x2a);
    code.extend(op(0xb4, field));
    code.extend([0x03, 0x2a, 0x53, 0x2a]);
    code.extend(op(0xb4, field));
    code.extend([0x03, 0x32, 0xc4, 0x3a, 1, 44]);
    for (opcode, index) in [
        (0xb6, virtual_call),
        (0xb7, special_call),
        (0xb9, iface_call),
    ] {
        code.extend([0xc4, 0x19, 1, 44]);
        code.extend(op(0xc0, component));
        code.extend(if opcode == 0xb9 {
            interface_call(index)
        } else {
            op(opcode, index)
        });
    }
    code.extend([0x60, 0x60, 0xac]);
    root.method("run", "()I", 1, &code, 3, 301);
    let mut fixture = Fixture::new(vec![interface, base, root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(6))
    );
    let Value::Reference(array) = fixture.stored("p/T", "array", "[Ljava/lang/Object;", receiver)
    else {
        panic!("lost array")
    };
    assert_eq!(fixture.heap.load_reference(array, 0).unwrap(), receiver);
    assert_eq!(fixture.heap.allocated_arrays(), 1);
}

//! Bounded multidimensional allocation through the existing runtime-aware engine.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_instance_method_body, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ArrayAccessError, ArrayElement, ClassId, DefinitionRegistry, FieldAccessError,
    FieldResolutionContext, FieldResolutionOptions, HeapError, HeapOptions, HierarchyOptions,
    InstanceOptions, ManagedHeap, MethodId, PrimitiveArrayKind, REFERENCE_ARRAY_ELEMENT_BYTES,
    Reference, ReferenceType, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver,
    TypeAccessError, build_hierarchy,
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
fn multi(index: u16, dimensions: u8) -> Vec<u8> {
    let mut code = op(0xc5, index);
    code.push(dimensions);
    code
}
fn push_int(code: &mut Vec<u8>, value: i32) {
    match value {
        -1..=5 => code.push((value + 3) as u8),
        -128..=127 => code.extend([0x10, value as u8]),
        _ => {
            code.push(0x11);
            code.extend((value as i16).to_be_bytes());
        }
    }
}
fn allocation(code: &mut Vec<u8>, target: u16, counts: &[i32]) {
    for &count in counts {
        push_int(code, count);
    }
    code.extend(multi(target, counts.len() as u8));
}
#[test]
fn asymmetric_complete_primitive_geometry_and_every_leaf_opcode_keep_defaults_and_values() {
    for (letter, kind, store, load, value, expected, ret) in [
        (
            "Z",
            PrimitiveArrayKind::Boolean,
            0x54,
            0x33,
            vec![0x06],
            Value::Int(1),
            0xac,
        ),
        (
            "B",
            PrimitiveArrayKind::Byte,
            0x54,
            0x33,
            vec![0x10, 0xfe],
            Value::Int(-2),
            0xac,
        ),
        (
            "C",
            PrimitiveArrayKind::Char,
            0x55,
            0x34,
            vec![0x02],
            Value::Int(65535),
            0xac,
        ),
        (
            "S",
            PrimitiveArrayKind::Short,
            0x56,
            0x35,
            vec![0x10, 0xfe],
            Value::Int(-2),
            0xac,
        ),
        (
            "I",
            PrimitiveArrayKind::Int,
            0x4f,
            0x2e,
            vec![0x10, 42],
            Value::Int(42),
            0xac,
        ),
        (
            "J",
            PrimitiveArrayKind::Long,
            0x50,
            0x2f,
            vec![0x0a],
            Value::Long(1),
            0xad,
        ),
        (
            "F",
            PrimitiveArrayKind::Float,
            0x51,
            0x30,
            vec![0x0c],
            Value::Float(1f32.to_bits()),
            0xae,
        ),
        (
            "D",
            PrimitiveArrayKind::Double,
            0x52,
            0x31,
            vec![0x0f],
            Value::Double(1f64.to_bits()),
            0xaf,
        ),
    ] {
        let mut root = ordinary();
        root.field("array", "Ljava/lang/Object;", 1);
        let field = root.reference(9, "p/T", "array", "Ljava/lang/Object;");
        let target = root.class(&format!("[[{letter}"));
        let mut code = vec![0x2a];
        allocation(&mut code, target, &[2, 3]);
        code.extend([0x59, 0x4c]);
        code.extend(op(0xb5, field));
        code.extend([0x2b, 0x04, 0x32, 0x4d, 0x2c, 0x05]);
        code.extend(value);
        code.push(store);
        code.extend([0x2c, 0x05, load, ret]);
        let descriptor = format!("(){letter}");
        root.method("run", &descriptor, 1, &code, 4, 3);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", &descriptor);
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(expected)
        );
        let Value::Reference(array) =
            fixture.stored("p/T", "array", "Ljava/lang/Object;", receiver)
        else {
            panic!("array field")
        };
        assert_eq!(fixture.heap.length(array).unwrap(), 2);
        let first = fixture.heap.load_reference(array, 0).unwrap();
        let last = fixture.heap.load_reference(array, 1).unwrap();
        assert_ne!(first, last);
        assert_eq!(fixture.heap.length(first).unwrap(), 3);
        assert_eq!(fixture.heap.length(last).unwrap(), 3);
        for index in 0..3 {
            assert_eq!(
                fixture.heap.load(first, index).unwrap(),
                match kind {
                    PrimitiveArrayKind::Long => Value::Long(0),
                    PrimitiveArrayKind::Float => Value::Float(0),
                    PrimitiveArrayKind::Double => Value::Double(0),
                    _ => Value::Int(0),
                }
            );
        }
        assert_eq!(
            fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
            vec![first.heap_ref().unwrap(), last.heap_ref().unwrap()]
        );
        assert_eq!(fixture.heap.allocated_arrays(), 3);
        assert_eq!(fixture.heap.total_elements(), 8);
        assert_eq!(
            fixture.heap.payload_bytes(),
            2 * REFERENCE_ARRAY_ELEMENT_BYTES + 6 * kind.element_bytes()
        );
        assert_eq!(
            fixture.heap.reference_array_type(array).unwrap(),
            ReferenceType::Array {
                dimensions: 2,
                element: ArrayElement::Primitive(kind),
                defining_loader: fixture
                    .registry
                    .defining_loader(fixture.classes["p/T"])
                    .unwrap()
            }
        );
    }
}
#[test]
fn partial_and_single_dimension_allocations_retain_full_types_and_null_components() {
    for (descriptor, counts) in [
        ("[[[I", vec![2, 3]),
        ("[[Lp/T;", vec![2]),
        ("[Lp/T;", vec![2]),
        ("[[Lq/I;", vec![2, 1]),
        ("[[Lq/Abstract;", vec![2, 1]),
    ] {
        let mut root = ordinary();
        root.field("array", "Ljava/lang/Object;", 1);
        let field = root.reference(9, "p/T", "array", "Ljava/lang/Object;");
        let target = root.class(descriptor);
        let mut code = vec![0x2a];
        allocation(&mut code, target, &counts);
        code.extend(op(0xb5, field));
        code.push(0xb1);
        root.method("run", "()V", 1, &code, 3, 1);
        let mut fixture = Fixture::new(vec![
            root,
            iface("q/I"),
            Class::new("q/Abstract", Some("java/lang/Object"), 0x421),
        ]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            O::Returned(None)
        );
        let Value::Reference(array) =
            fixture.stored("p/T", "array", "Ljava/lang/Object;", receiver)
        else {
            panic!("array field")
        };
        assert_eq!(fixture.heap.length(array).unwrap(), 2);
        let children: Vec<_> = fixture.heap.children(array).unwrap().collect();
        if counts.len() == 1 {
            assert!(children.is_empty());
            assert_eq!(
                fixture.heap.load_reference(array, 0).unwrap(),
                Reference::null()
            );
            assert_eq!(fixture.heap.allocated_arrays(), 1);
        } else {
            assert_eq!(children.len(), 2);
            assert_ne!(children[0], children[1]);
            for index in 0..2 {
                let child = fixture.heap.load_reference(array, index).unwrap();
                assert_eq!(fixture.heap.length(child).unwrap(), counts[1]);
                for element in 0..counts[1] {
                    assert_eq!(
                        fixture.heap.load_reference(child, element).unwrap(),
                        Reference::null()
                    );
                }
            }
            assert_eq!(fixture.heap.allocated_arrays(), 3);
        }
        let ReferenceType::Array { dimensions, .. } =
            fixture.heap.reference_array_type(array).unwrap()
        else {
            panic!("array type")
        };
        assert_eq!(
            usize::from(dimensions),
            descriptor.bytes().take_while(|c| *c == b'[').count()
        );
    }
}
#[test]
fn reached_zeros_stop_allocation_but_every_requested_count_is_scanned() {
    for (counts, nodes, elements) in [
        (vec![0, 7, 9], 1, 0),
        (vec![2, 0, 7], 3, 2),
        (vec![2, 3, 0], 9, 8),
    ] {
        let mut root = ordinary();
        let target = root.class("[[[I");
        let mut code = Vec::new();
        allocation(&mut code, target, &counts);
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 3, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            O::Returned(None)
        );
        assert_eq!(fixture.heap.allocated_arrays(), nodes);
        assert_eq!(fixture.heap.total_elements(), elements);
    }
    for counts in [vec![0, -1], vec![2, 0, -1], vec![-1, 0, 0]] {
        let mut root = ordinary();
        let target = root.class("[[[I");
        let mut code = Vec::new();
        allocation(&mut code, target, &counts);
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 3, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            O::NegativeArraySizeException
        );
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
}
#[test]
fn maximum_dimensions_use_bounded_reverse_extraction_and_iterative_actual_payloads() {
    let mut root = ordinary();
    root.field("array", "Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "Ljava/lang/Object;");
    let target = root.class(&format!("{}D", "[".repeat(255)));
    let mut code = vec![0x2a];
    allocation(&mut code, target, &vec![1; 255]);
    code.extend(op(0xb5, field));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 256, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        O::Returned(None)
    );
    let Value::Reference(mut array) =
        fixture.stored("p/T", "array", "Ljava/lang/Object;", receiver)
    else {
        panic!("array field")
    };
    for rank in (2..=255).rev() {
        let ReferenceType::Array { dimensions, .. } =
            fixture.heap.reference_array_type(array).unwrap()
        else {
            panic!("array type")
        };
        assert_eq!(usize::from(dimensions), rank);
        assert_eq!(fixture.heap.length(array).unwrap(), 1);
        array = fixture.heap.load_reference(array, 0).unwrap();
    }
    assert_eq!(fixture.heap.load(array, 0).unwrap(), Value::Double(0));
    assert_eq!(fixture.heap.allocated_arrays(), 255);
    assert_eq!(fixture.heap.total_elements(), 255);
    assert_eq!(
        fixture.heap.payload_bytes(),
        254 * REFERENCE_ARRAY_ELEMENT_BYTES + 8
    );
}

#[test]
fn typed_reverse_extraction_fails_before_constant_resolution_and_never_checks_signs_early() {
    for (prefix, dimensions, expected) in [
        (vec![], 2, E::StackUnderflow),
        (vec![0x04], 2, E::StackUnderflow),
        (
            vec![0x01, 0x04],
            2,
            E::WrongValueType {
                expected: ValueKind::Int,
                actual: ValueKind::Reference,
            },
        ),
        (
            vec![0x04, 0x01],
            2,
            E::WrongValueType {
                expected: ValueKind::Int,
                actual: ValueKind::Reference,
            },
        ),
        (
            vec![0x09],
            1,
            E::WrongValueType {
                expected: ValueKind::Int,
                actual: ValueKind::Long,
            },
        ),
    ] {
        let mut root = ordinary();
        let index = root.utf8("not Class");
        let mut code = prefix.clone();
        code.extend(multi(index, dimensions));
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert_eq!(error.kind, expected);
        assert_eq!(error.offset, Some(prefix.len() as u32));
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
    let mut root = ordinary();
    let index = root.utf8("not Class");
    let mut code = vec![0x02, 0x01];
    code.extend(multi(index, 2));
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap_err().kind,
        E::WrongValueType {
            expected: ValueKind::Int,
            actual: ValueKind::Reference
        }
    );
}
#[test]
fn target_shape_access_universe_and_missing_definition_precede_negative_counts() {
    for failure in 0..7 {
        let mut root = ordinary();
        let index = match failure {
            0 => root.utf8("not Class"),
            1 => root.class("p/T"),
            2 => root.class("[I"),
            3 => root.class("[[Lq/Hidden;"),
            4 => root.class("[[Lq/Missing;"),
            _ => root.class("[[I"),
        };
        let mut code = vec![0x03, 0x02];
        code.extend(multi(index, 2));
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 2, 1);
        let classes = vec![root, Class::new("q/Hidden", Some("java/lang/Object"), 0x20)];
        let mut fixture = if failure == 5 {
            Fixture::unbound(classes)
        } else {
            Fixture::limited(
                classes,
                HeapOptions {
                    max_arrays: 0,
                    ..HeapOptions::default()
                },
            )
        };
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert!(
                matches!(result.unwrap_err().kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::InvalidConstant{..})))
            ),
            1 | 2 => assert!(
                matches!(result.unwrap_err().kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::InvalidType(_))))
            ),
            3 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            4 => assert!(
                matches!(result.unwrap_err().kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::MissingDefinition{..}))))
            ),
            5 => assert!(
                matches!(result.unwrap_err().kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::MissingUniverse)))
            ),
            _ => assert_eq!(result.unwrap(), O::NegativeArraySizeException),
        }
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
}
#[test]
fn old_entrypoints_gate_before_extraction_and_unreachable_malformed_encodings_decode_eagerly() {
    let mut root = ordinary();
    let index = root.class("[[I");
    let mut code = multi(index, 2);
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
        assert_eq!(error.kind, E::UnsupportedInstruction(0xc5));
        assert_eq!(error.offset, Some(0));
    }
    for tail in [
        vec![0xc5, 0, 1, 0],
        vec![0xc5, 0, 1],
        vec![0xc5, 0],
        vec![0xc5],
    ] {
        let mut root = ordinary();
        let mut code = vec![0xb1];
        code.extend(tail);
        root.method("run", "()V", 1, &code, 0, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        assert!(matches!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::Decode(_)
        ));
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
}
#[test]
fn asymmetric_counts_are_not_reversed_and_unreachable_large_sizes_do_not_charge_quotas() {
    let mut root = ordinary();
    root.field("array", "Ljava/lang/Object;", 1);
    let field = root.reference(9, "p/T", "array", "Ljava/lang/Object;");
    let target = root.class("[[[I");
    let mut code = vec![0x2a, 0x03, 0x1b, 0x1c];
    code.extend(multi(target, 3));
    code.extend(op(0xb5, field));
    code.push(0xb1);
    root.method("run", "(II)V", 1, &code, 4, 3);
    let mut fixture = Fixture::limited(
        vec![root],
        HeapOptions {
            max_arrays: 1,
            max_array_length: 0,
            max_total_elements: 0,
            max_payload_bytes: 0,
        },
    );
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "(II)V");
    assert_eq!(
        fixture
            .run(
                method,
                receiver,
                &[Value::Int(i32::MAX), Value::Int(i32::MAX)]
            )
            .unwrap(),
        O::Returned(None)
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    assert_eq!(fixture.heap.total_elements(), 0);
    let Value::Reference(array) = fixture.stored("p/T", "array", "Ljava/lang/Object;", receiver)
    else {
        panic!("array field")
    };
    assert_eq!(fixture.heap.length(array).unwrap(), 0);
    assert_eq!(
        fixture
            .run(method, receiver, &[Value::Int(i32::MAX), Value::Int(-1)])
            .unwrap(),
        O::NegativeArraySizeException
    );
    assert_eq!(fixture.heap.allocated_arrays(), 1);
}

#[test]
fn exact_shared_geometry_quotas_are_checked_atomically_and_conflicts_follow_level_order() {
    for failure in 0..6 {
        let mut root = ordinary();
        let target = root.class("[[I");
        let mut code = Vec::new();
        allocation(&mut code, target, &[2, 3]);
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 2, 1);
        let exact = HeapOptions {
            max_arrays: 4,
            max_array_length: 3,
            max_total_elements: 9,
            max_payload_bytes: 28 + 2 * REFERENCE_ARRAY_ELEMENT_BYTES,
        };
        let options = match failure {
            0 => exact,
            1 => HeapOptions {
                max_arrays: 3,
                ..exact
            },
            2 => HeapOptions {
                max_array_length: 2,
                ..exact
            },
            3 => HeapOptions {
                max_total_elements: 8,
                ..exact
            },
            4 => HeapOptions {
                max_payload_bytes: exact.max_payload_bytes - 1,
                ..exact
            },
            _ => HeapOptions {
                max_arrays: 2,
                max_array_length: 1,
                ..exact
            },
        };
        let mut fixture = Fixture::limited(vec![root], options);
        let old = fixture
            .heap
            .allocate_primitive_array(PrimitiveArrayKind::Int, 1)
            .unwrap();
        fixture.heap.store(old, 0, Value::Int(41)).unwrap();
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.run(method, receiver, &[]);
        if failure == 0 {
            assert_eq!(result.unwrap(), O::Returned(None));
            assert_eq!(fixture.heap.allocated_arrays(), 4);
            assert_eq!(fixture.heap.total_elements(), 9);
            assert_eq!(fixture.heap.payload_bytes(), exact.max_payload_bytes);
        } else {
            let expected = match failure {
                1 => crabspot_runtime::HeapResource::Arrays,
                2 | 5 => crabspot_runtime::HeapResource::ArrayLength,
                3 => crabspot_runtime::HeapResource::TotalElements,
                _ => crabspot_runtime::HeapResource::PayloadBytes,
            };
            assert!(
                matches!(result.unwrap_err().kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Heap(HeapError::ResourceLimit{resource,..}) if *resource==expected))
            );
            assert_eq!(fixture.heap.allocated_arrays(), 1);
            assert_eq!(fixture.heap.total_elements(), 1);
            assert_eq!(fixture.heap.payload_bytes(), 4);
        }
        assert_eq!(fixture.heap.load(old, 0).unwrap(), Value::Int(41));
        assert_eq!(fixture.heap.children(old).unwrap().count(), 0);
        assert_eq!(fixture.heap.allocated_instances(), 1);
    }
}
#[test]
fn exact_cumulative_work_and_atomic_bulk_reservation_preserve_live_heap() {
    let mut root = ordinary();
    let target = root.class("[[I");
    let mut code = Vec::new();
    allocation(&mut code, target, &[2, 3]);
    code.extend([0x57, 0xb1]);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
    let method = fixture.method("p/T", "run", "()V");
    let mut resolver = SameLoaderResolver;
    let mut probe = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    probe.check_receiver(owner, receiver).unwrap();
    probe
        .new_multidimensional_array(owner, target, &[2, 3])
        .unwrap();
    let exact = probe.lookup_steps();
    drop(probe);
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
    assert!(
        matches!(error.kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::ResourceLimit{resource:crabspot_runtime::FieldResource::LookupSteps,limit,required}) if *limit==exact-1&&*required==exact)))
    );
    assert_eq!(context.lookup_steps(), exact - 5);
    assert_eq!(context.heap().allocated_arrays(), 3);
    drop(context);
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
        O::Returned(None)
    );
    assert_eq!(context.lookup_steps(), exact);
    assert_eq!(context.heap().allocated_arrays(), 6);
}
#[test]
fn lookup_policy_can_stop_negative_scan_after_zero_before_java_size_outcome() {
    let mut root = ordinary();
    let target = root.class("[[I");
    let mut code = Vec::new();
    allocation(&mut code, target, &[0, -1]);
    code.extend([0x57, 0xb1]);
    root.method("run", "()V", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
    let method = fixture.method("p/T", "run", "()V");
    let mut resolver = SameLoaderResolver;
    let mut probe = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    probe.check_receiver(owner, receiver).unwrap();
    assert!(matches!(
        probe.new_multidimensional_array(owner, target, &[0, -1]),
        Err(ArrayAccessError::Java(
            crabspot_runtime::ArrayJavaError::NegativeArraySize
        ))
    ));
    let exact = probe.lookup_steps();
    drop(probe);
    for (limit, java) in [(exact - 1, false), (exact, true)] {
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &fixture.registry,
            &mut fixture.heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let result = execute_instance_method_body(
            &mut context,
            method,
            receiver,
            &[],
            ExecutionOptions::default(),
            CallLimits::default(),
        );
        if java {
            assert_eq!(result.unwrap(), O::NegativeArraySizeException);
        } else {
            assert!(
                matches!(result.unwrap_err().kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::ResourceLimit{..}))))
            );
        }
        assert_eq!(context.heap().allocated_arrays(), 0);
    }
}
#[test]
fn exact_instruction_frame_code_and_operand_policies_keep_committed_array_graphs() {
    let mut root = ordinary();
    let target = root.class("[[I");
    let mut code = Vec::new();
    allocation(&mut code, target, &[2, 3]);
    code.extend([0xbe, 0xac]);
    let bytes = code.len();
    root.method("run", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    let limits = CallLimits {
        max_frames: 1,
        max_frame_slots: 3,
        max_prepared_code_bytes: bytes,
    };
    for (budget, limits, expected) in [
        (0, limits, E::InstructionBudgetExhausted { limit: 0 }),
        (
            5,
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
            5,
            CallLimits {
                max_frame_slots: 2,
                ..limits
            },
            E::FrameSlotLimit {
                limit: 2,
                required: 3,
            },
        ),
        (
            5,
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
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
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
    assert_eq!(fixture.heap.allocated_arrays(), 3);
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: 5
                },
                limits
            )
            .unwrap(),
        returned(Value::Int(2))
    );
    assert_eq!(fixture.heap.allocated_arrays(), 6);
    let mut root = ordinary();
    let target = root.class("[[I");
    let mut code = Vec::new();
    allocation(&mut code, target, &[2, 3]);
    code.push(0xb1);
    root.method("run", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap_err().kind,
        E::StackOverflow {
            required: 2,
            capacity: 1
        }
    );
    assert_eq!(fixture.heap.allocated_arrays(), 0);
}

#[test]
fn selected_declaring_origin_controls_allocation_across_virtual_special_interface_and_static_calls()
{
    let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    let target = base.class("[[Lq/Hidden;");
    let helper = base.reference(10, "q/Base", "helper", "()I");
    let mut code = Vec::new();
    allocation(&mut code, target, &[1, 2]);
    code.push(0xbe);
    code.extend(op(0xb8, helper));
    code.extend([0x60, 0xac]);
    base.method("baseBody", "()I", 1, &code, 2, 1);
    let mut code = Vec::new();
    allocation(&mut code, target, &[2]);
    code.extend([0xbe, 0xac]);
    base.method("helper", "()I", 9, &code, 1, 0);
    let mut interface = iface("q/I");
    let target = interface.class("[[Lq/Hidden;");
    let mut code = Vec::new();
    allocation(&mut code, target, &[2, 1]);
    code.extend([0xbe, 0xac]);
    interface.method("interfaceBody", "()I", 1, &code, 2, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    for (name, opcode, owner, body) in [
        ("virtual", 0xb6, "q/Base", "baseBody"),
        ("special", 0xb7, "q/Base", "baseBody"),
        ("interface", 0xb9, "q/I", "interfaceBody"),
    ] {
        let call = root.reference(if opcode == 0xb9 { 11 } else { 10 }, owner, body, "()I");
        let mut code = vec![0x2a];
        code.extend(if opcode == 0xb9 {
            interface_call(call)
        } else {
            op(opcode, call)
        });
        code.push(0xac);
        root.method(name, "()I", 1, &code, 1, 1);
    }
    let hidden_target = root.class("[[Lq/Hidden;");
    let mut code = Vec::new();
    allocation(&mut code, hidden_target, &[-1, -1]);
    code.extend([0xbe, 0xac]);
    root.method("inaccessible", "()I", 1, &code, 2, 1);
    let mut fixture = Fixture::new(vec![hidden, base, interface, root]);
    let receiver = fixture.object("p/T");
    for (name, result) in [("virtual", 3), ("special", 3), ("interface", 2)] {
        let method = fixture.method("p/T", name, "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(result))
        );
    }
    let method = fixture.method("p/T", "inaccessible", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        O::IllegalAccessError
    );
    assert_eq!(fixture.heap.allocated_arrays(), 9);
}
#[test]
fn selected_method_and_transition_site_provenance_survive_missing_target_resolution() {
    for interface in [false, true] {
        let mut body = if interface {
            iface("q/I")
        } else {
            Class::new("q/Base", Some("java/lang/Object"), 0x21)
        };
        let owner = body.name.clone();
        let target = body.class("[[Lq/Missing;");
        let mut code = Vec::new();
        allocation(&mut code, target, &[0, -1]);
        code.extend([0xbe, 0xac]);
        body.method("body", "()I", 1, &code, 2, 1);
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
        let call = root.reference(if interface { 11 } else { 10 }, &owner, "body", "()I");
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
        assert_eq!(error.offset, Some(2));
        assert_eq!(
            error.method.as_ref().unwrap().owner,
            Some(fixture.classes[&owner])
        );
        assert_eq!(error.call_site.as_ref().unwrap().offset, 1);
        assert_eq!(
            error.call_site.as_ref().unwrap().method.owner,
            Some(fixture.classes["p/T"])
        );
        assert!(
            matches!(error.kind,E::ArrayAccess(error) if matches!(error.as_ref(),ArrayAccessError::Type(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::MissingDefinition{origin,..}) if *origin==fixture.classes[&owner])))
        );
        assert_eq!(fixture.heap.allocated_arrays(), 0);
    }
}
#[test]
fn committed_nested_graph_and_element_effects_survive_later_java_and_host_failures() {
    for failure in 0..9 {
        let mut root = ordinary();
        root.field("array", "Ljava/lang/Object;", 1);
        let field = root.reference(9, "p/T", "array", "Ljava/lang/Object;");
        let target = root.class("[[Lp/T;");
        let missing = root.class("[[Lq/Missing;");
        let mut code = vec![0x2a];
        allocation(&mut code, target, &[2, 2]);
        code.extend(op(0xb5, field));
        code.push(0x2a);
        code.extend(op(0xb4, field));
        code.extend([0x03, 0x32, 0x03, 0x2a, 0x53]);
        let expected = match failure {
            0 => {
                allocation(&mut code, target, &[0, -1]);
                code.extend([0x57, 0xb1]);
                Some(O::NegativeArraySizeException)
            }
            1 => {
                allocation(&mut code, 1, &[1, 1]);
                code.extend([0x57, 0xb1]);
                None
            }
            2 => {
                code.extend([0x01, 0x03, 0x32, 0x57, 0xb1]);
                Some(O::NullPointerException)
            }
            3 => {
                code.push(0x2a);
                code.extend(op(0xb4, field));
                code.extend([0x05, 0x32, 0x57, 0xb1]);
                Some(O::ArrayIndexOutOfBoundsException)
            }
            4 => {
                code.push(0x2a);
                code.extend(op(0xb4, field));
                code.extend([0x03, 0x32, 0x03, 0x03, 0xbc, 10, 0x53, 0xb1]);
                Some(O::ArrayStoreException)
            }
            5 => {
                code.extend([0x04, 0x03, 0x6c, 0x57, 0xb1]);
                Some(O::ArithmeticException)
            }
            6 => {
                code.extend([0xc2, 0xb1]);
                None
            }
            7 => {
                allocation(&mut code, missing, &[1, 1]);
                code.extend([0x57, 0xb1]);
                None
            }
            _ => {
                allocation(&mut code, target, &[1, 1]);
                code.extend([0x57, 0xb1]);
                None
            }
        };
        root.method("run", "()V", 1, &code, 3, 1);
        let mut fixture = Fixture::limited(
            vec![root],
            HeapOptions {
                max_arrays: if failure == 8 { 3 } else { 4096 },
                ..HeapOptions::default()
            },
        );
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let result = fixture.run(method, receiver, &[]);
        if let Some(expected) = expected {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(result.is_err());
        }
        let Value::Reference(array) =
            fixture.stored("p/T", "array", "Ljava/lang/Object;", receiver)
        else {
            panic!("array field")
        };
        let first = fixture.heap.load_reference(array, 0).unwrap();
        let last = fixture.heap.load_reference(array, 1).unwrap();
        assert_ne!(first, last);
        assert_eq!(fixture.heap.load_reference(first, 0).unwrap(), receiver);
        assert_eq!(
            fixture.heap.load_reference(first, 1).unwrap(),
            Reference::null()
        );
        assert_eq!(
            fixture.heap.children(array).unwrap().collect::<Vec<_>>(),
            vec![first.heap_ref().unwrap(), last.heap_ref().unwrap()]
        );
        assert_eq!(
            fixture.heap.children(first).unwrap().collect::<Vec<_>>(),
            vec![receiver.heap_ref().unwrap()]
        );
        assert_eq!(
            fixture.heap.allocated_arrays(),
            if failure == 4 { 4 } else { 3 }
        );
        assert_eq!(fixture.heap.total_elements(), 6);
        assert_eq!(
            fixture.heap.payload_bytes(),
            6 * REFERENCE_ARRAY_ELEMENT_BYTES
        );
    }
}
#[test]
fn a_single_requested_primitive_dimension_uses_original_payload_and_exact_zero_bits() {
    for (name, expected, load, ret) in [
        ("[I", Value::Int(0), 0x2e, 0xac),
        ("[J", Value::Long(0), 0x2f, 0xad),
        ("[F", Value::Float(0), 0x30, 0xae),
        ("[D", Value::Double(0), 0x31, 0xaf),
    ] {
        let mut root = ordinary();
        let target = root.class(name);
        let mut code = Vec::new();
        allocation(&mut code, target, &[1]);
        code.extend([0x03, load, ret]);
        let descriptor = format!("(){}", &name[1..]);
        root.method("run", &descriptor, 1, &code, 2, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", &descriptor);
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(expected)
        );
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.heap.total_elements(), 1);
    }
}

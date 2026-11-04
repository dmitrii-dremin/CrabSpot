//! Numeric wrappers exercising runtime-aware casts and reference type tests.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExecutionError, ExecutionErrorKind as E, ExecutionOptions, ExecutionOutcome as O,
    Value, ValueKind, execute_instance_method_body, execute_static_method_body,
    execute_static_method_body_with_heap,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap, MethodId, Reference,
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
#[test]
fn nominal_and_inherited_interface_tests_produce_int_results_and_class_cast_outcomes() {
    for (target, matches) in [
        ("p/T", true),
        ("q/Base", true),
        ("java/lang/Object", true),
        ("q/I", true),
        ("q/J", true),
        ("q/Other", false),
        ("java/lang/Cloneable", false),
        ("[I", false),
        ("[Ljava/lang/Object;", false),
    ] {
        for opcode in [0xc0, 0xc1] {
            let base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
            let interface = iface("q/I");
            let mut child = iface("q/J");
            implement(&mut child, "q/I");
            let other = Class::new("q/Other", Some("java/lang/Object"), 0x21);
            let mut root = Class::new("p/T", Some("q/Base"), 0x21);
            implement(&mut root, "q/J");
            let target = root.class(target);
            let mut code = vec![0x2a];
            code.extend(op(opcode, target));
            if opcode == 0xc0 {
                code.extend([0x57, 0x04]);
            }
            code.push(0xac);
            root.method("run", "()I", 1, &code, 1, 1);
            let mut fixture = Fixture::new(vec![base, interface, child, other, root]);
            let receiver = fixture.object("p/T");
            let method = fixture.method("p/T", "run", "()I");
            let expected = if opcode == 0xc0 && !matches {
                O::ClassCastException
            } else {
                returned(Value::Int(if opcode == 0xc0 {
                    1
                } else {
                    i32::from(matches)
                }))
            };
            assert_eq!(fixture.run(method, receiver, &[]).unwrap(), expected);
            assert_eq!(fixture.heap.allocated_instances(), 1);
        }
    }
}
#[test]
fn all_primitive_array_kinds_respect_exact_components_core_supertypes_and_reference_array_negatives()
 {
    for (atype, source) in [
        (4, "[Z"),
        (5, "[C"),
        (6, "[F"),
        (7, "[D"),
        (8, "[B"),
        (9, "[S"),
        (10, "[I"),
        (11, "[J"),
    ] {
        for (target, matches) in [
            (source, true),
            (if source == "[I" { "[Z" } else { "[I" }, false),
            ("java/lang/Object", true),
            ("java/lang/Cloneable", true),
            ("java/io/Serializable", true),
            ("p/T", false),
            ("[Ljava/lang/Object;", false),
            ("[[I", false),
        ] {
            for opcode in [0xc0, 0xc1] {
                let mut root = ordinary();
                let target = root.class(target);
                let mut code = vec![0x03, 0xbc, atype];
                code.extend(op(opcode, target));
                if opcode == 0xc0 {
                    code.push(0xbe);
                }
                code.push(0xac);
                root.method("run", "()I", 1, &code, 1, 1);
                let mut fixture = Fixture::new(vec![root]);
                let receiver = fixture.object("p/T");
                let method = fixture.method("p/T", "run", "()I");
                let expected = if opcode == 0xc0 && !matches {
                    O::ClassCastException
                } else {
                    returned(Value::Int(if opcode == 0xc0 {
                        0
                    } else {
                        i32::from(matches)
                    }))
                };
                assert_eq!(
                    fixture.run(method, receiver, &[]).unwrap(),
                    expected,
                    "atype {atype}, opcode {opcode:x}"
                );
                assert_eq!(fixture.heap.allocated_arrays(), 1);
            }
        }
    }
}
#[test]
fn structural_target_rank_255_does_not_imply_reference_array_allocation() {
    for opcode in [0xc0, 0xc1] {
        let mut root = ordinary();
        let target = root.class(&format!("{}I", "[".repeat(255)));
        let mut code = vec![0x03, 0xbc, 10];
        code.extend(op(opcode, target));
        if opcode == 0xc0 {
            code.push(0x57);
            code.push(0x04);
        }
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            if opcode == 0xc0 {
                O::ClassCastException
            } else {
                returned(Value::Int(0))
            }
        );
        assert_eq!(fixture.heap.allocated_arrays(), 1);
    }
}
#[test]
fn null_skips_universe_and_missing_or_inaccessible_target_resolution() {
    for target in ["q/Missing", "q/Hidden", "[[Lq/Missing;"] {
        for opcode in [0xc0, 0xc1] {
            let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
            let mut root = ordinary();
            let target = root.class(target);
            let mut code = vec![0x01];
            code.extend(op(opcode, target));
            if opcode == 0xc0 {
                code.extend([0xc6, 0, 5, 0x03, 0xac, 0x04, 0xac]);
            } else {
                code.push(0xac);
            }
            root.method("run", "()I", 1, &code, 1, 1);
            let mut fixture = Fixture::unbound(vec![hidden, root]);
            let receiver = fixture.object("p/T");
            let method = fixture.method("p/T", "run", "()I");
            assert_eq!(
                fixture.run(method, receiver, &[]).unwrap(),
                returned(Value::Int(i32::from(opcode == 0xc0)))
            );
            assert!(fixture.heap.reference_type_universe().is_none());
        }
    }
}
#[test]
fn nonnull_requires_a_universe_before_target_resolution_and_access_precedes_cast_failure() {
    for opcode in [0xc0, 0xc1] {
        let mut root = ordinary();
        let target = root.class("q/Missing");
        let mut code = vec![0x2a];
        code.extend(op(opcode, target));
        if opcode == 0xc0 {
            code.extend([0x57, 0x03]);
        }
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::unbound(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::TypeAccess(Box::new(TypeAccessError::MissingUniverse))
        );
    }
    for target in ["q/Hidden", "[Lq/Hidden;"] {
        for opcode in [0xc0, 0xc1] {
            let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
            let mut root = ordinary();
            let target = root.class(target);
            let mut code = vec![0x2a];
            code.extend(op(opcode, target));
            code.extend([0x57, 0x03, 0xac]);
            root.method("run", "()I", 1, &code, 1, 1);
            let mut fixture = Fixture::new(vec![hidden, root]);
            let receiver = fixture.object("p/T");
            let method = fixture.method("p/T", "run", "()I");
            assert_eq!(
                fixture.run(method, receiver, &[]).unwrap(),
                O::IllegalAccessError
            );
        }
    }
}
#[test]
fn cp_shape_is_checked_even_for_null_but_typed_stack_errors_precede_runtime_cp_checks() {
    for opcode in [0xc0, 0xc1] {
        let mut root = ordinary();
        let mut code = vec![0x01];
        code.extend(op(opcode, 1));
        code.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &code, 1, 1);
        let mut fixture = Fixture::unbound(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert_eq!(error.offset, Some(1));
        assert!(
            matches!(error.kind,E::TypeAccess(error) if matches!(*error,TypeAccessError::InvalidConstant {..}))
        );
        for prefix in [vec![], vec![0x03]] {
            let mut root = ordinary();
            let mut code = prefix;
            code.extend(op(opcode, 1));
            code.extend([0x57, 0xb1]);
            root.method("run", "()V", 1, &code, 1, 1);
            let mut fixture = Fixture::unbound(vec![root]);
            let receiver = fixture.object("p/T");
            let method = fixture.method("p/T", "run", "()V");
            assert!(matches!(
                fixture.run(method, receiver, &[]).unwrap_err().kind,
                E::StackUnderflow
                    | E::WrongValueType {
                        expected: ValueKind::Reference,
                        actual: ValueKind::Int
                    }
            ));
        }
    }
}
#[test]
fn casts_preserve_reference_identity_other_operands_and_subsequent_field_array_and_call_use() {
    let mut root = ordinary();
    let target = root.class("p/T");
    let mut code = vec![0x2a, 0x59];
    code.extend(op(0xc0, target));
    code.extend([0xa5, 0, 5, 0x03, 0xac, 0x04, 0xac]);
    root.method("identity", "()I", 1, &code, 2, 1);
    let mut code = vec![0x10, 42, 0x2a];
    code.extend(op(0xc0, target));
    code.extend([0x57, 0xac]);
    root.method("operands", "()I", 1, &code, 2, 1);
    root.field("f", "I", 1);
    let field = root.reference(9, "p/T", "f", "I");
    let call = root.reference(10, "p/T", "helper", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xc0, target));
    code.extend([0x59, 0x10, 7]);
    code.extend(op(0xb5, field));
    code.extend(op(0xb6, call));
    code.push(0xac);
    root.method("useObject", "()I", 1, &code, 3, 1);
    let mut code = vec![0x2a];
    code.extend(op(0xb4, field));
    code.push(0xac);
    root.method("helper", "()I", 1, &code, 1, 1);
    let target = root.class("[I");
    let mut code = vec![0x04, 0xbc, 10];
    code.extend(op(0xc0, target));
    code.extend([0x59, 0x03, 0x10, 9, 0x4f, 0x03, 0x2e, 0xac]);
    root.method("useArray", "()I", 1, &code, 4, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    for (name, expected) in [
        ("identity", 1),
        ("operands", 42),
        ("useObject", 7),
        ("useArray", 9),
    ] {
        let method = fixture.method("p/T", name, "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(expected))
        );
    }
    assert_eq!(fixture.heap.allocated_arrays(), 1);
    assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(7));
}
#[test]
fn old_entry_points_still_reject_type_opcodes_and_decode_checks_remain_eager() {
    for opcode in [0xc0, 0xc1] {
        let mut root = ordinary();
        let mut code = op(opcode, 1);
        code.push(0xb1);
        root.method("staticBody", "()V", 9, &code, 0, 0);
        let mut fixture = Fixture::unbound(vec![root]);
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
        let mut root = ordinary();
        root.method("run", "()V", 1, &[0xb1, opcode, 0], 0, 1);
        let mut fixture = Fixture::unbound(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()V");
        assert!(matches!(
            fixture.run(method, receiver, &[]).unwrap_err().kind,
            E::Decode(_)
        ));
    }
    let mut root = ordinary();
    root.method("run", "()I", 1, &[0x08, 0xac, 0xc0, 0, 1], 1, 1);
    let mut fixture = Fixture::unbound(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap(),
        returned(Value::Int(5))
    );
}

#[test]
fn every_call_kind_uses_current_declaring_origin_for_target_access_and_static_helpers() {
    let mut hidden = iface("q/Target");
    hidden.flags = 0x600;
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    implement(&mut base, "q/Target");
    let target = base.class("q/Target");
    let helper = base.reference(10, "q/Base", "helper", "()I");
    let mut code = vec![0x2a];
    code.extend(op(0xc0, target));
    code.push(0x57);
    code.extend(op(0xb8, helper));
    code.extend([0x04, 0x60, 0xac]);
    base.method("body", "()I", 1, &code, 2, 1);
    let mut code = vec![0x03, 0xbc, 10];
    code.extend(op(0xc1, target));
    code.push(0xac);
    base.method("helper", "()I", 9, &code, 1, 0);
    let mut interface = iface("q/I");
    let target = interface.class("q/Target");
    let mut code = vec![0x2a];
    code.extend(op(0xc0, target));
    code.extend([0x57, 0x05, 0xac]);
    interface.method("interfaceBody", "()I", 1, &code, 1, 1);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    let class_call = root.reference(10, "q/Base", "body", "()I");
    let iface_call = root.reference(11, "q/I", "interfaceBody", "()I");
    for (name, opcode, call) in [("virtual", 0xb6, class_call), ("special", 0xb7, class_call)] {
        let mut code = vec![0x2a];
        code.extend(op(opcode, call));
        code.push(0xac);
        root.method(name, "()I", 1, &code, 1, 1);
    }
    let mut code = vec![0x2a];
    code.extend(interface_call(iface_call));
    code.push(0xac);
    root.method("interface", "()I", 1, &code, 1, 1);
    let target = root.class("q/Target");
    let mut code = vec![0x2a];
    code.extend(op(0xc0, target));
    code.extend([0x57, 0x03, 0xac]);
    root.method("direct", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![hidden, base, interface, root]);
    let receiver = fixture.object("p/T");
    for name in ["virtual", "special", "interface"] {
        let method = fixture.method("p/T", name, "()I");
        assert_eq!(
            fixture.run(method, receiver, &[]).unwrap(),
            returned(Value::Int(if name == "interface" { 2 } else { 1 })),
            "{name}"
        );
    }
    let direct = fixture.method("p/T", "direct", "()I");
    assert_eq!(
        fixture.run(direct, receiver, &[]).unwrap(),
        O::IllegalAccessError
    );
}
#[test]
fn unresolved_targets_keep_type_host_sources_and_nonroot_method_and_call_site_provenance() {
    for interface_target in [false, true] {
        let mut owner = if interface_target {
            iface("q/Owner")
        } else {
            Class::new("q/Owner", Some("java/lang/Object"), 0x21)
        };
        let target = owner.class("q/Missing");
        let mut code = vec![0x2a];
        code.extend(op(0xc1, target));
        code.push(0xac);
        owner.method("body", "()I", 1, &code, 1, 1);
        let mut root = Class::new(
            "p/T",
            Some(if interface_target {
                "java/lang/Object"
            } else {
                "q/Owner"
            }),
            0x21,
        );
        if interface_target {
            implement(&mut root, "q/Owner");
        }
        let call = root.reference(
            if interface_target { 11 } else { 10 },
            "q/Owner",
            "body",
            "()I",
        );
        let mut code = vec![0x2a];
        if interface_target {
            code.extend(interface_call(call));
        } else {
            code.extend(op(0xb7, call));
        }
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![owner, root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let error = fixture.run(method, receiver, &[]).unwrap_err();
        assert_eq!(error.offset, Some(1));
        assert_eq!(
            error.method.as_ref().unwrap().owner,
            Some(fixture.classes["q/Owner"])
        );
        assert_eq!(
            error.call_site.as_ref().unwrap().method.owner,
            Some(fixture.classes["p/T"])
        );
        assert!(
            matches!(&error.kind,E::TypeAccess(error) if matches!(error.as_ref(),TypeAccessError::Context(FieldAccessError::MissingDefinition {origin,..}) if *origin==fixture.classes["q/Owner"]))
        );
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.to_string().contains("type service"));
    }
}
#[test]
fn each_type_instruction_consumes_one_shared_fuel_unit_with_root_reservations_unchanged() {
    for opcode in [0xc0, 0xc1] {
        let mut root = ordinary();
        let target = root.class("p/T");
        let mut code = vec![0x2a];
        code.extend(op(opcode, target));
        if opcode == 0xc0 {
            code.extend([0x57, 0x04]);
        }
        code.push(0xac);
        let code_bytes = code.len();
        let budget = if opcode == 0xc0 { 5 } else { 3 };
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let limits = CallLimits {
            max_frames: 1,
            max_frame_slots: 2,
            max_prepared_code_bytes: code_bytes,
        };
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
                .unwrap(),
            returned(Value::Int(1))
        );
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[],
                    ExecutionOptions {
                        instruction_budget: 1
                    },
                    limits
                )
                .unwrap_err()
                .kind,
            E::InstructionBudgetExhausted { limit: 1 }
        );
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits {
                        max_frame_slots: 1,
                        ..limits
                    }
                )
                .unwrap_err()
                .kind,
            E::FrameSlotLimit {
                limit: 1,
                required: 2
            }
        );
        assert_eq!(
            fixture
                .run_limits(
                    method,
                    receiver,
                    &[],
                    ExecutionOptions::default(),
                    CallLimits {
                        max_prepared_code_bytes: code_bytes - 1,
                        ..limits
                    }
                )
                .unwrap_err()
                .kind,
            E::PreparedCodeLimit {
                limit: code_bytes - 1,
                required: code_bytes
            }
        );
    }
    let mut root = ordinary();
    let target = root.class("[I");
    let mut code = vec![0x2a];
    code.extend(op(0xc0, target));
    code.extend([0x57, 0xb1]);
    root.method("run", "()V", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture
            .run_limits(
                method,
                receiver,
                &[],
                ExecutionOptions {
                    instruction_budget: 2
                },
                CallLimits::default()
            )
            .unwrap(),
        O::ClassCastException
    );
}
#[test]
fn repeated_type_tests_share_cumulative_work_policy_without_a_second_type_cache() {
    let mut root = ordinary();
    let target = root.class("p/T");
    let mut code = vec![0x2a];
    code.extend(op(0xc1, target));
    code.extend([0x57, 0x2a]);
    code.extend(op(0xc1, target));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let owner = fixture.classes["p/T"];
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
        assert!(context.instance_of(owner, target, receiver).unwrap());
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
        matches!(error.kind,E::TypeAccess(error) if matches!(*error,TypeAccessError::Context(FieldAccessError::ResourceLimit {resource:crabspot_runtime::FieldResource::LookupSteps,..})))
    );
}
#[test]
fn long_null_target_names_do_not_scan_resolve_or_insert_target_bindings() {
    struct NeverResolve {
        calls: usize,
    }
    impl crabspot_runtime::HierarchyResolver for NeverResolve {
        fn resolve(
            &mut self,
            _: &DefinitionRegistry,
            _: crabspot_runtime::LoaderId,
            _: &[u16],
        ) -> Result<Option<ClassId>, crabspot_runtime::ResolveError> {
            self.calls += 1;
            Err(crabspot_runtime::ResolveError::new("null must not resolve"))
        }
    }
    for opcode in [0xc0, 0xc1] {
        let mut root = ordinary();
        let target = root.class(&"M".repeat(65_000));
        let mut code = vec![0x01];
        code.extend(op(opcode, target));
        if opcode == 0xc0 {
            code.extend([0x57, 0x03]);
        }
        code.push(0xac);
        root.method("run", "()I", 1, &code, 1, 1);
        let mut fixture = Fixture::unbound(vec![root]);
        let receiver = fixture.object("p/T");
        let owner = fixture.classes["p/T"];
        let method = fixture.method("p/T", "run", "()I");
        let mut resolver = NeverResolve { calls: 0 };
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
                .instance_of(owner, target, Reference::null())
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
        let symbols = context.resolved_symbols();
        let units = context.symbol_units();
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
            returned(Value::Int(0))
        );
        assert_eq!(context.resolved_symbols(), symbols);
        assert_eq!(context.symbol_units(), units);
        drop(context);
        assert_eq!(resolver.calls, 0);
    }
}
#[test]
fn failed_cast_access_or_target_resolution_keeps_completed_field_and_array_effects() {
    for failure in 0..6 {
        let hidden = Class::new("q/Hidden", Some("java/lang/Object"), 0x20);
        let mut root = ordinary();
        root.field("f", "I", 1);
        let field = root.reference(9, "p/T", "f", "I");
        let target = root.class(match failure {
            0 => "[I",
            1 => "q/Hidden",
            2 => "q/Missing",
            _ => "p/T",
        });
        let mut code = vec![0x2a, 0x10, 7];
        code.extend(op(0xb5, field));
        code.extend([0x03, 0xbc, 10, 0x57]);
        if failure == 4 {
            code.push(0x03);
        } else {
            code.push(0x2a);
        }
        code.extend(op(0xc0, if failure == 3 { 1 } else { target }));
        code.extend([0x57, 0x04, 0xac]);
        root.method("run", "()I", 1, &code, 2, 1);
        let mut fixture = if failure == 5 {
            Fixture::unbound(vec![hidden, root])
        } else {
            Fixture::new(vec![hidden, root])
        };
        let receiver = fixture.object("p/T");
        let method = fixture.method("p/T", "run", "()I");
        let result = fixture.run(method, receiver, &[]);
        match failure {
            0 => assert_eq!(result.unwrap(), O::ClassCastException),
            1 => assert_eq!(result.unwrap(), O::IllegalAccessError),
            2 | 3 | 5 => assert!(matches!(result.unwrap_err().kind, E::TypeAccess(_))),
            _ => assert!(matches!(result.unwrap_err().kind, E::WrongValueType { .. })),
        };
        assert_eq!(fixture.heap.allocated_arrays(), 1);
        assert_eq!(fixture.stored("p/T", "f", "I", receiver), Value::Int(7));
    }
}
#[test]
fn binding_a_previously_unbound_heap_invalidates_an_existing_execution_context() {
    let mut root = ordinary();
    let target = root.class("p/T");
    let mut code = vec![0x01];
    code.extend(op(0xc1, target));
    code.push(0xac);
    root.method("run", "()I", 1, &code, 1, 1);
    let mut fixture = Fixture::unbound(vec![root]);
    let receiver = fixture.object("p/T");
    let method = fixture.method("p/T", "run", "()I");
    let universe = ReferenceTypeUniverse {
        bootstrap_loader: fixture
            .registry
            .defining_loader(fixture.classes["p/T"])
            .unwrap(),
        object: fixture.classes["java/lang/Object"],
        cloneable: fixture.classes["java/lang/Cloneable"],
        serializable: fixture.classes["java/io/Serializable"],
    };
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
        .bind_reference_types(&fixture.registry, universe)
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

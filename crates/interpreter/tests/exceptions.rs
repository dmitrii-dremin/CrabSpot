//! Supplied Throwable identities, ordered handlers and iterative abrupt completion.
use crabspot_classfile::ParseOptions;
use crabspot_interpreter::{
    CallLimits, ExceptionLimits, ExceptionOutcome as EO, ExecutionError, ExecutionErrorKind as E,
    ExecutionOptions, ExecutionOutcome as O, Value, ValueKind, execute_instance_method_body,
    execute_instance_method_body_with_exceptions, execute_static_method_body,
};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError, FieldResolutionContext, FieldResolutionOptions,
    HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap, MethodId, PrimitiveArrayKind,
    Reference, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver, build_hierarchy,
};
use crabspot_runtime::{ExceptionAccessError, ExceptionProfile, VmExceptionKind};
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
fn same_frame_ordered_overlapping_typed_catches_keep_the_first_matching_row() {
    let mut root = ordinary();
    let base = root.class("q/BaseThrow");
    let throwable = root.class("java/lang/Throwable");
    root.method(
        "run",
        "(Ljava/lang/Throwable;)I",
        1,
        &[0x2b, 0xbf, 0x57, 0x04, 0xac, 0x57, 0x05, 0xac],
        1,
        2,
    );
    handlers(
        &mut root,
        &[[0, 2, 2, base], [0, 2, 5, throwable], [0, 2, 5, 0]],
    );
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let child = fixture.object("q/ChildThrow");
    let other = fixture.object("java/lang/Throwable");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", "(Ljava/lang/Throwable;)I");
    assert_eq!(
        fixture
            .e_run(&profile, method, receiver, &[Value::Reference(child)])
            .unwrap(),
        caught_int(1)
    );
    assert_eq!(
        fixture
            .e_run(&profile, method, receiver, &[Value::Reference(other)])
            .unwrap(),
        caught_int(2)
    );
}
#[test]
fn supplied_throw_uncaught_and_rethrow_preserve_exact_references_and_unused_operand_discard() {
    let mut root = ordinary();
    root.method(
        "run",
        "(Ljava/lang/Throwable;)V",
        1,
        &[0x08, 0x2b, 0xbf],
        2,
        2,
    );
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("q/ChildThrow");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", "(Ljava/lang/Throwable;)V");
    assert_eq!(
        fixture
            .e_run(&profile, method, receiver, &[Value::Reference(thrown)])
            .unwrap(),
        EO::Uncaught(thrown)
    );
    assert_eq!(fixture.heap.allocated_instances(), 2);
    assert_eq!(fixture.heap.allocated_arrays(), 0);
}
#[test]
fn handler_entry_preserves_locals_but_replaces_every_old_operand_and_slot() {
    for fail in [false, true] {
        let mut root = ordinary();
        let mut code = vec![0x08, 0x3d, 0x0a, 0x07, 0x2b, 0xbf, 0x4e];
        code.extend(if fail {
            vec![0x57, 0x03, 0xac]
        } else {
            vec![0x1c, 0xac]
        });
        root.method("run", "(Ljava/lang/Throwable;)I", 1, &code, 4, 4);
        handlers(&mut root, &[[0, 6, 6, 0]]);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let thrown = fixture.object("q/ChildThrow");
        let profile = fixture.profile();
        let method = fixture.method("p/T", "run", "(Ljava/lang/Throwable;)I");
        if fail {
            let error = fixture
                .e_run(&profile, method, receiver, &[Value::Reference(thrown)])
                .unwrap_err();
            assert_eq!(error.kind, E::StackUnderflow);
            assert_eq!(error.offset, Some(7));
        } else {
            assert_eq!(
                fixture
                    .e_run(&profile, method, receiver, &[Value::Reference(thrown)])
                    .unwrap(),
                caught_int(5)
            );
        }
    }
}
#[test]
fn caller_handlers_use_invocation_inclusive_start_and_exclusive_resume_for_all_call_forms() {
    let descriptor = "(Ljava/lang/Throwable;)I";
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("body", descriptor, 1, &[0x2b, 0xbf], 1, 2);
    let mut interface = iface("q/I");
    interface.method("defaultBody", descriptor, 1, &[0x2b, 0xbf], 1, 2);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    implement(&mut root, "q/I");
    root.method("staticBody", descriptor, 9, &[0x2a, 0xbf], 1, 1);
    for (name, opcode, owner, body) in [
        ("virtual", 0xb6, "q/Base", "body"),
        ("special", 0xb7, "q/Base", "body"),
        ("interface", 0xb9, "q/I", "defaultBody"),
        ("static", 0xb8, "p/T", "staticBody"),
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
        let start = code.len() as u16;
        code.extend(op(opcode, call));
        if opcode == 0xb9 {
            code.extend([2, 0]);
        }
        let end = code.len() as u16;
        code.extend([0x03, 0xac]);
        let handler = code.len() as u16;
        code.extend([0x57, 0x10, 7, 0xac]);
        root.method(name, descriptor, 1, &code, 2, 2);
        handlers(&mut root, &[[start, end, handler, 0]]);
    }
    let mut fixture = Fixture::new(vec![base, interface, root]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("q/ChildThrow");
    let profile = fixture.profile();
    for name in ["virtual", "special", "interface", "static"] {
        let method = fixture.method("p/T", name, descriptor);
        assert_eq!(
            fixture
                .e_run(&profile, method, receiver, &[Value::Reference(thrown)])
                .unwrap(),
            caught_int(7)
        );
    }
}
#[test]
fn catch_all_cleanup_rethrows_or_replaces_supplied_identity_without_constructing_objects() {
    for replace in [false, true] {
        let mut root = ordinary();
        root.field("touch", "I", 1);
        let field = root.reference(9, "p/T", "touch", "I");
        let throwable = root.class("java/lang/Throwable");
        let mut code = vec![0x2b, 0xbf, 0x4e, 0x2a, 0x10, 7];
        code.extend(op(0xb5, field));
        code.extend([if replace { 0x2c } else { 0x2d }, 0xbf, 0xb0]);
        root.method(
            "run",
            "(Ljava/lang/Throwable;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
            1,
            &code,
            2,
            4,
        );
        handlers(&mut root, &[[0, 2, 2, 0], [2, 11, 11, throwable]]);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let first = fixture.object("q/ChildThrow");
        let second = fixture.object("q/BaseThrow");
        let profile = fixture.profile();
        let method = fixture.method(
            "p/T",
            "run",
            "(Ljava/lang/Throwable;Ljava/lang/Throwable;)Ljava/lang/Throwable;",
        );
        assert_eq!(
            fixture
                .e_run(
                    &profile,
                    method,
                    receiver,
                    &[Value::Reference(first), Value::Reference(second)]
                )
                .unwrap(),
            EO::Returned(Some(Value::Reference(if replace { second } else { first })))
        );
        assert_eq!(fixture.stored("p/T", "touch", "I", receiver), Value::Int(7));
        assert_eq!(fixture.heap.allocated_instances(), 3);
    }
}
#[test]
fn inactive_and_later_catches_are_lazy_and_in_range_invalid_types_remain_host_failures() {
    for mode in 0..5 {
        let mut root = ordinary();
        let missing = root.class("q/Missing");
        let bad = root.class(if mode == 3 { "q/I" } else { "java/lang/Object" });
        root.method(
            "run",
            "(Ljava/lang/Throwable;)I",
            1,
            &[0x2b, 0xbf, 0x57, 0x04, 0xac],
            1,
            2,
        );
        let rows = match mode {
            0 => vec![[0, 1, 2, missing], [0, 2, 2, 0]],
            1 => vec![[0, 2, 2, 0], [0, 2, 2, missing]],
            2 => vec![[0, 2, 2, missing], [0, 2, 2, 0]],
            _ => vec![[0, 2, 2, bad], [0, 2, 2, 0]],
        };
        handlers(&mut root, &rows);
        let mut fixture = Fixture::new(vec![root, iface("q/I")]);
        let receiver = fixture.object("p/T");
        let thrown = fixture.object("q/ChildThrow");
        let profile = fixture.profile();
        let method = fixture.method("p/T", "run", "(Ljava/lang/Throwable;)I");
        let result = fixture.e_run(&profile, method, receiver, &[Value::Reference(thrown)]);
        if mode < 2 {
            assert_eq!(result.unwrap(), caught_int(1));
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.offset, Some(1));
            assert!(
                matches!(error.kind,E::ExceptionAccess(error) if if mode==2{matches!(error.as_ref(),ExceptionAccessError::Type(_))}else{matches!(error.as_ref(),ExceptionAccessError::InvalidCatchType{..})})
            );
        }
    }
}

#[test]
fn all_eleven_vm_produced_categories_and_null_athrow_are_host_unsupported_even_with_catch_all() {
    let mut other = Class::new("q/Other", Some("java/lang/Object"), 0x21);
    other.field("package", "I", 0);
    let mut interface = iface("q/I");
    interface.method("absent", "()I", 0x401, &[], 0, 1);
    let mut root = ordinary();
    implement(&mut root, "q/I");
    root.field("touch", "I", 1);
    root.field("staticField", "I", 9);
    let touch = root.reference(9, "p/T", "touch", "I");
    let hidden = root.reference(9, "q/Other", "package", "I");
    let static_field = root.reference(9, "p/T", "staticField", "I");
    let missing_field = root.reference(9, "p/T", "missing", "I");
    let missing_method = root.reference(10, "p/T", "missing", "()I");
    let abstract_method = root.reference(11, "q/I", "absent", "()I");
    let int_array = root.class("[I");
    let throwable = root.class("java/lang/Throwable");
    let mut cast = vec![0x2a];
    cast.extend(op(0xc0, int_array));
    cast.extend([0x57, 0x03, 0xac]);
    let mut store = vec![0x04];
    store.extend(op(0xbd, throwable));
    store.extend([0x03, 0x2a, 0x53, 0x03, 0xac]);
    let mut nofield = vec![0x2a];
    nofield.extend(op(0xb4, missing_field));
    nofield.push(0xac);
    let mut access = vec![0x01];
    access.extend(op(0xb4, hidden));
    access.push(0xac);
    let mut icce = vec![0x01];
    icce.extend(op(0xb4, static_field));
    icce.push(0xac);
    let mut nomethod = vec![0x2a];
    nomethod.extend(op(0xb6, missing_method));
    nomethod.push(0xac);
    let mut abstract_call = vec![0x2a];
    abstract_call.extend(op(0xb9, abstract_method));
    abstract_call.extend([1, 0, 0xac]);
    let cases = [
        (
            VmExceptionKind::ArithmeticException,
            vec![0x04, 0x03, 0x6c, 0xac],
        ),
        (VmExceptionKind::ClassCastException, cast),
        (VmExceptionKind::ArrayStoreException, store),
        (
            VmExceptionKind::NullPointerException,
            vec![0x01, 0xbe, 0xac],
        ),
        (
            VmExceptionKind::ArrayIndexOutOfBoundsException,
            vec![0x03, 0xbc, 10, 0x03, 0x2e, 0xac],
        ),
        (
            VmExceptionKind::NegativeArraySizeException,
            vec![0x02, 0xbc, 10, 0x57, 0x03, 0xac],
        ),
        (VmExceptionKind::NoSuchFieldError, nofield),
        (VmExceptionKind::IllegalAccessError, access),
        (VmExceptionKind::IncompatibleClassChangeError, icce),
        (VmExceptionKind::NoSuchMethodError, nomethod),
        (VmExceptionKind::AbstractMethodError, abstract_call),
        (VmExceptionKind::NullPointerException, vec![0x01, 0xbf]),
    ];
    for (index, (_, body)) in cases.iter().enumerate() {
        let mut code = vec![0x2a, 0x10, 9];
        code.extend(op(0xb5, touch));
        code.extend(body);
        let end = code.len() as u16;
        code.extend([0x57, 0x10, 7, 0xac]);
        root.method(&format!("case{index}"), "()I", 1, &code, 3, 1);
        handlers(&mut root, &[[0, end, end, 0]]);
    }
    let mut fixture = Fixture::new(vec![root, other, interface]);
    let receiver = fixture.object("p/T");
    let profile = fixture.profile();
    for (index, (kind, _)) in cases.iter().enumerate() {
        let method = fixture.method("p/T", &format!("case{index}"), "()I");
        let error = fixture
            .e_limits(
                &profile,
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
        assert_eq!(error.kind, E::UnsupportedVmException(*kind), "case{index}");
        assert_eq!(error.method.as_ref().unwrap().owner, Some(method.owner()));
        assert_eq!(fixture.stored("p/T", "touch", "I", receiver), Value::Int(9));
    }
    assert_eq!(fixture.heap.allocated_instances(), 1);
}
#[test]
fn athrow_typed_errors_and_checked_nonthrowable_identities_are_never_caught() {
    for (code, expected) in [
        (vec![0xbf], E::StackUnderflow),
        (
            vec![0x03, 0xbf],
            E::WrongValueType {
                expected: ValueKind::Reference,
                actual: ValueKind::Int,
            },
        ),
    ] {
        let mut root = ordinary();
        let mut body = code;
        let end = body.len() as u16;
        body.extend([0x57, 0xb1]);
        root.method("run", "()V", 1, &body, 1, 1);
        handlers(&mut root, &[[0, end, end, 0]]);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let profile = fixture.profile();
        let method = fixture.method("p/T", "run", "()V");
        assert_eq!(
            fixture
                .e_run(&profile, method, receiver, &[])
                .unwrap_err()
                .kind,
            expected
        );
    }
    let mut root = ordinary();
    root.method(
        "run",
        "(Ljava/lang/Object;)V",
        1,
        &[0x2b, 0xbf, 0x57, 0xb1],
        1,
        2,
    );
    handlers(&mut root, &[[0, 2, 2, 0]]);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let array = fixture
        .heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", "(Ljava/lang/Object;)V");
    for reference in [receiver, array] {
        assert!(matches!(
            fixture
                .e_run(&profile, method, receiver, &[Value::Reference(reference)])
                .unwrap_err()
                .kind,
            E::ExceptionAccess(_)
        ));
    }
}
#[test]
fn profile_validation_is_eager_before_frame_gate_and_old_entrypoints_keep_boundaries() {
    let mut root = ordinary();
    root.method("throw", "(Ljava/lang/Throwable;)V", 1, &[0x2b, 0xbf], 1, 2);
    root.method("handler", "()I", 1, &[0x03, 0xac, 0x57, 0x04, 0xac], 1, 1);
    handlers(&mut root, &[[0, 2, 2, 0]]);
    let mut first = Fixture::new(vec![root]);
    let profile = first.profile();
    let mut second = Fixture::new(vec![ordinary()]);
    let other = second.object("p/T");
    let method = first.method("p/T", "throw", "(Ljava/lang/Throwable;)V");
    let error = second
        .e_limits(
            &profile,
            method,
            other,
            &[],
            (
                ExecutionOptions::default(),
                CallLimits {
                    max_frames: 0,
                    ..CallLimits::default()
                },
                ExceptionLimits::default(),
            ),
        )
        .unwrap_err();
    assert!(
        matches!(error.kind,E::ExceptionAccess(error) if matches!(error.as_ref(),ExceptionAccessError::ProfileMismatch))
    );
    let receiver = first.object("p/T");
    let thrown = first.object("q/ChildThrow");
    assert_eq!(
        first
            .run(method, receiver, &[Value::Reference(thrown)])
            .unwrap_err()
            .kind,
        E::UnsupportedInstruction(0xbf)
    );
    let method = first.method("p/T", "handler", "()I");
    assert_eq!(
        first.run(method, receiver, &[]).unwrap_err().kind,
        E::UnsupportedMethod("exception handlers require exception dispatch")
    );
    let mut root = ordinary();
    root.method("static", "()V", 9, &[0xbf], 0, 0);
    let fixture = Fixture::new(vec![root]);
    let class = fixture.registry.class(fixture.classes["p/T"]).unwrap();
    assert_eq!(
        execute_static_method_body(class, 0, &[], ExecutionOptions::default())
            .unwrap_err()
            .kind,
        E::UnsupportedInstruction(0xbf)
    );
}
#[test]
fn eager_handler_structure_validates_boundaries_stack_and_array_catches_before_execution() {
    for kind in 0..7 {
        let mut root = ordinary();
        let array = root.class("[I");
        let (code, row, stack) = match kind {
            0 => (vec![0x10, 1, 0x57, 0x03, 0xac], [1, 3, 3, 0], 1),
            1 => (vec![0x10, 1, 0x57, 0x03, 0xac], [0, 1, 3, 0], 1),
            2 => (vec![0x10, 1, 0x57, 0x03, 0xac], [0, 2, 1, 0], 1),
            3 => (vec![0xc4, 0x19, 0, 0, 0x57, 0xb1], [1, 4, 5, 0], 1),
            4 => (vec![0x03, 0xac], [0, 1, 1, 0], 0),
            5 => (vec![0x03, 0xac], [0, 1, 1, array], 1),
            _ => {
                let mut code = vec![0xaa, 0, 0, 0];
                for _ in 0..4 {
                    code.extend([0, 0, 0, 0]);
                }
                code.push(0xb1);
                (code, [1, 20, 20, 0], 1)
            }
        };
        root.method("run", "()V", 1, &code, stack, 1);
        handlers(&mut root, &[row]);
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let profile = fixture.profile();
        let method = fixture.method("p/T", "run", "()V");
        assert!(
            matches!(
                fixture
                    .e_run(&profile, method, receiver, &[])
                    .unwrap_err()
                    .kind,
                E::InvalidExceptionHandler { index: 0, .. }
            ),
            "case{kind}"
        );
    }
}
#[test]
fn exact_dispatch_fuel_and_empty_table_unwind_limits_are_independent() {
    for caught in [false, true] {
        let mut root = ordinary();
        let mut code = vec![0x2b, 0xbf];
        if caught {
            code.extend([0x57, 0x04, 0xac]);
        }
        root.method("run", "(Ljava/lang/Throwable;)I", 1, &code, 1, 2);
        if caught {
            handlers(&mut root, &[[0, 2, 2, 0]]);
        }
        let mut fixture = Fixture::new(vec![root]);
        let receiver = fixture.object("p/T");
        let thrown = fixture.object("q/ChildThrow");
        let profile = fixture.profile();
        let method = fixture.method("p/T", "run", "(Ljava/lang/Throwable;)I");
        for limit in [0, 1] {
            let error = fixture
                .e_limits(
                    &profile,
                    method,
                    receiver,
                    &[Value::Reference(thrown)],
                    (
                        ExecutionOptions {
                            instruction_budget: 2,
                        },
                        CallLimits::default(),
                        ExceptionLimits {
                            max_dispatch_steps: limit,
                            ..ExceptionLimits::default()
                        },
                    ),
                )
                .unwrap_err();
            assert_eq!(
                error.kind,
                E::ExceptionDispatchLimit {
                    limit,
                    required: limit + 1
                }
            );
            assert_eq!(error.offset, Some(1));
        }
        let result = fixture.e_limits(
            &profile,
            method,
            receiver,
            &[Value::Reference(thrown)],
            (
                ExecutionOptions {
                    instruction_budget: 2,
                },
                CallLimits::default(),
                ExceptionLimits {
                    max_dispatch_steps: 2,
                    ..ExceptionLimits::default()
                },
            ),
        );
        if caught {
            assert_eq!(
                result.unwrap_err().kind,
                E::InstructionBudgetExhausted { limit: 2 }
            );
            assert_eq!(
                fixture
                    .e_limits(
                        &profile,
                        method,
                        receiver,
                        &[Value::Reference(thrown)],
                        (
                            ExecutionOptions {
                                instruction_budget: 5
                            },
                            CallLimits::default(),
                            ExceptionLimits {
                                max_dispatch_steps: 2,
                                ..ExceptionLimits::default()
                            }
                        )
                    )
                    .unwrap(),
                caught_int(1)
            );
        } else {
            assert_eq!(result.unwrap(), EO::Uncaught(thrown));
        }
    }
}

#[test]
fn handler_and_dispatch_totals_count_cache_entries_once_but_every_repeated_throw() {
    let descriptor = "(Ljava/lang/Throwable;)I";
    let mut root = ordinary();
    root.method(
        "helper",
        descriptor,
        1,
        &[0x2b, 0xbf, 0x57, 0x04, 0xac],
        1,
        2,
    );
    handlers(&mut root, &[[0, 2, 2, 0]]);
    let call = root.reference(10, "p/T", "helper", descriptor);
    let mut code = vec![0x2a, 0x2b];
    code.extend(op(0xb6, call));
    code.extend([0x57, 0x2a, 0x2b]);
    code.extend(op(0xb6, call));
    code.push(0xac);
    let end = code.len() as u16;
    code.extend([0x57, 0x05, 0xac]);
    root.method("run", descriptor, 1, &code, 2, 2);
    handlers(&mut root, &[[0, end, end, 0]]);
    let bytes = code.len() + 5;
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("q/ChildThrow");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", descriptor);
    let calls = CallLimits {
        max_frames: 2,
        max_frame_slots: 7,
        max_prepared_code_bytes: bytes,
    };
    let exact = ExceptionLimits {
        max_prepared_handlers: 2,
        max_dispatch_steps: 4,
    };
    assert_eq!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &[Value::Reference(thrown)],
                (ExecutionOptions::default(), calls, exact)
            )
            .unwrap(),
        caught_int(1)
    );
    let error = fixture
        .e_limits(
            &profile,
            method,
            receiver,
            &[Value::Reference(thrown)],
            (
                ExecutionOptions::default(),
                calls,
                ExceptionLimits {
                    max_prepared_handlers: 1,
                    ..exact
                },
            ),
        )
        .unwrap_err();
    assert_eq!(
        error.kind,
        E::ExceptionHandlerLimit {
            limit: 1,
            required: 2
        }
    );
    assert_eq!(
        error.method.as_ref().unwrap().index,
        fixture.method("p/T", "helper", descriptor).index()
    );
    assert_eq!(error.call_site.as_ref().unwrap().offset, 2);
    let error = fixture
        .e_limits(
            &profile,
            method,
            receiver,
            &[Value::Reference(thrown)],
            (
                ExecutionOptions::default(),
                calls,
                ExceptionLimits {
                    max_dispatch_steps: 3,
                    ..exact
                },
            ),
        )
        .unwrap_err();
    assert_eq!(
        error.kind,
        E::ExceptionDispatchLimit {
            limit: 3,
            required: 4
        }
    );
    assert_eq!(error.offset, Some(1));
    assert!(matches!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &[Value::Reference(thrown)],
                (
                    ExecutionOptions::default(),
                    CallLimits {
                        max_frame_slots: 6,
                        ..calls
                    },
                    exact
                )
            )
            .unwrap_err()
            .kind,
        E::FrameSlotLimit {
            limit: 6,
            required: 7
        }
    ));
    assert_eq!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &[Value::Reference(thrown)],
                (
                    ExecutionOptions::default(),
                    CallLimits {
                        max_prepared_code_bytes: bytes - 1,
                        ..calls
                    },
                    exact
                )
            )
            .unwrap_err()
            .kind,
        E::PreparedCodeLimit {
            limit: bytes - 1,
            required: bytes
        }
    );
}
#[test]
fn inactive_rows_consume_dispatch_work_and_handler_limit_precedes_zero_stack_guard() {
    let mut root = ordinary();
    let no_match = root.class("q/ChildThrow");
    root.method(
        "run",
        "(Ljava/lang/Throwable;)I",
        1,
        &[0x2b, 0xbf, 0x57, 0x04, 0xac],
        1,
        2,
    );
    handlers(
        &mut root,
        &[[0, 1, 2, 0], [0, 2, 2, no_match], [0, 2, 2, 0]],
    );
    root.method("zero", "()V", 1, &[0xb1], 0, 1);
    handlers(&mut root, &[[0, 1, 0, 0]]);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("java/lang/Throwable");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", "(Ljava/lang/Throwable;)I");
    let args = [Value::Reference(thrown)];
    assert_eq!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &args,
                (
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits {
                        max_dispatch_steps: 3,
                        ..ExceptionLimits::default()
                    }
                )
            )
            .unwrap_err()
            .kind,
        E::ExceptionDispatchLimit {
            limit: 3,
            required: 4
        }
    );
    assert_eq!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &args,
                (
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits {
                        max_dispatch_steps: 4,
                        ..ExceptionLimits::default()
                    }
                )
            )
            .unwrap(),
        caught_int(1)
    );
    let method = fixture.method("p/T", "zero", "()V");
    assert_eq!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &[],
                (
                    ExecutionOptions::default(),
                    CallLimits::default(),
                    ExceptionLimits {
                        max_prepared_handlers: 0,
                        ..ExceptionLimits::default()
                    }
                )
            )
            .unwrap_err()
            .kind,
        E::ExceptionHandlerLimit {
            limit: 0,
            required: 1
        }
    );
}
#[test]
fn caller_search_failures_identify_searched_method_invoke_pc_and_its_own_provenance() {
    let descriptor = "(Ljava/lang/Throwable;)I";
    let mut leaf = Class::new("q/Leaf", Some("java/lang/Object"), 0x21);
    leaf.method("throw", descriptor, 9, &[0x2a, 0xbf], 1, 1);
    // Static routing stays within each current class: cross-class transition uses
    // ordinary virtual bodies, while each nested throw still carries its identity.
    let mut base = Class::new("q/Base", Some("java/lang/Object"), 0x21);
    base.method("leaf", descriptor, 1, &[0x2b, 0xbf], 1, 2);
    let call = base.reference(10, "q/Base", "leaf", descriptor);
    let missing = base.class("q/Missing");
    let mut code = vec![0x2a, 0x2b];
    code.extend(op(0xb6, call));
    code.push(0xac);
    let handler = code.len() as u16;
    code.extend([0x57, 0x04, 0xac]);
    base.method("middle", descriptor, 1, &code, 2, 2);
    handlers(&mut base, &[[2, 5, handler, missing]]);
    let mut root = Class::new("p/T", Some("q/Base"), 0x21);
    let call = root.reference(10, "q/Base", "middle", descriptor);
    let mut code = vec![0x2a, 0x2b];
    code.extend(op(0xb7, call));
    code.push(0xac);
    root.method("run", descriptor, 1, &code, 2, 2);
    let mut fixture = Fixture::new(vec![root, base, leaf]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("q/ChildThrow");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", descriptor);
    let error = fixture
        .e_run(&profile, method, receiver, &[Value::Reference(thrown)])
        .unwrap_err();
    assert_eq!(error.offset, Some(2));
    assert_eq!(
        error.method.as_ref().unwrap().owner,
        Some(fixture.classes["q/Base"])
    );
    assert_eq!(
        error.method.as_ref().unwrap().index,
        fixture.method("q/Base", "middle", descriptor).index()
    );
    assert_eq!(
        error.call_site.as_ref().unwrap().method.owner,
        Some(fixture.classes["p/T"])
    );
    assert_eq!(error.call_site.as_ref().unwrap().offset, 2);
    assert!(
        matches!(error.kind,E::ExceptionAccess(error) if matches!(error.as_ref(),ExceptionAccessError::Type(_)))
    );
}
#[test]
fn typed_catch_current_origin_access_failures_do_not_become_caught_objects() {
    for denied in [false, true] {
        let hidden = Class::new("q/Hidden", Some("java/lang/Throwable"), 0x20);
        let mut body = Class::new(
            if denied { "p/T" } else { "q/Base" },
            Some("java/lang/Object"),
            0x21,
        );
        let catch = body.class("q/Hidden");
        body.method(
            "run",
            "(Ljava/lang/Throwable;)I",
            1,
            &[0x2b, 0xbf, 0x57, 0x04, 0xac],
            1,
            2,
        );
        handlers(&mut body, &[[0, 2, 2, catch], [0, 2, 2, 0]]);
        let owner = body.name.clone();
        let mut fixture = Fixture::new(vec![body, hidden]);
        let receiver = fixture.object(&owner);
        let thrown = fixture.object("q/Hidden");
        let profile = fixture.profile();
        let method = fixture.method(&owner, "run", "(Ljava/lang/Throwable;)I");
        let result = fixture.e_run(&profile, method, receiver, &[Value::Reference(thrown)]);
        if denied {
            let error = result.unwrap_err();
            assert_eq!(
                error.kind,
                E::UnsupportedVmException(VmExceptionKind::IllegalAccessError)
            );
            assert_eq!(error.offset, Some(1));
        } else {
            assert_eq!(result.unwrap(), caught_int(1));
        }
    }
}
#[test]
fn deep_unwind_is_iterative_and_charges_empty_frames_and_root_unwind_exactly() {
    let mut root = ordinary();
    let descriptor = "(ILjava/lang/Throwable;)I";
    let call = root.reference(10, "p/T", "run", descriptor);
    let mut code = vec![0x1b, 0x99, 0, 12, 0x2a, 0x1b, 0x04, 0x64, 0x2c];
    code.extend(op(0xb6, call));
    code.extend([0xac, 0x2c, 0xbf]);
    root.method("run", descriptor, 1, &code, 3, 3);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("q/ChildThrow");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", descriptor);
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: 20_000_000,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    let calls = CallLimits {
        max_frames: 10_001,
        max_frame_slots: 60_006,
        max_prepared_code_bytes: code.len(),
    };
    assert_eq!(
        execute_instance_method_body_with_exceptions(
            &mut context,
            &profile,
            method,
            receiver,
            &[Value::Int(10_000), Value::Reference(thrown)],
            ExecutionOptions::default(),
            calls,
            ExceptionLimits {
                max_prepared_handlers: 0,
                max_dispatch_steps: 20_002
            }
        )
        .unwrap(),
        EO::Uncaught(thrown)
    );
    assert_eq!(context.heap().allocated_instances(), 2);
    assert_eq!(context.heap().allocated_arrays(), 0);
}

#[test]
fn late_runtime_catch_work_failure_preserves_prior_field_effects_and_throw_pc() {
    let mut root = ordinary();
    root.field("touch", "I", 1);
    let touch = root.reference(9, "p/T", "touch", "I");
    let catch = root.class("q/BaseThrow");
    let mut code = vec![0x2a, 0x10, 7];
    code.extend(op(0xb5, touch));
    code.extend([0x2b, 0xbf, 0x57, 0x04, 0xac]);
    root.method("run", "(Ljava/lang/Throwable;)I", 1, &code, 2, 2);
    handlers(&mut root, &[[0, 8, 8, catch]]);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let thrown = fixture.object("q/ChildThrow");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", "(Ljava/lang/Throwable;)I");
    let mut resolver = SameLoaderResolver;
    let mut context = FieldResolutionContext::new_unnamed_modules(
        &fixture.registry,
        &mut fixture.heap,
        &mut resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap();
    assert_eq!(
        execute_instance_method_body_with_exceptions(
            &mut context,
            &profile,
            method,
            receiver,
            &[Value::Reference(thrown)],
            ExecutionOptions::default(),
            CallLimits::default(),
            ExceptionLimits::default()
        )
        .unwrap(),
        caught_int(1)
    );
    let exact = context.lookup_steps();
    drop(context);
    let field = fixture
        .registry
        .find_declared_field(method.owner(), &units("touch"), &units("I"))
        .unwrap()
        .unwrap();
    fixture
        .heap
        .put_field(receiver, field, Value::Int(0))
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
    let error = execute_instance_method_body_with_exceptions(
        &mut context,
        &profile,
        method,
        receiver,
        &[Value::Reference(thrown)],
        ExecutionOptions::default(),
        CallLimits::default(),
        ExceptionLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.offset, Some(7));
    assert!(
        matches!(error.kind,E::ExceptionAccess(error) if matches!(error.as_ref(),ExceptionAccessError::Context(FieldAccessError::ResourceLimit{..})|ExceptionAccessError::Type(_)))
    );
    assert_eq!(
        context.heap().get_field(receiver, field).unwrap(),
        Value::Int(7)
    );
}
#[test]
fn decode_and_code_limits_precede_opt_in_handler_count_but_legacy_table_guard_stays_first() {
    let mut root = ordinary();
    root.method("run", "()V", 1, &[0xb1, 0xff], 1, 1);
    handlers(&mut root, &[[0, 1, 0, 0]]);
    let mut fixture = Fixture::new(vec![root]);
    let receiver = fixture.object("p/T");
    let profile = fixture.profile();
    let method = fixture.method("p/T", "run", "()V");
    assert_eq!(
        fixture.run(method, receiver, &[]).unwrap_err().kind,
        E::UnsupportedMethod("exception handlers require exception dispatch")
    );
    let policy = ExceptionLimits {
        max_prepared_handlers: 0,
        ..ExceptionLimits::default()
    };
    assert!(matches!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &[],
                (ExecutionOptions::default(), CallLimits::default(), policy)
            )
            .unwrap_err()
            .kind,
        E::Decode(_)
    ));
    assert_eq!(
        fixture
            .e_limits(
                &profile,
                method,
                receiver,
                &[],
                (
                    ExecutionOptions::default(),
                    CallLimits {
                        max_prepared_code_bytes: 1,
                        ..CallLimits::default()
                    },
                    policy
                )
            )
            .unwrap_err()
            .kind,
        E::PreparedCodeLimit {
            limit: 1,
            required: 2
        }
    );
}

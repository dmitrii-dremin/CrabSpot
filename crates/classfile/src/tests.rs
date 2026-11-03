//! Synthetic class files exercise encodings that javac rarely emits and exact
//! malformed-format boundaries. Reference-compiler fixtures live in integration tests.
use crate::*;
#[path = "annotation_tests.rs"]
mod annotation_tests;

fn u2(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend(value.to_be_bytes());
}
fn u4(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_be_bytes());
}
fn words(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}

struct ClassBuilder {
    major: u16,
    minor: u16,
    flags: u16,
    this: u16,
    super_class: u16,
    interfaces: Vec<u16>,
    pool: Vec<Vec<u8>>,
    fields: Vec<Vec<u8>>,
    methods: Vec<Vec<u8>>,
    attributes: Vec<Vec<u8>>,
}
impl ClassBuilder {
    fn new() -> Self {
        let mut class = Self {
            major: 65,
            minor: 0,
            flags: 0x21,
            this: 0,
            super_class: 0,
            interfaces: Vec::new(),
            pool: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
        };
        class.this = class.class("Test");
        class.super_class = class.class("java/lang/Object");
        class
    }
    fn constant(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let index = self.pool.len() as u16 + 1;
        let mut bytes = vec![tag];
        bytes.extend(payload);
        self.pool.push(bytes);
        if matches!(tag, 5 | 6) {
            self.pool.push(Vec::new());
        }
        index
    }
    fn utf8(&mut self, name: &str) -> u16 {
        assert!(name.is_ascii());
        let mut bytes = Vec::new();
        u2(&mut bytes, name.len() as u16);
        bytes.extend(name.bytes());
        self.constant(1, &bytes)
    }
    fn class(&mut self, name: &str) -> u16 {
        let index = self.utf8(name);
        self.constant(7, &index.to_be_bytes())
    }
    fn nat(&mut self, name: &str, descriptor: &str) -> u16 {
        let name = self.utf8(name);
        let desc = self.utf8(descriptor);
        self.constant(12, &words(&[name, desc]))
    }
    fn attribute(&mut self, name: &str, body: &[u8]) -> Vec<u8> {
        let name = self.utf8(name);
        let mut bytes = words(&[name]);
        u4(&mut bytes, body.len() as u32);
        bytes.extend(body);
        bytes
    }
    fn member(
        &mut self,
        name: &str,
        descriptor: &str,
        flags: u16,
        attributes: &[Vec<u8>],
    ) -> Vec<u8> {
        let name = self.utf8(name);
        let descriptor = self.utf8(descriptor);
        let mut bytes = words(&[flags, name, descriptor, attributes.len() as u16]);
        for attribute in attributes {
            bytes.extend(attribute);
        }
        bytes
    }
    fn method(&mut self, name: &str, descriptor: &str, flags: u16, attributes: &[Vec<u8>]) {
        let bytes = self.member(name, descriptor, flags, attributes);
        self.methods.push(bytes);
    }
    fn field(&mut self, name: &str, descriptor: &str, flags: u16, attributes: &[Vec<u8>]) {
        let bytes = self.member(name, descriptor, flags, attributes);
        self.fields.push(bytes);
    }
    fn finish(self) -> Vec<u8> {
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe];
        u2(&mut bytes, self.minor);
        u2(&mut bytes, self.major);
        u2(&mut bytes, self.pool.len() as u16 + 1);
        for entry in self.pool {
            bytes.extend(entry);
        }
        bytes.extend(words(&[
            self.flags,
            self.this,
            self.super_class,
            self.interfaces.len() as u16,
        ]));
        bytes.extend(words(&self.interfaces));
        for table in [self.fields, self.methods, self.attributes] {
            u2(&mut bytes, table.len() as u16);
            for entry in table {
                bytes.extend(entry);
            }
        }
        bytes
    }
}

fn code_body(length: usize, locals: u16, handlers: &[[u16; 4]], attrs: &[Vec<u8>]) -> Vec<u8> {
    let mut body = words(&[8, locals]);
    u4(&mut body, length as u32);
    body.extend(std::iter::repeat_n(0xb1, length));
    u2(&mut body, handlers.len() as u16);
    for handler in handlers {
        body.extend(words(handler));
    }
    u2(&mut body, attrs.len() as u16);
    for attribute in attrs {
        body.extend(attribute);
    }
    body
}
fn code_class(name: &str, descriptor: &str, flags: u16, major: u16, locals: u16) -> Vec<u8> {
    let mut class = ClassBuilder::new();
    class.major = major;
    let code = class.attribute("Code", &code_body(1, locals, &[], &[]));
    class.method(name, descriptor, flags, &[code]);
    class.finish()
}

#[test]
fn versions_and_preview_policy() {
    for (major, minor) in [
        (45, 0),
        (45, 65535),
        (46, 17),
        (55, 65535),
        (56, 0),
        (64, 0),
        (65, 0),
    ] {
        let mut class = ClassBuilder::new();
        class.major = major;
        class.minor = minor;
        assert!(parse(&class.finish()).is_ok(), "{major}.{minor}");
    }
    for (major, minor) in [(44, 0), (66, 0), (56, 1), (64, 65535)] {
        let mut class = ClassBuilder::new();
        class.major = major;
        class.minor = minor;
        let error = parse_with_options(
            &class.finish(),
            ParseOptions {
                allow_preview: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(error.kind, ErrorKind::UnsupportedVersion { .. }));
    }
    let mut class = ClassBuilder::new();
    class.minor = 65535;
    let bytes = class.finish();
    assert_eq!(parse(&bytes).unwrap_err().kind, ErrorKind::PreviewDisabled);
    assert!(
        parse_with_options(
            &bytes,
            ParseOptions {
                allow_preview: true,
                ..Default::default()
            }
        )
        .unwrap()
        .version
        .is_preview()
    );
}

#[test]
fn byte_limit_and_actionable_truncation() {
    let bytes = ClassBuilder::new().finish();
    assert!(
        parse_with_options(
            &bytes,
            ParseOptions {
                max_class_bytes: bytes.len(),
                ..Default::default()
            }
        )
        .is_ok()
    );
    let error = parse_with_options(
        &bytes,
        ParseOptions {
            max_class_bytes: bytes.len() - 1,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::ResourceLimit {
            resource: "class-file bytes",
            limit: bytes.len() - 1,
            actual: bytes.len()
        }
    );
    let error = parse(&bytes[..7]).unwrap_err();
    assert_eq!(error.offset, 6);
    assert_eq!(
        error.kind,
        ErrorKind::UnexpectedEof {
            needed: 2,
            remaining: 1
        }
    );
    let mut trailing = bytes;
    trailing.extend([7, 8]);
    assert_eq!(
        parse(&trailing).unwrap_err().kind,
        ErrorKind::TrailingBytes(2)
    );
}

#[test]
fn attribute_bodies_are_bounded_and_extensions_preserved() {
    let mut class = ClassBuilder::new();
    let unknown = class.attribute("VendorExtension", &[0, 255, 128]);
    class.attributes.push(unknown);
    let annotation = class.attribute("RuntimeVisibleAnnotations", &[0, 0]);
    class.attributes.push(annotation);
    // A predefined name is an extension outside its standard location/version.
    let code = class.attribute("Code", &[255]);
    class.attributes.push(code);
    let parsed = parse(&class.finish()).unwrap();
    assert_eq!(
        parsed.attributes[0].body,
        AttributeBody::Opaque {
            bytes: vec![0, 255, 128],
            standard: false
        }
    );
    assert_eq!(
        parsed.attributes[1].body,
        AttributeBody::RuntimeVisibleAnnotations(Vec::new())
    );
    assert_eq!(
        parsed.attributes[2].body,
        AttributeBody::Opaque {
            bytes: vec![255],
            standard: false
        }
    );
    let mut legacy = ClassBuilder::new();
    legacy.major = 45;
    let attribute = legacy.attribute("Module", &[0xff]);
    legacy.attributes.push(attribute);
    assert!(parse(&legacy.finish()).is_ok());
    let mut invalid = ClassBuilder::new();
    let index = invalid.utf8("Test.java");
    let mut body = words(&[index]);
    body.push(0);
    let attr = invalid.attribute("SourceFile", &body);
    invalid.attributes.push(attr);
    assert!(matches!(
        parse(&invalid.finish()).unwrap_err().kind,
        ErrorKind::TrailingBytes(1)
    ));
    let mut duplicate = ClassBuilder::new();
    let source_file = duplicate.utf8("Test.java");
    let attr = duplicate.attribute("SourceFile", &words(&[source_file]));
    duplicate.attributes.extend([attr.clone(), attr]);
    assert!(parse(&duplicate.finish()).is_err());
    let mut truncated = ClassBuilder::new();
    let mut attr = truncated.attribute("Unknown", &[1]);
    attr[2..6].copy_from_slice(&u32::MAX.to_be_bytes());
    truncated.attributes.push(attr);
    assert!(matches!(
        parse(&truncated.finish()).unwrap_err().kind,
        ErrorKind::UnexpectedEof { .. }
    ));
}

#[test]
fn constructor_and_historical_strict_flags() {
    assert!(parse(&code_class("<init>", "()V", 0x0801, 60, 1)).is_ok());
    assert!(parse(&code_class("<init>", "()V", 0x0041, 65, 1)).is_err());
    for major in [45, 61, 65] {
        let mut class = ClassBuilder::new();
        class.major = major;
        class.method("m", "()V", 0x0c01, &[]);
        assert!(parse(&class.finish()).is_ok(), "major{major}");
    }
    for major in [46, 60] {
        let mut class = ClassBuilder::new();
        class.major = major;
        class.method("m", "()V", 0x0c01, &[]);
        assert!(parse(&class.finish()).is_err(), "major{major}");
    }
}

#[test]
fn class_initializers_follow_their_special_flag_and_descriptor_rules() {
    // Other raw flag bits are ignored even when they look abstract/native.
    assert!(parse(&code_class("<clinit>", "()V", 0x050f, 65, 0)).is_ok());
    assert!(parse(&code_class("<clinit>", "()V", 0, 50, 0)).is_ok());
    assert!(parse(&code_class("<clinit>", "(I)V", 0, 50, 0)).is_ok());
    assert!(parse(&code_class("<clinit>", "()V", 0, 51, 0)).is_err());
    assert!(parse(&code_class("<clinit>", "(I)V", 8, 51, 1)).is_err());
    assert!(parse(&code_class("<clinit>", "()I", 8, 65, 0)).is_err());
    let mut class = ClassBuilder::new();
    class.method("<clinit>", "()V", 0x0508, &[]);
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn interface_members_obey_local_format_rules() {
    for flags in [0, 1, 9, 0x0059, 0x4019] {
        let mut class = ClassBuilder::new();
        class.flags = 0x0601;
        class.field("x", "I", flags, &[]);
        assert!(parse(&class.finish()).is_err(), "{flags:x}");
    }
    let mut class = ClassBuilder::new();
    class.flags = 0x0601;
    class.field("x", "I", 0x1019, &[]);
    class.method("m", "()V", 0x0401, &[]);
    assert!(parse(&class.finish()).is_ok());
    for flags in [0x0400, 0x0403, 0x0405, 0x0411, 0x0421, 0x0501] {
        let mut class = ClassBuilder::new();
        class.flags = 0x0601;
        class.method("m", "()V", flags, &[]);
        assert!(parse(&class.finish()).is_err(), "{flags:x}");
    }
    let mut class = ClassBuilder::new();
    class.flags = 0x0601;
    let code = class.attribute("Code", &code_body(1, 1, &[], &[]));
    class.method("<init>", "()V", 1, &[code]);
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn code_and_exception_table_bounds() {
    for handlers in [
        vec![[0, 0, 0, 0]],
        vec![[0, 2, 0, 0]],
        vec![[0, 1, 1, 0]],
        vec![[0, 1, 0, 1]],
    ] {
        let mut class = ClassBuilder::new();
        let code = class.attribute("Code", &code_body(1, 1, &handlers, &[]));
        class.method("m", "()V", 1, &[code]);
        assert!(parse(&class.finish()).is_err());
    }
    let mut class = ClassBuilder::new();
    let code = class.attribute("Code", &code_body(1, 1, &[[0, 1, 0, 0]], &[]));
    class.method("m", "()V", 1, &[code]);
    assert!(parse(&class.finish()).is_ok());
    assert!(parse(&code_class("m", "(J)V", 1, 65, 2)).is_err());
    assert!(parse(&code_class("m", "(J)V", 1, 65, 3)).is_ok());
    assert!(parse(&code_class("m", "()V", 0x0101, 65, 1)).is_err());
    let mut class = ClassBuilder::new();
    let code = class.attribute("Code", &code_body(0, 1, &[], &[]));
    class.method("m", "()V", 1, &[code]);
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn parameter_slot_limit_includes_the_receiver() {
    let descriptor = format!("({})V", "I".repeat(255));
    let mut instance = ClassBuilder::new();
    instance.method("m", &descriptor, 0x0101, &[]);
    assert!(parse(&instance.finish()).is_err());
    let mut static_method = ClassBuilder::new();
    static_method.method("m", &descriptor, 0x0109, &[]);
    assert!(parse(&static_method.finish()).is_ok());
}

fn class_with_stack_map(table: &[u8]) -> Vec<u8> {
    let mut class = ClassBuilder::new();
    let table = class.attribute("StackMapTable", table);
    let code = class.attribute("Code", &code_body(32, 16, &[], &[table]));
    class.method("m", "()V", 1, &[code]);
    class.finish()
}

#[test]
fn all_stack_map_frame_forms_and_verification_types() {
    // Object index #2 is Test. Uninitialized offset0 is structurally in Code;
    // instruction-specific validation is deliberately left to bytecode verification.
    let mut table = words(&[8]);
    table.extend([
        0, 64, 1, 247, 0, 0, 2, 248, 0, 0, 251, 0, 0, 252, 0, 0, 3, 254, 0, 0, 4, 5, 6, 255, 0, 0,
        0, 2, 7, 0, 2, 8, 0, 0, 0, 1, 0,
    ]);
    let class = parse(&class_with_stack_map(&table)).unwrap();
    let AttributeBody::Code(code) = &class.methods[0].attributes[0].body else {
        panic!()
    };
    let AttributeBody::StackMapTable(frames) = &code.attributes[0].body else {
        panic!()
    };
    assert_eq!(frames.len(), 8);
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.bytecode_offset)
            .collect::<Vec<_>>(),
        (0..8).collect::<Vec<_>>()
    );
    assert_eq!(frames[0].kind, StackMapFrameKind::Same);
    assert_eq!(
        frames[1].kind,
        StackMapFrameKind::SameLocalsOneStack(VerificationType::Integer)
    );
    assert_eq!(
        frames[2].kind,
        StackMapFrameKind::SameLocalsOneStack(VerificationType::Float)
    );
    assert_eq!(frames[3].kind, StackMapFrameKind::Chop(3));
    assert_eq!(
        frames[5].kind,
        StackMapFrameKind::Append(vec![VerificationType::Double])
    );
    assert_eq!(
        frames[6].kind,
        StackMapFrameKind::Append(vec![
            VerificationType::Long,
            VerificationType::Null,
            VerificationType::UninitializedThis
        ])
    );
    assert_eq!(
        frames[7].kind,
        StackMapFrameKind::Full {
            locals: vec![
                VerificationType::Object(2),
                VerificationType::Uninitialized(0)
            ],
            stack: vec![VerificationType::Top]
        }
    );
}

#[test]
fn invalid_stack_maps_rejected() {
    for table in [
        vec![0, 1, 128],
        vec![0, 1, 246],
        vec![0, 1, 64, 9],
        vec![0, 1, 64, 7, 0, 1],
        vec![0, 1, 64, 8, 0, 32],
        vec![0, 1, 63],
        vec![0, 1, 247, 0],
    ] {
        assert!(parse(&class_with_stack_map(&table)).is_err(), "{table:?}");
    }
}

#[test]
fn dynamic_constants_and_bootstrap_references() {
    let mut class = ClassBuilder::new();
    let method_nat = class.nat("bootstrap", "()V");
    let method = class.constant(10, &words(&[class.this, method_nat]));
    let mut handle = vec![6];
    handle.extend(method.to_be_bytes());
    let handle = class.constant(15, &handle);
    let field_nat = class.nat("constant", "I");
    let dynamic = class.constant(17, &words(&[0, field_nat]));
    let call_nat = class.nat("call", "()V");
    class.constant(18, &words(&[0, call_nat]));
    let bootstrap = class.attribute("BootstrapMethods", &words(&[1, handle, 1, dynamic]));
    class.attributes.push(bootstrap);
    assert!(parse(&class.finish()).is_ok());
    let mut missing = ClassBuilder::new();
    let nat = missing.nat("constant", "I");
    missing.constant(17, &words(&[0, nat]));
    assert!(parse(&missing.finish()).is_err());
    let mut wrong = ClassBuilder::new();
    let bootstrap = wrong.attribute("BootstrapMethods", &words(&[1, wrong.this, 0]));
    wrong.attributes.push(bootstrap);
    assert!(parse(&wrong.finish()).is_err());
}

#[test]
fn constant_pool_encoding_bits_and_method_handle_kinds() {
    let mut class = ClassBuilder::new();
    let float = class.constant(4, &0x7fc00042u32.to_be_bytes());
    let double = class.constant(6, &0x8000000000000000u64.to_be_bytes());
    let long = class.constant(5, &u64::MAX.to_be_bytes());
    let integer = class.constant(3, &u32::MAX.to_be_bytes());
    let string = class.utf8("s");
    class.constant(8, &string.to_be_bytes());
    let desc = class.utf8("()V");
    class.constant(16, &desc.to_be_bytes());
    let field_nat = class.nat("f", "I");
    let field = class.constant(9, &words(&[class.this, field_nat]));
    let method_nat = class.nat("m", "()V");
    let method = class.constant(10, &words(&[class.this, method_nat]));
    let interface = class.constant(11, &words(&[class.this, method_nat]));
    let init_nat = class.nat("<init>", "()V");
    let init = class.constant(10, &words(&[class.this, init_nat]));
    for kind in 1..=9 {
        let index = match kind {
            1..=4 => field,
            8 => init,
            9 => interface,
            _ => method,
        };
        let mut body = vec![kind];
        body.extend(index.to_be_bytes());
        class.constant(15, &body);
    }
    let parsed = parse(&class.finish()).unwrap();
    assert_eq!(
        parsed.constant_pool.get(float).unwrap(),
        &Constant::Float(0x7fc00042)
    );
    assert_eq!(
        parsed.constant_pool.get(double).unwrap(),
        &Constant::Double(0x8000000000000000)
    );
    assert_eq!(parsed.constant_pool.get(long).unwrap(), &Constant::Long(-1));
    assert_eq!(
        parsed.constant_pool.get(integer).unwrap(),
        &Constant::Integer(-1)
    );
    for kind in [0, 1, 5, 8, 9, 10] {
        let mut wrong = ClassBuilder::new();
        let mut body = vec![kind];
        body.extend(wrong.this.to_be_bytes());
        wrong.constant(15, &body);
        assert!(parse(&wrong.finish()).is_err(), "kind{kind}");
    }
}

#[test]
fn invalid_pool_tags_indices_and_version_introductions() {
    let mut class = ClassBuilder::new();
    class.constant(2, &[]);
    assert_eq!(
        parse(&class.finish()).unwrap_err().kind,
        ErrorKind::InvalidConstantPoolTag(2)
    );
    let mut bytes = ClassBuilder::new().finish();
    bytes[8..10].copy_from_slice(&[0, 0]);
    assert!(parse(&bytes).is_err());
    for index in [0u16, 2, 65535] {
        let mut class = ClassBuilder::new();
        class.constant(8, &index.to_be_bytes());
        assert!(parse(&class.finish()).is_err(), "#{index}");
    }
    for (tag, major, payload) in [
        (15, 50, vec![6, 0, 1]),
        (16, 50, vec![0, 1]),
        (17, 54, vec![0, 0, 0, 1]),
        (18, 50, vec![0, 0, 0, 1]),
        (19, 52, vec![0, 1]),
        (20, 52, vec![0, 1]),
    ] {
        let mut class = ClassBuilder::new();
        class.major = major;
        class.constant(tag, &payload);
        assert!(parse(&class.finish()).is_err(), "tag{tag} major{major}");
    }
    for (name, descriptor) in [
        ("bad/name", "I"),
        ("f", "V"),
        ("m", "(V)V"),
        ("<clinit>", "()V"),
        ("<init>", "()I"),
    ] {
        let mut class = ClassBuilder::new();
        class.nat(name, descriptor);
        assert!(parse(&class.finish()).is_err(), "{name} {descriptor}");
    }
    for major in [51, 52] {
        let mut class = ClassBuilder::new();
        class.major = major;
        let nat = class.nat("m", "()V");
        let target = class.constant(11, &words(&[class.this, nat]));
        let mut handle = vec![6];
        handle.extend(target.to_be_bytes());
        class.constant(15, &handle);
        assert_eq!(parse(&class.finish()).is_ok(), major >= 52);
    }
}

#[test]
fn duplicate_members_and_class_initializers_rejected() {
    let mut class = ClassBuilder::new();
    class.field("x", "I", 1, &[]);
    class.field("x", "I", 1, &[]);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    class.method("m", "()V", 0x0101, &[]);
    class.method("m", "()V", 0x0101, &[]);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    class.major = 50;
    let code = class.attribute("Code", &code_body(1, 1, &[], &[]));
    class.method("<clinit>", "()V", 0, std::slice::from_ref(&code));
    class.method("<clinit>", "(I)V", 0, &[code]);
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn inner_class_historical_rule_and_local_names() {
    for major in [50, 51] {
        let mut class = ClassBuilder::new();
        class.major = major;
        let outer = class.class("Outer");
        let attribute = class.attribute("InnerClasses", &words(&[1, class.this, outer, 0, 0]));
        class.attributes.push(attribute);
        assert_eq!(parse(&class.finish()).is_ok(), major < 51);
    }
    let mut class = ClassBuilder::new();
    let attribute = class.attribute("InnerClasses", &words(&[1, class.this, class.this, 0, 0]));
    class.attributes.push(attribute);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    let bad_name = class.utf8("bad/name");
    let attr = class.attribute(
        "MethodParameters",
        &[
            1,
            bad_name.to_be_bytes()[0],
            bad_name.to_be_bytes()[1],
            0,
            0,
        ],
    );
    class.method("m", "()V", 0x0101, &[attr]);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    let bad_name = class.utf8("bad/name");
    let descriptor = class.utf8("I");
    let table = class.attribute(
        "LocalVariableTable",
        &words(&[1, 0, 1, bad_name, descriptor, 0]),
    );
    let code = class.attribute("Code", &code_body(1, 1, &[], &[table]));
    class.method("m", "()V", 1, &[code]);
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn method_parameter_count_is_retained_for_reflection_validation() {
    let mut class = ClassBuilder::new();
    // MethodParameters count matching belongs to the reflection libraries;
    // reserved access bits also remain available in the parsed representation.
    let attr = class.attribute("MethodParameters", &[1, 0, 0, 0, 1]);
    class.method("m", "()V", 0x0101, &[attr]);
    let parsed = parse(&class.finish()).unwrap();
    assert_eq!(
        parsed.methods[0].attributes[0].body,
        AttributeBody::MethodParameters(vec![MethodParameter {
            name_index: 0,
            access_flags: 1
        }])
    );
}

#[test]
fn record_nesting_and_opaque_code_extensions_are_grammar_bounded() {
    let mut class = ClassBuilder::new();
    let nested = class.attribute("Code", &[0xff]);
    let code = class.attribute("Code", &code_body(1, 1, &[], &[nested]));
    class.method("m", "()V", 1, &[code]);
    let parsed = parse(&class.finish()).unwrap();
    let AttributeBody::Code(code) = &parsed.methods[0].attributes[0].body else {
        panic!()
    };
    assert_eq!(
        code.attributes[0].body,
        AttributeBody::Opaque {
            bytes: vec![0xff],
            standard: false
        }
    );
    let mut class = ClassBuilder::new();
    let name = class.utf8("x");
    let descriptor = class.utf8("I");
    let nested = class.attribute("Record", &[0xff]);
    let mut body = words(&[1, name, descriptor, 1]);
    body.extend(nested);
    let record = class.attribute("Record", &body);
    class.attributes.push(record);
    let parsed = parse(&class.finish()).unwrap();
    let AttributeBody::Record(components) = &parsed.attributes[0].body else {
        panic!()
    };
    assert_eq!(
        components[0].attributes[0].body,
        AttributeBody::Opaque {
            bytes: vec![0xff],
            standard: false
        }
    );
}

#[test]
fn final_sealed_conflict_and_module_attribute_allowlist() {
    let mut class = ClassBuilder::new();
    class.flags = 0x31;
    let permitted = class.attribute("PermittedSubclasses", &words(&[0]));
    class.attributes.push(permitted);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    class.flags = 0x8000;
    class.this = class.class("module-info");
    class.super_class = 0;
    let module_name = class.utf8("java.base");
    let module_index = class.constant(19, &module_name.to_be_bytes());
    let module = class.attribute("Module", &words(&[module_index, 0, 0, 0, 0, 0, 0, 0]));
    let synthetic = class.attribute("Synthetic", &[]);
    class.attributes.extend([module, synthetic]);
    assert_eq!(
        parse(&class.finish()).unwrap_err().kind,
        ErrorKind::InvalidStructure("standard attribute is not permitted on module-info")
    );
    let mut class = ClassBuilder::new();
    let name = class.utf8("example.module");
    class.constant(19, &name.to_be_bytes());
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn inner_classes_reject_semantic_alias_duplicates() {
    let mut class = ClassBuilder::new();
    let alias = class.class("Test");
    let name = class.utf8("Test");
    let attribute = class.attribute(
        "InnerClasses",
        &words(&[2, class.this, 0, name, 1, alias, 0, name, 1]),
    );
    class.attributes.push(attribute);
    assert_eq!(
        parse(&class.finish()).unwrap_err().kind,
        ErrorKind::InvalidStructure("duplicate InnerClasses entry for the same class")
    );
    let mut class = ClassBuilder::new();
    let other = class.class("Other");
    let first_name = class.utf8("Test");
    let second_name = class.utf8("Other");
    let attribute = class.attribute(
        "InnerClasses",
        &words(&[2, class.this, 0, first_name, 1, other, 0, second_name, 1]),
    );
    class.attributes.push(attribute);
    assert!(parse(&class.finish()).is_ok());
}

#[test]
fn local_variable_attributes_check_semantic_duplicates_without_a_singleton_rule() {
    for table_name in ["LocalVariableTable", "LocalVariableTypeTable"] {
        for duplicate in [false, true] {
            let mut class = ClassBuilder::new();
            let first_name = class.utf8("x");
            let second_name = class.utf8(if duplicate { "x" } else { "y" });
            let descriptor = if table_name == "LocalVariableTable" {
                "Ljava/lang/String;"
            } else {
                "TT;"
            };
            let first_descriptor = class.utf8(descriptor);
            let second_descriptor = class.utf8(descriptor);
            let first = class.attribute(
                table_name,
                &words(&[1, 0, 1, first_name, first_descriptor, 0]),
            );
            let second = class.attribute(
                table_name,
                &words(&[1, 0, 1, second_name, second_descriptor, 0]),
            );
            let code = class.attribute("Code", &code_body(1, 1, &[], &[first, second]));
            class.method("m", "()V", 1, &[code]);
            let result = parse(&class.finish());
            if duplicate {
                assert_eq!(
                    result.unwrap_err().kind,
                    ErrorKind::InvalidStructure(
                        "a local variable appears in more than one table attribute of the same kind"
                    )
                );
            } else {
                assert!(result.is_ok());
            }
        }
    }
}

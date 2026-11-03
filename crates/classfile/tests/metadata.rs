//! Synthetic format cases complement the independently compiled Java 21 fixtures.
use crabspot_classfile::{AttributeBody, ClassFile, ErrorKind, parse};

fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}

#[derive(Clone)]
struct Member {
    flags: u16,
    name: u16,
    descriptor: u16,
    attributes: Vec<Vec<u8>>,
}

#[derive(Clone)]
struct Builder {
    pool: Vec<Vec<u8>>,
    flags: u16,
    this: u16,
    superclass: u16,
    fields: Vec<Member>,
    methods: Vec<Member>,
    attributes: Vec<Vec<u8>>,
}

impl Builder {
    fn new() -> Self {
        let mut builder = Self {
            pool: Vec::new(),
            flags: 0x21,
            this: 0,
            superclass: 0,
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
        };
        builder.this = builder.named(7, "example/Test");
        builder.superclass = builder.named(7, "java/lang/Object");
        builder
    }

    fn constant(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let index = (self.pool.len() + 1) as u16;
        let mut entry = vec![tag];
        entry.extend_from_slice(payload);
        self.pool.push(entry);
        if matches!(tag, 5 | 6) {
            self.pool.push(Vec::new());
        }
        index
    }

    fn utf8(&mut self, text: &str) -> u16 {
        assert!(text.is_ascii());
        let mut payload = (text.len() as u16).to_be_bytes().to_vec();
        payload.extend_from_slice(text.as_bytes());
        self.constant(1, &payload)
    }

    fn named(&mut self, tag: u8, name: &str) -> u16 {
        let index = self.utf8(name);
        self.constant(tag, &words(&[index]))
    }

    fn attribute(&mut self, name: &str, payload: &[u8]) -> Vec<u8> {
        let index = self.utf8(name);
        let mut bytes = words(&[index]);
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn class_attribute(&mut self, name: &str, payload: &[u8]) {
        let attribute = self.attribute(name, payload);
        self.attributes.push(attribute);
    }

    fn field(&mut self, descriptor: &str, flags: u16, attributes: Vec<Vec<u8>>) {
        let name = self.utf8("value");
        let descriptor = self.utf8(descriptor);
        self.fields.push(Member {
            flags,
            name,
            descriptor,
            attributes,
        });
    }

    fn method(&mut self, flags: u16, attributes: Vec<Vec<u8>>) {
        let name = self.utf8("method");
        let descriptor = self.utf8("()V");
        self.methods.push(Member {
            flags,
            name,
            descriptor,
            attributes,
        });
    }

    fn bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        bytes.extend(words(&[(self.pool.len() + 1) as u16]));
        bytes.extend(self.pool.iter().flatten());
        bytes.extend(words(&[self.flags, self.this, self.superclass, 0]));
        for members in [&self.fields, &self.methods] {
            bytes.extend(words(&[members.len() as u16]));
            for member in members {
                bytes.extend(words(&[
                    member.flags,
                    member.name,
                    member.descriptor,
                    member.attributes.len() as u16,
                ]));
                bytes.extend(member.attributes.iter().flatten());
            }
        }
        bytes.extend(words(&[self.attributes.len() as u16]));
        bytes.extend(self.attributes.iter().flatten());
        bytes
    }
}

fn accepted(builder: &Builder) -> ClassFile {
    parse(&builder.bytes()).unwrap_or_else(|error| panic!("unexpected rejection: {error}"))
}

fn rejected(builder: &Builder) {
    assert!(
        parse(&builder.bytes()).is_err(),
        "accepted malformed metadata"
    );
}

#[test]
fn constant_value_descriptor_and_constant_kind_matrix() {
    for descriptor in [
        "B",
        "C",
        "I",
        "S",
        "Z",
        "J",
        "F",
        "D",
        "Ljava/lang/String;",
        "Ljava/lang/Object;",
        "[I",
    ] {
        for kind in 0..5 {
            let mut builder = Builder::new();
            let index = match kind {
                0 => builder.constant(3, &i32::MIN.to_be_bytes()),
                1 => builder.constant(5, &i64::MIN.to_be_bytes()),
                2 => builder.constant(4, &0x7fc0_1234_u32.to_be_bytes()),
                3 => builder.constant(6, &0x7ff8_0000_0000_1234_u64.to_be_bytes()),
                _ => builder.named(8, "constant text"),
            };
            let attribute = builder.attribute("ConstantValue", &words(&[index]));
            // ConstantValue is allowed on instance fields, although initialization
            // semantics use it only for static fields (JVMS 4.7.2).
            builder.field(
                descriptor,
                if kind % 2 == 0 { 1 } else { 9 },
                vec![attribute],
            );
            let expected = match descriptor {
                "B" | "C" | "I" | "S" | "Z" => kind == 0,
                "J" => kind == 1,
                "F" => kind == 2,
                "D" => kind == 3,
                "Ljava/lang/String;" => kind == 4,
                _ => false,
            };
            if expected {
                let class = accepted(&builder);
                assert!(
                    matches!(class.fields[0].attributes[0].body, AttributeBody::ConstantValue(value) if value == index)
                );
            } else {
                rejected(&builder);
            }
        }
    }
}

#[test]
fn constant_value_rejects_invalid_indices_and_non_exact_payloads() {
    for payload in [
        vec![],
        vec![0],
        words(&[0]),
        words(&[u16::MAX]),
        vec![0, 1, 0],
    ] {
        let mut builder = Builder::new();
        let attribute = builder.attribute("ConstantValue", &payload);
        builder.field("I", 9, vec![attribute]);
        rejected(&builder);
    }
    let mut builder = Builder::new();
    let index = builder.utf8("wrong constant kind");
    let attribute = builder.attribute("ConstantValue", &words(&[index]));
    builder.field("I", 9, vec![attribute]);
    rejected(&builder);
}

#[test]
fn exceptions_decode_order_and_empty_table_and_reject_bad_references_and_counts() {
    let mut builder = Builder::new();
    let first = builder.named(7, "example/FirstException");
    let second = builder.named(7, "example/SecondException");
    let attribute = builder.attribute("Exceptions", &words(&[2, first, second]));
    builder.method(0x109, vec![attribute]);
    let class = accepted(&builder);
    assert!(
        matches!(&class.methods[0].attributes[0].body, AttributeBody::Exceptions(indices) if indices == &[first, second])
    );
    let mut builder = Builder::new();
    let attribute = builder.attribute("Exceptions", &words(&[0]));
    builder.method(0x109, vec![attribute]);
    assert!(
        matches!(&accepted(&builder).methods[0].attributes[0].body, AttributeBody::Exceptions(indices) if indices.is_empty())
    );
    for payload in [
        vec![],
        words(&[1]),
        words(&[1, 0]),
        words(&[1, u16::MAX]),
        words(&[u16::MAX]),
        words(&[0, 0]),
    ] {
        let mut builder = Builder::new();
        let attribute = builder.attribute("Exceptions", &payload);
        builder.method(0x109, vec![attribute]);
        rejected(&builder);
    }
    let mut builder = Builder::new();
    let wrong = builder.utf8("example/Exception");
    let attribute = builder.attribute("Exceptions", &words(&[1, wrong]));
    builder.method(0x109, vec![attribute]);
    rejected(&builder);
}

#[test]
fn enclosing_method_preserves_class_and_optional_method_and_checks_descriptor_kind() {
    for has_method in [false, true] {
        let mut builder = Builder::new();
        let enclosing = builder.named(7, "example/Outer");
        let method = if has_method {
            let name = builder.utf8("enclosing");
            let descriptor = builder.utf8("()V");
            builder.constant(12, &words(&[name, descriptor]))
        } else {
            0
        };
        builder.class_attribute("EnclosingMethod", &words(&[enclosing, method]));
        assert!(
            matches!(accepted(&builder).attributes[0].body, AttributeBody::EnclosingMethod { class_index, method_index } if class_index == enclosing && method_index == method)
        );
    }
    for mode in 0..6 {
        let mut builder = Builder::new();
        let enclosing = builder.named(7, "example/Outer");
        let name = builder.utf8("enclosing");
        let descriptor = builder.utf8("I");
        let field = builder.constant(12, &words(&[name, descriptor]));
        let payload = match mode {
            0 => words(&[0, 0]),
            1 => words(&[name, 0]),
            2 => words(&[enclosing, name]),
            3 => words(&[enclosing, field]),
            4 => words(&[enclosing]),
            _ => words(&[enclosing, 0, 0]),
        };
        builder.class_attribute("EnclosingMethod", &payload);
        rejected(&builder);
    }
}

#[test]
fn signature_and_source_file_are_utf8_indices_with_exact_two_byte_bodies() {
    for name in ["Signature", "SourceFile"] {
        let mut builder = Builder::new();
        // Consumer-specific string checks and generic-signature interpretation
        // are intentionally deferred; the class-file parser retains this index.
        let value = builder.utf8("not a Java identifier or generic signature");
        builder.class_attribute(name, &words(&[value]));
        let class = accepted(&builder);
        assert!(match class.attributes[0].body {
            AttributeBody::Signature(index) if name == "Signature" => index == value,
            AttributeBody::SourceFile(index) if name == "SourceFile" => index == value,
            _ => false,
        });
        for payload in [
            vec![],
            vec![0],
            words(&[0]),
            words(&[u16::MAX]),
            words(&[builder.this]),
            vec![0, value as u8, 0],
        ] {
            let mut invalid = builder.clone();
            invalid.attributes.clear();
            invalid.class_attribute(name, &payload);
            rejected(&invalid);
        }
    }
}

#[test]
fn source_debug_extension_validates_modified_utf8_and_preserves_original_bytes() {
    for payload in [
        vec![],
        b"SMAP\nTest.java\nJava\n".to_vec(),
        vec![0xc0, 0x80, 0xed, 0xa0, 0x80, 0xed, 0xb0, 0x80],
    ] {
        let mut builder = Builder::new();
        builder.class_attribute("SourceDebugExtension", &payload);
        assert!(
            matches!(&accepted(&builder).attributes[0].body, AttributeBody::SourceDebugExtension(bytes) if bytes == &payload)
        );
    }
    for payload in [
        vec![0],
        vec![0x80],
        vec![0xc0, 0x81],
        vec![0xc2],
        vec![0xe0, 0x80, 0x80],
        vec![0xed, 0xa0],
        vec![0xf0, 0x90, 0x80, 0x80],
    ] {
        let mut builder = Builder::new();
        builder.class_attribute("SourceDebugExtension", &payload);
        let error = parse(&builder.bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::InvalidModifiedUtf8));
    }
}

fn with_code_attribute(builder: &mut Builder, table: &str, payload: &[u8], max_locals: u16) {
    let nested = builder.attribute(table, payload);
    let mut code = words(&[0, max_locals]);
    code.extend_from_slice(&4_u32.to_be_bytes());
    code.extend_from_slice(&[0, 0, 0, 0xb1]);
    code.extend(words(&[0, 1]));
    code.extend(nested);
    let attribute = builder.attribute("Code", &code);
    builder.method(9, vec![attribute]);
}

fn nested_body(class: &ClassFile) -> &AttributeBody {
    let AttributeBody::Code(code) = &class.methods[0].attributes[0].body else {
        panic!("missing Code")
    };
    &code.attributes[0].body
}

#[test]
fn line_numbers_allow_zero_lines_and_duplicate_starts_but_require_in_code_offsets() {
    let mut builder = Builder::new();
    with_code_attribute(
        &mut builder,
        "LineNumberTable",
        &words(&[3, 0, 0, 3, u16::MAX, 3, 7]),
        0,
    );
    let class = accepted(&builder);
    let AttributeBody::LineNumberTable(lines) = nested_body(&class) else {
        panic!("missing lines")
    };
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.start_pc, line.line_number))
            .collect::<Vec<_>>(),
        [(0, 0), (3, u16::MAX), (3, 7)]
    );
    for payload in [
        vec![],
        words(&[1]),
        words(&[1, 4, 1]),
        words(&[1, u16::MAX, 1]),
        words(&[0, 0]),
    ] {
        let mut builder = Builder::new();
        with_code_attribute(&mut builder, "LineNumberTable", &payload, 0);
        rejected(&builder);
    }
}

#[test]
fn local_variable_tables_decode_zero_length_end_boundary_and_category_two_slots() {
    for table in ["LocalVariableTable", "LocalVariableTypeTable"] {
        let mut builder = Builder::new();
        let name = builder.utf8("local");
        let descriptor = builder.utf8(if table == "LocalVariableTable" {
            "J"
        } else {
            "Ljava/util/List<Ljava/lang/String;>;"
        });
        let payload = words(&[2, 0, 4, name, descriptor, 0, 3, 0, name, descriptor, 0]);
        with_code_attribute(&mut builder, table, &payload, 2);
        let class = accepted(&builder);
        let entries = match nested_body(&class) {
            AttributeBody::LocalVariableTable(entries)
            | AttributeBody::LocalVariableTypeTable(entries) => entries,
            _ => panic!("missing local metadata"),
        };
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].length, 4);
        assert_eq!(entries[0].name_index, name);
        assert_eq!(entries[0].descriptor_or_signature_index, descriptor);
        assert_eq!(entries[1].start_pc, 3);
        assert_eq!(entries[1].length, 0);
    }
}

#[test]
fn local_variable_tables_check_ranges_slots_names_indices_and_payload_lengths() {
    for table in ["LocalVariableTable", "LocalVariableTypeTable"] {
        for mode in 0..12 {
            let mut builder = Builder::new();
            let name = builder.utf8(if mode == 4 { "bad/name" } else { "local" });
            let descriptor = builder.utf8(if table == "LocalVariableTable" {
                "J"
            } else {
                "unparsed generic signature"
            });
            let mut entry = [0, 4, name, descriptor, 0];
            match mode {
                0 => entry[0] = 4,
                1 => entry[1] = 5,
                2 => {
                    entry[0] = 3;
                    entry[1] = u16::MAX;
                }
                3 => entry[4] = 2,
                5 => entry[2] = 0,
                6 => entry[2] = builder.this,
                7 => entry[3] = 0,
                8 => entry[3] = builder.this,
                9 => entry[4] = u16::MAX,
                _ => {}
            }
            let mut payload = words(&[1]);
            payload.extend(words(&entry));
            if mode == 10 {
                payload.pop();
            }
            if mode == 11 {
                payload.push(0);
            }
            with_code_attribute(&mut builder, table, &payload, 2);
            rejected(&builder);
        }
    }
    for descriptor in ["J", "D"] {
        let mut builder = Builder::new();
        let name = builder.utf8("local");
        let descriptor = builder.utf8(descriptor);
        with_code_attribute(
            &mut builder,
            "LocalVariableTable",
            &words(&[1, 0, 4, name, descriptor, 1]),
            2,
        );
        rejected(&builder);
    }
    let mut builder = Builder::new();
    let name = builder.utf8("local");
    let descriptor = builder.utf8("V");
    with_code_attribute(
        &mut builder,
        "LocalVariableTable",
        &words(&[1, 0, 4, name, descriptor, 0]),
        1,
    );
    rejected(&builder);
}

#[test]
fn nest_metadata_preserves_indices_and_rejects_conflicts_bad_types_and_lengths() {
    for members in [false, true] {
        let mut builder = Builder::new();
        let other = builder.named(7, "example/Other");
        let name = if members { "NestMembers" } else { "NestHost" };
        builder.class_attribute(
            name,
            &if members {
                words(&[1, other])
            } else {
                words(&[other])
            },
        );
        let class = accepted(&builder);
        assert!(match &class.attributes[0].body {
            AttributeBody::NestHost(index) => *index == other,
            AttributeBody::NestMembers(indices) => indices == &[other],
            _ => false,
        });
        for payload in [
            vec![],
            vec![0],
            if members { words(&[1, 0]) } else { words(&[0]) },
            if members {
                words(&[1, u16::MAX])
            } else {
                words(&[u16::MAX])
            },
            words(&[0, 0, 0]),
        ] {
            let mut invalid = builder.clone();
            invalid.attributes.clear();
            invalid.class_attribute(name, &payload);
            rejected(&invalid);
        }
        let mut invalid = builder.clone();
        invalid.attributes.clear();
        let wrong = invalid.utf8("not Class");
        invalid.class_attribute(
            name,
            &if members {
                words(&[1, wrong])
            } else {
                words(&[wrong])
            },
        );
        rejected(&invalid);
    }
    let mut builder = Builder::new();
    let host = builder.named(7, "example/Other");
    builder.class_attribute("NestHost", &words(&[host]));
    builder.class_attribute("NestMembers", &words(&[0]));
    rejected(&builder);
    let mut builder = Builder::new();
    builder.class_attribute("NestMembers", &words(&[0]));
    assert!(
        matches!(&accepted(&builder).attributes[0].body, AttributeBody::NestMembers(indices) if indices.is_empty())
    );
}

#[test]
fn local_variable_type_signature_is_preserved_for_its_later_consumer() {
    let mut builder = Builder::new();
    let name = builder.utf8("local");
    let signature = builder.utf8("not a generic signature");
    with_code_attribute(
        &mut builder,
        "LocalVariableTypeTable",
        &words(&[1, 0, 4, name, signature, 0]),
        1,
    );
    let class = accepted(&builder);
    assert!(
        matches!(nested_body(&class), AttributeBody::LocalVariableTypeTable(entries) if entries[0].descriptor_or_signature_index == signature)
    );
}

fn module_builder() -> Builder {
    let mut builder = Builder::new();
    builder.flags = 0x8000;
    builder.this = builder.named(7, "module-info");
    builder.superclass = 0;
    let module = builder.named(19, "java.base");
    builder.class_attribute("Module", &words(&[module, 0, 0, 0, 0, 0, 0, 0]));
    builder
}

#[test]
fn module_packages_and_main_class_decode_and_check_types_and_exact_lengths() {
    let mut builder = module_builder();
    let package = builder.named(20, "example/api");
    let main = builder.named(7, "example/Main");
    builder.class_attribute("ModulePackages", &words(&[1, package]));
    builder.class_attribute("ModuleMainClass", &words(&[main]));
    let class = accepted(&builder);
    assert!(
        matches!(&class.attributes[1].body, AttributeBody::ModulePackages(indices) if indices == &[package])
    );
    assert!(
        matches!(class.attributes[2].body, AttributeBody::ModuleMainClass(index) if index == main)
    );
    let mut empty = module_builder();
    empty.class_attribute("ModulePackages", &words(&[0]));
    assert!(
        matches!(&accepted(&empty).attributes[1].body, AttributeBody::ModulePackages(indices) if indices.is_empty())
    );

    for name in ["ModulePackages", "ModuleMainClass"] {
        let mut builder = module_builder();
        let wrong = builder.utf8("wrong kind");
        let mut payloads = vec![vec![], vec![0]];
        for index in [0, u16::MAX, wrong] {
            payloads.push(if name == "ModulePackages" {
                words(&[1, index])
            } else {
                words(&[index])
            });
        }
        payloads.push(if name == "ModulePackages" {
            words(&[0, 0])
        } else {
            words(&[builder.this, 0])
        });
        if name == "ModulePackages" {
            payloads.push(words(&[u16::MAX]));
        }
        for payload in payloads {
            let mut invalid = builder.clone();
            invalid.class_attribute(name, &payload);
            rejected(&invalid);
        }
    }
}

#[test]
fn module_packages_inventory_relationship_is_checked_through_public_parser() {
    let mut builder = module_builder();
    builder.attributes.clear();
    let module = builder.named(19, "example.module");
    let base = builder.named(19, "java.base");
    let package = builder.named(20, "example/api");
    let alias = builder.named(20, "example/api");
    let service = builder.named(7, "example/api/Service");
    let implementation = builder.named(7, "example/api/Implementation");
    builder.class_attribute(
        "Module",
        &words(&[
            module,
            0,
            0,
            1,
            base,
            0x8000,
            0,
            1,
            package,
            0,
            0,
            0,
            0,
            1,
            service,
            1,
            implementation,
        ]),
    );
    // The inventory is optional. If present, semantic names must include
    // exported/opened and service-implementation packages (JVMS 4.7.26).
    accepted(&builder);
    let mut complete = builder.clone();
    complete.class_attribute("ModulePackages", &words(&[1, alias]));
    accepted(&complete);
    let mut missing = builder.clone();
    missing.class_attribute("ModulePackages", &words(&[0]));
    rejected(&missing);
    builder.class_attribute("ModulePackages", &words(&[2, package, alias]));
    rejected(&builder);
}

use super::*;

fn constant_value(tag: u8, index: u16) -> Vec<u8> {
    let mut value = vec![tag];
    u2(&mut value, index);
    value
}
fn annotation_body(class: &mut ClassBuilder, values: &[Vec<u8>]) -> Vec<u8> {
    let annotation_type = class.utf8("LAnn;");
    let mut bytes = words(&[annotation_type, values.len() as u16]);
    for (position, value) in values.iter().enumerate() {
        let name = class.utf8(&format!("value{position}"));
        u2(&mut bytes, name);
        bytes.extend(value);
    }
    bytes
}
fn regular_body(class: &mut ClassBuilder, values: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = words(&[1]);
    bytes.extend(annotation_body(class, values));
    bytes
}
fn type_body(class: &mut ClassBuilder, target: &[u8], path: &[u8]) -> Vec<u8> {
    let mut bytes = words(&[1]);
    bytes.extend(target);
    bytes.push((path.len() / 2) as u8);
    bytes.extend(path);
    bytes.extend(annotation_body(class, &[]));
    bytes
}
fn default_class(arrays: usize) -> Vec<u8> {
    let mut class = ClassBuilder::new();
    class.flags = 0x2601;
    let integer = class.constant(3, &1u32.to_be_bytes());
    let mut value = Vec::new();
    for _ in 0..arrays {
        value.extend([b'[', 0, 1]);
    }
    value.extend(constant_value(b'I', integer));
    let attr = class.attribute("AnnotationDefault", &value);
    class.method("value", "()I", 0x0401, &[attr]);
    class.finish()
}

#[test]
fn all_annotation_element_tags_decode_to_typed_lossless_values() {
    let mut class = ClassBuilder::new();
    let integer = class.constant(3, &42u32.to_be_bytes());
    let float = class.constant(4, &0x80000000u32.to_be_bytes());
    let long = class.constant(5, &u64::MAX.to_be_bytes());
    let double = class.constant(6, &0x7ff8000000000042u64.to_be_bytes());
    let string = class.utf8("text");
    let constants = [
        (b'B', AnnotationConstantTag::Byte, integer),
        (b'C', AnnotationConstantTag::Char, integer),
        (b'D', AnnotationConstantTag::Double, double),
        (b'F', AnnotationConstantTag::Float, float),
        (b'I', AnnotationConstantTag::Int, integer),
        (b'J', AnnotationConstantTag::Long, long),
        (b'S', AnnotationConstantTag::Short, integer),
        (b'Z', AnnotationConstantTag::Boolean, integer),
        (b's', AnnotationConstantTag::String, string),
    ];
    let mut values: Vec<Vec<u8>> = constants
        .iter()
        .map(|(tag, _, index)| constant_value(*tag, *index))
        .collect();
    let enum_type = class.utf8("LChoice;");
    let enum_name = class.utf8("YES");
    let mut enumeration = vec![b'e'];
    enumeration.extend(words(&[enum_type, enum_name]));
    values.push(enumeration);
    let void = class.utf8("V");
    values.push(constant_value(b'c', void));
    let array_type = class.utf8("[Ljava/lang/String;");
    values.push(constant_value(b'c', array_type));
    let mut nested = vec![b'@'];
    nested.extend(annotation_body(&mut class, &[]));
    values.push(nested);
    let mut array = vec![b'[', 0, 2];
    array.extend(constant_value(b'I', integer));
    array.extend(constant_value(b's', string));
    values.push(array);
    let body = regular_body(&mut class, &values);
    let visible = class.attribute("RuntimeVisibleAnnotations", &body);
    let invisible = class.attribute("RuntimeInvisibleAnnotations", &body);
    class.attributes.extend([visible, invisible]);
    let parsed = parse(&class.finish()).unwrap();
    let AttributeBody::RuntimeVisibleAnnotations(annotations) = &parsed.attributes[0].body else {
        panic!()
    };
    let elements = &annotations[0].elements;
    assert_eq!(elements.len(), 14);
    for (element, (_, tag, constant_index)) in elements.iter().zip(constants) {
        assert_eq!(
            element.value,
            ElementValue::Constant {
                tag,
                constant_index
            }
        );
    }
    assert_eq!(
        elements[9].value,
        ElementValue::Enum {
            type_name_index: enum_type,
            const_name_index: enum_name
        }
    );
    assert_eq!(
        elements[10].value,
        ElementValue::Class {
            class_info_index: void
        }
    );
    assert_eq!(
        elements[11].value,
        ElementValue::Class {
            class_info_index: array_type
        }
    );
    assert!(
        matches!(&elements[12].value,ElementValue::Annotation(annotation) if annotation.elements.is_empty())
    );
    assert_eq!(
        elements[13].value,
        ElementValue::Array(vec![
            ElementValue::Constant {
                tag: AnnotationConstantTag::Int,
                constant_index: integer
            },
            ElementValue::Constant {
                tag: AnnotationConstantTag::String,
                constant_index: string
            }
        ])
    );
    assert_eq!(
        parsed.attributes[1].body,
        AttributeBody::RuntimeInvisibleAnnotations(annotations.clone())
    );
}

#[test]
fn parameter_annotations_preserve_descriptor_count_independence() {
    let mut class = ClassBuilder::new();
    let annotation = annotation_body(&mut class, &[]);
    let mut body = vec![2];
    body.extend(words(&[1]));
    body.extend(annotation);
    body.extend(words(&[0]));
    let visible = class.attribute("RuntimeVisibleParameterAnnotations", &body);
    let invisible = class.attribute("RuntimeInvisibleParameterAnnotations", &body);
    class.method("m", "()V", 0x0101, &[visible, invisible]);
    let parsed = parse(&class.finish()).unwrap();
    let AttributeBody::RuntimeVisibleParameterAnnotations(parameters) =
        &parsed.methods[0].attributes[0].body
    else {
        panic!()
    };
    assert_eq!(parameters.len(), 2);
    assert_eq!(parameters[0].len(), 1);
    assert!(parameters[1].is_empty());
    assert_eq!(
        parsed.methods[0].attributes[1].body,
        AttributeBody::RuntimeInvisibleParameterAnnotations(parameters.clone())
    );
}

#[test]
fn annotation_depth_policy_and_hard_cap_are_explicit() {
    assert!(
        parse_with_options(
            &default_class(2),
            ParseOptions {
                max_annotation_depth: 3,
                ..Default::default()
            }
        )
        .is_ok()
    );
    let error = parse_with_options(
        &default_class(3),
        ParseOptions {
            max_annotation_depth: 3,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::ResourceLimit {
            resource: "annotation nesting depth",
            limit: 3,
            actual: 4
        }
    );
    assert!(parse(&default_class(63)).is_ok());
    for depth in [64, 4096] {
        let error = parse_with_options(
            &default_class(depth),
            ParseOptions {
                max_annotation_depth: usize::MAX,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::ResourceLimit {
                resource: "annotation nesting depth",
                limit: 64,
                actual: 65
            }
        );
    }
    assert!(matches!(
        parse_with_options(
            &default_class(0),
            ParseOptions {
                max_annotation_depth: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        ErrorKind::ResourceLimit { limit: 0, .. }
    ));
}

#[test]
fn annotation_budget_is_shared_across_members_attributes_and_nested_values() {
    let mut class = ClassBuilder::new();
    let body = regular_body(&mut class, &[]);
    let field = class.attribute("RuntimeVisibleAnnotations", &body);
    class.field("x", "I", 1, &[field]);
    let top = class.attribute("RuntimeInvisibleAnnotations", &body);
    class.attributes.push(top);
    let bytes = class.finish();
    assert!(
        parse_with_options(
            &bytes,
            ParseOptions {
                max_annotation_values: 2,
                ..Default::default()
            }
        )
        .is_ok()
    );
    assert_eq!(
        parse_with_options(
            &bytes,
            ParseOptions {
                max_annotation_values: 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        ErrorKind::ResourceLimit {
            resource: "annotation and element-value nodes",
            limit: 1,
            actual: 2
        }
    );
    assert!(
        parse_with_options(
            &default_class(0),
            ParseOptions {
                max_annotation_values: 1,
                ..Default::default()
            }
        )
        .is_ok()
    );
    assert!(matches!(
        parse_with_options(
            &default_class(1),
            ParseOptions {
                max_annotation_values: 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        ErrorKind::ResourceLimit {
            limit: 1,
            actual: 2,
            ..
        }
    ));
    let mut class = ClassBuilder::new();
    let zero = class.attribute("RuntimeVisibleAnnotations", &[0, 0]);
    class.attributes.push(zero);
    assert!(
        parse_with_options(
            &class.finish(),
            ParseOptions {
                max_annotation_values: 0,
                max_annotation_depth: 0,
                ..Default::default()
            }
        )
        .is_ok()
    );
}

#[test]
fn annotation_invalid_tags_references_descriptors_and_names_rejected() {
    for tag in *b"BCDFIJSZs?" {
        let mut class = ClassBuilder::new();
        let value = constant_value(tag, class.this);
        let body = regular_body(&mut class, &[value]);
        let attr = class.attribute("RuntimeVisibleAnnotations", &body);
        class.attributes.push(attr);
        assert!(parse(&class.finish()).is_err(), "tag{tag}");
    }
    for descriptor in ["I", "[LAnn;", "L;", "()V"] {
        let mut class = ClassBuilder::new();
        let annotation_type = class.utf8(descriptor);
        let attr = class.attribute(
            "RuntimeVisibleAnnotations",
            &words(&[1, annotation_type, 0]),
        );
        class.attributes.push(attr);
        assert!(parse(&class.finish()).is_err(), "{descriptor}");
    }
    for (tag, text) in [
        (b'e', "I"),
        (b'e', "[LChoice;"),
        (b'c', "()V"),
        (b'c', "[V"),
    ] {
        let mut class = ClassBuilder::new();
        let descriptor = class.utf8(text);
        let mut value = constant_value(tag, descriptor);
        if tag == b'e' {
            let name = class.utf8("YES");
            u2(&mut value, name);
        }
        let body = regular_body(&mut class, &[value]);
        let attr = class.attribute("RuntimeVisibleAnnotations", &body);
        class.attributes.push(attr);
        assert!(parse(&class.finish()).is_err(), "{tag} {text}");
    }
    let mut class = ClassBuilder::new();
    let annotation_type = class.utf8("LAnn;");
    let name = class.utf8("<init>");
    let mut body = words(&[1, annotation_type, 1, name]);
    body.extend(constant_value(b's', name));
    let attr = class.attribute("RuntimeVisibleAnnotations", &body);
    class.attributes.push(attr);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    let enum_type = class.utf8("LChoice;");
    let name = class.utf8("bad/name");
    let mut value = vec![b'e'];
    value.extend(words(&[enum_type, name]));
    let body = regular_body(&mut class, &[value]);
    let attr = class.attribute("RuntimeVisibleAnnotations", &body);
    class.attributes.push(attr);
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn annotation_envelopes_are_exact_and_extensions_remain_opaque() {
    let mut base = ClassBuilder::new();
    let body = regular_body(&mut base, &[]);
    for end in 0..body.len() {
        let mut class = ClassBuilder::new();
        class.utf8("LAnn;");
        let attr = class.attribute("RuntimeVisibleAnnotations", &body[..end]);
        class.attributes.push(attr);
        assert!(parse(&class.finish()).is_err(), "prefix{end}");
    }
    let mut class = ClassBuilder::new();
    let mut body = regular_body(&mut class, &[]);
    body.push(0);
    let attr = class.attribute("RuntimeVisibleAnnotations", &body);
    class.attributes.push(attr);
    assert!(matches!(
        parse(&class.finish()).unwrap_err().kind,
        ErrorKind::TrailingBytes(1)
    ));
    let mut class = ClassBuilder::new();
    let body = regular_body(&mut class, &[]);
    let attr = class.attribute("RuntimeVisibleAnnotations", &body);
    class.attributes.extend([attr.clone(), attr]);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    class.major = 48;
    let attr = class.attribute("RuntimeVisibleAnnotations", &[255]);
    class.attributes.push(attr);
    let parsed = parse_with_options(
        &class.finish(),
        ParseOptions {
            max_annotation_depth: 0,
            max_annotation_values: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        parsed.attributes[0].body,
        AttributeBody::Opaque {
            bytes: vec![255],
            standard: false
        }
    );
}

#[test]
fn annotation_default_requires_an_annotation_element_context() {
    assert!(parse(&default_class(0)).is_ok());
    for (flags, name, descriptor) in [
        (0x21, "value", "()I"),
        (0x2601, "value", "(I)I"),
        (0x2601, "value", "()V"),
        (0x2601, "<clinit>", "()V"),
    ] {
        let mut class = ClassBuilder::new();
        class.flags = flags;
        let integer = class.constant(3, &1u32.to_be_bytes());
        let attr = class.attribute("AnnotationDefault", &constant_value(b'I', integer));
        class.method(name, descriptor, 0x0401, &[attr]);
        assert!(
            parse(&class.finish()).is_err(),
            "{flags:x} {name} {descriptor}"
        );
    }
}

#[derive(Clone, Copy)]
enum TestLocation {
    Class,
    Field,
    Method,
    Code,
}
fn type_class(target: &[u8], path: &[u8], location: TestLocation) -> Vec<u8> {
    let mut class = ClassBuilder::new();
    let body = type_body(&mut class, target, path);
    let visible = class.attribute("RuntimeVisibleTypeAnnotations", &body);
    let invisible = class.attribute("RuntimeInvisibleTypeAnnotations", &body);
    match location {
        TestLocation::Class => class.attributes.extend([visible, invisible]),
        TestLocation::Field => class.field("f", "Ljava/lang/Object;", 1, &[visible, invisible]),
        TestLocation::Method => {
            let exception = class.class("java/lang/Exception");
            let exceptions = class.attribute("Exceptions", &words(&[1, exception]));
            class.method("m", "(I)V", 0x0101, &[visible, invisible, exceptions]);
        }
        TestLocation::Code => {
            let code = class.attribute(
                "Code",
                &code_body(1, 1, &[[0, 1, 0, 0]], &[visible, invisible]),
            );
            class.method("m", "()V", 1, &[code]);
        }
    }
    class.finish()
}
fn annotation_attrs(class: &ClassFile, location: TestLocation) -> &[Attribute] {
    match location {
        TestLocation::Class => &class.attributes,
        TestLocation::Field => &class.fields[0].attributes,
        TestLocation::Method => &class.methods[0].attributes,
        TestLocation::Code => {
            let AttributeBody::Code(code) = &class.methods[0].attributes[0].body else {
                panic!()
            };
            &code.attributes
        }
    }
}

#[test]
fn every_type_annotation_target_and_path_kind_decodes() {
    use TypeAnnotationTarget as T;
    let cases: Vec<(Vec<u8>, TestLocation, T)> = vec![
        (vec![0x00, 0], TestLocation::Class, T::ClassTypeParameter(0)),
        (
            vec![0x10, 255, 255],
            TestLocation::Class,
            T::Supertype(65535),
        ),
        (
            vec![0x11, 1, 2],
            TestLocation::Class,
            T::ClassTypeParameterBound {
                type_parameter_index: 1,
                bound_index: 2,
            },
        ),
        (
            vec![0x01, 0],
            TestLocation::Method,
            T::MethodTypeParameter(0),
        ),
        (
            vec![0x12, 1, 2],
            TestLocation::Method,
            T::MethodTypeParameterBound {
                type_parameter_index: 1,
                bound_index: 2,
            },
        ),
        (vec![0x13], TestLocation::Field, T::Field),
        (vec![0x14], TestLocation::Method, T::MethodReturn),
        (vec![0x15], TestLocation::Method, T::MethodReceiver),
        (
            vec![0x16, 0],
            TestLocation::Method,
            T::MethodFormalParameter(0),
        ),
        (vec![0x17, 0, 0], TestLocation::Method, T::Throws(0)),
        (
            vec![0x40, 0, 1, 0, 0, 0, 1, 0, 0],
            TestLocation::Code,
            T::LocalVariable(vec![LocalVariableTarget {
                start_pc: 0,
                length: 1,
                index: 0,
            }]),
        ),
        (
            vec![0x41, 0, 1, 0, 0, 0, 1, 0, 0],
            TestLocation::Code,
            T::ResourceVariable(vec![LocalVariableTarget {
                start_pc: 0,
                length: 1,
                index: 0,
            }]),
        ),
        (
            vec![0x42, 0, 0],
            TestLocation::Code,
            T::ExceptionParameter(0),
        ),
        (vec![0x43, 0, 0], TestLocation::Code, T::InstanceOf(0)),
        (vec![0x44, 0, 0], TestLocation::Code, T::New(0)),
        (
            vec![0x45, 0, 0],
            TestLocation::Code,
            T::ConstructorReference(0),
        ),
        (vec![0x46, 0, 0], TestLocation::Code, T::MethodReference(0)),
        (
            vec![0x47, 0, 0, 3],
            TestLocation::Code,
            T::Cast {
                offset: 0,
                type_argument_index: 3,
            },
        ),
        (
            vec![0x48, 0, 0, 3],
            TestLocation::Code,
            T::ConstructorInvocationTypeArgument {
                offset: 0,
                type_argument_index: 3,
            },
        ),
        (
            vec![0x49, 0, 0, 3],
            TestLocation::Code,
            T::MethodInvocationTypeArgument {
                offset: 0,
                type_argument_index: 3,
            },
        ),
        (
            vec![0x4a, 0, 0, 3],
            TestLocation::Code,
            T::ConstructorReferenceTypeArgument {
                offset: 0,
                type_argument_index: 3,
            },
        ),
        (
            vec![0x4b, 0, 0, 3],
            TestLocation::Code,
            T::MethodReferenceTypeArgument {
                offset: 0,
                type_argument_index: 3,
            },
        ),
    ];
    let path = [0, 0, 1, 0, 2, 0, 3, 2];
    for (target, location, expected) in cases {
        let parsed = parse(&type_class(&target, &path, location)).unwrap();
        let attrs = annotation_attrs(&parsed, location);
        let AttributeBody::RuntimeVisibleTypeAnnotations(annotations) = &attrs[0].body else {
            panic!()
        };
        assert_eq!(annotations[0].target, expected);
        assert_eq!(
            annotations[0].path,
            vec![
                TypePathEntry {
                    kind: TypePathKind::Array,
                    type_argument_index: 0
                },
                TypePathEntry {
                    kind: TypePathKind::InnerType,
                    type_argument_index: 0
                },
                TypePathEntry {
                    kind: TypePathKind::WildcardBound,
                    type_argument_index: 0
                },
                TypePathEntry {
                    kind: TypePathKind::TypeArgument,
                    type_argument_index: 2
                }
            ]
        );
        assert_eq!(
            attrs[1].body,
            AttributeBody::RuntimeInvisibleTypeAnnotations(annotations.clone())
        );
    }
}

#[test]
fn type_annotation_locations_paths_and_available_ranges_checked() {
    for (target, location) in [
        (vec![0x13], TestLocation::Class),
        (vec![0x14], TestLocation::Field),
        (vec![0x13], TestLocation::Method),
        (vec![0x00, 0], TestLocation::Code),
        (vec![0xff], TestLocation::Class),
        (vec![0x18], TestLocation::Method),
    ] {
        assert!(parse(&type_class(&target, &[], location)).is_err());
    }
    for path in [[4, 0], [0, 1], [1, 2], [2, 3]] {
        assert!(
            parse(&type_class(&[0x13], &path, TestLocation::Field)).is_err(),
            "{path:?}"
        );
    }
    for (target, location) in [
        (vec![0x10, 0, 0], TestLocation::Class),
        (vec![0x16, 1], TestLocation::Method),
        (vec![0x17, 0, 1], TestLocation::Method),
        (vec![0x42, 0, 1], TestLocation::Code),
        (vec![0x43, 0, 1], TestLocation::Code),
        (vec![0x47, 0, 1, 0], TestLocation::Code),
        (vec![0x40, 0, 1, 0, 1, 0, 0, 0, 0], TestLocation::Code),
        (vec![0x40, 0, 1, 0, 0, 0, 2, 0, 0], TestLocation::Code),
        (vec![0x41, 0, 1, 0, 0, 0, 1, 0, 1], TestLocation::Code),
    ] {
        assert!(
            parse(&type_class(&target, &[], location)).is_err(),
            "{target:?}"
        );
    }
    let mut class = ClassBuilder::new();
    let body = type_body(&mut class, &[0x15], &[]);
    let attribute = class.attribute("RuntimeVisibleTypeAnnotations", &body);
    class.method("m", "()V", 0x0109, &[attribute]);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    let body = type_body(&mut class, &[0x17, 0, 0], &[]);
    let attribute = class.attribute("RuntimeVisibleTypeAnnotations", &body);
    class.method("m", "()V", 0x0101, &[attribute]);
    assert!(parse(&class.finish()).is_err());
    let mut class = ClassBuilder::new();
    let interface = class.class("Iface");
    class.interfaces.push(interface);
    let body = type_body(&mut class, &[0x10, 0, 0], &[]);
    let attribute = class.attribute("RuntimeVisibleTypeAnnotations", &body);
    class.attributes.push(attribute);
    assert!(parse(&class.finish()).is_ok());
    let mut class = ClassBuilder::new();
    class.flags = 0x0601;
    let body = type_body(&mut class, &[0x10, 255, 255], &[]);
    let attribute = class.attribute("RuntimeVisibleTypeAnnotations", &body);
    class.attributes.push(attribute);
    assert!(parse(&class.finish()).is_err());
}

#[test]
fn nested_annotations_share_the_same_hard_depth_and_node_limits() {
    let mut class = ClassBuilder::new();
    let annotation_type = class.utf8("LAnn;");
    let name = class.utf8("value");
    let mut annotation = words(&[annotation_type, 0]);
    for _ in 0..1000 {
        let mut next = words(&[annotation_type, 1, name]);
        next.push(b'@');
        next.extend(annotation);
        annotation = next;
    }
    let mut body = words(&[1]);
    body.extend(annotation);
    let attribute = class.attribute("RuntimeVisibleAnnotations", &body);
    class.attributes.push(attribute);
    let error = parse_with_options(
        &class.finish(),
        ParseOptions {
            max_annotation_depth: usize::MAX,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::ResourceLimit {
            resource: "annotation nesting depth",
            limit: 64,
            actual: 65
        }
    );
}

#[test]
fn marker_attributes_allow_multiplicity_and_source_debug_checks_modified_utf8() {
    let mut class = ClassBuilder::new();
    for name in ["Synthetic", "Synthetic", "Deprecated", "Deprecated"] {
        let attribute = class.attribute(name, &[]);
        class.attributes.push(attribute);
    }
    let raw = [b'A', 0xc0, 0x80, 0xed, 0xa0, 0x80];
    let attribute = class.attribute("SourceDebugExtension", &raw);
    class.attributes.push(attribute);
    let parsed = parse(&class.finish()).unwrap();
    assert_eq!(
        parsed.attributes[4].body,
        AttributeBody::SourceDebugExtension(raw.to_vec())
    );
    for raw in [vec![0], vec![0xc0, 0x81], vec![0xf0, 0x90, 0x80, 0x80]] {
        let mut class = ClassBuilder::new();
        let attribute = class.attribute("SourceDebugExtension", &raw);
        class.attributes.push(attribute);
        assert_eq!(
            parse(&class.finish()).unwrap_err().kind,
            ErrorKind::InvalidModifiedUtf8
        );
    }
}

#[test]
fn unused_module_package_names_are_checked_before_context() {
    for (tag, name, reason) in [
        (19, "bad:module", "invalid module name"),
        (20, "bad.package", "invalid package name"),
    ] {
        let mut class = ClassBuilder::new();
        let name = class.utf8(name);
        class.constant(tag, &name.to_be_bytes());
        assert_eq!(
            parse(&class.finish()).unwrap_err().kind,
            ErrorKind::InvalidStructure(reason)
        );
    }
    for tag in [19, 20] {
        let mut class = ClassBuilder::new();
        let name = class.utf8("valid/name");
        class.constant(tag, &name.to_be_bytes());
        assert_eq!(
            parse(&class.finish()).unwrap_err().kind,
            ErrorKind::InvalidStructure("Module/Package constants require ACC_MODULE")
        );
    }
}

use crabspot_classfile::{
    AnnotationConstantTag, Attribute, AttributeBody, ElementValue, TypeAnnotationTarget,
    TypePathKind, parse,
};

const ANNOTATED: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/Annotated.class");
const RICH: &[u8] = include_bytes!("../../../tests/fixtures/classes/fixtures/RichAnnotation.class");
const EXTRA: &[&[u8]] = &[
    ANNOTATED,
    RICH,
    include_bytes!("../../../tests/fixtures/classes/fixtures/Choice.class"),
    include_bytes!("../../../tests/fixtures/classes/fixtures/Nested.class"),
    include_bytes!("../../../tests/fixtures/classes/fixtures/InvisibleDeclaration.class"),
    include_bytes!("../../../tests/fixtures/classes/fixtures/VisibleType.class"),
    include_bytes!("../../../tests/fixtures/classes/fixtures/InvisibleType.class"),
    include_bytes!("../../../tests/fixtures/classes/fixtures/Service.class"),
    include_bytes!("../../../tests/fixtures/classes/fixtures/ServiceProvider.class"),
];

fn assert_typed(attributes: &[Attribute]) {
    for attribute in attributes {
        match &attribute.body {
            AttributeBody::Opaque { standard: true, .. } => {
                panic!("standard attribute still opaque")
            }
            AttributeBody::Code(code) => assert_typed(&code.attributes),
            AttributeBody::Record(components) => {
                for component in components {
                    assert_typed(&component.attributes);
                }
            }
            _ => {}
        }
    }
}

#[test]
fn all_extended_javac_fixtures_parse_without_opaque_standard_attributes() {
    for bytes in EXTRA {
        let class = parse(bytes).unwrap();
        assert_eq!(class.version.major, 65);
        assert_typed(&class.attributes);
        for member in class.fields.iter().chain(&class.methods) {
            assert_typed(&member.attributes);
        }
    }
}

#[test]
fn annotation_defaults_preserve_every_element_value_kind() {
    let class = parse(RICH).unwrap();
    let mut scalars = Vec::new();
    let mut composites = [false; 4];
    let mut defaults = 0;
    for method in &class.methods {
        for attribute in &method.attributes {
            if let AttributeBody::AnnotationDefault(value) = &attribute.body {
                defaults += 1;
                match value {
                    ElementValue::Constant {
                        tag,
                        constant_index,
                    } => {
                        scalars.push(*tag);
                        if *tag == AnnotationConstantTag::String {
                            assert_eq!(
                                class.constant_pool.utf8(*constant_index).unwrap().units(),
                                "annotation\0🦀".encode_utf16().collect::<Vec<_>>()
                            );
                        }
                    }
                    ElementValue::Enum {
                        const_name_index, ..
                    } => {
                        assert!(
                            class
                                .constant_pool
                                .utf8(*const_name_index)
                                .unwrap()
                                .equals_ascii("SECOND")
                        );
                        composites[0] = true;
                    }
                    ElementValue::Class { class_info_index } => {
                        assert!(
                            class
                                .constant_pool
                                .utf8(*class_info_index)
                                .unwrap()
                                .equals_ascii("V")
                        );
                        composites[1] = true;
                    }
                    ElementValue::Annotation(annotation) => {
                        assert!(
                            class
                                .constant_pool
                                .utf8(annotation.type_index)
                                .unwrap()
                                .equals_ascii("Lfixtures/Nested;")
                        );
                        assert_eq!(annotation.elements.len(), 1);
                        composites[2] = true;
                    }
                    ElementValue::Array(values) => {
                        assert_eq!(values.len(), 3);
                        composites[3] = true;
                    }
                }
            }
        }
    }
    assert_eq!(defaults, 13);
    for tag in [
        AnnotationConstantTag::Byte,
        AnnotationConstantTag::Char,
        AnnotationConstantTag::Double,
        AnnotationConstantTag::Float,
        AnnotationConstantTag::Int,
        AnnotationConstantTag::Long,
        AnnotationConstantTag::Short,
        AnnotationConstantTag::Boolean,
        AnnotationConstantTag::String,
    ] {
        assert!(scalars.contains(&tag));
    }
    assert_eq!(composites, [true; 4]);
}

#[test]
fn decodes_visible_invisible_parameter_and_code_type_annotations() {
    let class = parse(ANNOTATED).unwrap();
    assert!(class.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::RuntimeVisibleAnnotations(values) if !values.is_empty())));
    assert!(class.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::RuntimeInvisibleAnnotations(values) if !values.is_empty())));
    let field = class
        .fields
        .iter()
        .find(|field| {
            class
                .constant_pool
                .utf8(field.name_index)
                .unwrap()
                .equals_ascii("values")
        })
        .unwrap();
    for visible in [true, false] {
        let values = field
            .attributes
            .iter()
            .find_map(|attribute| match &attribute.body {
                AttributeBody::RuntimeVisibleTypeAnnotations(values) if visible => Some(values),
                AttributeBody::RuntimeInvisibleTypeAnnotations(values) if !visible => Some(values),
                _ => None,
            })
            .unwrap();
        assert!(values.iter().any(|value| {
            value
                .path
                .iter()
                .any(|part| part.kind == TypePathKind::TypeArgument)
        }));
    }
    let method = class
        .methods
        .iter()
        .find(|method| {
            class
                .constant_pool
                .utf8(method.name_index)
                .unwrap()
                .equals_ascii("method")
        })
        .unwrap();
    assert!(method.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::RuntimeVisibleParameterAnnotations(parameters) if parameters.len() == 1 && parameters[0].len() == 1)));
    assert!(method.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::RuntimeInvisibleParameterAnnotations(parameters) if parameters.len() == 1 && parameters[0].len() == 1)));
    let code = method
        .attributes
        .iter()
        .find_map(|attribute| match &attribute.body {
            AttributeBody::Code(code) => Some(code),
            _ => None,
        })
        .unwrap();
    let types = code
        .attributes
        .iter()
        .find_map(|attribute| match &attribute.body {
            AttributeBody::RuntimeVisibleTypeAnnotations(types) => Some(types),
            _ => None,
        })
        .unwrap();
    assert!(
        types
            .iter()
            .any(|value| matches!(value.target, TypeAnnotationTarget::Cast { .. }))
    );
    assert!(
        types
            .iter()
            .any(|value| matches!(value.target, TypeAnnotationTarget::New(_)))
    );
    assert!(
        types
            .iter()
            .any(|value| matches!(value.target, TypeAnnotationTarget::ExceptionParameter(_)))
    );
    assert!(
        types
            .iter()
            .any(|value| matches!(value.target, TypeAnnotationTarget::LocalVariable(_)))
    );
}

#[test]
fn every_extended_fixture_truncation_is_rejected_without_panicking() {
    for bytes in EXTRA {
        for end in 0..bytes.len() {
            assert!(parse(&bytes[..end]).is_err(), "prefix {end}");
        }
    }
}

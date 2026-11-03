use crabspot_classfile::{AttributeBody, Constant, parse};

const FIXTURES: &[(&str, &[u8])] = &[
    (
        "fixtures/Fixture",
        include_bytes!("../../../tests/fixtures/classes/fixtures/Fixture.class"),
    ),
    (
        "fixtures/Fixture$Inner",
        include_bytes!("../../../tests/fixtures/classes/fixtures/Fixture$Inner.class"),
    ),
    (
        "fixtures/Marker",
        include_bytes!("../../../tests/fixtures/classes/fixtures/Marker.class"),
    ),
    (
        "fixtures/Point",
        include_bytes!("../../../tests/fixtures/classes/fixtures/Point.class"),
    ),
    (
        "fixtures/Shape",
        include_bytes!("../../../tests/fixtures/classes/fixtures/Shape.class"),
    ),
    (
        "fixtures/Circle",
        include_bytes!("../../../tests/fixtures/classes/fixtures/Circle.class"),
    ),
    (
        "fixtures/Square",
        include_bytes!("../../../tests/fixtures/classes/fixtures/Square.class"),
    ),
    (
        "module-info",
        include_bytes!("../../../tests/fixtures/classes/module-info.class"),
    ),
];

#[test]
fn parses_genuine_java21_fixtures() {
    for (name, bytes) in FIXTURES {
        let class = parse(bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(class.version.major, 65, "{name}");
        assert_eq!(class.version.minor, 0, "{name}");
        assert_eq!(class.name().unwrap().to_string_lossy(), *name);
    }
}

#[test]
fn extracts_code_stack_maps_bootstraps_and_lossless_strings() {
    let class = parse(FIXTURES[0].1).unwrap();
    let branch = class
        .methods
        .iter()
        .find(|method| {
            class
                .constant_pool
                .utf8(method.name_index)
                .unwrap()
                .equals_ascii("branch")
        })
        .unwrap();
    let code = branch
        .attributes
        .iter()
        .find_map(|attribute| match &attribute.body {
            AttributeBody::Code(code) => Some(code),
            _ => None,
        })
        .unwrap();
    assert!(!code.code.is_empty());
    assert_eq!(code.exception_table.len(), 2);
    assert!(code.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::StackMapTable(frames) if !frames.is_empty())));
    assert!(class.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::BootstrapMethods(methods) if methods.len() == 2)));
    let expected: Vec<u16> = "nul\0supplementary🦀".encode_utf16().collect();
    assert!(
        class
            .constant_pool
            .entries()
            .iter()
            .any(|constant| matches!(constant, Constant::Utf8(value) if value.units() == expected))
    );
    assert!(class.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::RuntimeVisibleAnnotations(annotations) if annotations.len() == 1)));
}

#[test]
fn extracts_record_nest_and_sealed_metadata() {
    let point = parse(FIXTURES[3].1).unwrap();
    let components = point
        .attributes
        .iter()
        .find_map(|attribute| match &attribute.body {
            AttributeBody::Record(components) => Some(components),
            _ => None,
        })
        .unwrap();
    assert_eq!(components.len(), 2);
    assert_eq!(
        point
            .constant_pool
            .utf8(components[0].name_index)
            .unwrap()
            .to_string_lossy(),
        "x"
    );
    assert_eq!(
        point
            .constant_pool
            .utf8(components[1].descriptor_index)
            .unwrap()
            .to_string_lossy(),
        "I"
    );
    let inner = parse(FIXTURES[1].1).unwrap();
    assert!(
        inner
            .attributes
            .iter()
            .any(|attribute| matches!(attribute.body, AttributeBody::NestHost(_)))
    );
    let outer = parse(FIXTURES[0].1).unwrap();
    assert!(outer.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::NestMembers(members) if members.len() == 1)));
    let shape = parse(FIXTURES[4].1).unwrap();
    assert!(shape.attributes.iter().any(|attribute| matches!(&attribute.body, AttributeBody::PermittedSubclasses(subclasses) if subclasses.len() == 2)));
}

#[test]
fn extracts_module_declaration_tables() {
    let module = parse(FIXTURES[7].1).unwrap();
    assert_eq!(module.access_flags, 0x8000);
    let body = module
        .attributes
        .iter()
        .find(|attribute| {
            module
                .constant_pool
                .utf8(attribute.name_index)
                .unwrap()
                .equals_ascii("Module")
        })
        .unwrap();
    let AttributeBody::Module(body) = &body.body else {
        panic!("Module must have a typed body");
    };
    assert_eq!(body.requires.len(), 2);
    assert_eq!(body.exports.len(), 1);
    assert_eq!(body.opens.len(), 1);
    assert_eq!(body.uses.len(), 1);
    assert_eq!(body.provides.len(), 1);
    assert_eq!(
        module
            .constant_pool
            .class_name(body.uses[0])
            .unwrap()
            .to_string_lossy(),
        "fixtures/Service"
    );
    assert_eq!(
        module
            .constant_pool
            .class_name(body.provides[0].provides_with_index[0])
            .unwrap()
            .to_string_lossy(),
        "fixtures/ServiceProvider"
    );
}

#[test]
fn rejects_every_truncated_fixture_prefix() {
    for (name, bytes) in FIXTURES {
        for end in 0..bytes.len() {
            assert!(
                parse(&bytes[..end]).is_err(),
                "accepted {name} truncated at {end}"
            );
        }
    }
}

#[test]
fn rejects_trailing_bytes() {
    for (name, bytes) in FIXTURES {
        let mut extended = bytes.to_vec();
        extended.push(0);
        assert!(
            parse(&extended).is_err(),
            "accepted trailing data in {name}"
        );
    }
}

#[test]
fn deterministic_mutation_corpus_never_panics() {
    // Bounded mutation smoke test. This is not a coverage-guided fuzz campaign.
    for (name, bytes) in FIXTURES {
        for position in 0..bytes.len() {
            for replacement in [0x00, 0x7f, 0xff] {
                let mut mutated = bytes.to_vec();
                mutated[position] = replacement;
                let result = std::panic::catch_unwind(|| parse(&mutated));
                assert!(result.is_ok(), "parser panicked for {name} at {position}");
            }
        }
    }
}

#[test]
fn deterministic_arbitrary_input_never_panics() {
    let mut state = 0x4352_4142_5350_4f54_u64;
    for case in 0..10_000 {
        let length = case % 2048;
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.push(state as u8);
        }
        if case % 2 == 0 && length >= 8 {
            bytes[..8].copy_from_slice(&[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65]);
        }
        assert!(
            std::panic::catch_unwind(|| parse(&bytes)).is_ok(),
            "case {case}"
        );
    }
}

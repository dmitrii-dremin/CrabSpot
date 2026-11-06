#![allow(dead_code)]
#[path = "string_body_class.rs"]
pub mod support;
pub use support::{Class, instruction};
pub const STRING: &str = "java/lang/String";
pub const LATIN: &str = "java/lang/StringLatin1";
pub const CLASS: &str = "java/lang/Class";
pub fn string_equals(c: &mut Class) {
    let string = c.class(STRING);
    let compact = c.reference(9, STRING, "COMPACT_STRINGS", "Z");
    let coder = c.reference(9, STRING, "coder", "B");
    let value = c.reference(9, STRING, "value", "[B");
    let eq = c.reference(10, LATIN, "equals", "([B[B)Z");
    let code = [
        vec![0x2a, 0x2b, 0xa6, 0, 5, 4, 0xac, 0x2b],
        instruction(0xc1, string),
        vec![0x99, 0, 43, 0x2b],
        instruction(0xc0, string),
        vec![0x4d],
        instruction(0xb2, compact),
        vec![0x99, 0, 14, 0x2a],
        instruction(0xb4, coder),
        vec![0x2c],
        instruction(0xb4, coder),
        vec![0xa0, 0, 21, 0x2a],
        instruction(0xb4, value),
        vec![0x2c],
        instruction(0xb4, value),
        instruction(0xb8, eq),
        vec![0x99, 0, 7, 4, 0xa7, 0, 4, 3, 0xac],
    ]
    .concat();
    assert_eq!(code.len(), 56);
    c.method("equals", "(Ljava/lang/Object;)Z", 1, &code, 2, 3);
    let mut frames = vec![0, 4, 7, 252, 0, 28, 7];
    frames.extend(string.to_be_bytes());
    frames.extend([250, 0, 17, 64, 1]);
    c.stackmap(&frames);
}
pub fn latin(wide: bool) -> Class {
    let mut c = Class::new(LATIN, Some("java/lang/Object"), 0x30);
    for i in 0..if wide { 300 } else { 0 } {
        c.utf(&format!("pad{i}"));
    }
    c.field("$assertionsDisabled", "Z", 0x1018);
    c.method(
        "equals",
        "([B[B)Z",
        9,
        &[
            0x2a, 0xbe, 0x2b, 0xbe, 0xa0, 0, 30, 3, 0x3d, 0x1c, 0x2a, 0xbe, 0xa2, 0, 20, 0x2a,
            0x1c, 0x33, 0x2b, 0x1c, 0x33, 0x9f, 0, 5, 3, 0xac, 0x84, 2, 1, 0xa7, 0xff, 0xec, 4,
            0xac, 3, 0xac,
        ],
        3,
        3,
    );
    c.stackmap(&[0, 4, 252, 0, 9, 1, 16, 5, 250, 0, 1]);
    let own = c.class(LATIN);
    let desired = c.reference(10, CLASS, "desiredAssertionStatus", "()Z");
    let flag = c.reference(9, LATIN, "$assertionsDisabled", "Z");
    let mut code = if let Ok(i) = u8::try_from(own) {
        vec![0x12, i]
    } else {
        instruction(0x13, own)
    };
    let shift = code.len() - 2;
    code.extend(instruction(0xb6, desired));
    code.extend([0x9a, 0, 7, 4, 0xa7, 0, 4, 3]);
    code.extend(instruction(0xb3, flag));
    code.push(0xb1);
    c.method("<clinit>", "()V", 8, &code, 1, 0);
    c.stackmap(&[0, 2, (12 + shift) as u8, 64, 1]);
    c
}
pub fn class() -> Class {
    let mut c = Class::new(CLASS, Some("java/lang/Object"), 0x31);
    c.field("classLoader", "Ljava/lang/ClassLoader;", 0x12);
    c.field("EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;", 0x1a);
    c.field(
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
        0x1a,
    );
    let loader = c.reference(9, CLASS, "classLoader", "Ljava/lang/ClassLoader;");
    let query = c.reference(10, CLASS, "desiredAssertionStatus0", "(Ljava/lang/Class;)Z");
    let lock = c.reference(
        9,
        "java/lang/ClassLoader",
        "assertionLock",
        "Ljava/lang/Object;",
    );
    let statuses = c.reference(
        9,
        "java/lang/ClassLoader",
        "classAssertionStatus",
        "Ljava/util/Map;",
    );
    let name = c.reference(10, CLASS, "getName", "()Ljava/lang/String;");
    let status = c.reference(
        10,
        "java/lang/ClassLoader",
        "desiredAssertionStatus",
        "(Ljava/lang/String;)Z",
    );
    let code = [
        vec![0x2a],
        instruction(0xb4, loader),
        vec![0x4c, 0x2b, 0xc7, 0, 8, 0x2a],
        instruction(0xb8, query),
        vec![0xac, 0x2b],
        instruction(0xb4, lock),
        vec![0x59, 0x4d, 0xc2, 0x2b],
        instruction(0xb4, statuses),
        vec![0xc6, 0, 14, 0x2b, 0x2a],
        instruction(0xb6, name),
        instruction(0xb6, status),
        vec![
            0x2c, 0xc3, 0xac, 0x2c, 0xc3, 0xa7, 0, 8, 0x4e, 0x2c, 0xc3, 0x2d, 0xbf, 0x2a,
        ],
        instruction(0xb8, query),
        vec![0xac],
    ]
    .concat();
    assert_eq!(code.len(), 54);
    c.method("desiredAssertionStatus", "()Z", 1, &code, 2, 4);
    for row in [[21, 38, 44, 0], [39, 41, 44, 0], [44, 47, 44, 0]] {
        c.handler(row);
    }
    let loader_class = c.class("java/lang/ClassLoader");
    let object = c.class("java/lang/Object");
    let throwable = c.class("java/lang/Throwable");
    let mut frames = vec![0, 4, 252, 0, 14, 7];
    frames.extend(loader_class.to_be_bytes());
    frames.extend([252, 0, 24, 7]);
    frames.extend(object.to_be_bytes());
    frames.extend([68, 7]);
    frames.extend(throwable.to_be_bytes());
    frames.push(4);
    c.stackmap(&frames);
    let register = c.reference(10, CLASS, "registerNatives", "()V");
    let own = c.class(CLASS);
    let stream = c.class("java/io/ObjectStreamField");
    let empty = c.reference(9, CLASS, "EMPTY_CLASS_ARRAY", "[Ljava/lang/Class;");
    let serial = c.reference(
        9,
        CLASS,
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
    );
    let init = [
        instruction(0xb8, register),
        vec![3],
        instruction(0xbd, own),
        instruction(0xb3, empty),
        vec![3],
        instruction(0xbd, stream),
        instruction(0xb3, serial),
        vec![0xb1],
    ]
    .concat();
    assert_eq!(init.len(), 18);
    c.method("<clinit>", "()V", 8, &init, 1, 0);
    c.method("registerNatives", "()V", 0x10a, &[], 0, 0);
    c.method(
        "desiredAssertionStatus0",
        "(Ljava/lang/Class;)Z",
        0x10a,
        &[],
        0,
        1,
    );
    c.method("getName", "()Ljava/lang/String;", 1, &[1, 0xb0], 1, 1);
    c
}
pub fn loader() -> Class {
    let mut c = Class::new("java/lang/ClassLoader", Some("java/lang/Object"), 0x421);
    c.field("assertionLock", "Ljava/lang/Object;", 0x10);
    c.field("classAssertionStatus", "Ljava/util/Map;", 0);
    c.method(
        "desiredAssertionStatus",
        "(Ljava/lang/String;)Z",
        1,
        &[3, 0xac],
        1,
        2,
    );
    c
}
pub fn object() -> Class {
    let mut c = Class::new("java/lang/Object", None, 0x21);
    c.method("<init>", "()V", 1, &[0xb1], 0, 1);
    c.method(
        "equals",
        "(Ljava/lang/Object;)Z",
        1,
        &[0x2a, 0x2b, 0xa6, 0, 7, 4, 0xa7, 0, 4, 3, 0xac],
        2,
        2,
    );
    c.stackmap(&[0, 2, 9, 64, 1]);
    c.method("getClass", "()Ljava/lang/Class;", 0x111, &[], 0, 1);
    c
}

pub const CASE: &str = "java/lang/String$CaseInsensitiveComparator";
pub const COMPARATOR: &str = "java/util/Comparator";
pub const STREAM: &str = "java/io/ObjectStreamField";
pub fn string_class(padding: usize) -> Class {
    let mut c = Class::new(STRING, Some("java/lang/Object"), 0x31);
    for i in 0..padding {
        c.utf(&format!("unused{i}"));
    }
    c.field("value", "[B", 0x12);
    c.field("coder", "B", 0x12);
    c.field("COMPACT_STRINGS", "Z", 0x18);
    c.field(
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
        0x1a,
    );
    c.field("CASE_INSENSITIVE_ORDER", "Ljava/util/Comparator;", 0x19);
    let value = c.reference(9, STRING, "value", "[B");
    let coder = c.reference(9, STRING, "coder", "B");
    let compact = c.reference(9, STRING, "COMPACT_STRINGS", "Z");
    let serial = c.reference(
        9,
        STRING,
        "serialPersistentFields",
        "[Ljava/io/ObjectStreamField;",
    );
    let comparator = c.reference(
        9,
        STRING,
        "CASE_INSENSITIVE_ORDER",
        "Ljava/util/Comparator;",
    );
    let coder_method = c.reference(10, STRING, "coder", "()B");
    let ctor = c.reference(10, CASE, "<init>", "()V");
    let case = c.class(CASE);
    let stream = c.class(STREAM);
    c.method(
        "length",
        "()I",
        1,
        &[
            vec![0x2a],
            instruction(0xb4, value),
            vec![0xbe, 0x2a],
            instruction(0xb6, coder_method),
            vec![0x7a, 0xac],
        ]
        .concat(),
        2,
        1,
    );
    c.method(
        "isEmpty",
        "()Z",
        1,
        &[
            vec![0x2a],
            instruction(0xb4, value),
            vec![0xbe, 0x9a, 0, 7, 4, 0xa7, 0, 4, 3, 0xac],
        ]
        .concat(),
        1,
        1,
    );
    c.stackmap(&[0, 2, 12, 64, 1]);
    c.method(
        "coder",
        "()B",
        0,
        &[
            instruction(0xb2, compact),
            vec![0x99, 0, 10, 0x2a],
            instruction(0xb4, coder),
            vec![0xa7, 0, 4, 4, 0xac],
        ]
        .concat(),
        1,
        1,
    );
    c.stackmap(&[0, 2, 13, 64, 1]);
    c.method(
        "<clinit>",
        "()V",
        8,
        &[
            vec![4],
            instruction(0xb3, compact),
            vec![3],
            instruction(0xbd, stream),
            instruction(0xb3, serial),
            instruction(0xbb, case),
            vec![0x59],
            instruction(0xb7, ctor),
            instruction(0xb3, comparator),
            vec![0xb1],
        ]
        .concat(),
        2,
        0,
    );
    c.method("intern", "()Ljava/lang/String;", 0x101, &[], 0, 1);
    c.method("charAt", "(I)C", 1, &[0x10, 99, 0xac], 1, 2);
    c.method("hashCode", "()I", 1, &[4, 0xac], 1, 1);
    c.nest_members(&[CASE]);
    c
}
pub fn case_class(nest: bool) -> Class {
    let mut c = Class::new(CASE, Some("java/lang/Object"), 0x20);
    c.interface(COMPARATOR);
    let object = c.reference(10, "java/lang/Object", "<init>", "()V");
    c.method(
        "<init>",
        "()V",
        2,
        &[vec![0x2a], instruction(0xb7, object), vec![0xb1]].concat(),
        1,
        1,
    );
    if nest {
        c.nest_host(STRING);
    }
    c
}
pub fn compact_platform() -> Vec<Class> {
    let mut comparator = Class::new(COMPARATOR, Some("java/lang/Object"), 0x601);
    comparator.method("defaultMarker", "()V", 1, &[0xb1], 0, 1);
    vec![
        string_class(0),
        case_class(true),
        comparator,
        Class::plain(STREAM),
    ]
}

pub fn platform(wide: bool) -> Vec<Class> {
    let mut p = compact_platform();
    string_equals(&mut p[0]);
    p.push(latin(wide));
    p.push(loader());
    p.push(Class::new("java/util/Map", Some("java/lang/Object"), 0x601));
    p
}

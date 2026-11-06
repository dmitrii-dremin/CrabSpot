#![allow(dead_code)]
//! Independent normalized recipes and explicit canonical loader graph.
#[path = "string_body_class.rs"]
pub mod support;
use crabspot_interpreter::*;
use crabspot_runtime::*;
pub use support::{Class, instruction};
pub const STRING: &str = "java/lang/String";
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
pub fn platform() -> Vec<Class> {
    let mut comparator = Class::new(COMPARATOR, Some("java/lang/Object"), 0x601);
    comparator.method("defaultMarker", "()V", 1, &[0xb1], 0, 1);
    vec![
        string_class(0),
        case_class(true),
        comparator,
        Class::plain(STREAM),
    ]
}
pub fn app() -> Class {
    Class::plain("C")
}
pub fn call(c: &mut Class, name: &str, target: &str, desc: &str) {
    let index = c.reference(10, STRING, target, desc);
    c.method(
        name,
        "(Ljava/lang/String;)I",
        9,
        &[vec![0x2a], instruction(0xb6, index), vec![0xac]].concat(),
        1,
        1,
    );
}
pub struct World {
    pub g: support::Graph,
    pub h: ManagedHeap,
    pub owner: ClassId,
    pub string: ClassId,
    pub throwable: ClassId,
    pub error: ClassId,
    pub class: ClassId,
    pub comparator: ClassId,
    pub case: ClassId,
    pub stream: ClassId,
    pub plan: Option<CompactStringBodyPlan>,
    pub discovery: StringBodyDiscoveryUsage,
}
impl World {
    pub fn new(c: Class) -> Self {
        Self::build(
            c,
            platform(),
            Default::default(),
            Default::default(),
            true,
            true,
        )
    }
    pub fn build(
        c: Class,
        platform: Vec<Class>,
        heap: HeapOptions,
        body: StringBodyLimits,
        bind: bool,
        nests: bool,
    ) -> Self {
        let mut g = support::Graph::new();
        let boot = g.boot;
        let cloneable = g.add(
            boot,
            Class::new("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
        );
        let serializable = g.add(
            boot,
            Class::new("java/io/Serializable", Some("java/lang/Object"), 0x601),
        );
        let class = g.add(
            boot,
            Class::new("java/lang/Class", Some("java/lang/Object"), 0x31),
        );
        let throwable = g.add(boot, Class::plain("java/lang/Throwable"));
        let error = g.add(
            boot,
            Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        let mut roles = std::collections::HashMap::new();
        for c in platform {
            let name = c.name.clone();
            let id = g.add(boot, c);
            roles.insert(name, id);
        }
        let string = roles[STRING];
        let case = roles[CASE];
        let comparator = roles[COMPARATOR];
        let stream = roles[STREAM];
        let app = g.loader();
        for (name, id) in [
            (STRING, string),
            ("java/lang/Class", class),
            ("java/lang/Throwable", throwable),
            ("java/lang/Error", error),
            ("java/lang/Cloneable", cloneable),
            ("java/io/Serializable", serializable),
        ] {
            g.bind(app, name, id);
        }
        let owner = g.add(app, c);
        let discovery =
            probe_compact_string_bodies(g.domain.registry(), boot, string, Default::default())
                .unwrap();
        g.enable(Default::default());
        if nests {
            g.domain.enable_nest_access().unwrap();
        }
        let ids = g.classes.clone();
        g.prepare(&ids);
        let plans = ids.iter().map(|id| g.plan(*id)).collect::<Vec<_>>();
        let mut h = ManagedHeap::new_with_instances(heap, Default::default()).unwrap();
        h.admit_instance_layouts(g.domain.registry(), &plans.iter().collect::<Vec<_>>())
            .unwrap();
        h.bind_reference_types(
            g.domain.registry(),
            ReferenceTypeUniverse {
                bootstrap_loader: boot,
                object: g.object,
                cloneable,
                serializable,
            },
        )
        .unwrap();
        g.domain
            .enable_class_mirror_sites(ClassMirrorLimits::default().max_class_sites)
            .unwrap();
        h.bind_class_mirrors(
            g.domain.registry(),
            class,
            ClassMirrorLimits::default().max_mirrors,
        )
        .unwrap();
        h.bind_strings(g.domain.registry(), string, Default::default())
            .unwrap();
        if bind {
            h.bind_compact_string_bodies(
                g.domain.registry(),
                discovery.plan.as_ref().unwrap(),
                body,
            )
            .unwrap();
        }
        Self {
            g,
            h,
            owner,
            string,
            throwable,
            error,
            class,
            comparator,
            case,
            stream,
            plan: discovery.plan,
            discovery: discovery.usage,
        }
    }
    pub fn fresh(&mut self, u: &[u16]) -> Reference {
        self.context().fresh_string(u).unwrap()
    }
    pub fn context(&mut self) -> FieldResolutionContext<'_> {
        self.g
            .domain
            .member_context_unnamed_modules(&mut self.h, Default::default(), Default::default())
            .unwrap()
    }
    pub fn with<R>(
        &mut self,
        f: impl FnOnce(&mut InitializationSession<'_, '_>, StaticStateProfile) -> R,
    ) -> R {
        let throwable = self.throwable;
        let error = self.error;
        let mut context = self.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let mut session = InitializationSession::new_with_strings(
            &mut context,
            &statics,
            &exceptions,
            error,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        f(&mut session, statics)
    }
    pub fn method(&self, name: &str) -> MethodId {
        self.g.method(self.owner, name, "(Ljava/lang/String;)I")
    }
}
pub fn run(
    s: &mut InitializationSession<'_, '_>,
    m: MethodId,
    r: Reference,
) -> Result<ExceptionOutcome, ExecutionError> {
    s.run_static(
        m,
        &[Value::Reference(r)],
        Default::default(),
        Default::default(),
        Default::default(),
    )
}
pub fn int(result: Result<ExceptionOutcome, ExecutionError>) -> i32 {
    match result.unwrap() {
        ExceptionOutcome::Returned(Some(Value::Int(i))) => i,
        other => panic!("{other:?}"),
    }
}

pub fn value_reference(w: &mut World, r: Reference) -> Reference {
    let definition = w.g.domain.registry().class(w.string).unwrap();
    let index = definition
        .constant_pool
        .entries()
        .iter()
        .enumerate()
        .find_map(|(i, e)| {
            if let crabspot_classfile::Constant::Fieldref {
                name_and_type_index,
                ..
            } = e
            {
                if let Ok(crabspot_classfile::Constant::NameAndType { name_index, .. }) =
                    definition.constant_pool.get(*name_and_type_index)
                {
                    if definition
                        .constant_pool
                        .utf8(*name_index)
                        .unwrap()
                        .equals_ascii("value")
                    {
                        return Some(i as u16);
                    }
                }
            }
            None
        })
        .unwrap();
    let string = w.string;
    match w.context().read_field(string, index, r).unwrap() {
        Value::Reference(a) => a,
        _ => panic!(),
    }
}

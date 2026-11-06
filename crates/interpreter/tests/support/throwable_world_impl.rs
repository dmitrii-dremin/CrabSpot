use crabspot_interpreter::*;
use crabspot_runtime::*;
use recipes::support::{Graph, units};
pub use recipes::{CLASS, Class, LATIN, STREAM, STRING, instruction};
use std::collections::HashMap;
pub const THROWABLE: &str = "java/lang/Throwable";
pub const ARITHMETIC: &str = "java/lang/ArithmeticException";
pub struct World {
    pub g: Graph,
    pub h: ManagedHeap,
    pub owner: ClassId,
    pub string: ClassId,
    pub class: ClassId,
    pub latin: ClassId,
    pub stream: ClassId,
    pub throwable: ClassId,
    pub error: ClassId,
    pub arithmetic: ClassId,
    pub vm_limits: VmExceptionLimits,
    pub enabled: bool,
}
impl World {
    pub fn new(c: Class) -> Self {
        Self::build(c, Config::default())
    }
    pub fn build(c: Class, config: Config) -> Self {
        let Config {
            throwable_class,
            arithmetic_class,
            extra,
            heap,
            instances,
            traces,
            vm_limits,
            enabled,
        } = config;
        let classes = recipes::platform(false);
        let class = recipes::class();
        let policy = BootstrapAssertionPolicy::Disabled;
        let mut domain = LoadingDomain::new(Default::default(), Default::default()).unwrap();
        let boot = domain.create_loader().unwrap();
        let object = domain
            .context(Default::default())
            .define(
                boot,
                &units("java/lang/Object"),
                &recipes::object().bytes(),
                Default::default(),
            )
            .unwrap();
        let mut g = Graph {
            domain,
            boot,
            object,
            classes: vec![object],
            intended: HashMap::from([((boot, units("java/lang/Object")), object)]),
        };
        let cloneable = g.add(
            boot,
            Class::new("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
        );
        let serializable = g.add(
            boot,
            Class::new("java/io/Serializable", Some("java/lang/Object"), 0x601),
        );
        let class = g.add(boot, class);
        let throwable = g.add(boot, throwable_class);
        let error = g.add(
            boot,
            Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        let mut roles = HashMap::new();
        for c in classes {
            let n = c.name.clone();
            let id = g.add(boot, c);
            roles.insert(n, id);
        }
        let string = roles[STRING];
        let latin = roles[LATIN];
        let stream = roles[STREAM];
        for c in [
            exception("java/lang/Exception", THROWABLE),
            exception("java/lang/RuntimeException", "java/lang/Exception"),
        ] {
            g.add(boot, c);
        }
        let arithmetic = g.add(boot, arithmetic_class);
        for name in [
            "java/lang/NullPointerException",
            "java/lang/AssertionError",
            "java/lang/StackTraceElement",
            "java/util/Collections",
            "java/util/Collections$EmptySet",
            "java/util/Collections$EmptyList",
            "java/util/Collections$EmptyMap",
        ] {
            let parent = if name == "java/lang/NullPointerException" {
                "java/lang/RuntimeException"
            } else if name == "java/lang/AssertionError" {
                "java/lang/Error"
            } else {
                "java/lang/Object"
            };
            g.add(
                boot,
                Class::new(
                    name,
                    Some(parent),
                    if name.starts_with("java/util/") {
                        0x20
                    } else {
                        0x21
                    },
                ),
            );
        }
        let plan =
            probe_throwable_support(g.domain.registry(), boot, throwable, Default::default())
                .unwrap()
                .plan
                .expect("synthetic Throwable ABI");
        let app = g.loader();
        for (n, id) in [
            (STRING, string),
            (CLASS, class),
            ("java/lang/Throwable", throwable),
            ("java/lang/Error", error),
            ("java/lang/Cloneable", cloneable),
            ("java/io/Serializable", serializable),
        ] {
            g.bind(app, n, id);
        }
        for name in [
            "java/lang/Exception",
            "java/lang/RuntimeException",
            ARITHMETIC,
            "java/lang/NullPointerException",
            "java/lang/AssertionError",
            "java/lang/StackTraceElement",
        ] {
            let id = g
                .classes
                .iter()
                .copied()
                .find(|id| {
                    g.domain
                        .registry()
                        .class(*id)
                        .unwrap()
                        .name()
                        .unwrap()
                        .equals_ascii(name)
                })
                .unwrap();
            g.bind(app, name, id);
        }
        for c in extra {
            g.add(app, c);
        }
        let owner = g.add(app, c);
        g.bind(boot, "C", owner);
        let body =
            probe_compact_string_bodies(g.domain.registry(), boot, string, Default::default())
                .unwrap()
                .plan
                .unwrap();
        let equality =
            probe_compact_string_equality(g.domain.registry(), &body, Default::default())
                .unwrap()
                .plan
                .unwrap();
        g.enable(Default::default());
        g.domain.enable_nest_access().unwrap();
        let ids = g
            .classes
            .iter()
            .copied()
            .filter(|id| {
                !g.domain
                    .registry()
                    .class(*id)
                    .unwrap()
                    .name()
                    .unwrap()
                    .equals_ascii("java/lang/ClassLoader")
                    && !g
                        .domain
                        .registry()
                        .class(*id)
                        .unwrap()
                        .name()
                        .unwrap()
                        .equals_ascii("java/util/Map")
            })
            .collect::<Vec<_>>();
        g.prepare(&ids);
        let plans = ids.iter().map(|id| g.plan(*id)).collect::<Vec<_>>();
        let mut h = ManagedHeap::new_with_instances(heap, instances).unwrap();
        h.admit_instance_layouts(g.domain.registry(), &plans.iter().collect::<Vec<_>>())
            .unwrap();
        h.bind_reference_types(
            g.domain.registry(),
            ReferenceTypeUniverse {
                bootstrap_loader: boot,
                object,
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
        h.bind_compact_string_bodies(g.domain.registry(), &body, Default::default())
            .unwrap();
        {
            h.bind_compact_string_equality(g.domain.registry(), &equality, policy)
                .unwrap();
        }
        h.bind_throwable_support(g.domain.registry(), &plan, traces)
            .unwrap();
        Self {
            arithmetic,
            vm_limits,
            enabled,
            g,
            h,
            owner,
            string,
            class,
            latin,
            stream,
            throwable,
            error,
        }
    }
    pub fn context(&mut self) -> FieldResolutionContext<'_> {
        self.g
            .domain
            .member_context_unnamed_modules(&mut self.h, Default::default(), Default::default())
            .unwrap()
    }
    pub fn fresh(&mut self, units: &[u16]) -> Reference {
        self.context().fresh_string(units).unwrap()
    }
    pub fn mirror(&mut self, id: ClassId) -> Reference {
        self.context()
            .class_mirror_for_type(ReferenceType::Class(id))
            .unwrap()
    }
    pub fn method(&self, name: &str, desc: &str) -> MethodId {
        self.g.method(self.owner, name, desc)
    }
    pub fn with<R>(
        &mut self,
        f: impl FnOnce(&mut InitializationSession<'_, '_>, StaticStateProfile) -> R,
    ) -> R {
        let error = self.error;
        let throwable = self.throwable;
        let vm_limits = self.vm_limits;
        let enabled = self.enabled;
        let mut context = self.context();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let mut session = if enabled {
            InitializationSession::new_with_throwables(
                &mut context,
                &statics,
                &exceptions,
                error,
                Default::default(),
                Default::default(),
                vm_limits,
            )
        } else {
            InitializationSession::new_with_strings(
                &mut context,
                &statics,
                &exceptions,
                error,
                Default::default(),
                Default::default(),
            )
        }
        .unwrap();
        f(&mut session, statics)
    }
}
pub fn app() -> Class {
    Class::plain("C")
}
pub fn run(
    s: &mut InitializationSession<'_, '_>,
    m: MethodId,
    args: &[Value],
) -> Result<ExceptionOutcome, ExecutionError> {
    s.run_static(
        m,
        args,
        Default::default(),
        Default::default(),
        Default::default(),
    )
}
pub fn int(result: Result<ExceptionOutcome, ExecutionError>) -> i32 {
    match result.unwrap() {
        ExceptionOutcome::Returned(Some(Value::Int(n))) => n,
        other => panic!("{other:?}"),
    }
}
pub fn compare(c: &mut Class) {
    let eq = c.reference(10, "java/lang/Object", "equals", "(Ljava/lang/Object;)Z");
    c.method(
        "compare",
        "(Ljava/lang/Object;Ljava/lang/Object;)Z",
        9,
        &[vec![0x2a, 0x2b], instruction(0xb6, eq), vec![0xac]].concat(),
        2,
        2,
    );
}

pub struct Config {
    pub throwable_class: Class,
    pub arithmetic_class: Class,
    pub extra: Vec<Class>,
    pub heap: HeapOptions,
    pub instances: InstanceOptions,
    pub traces: TraceLimits,
    pub vm_limits: VmExceptionLimits,
    pub enabled: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            throwable_class: throwable(),
            arithmetic_class: exception(ARITHMETIC, "java/lang/RuntimeException"),
            extra: vec![],
            heap: Default::default(),
            instances: Default::default(),
            traces: Default::default(),
            vm_limits: Default::default(),
            enabled: true,
        }
    }
}
pub fn exception(name: &str, parent: &str) -> Class {
    let mut c = Class::new(name, Some(parent), 0x21);
    for desc in ["()V", "(Ljava/lang/String;)V"] {
        let init = c.reference(10, parent, "<init>", desc);
        let mut code = vec![0x2a];
        if desc != "()V" {
            code.push(0x2b);
        }
        code.extend(instruction(0xb7, init));
        code.push(0xb1);
        c.method(
            "<init>",
            desc,
            1,
            &code,
            2,
            if desc == "()V" { 1 } else { 2 },
        );
    }
    c
}
pub fn throwable() -> Class {
    let mut c = Class::plain(THROWABLE);
    c.field("backtrace", "Ljava/lang/Object;", 0x82);
    c.field("depth", "I", 0x82);
    c.field("detailMessage", "Ljava/lang/String;", 2);
    c.field("cause", "Ljava/lang/Throwable;", 2);
    let base = c.reference(10, "java/lang/Object", "<init>", "()V");
    let native = c.reference(
        10,
        THROWABLE,
        "fillInStackTrace",
        "(I)Ljava/lang/Throwable;",
    );
    let wrapper = c.reference(10, THROWABLE, "fillInStackTrace", "()Ljava/lang/Throwable;");
    let message = c.reference(9, THROWABLE, "detailMessage", "Ljava/lang/String;");
    let cause = c.reference(9, THROWABLE, "cause", "Ljava/lang/Throwable;");
    c.method(
        "fillInStackTrace",
        "(I)Ljava/lang/Throwable;",
        0x102,
        &[],
        0,
        2,
    );
    c.method(
        "fillInStackTrace",
        "()Ljava/lang/Throwable;",
        0x21,
        &[vec![0x2a, 3], instruction(0xb7, native), vec![0xb0]].concat(),
        2,
        1,
    );
    for desc in [
        "()V",
        "(Ljava/lang/String;)V",
        "(Ljava/lang/String;Ljava/lang/Throwable;)V",
        "(Ljava/lang/String;Ljava/lang/Throwable;ZZ)V",
    ] {
        let mut code = vec![0x2a];
        code.extend(instruction(0xb7, base));
        code.extend([0x2a]);
        code.extend(instruction(0xb6, wrapper));
        code.push(0x57);
        if desc != "()V" {
            code.extend([0x2a, 0x2b]);
            code.extend(instruction(0xb5, message));
        }
        code.push(0xb1);
        c.method(
            "<init>",
            desc,
            if desc.contains("ZZ") { 4 } else { 1 },
            &code,
            2,
            if desc.contains("ZZ") {
                5
            } else if desc.contains("Throwable;") {
                3
            } else if desc == "()V" {
                1
            } else {
                2
            },
        );
    }
    c.method(
        "getMessage",
        "()Ljava/lang/String;",
        1,
        &[vec![0x2a], instruction(0xb4, message), vec![0xb0]].concat(),
        1,
        1,
    );
    c.method(
        "getCause",
        "()Ljava/lang/Throwable;",
        0x21,
        &[vec![0x2a], instruction(0xb4, cause), vec![0xb0]].concat(),
        1,
        1,
    );
    c
}

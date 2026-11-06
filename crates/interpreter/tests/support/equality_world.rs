#![allow(dead_code)]
#[path = "equality_classes.rs"]
pub mod recipes;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use recipes::support::{Graph, units};
pub use recipes::{CLASS, Class, LATIN, STREAM, STRING, instruction};
use std::collections::HashMap;
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
}
impl World {
    pub fn new(c: Class, policy: BootstrapAssertionPolicy) -> Self {
        Self::build(
            c,
            recipes::platform(false),
            recipes::class(),
            Default::default(),
            policy,
            true,
        )
    }
    pub fn build(
        c: Class,
        classes: Vec<Class>,
        class: Class,
        heap: HeapOptions,
        policy: BootstrapAssertionPolicy,
        bind: bool,
    ) -> Self {
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
        let throwable = g.add(boot, Class::plain("java/lang/Throwable"));
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
        let owner = g.add(app, c);
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
        let mut h = ManagedHeap::new_with_instances(heap, Default::default()).unwrap();
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
        if bind {
            h.bind_compact_string_equality(g.domain.registry(), &equality, policy)
                .unwrap();
        }
        Self {
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

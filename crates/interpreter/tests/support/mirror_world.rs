#![allow(dead_code)]
#[path = "../../../runtime/tests/member_support/mod.rs"]
pub mod support;
use crabspot_interpreter::*;
use crabspot_runtime::*;
use std::collections::HashMap;
pub use support::{Class, instruction, units};
#[derive(Clone, Copy)]
pub enum ObjectNative {
    Valid,
    ValidWithSync,
    Missing,
    JavaReplacement,
    Flags(u16),
}
pub struct World {
    pub g: support::Graph,
    pub h: ManagedHeap,
    pub owner: ClassId,
    pub class: ClassId,
    pub throwable: ClassId,
    pub error: ClassId,
    pub app: LoaderId,
}
impl World {
    pub fn new(c: Class) -> Self {
        Self::build(c, ObjectNative::Valid, ClassMirrorLimits::default(), true)
    }
    pub fn build(c: Class, native: ObjectNative, limits: ClassMirrorLimits, bind: bool) -> Self {
        Self::with_dependencies(c, Vec::new(), native, limits, bind)
    }
    pub fn with_dependencies(
        c: Class,
        dependencies: Vec<Class>,
        native: ObjectNative,
        limits: ClassMirrorLimits,
        bind: bool,
    ) -> Self {
        let mut domain = LoadingDomain::new(
            RegistryOptions {
                max_definitions: 20000,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap();
        let boot = domain.create_loader().unwrap();
        let mut object = Class::new("java/lang/Object", None, 0x21);
        object.method("<init>", "()V", 1, &[0xb1], 0, 1);
        match native {
            ObjectNative::Missing => {}
            ObjectNative::JavaReplacement => {
                object.method("getClass", "()Ljava/lang/Class;", 0x11, &[1, 0xb0], 1, 1)
            }
            ObjectNative::Valid | ObjectNative::ValidWithSync => {
                object.method("getClass", "()Ljava/lang/Class;", 0x111, &[], 0, 1)
            }
            ObjectNative::Flags(flags) => {
                object.method("getClass", "()Ljava/lang/Class;", flags, &[], 0, 1)
            }
        }
        if matches!(native, ObjectNative::ValidWithSync) {
            object.method("ordinarySync", "()V", 0x21, &[0xb1], 0, 1);
        }
        let object = domain
            .context(Default::default())
            .define(
                boot,
                &units("java/lang/Object"),
                &object.bytes(),
                Default::default(),
            )
            .unwrap();
        let mut g = support::Graph {
            domain,
            boot,
            object,
            classes: vec![object],
            intended: HashMap::from([((boot, units("java/lang/Object")), object)]),
        };
        let app = g.loader();
        let mut class = Class::plain("java/lang/Class");
        class.flags = 0x31;
        class.field("hidden", "I", 1);
        class.method("expose", "()I", 1, &[0x10, 99, 0xac], 1, 1);
        let class = g.add(boot, class);
        g.bind(app, "java/lang/Class", class);
        let throwable = g.add(boot, Class::plain("java/lang/Throwable"));
        let error = g.add(
            boot,
            Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
        );
        g.bind(app, "java/lang/Throwable", throwable);
        g.bind(app, "java/lang/Error", error);
        for dependency in dependencies {
            g.add(app, dependency);
        }
        let owner = g.add(app, c);
        let mut h = g.ready();
        let universe = g.bind_core(&mut h);
        g.bind(app, "java/lang/Cloneable", universe.cloneable);
        g.bind(app, "java/io/Serializable", universe.serializable);
        if bind {
            g.domain
                .enable_class_mirror_sites(limits.max_class_sites)
                .unwrap();
            h.bind_class_mirrors(g.domain.registry(), class, limits.max_mirrors)
                .unwrap();
        }
        Self {
            g,
            h,
            owner,
            class,
            throwable,
            error,
            app,
        }
    }
    pub fn object(&mut self) -> Reference {
        self.h.allocate_instance(self.owner).unwrap()
    }
    pub fn with<R>(
        &mut self,
        feature: bool,
        f: impl FnOnce(&mut InitializationSession<'_, '_>, StaticStateProfile) -> R,
    ) -> R {
        let mut context = self
            .g
            .domain
            .member_context_unnamed_modules(&mut self.h, Default::default(), Default::default())
            .unwrap();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(self.throwable).unwrap();
        let mut session = if feature {
            InitializationSession::new_with_class_mirrors(
                &mut context,
                &statics,
                &exceptions,
                self.error,
                Default::default(),
                Default::default(),
            )
        } else {
            InitializationSession::new_with_monitors(
                &mut context,
                &statics,
                &exceptions,
                self.error,
                Default::default(),
                Default::default(),
            )
        }
        .unwrap();
        f(&mut session, statics)
    }
    pub fn run(
        &mut self,
        name: &str,
        desc: &str,
        args: &[Value],
    ) -> Result<ExceptionOutcome, ExecutionError> {
        let m = self.g.method(self.owner, name, desc);
        self.with(true, |s, _| {
            s.run_static(
                m,
                args,
                Default::default(),
                Default::default(),
                Default::default(),
            )
        })
    }
}
pub fn app() -> Class {
    Class::plain("C")
}
pub fn ldc(index: u16) -> Vec<u8> {
    if let Ok(i) = u8::try_from(index) {
        vec![0x12, i]
    } else {
        instruction(0x13, index)
    }
}
pub fn returned(r: Result<ExceptionOutcome, ExecutionError>) -> Reference {
    let ExceptionOutcome::Returned(Some(Value::Reference(r))) = r.unwrap() else {
        panic!("reference result expected")
    };
    r
}

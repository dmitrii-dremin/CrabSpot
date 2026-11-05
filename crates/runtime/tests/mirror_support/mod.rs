#![allow(dead_code)]
//! Public canonical mirror service and separate domain-site transactions.
#[path = "../member_support/mod.rs"]
mod member_support;
use crabspot_classfile::ParseOptions;
use crabspot_runtime::*;
pub use member_support::*;
use std::collections::HashMap;

pub struct World {
    pub g: Graph,
    pub h: ManagedHeap,
    pub class: ClassId,
    pub caller: ClassId,
    pub sites: Vec<u16>,
    pub methods: Vec<u16>,
    pub held: u16,
    pub opaque: u16,
    pub constructors: [u16; 3],
    pub u: ReferenceTypeUniverse,
}
pub fn world(limits: ClassMirrorLimits) -> World {
    world_role(limits, 0x111)
}
pub fn world_role(limits: ClassMirrorLimits, native_flags: u16) -> World {
    let mut domain =
        LoadingDomain::new(RegistryOptions::default(), LoadingDomainLimits::default()).unwrap();
    let boot = domain.create_loader().unwrap();
    let mut object = Class::new("java/lang/Object", None, 0x21);
    object.method(
        "getClass",
        "()Ljava/lang/Class;",
        native_flags,
        &[1, 0xb0],
        1,
        1,
    );
    object.method("<init>", "()V", 1, &[0xb1], 0, 1);
    object.method("plain", "()V", 1, &[0xb1], 0, 1);
    object.field("root", "I", 1);
    let id = domain
        .context(LoadingWorkLimits::default())
        .define(
            boot,
            &units(&object.name),
            &object.bytes(),
            ParseOptions::default(),
        )
        .unwrap();
    let mut g = Graph {
        domain,
        boot,
        object: id,
        classes: vec![id],
        intended: HashMap::from([((boot, units("java/lang/Object")), id)]),
    };
    let mut class = Class::new("java/lang/Class", Some("java/lang/Object"), 0x31);
    class.field("opaque", "I", 1);
    class.method("inspect", "()I", 1, &[3, 0xac], 1, 1);
    class.method("<init>", "()V", 1, &[0xb1], 0, 1);
    let own_constructor = class.reference(10, "java/lang/Class", "<init>", "()V");
    let object_constructor = class.reference(10, "java/lang/Object", "<init>", "()V");
    let class = g.add(boot, class);
    g.add(boot, Class::plain("java/lang/Throwable"));
    g.add(
        boot,
        Class::new("java/lang/Error", Some("java/lang/Throwable"), 0x21),
    );
    for name in ["I", "V", "p/A"] {
        g.add(boot, Class::plain(name));
    }
    let mut secret = Class::plain("hidden/Secret");
    secret.flags = 0x20;
    g.add(boot, secret);
    let mut caller = Class::plain("p/Caller");
    caller.field("held", "Ljava/lang/Object;", 1);
    let sites = [
        "p/A",
        "p/A",
        "I",
        "V",
        "[I",
        "[LI;",
        "[[LI;",
        "java/lang/Class",
        "[Ljava/lang/Class;",
        "hidden/Secret",
        "[Lhidden/Secret;",
        "missing/Target",
    ]
    .into_iter()
    .map(|n| caller.class(n))
    .collect();
    let methods = [
        (10, "java/lang/Object", "getClass"),
        (10, "[I", "getClass"),
        (10, "[Ljava/lang/Object;", "getClass"),
        (11, "java/lang/Cloneable", "getClass"),
        (11, "java/io/Serializable", "getClass"),
        (11, "[I", "getClass"),
        (10, "[I", "clone"),
        (10, "[Lhidden/Secret;", "getClass"),
    ]
    .into_iter()
    .map(|(t, o, n)| caller.reference(t, o, n, "()Ljava/lang/Class;"))
    .collect();
    let held = caller.reference(9, "p/Caller", "held", "Ljava/lang/Object;");
    let opaque = caller.reference(9, "java/lang/Class", "opaque", "I");
    caller.method("<init>", "()V", 1, &[0xb1], 0, 1);
    let caller_constructor = caller.reference(10, "p/Caller", "<init>", "()V");
    let caller = g.add(boot, caller);
    let mut h = g.ready();
    let u = g.bind_core(&mut h);
    g.domain
        .enable_class_mirror_sites(limits.max_class_sites)
        .unwrap();
    h.bind_class_mirrors(g.domain.registry(), class, limits.max_mirrors)
        .unwrap();
    World {
        g,
        h,
        class,
        caller,
        sites,
        methods,
        held,
        opaque,
        constructors: [own_constructor, object_constructor, caller_constructor],
        u,
    }
}
pub fn ctx(w: &mut World) -> FieldResolutionContext<'_> {
    w.g.domain
        .member_context_unnamed_modules(
            &mut w.h,
            FieldResolutionOptions::default(),
            LoadingWorkLimits::default(),
        )
        .unwrap()
}

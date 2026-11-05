//! Independent raw class/map emitter. These are synthetic facts, never replacement JDK implementations.
use crabspot_classfile::{ParseOptions, StackMapFrameKind as K, VerificationType as V};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, HierarchyOptions, HierarchyPlan, HierarchyResolver, LoaderId,
    RegistryOptions, ResolveError, build_hierarchy,
};
fn u2(out: &mut Vec<u8>, x: u16) {
    out.extend(x.to_be_bytes());
}
fn u4(out: &mut Vec<u8>, x: u32) {
    out.extend(x.to_be_bytes());
}
#[derive(Default)]
pub struct Pool(Vec<Vec<u8>>, bool);
impl Pool {
    pub fn add(&mut self, tag: u8, payload: &[u8]) -> u16 {
        if matches!(tag, 17 | 18) {
            self.1 = true;
        }
        let mut b = vec![tag];
        b.extend(payload);
        self.0.push(b);
        let i = u16::try_from(self.0.len()).unwrap();
        if matches!(tag, 5 | 6) {
            self.0.push(vec![]);
        }
        i
    }
    pub fn units(&mut self, text: &[u16]) -> u16 {
        let mut b = Vec::new();
        for &v in text {
            match v {
                1..=127 => b.push(v as u8),
                0..=2047 => b.extend([0xc0 | (v >> 6) as u8, 0x80 | (v & 63) as u8]),
                _ => b.extend([
                    0xe0 | (v >> 12) as u8,
                    0x80 | ((v >> 6) & 63) as u8,
                    0x80 | (v & 63) as u8,
                ]),
            }
        }
        let mut data = Vec::new();
        u2(&mut data, u16::try_from(b.len()).unwrap());
        data.extend(b);
        self.add(1, &data)
    }
    pub fn text(&mut self, s: &str) -> u16 {
        self.units(&s.encode_utf16().collect::<Vec<_>>())
    }
    pub fn class(&mut self, s: &str) -> u16 {
        let i = self.text(s);
        self.add(7, &i.to_be_bytes())
    }
    pub fn member(&mut self, tag: u8, owner: &str, name: &str, desc: &str) -> u16 {
        let c = self.class(owner);
        let n = self.text(name);
        let d = self.text(desc);
        let mut nat = Vec::new();
        u2(&mut nat, n);
        u2(&mut nat, d);
        let nat = self.add(12, &nat);
        let mut data = Vec::new();
        u2(&mut data, c);
        u2(&mut data, nat);
        self.add(tag, &data)
    }
    pub fn dynamic(&mut self, tag: u8, name: &str, desc: &str) -> u16 {
        let n = self.text(name);
        let d = self.text(desc);
        let mut b = Vec::new();
        u2(&mut b, n);
        u2(&mut b, d);
        let nat = self.add(12, &b);
        let mut b = vec![0, 0];
        u2(&mut b, nat);
        self.add(tag, &b)
    }
    pub fn string(&mut self, s: &str) -> u16 {
        let i = self.text(s);
        self.add(8, &i.to_be_bytes())
    }
    pub fn method_type(&mut self, s: &str) -> u16 {
        let i = self.text(s);
        self.add(16, &i.to_be_bytes())
    }
    pub fn handle(&mut self, kind: u8, target: u16) -> u16 {
        let mut b = vec![kind];
        u2(&mut b, target);
        self.add(15, &b)
    }
}
#[derive(Clone)]
pub struct Body {
    pub bytes: Vec<u8>,
    pub stack: u16,
    pub locals: u16,
    pub handlers: Vec<[u16; 4]>,
    pub maps: Option<Vec<(u16, K)>>,
}
impl Body {
    pub fn new(bytes: &[u8], stack: u16, locals: u16) -> Self {
        Self {
            bytes: bytes.to_vec(),
            stack,
            locals,
            handlers: vec![],
            maps: None,
        }
    }
    pub fn maps(mut self, maps: Vec<(u16, K)>) -> Self {
        self.maps = Some(maps);
        self
    }
}
struct Method {
    flags: u16,
    name: u16,
    desc: u16,
    body: Option<Body>,
}
pub struct Class {
    pub name: String,
    pub pool: Pool,
    pub major: u16,
    pub minor: u16,
    pub flags: u16,
    this: u16,
    parent: u16,
    interfaces: Vec<u16>,
    fields: Vec<[u16; 3]>,
    methods: Vec<Method>,
}
impl Class {
    pub fn new(name: &str, parent: Option<&str>) -> Self {
        let mut pool = Pool::default();
        let this = pool.class(name);
        let parent = parent.map_or(0, |s| pool.class(s));
        Self {
            name: name.into(),
            pool,
            major: 65,
            minor: 0,
            flags: 0x21,
            this,
            parent,
            interfaces: vec![],
            fields: vec![],
            methods: vec![],
        }
    }
    pub fn ordinary(name: &str) -> Self {
        Self::new(name, Some("java/lang/Object"))
    }
    pub fn interface(&mut self, name: &str) {
        self.interfaces.push(self.pool.class(name));
    }
    pub fn field(&mut self, name: &str, desc: &str, flags: u16) {
        self.fields
            .push([flags, self.pool.text(name), self.pool.text(desc)]);
    }
    pub fn method(&mut self, name: &str, desc: &str, flags: u16, body: Option<Body>) -> usize {
        let i = self.methods.len();
        self.methods.push(Method {
            flags,
            name: self.pool.text(name),
            desc: self.pool.text(desc),
            body,
        });
        i
    }
    pub fn run(&mut self, desc: &str, body: Body) -> usize {
        self.method("run", desc, 9, Some(body))
    }
    pub fn bytes(mut self) -> Vec<u8> {
        let method_count = self.methods.len();
        let bootstrap = if self.pool.1 {
            let name = self.pool.text("BootstrapMethods");
            let target = self.pool.member(10, &self.name, "bootstrap", "()V");
            let handle = self.pool.handle(6, target);
            Some((name, handle))
        } else {
            None
        };
        let code_name = self.pool.text("Code");
        let maps_name = self.pool.text("StackMapTable");
        let mut methods = Vec::new();
        for m in self.methods {
            for v in [m.flags, m.name, m.desc, u16::from(m.body.is_some())] {
                u2(&mut methods, v);
            }
            if let Some(body) = m.body {
                let mut code = Vec::new();
                u2(&mut code, body.stack);
                u2(&mut code, body.locals);
                u4(&mut code, u32::try_from(body.bytes.len()).unwrap());
                code.extend(body.bytes);
                u2(&mut code, u16::try_from(body.handlers.len()).unwrap());
                for row in body.handlers {
                    for v in row {
                        u2(&mut code, v);
                    }
                }
                u2(&mut code, u16::from(body.maps.is_some()));
                if let Some(maps) = body.maps {
                    let mut map_bytes = Vec::new();
                    u2(&mut map_bytes, u16::try_from(maps.len()).unwrap());
                    let mut previous = None;
                    for (offset, kind) in maps {
                        let delta = previous.map_or(offset, |p| offset - p - 1);
                        previous = Some(offset);
                        match kind {
                            K::Same => {
                                map_bytes.push(251);
                                u2(&mut map_bytes, delta);
                            }
                            K::SameLocalsOneStack(t) => {
                                map_bytes.push(247);
                                u2(&mut map_bytes, delta);
                                verification(&mut map_bytes, t);
                            }
                            K::Chop(n) => {
                                map_bytes.push(251 - n);
                                u2(&mut map_bytes, delta);
                            }
                            K::Append(ts) => {
                                map_bytes.push(251 + u8::try_from(ts.len()).unwrap());
                                u2(&mut map_bytes, delta);
                                for t in ts {
                                    verification(&mut map_bytes, t);
                                }
                            }
                            K::Full { locals, stack } => {
                                map_bytes.push(255);
                                u2(&mut map_bytes, delta);
                                for ts in [locals, stack] {
                                    u2(&mut map_bytes, u16::try_from(ts.len()).unwrap());
                                    for t in ts {
                                        verification(&mut map_bytes, t);
                                    }
                                }
                            }
                        }
                    }
                    u2(&mut code, maps_name);
                    u4(&mut code, u32::try_from(map_bytes.len()).unwrap());
                    code.extend(map_bytes);
                }
                u2(&mut methods, code_name);
                u4(&mut methods, u32::try_from(code.len()).unwrap());
                methods.extend(code);
            }
        }
        let mut out = vec![0xca, 0xfe, 0xba, 0xbe];
        u2(&mut out, self.minor);
        u2(&mut out, self.major);
        u2(&mut out, u16::try_from(self.pool.0.len() + 1).unwrap());
        for e in self.pool.0 {
            out.extend(e);
        }
        for v in [
            self.flags,
            self.this,
            self.parent,
            u16::try_from(self.interfaces.len()).unwrap(),
        ] {
            u2(&mut out, v);
        }
        for i in self.interfaces {
            u2(&mut out, i);
        }
        u2(&mut out, u16::try_from(self.fields.len()).unwrap());
        for row in self.fields {
            for v in row {
                u2(&mut out, v);
            }
            u2(&mut out, 0);
        }
        u2(&mut out, u16::try_from(method_count).unwrap());
        out.extend(methods);
        u2(&mut out, u16::from(bootstrap.is_some()));
        if let Some((name, handle)) = bootstrap {
            u2(&mut out, name);
            u4(&mut out, 6);
            u2(&mut out, 1);
            u2(&mut out, handle);
            u2(&mut out, 0);
        }
        out
    }
}
fn verification(out: &mut Vec<u8>, t: V) {
    match t {
        V::Top => out.push(0),
        V::Integer => out.push(1),
        V::Float => out.push(2),
        V::Double => out.push(3),
        V::Long => out.push(4),
        V::Null => out.push(5),
        V::UninitializedThis => out.push(6),
        V::Object(i) => {
            out.push(7);
            u2(out, i);
        }
        V::Uninitialized(i) => {
            out.push(8);
            u2(out, i);
        }
    }
}
pub fn op(opcode: u8, index: u16) -> Vec<u8> {
    let mut b = vec![opcode];
    u2(&mut b, index);
    b
}
pub const CORE_NAMES: [&str; 8] = [
    "java/lang/Object",
    "java/lang/Cloneable",
    "java/io/Serializable",
    "java/lang/Throwable",
    "java/lang/String",
    "java/lang/Class",
    "java/lang/invoke/MethodType",
    "java/lang/invoke/MethodHandle",
];
pub struct World {
    pub registry: DefinitionRegistry,
    pub loader: LoaderId,
    pub core: [ClassId; 8],
    pub classes: Vec<ClassId>,
}
impl World {
    pub fn new() -> Self {
        Self::with_options(RegistryOptions::default())
    }
    pub fn with_options(options: RegistryOptions) -> Self {
        Self::with_core(options, [0x21, 0x601, 0x601, 0x21, 0x21, 0x21, 0x21, 0x421])
    }
    pub fn with_core(options: RegistryOptions, flags: [u16; 8]) -> Self {
        let mut registry = DefinitionRegistry::new(options).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut ids = Vec::new();
        for (i, name) in CORE_NAMES.iter().enumerate() {
            let mut c = Class::new(name, if i == 0 { None } else { Some(CORE_NAMES[0]) });
            c.flags = flags[i];
            if i == 0 {
                c.field("rootField", "I", 4);
                c.method("<init>", "()V", 1, Some(Body::new(&[0xb1], 0, 1)));
                c.method("clone", "()Ljava/lang/Object;", 0x104, None);
            }
            let bytes = c.bytes();
            ids.push(
                registry
                    .define(
                        loader,
                        &name.encode_utf16().collect::<Vec<_>>(),
                        &bytes,
                        ParseOptions {
                            allow_preview: true,
                            ..ParseOptions::default()
                        },
                    )
                    .unwrap(),
            );
        }
        Self {
            registry,
            loader,
            core: ids.try_into().unwrap(),
            classes: vec![],
        }
    }
    pub fn add(&mut self, c: Class) -> ClassId {
        self.add_in(self.loader, c)
    }
    pub fn add_in(&mut self, loader: LoaderId, c: Class) -> ClassId {
        let name = c.name.encode_utf16().collect::<Vec<_>>();
        let id = self
            .registry
            .define(
                loader,
                &name,
                &c.bytes(),
                ParseOptions {
                    allow_preview: true,
                    ..ParseOptions::default()
                },
            )
            .unwrap();
        self.classes.push(id);
        id
    }
    pub fn plans(&self) -> Vec<HierarchyPlan> {
        self.plans_from(&self.classes)
    }
    pub fn plans_from(&self, roots: &[ClassId]) -> Vec<HierarchyPlan> {
        self.core
            .iter()
            .chain(roots.iter())
            .map(|&root| {
                build_hierarchy(
                    &self.registry,
                    root,
                    &mut Delegate(self.loader),
                    HierarchyOptions {
                        max_classes: 20000,
                        max_edges: 30000,
                        ..HierarchyOptions::default()
                    },
                )
                .unwrap()
            })
            .collect()
    }
}

struct Delegate(LoaderId);
impl HierarchyResolver for Delegate {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        registry
            .lookup_class(loader, name)
            .and_then(|found| {
                if found.is_some() {
                    Ok(found)
                } else {
                    registry.lookup_class(self.0, name)
                }
            })
            .map_err(|e| ResolveError::new(e.to_string()))
    }
}

#![allow(dead_code)]
//! Independently emitted, immutable class bytes and an explicit intended loader graph.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::*;
use std::collections::HashMap;
pub fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn w(b: &mut Vec<u8>, n: u16) {
    b.extend(n.to_be_bytes());
}
fn d(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_be_bytes());
}
pub struct Class {
    pub name: String,
    pub flags: u16,
    pub major: u16,
    pool: Vec<Vec<u8>>,
    this: u16,
    parent: u16,
    interfaces: Vec<u16>,
    fields: Vec<[u16; 3]>,
    cvs: HashMap<usize, (u16, u16)>,
    methods: Vec<Method>,
    attrs: Vec<(u16, Vec<u8>)>,
}
struct Method {
    flags: u16,
    name: u16,
    desc: u16,
    code_name: u16,
    code: Vec<u8>,
    stack: u16,
    locals: u16,
    handlers: Vec<[u16; 4]>,
    attrs: Vec<(u16, Vec<u8>)>,
}
impl Class {
    pub fn new(name: &str, parent: Option<&str>, flags: u16) -> Self {
        let mut c = Self {
            name: name.into(),
            flags,
            major: 65,
            pool: vec![],
            this: 0,
            parent: 0,
            interfaces: vec![],
            fields: vec![],
            cvs: HashMap::new(),
            methods: vec![],
            attrs: vec![],
        };
        c.this = c.class(name);
        c.parent = parent.map_or(0, |s| c.class(s));
        c
    }
    pub fn plain(name: &str) -> Self {
        Self::new(name, Some("java/lang/Object"), 0x21)
    }
    pub fn cp(&mut self, tag: u8, data: &[u8]) -> u16 {
        let mut b = vec![tag];
        b.extend(data);
        self.pool.push(b);
        u16::try_from(self.pool.len()).unwrap()
    }
    pub fn utf(&mut self, s: &str) -> u16 {
        self.utf_units(&s.encode_utf16().collect::<Vec<_>>())
    }
    pub fn utf_units(&mut self, s: &[u16]) -> u16 {
        let mut raw = vec![];
        for &u in s {
            match u {
                1..=127 => raw.push(u as u8),
                0..=2047 => raw.extend([0xc0 | (u >> 6) as u8, 0x80 | (u & 63) as u8]),
                _ => raw.extend([
                    0xe0 | (u >> 12) as u8,
                    0x80 | ((u >> 6) & 63) as u8,
                    0x80 | (u & 63) as u8,
                ]),
            }
        }
        let mut b = vec![];
        w(&mut b, u16::try_from(raw.len()).unwrap());
        b.extend(raw);
        self.cp(1, &b)
    }
    pub fn class(&mut self, s: &str) -> u16 {
        let u = self.utf(s);
        self.cp(7, &u.to_be_bytes())
    }
    pub fn interface(&mut self, s: &str) {
        let i = self.class(s);
        self.interfaces.push(i);
    }
    pub fn reference(&mut self, tag: u8, owner: &str, name: &str, desc: &str) -> u16 {
        let c = self.class(owner);
        let n = self.utf(name);
        let t = self.utf(desc);
        let mut b = vec![];
        w(&mut b, n);
        w(&mut b, t);
        let nat = self.cp(12, &b);
        b.clear();
        w(&mut b, c);
        w(&mut b, nat);
        self.cp(tag, &b)
    }
    pub fn duplicate(&mut self, i: u16) -> u16 {
        let b = self.pool[usize::from(i) - 1].clone();
        self.pool.push(b);
        u16::try_from(self.pool.len()).unwrap()
    }
    pub fn field(&mut self, name: &str, desc: &str, flags: u16) {
        let n = self.utf(name);
        let t = self.utf(desc);
        self.fields.push([flags, n, t]);
    }
    pub fn string(&mut self, units: &[u16]) -> u16 {
        let u = self.utf_units(units);
        self.cp(8, &u.to_be_bytes())
    }
    pub fn constant_field(&mut self, name: &str, desc: &str, flags: u16, constant: u16) {
        let attr = self.utf("ConstantValue");
        let index = self.fields.len();
        self.field(name, desc, flags);
        self.cvs.insert(index, (attr, constant));
    }
    pub fn method(
        &mut self,
        name: &str,
        desc: &str,
        flags: u16,
        code: &[u8],
        stack: u16,
        locals: u16,
    ) {
        let n = self.utf(name);
        let t = self.utf(desc);
        let cn = self.utf("Code");
        self.methods.push(Method {
            flags,
            name: n,
            desc: t,
            code_name: cn,
            code: code.to_vec(),
            stack,
            locals,
            handlers: vec![],
            attrs: vec![],
        });
    }
    pub fn nest_host(&mut self, host: &str) {
        let index = self.class(host);
        let name = self.utf("NestHost");
        self.attrs.push((name, index.to_be_bytes().to_vec()));
    }
    pub fn nest_members(&mut self, members: &[&str]) {
        let mut bytes = Vec::new();
        w(&mut bytes, members.len() as u16);
        for name in members {
            let index = self.class(name);
            w(&mut bytes, index);
        }
        let name = self.utf("NestMembers");
        self.attrs.push((name, bytes));
    }
    pub fn long_nest_members(&mut self, count: usize, tail: &[&str]) {
        let unrelated = self.class("absent/Unrelated");
        let mut bytes = Vec::new();
        w(&mut bytes, (count + tail.len()) as u16);
        for _ in 0..count {
            w(&mut bytes, unrelated);
        }
        for name in tail {
            let index = self.class(name);
            w(&mut bytes, index);
        }
        let name = self.utf("NestMembers");
        self.attrs.push((name, bytes));
    }
    pub fn stackmap(&mut self, frames: &[u8]) {
        let name = self.utf("StackMapTable");
        self.methods
            .last_mut()
            .unwrap()
            .attrs
            .push((name, frames.to_vec()));
    }
    pub fn code_byte(&mut self, name: &str, index: usize, value: u8) {
        let m = self
            .methods
            .iter_mut()
            .find(|m| self.pool[usize::from(m.name) - 1].ends_with(name.as_bytes()))
            .unwrap();
        m.code[index] = value;
    }
    pub fn method_flags(&mut self, name: &str, flags: u16) {
        let m = self
            .methods
            .iter_mut()
            .find(|m| self.pool[usize::from(m.name) - 1].ends_with(name.as_bytes()))
            .unwrap();
        m.flags = flags;
    }
    pub fn field_flags(&mut self, name: &str, flags: u16) {
        let m = self
            .fields
            .iter_mut()
            .find(|m| self.pool[usize::from(m[1]) - 1].ends_with(name.as_bytes()))
            .unwrap();
        m[0] = flags;
    }
    pub fn maximal_native_table(&mut self) {
        let names = (0..256)
            .map(|i| self.utf(&format!("m{i:03}")))
            .collect::<Vec<_>>();
        let descs = (0..256)
            .map(|i| self.utf(&format!("(){}B", "[".repeat(i))))
            .collect::<Vec<_>>();
        for i in 0..65_535 {
            self.methods.push(Method {
                flags: 0x109,
                name: names[i / 256],
                desc: descs[i % 256],
                code_name: 0,
                code: vec![],
                stack: 0,
                locals: 0,
                handlers: vec![],
                attrs: vec![],
            });
        }
    }
    pub fn handler(&mut self, row: [u16; 4]) {
        self.methods.last_mut().unwrap().handlers.push(row);
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut b = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0];
        w(&mut b, self.major);
        w(&mut b, u16::try_from(self.pool.len() + 1).unwrap());
        for e in &self.pool {
            b.extend(e);
        }
        for n in [
            self.flags,
            self.this,
            self.parent,
            self.interfaces.len() as u16,
        ] {
            w(&mut b, n);
        }
        for i in &self.interfaces {
            w(&mut b, *i);
        }
        w(&mut b, self.fields.len() as u16);
        for (index, f) in self.fields.iter().enumerate() {
            for n in f {
                w(&mut b, *n);
            }
            if let Some((attr, constant)) = self.cvs.get(&index) {
                w(&mut b, 1);
                w(&mut b, *attr);
                d(&mut b, 2);
                w(&mut b, *constant);
            } else {
                w(&mut b, 0);
            }
        }
        w(&mut b, self.methods.len() as u16);
        for m in &self.methods {
            let body =
                m.flags & 0x500 == 0 || self.pool[usize::from(m.name) - 1].ends_with(b"<clinit>");
            for n in [m.flags, m.name, m.desc, u16::from(body)] {
                w(&mut b, n);
            }
            if body {
                w(&mut b, m.code_name);
                d(
                    &mut b,
                    12 + m.code.len() as u32
                        + 8 * m.handlers.len() as u32
                        + m.attrs.iter().map(|(_, v)| 6 + v.len() as u32).sum::<u32>(),
                );
                w(&mut b, m.stack);
                w(&mut b, m.locals);
                d(&mut b, m.code.len() as u32);
                b.extend(&m.code);
                w(&mut b, m.handlers.len() as u16);
                for r in &m.handlers {
                    for n in r {
                        w(&mut b, *n);
                    }
                }
                w(&mut b, m.attrs.len() as u16);
                for (name, data) in &m.attrs {
                    w(&mut b, *name);
                    d(&mut b, data.len() as u32);
                    b.extend(data);
                }
            }
        }
        w(&mut b, self.attrs.len() as u16);
        for (name, data) in &self.attrs {
            w(&mut b, *name);
            d(&mut b, data.len() as u32);
            b.extend(data);
        }
        b
    }
}
pub fn instruction(op: u8, i: u16) -> Vec<u8> {
    vec![op, (i >> 8) as u8, i as u8]
}
pub struct Graph {
    pub domain: LoadingDomain,
    pub boot: LoaderId,
    pub object: ClassId,
    pub classes: Vec<ClassId>,
    pub intended: HashMap<(LoaderId, Vec<u16>), ClassId>,
}
impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}
impl Graph {
    pub fn new() -> Self {
        let mut domain = LoadingDomain::new(
            RegistryOptions {
                max_definitions: 20000,
                ..Default::default()
            },
            LoadingDomainLimits::default(),
        )
        .unwrap();
        let boot = domain.create_loader().unwrap();
        let mut c = Class::new("java/lang/Object", None, 0x21);
        c.method("<init>", "()V", 1, &[0xb1], 0, 1);
        let object = domain
            .context(LoadingWorkLimits::default())
            .define(boot, &units(&c.name), &c.bytes(), ParseOptions::default())
            .unwrap();
        Self {
            domain,
            boot,
            object,
            classes: vec![object],
            intended: HashMap::from([((boot, units("java/lang/Object")), object)]),
        }
    }
    pub fn loader(&mut self) -> LoaderId {
        let l = self.domain.create_loader().unwrap();
        self.bind(l, "java/lang/Object", self.object);
        l
    }
    pub fn add(&mut self, l: LoaderId, c: Class) -> ClassId {
        let id = self
            .domain
            .context(LoadingWorkLimits::default())
            .define(l, &units(&c.name), &c.bytes(), ParseOptions::default())
            .unwrap();
        self.intended.insert((l, units(&c.name)), id);
        self.classes.push(id);
        id
    }
    pub fn bind(&mut self, l: LoaderId, name: &str, c: ClassId) {
        self.domain
            .context(LoadingWorkLimits::default())
            .record_initiation(l, &units(name), c)
            .unwrap();
        self.intended.insert((l, units(name)), c);
    }
    pub fn plan(&self, c: ClassId) -> HierarchyPlan {
        build_hierarchy(
            self.domain.registry(),
            c,
            &mut Resolver(&self.intended),
            HierarchyOptions {
                max_classes: 20000,
                max_edges: 40000,
                ..Default::default()
            },
        )
        .unwrap()
    }
    pub fn enable(&mut self, limits: MemberResolutionLimits) {
        self.domain.enable_member_resolution(limits).unwrap();
    }
    pub fn prepare(&mut self, ids: &[ClassId]) {
        for id in ids {
            let p = self.plan(*id);
            self.domain
                .context(LoadingWorkLimits::default())
                .prepare_member_owner(&p)
                .unwrap();
        }
    }
    pub fn ready(&mut self) -> ManagedHeap {
        self.enable(MemberResolutionLimits::default());
        let ids = self.classes.clone();
        self.prepare(&ids);
        self.heap(&ids)
    }
    pub fn heap(&self, ids: &[ClassId]) -> ManagedHeap {
        let plans: Vec<_> = ids.iter().map(|c| self.plan(*c)).collect();
        let mut h = ManagedHeap::new_with_instances(
            HeapOptions::default(),
            InstanceOptions {
                max_layout_classes: 20000,
                max_layout_edges: 40000,
                ..Default::default()
            },
        )
        .unwrap();
        h.admit_instance_layouts(self.domain.registry(), &plans.iter().collect::<Vec<_>>())
            .unwrap();
        h
    }
    pub fn method(&self, c: ClassId, n: &str, t: &str) -> MethodId {
        self.domain
            .registry()
            .find_declared_method(c, &units(n), &units(t))
            .unwrap()
            .unwrap()
    }
    pub fn field(&self, c: ClassId, n: &str, t: &str) -> FieldId {
        self.domain
            .registry()
            .find_declared_field(c, &units(n), &units(t))
            .unwrap()
            .unwrap()
    }
    pub fn bind_core(&mut self, h: &mut ManagedHeap) -> ReferenceTypeUniverse {
        let cloneable = self.add(
            self.boot,
            Class::new("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
        );
        let serializable = self.add(
            self.boot,
            Class::new("java/io/Serializable", Some("java/lang/Object"), 0x601),
        );
        self.prepare(&[cloneable, serializable]);
        let p = [self.plan(cloneable), self.plan(serializable)];
        h.admit_instance_layouts(self.domain.registry(), &p.iter().collect::<Vec<_>>())
            .unwrap();
        let u = ReferenceTypeUniverse {
            bootstrap_loader: self.boot,
            object: self.object,
            cloneable,
            serializable,
        };
        h.bind_reference_types(self.domain.registry(), u).unwrap();
        u
    }
}
pub struct Resolver<'a>(pub &'a HashMap<(LoaderId, Vec<u16>), ClassId>);
impl HierarchyResolver for Resolver<'_> {
    fn resolve(
        &mut self,
        _: &DefinitionRegistry,
        l: LoaderId,
        n: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        Ok(self.0.get(&(l, n.to_vec())).copied())
    }
}

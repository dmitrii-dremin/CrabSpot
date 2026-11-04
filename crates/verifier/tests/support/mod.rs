//! Original independent classfile emitter. No verifier internals or JVM calls.
use crabspot_classfile::{AttributeBody, ClassFile, Code, Member, parse};
fn u2(bytes: &mut Vec<u8>, v: u16) {
    bytes.extend(v.to_be_bytes());
}
fn u4(bytes: &mut Vec<u8>, v: u32) {
    bytes.extend(v.to_be_bytes());
}
#[derive(Default)]
pub struct Pool {
    entries: Vec<Vec<u8>>,
    pub dynamic: bool,
}
impl Pool {
    pub fn add(&mut self, tag: u8, data: &[u8]) -> u16 {
        let mut entry = vec![tag];
        entry.extend(data);
        self.entries.push(entry);
        let index = u16::try_from(self.entries.len()).unwrap();
        if matches!(tag, 5 | 6) {
            self.entries.push(Vec::new());
        }
        index
    }
    pub fn text(&mut self, text: &str) -> u16 {
        self.units(&text.encode_utf16().collect::<Vec<_>>())
    }
    pub fn units(&mut self, text: &[u16]) -> u16 {
        let mut payload = Vec::new();
        for &v in text {
            match v {
                1..=0x7f => payload.push(v as u8),
                0..=0x7ff => payload.extend([0xc0 | (v >> 6) as u8, 0x80 | (v & 63) as u8]),
                _ => payload.extend([
                    0xe0 | (v >> 12) as u8,
                    0x80 | ((v >> 6) & 63) as u8,
                    0x80 | (v & 63) as u8,
                ]),
            }
        }
        let mut data = Vec::new();
        u2(&mut data, u16::try_from(payload.len()).unwrap());
        data.extend(payload);
        self.add(1, &data)
    }
    pub fn class(&mut self, name: &str) -> u16 {
        let name = self.text(name);
        self.class_index(name)
    }
    pub fn class_index(&mut self, name: u16) -> u16 {
        self.add(7, &name.to_be_bytes())
    }
    pub fn nat(&mut self, name: &str, desc: &str) -> u16 {
        let name = self.text(name);
        let desc = self.text(desc);
        self.nat_indices(name, desc)
    }
    pub fn nat_indices(&mut self, name: u16, desc: u16) -> u16 {
        let mut data = Vec::new();
        u2(&mut data, name);
        u2(&mut data, desc);
        self.add(12, &data)
    }
    pub fn member(&mut self, tag: u8, owner: &str, name: &str, desc: &str) -> u16 {
        let owner = self.class(owner);
        let nat = self.nat(name, desc);
        self.member_indices(tag, owner, nat)
    }
    pub fn member_indices(&mut self, tag: u8, owner: u16, nat: u16) -> u16 {
        let mut data = Vec::new();
        u2(&mut data, owner);
        u2(&mut data, nat);
        self.add(tag, &data)
    }
    pub fn string(&mut self, text: &[u16]) -> u16 {
        let index = self.units(text);
        self.add(8, &index.to_be_bytes())
    }
    pub fn handle(&mut self, kind: u8, target: u16) -> u16 {
        let mut data = vec![kind];
        u2(&mut data, target);
        self.add(15, &data)
    }
    pub fn method_type(&mut self, desc: &str) -> u16 {
        let desc = self.text(desc);
        self.add(16, &desc.to_be_bytes())
    }
    pub fn dynamic(&mut self, tag: u8, name: &str, desc: &str) -> u16 {
        self.dynamic = true;
        let nat = self.nat(name, desc);
        let mut data = vec![0, 0];
        u2(&mut data, nat);
        self.add(tag, &data)
    }
}
pub struct Builder {
    pub pool: Pool,
    pub name: u16,
    pub descriptor: u16,
    pub code_name: u16,
    pub flags: u16,
    pub code: Option<Vec<u8>>,
    pub stack: u16,
    pub locals: u16,
    this: u16,
    parent: u16,
}
impl Builder {
    pub fn new(desc: &str, code: &[u8], stack: u16, locals: u16) -> Self {
        let mut pool = Pool::default();
        let this = pool.class("probe/Synthetic");
        let parent = pool.class("java/lang/Object");
        let name = pool.text("run");
        let descriptor = pool.text(desc);
        let code_name = pool.text("Code");
        Self {
            pool,
            name,
            descriptor,
            code_name,
            flags: 9,
            code: Some(code.to_vec()),
            stack,
            locals,
            this,
            parent,
        }
    }
    pub fn finish(mut self) -> ClassFile {
        let bootstrap = if self.pool.dynamic {
            let attr = self.pool.text("BootstrapMethods");
            let target = self.pool.member(10, "probe/Synthetic", "bootstrap", "()V");
            let handle = self.pool.handle(6, target);
            Some((attr, handle))
        } else {
            None
        };
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        u2(
            &mut bytes,
            u16::try_from(self.pool.entries.len() + 1).unwrap(),
        );
        for e in self.pool.entries {
            bytes.extend(e);
        }
        for word in [
            0x21,
            self.this,
            self.parent,
            0,
            0,
            1,
            self.flags,
            self.name,
            self.descriptor,
            u16::from(self.code.is_some()),
        ] {
            u2(&mut bytes, word);
        }
        if let Some(code) = self.code {
            u2(&mut bytes, self.code_name);
            u4(&mut bytes, 12 + u32::try_from(code.len()).unwrap());
            u2(&mut bytes, self.stack);
            u2(&mut bytes, self.locals);
            u4(&mut bytes, u32::try_from(code.len()).unwrap());
            bytes.extend(code);
            u2(&mut bytes, 0);
            u2(&mut bytes, 0);
        }
        u2(&mut bytes, u16::from(bootstrap.is_some()));
        if let Some((name, handle)) = bootstrap {
            u2(&mut bytes, name);
            u4(&mut bytes, 6);
            u2(&mut bytes, 1);
            u2(&mut bytes, handle);
            u2(&mut bytes, 0);
        }
        parse(&bytes).unwrap()
    }
}
pub fn simple() -> ClassFile {
    Builder::new("()V", &[0xb1], 0, 0).finish()
}
pub fn code(class: &ClassFile) -> &Code {
    class.methods[0]
        .attributes
        .iter()
        .find_map(|a| {
            if let AttributeBody::Code(c) = &a.body {
                Some(c)
            } else {
                None
            }
        })
        .unwrap()
}
pub fn code_mut(class: &mut ClassFile) -> &mut Code {
    class.methods[0]
        .attributes
        .iter_mut()
        .find_map(|a| {
            if let AttributeBody::Code(c) = &mut a.body {
                Some(c)
            } else {
                None
            }
        })
        .unwrap()
}
pub fn member(class: &ClassFile) -> &Member {
    &class.methods[0]
}
pub fn op(op: u8, index: u16) -> Vec<u8> {
    let mut code = vec![op];
    code.extend(index.to_be_bytes());
    code
}

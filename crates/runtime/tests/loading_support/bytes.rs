//! Separately authored admission fixtures. Bodies are never executed or credited as verified code.
fn word(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_be_bytes());
}
fn dword(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_be_bytes());
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn item(&mut self, tag: u8, payload: &[u8]) -> u16 {
        let mut item = vec![tag];
        item.extend(payload);
        self.0.push(item);
        u16::try_from(self.0.len()).unwrap()
    }
    fn text(&mut self, units: &[u16]) -> u16 {
        let mut raw = Vec::new();
        for &u in units {
            match u {
                1..=0x7f => raw.push(u as u8),
                0..=0x7ff => raw.extend([0xc0 | (u >> 6) as u8, 0x80 | (u & 63) as u8]),
                _ => raw.extend([
                    0xe0 | (u >> 12) as u8,
                    0x80 | ((u >> 6) & 63) as u8,
                    0x80 | (u & 63) as u8,
                ]),
            }
        }
        let mut data = Vec::new();
        word(&mut data, u16::try_from(raw.len()).unwrap());
        data.extend(raw);
        self.item(1, &data)
    }
    fn class(&mut self, name: &[u16]) -> u16 {
        let utf = self.text(name);
        self.item(7, &utf.to_be_bytes())
    }
}
pub fn name(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
pub struct Class {
    pub name: Vec<u16>,
    pub parent: Option<Vec<u16>>,
    pub interfaces: Vec<Vec<u16>>,
    pub flags: u16,
    pub methods: Vec<Method>,
}
pub struct Method {
    pub name: String,
    pub descriptor: String,
    pub flags: u16,
}
impl Class {
    pub fn new(s: &str) -> Self {
        Self {
            name: name(s),
            parent: Some(name("java/lang/Object")),
            interfaces: vec![],
            flags: 0x21,
            methods: vec![],
        }
    }
    pub fn root() -> Self {
        Self {
            name: name("java/lang/Object"),
            parent: None,
            interfaces: vec![],
            flags: 0x21,
            methods: vec![],
        }
    }
    pub fn parent(mut self, s: &str) -> Self {
        self.parent = Some(name(s));
        self
    }
    pub fn interface(mut self) -> Self {
        self.flags = 0x601;
        self
    }
    pub fn implements(mut self, s: &str) -> Self {
        self.interfaces.push(name(s));
        self
    }
    pub fn method(mut self, s: &str, descriptor: &str, flags: u16) -> Self {
        self.methods.push(Method {
            name: s.into(),
            descriptor: descriptor.into(),
            flags,
        });
        self
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut cp = Pool::default();
        let this = cp.class(&self.name);
        let parent = self.parent.as_ref().map_or(0, |p| cp.class(p));
        let interfaces = self
            .interfaces
            .iter()
            .map(|s| cp.class(s))
            .collect::<Vec<_>>();
        let code_name = cp.text(&name("Code"));
        let mut methods = Vec::new();
        for method in &self.methods {
            let n = cp.text(&name(&method.name));
            let d = cp.text(&name(&method.descriptor));
            for v in [method.flags, n, d, u16::from(method.flags & 0x500 == 0)] {
                word(&mut methods, v);
            }
            if method.flags & 0x500 == 0 {
                let (desc_slots, result) = shape(&method.descriptor);
                let bytes = match result {
                    b'V' => vec![0xb1],
                    b'J' => vec![9, 0xad],
                    b'D' => vec![0x0e, 0xaf],
                    b'F' => vec![0x0b, 0xae],
                    b'L' | b'[' => vec![1, 0xb0],
                    _ => vec![3, 0xac],
                };
                word(&mut methods, code_name);
                dword(&mut methods, 12 + u32::try_from(bytes.len()).unwrap());
                word(&mut methods, 2);
                word(&mut methods, desc_slots + u16::from(method.flags & 8 == 0));
                dword(&mut methods, u32::try_from(bytes.len()).unwrap());
                methods.extend(bytes);
                word(&mut methods, 0);
                word(&mut methods, 0);
            }
        }
        let mut out = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        word(&mut out, u16::try_from(cp.0.len() + 1).unwrap());
        for item in cp.0 {
            out.extend(item);
        }
        for v in [
            self.flags,
            this,
            parent,
            u16::try_from(interfaces.len()).unwrap(),
        ] {
            word(&mut out, v);
        }
        for i in interfaces {
            word(&mut out, i);
        }
        word(&mut out, 0);
        word(&mut out, u16::try_from(self.methods.len()).unwrap());
        out.extend(methods);
        word(&mut out, 0);
        out
    }
}
fn shape(descriptor: &str) -> (u16, u8) {
    let bytes = descriptor.as_bytes();
    let mut at = 1;
    let mut slots = 0;
    while bytes[at] != b')' {
        let mut array = false;
        while bytes[at] == b'[' {
            array = true;
            at += 1;
        }
        let leaf = bytes[at];
        slots += if !array && matches!(leaf, b'J' | b'D') {
            2
        } else {
            1
        };
        if leaf == b'L' {
            while bytes[at] != b';' {
                at += 1;
            }
        }
        at += 1;
    }
    (slots, bytes[at + 1])
}

use crate::{Attribute, ConstantPool, JavaString, ParseError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClassVersion {
    pub major: u16,
    pub minor: u16,
}

impl ClassVersion {
    pub fn is_preview(self) -> bool {
        self.major >= 56 && self.minor == u16::MAX
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassFile {
    pub version: ClassVersion,
    pub constant_pool: ConstantPool,
    pub access_flags: u16,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<Member>,
    pub methods: Vec<Member>,
    pub attributes: Vec<Attribute>,
}

impl ClassFile {
    pub fn name(&self) -> Result<&JavaString, ParseError> {
        self.constant_pool.class_name(self.this_class)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub offset: usize,
    pub access_flags: u16,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<Attribute>,
}

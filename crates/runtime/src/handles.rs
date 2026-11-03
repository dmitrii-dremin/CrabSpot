/// An opaque defining-loader identity, valid only in its originating registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct LoaderId {
    pub(crate) registry: u64,
    pub(crate) index: usize,
}

/// An opaque unverified class-definition identity, scoped to one registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ClassId {
    pub(crate) registry: u64,
    pub(crate) index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct MethodId {
    pub(crate) owner: ClassId,
    pub(crate) index: usize,
}
impl MethodId {
    pub fn owner(self) -> ClassId {
        self.owner
    }
    /// Read-only declaration index; metadata access still validates this handle.
    pub fn index(self) -> usize {
        self.index
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct FieldId {
    pub(crate) owner: ClassId,
    pub(crate) index: usize,
}
impl FieldId {
    pub fn owner(self) -> ClassId {
        self.owner
    }
}

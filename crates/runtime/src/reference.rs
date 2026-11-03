/// Opaque managed identity: an owning-heap namespace, entry index and generation.
/// This is not a host address. ManagedHeap validates ownership, entry bounds and
/// generation before dereferencing a handle; callers cannot construct one.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct HeapRef {
    heap: u64,
    index: usize,
    generation: u64,
}
impl HeapRef {
    pub(crate) const fn components(self) -> (u64, usize, u64) {
        (self.heap, self.index, self.generation)
    }
}

/// Java null or an opaque managed identity issued by ManagedHeap.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Reference(Option<HeapRef>);
impl Reference {
    pub const fn null() -> Self {
        Self(None)
    }
    pub const fn is_null(self) -> bool {
        self.0.is_none()
    }
    pub const fn heap_ref(self) -> Option<HeapRef> {
        self.0
    }
    /// Trusted heap publication boundary; never exposed as a public issuer.
    pub(crate) const fn issued(heap: u64, index: usize, generation: u64) -> Self {
        Self(Some(HeapRef {
            heap,
            index,
            generation,
        }))
    }
}

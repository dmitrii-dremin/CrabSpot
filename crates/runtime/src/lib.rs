//! Runtime identities, hierarchy/layout plans, shared values, default storage and
//! a bounded managed arena for primitive arrays and ordinary instances.
//!
//! Admission and structural planning leave definitions unverified and unlinked.
//! This crate does not execute code, perform complete Java linkage, initialize
//! classes, or open files.
//! Default static allocation does not perform full preparation or initialization.
mod error;
mod field;
mod handles;
pub mod heap;
mod hierarchy;
pub mod instance;
mod reference;
mod registry;
mod source;
mod storage;
mod values;

pub use error::{HandleKind, RegistryError, RegistryResource};
pub use field::{
    FieldAccessError, FieldJavaError, FieldResolutionContext, FieldResolutionOptions, FieldResource,
};
pub use handles::{ClassId, FieldId, LoaderId, MethodId};
pub use heap::{
    HeapEntryKind, HeapError, HeapOptions, HeapResource, ManagedHeap, PrimitiveArrayKind,
};
pub use hierarchy::{
    ClassLayout, HierarchyError, HierarchyOptions, HierarchyPlan, HierarchyResolver,
    HierarchyResource, ResolveError, SameLoaderResolver, build_hierarchy,
};
pub use reference::{HeapRef, Reference};
pub use registry::{DefinitionRegistry, RegistryOptions};
pub use source::{DefinitionSource, InMemorySource, SourceError};
pub use storage::{StaticStorage, StaticStorageOptions, StorageError};
pub use values::{Value, ValueKind};

pub use instance::{InstanceClassKind, InstanceError, InstanceOptions, InstanceResource};

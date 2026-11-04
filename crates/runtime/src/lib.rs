//! Runtime identities, hierarchy/layout plans, shared values, default storage and
//! a bounded managed arena for primitive arrays and ordinary instances.
//!
//! Admission and structural planning leave definitions unverified and unlinked.
//! This crate does not execute code, perform complete Java linkage, initialize
//! classes, or open files.
//! Default static allocation does not perform full preparation or initialization.
mod error;
mod exception;
mod field;
mod handles;
pub mod heap;
mod hierarchy;
pub mod instance;
mod method;
mod method_values;
mod reference;
mod reference_array;
mod registry;
mod source;
mod storage;
mod types;
mod values;

pub use error::{HandleKind, RegistryError, RegistryResource};
pub use exception::{ExceptionAccessError, ExceptionProfile, ThrownException, VmExceptionKind};
pub use field::{
    FieldAccessError, FieldJavaError, FieldResolutionContext, FieldResolutionOptions,
    FieldResource, ReferenceFieldMismatch,
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
pub use reference_array::{ArrayAccessError, ArrayJavaError, REFERENCE_ARRAY_ELEMENT_BYTES};
pub use registry::{DefinitionRegistry, RegistryOptions};
pub use source::{DefinitionSource, InMemorySource, SourceError};
pub use storage::{StaticStorage, StaticStorageOptions, StorageError};
pub use values::{Value, ValueKind};

pub use instance::{InstanceClassKind, InstanceError, InstanceOptions, InstanceResource};
pub use method::{MethodAccessError, MethodJavaError};
pub use method_values::{
    InstanceInvocationKind, MethodReferenceMismatch, MethodReturnKind, MethodValuePosition,
    MethodValueShape,
};
pub use types::{
    ArrayElement, ReferenceType, ReferenceTypeUniverse, TypeAccessError, TypeJavaError,
};

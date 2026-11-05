//! Validation of explicitly supplied Throwable instances and lazy typed catches.
//! Tokens certify an unchanged admitted snapshot, never Java construction state.
use crate::{
    ClassId, FieldAccessError, FieldResolutionContext, HeapError, InstanceError, Reference,
    ReferenceType, ReferenceTypeUniverse, RegistryError, TypeAccessError, TypeJavaError,
};
use crabspot_classfile::Constant;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    registry: u64,
    heap: u64,
    layouts: usize,
    universe: ReferenceTypeUniverse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExceptionProfile {
    throwable: ClassId,
    snapshot: Snapshot,
}
impl ExceptionProfile {
    pub fn throwable(self) -> ClassId {
        self.throwable
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThrownException {
    profile: ExceptionProfile,
    reference: Reference,
    class: ClassId,
}
impl ThrownException {
    pub fn reference(self) -> Reference {
        self.reference
    }
    pub fn class(self) -> ClassId {
        self.class
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmExceptionKind {
    IllegalMonitorStateException,
    LinkageError,
    ArithmeticException,
    ClassCastException,
    ArrayStoreException,
    NullPointerException,
    ArrayIndexOutOfBoundsException,
    NegativeArraySizeException,
    NoSuchFieldError,
    IllegalAccessError,
    IncompatibleClassChangeError,
    NoSuchMethodError,
    AbstractMethodError,
    InstantiationError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExceptionAccessError {
    Context(FieldAccessError),
    Type(Box<TypeAccessError>),
    InvalidProfile {
        throwable: ClassId,
        reason: &'static str,
    },
    ProfileMismatch,
    NullThrowable,
    InvalidThrownType {
        expected: ClassId,
        actual: ClassId,
    },
    InvalidCatchConstant {
        origin: ClassId,
        index: u16,
    },
    InvalidCatchType {
        origin: ClassId,
        index: u16,
        class: ClassId,
    },
    InvalidThrownToken,
    Java(VmExceptionKind),
}
impl fmt::Display for ExceptionAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(error) => write!(f, "exception context failed: {error}"),
            Self::Type(error) => write!(f, "exception type validation failed: {error}"),
            Self::InvalidProfile { throwable, reason } => {
                write!(f, "invalid Throwable anchor {throwable:?}: {reason}")
            }
            Self::ProfileMismatch => f.write_str(
                "exception profile belongs to a different registry, heap or admitted snapshot",
            ),
            Self::NullThrowable => f.write_str("null is not a supplied Throwable instance"),
            Self::InvalidThrownType { expected, actual } => write!(
                f,
                "thrown class {actual:?} does not extend Throwable {expected:?}"
            ),
            Self::InvalidCatchConstant { origin, index } => write!(
                f,
                "constant #{index} in {origin:?} is not an ordinary catch-class reference"
            ),
            Self::InvalidCatchType {
                origin,
                index,
                class,
            } => write!(
                f,
                "catch constant #{index} in {origin:?} resolves to non-Throwable class/interface {class:?}"
            ),
            Self::InvalidThrownToken => f.write_str(
                "thrown token's profile or retained class differs from its checked instance",
            ),
            Self::Java(kind) => write!(
                f,
                "exception service requires VM exception production: {kind:?}"
            ),
        }
    }
}
impl std::error::Error for ExceptionAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(error) => Some(error),
            Self::Type(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}
impl From<FieldAccessError> for ExceptionAccessError {
    fn from(error: FieldAccessError) -> Self {
        Self::Context(error)
    }
}
impl From<RegistryError> for ExceptionAccessError {
    fn from(error: RegistryError) -> Self {
        Self::Context(FieldAccessError::Registry(error))
    }
}
impl From<InstanceError> for ExceptionAccessError {
    fn from(error: InstanceError) -> Self {
        Self::Context(FieldAccessError::Instance(error))
    }
}
impl From<HeapError> for ExceptionAccessError {
    fn from(error: HeapError) -> Self {
        Self::from(InstanceError::Heap(error))
    }
}
impl From<TypeAccessError> for ExceptionAccessError {
    fn from(error: TypeAccessError) -> Self {
        Self::Type(Box::new(error))
    }
}
fn catch_resolution_error(error: TypeAccessError) -> ExceptionAccessError {
    match error {
        TypeAccessError::Java(TypeJavaError::IllegalAccess) => {
            ExceptionAccessError::Java(VmExceptionKind::IllegalAccessError)
        }
        error => error.into(),
    }
}

impl FieldResolutionContext<'_> {
    /// Validate an explicitly designated bootstrap Throwable without loading or
    /// allocating an instance, and retain only the admitted snapshot's identities.
    pub fn prepare_exception_profile(
        &mut self,
        throwable: ClassId,
    ) -> Result<ExceptionProfile, ExceptionAccessError> {
        self.unchanged()?;
        let universe = self.universe()?;
        let definition = self.registry().class(throwable)?;
        let loader = self.registry().defining_loader(throwable)?;
        self.heap().instance_graph_node(throwable)?;
        self.step()?;
        let name = definition.name().map_err(RegistryError::Parse)?;
        for _ in name.units() {
            self.step()?;
        }
        let invalid = |reason| ExceptionAccessError::InvalidProfile { throwable, reason };
        if !name.equals_ascii("java/lang/Throwable") {
            return Err(invalid("expected exact java/lang/Throwable name"));
        }
        if loader != universe.bootstrap_loader {
            return Err(invalid(
                "Throwable must use the designated bootstrap loader",
            ));
        }
        if definition.access_flags & (1 | 0x200 | 0x400) != 1 {
            return Err(invalid(
                "Throwable must be a public ordinary nonabstract class",
            ));
        }
        self.step()?;
        if self.heap().instance_graph_node(throwable)?.superclass != Some(universe.object) {
            return Err(invalid(
                "Throwable must directly extend the designated Object",
            ));
        }
        Ok(ExceptionProfile {
            throwable,
            snapshot: Snapshot {
                registry: self.registry().identity(),
                heap: self.heap().identity,
                layouts: self.heap().retained_layout_classes(),
                universe,
            },
        })
    }

    /// Constant-size identity validation. Fresh contexts over the unchanged heap
    /// may reuse a token; allocations alone do not invalidate it.
    pub fn check_exception_profile(
        &mut self,
        profile: &ExceptionProfile,
    ) -> Result<(), ExceptionAccessError> {
        self.unchanged()?;
        let snapshot = profile.snapshot;
        if self.registry().identity() != snapshot.registry
            || self.heap().identity != snapshot.heap
            || self.heap().retained_layout_classes() != snapshot.layouts
            || self.heap().reference_type_universe() != Some(snapshot.universe)
        {
            return Err(ExceptionAccessError::ProfileMismatch);
        }
        self.registry().class(profile.throwable)?;
        self.heap().instance_graph_node(profile.throwable)?;
        Ok(())
    }

    pub fn prepare_throw(
        &mut self,
        profile: &ExceptionProfile,
        reference: Reference,
    ) -> Result<ThrownException, ExceptionAccessError> {
        self.check_exception_profile(profile)?;
        if reference.is_null() {
            return Err(ExceptionAccessError::NullThrowable);
        }
        let actual = self.heap().instance_class(reference)?;
        if !self.subclass(actual, profile.throwable)? {
            return Err(ExceptionAccessError::InvalidThrownType {
                expected: profile.throwable,
                actual,
            });
        }
        Ok(ThrownException {
            profile: *profile,
            reference,
            class: actual,
        })
    }

    /// Match one already active handler row. The caller owns ordered range search
    /// and its separate dispatch budget; this service charges typed resolution.
    pub fn exception_handler_matches(
        &mut self,
        profile: &ExceptionProfile,
        origin: ClassId,
        catch_type: u16,
        thrown: &ThrownException,
    ) -> Result<bool, ExceptionAccessError> {
        self.check_exception_profile(profile)?;
        let definition = self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        if thrown.profile != *profile {
            return Err(ExceptionAccessError::InvalidThrownToken);
        }
        let actual = self.heap().instance_class(thrown.reference)?;
        if actual != thrown.class {
            return Err(ExceptionAccessError::InvalidThrownToken);
        }
        if catch_type == 0 {
            return Ok(true);
        }
        let invalid = || ExceptionAccessError::InvalidCatchConstant {
            origin,
            index: catch_type,
        };
        let Constant::Class { name_index } = definition
            .constant_pool
            .get(catch_type)
            .map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        let name = definition
            .constant_pool
            .utf8(*name_index)
            .map_err(|_| invalid())?;
        if name.units().first() == Some(&91) {
            return Err(invalid());
        }
        let ReferenceType::Class(class) = self
            .resolve_reference_type(origin, catch_type)
            .map_err(catch_resolution_error)?
        else {
            return Err(invalid());
        };
        if self.registry().class(class)?.access_flags & 0x200 != 0
            || !self.subclass(class, profile.throwable)?
        {
            return Err(ExceptionAccessError::InvalidCatchType {
                origin,
                index: catch_type,
                class,
            });
        }
        Ok(self.subclass(thrown.class, class)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DefinitionRegistry, FieldResolutionOptions, HeapOptions, HierarchyOptions, InstanceOptions,
        ManagedHeap, PrimitiveArrayKind, RegistryOptions, SameLoaderResolver, build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    use std::error::Error;

    struct Fixture {
        registry: DefinitionRegistry,
        heap: ManagedHeap,
        throwable: ClassId,
        child: ClassId,
        origin: ClassId,
        base: ClassId,
        unadmitted: ClassId,
    }
    fn fixture() -> Fixture {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut classes = Vec::new();
        for (name, parent, flags) in [
            ("java/lang/Object", None, 0x21_u16),
            ("java/lang/Cloneable", Some("java/lang/Object"), 0x601),
            ("java/io/Serializable", Some("java/lang/Object"), 0x601),
            ("java/lang/Throwable", Some("java/lang/Object"), 0x21),
            ("Base", Some("java/lang/Throwable"), 0x21),
            ("Child", Some("Base"), 0x21),
            ("Caller", Some("java/lang/Object"), 0x21),
            ("Unadmitted", Some("java/lang/Object"), 0x21),
        ] {
            let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 8];
            for (index, text) in [
                (1, name),
                (3, parent.unwrap_or("java/lang/Object")),
                (5, "Base"),
            ] {
                bytes.push(1);
                bytes.extend((text.len() as u16).to_be_bytes());
                bytes.extend(text.bytes());
                bytes.extend([7, 0, index]);
            }
            bytes.extend([1, 0, 1, b'X']);
            for word in [flags, 2, if parent.is_some() { 4 } else { 0 }, 0, 0, 0, 0] {
                bytes.extend(word.to_be_bytes());
            }
            classes.push(
                registry
                    .define(
                        loader,
                        &name.encode_utf16().collect::<Vec<_>>(),
                        &bytes,
                        ParseOptions::default(),
                    )
                    .unwrap(),
            );
        }
        let universe = ReferenceTypeUniverse {
            bootstrap_loader: loader,
            object: classes[0],
            cloneable: classes[1],
            serializable: classes[2],
        };
        let plans: Vec<_> = classes[..7]
            .iter()
            .map(|class| {
                build_hierarchy(
                    &registry,
                    *class,
                    &mut SameLoaderResolver,
                    HierarchyOptions::default(),
                )
                .unwrap()
            })
            .collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(&registry, universe).unwrap();
        Fixture {
            registry,
            heap,
            throwable: classes[3],
            base: classes[4],
            child: classes[5],
            origin: classes[6],
            unadmitted: classes[7],
        }
    }

    #[test]
    fn forged_profile_fingerprints_and_anchor_handles_cannot_bypass_snapshot_guards() {
        let mut f = fixture();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
        let before = ctx.lookup_steps();
        let mut changed_registry = profile;
        changed_registry.snapshot.registry += 1;
        let mut changed_heap = profile;
        changed_heap.snapshot.heap += 1;
        let mut changed_layouts = profile;
        changed_layouts.snapshot.layouts += 1;
        let mut changed_universe = profile;
        changed_universe.snapshot.universe.object = profile.snapshot.universe.cloneable;
        for forged in [
            changed_registry,
            changed_heap,
            changed_layouts,
            changed_universe,
        ] {
            assert_eq!(
                ctx.check_exception_profile(&forged),
                Err(ExceptionAccessError::ProfileMismatch)
            );
            assert_eq!(
                ctx.prepare_throw(&forged, Reference::null()),
                Err(ExceptionAccessError::ProfileMismatch)
            );
        }
        let invalid_class = ClassId {
            index: usize::MAX,
            ..profile.throwable
        };
        let invalid = ExceptionProfile {
            throwable: invalid_class,
            ..profile
        };
        assert!(matches!(
            ctx.check_exception_profile(&invalid),
            Err(ExceptionAccessError::Context(FieldAccessError::Registry(
                RegistryError::InvalidHandle { .. }
            )))
        ));
        assert!(matches!(
            ctx.prepare_exception_profile(invalid_class),
            Err(ExceptionAccessError::Context(FieldAccessError::Registry(
                RegistryError::InvalidHandle { .. }
            )))
        ));
        assert_eq!(
            ctx.check_exception_profile(&ExceptionProfile {
                throwable: f.unadmitted,
                ..profile
            }),
            Err(ExceptionAccessError::Context(FieldAccessError::Instance(
                InstanceError::LayoutMissing {
                    class: f.unadmitted
                }
            )))
        );
        assert_eq!(ctx.lookup_steps(), before);
    }

    #[test]
    fn forged_thrown_tokens_recheck_profile_actual_class_and_every_handle_component() {
        let mut f = fixture();
        let value = f.heap.allocate_instance(f.child).unwrap();
        let primitive = f
            .heap
            .allocate_primitive_array(PrimitiveArrayKind::Byte, 0)
            .unwrap();
        let (identity, index, generation) = value.heap_ref().unwrap().components();
        let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
        let foreign_value = foreign
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let profile = ctx.prepare_exception_profile(f.throwable).unwrap();
        let token = ctx.prepare_throw(&profile, value).unwrap();
        let before = ctx.lookup_steps();
        let mut other_profile = profile;
        other_profile.snapshot.heap += 1;
        for forged in [
            ThrownException {
                profile: other_profile,
                ..token
            },
            ThrownException {
                class: f.base,
                ..token
            },
        ] {
            assert_eq!(
                ctx.exception_handler_matches(&profile, f.origin, 0, &forged),
                Err(ExceptionAccessError::InvalidThrownToken)
            );
        }
        for (reference, error) in [
            (foreign_value, HeapError::ForeignHeap),
            (
                Reference::issued(identity, usize::MAX, generation),
                HeapError::MissingEntry { index: usize::MAX },
            ),
            (
                Reference::issued(identity, index, generation + 1),
                HeapError::StaleGeneration {
                    expected: generation,
                    actual: generation + 1,
                },
            ),
            (Reference::null(), HeapError::NullReference),
            (
                primitive,
                HeapError::WrongEntryKind {
                    expected: crate::HeapEntryKind::Instance,
                    actual: crate::HeapEntryKind::PrimitiveArray,
                },
            ),
        ] {
            let expected = ExceptionAccessError::from(error);
            let forged = ThrownException { reference, ..token };
            assert_eq!(
                ctx.exception_handler_matches(&profile, f.origin, 0, &forged),
                Err(expected.clone())
            );
            assert_eq!(
                ctx.exception_handler_matches(&profile, f.origin, u16::MAX, &forged),
                Err(expected.clone())
            );
            if reference.is_null() {
                assert_eq!(
                    ctx.prepare_throw(&profile, reference),
                    Err(ExceptionAccessError::NullThrowable)
                );
            } else {
                assert_eq!(ctx.prepare_throw(&profile, reference), Err(expected));
            }
        }
        assert_eq!(ctx.lookup_steps(), before);
        assert_eq!(
            ctx.exception_handler_matches(
                &profile,
                f.unadmitted,
                0,
                &ThrownException {
                    profile: other_profile,
                    ..token
                }
            ),
            Err(ExceptionAccessError::Context(FieldAccessError::Instance(
                InstanceError::LayoutMissing {
                    class: f.unadmitted
                }
            )))
        );
    }

    #[test]
    fn catch_error_mapping_is_direct_and_preserves_nested_sources_and_categories() {
        let direct = TypeAccessError::Java(TypeJavaError::IllegalAccess);
        assert_eq!(
            catch_resolution_error(direct.clone()),
            ExceptionAccessError::Java(VmExceptionKind::IllegalAccessError)
        );
        for nested in [
            TypeAccessError::Java(TypeJavaError::ClassCast),
            TypeAccessError::Context(FieldAccessError::Java(crate::FieldJavaError::IllegalAccess)),
            TypeAccessError::Context(FieldAccessError::ReferenceType(Box::new(direct))),
        ] {
            let result = catch_resolution_error(nested.clone());
            assert_eq!(result, ExceptionAccessError::Type(Box::new(nested)));
            assert!(result.source().is_some());
            assert!(result.to_string().contains("type validation"));
        }
        let wrapped = ExceptionAccessError::from(HeapError::ForeignHeap);
        assert!(wrapped.source().is_some());
        assert!(wrapped.to_string().contains("context"));
        let categories = [
            VmExceptionKind::ArithmeticException,
            VmExceptionKind::ClassCastException,
            VmExceptionKind::ArrayStoreException,
            VmExceptionKind::NullPointerException,
            VmExceptionKind::ArrayIndexOutOfBoundsException,
            VmExceptionKind::NegativeArraySizeException,
            VmExceptionKind::NoSuchFieldError,
            VmExceptionKind::IllegalAccessError,
            VmExceptionKind::IncompatibleClassChangeError,
            VmExceptionKind::NoSuchMethodError,
            VmExceptionKind::AbstractMethodError,
            VmExceptionKind::InstantiationError,
        ];
        for category in categories {
            let error = ExceptionAccessError::Java(category);
            assert!(error.source().is_none());
            assert!(error.to_string().contains(&format!("{category:?}")));
        }
    }
}

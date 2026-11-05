//! Checked construction metadata and memory. Frame provenance, initialization
//! completion and unfinished-reference aliases belong to the interpreter.
use crate::{
    ClassId, FieldAccessError, FieldJavaError, FieldResolutionContext, InitializationAccessError,
    InitializationClaim, InstanceError, MethodAccessError, MethodId, MethodJavaError,
    MethodValueShape, Reference, ReferenceType, RegistryError, TypeAccessError, TypeJavaError,
    Value, initialization::InitializationFingerprint,
};
use crabspot_classfile::{Constant, Member};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructorReceiverKind {
    Allocation,
    This,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructorFieldReceiverKind {
    Ordinary,
    UninitializedThis,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructionJavaError {
    LinkageError,
    NoSuchMethod,
    NoSuchField,
    IllegalAccess,
    IncompatibleClassChange,
    NullPointer,
    Instantiation,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConstructionAccessError {
    Java(ConstructionJavaError),
    Method(Box<MethodAccessError>),
    Field(Box<FieldAccessError>),
    Type(Box<TypeAccessError>),
    Initialization(Box<InitializationAccessError>),
    Instance(Box<InstanceError>),
    InvalidMetadata(&'static str),
    InvalidReceiverKind,
    InvalidConstructorOwner { origin: MethodId, owner: ClassId },
    InvalidConstructorReceiver { expected: ClassId, actual: ClassId },
    TokenMismatch,
}
impl fmt::Display for ConstructionAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Java(error) => write!(f, "construction access failed: {error:?}"),
            Self::Method(error) => write!(f, "construction method operation failed: {error}"),
            Self::Field(error) => write!(f, "construction field operation failed: {error}"),
            Self::Type(error) => write!(f, "construction type operation failed: {error}"),
            Self::Initialization(error) => write!(f, "construction initialization failed: {error}"),
            Self::Instance(error) => write!(f, "construction instance operation failed: {error}"),
            Self::InvalidMetadata(reason) => write!(f, "invalid construction metadata: {reason}"),
            Self::InvalidReceiverKind => {
                f.write_str("invalid construction receiver mode or identity")
            }
            Self::InvalidConstructorOwner { origin, owner } => {
                write!(f, "constructor {origin:?} cannot use owner {owner:?}")
            }
            Self::InvalidConstructorReceiver { expected, actual } => write!(
                f,
                "constructor receiver {actual:?} does not match {expected:?}"
            ),
            Self::TokenMismatch => {
                f.write_str("construction token does not match this checked snapshot")
            }
        }
    }
}
impl std::error::Error for ConstructionAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Method(e) => Some(e.as_ref()),
            Self::Field(e) => Some(e.as_ref()),
            Self::Type(e) => Some(e.as_ref()),
            Self::Initialization(e) => Some(e.as_ref()),
            Self::Instance(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl From<MethodAccessError> for ConstructionAccessError {
    fn from(error: MethodAccessError) -> Self {
        use ConstructionJavaError as J;
        match error {
            MethodAccessError::Java(MethodJavaError::LinkageError) => Self::Java(J::LinkageError),
            MethodAccessError::Java(MethodJavaError::NoSuchMethod) => Self::Java(J::NoSuchMethod),
            MethodAccessError::Java(MethodJavaError::IllegalAccess) => Self::Java(J::IllegalAccess),
            MethodAccessError::Java(MethodJavaError::IncompatibleClassChange) => {
                Self::Java(J::IncompatibleClassChange)
            }
            MethodAccessError::Java(MethodJavaError::NullPointer) => Self::Java(J::NullPointer),
            _ => Self::Method(Box::new(error)),
        }
    }
}
impl From<FieldAccessError> for ConstructionAccessError {
    fn from(error: FieldAccessError) -> Self {
        use ConstructionJavaError as J;
        match error {
            FieldAccessError::Java(FieldJavaError::LinkageError) => Self::Java(J::LinkageError),
            FieldAccessError::Java(FieldJavaError::NoSuchField) => Self::Java(J::NoSuchField),
            FieldAccessError::Java(FieldJavaError::IllegalAccess) => Self::Java(J::IllegalAccess),
            FieldAccessError::Java(FieldJavaError::IncompatibleClassChange) => {
                Self::Java(J::IncompatibleClassChange)
            }
            FieldAccessError::Java(FieldJavaError::NullPointer) => Self::Java(J::NullPointer),
            _ => Self::Field(Box::new(error)),
        }
    }
}
impl From<TypeAccessError> for ConstructionAccessError {
    fn from(error: TypeAccessError) -> Self {
        match error {
            TypeAccessError::Java(TypeJavaError::IllegalAccess) => {
                Self::Java(ConstructionJavaError::IllegalAccess)
            }
            _ => Self::Type(Box::new(error)),
        }
    }
}
impl From<InitializationAccessError> for ConstructionAccessError {
    fn from(error: InitializationAccessError) -> Self {
        Self::Initialization(Box::new(error))
    }
}
impl From<InstanceError> for ConstructionAccessError {
    fn from(error: InstanceError) -> Self {
        Self::Instance(Box::new(error))
    }
}
impl From<RegistryError> for ConstructionAccessError {
    fn from(error: RegistryError) -> Self {
        Self::Field(Box::new(FieldAccessError::Registry(error)))
    }
}

#[derive(Debug)]
pub struct ResolvedNew {
    fingerprint: InitializationFingerprint,
    origin: ClassId,
    index: u16,
    name: u16,
    class: ClassId,
    flags: u16,
    class_name: u16,
}
impl ResolvedNew {
    pub fn class(&self) -> ClassId {
        self.class
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MemberKey {
    name: u16,
    descriptor: u16,
    flags: u16,
}
impl MemberKey {
    fn of(member: &Member) -> Self {
        Self {
            name: member.name_index,
            descriptor: member.descriptor_index,
            flags: member.access_flags,
        }
    }
}
#[derive(Debug)]
pub struct ResolvedConstructor {
    fingerprint: InitializationFingerprint,
    method: MethodId,
    member: MemberKey,
    receiver: Reference,
    actual: ClassId,
}
impl ResolvedConstructor {
    pub fn method(&self) -> MethodId {
        self.method
    }
}

impl FieldResolutionContext<'_> {
    pub fn resolve_new(
        &mut self,
        claim: &InitializationClaim,
        origin: ClassId,
        index: u16,
    ) -> Result<ResolvedNew, ConstructionAccessError> {
        self.check_initialization_class(claim, origin)?;
        let definition = self.registry().class(origin)?;
        let invalid =
            || ConstructionAccessError::InvalidMetadata("new requires Class/Utf8 metadata");
        let Constant::Class { name_index } =
            definition.constant_pool.get(index).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        let name = definition
            .constant_pool
            .utf8(*name_index)
            .map_err(|_| invalid())?;
        if name.units().first() == Some(&91) {
            return Err(ConstructionAccessError::InvalidMetadata(
                "new cannot name an array",
            ));
        }
        let name_index = *name_index;
        let ReferenceType::Class(class) = self.resolve_reference_type(origin, index)? else {
            return Err(invalid());
        };
        let target = self.registry().class(class)?;
        if target.access_flags & (0x200 | 0x400) != 0 {
            return Err(ConstructionAccessError::Java(
                ConstructionJavaError::Instantiation,
            ));
        }
        self.heap().check_reserved_class(class)?;
        let Constant::Class {
            name_index: class_name,
        } = target
            .constant_pool
            .get(target.this_class)
            .map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        Ok(ResolvedNew {
            fingerprint: claim.fingerprint(),
            origin,
            index,
            name: name_index,
            class,
            flags: target.access_flags,
            class_name: *class_name,
        })
    }
    /// Allocate default storage after the engine has initialized access.class().
    /// This service does not know lifecycle completion or certify construction.
    pub fn allocate_new(
        &mut self,
        claim: &InitializationClaim,
        access: &ResolvedNew,
    ) -> Result<Reference, ConstructionAccessError> {
        self.check_initialization_class(claim, access.origin)?;
        if claim.fingerprint() != access.fingerprint {
            return Err(ConstructionAccessError::TokenMismatch);
        }
        let origin = self.registry().class(access.origin)?;
        if !matches!(origin.constant_pool.get(access.index), Ok(Constant::Class { name_index }) if *name_index == access.name)
        {
            return Err(ConstructionAccessError::TokenMismatch);
        }
        let target = self.registry().class(access.class)?;
        self.heap().instance_graph_node(access.class)?;
        if target.access_flags != access.flags
            || !matches!(target.constant_pool.get(target.this_class), Ok(Constant::Class { name_index }) if *name_index == access.class_name)
        {
            return Err(ConstructionAccessError::TokenMismatch);
        }
        if target.access_flags & (0x200 | 0x400) != 0 {
            return Err(ConstructionAccessError::Java(
                ConstructionJavaError::Instantiation,
            ));
        }
        let mut current = Some(access.class);
        for _ in 0..self.heap().retained_layout_classes() {
            let Some(class) = current else {
                return Ok(self.heap_mut().allocate_instance(access.class)?);
            };
            self.step()?;
            let (parent, fields) = self.heap().instance_allocation_shape(class)?;
            for _ in 0..fields {
                self.step()?;
            }
            current = parent;
        }
        if current.is_none() {
            return Ok(self.heap_mut().allocate_instance(access.class)?);
        }
        Err(ConstructionAccessError::InvalidMetadata(
            "cyclic allocation shape chain",
        ))
    }
    pub fn constructor_invocation_shape(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> Result<MethodValueShape, ConstructionAccessError> {
        Ok(self.constructor_shape(origin, index)?)
    }
    pub fn resolve_constructor(
        &mut self,
        claim: &InitializationClaim,
        origin: MethodId,
        index: u16,
        receiver: Reference,
        kind: ConstructorReceiverKind,
    ) -> Result<ResolvedConstructor, ConstructionAccessError> {
        self.check_initialization_class(claim, origin.owner())?;
        self.registry().method(origin)?;
        let (owner, method) = self.constructor_declaration(origin.owner(), index)?;
        if method.owner() != owner {
            return Err(ConstructionAccessError::Java(
                ConstructionJavaError::NoSuchMethod,
            ));
        }
        let member = self.registry().method(method)?;
        if member.access_flags & 8 != 0 {
            return Err(ConstructionAccessError::Java(
                ConstructionJavaError::IncompatibleClassChange,
            ));
        }
        let member_key = MemberKey::of(member);
        if receiver.is_null() {
            return Err(ConstructionAccessError::Java(
                ConstructionJavaError::NullPointer,
            ));
        }
        let actual = self.heap().instance_class(receiver)?;
        match kind {
            ConstructorReceiverKind::Allocation => {
                if actual != owner {
                    return Err(ConstructionAccessError::InvalidConstructorReceiver {
                        expected: owner,
                        actual,
                    });
                }
                if member_key.flags & 4 != 0
                    && !self.same_package(origin.owner(), owner)?
                    && !self.subclass(actual, origin.owner())?
                {
                    return Err(ConstructionAccessError::InvalidConstructorReceiver {
                        expected: origin.owner(),
                        actual,
                    });
                }
            }
            ConstructorReceiverKind::This => {
                if !self.is_constructor(origin)? {
                    return Err(ConstructionAccessError::InvalidReceiverKind);
                }
                if owner != origin.owner()
                    && self.heap().instance_graph_node(origin.owner())?.superclass != Some(owner)
                {
                    return Err(ConstructionAccessError::InvalidConstructorOwner { origin, owner });
                }
                if !self.subclass(actual, origin.owner())? {
                    return Err(ConstructionAccessError::InvalidConstructorReceiver {
                        expected: origin.owner(),
                        actual,
                    });
                }
            }
        }
        // VM Class payloads are already issued objects, never unfinished
        // constructor receivers. Preserve symbolic/category/receiver precedence,
        // then refuse authority even for a Class-origin call to Object.<init>.
        self.heap().reject_opaque(receiver)?;
        if owner == self.universe()?.object {
            let definition = self.registry().class(owner)?;
            if definition.access_flags & (1 | 0x200 | 0x400) != 1
                || self.heap().instance_graph_node(owner)?.superclass.is_some()
            {
                return Err(ConstructionAccessError::InvalidMetadata(
                    "Object constructor requires a public concrete root",
                ));
            }
        }
        Ok(ResolvedConstructor {
            fingerprint: claim.fingerprint(),
            method,
            member: member_key,
            receiver,
            actual,
        })
    }
    fn is_constructor(&self, method: MethodId) -> Result<bool, ConstructionAccessError> {
        let definition = self.registry().class(method.owner())?;
        let member = self.registry().method(method)?;
        Ok(member.access_flags & 8 == 0
            && definition
                .constant_pool
                .utf8(member.name_index)
                .map_err(RegistryError::Parse)?
                .equals_ascii("<init>"))
    }
    fn check_constructor_token(
        &self,
        claim: &InitializationClaim,
        constructor: &ResolvedConstructor,
    ) -> Result<(), ConstructionAccessError> {
        self.check_initialization_class(claim, constructor.method.owner())?;
        if claim.fingerprint() != constructor.fingerprint
            || MemberKey::of(self.registry().method(constructor.method)?) != constructor.member
            || !self.is_constructor(constructor.method)?
            || self.heap().instance_class(constructor.receiver)? != constructor.actual
        {
            return Err(ConstructionAccessError::TokenMismatch);
        }
        Ok(())
    }
    pub fn write_constructor_field(
        &mut self,
        claim: &InitializationClaim,
        constructor: &ResolvedConstructor,
        index: u16,
        receiver: Reference,
        value: Value,
        kind: ConstructorFieldReceiverKind,
    ) -> Result<(), ConstructionAccessError> {
        self.check_constructor_token(claim, constructor)?;
        let origin = constructor.method.owner();
        let selected = self.select(origin, index)?;
        let member = self.registry().field(selected.field)?;
        let flags = member.access_flags;
        if flags & 8 != 0 {
            return Err(ConstructionAccessError::Java(
                ConstructionJavaError::IncompatibleClassChange,
            ));
        }
        if flags & 16 != 0 && selected.field.owner() != origin {
            return Err(ConstructionAccessError::Java(
                ConstructionJavaError::IllegalAccess,
            ));
        }
        if kind == ConstructorFieldReceiverKind::UninitializedThis {
            if selected.symbolic_owner != origin {
                return Err(ConstructionAccessError::InvalidConstructorOwner {
                    origin: constructor.method,
                    owner: selected.symbolic_owner,
                });
            }
            if selected.field.owner() != origin {
                return Err(ConstructionAccessError::InvalidConstructorOwner {
                    origin: constructor.method,
                    owner: selected.field.owner(),
                });
            }
            if receiver != constructor.receiver {
                return Err(ConstructionAccessError::InvalidReceiverKind);
            }
        }
        self.field_receiver(&selected, receiver)?;
        self.check_field_assignment(selected.field, value)?;
        // Admitted shape kinds are already classified from immutable descriptor
        // metadata. Do not rescan a long null-valued descriptor without a charge.
        let registry = self.registry();
        let proof =
            self.heap()
                .constructor_field_proof(registry, constructor, receiver, selected.field)?;
        Ok(self
            .heap_mut()
            .commit_constructor_field(registry, proof, value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod rt {
        pub use crate::*;
    }
    #[allow(dead_code)]
    mod support {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/construction.rs"
        ));
    }
    use crate::{FieldResolutionOptions, SameLoaderResolver};
    use std::error::Error;
    use support::*;

    #[test]
    fn only_direct_expected_java_errors_are_elevated_and_wrappers_keep_sources() {
        assert_eq!(
            ConstructionAccessError::from(MethodAccessError::Java(MethodJavaError::NoSuchMethod)),
            ConstructionAccessError::Java(ConstructionJavaError::NoSuchMethod)
        );
        assert_eq!(
            ConstructionAccessError::from(FieldAccessError::Java(FieldJavaError::NoSuchField)),
            ConstructionAccessError::Java(ConstructionJavaError::NoSuchField)
        );
        assert_eq!(
            ConstructionAccessError::from(TypeAccessError::Java(TypeJavaError::IllegalAccess)),
            ConstructionAccessError::Java(ConstructionJavaError::IllegalAccess)
        );
        let nested =
            MethodAccessError::Context(FieldAccessError::Java(FieldJavaError::IllegalAccess));
        assert_eq!(
            ConstructionAccessError::from(nested.clone()),
            ConstructionAccessError::Method(Box::new(nested))
        );
        for error in [
            ConstructionAccessError::from(MethodAccessError::Java(MethodJavaError::AbstractMethod)),
            ConstructionAccessError::from(TypeAccessError::Java(TypeJavaError::ClassCast)),
            ConstructionAccessError::from(InitializationAccessError::AlreadyClaimed),
            ConstructionAccessError::from(InstanceError::Disabled),
        ] {
            assert!(error.source().is_some());
            assert!(!error.to_string().is_empty());
        }
        for java in [
            ConstructionJavaError::NoSuchMethod,
            ConstructionJavaError::NoSuchField,
            ConstructionJavaError::IllegalAccess,
            ConstructionJavaError::IncompatibleClassChange,
            ConstructionJavaError::NullPointer,
            ConstructionJavaError::Instantiation,
        ] {
            assert!(ConstructionAccessError::Java(java).source().is_none());
        }
        assert!(std::mem::size_of::<ConstructionAccessError>() <= 64);
    }
    #[test]
    fn forged_new_fingerprints_and_metadata_never_allocate_or_spend_layout_work() {
        let mut w = world();
        let own = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[],
            &[],
            &["C"],
            65,
        );
        let mut resolver = SameLoaderResolver;
        let mut heap = heap(&w, &[own.class, w.error], &mut resolver);
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        for variant in 0..6 {
            let mut access = ctx.resolve_new(&claim, own.class, own.classes[0]).unwrap();
            match variant {
                0 => access.index = 0,
                1 => access.name = u16::MAX,
                2 => access.flags ^= 16,
                3 => access.class_name = u16::MAX,
                4 => access.origin.index = usize::MAX,
                _ => access.class.index = usize::MAX,
            }
            let before = ctx.lookup_steps();
            assert!(ctx.allocate_new(&claim, &access).is_err());
            assert_eq!(ctx.lookup_steps(), before);
            assert_eq!(ctx.heap().allocated_instances(), 0);
        }
        let access = ctx.resolve_new(&claim, own.class, own.classes[0]).unwrap();
        assert!(ctx.allocate_new(&claim, &access).is_ok());
    }
    #[test]
    fn forged_constructor_keys_and_handles_cannot_authorize_final_writes() {
        let mut w = world();
        let own = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[Field::new("x", "I", 17, None)],
            &[
                Method::new("<init>", "()V", 1),
                Method::new("helper", "()V", 9),
            ],
            &[("C", "<init>", "()V", 10), ("C", "x", "I", 9)],
            &[],
            65,
        );
        let mut resolver = SameLoaderResolver;
        let mut heap = heap(&w, &[own.class, w.error], &mut resolver);
        let receiver = heap.allocate_instance(own.class).unwrap();
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        let x = field(&w, own.class, "x", "I");
        for variant in 0..5 {
            let mut ctor = ctx
                .resolve_constructor(
                    &claim,
                    own.methods[0],
                    own.refs[0],
                    receiver,
                    ConstructorReceiverKind::Allocation,
                )
                .unwrap();
            match variant {
                0 => ctor.member.descriptor = u16::MAX,
                1 => ctor.method.index = usize::MAX,
                2 => {
                    ctor.method = own.methods[1];
                    ctor.member = MemberKey::of(w.registry.method(own.methods[1]).unwrap());
                }
                3 => ctor.actual = w.u.object,
                _ => ctor.receiver = Reference::null(),
            }
            assert!(
                ctx.write_constructor_field(
                    &claim,
                    &ctor,
                    own.refs[1],
                    receiver,
                    Value::Int(99),
                    ConstructorFieldReceiverKind::Ordinary
                )
                .is_err()
            );
            assert_eq!(ctx.heap().get_field(receiver, x), Ok(Value::Int(0)));
        }
    }
    #[test]
    fn numeric_constructor_shape_and_lookup_limits_fail_at_inclusive_steps() {
        let mut w = world();
        let own = define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[],
            &[Method::new("<init>", "()V", 1)],
            &[("C", "<init>", "()V", 10)],
            &[],
            65,
        );
        let mut resolver = SameLoaderResolver;
        let mut heap = heap(&w, &[own.class, w.error], &mut resolver);
        let receiver = heap.allocate_instance(own.class).unwrap();
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let seed = ctx.lookup_steps();
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        drop(ctx);
        for extra in [0, 1, 2] {
            let mut ctx = FieldResolutionContext::new_unnamed_modules(
                &w.registry,
                &mut heap,
                &mut resolver,
                FieldResolutionOptions {
                    max_lookup_steps: seed + extra,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(
                ctx.constructor_invocation_shape(own.class, own.refs[0])
                    .is_ok()
            );
            assert_eq!(ctx.lookup_steps(), seed);
            let result = ctx.resolve_constructor(
                &claim,
                own.methods[0],
                own.refs[0],
                receiver,
                ConstructorReceiverKind::Allocation,
            );
            if extra < 2 {
                assert!(result.is_err());
                assert_eq!(ctx.lookup_steps(), seed + extra);
            } else {
                assert_eq!(result.unwrap().method(), own.methods[0]);
                assert_eq!(ctx.lookup_steps(), seed + 2);
            }
        }
    }
}

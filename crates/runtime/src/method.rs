//! Bounded class/interface method resolution and identity-aware selection.
use crate::{
    ClassId, FieldAccessError, FieldResolutionContext, HeapError, InstanceError,
    InstanceInvocationKind, LoaderId, MethodId, MethodReferenceMismatch, MethodValuePosition,
    MethodValueShape, Reference, RegistryError, TypeAccessError, ValueKind,
};
use crabspot_classfile::{
    Constant, JavaString,
    descriptor::{FieldType, parse_method_descriptor},
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodJavaError {
    LinkageError,
    NoSuchMethod,
    IllegalAccess,
    IncompatibleClassChange,
    NullPointer,
    AbstractMethod,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MethodAccessError {
    MemberResolution(Box<crate::MemberResolutionError>),
    ClassMirror(Box<crate::ClassMirrorError>),
    Java(MethodJavaError),
    Context(FieldAccessError),
    Type(Box<TypeAccessError>),
    UnsupportedLoadingConstraint {
        origin: ClassId,
        method: MethodId,
    },
    UnsupportedOverrideConstraint {
        method: MethodId,
        declaration: MethodId,
    },
    ArgumentCount {
        method: MethodId,
        expected: usize,
        actual: usize,
    },
    ValueKindMismatch {
        method: MethodId,
        position: MethodValuePosition,
        expected: Option<ValueKind>,
        actual: Option<ValueKind>,
    },
    IncompatibleReference(Box<MethodReferenceMismatch>),
    InvalidConstant {
        origin: ClassId,
        index: u16,
    },
    UnsupportedDescriptor,
    UnsupportedSpecialMethod,
    UnsupportedSignaturePolymorphic {
        method: MethodId,
    },
    /// An eligible declaration witness, not a maximally-specific selected target.
    UnsupportedInterfaceMethods {
        owner: ClassId,
    },
    UnsupportedNestAccess {
        origin: ClassId,
        owner: ClassId,
    },
    InvalidOverride {
        method: MethodId,
        overridden: MethodId,
    },
    InvalidSpecialOwner {
        origin: ClassId,
        owner: ClassId,
    },
}
impl fmt::Display for MethodAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClassMirror(error) => error.fmt(f),
            Self::MemberResolution(error) => error.fmt(f),
            Self::Java(error) => write!(f, "method access failed: {error:?}"),
            Self::Context(error) => write!(f, "method context failed: {error}"),
            Self::Type(error) => write!(f, "method reference type validation failed: {error}"),
            Self::UnsupportedLoadingConstraint { origin, method } => write!(
                f,
                "nominal signature resolution from {origin:?} to {method:?} requires unsupported loading constraints"
            ),
            Self::UnsupportedOverrideConstraint {
                method,
                declaration,
            } => write!(
                f,
                "nominal signature of {method:?} matches {declaration:?} under a different defining loader"
            ),
            Self::ArgumentCount {
                method,
                expected,
                actual,
            } => write!(
                f,
                "method {method:?} requires {expected} arguments, found {actual}"
            ),
            Self::ValueKindMismatch {
                method,
                position,
                expected,
                actual,
            } => write!(
                f,
                "method {method:?} {position:?} requires {expected:?}, found {actual:?}"
            ),
            Self::IncompatibleReference(mismatch) => write!(
                f,
                "method {:?} {:?} reference type {:?} is not assignable to {:?}",
                mismatch.method, mismatch.position, mismatch.source, mismatch.target
            ),
            Self::InvalidConstant { origin, index } => write!(
                f,
                "constant #{index} in {origin:?} is not a valid method reference for this call kind"
            ),
            Self::UnsupportedDescriptor => {
                f.write_str("method descriptor is unsupported by the selected execution profile")
            }
            Self::UnsupportedSpecialMethod => {
                f.write_str("initialization methods are excluded from ordinary calls")
            }
            Self::UnsupportedSignaturePolymorphic { method } => write!(
                f,
                "signature-polymorphic method {method:?} requires unsupported invocation services"
            ),
            Self::UnsupportedInterfaceMethods { owner } => write!(
                f,
                "matching interface declaration in {owner:?} requires unsupported interface method resolution"
            ),
            Self::UnsupportedNestAccess { origin, owner } => write!(
                f,
                "private method access from {origin:?} to {owner:?} requires unsupported nest checks"
            ),
            Self::InvalidOverride { method, overridden } => write!(
                f,
                "method {method:?} overrides final declaration {overridden:?}"
            ),
            Self::InvalidSpecialOwner { origin, owner } => write!(
                f,
                "special-call owner {owner:?} is not the current type, a superclass, a direct superinterface or the current interface's Object dependency of {origin:?}"
            ),
        }
    }
}
impl std::error::Error for MethodAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ClassMirror(error) => Some(error.as_ref()),
            Self::MemberResolution(error) => Some(error.as_ref()),
            Self::Context(error) => Some(error),
            Self::Type(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}
impl From<FieldAccessError> for MethodAccessError {
    fn from(error: FieldAccessError) -> Self {
        Self::Context(error)
    }
}
impl From<RegistryError> for MethodAccessError {
    fn from(error: RegistryError) -> Self {
        Self::Context(FieldAccessError::Registry(error))
    }
}
impl From<InstanceError> for MethodAccessError {
    fn from(error: InstanceError) -> Self {
        Self::Context(FieldAccessError::Instance(error))
    }
}
impl From<TypeAccessError> for MethodAccessError {
    fn from(error: TypeAccessError) -> Self {
        Self::Type(Box::new(error))
    }
}
fn allocation() -> MethodAccessError {
    InstanceError::Heap(HeapError::AllocationFailure {
        resource: "method lookup metadata",
    })
    .into()
}
fn reserve<T>(values: &mut Vec<T>, amount: usize) -> Result<(), MethodAccessError> {
    values.try_reserve(amount).map_err(|_| allocation())
}
fn reserve_set<T: Eq + std::hash::Hash>(
    set: &mut HashSet<T>,
    amount: usize,
) -> Result<(), MethodAccessError> {
    set.try_reserve(amount).map_err(|_| allocation())
}
fn reserve_map<K: Eq + std::hash::Hash, V>(
    map: &mut HashMap<K, V>,
    amount: usize,
) -> Result<(), MethodAccessError> {
    map.try_reserve(amount).map_err(|_| allocation())
}
fn nominal_owner(owner: crate::ReferenceType) -> Result<ClassId, MethodAccessError> {
    match owner {
        crate::ReferenceType::Class(class) => Ok(class),
        _ => Err(FieldAccessError::UnsupportedArrayOwner.into()),
    }
}
type Package<'a> = (LoaderId, &'a [u16]);
struct Symbol<'a> {
    class_index: u16,
    owner: &'a JavaString,
    name: &'a JavaString,
    descriptor: &'a JavaString,
    nominal: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Resolved {
    pub(crate) owner: crate::ReferenceType,
    pub(crate) method: MethodId,
    pub(crate) protected_receiver: Option<ClassId>,
}
#[derive(Clone, Copy)]
struct Candidate {
    method: MethodId,
    flags: u16,
}
struct InterfaceCandidates {
    declarations: Vec<Candidate>,
    maximal: Vec<bool>,
}
impl InterfaceCandidates {
    fn unique_default(&self) -> Option<MethodId> {
        let mut defaults = self
            .declarations
            .iter()
            .zip(&self.maximal)
            .filter(|(candidate, maximal)| **maximal && candidate.flags & 0x400 == 0)
            .map(|(candidate, _)| candidate.method);
        let first = defaults.next()?;
        defaults.next().is_none().then_some(first)
    }
    fn resolution(&self) -> Option<MethodId> {
        self.unique_default()
            .or_else(|| self.declarations.first().map(|candidate| candidate.method))
    }
    fn selection(&self) -> Result<MethodId, MethodAccessError> {
        if let Some(method) = self.unique_default() {
            return Ok(method);
        }
        let conflict = self
            .declarations
            .iter()
            .zip(&self.maximal)
            .any(|(candidate, maximal)| *maximal && candidate.flags & 0x400 == 0);
        Err(MethodAccessError::Java(if conflict {
            MethodJavaError::IncompatibleClassChange
        } else {
            MethodJavaError::AbstractMethod
        }))
    }
}

impl<'a> FieldResolutionContext<'a> {
    fn method_symbol(
        &mut self,
        origin: ClassId,
        index: u16,
        interface: bool,
    ) -> Result<Symbol<'a>, MethodAccessError> {
        Ok(self
            .method_symbol_and_signature(origin, index, interface, false)?
            .0)
    }
    fn method_symbol_and_signature(
        &mut self,
        origin: ClassId,
        index: u16,
        interface: bool,
        shape_only: bool,
    ) -> Result<(Symbol<'a>, crate::method_values::Signature<'a>), MethodAccessError> {
        self.read_method_symbol(origin, index, interface, shape_only, false)
    }
    fn read_method_symbol(
        &mut self,
        origin: ClassId,
        index: u16,
        interface: bool,
        shape_only: bool,
        constructor: bool,
    ) -> Result<(Symbol<'a>, crate::method_values::Signature<'a>), MethodAccessError> {
        let invalid = || MethodAccessError::InvalidConstant { origin, index };
        let pool = &self.registry().class(origin)?.constant_pool;
        let (class_index, name_and_type_index) = match pool.get(index).map_err(|_| invalid())? {
            Constant::Methodref {
                class_index,
                name_and_type_index,
            } if !interface => (*class_index, *name_and_type_index),
            Constant::InterfaceMethodref {
                class_index,
                name_and_type_index,
            } if interface => (*class_index, *name_and_type_index),
            _ => return Err(invalid()),
        };
        let owner = pool.class_name(class_index).map_err(|_| invalid())?;
        if !shape_only && owner.units().first() == Some(&91) && !self.class_sites_enabled() {
            return Err(FieldAccessError::UnsupportedArrayOwner.into());
        }
        let Constant::NameAndType {
            name_index,
            descriptor_index,
        } = pool.get(name_and_type_index).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        let name = pool.utf8(*name_index).map_err(|_| invalid())?;
        let descriptor = pool.utf8(*descriptor_index).map_err(|_| invalid())?;
        if constructor && !name.equals_ascii("<init>") {
            return Err(invalid());
        }
        if !constructor && (name.equals_ascii("<init>") || name.equals_ascii("<clinit>")) {
            return Err(MethodAccessError::UnsupportedSpecialMethod);
        }
        if !shape_only
            && owner.units().first() == Some(&91)
            && !(name.equals_ascii("getClass") && descriptor.equals_ascii("()Ljava/lang/Class;"))
        {
            return Err(FieldAccessError::UnsupportedArrayOwner.into());
        }
        let signature =
            self.inspect_signature(descriptor.units())
                .map_err(|error| match error {
                    MethodAccessError::Context(FieldAccessError::InvalidState(_)) => invalid(),
                    error => error,
                })?;
        if constructor && signature.result != crate::MethodReturnKind::Void {
            return Err(invalid());
        }
        Ok((
            Symbol {
                class_index,
                owner,
                name,
                descriptor,
                nominal: signature.nominal,
            },
            signature,
        ))
    }
    pub(crate) fn constructor_shape(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> Result<crate::MethodValueShape, MethodAccessError> {
        self.unchanged()?;
        self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        self.read_method_symbol(origin, index, false, true, true)?
            .1
            .shape()
    }
    pub(crate) fn constructor_declaration(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> Result<(ClassId, MethodId), MethodAccessError> {
        let symbol = self
            .read_method_symbol(origin, index, false, false, true)?
            .0;
        let resolved = self.resolve_method_common(origin, index, &symbol, false)?;
        Ok((nominal_owner(resolved.owner)?, resolved.method))
    }
    pub fn instance_invocation_shape(
        &mut self,
        origin: ClassId,
        index: u16,
        kind: InstanceInvocationKind,
    ) -> Result<MethodValueShape, MethodAccessError> {
        self.unchanged()?;
        let definition = self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        let interface = match kind {
            InstanceInvocationKind::Virtual => false,
            InstanceInvocationKind::Interface => true,
            InstanceInvocationKind::Special => match definition.constant_pool.get(index) {
                Ok(Constant::Methodref { .. }) => false,
                Ok(Constant::InterfaceMethodref { .. }) if definition.version.major >= 52 => true,
                _ => return Err(MethodAccessError::InvalidConstant { origin, index }),
            },
        };
        self.method_symbol_and_signature(origin, index, interface, true)?
            .1
            .shape()
    }
    fn runtime_package(&self, class: ClassId) -> Result<Package<'a>, MethodAccessError> {
        let registry = self.registry();
        let name = registry
            .class(class)?
            .name()
            .map_err(RegistryError::Parse)?
            .units();
        let end = name.iter().rposition(|unit| *unit == 47).unwrap_or(0);
        Ok((registry.defining_loader(class)?, &name[..end]))
    }
    fn is_signature_polymorphic(&self, method: MethodId) -> Result<bool, MethodAccessError> {
        let registry = self.registry();
        let class = registry.class(method.owner())?;
        let name = class.name().map_err(RegistryError::Parse)?;
        if !(name.equals_ascii("java/lang/invoke/MethodHandle")
            || name.equals_ascii("java/lang/invoke/VarHandle"))
        {
            return Ok(false);
        }
        let member = registry.method(method)?;
        if member.access_flags & 0x180 != 0x180 {
            return Ok(false);
        }
        let descriptor = class
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(RegistryError::Parse)?;
        let parsed = parse_method_descriptor(descriptor.units()).map_err(RegistryError::Parse)?;
        Ok(
            matches!(parsed.parameters.as_slice(), [FieldType::Array { dimensions: 1, component }] if matches!(component.as_ref(), FieldType::Object(name) if name.equals_ascii("java/lang/Object"))),
        )
    }
    fn declared_method(
        &mut self,
        class: ClassId,
        symbol: &Symbol<'_>,
        polymorphic: bool,
    ) -> Result<Option<MethodId>, MethodAccessError> {
        let registry = self.registry();
        let definition = registry.class(class)?;
        let mut matching_name = 0usize;
        let mut named = None;
        let mut exact = None;
        for (index, member) in definition.methods.iter().enumerate() {
            self.step()?;
            if definition
                .constant_pool
                .utf8(member.name_index)
                .map_err(RegistryError::Parse)?
                != symbol.name
            {
                continue;
            }
            matching_name = matching_name
                .checked_add(1)
                .ok_or(FieldAccessError::InvalidState("method name count overflow"))?;
            let method = MethodId {
                owner: class,
                index,
            };
            named = Some(method);
            if definition
                .constant_pool
                .utf8(member.descriptor_index)
                .map_err(RegistryError::Parse)?
                == symbol.descriptor
                && exact.is_none()
            {
                exact = Some(method);
            }
        }
        if polymorphic && matching_name == 1 {
            if let Some(method) = named {
                if self.is_signature_polymorphic(method)? {
                    return Err(MethodAccessError::UnsupportedSignaturePolymorphic { method });
                }
            }
        }
        Ok(exact)
    }
    // Each edge is charged before growing the pending work list, even when its
    // target will subsequently be deduplicated. No metadata is cloned.
    fn enqueue_interfaces(
        &mut self,
        class: ClassId,
        work: &mut Vec<ClassId>,
        reverse: bool,
    ) -> Result<(), MethodAccessError> {
        let len = self.heap().instance_graph_node(class)?.interfaces.len();
        for edge in 0..len {
            self.step()?;
            let index = if reverse { len - 1 - edge } else { edge };
            let target = self
                .heap()
                .instance_graph_node(class)?
                .interfaces
                .get(index)
                .copied()
                .ok_or(FieldAccessError::InvalidState("interface edge disappeared"))?;
            reserve(work, 1)?;
            work.push(target);
        }
        Ok(())
    }
    fn interface_roots(&mut self, owner: ClassId) -> Result<Vec<ClassId>, MethodAccessError> {
        let mut work = Vec::new();
        let mut current = Some(owner);
        for _ in 0..self.heap().retained_layout_classes() {
            let Some(class) = current else { break };
            self.step()?;
            self.enqueue_interfaces(class, &mut work, false)?;
            let node = self.heap().instance_graph_node(class)?;
            // Object is a structural dependency of an interface, not a
            // superinterface and not a source of inherited interface methods.
            current = if node.flags & 0x200 != 0 {
                None
            } else {
                node.superclass
            };
        }
        if current.is_some() {
            return Err(FieldAccessError::InvalidState("cyclic interface root ancestry").into());
        }
        work.reverse();
        Ok(work)
    }
    fn interface_ancestry(
        &mut self,
        mut work: Vec<ClassId>,
        ancestor: ClassId,
    ) -> Result<bool, MethodAccessError> {
        let mut visited = HashSet::new();
        while let Some(interface) = work.pop() {
            if visited.contains(&interface) {
                continue;
            }
            self.step()?;
            reserve_set(&mut visited, 1)?;
            visited.insert(interface);
            if interface == ancestor {
                return Ok(true);
            }
            self.enqueue_interfaces(interface, &mut work, true)?;
        }
        Ok(false)
    }
    fn strict_subinterface(
        &mut self,
        descendant: ClassId,
        ancestor: ClassId,
    ) -> Result<bool, MethodAccessError> {
        self.step()?;
        let mut work = Vec::new();
        self.enqueue_interfaces(descendant, &mut work, true)?;
        self.interface_ancestry(work, ancestor)
    }
    fn interface_candidates(
        &mut self,
        owner: ClassId,
        symbol: &Symbol<'_>,
    ) -> Result<InterfaceCandidates, MethodAccessError> {
        let mut work = self.interface_roots(owner)?;
        let mut visited = HashSet::new();
        let mut declarations = Vec::new();
        while let Some(interface) = work.pop() {
            if visited.contains(&interface) {
                continue;
            }
            self.step()?;
            reserve_set(&mut visited, 1)?;
            visited.insert(interface);
            if let Some(method) = self.declared_method(interface, symbol, false)? {
                let flags = self.registry().method(method)?.access_flags;
                if flags & 0xa == 0 {
                    reserve(&mut declarations, 1)?;
                    declarations.push(Candidate { method, flags });
                }
            }
            self.enqueue_interfaces(interface, &mut work, true)?;
        }
        let mut maximal = Vec::new();
        reserve(&mut maximal, declarations.len())?;
        for candidate in &declarations {
            let mut dominated = false;
            for other in &declarations {
                self.step()?; // candidate-pair comparison, including identity
                if candidate.method == other.method {
                    continue;
                }
                if self.strict_subinterface(other.method.owner(), candidate.method.owner())? {
                    dominated = true;
                    break;
                }
            }
            maximal.push(!dominated);
        }
        Ok(InterfaceCandidates {
            declarations,
            maximal,
        })
    }
    fn resolve_method(
        &mut self,
        origin: ClassId,
        index: u16,
        symbol: &Symbol<'a>,
        interface: bool,
    ) -> Result<Resolved, MethodAccessError> {
        let resolved = self.resolve_method_common(origin, index, symbol, interface)?;
        if self.registry().method(resolved.method)?.access_flags & 8 != 0 {
            return Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange,
            ));
        }
        Ok(resolved)
    }
    // Symbolic lookup, member access and loading constraints are shared by all
    // call kinds. Category checks follow them, without receiver selection here.
    fn resolve_method_common(
        &mut self,
        origin: ClassId,
        index: u16,
        symbol: &Symbol<'a>,
        interface: bool,
    ) -> Result<Resolved, MethodAccessError> {
        if self.is_member_checked() {
            let ticket = match self.member_begin(origin, index)? {
                Ok(value) => {
                    let resolved = crate::loading::member::method_hit(value)?;
                    if matches!(resolved.owner, crate::ReferenceType::Array { .. }) {
                        let universe = self.universe()?;
                        self.validate_reference_type(resolved.owner, universe)?;
                    }
                    return Ok(resolved);
                }
                Err(ticket) => ticket,
            };
            let mut owner = None;
            let result = (|| {
                let resolved = if symbol.owner.units().first() == Some(&91) {
                    let ty = self
                        .resolve_class_mirror_type(origin, symbol.class_index)
                        .map_err(|e| {
                            if matches!(*e, crate::ClassMirrorError::IllegalAccess) {
                                MethodAccessError::Java(MethodJavaError::IllegalAccess)
                            } else {
                                MethodAccessError::ClassMirror(e)
                            }
                        })?;
                    owner = Some(ty);
                    if interface {
                        return Err(MethodAccessError::Java(
                            MethodJavaError::IncompatibleClassChange,
                        ));
                    }
                    let object = self.universe()?.object;
                    let mut resolved = self.resolve_method_owner(origin, symbol, false, object)?;
                    resolved.owner = ty;
                    resolved
                } else {
                    let class = self.resolve_owner(origin, symbol.owner)?;
                    owner = Some(crate::ReferenceType::Class(class));
                    self.resolve_method_owner(origin, symbol, interface, class)?
                };
                if !self.member_emit(
                    origin,
                    resolved.method.owner(),
                    symbol.descriptor.units(),
                    false,
                )? {
                    return Err(MethodAccessError::Java(MethodJavaError::LinkageError));
                }
                Ok(resolved)
            })();
            return self.member_finish_method(ticket, owner, result);
        }
        let owner = self.resolve_owner(origin, symbol.owner)?;
        self.resolve_method_owner(origin, symbol, interface, owner)
    }
    fn resolve_method_owner(
        &mut self,
        origin: ClassId,
        symbol: &Symbol<'a>,
        interface: bool,
        owner: ClassId,
    ) -> Result<Resolved, MethodAccessError> {
        if self.registry().class(owner)?.access_flags & 1 == 0
            && !self.same_package(origin, owner)?
        {
            return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
        }
        if (self.heap().instance_graph_node(owner)?.flags & 0x200 != 0) != interface {
            return Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange,
            ));
        }
        let method = if interface {
            self.step()?;
            if let Some(method) = self.declared_method(owner, symbol, false)? {
                method
            } else {
                let object = self.heap().instance_graph_node(owner)?.superclass.ok_or(
                    FieldAccessError::InvalidState("interface has no Object dependency"),
                )?;
                self.step()?;
                let object_method = self.declared_method(object, symbol, false)?;
                match object_method {
                    Some(method) if self.registry().method(method)?.access_flags & 9 == 1 => method,
                    _ => self
                        .interface_candidates(owner, symbol)?
                        .resolution()
                        .ok_or(MethodAccessError::Java(MethodJavaError::NoSuchMethod))?,
                }
            }
        } else {
            let mut current = Some(owner);
            let mut found = None;
            for _ in 0..self.heap().retained_layout_classes() {
                let Some(class) = current else {
                    break;
                };
                self.step()?;
                if let Some(method) = self.declared_method(class, symbol, true)? {
                    found = Some(method);
                    break;
                }
                current = self.heap().instance_graph_node(class)?.superclass;
            }
            match found {
                Some(method) => method,
                None if current.is_none() => self
                    .interface_candidates(owner, symbol)?
                    .resolution()
                    .ok_or(MethodAccessError::Java(MethodJavaError::NoSuchMethod))?,
                None => {
                    return Err(FieldAccessError::InvalidState(
                        "cyclic method resolution ancestry",
                    )
                    .into());
                }
            }
        };
        let flags = self.registry().method(method)?.access_flags;
        let declaring = method.owner();
        let mut protected_receiver = None;
        if flags & 1 != 0 { /* public */
        } else if flags & 2 != 0 {
            if origin != declaring {
                // Nest membership requires the same runtime package. Checked
                // mode can prove this denial without claiming nest support.
                if self.is_member_checked() && !self.same_package(origin, declaring)? {
                    return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
                }
                return Err(MethodAccessError::UnsupportedNestAccess {
                    origin,
                    owner: declaring,
                });
            }
        } else if flags & 4 != 0 {
            if !self.same_package(origin, declaring)? {
                if !self.subclass(origin, declaring)? {
                    return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
                }
                if flags & 8 == 0 {
                    if !(self.subclass(origin, owner)? || self.subclass(owner, origin)?) {
                        return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
                    }
                    protected_receiver = Some(origin);
                }
            }
        } else if !self.same_package(origin, declaring)? {
            return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
        }
        if !self.is_member_checked()
            && symbol.nominal
            && self.registry().defining_loader(origin)?
                != self.registry().defining_loader(declaring)?
        {
            return Err(MethodAccessError::UnsupportedLoadingConstraint { origin, method });
        }
        Ok(Resolved {
            owner: crate::ReferenceType::Class(owner),
            method,
            protected_receiver,
        })
    }
    /// Resolve a registered ordinary static declaration. This performs no body
    /// preparation, receiver dispatch or class initialization.
    pub fn resolve_static(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> Result<MethodId, MethodAccessError> {
        self.unchanged()?;
        let definition = self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        let interface = match definition.constant_pool.get(index) {
            Ok(Constant::Methodref { .. }) => false,
            Ok(Constant::InterfaceMethodref { .. }) if definition.version.major >= 52 => true,
            _ => return Err(MethodAccessError::InvalidConstant { origin, index }),
        };
        let symbol = self.method_symbol(origin, index, interface)?;
        let resolved = self.resolve_method_common(origin, index, &symbol, interface)?;
        if self.registry().method(resolved.method)?.access_flags & 8 == 0 {
            return Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange,
            ));
        }
        Ok(resolved.method)
    }
    fn receiver_chain(&mut self, mut class: ClassId) -> Result<Vec<ClassId>, MethodAccessError> {
        let mut chain = Vec::new();
        for _ in 0..self.heap().retained_layout_classes() {
            self.step()?;
            reserve(&mut chain, 1)?;
            chain.push(class);
            match self.heap().instance_graph_node(class)?.superclass {
                Some(parent) => class = parent,
                None => return Ok(chain),
            }
        }
        Err(FieldAccessError::InvalidState("cyclic virtual receiver ancestry").into())
    }
    fn select_override(
        &mut self,
        resolved: MethodId,
        actual: ClassId,
        symbol: &Symbol<'_>,
    ) -> Result<MethodId, MethodAccessError> {
        let chain = self.receiver_chain(actual)?;
        let private = self.registry().method(resolved)?.access_flags & 2 != 0;
        let interface = self.heap().instance_graph_node(resolved.owner())?.flags & 0x200 != 0;
        let mut selected = resolved;
        let mut reached = interface;
        let mut open = self.registry().method(resolved)?.access_flags & 5 != 0;
        let mut packages = HashSet::new();
        reserve_set(&mut packages, 1)?;
        packages.insert(self.runtime_package(resolved.owner())?);
        let mut public_final = None;
        let mut package_finals = HashMap::new();
        for class in chain.into_iter().rev() {
            let Some(method) = self.declared_method(class, symbol, false)? else {
                continue;
            };
            if method == resolved {
                reached = true;
            }
            let flags = self.registry().method(method)?.access_flags;
            if flags & 0xa != 0 {
                continue;
            } // static/private declarations do not override
            self.step()?; // matching-declaration override/finality relation work
            let package = self.runtime_package(class)?;
            if let Some(overridden) = public_final.or_else(|| package_finals.get(&package).copied())
            {
                return Err(MethodAccessError::InvalidOverride { method, overridden });
            }
            if flags & 16 != 0 {
                if flags & 5 != 0 {
                    public_final = Some(method);
                } else {
                    reserve_map(&mut package_finals, 1)?;
                    package_finals.insert(package, method);
                }
            }
            if private || !reached || method == resolved {
                continue;
            }
            if interface || open || packages.contains(&package) {
                selected = method;
                if !packages.contains(&package) {
                    reserve_set(&mut packages, 1)?;
                    packages.insert(package);
                }
                if flags & 5 != 0 {
                    open = true;
                }
            }
        }
        if !reached {
            return Err(FieldAccessError::InvalidState(
                "resolved method is outside receiver ancestry",
            )
            .into());
        }
        if interface && !private && selected == resolved {
            return self.interface_candidates(actual, symbol)?.selection();
        }
        Ok(selected)
    }
    /// Select one registered body using the context's existing graph, bindings and budgets.
    /// This does not prepare Code, execute native bodies or establish verification.
    pub fn select_virtual(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
    ) -> Result<MethodId, MethodAccessError> {
        self.select_method(origin, index, receiver, false)
    }
    /// Resolve an InterfaceMethodref and select a body using the same bounded
    /// admitted graph as virtual calls. Code preparation remains a separate step.
    pub fn select_interface(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
    ) -> Result<MethodId, MethodAccessError> {
        self.select_method(origin, index, receiver, true)
    }
    fn special_start(
        &mut self,
        origin: ClassId,
        owner: ClassId,
    ) -> Result<ClassId, MethodAccessError> {
        if origin == owner {
            return Ok(owner);
        }
        self.step()?;
        let origin_interface = self.heap().instance_graph_node(origin)?.flags & 0x200 != 0;
        self.step()?;
        let owner_interface = self.heap().instance_graph_node(owner)?.flags & 0x200 != 0;
        if !origin_interface && !owner_interface {
            if self.subclass(origin, owner)? {
                // Java SE 21 treats ACC_SUPER as set, including old classfiles
                // whose raw stored flag is clear. The actual receiver is absent
                // from this choice of lookup origin.
                return self.heap().instance_graph_node(origin)?.superclass.ok_or(
                    FieldAccessError::InvalidState("strict superclass without a direct parent")
                        .into(),
                );
            }
        } else if owner_interface {
            let len = self.heap().instance_graph_node(origin)?.interfaces.len();
            for index in 0..len {
                self.step()?;
                let interface = self
                    .heap()
                    .instance_graph_node(origin)?
                    .interfaces
                    .get(index)
                    .ok_or(FieldAccessError::InvalidState(
                        "special owner edge disappeared",
                    ))?;
                if *interface == owner {
                    return Ok(owner);
                }
            }
        } else {
            self.step()?; // interface-origin Object dependency edge comparison
            if self.heap().instance_graph_node(origin)?.superclass == Some(owner) {
                return Ok(owner);
            }
        }
        Err(MethodAccessError::InvalidSpecialOwner { origin, owner })
    }
    pub(crate) fn require_method_receiver(
        &mut self,
        actual: ClassId,
        expected: ClassId,
    ) -> Result<(), MethodAccessError> {
        self.step()?;
        let interface = self.heap().instance_graph_node(expected)?.flags & 0x200 != 0;
        let compatible = if interface {
            let roots = self.interface_roots(actual)?;
            self.interface_ancestry(roots, expected)?
        } else {
            self.subclass(actual, expected)?
        };
        if compatible {
            Ok(())
        } else {
            Err(FieldAccessError::InvalidReceiver { expected, actual }.into())
        }
    }
    fn special_lookup(
        &mut self,
        start: ClassId,
        symbol: &Symbol<'_>,
    ) -> Result<MethodId, MethodAccessError> {
        let mut current = Some(start);
        for _ in 0..self.heap().retained_layout_classes() {
            let Some(class) = current else { break };
            self.step()?;
            if let Some(method) = self.declared_method(class, symbol, false)? {
                if self.registry().method(method)?.access_flags & 8 == 0 {
                    return Ok(method);
                }
            }
            let node = self.heap().instance_graph_node(class)?;
            if node.flags & 0x200 != 0 {
                let object = node.superclass.ok_or(FieldAccessError::InvalidState(
                    "special interface has no Object dependency",
                ))?;
                self.step()?;
                if let Some(method) = self.declared_method(object, symbol, false)? {
                    if self.registry().method(method)?.access_flags & 9 == 1 {
                        return Ok(method);
                    }
                }
                current = None;
                break;
            }
            current = node.superclass;
        }
        if current.is_some() {
            return Err(FieldAccessError::InvalidState("cyclic special selection ancestry").into());
        }
        self.interface_candidates(start, symbol)?.selection()
    }
    /// Resolve an ordinary nonconstructor special call and search its prescribed
    /// type, independently of receiver overrides. No constructor state is implied.
    pub fn select_special(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
    ) -> Result<MethodId, MethodAccessError> {
        self.unchanged()?;
        let definition = self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        let interface = match definition.constant_pool.get(index) {
            Ok(Constant::Methodref { .. }) => false,
            Ok(Constant::InterfaceMethodref { .. }) if definition.version.major >= 52 => true,
            _ => return Err(MethodAccessError::InvalidConstant { origin, index }),
        };
        let symbol = self.method_symbol(origin, index, interface)?;
        let resolved = self.resolve_method(origin, index, &symbol, interface)?;
        let owner = nominal_owner(resolved.owner)?;
        let start = self.special_start(origin, owner)?;
        if receiver.is_null() {
            return Err(MethodAccessError::Java(MethodJavaError::NullPointer));
        }
        let actual = self.heap().instance_class(receiver)?;
        self.require_method_receiver(actual, origin)?;
        self.require_method_receiver(actual, owner)?;
        let selected = self.special_lookup(start, &symbol)?;
        self.require_method_receiver(actual, selected.owner())?;
        if self.registry().method(selected)?.access_flags & 0x400 != 0 {
            return Err(MethodAccessError::Java(MethodJavaError::AbstractMethod));
        }
        self.member_selected(
            origin,
            index,
            resolved.method,
            selected,
            symbol.descriptor.units(),
        )?;
        self.selected_reference_profile(selected, &symbol, receiver)?;
        Ok(selected)
    }
    fn selected_reference_profile(
        &mut self,
        method: MethodId,
        symbol: &Symbol<'_>,
        receiver: Reference,
    ) -> Result<(), MethodAccessError> {
        let definition = self.registry().class(method.owner())?;
        let member = self.registry().method(method)?;
        if definition
            .constant_pool
            .utf8(member.name_index)
            .map_err(RegistryError::Parse)?
            != symbol.name
            || definition
                .constant_pool
                .utf8(member.descriptor_index)
                .map_err(RegistryError::Parse)?
                != symbol.descriptor
        {
            return Err(FieldAccessError::InvalidState(
                "selected method differs from symbolic name/descriptor",
            )
            .into());
        }
        self.check_method_reference_profile(method, Some(receiver))
    }
    fn select_method(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
        interface: bool,
    ) -> Result<MethodId, MethodAccessError> {
        self.unchanged()?;
        self.registry().class(origin)?;
        self.heap().instance_graph_node(origin)?;
        let symbol = self.method_symbol(origin, index, interface)?;
        let resolved = self.resolve_method(origin, index, &symbol, interface)?;
        if receiver.is_null() {
            return Err(MethodAccessError::Java(MethodJavaError::NullPointer));
        }
        if self.class_sites_enabled() && self.get_class_signature(resolved.method)? {
            let actual = self
                .reference_type(receiver)?
                .ok_or(MethodAccessError::Java(MethodJavaError::NullPointer))?;
            if matches!(actual, crate::ReferenceType::Array { .. })
                || matches!(resolved.owner, crate::ReferenceType::Array { .. })
            {
                if !self.is_assignable(actual, resolved.owner)? {
                    return Err(if interface {
                        MethodAccessError::Java(MethodJavaError::IncompatibleClassChange)
                    } else {
                        FieldAccessError::InvalidState(
                            "array getClass receiver differs from symbolic owner",
                        )
                        .into()
                    });
                }
                // A nominal interface must really be implemented, even for an
                // ordinary receiver: runtime assignment is stricter than verification.
                self.member_selected(
                    origin,
                    index,
                    resolved.method,
                    resolved.method,
                    symbol.descriptor.units(),
                )?;
                return Ok(resolved.method);
            }
        }
        let owner = nominal_owner(resolved.owner)?;
        let actual = self.heap().instance_class(receiver)?;
        if interface {
            let roots = self.interface_roots(actual)?;
            if !self.interface_ancestry(roots, owner)? {
                return Err(MethodAccessError::Java(
                    MethodJavaError::IncompatibleClassChange,
                ));
            }
        } else if !self.subclass(actual, owner)? {
            return Err(FieldAccessError::InvalidReceiver {
                expected: owner,
                actual,
            }
            .into());
        }
        if let Some(expected) = resolved.protected_receiver {
            if !self.subclass(actual, expected)? {
                return Err(FieldAccessError::InvalidReceiver { expected, actual }.into());
            }
        }
        let selected = self.select_override(resolved.method, actual, &symbol)?;
        let flags = self.registry().method(selected)?.access_flags;
        if interface && flags & 3 == 0 {
            return Err(MethodAccessError::Java(MethodJavaError::IllegalAccess));
        }
        if flags & 0x400 != 0 {
            return Err(MethodAccessError::Java(MethodJavaError::AbstractMethod));
        }
        self.member_selected(
            origin,
            index,
            resolved.method,
            selected,
            symbol.descriptor.units(),
        )?;
        self.selected_reference_profile(selected, &symbol, receiver)?;
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FieldResolutionOptions, HeapOptions, HierarchyOptions, InstanceOptions, ManagedHeap,
        RegistryOptions, SameLoaderResolver, build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    use std::error::Error;

    fn fixture() -> (crate::DefinitionRegistry, ClassId) {
        let mut registry = crate::DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 7];
        for (tag, data) in [
            (1, b"java/lang/Object".as_slice()),
            (7, &[0, 1]),
            (1, b"m".as_slice()),
            (1, b"()I".as_slice()),
            (12, &[0, 3, 0, 4]),
            (10, &[0, 2, 0, 5]),
        ] {
            bytes.push(tag);
            if tag == 1 {
                bytes.extend((data.len() as u16).to_be_bytes());
            }
            bytes.extend(data);
        }
        for word in [0x21_u16, 2, 0, 0, 0, 1, 0x101, 3, 4, 0, 0] {
            bytes.extend(word.to_be_bytes());
        }
        let class = registry
            .define(
                loader,
                &"java/lang/Object".encode_utf16().collect::<Vec<_>>(),
                &bytes,
                ParseOptions::default(),
            )
            .unwrap();
        (registry, class)
    }
    #[test]
    fn deterministic_temporary_reservation_overflows_preserve_existing_values() {
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        let loader = LoaderId {
            registry: 1,
            index: 0,
        };
        let mut values = vec![class];
        let mut set = HashSet::from([class]);
        let mut map = HashMap::from([(
            (loader, &[65_u16][..]),
            MethodId {
                owner: class,
                index: 0,
            },
        )]);
        for result in [
            reserve(&mut values, usize::MAX),
            reserve_set(&mut set, usize::MAX),
            reserve_map(&mut map, usize::MAX),
        ] {
            assert_eq!(result.unwrap_err(), allocation());
        }
        assert_eq!(values, [class]);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&class));
        assert_eq!(map.len(), 1);
    }
    #[test]
    fn private_forged_method_and_origin_handles_remain_checked() {
        let (registry, class) = fixture();
        let plan = build_hierarchy(
            &registry,
            class,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        let receiver = heap.allocate_instance(class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let forged_origin = ClassId {
            registry: class.registry,
            index: usize::MAX,
        };
        assert!(matches!(
            context.select_virtual(forged_origin, 6, receiver),
            Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
        ));
        assert!(matches!(
            context.select_interface(forged_origin, 6, receiver),
            Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
        ));
        assert!(matches!(
            context.select_special(forged_origin, 6, receiver),
            Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
        ));
        let symbol = context.method_symbol(class, 6, false).unwrap();
        assert!(matches!(
            context.special_lookup(forged_origin, &symbol),
            Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
        ));
        assert!(matches!(
            context.require_method_receiver(class, forged_origin),
            Err(MethodAccessError::Context(FieldAccessError::Instance(_)))
        ));
        let forged = MethodId {
            owner: class,
            index: usize::MAX,
        };
        assert!(matches!(
            context.select_override(forged, class, &symbol),
            Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
        ));
        assert_eq!(
            context.select_virtual(class, 6, receiver).unwrap(),
            MethodId {
                owner: class,
                index: 0
            }
        );
    }
    #[test]
    fn static_resolution_checks_forged_origins_before_cp_and_preserves_work() {
        let (registry, class) = fixture();
        let plan = build_hierarchy(
            &registry,
            class,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut context = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let before = context.lookup_steps();
        for origin in [
            ClassId {
                registry: class.registry,
                index: usize::MAX,
            },
            ClassId {
                registry: 0,
                index: class.index,
            },
        ] {
            assert!(matches!(
                context.resolve_static(origin, 0),
                Err(MethodAccessError::Context(FieldAccessError::Registry(_)))
            ));
            assert_eq!(context.lookup_steps(), before);
        }
        for index in [0, 1, u16::MAX] {
            assert_eq!(
                context.resolve_static(class, index),
                Err(MethodAccessError::InvalidConstant {
                    origin: class,
                    index
                })
            );
            assert_eq!(context.lookup_steps(), before);
        }
        assert_eq!(
            context.resolve_static(class, 6),
            Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange
            ))
        );
        assert_eq!(context.lookup_steps() - before, 2);
    }
    #[test]
    fn context_java_errors_are_not_implicitly_reclassified_and_keep_sources() {
        let inner = FieldAccessError::Java(crate::FieldJavaError::IllegalAccess);
        let wrapped = MethodAccessError::from(inner.clone());
        assert_eq!(wrapped, MethodAccessError::Context(inner));
        assert!(wrapped.source().is_some());
        let allocation = allocation();
        assert!(allocation.source().unwrap().source().is_some());
        assert!(allocation.to_string().contains("context"));
        assert!(
            MethodAccessError::Java(MethodJavaError::NoSuchMethod)
                .source()
                .is_none()
        );
        let owner = ClassId {
            registry: 1,
            index: 0,
        };
        let error = MethodAccessError::InvalidSpecialOwner {
            origin: owner,
            owner,
        };
        assert!(error.to_string().contains("special-call owner"));
        assert!(error.source().is_none());
    }
    #[test]
    fn resolution_witnesses_do_not_replace_maximal_receiver_selection() {
        let method = |index| MethodId {
            owner: ClassId { registry: 1, index },
            index: 0,
        };
        let candidates = InterfaceCandidates {
            declarations: vec![
                Candidate {
                    method: method(0),
                    flags: 1,
                },
                Candidate {
                    method: method(1),
                    flags: 0x401,
                },
            ],
            maximal: vec![false, true],
        };
        assert_eq!(candidates.resolution(), Some(method(0)));
        assert_eq!(
            candidates.selection(),
            Err(MethodAccessError::Java(MethodJavaError::AbstractMethod))
        );
        let candidates = InterfaceCandidates {
            declarations: vec![
                Candidate {
                    method: method(0),
                    flags: 0x401,
                },
                Candidate {
                    method: method(1),
                    flags: 1,
                },
                Candidate {
                    method: method(2),
                    flags: 1,
                },
            ],
            maximal: vec![true, true, true],
        };
        assert_eq!(candidates.resolution(), Some(method(0)));
        assert_eq!(
            candidates.selection(),
            Err(MethodAccessError::Java(
                MethodJavaError::IncompatibleClassChange
            ))
        );
        let candidates = InterfaceCandidates {
            declarations: vec![
                Candidate {
                    method: method(0),
                    flags: 0x401,
                },
                Candidate {
                    method: method(1),
                    flags: 1,
                },
            ],
            maximal: vec![true, true],
        };
        assert_eq!(candidates.resolution(), Some(method(1)));
        assert_eq!(candidates.selection(), Ok(method(1)));
        let empty = InterfaceCandidates {
            declarations: Vec::new(),
            maximal: Vec::new(),
        };
        assert_eq!(empty.resolution(), None);
        assert_eq!(
            empty.selection(),
            Err(MethodAccessError::Java(MethodJavaError::AbstractMethod))
        );
    }
}

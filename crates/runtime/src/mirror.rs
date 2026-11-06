//! Canonical VM Class objects. The domain owns symbolic outcomes; only a bound
//! heap owns references. Neither service changes Java initialization state.
use crate::heap::{Entry, EntryPayload, LIVE_GENERATION, reserve_entries};
use crate::*;
use crabspot_classfile::Constant;
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClassMirrorLimits {
    pub max_mirrors: u64,
    pub max_class_sites: u64,
}
impl Default for ClassMirrorLimits {
    fn default() -> Self {
        Self {
            max_mirrors: 16_384,
            max_class_sites: 262_144,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassMirrorResource {
    Mirrors,
    ClassSites,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassSiteOutcome {
    Resolved(ReferenceType),
    IllegalAccess,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassMirrorError {
    Disabled,
    PolicyConflict,
    InvalidBinding(&'static str),
    InvalidState(&'static str),
    Limit {
        resource: ClassMirrorResource,
        limit: u64,
        required: u64,
    },
    Overflow {
        resource: ClassMirrorResource,
    },
    AllocationFailure,
    IllegalAccess,
    Context(Box<FieldAccessError>),
    Type(Box<TypeAccessError>),
    Instance(InstanceError),
    Registry(RegistryError),
}
impl fmt::Display for ClassMirrorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(e) => e.fmt(f),
            Self::Type(e) => e.fmt(f),
            Self::Instance(e) => e.fmt(f),
            Self::Registry(e) => e.fmt(f),
            other => write!(f, "class mirror: {other:?}"),
        }
    }
}
impl std::error::Error for ClassMirrorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(e) => Some(e.as_ref()),
            Self::Type(e) => Some(e.as_ref()),
            Self::Instance(e) => Some(e),
            Self::Registry(e) => Some(e),
            _ => None,
        }
    }
}
macro_rules! from_error {
    ($t:ty,$v:ident) => {
        impl From<$t> for Box<ClassMirrorError> {
            fn from(e: $t) -> Self {
                Box::new(ClassMirrorError::$v(e.into()))
            }
        }
    };
}
from_error!(FieldAccessError, Context);
from_error!(TypeAccessError, Type);
from_error!(InstanceError, Instance);
from_error!(RegistryError, Registry);
type R<T> = Result<T, Box<ClassMirrorError>>;
fn error(e: ClassMirrorError) -> Box<ClassMirrorError> {
    Box::new(e)
}
fn next(old: u64, limit: u64, resource: ClassMirrorResource) -> R<u64> {
    let required = old
        .checked_add(1)
        .ok_or_else(|| error(ClassMirrorError::Overflow { resource }))?;
    if required > limit {
        return Err(error(ClassMirrorError::Limit {
            resource,
            limit,
            required,
        }));
    }
    Ok(required)
}
pub(crate) struct ClassSites {
    limit: u64,
    pub(crate) rows: HashMap<(ClassId, u16), ClassSiteOutcome>,
    #[cfg(test)]
    fail_reserve: bool,
}
impl ClassSites {
    fn reserve(&mut self) -> R<()> {
        #[cfg(test)]
        if self.fail_reserve {
            return Err(error(ClassMirrorError::AllocationFailure));
        }
        self.rows
            .try_reserve(1)
            .map_err(|_| error(ClassMirrorError::AllocationFailure))
    }
    fn validate(&self) -> R<()> {
        if self.rows.len() as u64 > self.limit {
            return Err(error(ClassMirrorError::InvalidState(
                "class-site count exceeds policy",
            )));
        }
        Ok(())
    }
}
#[derive(Debug)]
pub(crate) struct Mirrors {
    pub(crate) class: ClassId,
    registry: u64,
    universe: ReferenceTypeUniverse,
    classes: usize,
    limit: u64,
    index: HashMap<ReferenceType, HeapRef>,
    roots: Vec<HeapRef>,
    #[cfg(test)]
    fail_reserve_after: Option<usize>,
}
impl Mirrors {
    fn validate(&self) -> R<()> {
        if self.roots.len() != self.index.len() || self.roots.len() as u64 > self.limit {
            return Err(error(ClassMirrorError::InvalidState(
                "mirror index/root counts differ",
            )));
        }
        Ok(())
    }
    fn reserving(&mut self) -> R<()> {
        #[cfg(test)]
        if let Some(left) = &mut self.fail_reserve_after {
            if *left == 0 {
                return Err(error(ClassMirrorError::AllocationFailure));
            }
            *left -= 1;
        }
        Ok(())
    }
}
impl ManagedHeap {
    pub fn bind_class_mirrors(
        &mut self,
        registry: &DefinitionRegistry,
        class: ClassId,
        max_mirrors: u64,
    ) -> R<()> {
        let universe = self.reference_type_universe().ok_or_else(|| {
            error(ClassMirrorError::InvalidBinding(
                "reference universe is absent",
            ))
        })?;
        let definition = registry.class(class)?;
        let node = self.instance_graph_node(class)?;
        if self.instance_graph_registry()? != Some(registry.identity())
            || registry.defining_loader(class)? != universe.bootstrap_loader
            || !definition
                .name()
                .map_err(RegistryError::Parse)?
                .equals_ascii("java/lang/Class")
            || definition.access_flags & (1 | 16 | 0x200 | 0x400) != 17
            || node.superclass != Some(universe.object)
        {
            return Err(error(ClassMirrorError::InvalidBinding(
                "Class role must be public final ordinary bootstrap Class directly extending Object",
            )));
        }
        if let Some(m) = &self.mirrors {
            if m.class == class
                && m.registry == registry.identity()
                && m.universe == universe
                && m.classes == self.retained_layout_classes()
                && m.limit == max_mirrors
            {
                return Ok(());
            }
            return Err(error(ClassMirrorError::PolicyConflict));
        }
        if !self.entries.is_empty() || self.initialization_claim.is_some() {
            return Err(error(ClassMirrorError::InvalidBinding(
                "first Class binding requires a pristine unclaimed heap",
            )));
        }
        self.mirrors = Some(Mirrors {
            class,
            registry: registry.identity(),
            universe,
            classes: self.retained_layout_classes(),
            limit: max_mirrors,
            index: HashMap::new(),
            roots: Vec::new(),
            #[cfg(test)]
            fail_reserve_after: None,
        });
        Ok(())
    }
    pub fn class_mirror_count(&self) -> u64 {
        self.mirrors.as_ref().map_or(0, |m| m.roots.len() as u64)
    }
    pub fn class_mirror_roots(&self) -> impl ExactSizeIterator<Item = HeapRef> + '_ {
        self.mirrors
            .as_ref()
            .map_or(&[][..], |m| m.roots.as_slice())
            .iter()
            .copied()
    }
    pub fn mirror_represented_type(
        &self,
        reference: Reference,
    ) -> Result<Option<ReferenceType>, HeapError> {
        if reference.is_null() {
            return Ok(None);
        }
        Ok(match self.entry(reference)?.payload {
            EntryPayload::Mirror(ty) => Some(ty),
            _ => None,
        })
    }
    /// The interpreter calls this for preselected/direct instance roots as well
    /// as ordinary invocation. It does not authorize a native implementation.
    pub fn check_class_mirror_method_entry(
        &self,
        method: MethodId,
        receiver: Reference,
    ) -> Result<(), InstanceError> {
        if self
            .mirrors
            .as_ref()
            .is_some_and(|m| m.class == method.owner())
        {
            self.reject_opaque(receiver)?;
        }
        Ok(())
    }
}
fn class_tag(registry: &DefinitionRegistry, origin: ClassId, index: u16) -> R<()> {
    let definition = registry.class(origin)?;
    match definition.constant_pool.get(index) {
        Ok(Constant::Class { name_index })
            if definition.constant_pool.utf8(*name_index).is_ok() =>
        {
            Ok(())
        }
        _ => Err(TypeAccessError::InvalidConstant { origin, index }.into()),
    }
}
impl LoadingDomain {
    pub fn enable_class_mirror_sites(&mut self, max_class_sites: u64) -> R<()> {
        let store = self
            .members
            .as_mut()
            .ok_or_else(|| error(ClassMirrorError::Disabled))?;
        if let Some(s) = &store.class_sites {
            if s.limit != max_class_sites {
                return Err(error(ClassMirrorError::PolicyConflict));
            }
        } else {
            store.class_sites = Some(ClassSites {
                limit: max_class_sites,
                rows: HashMap::new(),
                #[cfg(test)]
                fail_reserve: false,
            });
        }
        Ok(())
    }
    pub fn class_site_count(&self) -> u64 {
        self.members
            .as_ref()
            .and_then(|s| s.class_sites.as_ref())
            .map_or(0, |s| s.rows.len() as u64)
    }
    pub fn class_site(&self, origin: ClassId, index: u16) -> R<Option<ClassSiteOutcome>> {
        class_tag(self.registry(), origin, index)?;
        let Some(s) = self.members.as_ref().and_then(|s| s.class_sites.as_ref()) else {
            return Ok(None);
        };
        s.validate()?;
        Ok(s.rows.get(&(origin, index)).copied())
    }
}
impl FieldResolutionContext<'_> {
    pub(crate) fn class_sites_enabled(&self) -> bool {
        self.member
            .as_ref()
            .is_some_and(|a| a.store.class_sites.is_some())
    }
    pub fn check_class_mirror_profile(&self) -> R<()> {
        self.unchanged()?;
        if !self.class_sites_enabled() {
            return Err(error(ClassMirrorError::Disabled));
        }
        let m = self
            .heap
            .mirrors
            .as_ref()
            .ok_or_else(|| error(ClassMirrorError::Disabled))?;
        if m.registry != self.registry.identity()
            || m.universe != self.universe()?
            || m.classes != self.heap.retained_layout_classes()
        {
            return Err(error(ClassMirrorError::InvalidBinding(
                "mirror context affinity differs",
            )));
        }
        m.validate()?;
        self.registry.class(m.class)?;
        self.heap.instance_graph_node(m.class)?;
        Ok(())
    }
    pub fn resolve_class_mirror_type(&mut self, origin: ClassId, index: u16) -> R<ReferenceType> {
        self.unchanged()?;
        class_tag(self.registry, origin, index)?;
        self.heap.instance_graph_node(origin)?;
        if !self.class_sites_enabled() {
            return Err(error(ClassMirrorError::Disabled));
        }
        self.step()?;
        let s = self
            .member
            .as_ref()
            .and_then(|a| a.store.class_sites.as_ref())
            .ok_or_else(|| error(ClassMirrorError::Disabled))?;
        s.validate()?;
        if let Some(outcome) = s.rows.get(&(origin, index)).copied() {
            return match outcome {
                ClassSiteOutcome::IllegalAccess => Err(error(ClassMirrorError::IllegalAccess)),
                ClassSiteOutcome::Resolved(ty) => {
                    self.validate_reference_type(ty, self.universe()?)?;
                    Ok(ty)
                }
            };
        }
        next(
            s.rows.len() as u64,
            s.limit,
            ClassMirrorResource::ClassSites,
        )?;
        self.step()?;
        self.member
            .as_mut()
            .and_then(|a| a.store.class_sites.as_mut())
            .ok_or_else(|| error(ClassMirrorError::Disabled))?
            .reserve()?;
        let result = self.resolve_reference_type(origin, index);
        let outcome = match &result {
            Ok(ty) => Some(ClassSiteOutcome::Resolved(*ty)),
            Err(TypeAccessError::Java(TypeJavaError::IllegalAccess)) => {
                Some(ClassSiteOutcome::IllegalAccess)
            }
            _ => None,
        };
        if let Some(outcome) = outcome {
            let s = self
                .member
                .as_mut()
                .and_then(|a| a.store.class_sites.as_mut())
                .ok_or_else(|| error(ClassMirrorError::Disabled))?;
            s.validate()?;
            next(
                s.rows.len() as u64,
                s.limit,
                ClassMirrorResource::ClassSites,
            )?;
            if s.rows.contains_key(&(origin, index)) || s.rows.capacity() <= s.rows.len() {
                return Err(error(ClassMirrorError::InvalidState(
                    "class-site reservation changed",
                )));
            }
            s.rows.insert((origin, index), outcome);
        }
        match result {
            Ok(ty) => Ok(ty),
            Err(TypeAccessError::Java(TypeJavaError::IllegalAccess)) => {
                Err(error(ClassMirrorError::IllegalAccess))
            }
            Err(e) => Err(e.into()),
        }
    }
    pub fn class_mirror_for_type(&mut self, ty: ReferenceType) -> R<Reference> {
        self.check_class_mirror_profile()?;
        self.validate_reference_type(ty, self.universe()?)?;
        self.step()?;
        let m = self
            .heap
            .mirrors
            .as_ref()
            .ok_or_else(|| error(ClassMirrorError::Disabled))?;
        if let Some(handle) = m.index.get(&ty).copied() {
            self.heap
                .validate_root(handle)
                .map_err(InstanceError::from)?;
            let (heap, index, generation) = handle.components();
            let reference = Reference::issued(heap, index, generation);
            if self
                .heap
                .mirror_represented_type(reference)
                .map_err(InstanceError::from)?
                != Some(ty)
            {
                return Err(error(ClassMirrorError::InvalidState(
                    "mirror index differs from payload",
                )));
            }
            return Ok(reference);
        }
        next(m.roots.len() as u64, m.limit, ClassMirrorResource::Mirrors)?;
        let state = self
            .heap
            .instances
            .as_ref()
            .ok_or(InstanceError::Disabled)?;
        let count = state.count.checked_add(1).ok_or_else(|| {
            Box::<ClassMirrorError>::from(InstanceError::ResourceLimit {
                resource: InstanceResource::Instances,
                limit: state.options.max_instances,
                required: usize::MAX,
            })
        })?;
        if count > state.options.max_instances {
            return Err(InstanceError::ResourceLimit {
                resource: InstanceResource::Instances,
                limit: state.options.max_instances,
                required: count,
            }
            .into());
        }
        let index = self.heap.entries.len();
        index
            .checked_add(1)
            .ok_or_else(|| error(ClassMirrorError::InvalidState("arena index exhausted")))?;
        self.step()?;
        self.heap
            .mirrors
            .as_mut()
            .ok_or_else(|| error(ClassMirrorError::Disabled))?
            .reserving()?;
        reserve_entries(&mut self.heap.entries, 1).map_err(InstanceError::from)?;
        let m = self
            .heap
            .mirrors
            .as_mut()
            .ok_or_else(|| error(ClassMirrorError::Disabled))?;
        m.reserving()?;
        m.index
            .try_reserve(1)
            .map_err(|_| error(ClassMirrorError::AllocationFailure))?;
        m.reserving()?;
        m.roots
            .try_reserve(1)
            .map_err(|_| error(ClassMirrorError::AllocationFailure))?;
        let reference = Reference::issued(self.heap.identity, index, LIVE_GENERATION);
        let handle = reference
            .heap_ref()
            .ok_or_else(|| error(ClassMirrorError::InvalidState("minted null")))?;
        let state = self
            .heap
            .instances
            .as_mut()
            .ok_or(InstanceError::Disabled)?;
        // Every lookup/reserve has succeeded. Exclusive, callback-free access
        // now publishes all scalar counters and reserved containers together.
        self.heap.entries.push(Entry {
            generation: LIVE_GENERATION,
            string_backing: None,
            payload: EntryPayload::Mirror(ty),
        });
        m.index.insert(ty, handle);
        m.roots.push(handle);
        state.count = count;
        Ok(reference)
    }
    pub(crate) fn get_class_signature(&self, method: MethodId) -> Result<bool, MethodAccessError> {
        let definition = self.registry.class(method.owner())?;
        let member = self.registry.method(method)?;
        let Some(u) = self.heap.reference_type_universe() else {
            return Ok(false);
        };
        if method.owner() != u.object {
            return Ok(false);
        }
        let name = definition
            .constant_pool
            .utf8(member.name_index)
            .map_err(RegistryError::Parse)?;
        let desc = definition
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(RegistryError::Parse)?;
        Ok(name.units().len() == 8
            && desc.units().len() == 19
            && name.equals_ascii("getClass")
            && desc.equals_ascii("()Ljava/lang/Class;"))
    }
    pub fn is_object_get_class(&mut self, method: MethodId) -> R<bool> {
        self.unchanged()?;
        let definition = self.registry.class(method.owner())?;
        let member = self.registry.method(method)?;
        let Some(u) = self.heap.reference_type_universe() else {
            return Ok(false);
        };
        if method.owner() != u.object {
            return Ok(false);
        }
        let name = definition
            .constant_pool
            .utf8(member.name_index)
            .map_err(RegistryError::Parse)?;
        let desc = definition
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(RegistryError::Parse)?;
        if name.units().len() != 8
            || desc.units().len() != 19
            || !name.equals_ascii("getClass")
            || !desc.equals_ascii("()Ljava/lang/Class;")
        {
            return Ok(false);
        }
        self.step()?;
        if member.access_flags & (1 | 2 | 4 | 8 | 16 | 32 | 0x100 | 0x400) != 0x111 {
            return Err(error(ClassMirrorError::InvalidBinding(
                "Object.getClass has an incompatible native role",
            )));
        }
        // Registry admission owns immutable parsed metadata. The parser rejects
        // Code on every ordinary native method, so native flags establish the
        // no-Code invariant without rescanning arbitrary extension attributes.
        self.check_class_mirror_profile()?;
        let target = self.resolve_reference_span(method.owner(), &desc.units()[2..])?;
        if Some(target)
            != self
                .heap
                .mirrors
                .as_ref()
                .map(|m| ReferenceType::Class(m.class))
        {
            return Err(error(ClassMirrorError::InvalidBinding(
                "Object.getClass return identity differs",
            )));
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "mirror_tests.rs"]
mod tests;

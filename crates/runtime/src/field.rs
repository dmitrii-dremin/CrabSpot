//! Bounded field resolution and access for an explicitly selected unnamed-module profile.
use crate::{
    ClassId, DefinitionRegistry, FieldId, HeapError, HierarchyResolver, InstanceError, LoaderId,
    ManagedHeap, Reference, ReferenceType, RegistryError, ResolveError, TypeAccessError, Value,
};
use crabspot_classfile::{Constant, JavaString};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldResolutionOptions {
    pub max_symbols: usize,
    pub max_symbol_units: usize,
    pub max_lookup_steps: usize,
}
impl Default for FieldResolutionOptions {
    fn default() -> Self {
        Self {
            max_symbols: 16_384,
            max_symbol_units: 1_048_576,
            max_lookup_steps: 1_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldJavaError {
    LinkageError,
    NoSuchField,
    IllegalAccess,
    IncompatibleClassChange,
    NullPointer,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldResource {
    Symbols,
    SymbolUnits,
    LookupSteps,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceFieldMismatch {
    pub field: FieldId,
    pub source: ReferenceType,
    pub target: ReferenceType,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldAccessError {
    MemberResolution(Box<crate::MemberResolutionError>),
    ClassMirror(Box<crate::ClassMirrorError>),
    StringBody(Box<crate::StringBodyError>),
    Java(FieldJavaError),
    Registry(RegistryError),
    Instance(InstanceError),
    ReferenceType(Box<TypeAccessError>),
    IncompatibleReference(Box<ReferenceFieldMismatch>),
    Resolve {
        origin: ClassId,
        name: JavaString,
        error: ResolveError,
    },
    MissingDefinition {
        origin: ClassId,
        name: JavaString,
    },
    NameMismatch {
        requested: JavaString,
        returned: ClassId,
    },
    ConflictingBinding {
        loader: LoaderId,
        name: JavaString,
    },
    InvalidConstant {
        origin: ClassId,
        index: u16,
    },
    UnsupportedNestAccess {
        origin: ClassId,
        owner: ClassId,
    },
    UnsupportedLoadingConstraint {
        origin: ClassId,
        owner: ClassId,
    },
    UnsupportedArrayOwner,
    ContextChanged,
    InvalidState(&'static str),
    InvalidReceiver {
        expected: ClassId,
        actual: ClassId,
    },
    ResourceLimit {
        resource: FieldResource,
        limit: usize,
        required: usize,
    },
}
impl fmt::Display for FieldAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MemberResolution(error) => error.fmt(f),
            Self::ClassMirror(error) => error.fmt(f),
            Self::StringBody(error) => error.fmt(f),
            Self::Java(error) => write!(f, "field access failed: {error:?}"),
            Self::Registry(error) => write!(f, "field registry operation failed: {error}"),
            Self::Instance(error) => write!(f, "field instance operation failed: {error}"),
            Self::ReferenceType(error) => {
                write!(f, "field reference type validation failed: {error}")
            }
            Self::IncompatibleReference(mismatch) => write!(
                f,
                "reference type {:?} is not assignable to {:?} for field {:?}",
                mismatch.source, mismatch.target, mismatch.field
            ),
            Self::Resolve {
                origin,
                name,
                error,
            } => write!(
                f,
                "field owner {} from {origin:?} could not be resolved: {error}",
                name.escaped()
            ),
            Self::MissingDefinition { origin, name } => {
                write!(f, "no field owner {} from {origin:?}", name.escaped())
            }
            Self::NameMismatch {
                requested,
                returned,
            } => write!(
                f,
                "resolver returned {returned:?} for a different name than {}",
                requested.escaped()
            ),
            Self::ConflictingBinding { loader, name } => write!(
                f,
                "conflicting hierarchy binding for {} in {loader:?}",
                name.escaped()
            ),
            Self::InvalidConstant { origin, index } => {
                write!(f, "constant #{index} in {origin:?} is not a valid Fieldref")
            }
            Self::UnsupportedNestAccess { origin, owner } => write!(
                f,
                "private field access from {origin:?} to {owner:?} requires unsupported nest checks"
            ),
            Self::UnsupportedLoadingConstraint { origin, owner } => write!(
                f,
                "reference field access from {origin:?} to {owner:?} requires unsupported loading constraints"
            ),
            Self::UnsupportedArrayOwner => f.write_str("array field owners are unsupported"),
            Self::ContextChanged => {
                f.write_str("the heap or admitted hierarchy changed; construct a new field context")
            }
            Self::InvalidState(reason) => write!(f, "invalid field resolution state: {reason}"),
            Self::InvalidReceiver { expected, actual } => write!(
                f,
                "receiver {actual:?} is not a supported instance of {expected:?}"
            ),
            Self::ResourceLimit {
                resource,
                limit,
                required,
            } => write!(
                f,
                "field {resource:?} limit {limit} exceeded (required {required})"
            ),
        }
    }
}
impl std::error::Error for FieldAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MemberResolution(error) => Some(error.as_ref()),
            Self::ClassMirror(error) => Some(error.as_ref()),
            Self::StringBody(error) => Some(error.as_ref()),
            Self::Registry(error) => Some(error),
            Self::Instance(error) => Some(error),
            Self::ReferenceType(error) => Some(error.as_ref()),
            Self::Resolve { error, .. } => Some(error),
            _ => None,
        }
    }
}
impl From<RegistryError> for FieldAccessError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}
impl From<InstanceError> for FieldAccessError {
    fn from(error: InstanceError) -> Self {
        Self::Instance(error)
    }
}

type Bindings<'a> = HashMap<(LoaderId, &'a [u16]), ClassId>;
fn allocation() -> FieldAccessError {
    InstanceError::Heap(HeapError::AllocationFailure {
        resource: "field resolution metadata",
    })
    .into()
}
fn reserve_vec<T>(values: &mut Vec<T>, additional: usize) -> Result<(), FieldAccessError> {
    values.try_reserve(additional).map_err(|_| allocation())
}
fn reserve_bindings(
    bindings: &mut Bindings<'_>,
    additional: usize,
) -> Result<(), FieldAccessError> {
    bindings.try_reserve(additional).map_err(|_| allocation())
}
fn reserve_visited(
    visited: &mut HashSet<ClassId>,
    additional: usize,
) -> Result<(), FieldAccessError> {
    visited.try_reserve(additional).map_err(|_| allocation())
}
fn increased(
    current: usize,
    amount: usize,
    limit: usize,
    resource: FieldResource,
) -> Result<usize, FieldAccessError> {
    let required = current
        .checked_add(amount)
        .ok_or(FieldAccessError::ResourceLimit {
            resource,
            limit,
            required: usize::MAX,
        })?;
    if required > limit {
        Err(FieldAccessError::ResourceLimit {
            resource,
            limit,
            required,
        })
    } else {
        Ok(required)
    }
}
pub(crate) fn step(
    steps: &mut usize,
    options: FieldResolutionOptions,
) -> Result<(), FieldAccessError> {
    *steps = increased(
        *steps,
        1,
        options.max_lookup_steps,
        FieldResource::LookupSteps,
    )?;
    Ok(())
}
fn bind<'a>(
    bindings: &mut Bindings<'a>,
    units: &mut usize,
    options: FieldResolutionOptions,
    loader: LoaderId,
    name: &'a JavaString,
    class: ClassId,
) -> Result<(), FieldAccessError> {
    bind_units(bindings, units, options, loader, name.units(), class)
}
fn bind_units<'a>(
    bindings: &mut Bindings<'a>,
    units: &mut usize,
    options: FieldResolutionOptions,
    loader: LoaderId,
    name: &'a [u16],
    class: ClassId,
) -> Result<(), FieldAccessError> {
    let key = (loader, name);
    if let Some(existing) = bindings.get(&key) {
        return if *existing == class {
            Ok(())
        } else {
            Err(FieldAccessError::ConflictingBinding {
                loader,
                name: JavaString::from_units(name.to_vec()),
            })
        };
    }
    increased(
        bindings.len(),
        1,
        options.max_symbols,
        FieldResource::Symbols,
    )?;
    let next_units = increased(
        *units,
        name.len(),
        options.max_symbol_units,
        FieldResource::SymbolUnits,
    )?;
    reserve_bindings(bindings, 1)?;
    bindings.insert(key, class);
    *units = next_units;
    Ok(())
}
struct Symbol<'a> {
    owner: &'a JavaString,
    name: &'a JavaString,
    descriptor: &'a JavaString,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Selected {
    pub(crate) symbolic_owner: ClassId,
    pub(crate) field: FieldId,
    pub(crate) protected_receiver: Option<ClassId>,
}

/// A reusable access context. It neither loads definitions nor changes lifecycle state.
pub struct FieldResolutionContext<'a> {
    pub(crate) registry: &'a DefinitionRegistry,
    pub(crate) heap: &'a mut ManagedHeap,
    resolver: Option<&'a mut dyn HierarchyResolver>,
    pub(crate) member: Option<crate::loading::member::Authority<'a>>,
    pub(crate) options: FieldResolutionOptions,
    bindings: Bindings<'a>,
    units: usize,
    pub(crate) steps: usize,
    admitted_classes: usize,
    heap_identity: u64,
    reference_universe: Option<crate::ReferenceTypeUniverse>,
}
impl<'a> FieldResolutionContext<'a> {
    pub(crate) fn from_member_parts(
        registry: &'a DefinitionRegistry,
        heap: &'a mut ManagedHeap,
        authority: crate::loading::member::Authority<'a>,
        options: FieldResolutionOptions,
    ) -> Self {
        let admitted_classes = heap.retained_layout_classes();
        let heap_identity = heap.identity;
        let reference_universe = heap.reference_type_universe();
        Self {
            registry,
            heap,
            resolver: None,
            member: Some(authority),
            options,
            bindings: HashMap::new(),
            units: 0,
            steps: 0,
            admitted_classes,
            heap_identity,
            reference_universe,
        }
    }
    pub fn new_unnamed_modules(
        registry: &'a DefinitionRegistry,
        heap: &'a mut ManagedHeap,
        resolver: &'a mut dyn HierarchyResolver,
        options: FieldResolutionOptions,
    ) -> Result<Self, FieldAccessError> {
        let bound = heap.instance_graph_registry()?;
        if bound.is_none() {
            return Err(FieldAccessError::InvalidState(
                "instance storage must have admitted layouts",
            ));
        }
        if bound != Some(registry.identity()) {
            return Err(InstanceError::ForeignRegistry.into());
        }
        let mut bindings = HashMap::new();
        let mut units = 0;
        let mut steps = 0;
        for class in heap.instance_graph_classes()? {
            step(&mut steps, options)?;
            let definition = registry.class(class)?;
            let loader = registry.defining_loader(class)?;
            bind(
                &mut bindings,
                &mut units,
                options,
                loader,
                definition.name().map_err(RegistryError::Parse)?,
                class,
            )?;
            let node = heap.instance_graph_node(class)?;
            if node.flags != definition.access_flags
                || node.superclass.is_none() != (definition.super_class == 0)
            {
                return Err(FieldAccessError::InvalidState(
                    "admitted class flags or superclass presence differ from immutable metadata",
                ));
            }
            if let Some(superclass) = node.superclass {
                step(&mut steps, options)?;
                let name = definition
                    .constant_pool
                    .class_name(definition.super_class)
                    .map_err(RegistryError::Parse)?;
                heap.instance_graph_node(superclass)?;
                if registry
                    .class(superclass)?
                    .name()
                    .map_err(RegistryError::Parse)?
                    != name
                {
                    return Err(FieldAccessError::NameMismatch {
                        requested: name.clone(),
                        returned: superclass,
                    });
                }
                bind(&mut bindings, &mut units, options, loader, name, superclass)?;
            }
            if node.interfaces.len() != definition.interfaces.len() {
                return Err(FieldAccessError::InvalidState(
                    "admitted interface count differs from immutable metadata",
                ));
            }
            for (interface, index) in node.interfaces.iter().zip(&definition.interfaces) {
                step(&mut steps, options)?;
                heap.instance_graph_node(*interface)?;
                let name = definition
                    .constant_pool
                    .class_name(*index)
                    .map_err(RegistryError::Parse)?;
                if registry
                    .class(*interface)?
                    .name()
                    .map_err(RegistryError::Parse)?
                    != name
                {
                    return Err(FieldAccessError::NameMismatch {
                        requested: name.clone(),
                        returned: *interface,
                    });
                }
                bind(&mut bindings, &mut units, options, loader, name, *interface)?;
            }
        }
        let admitted_classes = heap.retained_layout_classes();
        let heap_identity = heap.identity;
        let reference_universe = heap.reference_type_universe();
        Ok(Self {
            registry,
            heap,
            resolver: Some(resolver),
            member: None,
            options,
            bindings,
            units,
            steps,
            admitted_classes,
            heap_identity,
            reference_universe,
        })
    }
    pub fn registry(&self) -> &'a DefinitionRegistry {
        self.registry
    }
    pub fn heap(&self) -> &ManagedHeap {
        self.heap
    }
    pub fn heap_mut(&mut self) -> &mut ManagedHeap {
        self.heap
    }
    pub fn resolved_symbols(&self) -> usize {
        self.bindings.len()
    }
    pub fn symbol_units(&self) -> usize {
        self.units
    }
    pub fn lookup_steps(&self) -> usize {
        self.steps
    }
    pub(crate) fn unchanged(&self) -> Result<(), FieldAccessError> {
        if self.heap.identity != self.heap_identity
            || self.heap.retained_layout_classes() != self.admitted_classes
            || self.heap.reference_type_universe() != self.reference_universe
        {
            Err(FieldAccessError::ContextChanged)
        } else {
            Ok(())
        }
    }
    pub(crate) fn step(&mut self) -> Result<(), FieldAccessError> {
        step(&mut self.steps, self.options)
    }
    /// Reserve an entire staging operation's node/link work atomically. Existing
    /// single-step operations retain their established failure counters.
    pub(crate) fn charge_bulk_work(
        &mut self,
        nodes: usize,
        links: usize,
    ) -> Result<(), FieldAccessError> {
        let amount = nodes
            .checked_add(links)
            .ok_or(FieldAccessError::ResourceLimit {
                resource: FieldResource::LookupSteps,
                limit: self.options.max_lookup_steps,
                required: usize::MAX,
            })?;
        self.steps = increased(
            self.steps,
            amount,
            self.options.max_lookup_steps,
            FieldResource::LookupSteps,
        )?;
        Ok(())
    }
    fn symbol(&self, origin: ClassId, index: u16) -> Result<Symbol<'a>, FieldAccessError> {
        let invalid = || FieldAccessError::InvalidConstant { origin, index };
        let registry: &'a DefinitionRegistry = self.registry;
        let pool = &registry.class(origin)?.constant_pool;
        let Constant::Fieldref {
            class_index,
            name_and_type_index,
        } = pool.get(index).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        let owner = pool.class_name(*class_index).map_err(|_| invalid())?;
        if owner.units().first() == Some(&91) {
            return Err(FieldAccessError::UnsupportedArrayOwner);
        }
        let Constant::NameAndType {
            name_index,
            descriptor_index,
        } = pool.get(*name_and_type_index).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        Ok(Symbol {
            owner,
            name: pool.utf8(*name_index).map_err(|_| invalid())?,
            descriptor: pool.utf8(*descriptor_index).map_err(|_| invalid())?,
        })
    }
    pub(crate) fn resolve_owner(
        &mut self,
        origin: ClassId,
        name: &'a JavaString,
    ) -> Result<ClassId, FieldAccessError> {
        self.resolve_type_name(origin, name.units())
    }
    pub(crate) fn resolve_type_name(
        &mut self,
        origin: ClassId,
        name: &'a [u16],
    ) -> Result<ClassId, FieldAccessError> {
        if self.member.is_some() {
            return self.member_type_name(origin, name);
        }
        let loader = self.registry.defining_loader(origin)?;
        if let Some(class) = self.bindings.get(&(loader, name)) {
            return Ok(*class);
        }
        increased(
            self.bindings.len(),
            1,
            self.options.max_symbols,
            FieldResource::Symbols,
        )?;
        increased(
            self.units,
            name.len(),
            self.options.max_symbol_units,
            FieldResource::SymbolUnits,
        )?;
        let class = self
            .resolver
            .as_deref_mut()
            .ok_or(FieldAccessError::InvalidState("legacy resolver absent"))?
            .resolve(self.registry, loader, name)
            .map_err(|error| FieldAccessError::Resolve {
                origin,
                name: JavaString::from_units(name.to_vec()),
                error,
            })?
            .ok_or_else(|| FieldAccessError::MissingDefinition {
                origin,
                name: JavaString::from_units(name.to_vec()),
            })?;
        if self
            .registry
            .class(class)?
            .name()
            .map_err(RegistryError::Parse)?
            .units()
            != name
        {
            return Err(FieldAccessError::NameMismatch {
                requested: JavaString::from_units(name.to_vec()),
                returned: class,
            });
        }
        self.heap.instance_graph_node(class)?;
        bind_units(
            &mut self.bindings,
            &mut self.units,
            self.options,
            loader,
            name,
            class,
        )?;
        Ok(class)
    }
    pub(crate) fn same_package(
        &self,
        first: ClassId,
        second: ClassId,
    ) -> Result<bool, FieldAccessError> {
        if self.registry.defining_loader(first)? != self.registry.defining_loader(second)? {
            return Ok(false);
        }
        fn package(name: &JavaString) -> &[u16] {
            let end = name
                .units()
                .iter()
                .rposition(|unit| *unit == 47)
                .unwrap_or(0);
            &name.units()[..end]
        }
        Ok(package(
            self.registry
                .class(first)?
                .name()
                .map_err(RegistryError::Parse)?,
        ) == package(
            self.registry
                .class(second)?
                .name()
                .map_err(RegistryError::Parse)?,
        ))
    }
    pub(crate) fn subclass(
        &mut self,
        mut class: ClassId,
        ancestor: ClassId,
    ) -> Result<bool, FieldAccessError> {
        for _ in 0..self.admitted_classes {
            self.step()?;
            let node = self.heap.instance_graph_node(class)?;
            if class == ancestor {
                return Ok(true);
            }
            if node.flags & 0x200 != 0 {
                return Ok(false);
            }
            match node.superclass {
                Some(parent) => class = parent,
                None => return Ok(false),
            }
        }
        Err(FieldAccessError::InvalidState(
            "cyclic admitted superclass chain",
        ))
    }
    fn lookup(&mut self, owner: ClassId, symbol: &Symbol<'_>) -> Result<FieldId, FieldAccessError> {
        increased(
            self.steps,
            1,
            self.options.max_lookup_steps,
            FieldResource::LookupSteps,
        )?;
        let mut stack = Vec::new();
        reserve_vec(&mut stack, 1)?;
        stack.push(owner);
        let mut visited = HashSet::new();
        while let Some(class) = stack.pop() {
            if visited.contains(&class) {
                continue;
            }
            self.step()?;
            reserve_visited(&mut visited, 1)?;
            visited.insert(class);
            let definition = self.registry.class(class)?;
            for (index, candidate) in definition.fields.iter().enumerate() {
                self.step()?;
                if definition
                    .constant_pool
                    .utf8(candidate.name_index)
                    .map_err(RegistryError::Parse)?
                    == symbol.name
                    && definition
                        .constant_pool
                        .utf8(candidate.descriptor_index)
                        .map_err(RegistryError::Parse)?
                        == symbol.descriptor
                {
                    return Ok(FieldId {
                        owner: class,
                        index,
                    });
                }
            }
            let node = self.heap.instance_graph_node(class)?;
            let additional = node
                .interfaces
                .len()
                .checked_add(usize::from(
                    node.flags & 0x200 == 0 && node.superclass.is_some(),
                ))
                .ok_or(FieldAccessError::InvalidState("lookup worklist overflow"))?;
            reserve_vec(&mut stack, additional)?;
            if node.flags & 0x200 == 0 {
                if let Some(parent) = node.superclass {
                    stack.push(parent);
                }
            }
            stack.extend(node.interfaces.iter().rev().copied());
        }
        Err(FieldAccessError::Java(FieldJavaError::NoSuchField))
    }
    pub(crate) fn select(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> Result<Selected, FieldAccessError> {
        self.unchanged()?;
        self.registry.class(origin)?;
        self.heap.instance_graph_node(origin)?;
        let symbol = self.symbol(origin, index)?;
        if self.is_member_checked() {
            let ticket = match self.member_begin(origin, index)? {
                Ok(value) => return crate::loading::member::field_hit(value),
                Err(ticket) => ticket,
            };
            let mut owner = None;
            let result = (|| {
                let class = self.resolve_owner(origin, symbol.owner)?;
                owner = Some(class);
                let selected = self.select_owner(origin, &symbol, class)?;
                if !self.member_emit(
                    origin,
                    selected.field.owner(),
                    symbol.descriptor.units(),
                    true,
                )? {
                    return Err(FieldAccessError::Java(FieldJavaError::LinkageError));
                }
                Ok(selected)
            })();
            return self.member_finish_field(ticket, owner, result);
        }
        let owner = self.resolve_owner(origin, symbol.owner)?;
        self.select_owner(origin, &symbol, owner)
    }
    fn select_owner(
        &mut self,
        origin: ClassId,
        symbol: &Symbol<'a>,
        owner: ClassId,
    ) -> Result<Selected, FieldAccessError> {
        if self.registry.class(owner)?.access_flags & 1 == 0 && !self.same_package(origin, owner)? {
            return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
        }
        let field = self.lookup(owner, symbol)?;
        let member = self.registry.field(field)?;
        let flags = member.access_flags;
        let declaring = field.owner();
        let mut protected_receiver = None;
        if flags & 1 != 0 { /* public */
        } else if flags & 2 != 0 {
            if origin != declaring {
                // Nest membership requires the same runtime package. Checked
                // mode can prove this denial without claiming nest support.
                if self.is_member_checked()
                    && !(if self.nest_access_enabled() {
                        self.nest_same_package(origin, declaring)?
                    } else {
                        self.same_package(origin, declaring)?
                    })
                {
                    return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
                }
                if self.nest_access_enabled() {
                    if !self.same_nest(origin, declaring)? {
                        return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
                    }
                } else {
                    return Err(FieldAccessError::UnsupportedNestAccess {
                        origin,
                        owner: declaring,
                    });
                }
            }
        } else if flags & 4 != 0 {
            if !self.same_package(origin, declaring)? {
                if !self.subclass(origin, declaring)? {
                    return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
                }
                if flags & 8 == 0 {
                    if !(self.subclass(origin, owner)? || self.subclass(owner, origin)?) {
                        return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
                    }
                    protected_receiver = Some(origin);
                }
            }
        } else if !self.same_package(origin, declaring)? {
            return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
        }
        if !self.is_member_checked()
            && symbol
                .descriptor
                .units()
                .iter()
                .copied()
                .find(|unit| *unit != 91)
                == Some(76)
            && self.registry.defining_loader(origin)? != self.registry.defining_loader(declaring)?
        {
            return Err(FieldAccessError::UnsupportedLoadingConstraint {
                origin,
                owner: declaring,
            });
        }
        Ok(Selected {
            symbolic_owner: owner,
            field,
            protected_receiver,
        })
    }
    pub(crate) fn select_field_id(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> Result<FieldId, FieldAccessError> {
        Ok(self.select(origin, index)?.field)
    }
    pub(crate) fn field_receiver(
        &mut self,
        selected: &Selected,
        receiver: Reference,
    ) -> Result<(), FieldAccessError> {
        if receiver.is_null() {
            return Err(FieldAccessError::Java(FieldJavaError::NullPointer));
        }
        let actual = self.heap.instance_class(receiver)?;
        if !self.subclass(actual, selected.symbolic_owner)? {
            return Err(FieldAccessError::InvalidReceiver {
                expected: selected.symbolic_owner,
                actual,
            });
        }
        if let Some(expected) = selected.protected_receiver {
            if !self.subclass(actual, expected)? {
                return Err(FieldAccessError::InvalidReceiver { expected, actual });
            }
        }
        Ok(())
    }
    pub fn check_receiver(
        &mut self,
        owner: ClassId,
        receiver: Reference,
    ) -> Result<(), FieldAccessError> {
        self.unchanged()?;
        self.registry.class(owner)?;
        self.heap.instance_graph_node(owner)?;
        let actual = self.heap.instance_class(receiver)?;
        if self.subclass(actual, owner)? {
            Ok(())
        } else {
            Err(FieldAccessError::InvalidReceiver {
                expected: owner,
                actual,
            })
        }
    }
    pub fn read_field(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
    ) -> Result<Value, FieldAccessError> {
        let selected = self.select(origin, index)?;
        if self.registry.field(selected.field)?.access_flags & 8 != 0 {
            return Err(FieldAccessError::Java(
                FieldJavaError::IncompatibleClassChange,
            ));
        }
        self.field_receiver(&selected, receiver)?;
        if let Some(value) = self
            .read_string_body_field(selected.field, receiver)
            .map_err(FieldAccessError::StringBody)?
        {
            return Ok(value);
        }
        Ok(self.heap.get_field(receiver, selected.field)?)
    }
    pub fn write_field(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
        value: Value,
    ) -> Result<(), FieldAccessError> {
        let selected = self.select(origin, index)?;
        let flags = self.registry.field(selected.field)?.access_flags;
        if flags & 8 != 0 {
            return Err(FieldAccessError::Java(
                FieldJavaError::IncompatibleClassChange,
            ));
        }
        if flags & 16 != 0 {
            return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
        }
        self.field_receiver(&selected, receiver)?;
        if self.check_field_assignment(selected.field, value)? {
            if let Value::Reference(reference) = value {
                return Ok(self.heap.commit_reference_field(
                    receiver,
                    selected.field,
                    reference,
                )?);
            }
        }
        Ok(self.heap.put_field(receiver, selected.field, value)?)
    }
    // Returns true only for a checked nonnull reference destination. Callers
    // retain their own final/receiver authority and guarded publication boundary.
    pub(crate) fn check_field_assignment(
        &mut self,
        field: FieldId,
        value: Value,
    ) -> Result<bool, FieldAccessError> {
        let definition = self.registry.class(field.owner())?;
        let descriptor = definition
            .constant_pool
            .utf8(self.registry.field(field)?.descriptor_index)
            .map_err(RegistryError::Parse)?;
        if let Value::Reference(reference) = value {
            if !reference.is_null() && matches!(descriptor.units().first(), Some(76 | 91)) {
                let source = self
                    .reference_type(reference)
                    .map_err(|error| FieldAccessError::ReferenceType(Box::new(error)))?
                    .ok_or_else(|| {
                        FieldAccessError::ReferenceType(Box::new(TypeAccessError::InvalidType(
                            "nonnull field value has no runtime type",
                        )))
                    })?;
                let target = self
                    .resolve_reference_field_type(field)
                    .map_err(|error| FieldAccessError::ReferenceType(Box::new(error)))?;
                if !self
                    .is_assignable(source, target)
                    .map_err(|error| FieldAccessError::ReferenceType(Box::new(error)))?
                {
                    return Err(FieldAccessError::IncompatibleReference(Box::new(
                        ReferenceFieldMismatch {
                            field,
                            source,
                            target,
                        },
                    )));
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HeapOptions, HierarchyOptions, InstanceOptions, MethodId, RegistryOptions,
        SameLoaderResolver, build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    #[test]
    fn bulk_work_reservations_are_checked_atomically_at_exact_and_machine_boundaries() {
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
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let before = ctx.lookup_steps();
        ctx.options.max_lookup_steps = before + 5;
        assert_eq!(
            ctx.charge_bulk_work(3, 3),
            Err(FieldAccessError::ResourceLimit {
                resource: FieldResource::LookupSteps,
                limit: before + 5,
                required: before + 6
            })
        );
        assert_eq!(ctx.lookup_steps(), before);
        ctx.charge_bulk_work(3, 2).unwrap();
        assert_eq!(ctx.lookup_steps(), before + 5);
        ctx.charge_bulk_work(0, 0).unwrap();
        ctx.options.max_lookup_steps = usize::MAX;
        let before = ctx.lookup_steps();
        for (nodes, links) in [(usize::MAX, 1), (usize::MAX, 0)] {
            assert_eq!(
                ctx.charge_bulk_work(nodes, links),
                Err(FieldAccessError::ResourceLimit {
                    resource: FieldResource::LookupSteps,
                    limit: usize::MAX,
                    required: usize::MAX
                })
            );
            assert_eq!(ctx.lookup_steps(), before);
        }
    }
    #[test]
    fn reference_diagnostics_preserve_wrapped_sources_and_mismatch_identities() {
        use std::error::Error;
        let wrapped = FieldAccessError::ReferenceType(Box::new(TypeAccessError::Java(
            crate::TypeJavaError::IllegalAccess,
        )));
        assert!(wrapped.source().is_some());
        assert!(matches!(wrapped, FieldAccessError::ReferenceType(_)));
        assert!(wrapped.to_string().contains("reference type"));
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        let field = FieldId {
            owner: class,
            index: 0,
        };
        let error = FieldAccessError::IncompatibleReference(Box::new(ReferenceFieldMismatch {
            field,
            source: ReferenceType::Class(class),
            target: ReferenceType::Class(class),
        }));
        assert!(error.source().is_none());
        assert!(error.to_string().contains("not assignable"));
    }

    fn fixture() -> (DefinitionRegistry, ClassId) {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0, 7];
        for (tag, data) in [
            (1, b"java/lang/Object".as_slice()),
            (7, &[0, 1]),
            (1, b"x".as_slice()),
            (1, b"I".as_slice()),
            (12, &[0, 3, 0, 4]),
            (9, &[0, 2, 0, 5]),
        ] {
            bytes.push(tag);
            if tag == 1 {
                bytes.extend((data.len() as u16).to_be_bytes());
            }
            bytes.extend(data);
        }
        for word in [0x21_u16, 2, 0, 0, 1, 1, 3, 4, 0, 0, 0] {
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
    fn all_counter_overflows_and_exact_boundaries_are_checked_before_mutation() {
        for resource in [
            FieldResource::Symbols,
            FieldResource::SymbolUnits,
            FieldResource::LookupSteps,
        ] {
            assert_eq!(
                increased(usize::MAX, 1, usize::MAX, resource).unwrap_err(),
                FieldAccessError::ResourceLimit {
                    resource,
                    limit: usize::MAX,
                    required: usize::MAX
                }
            );
            assert_eq!(
                increased(usize::MAX, 0, usize::MAX, resource).unwrap(),
                usize::MAX
            );
            assert_eq!(
                increased(0, 1, 0, resource).unwrap_err(),
                FieldAccessError::ResourceLimit {
                    resource,
                    limit: 0,
                    required: 1
                }
            );
        }
        let mut steps = usize::MAX;
        assert!(
            step(
                &mut steps,
                FieldResolutionOptions {
                    max_lookup_steps: usize::MAX,
                    ..FieldResolutionOptions::default()
                }
            )
            .is_err()
        );
        assert_eq!(steps, usize::MAX);
        let mut bindings = HashMap::new();
        let mut units = usize::MAX;
        let name = JavaString::from_units(vec![65]);
        let loader = LoaderId {
            registry: 1,
            index: 0,
        };
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        assert!(
            bind(
                &mut bindings,
                &mut units,
                FieldResolutionOptions {
                    max_symbol_units: usize::MAX,
                    ..FieldResolutionOptions::default()
                },
                loader,
                &name,
                class
            )
            .is_err()
        );
        assert!(bindings.is_empty());
        assert_eq!(units, usize::MAX);
    }
    #[test]
    fn deterministic_reservation_overflow_preserves_existing_metadata() {
        let class = ClassId {
            registry: 1,
            index: 0,
        };
        let loader = LoaderId {
            registry: 1,
            index: 0,
        };
        let name = JavaString::from_units(vec![65]);
        let mut values = vec![class];
        let mut visited = HashSet::from([class]);
        let mut bindings = HashMap::from([((loader, name.units()), class)]);
        for result in [
            reserve_vec(&mut values, usize::MAX),
            reserve_visited(&mut visited, usize::MAX),
            reserve_bindings(&mut bindings, usize::MAX),
        ] {
            assert_eq!(result.unwrap_err(), allocation());
        }
        assert_eq!(values, [class]);
        assert!(visited.contains(&class));
        assert_eq!(visited.len(), 1);
        assert_eq!(bindings.get(&(loader, name.units())), Some(&class));
        assert_eq!(bindings.len(), 1);
    }
    #[test]
    fn bindings_preserve_surrogate_units_and_reject_contradictory_identity_without_charge() {
        let loader = LoaderId {
            registry: 1,
            index: 0,
        };
        let first = ClassId {
            registry: 1,
            index: 0,
        };
        let second = ClassId {
            registry: 1,
            index: 1,
        };
        let first_name = JavaString::from_units(vec![0xd800]);
        let second_name = JavaString::from_units(vec![0xd801]);
        let mut bindings = HashMap::new();
        let mut units = 0;
        let options = FieldResolutionOptions::default();
        bind(
            &mut bindings,
            &mut units,
            options,
            loader,
            &first_name,
            first,
        )
        .unwrap();
        bind(
            &mut bindings,
            &mut units,
            options,
            loader,
            &second_name,
            second,
        )
        .unwrap();
        bind(
            &mut bindings,
            &mut units,
            options,
            loader,
            &first_name,
            first,
        )
        .unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(units, 2);
        assert_eq!(
            bind(
                &mut bindings,
                &mut units,
                options,
                loader,
                &first_name,
                second
            )
            .unwrap_err(),
            FieldAccessError::ConflictingBinding {
                loader,
                name: first_name.clone()
            }
        );
        assert_eq!(bindings.len(), 2);
        assert_eq!(units, 2);
    }
    #[test]
    fn forged_class_and_cached_binding_indices_remain_checked_and_method_index_is_not_a_proof() {
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
        let forged = ClassId {
            registry: class.registry,
            index: usize::MAX,
        };
        assert!(matches!(
            context.read_field(forged, 6, receiver),
            Err(FieldAccessError::Registry(_))
        ));
        assert!(matches!(
            context.check_receiver(forged, receiver),
            Err(FieldAccessError::Registry(_))
        ));
        let name = registry.class(class).unwrap().name().unwrap();
        let loader = registry.defining_loader(class).unwrap();
        context.bindings.insert((loader, name.units()), forged);
        assert!(matches!(
            context.read_field(class, 6, receiver),
            Err(FieldAccessError::Registry(_))
        ));
        let method = MethodId {
            owner: class,
            index: usize::MAX,
        };
        assert_eq!(method.index(), usize::MAX);
        assert!(registry.method(method).is_err());
    }
}

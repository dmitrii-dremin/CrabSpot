//! Bounded field resolution and access for an explicitly selected unnamed-module profile.
use crate::{
    ClassId, DefinitionRegistry, FieldId, HeapError, HierarchyResolver, InstanceError, LoaderId,
    ManagedHeap, Reference, RegistryError, ResolveError, Value,
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
pub enum FieldAccessError {
    Java(FieldJavaError),
    Registry(RegistryError),
    Instance(InstanceError),
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
            Self::Java(error) => write!(f, "field access failed: {error:?}"),
            Self::Registry(error) => write!(f, "field registry operation failed: {error}"),
            Self::Instance(error) => write!(f, "field instance operation failed: {error}"),
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
            Self::Registry(error) => Some(error),
            Self::Instance(error) => Some(error),
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
fn step(steps: &mut usize, options: FieldResolutionOptions) -> Result<(), FieldAccessError> {
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
struct Selected {
    symbolic_owner: ClassId,
    field: FieldId,
    protected_receiver: Option<ClassId>,
}

/// A reusable access context. It neither loads definitions nor changes lifecycle state.
pub struct FieldResolutionContext<'a> {
    registry: &'a DefinitionRegistry,
    heap: &'a mut ManagedHeap,
    resolver: &'a mut dyn HierarchyResolver,
    options: FieldResolutionOptions,
    bindings: Bindings<'a>,
    units: usize,
    steps: usize,
    admitted_classes: usize,
    heap_identity: u64,
    reference_universe: Option<crate::ReferenceTypeUniverse>,
}
impl<'a> FieldResolutionContext<'a> {
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
            resolver,
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
    fn select(&mut self, origin: ClassId, index: u16) -> Result<Selected, FieldAccessError> {
        self.unchanged()?;
        self.registry.class(origin)?;
        self.heap.instance_graph_node(origin)?;
        let symbol = self.symbol(origin, index)?;
        let owner = self.resolve_owner(origin, symbol.owner)?;
        if self.registry.class(owner)?.access_flags & 1 == 0 && !self.same_package(origin, owner)? {
            return Err(FieldAccessError::Java(FieldJavaError::IllegalAccess));
        }
        let field = self.lookup(owner, &symbol)?;
        let member = self.registry.field(field)?;
        let flags = member.access_flags;
        let declaring = field.owner();
        let mut protected_receiver = None;
        if flags & 1 != 0 { /* public */
        } else if flags & 2 != 0 {
            if origin != declaring {
                return Err(FieldAccessError::UnsupportedNestAccess {
                    origin,
                    owner: declaring,
                });
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
        if symbol
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
    fn field_receiver(
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
        Ok(self.heap.put_field(receiver, selected.field, value)?)
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

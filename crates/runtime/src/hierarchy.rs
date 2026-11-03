//! Bounded structural relationships and logical field locations. A successful
//! plan is neither a full Java linker result nor a verification/initialization proof.
use crate::{ClassId, DefinitionRegistry, FieldId, LoaderId, RegistryError};
use crabspot_classfile::{ClassFile, JavaString};
use std::{collections::HashMap, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveError {
    pub message: String,
}
impl ResolveError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ResolveError {}

pub trait HierarchyResolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        initiating_loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError>;
}
/// Explicit same-loader lookup only; no parent delegation or class creation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SameLoaderResolver;
impl SameLoaderResolver {
    pub fn new() -> Self {
        Self
    }
}
impl HierarchyResolver for SameLoaderResolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        initiating_loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        registry
            .lookup_class(initiating_loader, name)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HierarchyOptions {
    pub max_classes: usize,
    pub max_edges: usize,
    pub max_declared_fields: usize,
    pub max_instance_slots: usize,
}
impl Default for HierarchyOptions {
    fn default() -> Self {
        Self {
            max_classes: 1024,
            max_edges: 8192,
            max_declared_fields: 65536,
            max_instance_slots: 65536,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HierarchyResource {
    Classes,
    Edges,
    DeclaredFields,
    InstanceSlots,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HierarchyError {
    Registry(RegistryError),
    Resolve {
        origin: ClassId,
        name: JavaString,
        error: ResolveError,
    },
    MissingDefinition {
        origin: ClassId,
        initiating_loader: LoaderId,
        name: JavaString,
    },
    NameMismatch {
        origin: ClassId,
        requested: JavaString,
        returned: ClassId,
        actual: JavaString,
    },
    WrongSuperclassKind {
        class: ClassId,
        superclass: ClassId,
    },
    WrongInterfaceKind {
        class: ClassId,
        interface: ClassId,
    },
    FinalSuperclass {
        class: ClassId,
        superclass: ClassId,
    },
    Cycle {
        class: ClassId,
        dependency: ClassId,
    },
    ResourceLimit {
        resource: HierarchyResource,
        limit: usize,
        required: usize,
    },
    InvalidState(&'static str),
}
impl fmt::Display for HierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(reason) => write!(f, "invalid internal hierarchy state: {reason}"),
            Self::Registry(error) => write!(f, "hierarchy definition lookup failed: {error}"),
            Self::Resolve {
                origin,
                name,
                error,
            } => write!(
                f,
                "resolving {} from {origin:?} failed: {error}",
                name.escaped()
            ),
            Self::MissingDefinition {
                origin,
                initiating_loader,
                name,
            } => write!(
                f,
                "{} referenced by {origin:?} is unavailable in loader context {initiating_loader:?}",
                name.escaped()
            ),
            Self::NameMismatch {
                origin,
                requested,
                returned,
                actual,
            } => write!(
                f,
                "resolver returned {returned:?} named {} for {} referenced by {origin:?}",
                actual.escaped(),
                requested.escaped()
            ),
            Self::WrongSuperclassKind { class, superclass } => write!(
                f,
                "{class:?} refers to interface {superclass:?} as its superclass"
            ),
            Self::WrongInterfaceKind { class, interface } => write!(
                f,
                "{class:?} refers to class {interface:?} as a direct interface"
            ),
            Self::FinalSuperclass { class, superclass } => {
                write!(f, "{class:?} extends final class {superclass:?}")
            }
            Self::Cycle { class, dependency } => write!(
                f,
                "hierarchy cycle from {class:?} to active ancestor {dependency:?}"
            ),
            Self::ResourceLimit {
                resource,
                limit,
                required,
            } => write!(
                f,
                "hierarchy {resource:?} limit {limit} exceeded (required {required})"
            ),
        }
    }
}
impl std::error::Error for HierarchyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(error) => Some(error),
            Self::Resolve { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Own fields appear once. Long/Double fields each occupy one logical typed-value
/// location, unlike their two execution-frame slots. Descriptors remain in registry metadata.
#[derive(Debug)]
pub struct ClassLayout {
    class_id: ClassId,
    superclass: Option<ClassId>,
    interfaces: Vec<ClassId>,
    instance_fields: Vec<FieldId>,
    static_fields: Vec<FieldId>,
    inherited_slots: usize,
    slots: usize,
    is_interface: bool,
}
impl ClassLayout {
    pub fn class_id(&self) -> ClassId {
        self.class_id
    }
    pub fn direct_superclass(&self) -> Option<ClassId> {
        self.superclass
    }
    pub fn direct_interfaces(&self) -> &[ClassId] {
        &self.interfaces
    }
    pub fn declared_instance_fields(&self) -> &[FieldId] {
        &self.instance_fields
    }
    pub fn declared_static_fields(&self) -> &[FieldId] {
        &self.static_fields
    }
    pub fn inherited_instance_slots(&self) -> usize {
        self.inherited_slots
    }
    pub fn instance_slots(&self) -> usize {
        self.slots
    }
}

#[derive(Debug)]
pub struct HierarchyPlan {
    root: ClassId,
    classes: Vec<ClassId>,
    layouts: HashMap<ClassId, ClassLayout>,
}
impl HierarchyPlan {
    pub fn root(&self) -> ClassId {
        self.root
    }
    /// Deterministic dependency-first DFS completion order.
    pub fn classes(&self) -> &[ClassId] {
        &self.classes
    }
    pub fn layout(&self, class: ClassId) -> Option<&ClassLayout> {
        self.layouts.get(&class)
    }
    pub fn instance_field_slot(&self, receiver: ClassId, field: FieldId) -> Option<usize> {
        let owner = self.layouts.get(&field.owner())?;
        let index = owner
            .instance_fields
            .iter()
            .position(|candidate| *candidate == field)?;
        let slot = owner.inherited_slots.checked_add(index)?;
        let receiver_layout = self.layouts.get(&receiver)?;
        if slot >= receiver_layout.slots {
            return None;
        }
        let mut current = receiver;
        // The built graph is acyclic; the explicit bound also keeps this query
        // nonrecursive and total if internal invariants are ever damaged.
        for _ in 0..self.classes.len() {
            let layout = self.layouts.get(&current)?;
            if layout.is_interface {
                return None;
            }
            if current == field.owner() {
                return Some(slot);
            }
            current = layout.superclass?;
        }
        None
    }
    pub fn declared_static_field_slot(&self, field: FieldId) -> Option<usize> {
        self.layouts
            .get(&field.owner())?
            .static_fields
            .iter()
            .position(|candidate| *candidate == field)
    }
}

#[derive(Clone, Copy)]
enum Visit {
    Gray,
    Black,
}
struct Pending<'a> {
    class: ClassId,
    metadata: &'a ClassFile,
    superclass: Option<ClassId>,
    interfaces: Vec<ClassId>,
    next: usize,
}
impl Pending<'_> {
    fn next_dependency(&mut self) -> Option<ClassId> {
        let index = self.next;
        self.next = self.next.saturating_add(1);
        match self.superclass {
            Some(superclass) if index == 0 => Some(superclass),
            Some(_) => self.interfaces.get(index.checked_sub(1)?).copied(),
            None => self.interfaces.get(index).copied(),
        }
    }
}
#[derive(Default)]
struct Counts {
    classes: usize,
    edges: usize,
    fields: usize,
}
fn increased(
    current: usize,
    amount: usize,
    limit: usize,
    resource: HierarchyResource,
) -> Result<usize, HierarchyError> {
    let required = current
        .checked_add(amount)
        .ok_or(HierarchyError::ResourceLimit {
            resource,
            limit,
            required: usize::MAX,
        })?;
    if required > limit {
        Err(HierarchyError::ResourceLimit {
            resource,
            limit,
            required,
        })
    } else {
        Ok(required)
    }
}
fn registry_error(error: RegistryError) -> HierarchyError {
    HierarchyError::Registry(error)
}

fn resolve_symbol<'a, R: HierarchyResolver + ?Sized>(
    registry: &'a DefinitionRegistry,
    origin: ClassId,
    loader: LoaderId,
    name: &'a [u16],
    resolver: &mut R,
    cache: &mut HashMap<(LoaderId, &'a [u16]), ClassId>,
) -> Result<ClassId, HierarchyError> {
    if let Some(target) = cache.get(&(loader, name)) {
        return Ok(*target);
    }
    let target = resolver
        .resolve(registry, loader, name)
        .map_err(|error| HierarchyError::Resolve {
            origin,
            name: JavaString::from_units(name.to_vec()),
            error,
        })?
        .ok_or_else(|| HierarchyError::MissingDefinition {
            origin,
            initiating_loader: loader,
            name: JavaString::from_units(name.to_vec()),
        })?;
    let declared = registry
        .class(target)
        .map_err(registry_error)?
        .name()
        .map_err(|error| registry_error(RegistryError::Parse(error)))?;
    if declared.units() != name {
        return Err(HierarchyError::NameMismatch {
            origin,
            requested: JavaString::from_units(name.to_vec()),
            returned: target,
            actual: declared.clone(),
        });
    }
    cache.insert((loader, name), target);
    Ok(target)
}

fn enter<'a, R: HierarchyResolver + ?Sized>(
    registry: &'a DefinitionRegistry,
    class: ClassId,
    resolver: &mut R,
    cache: &mut HashMap<(LoaderId, &'a [u16]), ClassId>,
    options: HierarchyOptions,
    counts: &mut Counts,
) -> Result<Pending<'a>, HierarchyError> {
    let metadata = registry.class(class).map_err(registry_error)?;
    counts.classes = increased(
        counts.classes,
        1,
        options.max_classes,
        HierarchyResource::Classes,
    )?;
    let edge_count = metadata
        .interfaces
        .len()
        .checked_add(usize::from(metadata.super_class != 0))
        .ok_or(HierarchyError::ResourceLimit {
            resource: HierarchyResource::Edges,
            limit: options.max_edges,
            required: usize::MAX,
        })?;
    counts.edges = increased(
        counts.edges,
        edge_count,
        options.max_edges,
        HierarchyResource::Edges,
    )?;
    counts.fields = increased(
        counts.fields,
        metadata.fields.len(),
        options.max_declared_fields,
        HierarchyResource::DeclaredFields,
    )?;
    let loader = registry.defining_loader(class).map_err(registry_error)?;
    let superclass = if metadata.super_class == 0 {
        None
    } else {
        let name = metadata
            .constant_pool
            .class_name(metadata.super_class)
            .map_err(|error| registry_error(RegistryError::Parse(error)))?
            .units();
        let target = resolve_symbol(registry, class, loader, name, resolver, cache)?;
        let parent = registry.class(target).map_err(registry_error)?;
        if parent.access_flags & 0x0200 != 0 {
            return Err(HierarchyError::WrongSuperclassKind {
                class,
                superclass: target,
            });
        }
        // Interfaces contribute no inherited storage, but their direct Object
        // superclass still has to satisfy the role/finality checks (JVMS 5.3.5).
        if parent.access_flags & 0x0010 != 0 {
            return Err(HierarchyError::FinalSuperclass {
                class,
                superclass: target,
            });
        }
        Some(target)
    };
    let mut interfaces = Vec::new();
    for index in &metadata.interfaces {
        let name = metadata
            .constant_pool
            .class_name(*index)
            .map_err(|error| registry_error(RegistryError::Parse(error)))?
            .units();
        let target = resolve_symbol(registry, class, loader, name, resolver, cache)?;
        if registry.class(target).map_err(registry_error)?.access_flags & 0x0200 == 0 {
            return Err(HierarchyError::WrongInterfaceKind {
                class,
                interface: target,
            });
        }
        interfaces.push(target);
    }
    Ok(Pending {
        class,
        metadata,
        superclass,
        interfaces,
        next: 0,
    })
}

fn finish(
    current: Pending<'_>,
    layouts: &HashMap<ClassId, ClassLayout>,
    options: HierarchyOptions,
) -> Result<ClassLayout, HierarchyError> {
    let is_interface = current.metadata.access_flags & 0x0200 != 0;
    let inherited = if is_interface {
        0
    } else {
        match current.superclass {
            None => 0,
            Some(parent) => {
                layouts
                    .get(&parent)
                    .ok_or(HierarchyError::InvalidState(
                        "superclass layout was not completed before its dependent",
                    ))?
                    .slots
            }
        }
    };
    let own_instance_count = current
        .metadata
        .fields
        .iter()
        .filter(|field| field.access_flags & 0x0008 == 0)
        .count();
    let slots = if is_interface {
        0
    } else {
        increased(
            inherited,
            own_instance_count,
            options.max_instance_slots,
            HierarchyResource::InstanceSlots,
        )?
    };
    let mut instance_fields = Vec::new();
    let mut static_fields = Vec::new();
    for (index, field) in current.metadata.fields.iter().enumerate() {
        let handle = FieldId {
            owner: current.class,
            index,
        };
        if field.access_flags & 0x0008 != 0 {
            static_fields.push(handle);
        } else {
            instance_fields.push(handle);
        }
    }
    Ok(ClassLayout {
        class_id: current.class,
        superclass: current.superclass,
        interfaces: current.interfaces,
        instance_fields,
        static_fields,
        inherited_slots: inherited,
        slots,
        is_interface,
    })
}

/// Build an immutable structural/layout plan without creating, executing,
/// verifying, linking or initializing any definition. Resolution is explicit.
pub fn build_hierarchy<R: HierarchyResolver + ?Sized>(
    registry: &DefinitionRegistry,
    root: ClassId,
    resolver: &mut R,
    options: HierarchyOptions,
) -> Result<HierarchyPlan, HierarchyError> {
    let mut counts = Counts::default();
    let mut cache = HashMap::new();
    let first = enter(registry, root, resolver, &mut cache, options, &mut counts)?;
    let mut pending = vec![first];
    let mut visits = HashMap::new();
    visits.insert(root, Visit::Gray);
    let mut classes = Vec::new();
    let mut layouts: HashMap<ClassId, ClassLayout> = HashMap::new();
    while let Some(current) = pending.last_mut() {
        if let Some(dependency) = current.next_dependency() {
            match visits.get(&dependency) {
                Some(Visit::Black) => continue,
                Some(Visit::Gray) => {
                    return Err(HierarchyError::Cycle {
                        class: current.class,
                        dependency,
                    });
                }
                None => {
                    let next = enter(
                        registry,
                        dependency,
                        resolver,
                        &mut cache,
                        options,
                        &mut counts,
                    )?;
                    visits.insert(dependency, Visit::Gray);
                    pending.push(next);
                    continue;
                }
            }
        }
        let current = pending.pop().ok_or(HierarchyError::InvalidState(
            "active traversal stack unexpectedly empty",
        ))?;
        let layout = finish(current, &layouts, options)?;
        classes.push(layout.class_id);
        visits.insert(layout.class_id, Visit::Black);
        layouts.insert(layout.class_id, layout);
    }
    Ok(HierarchyPlan {
        root,
        classes,
        layouts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RegistryOptions;
    use crabspot_classfile::ParseOptions;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn definition(name: &str, flags: u16, super_name: Option<&str>, field: bool) -> Vec<u8> {
        fn utf8(text: &str) -> Vec<u8> {
            let mut result = vec![1];
            result.extend((text.len() as u16).to_be_bytes());
            result.extend(text.bytes());
            result
        }
        let mut pool = vec![utf8(name), vec![7, 0, 1]];
        let superclass = if let Some(name) = super_name {
            pool.push(utf8(name));
            pool.push(vec![7, 0, 3]);
            4
        } else {
            0
        };
        let field_indices = if field {
            let index = pool.len() as u16 + 1;
            pool.push(utf8("f"));
            pool.push(utf8("J"));
            Some((index, index + 1))
        } else {
            None
        };
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        bytes.extend((pool.len() as u16 + 1).to_be_bytes());
        for entry in pool {
            bytes.extend(entry);
        }
        for word in [flags, 2, superclass, 0, u16::from(field)] {
            bytes.extend(word.to_be_bytes());
        }
        if let Some((name, descriptor)) = field_indices {
            for word in [1u16, name, descriptor, 0] {
                bytes.extend(word.to_be_bytes());
            }
        }
        bytes.extend([0, 0, 0, 0]);
        bytes
    }
    fn id(index: usize) -> ClassId {
        ClassId { registry: 1, index }
    }
    fn field(owner: ClassId, index: usize) -> FieldId {
        FieldId { owner, index }
    }
    fn layout(
        class_id: ClassId,
        superclass: Option<ClassId>,
        inherited_slots: usize,
        slots: usize,
        is_interface: bool,
    ) -> ClassLayout {
        ClassLayout {
            class_id,
            superclass,
            interfaces: Vec::new(),
            instance_fields: Vec::new(),
            static_fields: Vec::new(),
            inherited_slots,
            slots,
            is_interface,
        }
    }

    #[test]
    fn hierarchy_accounting_is_checked_at_machine_integer_boundaries() {
        for resource in [
            HierarchyResource::Classes,
            HierarchyResource::Edges,
            HierarchyResource::DeclaredFields,
            HierarchyResource::InstanceSlots,
        ] {
            assert_eq!(
                increased(usize::MAX, 1, usize::MAX, resource).unwrap_err(),
                HierarchyError::ResourceLimit {
                    resource,
                    limit: usize::MAX,
                    required: usize::MAX
                }
            );
            assert_eq!(
                increased(usize::MAX, 0, usize::MAX, resource).unwrap(),
                usize::MAX
            );
            assert_eq!(increased(2, 3, 5, resource).unwrap(), 5);
            assert_eq!(
                increased(2, 4, 5, resource).unwrap_err(),
                HierarchyError::ResourceLimit {
                    resource,
                    limit: 5,
                    required: 6
                }
            );
        }
    }

    #[test]
    fn final_object_is_rejected_for_interface_dependency_despite_zero_storage_inheritance() {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let object = registry
            .define(
                loader,
                &units("java/lang/Object"),
                &definition("java/lang/Object", 0x0031, None, false),
                ParseOptions::default(),
            )
            .unwrap();
        let interface = registry
            .define(
                loader,
                &units("I"),
                &definition("I", 0x0601, Some("java/lang/Object"), false),
                ParseOptions::default(),
            )
            .unwrap();
        assert_eq!(
            build_hierarchy(
                &registry,
                interface,
                &mut SameLoaderResolver,
                HierarchyOptions::default()
            )
            .unwrap_err(),
            HierarchyError::FinalSuperclass {
                class: interface,
                superclass: object
            }
        );
        assert_eq!(registry.definition_count(), 2);
    }

    #[test]
    fn missing_parent_layout_and_slot_sum_overflow_cannot_create_successful_layouts() {
        let metadata =
            crabspot_classfile::parse(&definition("C", 0x0021, Some("java/lang/Object"), true))
                .unwrap();
        let pending = || Pending {
            class: id(1),
            metadata: &metadata,
            superclass: Some(id(0)),
            interfaces: Vec::new(),
            next: 0,
        };
        assert_eq!(
            finish(pending(), &HashMap::new(), HierarchyOptions::default()).unwrap_err(),
            HierarchyError::InvalidState(
                "superclass layout was not completed before its dependent"
            )
        );
        let layouts = HashMap::from([(id(0), layout(id(0), None, 0, usize::MAX, false))]);
        assert_eq!(
            finish(
                pending(),
                &layouts,
                HierarchyOptions {
                    max_instance_slots: usize::MAX,
                    ..Default::default()
                }
            )
            .unwrap_err(),
            HierarchyError::ResourceLimit {
                resource: HierarchyResource::InstanceSlots,
                limit: usize::MAX,
                required: usize::MAX
            }
        );
    }

    #[test]
    fn private_forged_field_handles_and_damaged_parent_cycles_do_not_escape_query_bounds() {
        let base = id(0);
        let child = id(1);
        let unrelated = id(2);
        let interface = id(3);
        let mut base_layout = layout(base, None, 0, 1, false);
        base_layout.instance_fields.push(field(base, 0));
        base_layout.static_fields.push(field(base, 1));
        let mut child_layout = layout(child, Some(base), 1, 2, false);
        child_layout.instance_fields.push(field(child, 0));
        child_layout.static_fields.push(field(child, 1));
        let mut unrelated_layout = layout(unrelated, None, 0, 1, false);
        unrelated_layout.instance_fields.push(field(unrelated, 0));
        let mut interface_layout = layout(interface, Some(base), 0, 0, true);
        interface_layout.static_fields.push(field(interface, 0));
        let mut plan = HierarchyPlan {
            root: child,
            classes: vec![base, child, unrelated, interface],
            layouts: HashMap::from([
                (base, base_layout),
                (child, child_layout),
                (unrelated, unrelated_layout),
                (interface, interface_layout),
            ]),
        };
        assert_eq!(plan.instance_field_slot(child, field(base, 0)), Some(0));
        assert_eq!(plan.instance_field_slot(child, field(child, 0)), Some(1));
        assert_eq!(plan.instance_field_slot(unrelated, field(base, 0)), None);
        assert_eq!(plan.instance_field_slot(interface, field(base, 0)), None);
        assert_eq!(plan.declared_static_field_slot(field(base, 1)), Some(0));
        assert_eq!(plan.declared_static_field_slot(field(child, 1)), Some(0));
        assert_eq!(
            plan.declared_static_field_slot(field(interface, 0)),
            Some(0)
        );
        for owner in [
            base,
            id(99),
            ClassId {
                registry: 2,
                index: 0,
            },
        ] {
            assert_eq!(
                plan.instance_field_slot(child, field(owner, usize::MAX)),
                None
            );
            assert_eq!(
                plan.declared_static_field_slot(field(owner, usize::MAX)),
                None
            );
        }
        assert_eq!(plan.instance_field_slot(id(99), field(base, 0)), None);
        assert_eq!(
            plan.instance_field_slot(
                ClassId {
                    registry: 2,
                    index: 0
                },
                field(base, 0)
            ),
            None
        );
        assert_eq!(plan.instance_field_slot(child, field(base, 1)), None);
        assert_eq!(plan.declared_static_field_slot(field(base, 0)), None);
        plan.layouts.get_mut(&child).unwrap().superclass = Some(child);
        assert_eq!(plan.instance_field_slot(child, field(base, 0)), None);
    }
}

use super::Spelling;
use crabspot_classfile::ClassFile;
use crabspot_runtime::{
    ClassId, ClassLayout, DefinitionRegistry, HierarchyPlan, LoaderId, RegistryError,
};
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerificationBinding {
    pub initiating_loader: LoaderId,
    pub definition: ClassId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerificationCore {
    pub bootstrap_loader: LoaderId,
    pub object: ClassId,
    pub cloneable: ClassId,
    pub serializable: ClassId,
    pub throwable: ClassId,
    pub string: ClassId,
    pub class: ClassId,
    pub method_type: ClassId,
    pub method_handle: ClassId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum VerificationCoreRole {
    Object,
    Cloneable,
    Serializable,
    Throwable,
    String,
    Class,
    MethodType,
    MethodHandle,
}
impl VerificationCore {
    pub(super) fn roles(self) -> [(VerificationCoreRole, ClassId, &'static str); 8] {
        use VerificationCoreRole::*;
        [
            (Object, self.object, "java/lang/Object"),
            (Cloneable, self.cloneable, "java/lang/Cloneable"),
            (Serializable, self.serializable, "java/io/Serializable"),
            (Throwable, self.throwable, "java/lang/Throwable"),
            (String, self.string, "java/lang/String"),
            (Class, self.class, "java/lang/Class"),
            (MethodType, self.method_type, "java/lang/invoke/MethodType"),
            (
                MethodHandle,
                self.method_handle,
                "java/lang/invoke/MethodHandle",
            ),
        ]
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerificationEnvironmentOptions {
    pub max_class_rows: u64,
    pub max_edges: u64,
    pub max_bindings: u64,
    pub max_utf16_units: u64,
    pub max_work_units: u64,
}
impl Default for VerificationEnvironmentOptions {
    fn default() -> Self {
        Self {
            max_class_rows: 65_536,
            max_edges: 262_144,
            max_bindings: 262_144,
            max_utf16_units: 8_388_608,
            max_work_units: 16_777_216,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VerificationEnvironmentUsage {
    pub class_rows: u64,
    pub edges: u64,
    pub bindings: u64,
    pub utf16_units: u64,
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationEnvironmentResource {
    ClassRows,
    Edges,
    Bindings,
    Utf16Units,
    WorkUnits,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationEnvironmentLocation {
    Plan(usize),
    ClassRow { plan: usize, row: usize },
    Class(ClassId),
    Binding(usize),
    Core(VerificationCoreRole),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationEnvironmentErrorKind {
    Registry(RegistryError),
    MissingClass {
        class: ClassId,
    },
    ConflictingHierarchy {
        class: ClassId,
    },
    MissingDependency {
        class: ClassId,
        dependency: ClassId,
    },
    BindingConflict {
        initiating_loader: LoaderId,
        existing: ClassId,
        incoming: ClassId,
    },
    InvalidCore {
        role: VerificationCoreRole,
        class: ClassId,
        reason: &'static str,
    },
    InvalidMetadata {
        class: ClassId,
        reason: &'static str,
    },
    ResourceLimit {
        resource: VerificationEnvironmentResource,
        limit: u64,
        required: u64,
    },
    AccountingOverflow {
        resource: VerificationEnvironmentResource,
    },
    AllocationFailure {
        resource: &'static str,
    },
}
impl From<RegistryError> for VerificationEnvironmentErrorKind {
    fn from(e: RegistryError) -> Self {
        Self::Registry(e)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationEnvironmentError {
    pub location: VerificationEnvironmentLocation,
    pub usage: VerificationEnvironmentUsage,
    pub kind: VerificationEnvironmentErrorKind,
}
impl fmt::Display for VerificationEnvironmentErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            e => write!(f, "{e:?}"),
        }
    }
}
impl std::error::Error for VerificationEnvironmentErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e),
            _ => None,
        }
    }
}
impl fmt::Display for VerificationEnvironmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "environment {:?}: {}", self.location, self.kind)
    }
}
impl std::error::Error for VerificationEnvironmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.kind)
    }
}
#[derive(Debug)]
pub(super) struct Node<'a> {
    pub layout: &'a ClassLayout,
    pub metadata: &'a ClassFile,
    pub loader: LoaderId,
    pub name: &'a [u16],
    pub package: usize,
}
#[derive(Debug)]
pub struct VerificationEnvironment<'a> {
    pub(super) registry: &'a DefinitionRegistry,
    pub(super) nodes: HashMap<ClassId, Node<'a>>,
    pub(super) bindings: HashMap<(LoaderId, Spelling<'a>), ClassId>,
    pub(super) core: VerificationCore,
    pub(super) spelling_hash: std::collections::hash_map::RandomState,
    usage: VerificationEnvironmentUsage,
}
type E = VerificationEnvironmentErrorKind;
type R = VerificationEnvironmentResource;
type L = VerificationEnvironmentLocation;
type B<T> = Result<T, E>;
struct Builder<'a> {
    env: VerificationEnvironment<'a>,
    options: VerificationEnvironmentOptions,
    location: L,
    order: Vec<ClassId>,
}
impl VerificationEnvironment<'_> {
    pub fn usage(&self) -> VerificationEnvironmentUsage {
        self.usage
    }
}
impl<'a> VerificationEnvironment<'a> {
    pub fn new(
        registry: &'a DefinitionRegistry,
        plans: &[&'a HierarchyPlan],
        bindings: &[VerificationBinding],
        core: VerificationCore,
        options: VerificationEnvironmentOptions,
    ) -> Result<Self, Box<VerificationEnvironmentError>> {
        let mut b = Builder {
            env: Self {
                registry,
                nodes: HashMap::new(),
                bindings: HashMap::new(),
                core,
                spelling_hash: std::collections::hash_map::RandomState::new(),
                usage: VerificationEnvironmentUsage::default(),
            },
            options,
            location: L::Plan(0),
            order: Vec::new(),
        };
        if let Err(kind) = b.build(plans, bindings) {
            return Err(Box::new(VerificationEnvironmentError {
                location: b.location,
                usage: b.env.usage,
                kind,
            }));
        }
        Ok(b.env)
    }
}
impl<'a> Builder<'a> {
    fn charge(&mut self, rows: u64, edges: u64, bindings: u64, utf: u64, work: u64) -> B<()> {
        let u = self.env.usage;
        let o = self.options;
        let inputs = [
            (u.class_rows, rows, o.max_class_rows, R::ClassRows),
            (u.edges, edges, o.max_edges, R::Edges),
            (u.bindings, bindings, o.max_bindings, R::Bindings),
            (u.utf16_units, utf, o.max_utf16_units, R::Utf16Units),
            (u.work_units, work, o.max_work_units, R::WorkUnits),
        ];
        let mut t = [0; 5];
        for (dst, (old, n, limit, resource)) in t.iter_mut().zip(inputs) {
            *dst = old
                .checked_add(n)
                .ok_or(E::AccountingOverflow { resource })?;
            if *dst > limit {
                return Err(E::ResourceLimit {
                    resource,
                    limit,
                    required: *dst,
                });
            }
        }
        self.env.usage = VerificationEnvironmentUsage {
            class_rows: t[0],
            edges: t[1],
            bindings: t[2],
            utf16_units: t[3],
            work_units: t[4],
        };
        Ok(())
    }
    fn work(&mut self, n: u64) -> B<()> {
        self.charge(0, 0, 0, 0, n)
    }
    fn utf(&mut self, n: usize) -> B<()> {
        let n = u64::try_from(n).map_err(|_| E::AccountingOverflow {
            resource: R::Utf16Units,
        })?;
        self.charge(0, 0, 0, n, n)
    }
    fn alloc() -> E {
        E::AllocationFailure {
            resource: "verification environment indices",
        }
    }
    fn bind(&mut self, loader: LoaderId, class: ClassId) -> B<()> {
        self.charge(0, 0, 1, 0, 1)?;
        self.env.registry.class(class)?;
        let node = self
            .env
            .nodes
            .get(&class)
            .ok_or(E::MissingClass { class })?;
        let name = node.name;
        self.utf(name.len())?;
        let spelling = Spelling::new(name, &self.env.spelling_hash);
        if let Some(existing) = self.env.registry.lookup_class(loader, name)? {
            if existing != class {
                return Err(E::BindingConflict {
                    initiating_loader: loader,
                    existing,
                    incoming: class,
                });
            }
        }
        if let Some(existing) = self.env.bindings.get(&(loader, spelling)) {
            if *existing != class {
                return Err(E::BindingConflict {
                    initiating_loader: loader,
                    existing: *existing,
                    incoming: class,
                });
            }
            return Ok(());
        }
        self.env
            .bindings
            .try_reserve(1)
            .map_err(|_| Self::alloc())?;
        self.env.bindings.insert((loader, spelling), class);
        Ok(())
    }
    fn build(&mut self, plans: &[&'a HierarchyPlan], explicit: &[VerificationBinding]) -> B<()> {
        for (p, plan) in plans.iter().enumerate() {
            self.location = L::Plan(p);
            self.work(1)?;
            let mut root = false;
            for (row, class) in plan.classes().iter().copied().enumerate() {
                self.location = L::ClassRow { plan: p, row };
                self.charge(1, 0, 0, 0, 1)?;
                let metadata = self.env.registry.class(class)?;
                let loader = self.env.registry.defining_loader(class)?;
                let layout = plan.layout(class).ok_or(E::MissingClass { class })?;
                root |= class == plan.root();
                if layout.direct_superclass().is_some() {
                    self.charge(0, 1, 0, 0, 1)?;
                }
                for _ in layout.direct_interfaces() {
                    self.charge(0, 1, 0, 0, 1)?;
                }
                if let Some(old) = self.env.nodes.get(&class) {
                    let old = old.layout;
                    self.work(1)?;
                    if old.direct_superclass() != layout.direct_superclass()
                        || old.direct_interfaces().len() != layout.direct_interfaces().len()
                    {
                        return Err(E::ConflictingHierarchy { class });
                    }
                    for (a, b) in old
                        .direct_interfaces()
                        .iter()
                        .zip(layout.direct_interfaces())
                    {
                        self.work(1)?;
                        if a != b {
                            return Err(E::ConflictingHierarchy { class });
                        }
                    }
                } else {
                    self.env.nodes.try_reserve(1).map_err(|_| Self::alloc())?;
                    self.order.try_reserve(1).map_err(|_| Self::alloc())?;
                    self.env.nodes.insert(
                        class,
                        Node {
                            layout,
                            metadata,
                            loader,
                            name: &[],
                            package: 0,
                        },
                    );
                    self.order.push(class);
                }
            }
            if !root {
                return Err(E::InvalidMetadata {
                    class: plan.root(),
                    reason: "plan root is not a physical class row",
                });
            }
        }
        let mut packages: HashMap<(LoaderId, Spelling<'a>), usize> = HashMap::new();
        for index in 0..self.order.len() {
            let class = self.order[index];
            self.location = L::Class(class);
            self.work(1)?;
            let node = self
                .env
                .nodes
                .get(&class)
                .ok_or(E::MissingClass { class })?;
            let metadata = node.metadata;
            let loader = node.loader;
            self.work(2)?;
            let name = metadata.name().map_err(RegistryError::Parse)?.units();
            self.utf(name.len())?;
            if crate::scan::inspect(name, metadata.this_class, crate::scan::Role::Class, None)
                .is_err()
                || name.first() == Some(&91)
            {
                return Err(E::InvalidMetadata {
                    class,
                    reason: "definition requires a nonarray binary name",
                });
            }
            let end = name.iter().rposition(|u| *u == 47).unwrap_or(0);
            let pkg = &name[..end];
            let spelling = Spelling::new(pkg, &self.env.spelling_hash);
            let next = packages.len();
            let package = if let Some(package) = packages.get(&(loader, spelling)) {
                *package
            } else {
                packages.try_reserve(1).map_err(|_| Self::alloc())?;
                packages.insert((loader, spelling), next);
                next
            };
            let node = self
                .env
                .nodes
                .get_mut(&class)
                .ok_or(E::MissingClass { class })?;
            node.name = name;
            node.package = package;
        }
        for index in 0..self.order.len() {
            let class = self.order[index];
            self.location = L::Class(class);
            self.work(1)?;
            let node = self
                .env
                .nodes
                .get(&class)
                .ok_or(E::MissingClass { class })?;
            let layout = node.layout;
            let metadata = node.metadata;
            if (metadata.super_class == 0) != layout.direct_superclass().is_none()
                || metadata.interfaces.len() != layout.direct_interfaces().len()
            {
                return Err(E::InvalidMetadata {
                    class,
                    reason: "retained relations differ from metadata",
                });
            }
            if let Some(parent) = layout.direct_superclass() {
                self.edge(class, parent, metadata.super_class, false)?;
            } else if class != self.env.core.object {
                return Err(E::InvalidCore {
                    role: VerificationCoreRole::Object,
                    class,
                    reason: "another retained root",
                });
            }
            for (cp, interface) in metadata
                .interfaces
                .iter()
                .copied()
                .zip(layout.direct_interfaces().iter().copied())
            {
                self.edge(class, interface, cp, true)?;
            }
        }
        // Iterative gray/black DFS: duplicate sibling edges are not active cycles.
        let mut marks: HashMap<ClassId, u8> = HashMap::new();
        let mut stack = Vec::new();
        for index in 0..self.order.len() {
            let root = self.order[index];
            if marks.get(&root) == Some(&2) {
                continue;
            }
            self.work(1)?;
            marks.try_reserve(1).map_err(|_| Self::alloc())?;
            stack.try_reserve(1).map_err(|_| Self::alloc())?;
            marks.insert(root, 1);
            stack.push((root, 0usize));
            while let Some((class, next)) = stack.last_mut() {
                let id = *class;
                let node = self
                    .env
                    .nodes
                    .get(&id)
                    .ok_or(E::MissingClass { class: id })?;
                let n = *next;
                *next = next.checked_add(1).ok_or(E::AccountingOverflow {
                    resource: R::WorkUnits,
                })?;
                let dependency = if n == 0 {
                    node.layout.direct_superclass()
                } else {
                    node.layout.direct_interfaces().get(n - 1).copied()
                };
                if n > 0 && n > node.layout.direct_interfaces().len() {
                    marks.insert(id, 2);
                    stack.pop();
                    continue;
                }
                let Some(dependency) = dependency else {
                    continue;
                };
                self.work(1)?;
                match marks.get(&dependency) {
                    Some(1) => {
                        return Err(E::InvalidMetadata {
                            class: id,
                            reason: "cyclic retained graph",
                        });
                    }
                    Some(2) => {}
                    _ => {
                        self.work(1)?;
                        marks.try_reserve(1).map_err(|_| Self::alloc())?;
                        stack.try_reserve(1).map_err(|_| Self::alloc())?;
                        marks.insert(dependency, 1);
                        stack.push((dependency, 0));
                    }
                }
            }
        }
        for index in 0..self.order.len() {
            let class = self.order[index];
            self.location = L::Class(class);
            let node = self
                .env
                .nodes
                .get(&class)
                .ok_or(E::MissingClass { class })?;
            let loader = node.loader;
            let layout = node.layout;
            self.bind(loader, class)?;
            if let Some(parent) = layout.direct_superclass() {
                self.bind(loader, parent)?;
            }
            for interface in layout.direct_interfaces() {
                self.bind(loader, *interface)?;
            }
        }
        for (index, binding) in explicit.iter().enumerate() {
            self.location = L::Binding(index);
            self.bind(binding.initiating_loader, binding.definition)?;
        }
        for (role, class, name) in self.env.core.roles() {
            self.location = L::Core(role);
            self.work(1)?;
            let node = self
                .env
                .nodes
                .get(&class)
                .ok_or(E::MissingClass { class })?;
            let marker = matches!(
                role,
                VerificationCoreRole::Cloneable | VerificationCoreRole::Serializable
            );
            let invalid = node.loader != self.env.core.bootstrap_loader
                || !node.name.iter().copied().eq(name.bytes().map(u16::from))
                || node.metadata.access_flags & 1 == 0
                || (node.metadata.access_flags & 0x200 != 0) != marker
                || (marker
                    && (node.metadata.access_flags & 0x400 == 0
                        || node.layout.direct_superclass() != Some(self.env.core.object)
                        || !node.layout.direct_interfaces().is_empty()))
                || (role == VerificationCoreRole::Object
                    && node.layout.direct_superclass().is_some());
            if invalid {
                return Err(E::InvalidCore {
                    role,
                    class,
                    reason: "core identity, loader, name, kind or required edges differ",
                });
            }
        }
        Ok(())
    }
    fn edge(&mut self, class: ClassId, target: ClassId, index: u16, interface: bool) -> B<()> {
        self.work(1)?;
        let source = self
            .env
            .nodes
            .get(&class)
            .ok_or(E::MissingClass { class })?;
        let metadata = source.metadata;
        self.env.registry.class(target)?;
        let target_node = self.env.nodes.get(&target).ok_or(E::MissingDependency {
            class,
            dependency: target,
        })?;
        let name = target_node.name;
        let flags = target_node.metadata.access_flags;
        self.work(2)?;
        let spelling = metadata
            .constant_pool
            .class_name(index)
            .map_err(RegistryError::Parse)?
            .units();
        self.utf(spelling.len())?;
        self.utf(name.len())?;
        if spelling != name
            || (flags & 0x200 != 0) != interface
            || (!interface && flags & 0x10 != 0)
        {
            return Err(E::InvalidMetadata {
                class,
                reason: "dependency name, kind or finality differs",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::World;
    use super::*;
    #[test]
    fn environment_counter_overflow_and_late_limit_are_atomic() {
        let world = World::new(&[], &[]);
        let mut b = Builder {
            env: world.environment(),
            options: VerificationEnvironmentOptions {
                max_work_units: u64::MAX,
                ..Default::default()
            },
            location: L::Plan(0),
            order: vec![],
        };
        b.env.usage.work_units = u64::MAX;
        let before = b.env.usage;
        assert!(matches!(
            b.charge(1, 1, 1, 1, 1),
            Err(E::AccountingOverflow {
                resource: R::WorkUnits
            })
        ));
        assert_eq!(b.env.usage, before);
        b.env.usage = VerificationEnvironmentUsage::default();
        b.options.max_work_units = 0;
        assert!(b.charge(1, 1, 1, 1, 1).is_err());
        assert_eq!(b.env.usage, VerificationEnvironmentUsage::default());
    }
}

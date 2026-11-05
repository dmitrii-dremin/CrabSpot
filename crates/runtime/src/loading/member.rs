//! Domain-owned terminal member sites and preparation evidence.
use super::*;
use crate::{
    FieldAccessError, FieldId, FieldJavaError, FieldResolutionContext, FieldResolutionOptions,
    InstanceError, ManagedHeap, MethodAccessError, MethodJavaError,
};
use crate::{field::Selected, method::Resolved};
use crabspot_classfile::Constant;
use std::collections::HashSet;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemberResolutionLimits {
    pub max_prepared_classes: u64,
    pub max_sites: u64,
}
impl Default for MemberResolutionLimits {
    fn default() -> Self {
        Self {
            max_prepared_classes: 65_536,
            max_sites: 262_144,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MemberResolutionUsage {
    pub prepared_classes: u64,
    pub sites: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberResolutionResource {
    PreparedClasses,
    Sites,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberDescriptorPosition {
    Field,
    Parameter(usize),
    Return,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberResolutionFailure {
    NoSuchField,
    NoSuchMethod,
    IllegalAccess,
    IncompatibleClassChange,
    LinkageError,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberResolutionOutcome {
    Field(FieldId),
    Method(MethodId),
    Failure(MemberResolutionFailure),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberResolutionError {
    pub kind: MemberResolutionErrorKind,
    pub origin: Option<ClassId>,
    pub index: Option<u16>,
    pub position: Option<MemberDescriptorPosition>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemberResolutionErrorKind {
    Disabled,
    AlreadyEnabledWithDifferentLimits,
    MissingPreparation {
        class: ClassId,
    },
    MissingInitiation {
        origin: ClassId,
        loader: LoaderId,
    },
    InvalidConstant {
        origin: ClassId,
        index: u16,
    },
    UnprovenSelection {
        resolved: MethodId,
        selected: MethodId,
    },
    InvalidState(&'static str),
    ResourceLimit {
        resource: MemberResolutionResource,
        limit: u64,
        required: u64,
    },
    AccountingOverflow {
        resource: MemberResolutionResource,
    },
    AllocationFailure,
    Registry(Box<RegistryError>),
    Field(Box<FieldAccessError>),
    Loading(Box<LoadingConstraintError>),
}
impl fmt::Display for MemberResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "member {:?}/{:?} {:?}: {}",
            self.origin, self.index, self.position, self.kind
        )
    }
}
impl fmt::Display for MemberResolutionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            Self::Field(e) => e.fmt(f),
            Self::Loading(e) => e.fmt(f),
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for MemberResolutionErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e.as_ref()),
            Self::Field(e) => Some(e.as_ref()),
            Self::Loading(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl std::error::Error for MemberResolutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.kind)
    }
}
impl From<Box<MemberResolutionError>> for FieldAccessError {
    fn from(e: Box<MemberResolutionError>) -> Self {
        Self::MemberResolution(e)
    }
}
impl From<Box<MemberResolutionError>> for MethodAccessError {
    fn from(e: Box<MemberResolutionError>) -> Self {
        Self::MemberResolution(e)
    }
}
type M = MemberResolutionErrorKind;
type MR<T> = std::result::Result<T, Box<MemberResolutionError>>;
#[derive(Clone, Copy, Default)]
struct Site {
    origin: Option<ClassId>,
    index: Option<u16>,
    position: Option<MemberDescriptorPosition>,
}
impl Site {
    fn error(self, kind: M) -> Box<MemberResolutionError> {
        Box::new(MemberResolutionError {
            kind,
            origin: self.origin,
            index: self.index,
            position: self.position,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cached {
    Field(Selected),
    Method(Resolved),
    Failure(MemberResolutionFailure),
}
#[derive(Clone, Copy)]
pub(crate) struct Record {
    key: (ClassId, u16),
    owner: crate::ReferenceType,
    tag: u8,
    pub(crate) value: Cached,
}
impl Record {
    fn outcome(self) -> MemberResolutionOutcome {
        match self.value {
            Cached::Field(f) => MemberResolutionOutcome::Field(f.field),
            Cached::Method(m) => MemberResolutionOutcome::Method(m.method),
            Cached::Failure(f) => MemberResolutionOutcome::Failure(f),
        }
    }
}
pub(crate) struct Store {
    pub(crate) class_sites: Option<crate::mirror::ClassSites>,
    limits: MemberResolutionLimits,
    usage: MemberResolutionUsage,
    prepared: HashSet<ClassId>,
    sites: HashMap<(ClassId, u16), Record>,
}
impl Store {
    fn validate(&self) -> std::result::Result<(), M> {
        if self.usage.sites != self.sites.len() as u64
            || self.usage.prepared_classes != self.prepared.len() as u64
            || self.usage.sites > self.limits.max_sites
            || self.usage.prepared_classes > self.limits.max_prepared_classes
        {
            return Err(M::InvalidState("member inventory counts or caps differ"));
        }
        Ok(())
    }
    fn new(limits: MemberResolutionLimits) -> Self {
        Self {
            limits,
            usage: Default::default(),
            class_sites: None,
            prepared: HashSet::new(),
            sites: HashMap::new(),
        }
    }
}
pub(crate) struct Authority<'a> {
    ledger: &'a mut Ledger,
    pub(crate) store: &'a mut Store,
    work: Work,
    site: Site,
}
pub(crate) struct Ticket {
    key: (ClassId, u16),
    tag: u8,
    next: u64,
}
fn next(old: u64, limit: u64, resource: MemberResolutionResource) -> std::result::Result<u64, M> {
    let required = old
        .checked_add(1)
        .ok_or(M::AccountingOverflow { resource })?;
    if required > limit {
        return Err(M::ResourceLimit {
            resource,
            limit,
            required,
        });
    }
    Ok(required)
}
fn tag(registry: &DefinitionRegistry, origin: ClassId, index: u16) -> std::result::Result<u8, M> {
    let class = registry
        .class(origin)
        .map_err(|e| M::Registry(Box::new(e)))?;
    match class.constant_pool.get(index) {
        Ok(Constant::Fieldref { .. }) => Ok(9),
        Ok(Constant::Methodref { .. }) => Ok(10),
        Ok(Constant::InterfaceMethodref { .. }) => Ok(11),
        _ => Err(M::InvalidConstant { origin, index }),
    }
}
impl LoadingDomain {
    pub fn enable_member_resolution(&mut self, limits: MemberResolutionLimits) -> MR<()> {
        if let Some(store) = &self.members {
            if store.limits != limits {
                return Err(Site::default().error(M::AlreadyEnabledWithDifferentLimits));
            }
        } else {
            self.members = Some(Store::new(limits))
        }
        Ok(())
    }
    pub fn member_resolution_usage(&self) -> MemberResolutionUsage {
        self.members
            .as_ref()
            .map_or(Default::default(), |s| s.usage)
    }
    pub fn member_site(&self, origin: ClassId, index: u16) -> MR<Option<MemberResolutionOutcome>> {
        let site = Site {
            origin: Some(origin),
            index: Some(index),
            position: None,
        };
        let expected = tag(&self.registry, origin, index).map_err(|e| site.error(e))?;
        if let Some(store) = &self.members {
            store.validate().map_err(|e| site.error(e))?;
        }
        let Some(record) = self
            .members
            .as_ref()
            .and_then(|s| s.sites.get(&(origin, index)))
            .copied()
        else {
            return Ok(None);
        };
        validate_record(
            &self.registry,
            self.members.as_ref().and_then(|s| s.class_sites.as_ref()),
            (origin, index),
            expected,
            record,
        )
        .map_err(|e| site.error(e))?;
        Ok(Some(record.outcome()))
    }
    pub fn member_context_unnamed_modules<'a>(
        &'a mut self,
        heap: &'a mut ManagedHeap,
        fields: FieldResolutionOptions,
        limits: LoadingWorkLimits,
    ) -> MR<FieldResolutionContext<'a>> {
        let store = self
            .members
            .as_mut()
            .ok_or_else(|| Site::default().error(M::Disabled))?;
        store.validate().map_err(|e| Site::default().error(e))?;
        let bound = heap
            .instance_graph_registry()
            .map_err(|e| Site::default().error(M::Field(Box::new(e.into()))))?;
        if bound != Some(self.registry.identity()) {
            return Err(
                Site::default().error(M::Field(Box::new(InstanceError::ForeignRegistry.into())))
            );
        }
        let authority = Authority {
            ledger: &mut self.ledger,
            store,
            work: Work {
                limits,
                usage: Default::default(),
                site: None,
                #[cfg(test)]
                fail_reserve_after: None,
            },
            site: Default::default(),
        };
        let mut context =
            FieldResolutionContext::from_member_parts(&self.registry, heap, authority, fields);
        context.validate_member_snapshot()?;
        Ok(context)
    }
}
impl LoadingContext<'_> {
    pub fn prepare_member_owner(&mut self, plan: &HierarchyPlan) -> MR<()> {
        let site = Site {
            origin: Some(plan.root()),
            ..Default::default()
        };
        self.work.site = Some(LoadingPreparationSite {
            class: plan.root(),
            origin: None,
            related: None,
            position: None,
        });
        let store = self
            .domain
            .members
            .as_mut()
            .ok_or_else(|| site.error(M::Disabled))?;
        store.validate().map_err(|e| site.error(e))?;
        let facts = prepare::preflight(
            &self.domain.registry,
            &self.domain.ledger,
            &mut self.work,
            plan,
        )
        .map_err(|e| site.error(M::Loading(self.work.error(e))))?;
        self.work
            .step()
            .map_err(|e| site.error(M::Loading(self.work.error(e))))?;
        if store.prepared.contains(&plan.root()) {
            return Ok(());
        }
        self.work
            .step()
            .map_err(|e| site.error(M::Loading(self.work.error(e))))?;
        let next = next(
            store.usage.prepared_classes,
            store.limits.max_prepared_classes,
            MemberResolutionResource::PreparedClasses,
        )
        .map_err(|e| site.error(e))?;
        self.work
            .scratch(1)
            .map_err(|e| site.error(M::Loading(self.work.error(e))))?;
        self.work
            .reserving()
            .map_err(|e| site.error(M::Loading(self.work.error(e))))?;
        store
            .prepared
            .try_reserve(1)
            .map_err(|_| site.error(M::AllocationFailure))?;
        prepare::declared(
            &self.domain.registry,
            &facts,
            &mut self.domain.ledger,
            &mut self.work,
        )
        .and_then(|_| {
            prepare::inherited(
                &self.domain.registry,
                &facts,
                &mut self.domain.ledger,
                &mut self.work,
            )
        })
        .map_err(|e| site.error(M::Loading(self.work.error(e))))?;
        store.prepared.insert(plan.root());
        store.usage.prepared_classes = next;
        Ok(())
    }
}
fn validate_record(
    registry: &DefinitionRegistry,
    class_sites: Option<&crate::mirror::ClassSites>,
    key: (ClassId, u16),
    expected: u8,
    record: Record,
) -> std::result::Result<(), M> {
    if record.key != key {
        return Err(M::InvalidState("terminal CP key differs"));
    }
    if record.tag != expected {
        return Err(M::InvalidState("terminal CP tag differs"));
    }
    match record.owner {
        crate::ReferenceType::Class(class) => {
            registry
                .class(class)
                .map_err(|e| M::Registry(Box::new(e)))?;
        }
        crate::ReferenceType::Array {
            dimensions,
            element,
            defining_loader,
        } => {
            if dimensions == 0 {
                return Err(M::InvalidState("terminal array rank is zero"));
            }
            registry
                .lookup_class(defining_loader, &[88])
                .map_err(|e| M::Registry(Box::new(e)))?;
            if let crate::ArrayElement::Class(class) = element {
                if registry
                    .defining_loader(class)
                    .map_err(|e| M::Registry(Box::new(e)))?
                    != defining_loader
                {
                    return Err(M::InvalidState("terminal array leaf loader differs"));
                }
            }
            if expected == 9 {
                return Err(M::InvalidState("array field record"));
            }
            // The original immutable owner Class slot has already resolved in
            // the domain. Compare scalar identity, never reload/reparse a name:
            // this remains valid for terminal replay on a smaller unbound heap.
            let definition = registry
                .class(key.0)
                .map_err(|e| M::Registry(Box::new(e)))?;
            let class_index = match definition.constant_pool.get(key.1) {
                Ok(
                    Constant::Methodref { class_index, .. }
                    | Constant::InterfaceMethodref { class_index, .. },
                ) => *class_index,
                _ => return Err(M::InvalidState("array member has no owner Class slot")),
            };
            if class_sites
                .and_then(|s| s.rows.get(&(key.0, class_index)))
                .copied()
                != Some(crate::ClassSiteOutcome::Resolved(record.owner))
            {
                return Err(M::InvalidState(
                    "terminal structural owner differs from Class site",
                ));
            }
        }
    }
    match record.value {
        Cached::Field(field) if expected == 9 => {
            registry
                .field(field.field)
                .map_err(|e| M::Registry(Box::new(e)))?;
            if field.protected_receiver.is_some_and(|class| class != key.0) {
                return Err(M::InvalidState("terminal field qualifier differs"));
            }
            if crate::ReferenceType::Class(field.symbolic_owner) != record.owner {
                return Err(M::InvalidState("terminal field owner differs"));
            }
        }
        Cached::Method(method) if expected == 10 || expected == 11 => {
            registry
                .method(method.method)
                .map_err(|e| M::Registry(Box::new(e)))?;
            if method
                .protected_receiver
                .is_some_and(|class| class != key.0)
            {
                return Err(M::InvalidState("terminal method qualifier differs"));
            }
            if method.owner != record.owner {
                return Err(M::InvalidState("terminal method owner differs"));
            }
            if matches!(record.owner, crate::ReferenceType::Array { .. }) {
                let origin = registry
                    .class(key.0)
                    .map_err(|e| M::Registry(Box::new(e)))?;
                let class_index = match origin.constant_pool.get(key.1) {
                    Ok(
                        Constant::Methodref { class_index, .. }
                        | Constant::InterfaceMethodref { class_index, .. },
                    ) => *class_index,
                    _ => return Err(M::InvalidState("array record lacks method owner Class")),
                };
                let name = origin
                    .constant_pool
                    .class_name(class_index)
                    .map_err(|e| M::Registry(Box::new(RegistryError::Parse(e))))?;
                let declaring = registry
                    .class(method.method.owner())
                    .map_err(|e| M::Registry(Box::new(e)))?;
                let member = registry
                    .method(method.method)
                    .map_err(|e| M::Registry(Box::new(e)))?;
                if name.units().first() != Some(&91)
                    || method.protected_receiver.is_some()
                    || !declaring
                        .name()
                        .map_err(|e| M::Registry(Box::new(RegistryError::Parse(e))))?
                        .equals_ascii("java/lang/Object")
                    || !declaring
                        .constant_pool
                        .utf8(member.name_index)
                        .map_err(|e| M::Registry(Box::new(RegistryError::Parse(e))))?
                        .equals_ascii("getClass")
                    || !declaring
                        .constant_pool
                        .utf8(member.descriptor_index)
                        .map_err(|e| M::Registry(Box::new(RegistryError::Parse(e))))?
                        .equals_ascii("()Ljava/lang/Class;")
                {
                    return Err(M::InvalidState("array member role differs"));
                }
            }
        }
        Cached::Failure(MemberResolutionFailure::NoSuchMethod) if expected == 9 => {
            return Err(M::InvalidState("method failure at field site"));
        }
        Cached::Failure(MemberResolutionFailure::NoSuchField) if expected != 9 => {
            return Err(M::InvalidState("field failure at method site"));
        }
        Cached::Failure(_) => {}
        _ => return Err(M::InvalidState("terminal member kind differs")),
    }
    Ok(())
}
impl Authority<'_> {
    fn err(&self, kind: M) -> Box<MemberResolutionError> {
        self.site.error(kind)
    }
    fn loading<T>(&self, result: Result<T>) -> MR<T> {
        result.map_err(|e| self.err(M::Loading(self.work.error(e))))
    }
    fn actual(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> MR<Option<ClassId>> {
        let result = (|| {
            self.work.loader(registry, loader)?;
            self.work.name(name)?;
            self.ledger.lookup(name, loader, &mut self.work)
        })();
        self.loading(result)
    }
    fn witness(&mut self, class: ClassId) -> MR<()> {
        let result = self.work.step();
        self.loading(result)?;
        if !self.store.prepared.contains(&class) {
            return Err(self.err(M::MissingPreparation { class }));
        }
        Ok(())
    }
    fn begin(
        &mut self,
        registry: &DefinitionRegistry,
        heap: &ManagedHeap,
        origin: ClassId,
        index: u16,
    ) -> MR<std::result::Result<Record, Ticket>> {
        self.site = Site {
            origin: Some(origin),
            index: Some(index),
            position: None,
        };
        let result = (|| {
            let expected = tag(registry, origin, index).map_err(|e| self.err(e))?;
            let r = self.work.step();
            self.loading(r)?;
            self.store.validate().map_err(|e| self.err(e))?;
            if let Some(record) = self.store.sites.get(&(origin, index)).copied() {
                validate_record(
                    registry,
                    self.store.class_sites.as_ref(),
                    (origin, index),
                    expected,
                    record,
                )
                .map_err(|e| self.err(e))?;
                if !matches!(record.value, Cached::Failure(_)) {
                    match record.owner {
                        crate::ReferenceType::Class(class) => {
                            heap.instance_graph_node(class)
                                .map_err(|e| self.err(M::Field(Box::new(e.into()))))?;
                        }
                        crate::ReferenceType::Array {
                            dimensions: _,
                            element,
                            defining_loader,
                        } => {
                            let u = heap.reference_type_universe().ok_or_else(|| {
                                self.err(M::InvalidState(
                                    "array member requires a reference universe",
                                ))
                            })?;
                            let expected = match element {
                                crate::ArrayElement::Primitive(_) => u.bootstrap_loader,
                                crate::ArrayElement::Class(class) => {
                                    heap.instance_graph_node(class)
                                        .map_err(|e| self.err(M::Field(Box::new(e.into()))))?;
                                    registry
                                        .defining_loader(class)
                                        .map_err(|e| self.err(M::Registry(Box::new(e))))?
                                }
                            };
                            if expected != defining_loader {
                                return Err(
                                    self.err(M::InvalidState("terminal array loader differs"))
                                );
                            }
                        }
                    }
                    let declaring = match record.value {
                        Cached::Field(f) => f.field.owner(),
                        Cached::Method(m) => m.method.owner(),
                        _ => return Err(self.err(M::InvalidState("terminal success kind"))),
                    };
                    heap.instance_graph_node(declaring)
                        .map_err(|e| self.err(M::Field(Box::new(e.into()))))?;
                    if matches!(record.owner, crate::ReferenceType::Array { .. })
                        && heap
                            .reference_type_universe()
                            .is_none_or(|u| u.object != declaring)
                    {
                        return Err(
                            self.err(M::InvalidState("array member declaring Object differs"))
                        );
                    }
                }
                return Ok(Ok(record));
            }
            let r = self.work.step();
            self.loading(r)?;
            let count = next(
                self.store.usage.sites,
                self.store.limits.max_sites,
                MemberResolutionResource::Sites,
            )
            .map_err(|e| self.err(e))?;
            let r = self.work.scratch(1);
            self.loading(r)?;
            let r = self.work.reserving();
            self.loading(r)?;
            self.store
                .sites
                .try_reserve(1)
                .map_err(|_| self.err(M::AllocationFailure))?;
            Ok(Err(Ticket {
                key: (origin, index),
                tag: expected,
                next: count,
            }))
        })();
        if !matches!(result, Ok(Err(_))) {
            self.site = Default::default();
        }
        result
    }
    fn commit(
        &mut self,
        registry: &DefinitionRegistry,
        ticket: Ticket,
        owner: crate::ReferenceType,
        value: Cached,
    ) -> MR<()> {
        // All fallible allocation and logical funding happened before resolution.
        // Exclusive callback-free access keeps this reserved slot available. These
        // scalar/handle guards reject damaged private staging before publication.
        self.store.validate().map_err(|e| self.err(e))?;
        let expected = tag(registry, ticket.key.0, ticket.key.1).map_err(|e| self.err(e))?;
        let next = next(
            self.store.usage.sites,
            self.store.limits.max_sites,
            MemberResolutionResource::Sites,
        )
        .map_err(|e| self.err(e))?;
        if ticket.tag != expected
            || ticket.next != next
            || self.store.sites.contains_key(&ticket.key)
            || self.store.sites.capacity() <= self.store.sites.len()
            || self.site.origin != Some(ticket.key.0)
            || self.site.index != Some(ticket.key.1)
        {
            return Err(self.err(M::InvalidState("terminal reservation differs")));
        }
        let record = Record {
            key: ticket.key,
            owner,
            tag: ticket.tag,
            value,
        };
        validate_record(
            registry,
            self.store.class_sites.as_ref(),
            ticket.key,
            expected,
            record,
        )
        .map_err(|e| self.err(e))?;
        self.store.sites.insert(ticket.key, record);
        self.store.usage.sites = ticket.next;
        Ok(())
    }
    fn descriptor(
        &mut self,
        registry: &DefinitionRegistry,
        units: &[u16],
        field: bool,
        left: LoaderId,
        right: LoaderId,
        selection: Option<(MethodId, MethodId)>,
    ) -> MR<bool> {
        self.site.position = field.then_some(MemberDescriptorPosition::Field);
        let r = self.work.utf(units.len());
        self.loading(r)?;
        let mut p = 0;
        let mut parameter = 0;
        let mut returns = field;
        if !field {
            if units.first() != Some(&40) {
                return Err(self.err(M::InvalidState("member method descriptor")));
            }
            p = 1;
        }
        loop {
            if !field && !returns && units.get(p) == Some(&41) {
                p += 1;
                returns = true;
            }
            self.site.position = Some(if field {
                MemberDescriptorPosition::Field
            } else if returns {
                MemberDescriptorPosition::Return
            } else {
                MemberDescriptorPosition::Parameter(parameter)
            });
            let mut rank = 0;
            while units.get(p) == Some(&91) {
                rank += 1;
                p += 1;
                if rank > 255 {
                    return Err(self.err(M::InvalidState("member array rank")));
                }
            }
            let t = *units
                .get(p)
                .ok_or_else(|| self.err(M::InvalidState("truncated member descriptor")))?;
            p += 1;
            match t {
                66 | 67 | 68 | 70 | 73 | 74 | 83 | 90 => {}
                86 if returns && rank == 0 && !field => {}
                76 => {
                    let start = p;
                    while units.get(p).is_some_and(|u| *u != 59) {
                        p += 1;
                    }
                    if units.get(p) != Some(&59) {
                        return Err(self.err(M::InvalidState("member nominal delimiter")));
                    }
                    let name = &units[start..p];
                    p += 1;
                    if left != right {
                        if let Some((resolved, selected)) = selection {
                            let r = (|| {
                                self.work.loader(registry, left)?;
                                self.work.loader(registry, right)?;
                                self.work.name(name)?;
                                self.ledger.constrained(name, left, right, &mut self.work)
                            })();
                            if !self.loading(r)? {
                                let a = self.actual(registry, left, name)?;
                                let b = if a.is_some() {
                                    self.actual(registry, right, name)?
                                } else {
                                    None
                                };
                                if a.is_none() || a != b {
                                    return Err(
                                        self.err(M::UnprovenSelection { resolved, selected })
                                    );
                                }
                            }
                        } else {
                            let r = (|| {
                                self.work.loader(registry, left)?;
                                self.work.loader(registry, right)?;
                                self.work.name(name)?;
                                self.ledger.impose(name, left, right, &mut self.work)
                            })();
                            match r {
                                Err(E::ConstraintConflict { .. }) => return Ok(false),
                                other => self.loading(other)?,
                            }
                        }
                    }
                }
                _ => return Err(self.err(M::InvalidState("invalid member descriptor type"))),
            }
            if field || returns {
                if p != units.len() {
                    return Err(self.err(M::InvalidState("trailing member descriptor")));
                }
                self.site.position = None;
                return Ok(true);
            }
            parameter += 1;
            if parameter > 255 {
                return Err(self.err(M::InvalidState("member parameter count")));
            }
        }
    }
}

impl<'a> FieldResolutionContext<'a> {
    pub fn member_loading_work(&self) -> Option<LoadingWorkUsage> {
        self.member.as_ref().map(|a| a.work.usage)
    }
    pub(crate) fn is_member_checked(&self) -> bool {
        self.member.is_some()
    }
    fn validate_member_snapshot(&mut self) -> MR<()> {
        let registry = self.registry;
        let mut seen = 0usize;
        for (class, metadata) in registry.definition_rows() {
            let a = self
                .member
                .as_mut()
                .ok_or_else(|| Site::default().error(M::Disabled))?;
            a.site = Site {
                origin: Some(class),
                ..Default::default()
            };
            let r = a.work.step();
            a.loading(r)?;
            let present = self
                .heap
                .instance_graph_contains(class)
                .map_err(|e| a.err(M::Field(Box::new(e.into()))))?;
            if !present {
                continue;
            }
            seen = seen
                .checked_add(1)
                .ok_or_else(|| a.err(M::InvalidState("admitted count overflow")))?;
            crate::field::step(&mut self.steps, self.options)
                .map_err(|e| a.err(M::Field(Box::new(e))))?;
            let r = a.work.step();
            a.loading(r)?;
            let node = self
                .heap
                .instance_graph_node(class)
                .map_err(|e| a.err(M::Field(Box::new(e.into()))))?;
            if metadata.access_flags != node.flags
                || (metadata.super_class == 0) != node.superclass.is_none()
                || metadata.interfaces.len() != node.interfaces.len()
            {
                return Err(a.err(M::InvalidState("heap metadata differs from domain")));
            }
            let loader = registry
                .defining_loader(class)
                .map_err(|e| a.err(M::Registry(Box::new(e))))?;
            let r = a.work.charge(0, 2, 0);
            a.loading(r)?;
            let name = metadata
                .name()
                .map_err(|e| a.err(M::Registry(Box::new(RegistryError::Parse(e)))))?;
            match a.actual(registry, loader, name.units())? {
                Some(actual) if actual == class => {}
                Some(_) => {
                    return Err(a.err(M::InvalidState("heap edge differs from actual initiation")));
                }
                None => {
                    return Err(a.err(M::MissingInitiation {
                        origin: class,
                        loader,
                    }));
                }
            }
            if let Some(parent) = node.superclass {
                crate::field::step(&mut self.steps, self.options)
                    .map_err(|e| a.err(M::Field(Box::new(e))))?;
                let r = a.work.step();
                a.loading(r)?;
                self.heap
                    .instance_graph_node(parent)
                    .map_err(|e| a.err(M::Field(Box::new(e.into()))))?;
                let r = a.work.charge(0, 2, 0);
                a.loading(r)?;
                let name = metadata
                    .constant_pool
                    .class_name(metadata.super_class)
                    .map_err(|e| a.err(M::Registry(Box::new(RegistryError::Parse(e)))))?;
                match a.actual(registry, loader, name.units())? {
                    Some(actual) if actual == parent => {}
                    Some(_) => {
                        return Err(
                            a.err(M::InvalidState("heap edge differs from actual initiation"))
                        );
                    }
                    None => {
                        return Err(a.err(M::MissingInitiation {
                            origin: class,
                            loader,
                        }));
                    }
                }
            }
            for (index, interface) in metadata.interfaces.iter().zip(node.interfaces) {
                crate::field::step(&mut self.steps, self.options)
                    .map_err(|e| a.err(M::Field(Box::new(e))))?;
                let r = a.work.step();
                a.loading(r)?;
                self.heap
                    .instance_graph_node(*interface)
                    .map_err(|e| a.err(M::Field(Box::new(e.into()))))?;
                let r = a.work.charge(0, 2, 0);
                a.loading(r)?;
                let name = metadata
                    .constant_pool
                    .class_name(*index)
                    .map_err(|e| a.err(M::Registry(Box::new(RegistryError::Parse(e)))))?;
                match a.actual(registry, loader, name.units())? {
                    Some(actual) if actual == *interface => {}
                    Some(_) => {
                        return Err(
                            a.err(M::InvalidState("heap edge differs from actual initiation"))
                        );
                    }
                    None => {
                        return Err(a.err(M::MissingInitiation {
                            origin: class,
                            loader,
                        }));
                    }
                }
            }
            a.witness(class)?;
        }
        let a = self
            .member
            .as_mut()
            .ok_or_else(|| Site::default().error(M::Disabled))?;
        if seen != self.heap.retained_layout_classes() {
            return Err(a.err(M::InvalidState("heap contains a class outside domain")));
        }
        a.site = Default::default();
        Ok(())
    }
    pub(crate) fn member_type_name(
        &mut self,
        origin: ClassId,
        name: &'a [u16],
    ) -> std::result::Result<ClassId, FieldAccessError> {
        let registry = self.registry;
        let a = self
            .member
            .as_mut()
            .ok_or_else(|| Site::default().error(M::Disabled))?;
        if a.site.index.is_none() {
            a.site = Site {
                origin: Some(origin),
                ..Default::default()
            }
        }
        let result: MR<ClassId> = (|| {
            let loader = registry
                .defining_loader(origin)
                .map_err(|e| a.err(M::Registry(Box::new(e))))?;
            let class = a
                .actual(registry, loader, name)?
                .ok_or_else(|| a.err(M::MissingInitiation { origin, loader }))?;
            self.heap
                .instance_graph_node(class)
                .map_err(|e| a.err(M::Field(Box::new(e.into()))))?;
            Ok(class)
        })();
        if a.site.index.is_none() {
            a.site = Default::default()
        }
        result.map_err(FieldAccessError::from)
    }
    pub(crate) fn member_begin(
        &mut self,
        origin: ClassId,
        index: u16,
    ) -> MR<std::result::Result<Cached, Ticket>> {
        self.unchanged().map_err(|e| {
            Site {
                origin: Some(origin),
                index: Some(index),
                position: None,
            }
            .error(M::Field(Box::new(e)))
        })?;
        self.registry.class(origin).map_err(|e| {
            Site {
                origin: Some(origin),
                index: Some(index),
                position: None,
            }
            .error(M::Registry(Box::new(e)))
        })?;
        self.heap.instance_graph_node(origin).map_err(|e| {
            Site {
                origin: Some(origin),
                index: Some(index),
                position: None,
            }
            .error(M::Field(Box::new(e.into())))
        })?;
        let a = self
            .member
            .as_mut()
            .ok_or_else(|| Site::default().error(M::Disabled))?;
        a.begin(self.registry, self.heap, origin, index)
            .map(|r| r.map(|record| record.value))
    }
    pub(crate) fn member_emit(
        &mut self,
        origin: ClassId,
        declaring: ClassId,
        units: &[u16],
        field: bool,
    ) -> MR<bool> {
        let registry = self.registry;
        let a = self
            .member
            .as_mut()
            .ok_or_else(|| Site::default().error(M::Disabled))?;
        let left = registry
            .defining_loader(origin)
            .map_err(|e| a.err(M::Registry(Box::new(e))))?;
        let right = registry
            .defining_loader(declaring)
            .map_err(|e| a.err(M::Registry(Box::new(e))))?;
        a.descriptor(registry, units, field, left, right, None)
    }
    pub(crate) fn member_finish_field(
        &mut self,
        ticket: Ticket,
        owner: Option<ClassId>,
        result: std::result::Result<Selected, FieldAccessError>,
    ) -> std::result::Result<Selected, FieldAccessError> {
        let Some(a) = &mut self.member else {
            return result;
        };
        let value = match &result {
            Ok(f) => Some(Cached::Field(*f)),
            Err(FieldAccessError::Java(e)) => field_failure(*e).map(Cached::Failure),
            _ => None,
        };
        if let (Some(owner), Some(value)) = (owner, value) {
            if let Err(error) = a.commit(
                self.registry,
                ticket,
                crate::ReferenceType::Class(owner),
                value,
            ) {
                a.site = Default::default();
                return Err(error.into());
            }
        }
        a.site = Default::default();
        result
    }
    pub(crate) fn member_finish_method(
        &mut self,
        ticket: Ticket,
        owner: Option<crate::ReferenceType>,
        result: std::result::Result<Resolved, MethodAccessError>,
    ) -> std::result::Result<Resolved, MethodAccessError> {
        let Some(a) = &mut self.member else {
            return result;
        };
        let value = match &result {
            Ok(m) => Some(Cached::Method(*m)),
            Err(MethodAccessError::Java(e)) => method_failure(*e).map(Cached::Failure),
            _ => None,
        };
        if let (Some(owner), Some(value)) = (owner, value) {
            if let Err(error) = a.commit(self.registry, ticket, owner, value) {
                a.site = Default::default();
                return Err(error.into());
            }
        }
        a.site = Default::default();
        result
    }
    pub(crate) fn member_selected(
        &mut self,
        origin: ClassId,
        index: u16,
        resolved: MethodId,
        selected: MethodId,
        descriptor: &[u16],
    ) -> std::result::Result<(), MethodAccessError> {
        let Some(a) = &mut self.member else {
            return Ok(());
        };
        a.site = Site {
            origin: Some(origin),
            index: Some(index),
            position: None,
        };
        let registry = self.registry;
        let result: MR<()> = (|| {
            let left = registry
                .defining_loader(resolved.owner())
                .map_err(|e| a.err(M::Registry(Box::new(e))))?;
            let right = registry
                .defining_loader(selected.owner())
                .map_err(|e| a.err(M::Registry(Box::new(e))))?;
            if left != right {
                a.descriptor(
                    registry,
                    descriptor,
                    false,
                    left,
                    right,
                    Some((resolved, selected)),
                )?;
            }
            Ok(())
        })();
        a.site = Default::default();
        result.map_err(MethodAccessError::from)
    }
}
fn field_failure(error: FieldJavaError) -> Option<MemberResolutionFailure> {
    use MemberResolutionFailure as F;
    Some(match error {
        FieldJavaError::NoSuchField => F::NoSuchField,
        FieldJavaError::IllegalAccess => F::IllegalAccess,
        FieldJavaError::IncompatibleClassChange => F::IncompatibleClassChange,
        FieldJavaError::LinkageError => F::LinkageError,
        _ => return None,
    })
}
fn method_failure(error: MethodJavaError) -> Option<MemberResolutionFailure> {
    use MemberResolutionFailure as F;
    Some(match error {
        MethodJavaError::NoSuchMethod => F::NoSuchMethod,
        MethodJavaError::IllegalAccess => F::IllegalAccess,
        MethodJavaError::IncompatibleClassChange => F::IncompatibleClassChange,
        MethodJavaError::LinkageError => F::LinkageError,
        _ => return None,
    })
}
pub(crate) fn field_hit(value: Cached) -> std::result::Result<Selected, FieldAccessError> {
    use MemberResolutionFailure as F;
    match value {
        Cached::Field(f) => Ok(f),
        Cached::Failure(f) => Err(FieldAccessError::Java(match f {
            F::NoSuchField => FieldJavaError::NoSuchField,
            F::IllegalAccess => FieldJavaError::IllegalAccess,
            F::IncompatibleClassChange => FieldJavaError::IncompatibleClassChange,
            F::LinkageError => FieldJavaError::LinkageError,
            _ => {
                return Err(FieldAccessError::InvalidState(
                    "method failure at field site",
                ));
            }
        })),
        _ => Err(FieldAccessError::InvalidState(
            "method success at field site",
        )),
    }
}
pub(crate) fn method_hit(value: Cached) -> std::result::Result<Resolved, MethodAccessError> {
    use MemberResolutionFailure as F;
    match value {
        Cached::Method(m) => Ok(m),
        Cached::Failure(f) => Err(MethodAccessError::Java(match f {
            F::NoSuchMethod => MethodJavaError::NoSuchMethod,
            F::IllegalAccess => MethodJavaError::IllegalAccess,
            F::IncompatibleClassChange => MethodJavaError::IncompatibleClassChange,
            F::LinkageError => MethodJavaError::LinkageError,
            _ => return Err(FieldAccessError::InvalidState("field failure at method site").into()),
        })),
        _ => Err(FieldAccessError::InvalidState("field success at method site").into()),
    }
}

#[cfg(test)]
#[path = "member_tests.rs"]
mod tests;

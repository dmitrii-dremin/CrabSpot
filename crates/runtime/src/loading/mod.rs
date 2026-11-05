//! Checked initiating records and name-specific preparation obligations.
mod ledger;
pub(crate) mod member;
pub use member::*;
mod prepare;
use crate::{
    ClassId, DefinitionRegistry, HierarchyPlan, LoaderId, MethodId, RegistryError, RegistryOptions,
};
use crabspot_classfile::ParseOptions;
use std::{collections::HashMap, fmt, hash::BuildHasher};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadingDomainLimits {
    pub max_names: u64,
    pub max_name_units: u64,
    pub max_pairs: u64,
    pub max_initiations: u64,
    pub max_unions: u64,
}
impl Default for LoadingDomainLimits {
    fn default() -> Self {
        Self {
            max_names: 65_536,
            max_name_units: 4_194_304,
            max_pairs: 262_144,
            max_initiations: 131_072,
            max_unions: 262_144,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LoadingDomainUsage {
    pub names: u64,
    pub name_units: u64,
    pub pairs: u64,
    pub initiations: u64,
    pub unions: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadingWorkLimits {
    pub max_utf16_units: u64,
    pub max_work_units: u64,
    pub max_scratch_entries: u64,
}
impl Default for LoadingWorkLimits {
    fn default() -> Self {
        Self {
            max_utf16_units: 16_777_216,
            max_work_units: 67_108_864,
            max_scratch_entries: 4_194_304,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LoadingWorkUsage {
    pub utf16_units: u64,
    pub work_units: u64,
    pub scratch_entries: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadingConstraintResource {
    Names,
    NameUnits,
    Pairs,
    Initiations,
    Unions,
    Utf16Units,
    WorkUnits,
    ScratchEntries,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadingDescriptorPosition {
    Parameter(usize),
    Return,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadingPreparationSite {
    pub class: ClassId,
    pub origin: Option<MethodId>,
    pub related: Option<MethodId>,
    pub position: Option<LoadingDescriptorPosition>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadingConstraintError {
    pub kind: LoadingConstraintErrorKind,
    pub site: Option<LoadingPreparationSite>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadingConstraintErrorKind {
    Registry(Box<RegistryError>),
    WrongReturnedName {
        loader: LoaderId,
        class: ClassId,
    },
    DuplicateDerivation {
        loader: LoaderId,
        existing: ClassId,
    },
    InitiationConflict {
        loader: LoaderId,
        existing: ClassId,
        incoming: ClassId,
    },
    DefinitionConflict {
        loader: LoaderId,
        required: ClassId,
    },
    ConstraintConflict {
        left: LoaderId,
        right: LoaderId,
        left_class: ClassId,
        right_class: ClassId,
    },
    MissingInitiation {
        origin: ClassId,
        dependency: ClassId,
    },
    InconsistentHierarchy {
        class: ClassId,
    },
    InvalidMetadata(&'static str),
    ResourceLimit {
        resource: LoadingConstraintResource,
        limit: u64,
        required: u64,
    },
    AccountingOverflow {
        resource: LoadingConstraintResource,
    },
    AllocationFailure,
}
impl fmt::Display for LoadingConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "loading {:?}: {}", self.site, self.kind)
    }
}
impl fmt::Display for LoadingConstraintErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for LoadingConstraintErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl std::error::Error for LoadingConstraintError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.kind)
    }
}
type E = LoadingConstraintErrorKind;
type R = LoadingConstraintResource;
type Result<T> = std::result::Result<T, E>;
impl From<RegistryError> for E {
    fn from(e: RegistryError) -> Self {
        Self::Registry(Box::new(e))
    }
}
fn invalid(why: &'static str) -> E {
    E::InvalidMetadata(why)
}
fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<()> {
    v.try_reserve(n).map_err(|_| E::AllocationFailure)
}
fn amount(n: usize, r: R) -> Result<u64> {
    u64::try_from(n).map_err(|_| E::AccountingOverflow { resource: r })
}
struct Work {
    limits: LoadingWorkLimits,
    usage: LoadingWorkUsage,
    site: Option<LoadingPreparationSite>,
    #[cfg(test)]
    fail_reserve_after: Option<usize>,
}
impl Work {
    fn reserving(&mut self) -> Result<()> {
        #[cfg(test)]
        if let Some(left) = &mut self.fail_reserve_after {
            if *left == 0 {
                return Err(E::AllocationFailure);
            }
            *left -= 1;
        }
        Ok(())
    }
    fn charge(&mut self, utf: u64, work: u64, scratch: u64) -> Result<()> {
        let values = [
            (
                self.usage.utf16_units,
                utf,
                self.limits.max_utf16_units,
                R::Utf16Units,
            ),
            (
                self.usage.work_units,
                work,
                self.limits.max_work_units,
                R::WorkUnits,
            ),
            (
                self.usage.scratch_entries,
                scratch,
                self.limits.max_scratch_entries,
                R::ScratchEntries,
            ),
        ];
        let mut next = [0; 3];
        for (out, (old, n, limit, resource)) in next.iter_mut().zip(values) {
            *out = old
                .checked_add(n)
                .ok_or(E::AccountingOverflow { resource })?;
            if *out > limit {
                return Err(E::ResourceLimit {
                    resource,
                    limit,
                    required: *out,
                });
            }
        }
        self.usage = LoadingWorkUsage {
            utf16_units: next[0],
            work_units: next[1],
            scratch_entries: next[2],
        };
        Ok(())
    }
    fn step(&mut self) -> Result<()> {
        self.charge(0, 1, 0)
    }
    fn utf(&mut self, n: usize) -> Result<()> {
        let n = amount(n, R::Utf16Units)?;
        self.charge(n, n, 0)
    }
    fn scratch(&mut self, n: usize) -> Result<()> {
        let n = amount(n, R::ScratchEntries)?;
        self.charge(0, n, n)
    }
    fn loader(&mut self, registry: &DefinitionRegistry, id: LoaderId) -> Result<()> {
        self.step()?;
        registry.loader_index(id)?;
        Ok(())
    }
    fn name(&mut self, name: &[u16]) -> Result<()> {
        if name.len() > usize::from(u16::MAX) {
            crate::registry::validate_name(name)?;
        }
        self.utf(name.len())?;
        crate::registry::validate_name(name)?;
        Ok(())
    }
    fn error(&self, kind: E) -> Box<LoadingConstraintError> {
        Box::new(LoadingConstraintError {
            kind,
            site: self.site,
        })
    }
}
struct Pair {
    parent: usize,
    size: usize,
    known: Option<ClassId>,
    actual: Option<ClassId>,
}
struct Ledger {
    limits: LoadingDomainLimits,
    usage: LoadingDomainUsage,
    names: Vec<Vec<u16>>,
    name_index: HashMap<u64, Vec<usize>>,
    hash: std::collections::hash_map::RandomState,
    pairs: Vec<Pair>,
    pair_index: HashMap<(usize, LoaderId), usize>,
}
impl Ledger {
    fn new(limits: LoadingDomainLimits) -> Self {
        Self {
            limits,
            usage: LoadingDomainUsage::default(),
            names: Vec::new(),
            name_index: HashMap::new(),
            hash: Default::default(),
            pairs: Vec::new(),
            pair_index: HashMap::new(),
        }
    }
}
pub struct LoadingDomain {
    registry: DefinitionRegistry,
    ledger: Ledger,
    pub(crate) members: Option<member::Store>,
}
impl fmt::Debug for LoadingDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadingDomain")
            .field("usage", &self.ledger.usage)
            .field("definitions", &self.registry.definition_count())
            .finish_non_exhaustive()
    }
}
pub struct LoadingContext<'a> {
    domain: &'a mut LoadingDomain,
    work: Work,
}
impl fmt::Debug for LoadingContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadingContext")
            .field("usage", &self.work.usage)
            .field("domain_usage", &self.domain.usage())
            .finish_non_exhaustive()
    }
}
impl LoadingDomain {
    pub fn new(
        registry: RegistryOptions,
        limits: LoadingDomainLimits,
    ) -> std::result::Result<Self, Box<LoadingConstraintError>> {
        Ok(Self {
            registry: DefinitionRegistry::new(registry).map_err(|e| {
                Box::new(LoadingConstraintError {
                    kind: e.into(),
                    site: None,
                })
            })?,
            ledger: Ledger::new(limits),
            members: None,
        })
    }
    pub fn registry(&self) -> &DefinitionRegistry {
        &self.registry
    }
    pub fn create_loader(&mut self) -> std::result::Result<LoaderId, Box<LoadingConstraintError>> {
        self.registry.create_loader().map_err(|e| {
            Box::new(LoadingConstraintError {
                kind: e.into(),
                site: None,
            })
        })
    }
    pub fn usage(&self) -> LoadingDomainUsage {
        self.ledger.usage
    }
    pub fn context(&mut self, limits: LoadingWorkLimits) -> LoadingContext<'_> {
        LoadingContext {
            domain: self,
            work: Work {
                limits,
                usage: LoadingWorkUsage::default(),
                site: None,
                #[cfg(test)]
                fail_reserve_after: None,
            },
        }
    }
}
impl LoadingContext<'_> {
    pub fn usage(&self) -> LoadingWorkUsage {
        self.work.usage
    }
    pub fn domain_usage(&self) -> LoadingDomainUsage {
        self.domain.usage()
    }
    pub fn lookup_initiated(
        &mut self,
        loader: LoaderId,
        name: &[u16],
    ) -> std::result::Result<Option<ClassId>, Box<LoadingConstraintError>> {
        self.work.site = None;
        let result = (|| {
            self.work.loader(&self.domain.registry, loader)?;
            self.work.name(name)?;
            self.domain.ledger.lookup(name, loader, &mut self.work)
        })();
        result.map_err(|e| self.work.error(e))
    }
    pub fn constrained(
        &mut self,
        name: &[u16],
        left: LoaderId,
        right: LoaderId,
    ) -> std::result::Result<bool, Box<LoadingConstraintError>> {
        self.work.site = None;
        let result = (|| {
            self.work.loader(&self.domain.registry, left)?;
            self.work.loader(&self.domain.registry, right)?;
            self.work.name(name)?;
            self.domain
                .ledger
                .constrained(name, left, right, &mut self.work)
        })();
        result.map_err(|e| self.work.error(e))
    }
    pub fn impose(
        &mut self,
        name: &[u16],
        left: LoaderId,
        right: LoaderId,
    ) -> std::result::Result<(), Box<LoadingConstraintError>> {
        self.work.site = None;
        let result = (|| {
            self.work.loader(&self.domain.registry, left)?;
            self.work.loader(&self.domain.registry, right)?;
            self.work.name(name)?;
            self.domain.ledger.impose(name, left, right, &mut self.work)
        })();
        result.map_err(|e| self.work.error(e))
    }
    pub fn record_initiation(
        &mut self,
        loader: LoaderId,
        name: &[u16],
        class: ClassId,
    ) -> std::result::Result<(), Box<LoadingConstraintError>> {
        self.work.site = None;
        let result = (|| {
            let registry = &self.domain.registry;
            self.work.loader(registry, loader)?;
            self.work.name(name)?;
            self.work.step()?;
            let definition = registry.class(class)?;
            self.work.charge(0, 2, 0)?;
            let actual = definition.name().map_err(RegistryError::Parse)?.units();
            if !equal(&mut self.work, name, actual)? {
                return Err(E::WrongReturnedName { loader, class });
            }
            let plan = self
                .domain
                .ledger
                .bind(name, loader, Some(class), &mut self.work)?;
            if let Some(plan) = plan {
                self.domain.ledger.commit_bind(plan, class);
            }
            Ok(())
        })();
        result.map_err(|e| self.work.error(e))
    }
    pub fn define(
        &mut self,
        loader: LoaderId,
        name: &[u16],
        bytes: &[u8],
        options: ParseOptions,
    ) -> std::result::Result<ClassId, Box<LoadingConstraintError>> {
        self.work.site = None;
        let result = (|| {
            self.work.loader(&self.domain.registry, loader)?;
            self.work.name(name)?;
            let query = self.domain.ledger.name_id(name, &mut self.work)?;
            if let Some(existing) =
                self.domain
                    .ledger
                    .lookup_queried(query, loader, &mut self.work)?
            {
                return Err(E::DuplicateDerivation { loader, existing });
            }
            let prepared = self
                .domain
                .registry
                .stage_definition(loader, name, bytes, options)?;
            let plan = self
                .domain
                .ledger
                .bind_queried(name, loader, None, query, &mut self.work)?
                .ok_or(invalid("fresh derivation already initiated"))?;
            self.work.step()?;
            self.domain.registry.validate_definition_stage(&prepared)?;
            let class = self.domain.registry.publish_definition(prepared);
            self.domain.ledger.commit_bind(plan, class);
            Ok(class)
        })();
        result.map_err(|e| self.work.error(e))
    }
    pub fn prepare_constraints(
        &mut self,
        plan: &HierarchyPlan,
    ) -> std::result::Result<(), Box<LoadingConstraintError>> {
        self.work.site = Some(LoadingPreparationSite {
            class: plan.root(),
            origin: None,
            related: None,
            position: None,
        });
        prepare::prepare(
            &self.domain.registry,
            &mut self.domain.ledger,
            &mut self.work,
            plan,
        )
        .map_err(|e| self.work.error(e))
    }
}
fn equal(work: &mut Work, a: &[u16], b: &[u16]) -> Result<bool> {
    work.step()?;
    equal_paid(work, a, b)
}
fn equal_paid(work: &mut Work, a: &[u16], b: &[u16]) -> Result<bool> {
    if a.len() != b.len() {
        return Ok(false);
    }
    work.utf(a.len())?;
    Ok(a == b)
}
#[cfg(test)]
mod tests;

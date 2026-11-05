use crate::error::Result;
use crate::{
    ApplicationError, ApplicationErrorKind as K, ApplicationResource as R, ApplicationStage as S,
    ApplicationUsage, ImageLimits, OrchestrationUsage,
};
pub(crate) struct Meter {
    pub(crate) limits: ImageLimits,
    pub(crate) usage: OrchestrationUsage,
}
impl Meter {
    pub(crate) fn new(limits: ImageLimits) -> Self {
        Self {
            limits,
            usage: Default::default(),
        }
    }
    pub(crate) fn snapshot(&self, mut usage: ApplicationUsage) -> ApplicationUsage {
        usage.orchestration = self.usage;
        usage
    }
    pub(crate) fn error(
        &self,
        stage: S,
        kind: K,
        usage: ApplicationUsage,
    ) -> Box<ApplicationError> {
        ApplicationError::new(stage, kind, self.snapshot(usage))
    }
    pub(crate) fn add(old: u64, n: u64, limit: u64, resource: R) -> std::result::Result<u64, K> {
        let required = old
            .checked_add(n)
            .ok_or(K::AccountingOverflow { resource })?;
        if required > limit {
            Err(K::ResourceLimit {
                resource,
                limit,
                required,
            })
        } else {
            Ok(required)
        }
    }
    pub(crate) fn charge(
        &mut self,
        utf: u64,
        work: u64,
        scratch: u64,
        stage: S,
        usage: ApplicationUsage,
    ) -> Result<()> {
        let check = (|| {
            Ok::<_, K>((
                Self::add(
                    self.usage.utf16_units,
                    utf,
                    self.limits.max_utf16_units,
                    R::Utf16Units,
                )?,
                Self::add(
                    self.usage.work_units,
                    work,
                    self.limits.max_inventory_work,
                    R::WorkUnits,
                )?,
                Self::add(
                    self.usage.scratch_entries,
                    scratch,
                    self.limits.max_scratch_entries,
                    R::ScratchEntries,
                )?,
            ))
        })();
        let (u, w, s) = check.map_err(|e| self.error(stage, e, usage))?;
        self.usage.utf16_units = u;
        self.usage.work_units = w;
        self.usage.scratch_entries = s;
        Ok(())
    }
    pub(crate) fn work(&mut self, n: u64, stage: S, usage: ApplicationUsage) -> Result<()> {
        self.charge(0, n, 0, stage, usage)
    }
    pub(crate) fn utf(&mut self, n: usize, stage: S, usage: ApplicationUsage) -> Result<()> {
        self.charge(n as u64, n as u64, 0, stage, usage)
    }
    pub(crate) fn retain(&mut self, n: usize, stage: S, usage: ApplicationUsage) -> Result<()> {
        self.charge(0, n as u64, n as u64, stage, usage)
    }
    pub(crate) fn inventory(&mut self, n: u64, usage: ApplicationUsage) -> Result<()> {
        let next = Self::add(
            self.usage.inventory_work,
            n,
            self.limits.max_inventory_work,
            R::InventoryWork,
        )
        .map_err(|e| self.error(S::Input, e, usage))?;
        self.charge(0, n, 0, S::Input, usage)?;
        self.usage.inventory_work = next;
        Ok(())
    }
    pub(crate) fn alloc(&self, stage: S, usage: ApplicationUsage) -> Box<ApplicationError> {
        self.error(stage, K::AllocationFailure, usage)
    }
}

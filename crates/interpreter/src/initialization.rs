//! Single-owner initialization records. Only the shared engine can complete them.
use crate::{
    CallLimits, ConstructionLimits, ExceptionLimits, ExceptionOutcome, ExecutionError,
    ExecutionErrorKind as E, ExecutionOptions, Value, construction::ConstructionState,
};
use crabspot_runtime::{
    ClassId, ExceptionProfile, FieldResolutionContext, HeapRef, InitializationAccessError,
    InitializationClaim, InitializationPlan, MethodId, Reference, StaticStateProfile,
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitializationLimits {
    pub max_classes: usize,
    pub max_plan_dependencies: usize,
    pub max_constant_values: usize,
    pub max_pending: usize,
    pub max_steps: u64,
}
impl Default for InitializationLimits {
    fn default() -> Self {
        Self {
            max_classes: 16_384,
            max_plan_dependencies: 1_048_576,
            max_constant_values: 65_536,
            max_pending: 16_384,
            max_steps: 1_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationResource {
    Classes,
    PlanDependencies,
    ConstantValues,
    Pending,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationStatus {
    Uninitialized,
    InProgress,
    Completed,
    Erroneous { cause: Reference },
    Aborted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationOutcome {
    Completed,
    Uncaught(Reference),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Phase {
    Dependencies,
    Body,
    Completing,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordStatus {
    InProgress { owner: u64, phase: Phase },
    Completed,
    Erroneous { cause: Reference },
    Aborted,
}
pub(crate) struct Record {
    pub class: ClassId,
    pub status: RecordStatus,
    pub plan: InitializationPlan,
}
/// Read-only native lifecycle history for this permanent initialization session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StringEqualityUsage {
    pub natives_registered: bool,
    pub assertion_queries: u64,
}
pub(crate) struct SessionState {
    pub records: Vec<Record>,
    pub indices: HashMap<ClassId, usize>,
    pub limits: InitializationLimits,
    pub admitted_count: usize,
    pub dependencies: usize,
    pub constants: usize,
    pub evaluation_owner: u64,
    pub steps: u64,
    pub construction: ConstructionState,
    pub equality: StringEqualityUsage,
    pub vm_exceptions: crate::VmExceptionUsage,
}
pub(crate) fn allocation(resource: &'static str) -> ExecutionError {
    ExecutionError::new(
        None,
        E::InitializationAccess(Box::new(InitializationAccessError::AllocationFailure {
            resource,
        })),
    )
}
pub(crate) fn overflow(resource: &'static str) -> ExecutionError {
    ExecutionError::new(None, E::InitializationAccountingOverflow { resource })
}
impl SessionState {
    pub fn charge(&mut self) -> Result<(), ExecutionError> {
        let required = self
            .steps
            .checked_add(1)
            .ok_or_else(|| overflow("initialization steps"))?;
        if required > self.limits.max_steps {
            return Err(ExecutionError::new(
                None,
                E::InitializationStepLimit {
                    limit: self.limits.max_steps,
                    required,
                },
            ));
        }
        self.steps = required;
        Ok(())
    }
    pub fn record(&self, class: ClassId) -> Result<Option<usize>, ExecutionError> {
        let Some(&index) = self.indices.get(&class) else {
            return Ok(None);
        };
        if self
            .records
            .get(index)
            .is_none_or(|record| record.class != class)
        {
            return Err(ExecutionError::new(
                None,
                E::InvalidMethod("invalid initialization record index"),
            ));
        }
        Ok(Some(index))
    }
    pub fn abort(&mut self) {
        for record in &mut self.records {
            if matches!(record.status, RecordStatus::InProgress { .. }) {
                record.status = RecordStatus::Aborted;
            }
        }
    }
    fn begin_evaluation(&mut self) -> Result<u64, ExecutionError> {
        self.steps = 0;
        let owner = self
            .evaluation_owner
            .checked_add(1)
            .ok_or_else(|| overflow("evaluation owner"))?;
        self.evaluation_owner = owner;
        Ok(owner)
    }
}
struct EvaluationGuard<'a> {
    state: &'a mut SessionState,
}
impl Drop for EvaluationGuard<'_> {
    fn drop(&mut self) {
        self.state.abort();
    }
}

/// One permanent initialization domain over the exclusively borrowed context.
/// Completed denotes only this profile's supported single-session procedure.
pub struct InitializationSession<'context, 'registry> {
    context: &'context mut FieldResolutionContext<'registry>,
    statics: StaticStateProfile,
    exceptions: ExceptionProfile,
    claim: InitializationClaim,
    state: SessionState,
    monitors: Option<crate::MonitorLimits>,
    class_mirrors: bool,
    strings: bool,
    throwables: Option<crate::VmExceptionLimits>,
}
impl<'context, 'registry> InitializationSession<'context, 'registry> {
    pub fn new(
        context: &'context mut FieldResolutionContext<'registry>,
        statics: &StaticStateProfile,
        exceptions: &ExceptionProfile,
        error_class: ClassId,
        limits: InitializationLimits,
    ) -> Result<Self, ExecutionError> {
        context
            .check_static_profile(statics)
            .map_err(|error| ExecutionError::new(None, E::StaticAccess(Box::new(error))))?;
        context
            .check_exception_profile(exceptions)
            .map_err(|error| ExecutionError::new(None, E::ExceptionAccess(Box::new(error))))?;
        let count = statics.class_count();
        if count > limits.max_classes {
            return Err(ExecutionError::new(
                None,
                E::InitializationLimit {
                    resource: InitializationResource::Classes,
                    limit: limits.max_classes,
                    required: count,
                },
            ));
        }
        let mut records = Vec::new();
        records
            .try_reserve_exact(count)
            .map_err(|_| allocation("initialization records"))?;
        let mut indices = HashMap::new();
        indices
            .try_reserve(count)
            .map_err(|_| allocation("initialization record map"))?;
        let claim = context
            .claim_initialization(statics, exceptions, error_class)
            .map_err(|error| ExecutionError::new(None, E::InitializationAccess(Box::new(error))))?;
        Ok(Self {
            context,
            statics: *statics,
            exceptions: *exceptions,
            claim,
            monitors: None,
            class_mirrors: false,
            strings: false,
            throwables: None,
            state: SessionState {
                records,
                indices,
                limits,
                admitted_count: count,
                dependencies: 0,
                constants: 0,
                evaluation_owner: 0,
                steps: 0,
                construction: ConstructionState::default(),
                equality: StringEqualityUsage::default(),
                vm_exceptions: crate::VmExceptionUsage::default(),
            },
        })
    }
    /// Enable bounded monitors for one logical thread. Each evaluation owns a fresh
    /// ledger; a failed evaluation cannot leak ownership into a later one.
    pub fn new_with_monitors(
        context: &'context mut FieldResolutionContext<'registry>,
        statics: &StaticStateProfile,
        exceptions: &ExceptionProfile,
        error_class: ClassId,
        limits: InitializationLimits,
        monitors: crate::MonitorLimits,
    ) -> Result<Self, ExecutionError> {
        let mut session = Self::new(context, statics, exceptions, error_class, limits)?;
        session.monitors = Some(monitors);
        Ok(session)
    }
    /// Enable canonical passive class literals and the exact Object.getClass native.
    /// Authority is checked before the one-shot initialization claim is consumed.
    pub fn new_with_class_mirrors(
        context: &'context mut FieldResolutionContext<'registry>,
        statics: &StaticStateProfile,
        exceptions: &ExceptionProfile,
        error_class: ClassId,
        limits: InitializationLimits,
        monitors: crate::MonitorLimits,
    ) -> Result<Self, ExecutionError> {
        context
            .check_class_mirror_profile()
            .map_err(|error| ExecutionError::new(None, E::ClassMirror(error)))?;
        let mut session =
            Self::new_with_monitors(context, statics, exceptions, error_class, limits, monitors)?;
        session.class_mirrors = true;
        Ok(session)
    }
    /// Enable immutable String literals, ConstantValue installation and exact intern.
    /// Mirror and String authority checks precede the permanent initialization claim.
    pub fn new_with_strings(
        context: &'context mut FieldResolutionContext<'registry>,
        statics: &StaticStateProfile,
        exceptions: &ExceptionProfile,
        error_class: ClassId,
        limits: InitializationLimits,
        monitors: crate::MonitorLimits,
    ) -> Result<Self, ExecutionError> {
        context
            .check_class_mirror_profile()
            .map_err(|error| ExecutionError::new(None, E::ClassMirror(error)))?;
        context
            .check_string_profile()
            .map_err(|error| ExecutionError::new(None, E::String(error)))?;
        let mut session =
            Self::new_with_monitors(context, statics, exceptions, error_class, limits, monitors)?;
        session.class_mirrors = true;
        session.strings = true;
        Ok(session)
    }
    /// Opt into genuine Throwable capture and bounded VM arithmetic exceptions.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_throwables(
        context: &'context mut FieldResolutionContext<'registry>,
        statics: &StaticStateProfile,
        exceptions: &ExceptionProfile,
        error_class: ClassId,
        limits: InitializationLimits,
        monitors: crate::MonitorLimits,
        vm_exceptions: crate::VmExceptionLimits,
    ) -> Result<Self, ExecutionError> {
        context
            .check_class_mirror_profile()
            .map_err(|e| ExecutionError::new(None, E::ClassMirror(e)))?;
        context
            .check_string_profile()
            .map_err(|e| ExecutionError::new(None, E::String(e)))?;
        context
            .check_throwable_profile()
            .map_err(|e| ExecutionError::new(None, E::ThrowableAccess(e)))?;
        let mut session =
            Self::new_with_monitors(context, statics, exceptions, error_class, limits, monitors)?;
        session.class_mirrors = true;
        session.strings = true;
        session.throwables = Some(vm_exceptions);
        Ok(session)
    }
    pub fn vm_exception_usage(&self) -> crate::VmExceptionUsage {
        self.state.vm_exceptions
    }
    pub fn throwable_trace(
        &mut self,
        reference: Reference,
    ) -> Result<Option<&[crabspot_runtime::TraceFrame]>, ExecutionError> {
        self.context
            .throwable_trace(reference)
            .map_err(|e| ExecutionError::new(None, E::ThrowableAccess(e)))
    }
    pub fn initialize(
        &mut self,
        class: ClassId,
        options: ExecutionOptions,
        call_limits: CallLimits,
        exception_limits: ExceptionLimits,
    ) -> Result<InitializationOutcome, ExecutionError> {
        self.initialize_with_construction_limits(
            class,
            options,
            call_limits,
            exception_limits,
            ConstructionLimits::default(),
        )
    }
    pub fn initialize_with_construction_limits(
        &mut self,
        class: ClassId,
        options: ExecutionOptions,
        call_limits: CallLimits,
        exception_limits: ExceptionLimits,
        construction_limits: ConstructionLimits,
    ) -> Result<InitializationOutcome, ExecutionError> {
        self.state.construction.reset(construction_limits);
        let guard = EvaluationGuard {
            state: &mut self.state,
        };
        let owner = guard.state.begin_evaluation()?;
        let execution = crate::execute::SessionExecution {
            context: self.context,
            claim: &self.claim,
            statics: &self.statics,
            exceptions: &self.exceptions,
            state: &mut *guard.state,
            owner,
            monitors: self.monitors,
            class_mirrors: self.class_mirrors,
            strings: self.strings,
            throwables: self.throwables,
        };
        match crate::execute::execute_initializing(
            execution,
            crate::execute::InitializationRoot::Class(class),
            options,
            call_limits,
            exception_limits,
        )? {
            crate::execute::InitializationResult::Completed => Ok(InitializationOutcome::Completed),
            crate::execute::InitializationResult::Uncaught(reference) => {
                Ok(InitializationOutcome::Uncaught(reference))
            }
            crate::execute::InitializationResult::Returned(_) => Err(ExecutionError::new(
                None,
                E::InvalidMethod("body result escaped initialization root"),
            )),
        }
    }
    pub fn run_static(
        &mut self,
        method: MethodId,
        arguments: &[Value],
        options: ExecutionOptions,
        call_limits: CallLimits,
        exception_limits: ExceptionLimits,
    ) -> Result<ExceptionOutcome, ExecutionError> {
        self.run_static_with_construction_limits(
            method,
            arguments,
            options,
            call_limits,
            exception_limits,
            ConstructionLimits::default(),
        )
    }
    pub fn run_static_with_construction_limits(
        &mut self,
        method: MethodId,
        arguments: &[Value],
        options: ExecutionOptions,
        call_limits: CallLimits,
        exception_limits: ExceptionLimits,
        construction_limits: ConstructionLimits,
    ) -> Result<ExceptionOutcome, ExecutionError> {
        self.state.construction.reset(construction_limits);
        let guard = EvaluationGuard {
            state: &mut self.state,
        };
        let owner = guard.state.begin_evaluation()?;
        let execution = crate::execute::SessionExecution {
            context: self.context,
            claim: &self.claim,
            statics: &self.statics,
            exceptions: &self.exceptions,
            state: &mut *guard.state,
            owner,
            monitors: self.monitors,
            class_mirrors: self.class_mirrors,
            strings: self.strings,
            throwables: self.throwables,
        };
        match crate::execute::execute_initializing(
            execution,
            crate::execute::InitializationRoot::Static { method, arguments },
            options,
            call_limits,
            exception_limits,
        )? {
            crate::execute::InitializationResult::Returned(value) => {
                Ok(ExceptionOutcome::Returned(value))
            }
            crate::execute::InitializationResult::Uncaught(reference) => {
                Ok(ExceptionOutcome::Uncaught(reference))
            }
            crate::execute::InitializationResult::Completed => Err(ExecutionError::new(
                None,
                E::InvalidMethod("initialization-only completion escaped static root"),
            )),
        }
    }
    /// Invoke the checked actual main after initializing its named entry class,
    /// in one evaluation with the normal shared root preflight and all budgets.
    pub fn run_application_main(
        &mut self,
        entry: &crabspot_runtime::ResolvedApplicationMain,
        argument_array: Reference,
        options: ExecutionOptions,
        call_limits: CallLimits,
        exception_limits: ExceptionLimits,
        construction_limits: ConstructionLimits,
    ) -> Result<ExceptionOutcome, ExecutionError> {
        self.state.construction.reset(construction_limits);
        self.context
            .check_application_main(entry)
            .map_err(|error| ExecutionError::new(None, E::ApplicationMain(error)))?;
        let guard = EvaluationGuard {
            state: &mut self.state,
        };
        let owner = guard.state.begin_evaluation()?;
        let arguments = [Value::Reference(argument_array)];
        let execution = crate::execute::SessionExecution {
            context: self.context,
            claim: &self.claim,
            statics: &self.statics,
            exceptions: &self.exceptions,
            state: &mut *guard.state,
            owner,
            monitors: self.monitors,
            class_mirrors: self.class_mirrors,
            strings: self.strings,
            throwables: self.throwables,
        };
        match crate::execute::execute_initializing(
            execution,
            crate::execute::InitializationRoot::Application {
                named: entry.named_class(),
                method: entry.method(),
                arguments: &arguments,
            },
            options,
            call_limits,
            exception_limits,
        )? {
            crate::execute::InitializationResult::Returned(value) => {
                Ok(ExceptionOutcome::Returned(value))
            }
            crate::execute::InitializationResult::Uncaught(reference) => {
                Ok(ExceptionOutcome::Uncaught(reference))
            }
            crate::execute::InitializationResult::Completed => Err(ExecutionError::new(
                None,
                E::InvalidMethod("initializer completion escaped application main root"),
            )),
        }
    }
    pub fn status(&self, class: ClassId) -> Result<InitializationStatus, ExecutionError> {
        self.context
            .check_initialization_class(&self.claim, class)
            .map_err(|error| ExecutionError::new(None, E::InitializationAccess(Box::new(error))))?;
        let status = self
            .state
            .record(class)?
            .map(|index| {
                self.state
                    .records
                    .get(index)
                    .map(|record| record.status)
                    .ok_or_else(|| {
                        ExecutionError::new(
                            None,
                            E::InvalidMethod("missing initialization status record"),
                        )
                    })
            })
            .transpose()?;
        Ok(match status {
            None => InitializationStatus::Uninitialized,
            Some(RecordStatus::InProgress { .. }) => InitializationStatus::InProgress,
            Some(RecordStatus::Completed) => InitializationStatus::Completed,
            Some(RecordStatus::Erroneous { cause }) => InitializationStatus::Erroneous { cause },
            Some(RecordStatus::Aborted) => InitializationStatus::Aborted,
        })
    }
    pub fn context(&self) -> &FieldResolutionContext<'registry> {
        self.context
    }
    pub fn retained_roots(&self) -> impl Iterator<Item = HeapRef> + '_ {
        self.state.records.iter().filter_map(|record| {
            if let RecordStatus::Erroneous { cause } = record.status {
                cause.heap_ref()
            } else {
                None
            }
        })
    }
    pub fn string_equality_usage(&self) -> StringEqualityUsage {
        self.state.equality
    }
    pub fn last_construction_steps(&self) -> u64 {
        self.state.construction.steps
    }
    pub fn last_initialization_steps(&self) -> u64 {
        self.state.steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty() -> SessionState {
        SessionState {
            records: Vec::new(),
            indices: HashMap::new(),
            limits: InitializationLimits::default(),
            admitted_count: 0,
            dependencies: 0,
            constants: 0,
            evaluation_owner: 0,
            steps: 0,
            construction: ConstructionState::default(),
            equality: StringEqualityUsage::default(),
            vm_exceptions: crate::VmExceptionUsage::default(),
        }
    }
    #[test]
    fn scheduler_failed_charges_leave_counter_and_distinguish_overflow() {
        let mut state = empty();
        state.limits.max_steps = 0;
        assert_eq!(
            state.charge().unwrap_err().kind,
            E::InitializationStepLimit {
                limit: 0,
                required: 1
            }
        );
        assert_eq!(state.steps, 0);
        state.steps = u64::MAX;
        state.limits.max_steps = u64::MAX;
        assert_eq!(
            state.charge().unwrap_err().kind,
            E::InitializationAccountingOverflow {
                resource: "initialization steps"
            }
        );
        assert_eq!(state.steps, u64::MAX);
        state.steps = u64::MAX - 1;
        state.charge().unwrap();
        assert_eq!(state.steps, u64::MAX);
    }
    #[test]
    fn evaluation_owner_exhaustion_resets_steps_without_reusing_identity() {
        let mut state = empty();
        state.evaluation_owner = u64::MAX - 1;
        state.steps = 3;
        assert_eq!(state.begin_evaluation().unwrap(), u64::MAX);
        assert_eq!(state.steps, 0);
        state.steps = 5;
        assert_eq!(
            state.begin_evaluation().unwrap_err().kind,
            E::InitializationAccountingOverflow {
                resource: "evaluation owner"
            }
        );
        assert_eq!(state.evaluation_owner, u64::MAX);
        assert_eq!(state.steps, 0);
    }
}

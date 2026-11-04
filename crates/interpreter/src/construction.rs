//! Evaluation-local construction accounting; no global object completion state.
use crate::{ExecutionError, ExecutionErrorKind as E};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructionLimits {
    pub max_alias_steps: u64,
}
impl Default for ConstructionLimits {
    fn default() -> Self {
        Self {
            max_alias_steps: 1_048_576,
        }
    }
}
pub(crate) struct ConstructionState {
    pub steps: u64,
    limit: u64,
    activation: u64,
}
impl Default for ConstructionState {
    fn default() -> Self {
        Self {
            steps: 0,
            limit: ConstructionLimits::default().max_alias_steps,
            activation: 0,
        }
    }
}
impl ConstructionState {
    pub fn reset(&mut self, limits: ConstructionLimits) {
        self.steps = 0;
        self.limit = limits.max_alias_steps;
        self.activation = 0;
    }
    pub fn charge(&mut self, locals: usize, values: usize, pc: u32) -> Result<(), ExecutionError> {
        let overflow = || {
            ExecutionError::new(
                Some(pc),
                E::ConstructionAccountingOverflow {
                    resource: "alias scans",
                },
            )
        };
        let amount = u64::try_from(locals)
            .ok()
            .and_then(|n| n.checked_add(1))
            .and_then(|n| u64::try_from(values).ok().and_then(|v| n.checked_add(v)))
            .ok_or_else(overflow)?;
        let required = self.steps.checked_add(amount).ok_or_else(overflow)?;
        if required > self.limit {
            return Err(ExecutionError::new(
                Some(pc),
                E::ConstructionStepLimit {
                    limit: self.limit,
                    required,
                },
            ));
        }
        self.steps = required;
        Ok(())
    }
    pub fn next_activation(&mut self, pc: u32) -> Result<u64, ExecutionError> {
        let value = self.activation.checked_add(1).ok_or_else(|| {
            ExecutionError::new(
                Some(pc),
                E::ConstructionAccountingOverflow {
                    resource: "constructor activation",
                },
            )
        })?;
        self.activation = value;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_funding_is_atomic_on_limit_and_overflow_and_uses_actual_counts() {
        let mut state = ConstructionState::default();
        state.reset(ConstructionLimits { max_alias_steps: 4 });
        state.charge(2, 1, 7).unwrap();
        assert_eq!(state.steps, 4);
        assert_eq!(
            state.charge(0, 0, 8).unwrap_err().kind,
            E::ConstructionStepLimit {
                limit: 4,
                required: 5
            }
        );
        assert_eq!(state.steps, 4);
        state.steps = u64::MAX;
        state.limit = u64::MAX;
        assert!(matches!(
            state.charge(0, 0, 8).unwrap_err().kind,
            E::ConstructionAccountingOverflow { .. }
        ));
        assert_eq!(state.steps, u64::MAX);
        state.reset(ConstructionLimits {
            max_alias_steps: u64::MAX,
        });
        assert_eq!(state.steps, 0);
        state.charge(1, 1, 9).unwrap();
        assert_eq!(state.steps, 3);
    }
    #[test]
    fn constructor_activation_exhaustion_never_reuses_a_live_key() {
        let mut state = ConstructionState {
            activation: u64::MAX - 1,
            ..ConstructionState::default()
        };
        assert_eq!(state.next_activation(0).unwrap(), u64::MAX);
        assert!(matches!(
            state.next_activation(1).unwrap_err().kind,
            E::ConstructionAccountingOverflow {
                resource: "constructor activation"
            }
        ));
        assert_eq!(state.activation, u64::MAX);
        state.reset(ConstructionLimits::default());
        assert_eq!(state.next_activation(2).unwrap(), 1);
    }
}

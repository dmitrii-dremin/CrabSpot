//! Evaluation-local single-owner monitors. Keys are validated by the shared
//! engine before entry; the ledger never promotes references or resolves names.
//!
//! Every successful acquisition is represented by one positive explicit row or
//! one implicit receipt. All fallible provisioning precedes logical publication.
//! Flat maps hold scalar identities only: dropping the service is iterative and
//! requires neither allocations, heap access nor remaining policy work.
//!
//! A claim is a positive explicit (depth, key) row or one implicit invocation
//! receipt. Reentry adds recursion, not another explicit claim. Implicit receipts
//! cannot satisfy a callee's explicit exit; their paired enter/exit is attributed
//! to its caller. Keys and empty depth rows retire immediately. Empty explicit
//! maps discard their buffers even while an implicit receipt remains.
//!
//! Operations fund one step before semantic changes; identity/null validation is
//! the engine's preceding responsibility. Completion is free without claims and
//! otherwise costs one step, checks explicit balance, then releases a receipt.
//! This enforces both optional structured-locking rules for one logical thread.
//! It does not implement contention, wait/notify, exception materialization, Class
//! mirrors or Java memory-order guarantees. A future mirror issuer must unify its
//! object identity with the canonical declaring-class key before enabling both.
use crabspot_runtime::{ClassId, HeapRef, VmExceptionKind};
use std::{collections::HashMap, fmt};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Inclusive ownership ceilings, reset only for a fresh engine evaluation.
pub struct MonitorLimits {
    pub max_steps: u64,
    pub max_active_monitors: u64,
    pub max_active_claims: u64,
    pub max_recursion: u64,
}
impl Default for MonitorLimits {
    fn default() -> Self {
        Self {
            max_steps: 1_048_576,
            max_active_monitors: 16_384,
            max_active_claims: 65_536,
            max_recursion: 65_536,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorResource {
    Steps,
    ActiveMonitors,
    ActiveClaims,
    Recursion,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MonitorError {
    Limit {
        resource: MonitorResource,
        limit: u64,
        required: u64,
    },
    AccountingOverflow {
        resource: MonitorResource,
    },
    AllocationFailure,
    InvalidState(&'static str),
}
impl fmt::Display for MonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit {
                resource,
                limit,
                required,
            } => write!(f, "monitor {resource:?} limit {limit}, required {required}"),
            Self::AccountingOverflow { resource } => {
                write!(f, "monitor {resource:?} accounting overflow")
            }
            Self::AllocationFailure => f.write_str("cannot reserve monitor ownership records"),
            Self::InvalidState(why) => write!(f, "invalid monitor state: {why}"),
        }
    }
}
impl std::error::Error for MonitorError {}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum MonitorKey {
    Object(HeapRef),
    Class(ClassId),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MonitorFailure {
    Java(VmExceptionKind),
    Host(MonitorError),
}
impl From<MonitorError> for MonitorFailure {
    fn from(e: MonitorError) -> Self {
        Self::Host(e)
    }
}
type Result<T> = std::result::Result<T, MonitorFailure>;
use MonitorResource as R;
fn invalid(why: &'static str) -> MonitorFailure {
    MonitorError::InvalidState(why).into()
}
fn imse() -> MonitorFailure {
    MonitorFailure::Java(VmExceptionKind::IllegalMonitorStateException)
}
fn increment(old: u64, added: u64, limit: u64, resource: R) -> Result<u64> {
    let required = old
        .checked_add(added)
        .ok_or(MonitorError::AccountingOverflow { resource })?;
    if required > limit {
        Err(MonitorError::Limit {
            resource,
            limit,
            required,
        }
        .into())
    } else {
        Ok(required)
    }
}
#[derive(Default)]
struct FrameMonitors {
    explicit: HashMap<MonitorKey, u64>,
    implicit: Option<MonitorKey>,
}
impl FrameMonitors {
    fn claims(&self) -> u64 {
        self.explicit.len() as u64 + u64::from(self.implicit.is_some())
    }
}
struct Acquisition {
    depth: usize,
    key: MonitorKey,
    implicit: bool,
    before_keys: usize,
    before_frames: usize,
    before_claims: u64,
    before_global: Option<u64>,
    before_local: Option<u64>,
    before_receipt: Option<MonitorKey>,
    new_claims: u64,
    new_global: u64,
    new_local: u64,
}
pub(crate) struct MonitorLedger {
    limits: MonitorLimits,
    steps: u64,
    claims: u64,
    keys: HashMap<MonitorKey, u64>,
    frames: HashMap<usize, FrameMonitors>,
    #[cfg(test)]
    fail_reserve_after: Option<usize>,
}
impl MonitorLedger {
    pub(crate) fn new(limits: MonitorLimits) -> Self {
        Self {
            limits,
            steps: 0,
            claims: 0,
            keys: HashMap::new(),
            frames: HashMap::new(),
            #[cfg(test)]
            fail_reserve_after: None,
        }
    }
    fn step(&mut self) -> Result<()> {
        self.steps = increment(self.steps, 1, self.limits.max_steps, R::Steps)?;
        Ok(())
    }
    fn reserving(&mut self) -> Result<()> {
        #[cfg(test)]
        if let Some(left) = &mut self.fail_reserve_after {
            if *left == 0 {
                return Err(MonitorError::AllocationFailure.into());
            }
            *left -= 1
        }
        Ok(())
    }
    fn coherent(&self) -> Result<()> {
        if (self.claims == 0) != (self.keys.is_empty())
            || (self.claims == 0) != (self.frames.is_empty())
            || self.keys.len() as u64 > self.claims
            || self.frames.len() as u64 > self.claims
        {
            return Err(invalid("global claim inventory differs"));
        }
        Ok(())
    }
    fn frame_shape_coherent(&self, frame: &FrameMonitors) -> Result<()> {
        if frame.claims() == 0 || frame.claims() > self.claims {
            return Err(invalid("empty or excessive invocation claims"));
        }
        if let Some(receipt) = frame.implicit {
            if !self.keys.get(&receipt).is_some_and(|n| *n > 0) {
                return Err(invalid("implicit receipt lacks ownership"));
            }
        }
        Ok(())
    }
    fn frame_coherent(&self, frame: &FrameMonitors, key: MonitorKey) -> Result<()> {
        self.frame_shape_coherent(frame)?;
        let local = frame.explicit.get(&key).copied();
        if local == Some(0) {
            return Err(invalid("zero explicit row"));
        }
        let own = local
            .unwrap_or(0)
            .checked_add(u64::from(frame.implicit == Some(key)))
            .ok_or_else(|| invalid("invocation count overflow"))?;
        let total = self.keys.get(&key).copied().unwrap_or(0);
        if total < own || (self.claims == frame.claims() && total != own) {
            return Err(invalid("invocation and thread counts differ"));
        }
        Ok(())
    }
    /// Free scalar activation guard; every frame publication must start vacant.
    pub(crate) fn check_new_frame(&self, depth: usize) -> Result<()> {
        self.coherent()?;
        if self.frames.contains_key(&depth) {
            Err(invalid("frame depth still has monitor authority"))
        } else {
            Ok(())
        }
    }
    fn stage(&self, depth: usize, key: MonitorKey, implicit: bool) -> Result<Acquisition> {
        let global = self.keys.get(&key).copied();
        let frame = self.frames.get(&depth);
        let local = frame.and_then(|f| f.explicit.get(&key).copied());
        let receipt = frame.and_then(|f| f.implicit);
        increment(
            self.keys.len() as u64,
            u64::from(global.is_none()),
            self.limits.max_active_monitors,
            R::ActiveMonitors,
        )?;
        let claims = increment(
            self.claims,
            u64::from(implicit || local.is_none()),
            self.limits.max_active_claims,
            R::ActiveClaims,
        )?;
        let total = increment(
            global.unwrap_or(0),
            1,
            self.limits.max_recursion,
            R::Recursion,
        )?;
        self.coherent()?;
        if global == Some(0) {
            return Err(invalid("zero thread ownership"));
        }
        if let Some(frame) = frame {
            self.frame_coherent(frame, key)?;
            if implicit {
                return Err(invalid("implicit activation reused a live frame"));
            }
        }
        let next = local
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("explicit count overflow"))?;
        Ok(Acquisition {
            depth,
            key,
            implicit,
            before_keys: self.keys.len(),
            before_frames: self.frames.len(),
            before_claims: self.claims,
            before_global: global,
            before_local: local,
            before_receipt: receipt,
            new_claims: claims,
            new_global: total,
            new_local: next,
        })
    }
    fn commit(&mut self, stage: Acquisition, fresh: Option<FrameMonitors>) -> Result<()> {
        let frame = self.frames.get(&stage.depth);
        let local = frame.and_then(|f| f.explicit.get(&stage.key).copied());
        let receipt = frame.and_then(|f| f.implicit);
        if self.claims != stage.before_claims
            || self.keys.len() != stage.before_keys
            || self.frames.len() != stage.before_frames
            || self.keys.get(&stage.key).copied() != stage.before_global
            || local != stage.before_local
            || receipt != stage.before_receipt
            || stage.new_global
                != stage
                    .before_global
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| invalid("damaged recursion stage"))?
            || stage.new_local
                != stage
                    .before_local
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| invalid("damaged local stage"))?
            || stage.new_claims
                != stage
                    .before_claims
                    .checked_add(u64::from(stage.implicit || stage.before_local.is_none()))
                    .ok_or_else(|| invalid("damaged claim stage"))?
            || stage.new_global > self.limits.max_recursion
            || stage.new_claims > self.limits.max_active_claims
            || (stage.before_global.is_none() && self.keys.capacity() <= self.keys.len())
        {
            return Err(invalid("acquisition stage changed before publication"));
        }
        self.coherent()?;
        if let Some(frame) = frame {
            self.frame_coherent(frame, stage.key)?;
            if stage.implicit || fresh.is_some() {
                return Err(invalid("duplicate invocation stage"));
            }
            if stage.before_local.is_none() && frame.explicit.capacity() <= frame.explicit.len() {
                return Err(invalid("explicit reservation absent"));
            }
        } else if self.frames.capacity() <= self.frames.len()
            || fresh
                .as_ref()
                .is_none_or(|f| f.claims() != 0 || (!stage.implicit && f.explicit.capacity() == 0))
        {
            return Err(invalid("invocation reservation absent"));
        }
        let prospective = self
            .keys
            .len()
            .checked_add(usize::from(stage.before_global.is_none()))
            .ok_or_else(|| invalid("damaged active monitor stage"))?;
        if prospective as u64 > self.limits.max_active_monitors {
            return Err(invalid("active monitor reservation exceeds policy"));
        }
        // Obtain every fallible checked reference before changing live ownership.
        let global = if stage.before_global.is_some() {
            Some(
                self.keys
                    .get_mut(&stage.key)
                    .ok_or_else(|| invalid("existing global counter absent"))?,
            )
        } else {
            None
        };
        if let Some(mut fresh) = fresh {
            if stage.implicit {
                fresh.implicit = Some(stage.key)
            } else {
                fresh.explicit.insert(stage.key, stage.new_local);
            }
            self.frames.insert(stage.depth, fresh); // fresh, reserved outer row
        } else {
            let frame = self
                .frames
                .get_mut(&stage.depth)
                .ok_or_else(|| invalid("existing invocation absent"))?;
            if stage.before_local.is_some() {
                let local = frame
                    .explicit
                    .get_mut(&stage.key)
                    .ok_or_else(|| invalid("existing local counter absent"))?;
                *local = stage.new_local;
            } else {
                frame.explicit.insert(stage.key, stage.new_local);
            } // fresh, reserved local row
        }
        if let Some(global) = global {
            *global = stage.new_global;
        } else {
            self.keys.insert(stage.key, stage.new_global);
        } // fresh, reserved identity only
        self.claims = stage.new_claims;
        Ok(())
    }

    fn acquire(&mut self, depth: usize, key: MonitorKey, implicit: bool) -> Result<()> {
        self.step()?;
        let stage = self.stage(depth, key, implicit)?;
        if stage.before_global.is_none() {
            self.reserving()?;
            self.keys
                .try_reserve(1)
                .map_err(|_| MonitorError::AllocationFailure)?;
        }
        let mut fresh = None;
        if !self.frames.contains_key(&depth) {
            self.reserving()?;
            self.frames
                .try_reserve(1)
                .map_err(|_| MonitorError::AllocationFailure)?;
            let mut row = FrameMonitors::default();
            if !implicit {
                self.reserving()?;
                row.explicit
                    .try_reserve(1)
                    .map_err(|_| MonitorError::AllocationFailure)?;
            }
            fresh = Some(row)
        } else if stage.before_local.is_none() && !implicit {
            self.reserving()?;
            self.frames
                .get_mut(&depth)
                .ok_or_else(|| invalid("frame disappeared during reservation"))?
                .explicit
                .try_reserve(1)
                .map_err(|_| MonitorError::AllocationFailure)?;
        }
        self.commit(stage, fresh)
    }
    pub(crate) fn enter_explicit(&mut self, depth: usize, key: MonitorKey) -> Result<()> {
        self.acquire(depth, key, false)
    }
    pub(crate) fn enter_implicit(&mut self, depth: usize, key: MonitorKey) -> Result<()> {
        self.acquire(depth, key, true)
    }
    pub(crate) fn exit_explicit(&mut self, depth: usize, key: MonitorKey) -> Result<()> {
        self.step()?;
        self.coherent()?;
        if self.keys.get(&key) == Some(&0) {
            return Err(invalid("zero thread ownership"));
        }
        let frame = self.frames.get(&depth);
        if let Some(frame) = frame {
            self.frame_coherent(frame, key)?;
        }
        let local = frame
            .and_then(|f| f.explicit.get(&key).copied())
            .ok_or_else(imse)?;
        let total = self
            .keys
            .get(&key)
            .copied()
            .ok_or_else(|| invalid("explicit row lacks thread ownership"))?;
        let local = local
            .checked_sub(1)
            .ok_or_else(|| invalid("explicit decrement underflow"))?;
        let total = total
            .checked_sub(1)
            .ok_or_else(|| invalid("thread decrement underflow"))?;
        let claims = self
            .claims
            .checked_sub(u64::from(local == 0))
            .ok_or_else(|| invalid("claim decrement underflow"))?;
        let retire_frame =
            local == 0 && frame.is_some_and(|f| f.explicit.len() == 1 && f.implicit.is_none());
        if (claims == 0) != (self.keys.len() == usize::from(total == 0))
            || (claims == 0) != (self.frames.len() == usize::from(retire_frame))
        {
            return Err(invalid("release would leave orphan ownership"));
        }
        let global = self
            .keys
            .get_mut(&key)
            .ok_or_else(|| invalid("release global counter absent"))?;
        let frame = self
            .frames
            .get_mut(&depth)
            .ok_or_else(|| invalid("explicit frame absent"))?;
        let counter = if local != 0 {
            Some(
                frame
                    .explicit
                    .get_mut(&key)
                    .ok_or_else(|| invalid("release local counter absent"))?,
            )
        } else {
            None
        };
        // Every remaining operation is a checked-position update or removal.
        if let Some(counter) = counter {
            *counter = local;
        } else {
            frame.explicit.remove(&key);
        }
        if frame.explicit.is_empty() {
            frame.explicit = HashMap::new();
        }
        if total != 0 {
            *global = total;
        }
        if retire_frame {
            self.frames.remove(&depth);
        }
        if total == 0 {
            self.keys.remove(&key);
        }
        self.claims = claims;
        Ok(())
    }
    pub(crate) fn finish(&mut self, depth: usize) -> Result<()> {
        let Some(frame) = self.frames.get(&depth) else {
            return Ok(());
        };
        if frame.claims() == 0 {
            return Err(invalid("empty invocation record"));
        }
        self.step()?;
        self.coherent()?;
        let frame = self
            .frames
            .get(&depth)
            .ok_or_else(|| invalid("completion frame absent"))?;
        self.frame_shape_coherent(frame)?;
        if !frame.explicit.is_empty() {
            return Err(imse());
        }
        let key = frame
            .implicit
            .ok_or_else(|| invalid("completion receipt absent"))?;
        self.frame_coherent(frame, key)?;
        let total = self
            .keys
            .get(&key)
            .copied()
            .and_then(|v| v.checked_sub(1))
            .ok_or_else(|| invalid("receipt decrement underflow"))?;
        let claims = self
            .claims
            .checked_sub(1)
            .ok_or_else(|| invalid("implicit claim underflow"))?;
        if (claims == 0) != (self.keys.len() == usize::from(total == 0))
            || (claims == 0) != (self.frames.len() == 1)
        {
            return Err(invalid("implicit release would orphan ownership"));
        }
        let counter = self
            .keys
            .get_mut(&key)
            .ok_or_else(|| invalid("completion global counter absent"))?;
        if total != 0 {
            *counter = total;
        }
        self.frames.remove(&depth);
        if total == 0 {
            self.keys.remove(&key);
        }
        self.claims = claims;
        Ok(())
    }
}
#[cfg(test)]
#[path = "monitor_tests.rs"]
mod tests;

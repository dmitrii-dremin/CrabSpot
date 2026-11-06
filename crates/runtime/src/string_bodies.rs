//! Canonical compact String Code authority and lazily published read-only storage.
use crate::heap::{Entry, EntryPayload, LIVE_GENERATION, Payload, reserve_entries};
use crate::*;
use crabspot_classfile::Member;
use std::fmt;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringBodyLimits {
    pub max_work_units: u64,
}
impl Default for StringBodyLimits {
    fn default() -> Self {
        Self {
            max_work_units: 33_554_432,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StringBodyUsage {
    pub work_units: u64,
    pub backings: u64,
    pub backing_bytes: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringBodyDiscoveryLimits {
    pub max_work_units: u64,
}
impl Default for StringBodyDiscoveryLimits {
    fn default() -> Self {
        Self {
            max_work_units: 4_194_304,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StringBodyDiscoveryUsage {
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringBodyResource {
    DiscoveryWork,
    BackingWork,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StringBodyErrorKind {
    UnsupportedShape,
    InvalidBinding(&'static str),
    InvalidState(&'static str),
    PolicyConflict,
    InvalidReceiver,
    UnsupportedAssertionTarget {
        represented: ReferenceType,
    },
    BackingLength,
    AllocationFailure,
    Limit {
        resource: StringBodyResource,
        limit: u64,
        required: u64,
    },
    Overflow {
        resource: StringBodyResource,
    },
    Registry(Box<RegistryError>),
    Context(Box<FieldAccessError>),
    Instance(Box<InstanceError>),
    Heap(Box<HeapError>),
    String(Box<StringError>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringBodyError {
    pub kind: StringBodyErrorKind,
    pub discovery_usage: StringBodyDiscoveryUsage,
}
impl fmt::Display for StringBodyErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            Self::Context(e) => e.fmt(f),
            Self::Instance(e) => e.fmt(f),
            Self::Heap(e) => e.fmt(f),
            Self::String(e) => e.fmt(f),
            e => write!(f, "String body: {e:?}"),
        }
    }
}
impl fmt::Display for StringBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
impl std::error::Error for StringBodyErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e.as_ref()),
            Self::Context(e) => Some(e.as_ref()),
            Self::Instance(e) => Some(e.as_ref()),
            Self::Heap(e) => Some(e.as_ref()),
            Self::String(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
impl std::error::Error for StringBodyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.kind)
    }
}
pub(crate) type R<T> = Result<T, Box<StringBodyError>>;
pub(crate) fn error(kind: StringBodyErrorKind) -> Box<StringBodyError> {
    Box::new(StringBodyError {
        kind,
        discovery_usage: Default::default(),
    })
}
macro_rules! from_error {
    ($t:ty,$v:ident) => {
        impl From<$t> for Box<StringBodyError> {
            fn from(e: $t) -> Self {
                error(StringBodyErrorKind::$v(Box::new(e)))
            }
        }
    };
}
from_error!(RegistryError, Registry);
from_error!(FieldAccessError, Context);
from_error!(InstanceError, Instance);
from_error!(HeapError, Heap);
impl From<Box<StringError>> for Box<StringBodyError> {
    fn from(e: Box<StringError>) -> Self {
        error(StringBodyErrorKind::String(e))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MemberKey(u16, u16, u16);
impl MemberKey {
    pub(crate) fn of(m: &Member) -> Self {
        Self(m.name_index, m.descriptor_index, m.access_flags)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactStringBodyPlan {
    pub(crate) registry: u64,
    pub(crate) bootstrap: LoaderId,
    pub(crate) string: ClassId,
    pub(crate) roots: [ClassId; 3],
    pub(crate) methods: [MethodId; 4],
    pub(crate) method_keys: [MemberKey; 4],
    pub(crate) constructor: MethodId,
    pub(crate) constructor_key: MemberKey,
    pub(crate) fields: [FieldId; 5],
    pub(crate) field_keys: [MemberKey; 5],
    pub(crate) comparator_edge: usize,
}
impl CompactStringBodyPlan {
    pub fn private_roots(&self) -> [ClassId; 3] {
        self.roots
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringBodyDiscovery {
    pub plan: Option<CompactStringBodyPlan>,
    pub usage: StringBodyDiscoveryUsage,
}
#[path = "string_body_probe.rs"]
mod probe;
pub use probe::probe_compact_string_bodies;
#[path = "string_equality.rs"]
pub(crate) mod equality;
pub use equality::*;
#[derive(Debug)]
pub(crate) struct BodyState {
    plan: CompactStringBodyPlan,
    heap: u64,
    universe: ReferenceTypeUniverse,
    classes: usize,
    limits: StringBodyLimits,
    usage: StringBodyUsage,
    #[cfg(test)]
    fail_reserve_after: Option<usize>,
}
impl BodyState {
    fn charge(&mut self, add: u64) -> R<()> {
        let resource = StringBodyResource::BackingWork;
        let required = self
            .usage
            .work_units
            .checked_add(add)
            .ok_or_else(|| error(StringBodyErrorKind::Overflow { resource }))?;
        if required > self.limits.max_work_units {
            return Err(error(StringBodyErrorKind::Limit {
                resource,
                limit: self.limits.max_work_units,
                required,
            }));
        }
        self.usage.work_units = required;
        Ok(())
    }
    fn reserving(&mut self) -> R<()> {
        #[cfg(test)]
        if let Some(left) = &mut self.fail_reserve_after {
            if *left == 0 {
                return Err(error(StringBodyErrorKind::AllocationFailure));
            }
            *left -= 1;
        }
        Ok(())
    }
    pub(crate) fn permits(&self, method: MethodId) -> bool {
        self.plan.methods[..3].contains(&method)
    }
}
impl ManagedHeap {
    pub fn bind_compact_string_bodies(
        &mut self,
        registry: &DefinitionRegistry,
        plan: &CompactStringBodyPlan,
        limits: StringBodyLimits,
    ) -> R<()> {
        if registry.identity() != plan.registry {
            return Err(error(StringBodyErrorKind::InvalidBinding(
                "body plan belongs to another registry",
            )));
        }
        let u = self.reference_type_universe().ok_or_else(|| {
            error(StringBodyErrorKind::InvalidBinding(
                "body plan needs a universe",
            ))
        })?;
        let string = self
            .strings
            .as_ref()
            .ok_or_else(|| {
                error(StringBodyErrorKind::InvalidBinding(
                    "bind the String kernel first",
                ))
            })?
            .class;
        if u.bootstrap_loader != plan.bootstrap
            || string != plan.string
            || self.instance_graph_registry()? != Some(plan.registry)
        {
            return Err(error(StringBodyErrorKind::InvalidBinding(
                "body role namespace differs",
            )));
        }
        for class in [plan.string, plan.roots[0], plan.roots[1], plan.roots[2]] {
            registry.class(class)?;
            self.instance_graph_node(class)?;
            if registry.defining_loader(class)? != plan.bootstrap {
                return Err(error(StringBodyErrorKind::InvalidBinding(
                    "body role loader differs",
                )));
            }
        }
        if self.instance_graph_node(plan.string)?.superclass != Some(u.object)
            || self.instance_graph_node(plan.roots[0])?.superclass != Some(u.object)
            || self
                .instance_graph_node(plan.roots[0])?
                .interfaces
                .get(plan.comparator_edge)
                != Some(&plan.roots[1])
        {
            return Err(error(StringBodyErrorKind::InvalidBinding(
                "body role edges differ",
            )));
        }
        for (i, key) in plan.methods.iter().zip(plan.method_keys) {
            if i.owner() != plan.string || MemberKey::of(registry.method(*i)?) != key {
                return Err(error(StringBodyErrorKind::InvalidBinding(
                    "body method proof differs",
                )));
            }
        }
        for (i, key) in plan.fields.iter().zip(plan.field_keys) {
            if i.owner() != plan.string || MemberKey::of(registry.field(*i)?) != key {
                return Err(error(StringBodyErrorKind::InvalidBinding(
                    "body field proof differs",
                )));
            }
        }
        if plan.constructor.owner() != plan.roots[0]
            || MemberKey::of(registry.method(plan.constructor)?) != plan.constructor_key
        {
            return Err(error(StringBodyErrorKind::InvalidBinding(
                "constructor proof differs",
            )));
        }
        if let Some(old) = &self.string_bodies {
            if old.plan == *plan
                && old.heap == self.identity
                && old.classes == self.retained_layout_classes()
                && old.universe == u
                && old.limits == limits
            {
                return Ok(());
            }
            return Err(error(StringBodyErrorKind::PolicyConflict));
        }
        if !self.entries.is_empty()
            || self.initialization_claim.is_some()
            || self.string_usage() != StringUsage::default()
        {
            return Err(error(StringBodyErrorKind::InvalidBinding(
                "body binding requires pristine entries, String ledger and claim",
            )));
        }
        self.string_bodies = Some(BodyState {
            plan: plan.clone(),
            heap: self.identity,
            universe: u,
            classes: self.retained_layout_classes(),
            limits,
            usage: Default::default(),
            #[cfg(test)]
            fail_reserve_after: None,
        });
        Ok(())
    }
    pub(crate) fn permits_string_method(&self, method: MethodId) -> R<bool> {
        if self
            .string_bodies
            .as_ref()
            .is_some_and(|b| b.permits(method))
            || self
                .string_equality
                .as_ref()
                .is_some_and(|e| e.string_method(method))
        {
            self.body_affinity()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub(crate) fn body_affinity(&self) -> R<()> {
        let b = self.string_bodies.as_ref().ok_or_else(|| {
            error(StringBodyErrorKind::InvalidBinding(
                "String bodies are not bound",
            ))
        })?;
        if b.heap != self.identity
            || b.classes != self.retained_layout_classes()
            || Some(b.universe) != self.reference_type_universe()
            || self
                .strings
                .as_ref()
                .is_none_or(|s| s.class != b.plan.string)
            || self.instance_graph_registry()? != Some(b.plan.registry)
        {
            return Err(error(StringBodyErrorKind::InvalidState(
                "String body affinity differs",
            )));
        }
        Ok(())
    }
    pub fn string_body_usage(&self) -> StringBodyUsage {
        self.string_bodies
            .as_ref()
            .map_or(Default::default(), |b| b.usage)
    }
    pub fn string_backing(&self, receiver: Reference) -> R<Option<(HeapRef, u8)>> {
        let EntryPayload::String(p) = &self.entry(receiver)?.payload else {
            return Err(error(StringBodyErrorKind::InvalidReceiver));
        };
        if let Some((h, coder)) = p.backing {
            self.body_affinity()?;
            let owner = receiver
                .heap_ref()
                .ok_or(HeapError::NullReference)?
                .components()
                .1;
            let (_, i, _) = h.components();
            self.checked_handle(h)?;
            let e = self
                .entries
                .get(i)
                .ok_or(HeapError::MissingEntry { index: i })?;
            let expected = match coder {
                0 => Some(p.units.len()),
                1 => p.units.len().checked_mul(2),
                _ => None,
            };
            let length = match &e.payload {
                EntryPayload::Array(Payload::Byte(bytes)) => Some(bytes.len()),
                _ => None,
            };
            if expected.is_none()
                || length != expected
                || e.string_backing != Some(crate::heap::StringBackingOwner { owner, coder })
            {
                return Err(error(StringBodyErrorKind::InvalidState(
                    "String backing owner, coder or length differs",
                )));
            }
        }
        Ok(p.backing)
    }
    fn materialize_string_backing(&mut self, receiver: Reference) -> R<(HeapRef, u8)> {
        self.body_affinity()?;
        let h = receiver.heap_ref().ok_or(HeapError::NullReference)?;
        let index = self.checked_handle(h)?;
        if !matches!(self.entry(receiver)?.payload, EntryPayload::String(_)) {
            return Err(error(StringBodyErrorKind::InvalidReceiver));
        }
        self.string_bodies
            .as_mut()
            .ok_or_else(|| error(StringBodyErrorKind::InvalidState("body state missing")))?
            .charge(1)?;
        if let Some(pair) = self.string_backing(receiver)? {
            return Ok(pair);
        }
        let n = match &self.entry(receiver)?.payload {
            EntryPayload::String(p) => p.units.len(),
            _ => return Err(error(StringBodyErrorKind::InvalidReceiver)),
        };
        let n64 = u64::try_from(n).map_err(|_| error(StringBodyErrorKind::BackingLength))?;
        self.string_bodies
            .as_mut()
            .ok_or_else(|| error(StringBodyErrorKind::InvalidState("body state missing")))?
            .charge(n64)?;
        let units = match &self.entry(receiver)?.payload {
            EntryPayload::String(p) => p.units.as_slice(),
            _ => return Err(error(StringBodyErrorKind::InvalidReceiver)),
        };
        let mut latin = true;
        for &u in units {
            latin &= u <= 255;
        }
        let coder = u8::from(!latin);
        let bytes = if latin {
            n
        } else {
            n.checked_mul(2)
                .ok_or_else(|| error(StringBodyErrorKind::BackingLength))?
        };
        i32::try_from(bytes).map_err(|_| error(StringBodyErrorKind::BackingLength))?;
        let arrays = crate::heap::increased(
            self.arrays,
            1,
            self.options.max_arrays,
            HeapResource::Arrays,
        )?;
        crate::heap::increased(
            0,
            bytes,
            self.options.max_array_length,
            HeapResource::ArrayLength,
        )?;
        let elements = crate::heap::increased(
            self.elements,
            bytes,
            self.options.max_total_elements,
            HeapResource::TotalElements,
        )?;
        let payload_bytes = crate::heap::increased(
            self.bytes,
            bytes,
            self.options.max_payload_bytes,
            HeapResource::PayloadBytes,
        )?;
        let b = self
            .string_bodies
            .as_mut()
            .ok_or_else(|| error(StringBodyErrorKind::InvalidState("body state missing")))?;
        let count =
            b.usage.backings.checked_add(1).ok_or_else(|| {
                error(StringBodyErrorKind::InvalidState("backing count overflow"))
            })?;
        let total = b
            .usage
            .backing_bytes
            .checked_add(bytes as u64)
            .ok_or_else(|| {
                error(StringBodyErrorKind::InvalidState(
                    "backing byte count overflow",
                ))
            })?;
        b.charge(bytes as u64)?;
        b.reserving()?;
        let mut data = Vec::new();
        data.try_reserve_exact(bytes)
            .map_err(|_| error(StringBodyErrorKind::AllocationFailure))?;
        let units = match &self
            .entries
            .get(index)
            .ok_or(HeapError::MissingEntry { index })?
            .payload
        {
            EntryPayload::String(p) => &p.units,
            _ => return Err(error(StringBodyErrorKind::InvalidReceiver)),
        };
        for &u in units {
            data.push(u as u8 as i8);
            if !latin {
                data.push((u >> 8) as u8 as i8);
            }
        }
        b.reserving()?;
        reserve_entries(&mut self.entries, 1)?;
        let target = self.entries.len();
        target
            .checked_add(1)
            .ok_or_else(|| error(StringBodyErrorKind::InvalidState("arena index exhausted")))?;
        let backing = Reference::issued(self.identity, target, LIVE_GENERATION)
            .heap_ref()
            .ok_or_else(|| error(StringBodyErrorKind::InvalidState("backing is null")))?;
        if self.entries.capacity() <= target {
            return Err(error(StringBodyErrorKind::InvalidState(
                "backing arena capacity differs",
            )));
        }
        let p = match &mut self
            .entries
            .get_mut(index)
            .ok_or(HeapError::MissingEntry { index })?
            .payload
        {
            EntryPayload::String(p) => p,
            _ => {
                return Err(error(StringBodyErrorKind::InvalidState(
                    "backing owner changed kind",
                )));
            }
        };
        if p.backing.is_some() || data.len() != bytes {
            return Err(error(StringBodyErrorKind::InvalidState(
                "backing stage changed",
            )));
        }
        // All scalar guards, capacities and mutable targets are established.
        // The temporary future handle is never observable before this commit.
        p.backing = Some((backing, coder));
        self.entries.push(Entry {
            generation: LIVE_GENERATION,
            string_backing: Some(crate::heap::StringBackingOwner {
                owner: index,
                coder,
            }),
            payload: EntryPayload::Array(Payload::Byte(data)),
        });
        self.arrays = arrays;
        self.elements = elements;
        self.bytes = payload_bytes;
        b.usage.backings = count;
        b.usage.backing_bytes = total;
        Ok((backing, coder))
    }
}
impl FieldResolutionContext<'_> {
    pub(crate) fn read_string_body_field(
        &mut self,
        field: FieldId,
        receiver: Reference,
    ) -> R<Option<Value>> {
        let Some(b) = &self.heap.string_bodies else {
            return Ok(None);
        };
        if field != b.plan.fields[0] && field != b.plan.fields[1] {
            return Ok(None);
        }
        if !matches!(self.heap.entry(receiver)?.payload, EntryPayload::String(_)) {
            return Ok(None);
        }
        self.check_string_profile()?;
        let value = field == b.plan.fields[0];
        let (h, coder) = self.heap.materialize_string_backing(receiver)?;
        let (a, i, g) = h.components();
        Ok(Some(if value {
            Value::Reference(Reference::issued(a, i, g))
        } else {
            Value::Int(i32::from(coder))
        }))
    }
}

#[cfg(test)]
#[path = "string_body_tests.rs"]
mod tests;

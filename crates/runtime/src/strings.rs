//! Immutable kernel Strings. UTF-16 contents live once in the arena; scalar
//! fingerprints and canonical root indices carry the pool's private authority.
use crate::heap::{Entry, EntryPayload, LIVE_GENERATION, reserve_entries};
use crate::*;
use crabspot_classfile::{Constant, ConstantPool};
use std::{
    collections::{HashMap, hash_map::RandomState},
    fmt,
    hash::BuildHasher,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringLimits {
    pub max_objects: u64,
    pub max_retained_units: u64,
    pub max_interned: u64,
    pub max_literal_sites: u64,
    pub max_work_units: u64,
}
impl Default for StringLimits {
    fn default() -> Self {
        Self {
            max_objects: 65_536,
            max_retained_units: 4_194_304,
            max_interned: 65_536,
            max_literal_sites: 262_144,
            max_work_units: 16_777_216,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StringUsage {
    pub objects: u64,
    pub retained_units: u64,
    pub interned: u64,
    pub literal_sites: u64,
    pub work_units: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringResource {
    Objects,
    RetainedUnits,
    Interned,
    LiteralSites,
    WorkUnits,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StringError {
    Disabled,
    PolicyConflict,
    InvalidBinding(&'static str),
    InvalidState(&'static str),
    InvalidLiteral {
        origin: ClassId,
        index: u16,
    },
    InvalidReceiver,
    Limit {
        resource: StringResource,
        limit: u64,
        required: u64,
    },
    Overflow {
        resource: StringResource,
    },
    AllocationFailure,
    Registry(RegistryError),
    Context(Box<FieldAccessError>),
    Instance(InstanceError),
    Heap(Box<HeapError>),
}
impl fmt::Display for StringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(e) => e.fmt(f),
            Self::Context(e) => e.fmt(f),
            Self::Instance(e) => e.fmt(f),
            Self::Heap(e) => e.fmt(f),
            e => write!(f, "String service: {e:?}"),
        }
    }
}
impl std::error::Error for StringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(e) => Some(e),
            Self::Context(e) => Some(e.as_ref()),
            Self::Instance(e) => Some(e),
            Self::Heap(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
macro_rules! convert {
    ($t:ty,$v:ident) => {
        impl From<$t> for Box<StringError> {
            fn from(e: $t) -> Self {
                Box::new(StringError::$v(e.into()))
            }
        }
    };
}
convert!(RegistryError, Registry);
convert!(FieldAccessError, Context);
convert!(InstanceError, Instance);
convert!(HeapError, Heap);
impl From<TypeAccessError> for Box<StringError> {
    fn from(e: TypeAccessError) -> Self {
        FieldAccessError::ReferenceType(Box::new(e)).into()
    }
}
type R<T> = Result<T, Box<StringError>>;
fn err(e: StringError) -> Box<StringError> {
    Box::new(e)
}
fn invalid(s: &'static str) -> Box<StringError> {
    err(StringError::InvalidState(s))
}
fn increased(old: u64, add: u64, limit: u64, resource: StringResource) -> R<u64> {
    let required = old
        .checked_add(add)
        .ok_or_else(|| err(StringError::Overflow { resource }))?;
    if required > limit {
        return Err(err(StringError::Limit {
            resource,
            limit,
            required,
        }));
    }
    Ok(required)
}
fn units_count(n: usize) -> R<u64> {
    u64::try_from(n).map_err(|_| {
        err(StringError::Overflow {
            resource: StringResource::RetainedUnits,
        })
    })
}
#[derive(Debug)]
pub(crate) struct StringPayload {
    pub(crate) units: Vec<u16>,
    pub(crate) backing: Option<(HeapRef, u8)>,
    canonical: Option<usize>,
}
#[derive(Clone, Copy, Debug)]
struct LiteralSite {
    origin: ClassId,
    index: u16,
    utf8: u16,
    handle: HeapRef,
    root: usize,
}
#[derive(Debug)]
pub(crate) struct Strings {
    pub(crate) class: ClassId,
    registry: u64,
    universe: ReferenceTypeUniverse,
    classes: usize,
    limits: StringLimits,
    usage: StringUsage,
    hash: RandomState,
    buckets: HashMap<u64, Vec<HeapRef>>,
    roots: Vec<HeapRef>,
    sites: HashMap<(ClassId, u16), LiteralSite>,
    #[cfg(test)]
    fail_reserve_after: Option<usize>,
    #[cfg(test)]
    force_fingerprint: Option<u64>,
}
fn payload(entries: &[Entry], heap: u64, reference: Reference) -> R<&StringPayload> {
    let h = reference
        .heap_ref()
        .ok_or_else(|| err(StringError::InvalidReceiver))?;
    let (owner, index, generation) = h.components();
    if owner != heap {
        return Err(HeapError::ForeignHeap.into());
    }
    let e = entries
        .get(index)
        .ok_or(HeapError::MissingEntry { index })?;
    if e.generation != generation {
        return Err(HeapError::StaleGeneration {
            expected: e.generation,
            actual: generation,
        }
        .into());
    }
    match &e.payload {
        EntryPayload::String(p) => Ok(p),
        _ => Err(err(StringError::InvalidReceiver)),
    }
}
fn reference(h: HeapRef) -> Reference {
    let (a, b, c) = h.components();
    Reference::issued(a, b, c)
}
impl Strings {
    fn validate(&self) -> R<()> {
        if self.usage.interned != self.roots.len() as u64
            || self.usage.literal_sites != self.sites.len() as u64
            || self.usage.objects < self.usage.interned
            || self.usage.objects > self.limits.max_objects
            || self.usage.retained_units > self.limits.max_retained_units
            || self.usage.interned > self.limits.max_interned
            || self.usage.literal_sites > self.limits.max_literal_sites
            || self.usage.work_units > self.limits.max_work_units
        {
            return Err(invalid("String inventory counts or policy differ"));
        }
        Ok(())
    }
    fn charge(&mut self, n: u64) -> R<()> {
        self.usage.work_units = increased(
            self.usage.work_units,
            n,
            self.limits.max_work_units,
            StringResource::WorkUnits,
        )?;
        Ok(())
    }
    fn reserving(&mut self) -> R<()> {
        #[cfg(test)]
        if let Some(left) = &mut self.fail_reserve_after {
            if *left == 0 {
                return Err(err(StringError::AllocationFailure));
            }
            *left -= 1;
        }
        Ok(())
    }
    fn canonical(&self, entries: &[Entry], heap: u64, h: HeapRef) -> R<()> {
        let p = payload(entries, heap, reference(h))?;
        if p.canonical.and_then(|i| self.roots.get(i)).copied() != Some(h) {
            return Err(invalid("String canonical root differs"));
        }
        Ok(())
    }
    fn lookup(&mut self, entries: &[Entry], heap: u64, units: &[u16]) -> R<(u64, Option<HeapRef>)> {
        let n = units_count(units.len())?;
        let debit = n.checked_add(1).ok_or_else(|| {
            err(StringError::Overflow {
                resource: StringResource::WorkUnits,
            })
        })?;
        self.charge(debit)?;
        let fingerprint = self.hash.hash_one(units);
        #[cfg(test)]
        let fingerprint = self.force_fingerprint.unwrap_or(fingerprint);
        let len = self.buckets.get(&fingerprint).map_or(0, Vec::len);
        for index in 0..len {
            self.charge(1)?;
            let h = *self
                .buckets
                .get(&fingerprint)
                .and_then(|b| b.get(index))
                .ok_or_else(|| invalid("String bucket changed"))?;
            self.canonical(entries, heap, h)?;
            let candidate = payload(entries, heap, reference(h))?;
            if candidate.units.len() == units.len() {
                self.charge(n)?;
                if candidate.units == units {
                    return Ok((fingerprint, Some(h)));
                }
            }
        }
        Ok((fingerprint, None))
    }
    // Reserve only actual additions. Existing buckets are updated through
    // checked mutable references; a new scalar-key row is inserted reserved.
    fn reserve_pool(&mut self, hash: u64) -> R<Option<Vec<HeapRef>>> {
        self.reserving()?;
        self.roots
            .try_reserve(1)
            .map_err(|_| err(StringError::AllocationFailure))?;
        if self.buckets.contains_key(&hash) {
            self.reserving()?;
            self.buckets
                .get_mut(&hash)
                .ok_or_else(|| invalid("pool bucket disappeared"))?
                .try_reserve(1)
                .map_err(|_| err(StringError::AllocationFailure))?;
            Ok(None)
        } else {
            self.reserving()?;
            self.buckets
                .try_reserve(1)
                .map_err(|_| err(StringError::AllocationFailure))?;
            let mut bucket = Vec::new();
            self.reserving()?;
            bucket
                .try_reserve(1)
                .map_err(|_| err(StringError::AllocationFailure))?;
            Ok(Some(bucket))
        }
    }
}
impl ManagedHeap {
    pub fn bind_strings(
        &mut self,
        registry: &DefinitionRegistry,
        string: ClassId,
        limits: StringLimits,
    ) -> R<()> {
        let u = self.reference_type_universe().ok_or_else(|| {
            err(StringError::InvalidBinding(
                "String requires a reference universe",
            ))
        })?;
        let m = self.mirrors.as_ref().ok_or_else(|| {
            err(StringError::InvalidBinding(
                "bind class mirrors before Strings",
            ))
        })?;
        registry.class(m.class)?;
        let definition = registry.class(string)?;
        let node = self.instance_graph_node(string)?;
        if self.instance_graph_registry()? != Some(registry.identity())
            || registry.defining_loader(string)? != u.bootstrap_loader
            || !definition
                .name()
                .map_err(RegistryError::Parse)?
                .equals_ascii("java/lang/String")
            || definition.access_flags & (1 | 16 | 0x200 | 0x400) != 17
            || node.superclass != Some(u.object)
        {
            return Err(err(StringError::InvalidBinding(
                "String must be public final ordinary bootstrap String extending Object",
            )));
        }
        if let Some(s) = &self.strings {
            if s.class == string
                && s.registry == registry.identity()
                && s.universe == u
                && s.classes == self.retained_layout_classes()
                && s.limits == limits
            {
                return Ok(());
            }
            return Err(err(StringError::PolicyConflict));
        }
        if !self.entries.is_empty() || self.initialization_claim.is_some() {
            return Err(err(StringError::InvalidBinding(
                "first String binding requires a pristine unclaimed heap",
            )));
        }
        self.strings = Some(Strings {
            class: string,
            registry: registry.identity(),
            universe: u,
            classes: self.retained_layout_classes(),
            limits,
            usage: Default::default(),
            hash: RandomState::new(),
            buckets: HashMap::new(),
            roots: Vec::new(),
            sites: HashMap::new(),
            #[cfg(test)]
            fail_reserve_after: None,
            #[cfg(test)]
            force_fingerprint: None,
        });
        Ok(())
    }
    pub fn string_usage(&self) -> StringUsage {
        self.strings
            .as_ref()
            .map_or(Default::default(), |s| s.usage)
    }
    pub fn string_roots(&self) -> impl ExactSizeIterator<Item = HeapRef> + '_ {
        self.strings
            .as_ref()
            .map_or(&[][..], |s| s.roots.as_slice())
            .iter()
            .copied()
    }
    pub fn string_units(&self, r: Reference) -> R<Option<&[u16]>> {
        if r.is_null() {
            return Ok(None);
        }
        let e = self.entry(r)?;
        match &e.payload {
            EntryPayload::String(p) => {
                let s = self
                    .strings
                    .as_ref()
                    .ok_or_else(|| invalid("String payload without authority"))?;
                s.validate()?;
                if let Some(i) = p.canonical {
                    if s.roots.get(i).copied() != r.heap_ref() {
                        return Err(invalid("String root index differs"));
                    }
                }
                Ok(Some(&p.units))
            }
            _ => Ok(None),
        }
    }
    fn object_quota(&self, n: usize) -> R<(u64, u64, usize)> {
        let s = self
            .strings
            .as_ref()
            .ok_or_else(|| err(StringError::Disabled))?;
        let objects = increased(
            s.usage.objects,
            1,
            s.limits.max_objects,
            StringResource::Objects,
        )?;
        let units = increased(
            s.usage.retained_units,
            units_count(n)?,
            s.limits.max_retained_units,
            StringResource::RetainedUnits,
        )?;
        let instance = self.instances.as_ref().ok_or(InstanceError::Disabled)?;
        let count = instance
            .count
            .checked_add(1)
            .ok_or(InstanceError::ResourceLimit {
                resource: InstanceResource::Instances,
                limit: instance.options.max_instances,
                required: usize::MAX,
            })?;
        if count > instance.options.max_instances {
            return Err(InstanceError::ResourceLimit {
                resource: InstanceResource::Instances,
                limit: instance.options.max_instances,
                required: count,
            }
            .into());
        }
        self.entries
            .len()
            .checked_add(1)
            .ok_or_else(|| invalid("String arena index exhausted"))?;
        Ok((objects, units, count))
    }
    fn prepare_object(&mut self, units: &[u16]) -> R<Vec<u16>> {
        let s = self
            .strings
            .as_mut()
            .ok_or_else(|| err(StringError::Disabled))?;
        let debit = units_count(units.len())?.checked_add(1).ok_or_else(|| {
            err(StringError::Overflow {
                resource: StringResource::WorkUnits,
            })
        })?;
        s.charge(debit)?;
        s.reserving()?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(units.len())
            .map_err(|_| err(StringError::AllocationFailure))?;
        copy.extend_from_slice(units);
        s.reserving()?;
        reserve_entries(&mut self.entries, 1)?;
        Ok(copy)
    }
    fn fresh_kernel_string(&mut self, units: &[u16]) -> R<Reference> {
        let (objects, retained, count) = self.object_quota(units.len())?;
        let copy = self.prepare_object(units)?;
        let r = Reference::issued(self.identity, self.entries.len(), LIVE_GENERATION);
        let s = self
            .strings
            .as_mut()
            .ok_or_else(|| err(StringError::Disabled))?;
        let state = self.instances.as_mut().ok_or(InstanceError::Disabled)?;
        self.entries.push(Entry {
            generation: LIVE_GENERATION,
            string_backing: None,
            payload: EntryPayload::String(StringPayload {
                units: copy,
                canonical: None,
                backing: None,
            }),
        });
        s.usage.objects = objects;
        s.usage.retained_units = retained;
        state.count = count;
        Ok(r)
    }
    fn literal_kernel_string(
        &mut self,
        origin: ClassId,
        index: u16,
        utf8: u16,
        pool: &ConstantPool,
    ) -> R<Reference> {
        let s = self
            .strings
            .as_mut()
            .ok_or_else(|| err(StringError::Disabled))?;
        s.validate()?;
        s.charge(1)?;
        if let Some(site) = s.sites.get(&(origin, index)).copied() {
            if site.origin != origin || site.index != index || site.utf8 != utf8 {
                return Err(invalid("literal site source affinity differs"));
            }
            s.canonical(&self.entries, self.identity, site.handle)?;
            if payload(&self.entries, self.identity, reference(site.handle))?.canonical
                != Some(site.root)
            {
                return Err(invalid("literal site root affinity differs"));
            }
            return Ok(reference(site.handle));
        }
        let next_site = increased(
            s.usage.literal_sites,
            1,
            s.limits.max_literal_sites,
            StringResource::LiteralSites,
        )?;
        s.reserving()?;
        s.sites
            .try_reserve(1)
            .map_err(|_| err(StringError::AllocationFailure))?;
        s.charge(1)?;
        let units = pool
            .utf8(utf8)
            .map_err(|_| err(StringError::InvalidLiteral { origin, index }))?
            .units();
        let (hash, hit) = s.lookup(&self.entries, self.identity, units)?;
        if let Some(h) = hit {
            if s.sites.contains_key(&(origin, index)) || s.sites.capacity() <= s.sites.len() {
                return Err(invalid("literal site reservation changed"));
            }
            let root = payload(&self.entries, self.identity, reference(h))?
                .canonical
                .ok_or_else(|| invalid("literal hit has no canonical root"))?;
            s.sites.insert(
                (origin, index),
                LiteralSite {
                    origin,
                    index,
                    utf8,
                    handle: h,
                    root,
                },
            );
            s.usage.literal_sites = next_site;
            return Ok(reference(h));
        }
        let interned = increased(
            s.usage.interned,
            1,
            s.limits.max_interned,
            StringResource::Interned,
        )?;
        let (objects, retained, count) = self.object_quota(units.len())?;
        let copy = self.prepare_object(units)?;
        let s = self
            .strings
            .as_mut()
            .ok_or_else(|| err(StringError::Disabled))?;
        s.charge(1)?;
        let bucket = s.reserve_pool(hash)?;
        let root = s.roots.len();
        let r = Reference::issued(self.identity, self.entries.len(), LIVE_GENERATION);
        let h = r
            .heap_ref()
            .ok_or_else(|| invalid("String publication issued null"))?;
        // Fixed staging invariants precede every live mutation.
        if root as u64 != s.usage.interned
            || s.roots.capacity() <= root
            || s.sites.capacity() <= s.sites.len()
            || s.sites.contains_key(&(origin, index))
        {
            return Err(invalid("String literal staging differs"));
        }
        let existing = if bucket.is_none() {
            Some(
                s.buckets
                    .get_mut(&hash)
                    .ok_or_else(|| invalid("String bucket absent"))?,
            )
        } else {
            None
        };
        let state = self.instances.as_mut().ok_or(InstanceError::Disabled)?;
        self.entries.push(Entry {
            generation: LIVE_GENERATION,
            string_backing: None,
            payload: EntryPayload::String(StringPayload {
                units: copy,
                canonical: Some(root),
                backing: None,
            }),
        });
        if let Some(b) = existing {
            b.push(h);
        } else if let Some(mut b) = bucket {
            b.push(h);
            s.buckets.insert(hash, b);
        }
        s.roots.push(h);
        s.sites.insert(
            (origin, index),
            LiteralSite {
                origin,
                index,
                utf8,
                handle: h,
                root,
            },
        );
        s.usage.objects = objects;
        s.usage.retained_units = retained;
        s.usage.interned = interned;
        s.usage.literal_sites = next_site;
        state.count = count;
        Ok(r)
    }
    fn intern_kernel_string(&mut self, r: Reference) -> R<Reference> {
        let p = payload(&self.entries, self.identity, r)?;
        let s = self
            .strings
            .as_mut()
            .ok_or_else(|| err(StringError::Disabled))?;
        s.validate()?;
        s.charge(1)?;
        if p.canonical.is_some() {
            s.canonical(
                &self.entries,
                self.identity,
                r.heap_ref().ok_or_else(|| invalid("canonical null"))?,
            )?;
            return Ok(r);
        }
        let (hash, hit) = s.lookup(&self.entries, self.identity, &p.units)?;
        if let Some(h) = hit {
            return Ok(reference(h));
        }
        let interned = increased(
            s.usage.interned,
            1,
            s.limits.max_interned,
            StringResource::Interned,
        )?;
        s.charge(1)?;
        let bucket = s.reserve_pool(hash)?;
        let h = r
            .heap_ref()
            .ok_or_else(|| invalid("intern receiver null"))?;
        let (_, index, _) = h.components();
        let root = s.roots.len();
        let p = match &mut self
            .entries
            .get_mut(index)
            .ok_or(HeapError::MissingEntry { index })?
            .payload
        {
            EntryPayload::String(p) => p,
            _ => return Err(invalid("intern receiver changed payload")),
        };
        if p.canonical.is_some() || s.roots.capacity() <= root || root as u64 != s.usage.interned {
            return Err(invalid("intern staging differs"));
        }
        let existing = if bucket.is_none() {
            Some(
                s.buckets
                    .get_mut(&hash)
                    .ok_or_else(|| invalid("intern bucket absent"))?,
            )
        } else {
            None
        };
        // Receiver storage already exists; publish the receiver itself, never a
        // replacement, and do not touch object/unit/instance counters.
        p.canonical = Some(root);
        if let Some(b) = existing {
            b.push(h);
        } else if let Some(mut b) = bucket {
            b.push(h);
            s.buckets.insert(hash, b);
        }
        s.roots.push(h);
        s.usage.interned = interned;
        Ok(r)
    }
}
impl FieldResolutionContext<'_> {
    pub fn check_string_profile(&self) -> R<()> {
        self.unchanged()?;
        self.check_class_mirror_profile().map_err(|e| match *e {
            ClassMirrorError::Context(e) => err(StringError::Context(e)),
            ClassMirrorError::Registry(e) => err(StringError::Registry(e)),
            ClassMirrorError::Instance(e) => err(StringError::Instance(e)),
            ClassMirrorError::Type(e) => err(StringError::Context(Box::new(
                FieldAccessError::ReferenceType(e),
            ))),
            _ => err(StringError::InvalidBinding(
                "String requires checked bound mirror authority",
            )),
        })?;
        let s = self
            .heap
            .strings
            .as_ref()
            .ok_or_else(|| err(StringError::Disabled))?;
        s.validate()?;
        if s.registry != self.registry.identity()
            || Some(s.universe) != self.heap.reference_type_universe()
            || s.classes != self.heap.retained_layout_classes()
        {
            return Err(err(StringError::InvalidBinding(
                "String namespace or heap snapshot differs",
            )));
        }
        self.registry.class(s.class)?;
        self.heap.instance_graph_node(s.class)?;
        Ok(())
    }
    pub fn fresh_string(&mut self, units: &[u16]) -> R<Reference> {
        self.check_string_profile()?;
        self.heap.fresh_kernel_string(units)
    }
    pub fn string_literal(&mut self, origin: ClassId, index: u16) -> R<Reference> {
        self.check_string_profile()?;
        let definition = self.registry.class(origin)?;
        self.heap.instance_graph_node(origin)?;
        let Constant::String { string_index } = definition
            .constant_pool
            .get(index)
            .map_err(|_| err(StringError::InvalidLiteral { origin, index }))?
        else {
            return Err(err(StringError::InvalidLiteral { origin, index }));
        };
        self.heap
            .literal_kernel_string(origin, index, *string_index, &definition.constant_pool)
    }
    pub fn intern_string(&mut self, receiver: Reference) -> R<Reference> {
        self.check_string_profile()?;
        self.heap.intern_kernel_string(receiver)
    }
    fn intern_role(&self, method: MethodId) -> R<Option<bool>> {
        let definition = self.registry.class(method.owner())?;
        let member = self.registry.method(method)?;
        let Some(s) = &self.heap.strings else {
            return Ok(None);
        };
        if method.owner() != s.class {
            return Ok(None);
        }
        let name = definition
            .constant_pool
            .utf8(member.name_index)
            .map_err(RegistryError::Parse)?;
        let descriptor = definition
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(RegistryError::Parse)?;
        if name.units().len() != 6
            || descriptor.units().len() != 20
            || !name.equals_ascii("intern")
            || !descriptor.equals_ascii("()Ljava/lang/String;")
        {
            return Ok(None);
        }
        Ok(Some(
            member.access_flags & (1 | 2 | 4 | 8 | 32 | 0x100 | 0x400) == 0x101,
        ))
    }
    pub fn is_string_intern(&mut self, method: MethodId) -> R<bool> {
        self.unchanged()?;
        let role = self.intern_role(method)?;
        let Some(valid) = role else {
            return Ok(false);
        };
        self.step()?;
        if !valid {
            return Err(err(StringError::InvalidBinding(
                "String.intern must be public native instance without synchronized/abstract/Code replacement",
            )));
        }
        self.check_string_profile()?;
        // Checked context opening attested the String role's actual self-name
        // initiation under its defining bootstrap loader. Its immutable exact
        // descriptor returns that same role, not an unrelated name assertion.
        Ok(true)
    }
    pub(crate) fn check_string_method_entry(
        &mut self,
        method: MethodId,
        receiver: Reference,
    ) -> Result<(), MethodAccessError> {
        let Some(s) = &self.heap.strings else {
            return Ok(());
        };
        if method.owner() != s.class {
            return Ok(());
        }
        if !matches!(
            self.heap
                .entry(receiver)
                .map_err(InstanceError::from)?
                .payload,
            EntryPayload::String(_)
        ) {
            return Ok(());
        }
        self.check_string_profile()
            .map_err(MethodAccessError::String)?;
        match self
            .intern_role(method)
            .map_err(MethodAccessError::String)?
        {
            Some(true) => return Ok(()),
            Some(false) => {
                return Err(MethodAccessError::String(err(StringError::InvalidBinding(
                    "String.intern has an incompatible native role",
                ))));
            }
            None => {}
        }
        if self
            .heap
            .permits_string_method(method)
            .map_err(|e| MethodAccessError::Context(FieldAccessError::StringBody(e)))?
        {
            return Ok(());
        }
        Err(InstanceError::OpaqueStringObject {
            class: method.owner(),
        }
        .into())
    }
}

#[cfg(test)]
#[path = "string_tests.rs"]
pub(crate) mod tests;

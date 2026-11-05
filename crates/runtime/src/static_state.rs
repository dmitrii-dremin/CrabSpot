//! Heap-owned default static memory. No lifecycle transition or initializer runs.
use crate::{
    ClassId, FieldAccessError, FieldId, FieldJavaError, FieldResolutionContext, HeapError, HeapRef,
    InstanceError, ReferenceFieldMismatch, RegistryError, TypeAccessError, Value, ValueKind,
    instance::{FieldKind, classify},
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

pub const STATIC_STATE_SLOT_BYTES: usize = core::mem::size_of::<Value>();
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticStateOptions {
    pub max_classes: usize,
    pub max_slots: usize,
    pub max_payload_bytes: usize,
}
impl Default for StaticStateOptions {
    fn default() -> Self {
        Self {
            max_classes: 16_384,
            max_slots: 65_536,
            max_payload_bytes: 8_388_608,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticStateProfile {
    registry: u64,
    heap: u64,
    classes: usize,
    slots: usize,
    bytes: usize,
}
impl StaticStateProfile {
    pub fn class_count(self) -> usize {
        self.classes
    }
    pub fn slot_count(self) -> usize {
        self.slots
    }
    pub fn payload_bytes(self) -> usize {
        self.bytes
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticResource {
    Classes,
    Slots,
    PayloadBytes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticJavaError {
    LinkageError,
    NoSuchField,
    IllegalAccess,
    IncompatibleClassChange,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaticAccessError {
    Context(FieldAccessError),
    Type(Box<TypeAccessError>),
    Heap(HeapError),
    Java(StaticJavaError),
    ProfileMismatch,
    OptionsMismatch,
    InvalidState(&'static str),
    MissingLocation {
        field: FieldId,
    },
    ValueKindMismatch {
        field: FieldId,
        expected: ValueKind,
        actual: ValueKind,
    },
    IncompatibleReference(Box<ReferenceFieldMismatch>),
    ResourceLimit {
        resource: StaticResource,
        limit: usize,
        required: usize,
    },
    AccountingOverflow {
        resource: StaticResource,
    },
    AllocationFailure {
        resource: &'static str,
    },
}
impl fmt::Display for StaticAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(error) => write!(f, "static context failed: {error}"),
            Self::Type(error) => write!(f, "static reference type failed: {error}"),
            Self::Heap(error) => write!(f, "static heap validation failed: {error}"),
            Self::Java(error) => write!(f, "static access failed: {error:?}"),
            Self::ProfileMismatch => {
                f.write_str("static profile belongs to a different or changed heap state")
            }
            Self::OptionsMismatch => {
                f.write_str("static allocation options differ from the first successful allocation")
            }
            Self::InvalidState(reason) => write!(f, "invalid static state: {reason}"),
            Self::MissingLocation { field } => write!(f, "no static location for {field:?}"),
            Self::ValueKindMismatch {
                field,
                expected,
                actual,
            } => write!(
                f,
                "static {field:?} requires {expected:?}, found {actual:?}"
            ),
            Self::IncompatibleReference(error) => write!(
                f,
                "static {:?} reference type {:?} is not assignable to {:?}",
                error.field, error.source, error.target
            ),
            Self::ResourceLimit {
                resource,
                limit,
                required,
            } => write!(
                f,
                "static {resource:?} limit {limit} exceeded (required {required})"
            ),
            Self::AccountingOverflow { resource } => {
                write!(f, "static {resource:?} accounting overflow")
            }
            Self::AllocationFailure { resource } => {
                write!(f, "static allocation failed for {resource}")
            }
        }
    }
}
impl std::error::Error for StaticAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Context(error) => Some(error),
            Self::Type(error) => Some(error.as_ref()),
            Self::Heap(error) => Some(error),
            _ => None,
        }
    }
}
impl From<FieldAccessError> for StaticAccessError {
    fn from(error: FieldAccessError) -> Self {
        Self::Context(error)
    }
}
impl From<RegistryError> for StaticAccessError {
    fn from(error: RegistryError) -> Self {
        FieldAccessError::Registry(error).into()
    }
}
impl From<InstanceError> for StaticAccessError {
    fn from(error: InstanceError) -> Self {
        FieldAccessError::Instance(error).into()
    }
}
impl From<HeapError> for StaticAccessError {
    fn from(error: HeapError) -> Self {
        Self::Heap(error)
    }
}
impl From<TypeAccessError> for StaticAccessError {
    fn from(error: TypeAccessError) -> Self {
        Self::Type(Box::new(error))
    }
}
fn selector_error(error: FieldAccessError) -> StaticAccessError {
    match error {
        FieldAccessError::Java(FieldJavaError::LinkageError) => {
            StaticAccessError::Java(StaticJavaError::LinkageError)
        }
        FieldAccessError::Java(FieldJavaError::NoSuchField) => {
            StaticAccessError::Java(StaticJavaError::NoSuchField)
        }
        FieldAccessError::Java(FieldJavaError::IllegalAccess) => {
            StaticAccessError::Java(StaticJavaError::IllegalAccess)
        }
        FieldAccessError::Java(FieldJavaError::IncompatibleClassChange) => {
            StaticAccessError::Java(StaticJavaError::IncompatibleClassChange)
        }
        error => error.into(),
    }
}
fn reserve<T>(values: &mut Vec<T>, additional: usize) -> Result<(), StaticAccessError> {
    values
        .try_reserve(additional)
        .map_err(|_| StaticAccessError::AllocationFailure {
            resource: "static metadata/values",
        })
}
fn reserve_map<K: Eq + std::hash::Hash, V>(
    values: &mut HashMap<K, V>,
    additional: usize,
) -> Result<(), StaticAccessError> {
    values
        .try_reserve(additional)
        .map_err(|_| StaticAccessError::AllocationFailure {
            resource: "static positions/classification",
        })
}
fn reserve_set(values: &mut HashSet<ClassId>, additional: usize) -> Result<(), StaticAccessError> {
    values
        .try_reserve(additional)
        .map_err(|_| StaticAccessError::AllocationFailure {
            resource: "static class membership",
        })
}
fn add(
    current: usize,
    amount: usize,
    resource: StaticResource,
) -> Result<usize, StaticAccessError> {
    current
        .checked_add(amount)
        .ok_or(StaticAccessError::AccountingOverflow { resource })
}
fn payload_bytes(slots: usize) -> Result<usize, StaticAccessError> {
    slots
        .checked_mul(STATIC_STATE_SLOT_BYTES)
        .ok_or(StaticAccessError::AccountingOverflow {
            resource: StaticResource::PayloadBytes,
        })
}
fn limit(
    required: usize,
    maximum: usize,
    resource: StaticResource,
) -> Result<(), StaticAccessError> {
    if required > maximum {
        Err(StaticAccessError::ResourceLimit {
            resource,
            limit: maximum,
            required,
        })
    } else {
        Ok(())
    }
}
#[derive(Debug)]
struct Slot {
    kind: FieldKind,
    is_final: bool,
    value: Value,
}
#[derive(Debug)]
pub(crate) struct StaticState {
    registry: u64,
    heap: u64,
    options: StaticStateOptions,
    classes: HashSet<ClassId>,
    ranges: HashMap<ClassId, (usize, usize)>,
    fields: Vec<FieldId>,
    slots: Vec<Slot>,
    positions: HashMap<FieldId, usize>,
    bytes: usize,
}
impl StaticState {
    fn empty(registry: u64, heap: u64, options: StaticStateOptions) -> Self {
        Self {
            registry,
            heap,
            options,
            classes: HashSet::new(),
            ranges: HashMap::new(),
            fields: Vec::new(),
            slots: Vec::new(),
            positions: HashMap::new(),
            bytes: 0,
        }
    }
    fn profile(&self) -> StaticStateProfile {
        StaticStateProfile {
            registry: self.registry,
            heap: self.heap,
            classes: self.classes.len(),
            slots: self.slots.len(),
            bytes: self.bytes,
        }
    }
    fn counts(&self) -> Result<(), StaticAccessError> {
        if self.ranges.len() != self.classes.len()
            || self.fields.len() != self.slots.len()
            || self.bytes != payload_bytes(self.slots.len())?
        {
            return Err(StaticAccessError::InvalidState(
                "field/value/payload counts differ",
            ));
        }
        Ok(())
    }
}
fn reserve_destinations(
    state: &mut StaticState,
    classes: usize,
    slots: usize,
) -> Result<(), StaticAccessError> {
    reserve_set(&mut state.classes, classes)?;
    reserve_map(&mut state.ranges, classes)?;
    reserve(&mut state.fields, slots)?;
    reserve(&mut state.slots, slots)?;
    reserve_map(&mut state.positions, slots)
}
impl FieldResolutionContext<'_> {
    pub fn allocate_static_state(
        &mut self,
        options: StaticStateOptions,
    ) -> Result<StaticStateProfile, StaticAccessError> {
        self.unchanged()?;
        let target = self.heap().retained_layout_classes();
        let registry = self.registry().identity();
        let heap_identity = self.heap().identity;
        let (old_classes, old_slots) = if let Some(state) = &self.heap().static_state {
            if state.registry != registry || state.heap != heap_identity {
                return Err(StaticAccessError::InvalidState(
                    "retained static ownership differs from heap",
                ));
            }
            if state.options != options {
                return Err(StaticAccessError::OptionsMismatch);
            }
            state.counts()?;
            if state.classes.len() > target {
                return Err(StaticAccessError::InvalidState(
                    "admitted static class membership shrank",
                ));
            }
            if state.classes.len() == target {
                return Ok(state.profile());
            }
            (state.classes.len(), state.slots.len())
        } else {
            (0, 0)
        };
        limit(target, options.max_classes, StaticResource::Classes)?;
        self.charge_bulk_work(target, 0)?;
        let mut inventory = Vec::new();
        reserve(&mut inventory, target)?;
        for class in self.heap().instance_graph_classes()? {
            if inventory.len() >= target {
                return Err(StaticAccessError::InvalidState(
                    "admitted class inventory exceeds its count",
                ));
            }
            inventory.push(class);
        }
        if inventory.len() != target {
            return Err(StaticAccessError::InvalidState(
                "admitted class inventory is incomplete",
            ));
        }
        inventory.sort_unstable_by_key(|class| class.index);
        if inventory.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(StaticAccessError::InvalidState(
                "admitted class inventory has duplicates",
            ));
        }
        if let Some(state) = &self.heap().static_state {
            // Staging below performs no resolver callback or old-value mutation.
            // This retained-prefix proof therefore remains valid through append.
            self.validate_static_prefix(state)?;
            for class in &state.classes {
                self.registry().class(*class)?;
                self.heap().instance_graph_node(*class)?;
                if inventory
                    .binary_search_by_key(&class.index, |item| item.index)
                    .ok()
                    .and_then(|i| inventory.get(i))
                    != Some(class)
                {
                    return Err(StaticAccessError::InvalidState(
                        "retained static class is no longer admitted",
                    ));
                }
            }
        }
        let mut new_classes = Vec::new();
        reserve(&mut new_classes, target - old_classes)?;
        let mut fields = Vec::new();
        let mut slots = Vec::new();
        let mut kinds = HashMap::new();
        let mut total_slots = old_slots;
        for class in inventory {
            self.registry().class(class)?;
            self.heap().instance_graph_node(class)?;
            if self
                .heap()
                .static_state
                .as_ref()
                .is_some_and(|state| state.classes.contains(&class))
            {
                continue;
            }
            let range_start = total_slots;
            let definition = self.registry().class(class)?;
            for (index, member) in definition.fields.iter().enumerate() {
                self.step()?;
                if member.access_flags & 8 == 0 {
                    continue;
                }
                total_slots = add(total_slots, 1, StaticResource::Slots)?;
                limit(total_slots, options.max_slots, StaticResource::Slots)?;
                let bytes = payload_bytes(total_slots)?;
                limit(
                    bytes,
                    options.max_payload_bytes,
                    StaticResource::PayloadBytes,
                )?;
                let field = FieldId {
                    owner: class,
                    index,
                };
                let key = (class, member.descriptor_index);
                let kind = if let Some(kind) = kinds.get(&key) {
                    *kind
                } else {
                    let descriptor = definition
                        .constant_pool
                        .utf8(member.descriptor_index)
                        .map_err(RegistryError::Parse)?
                        .units();
                    self.charge_bulk_work(descriptor.len(), 0)?;
                    let kind = classify(field, descriptor)?;
                    reserve_map(&mut kinds, 1)?;
                    kinds.insert(key, kind);
                    kind
                };
                reserve(&mut fields, 1)?;
                reserve(&mut slots, 1)?;
                fields.push(field);
                slots.push(Slot {
                    kind,
                    is_final: member.access_flags & 16 != 0,
                    value: kind.default_value(),
                });
            }
            new_classes.push((class, (range_start, total_slots)));
        }
        let bytes = payload_bytes(total_slots)?;
        // All retained invariants and every reservation precede the first
        // published membership/location. No callback or fallible work follows.
        if let Some(state) = self.heap_mut().static_state.as_mut() {
            reserve_destinations(state, new_classes.len(), fields.len())?;
            for (class, range) in new_classes {
                state.classes.insert(class);
                state.ranges.insert(class, range);
            }
            for (field, slot) in fields.into_iter().zip(slots) {
                state.positions.insert(field, state.fields.len());
                state.fields.push(field);
                state.slots.push(slot);
            }
            state.bytes = bytes;
            Ok(state.profile())
        } else {
            let mut state = StaticState::empty(registry, heap_identity, options);
            reserve_set(&mut state.classes, new_classes.len())?;
            reserve_map(&mut state.ranges, new_classes.len())?;
            reserve_map(&mut state.positions, fields.len())?;
            for (class, range) in new_classes {
                state.classes.insert(class);
                state.ranges.insert(class, range);
            }
            for (index, field) in fields.iter().copied().enumerate() {
                state.positions.insert(field, index);
            }
            state.fields = fields;
            state.slots = slots;
            state.bytes = bytes;
            let profile = state.profile();
            self.heap_mut().static_state = Some(state);
            Ok(profile)
        }
    }
    fn validate_static_profile(
        &self,
        profile: &StaticStateProfile,
    ) -> Result<(), StaticAccessError> {
        self.unchanged()?;
        let Some(state) = &self.heap().static_state else {
            return Err(StaticAccessError::ProfileMismatch);
        };
        if profile.registry != self.registry().identity()
            || profile.heap != self.heap().identity
            || *profile != state.profile()
            || profile.classes != self.heap().retained_layout_classes()
        {
            return Err(StaticAccessError::ProfileMismatch);
        }
        state.counts()?;
        Ok(())
    }
    pub fn check_static_profile(
        &mut self,
        profile: &StaticStateProfile,
    ) -> Result<(), StaticAccessError> {
        self.validate_static_profile(profile)
    }
    pub fn static_fields(
        &self,
        profile: &StaticStateProfile,
    ) -> Result<&[FieldId], StaticAccessError> {
        self.validate_static_profile(profile)?;
        Ok(&self
            .heap()
            .static_state
            .as_ref()
            .ok_or(StaticAccessError::ProfileMismatch)?
            .fields)
    }
    fn static_location(&self, field: FieldId) -> Result<(usize, &Slot), StaticAccessError> {
        let definition = self.registry().class(field.owner())?;
        let member = self.registry().field(field)?;
        self.heap().instance_graph_node(field.owner())?;
        if member.access_flags & 8 == 0 {
            return Err(StaticAccessError::MissingLocation { field });
        }
        let state = self
            .heap()
            .static_state
            .as_ref()
            .ok_or(StaticAccessError::ProfileMismatch)?;
        if !state.classes.contains(&field.owner()) {
            return Err(StaticAccessError::MissingLocation { field });
        }
        let index = *state
            .positions
            .get(&field)
            .ok_or(StaticAccessError::MissingLocation { field })?;
        if state.fields.get(index) != Some(&field) {
            return Err(StaticAccessError::InvalidState(
                "static position differs from field order",
            ));
        }
        let slot = state
            .slots
            .get(index)
            .ok_or(StaticAccessError::InvalidState(
                "static position is outside values",
            ))?;
        let first = definition
            .constant_pool
            .utf8(member.descriptor_index)
            .map_err(RegistryError::Parse)?
            .units()
            .first()
            .copied();
        let expected = match first {
            Some(90) => FieldKind::Boolean,
            Some(66) => FieldKind::Byte,
            Some(67) => FieldKind::Char,
            Some(83) => FieldKind::Short,
            Some(73) => FieldKind::Int,
            Some(74) => FieldKind::Long,
            Some(70) => FieldKind::Float,
            Some(68) => FieldKind::Double,
            Some(76 | 91) => FieldKind::Reference,
            _ => {
                return Err(StaticAccessError::InvalidState(
                    "invalid immutable static descriptor tag",
                ));
            }
        };
        if slot.kind != expected || slot.is_final != (member.access_flags & 16 != 0) {
            return Err(StaticAccessError::InvalidState(
                "retained static kind/finality differs from declaration",
            ));
        }
        Ok((index, slot))
    }
    fn read_static_location(&self, field: FieldId) -> Result<Value, StaticAccessError> {
        let (_, slot) = self.static_location(field)?;
        let expected = slot.kind.default_value().kind();
        if slot.value.kind() != expected {
            return Err(StaticAccessError::ValueKindMismatch {
                field,
                expected,
                actual: slot.value.kind(),
            });
        }
        if let Value::Reference(reference) = slot.value {
            if !reference.is_null() {
                self.heap().entry(reference)?;
            }
        } else if slot.kind.store(field, slot.value)? != slot.value {
            return Err(StaticAccessError::InvalidState(
                "retained narrow static value is not normalized",
            ));
        }
        Ok(slot.value)
    }
    fn validate_static_prefix(&self, state: &StaticState) -> Result<(), StaticAccessError> {
        state.counts()?;
        if state.positions.len() != state.fields.len() {
            return Err(StaticAccessError::InvalidState(
                "static position count differs from field count",
            ));
        }
        for (index, field) in state.fields.iter().enumerate() {
            if state.positions.get(field) != Some(&index) {
                return Err(StaticAccessError::InvalidState(
                    "static prefix positions are not a bijection",
                ));
            }
            self.read_static_location(*field)?;
            let &(start, end) =
                state
                    .ranges
                    .get(&field.owner())
                    .ok_or(StaticAccessError::InvalidState(
                        "static owner range is missing",
                    ))?;
            if index < start || index >= end {
                return Err(StaticAccessError::InvalidState(
                    "static field is outside its owner range",
                ));
            }
        }
        for class in &state.classes {
            let &(start, end) = state
                .ranges
                .get(class)
                .ok_or(StaticAccessError::InvalidState(
                    "static class range is missing",
                ))?;
            if start > end
                || end > state.fields.len()
                || state.fields[start..end]
                    .iter()
                    .any(|field| field.owner() != *class)
            {
                return Err(StaticAccessError::InvalidState(
                    "static class range is invalid",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn initialization_location(
        &self,
        field: FieldId,
    ) -> Result<(usize, FieldKind), StaticAccessError> {
        let (index, slot) = self.static_location(field)?;
        Ok((index, slot.kind))
    }
    pub(crate) fn initialization_own_fields(
        &self,
        class: ClassId,
    ) -> Result<&[FieldId], StaticAccessError> {
        self.registry().class(class)?;
        self.heap().instance_graph_node(class)?;
        let state = self
            .heap()
            .static_state
            .as_ref()
            .ok_or(StaticAccessError::ProfileMismatch)?;
        let &(start, end) = state
            .ranges
            .get(&class)
            .ok_or(StaticAccessError::InvalidState(
                "static class range is missing",
            ))?;
        let fields = state
            .fields
            .get(start..end)
            .ok_or(StaticAccessError::InvalidState(
                "static class range is outside storage",
            ))?;
        if start
            .checked_sub(1)
            .and_then(|index| state.fields.get(index))
            .is_some_and(|field| field.owner() == class)
            || state
                .fields
                .get(end)
                .is_some_and(|field| field.owner() == class)
        {
            return Err(StaticAccessError::InvalidState(
                "static class range is incomplete",
            ));
        }
        Ok(fields)
    }
    pub(crate) fn publish_initialization_constants(
        &mut self,
        class: ClassId,
        entries: &[crate::initialization::InitializationConstant],
    ) -> Result<(), StaticAccessError> {
        // The sole caller validated every entry/destination under this exclusive
        // context borrow. All fallible indexing precedes the first mutation.
        let state = self
            .heap_mut()
            .static_state
            .as_mut()
            .ok_or(StaticAccessError::ProfileMismatch)?;
        let &(start, end) = state
            .ranges
            .get(&class)
            .ok_or(StaticAccessError::InvalidState(
                "static class range is missing",
            ))?;
        let slots = state
            .slots
            .get_mut(start..end)
            .ok_or(StaticAccessError::InvalidState(
                "static class range is outside values",
            ))?;
        let mut next = entries.iter().peekable();
        for (offset, slot) in slots.iter_mut().enumerate() {
            if next
                .peek()
                .is_some_and(|entry| entry.location.checked_sub(start) == Some(offset))
            {
                if let Some(entry) = next.next() {
                    slot.value = entry.value;
                }
            }
        }
        Ok(())
    }
    pub fn inspect_static(
        &self,
        profile: &StaticStateProfile,
        field: FieldId,
    ) -> Result<Value, StaticAccessError> {
        self.validate_static_profile(profile)?;
        self.read_static_location(field)
    }
    pub fn static_roots(
        &self,
        profile: &StaticStateProfile,
    ) -> Result<impl Iterator<Item = HeapRef> + '_, StaticAccessError> {
        self.validate_static_profile(profile)?;
        let state = self
            .heap()
            .static_state
            .as_ref()
            .ok_or(StaticAccessError::ProfileMismatch)?;
        for field in &state.fields {
            self.read_static_location(*field)?;
        }
        Ok(state.slots.iter().filter_map(|slot| {
            if let Value::Reference(reference) = slot.value {
                reference.heap_ref()
            } else {
                None
            }
        }))
    }
    pub fn read_static(
        &mut self,
        profile: &StaticStateProfile,
        origin: ClassId,
        index: u16,
    ) -> Result<Value, StaticAccessError> {
        self.validate_static_profile(profile)?;
        let field = self
            .select_field_id(origin, index)
            .map_err(selector_error)?;
        if self.registry().field(field)?.access_flags & 8 == 0 {
            return Err(StaticAccessError::Java(
                StaticJavaError::IncompatibleClassChange,
            ));
        }
        self.read_static_location(field)
    }
    pub fn write_static(
        &mut self,
        profile: &StaticStateProfile,
        origin: ClassId,
        index: u16,
        value: Value,
    ) -> Result<(), StaticAccessError> {
        self.validate_static_profile(profile)?;
        let field = self
            .select_field_id(origin, index)
            .map_err(selector_error)?;
        let flags = self.registry().field(field)?.access_flags;
        if flags & 8 == 0 {
            return Err(StaticAccessError::Java(
                StaticJavaError::IncompatibleClassChange,
            ));
        }
        if flags & 16 != 0 {
            return Err(StaticAccessError::Java(StaticJavaError::IllegalAccess));
        }
        self.write_static_value(profile, field, value, None)
    }
    pub(crate) fn declaring_initializer(
        &self,
        origin: crate::MethodId,
        owner: ClassId,
    ) -> Result<bool, StaticAccessError> {
        let definition = self.registry().class(origin.owner())?;
        let member = self.registry().method(origin)?;
        self.heap().instance_graph_node(origin.owner())?;
        Ok(origin.owner() == owner
            && definition
                .constant_pool
                .utf8(member.name_index)
                .map_err(RegistryError::Parse)?
                .equals_ascii("<clinit>")
            && definition
                .constant_pool
                .utf8(member.descriptor_index)
                .map_err(RegistryError::Parse)?
                .equals_ascii("()V"))
    }
    pub(crate) fn write_initialization_value(
        &mut self,
        profile: &StaticStateProfile,
        field: FieldId,
        value: Value,
        origin: crate::MethodId,
    ) -> Result<(), StaticAccessError> {
        self.write_static_value(profile, field, value, Some(origin))
    }
    fn write_static_value(
        &mut self,
        profile: &StaticStateProfile,
        field: FieldId,
        value: Value,
        origin: Option<crate::MethodId>,
    ) -> Result<(), StaticAccessError> {
        self.validate_static_profile(profile)?;
        let member = self.registry().field(field)?;
        if member.access_flags & 8 == 0 {
            return Err(StaticAccessError::Java(
                StaticJavaError::IncompatibleClassChange,
            ));
        }
        if member.access_flags & 16 != 0
            && !origin
                .map(|method| self.declaring_initializer(method, field.owner()))
                .transpose()?
                .unwrap_or(false)
        {
            return Err(StaticAccessError::Java(StaticJavaError::IllegalAccess));
        }
        let (_, slot) = self.static_location(field)?;
        let expected = slot.kind.default_value().kind();
        if value.kind() != expected {
            return Err(StaticAccessError::ValueKindMismatch {
                field,
                expected,
                actual: value.kind(),
            });
        }
        if let Value::Reference(reference) = value {
            if !reference.is_null() {
                let source =
                    self.reference_type(reference)?
                        .ok_or(StaticAccessError::InvalidState(
                            "nonnull static source has no runtime type",
                        ))?;
                let target = self.resolve_reference_field_type(field)?;
                if !self.is_assignable(source, target)? {
                    return Err(StaticAccessError::IncompatibleReference(Box::new(
                        ReferenceFieldMismatch {
                            field,
                            source,
                            target,
                        },
                    )));
                }
            }
        }
        if origin.is_none() {
            self.commit_static(profile, field, value)
        } else {
            self.commit_static_origin(profile, field, value, origin)
        }
    }
    fn commit_static(
        &mut self,
        profile: &StaticStateProfile,
        field: FieldId,
        value: Value,
    ) -> Result<(), StaticAccessError> {
        self.commit_static_origin(profile, field, value, None)
    }
    fn commit_static_origin(
        &mut self,
        profile: &StaticStateProfile,
        field: FieldId,
        value: Value,
        origin: Option<crate::MethodId>,
    ) -> Result<(), StaticAccessError> {
        self.validate_static_profile(profile)?;
        let (index, slot) = self.static_location(field)?;
        if slot.is_final
            && !origin
                .map(|method| self.declaring_initializer(method, field.owner()))
                .transpose()?
                .unwrap_or(false)
        {
            return Err(StaticAccessError::Java(StaticJavaError::IllegalAccess));
        }
        let expected = slot.kind.default_value().kind();
        if value.kind() != expected {
            return Err(StaticAccessError::ValueKindMismatch {
                field,
                expected,
                actual: value.kind(),
            });
        }
        let normalized = if let Value::Reference(reference) = value {
            if !reference.is_null() {
                self.heap().entry(reference)?;
            }
            value
        } else {
            slot.kind.store(field, value)?
        };
        let target = self
            .heap_mut()
            .static_state
            .as_mut()
            .ok_or(StaticAccessError::ProfileMismatch)?
            .slots
            .get_mut(index)
            .ok_or(StaticAccessError::InvalidState(
                "validated static slot disappeared",
            ))?;
        target.value = normalized;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod rt {
        pub use crate::*;
    }
    #[allow(dead_code)]
    mod init_fixtures {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/initialization.rs"
        ));
    }

    #[test]
    fn a_late_foreign_field_in_a_corrupt_constant_owner_range_cannot_partially_publish() {
        use init_fixtures as f;
        let mut w = f::world();
        let own = f::define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "Constants",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[
                f::Field::new("a", "I", 9, Some(f::Cv::Int(41))),
                f::Field::new("b", "I", 25, Some(f::Cv::Int(79))),
            ],
            &[],
            &[],
            65,
        );
        let other = f::define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "Other",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[f::Field::new("later", "J", 9, None)],
            &[],
            &[],
            65,
        );
        let mut heap = f::heap(
            &w,
            &[w.error, own.class, other.class],
            &mut SameLoaderResolver,
        );
        let mut resolver = SameLoaderResolver;
        let mut ctx = f::context(&w, &mut heap, &mut resolver);
        let (s, e) = f::profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        let plan = ctx
            .plan_initialization(&claim, own.class, f::limits())
            .unwrap();
        let a = f::field(&w, own.class, "a", "I");
        ctx.commit_static(&s, a, Value::Int(13)).unwrap();
        let before = snapshot(ctx.heap());
        let original = state(&mut ctx).ranges[&own.class];
        state(&mut ctx)
            .ranges
            .insert(own.class, (original.0, original.1 + 1));
        let corrupt = snapshot(ctx.heap());
        assert!(ctx.install_initialization_constants(&claim, &plan).is_err());
        assert_eq!(snapshot(ctx.heap()), corrupt);
        assert_eq!(ctx.inspect_static(&s, a), Ok(Value::Int(13)));
        state(&mut ctx).ranges.insert(own.class, original);
        assert_eq!(snapshot(ctx.heap()), before);
    }
    use crate::{
        DefinitionRegistry, FieldResolutionOptions, HeapOptions, HierarchyOptions, InstanceOptions,
        ManagedHeap, Reference, ReferenceTypeUniverse, RegistryOptions, SameLoaderResolver,
        build_hierarchy,
    };
    use crabspot_classfile::ParseOptions;
    use std::error::Error;
    fn words(values: &[u16]) -> Vec<u8> {
        values.iter().copied().flat_map(u16::to_be_bytes).collect()
    }
    fn define(
        registry: &mut DefinitionRegistry,
        loader: crate::LoaderId,
        name: &str,
        parent: Option<&str>,
        flags: u16,
        fields: &[(&str, &str, u16)],
    ) -> ClassId {
        let mut entries = Vec::new();
        let mut text = |value: &str| {
            let index = (entries.len() + 1) as u16;
            let mut entry = vec![1];
            entry.extend(words(&[value.len() as u16]));
            entry.extend(value.bytes());
            entries.push(entry);
            index
        };
        let own_name = text(name);
        let parent_name = text(parent.unwrap_or("java/lang/Object"));
        let mut fields_cp = Vec::new();
        for (name, desc, flags) in fields {
            fields_cp.push((text(name), text(desc), *flags));
        }
        let own = (entries.len() + 1) as u16;
        entries.push([vec![7], words(&[own_name])].concat());
        let parent_cp = (entries.len() + 1) as u16;
        entries.push([vec![7], words(&[parent_name])].concat());
        let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
        bytes.extend(words(&[(entries.len() + 1) as u16]));
        bytes.extend(entries.into_iter().flatten());
        bytes.extend(words(&[
            flags,
            own,
            if parent.is_some() { parent_cp } else { 0 },
            0,
            fields.len() as u16,
        ]));
        for (name, desc, flags) in fields_cp {
            bytes.extend(words(&[flags, name, desc, 0]));
        }
        bytes.extend(words(&[0, 0]));
        registry
            .define(
                loader,
                &name.encode_utf16().collect::<Vec<_>>(),
                &bytes,
                ParseOptions::default(),
            )
            .unwrap()
    }
    struct Fixture {
        registry: DefinitionRegistry,
        heap: ManagedHeap,
        owner: ClassId,
        extra: ClassId,
        fields: Vec<FieldId>,
    }
    fn fixture() -> Fixture {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let l = registry.create_loader().unwrap();
        let object = define(&mut registry, l, "java/lang/Object", None, 0x21, &[]);
        let cloneable = define(
            &mut registry,
            l,
            "java/lang/Cloneable",
            Some("java/lang/Object"),
            0x601,
            &[],
        );
        let serializable = define(
            &mut registry,
            l,
            "java/io/Serializable",
            Some("java/lang/Object"),
            0x601,
            &[],
        );
        let owner = define(
            &mut registry,
            l,
            "C",
            Some("java/lang/Object"),
            0x21,
            &[
                ("z", "Z", 9),
                ("b", "B", 9),
                ("r", "Ljava/lang/Object;", 9),
                ("final", "Ljava/lang/Object;", 0x19),
                ("instance", "I", 1),
            ],
        );
        let extra = define(
            &mut registry,
            l,
            "Extra",
            Some("java/lang/Object"),
            0x21,
            &[("later", "J", 9)],
        );
        let plans: Vec<_> = [cloneable, serializable, owner]
            .iter()
            .map(|class| {
                build_hierarchy(
                    &registry,
                    *class,
                    &mut SameLoaderResolver,
                    HierarchyOptions::default(),
                )
                .unwrap()
            })
            .collect();
        let mut heap =
            ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
                .unwrap();
        heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(
            &registry,
            ReferenceTypeUniverse {
                bootstrap_loader: l,
                object,
                cloneable,
                serializable,
            },
        )
        .unwrap();
        let fields = (0..5).map(|index| FieldId { owner, index }).collect();
        Fixture {
            registry,
            heap,
            owner,
            extra,
            fields,
        }
    }
    #[derive(Debug, Eq, PartialEq)]
    struct StateCopy {
        profile: StaticStateProfile,
        options: StaticStateOptions,
        classes: HashSet<ClassId>,
        ranges: HashMap<ClassId, (usize, usize)>,
        fields: Vec<FieldId>,
        slots: Vec<(FieldKind, bool, Value)>,
        positions: HashMap<FieldId, usize>,
    }
    fn snapshot(heap: &ManagedHeap) -> StateCopy {
        let state = heap.static_state.as_ref().unwrap();
        StateCopy {
            profile: state.profile(),
            options: state.options,
            classes: state.classes.clone(),
            ranges: state.ranges.clone(),
            fields: state.fields.clone(),
            slots: state
                .slots
                .iter()
                .map(|slot| (slot.kind, slot.is_final, slot.value))
                .collect(),
            positions: state.positions.clone(),
        }
    }
    fn state<'s>(ctx: &'s mut FieldResolutionContext<'_>) -> &'s mut StaticState {
        ctx.heap_mut().static_state.as_mut().unwrap()
    }

    #[test]
    fn arithmetic_and_all_reservation_helpers_fail_without_losing_existing_contents() {
        for resource in [StaticResource::Classes, StaticResource::Slots] {
            assert_eq!(
                add(usize::MAX, 1, resource),
                Err(StaticAccessError::AccountingOverflow { resource })
            );
            assert_eq!(add(usize::MAX, 0, resource), Ok(usize::MAX));
        }
        assert_eq!(
            payload_bytes(usize::MAX),
            Err(StaticAccessError::AccountingOverflow {
                resource: StaticResource::PayloadBytes
            })
        );
        assert_eq!(limit(usize::MAX, usize::MAX, StaticResource::Slots), Ok(()));
        let mut f = fixture();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let p = ctx
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        let before = snapshot(ctx.heap());
        assert!(matches!(
            reserve_destinations(state(&mut ctx), 1, usize::MAX),
            Err(StaticAccessError::AllocationFailure { .. })
        ));
        assert_eq!(snapshot(ctx.heap()), before);
        ctx.check_static_profile(&p).unwrap();
        let mut values = vec![Value::Int(41)];
        let mut map = HashMap::from([(f.fields[0], 0_usize)]);
        let mut set = HashSet::from([f.owner]);
        assert!(reserve(&mut values, usize::MAX).is_err());
        assert!(reserve_map(&mut map, usize::MAX).is_err());
        assert!(reserve_set(&mut set, usize::MAX).is_err());
        assert_eq!(values, [Value::Int(41)]);
        assert_eq!(map.get(&f.fields[0]), Some(&0));
        assert_eq!(set, HashSet::from([f.owner]));
    }

    #[test]
    fn forged_profiles_and_missing_or_corrupt_containers_are_not_valid_snapshots() {
        let mut f = fixture();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let p = ctx
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        let before = ctx.lookup_steps();
        for forged in [
            StaticStateProfile {
                registry: p.registry + 1,
                ..p
            },
            StaticStateProfile {
                heap: p.heap + 1,
                ..p
            },
            StaticStateProfile {
                classes: p.classes + 1,
                ..p
            },
            StaticStateProfile {
                slots: p.slots + 1,
                ..p
            },
            StaticStateProfile {
                bytes: p.bytes + 1,
                ..p
            },
        ] {
            assert_eq!(
                ctx.check_static_profile(&forged),
                Err(StaticAccessError::ProfileMismatch)
            );
        }
        let retained = ctx.heap_mut().static_state.take();
        assert_eq!(
            ctx.check_static_profile(&p),
            Err(StaticAccessError::ProfileMismatch)
        );
        ctx.heap_mut().static_state = retained;
        state(&mut ctx).bytes += 1;
        assert!(matches!(
            ctx.allocate_static_state(StaticStateOptions::default()),
            Err(StaticAccessError::InvalidState(_))
        ));
        state(&mut ctx).bytes -= 1;
        state(&mut ctx).heap += 1;
        assert!(matches!(
            ctx.allocate_static_state(StaticStateOptions::default()),
            Err(StaticAccessError::InvalidState(_))
        ));
        state(&mut ctx).heap -= 1;
        assert_eq!(ctx.lookup_steps(), before);
    }

    #[test]
    fn private_commit_revalidates_kind_finality_owner_position_and_source_before_mutation() {
        let mut f = fixture();
        let value = f.heap.allocate_instance(f.owner).unwrap();
        let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
        let foreign_ref = foreign
            .allocate_primitive_array(crate::PrimitiveArrayKind::Int, 0)
            .unwrap();
        let (id, index, generation) = value.heap_ref().unwrap().components();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let p = ctx
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        ctx.commit_static(&p, f.fields[2], Value::Reference(value))
            .unwrap();
        let before = snapshot(ctx.heap());
        for (field, value) in [
            (f.fields[0], Value::Reference(foreign_ref)),
            (f.fields[3], Value::Reference(foreign_ref)),
            (f.fields[4], Value::Int(4)),
            (
                FieldId {
                    index: usize::MAX,
                    ..f.fields[0]
                },
                Value::Int(1),
            ),
            (
                FieldId {
                    owner: ClassId {
                        registry: f.owner.registry + 1,
                        ..f.owner
                    },
                    index: 0,
                },
                Value::Int(1),
            ),
        ] {
            assert!(ctx.commit_static(&p, field, value).is_err());
            assert_eq!(snapshot(ctx.heap()), before);
        }
        for source in [
            foreign_ref,
            Reference::issued(id, usize::MAX, generation),
            Reference::issued(id, index, generation + 1),
        ] {
            assert!(matches!(
                ctx.commit_static(&p, f.fields[2], Value::Reference(source)),
                Err(StaticAccessError::Heap(_))
            ));
            assert_eq!(snapshot(ctx.heap()), before);
        }
        state(&mut ctx).positions.insert(f.fields[2], usize::MAX);
        let corrupted = snapshot(ctx.heap());
        assert!(matches!(
            ctx.commit_static(&p, f.fields[2], Value::Reference(value)),
            Err(StaticAccessError::InvalidState(_))
        ));
        assert_eq!(snapshot(ctx.heap()), corrupted);
        state(&mut ctx).positions.insert(f.fields[2], 2);
        state(&mut ctx).slots[2].kind = FieldKind::Int;
        let corrupted = snapshot(ctx.heap());
        assert!(matches!(
            ctx.commit_static(&p, f.fields[2], Value::Reference(value)),
            Err(StaticAccessError::InvalidState(_))
        ));
        assert_eq!(snapshot(ctx.heap()), corrupted);
        state(&mut ctx).slots[2].kind = FieldKind::Reference;
        state(&mut ctx).slots[2].is_final = true;
        assert!(matches!(
            ctx.commit_static(&p, f.fields[2], Value::Reference(value)),
            Err(StaticAccessError::InvalidState(_))
        ));
        state(&mut ctx).slots[2].is_final = false;
        assert_eq!(snapshot(ctx.heap()), before);
    }

    #[test]
    fn read_and_root_validation_never_filter_bad_kinds_narrow_values_or_stale_handles() {
        let mut f = fixture();
        let value = f.heap.allocate_instance(f.owner).unwrap();
        let (id, index, generation) = value.heap_ref().unwrap().components();
        let mut resolver = SameLoaderResolver;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        let p = ctx
            .allocate_static_state(StaticStateOptions::default())
            .unwrap();
        for (slot, bad) in [
            (0, Value::Long(3)),
            (0, Value::Int(9)),
            (1, Value::Int(128)),
            (
                2,
                Value::Reference(Reference::issued(id + 1, index, generation)),
            ),
            (
                2,
                Value::Reference(Reference::issued(id, usize::MAX, generation)),
            ),
            (
                2,
                Value::Reference(Reference::issued(id, index, generation + 1)),
            ),
        ] {
            let saved = state(&mut ctx).slots[slot].value;
            state(&mut ctx).slots[slot].value = bad;
            let before = snapshot(ctx.heap());
            assert!(ctx.inspect_static(&p, f.fields[slot]).is_err());
            assert!(ctx.static_roots(&p).is_err());
            assert_eq!(snapshot(ctx.heap()), before);
            state(&mut ctx).slots[slot].value = saved;
        }
        state(&mut ctx).positions.remove(&f.fields[2]);
        assert_eq!(
            ctx.inspect_static(&p, f.fields[2]),
            Err(StaticAccessError::MissingLocation { field: f.fields[2] })
        );
        assert!(ctx.static_roots(&p).is_err());
    }

    #[test]
    fn extension_preserves_nonempty_prefix_on_late_quota_work_and_reservation_failure() {
        for late_work in [false, true] {
            let mut f = fixture();
            let value = f.heap.allocate_instance(f.owner).unwrap();
            let plan = build_hierarchy(
                &f.registry,
                f.extra,
                &mut SameLoaderResolver,
                HierarchyOptions::default(),
            )
            .unwrap();
            let mut resolver = SameLoaderResolver;
            let options = if late_work {
                StaticStateOptions::default()
            } else {
                StaticStateOptions {
                    max_slots: 4,
                    ..StaticStateOptions::default()
                }
            };
            let (p, before) = {
                let mut ctx = FieldResolutionContext::new_unnamed_modules(
                    &f.registry,
                    &mut f.heap,
                    &mut resolver,
                    FieldResolutionOptions::default(),
                )
                .unwrap();
                let p = ctx.allocate_static_state(options).unwrap();
                ctx.commit_static(&p, f.fields[2], Value::Reference(value))
                    .unwrap();
                (p, snapshot(ctx.heap()))
            };
            f.heap
                .admit_instance_layouts(&f.registry, &[&plan])
                .unwrap();
            let seed = FieldResolutionContext::new_unnamed_modules(
                &f.registry,
                &mut f.heap,
                &mut resolver,
                FieldResolutionOptions::default(),
            )
            .unwrap()
            .lookup_steps();
            let max_lookup_steps = if late_work {
                seed + p.classes + 1 + 1
            } else {
                usize::MAX
            };
            let mut ctx = FieldResolutionContext::new_unnamed_modules(
                &f.registry,
                &mut f.heap,
                &mut resolver,
                FieldResolutionOptions {
                    max_lookup_steps,
                    ..FieldResolutionOptions::default()
                },
            )
            .unwrap();
            let error = ctx.allocate_static_state(options).unwrap_err();
            if late_work {
                assert!(matches!(
                    error,
                    StaticAccessError::Context(FieldAccessError::ResourceLimit { .. })
                ));
            } else {
                assert_eq!(
                    error,
                    StaticAccessError::ResourceLimit {
                        resource: StaticResource::Slots,
                        limit: 4,
                        required: 5
                    }
                );
            }
            assert_eq!(snapshot(ctx.heap()), before);
            assert!(matches!(
                reserve_destinations(state(&mut ctx), 1, usize::MAX),
                Err(StaticAccessError::AllocationFailure { .. })
            ));
            assert_eq!(snapshot(ctx.heap()), before);
            drop(ctx);
            if late_work {
                let mut ctx = FieldResolutionContext::new_unnamed_modules(
                    &f.registry,
                    &mut f.heap,
                    &mut resolver,
                    FieldResolutionOptions::default(),
                )
                .unwrap();
                let next = ctx.allocate_static_state(options).unwrap();
                assert_eq!(next.slots, 5);
                assert_eq!(
                    ctx.inspect_static(&next, f.fields[2]),
                    Ok(Value::Reference(value))
                );
                assert_eq!(
                    ctx.static_roots(&next).unwrap().collect::<Vec<_>>(),
                    [value.heap_ref().unwrap()]
                );
            }
        }
    }

    #[test]
    fn duplicate_prefix_fields_plus_a_phantom_position_cannot_pass_extension_validation() {
        let mut f = fixture();
        let plan = build_hierarchy(
            &f.registry,
            f.extra,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        let mut resolver = SameLoaderResolver;
        {
            let mut ctx = FieldResolutionContext::new_unnamed_modules(
                &f.registry,
                &mut f.heap,
                &mut resolver,
                FieldResolutionOptions::default(),
            )
            .unwrap();
            ctx.allocate_static_state(StaticStateOptions::default())
                .unwrap();
            let state = state(&mut ctx);
            state.fields[1] = state.fields[0];
            state.positions.remove(&f.fields[1]);
            state.positions.insert(
                FieldId {
                    owner: f.extra,
                    index: 0,
                },
                1,
            );
        }
        f.heap
            .admit_instance_layouts(&f.registry, &[&plan])
            .unwrap();
        let before = snapshot(&f.heap);
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        assert_eq!(
            ctx.allocate_static_state(StaticStateOptions::default()),
            Err(StaticAccessError::InvalidState(
                "static prefix positions are not a bijection"
            ))
        );
        assert_eq!(snapshot(ctx.heap()), before);
    }

    #[test]
    fn only_direct_selector_java_errors_are_mapped_and_all_wrappers_keep_sources() {
        assert_eq!(
            selector_error(FieldAccessError::Java(FieldJavaError::NoSuchField)),
            StaticAccessError::Java(StaticJavaError::NoSuchField)
        );
        assert_eq!(
            selector_error(FieldAccessError::Java(FieldJavaError::IllegalAccess)),
            StaticAccessError::Java(StaticJavaError::IllegalAccess)
        );
        assert_eq!(
            selector_error(FieldAccessError::Java(
                FieldJavaError::IncompatibleClassChange
            )),
            StaticAccessError::Java(StaticJavaError::IncompatibleClassChange)
        );
        for error in [
            FieldAccessError::Java(FieldJavaError::NullPointer),
            FieldAccessError::ReferenceType(Box::new(TypeAccessError::Java(
                crate::TypeJavaError::IllegalAccess,
            ))),
        ] {
            let wrapped = selector_error(error.clone());
            assert_eq!(wrapped, StaticAccessError::Context(error));
            assert!(wrapped.source().is_some());
        }
        let error = StaticAccessError::from(TypeAccessError::Java(crate::TypeJavaError::ClassCast));
        assert!(matches!(error, StaticAccessError::Type(_)));
        assert!(error.source().is_some());
        let error = StaticAccessError::from(HeapError::ForeignHeap);
        assert!(error.source().is_some());
        assert!(error.to_string().contains("heap"));
    }

    #[test]
    fn initialization_owner_ranges_reject_corruption_and_extend_without_changing_old_or_zero_ranges()
     {
        let mut f = fixture();
        let extra = build_hierarchy(
            &f.registry,
            f.extra,
            &mut SameLoaderResolver,
            HierarchyOptions::default(),
        )
        .unwrap();
        let mut resolver = SameLoaderResolver;
        let before = {
            let mut ctx = FieldResolutionContext::new_unnamed_modules(
                &f.registry,
                &mut f.heap,
                &mut resolver,
                FieldResolutionOptions::default(),
            )
            .unwrap();
            ctx.allocate_static_state(StaticStateOptions::default())
                .unwrap();
            let original = state(&mut ctx).ranges[&f.owner];
            assert_eq!(
                ctx.initialization_own_fields(f.owner).unwrap(),
                &f.fields[..4]
            );
            for range in [
                (0, usize::MAX),
                (2, 1),
                (original.0, original.1 - 1),
                (original.0 + 1, original.1),
            ] {
                state(&mut ctx).ranges.insert(f.owner, range);
                assert!(ctx.initialization_own_fields(f.owner).is_err());
            }
            state(&mut ctx).ranges.insert(f.owner, original);
            state(&mut ctx).ranges.clone()
        };
        f.heap
            .admit_instance_layouts(&f.registry, &[&extra])
            .unwrap();
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &f.registry,
            &mut f.heap,
            &mut resolver,
            FieldResolutionOptions::default(),
        )
        .unwrap();
        ctx.allocate_static_state(StaticStateOptions::default())
            .unwrap();
        for (class, range) in before {
            assert_eq!(state(&mut ctx).ranges.get(&class), Some(&range));
        }
        assert_eq!(
            ctx.initialization_own_fields(f.owner).unwrap(),
            &f.fields[..4]
        );
        assert_eq!(
            ctx.initialization_own_fields(f.extra).unwrap(),
            [FieldId {
                owner: f.extra,
                index: 0
            }]
        );
    }
}

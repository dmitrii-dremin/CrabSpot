//! Checked initialization metadata and operation tokens. Completion belongs only
//! to the interpreter session; these services never certify lifecycle success.
use crate::{
    ClassId, ExceptionAccessError, ExceptionProfile, FieldAccessError, FieldId,
    FieldResolutionContext, MethodId, RegistryError, StaticAccessError, StaticJavaError,
    StaticStateProfile, ThrownException, Value,
    instance::{FieldKind, classify},
};
use crabspot_classfile::{AttributeBody, Constant};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InitializationFingerprint {
    registry: u64,
    heap: u64,
    statics: StaticStateProfile,
    exceptions: ExceptionProfile,
    error: ClassId,
}
#[derive(Debug)]
pub struct InitializationClaim {
    fingerprint: InitializationFingerprint,
}
impl InitializationClaim {
    pub(crate) fn fingerprint(&self) -> InitializationFingerprint {
        self.fingerprint
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationStaticKind {
    Read,
    Write,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationPlanResource {
    Dependencies,
    Constants,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitializationPlanLimits {
    pub max_dependencies: usize,
    pub max_constants: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct InitializationConstant {
    pub(crate) field: FieldId,
    pub(crate) location: usize,
    pub(crate) value: Value,
    pub(crate) string: bool,
    descriptor: u16,
    attribute: usize,
    constant: u16,
}
#[derive(Debug)]
pub struct InitializationPlan {
    fingerprint: InitializationFingerprint,
    class: ClassId,
    initializer: Option<MethodId>,
    dependencies: Vec<ClassId>,
    constants: Vec<InitializationConstant>,
    constant_count: usize,
    strings: Option<ClassId>,
    string_count: usize,
}
impl InitializationPlan {
    pub fn class(&self) -> ClassId {
        self.class
    }
    pub fn initializer(&self) -> Option<MethodId> {
        self.initializer
    }
    pub fn dependencies(&self) -> &[ClassId] {
        &self.dependencies
    }
    pub fn constant_count(&self) -> usize {
        self.constants.len()
    }
}
#[derive(Debug)]
pub struct ResolvedInitializationStatic {
    fingerprint: InitializationFingerprint,
    origin: MethodId,
    index: u16,
    field: FieldId,
    location: usize,
    kind: InitializationStaticKind,
    symbol: StaticSymbol,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StaticSymbol {
    origin: MethodId,
    field: FieldId,
    index: u16,
    class_index: u16,
    owner_name: u16,
    nat: u16,
    name: u16,
    descriptor: u16,
    origin_name: u16,
    origin_descriptor: u16,
    field_name: u16,
    field_descriptor: u16,
    field_flags: u16,
}
impl ResolvedInitializationStatic {
    pub fn field(&self) -> FieldId {
        self.field
    }
    pub fn kind(&self) -> InitializationStaticKind {
        self.kind
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InitializationAccessError {
    String(Box<crate::StringError>),
    Context(FieldAccessError),
    Static(Box<StaticAccessError>),
    Exception(Box<ExceptionAccessError>),
    AlreadyClaimed,
    ClaimMismatch,
    InvalidErrorAnchor {
        class: ClassId,
        reason: &'static str,
    },
    InvalidMetadata(&'static str),
    UnsupportedInitializerDescriptor {
        method: MethodId,
    },
    UnsupportedStringConstant {
        field: FieldId,
    },
    PlanLimit {
        resource: InitializationPlanResource,
        limit: usize,
        required: usize,
    },
    AccountingOverflow {
        resource: &'static str,
    },
    AllocationFailure {
        resource: &'static str,
    },
    Java(StaticJavaError),
}
impl fmt::Display for InitializationAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(error) => error.fmt(f),
            Self::Context(error) => write!(f, "initialization context failed: {error}"),
            Self::Static(error) => write!(f, "initialization static operation failed: {error}"),
            Self::Exception(error) => {
                write!(f, "initialization exception operation failed: {error}")
            }
            Self::AlreadyClaimed => f.write_str("heap already belongs to an initialization domain"),
            Self::ClaimMismatch => {
                f.write_str("initialization claim/token belongs to another heap snapshot")
            }
            Self::InvalidErrorAnchor { class, reason } => {
                write!(f, "invalid Error anchor {class:?}: {reason}")
            }
            Self::InvalidMetadata(reason) => write!(f, "invalid initialization metadata: {reason}"),
            Self::UnsupportedInitializerDescriptor { method } => write!(
                f,
                "initializer {method:?} requires unsupported arguments or result"
            ),
            Self::UnsupportedStringConstant { field } => write!(
                f,
                "String ConstantValue for {field:?} requires unsupported materialization"
            ),
            Self::PlanLimit {
                resource,
                limit,
                required,
            } => write!(
                f,
                "initialization plan {resource:?} limit {limit} exceeded (required {required})"
            ),
            Self::AccountingOverflow { resource } => {
                write!(f, "initialization accounting overflow for {resource}")
            }
            Self::AllocationFailure { resource } => {
                write!(f, "initialization allocation failed for {resource}")
            }
            Self::Java(error) => write!(f, "initialization static access failed: {error:?}"),
        }
    }
}
impl std::error::Error for InitializationAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::String(error) => Some(error.as_ref()),
            Self::Context(error) => Some(error),
            Self::Static(error) => Some(error.as_ref()),
            Self::Exception(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}
impl From<FieldAccessError> for InitializationAccessError {
    fn from(error: FieldAccessError) -> Self {
        Self::Context(error)
    }
}
impl From<RegistryError> for InitializationAccessError {
    fn from(error: RegistryError) -> Self {
        FieldAccessError::Registry(error).into()
    }
}
impl From<crate::InstanceError> for InitializationAccessError {
    fn from(error: crate::InstanceError) -> Self {
        FieldAccessError::Instance(error).into()
    }
}
impl From<StaticAccessError> for InitializationAccessError {
    fn from(error: StaticAccessError) -> Self {
        Self::Static(Box::new(error))
    }
}
impl From<ExceptionAccessError> for InitializationAccessError {
    fn from(error: ExceptionAccessError) -> Self {
        Self::Exception(Box::new(error))
    }
}
fn operation_error(error: StaticAccessError) -> InitializationAccessError {
    match error {
        StaticAccessError::Java(error) => InitializationAccessError::Java(error),
        error => error.into(),
    }
}
fn selection_error(error: FieldAccessError) -> InitializationAccessError {
    match error {
        FieldAccessError::Java(crate::FieldJavaError::LinkageError) => {
            InitializationAccessError::Java(StaticJavaError::LinkageError)
        }
        FieldAccessError::Java(crate::FieldJavaError::NoSuchField) => {
            InitializationAccessError::Java(StaticJavaError::NoSuchField)
        }
        FieldAccessError::Java(crate::FieldJavaError::IllegalAccess) => {
            InitializationAccessError::Java(StaticJavaError::IllegalAccess)
        }
        FieldAccessError::Java(crate::FieldJavaError::IncompatibleClassChange) => {
            InitializationAccessError::Java(StaticJavaError::IncompatibleClassChange)
        }
        error => error.into(),
    }
}
fn invalid(reason: &'static str) -> InitializationAccessError {
    InitializationAccessError::InvalidMetadata(reason)
}
fn reserve<T>(values: &mut Vec<T>, amount: usize) -> Result<(), InitializationAccessError> {
    values
        .try_reserve(amount)
        .map_err(|_| InitializationAccessError::AllocationFailure {
            resource: "initialization plan/worklist",
        })
}
fn reserve_map<K: Eq + std::hash::Hash, V>(
    map: &mut HashMap<K, V>,
    amount: usize,
) -> Result<(), InitializationAccessError> {
    map.try_reserve(amount)
        .map_err(|_| InitializationAccessError::AllocationFailure {
            resource: "initialization descriptor metadata",
        })
}
fn reserve_seen(seen: &mut HashSet<ClassId>) -> Result<(), InitializationAccessError> {
    seen.try_reserve(1)
        .map_err(|_| InitializationAccessError::AllocationFailure {
            resource: "initialization interface identities",
        })
}
fn plan_limit(
    current: usize,
    maximum: usize,
    resource: InitializationPlanResource,
) -> Result<(), InitializationAccessError> {
    let required = current
        .checked_add(1)
        .ok_or(InitializationAccessError::AccountingOverflow {
            resource: "initialization plan entries",
        })?;
    if required > maximum {
        Err(InitializationAccessError::PlanLimit {
            resource,
            limit: maximum,
            required,
        })
    } else {
        Ok(())
    }
}
fn constant_value(
    field: FieldId,
    kind: FieldKind,
    constant: &Constant,
) -> Result<Value, InitializationAccessError> {
    let value = match (kind, constant) {
        (
            FieldKind::Boolean
            | FieldKind::Byte
            | FieldKind::Char
            | FieldKind::Short
            | FieldKind::Int,
            Constant::Integer(value),
        ) => Value::Int(*value),
        (FieldKind::Long, Constant::Long(value)) => Value::Long(*value),
        (FieldKind::Float, Constant::Float(value)) => Value::Float(*value),
        (FieldKind::Double, Constant::Double(value)) => Value::Double(*value),
        (FieldKind::Reference, Constant::String { .. }) => {
            return Err(InitializationAccessError::UnsupportedStringConstant { field });
        }
        _ => return Err(invalid("ConstantValue tag differs from its field type")),
    };
    kind.store(field, value)
        .map_err(InitializationAccessError::from)
}

impl FieldResolutionContext<'_> {
    pub fn claim_initialization(
        &mut self,
        statics: &StaticStateProfile,
        exceptions: &ExceptionProfile,
        error_class: ClassId,
    ) -> Result<InitializationClaim, InitializationAccessError> {
        self.check_static_profile(statics)?;
        self.check_exception_profile(exceptions)?;
        if self.heap().initialization_claim.is_some() {
            return Err(InitializationAccessError::AlreadyClaimed);
        }
        let universe = self
            .heap()
            .reference_type_universe()
            .ok_or_else(|| invalid("checked exception profile has no universe"))?;
        let definition = self.registry().class(error_class)?;
        let loader = self.registry().defining_loader(error_class)?;
        self.heap().instance_graph_node(error_class)?;
        self.step()?;
        let name = definition.name().map_err(RegistryError::Parse)?;
        for _ in name.units() {
            self.step()?;
        }
        let bad = |reason| InitializationAccessError::InvalidErrorAnchor {
            class: error_class,
            reason,
        };
        if !name.equals_ascii("java/lang/Error") {
            return Err(bad("expected exact java/lang/Error name"));
        }
        if loader != universe.bootstrap_loader {
            return Err(bad("Error must use the designated bootstrap loader"));
        }
        if definition.access_flags & (1 | 0x200 | 0x400) != 1 {
            return Err(bad("Error must be a public ordinary nonabstract class"));
        }
        self.step()?;
        if self.heap().instance_graph_node(error_class)?.superclass != Some(exceptions.throwable())
        {
            return Err(bad("Error must directly extend the designated Throwable"));
        }
        let fingerprint = InitializationFingerprint {
            registry: self.registry().identity(),
            heap: self.heap().identity,
            statics: *statics,
            exceptions: *exceptions,
            error: error_class,
        };
        self.heap_mut().initialization_claim = Some(fingerprint);
        Ok(InitializationClaim { fingerprint })
    }
    fn validate_initialization_claim(
        &self,
        claim: &InitializationClaim,
    ) -> Result<(), InitializationAccessError> {
        self.unchanged()?;
        let f = claim.fingerprint;
        if f.registry != self.registry().identity()
            || f.heap != self.heap().identity
            || self.heap().initialization_claim != Some(f)
        {
            return Err(InitializationAccessError::ClaimMismatch);
        }
        // Read-only static inspection validates its fingerprint without work.
        self.static_fields(&f.statics)?;
        // The exception snapshot is immutable and already part of the permanent
        // claim. Existing context validation checks its graph/universe lifetime.
        self.registry().class(f.error)?;
        self.heap().instance_graph_node(f.error)?;
        Ok(())
    }
    pub fn check_initialization_claim(
        &mut self,
        claim: &InitializationClaim,
    ) -> Result<(), InitializationAccessError> {
        self.validate_initialization_claim(claim)?;
        self.check_exception_profile(&claim.fingerprint.exceptions)?;
        Ok(())
    }
    pub fn check_initialization_class(
        &self,
        claim: &InitializationClaim,
        class: ClassId,
    ) -> Result<(), InitializationAccessError> {
        self.validate_initialization_claim(claim)?;
        self.registry().class(class)?;
        self.heap().instance_graph_node(class)?;
        Ok(())
    }
    pub fn plan_initialization(
        &mut self,
        claim: &InitializationClaim,
        class: ClassId,
        limits: InitializationPlanLimits,
    ) -> Result<InitializationPlan, InitializationAccessError> {
        self.plan_initialization_common(claim, class, limits, None)
    }
    pub fn plan_initialization_with_strings(
        &mut self,
        claim: &InitializationClaim,
        class: ClassId,
        limits: InitializationPlanLimits,
    ) -> Result<InitializationPlan, InitializationAccessError> {
        self.check_string_profile()
            .map_err(InitializationAccessError::String)?;
        let role = self
            .heap
            .strings
            .as_ref()
            .ok_or_else(|| invalid("String planner authority is absent"))?
            .class;
        self.plan_initialization_common(claim, class, limits, Some(role))
    }
    fn plan_initialization_common(
        &mut self,
        claim: &InitializationClaim,
        class: ClassId,
        limits: InitializationPlanLimits,
        strings: Option<ClassId>,
    ) -> Result<InitializationPlan, InitializationAccessError> {
        self.check_initialization_claim(claim)?;
        let definition = self.registry().class(class)?;
        self.heap().instance_graph_node(class)?;
        self.step()?;
        let mut constants = Vec::new();
        let mut string_count = 0usize;
        let mut kinds = HashMap::new();
        for (field_index, member) in definition.fields.iter().enumerate() {
            self.step()?;
            for (attribute, body) in member.attributes.iter().enumerate() {
                self.step()?;
                if member.access_flags & 8 == 0 {
                    continue;
                }
                let AttributeBody::ConstantValue(constant) = body.body else {
                    continue;
                };
                plan_limit(
                    constants.len(),
                    limits.max_constants,
                    InitializationPlanResource::Constants,
                )?;
                let field = FieldId {
                    owner: class,
                    index: field_index,
                };
                let key = (class, member.descriptor_index);
                let kind = if let Some(kind) = kinds.get(&key) {
                    *kind
                } else {
                    let units = definition
                        .constant_pool
                        .utf8(member.descriptor_index)
                        .map_err(RegistryError::Parse)?
                        .units();
                    self.charge_bulk_work(units.len(), 0)?;
                    let kind = classify(field, units)?;
                    reserve_map(&mut kinds, 1)?;
                    kinds.insert(key, kind);
                    kind
                };
                let (location, retained) = self.initialization_location(field)?;
                if retained != kind {
                    return Err(invalid("retained constant field kind differs"));
                }
                let source = definition
                    .constant_pool
                    .get(constant)
                    .map_err(RegistryError::Parse)?;
                let string = matches!(source, Constant::String { .. });
                let value = if string && strings.is_some() {
                    if kind != FieldKind::Reference
                        || !definition
                            .constant_pool
                            .utf8(member.descriptor_index)
                            .map_err(RegistryError::Parse)?
                            .equals_ascii("Ljava/lang/String;")
                    {
                        return Err(invalid("String constant descriptor differs"));
                    }
                    if string_count == 0 {
                        let descriptor = definition
                            .constant_pool
                            .utf8(member.descriptor_index)
                            .map_err(RegistryError::Parse)?
                            .units();
                        let target = self
                            .resolve_reference_span(class, descriptor)
                            .map_err(|e| InitializationAccessError::String(e.into()))?;
                        if Some(target) != strings.map(crate::ReferenceType::Class) {
                            return Err(InitializationAccessError::String(Box::new(
                                crate::StringError::InvalidBinding(
                                    "constant owner does not initiate the designated String",
                                ),
                            )));
                        }
                    }
                    Value::Reference(crate::Reference::null())
                } else {
                    constant_value(field, kind, source)?
                };
                if string {
                    string_count = string_count
                        .checked_add(1)
                        .ok_or_else(|| invalid("String constant count overflow"))?;
                }
                reserve(&mut constants, 1)?;
                constants.push(InitializationConstant {
                    field,
                    location,
                    value,
                    string,
                    descriptor: member.descriptor_index,
                    attribute,
                    constant,
                });
            }
        }
        let mut initializer = None;
        for (index, member) in definition.methods.iter().enumerate() {
            self.step()?;
            if definition
                .constant_pool
                .utf8(member.name_index)
                .map_err(RegistryError::Parse)?
                .equals_ascii("<clinit>")
            {
                let method = MethodId {
                    owner: class,
                    index,
                };
                if initializer.replace(method).is_some() {
                    return Err(invalid("multiple class initializers"));
                }
                if !definition
                    .constant_pool
                    .utf8(member.descriptor_index)
                    .map_err(RegistryError::Parse)?
                    .equals_ascii("()V")
                {
                    return Err(
                        InitializationAccessError::UnsupportedInitializerDescriptor { method },
                    );
                }
            }
        }
        let mut dependencies = Vec::new();
        if definition.access_flags & 0x200 == 0 {
            if let Some(parent) = self.heap().instance_graph_node(class)?.superclass {
                self.step()?;
                self.registry().class(parent)?;
                if self.heap().instance_graph_node(parent)?.flags & 0x200 != 0 {
                    return Err(invalid("class superclass is an interface"));
                }
                plan_limit(
                    dependencies.len(),
                    limits.max_dependencies,
                    InitializationPlanResource::Dependencies,
                )?;
                reserve(&mut dependencies, 1)?;
                dependencies.push(parent);
            }
            self.initialization_interfaces(class, &mut dependencies, limits.max_dependencies)?;
        }
        let constant_count = constants.len();
        Ok(InitializationPlan {
            fingerprint: claim.fingerprint,
            class,
            initializer,
            dependencies,
            constants,
            constant_count,
            strings,
            string_count,
        })
    }
    fn initialization_interfaces(
        &mut self,
        class: ClassId,
        dependencies: &mut Vec<ClassId>,
        maximum: usize,
    ) -> Result<(), InitializationAccessError> {
        let mut work = Vec::new();
        let mut visited = HashSet::new();
        let root_count = self.heap().instance_graph_node(class)?.interfaces.len();
        for index in (0..root_count).rev() {
            self.step()?;
            let interface = *self
                .heap()
                .instance_graph_node(class)?
                .interfaces
                .get(index)
                .ok_or_else(|| invalid("interface edge disappeared"))?;
            reserve(&mut work, 1)?;
            work.push((interface, None));
        }
        while let Some((interface, exit)) = work.pop() {
            if let Some(qualifies) = exit {
                if qualifies {
                    plan_limit(
                        dependencies.len(),
                        maximum,
                        InitializationPlanResource::Dependencies,
                    )?;
                    reserve(dependencies, 1)?;
                    dependencies.push(interface);
                }
                continue;
            }
            if visited.contains(&interface) {
                continue;
            }
            self.step()?;
            let definition = self.registry().class(interface)?;
            if self.heap().instance_graph_node(interface)?.flags & 0x200 == 0 {
                return Err(invalid("interface dependency is an ordinary class"));
            }
            reserve_seen(&mut visited)?;
            visited.insert(interface);
            let mut qualifies = false;
            for member in &definition.methods {
                self.step()?;
                if member.access_flags & (0x400 | 8) == 0
                    && !definition
                        .constant_pool
                        .utf8(member.name_index)
                        .map_err(RegistryError::Parse)?
                        .equals_ascii("<clinit>")
                {
                    qualifies = true;
                    break;
                }
            }
            reserve(&mut work, 1)?;
            work.push((interface, Some(qualifies)));
            let count = self.heap().instance_graph_node(interface)?.interfaces.len();
            for index in (0..count).rev() {
                self.step()?;
                let child = *self
                    .heap()
                    .instance_graph_node(interface)?
                    .interfaces
                    .get(index)
                    .ok_or_else(|| invalid("interface edge disappeared"))?;
                reserve(&mut work, 1)?;
                work.push((child, None));
            }
        }
        Ok(())
    }
    pub fn install_initialization_constants(
        &mut self,
        claim: &InitializationClaim,
        plan: &InitializationPlan,
    ) -> Result<(), InitializationAccessError> {
        self.check_initialization_claim(claim)?;
        if plan.fingerprint != claim.fingerprint {
            return Err(InitializationAccessError::ClaimMismatch);
        }
        self.registry().class(plan.class)?;
        self.heap().instance_graph_node(plan.class)?;
        if plan.constants.len() != plan.constant_count {
            return Err(invalid("constant plan count differs"));
        }
        if plan.constants.is_empty() {
            return Ok(());
        }
        let count = self.initialization_own_fields(plan.class)?.len();
        self.charge_bulk_work(count, 0)?;
        let mut next = 0usize;
        let mut strings_seen = 0usize;
        for field in self.initialization_own_fields(plan.class)? {
            if field.owner() != plan.class {
                return Err(invalid("constant destination owner range differs"));
            }
            let (location, kind) = self.initialization_location(*field)?;
            if let Some(entry) = plan.constants.get(next) {
                if entry.field == *field {
                    let definition = self.registry().class(plan.class)?;
                    let member = self.registry().field(*field)?;
                    if location != entry.location
                        || member.descriptor_index != entry.descriptor
                        || !matches!(member.attributes.get(entry.attribute).map(|a|&a.body),Some(AttributeBody::ConstantValue(index)) if *index==entry.constant)
                    {
                        return Err(invalid("constant plan differs from immutable declaration"));
                    }
                    let source = definition
                        .constant_pool
                        .get(entry.constant)
                        .map_err(RegistryError::Parse)?;
                    if entry.string {
                        strings_seen += 1;
                        if plan.strings.is_none()
                            || kind != FieldKind::Reference
                            || !matches!(source, Constant::String { .. })
                            || !definition
                                .constant_pool
                                .utf8(entry.descriptor)
                                .map_err(RegistryError::Parse)?
                                .equals_ascii("Ljava/lang/String;")
                            || entry.value != Value::Reference(crate::Reference::null())
                        {
                            return Err(invalid(
                                "String constant plan differs from immutable source",
                            ));
                        }
                    } else {
                        let expected = constant_value(*field, kind, source)?;
                        if expected != entry.value {
                            return Err(invalid(
                                "constant plan value differs from immutable constant",
                            ));
                        }
                    }
                    next += 1;
                }
            }
        }
        if next != plan.constants.len() {
            return Err(invalid(
                "constant plan destinations are unordered or absent",
            ));
        }
        if strings_seen != plan.string_count {
            return Err(invalid("String constant count differs"));
        }
        if plan.string_count != 0 {
            self.check_string_profile()
                .map_err(InitializationAccessError::String)?;
            let role = self
                .heap
                .strings
                .as_ref()
                .ok_or_else(|| invalid("String constant authority absent"))?
                .class;
            if plan.strings != Some(role) {
                return Err(invalid("String plan role differs"));
            }
            // The new String-capable staging pass copies scalar rows, not text.
            // Fund its actual row visits; old all-numeric paths remain unchanged.
            self.charge_bulk_work(plan.constants.len(), 0)?;
            let mut staged = Vec::new();
            reserve(&mut staged, plan.constants.len())?;
            for entry in &plan.constants {
                let mut value = entry.clone();
                if entry.string {
                    value.value = Value::Reference(
                        self.string_literal(plan.class, entry.constant)
                            .map_err(InitializationAccessError::String)?,
                    );
                }
                staged.push(value);
            }
            self.publish_initialization_constants(plan.class, &staged)?;
        } else {
            self.publish_initialization_constants(plan.class, &plan.constants)?;
        }
        Ok(())
    }
    pub fn resolve_initialization_static(
        &mut self,
        claim: &InitializationClaim,
        origin: MethodId,
        index: u16,
        kind: InitializationStaticKind,
    ) -> Result<ResolvedInitializationStatic, InitializationAccessError> {
        self.check_initialization_claim(claim)?;
        self.registry().method(origin)?;
        self.heap().instance_graph_node(origin.owner())?;
        let field = self
            .select_field_id(origin.owner(), index)
            .map_err(selection_error)?;
        let flags = self.registry().field(field)?.access_flags;
        if flags & 8 == 0 {
            return Err(InitializationAccessError::Java(
                StaticJavaError::IncompatibleClassChange,
            ));
        }
        if kind == InitializationStaticKind::Write
            && flags & 16 != 0
            && !self.declaring_initializer(origin, field.owner())?
        {
            return Err(InitializationAccessError::Java(
                StaticJavaError::IllegalAccess,
            ));
        }
        let (location, _) = self.initialization_location(field)?;
        let symbol = self.initialization_static_symbol(origin, index, field)?;
        Ok(ResolvedInitializationStatic {
            fingerprint: claim.fingerprint,
            origin,
            index,
            field,
            location,
            kind,
            symbol,
        })
    }
    fn initialization_static_symbol(
        &self,
        origin: MethodId,
        index: u16,
        field: FieldId,
    ) -> Result<StaticSymbol, InitializationAccessError> {
        let definition = self.registry().class(origin.owner())?;
        let method = self.registry().method(origin)?;
        self.heap().instance_graph_node(origin.owner())?;
        let pool = &definition.constant_pool;
        let Constant::Fieldref {
            class_index,
            name_and_type_index,
        } = pool.get(index).map_err(RegistryError::Parse)?
        else {
            return Err(invalid("static continuation constant is not a Fieldref"));
        };
        let Constant::Class {
            name_index: owner_name,
        } = pool.get(*class_index).map_err(RegistryError::Parse)?
        else {
            return Err(invalid("static continuation owner is not a Class"));
        };
        pool.utf8(*owner_name).map_err(RegistryError::Parse)?;
        let Constant::NameAndType {
            name_index,
            descriptor_index,
        } = pool
            .get(*name_and_type_index)
            .map_err(RegistryError::Parse)?
        else {
            return Err(invalid("static continuation NameAndType is invalid"));
        };
        pool.utf8(*name_index).map_err(RegistryError::Parse)?;
        pool.utf8(*descriptor_index).map_err(RegistryError::Parse)?;
        let member = self.registry().field(field)?;
        Ok(StaticSymbol {
            origin,
            field,
            index,
            class_index: *class_index,
            owner_name: *owner_name,
            nat: *name_and_type_index,
            name: *name_index,
            descriptor: *descriptor_index,
            origin_name: method.name_index,
            origin_descriptor: method.descriptor_index,
            field_name: member.name_index,
            field_descriptor: member.descriptor_index,
            field_flags: member.access_flags,
        })
    }
    fn validate_initialization_access(
        &mut self,
        claim: &InitializationClaim,
        access: &ResolvedInitializationStatic,
        kind: InitializationStaticKind,
    ) -> Result<(), InitializationAccessError> {
        self.check_initialization_claim(claim)?;
        if access.fingerprint != claim.fingerprint {
            return Err(InitializationAccessError::ClaimMismatch);
        }
        if access.kind != kind {
            return Err(invalid(
                "static continuation used for another operation kind",
            ));
        }
        if self.initialization_static_symbol(access.origin, access.index, access.field)?
            != access.symbol
        {
            return Err(invalid("static continuation scalar metadata changed"));
        }
        let member = self.registry().field(access.field)?;
        let (location, _) = self.initialization_location(access.field)?;
        if location != access.location {
            return Err(invalid("static continuation location changed"));
        }
        if kind == InitializationStaticKind::Write
            && member.access_flags & 16 != 0
            && !self.declaring_initializer(access.origin, access.field.owner())?
        {
            return Err(InitializationAccessError::Java(
                StaticJavaError::IllegalAccess,
            ));
        }
        Ok(())
    }
    pub fn read_initialization_static(
        &mut self,
        claim: &InitializationClaim,
        access: &ResolvedInitializationStatic,
    ) -> Result<Value, InitializationAccessError> {
        self.validate_initialization_access(claim, access, InitializationStaticKind::Read)?;
        self.inspect_static(&claim.fingerprint.statics, access.field)
            .map_err(operation_error)
    }
    pub fn write_initialization_static(
        &mut self,
        claim: &InitializationClaim,
        access: &ResolvedInitializationStatic,
        value: Value,
    ) -> Result<(), InitializationAccessError> {
        self.validate_initialization_access(claim, access, InitializationStaticKind::Write)?;
        self.write_initialization_value(
            &claim.fingerprint.statics,
            access.field,
            value,
            access.origin,
        )
        .map_err(operation_error)
    }
    pub fn is_initialization_error(
        &mut self,
        claim: &InitializationClaim,
        thrown: &ThrownException,
    ) -> Result<bool, InitializationAccessError> {
        self.check_initialization_claim(claim)?;
        self.exception_handler_matches(
            &claim.fingerprint.exceptions,
            claim.fingerprint.error,
            0,
            thrown,
        )?;
        Ok(self.subclass(thrown.class(), claim.fingerprint.error)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod rt {
        pub use crate::*;
    }
    #[allow(dead_code)]
    mod fixtures {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/initialization.rs"
        ));
    }
    use fixtures::*;
    use rt::*;
    use std::error::Error;

    fn owner(w: &mut World) -> Def {
        define(
            &mut w.registry,
            w.u.bootstrap_loader,
            "C",
            Some("java/lang/Object"),
            &[],
            0x21,
            &[
                Field::new("a", "I", 9, Some(Cv::Int(41))),
                Field::new("b", "I", 25, Some(Cv::Int(79))),
                Field::new("r", "Ljava/lang/Object;", 25, None),
            ],
            &[
                Method::new("<clinit>", "()V", 8),
                Method::new("helper", "()V", 9),
            ],
            &[
                ("C", "a", "I"),
                ("C", "b", "I"),
                ("C", "r", "Ljava/lang/Object;"),
            ],
            65,
        )
    }
    fn values(ctx: &FieldResolutionContext<'_>, s: &StaticStateProfile) -> Vec<(FieldId, Value)> {
        ctx.static_fields(s)
            .unwrap()
            .iter()
            .map(|field| (*field, ctx.inspect_static(s, *field).unwrap()))
            .collect()
    }

    #[test]
    fn claim_plan_and_static_tokens_keep_checked_heap_fingerprints_and_handle_bounds() {
        let mut w = world();
        let own = owner(&mut w);
        let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        let before = ctx.lookup_steps();
        for fingerprint in [
            InitializationFingerprint {
                heap: claim.fingerprint.heap + 1,
                ..claim.fingerprint
            },
            InitializationFingerprint {
                registry: claim.fingerprint.registry + 1,
                ..claim.fingerprint
            },
            InitializationFingerprint {
                error: w.u.object,
                ..claim.fingerprint
            },
        ] {
            let forged = InitializationClaim { fingerprint };
            assert_eq!(
                ctx.check_initialization_claim(&forged),
                Err(InitializationAccessError::ClaimMismatch)
            );
            assert_eq!(
                ctx.check_initialization_class(&forged, w.error),
                Err(InitializationAccessError::ClaimMismatch)
            );
        }
        let forged_class = ClassId {
            index: usize::MAX,
            ..own.class
        };
        assert!(matches!(
            ctx.check_initialization_class(&claim, forged_class),
            Err(InitializationAccessError::Context(
                FieldAccessError::Registry(_)
            ))
        ));
        assert_eq!(ctx.lookup_steps(), before);
        let mut plan = ctx
            .plan_initialization(&claim, own.class, limits())
            .unwrap();
        plan.fingerprint.heap += 1;
        assert_eq!(
            ctx.install_initialization_constants(&claim, &plan),
            Err(InitializationAccessError::ClaimMismatch)
        );
        let mut access = ctx
            .resolve_initialization_static(
                &claim,
                own.methods[0],
                own.refs[0],
                InitializationStaticKind::Write,
            )
            .unwrap();
        access.fingerprint.heap += 1;
        assert_eq!(
            ctx.write_initialization_static(&claim, &access, Value::Int(5)),
            Err(InitializationAccessError::ClaimMismatch)
        );
    }

    #[test]
    fn late_corrupt_constant_metadata_values_order_and_locations_never_partially_install() {
        let mut w = world();
        let own = owner(&mut w);
        let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
        let value = heap.allocate_instance(own.class).unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        let reference = ctx
            .resolve_initialization_static(
                &claim,
                own.methods[0],
                own.refs[2],
                InitializationStaticKind::Write,
            )
            .unwrap();
        ctx.write_initialization_static(&claim, &reference, Value::Reference(value))
            .unwrap();
        let before = values(&ctx, &s);
        let roots = ctx.static_roots(&s).unwrap().collect::<Vec<_>>();
        for corruption in 0..8 {
            let mut plan = ctx
                .plan_initialization(&claim, own.class, limits())
                .unwrap();
            match corruption {
                0 => plan.constants[1].value = Value::Int(123),
                1 => plan.constants[1].location = usize::MAX,
                2 => plan.constants[1].descriptor = u16::MAX,
                3 => plan.constants[1].attribute = usize::MAX,
                4 => plan.constants[1].constant = u16::MAX,
                5 => {
                    plan.constants.swap(0, 1);
                }
                6 => {
                    plan.constants.pop();
                }
                _ => plan.constants[1].field = plan.constants[0].field,
            };
            assert!(ctx.install_initialization_constants(&claim, &plan).is_err());
            assert_eq!(values(&ctx, &s), before);
            assert_eq!(ctx.static_roots(&s).unwrap().collect::<Vec<_>>(), roots);
            assert_eq!(
                (
                    ctx.heap().allocated_arrays(),
                    ctx.heap().allocated_instances()
                ),
                (0, 1)
            );
        }
        let plan = ctx
            .plan_initialization(&claim, own.class, limits())
            .unwrap();
        ctx.install_initialization_constants(&claim, &plan).unwrap();
        assert_eq!(
            ctx.inspect_static(&s, field(&w, own.class, "b", "I")),
            Ok(Value::Int(79))
        );
        assert_eq!(ctx.static_roots(&s).unwrap().collect::<Vec<_>>(), roots);
    }

    #[test]
    fn static_continuation_forgery_and_bad_source_handles_fail_before_final_mutation() {
        let mut w = world();
        let own = owner(&mut w);
        let mut heap = heap(&w, &[w.error, own.class], &mut SameLoaderResolver);
        let value = heap.allocate_instance(own.class).unwrap();
        let (id, index, generation) = value.heap_ref().unwrap().components();
        let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
        let foreign_ref = foreign
            .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
            .unwrap();
        let mut resolver = SameLoaderResolver;
        let mut ctx = context(&w, &mut heap, &mut resolver);
        let (s, e) = profiles(&mut ctx, w.throwable);
        let claim = ctx.claim_initialization(&s, &e, w.error).unwrap();
        let before = values(&ctx, &s);
        for corruption in 0..7 {
            let mut access = ctx
                .resolve_initialization_static(
                    &claim,
                    own.methods[0],
                    own.refs[1],
                    InitializationStaticKind::Write,
                )
                .unwrap();
            match corruption {
                0 => access.location = usize::MAX,
                1 => access.index = own.refs[0],
                2 => access.field = field(&w, own.class, "a", "I"),
                3 => {
                    access.origin = MethodId {
                        index: usize::MAX,
                        ..own.methods[0]
                    }
                }
                4 => access.symbol.field_flags ^= 16,
                5 => access.kind = InitializationStaticKind::Read,
                _ => {
                    access.origin = own.methods[1];
                    access.symbol.origin = own.methods[1];
                    access.symbol.origin_name =
                        w.registry.method(own.methods[1]).unwrap().name_index;
                }
            }
            assert!(
                ctx.write_initialization_static(&claim, &access, Value::Int(57))
                    .is_err()
            );
            assert_eq!(values(&ctx, &s), before);
        }
        let access = ctx
            .resolve_initialization_static(
                &claim,
                own.methods[0],
                own.refs[2],
                InitializationStaticKind::Write,
            )
            .unwrap();
        for source in [
            foreign_ref,
            Reference::issued(id, usize::MAX, generation),
            Reference::issued(id, index, generation + 1),
        ] {
            assert!(
                matches!(ctx.write_initialization_static(&claim,&access,Value::Reference(source)),Err(InitializationAccessError::Static(error)) if matches!(*error,StaticAccessError::Type(_)))
            );
            assert_eq!(values(&ctx, &s), before);
        }
        ctx.write_initialization_static(&claim, &access, Value::Reference(value))
            .unwrap();
        assert_eq!(
            ctx.static_roots(&s).unwrap().collect::<Vec<_>>(),
            [value.heap_ref().unwrap()]
        );
    }

    #[test]
    fn arithmetic_reservation_and_nested_java_failures_remain_structured_host_errors() {
        assert!(matches!(
            plan_limit(
                usize::MAX,
                usize::MAX,
                InitializationPlanResource::Dependencies
            ),
            Err(InitializationAccessError::AccountingOverflow { .. })
        ));
        let mut vec = vec![41_u16];
        let mut map = HashMap::from([(1_u16, 9_u16)]);
        assert!(matches!(
            reserve(&mut vec, usize::MAX),
            Err(InitializationAccessError::AllocationFailure { .. })
        ));
        assert!(matches!(
            reserve_map(&mut map, usize::MAX),
            Err(InitializationAccessError::AllocationFailure { .. })
        ));
        assert_eq!(vec, [41]);
        assert_eq!(map.get(&1), Some(&9));
        let direct = operation_error(StaticAccessError::Java(StaticJavaError::IllegalAccess));
        assert_eq!(
            direct,
            InitializationAccessError::Java(StaticJavaError::IllegalAccess)
        );
        for nested in [
            StaticAccessError::Context(FieldAccessError::Java(
                crate::FieldJavaError::IllegalAccess,
            )),
            StaticAccessError::Type(Box::new(TypeAccessError::Java(
                crate::TypeJavaError::ClassCast,
            ))),
        ] {
            let wrapped = operation_error(nested.clone());
            assert_eq!(wrapped, InitializationAccessError::Static(Box::new(nested)));
            assert!(wrapped.source().is_some());
        }
        let wrapped = InitializationAccessError::from(ExceptionAccessError::Java(
            crate::VmExceptionKind::IllegalAccessError,
        ));
        assert!(matches!(wrapped, InitializationAccessError::Exception(_)));
        assert!(wrapped.source().is_some());
        assert!(wrapped.to_string().contains("exception"));
        let wrapped = selection_error(FieldAccessError::Java(crate::FieldJavaError::NullPointer));
        assert!(matches!(wrapped, InitializationAccessError::Context(_)));
        assert!(wrapped.source().is_some());
        for (kind, constant) in [
            (FieldKind::Int, Constant::Long(1)),
            (FieldKind::Double, Constant::Float(1)),
            (FieldKind::Reference, Constant::Integer(0)),
        ] {
            assert!(matches!(
                constant_value(
                    FieldId {
                        owner: ClassId {
                            registry: 1,
                            index: 0
                        },
                        index: 0
                    },
                    kind,
                    &constant
                ),
                Err(InitializationAccessError::InvalidMetadata(_))
            ));
        }
    }
}

#[cfg(test)]
mod string_plan_guards {
    use super::*;
    use crate::strings::tests::{claim, context, fixture};
    #[test]
    fn corrupt_late_string_plan_rows_reject_before_derivation_or_any_field_write() {
        for damage in 0..7 {
            let mut f = fixture(Default::default());
            let boot = f.g.boot;
            let class = f.constants;
            let fields = f.fields;
            let caller = f.caller;
            let mut c = context(&mut f);
            let (claim, statics) = claim(&mut c, boot);
            let mut plan = c
                .plan_initialization_with_strings(
                    &claim,
                    class,
                    InitializationPlanLimits {
                        max_dependencies: 100,
                        max_constants: 100,
                    },
                )
                .unwrap();
            match damage {
                0 => plan.constants[2].location = usize::MAX,
                1 => plan.constants[2].field = fields[0],
                2 => plan.constants[2].attribute = usize::MAX,
                3 => plan.constants[2].constant = 0,
                4 => plan.constants[2].value = Value::Int(17),
                5 => plan.string_count = 0,
                _ => plan.strings = Some(caller),
            }
            assert!(c.install_initialization_constants(&claim, &plan).is_err());
            assert_eq!(c.heap().string_usage(), crate::StringUsage::default());
            assert_eq!(
                c.inspect_static(&statics, fields[0]).unwrap(),
                Value::Int(0)
            );
            for field in &fields[1..] {
                assert_eq!(
                    c.inspect_static(&statics, *field).unwrap(),
                    Value::Reference(crate::Reference::null())
                );
            }
        }
    }
}

use crate::{
    ApplicationDirectories, ApplicationErrorKind as K, ApplicationOptions,
    ApplicationResource as R, ApplicationStage as S, ApplicationUsage, InputRole,
    StaticIntObservation,
};
use crate::{error::Result, meter::Meter, source};
use crabspot_classfile::{AttributeBody, Constant, parse_with_options};
use crabspot_runtime::*;
use crabspot_verifier::*;
use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};
const IMPORTS: [&str; 7] = [
    "java/lang/Object",
    "java/lang/String",
    "java/lang/Throwable",
    "java/lang/Error",
    "java/lang/Cloneable",
    "java/io/Serializable",
    "java/lang/Class",
];
pub(crate) struct BootstrapRoles {
    pub(crate) core: VerificationCore,
    pub(crate) error: ClassId,
}
pub(crate) struct PreparedImage {
    pub(crate) domain: LoadingDomain,
    pub(crate) routes: Routes,
    pub(crate) heap: ManagedHeap,
    pub(crate) roles: BootstrapRoles,
    pub(crate) named: ClassId,
    pub(crate) app_loader: LoaderId,
    pub(crate) options: ApplicationOptions,
    pub(crate) usage: ApplicationUsage,
    pub(crate) meter: Meter,
}
pub(crate) struct Routes {
    hash: std::collections::hash_map::RandomState,
    table: HashMap<(LoaderId, u64), Vec<ClassId>>,
}
impl Routes {
    fn new() -> Self {
        Self {
            hash: Default::default(),
            table: HashMap::new(),
        }
    }
    fn lookup(
        &self,
        r: &DefinitionRegistry,
        l: LoaderId,
        name: &[u16],
        m: &mut Meter,
        u: ApplicationUsage,
        s: S,
    ) -> Result<Option<ClassId>> {
        m.utf(name.len(), s, u)?;
        m.work(1, s, u)?;
        let key = (l, self.hash.hash_one(name));
        if let Some(bucket) = self.table.get(&key) {
            for id in bucket {
                m.work(3, s, u)?;
                let stored = r
                    .class(*id)
                    .map_err(|e| m.error(s, K::Registry(e), u))?
                    .name()
                    .map_err(|e| m.error(s, K::Parse(e), u))?;
                m.utf(name.len() + stored.units().len(), s, u)?;
                if stored.units() == name {
                    return Ok(Some(*id));
                }
            }
        }
        Ok(None)
    }
    fn add(
        &mut self,
        r: &DefinitionRegistry,
        l: LoaderId,
        id: ClassId,
        m: &mut Meter,
        u: ApplicationUsage,
        stage: S,
    ) -> Result<()> {
        m.work(2, stage, u)?;
        let name = r
            .class(id)
            .map_err(|e| m.error(stage, K::Registry(e), u))?
            .name()
            .map_err(|e| m.error(stage, K::Parse(e), u))?;
        m.utf(name.units().len(), stage, u)?;
        m.work(1, stage, u)?;
        let key = (l, self.hash.hash_one(name.units()));
        m.retain(1, stage, u)?;
        self.table.try_reserve(1).map_err(|_| m.alloc(stage, u))?;
        let bucket = match self.table.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut fresh = Vec::new();
                fresh.try_reserve(1).map_err(|_| m.alloc(stage, u))?;
                entry.insert(fresh)
            }
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
        };
        bucket.try_reserve(1).map_err(|_| m.alloc(stage, u))?;
        bucket.push(id);
        Ok(())
    }
}
fn binary(text: &str, m: &mut Meter, u: ApplicationUsage, stage: S) -> Result<Vec<u16>> {
    m.work(text.len() as u64, stage, u)?;
    m.utf(text.encode_utf16().count(), stage, u)?;
    if text
        .split('.')
        .any(|p| !source::component(p) || p.chars().any(|c| ";[".contains(c)))
    {
        return Err(m.error(stage, K::InvalidInput("invalid binary name"), u));
    }
    let mut v = Vec::new();
    v.try_reserve(text.len()).map_err(|_| m.alloc(stage, u))?;
    for c in text.encode_utf16() {
        v.push(if c == 46 { 47 } else { c })
    }
    Ok(v)
}
fn query_name(text: &str, m: &mut Meter, u: ApplicationUsage) -> Result<()> {
    m.work(text.len() as u64, S::Input, u)?;
    m.utf(text.encode_utf16().count(), S::Input, u)?;
    if text.is_empty() || text.chars().any(|c| ".;[/".contains(c)) {
        Err(m.error(S::Input, K::InvalidInput("invalid observation field"), u))
    } else {
        Ok(())
    }
}

fn module(
    bytes: &[u8],
    options: ApplicationOptions,
    m: &mut Meter,
    u: ApplicationUsage,
) -> Result<()> {
    let c = parse_with_options(bytes, options.parse)
        .map_err(|e| m.error(S::Definition, K::Parse(e), u))?;
    m.work(2, S::Definition, u)?;
    let name = c
        .name()
        .map_err(|e| m.error(S::Definition, K::Parse(e), u))?;
    m.utf(name.units().len() + 11, S::Definition, u)?;
    if c.access_flags != 0x8000 || !name.equals_ascii("module-info") {
        return Err(m.error(S::Definition, K::InvalidModule, u));
    }
    for a in &c.attributes {
        m.work(1, S::Definition, u)?;
        if let AttributeBody::Module(module) = &a.body {
            m.work(2, S::Definition, u)?;
            if let Ok(Constant::Module { name_index }) =
                c.constant_pool.get(module.module_name_index)
            {
                let n = c
                    .constant_pool
                    .utf8(*name_index)
                    .map_err(|e| m.error(S::Definition, K::Parse(e), u))?;
                m.utf(n.units().len() + 9, S::Definition, u)?;
                if n.equals_ascii("java.base") {
                    return Ok(());
                }
            }
        }
    }
    Err(m.error(S::Definition, K::InvalidModule, u))
}

struct Admission<'a> {
    domain: &'a mut LoadingDomain,
    routes: &'a mut Routes,
    ids: &'a mut Vec<ClassId>,
    options: ApplicationOptions,
    meter: &'a mut Meter,
    usage: &'a mut ApplicationUsage,
}
impl Admission<'_> {
    fn admit(
        &mut self,
        leaves: Vec<source::Leaf>,
        role: InputRole,
        loader: LoaderId,
        boot: LoaderId,
    ) -> Result<()> {
        let d = &mut *self.domain;
        let routes = &mut *self.routes;
        let ids = &mut *self.ids;
        let o = self.options;
        let m = &mut *self.meter;
        let u = &mut *self.usage;
        for leaf in leaves {
            let result: Result<()> = (|| {
                let b = source::read(&leaf, role, m, u)?;
                if leaf.relative.ends_with("module-info.class")
                    && leaf.relative.rsplit('/').next() == Some("module-info.class")
                {
                    if role != InputRole::Platform || leaf.relative != "module-info.class" {
                        return Err(m.error(S::Definition, K::InvalidModule, *u));
                    }
                    module(&b, o, m, *u)?;
                    u.module_descriptors += 1;
                    return Ok(());
                }
                let name = &leaf.units[..leaf.units.len() - 6];
                m.utf(name.len(), S::Definition, *u)?;
                let current = u
                    .platform
                    .ordinary_definitions
                    .checked_add(u.application.ordinary_definitions)
                    .ok_or_else(|| {
                        m.error(
                            S::Definition,
                            K::AccountingOverflow {
                                resource: R::Definitions,
                            },
                            *u,
                        )
                    })?;
                Meter::add(current, 1, o.image.max_definitions, R::Definitions)
                    .map_err(|e| m.error(S::Definition, e, *u))?;
                if role == InputRole::Application {
                    if name.starts_with(&[106, 97, 118, 97, 47]) {
                        return Err(m.error(S::Definition, K::ForbiddenApplicationName, *u));
                    }
                    if routes
                        .lookup(d.registry(), boot, name, m, *u, S::Definition)?
                        .is_some()
                    {
                        return Err(m.error(S::Definition, K::NamespaceConflict, *u));
                    }
                }
                m.retain(1, S::Definition, *u)?;
                ids.try_reserve(1).map_err(|_| m.alloc(S::Definition, *u))?;
                let id = d
                    .context(o.loading_work)
                    .define(loader, name, &b, o.parse)
                    .map_err(|e| m.error(S::Definition, K::Loading(e), *u))?;
                match role {
                    InputRole::Platform => u.platform.ordinary_definitions += 1,
                    InputRole::Application => u.application.ordinary_definitions += 1,
                }
                let class = d
                    .registry()
                    .class(id)
                    .map_err(|e| m.error(S::Definition, K::Registry(e), *u))?;
                if !(51..=65).contains(&class.version.major) || class.version.minor == 65535 {
                    return Err(m.error(
                        S::Definition,
                        K::UnsupportedVersion {
                            major: class.version.major,
                            minor: class.version.minor,
                        },
                        *u,
                    ));
                }
                routes.add(d.registry(), loader, id, m, *u, S::Definition)?;
                ids.push(id);
                Ok(())
            })();
            result.map_err(|mut e| {
                e.role = Some(role);
                e.relative_path = Some(leaf.relative);
                e
            })?;
        }
        Ok(())
    }
}

struct Resolver<'a> {
    routes: &'a Routes,
    meter: &'a mut Meter,
    usage: ApplicationUsage,
    error: Option<Box<crate::ApplicationError>>,
}
impl HierarchyResolver for Resolver<'_> {
    fn resolve(
        &mut self,
        r: &DefinitionRegistry,
        l: LoaderId,
        n: &[u16],
    ) -> std::result::Result<Option<ClassId>, ResolveError> {
        match self
            .routes
            .lookup(r, l, n, self.meter, self.usage, S::Hierarchy)
        {
            Ok(v) => Ok(v),
            Err(e) => {
                self.error = Some(e);
                Err(ResolveError::new("funded image route failed"))
            }
        }
    }
}
fn plan_edges(plan: &HierarchyPlan, m: &mut Meter, u: ApplicationUsage) -> Result<u64> {
    m.work(plan.classes().len() as u64, S::Hierarchy, u)?;
    let mut edges = 0u64;
    for class in plan.classes() {
        let layout = plan.layout(*class).ok_or_else(|| {
            m.error(
                S::Hierarchy,
                K::Invariant("physical plan layout missing"),
                u,
            )
        })?;
        let added = (layout.direct_interfaces().len() as u64)
            .checked_add(u64::from(layout.direct_superclass().is_some()))
            .ok_or_else(|| {
                m.error(
                    S::Hierarchy,
                    K::AccountingOverflow {
                        resource: R::PlanEdges,
                    },
                    u,
                )
            })?;
        m.work(added, S::Hierarchy, u)?;
        edges = edges.checked_add(added).ok_or_else(|| {
            m.error(
                S::Hierarchy,
                K::AccountingOverflow {
                    resource: R::PlanEdges,
                },
                u,
            )
        })?;
    }
    Ok(edges)
}

fn values(l: ClassTypeVerificationLimits) -> [u64; 10] {
    [
        l.max_methods,
        l.max_code_bytes,
        l.max_code_metadata_entries,
        l.max_code_utf16_units,
        l.max_code_work_units,
        l.max_type_symbols,
        l.max_state_cells,
        l.max_edge_checks,
        l.max_type_utf16_units,
        l.max_type_work_units,
    ]
}
fn usage_values(u: ClassTypeVerificationUsage) -> [u64; 10] {
    let v = u.verification;
    [
        u.methods_started,
        v.code.code_bytes,
        v.code.metadata_entries,
        v.code.utf16_units,
        v.code.work_units,
        v.type_symbols,
        v.state_cells,
        v.edge_checks,
        v.utf16_units,
        v.work_units,
    ]
}
fn limits(v: [u64; 10]) -> ClassTypeVerificationLimits {
    ClassTypeVerificationLimits {
        max_methods: v[0],
        max_code_bytes: v[1],
        max_code_metadata_entries: v[2],
        max_code_utf16_units: v[3],
        max_code_work_units: v[4],
        max_type_symbols: v[5],
        max_state_cells: v[6],
        max_edge_checks: v[7],
        max_type_utf16_units: v[8],
        max_type_work_units: v[9],
    }
}
fn usage(v: [u64; 10], bodies: u64, bodyless: u64) -> ClassTypeVerificationUsage {
    ClassTypeVerificationUsage {
        methods_started: v[0],
        code_bodies: bodies,
        bodyless,
        verification: TypeVerificationUsage {
            code: CodeValidationUsage {
                code_bytes: v[1],
                metadata_entries: v[2],
                utf16_units: v[3],
                work_units: v[4],
            },
            type_symbols: v[5],
            state_cells: v[6],
            edge_checks: v[7],
            utf16_units: v[8],
            work_units: v[9],
        },
    }
}
fn resources() -> [ClassTypeVerificationResource; 10] {
    use ClassTypeVerificationResource as C;
    use CodeValidationResource as B;
    use TypeVerificationResource as T;
    [
        C::Methods,
        C::Code(B::CodeBytes),
        C::Code(B::MetadataEntries),
        C::Code(B::Utf16Units),
        C::Code(B::WorkUnits),
        C::Type(T::TypeSymbols),
        C::Type(T::StateCells),
        C::Type(T::EdgeChecks),
        C::Type(T::Utf16Units),
        C::Type(T::WorkUnits),
    ]
}
fn proof_failure(
    e: Box<ClassTypeVerificationError>,
    old: [u64; 10],
    cap: [u64; 10],
    clamped: [u64; 10],
    individual: [u64; 10],
) -> K {
    let image = match &e.kind {
        ClassTypeVerificationErrorKind::AggregateLimit {
            resource,
            limit,
            required,
            ..
        } => resources()
            .iter()
            .position(|r| r == resource)
            .filter(|i| clamped[*i] < individual[*i] && *limit == clamped[*i])
            .map(|i| (*resource, cap[i], old[i].checked_add(*required))),
        _ => None,
    };
    match image {
        Some((resource, limit, Some(required))) => K::VerificationLimit {
            resource,
            limit,
            required,
            source: e,
        },
        Some((resource, _, None)) => K::VerificationAccountingOverflow {
            resource,
            source: Some(e),
        },
        None => K::Verification(e),
    }
}
fn prove(
    env: &VerificationEnvironment<'_>,
    id: ClassId,
    o: ApplicationOptions,
    m: &Meter,
    u: &mut ApplicationUsage,
) -> Result<()> {
    let old = usage_values(u.verification);
    let cap = values(o.verification_total);
    let individual = values(o.class.total);
    let mut clamped = [0; 10];
    for i in 0..10 {
        let remaining = cap[i].checked_sub(old[i]).ok_or_else(|| {
            m.error(
                S::Verification,
                K::AccountingOverflow {
                    resource: R::Verification(resources()[i]),
                },
                *u,
            )
        })?;
        clamped[i] = individual[i].min(remaining)
    }
    let mut options = o.class;
    options.total = limits(clamped);
    let result = verify_class_types(env, id, options);
    let accepted = match &result {
        Ok(p) => p.usage(),
        Err(e) => e.usage,
    };
    let child = usage_values(accepted);
    let mut next = old;
    for i in 0..10 {
        next[i] = Meter::add(old[i], child[i], cap[i], R::Verification(resources()[i]))
            .map_err(|e| m.error(S::Verification, e, *u))?;
    }
    let bodies = u
        .verification
        .code_bodies
        .checked_add(accepted.code_bodies)
        .ok_or_else(|| m.error(S::Verification, K::Invariant("body counter overflow"), *u))?;
    let bodyless = u
        .verification
        .bodyless
        .checked_add(accepted.bodyless)
        .ok_or_else(|| {
            m.error(
                S::Verification,
                K::Invariant("bodyless counter overflow"),
                *u,
            )
        })?;
    u.verification = usage(next, bodies, bodyless);
    match result {
        Ok(_) => {
            u.proved_classes += 1;
            Ok(())
        }
        Err(e) => {
            let kind = proof_failure(e, old, cap, clamped, individual);
            let mut error = m.error(S::Verification, kind, *u);
            error.class = Some(id);
            Err(error)
        }
    }
}
pub(crate) fn prepare(
    directories: ApplicationDirectories<'_>,
    entry: &str,
    observations: &[StaticIntObservation<'_>],
    options: ApplicationOptions,
) -> Result<PreparedImage> {
    let mut meter = Meter::new(options.image);
    let mut used = ApplicationUsage::default();
    if options.parse.allow_preview {
        return Err(meter.error(
            S::Input,
            K::InvalidInput("preview option is unsupported"),
            used,
        ));
    }
    Meter::add(
        0,
        observations.len() as u64,
        options.image.max_observations,
        R::Observations,
    )
    .map_err(|e| meter.error(S::Input, e, used))?;
    let named_name = binary(entry, &mut meter, used, S::Input)?;
    for (index, observation) in observations.iter().enumerate() {
        let valid = (|| {
            binary(observation.binary_class, &mut meter, used, S::Input)?;
            query_name(observation.field, &mut meter, used)
        })();
        valid.map_err(|mut e| {
            e.observation = Some(index);
            e
        })?;
    }
    let roots = source::roots(directories, &mut meter, used)?;
    let platform_leaves = source::inventory(&roots[0], InputRole::Platform, &mut meter, &mut used)?;
    let application_leaves =
        source::inventory(&roots[1], InputRole::Application, &mut meter, &mut used)?;
    let mut domain = LoadingDomain::new(options.registry, options.domain)
        .map_err(|e| meter.error(S::Definition, K::Loading(e), used))?;
    domain
        .enable_member_resolution(options.member)
        .map_err(|e| meter.error(S::Definition, K::Member(e), used))?;
    let boot = domain
        .create_loader()
        .map_err(|e| meter.error(S::Definition, K::Loading(e), used))?;
    let app = domain
        .create_loader()
        .map_err(|e| meter.error(S::Definition, K::Loading(e), used))?;
    let mut routes = Routes::new();
    let mut ids = Vec::new();
    let bootstrap_count;
    {
        let mut admission = Admission {
            domain: &mut domain,
            routes: &mut routes,
            ids: &mut ids,
            options,
            meter: &mut meter,
            usage: &mut used,
        };
        admission.admit(platform_leaves, InputRole::Platform, boot, boot)?;
        bootstrap_count = admission.ids.len();
        admission.admit(application_leaves, InputRole::Application, app, boot)?;
    }
    let named = routes
        .lookup(
            domain.registry(),
            app,
            &named_name,
            &mut meter,
            used,
            S::Definition,
        )?
        .ok_or_else(|| meter.error(S::Definition, K::MissingEntry, used))?;
    let mut roles = Vec::new();
    for role in [
        "java/lang/Object",
        "java/lang/Cloneable",
        "java/io/Serializable",
        "java/lang/Throwable",
        "java/lang/Error",
        "java/lang/String",
        "java/lang/Class",
        "java/lang/invoke/MethodType",
        "java/lang/invoke/MethodHandle",
    ] {
        let name: Vec<_> = role.encode_utf16().collect();
        let class = routes
            .lookup(domain.registry(), boot, &name, &mut meter, used, S::Imports)?
            .ok_or_else(|| meter.error(S::Imports, K::MissingRole(role), used))?;
        meter.retain(1, S::Imports, used)?;
        roles
            .try_reserve(1)
            .map_err(|_| meter.alloc(S::Imports, used))?;
        roles.push(class);
    }
    let core = VerificationCore {
        bootstrap_loader: boot,
        object: roles[0],
        cloneable: roles[1],
        serializable: roles[2],
        throwable: roles[3],
        string: roles[5],
        class: roles[6],
        method_type: roles[7],
        method_handle: roles[8],
    };
    let error = roles[4];
    let mut bindings = Vec::new();
    let mut imported = Vec::new();
    for import in IMPORTS {
        let name: Vec<_> = import.encode_utf16().collect();
        let class = routes
            .lookup(domain.registry(), boot, &name, &mut meter, used, S::Imports)?
            .ok_or_else(|| meter.error(S::Imports, K::MissingRole(import), used))?;
        meter.retain(2, S::Imports, used)?;
        bindings
            .try_reserve(1)
            .map_err(|_| meter.alloc(S::Imports, used))?;
        imported
            .try_reserve(1)
            .map_err(|_| meter.alloc(S::Imports, used))?;
        domain
            .context(options.loading_work)
            .record_initiation(app, &name, class)
            .map_err(|e| meter.error(S::Imports, K::Loading(e), used))?;
        routes.add(domain.registry(), app, class, &mut meter, used, S::Imports)?;
        bindings.push(VerificationBinding {
            initiating_loader: app,
            definition: class,
        });
        imported.push(class);
    }
    // Eager input policy applies to every application Class CP, including dead code.
    for id in &ids[bootstrap_count..] {
        let c = domain
            .registry()
            .class(*id)
            .map_err(|e| meter.error(S::Dependencies, K::Registry(e), used))?;
        for (index, constant) in c.constant_pool.entries().iter().enumerate() {
            meter.work(1, S::Dependencies, used)?;
            if let Constant::Class { name_index } = constant {
                let n = c
                    .constant_pool
                    .utf8(*name_index)
                    .map_err(|e| meter.error(S::Dependencies, K::Parse(e), used))?;
                meter.utf(n.units().len(), S::Dependencies, used)?;
                let mut name = n.units();
                if name.first() == Some(&91) {
                    while name.first() == Some(&91) {
                        name = &name[1..]
                    }
                    if name.first() != Some(&76) {
                        continue;
                    }
                    name = &name[1..name.len() - 1];
                }
                if routes
                    .lookup(
                        domain.registry(),
                        app,
                        name,
                        &mut meter,
                        used,
                        S::Dependencies,
                    )?
                    .is_none()
                {
                    let kind = if routes
                        .lookup(
                            domain.registry(),
                            boot,
                            name,
                            &mut meter,
                            used,
                            S::Dependencies,
                        )?
                        .is_some()
                    {
                        K::UnsupportedPlatformReference
                    } else {
                        K::MissingApplicationDependency
                    };
                    let mut e = meter.error(S::Dependencies, kind, used);
                    e.class = Some(*id);
                    e.cp_index = u16::try_from(index).ok();
                    return Err(e);
                }
            }
        }
    }
    let mut plans = Vec::new();
    let mut plan_index = HashMap::new();
    for id in &ids {
        meter.retain(2, S::Hierarchy, used)?;
        plans
            .try_reserve(1)
            .map_err(|_| meter.alloc(S::Hierarchy, used))?;
        plan_index
            .try_reserve(1)
            .map_err(|_| meter.alloc(S::Hierarchy, used))?;
        let rows = options
            .environment
            .max_class_rows
            .checked_sub(used.physical_plan_rows)
            .ok_or_else(|| {
                meter.error(
                    S::Hierarchy,
                    K::AccountingOverflow {
                        resource: R::PlanRows,
                    },
                    used,
                )
            })?;
        let edges = options
            .environment
            .max_edges
            .checked_sub(used.physical_plan_edges)
            .ok_or_else(|| {
                meter.error(
                    S::Hierarchy,
                    K::AccountingOverflow {
                        resource: R::PlanEdges,
                    },
                    used,
                )
            })?;
        let mut policy = options.hierarchy;
        policy.max_classes = policy
            .max_classes
            .min(usize::try_from(rows).unwrap_or(usize::MAX));
        policy.max_edges = policy
            .max_edges
            .min(usize::try_from(edges).unwrap_or(usize::MAX));
        let mut resolver = Resolver {
            routes: &routes,
            meter: &mut meter,
            usage: used,
            error: None,
        };
        let result = build_hierarchy(domain.registry(), *id, &mut resolver, policy);
        if let Some(e) = resolver.error {
            return Err(e);
        }
        let plan = result.map_err(|e| meter.error(S::Hierarchy, K::Hierarchy(e), used))?;
        let next_rows = Meter::add(
            used.physical_plan_rows,
            plan.classes().len() as u64,
            options.environment.max_class_rows,
            R::PlanRows,
        )
        .map_err(|e| meter.error(S::Hierarchy, e, used))?;
        let next_edges = Meter::add(
            used.physical_plan_edges,
            plan_edges(&plan, &mut meter, used)?,
            options.environment.max_edges,
            R::PlanEdges,
        )
        .map_err(|e| meter.error(S::Hierarchy, e, used))?;
        used.physical_plan_rows = next_rows;
        used.physical_plan_edges = next_edges;
        plan_index.insert(*id, plans.len());
        plans.push(plan);
    }
    let mut executable = HashSet::new();
    let mut pending = Vec::new();
    for id in ids[bootstrap_count..].iter().chain(&imported) {
        meter.retain(1, S::Verification, used)?;
        pending
            .try_reserve(1)
            .map_err(|_| meter.alloc(S::Verification, used))?;
        pending.push(*id);
    }
    while let Some(id) = pending.pop() {
        meter.work(1, S::Verification, used)?;
        if executable.contains(&id) {
            continue;
        }
        meter.retain(1, S::Verification, used)?;
        executable
            .try_reserve(1)
            .map_err(|_| meter.alloc(S::Verification, used))?;
        executable.insert(id);
        let index = *plan_index.get(&id).ok_or_else(|| {
            meter.error(
                S::Verification,
                K::Invariant("executable plan missing"),
                used,
            )
        })?;
        let layout = plans[index].layout(id).ok_or_else(|| {
            meter.error(S::Verification, K::Invariant("root layout missing"), used)
        })?;
        for dependency in layout
            .direct_superclass()
            .iter()
            .chain(layout.direct_interfaces())
        {
            meter.work(1, S::Verification, used)?;
            meter.retain(1, S::Verification, used)?;
            pending
                .try_reserve(1)
                .map_err(|_| meter.alloc(S::Verification, used))?;
            pending.push(*dependency);
        }
    }
    let mut refs = Vec::new();
    meter.retain(plans.len(), S::Environment, used)?;
    refs.try_reserve(plans.len())
        .map_err(|_| meter.alloc(S::Environment, used))?;
    refs.extend(plans.iter());
    {
        let environment = VerificationEnvironment::new(
            domain.registry(),
            &refs,
            &bindings,
            core,
            options.environment,
        )
        .map_err(|e| meter.error(S::Environment, K::Environment(e), used))?;
        for id in &ids {
            meter.work(1, S::Verification, used)?;
            if executable.contains(id) {
                prove(&environment, *id, options, &meter, &mut used)?;
            }
        }
    }
    let mut executable_plans = Vec::new();
    for id in &ids {
        meter.work(1, S::Preparation, used)?;
        if executable.contains(id) {
            let plan = &plans[plan_index[id]];
            // Every row admitted by this root belongs to the exact proved closure.
            for class in plan.classes() {
                meter.work(1, S::Preparation, used)?;
                if !executable.contains(class) {
                    return Err(meter.error(
                        S::Preparation,
                        K::Invariant("unproved layout dependency"),
                        used,
                    ));
                }
            }
            domain
                .context(options.loading_work)
                .prepare_member_owner(plan)
                .map_err(|e| meter.error(S::Preparation, K::Member(e), used))?;
            used.preparation_witnesses += 1;
            meter.retain(1, S::Preparation, used)?;
            executable_plans
                .try_reserve(1)
                .map_err(|_| meter.alloc(S::Preparation, used))?;
            executable_plans.push(plan);
        }
    }
    if used.preparation_witnesses != used.proved_classes
        || used.proved_classes != executable.len() as u64
    {
        return Err(meter.error(
            S::Preparation,
            K::Invariant("proof/witness membership differs"),
            used,
        ));
    }
    let mut heap =
        ManagedHeap::new_with_instances(options.heap, options.instance).map_err(|e| {
            meter.error(
                S::Layout,
                K::Setup {
                    kind: crate::SetupKind::Heap,
                    source: Box::new(e),
                },
                used,
            )
        })?;
    heap.admit_instance_layouts(domain.registry(), &executable_plans)
        .map_err(|e| {
            meter.error(
                S::Layout,
                K::Setup {
                    kind: crate::SetupKind::Instance,
                    source: Box::new(e),
                },
                used,
            )
        })?;
    used.admitted_executable_classes = heap.retained_layout_classes() as u64;
    if used.admitted_executable_classes != used.proved_classes {
        return Err(meter.error(
            S::Layout,
            K::Invariant("admitted union differs from proof set"),
            used,
        ));
    }
    Ok(PreparedImage {
        domain,
        routes,
        heap,
        roles: BootstrapRoles { core, error },
        named,
        app_loader: app,
        options,
        usage: used,
        meter,
    })
}
pub(crate) fn resolve_observations(
    routes: &Routes,
    registry: &DefinitionRegistry,
    loader: LoaderId,
    observations: &[StaticIntObservation<'_>],
    meter: &mut Meter,
    usage: ApplicationUsage,
) -> Result<Vec<FieldId>> {
    let mut fields = Vec::new();
    for (query, o) in observations.iter().enumerate() {
        let result: Result<()> = (|| {
            let name = binary(o.binary_class, meter, usage, S::Observations)?;
            let class = routes
                .lookup(registry, loader, &name, meter, usage, S::Observations)?
                .ok_or_else(|| meter.error(S::Observations, K::InvalidObservation, usage))?;
            meter.work(1, S::Observations, usage)?;
            if registry
                .defining_loader(class)
                .map_err(|e| meter.error(S::Observations, K::Registry(e), usage))?
                != loader
            {
                return Err(meter.error(S::Observations, K::InvalidObservation, usage));
            }
            let c = registry
                .class(class)
                .map_err(|e| meter.error(S::Observations, K::Registry(e), usage))?;
            meter.work(o.field.len() as u64, S::Observations, usage)?;
            let target_units = o.field.encode_utf16().count();
            meter.utf(target_units, S::Observations, usage)?;
            let mut target = Vec::new();
            target
                .try_reserve(target_units)
                .map_err(|_| meter.alloc(S::Observations, usage))?;
            target.extend(o.field.encode_utf16());
            let mut found = None;
            let mut replay_work = 1u64;
            let mut replay_utf = 0u64;
            for field in &c.fields {
                meter.work(3, S::Observations, usage)?;
                let n = c
                    .constant_pool
                    .utf8(field.name_index)
                    .map_err(|e| meter.error(S::Observations, K::Parse(e), usage))?;
                let compared = (n.units().len() + target.len()) as u64;
                meter.utf(compared as usize, S::Observations, usage)?;
                replay_work = replay_work.checked_add(2).ok_or_else(|| {
                    meter.error(
                        S::Observations,
                        K::AccountingOverflow {
                            resource: R::WorkUnits,
                        },
                        usage,
                    )
                })?;
                replay_utf = replay_utf.checked_add(compared).ok_or_else(|| {
                    meter.error(
                        S::Observations,
                        K::AccountingOverflow {
                            resource: R::Utf16Units,
                        },
                        usage,
                    )
                })?;
                if n.units() != target {
                    continue;
                }
                let d = c
                    .constant_pool
                    .utf8(field.descriptor_index)
                    .map_err(|e| meter.error(S::Observations, K::Parse(e), usage))?;
                let compared = (d.units().len() + 1) as u64;
                meter.utf(compared as usize, S::Observations, usage)?;
                replay_work = replay_work.checked_add(1).ok_or_else(|| {
                    meter.error(
                        S::Observations,
                        K::AccountingOverflow {
                            resource: R::WorkUnits,
                        },
                        usage,
                    )
                })?;
                replay_utf = replay_utf.checked_add(compared).ok_or_else(|| {
                    meter.error(
                        S::Observations,
                        K::AccountingOverflow {
                            resource: R::Utf16Units,
                        },
                        usage,
                    )
                })?;
                if d.equals_ascii("I") && field.access_flags & 8 != 0 {
                    let work = replay_work.checked_add(replay_utf).ok_or_else(|| {
                        meter.error(
                            S::Observations,
                            K::AccountingOverflow {
                                resource: R::WorkUnits,
                            },
                            usage,
                        )
                    })?;
                    meter.charge(replay_utf, work, 0, S::Observations, usage)?;
                    found = registry
                        .find_declared_field(class, &target, &[73])
                        .map_err(|e| meter.error(S::Observations, K::Registry(e), usage))?;
                    break;
                }
            }
            let id =
                found.ok_or_else(|| meter.error(S::Observations, K::InvalidObservation, usage))?;
            meter.retain(1, S::Observations, usage)?;
            fields
                .try_reserve(1)
                .map_err(|_| meter.alloc(S::Observations, usage))?;
            fields.push(id);
            Ok(())
        })();
        result.map_err(|mut e| {
            e.observation = Some(query);
            e
        })?;
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absolute_required_overflow_keeps_exact_child_error() {
        use std::error::Error;
        let mut r = DefinitionRegistry::new(Default::default()).unwrap();
        let l = r.create_loader().unwrap();
        let class = r
            .define(
                l,
                &"java/lang/Object".encode_utf16().collect::<Vec<_>>(),
                &crate::tests::definition("java/lang/Object", None, 0x21, false, false),
                Default::default(),
            )
            .unwrap();
        let child = Box::new(ClassTypeVerificationError {
            class,
            method_index: Some(3),
            usage: Default::default(),
            kind: ClassTypeVerificationErrorKind::AggregateLimit {
                resource: ClassTypeVerificationResource::Methods,
                limit: u64::MAX - 1,
                required: u64::MAX,
                source: None,
            },
        });
        let mut old = [0; 10];
        old[0] = 1;
        let cap = [u64::MAX; 10];
        let mut clamp = cap;
        clamp[0] -= 1;
        let kind = proof_failure(child, old, cap, clamp, cap);
        assert!(matches!(
            kind,
            K::VerificationAccountingOverflow {
                resource: ClassTypeVerificationResource::Methods,
                source: Some(_)
            }
        ));
        let e = crate::ApplicationError::new(S::Verification, kind, Default::default());
        let original = e
            .source()
            .unwrap()
            .downcast_ref::<ClassTypeVerificationError>()
            .unwrap();
        assert_eq!(original.class, class);
        assert_eq!(original.method_index, Some(3));
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    #[test]
    fn checked_definition_prefix_survives_late_route_failure_and_profile_refusal() {
        for mode in 0..3 {
            let tree = crate::tests::Tree::new();
            let mut bytes = crate::tests::definition("java/lang/Object", None, 0x21, false, false);
            if mode == 1 {
                bytes = vec![0]
            } else if mode == 2 {
                bytes[7] = 50
            }
            tree.put(true, "java/lang/Object", &bytes);
            let mut usage = ApplicationUsage::default();
            let mut scan = Meter::new(Default::default());
            let leaves =
                source::inventory(&tree.boot, InputRole::Platform, &mut scan, &mut usage).unwrap();
            let mut domain = LoadingDomain::new(Default::default(), Default::default()).unwrap();
            let boot = domain.create_loader().unwrap();
            let mut routes = Routes::new();
            let mut ids = Vec::new();
            let mut meter = Meter::new(crate::ImageLimits {
                max_scratch_entries: if mode == 0 { 1 } else { u64::MAX },
                ..Default::default()
            });
            let mut admission = Admission {
                domain: &mut domain,
                routes: &mut routes,
                ids: &mut ids,
                options: Default::default(),
                meter: &mut meter,
                usage: &mut usage,
            };
            let e = admission
                .admit(leaves, InputRole::Platform, boot, boot)
                .unwrap_err();
            if mode == 1 {
                assert_eq!(domain.registry().definition_count(), 0);
                assert_eq!(e.usage.platform.ordinary_definitions, 0);
                assert!(matches!(e.kind, K::Loading(_)))
            } else {
                assert_eq!(domain.registry().definition_count(), 1);
                assert_eq!(e.usage.platform.ordinary_definitions, 1);
                if mode == 0 {
                    assert!(matches!(
                        e.kind,
                        K::ResourceLimit {
                            resource: R::ScratchEntries,
                            ..
                        }
                    ))
                } else {
                    assert!(matches!(e.kind, K::UnsupportedVersion { major: 50, .. }))
                }
            }
            assert!(ids.is_empty());
            assert!(routes.table.is_empty());
            assert_eq!(e.usage.argument_arrays, 0);
        }
    }
}

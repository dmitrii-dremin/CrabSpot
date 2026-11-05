pub mod bytes;
pub use bytes::{Class, name};
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, HierarchyOptions, HierarchyPlan, HierarchyResolver, LoaderId,
    LoadingDomain, LoadingDomainLimits, LoadingWorkLimits, RegistryOptions, ResolveError,
    build_hierarchy,
};
use std::collections::HashMap;
pub fn domain() -> LoadingDomain {
    LoadingDomain::new(RegistryOptions::default(), LoadingDomainLimits::default()).unwrap()
}
pub fn define(d: &mut LoadingDomain, loader: LoaderId, c: &Class) -> ClassId {
    d.context(LoadingWorkLimits::default())
        .define(loader, &c.name, &c.bytes(), ParseOptions::default())
        .unwrap()
}
/// Independent intended graph. Only explicit bind() updates actual initiating records.
pub struct Graph {
    pub domain: LoadingDomain,
    pub boot: LoaderId,
    pub object: ClassId,
    pub intended: HashMap<(LoaderId, Vec<u16>), ClassId>,
}
impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}
impl Graph {
    pub fn new() -> Self {
        Self::with_limits(LoadingDomainLimits::default())
    }
    pub fn with_limits(limits: LoadingDomainLimits) -> Self {
        Self::with_options(RegistryOptions::default(), limits)
    }
    pub fn with_options(registry: RegistryOptions, limits: LoadingDomainLimits) -> Self {
        let mut domain = LoadingDomain::new(registry, limits).unwrap();
        let boot = domain.create_loader().unwrap();
        let object = define(&mut domain, boot, &Class::root());
        let mut intended = HashMap::new();
        intended.insert((boot, name("java/lang/Object")), object);
        Self {
            domain,
            boot,
            object,
            intended,
        }
    }
    pub fn loader(&mut self) -> LoaderId {
        let loader = self.domain.create_loader().unwrap();
        self.bind(loader, "java/lang/Object", self.object);
        loader
    }
    pub fn define(&mut self, loader: LoaderId, c: Class) -> ClassId {
        let class = define(&mut self.domain, loader, &c);
        self.intended.insert((loader, c.name), class);
        class
    }
    pub fn bind(&mut self, loader: LoaderId, n: &str, class: ClassId) {
        self.domain
            .context(LoadingWorkLimits::default())
            .record_initiation(loader, &name(n), class)
            .unwrap();
        self.intended.insert((loader, name(n)), class);
    }
    pub fn plan(&self, root: ClassId) -> HierarchyPlan {
        build_hierarchy(
            self.domain.registry(),
            root,
            &mut Resolver(&self.intended),
            HierarchyOptions {
                max_classes: 20000,
                max_edges: 40000,
                ..Default::default()
            },
        )
        .unwrap()
    }
}
struct Resolver<'a>(&'a HashMap<(LoaderId, Vec<u16>), ClassId>);
impl HierarchyResolver for Resolver<'_> {
    fn resolve(
        &mut self,
        _registry: &DefinitionRegistry,
        loader: LoaderId,
        n: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        Ok(self.0.get(&(loader, n.to_vec())).copied())
    }
}

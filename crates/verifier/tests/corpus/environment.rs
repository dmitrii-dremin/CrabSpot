use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, HierarchyError, HierarchyPlan, HierarchyResolver, LoaderId,
    ResolveError, build_hierarchy,
};
use crabspot_verifier::{VerificationBinding, VerificationCore, VerificationEnvironment};
use std::path::Path;

struct Loaders {
    boot: LoaderId,
    platform: LoaderId,
    application: LoaderId,
}

impl HierarchyResolver for Loaders {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        initiating: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        for loader in [self.boot, self.platform, self.application] {
            let found = registry
                .lookup_class(loader, name)
                .map_err(|error| ResolveError::new(error.to_string()))?;
            if found.is_some() || loader == initiating {
                return Ok(found);
            }
        }
        unreachable!("all fixture initiating loaders are known")
    }
}

pub struct Corpus {
    pub registry: DefinitionRegistry,
    pub targets: Vec<(String, ClassId)>,
    plans: Vec<HierarchyPlan>,
    bindings: Vec<VerificationBinding>,
    core: VerificationCore,
}

impl Corpus {
    pub fn new(facts: &Path, targets: &[(String, Vec<u8>)]) -> Self {
        Self::with_options(facts, targets, ParseOptions::default()).unwrap()
    }

    pub fn with_options(
        facts: &Path,
        targets: &[(String, Vec<u8>)],
        options: ParseOptions,
    ) -> Result<Self, HierarchyError> {
        let mut registry = DefinitionRegistry::new(Default::default()).unwrap();
        let mut loaders = Loaders {
            boot: registry.create_loader().unwrap(),
            platform: registry.create_loader().unwrap(),
            application: registry.create_loader().unwrap(),
        };
        let mut definitions = Vec::new();
        let mut bindings = Vec::new();
        let rows = std::fs::read_to_string(facts.join("facts.tsv")).unwrap();
        for row in rows.lines().filter(|line| line.starts_with("C\t")) {
            let mut columns = row.split('\t').skip(1);
            let loader = match columns.next().unwrap() {
                "bootstrap" => loaders.boot,
                "platform" => loaders.platform,
                other => panic!("unexpected JDK defining loader {other}"),
            };
            let encoded = columns.next().unwrap();
            assert_eq!(encoded.len() % 4, 0);
            let units: Vec<_> = (0..encoded.len())
                .step_by(4)
                .map(|i| u16::from_str_radix(&encoded[i..i + 4], 16).unwrap())
                .collect();
            let name = String::from_utf16(&units).unwrap();
            let bytes = std::fs::read(facts.join("classes").join(format!("{name}.class"))).unwrap();
            let definition = registry
                .define(loader, &units, &bytes, ParseOptions::default())
                .unwrap_or_else(|e| panic!("JDK metadata {name}: {e}"));
            definitions.push(definition);
            bindings.push(VerificationBinding {
                initiating_loader: loaders.application,
                definition,
            });
            if loader == loaders.boot {
                bindings.push(VerificationBinding {
                    initiating_loader: loaders.platform,
                    definition,
                });
            }
        }
        let boot_class = |name: &str| {
            registry
                .lookup_class(loaders.boot, &name.encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .unwrap()
        };
        let core = VerificationCore {
            bootstrap_loader: loaders.boot,
            object: boot_class("java/lang/Object"),
            cloneable: boot_class("java/lang/Cloneable"),
            serializable: boot_class("java/io/Serializable"),
            throwable: boot_class("java/lang/Throwable"),
            string: boot_class("java/lang/String"),
            class: boot_class("java/lang/Class"),
            method_type: boot_class("java/lang/invoke/MethodType"),
            method_handle: boot_class("java/lang/invoke/MethodHandle"),
        };
        let targets = targets
            .iter()
            .map(|(name, bytes)| {
                let id = registry
                    .define(
                        loaders.application,
                        &name.encode_utf16().collect::<Vec<_>>(),
                        bytes,
                        options,
                    )
                    .map_err(HierarchyError::Registry)?;
                definitions.push(id);
                Ok((name.clone(), id))
            })
            .collect::<Result<_, HierarchyError>>()?;
        let plans = definitions
            .into_iter()
            .map(|id| build_hierarchy(&registry, id, &mut loaders, Default::default()))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            registry,
            targets,
            plans,
            bindings,
            core,
        })
    }

    pub fn environment(&self) -> VerificationEnvironment<'_> {
        let plans = self.plans.iter().collect::<Vec<_>>();
        VerificationEnvironment::new(
            &self.registry,
            &plans,
            &self.bindings,
            self.core,
            Default::default(),
        )
        .unwrap()
    }
}

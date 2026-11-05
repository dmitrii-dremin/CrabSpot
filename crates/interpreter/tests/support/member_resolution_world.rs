use super::construction_core::core;
use crabspot_classfile::{Constant, ParseOptions};
use crabspot_interpreter::{ExceptionOutcome, ExecutionErrorKind, InitializationSession};
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldId, HierarchyResolver, LoaderId, LoadingConstraintErrorKind,
    LoadingDomain, ManagedHeap, MethodId, ReferenceTypeUniverse, ResolveError, StaticStateProfile,
    Value, VmExceptionKind, build_hierarchy,
};
use std::{collections::HashMap, fs, path::Path};

pub const BOOT: usize = 0;
pub const A: usize = 1;
pub const B: usize = 2;
pub const C: usize = 3;
pub const D: usize = 4;
pub const E: usize = 5;
const NAMES: [&str; 6] = ["bootstrap", "A", "B", "C", "D", "E"];
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/member-resolution"
);

pub fn units(name: &str) -> Vec<u16> {
    name.encode_utf16().collect()
}

#[derive(Default)]
struct HierarchyRoutes(HashMap<(LoaderId, Vec<u16>), ClassId>);

impl HierarchyResolver for HierarchyRoutes {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        if let Some(target) = self.0.get(&(loader, name.to_vec())) {
            return Ok(Some(*target));
        }
        registry
            .lookup_class(loader, name)
            .map_err(|e| ResolveError::new(e.to_string()))
    }
}

pub struct World {
    pub domain: LoadingDomain,
    pub loaders: [LoaderId; 6],
    family: &'static str,
    layouts: Vec<ClassId>,
    routes: HierarchyRoutes,
}

impl World {
    pub fn new(family: &'static str) -> Self {
        let mut domain = LoadingDomain::new(Default::default(), Default::default()).unwrap();
        domain.enable_member_resolution(Default::default()).unwrap();
        let loaders = std::array::from_fn(|_| domain.create_loader().unwrap());
        let mut world = Self {
            domain,
            loaders,
            family,
            layouts: Vec::new(),
            routes: HierarchyRoutes::default(),
        };
        for (name, parent, interfaces, interface) in [
            ("java/lang/Object", None, &[][..], false),
            (
                "java/lang/Cloneable",
                Some("java/lang/Object"),
                &[][..],
                true,
            ),
            (
                "java/io/Serializable",
                Some("java/lang/Object"),
                &[][..],
                true,
            ),
            (
                "java/lang/Throwable",
                Some("java/lang/Object"),
                &["java/io/Serializable"][..],
                false,
            ),
            (
                "java/lang/Error",
                Some("java/lang/Throwable"),
                &[][..],
                false,
            ),
        ] {
            let id = world
                .domain
                .context(Default::default())
                .define(
                    loaders[BOOT],
                    &units(name),
                    &core(name, parent, interfaces, interface),
                    ParseOptions::default(),
                )
                .unwrap();
            world.layouts.push(id);
            for client in [A, B, C, D, E] {
                world.alias(client, name, id);
            }
        }
        world
    }

    fn bytes(&self, name: &str) -> Vec<u8> {
        let path = Path::new(FIXTURES)
            .join(self.family)
            .join("classes")
            .join(format!("{name}.class"));
        fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    pub fn define(&mut self, loader: usize, name: &str) -> ClassId {
        let bytes = self.bytes(name);
        let class = self
            .domain
            .context(Default::default())
            .define(
                self.loaders[loader],
                &units(name),
                &bytes,
                Default::default(),
            )
            .unwrap();
        self.layouts.push(class);
        class
    }

    pub fn alias(&mut self, loader: usize, name: &str, target: ClassId) {
        let key = units(name);
        let loader = self.loaders[loader];
        self.domain
            .context(Default::default())
            .record_initiation(loader, &key, target)
            .unwrap();
        self.routes.0.insert((loader, key), target);
    }

    pub fn class(&self, loader: usize, name: &str) -> ClassId {
        self.domain
            .registry()
            .lookup_class(self.loaders[loader], &units(name))
            .unwrap()
            .unwrap()
    }

    pub fn method(&self, owner: ClassId, name: &str, desc: &str) -> MethodId {
        self.domain
            .registry()
            .find_declared_method(owner, &units(name), &units(desc))
            .unwrap()
            .unwrap()
    }

    pub fn field(&self, owner: ClassId, name: &str, desc: &str) -> FieldId {
        self.domain
            .registry()
            .find_declared_field(owner, &units(name), &units(desc))
            .unwrap()
            .unwrap()
    }

    pub fn member_cp(&self, owner: ClassId, symbolic: &str, name: &str, descriptor: &str) -> u16 {
        let pool = &self.domain.registry().class(owner).unwrap().constant_pool;
        let matches: Vec<_> = pool
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let (class, member) = match entry {
                    Constant::Fieldref {
                        class_index,
                        name_and_type_index,
                    }
                    | Constant::Methodref {
                        class_index,
                        name_and_type_index,
                    }
                    | Constant::InterfaceMethodref {
                        class_index,
                        name_and_type_index,
                    } => (*class_index, *name_and_type_index),
                    _ => return None,
                };
                let Constant::NameAndType {
                    name_index,
                    descriptor_index,
                } = pool.get(member).unwrap()
                else {
                    panic!("invalid NameAndType")
                };
                (pool.class_name(class).unwrap().equals_ascii(symbolic)
                    && pool.utf8(*name_index).unwrap().equals_ascii(name)
                    && pool
                        .utf8(*descriptor_index)
                        .unwrap()
                        .equals_ascii(descriptor))
                .then(|| u16::try_from(index).unwrap())
            })
            .collect();
        assert_eq!(matches.len(), 1, "{symbolic}.{name}{descriptor}");
        matches[0]
    }

    pub fn loader_name(&self, id: ClassId) -> &'static str {
        owner_name(&self.loaders, self.domain.registry(), id)
    }

    /// Act like an explicit local load, retaining an initiating record only on success.
    pub fn local(&mut self, loader: usize, leaf: &str) -> &'static str {
        let name = format!("value/{leaf}");
        let key = units(&name);
        let namespace = self.loaders[loader];
        let existing = self
            .domain
            .context(Default::default())
            .lookup_initiated(namespace, &key)
            .unwrap();
        if let Some(class) = existing {
            return self.loader_name(class);
        }
        let bytes = self.bytes(&name);
        match self.domain.context(Default::default()).define(
            namespace,
            &key,
            &bytes,
            Default::default(),
        ) {
            Ok(id) => {
                self.layouts.push(id);
                self.loader_name(id)
            }
            Err(e)
                if matches!(
                    e.kind,
                    LoadingConstraintErrorKind::DefinitionConflict { .. }
                ) =>
            {
                "LinkageError"
            }
            Err(e) => panic!("unexpected load error for {name}: {e}"),
        }
    }

    pub fn requires(&mut self, left: usize, right: usize, leaf: &str) -> bool {
        self.domain
            .context(Default::default())
            .constrained(
                &units(&format!("value/{leaf}")),
                self.loaders[left],
                self.loaders[right],
            )
            .unwrap()
    }

    pub fn prepare_failure(&mut self, root: ClassId) -> LoadingConstraintErrorKind {
        use std::error::Error;
        let plan = build_hierarchy(
            self.domain.registry(),
            root,
            &mut self.routes,
            Default::default(),
        )
        .unwrap();
        let error = self
            .domain
            .context(Default::default())
            .prepare_member_owner(&plan)
            .unwrap_err();
        error
            .source()
            .unwrap()
            .downcast_ref::<crabspot_runtime::LoadingConstraintError>()
            .unwrap()
            .kind
            .clone()
    }

    pub fn with_session<T>(
        &mut self,
        run: impl FnOnce(&mut InitializationSession<'_, '_>, &StaticStateProfile) -> T,
    ) -> T {
        self.with_session_excluding(&[], run)
    }

    pub fn with_session_excluding<T>(
        &mut self,
        excluded: &[ClassId],
        run: impl FnOnce(&mut InitializationSession<'_, '_>, &StaticStateProfile) -> T,
    ) -> T {
        let plans = self
            .layouts
            .iter()
            .filter(|id| !excluded.contains(id))
            .map(|id| {
                build_hierarchy(
                    self.domain.registry(),
                    *id,
                    &mut self.routes,
                    Default::default(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        for plan in &plans {
            self.domain
                .context(Default::default())
                .prepare_member_owner(plan)
                .unwrap();
        }
        let universe = ReferenceTypeUniverse {
            bootstrap_loader: self.loaders[BOOT],
            object: self.class(BOOT, "java/lang/Object"),
            cloneable: self.class(BOOT, "java/lang/Cloneable"),
            serializable: self.class(BOOT, "java/io/Serializable"),
        };
        let mut heap =
            ManagedHeap::new_with_instances(Default::default(), Default::default()).unwrap();
        heap.admit_instance_layouts(self.domain.registry(), &plans.iter().collect::<Vec<_>>())
            .unwrap();
        heap.bind_reference_types(self.domain.registry(), universe)
            .unwrap();
        let throwable = self.class(BOOT, "java/lang/Throwable");
        let error = self.class(BOOT, "java/lang/Error");
        let mut context = self
            .domain
            .member_context_unnamed_modules(&mut heap, Default::default(), Default::default())
            .unwrap();
        let statics = context.allocate_static_state(Default::default()).unwrap();
        let exceptions = context.prepare_exception_profile(throwable).unwrap();
        let mut session = InitializationSession::new(
            &mut context,
            &statics,
            &exceptions,
            error,
            Default::default(),
        )
        .unwrap();
        run(&mut session, &statics)
    }
}

pub fn owner_name(
    loaders: &[LoaderId; 6],
    registry: &DefinitionRegistry,
    class: ClassId,
) -> &'static str {
    let defining = registry.defining_loader(class).unwrap();
    NAMES[loaders.iter().position(|id| *id == defining).unwrap()]
}

/// The initialization profile exposes unmaterialized VM failures as an explicit category.
pub fn call(s: &mut InitializationSession<'_, '_>, method: MethodId, args: &[Value]) -> String {
    match s.run_static(
        method,
        args,
        Default::default(),
        Default::default(),
        Default::default(),
    ) {
        Ok(ExceptionOutcome::Returned(Some(Value::Int(n)))) => format!("I:{n}"),
        Err(error) => match error.kind {
            ExecutionErrorKind::UnsupportedVmException(kind) => match kind {
                VmExceptionKind::LinkageError => "LinkageError",
                VmExceptionKind::IllegalAccessError => "IllegalAccessError",
                VmExceptionKind::NoSuchFieldError => "NoSuchFieldError",
                VmExceptionKind::NoSuchMethodError => "NoSuchMethodError",
                VmExceptionKind::NullPointerException => "NullPointerException",
                VmExceptionKind::IncompatibleClassChangeError => "IncompatibleClassChangeError",
                other => panic!("unexpected VM outcome: {other:?}"),
            }
            .to_owned(),
            _ => panic!("unexpected host error: {error:?}"),
        },
        other => panic!("unexpected return: {other:?}"),
    }
}

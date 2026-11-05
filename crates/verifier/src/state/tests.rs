//! Private invariant fixtures are explicit synthetic metadata, never JDK bodies.
use super::{frames::*, symbols::*, *};
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    DefinitionRegistry, HierarchyOptions, HierarchyPlan, RegistryOptions, SameLoaderResolver,
    build_hierarchy,
};

#[derive(Clone, Copy)]
pub(super) struct Spec<'a> {
    pub name: &'a str,
    pub parent: Option<&'a str>,
    pub interfaces: &'a [&'a str],
    pub flags: u16,
    pub fields: &'a [(&'a str, &'a str, u16)],
    pub methods: &'a [(&'a str, &'a str, u16)],
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|n| n.to_be_bytes()).collect()
}
impl Pool {
    fn utf(&mut self, text: &str) -> u16 {
        assert!(text.is_ascii());
        let mut bytes = vec![1];
        bytes.extend(words(&[text.len() as u16]));
        bytes.extend(text.bytes());
        self.0.push(bytes);
        self.0.len() as u16
    }
    fn class(&mut self, text: &str) -> u16 {
        let name = self.utf(text);
        let mut bytes = vec![7];
        bytes.extend(words(&[name]));
        self.0.push(bytes);
        self.0.len() as u16
    }
}
fn definition(spec: Spec<'_>) -> Vec<u8> {
    let mut cp = Pool::default();
    let this = cp.class(spec.name);
    let parent = spec.parent.map(|n| cp.class(n)).unwrap_or(0);
    let interfaces: Vec<_> = spec.interfaces.iter().map(|n| cp.class(n)).collect();
    let fields: Vec<_> = spec
        .fields
        .iter()
        .map(|(n, d, f)| (cp.utf(n), cp.utf(d), *f))
        .collect();
    let methods: Vec<_> = spec
        .methods
        .iter()
        .map(|(n, d, f)| (cp.utf(n), cp.utf(d), *f))
        .collect();
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(cp.0.len() + 1) as u16]));
    bytes.extend(cp.0.into_iter().flatten());
    bytes.extend(words(&[spec.flags, this, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[fields.len() as u16]));
    for (n, d, f) in fields {
        bytes.extend(words(&[f, n, d, 0]));
    }
    bytes.extend(words(&[methods.len() as u16]));
    for (n, d, f) in methods {
        bytes.extend(words(&[f, n, d, 0]));
    }
    bytes.extend(words(&[0]));
    bytes
}
pub(super) struct World {
    pub registry: DefinitionRegistry,
    pub plans: Vec<HierarchyPlan>,
    pub core: VerificationCore,
    pub extra: Vec<ClassId>,
}
impl World {
    pub fn new(extras: &[Spec<'_>], object_fields: &[(&str, &str, u16)]) -> Self {
        let mut registry = DefinitionRegistry::new(RegistryOptions::default()).unwrap();
        let loader = registry.create_loader().unwrap();
        let mut ids = Vec::new();
        let names = [
            "java/lang/Object",
            "java/lang/Cloneable",
            "java/io/Serializable",
            "java/lang/Throwable",
            "java/lang/String",
            "java/lang/Class",
            "java/lang/invoke/MethodType",
            "java/lang/invoke/MethodHandle",
        ];
        for (i, name) in names.iter().enumerate() {
            let spec = Spec {
                name,
                parent: if i == 0 { None } else { Some(names[0]) },
                interfaces: &[],
                flags: if i == 1 || i == 2 { 0x601 } else { 0x21 },
                fields: if i == 0 { object_fields } else { &[] },
                methods: &[],
            };
            ids.push(
                registry
                    .define(
                        loader,
                        &name.encode_utf16().collect::<Vec<_>>(),
                        &definition(spec),
                        ParseOptions::default(),
                    )
                    .unwrap(),
            );
        }
        let core = VerificationCore {
            bootstrap_loader: loader,
            object: ids[0],
            cloneable: ids[1],
            serializable: ids[2],
            throwable: ids[3],
            string: ids[4],
            class: ids[5],
            method_type: ids[6],
            method_handle: ids[7],
        };
        let mut extra = Vec::new();
        for spec in extras {
            let id = registry
                .define(
                    loader,
                    &spec.name.encode_utf16().collect::<Vec<_>>(),
                    &definition(*spec),
                    ParseOptions::default(),
                )
                .unwrap();
            ids.push(id);
            extra.push(id);
        }
        let plans = ids
            .iter()
            .map(|id| {
                build_hierarchy(
                    &registry,
                    *id,
                    &mut SameLoaderResolver,
                    HierarchyOptions::default(),
                )
                .unwrap()
            })
            .collect();
        Self {
            registry,
            plans,
            core,
            extra,
        }
    }
    pub fn environment(&self) -> VerificationEnvironment<'_> {
        VerificationEnvironment::new(
            &self.registry,
            &self.plans.iter().collect::<Vec<_>>(),
            &[],
            self.core,
            VerificationEnvironmentOptions::default(),
        )
        .unwrap()
    }
}
fn empty() -> World {
    World::new(&[], &[])
}

#[test]
fn proof_multicounter_limit_and_overflow_leave_every_counter_unchanged() {
    let world = empty();
    let env = world.environment();
    let mut p = Proof::new(
        &env,
        world.core.object,
        TypeVerificationOptions {
            max_work_units: 3,
            ..Default::default()
        },
        CodeValidationUsage::default(),
    );
    assert!(p.charge(1, 1, 1, 1, 4).is_err());
    assert_eq!(p.usage, TypeVerificationUsage::default());
    p.options.max_work_units = u64::MAX;
    p.usage.work_units = u64::MAX;
    let before = p.usage;
    assert!(matches!(
        p.charge(1, 1, 1, 1, 1),
        Err(TypeVerificationErrorKind::AccountingOverflow {
            resource: TypeVerificationResource::WorkUnits
        })
    ));
    assert_eq!(p.usage, before);
}
#[test]
fn impossible_reservation_is_structured_and_preserves_existing_values() {
    let mut values = vec![Ty::Int];
    assert!(matches!(
        reserve(&mut values, usize::MAX, "test cells"),
        Err(TypeVerificationErrorKind::AllocationFailure {
            resource: "test cells"
        })
    ));
    assert_eq!(values, vec![Ty::Int]);
}
#[test]
fn corrupt_local_indices_tails_and_flags_fail_without_panicking() {
    let world = empty();
    let env = world.environment();
    let mut p = Proof::new(
        &env,
        world.core.object,
        TypeVerificationOptions::default(),
        CodeValidationUsage::default(),
    );
    assert!(p.local_values(&Locals::Shared(usize::MAX)).is_err());
    let mut frame = Frame {
        locals: Locals::Owned(vec![Ty::Long, Ty::Int]),
        stack: vec![Ty::WideTail],
        pending: true,
    };
    assert!(p.load(&frame, 0, Some(Ty::Long)).is_err());
    assert!(p.pop(&mut frame).is_err());
    p.arena.push(Prefix {
        values: vec![],
        this_count: 0,
    });
    p.maps.push(Map {
        offset: 0,
        locals: 0,
        stack: vec![],
        pending: false,
        location: CodeValidationLocation::Method,
    });
    frame.locals = Locals::Shared(0);
    frame.stack.clear();
    assert!(matches!(
        p.compare_map(&frame, 0, None),
        Err(TypeVerificationErrorKind::TypeRule {
            reason: "constructor initialization flag is lost at a map"
        })
    ));
}
#[test]
fn immutable_prefix_identity_is_constant_time_but_substituted_view_is_checked() {
    let world = empty();
    let env = world.environment();
    let mut p = Proof::new(
        &env,
        world.core.object,
        TypeVerificationOptions {
            max_work_units: 0,
            ..Default::default()
        },
        CodeValidationUsage::default(),
    );
    p.arena.push(Prefix {
        values: vec![Ty::Top; 30_000],
        this_count: 0,
    });
    p.maps.push(Map {
        offset: 0,
        locals: 0,
        stack: vec![],
        pending: false,
        location: CodeValidationLocation::Method,
    });
    let frame = Frame {
        locals: Locals::Shared(0),
        stack: vec![],
        pending: false,
    };
    for _ in 0..10_000 {
        p.compare_map(&frame, 0, None).unwrap();
    }
    assert_eq!(p.usage.work_units, 0);
    assert_eq!(p.usage.state_cells, 0);
    assert!(matches!(
        p.compare_map(&frame, 0, Some((Ty::This, Ty::Null))),
        Err(TypeVerificationErrorKind::ResourceLimit {
            resource: TypeVerificationResource::WorkUnits,
            required: 30_000,
            ..
        })
    ));
}
#[test]
fn no_alias_substitution_keeps_shared_prefix_and_real_write_funds_cow() {
    let world = empty();
    let env = world.environment();
    let mut p = Proof::new(
        &env,
        world.core.object,
        TypeVerificationOptions::default(),
        CodeValidationUsage::default(),
    );
    p.arena.push(Prefix {
        values: vec![Ty::Long, Ty::WideTail, Ty::Int],
        this_count: 0,
    });
    let mut frame = Frame {
        locals: Locals::Shared(0),
        stack: vec![],
        pending: false,
    };
    p.substitute_locals(&mut frame, Ty::Uninit(12), Ty::Top)
        .unwrap();
    assert!(matches!(frame.locals, Locals::Shared(0)));
    assert_eq!(p.usage.state_cells, 0);
    p.store(&mut frame, 1, Ty::Int, 3).unwrap();
    assert_eq!(
        p.local_values(&frame.locals).unwrap(),
        &[Ty::Top, Ty::Int, Ty::Int]
    );
    assert_eq!(p.arena[0].values, vec![Ty::Long, Ty::WideTail, Ty::Int]);
    assert_eq!(p.usage.state_cells, 3);
}
#[test]
fn forged_symbol_and_signature_indices_are_structured_errors() {
    let world = empty();
    let env = world.environment();
    let mut p = Proof::new(
        &env,
        world.core.object,
        TypeVerificationOptions::default(),
        CodeValidationUsage::default(),
    );
    assert!(p.resolve(usize::MAX).is_err());
    assert!(p.symbol_name(usize::MAX).is_err());
    assert!(
        p.require_assign(
            Ty::Class(usize::MAX),
            Ty::Array(1, Leaf::Primitive(Primitive::Int))
        )
        .is_err()
    );
}
#[test]
fn failed_environment_construction_is_transactional_and_retryable() {
    let world = empty();
    let count = world.registry.definition_count();
    let rows: Vec<_> = world.plans.iter().map(|p| p.classes().to_vec()).collect();
    let plans = world.plans.iter().collect::<Vec<_>>();
    let error = VerificationEnvironment::new(
        &world.registry,
        &plans,
        &[],
        world.core,
        VerificationEnvironmentOptions {
            max_bindings: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        error.kind,
        VerificationEnvironmentErrorKind::ResourceLimit {
            resource: VerificationEnvironmentResource::Bindings,
            ..
        }
    ));
    assert_eq!(world.registry.definition_count(), count);
    assert_eq!(
        world
            .plans
            .iter()
            .map(|p| p.classes().to_vec())
            .collect::<Vec<_>>(),
        rows
    );
    let a = world.environment();
    let b = world.environment();
    assert_eq!(a.usage(), b.usage());
}

#[test]
fn spelling_rehash_is_scalar_only_and_collisions_preserve_exact_identity() {
    struct Count(usize);
    impl std::hash::Hasher for Count {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0 += bytes.len();
        }
    }
    let long = vec![65; 65_535];
    let hash = std::collections::hash_map::RandomState::new();
    let key = Spelling::new(&long, &hash);
    let mut count = Count(0);
    key.hash(&mut count);
    assert_eq!(count.0, 8);
    let other = [66];
    let collision = Spelling {
        fingerprint: key.fingerprint,
        units: &other,
    };
    assert_ne!(key, collision);
    let mut table = std::collections::HashMap::new();
    table.insert(key, 1);
    table.insert(collision, 2);
    for n in 0..16 {
        table.try_reserve(1usize << n).unwrap();
    }
    assert_eq!(table.get(&key), Some(&1));
    assert_eq!(table.get(&collision), Some(&2));
}

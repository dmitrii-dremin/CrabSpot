//! Heap-owned static defaults, checked access, identity and atomic extension.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError as C, FieldResolutionContext,
    FieldResolutionOptions, FieldResource as R, HeapError, HeapOptions, HierarchyOptions,
    HierarchyPlan, HierarchyResolver, InstanceOptions, LoaderId, ManagedHeap, PrimitiveArrayKind,
    Reference, ReferenceFieldMismatch, ReferenceType as T, ReferenceTypeUniverse as U,
    RegistryOptions, ResolveError, STATIC_STATE_SLOT_BYTES as BYTES, SameLoaderResolver,
    StaticAccessError as E, StaticJavaError as J, StaticResource as SR, StaticStateOptions as O,
    TypeAccessError as TE, Value, ValueKind, build_hierarchy,
};
use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn words(values: &[u16]) -> Vec<u8> {
    values.iter().copied().flat_map(u16::to_be_bytes).collect()
}
#[derive(Default)]
struct Pool(Vec<Vec<u8>>);
impl Pool {
    fn add(&mut self, tag: u8, data: &[u8]) -> u16 {
        let mut bytes = vec![tag];
        bytes.extend(data);
        self.0.push(bytes);
        self.0.len() as u16
    }
    fn text(&mut self, text: &str) -> u16 {
        let mut encoded = Vec::new();
        for unit in text.encode_utf16() {
            match unit {
                1..=127 => encoded.push(unit as u8),
                0..=0x7ff => encoded.extend([0xc0 | (unit >> 6) as u8, 0x80 | (unit & 63) as u8]),
                _ => encoded.extend([
                    0xe0 | (unit >> 12) as u8,
                    0x80 | ((unit >> 6) & 63) as u8,
                    0x80 | (unit & 63) as u8,
                ]),
            }
        }
        let mut bytes = words(&[encoded.len() as u16]);
        bytes.extend(encoded);
        if let Some(index) = self
            .0
            .iter()
            .position(|entry| entry.first() == Some(&1) && entry.get(1..) == Some(bytes.as_slice()))
        {
            return (index + 1) as u16;
        }
        self.add(1, &bytes)
    }
    fn class(&mut self, name: &str) -> u16 {
        let index = self.text(name);
        self.add(7, &words(&[index]))
    }
}
struct Def {
    class: ClassId,
    refs: Vec<u16>,
    wrong_constant: u16,
}
// The fixture keeps each independent class-file property explicit.
#[allow(clippy::too_many_arguments)]
fn define(
    registry: &mut DefinitionRegistry,
    loader: LoaderId,
    name: &str,
    parent: Option<&str>,
    interfaces: &[&str],
    flags: u16,
    fields: &[(&str, &str, u16)],
    refs: &[(&str, &str, &str)],
) -> Def {
    let mut pool = Pool::default();
    let this = pool.class(name);
    let parent = parent.map(|name| pool.class(name)).unwrap_or(0);
    let interfaces: Vec<_> = interfaces.iter().map(|name| pool.class(name)).collect();
    let fields: Vec<_> = fields
        .iter()
        .map(|(name, desc, flags)| (pool.text(name), pool.text(desc), *flags))
        .collect();
    let mut indices = Vec::new();
    for (owner, name, desc) in refs {
        let owner = pool.class(owner);
        let name = pool.text(name);
        let desc = pool.text(desc);
        let nat = pool.add(12, &words(&[name, desc]));
        indices.push(pool.add(9, &words(&[owner, nat])));
    }
    let wrong_constant = pool.text("not a Fieldref");
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[flags, this, parent, interfaces.len() as u16]));
    bytes.extend(words(&interfaces));
    bytes.extend(words(&[fields.len() as u16]));
    for (name, desc, flags) in fields {
        bytes.extend(words(&[flags, name, desc, 0]));
    }
    bytes.extend(words(&[0, 0]));
    Def {
        class: registry
            .define(loader, &units(name), &bytes, ParseOptions::default())
            .unwrap(),
        refs: indices,
        wrong_constant,
    }
}
fn object(registry: &mut DefinitionRegistry, loader: LoaderId) -> ClassId {
    define(
        registry,
        loader,
        "java/lang/Object",
        None,
        &[],
        0x21,
        &[],
        &[],
    )
    .class
}
fn plan(registry: &DefinitionRegistry, class: ClassId) -> HierarchyPlan {
    build_hierarchy(
        registry,
        class,
        &mut SameLoaderResolver,
        HierarchyOptions::default(),
    )
    .unwrap()
}
fn heap(registry: &DefinitionRegistry, classes: &[ClassId]) -> ManagedHeap {
    let plans: Vec<_> = classes.iter().map(|class| plan(registry, *class)).collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap
}
fn field(
    registry: &DefinitionRegistry,
    class: ClassId,
    name: &str,
    desc: &str,
) -> crabspot_runtime::FieldId {
    registry
        .find_declared_field(class, &units(name), &units(desc))
        .unwrap()
        .unwrap()
}
#[derive(Default)]
struct Resolver {
    selected: HashMap<(LoaderId, Vec<u16>), ClassId>,
    responses: VecDeque<Result<Option<ClassId>, ResolveError>>,
    calls: Rc<Cell<usize>>,
}
impl HierarchyResolver for Resolver {
    fn resolve(
        &mut self,
        registry: &DefinitionRegistry,
        loader: LoaderId,
        name: &[u16],
    ) -> Result<Option<ClassId>, ResolveError> {
        self.calls.set(self.calls.get() + 1);
        if let Some(response) = self.responses.pop_front() {
            return response;
        }
        if let Some(class) = self.selected.get(&(loader, name.to_vec())) {
            return Ok(Some(*class));
        }
        registry
            .lookup_class(loader, name)
            .map_err(|error| ResolveError::new(error.to_string()))
    }
}

fn setup() -> (DefinitionRegistry, U) {
    setup_options(RegistryOptions::default())
}
fn setup_options(options: RegistryOptions) -> (DefinitionRegistry, U) {
    let mut registry = DefinitionRegistry::new(options).unwrap();
    let loader = registry.create_loader().unwrap();
    let object = object(&mut registry, loader);
    let cloneable = define(
        &mut registry,
        loader,
        "java/lang/Cloneable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
    )
    .class;
    let serializable = define(
        &mut registry,
        loader,
        "java/io/Serializable",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
    )
    .class;
    (
        registry,
        U {
            bootstrap_loader: loader,
            object,
            cloneable,
            serializable,
        },
    )
}
fn bound_heap(registry: &DefinitionRegistry, u: U, classes: &[ClassId]) -> ManagedHeap {
    let mut classes = classes.to_vec();
    classes.extend([u.cloneable, u.serializable]);
    let mut heap = heap(registry, &classes);
    heap.bind_reference_types(registry, u).unwrap();
    heap
}
fn context<'a>(
    registry: &'a DefinitionRegistry,
    heap: &'a mut ManagedHeap,
    resolver: &'a mut dyn HierarchyResolver,
) -> FieldResolutionContext<'a> {
    FieldResolutionContext::new_unnamed_modules(
        registry,
        heap,
        resolver,
        FieldResolutionOptions::default(),
    )
    .unwrap()
}
fn children(heap: &ManagedHeap, reference: Reference) -> Vec<crabspot_runtime::HeapRef> {
    heap.children(reference).unwrap().collect()
}
fn mismatch(field: crabspot_runtime::FieldId, source: T, target: T) -> E {
    E::IncompatibleReference(Box::new(ReferenceFieldMismatch {
        field,
        source,
        target,
    }))
}

#[test]
fn every_default_and_raw_primitive_store_uses_one_logical_location_without_heap_quota() {
    let (mut registry, u) = setup();
    let cases = [
        ("z", "Z", Value::Int(3), Value::Int(1)),
        ("b", "B", Value::Int(128), Value::Int(-128)),
        ("c", "C", Value::Int(-1), Value::Int(65535)),
        ("s", "S", Value::Int(32768), Value::Int(-32768)),
        ("i", "I", Value::Int(i32::MIN), Value::Int(i32::MIN)),
        ("j", "J", Value::Long(i64::MAX), Value::Long(i64::MAX)),
        ("f", "F", Value::Float(0x7fc01234), Value::Float(0x7fc01234)),
        (
            "d",
            "D",
            Value::Double(0x8000000000000000),
            Value::Double(0x8000000000000000),
        ),
        (
            "o",
            "Ljava/lang/Object;",
            Value::Reference(Reference::null()),
            Value::Reference(Reference::null()),
        ),
        (
            "a",
            "[[I",
            Value::Reference(Reference::null()),
            Value::Reference(Reference::null()),
        ),
    ];
    let fields: Vec<_> = cases
        .iter()
        .map(|(name, desc, _, _)| (*name, *desc, 9))
        .collect();
    let refs: Vec<_> = cases
        .iter()
        .map(|(name, desc, _, _)| ("C", *name, *desc))
        .collect();
    let owner = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &refs,
    );
    let plan = plan(&registry, owner.class);
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions {
            max_arrays: 0,
            max_array_length: 0,
            max_total_elements: 0,
            max_payload_bytes: 0,
        },
        InstanceOptions {
            max_instances: 0,
            max_instance_slots: 0,
            max_total_instance_slots: 0,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let profile = ctx.allocate_static_state(O::default()).unwrap();
    assert_eq!(
        (
            profile.class_count(),
            profile.slot_count(),
            profile.payload_bytes()
        ),
        (2, cases.len(), BYTES * cases.len())
    );
    assert_eq!(ctx.static_roots(&profile).unwrap().count(), 0);
    for (index, (name, desc, value, expected)) in cases.iter().copied().enumerate() {
        let id = field(&registry, owner.class, name, desc);
        let default = match desc {
            "J" => Value::Long(0),
            "F" => Value::Float(0),
            "D" => Value::Double(0),
            "Ljava/lang/Object;" | "[[I" => Value::Reference(Reference::null()),
            _ => Value::Int(0),
        };
        assert_eq!(ctx.inspect_static(&profile, id), Ok(default));
        assert_eq!(
            ctx.read_static(&profile, owner.class, owner.refs[index]),
            Ok(default)
        );
        ctx.write_static(&profile, owner.class, owner.refs[index], value)
            .unwrap();
        assert_eq!(ctx.inspect_static(&profile, id), Ok(expected));
        let wrong = if expected.kind() == ValueKind::Int {
            Value::Long(0)
        } else {
            Value::Int(0)
        };
        assert_eq!(
            ctx.write_static(&profile, owner.class, owner.refs[index], wrong),
            Err(E::ValueKindMismatch {
                field: id,
                expected: expected.kind(),
                actual: wrong.kind()
            })
        );
        assert_eq!(ctx.inspect_static(&profile, id), Ok(expected));
    }
    let before = ctx.lookup_steps();
    let saved = ctx.static_fields(&profile).unwrap().to_vec();
    assert_eq!(ctx.allocate_static_state(O::default()), Ok(profile));
    ctx.check_static_profile(&profile).unwrap();
    assert_eq!(ctx.lookup_steps(), before);
    assert_eq!(ctx.static_fields(&profile).unwrap(), saved);
    assert_eq!(
        (
            ctx.heap().allocated_arrays(),
            ctx.heap().allocated_instances(),
            ctx.heap().payload_bytes()
        ),
        (0, 0, 0)
    );
}

#[test]
fn hidden_inherited_and_interface_locations_follow_the_shared_selector_and_final_order() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let i = define(
        &mut registry,
        l,
        "I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[("shared", "I", 0x19)],
        &[],
    );
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 9), ("shared", "I", 9), ("inherited", "J", 9)],
        &[],
    );
    let own = define(
        &mut registry,
        l,
        "C",
        Some("Base"),
        &["I"],
        0x21,
        &[("x", "I", 9), ("instance", "I", 1), ("final", "I", 0x19)],
        &[
            ("C", "x", "I"),
            ("Base", "x", "I"),
            ("C", "inherited", "J"),
            ("C", "shared", "I"),
            ("C", "instance", "I"),
            ("C", "final", "I"),
            ("C", "missing", "I"),
            ("[I", "x", "I"),
        ],
    );
    let mut heap = heap(&registry, &[own.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    assert_eq!(p.slot_count(), 6);
    ctx.write_static(&p, own.class, own.refs[0], Value::Int(11))
        .unwrap();
    ctx.write_static(&p, own.class, own.refs[1], Value::Int(22))
        .unwrap();
    ctx.write_static(&p, own.class, own.refs[2], Value::Long(33))
        .unwrap();
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, base.class, "x", "I")),
        Ok(Value::Int(22))
    );
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, own.class, "x", "I")),
        Ok(Value::Int(11))
    );
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, base.class, "inherited", "J")),
        Ok(Value::Long(33))
    );
    assert_eq!(
        ctx.write_static(&p, own.class, own.refs[3], Value::Long(0)),
        Err(E::Java(J::IllegalAccess))
    );
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, i.class, "shared", "I")),
        Ok(Value::Int(0))
    );
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, base.class, "shared", "I")),
        Ok(Value::Int(0))
    );
    for (index, error) in [
        (4, E::Java(J::IncompatibleClassChange)),
        (5, E::Java(J::IllegalAccess)),
        (6, E::Java(J::NoSuchField)),
        (7, E::Context(C::UnsupportedArrayOwner)),
    ] {
        assert_eq!(
            ctx.write_static(
                &p,
                own.class,
                own.refs[index],
                Value::Reference(Reference::null())
            ),
            Err(error)
        );
    }
    assert_eq!(
        ctx.read_static(&p, own.class, own.refs[4]),
        Err(E::Java(J::IncompatibleClassChange))
    );
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, own.class, "instance", "I")),
        Err(E::MissingLocation {
            field: field(&registry, own.class, "instance", "I")
        })
    );
    assert_eq!(
        ctx.read_static(&p, own.class, own.wrong_constant),
        Err(E::Context(C::InvalidConstant {
            origin: own.class,
            index: own.wrong_constant
        }))
    );
}

#[test]
fn static_reference_roots_preserve_aliases_arrays_cycles_and_failed_assignment() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let target = define(
        &mut registry,
        l,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("edge", "Ljava/lang/Object;", 1)],
        &[("Target", "edge", "Ljava/lang/Object;")],
    );
    let own = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("a", "Ljava/lang/Object;", 9),
            ("b", "Ljava/lang/Object;", 9),
            ("target", "LTarget;", 9),
            ("ints", "[I", 9),
            ("objects", "[Ljava/lang/Object;", 9),
        ],
        &[
            ("C", "a", "Ljava/lang/Object;"),
            ("C", "b", "Ljava/lang/Object;"),
            ("C", "target", "LTarget;"),
            ("C", "ints", "[I"),
            ("C", "objects", "[Ljava/lang/Object;"),
        ],
    );
    let mut heap = bound_heap(&registry, u, &[target.class, own.class]);
    let value = heap.allocate_instance(target.class).unwrap();
    let unrelated = heap.allocate_instance(own.class).unwrap();
    let ints = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 2)
        .unwrap();
    heap.store(ints, 0, Value::Int(97)).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(target.class, target.refs[0], value, Value::Reference(value))
        .unwrap();
    let old_children = children(ctx.heap(), value);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    for index in [0, 1, 2] {
        ctx.write_static(&p, own.class, own.refs[index], Value::Reference(value))
            .unwrap();
    }
    assert_eq!(
        ctx.static_roots(&p).unwrap().collect::<Vec<_>>(),
        vec![value.heap_ref().unwrap(); 3]
    );
    ctx.write_static(&p, own.class, own.refs[3], Value::Reference(ints))
        .unwrap();
    let matrix = ctx
        .allocate_reference_array(
            T::Array {
                dimensions: 1,
                element: crabspot_runtime::ArrayElement::Primitive(PrimitiveArrayKind::Int),
                defining_loader: l,
            },
            1,
        )
        .unwrap();
    ctx.write_static(&p, own.class, own.refs[4], Value::Reference(matrix))
        .unwrap();
    let roots = ctx.static_roots(&p).unwrap().collect::<Vec<_>>();
    let id = field(&registry, own.class, "target", "LTarget;");
    assert_eq!(
        ctx.write_static(&p, own.class, own.refs[2], Value::Reference(unrelated)),
        Err(mismatch(id, T::Class(own.class), T::Class(target.class)))
    );
    assert_eq!(ctx.static_roots(&p).unwrap().collect::<Vec<_>>(), roots);
    assert_eq!(children(ctx.heap(), value), old_children);
    assert_eq!(ctx.heap().load(ints, 0), Ok(Value::Int(97)));
    assert!(matches!(
        ctx.write_static(&p, own.class, own.refs[4], Value::Reference(ints)),
        Err(E::IncompatibleReference(_))
    ));
    ctx.write_static(
        &p,
        own.class,
        own.refs[0],
        Value::Reference(Reference::null()),
    )
    .unwrap();
    assert_eq!(
        ctx.static_roots(&p).unwrap().collect::<Vec<_>>(),
        roots[1..]
    );
}

#[test]
fn options_and_profiles_are_heap_owned_and_universe_binding_needs_only_a_fresh_context() {
    let (mut registry, u) = setup();
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 9), ("r", "Ljava/lang/Object;", 9)],
        &[("C", "x", "I"), ("C", "r", "Ljava/lang/Object;")],
    );
    let mut heap = heap(&registry, &[own.class, u.cloneable, u.serializable]);
    let value = heap.allocate_instance(own.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    ctx.write_static(&p, own.class, own.refs[0], Value::Int(43))
        .unwrap();
    ctx.write_static(
        &p,
        own.class,
        own.refs[1],
        Value::Reference(Reference::null()),
    )
    .unwrap();
    assert_eq!(
        ctx.write_static(&p, own.class, own.refs[1], Value::Reference(value)),
        Err(E::Type(Box::new(TE::MissingUniverse)))
    );
    let before = ctx.lookup_steps();
    assert_eq!(
        ctx.allocate_static_state(O {
            max_slots: 0,
            ..O::default()
        }),
        Err(E::OptionsMismatch)
    );
    assert_eq!(ctx.lookup_steps(), before);
    ctx.heap_mut().bind_reference_types(&registry, u).unwrap();
    assert_eq!(
        ctx.check_static_profile(&p),
        Err(E::Context(C::ContextChanged))
    );
    drop(ctx);
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    ctx.check_static_profile(&p).unwrap();
    assert_eq!(ctx.allocate_static_state(O::default()), Ok(p));
    assert_eq!(ctx.lookup_steps(), before);
    ctx.write_static(&p, own.class, own.refs[1], Value::Reference(value))
        .unwrap();
    assert_eq!(
        ctx.read_static(&p, own.class, own.refs[0]),
        Ok(Value::Int(43))
    );
}

#[test]
fn atomic_extension_appends_new_declared_locations_preserves_values_and_allows_work_retry() {
    let (mut registry, u) = setup();
    let early = define(
        &mut registry,
        u.bootstrap_loader,
        "Early",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "Ljava/lang/Object;", 9)],
        &[("Early", "r", "Ljava/lang/Object;")],
    );
    let late = define(
        &mut registry,
        u.bootstrap_loader,
        "Late",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 9), ("saved", "Ljava/lang/Object;", 9)],
        &[("Late", "x", "I"), ("Late", "saved", "Ljava/lang/Object;")],
    );
    let empty = define(
        &mut registry,
        u.bootstrap_loader,
        "Empty",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let mut heap = bound_heap(&registry, u, &[late.class]);
    let value = heap.allocate_instance(late.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let p = {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let p = ctx.allocate_static_state(O::default()).unwrap();
        ctx.write_static(&p, late.class, late.refs[0], Value::Int(71))
            .unwrap();
        ctx.write_static(&p, late.class, late.refs[1], Value::Reference(value))
            .unwrap();
        p
    };
    let oldfields = [
        field(&registry, late.class, "x", "I"),
        field(&registry, late.class, "saved", "Ljava/lang/Object;"),
    ];
    let plans = [plan(&registry, early.class), plan(&registry, empty.class)];
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    let seed = context(&registry, &mut heap, &mut resolver).lookup_steps();
    for allowance in [
        p.class_count() + 1,
        p.class_count() + 3 + "Ljava/lang/Object;".len() - 1,
    ] {
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: seed + allowance,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        assert_eq!(ctx.check_static_profile(&p), Err(E::ProfileMismatch));
        assert!(matches!(
            ctx.allocate_static_state(O::default()),
            Err(E::Context(C::ResourceLimit {
                resource: R::LookupSteps,
                ..
            }))
        ));
        assert!(ctx.lookup_steps() <= seed + allowance);
        drop(ctx);
    }
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let next = ctx.allocate_static_state(O::default()).unwrap();
    assert_eq!(next.class_count(), p.class_count() + 2);
    assert_eq!(next.slot_count(), 3);
    assert_eq!(
        ctx.static_fields(&next).unwrap(),
        [
            oldfields[0],
            oldfields[1],
            field(&registry, early.class, "r", "Ljava/lang/Object;")
        ]
    );
    assert_eq!(ctx.inspect_static(&next, oldfields[0]), Ok(Value::Int(71)));
    ctx.write_static(&next, early.class, early.refs[0], Value::Reference(value))
        .unwrap();
    let before = ctx.lookup_steps();
    assert_eq!(ctx.allocate_static_state(O::default()), Ok(next));
    assert_eq!(ctx.lookup_steps(), before);
    assert_eq!(
        ctx.static_roots(&next).unwrap().collect::<Vec<_>>(),
        [value.heap_ref().unwrap(); 2]
    );
}

#[test]
fn class_slot_payload_and_work_boundaries_are_inclusive_without_eager_policy_allocation() {
    let (mut registry, u) = setup();
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("instance", "J", 1), ("a", "I", 9), ("b", "I", 9)],
        &[],
    );
    let mut resolver = SameLoaderResolver;
    for (options, expected) in [
        (
            O {
                max_classes: 0,
                ..O::default()
            },
            Some((SR::Classes, 0, 2)),
        ),
        (
            O {
                max_slots: 0,
                ..O::default()
            },
            Some((SR::Slots, 0, 1)),
        ),
        (
            O {
                max_slots: 1,
                ..O::default()
            },
            Some((SR::Slots, 1, 2)),
        ),
        (
            O {
                max_payload_bytes: BYTES - 1,
                ..O::default()
            },
            Some((SR::PayloadBytes, BYTES - 1, BYTES)),
        ),
        (
            O {
                max_classes: 2,
                max_slots: 2,
                max_payload_bytes: 2 * BYTES,
            },
            None,
        ),
        (
            O {
                max_classes: usize::MAX,
                max_slots: usize::MAX,
                max_payload_bytes: usize::MAX,
            },
            None,
        ),
    ] {
        let mut heap = heap(&registry, &[own.class]);
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let seed = ctx.lookup_steps();
        let result = ctx.allocate_static_state(options);
        if let Some((resource, limit, required)) = expected {
            assert_eq!(
                result,
                Err(E::ResourceLimit {
                    resource,
                    limit,
                    required
                })
            );
            assert_eq!(
                ctx.allocate_static_state(O::default())
                    .unwrap()
                    .slot_count(),
                2
            );
        } else {
            let p = result.unwrap();
            assert_eq!(p.payload_bytes(), 2 * BYTES);
            assert_eq!(ctx.lookup_steps() - seed, 6);
        }
    }
    for extra in [1, 2, 3, 4, 5, 6] {
        let mut heap = heap(&registry, &[own.class]);
        let seed = context(&registry, &mut heap, &mut resolver).lookup_steps();
        let limit = seed + extra;
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            FieldResolutionOptions {
                max_lookup_steps: limit,
                ..FieldResolutionOptions::default()
            },
        )
        .unwrap();
        let result = ctx.allocate_static_state(O::default());
        if extra < 6 {
            assert!(matches!(
                result,
                Err(E::Context(C::ResourceLimit {
                    resource: R::LookupSteps,
                    ..
                }))
            ));
        } else {
            result.unwrap();
        }
        assert!(ctx.lookup_steps() <= limit);
    }
}

#[test]
fn empty_classes_have_membership_with_zero_slots_and_zero_payload() {
    let (mut registry, u) = setup();
    let other = define(
        &mut registry,
        u.bootstrap_loader,
        "Empty",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let mut heap = heap(&registry, &[u.object]);
    let mut resolver = SameLoaderResolver;
    let options = O {
        max_classes: 2,
        max_slots: 0,
        max_payload_bytes: 0,
    };
    let p = {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let before = ctx.lookup_steps();
        let p = ctx.allocate_static_state(options).unwrap();
        assert_eq!(ctx.lookup_steps() - before, 1);
        assert_eq!(
            (p.class_count(), p.slot_count(), p.payload_bytes()),
            (1, 0, 0)
        );
        p
    };
    let plan = plan(&registry, other.class);
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(ctx.check_static_profile(&p), Err(E::ProfileMismatch));
    let next = ctx.allocate_static_state(options).unwrap();
    assert_eq!(
        (next.class_count(), next.slot_count(), next.payload_bytes()),
        (2, 0, 0)
    );
    assert!(ctx.static_fields(&next).unwrap().is_empty());
    assert_eq!(ctx.static_roots(&next).unwrap().count(), 0);
}

#[test]
fn static_private_package_and_protected_access_keep_loader_scoped_rules() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let fields = [
        ("private", "I", 10),
        ("package", "I", 8),
        ("protected", "I", 12),
        ("public", "I", 9),
    ];
    let refs: Vec<_> = fields
        .iter()
        .map(|(name, desc, _)| ("a/Base", *name, *desc))
        .collect();
    let base = define(
        &mut registry,
        l,
        "a/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &refs,
    );
    let sibling = define(
        &mut registry,
        l,
        "a/Sibling",
        Some("a/Base"),
        &[],
        0x21,
        &[],
        &[],
    );
    let same = define(
        &mut registry,
        l,
        "a/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &refs,
    );
    let origin = define(
        &mut registry,
        l,
        "b/Origin",
        Some("a/Base"),
        &[],
        0x21,
        &[],
        &[("a/Sibling", "protected", "I")],
    );
    let other = define(
        &mut registry,
        l,
        "c/Other",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &refs,
    );
    let mut heap = heap(
        &registry,
        &[sibling.class, same.class, origin.class, other.class],
    );
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    ctx.write_static(&p, base.class, base.refs[0], Value::Int(9))
        .unwrap();
    assert_eq!(
        ctx.read_static(&p, same.class, same.refs[0]),
        Err(E::Context(C::UnsupportedNestAccess {
            origin: same.class,
            owner: base.class
        }))
    );
    assert_eq!(
        ctx.read_static(&p, same.class, same.refs[1]),
        Ok(Value::Int(0))
    );
    assert_eq!(
        ctx.read_static(&p, other.class, other.refs[1]),
        Err(E::Java(J::IllegalAccess))
    );
    assert_eq!(
        ctx.read_static(&p, other.class, other.refs[2]),
        Err(E::Java(J::IllegalAccess))
    );
    ctx.write_static(&p, origin.class, origin.refs[0], Value::Int(51))
        .unwrap();
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, base.class, "protected", "I")),
        Ok(Value::Int(51))
    );
}

fn delegated_heap(
    registry: &DefinitionRegistry,
    u: U,
    roots: &[ClassId],
    resolver: &mut Resolver,
) -> ManagedHeap {
    let plans: Vec<_> = roots
        .iter()
        .chain([&u.cloneable, &u.serializable])
        .map(|class| {
            build_hierarchy(registry, *class, resolver, HierarchyOptions::default()).unwrap()
        })
        .collect();
    let mut heap =
        ManagedHeap::new_with_instances(HeapOptions::default(), InstanceOptions::default())
            .unwrap();
    heap.admit_instance_layouts(registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(registry, u).unwrap();
    heap
}

#[test]
fn delegated_numeric_aliases_work_but_nominal_and_same_text_package_boundaries_remain() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let other = registry.create_loader().unwrap();
    let base = define(
        &mut registry,
        l,
        "p/Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("numeric", "I", 9),
            ("nominal", "Ljava/lang/Object;", 9),
            ("package", "I", 8),
            ("array", "[I", 9),
        ],
        &[],
    );
    let origin = define(
        &mut registry,
        other,
        "p/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("p/Base", "numeric", "I"),
            ("p/Base", "numeric", "I"),
            ("p/Base", "nominal", "Ljava/lang/Object;"),
            ("p/Base", "package", "I"),
            ("p/Base", "array", "[I"),
        ],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((other, units("java/lang/Object")), u.object);
    resolver
        .selected
        .insert((other, units("p/Base")), base.class);
    let mut heap = delegated_heap(&registry, u, &[base.class, origin.class], &mut resolver);
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let ints = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    ctx.write_static(&p, origin.class, origin.refs[0], Value::Int(25))
        .unwrap();
    assert_eq!(
        ctx.read_static(&p, origin.class, origin.refs[1]),
        Ok(Value::Int(25))
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        ctx.write_static(
            &p,
            origin.class,
            origin.refs[2],
            Value::Reference(Reference::null())
        ),
        Err(E::Context(C::UnsupportedLoadingConstraint {
            origin: origin.class,
            owner: base.class
        }))
    );
    assert_eq!(
        ctx.read_static(&p, origin.class, origin.refs[3]),
        Err(E::Java(J::IllegalAccess))
    );
    ctx.write_static(&p, origin.class, origin.refs[4], Value::Reference(ints))
        .unwrap();
    assert_eq!(
        ctx.static_roots(&p).unwrap().collect::<Vec<_>>(),
        [ints.heap_ref().unwrap()]
    );
}

#[test]
fn inaccessible_descriptor_types_and_actual_cross_loader_values_use_identity_assignment() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let hidden = define(
        &mut registry,
        l,
        "p/Hidden",
        Some("java/lang/Object"),
        &[],
        0x20,
        &[],
        &[],
    );
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let child = define(
        &mut registry,
        second,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[],
        &[],
    );
    let own = define(
        &mut registry,
        l,
        "q/Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("hidden", "Lp/Hidden;", 9),
            ("base", "LBase;", 9),
            ("bases", "[LBase;", 9),
        ],
        &[],
    );
    let caller = define(
        &mut registry,
        l,
        "r/Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[
            ("q/Owner", "hidden", "Lp/Hidden;"),
            ("q/Owner", "base", "LBase;"),
            ("q/Owner", "bases", "[LBase;"),
        ],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("Base")), base.class);
    let mut heap = delegated_heap(
        &registry,
        u,
        &[hidden.class, child.class, own.class, caller.class],
        &mut resolver,
    );
    let hidden_ref = heap.allocate_instance(hidden.class).unwrap();
    let child_ref = heap.allocate_instance(child.class).unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let array = ctx
        .allocate_reference_array(T::Class(child.class), 1)
        .unwrap();
    ctx.write_reference_array(array, 0, child_ref).unwrap();
    let p = ctx.allocate_static_state(O::default()).unwrap();
    for (index, value) in [hidden_ref, child_ref, array].iter().copied().enumerate() {
        ctx.write_static(
            &p,
            caller.class,
            caller.refs[index],
            Value::Reference(value),
        )
        .unwrap();
        assert_eq!(
            ctx.read_static(&p, caller.class, caller.refs[index]),
            Ok(Value::Reference(value))
        );
    }
    assert_eq!(
        ctx.static_roots(&p).unwrap().collect::<Vec<_>>(),
        [
            hidden_ref.heap_ref().unwrap(),
            child_ref.heap_ref().unwrap(),
            array.heap_ref().unwrap()
        ]
    );
}

#[test]
fn foreign_profiles_fields_and_same_size_heap_replacement_do_not_gain_access() {
    let (mut registry, u) = setup();
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 9)],
        &[("C", "x", "I")],
    );
    let mut first = heap(&registry, &[own.class]);
    let mut second = heap(&registry, &[own.class]);
    let mut resolver = SameLoaderResolver;
    let p = context(&registry, &mut first, &mut resolver)
        .allocate_static_state(O::default())
        .unwrap();
    let other = context(&registry, &mut second, &mut resolver)
        .allocate_static_state(O::default())
        .unwrap();
    let (mut foreign, fu) = setup();
    let foreign_owner = define(
        &mut foreign,
        fu.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 9)],
        &[],
    );
    let foreign_field = field(&foreign, foreign_owner.class, "x", "I");
    let mut ctx = context(&registry, &mut first, &mut resolver);
    let before = ctx.lookup_steps();
    assert_eq!(ctx.check_static_profile(&other), Err(E::ProfileMismatch));
    assert_eq!(
        ctx.write_static(&other, own.class, own.refs[0], Value::Int(9)),
        Err(E::ProfileMismatch)
    );
    assert!(matches!(
        ctx.inspect_static(&p, foreign_field),
        Err(E::Context(C::Registry(_)))
    ));
    let old = std::mem::replace(ctx.heap_mut(), second);
    assert_eq!(
        old.retained_layout_classes(),
        ctx.heap().retained_layout_classes()
    );
    assert_eq!(
        ctx.check_static_profile(&p),
        Err(E::Context(C::ContextChanged))
    );
    assert_eq!(ctx.lookup_steps(), before);
    drop(ctx);
    let mut ctx = context(&registry, &mut first, &mut resolver);
    assert_eq!(ctx.check_static_profile(&p), Err(E::ProfileMismatch));
    ctx.check_static_profile(&other).unwrap();
}

#[test]
fn foreign_reference_values_fail_after_static_and_final_checks_without_changing_roots() {
    let (mut registry, u) = setup();
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("r", "Ljava/lang/Object;", 9),
            ("final", "Ljava/lang/Object;", 0x19),
            ("instance", "Ljava/lang/Object;", 1),
        ],
        &[
            ("C", "r", "Ljava/lang/Object;"),
            ("C", "final", "Ljava/lang/Object;"),
            ("C", "instance", "Ljava/lang/Object;"),
        ],
    );
    let mut heap = bound_heap(&registry, u, &[own.class]);
    let value = heap.allocate_instance(own.class).unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    ctx.write_static(&p, own.class, own.refs[0], Value::Reference(value))
        .unwrap();
    assert_eq!(
        ctx.write_static(&p, own.class, own.refs[0], Value::Reference(foreign_ref)),
        Err(E::Type(Box::new(TE::Heap(HeapError::ForeignHeap))))
    );
    assert_eq!(
        ctx.write_static(&p, own.class, own.refs[1], Value::Reference(foreign_ref)),
        Err(E::Java(J::IllegalAccess))
    );
    assert_eq!(
        ctx.write_static(&p, own.class, own.refs[2], Value::Reference(foreign_ref)),
        Err(E::Java(J::IncompatibleClassChange))
    );
    assert_eq!(
        ctx.static_roots(&p).unwrap().collect::<Vec<_>>(),
        [value.heap_ref().unwrap()]
    );
}

#[test]
fn full_descriptor_work_and_pre_callback_binding_limits_preserve_static_values() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let target = define(
        &mut registry,
        second,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let own = define(
        &mut registry,
        l,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LTarget;", 9)],
        &[("C", "r", "LTarget;")],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver.selected.insert((l, units("Target")), target.class);
    let mut heap = delegated_heap(&registry, u, &[target.class, own.class], &mut resolver);
    let value = heap.allocate_instance(target.class).unwrap();
    let (p, seed_symbols, seed_units) = {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        let p = ctx.allocate_static_state(O::default()).unwrap();
        (p, ctx.resolved_symbols(), ctx.symbol_units())
    };
    for options in [
        FieldResolutionOptions {
            max_symbols: seed_symbols,
            ..FieldResolutionOptions::default()
        },
        FieldResolutionOptions {
            max_symbol_units: seed_units + 5,
            ..FieldResolutionOptions::default()
        },
    ] {
        resolver.calls.set(0);
        let calls = resolver.calls.clone();
        let mut ctx = FieldResolutionContext::new_unnamed_modules(
            &registry,
            &mut heap,
            &mut resolver,
            options,
        )
        .unwrap();
        assert!(
            matches!(ctx.write_static(&p,own.class,own.refs[0],Value::Reference(value)),Err(E::Type(error)) if matches!(*error,TE::Context(C::ResourceLimit {resource:R::Symbols|R::SymbolUnits,..})))
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(
            ctx.inspect_static(&p, field(&registry, own.class, "r", "LTarget;")),
            Ok(Value::Reference(Reference::null()))
        );
    }
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    ctx.write_static(&p, own.class, own.refs[0], Value::Reference(value))
        .unwrap();
    let exact = ctx.lookup_steps() - before;
    let roots = ctx.static_roots(&p).unwrap().collect::<Vec<_>>();
    let before = ctx.lookup_steps();
    ctx.write_static(&p, own.class, own.refs[0], Value::Reference(value))
        .unwrap();
    assert_eq!(ctx.lookup_steps() - before, exact);
    assert_eq!(calls.get(), 1);
    assert_eq!(ctx.static_roots(&p).unwrap().collect::<Vec<_>>(), roots);
    assert_eq!(ctx.symbol_units(), seed_units + 6);
    assert_eq!(ctx.resolved_symbols(), seed_symbols + 1);
}

#[test]
fn a_shared_near_limit_descriptor_is_classified_once_atomically_without_owned_names() {
    let (mut registry, u) = setup();
    let descriptor = format!("L{};", "X".repeat(65_533));
    let names: Vec<_> = (0..100).map(|i| format!("r{i}")).collect();
    let fields: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), descriptor.as_str(), 9))
        .collect();
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &[],
    );
    let metadata = registry.class(own.class).unwrap();
    assert!(
        metadata
            .fields
            .iter()
            .all(|field| field.descriptor_index == metadata.fields[0].descriptor_index)
    );
    let mut heap = heap(&registry, &[own.class]);
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let seed = context(&registry, &mut heap, &mut resolver).lookup_steps();
    let limit = seed + 2 + 1 + descriptor.len() - 1;
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: limit,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        ctx.allocate_static_state(O::default()),
        Err(E::Context(C::ResourceLimit {
            resource: R::LookupSteps,
            limit,
            required: seed + 2 + 1 + descriptor.len()
        }))
    );
    assert_eq!(ctx.lookup_steps(), seed + 3);
    drop(ctx);
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    let p = ctx.allocate_static_state(O::default()).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 2 + 100 + 65_535);
    assert_eq!(p.slot_count(), 100);
    assert_eq!(ctx.static_roots(&p).unwrap().count(), 0);
    for field in ctx.static_fields(&p).unwrap() {
        assert_eq!(
            ctx.inspect_static(&p, *field),
            Ok(Value::Reference(Reference::null()))
        );
    }
    let before = ctx.lookup_steps();
    assert_eq!(ctx.allocate_static_state(O::default()), Ok(p));
    assert_eq!(ctx.lookup_steps(), before);
    assert_eq!(calls.get(), 0);
}

#[test]
fn deep_hierarchy_allocates_each_declared_static_once_and_drops_without_recursion() {
    const DEPTH: usize = 10_000;
    let mut registry = DefinitionRegistry::new(RegistryOptions {
        max_definitions: DEPTH + 1,
        ..RegistryOptions::default()
    })
    .unwrap();
    let l = registry.create_loader().unwrap();
    let object = object(&mut registry, l);
    let mut parent = String::from("java/lang/Object");
    let mut last = object;
    let mut first = None;
    for i in 0..DEPTH {
        let name = format!("C{i}");
        last = define(
            &mut registry,
            l,
            &name,
            Some(&parent),
            &[],
            0x21,
            &[("value", "J", 9)],
            &[],
        )
        .class;
        if first.is_none() {
            first = Some(last);
        }
        parent = name;
    }
    let plan = build_hierarchy(
        &registry,
        last,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 1,
            max_edges: DEPTH,
            max_declared_fields: DEPTH,
            max_instance_slots: 0,
        },
    )
    .unwrap();
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 1,
            max_layout_edges: DEPTH,
            max_layout_fields: 0,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &[&plan]).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = ctx.lookup_steps();
    let p = ctx.allocate_static_state(O::default()).unwrap();
    assert_eq!(ctx.lookup_steps() - before, 3 * DEPTH + 1);
    assert_eq!(
        (p.class_count(), p.slot_count(), p.payload_bytes()),
        (DEPTH + 1, DEPTH, DEPTH * BYTES)
    );
    let fields = ctx.static_fields(&p).unwrap();
    assert_eq!(fields[0], field(&registry, first.unwrap(), "value", "J"));
    assert_eq!(fields[DEPTH - 1], field(&registry, last, "value", "J"));
    assert_eq!(ctx.static_roots(&p).unwrap().count(), 0);
    assert_eq!(
        (
            ctx.heap().allocated_arrays(),
            ctx.heap().allocated_instances()
        ),
        (0, 0)
    );
    drop(ctx);
    drop(heap);
    drop(plan);
    drop(registry);
}

#[test]
fn static_allocations_and_writes_preserve_existing_exception_profile_and_instance_state() {
    let (mut registry, u) = setup();
    let throwable = define(
        &mut registry,
        u.bootstrap_loader,
        "java/lang/Throwable",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    );
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("x", "I", 9)],
        &[("C", "x", "I")],
    );
    let mut heap = bound_heap(&registry, u, &[throwable.class, own.class]);
    let value = heap.allocate_instance(throwable.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let ep = ctx.prepare_exception_profile(throwable.class).unwrap();
    let thrown = ctx.prepare_throw(&ep, value).unwrap();
    let p = ctx.allocate_static_state(O::default()).unwrap();
    ctx.check_exception_profile(&ep).unwrap();
    ctx.write_static(&p, own.class, own.refs[0], Value::Int(83))
        .unwrap();
    assert!(
        ctx.exception_handler_matches(&ep, own.class, 0, &thrown)
            .unwrap()
    );
    ctx.check_static_profile(&p).unwrap();
    assert_eq!(ctx.heap().instance_class(value), Ok(throwable.class));
}

#[test]
fn numeric_narrowing_covers_negative_masks_and_raw_ieee_payload_boundaries() {
    let (mut registry, u) = setup();
    let specs = [
        ("z", "Z"),
        ("b", "B"),
        ("c", "C"),
        ("s", "S"),
        ("f", "F"),
        ("d", "D"),
    ];
    let fields: Vec<_> = specs.iter().map(|(n, d)| (*n, *d, 9)).collect();
    let refs: Vec<_> = specs.iter().map(|(n, d)| ("C", *n, *d)).collect();
    let own = define(
        &mut registry,
        u.bootstrap_loader,
        "C",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &refs,
    );
    let mut heap = heap(&registry, &[own.class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    for (index, input, output) in [
        (0, Value::Int(-2), Value::Int(0)),
        (0, Value::Int(-1), Value::Int(1)),
        (1, Value::Int(-129), Value::Int(127)),
        (2, Value::Int(65536), Value::Int(0)),
        (3, Value::Int(-32769), Value::Int(32767)),
        (4, Value::Float(0x80000000), Value::Float(0x80000000)),
        (4, Value::Float(1), Value::Float(1)),
        (4, Value::Float(0xff800123), Value::Float(0xff800123)),
        (5, Value::Double(1), Value::Double(1)),
        (
            5,
            Value::Double(0x7ff8000000001234),
            Value::Double(0x7ff8000000001234),
        ),
    ] {
        ctx.write_static(&p, own.class, own.refs[index], input)
            .unwrap();
        assert_eq!(ctx.read_static(&p, own.class, own.refs[index]), Ok(output));
    }
}

#[test]
fn constant_values_and_a_real_putstatic_initializer_remain_unapplied_in_host_memory() {
    let (mut registry, u) = setup();
    let mut pool = Pool::default();
    let own = pool.class("Constants");
    let parent = pool.class("java/lang/Object");
    let numeric = pool.text("number");
    let int = pool.text("I");
    let constant_name = pool.text("ConstantValue");
    let number = pool.add(3, &73_i32.to_be_bytes());
    let text = pool.text("text");
    let string_desc = pool.text("Ljava/lang/String;");
    let contents = pool.text("unapplied");
    let string = pool.add(8, &words(&[contents]));
    let runtime = pool.text("runtime");
    let nat = pool.add(12, &words(&[runtime, int]));
    let reference = pool.add(9, &words(&[own, nat]));
    let init = pool.text("<clinit>");
    let void = pool.text("()V");
    let code_name = pool.text("Code");
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend(words(&[(pool.0.len() + 1) as u16]));
    bytes.extend(pool.0.into_iter().flatten());
    bytes.extend(words(&[0x21, own, parent, 0, 3]));
    for (name, desc, constant) in [(numeric, int, number), (text, string_desc, string)] {
        bytes.extend(words(&[0x19, name, desc, 1, constant_name]));
        bytes.extend(2_u32.to_be_bytes());
        bytes.extend(words(&[constant]));
    }
    bytes.extend(words(&[9, runtime, int, 0, 1, 8, init, void, 1, code_name]));
    let [hi, lo] = reference.to_be_bytes();
    let code = [0x10, 99, 0xb3, hi, lo, 0xb1];
    bytes.extend(18_u32.to_be_bytes());
    bytes.extend(words(&[1, 0]));
    bytes.extend(6_u32.to_be_bytes());
    bytes.extend(code);
    bytes.extend(words(&[0, 0, 0]));
    let class = registry
        .define(
            u.bootstrap_loader,
            &units("Constants"),
            &bytes,
            ParseOptions::default(),
        )
        .unwrap();
    let metadata = registry.class(class).unwrap();
    assert!(matches!(
        metadata.fields[0].attributes[0].body,
        crabspot_classfile::AttributeBody::ConstantValue { .. }
    ));
    let crabspot_classfile::AttributeBody::Code(body) = &metadata.methods[0].attributes[0].body
    else {
        panic!("missing Code");
    };
    assert_eq!(body.code, code);
    let mut heap = heap(&registry, &[class]);
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let p = ctx.allocate_static_state(O::default()).unwrap();
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, class, "number", "I")),
        Ok(Value::Int(0))
    );
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, class, "runtime", "I")),
        Ok(Value::Int(0))
    );
    assert_eq!(
        ctx.inspect_static(&p, field(&registry, class, "text", "Ljava/lang/String;")),
        Ok(Value::Reference(Reference::null()))
    );
    assert_eq!(ctx.static_roots(&p).unwrap().count(), 0);
}

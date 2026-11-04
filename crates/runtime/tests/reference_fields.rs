//! Checked reference field storage and atomic immediate child-edge enumeration.
use crabspot_classfile::ParseOptions;
use crabspot_runtime::{
    ClassId, DefinitionRegistry, FieldAccessError as E, FieldJavaError as J,
    FieldResolutionContext, FieldResolutionOptions, FieldResource as R, HeapEntryKind, HeapError,
    HeapOptions, HierarchyOptions, HierarchyPlan, HierarchyResolver, InstanceError,
    InstanceOptions, LoaderId, ManagedHeap, PrimitiveArrayKind, Reference, ReferenceFieldMismatch,
    ReferenceType as T, ReferenceTypeUniverse as U, RegistryOptions, ResolveError,
    SameLoaderResolver, TypeAccessError as TE, Value, ValueKind, build_hierarchy,
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
fn self_links_aliases_overwrites_null_clearing_and_cycles_preserve_immediate_slot_order() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let node = define(
        &mut registry,
        l,
        "Node",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("n", "I", 1),
            ("a", "LNode;", 1),
            ("b", "Ljava/lang/Object;", 1),
            ("c", "LNode;", 1),
        ],
        &[
            ("Node", "a", "LNode;"),
            ("Node", "b", "Ljava/lang/Object;"),
            ("Node", "c", "LNode;"),
        ],
    );
    let mut heap = bound_heap(&registry, u, &[node.class]);
    let a = heap.allocate_instance(node.class).unwrap();
    let b = heap.allocate_instance(node.class).unwrap();
    let c = heap.allocate_instance(node.class).unwrap();
    let before = (
        heap.allocated_instances(),
        heap.total_instance_slots(),
        heap.allocated_arrays(),
        heap.retained_layout_fields(),
    );
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (index, value) in [(0, a), (1, b), (2, b)] {
        ctx.write_field(node.class, node.refs[index], a, Value::Reference(value))
            .unwrap();
    }
    assert_eq!(
        children(ctx.heap(), a),
        [
            a.heap_ref().unwrap(),
            b.heap_ref().unwrap(),
            b.heap_ref().unwrap()
        ]
    );
    ctx.write_field(node.class, node.refs[0], a, Value::Reference(c))
        .unwrap();
    ctx.write_field(
        node.class,
        node.refs[1],
        a,
        Value::Reference(Reference::null()),
    )
    .unwrap();
    assert_eq!(
        children(ctx.heap(), a),
        [c.heap_ref().unwrap(), b.heap_ref().unwrap()]
    );
    ctx.write_field(node.class, node.refs[0], a, Value::Reference(b))
        .unwrap();
    ctx.write_field(node.class, node.refs[0], b, Value::Reference(a))
        .unwrap();
    assert_eq!(children(ctx.heap(), b), [a.heap_ref().unwrap()]);
    assert_eq!(
        ctx.read_field(node.class, node.refs[0], a),
        Ok(Value::Reference(b))
    );
    let id = field(&registry, node.class, "a", "LNode;");
    assert_eq!(
        ctx.heap_mut().put_field(a, id, Value::Reference(c)),
        Err(InstanceError::UnsupportedReferenceStore { field: id })
    );
    assert_eq!(
        before,
        (
            ctx.heap().allocated_instances(),
            ctx.heap().total_instance_slots(),
            ctx.heap().allocated_arrays(),
            ctx.heap().retained_layout_fields()
        )
    );
}

#[test]
fn inherited_hidden_fields_keep_owner_locations_and_failed_overwrites_are_atomic() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "Ljava/lang/Object;", 1)],
        &[],
    );
    let child = define(
        &mut registry,
        l,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[("r", "LChild;", 1)],
        &[
            ("Base", "r", "Ljava/lang/Object;"),
            ("Child", "r", "LChild;"),
        ],
    );
    let mut heap = bound_heap(&registry, u, &[child.class]);
    let receiver = heap.allocate_instance(child.class).unwrap();
    let other = heap.allocate_instance(base.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(
        child.class,
        child.refs[0],
        receiver,
        Value::Reference(other),
    )
    .unwrap();
    ctx.write_field(
        child.class,
        child.refs[1],
        receiver,
        Value::Reference(receiver),
    )
    .unwrap();
    let before = children(ctx.heap(), receiver);
    assert_eq!(
        before,
        [other.heap_ref().unwrap(), receiver.heap_ref().unwrap()]
    );
    assert_eq!(
        ctx.write_field(
            child.class,
            child.refs[1],
            receiver,
            Value::Reference(other)
        ),
        Err(mismatch(
            field(&registry, child.class, "r", "LChild;"),
            T::Class(base.class),
            T::Class(child.class)
        ))
    );
    assert_eq!(children(ctx.heap(), receiver), before);
    assert_eq!(
        ctx.read_field(child.class, child.refs[0], receiver),
        Ok(Value::Reference(other))
    );
}

#[test]
fn all_primitive_arrays_store_to_exact_arrays_and_core_supertypes_but_not_other_ranks() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let descriptors = [
        "[Z",
        "[B",
        "[C",
        "[S",
        "[I",
        "[J",
        "[F",
        "[D",
        "Ljava/lang/Object;",
        "Ljava/lang/Cloneable;",
        "Ljava/io/Serializable;",
        "[[I",
        "[Ljava/lang/Object;",
    ];
    let fields: Vec<_> = descriptors.iter().map(|desc| ("r", *desc, 1)).collect();
    let refs: Vec<_> = descriptors
        .iter()
        .map(|desc| ("Holder", "r", *desc))
        .collect();
    let holder = define(
        &mut registry,
        l,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &fields,
        &refs,
    );
    let kinds = [
        PrimitiveArrayKind::Boolean,
        PrimitiveArrayKind::Byte,
        PrimitiveArrayKind::Char,
        PrimitiveArrayKind::Short,
        PrimitiveArrayKind::Int,
        PrimitiveArrayKind::Long,
        PrimitiveArrayKind::Float,
        PrimitiveArrayKind::Double,
    ];
    let mut heap = bound_heap(&registry, u, &[holder.class]);
    let receiver = heap.allocate_instance(holder.class).unwrap();
    let arrays: Vec<_> = kinds
        .iter()
        .map(|kind| heap.allocate_primitive_array(*kind, 0).unwrap())
        .collect();
    let before = (
        heap.allocated_arrays(),
        heap.payload_bytes(),
        heap.total_instance_slots(),
    );
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    for (index, reference) in arrays.iter().copied().enumerate() {
        ctx.write_field(
            holder.class,
            holder.refs[index],
            receiver,
            Value::Reference(reference),
        )
        .unwrap();
        for target in [8, 9, 10] {
            ctx.write_field(
                holder.class,
                holder.refs[target],
                receiver,
                Value::Reference(reference),
            )
            .unwrap();
        }
        for target in [(index + 1) % 8, 11, 12] {
            let old = ctx
                .read_field(holder.class, holder.refs[target], receiver)
                .unwrap();
            let edges = children(ctx.heap(), receiver);
            assert!(matches!(
                ctx.write_field(
                    holder.class,
                    holder.refs[target],
                    receiver,
                    Value::Reference(reference)
                ),
                Err(E::IncompatibleReference(_))
            ));
            assert_eq!(
                ctx.read_field(holder.class, holder.refs[target], receiver),
                Ok(old)
            );
            assert_eq!(children(ctx.heap(), receiver), edges);
        }
        assert!(children(ctx.heap(), reference).is_empty());
    }
    assert_eq!(
        before,
        (
            ctx.heap().allocated_arrays(),
            ctx.heap().payload_bytes(),
            ctx.heap().total_instance_slots()
        )
    );
}

#[test]
fn public_fields_with_inaccessible_nominal_types_do_not_apply_cast_access_rules() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
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
    let holder = define(
        &mut registry,
        l,
        "q/Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "Lp/Hidden;", 1)],
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
        &[("q/Holder", "r", "Lp/Hidden;"), ("p/Hidden", "unused", "I")],
    );
    let mut heap = bound_heap(&registry, u, &[hidden.class, holder.class, caller.class]);
    let receiver = heap.allocate_instance(holder.class).unwrap();
    let value = heap.allocate_instance(hidden.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(
        caller.class,
        caller.refs[0],
        receiver,
        Value::Reference(value),
    )
    .unwrap();
    assert_eq!(
        ctx.read_field(caller.class, caller.refs[0], receiver),
        Ok(Value::Reference(value))
    );
    let crabspot_classfile::Constant::Fieldref { class_index, .. } = registry
        .class(caller.class)
        .unwrap()
        .constant_pool
        .get(caller.refs[1])
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        ctx.resolve_reference_type(caller.class, *class_index),
        Err(TE::Java(crabspot_runtime::TypeJavaError::IllegalAccess))
    );
}

#[test]
fn interface_nominal_assignment_accepts_implementations_and_rejects_unrelated_instances() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let interface = define(
        &mut registry,
        l,
        "I",
        Some("java/lang/Object"),
        &[],
        0x601,
        &[],
        &[],
    )
    .class;
    let good = define(
        &mut registry,
        l,
        "Good",
        Some("java/lang/Object"),
        &["I"],
        0x21,
        &[],
        &[],
    )
    .class;
    let holder = define(
        &mut registry,
        l,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LI;", 1)],
        &[("Holder", "r", "LI;")],
    );
    let mut heap = bound_heap(&registry, u, &[good, holder.class]);
    let receiver = heap.allocate_instance(holder.class).unwrap();
    let value = heap.allocate_instance(good).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(
        holder.class,
        holder.refs[0],
        receiver,
        Value::Reference(value),
    )
    .unwrap();
    assert_eq!(
        ctx.write_field(
            holder.class,
            holder.refs[0],
            receiver,
            Value::Reference(receiver)
        ),
        Err(mismatch(
            field(&registry, holder.class, "r", "LI;"),
            T::Class(holder.class),
            T::Class(interface)
        ))
    );
    assert_eq!(children(ctx.heap(), receiver), [value.heap_ref().unwrap()]);
}

#[test]
fn inherited_descriptor_uses_declaring_loader_and_cross_loader_nominal_boundary_remains() {
    let (mut registry, u) = setup();
    let first = u.bootstrap_loader;
    let second = registry.create_loader().unwrap();
    let same1 = define(
        &mut registry,
        first,
        "Same",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let base = define(
        &mut registry,
        first,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LSame;", 1), ("a", "[I", 1)],
        &[("Base", "r", "LSame;"), ("Base", "a", "[I")],
    );
    let same2 = define(
        &mut registry,
        second,
        "Same",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let child = define(
        &mut registry,
        second,
        "Child",
        Some("Base"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let caller = define(
        &mut registry,
        second,
        "Caller",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("Base", "r", "LSame;"), ("Base", "a", "[I")],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .selected
        .insert((second, units("Base")), base.class);
    let roots = [
        u.cloneable,
        u.serializable,
        same1,
        same2,
        child,
        caller.class,
    ];
    let plans: Vec<_> = roots
        .iter()
        .map(|class| {
            build_hierarchy(
                &registry,
                *class,
                &mut resolver,
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
    heap.bind_reference_types(&registry, u).unwrap();
    let receiver = heap.allocate_instance(child).unwrap();
    let value1 = heap.allocate_instance(same1).unwrap();
    let value2 = heap.allocate_instance(same2).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(base.class, base.refs[0], receiver, Value::Reference(value1))
        .unwrap();
    assert_eq!(
        ctx.write_field(base.class, base.refs[0], receiver, Value::Reference(value2)),
        Err(mismatch(
            field(&registry, base.class, "r", "LSame;"),
            T::Class(same2),
            T::Class(same1)
        ))
    );
    for receiver in [Reference::null(), receiver] {
        for value in [
            Value::Reference(Reference::null()),
            Value::Reference(value1),
            Value::Int(0),
        ] {
            assert_eq!(
                ctx.write_field(caller.class, caller.refs[0], receiver, value),
                Err(E::UnsupportedLoadingConstraint {
                    origin: caller.class,
                    owner: base.class
                })
            );
            assert_eq!(
                ctx.read_field(caller.class, caller.refs[0], receiver),
                Err(E::UnsupportedLoadingConstraint {
                    origin: caller.class,
                    owner: base.class
                })
            );
        }
    }
    ctx.write_field(
        caller.class,
        caller.refs[1],
        receiver,
        Value::Reference(array),
    )
    .unwrap();
    assert_eq!(
        ctx.read_field(caller.class, caller.refs[1], receiver),
        Ok(Value::Reference(array))
    );
}

#[test]
fn static_final_access_receiver_and_computational_kind_failures_precede_value_types() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let owner = define(
        &mut registry,
        l,
        "Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[
            ("r", "Ljava/lang/Object;", 1),
            ("f", "Ljava/lang/Object;", 0x11),
            ("s", "Ljava/lang/Object;", 9),
            ("p", "Ljava/lang/Object;", 2),
            ("n", "I", 1),
        ],
        &[
            ("Owner", "r", "Ljava/lang/Object;"),
            ("Owner", "f", "Ljava/lang/Object;"),
            ("Owner", "s", "Ljava/lang/Object;"),
            ("Owner", "n", "I"),
        ],
    );
    let peer = define(
        &mut registry,
        l,
        "Peer",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[("Owner", "p", "Ljava/lang/Object;")],
    );
    let mut heap = bound_heap(&registry, u, &[owner.class, peer.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let array = heap
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut foreign = ManagedHeap::new(HeapOptions::default()).unwrap();
    let foreign_ref = foreign
        .allocate_primitive_array(PrimitiveArrayKind::Int, 0)
        .unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(
        owner.class,
        owner.refs[0],
        receiver,
        Value::Reference(receiver),
    )
    .unwrap();
    let old = children(ctx.heap(), receiver);
    for receiver in [Reference::null(), receiver] {
        for (index, error) in [(1, J::IllegalAccess), (2, J::IncompatibleClassChange)] {
            assert_eq!(
                ctx.write_field(
                    owner.class,
                    owner.refs[index],
                    receiver,
                    Value::Reference(foreign_ref)
                ),
                Err(E::Java(error))
            );
        }
    }
    assert_eq!(
        ctx.write_field(
            peer.class,
            peer.refs[0],
            Reference::null(),
            Value::Reference(foreign_ref)
        ),
        Err(E::UnsupportedNestAccess {
            origin: peer.class,
            owner: owner.class
        })
    );
    assert_eq!(
        ctx.write_field(
            owner.class,
            owner.refs[0],
            Reference::null(),
            Value::Reference(foreign_ref)
        ),
        Err(E::Java(J::NullPointer))
    );
    assert_eq!(
        ctx.write_field(
            owner.class,
            owner.refs[0],
            foreign_ref,
            Value::Reference(foreign_ref)
        ),
        Err(E::Instance(InstanceError::Heap(HeapError::ForeignHeap)))
    );
    assert_eq!(
        ctx.write_field(
            owner.class,
            owner.refs[0],
            array,
            Value::Reference(foreign_ref)
        ),
        Err(E::Instance(InstanceError::Heap(
            HeapError::WrongEntryKind {
                expected: HeapEntryKind::Instance,
                actual: HeapEntryKind::PrimitiveArray
            }
        )))
    );
    for (index, value, expected, actual) in [
        (0, Value::Int(0), ValueKind::Reference, ValueKind::Int),
        (
            3,
            Value::Reference(foreign_ref),
            ValueKind::Int,
            ValueKind::Reference,
        ),
    ] {
        assert_eq!(
            ctx.write_field(owner.class, owner.refs[index], receiver, value),
            Err(E::Instance(InstanceError::Heap(
                HeapError::ValueKindMismatch { expected, actual }
            )))
        );
    }
    assert_eq!(
        ctx.write_field(
            owner.class,
            owner.refs[0],
            receiver,
            Value::Reference(foreign_ref)
        ),
        Err(E::ReferenceType(Box::new(TE::Heap(HeapError::ForeignHeap))))
    );
    assert_eq!(children(ctx.heap(), receiver), old);
    for index in [0, u16::MAX, owner.wrong_constant] {
        assert_eq!(
            ctx.write_field(
                owner.class,
                index,
                Reference::null(),
                Value::Reference(foreign_ref)
            ),
            Err(E::InvalidConstant {
                origin: owner.class,
                index
            })
        );
    }
}

#[test]
fn null_numeric_and_wrong_kind_stores_keep_old_unbound_universe_behavior() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let owner = define(
        &mut registry,
        l,
        "Owner",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LMissing;", 1), ("n", "B", 1)],
        &[("Owner", "r", "LMissing;"), ("Owner", "n", "B")],
    );
    let mut heap = heap(&registry, &[owner.class]);
    let receiver = heap.allocate_instance(owner.class).unwrap();
    let mut resolver = Resolver::default();
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(
        owner.class,
        owner.refs[0],
        receiver,
        Value::Reference(Reference::null()),
    )
    .unwrap();
    ctx.write_field(owner.class, owner.refs[1], receiver, Value::Int(255))
        .unwrap();
    assert_eq!(
        ctx.read_field(owner.class, owner.refs[1], receiver),
        Ok(Value::Int(-1))
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(
        ctx.write_field(owner.class, owner.refs[0], receiver, Value::Int(0)),
        Err(E::Instance(InstanceError::Heap(
            HeapError::ValueKindMismatch {
                expected: ValueKind::Reference,
                actual: ValueKind::Int
            }
        )))
    );
    assert_eq!(
        ctx.write_field(
            owner.class,
            owner.refs[0],
            receiver,
            Value::Reference(receiver)
        ),
        Err(E::ReferenceType(Box::new(TE::MissingUniverse)))
    );
    assert!(children(ctx.heap(), receiver).is_empty());
}

#[test]
fn exact_descriptor_and_cache_hit_budgets_preserve_old_contents_on_late_failure() {
    let (mut registry, u) = setup();
    let node = define(
        &mut registry,
        u.bootstrap_loader,
        "Node",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LNode;", 1)],
        &[("Node", "r", "LNode;")],
    );
    let mut heap = bound_heap(&registry, u, &[node.class]);
    let a = heap.allocate_instance(node.class).unwrap();
    let b = heap.allocate_instance(node.class).unwrap();
    let mut resolver = SameLoaderResolver;
    for limit in [21, 22, 36, 37, usize::MAX] {
        heap.put_field(
            a,
            field(&registry, node.class, "r", "LNode;"),
            Value::Reference(Reference::null()),
        )
        .unwrap();
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
        assert_eq!(ctx.lookup_steps(), 7);
        let first = ctx.write_field(node.class, node.refs[0], a, Value::Reference(a));
        if limit == 21 {
            assert_eq!(
                first,
                Err(E::ReferenceType(Box::new(TE::Context(E::ResourceLimit {
                    resource: R::LookupSteps,
                    limit,
                    required: 22
                }))))
            );
            assert!(children(ctx.heap(), a).is_empty());
            continue;
        }
        first.unwrap();
        assert_eq!(ctx.lookup_steps(), 22);
        if limit == 22 {
            continue;
        }
        let second = ctx.write_field(node.class, node.refs[0], a, Value::Reference(b));
        if limit == 36 {
            assert_eq!(
                second,
                Err(E::ReferenceType(Box::new(TE::Context(E::ResourceLimit {
                    resource: R::LookupSteps,
                    limit,
                    required: 37
                }))))
            );
            assert_eq!(children(ctx.heap(), a), [a.heap_ref().unwrap()]);
        } else {
            second.unwrap();
            assert_eq!(ctx.lookup_steps(), 37);
            assert_eq!(children(ctx.heap(), a), [b.heap_ref().unwrap()]);
        }
    }
}

#[test]
fn descriptor_binding_callback_symbol_and_late_work_failures_leave_old_edges_intact() {
    let (mut registry, u) = setup();
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
    )
    .class;
    let holder = define(
        &mut registry,
        u.bootstrap_loader,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LTarget;", 1)],
        &[("Holder", "r", "LTarget;")],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .selected
        .insert((u.bootstrap_loader, units("Target")), target);
    let roots = [u.cloneable, u.serializable, holder.class, target];
    let plans: Vec<_> = roots
        .iter()
        .map(|class| {
            build_hierarchy(
                &registry,
                *class,
                &mut resolver,
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
    heap.bind_reference_types(&registry, u).unwrap();
    let receiver = heap.allocate_instance(holder.class).unwrap();
    let a = heap.allocate_instance(target).unwrap();
    let b = heap.allocate_instance(target).unwrap();
    {
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        ctx.write_field(holder.class, holder.refs[0], receiver, Value::Reference(a))
            .unwrap();
        assert_eq!(ctx.lookup_steps(), 26);
    }
    let before = children(&heap, receiver);
    let (symbols, name_units) = {
        let ctx = context(&registry, &mut heap, &mut resolver);
        (ctx.resolved_symbols(), ctx.symbol_units())
    };
    assert_eq!((symbols, name_units), (6, 83));
    let (_, foreign) = setup();
    for response in [
        Err(ResolveError::new("temporary descriptor lookup failure")),
        Ok(None),
        Ok(Some(holder.class)),
        Ok(Some(foreign.object)),
    ] {
        resolver.responses.push_back(response);
        let mut ctx = context(&registry, &mut heap, &mut resolver);
        assert!(
            matches!(ctx.write_field(holder.class,holder.refs[0],receiver,Value::Reference(b)),Err(E::ReferenceType(error)) if matches!(*error,TE::Context(_)))
        );
        assert_eq!(children(ctx.heap(), receiver), before);
        assert_eq!(ctx.resolved_symbols(), symbols);
    }
    for (options, expected) in [
        (
            FieldResolutionOptions {
                max_symbols: symbols,
                ..FieldResolutionOptions::default()
            },
            E::ResourceLimit {
                resource: R::Symbols,
                limit: symbols,
                required: symbols + 1,
            },
        ),
        (
            FieldResolutionOptions {
                max_symbol_units: name_units + 5,
                ..FieldResolutionOptions::default()
            },
            E::ResourceLimit {
                resource: R::SymbolUnits,
                limit: name_units + 5,
                required: name_units + 6,
            },
        ),
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
        assert_eq!(
            ctx.write_field(holder.class, holder.refs[0], receiver, Value::Reference(b)),
            Err(E::ReferenceType(Box::new(TE::Context(expected))))
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(children(ctx.heap(), receiver), before);
    }
    let mut ctx = FieldResolutionContext::new_unnamed_modules(
        &registry,
        &mut heap,
        &mut resolver,
        FieldResolutionOptions {
            max_lookup_steps: 25,
            ..FieldResolutionOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        ctx.write_field(holder.class, holder.refs[0], receiver, Value::Reference(b)),
        Err(E::ReferenceType(Box::new(TE::Context(E::ResourceLimit {
            resource: R::LookupSteps,
            limit: 25,
            required: 26
        }))))
    );
    assert_eq!(ctx.resolved_symbols(), symbols + 1);
    assert_eq!(children(ctx.heap(), receiver), before);
}

#[test]
fn missing_or_unadmitted_descriptor_targets_are_host_errors_but_null_still_clears() {
    let (mut registry, u) = setup();
    let l = u.bootstrap_loader;
    let target = define(
        &mut registry,
        l,
        "Target",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let holder = define(
        &mut registry,
        l,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LTarget;", 1), ("m", "LMissing;", 1)],
        &[("Holder", "r", "LTarget;"), ("Holder", "m", "LMissing;")],
    );
    let mut heap = bound_heap(&registry, u, &[holder.class]);
    let receiver = heap.allocate_instance(holder.class).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    assert_eq!(
        ctx.write_field(
            holder.class,
            holder.refs[0],
            receiver,
            Value::Reference(receiver)
        ),
        Err(E::ReferenceType(Box::new(TE::Context(E::Instance(
            InstanceError::LayoutMissing { class: target }
        )))))
    );
    assert!(
        matches!(ctx.write_field(holder.class,holder.refs[1],receiver,Value::Reference(receiver)),Err(E::ReferenceType(error)) if matches!(*error,TE::Context(E::MissingDefinition { .. })))
    );
    for index in &holder.refs {
        ctx.write_field(
            holder.class,
            *index,
            receiver,
            Value::Reference(Reference::null()),
        )
        .unwrap();
    }
    assert!(children(ctx.heap(), receiver).is_empty());
}

#[test]
fn unicode_descriptor_aliases_use_exact_borrowed_units_without_duplicate_descriptor_symbols() {
    let (mut registry, u) = setup();
    let second = registry.create_loader().unwrap();
    let name = "p/𝄞";
    let descriptor = "Lp/𝄞;";
    let target = define(
        &mut registry,
        second,
        name,
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let holder = define(
        &mut registry,
        u.bootstrap_loader,
        "Holder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("réf", descriptor, 1)],
        &[("Holder", "réf", descriptor)],
    );
    let mut resolver = Resolver::default();
    resolver
        .selected
        .insert((second, units("java/lang/Object")), u.object);
    resolver
        .selected
        .insert((u.bootstrap_loader, units(name)), target);
    let roots = [u.cloneable, u.serializable, target, holder.class];
    let plans: Vec<_> = roots
        .iter()
        .map(|class| {
            build_hierarchy(
                &registry,
                *class,
                &mut resolver,
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
    heap.bind_reference_types(&registry, u).unwrap();
    let receiver = heap.allocate_instance(holder.class).unwrap();
    let value = heap.allocate_instance(target).unwrap();
    resolver.calls.set(0);
    let calls = resolver.calls.clone();
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    let before = (ctx.resolved_symbols(), ctx.symbol_units());
    for _ in 0..2 {
        let steps = ctx.lookup_steps();
        ctx.write_field(
            holder.class,
            holder.refs[0],
            receiver,
            Value::Reference(value),
        )
        .unwrap();
        assert_eq!(
            ctx.lookup_steps() - steps,
            descriptor.encode_utf16().count() + 9
        );
    }
    assert_eq!(
        (ctx.resolved_symbols(), ctx.symbol_units()),
        (before.0 + 1, before.1 + name.encode_utf16().count())
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(children(ctx.heap(), receiver), [value.heap_ref().unwrap()]);
}

#[test]
fn deep_assignment_and_maximum_borrowed_reference_descriptor_store_without_recursion() {
    const DEPTH: usize = 10_000;
    let (mut registry, u) = setup_options(RegistryOptions {
        max_definitions: DEPTH + 6,
        ..RegistryOptions::default()
    });
    let l = u.bootstrap_loader;
    let base = define(
        &mut registry,
        l,
        "Base",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", "LBase;", 1)],
        &[],
    )
    .class;
    let mut previous = String::from("Base");
    let mut last = None;
    for index in 0..DEPTH {
        let name = format!("C{index}");
        last = Some(define(
            &mut registry,
            l,
            &name,
            Some(&previous),
            &[],
            0x21,
            &[],
            if index + 1 == DEPTH {
                &[("Base", "r", "LBase;")]
            } else {
                &[]
            },
        ));
        previous = name;
    }
    let last = last.unwrap();
    let leaf = "X".repeat(65_533);
    let descriptor = format!("L{leaf};");
    let target = define(
        &mut registry,
        l,
        &leaf,
        Some("java/lang/Object"),
        &[],
        0x21,
        &[],
        &[],
    )
    .class;
    let holder = define(
        &mut registry,
        l,
        "LongHolder",
        Some("java/lang/Object"),
        &[],
        0x21,
        &[("r", &descriptor, 1)],
        &[("LongHolder", "r", &descriptor)],
    );
    let deep = build_hierarchy(
        &registry,
        last.class,
        &mut SameLoaderResolver,
        HierarchyOptions {
            max_classes: DEPTH + 2,
            max_edges: DEPTH + 1,
            max_declared_fields: 1,
            max_instance_slots: 1,
        },
    )
    .unwrap();
    let plans = [
        plan(&registry, u.cloneable),
        plan(&registry, u.serializable),
        plan(&registry, target),
        plan(&registry, holder.class),
        deep,
    ];
    let mut heap = ManagedHeap::new_with_instances(
        HeapOptions::default(),
        InstanceOptions {
            max_layout_classes: DEPTH + 6,
            max_layout_edges: DEPTH + 5,
            ..InstanceOptions::default()
        },
    )
    .unwrap();
    heap.admit_instance_layouts(&registry, &plans.iter().collect::<Vec<_>>())
        .unwrap();
    heap.bind_reference_types(&registry, u).unwrap();
    let receiver = heap.allocate_instance(last.class).unwrap();
    let holder_ref = heap.allocate_instance(holder.class).unwrap();
    let value = heap.allocate_instance(target).unwrap();
    let mut resolver = SameLoaderResolver;
    let mut ctx = context(&registry, &mut heap, &mut resolver);
    ctx.write_field(
        last.class,
        last.refs[0],
        receiver,
        Value::Reference(receiver),
    )
    .unwrap();
    assert_eq!(
        children(ctx.heap(), receiver),
        [receiver.heap_ref().unwrap()]
    );
    let before = ctx.lookup_steps();
    ctx.write_field(
        holder.class,
        holder.refs[0],
        holder_ref,
        Value::Reference(value),
    )
    .unwrap();
    assert_eq!(ctx.lookup_steps() - before, 65_535 + 9);
    assert_eq!(
        children(ctx.heap(), holder_ref),
        [value.heap_ref().unwrap()]
    );
    assert!(registry.class(base).is_ok());
    drop(ctx);
    drop(heap);
    drop(plans);
    drop(registry);
}

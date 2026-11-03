# Runtime hierarchy and layout increment

Status: implementation authorized by `runtime-registry-review.md` on 2026-11-03, using the coordination API below. This does not authorize claims of complete Java linking or milestone completion.

## Purpose and boundary

Compute a checked hierarchy and runtime field-layout plan from registered definitions. This supplies actual type relationships and owner-qualified field locations for later execution services. It is not a full Java linker, verifier, initialization engine or application entry point. The plan records which checks it proves, and explicitly leaves bytecode verification and access/resolution obligations outstanding.

Use the runtime registry's scoped identities. An explicit resolver maps a symbolic class name in a defining-loader context to an already registered ClassId. It may implement an explicitly supplied delegation policy; do not infer parent relationships from numeric loader IDs. Every returned handle must belong to the registry and its declared name must match the request. A returned class may have a different defining loader because delegation is legitimate. Cache each context/name result consistently for this plan.

Do not synthesize java/lang/Object or turn missing superclass data into an empty successful layout. Test-only structural classfiles can represent a supplied root fixture; there is no built-in production fallback. The actual source adapter remains responsible for supplying real class definitions when execution needs them.

## Graph checks

Resolve direct superclass and interface references from their owning definition's loader context. Validate class versus interface roles, final superclass prohibition, and class/interface cycles using an explicit bounded traversal. Retain ClassId identity throughout; equal binary names in different defining loaders are not interchangeable. Parse-time name/flag checks do not substitute for checking the referenced definition.

Do not make a missing declaration appear inherited by returning the first method with a matching name. This increment computes the listed structural relationships and layouts; complete class derivation, sealed/module/package checks, final-method overriding rules, method/interface resolution, access checks, default-method selection and dispatch each remain named future responsibilities. A successful plan is not proof that the JVM would accept the entire hierarchy for loading.

The output records resolved direct relationships, useful transitive relationships and deterministic field-layout data. It is an immutable result associated with the registry definitions it examined. Registration alone continues to be unverified/unlinked; consuming a hierarchy plan must not manufacture a verification proof or initialized state.

## Layout rules

Use owner-qualified FieldId values. Instance layout retains inherited instance fields followed by the current class's instance fields in a deterministic implementation order. A hidden field is a distinct storage location because its declaring owner differs. Static fields are stored only by their declaring class; do not copy them into each subclass's static storage.

Represent inherited layout through a superclass link and inherited-slot count, storing each definition's own instance/static FieldId vectors only once. Do not materialize a complete copied ancestor field vector for every node. Field-slot queries walk the bounded parent chain and use the declaring owner's prefix count. Interfaces have zero instance slots; their Object classfile reference is a graph dependency, not inherited instance storage for an interface.

The initial layout unit may be a logical typed-value slot rather than a byte address. A Long or Double field then occupies one logical value location, unlike the two JVM local/operand slots required by an execution frame. Keep that distinction explicit. Reference field descriptors are recorded even while heap/reference execution is unavailable; do not substitute integer zero as a Java reference value.

Layout computation does not initialize static values. Eventual preparation creates Java default values. ConstantValue assignment must occur as part of the initialization protocol before the corresponding class initializer, rather than being incorrectly described as general preparation. This ordering is defined by [JVMS preparation](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.2) and [initialization](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.5).

## Bounds and acceptance

Apply explicit policies to visited definitions, graph edges and aggregate retained layout entries. Check before expanding worklists or copying inherited layouts. The implementation must not claim that a node limit alone bounds all layout copies in a deep hierarchy. Report resource failures separately from invalid relationships and missing definitions.

## Coordination API

Add `build_hierarchy(registry: &DefinitionRegistry, root: ClassId, resolver: &mut impl HierarchyResolver, options: HierarchyOptions) -> Result<HierarchyPlan, HierarchyError>`; support trait objects with the equivalent generic `?Sized` form if convenient. `HierarchyResolver::resolve(&mut self, registry: &DefinitionRegistry, initiating_loader: LoaderId, name: &[u16]) -> Result<Option<ClassId>, ResolveError>` only selects already registered definitions. A `SameLoaderResolver` performs explicit same-loader registry lookup; it has no implicit delegation. Tests can supply a resolver with an explicit parent mapping. Resolver failures and missing names remain distinct host errors.

`HierarchyOptions` contains usize fields `max_classes`, `max_edges`, `max_declared_fields`, `max_instance_slots`, defaulting respectively to 1,024, 8,192, 65,536 and 65,536. Count unique visited definitions, all direct superclass/interface edges, all retained declared field handles including statics, and total logical instance slots per individual class. These are independent policies; no large allowance triggers eager allocation. Borrow registry name slices for the temporary context/name cache instead of cloning long names repeatedly. No cache or plan query mutates the registry or creates classes.

`HierarchyPlan` exposes `root() -> ClassId`, `classes() -> &[ClassId]` in deterministic dependency-first order, and `layout(ClassId) -> Option<&ClassLayout>`. `ClassLayout` exposes `class_id()`, `direct_superclass() -> Option<ClassId>`, `direct_interfaces() -> &[ClassId]`, `declared_instance_fields() -> &[FieldId]`, `declared_static_fields() -> &[FieldId]`, `inherited_instance_slots() -> usize`, and `instance_slots() -> usize`. Keep its state private.

Provide `HierarchyPlan::instance_field_slot(receiver: ClassId, field: FieldId) -> Option<usize>` and `declared_static_field_slot(field: FieldId) -> Option<usize>`. A query succeeds only for an applicable retained owner/member relationship; unknown, foreign or unrelated handles return None. Slots are logical typed-value locations, not byte offsets. Static lookup never searches an ancestor implicitly. There is no assignability, access-check or virtual-dispatch API in this increment.

Structured errors cover registry/handle failures, resolver failures, missing definitions, returned-name mismatch, wrong superclass/interface kind, final superclass, cycles and resource exhaustion. Include the originating ClassId and symbolic reference where useful. Errors are not fabricated Java linkage exception objects. All successful state is published only in the returned immutable plan; a failed build leaves registry definitions untouched.

Tests cover genuine compiled inheritance/interface fixtures, isolated defining-loader identities, explicit delegated resolution, wrong-registry/name resolver results, missing dependencies, final superclass rejection, class/interface mismatch, cycles, inherited field hiding and static ownership. Include deep synthetic graphs demonstrating nonrecursive traversal and layout-entry limits. No source callback or missing definition may silently invent successful ancestry.

After this plan is reviewed, introduce actual static storage and a separately specified initializer execution bridge, then managed heap operations and exception/monitor services. General Java startup remains gated on the required linkage, verification and lifecycle responsibilities. M2 and M3 acceptance do not follow merely from computing a layout.

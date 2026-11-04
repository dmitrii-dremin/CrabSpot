# Reference types, checkcast and instanceof

Status: implemented and approved by the technical director on 2026-11-04; see `m2-reference-types-review.md` for frozen-source review and final integrated evidence. The contract below defines the accepted bounded scope. Fresh hosted validation is recorded separately. Full M2 remains open.

## Scope and ownership

Add a reusable reference-type relation and execute checkcast (c0) and instanceof (c1) in the existing runtime-aware engine. Sources are existing ordinary instances and primitive arrays; targets include nominal classes/interfaces and structural array types of any valid rank. Preserve numeric/void signatures, host-selected ordinary class roots, the existing frame loop and public execution entry points.

No reference-array allocation, non-null reference-field stores, reference arguments/results, constructors, initialization, class mirrors, array method dispatch, verifier, named-module/nest access, exception-object/handler dispatch or collection is included. The runtime worker owns runtime production and synthetic tests; the interpreter worker owns its production and synthetic tests; the coordinator owns genuine fixtures, oracle, integration and final checks. Separate worktrees remain mandatory. No NanoSpec.

## Exact public API

Export these runtime types (derive Clone, Copy, Debug, Eq and PartialEq; ReferenceType and ArrayElement may also derive Hash):

```rust
pub struct ReferenceTypeUniverse {
    pub bootstrap_loader: LoaderId,
    pub object: ClassId,
    pub cloneable: ClassId,
    pub serializable: ClassId,
}
pub enum ArrayElement {
    Primitive(PrimitiveArrayKind),
    Class(ClassId),
}
pub enum ReferenceType {
    Class(ClassId), // a nominal class OR interface
    Array {
        dimensions: u8,
        element: ArrayElement, // non-array leaf
        defining_loader: LoaderId,
    },
}
pub enum TypeJavaError { IllegalAccess, ClassCast }

impl ManagedHeap {
    pub fn bind_reference_types(
        &mut self,
        registry: &DefinitionRegistry,
        universe: ReferenceTypeUniverse,
    ) -> Result<(), TypeAccessError>;
    pub fn reference_type_universe(&self) -> Option<ReferenceTypeUniverse>;
}
impl FieldResolutionContext<'_> {
    pub fn resolve_reference_type(
        &mut self, origin: ClassId, index: u16,
    ) -> Result<ReferenceType, TypeAccessError>;
    pub fn reference_type(
        &mut self, reference: Reference,
    ) -> Result<Option<ReferenceType>, TypeAccessError>;
    pub fn is_assignable(
        &mut self, source: ReferenceType, target: ReferenceType,
    ) -> Result<bool, TypeAccessError>;
    pub fn check_cast(
        &mut self, origin: ClassId, index: u16, reference: Reference,
    ) -> Result<Reference, TypeAccessError>;
    pub fn instance_of(
        &mut self, origin: ClassId, index: u16, reference: Reference,
    ) -> Result<bool, TypeAccessError>;
}
```

TypeAccessError is Clone/Debug/Eq/PartialEq and implements Display/Error:

```rust
pub enum TypeAccessError {
    Java(TypeJavaError),
    Context(FieldAccessError),
    Heap(HeapError),
    MissingUniverse,
    UniverseConflict,
    InvalidUniverse(&'static str),
    InvalidType(&'static str),
    InvalidConstant { origin: ClassId, index: u16 },
}
```

Wrap registry/instance/resolver/budget errors through Context using the existing checked errors; preserve Error::source. Java categories are explicit, never inferred from a wrapped Context Java error. Add InstanceError::ReferenceUniverseMismatch { class: ClassId, expected_object: ClassId } for a new layout inconsistent with an already bound universe. This is a host admission failure.

The structural ReferenceType is a checked service value, not a new ClassId or heap-reference issuer. Public construction does not bypass validation: every public relation call checks its nominal handles, array rank and defining loader. A zero array rank is InvalidType. A nominal leaf's loader must equal its actual defining loader; a primitive leaf's loader must equal the bound bootstrap loader. Thus equivalent array leaves/ranks from different defining loaders cannot accidentally compare as equal.

## Explicit heap universe

The caller designates the bootstrap LoaderId explicitly. Bind only after instance layouts have been admitted to the same registry. Validate all four identities through registry APIs. Object must be the named public ordinary java/lang/Object with no superclass; Cloneable and Serializable must be the named public abstract interfaces java/lang/Cloneable and java/io/Serializable, with the selected Object dependency and no direct superinterfaces. All three must have the designated defining loader and be admitted. Names are validation, never the source of bootstrap identity. This is a host-provided structural library profile, not proof of complete genuine platform definitions.

The admitted graph must belong to one selected Object universe: every retained node with no superclass is exactly that Object. Existing checked acyclic dependencies then ensure every retained class/interface ultimately belongs to it. Scan existing nodes once when binding; do not walk each ancestry independently. Validate completely before publishing the binding. Failed binding changes no option, layout, object, array or counter. Repeating an identical binding succeeds; a different binding returns UniverseConflict without replacement. No reset API is provided.

After binding, admission of additional layout plans must enforce the same root identity before any batch commit. A new incompatible root returns ReferenceUniverseMismatch atomically. Keep the existing multi-plan conflict/dedup/quota behavior. No new graph copy is needed. The binding scan and extra admission checks are bounded by existing admitted/staged class limits and need no additional allocation; they are explicit host configuration, outside a context's lookup counter.

Primitive arrays allocated before binding keep their handles, contents and counters. Their runtime type becomes rank-one with the exact primitive kind and the bound bootstrap loader when queried through the context. Existing registry-free allocation/load/store/length APIs remain usable without a universe. Ordinary non-cast execution also remains usable without it.

Extend context invalidation to compare the captured universe as well as heap identity and admitted-class count. Binding a previously unbound heap through heap_mut invalidates that existing context; recreate it afterward. An identical rebinding does not invalidate. Heap replacement and successful additional layout admission retain their prior invalidation behavior. Failed binding/admission does not invalidate.

## Origin-bound targets and identity relation

Nominal targets need not be receiver ancestors. Admit their hierarchy plans as additional roots before creating the context, including interfaces or abstract types that will never be allocated. Resolution uses the current origin's defining loader, the same UTF-16 binding cache, and the same explicit resolver. A registered but unadmitted result remains a structured LayoutMissing host error. No automatic loading, extra hidden graph or implicit layout admission is introduced.

For CONSTANT_Class names beginning with '[', parse the rank and primitive/nominal leaf without constructing a recursive descriptor tree. Resolve nominal leaf names through the origin's loader. Preserve borrowed UTF-16 slices from immutable CP storage, including array-leaf substrings; a narrow slice-based shared binding helper may replace the current JavaString-only helper, preserving all old semantics and budget counts. Do not allocate permanent duplicate names merely to resolve array leaves. Error diagnostics may own names as before.

Target access uses the existing unnamed-module policy: public nominal types or the same runtime package, where loader identity is part of the package. Apply nominal-leaf access to reference-array targets; primitive-array targets are accessible. Access failure is TypeJavaError::IllegalAccess. Missing definitions, resolver failures and unsupported host metadata remain host errors. No new field/method loading-constraint support follows from resolving a cast target.

Nominal assignment uses class ancestry and interface membership by exact ClassId. An interface type is assignable to the selected Object, itself and its superinterfaces; it is not a class subclass for protected access. Arrays are assignable to the selected Object, Cloneable and Serializable, and use component covariance; primitive components must match exactly. Apply component reduction iteratively, including unequal ranks. In particular int[][] is assignable to Object[], while int[] is not; int[] and boolean[] remain distinct. Ordinary instances cannot be arrays. Keep the protected-access subclass helper unchanged. These rules follow [JVMS checkcast/instanceof](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.checkcast); array identity/access follows [JVMS 5.3.3](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.3.3).

reference_type returns None for null after checking context validity, without requiring a universe. For non-null, require a universe, validate heap ownership/index/generation, then inspect the entry kind and checked admitted ClassId or primitive kind. is_assignable requires a universe even for identical supplied types, and validates both types before any identity shortcut. It is a type relation, not an access check: access belongs to origin-bound resolution.

## Instruction and error order

For check_cast and instance_of: check unchanged context, registered/admitted origin, then CP index/Class tag/Utf8 shape. Invalid CP is InvalidConstant even with null. Null returns unchanged/null or false immediately: no target resolution, access check, universe requirement, symbol-cache insertion or resolver callback. This is deliberately instruction-specific, consistent with [JVMS checkcast](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.checkcast) and [instanceof](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.instanceof).

For non-null, require the universe and validate the managed source handle before resolving/access-checking the target. Then apply the relation. check_cast returns the identical Reference on success, otherwise Java(ClassCast); instance_of returns the relation result. Target resolution/access precedes a Java cast failure. Host-invalid handle/metadata precedence is this explicit bounded policy, not a verifier claim. resolve_reference_type performs origin/CP validation and requires the universe before resolution, independent of null.

Interpreter c0/c1 are enabled only in Services::Fields. Extract a typed reference at the reached instruction, then call the corresponding service with the current frame's origin and CP index. Thus malformed stack failure precedes runtime CP checks; this is host-policy ordering. Push the identical reference for c0 or Int(0/1) for c1. Map only explicit TypeJavaError::ClassCast to a new ExecutionOutcome::ClassCastException, and IllegalAccess to the existing outcome. Add ExecutionErrorKind::TypeAccess(Box<TypeAccessError>) for host errors, including all other wrapped categories. Preserve contextualized method/offset/transition-site provenance.

The decoder already supplies these fixed-width encodings; full-code decoding remains eager, while unreachable CP semantics remain unverified. No new frame engine, prepared-method cache or execution entry point. Existing numeric/heap-only entry points continue to reject c0/c1. Shared fuel, frame/slot/code limits and all previous effects persist. Null must not become an UnsupportedInstruction merely because a runtime-aware context has no universe.

## Work and memory accounting

Reuse FieldResolutionOptions and the same cumulative symbols, units and LookupSteps counters. New type-name/array-leaf bindings use the existing symbol/unit limits, charging before resolver callbacks and retaining successful bindings as before. Do not retain a second type cache. Before scanning/hashing a target name for a non-null resolution, charge one work step per UTF-16 unit; array parsing is allocation-free. This charge also applies on a binding hit. Null only performs constant-index/tag access and does not scan/resolve the target name.

Charge nominal validation and each examined graph node, edge enqueue/comparison, type-pair/component-reduction step before work. Shared graph traversal is iterative, with identity-deduplicated visited sets and fallible reservation. Duplicate edges must not create uncharged fanout. No all-pairs ancestry matrix, copied inherited graph or recursive rank walk. Public arbitrary type values must not bypass charges via equality. Preserve exact prior field/method invocation budget tests; new checks may reuse their helpers only without silently changing old accounting.

## Required acceptance

Runtime tests: valid/invalid/foreign core identities and loader roles; root mismatch; idempotent/conflicting binding; binding atomicity and context invalidation; primitive arrays allocated before binding; old registry-free APIs; later compatible/incompatible multi-plan admission; target roots outside receiver ancestry; origin-dependent delegation and exact UTF-16 aliases; public/package access for nominal and array leaf targets; nominal/interface diamonds and interface-to-Object without protected-access widening; all eight primitive arrays; nominal and primitive leaf/rank/loader matrices through rank255; public forged structural type values; null skipping missing/inaccessible targets and callbacks; CP-invalid null; source-handle checks; exact/cumulative budgets and pre-callback limits; deep iterative graph/rank handling and private corruption/reservation guards.

Interpreter tests: c0/c1 results and failure mapping; preserved reference identity and stack operands; null with no universe and unresolved/inaccessible targets; malformed CP/stack/encoding; earlier API rejection; current-frame origin after all accepted call kinds; cast followed by field/array/call use; shared fuel and work exhaustion; failures retaining earlier writes/arrays; new ClassCastException formatting and provenance. Preserve all earlier regression suites.

The coordinator supplies genuine Java numeric wrappers over this and locally allocated primitive arrays, independent JDK result rows and explicit structural platform metadata for Rust tests. Structural targets include reference-array types despite reference-array allocation remaining unsupported. Preserve 11,119 prior execution comparisons, both 151-case service oracles and the 65-class corpus. Gate: frozen technical review, fmt, strict workspace/all-target Clippy, full debug tests, optimized runtime/interpreter tests, fixture/reference reproduction and expanded parser smoke. Publish fresh hosted Linux/Windows evidence separately. No full M2, construction, full conformance or performance approval is implied.

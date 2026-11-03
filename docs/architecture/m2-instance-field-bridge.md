# Instance method-body and field execution

Status: implemented and approved in `m2-instance-field-bridge-review.md` on 2026-11-03. This supersedes `m2-instance-field-bridge-next.md`. Full M2 remains open.

## Scope and ownership

Implement a reusable runtime field-access context and extend the existing interpreter loop with an explicitly supplied instance receiver, getfield and putfield. Keep all existing entry points and behavior. The context uses registered identities and the heap's admitted hierarchy, rather than resolving by a display name or constructing a second graph. It is an explicit unnamed-module execution profile, not a general loader, verifier, constructor protocol or initialized Java invocation.

The runtime worker owns runtime production and resolution tests. The interpreter worker owns interpreter production and synthetic bridge tests. The coordinator owns genuine fixtures, reference tooling, independent integration and workspace integration. Workers use separate worktrees. Review the runtime API handshake before dependent integration.

## Runtime API

Export the following from `crabspot_runtime`; implementation may live in `field.rs` with narrowly scoped internal hierarchy accessors in `instance.rs`.

```rust
pub struct FieldResolutionOptions {
    pub max_symbols: usize,       // default 16_384
    pub max_symbol_units: usize,  // default 1_048_576 UTF-16 units
    pub max_lookup_steps: usize,  // default 1_000_000, cumulative per context
}
pub struct FieldResolutionContext<'a> { /* private */ }
impl<'a> FieldResolutionContext<'a> {
    pub fn new_unnamed_modules(
        registry: &'a DefinitionRegistry,
        heap: &'a mut ManagedHeap,
        resolver: &'a mut dyn HierarchyResolver,
        options: FieldResolutionOptions,
    ) -> Result<Self, FieldAccessError>;
    pub fn registry(&self) -> &'a DefinitionRegistry;
    pub fn heap(&self) -> &ManagedHeap;
    pub fn heap_mut(&mut self) -> &mut ManagedHeap;
    pub fn check_receiver(&mut self, owner: ClassId, receiver: Reference)
        -> Result<(), FieldAccessError>;
    pub fn read_field(&mut self, origin: ClassId, index: u16, receiver: Reference)
        -> Result<Value, FieldAccessError>;
    pub fn write_field(&mut self, origin: ClassId, index: u16,
        receiver: Reference, value: Value) -> Result<(), FieldAccessError>;
    pub fn resolved_symbols(&self) -> usize;
    pub fn symbol_units(&self) -> usize;
    pub fn lookup_steps(&self) -> usize;
}
```

The constructor requires enabled instance storage bound to this registry through admitted layouts. It does not allocate instances or change layouts. Calling it explicitly selects an environment in which participating definitions belong to their defining loaders' unnamed modules. Named-module access/export/readability semantics are not represented or claimed.

Seed a bounded binding table from every admitted class's own `(defining loader, exact UTF-16 name)` and every resolved direct hierarchy dependency `(origin defining loader, dependency symbolic name)`. Reject contradictory bindings, including two admitted plans that imply different ClassIds for the same initiating-loader/name pair. Include interface Object dependencies. Check counters before fallible allocation; no cloned field lists or ancestor graphs. This prevents a later resolver from choosing a different superclass identity than the graph already used for storage.

For other symbolic owner names call the explicit resolver once per successful unique loader/name binding; validate returned registry identity and exact name, require an admitted layout, then retain the binding. Missing definitions, callback errors and invalid returned identities are host errors, not fabricated Java loading exceptions. No source loading or fallback Object exists. A failed lookup need not cache a host failure. Definitions and successful bindings remain immutable for this context.

`heap_mut()` permits existing array operations, instance allocation and field memory services. Capture both the heap's private identity and admitted class count, and reject later context operations if either changes. This also detects a caller replacing/swapping the heap through the mutable accessor. Admission never mutates an existing shape, so the class count detects all actual layout extension within that heap. Construct a new context after admitting more layouts or replacing the heap. Object/array allocation and value mutation do not invalidate it. Check this invariant before using cached bindings or relationships.

Every admitted class/edge visited while seeding, graph node visited during lookup/relationship checking, and field declaration compared during lookup consumes one lookup step, checked before work. Symbols count unique loader/name bindings; symbol units count their retained name units. All counters are inclusive, checked for overflow and cumulative across reuse. Cache hits do not insert or charge names again. Field-result caching is optional; no additional unbounded cache is permitted. A traversal uses explicit stacks/sets bounded by admitted classes/edges; the constructor has already charged those edges to the cumulative work budget. The remaining work allowance is not a pending-stack-slot limit. A new context explicitly starts new budgets. Exhaustion is a host policy failure, not a Java exception.

## Structured failure surface

Define `FieldJavaError::{NoSuchField, IllegalAccess, IncompatibleClassChange, NullPointer}` and `FieldResource::{Symbols, SymbolUnits, LookupSteps}`. `FieldAccessError` has:

- `Java(FieldJavaError)`;
- `Registry(RegistryError)`, `Instance(InstanceError)`;
- `Resolve { origin: ClassId, name: JavaString, error: ResolveError }`;
- `MissingDefinition { origin: ClassId, name: JavaString }`;
- `NameMismatch { requested: JavaString, returned: ClassId }`;
- `ConflictingBinding { loader: LoaderId, name: JavaString }`;
- `InvalidConstant { origin: ClassId, index: u16 }`;
- `UnsupportedNestAccess { origin: ClassId, owner: ClassId }`;
- `UnsupportedLoadingConstraint { origin: ClassId, owner: ClassId }`;
- `UnsupportedArrayOwner`, `ContextChanged`, `InvalidState(&'static str)`;
- `InvalidReceiver { expected: ClassId, actual: ClassId }`;
- `ResourceLimit { resource: FieldResource, limit: usize, required: usize }`.

Wrap heap failures through `InstanceError::Heap`, including null/wrong-kind/foreign handles in the host receiver check and allocation failures. `required` saturates to usize::MAX only for checked overflow, following existing resource errors. Add Display/Error and useful sources; diagnostic payload boxing is allowed to keep interpreter errors small. Do not expose private handle fields.

## Resolution and access

The origin D is the class containing the currently executing method's constant pool. Both origin D and resolved owner C must have admitted layouts in this context's fixed graph. Check the origin's registry identity and admitted membership before selecting a symbol, including public-field accesses that would not otherwise need origin ancestry. Missing layouts are host Instance/LayoutMissing errors. Resolve the Fieldref owner C through D's defining loader. Reject non-Fieldref/wrong indices structurally. Array owner names are explicit unsupported. The heap graph is authoritative for already resolved superclass/interface identities.

Lookup matches exact name AND descriptor: inspect C's own declarations first, then its direct interfaces recursively in class-file order, then its superclass recursively. Use iterative depth-first traversal preserving that precedence, with a visited set to avoid diamond repetition. For interfaces, the class-file Object dependency is not a field-lookup superclass. Static interface declarations must participate; never skip one to find an instance field in a superclass. Return NoSuchField if the complete supported graph search fails. [JVMS field resolution](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.3.2).

Check symbolic owner accessibility before field lookup. Under this explicitly unnamed-module profile, public owners are accessible; nonpublic owners require the same runtime package. A runtime package compares exact UTF-16 text before the final slash AND defining LoaderId, including the empty package.

For the selected field, public access succeeds. Same-class private access succeeds without nest resolution; any other private access returns UnsupportedNestAccess, even where a future nest check could reject it. Package access requires the same runtime package. Protected access succeeds in the same runtime package; otherwise D must be the declaring class E or its subclass. For a non-static protected field, additionally require symbolic owner C to be D, a superclass of D or a subclass of D, and check the actual non-null receiver is D or a subclass. The symbolic-owner/receiver restrictions do not apply to protected static fields: after successful access checking, b4/b5 reject those with IncompatibleClassChange. A malformed non-static receiver violating that verifier obligation is a host InvalidReceiver error, not a fabricated verifier result. [JVMS access control](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.4).

Do not require accessibility of E separately when an accessible symbolic C exposes E's public inherited field. All ancestry/package comparisons use checked identities. Interface Object dependencies are not class inheritance for these queries.

Primitive fields and arrays with primitive leaf types have no reference-class loading constraint. Reference fields whose leaf is a class/interface are supported when E and D have the same defining loader; cross-loader cases return UnsupportedLoadingConstraint before successful resolution. This restriction avoids claiming to impose persistent cross-loader type constraints. Reference reads yield the existing null defaults; reference writes permit null only. Non-null writes remain InstanceError::UnsupportedReferenceStore. Named modules, nest membership and general assignability remain explicit later work.

After resolution/access/loading-constraint checks, getfield/putfield reject a static selected field with IncompatibleClassChange. putfield then rejects every final field with IllegalAccess because constructor bodies are excluded. Only then inspect null and return NullPointer. For non-null receivers validate heap identity/kind and require the actual class to be C or its subclass (and the protected restriction above); incompatible instance identities are host InvalidReceiver. Then delegate typed storage access. Field kind, narrowing, raw floating bits and null-only stores retain the approved memory-service semantics. Resolution failure, staticness and finality must win over a null receiver. Host failures on malformed unverified operand types have no promised Java error precedence. [getfield](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.getfield), [putfield](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.putfield).

`check_receiver` is for host entry selection: non-null ordinary instance, same registry and actual class equal/subclass of owner. It does not resolve a symbolic method or perform invocation access checking. Null is a host Instance/Heap error here; bytecode null dereferences use FieldJavaError::NullPointer.

## Interpreter API and shared engine

```rust
pub fn execute_instance_method_body(
    context: &mut FieldResolutionContext<'_>,
    method: MethodId,
    receiver: Reference,
    arguments: &[Value],
    options: ExecutionOptions,
    limits: CallLimits,
) -> Result<ExecutionOutcome, ExecutionError>;
```

Obtain the exact class and method through the context registry. A checked read-only `MethodId::index() -> usize` accessor is authorized if needed; ownership/index validation still uses registry.method. The root must be an ordinary non-static class method with Code, numeric/void descriptor, no synchronization/native/abstract/special name and no handlers, under existing eager metadata/decode rules. Reject interface-root bodies explicitly for this increment. The host selects the body; there is no virtual dispatch or invocation access claim. Private ordinary bodies may be selected explicitly.

Check the root receiver before execution and put it in local zero, followed by numeric arguments at their normal widths. Root frame reservation includes its full declared max_locals/max_stack. Validate local capacity including the receiver. Preserve original numeric argument indexing in diagnostics.

Reuse the existing decoder, prepared methods, frame stack, instruction step loop, arithmetic, arrays and same-class static-helper calls. Only the root may be an instance body; invokestatic callees retain their existing numeric/static checks and start arguments at local zero. Their origin remains the selected MethodId's declaring ClassId, even if the receiver is a subclass. A call to the instance root through invokestatic must fail, including if its prepared body is already cached. Cache admission mode must not bypass declaration checks.

Enable b4/b5 only through this runtime-aware entry point, with checked stack types/categories and one shared instruction-budget charge each. Earlier static entry points continue to reject them even with a heap. All existing primitive-array operations remain available in instance mode. `invokevirtual`, `invokespecial`, `invokeinterface`, `new`, getstatic/putstatic, reference parameters/results, constructors, initialization, exception dispatch and monitors remain unsupported. Do not clone the engine into a second interpreter.

Extend ExecutionOutcome with `NoSuchFieldError`, `IllegalAccessError` and `IncompatibleClassChangeError`, preserving the existing symbolic NullPointerException. Map only FieldAccessError::Java to these outcomes. Add `ExecutionErrorKind::FieldAccess(Box<FieldAccessError>)` for host/unsupported/resource errors, and retain current method/bytecode/call-site provenance. Earlier completed field stores/array allocations persist on every exit. Failed field instructions do not change field values. The service does not create Java error objects.

## Acceptance

Require independent genuine Java 21 method-body comparisons covering all eight field kinds, mixed-width arguments, inherited-body selection with child receivers, field hiding, null reference reads/writes, protected inherited fields, receiver aliases, loops, static helpers, arrays and persistent writes before arithmetic failure. Add raw Z/B/C/S putfield bytecodes without narrowing or narrow returns, using the reviewed field-store oracle.

Synthetic runtime tests must cover own/interface/superclass precedence, interface diamonds, same name/different descriptors, missing fields, inaccessible symbolic owner, public inherited fields from inaccessible declarers, same/different loader packages, private same-class/cross-nest unsupported, protected symbolic-owner and receiver constraints, cross-loader reference constraints, foreign heap/registry/wrong receiver kinds, alias binding consistency and contradictory admitted graphs. Test all limits at zero/exact/exhausted/usizeMAX with small real inputs, stable symbol caching, context extension and equal-count heap replacement invalidation, and deep iterative traversal/drop. Private tests exercise forged metadata and arithmetic/reservation failure where public construction cannot.

Synthetic interpreter tests cover null versus missing/static/final/access ordering, malformed CP/operand types, every field value kind, category-two frames, old-entry gating, shared fuel/frame/slot/code limits, static call to cached instance root rejection, unsupported descriptors/declarations/opcodes and all persistent-error paths. Preserve all 8,285 execution cases and both 151-case service oracles. Run debug/release, formatting, strict workspace Clippy, reproducible fixtures/references and final director review. Hosted Linux/Windows regression evidence is recorded separately. No full-M2, full-linkage, constructor, conformance or performance claim follows from this increment.

# Modern method type-state verification

Status: exact contract authorized for bounded implementation on 2026-11-04 after coordinator and both worker readbacks. The complete modern method proof is the immediate objective. This authorizes the isolated production and independent-test tasks below, not acceptance of an implementation. The accepted standalone Code-validation API remains unchanged. No NanoSpec.

## Objective and proof boundary

Add a standalone modern method type checker to `crabspot-verifier`. Cover every instruction accepted for the selected modern version, independently of interpreter opcode support, including dynamic constants/invocation and monitor instructions. Do not execute, initialize, allocate Java objects, invoke source callbacks or manufacture VerifyError objects. This is a method proof relative to an explicitly supplied immutable class environment, not a complete class-linking or application-startup certificate.

The normative rule inventory is [JVMS 4.10.1](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10.1), especially the verification type system, merged Code/stack-map algorithm, protected-member rules and instruction predicates. Implement those rules as an independently written Rust checker, not copied reference-JVM code. Stack-map checking is distinct from runtime assignment: interface destinations, array stores, generic references and constructor exception states require verifier-specific treatment. The exact implementation/test matrix must map every modern opcode to its rule and every non-Code obligation to an implemented check or named deferred boundary.

Selected methods initially require major 51 through 65, with the accepted version/preview envelope. Major 50 fallback and earlier inference, jsr/ret and historical parameterized initializers remain explicitly unsupported by this new API, not invalid class-file claims. Dependency classes in the environment may use any registry-supported version. Old Code-validation and execution entry points retain their current version policies.

Full class verification still requires an aggregate over all declarations plus non-Code obligations and loading constraints. This result must not be named VerifiedClass. Include the selected method's no-final-override obligation, including abstract/native declarations; do not certify unrelated method bodies.

## Immutable environment: settled direction

Use existing runtime `DefinitionRegistry`, `ClassId`, `LoaderId` and `HierarchyPlan` identities. The verifier may add a dependency on runtime; runtime must not depend back on verifier. Classfile and bytecode remain independent. No heap, static storage, initialization profile or FieldResolutionContext is involved. No duplicate identity issuer or generic host `is_assignable`/`passes_protected` callback is introduced.

The environment borrows the registry and retained hierarchy plans. It owns only bounded indices, compact facts and explicit initiating-loader bindings. Results borrow that exact environment; no public mutation/rebind/reset exists. The registry borrow prevents adding or changing definitions while the environment lives. Different environments over one registry may have different delegations and must never share proof authority implicitly.

```rust
pub struct VerificationBinding {
    pub initiating_loader: LoaderId,
    pub definition: ClassId,
}
pub struct VerificationCore {
    pub bootstrap_loader: LoaderId,
    pub object: ClassId,
    pub cloneable: ClassId,
    pub serializable: ClassId,
    pub throwable: ClassId,
    pub string: ClassId,
    pub class: ClassId,
    pub method_type: ClassId,
    pub method_handle: ClassId,
}
pub struct VerificationEnvironment<'a> { /* private */ }
impl<'a> VerificationEnvironment<'a> {
    pub fn new(
        registry: &'a DefinitionRegistry,
        plans: &[&'a HierarchyPlan],
        bindings: &[VerificationBinding],
        core: VerificationCore,
        options: VerificationEnvironmentOptions,
    ) -> Result<Self, Box<VerificationEnvironmentError>>;
    pub fn usage(&self) -> VerificationEnvironmentUsage;
}
```

The class union must be closed over every retained direct superclass/interface identity. Deduplicate identical ClassIds, but reject any disagreement about their ordered direct relations. Charge physical input rows and edges before deduplication or enqueue. Validate registry membership, metadata relation names/kinds, no cycles, and termination at the designated Object. An interface's metadata Object relation is retained explicitly; each verification relation decides how it participates.

Seed bindings from each retained definition's own defining loader and from its resolved hierarchy edges in the declaring class's loader. Explicit bindings add delegation for other necessary symbols. A binding's name is the target definition's exact UTF-16 name; callers do not supply an independently mutable alias spelling. Same initiating-loader/name cannot map to different ClassIds, including conflicts with locally defined names. Missing referenced facts during checking are a distinct unavailable-prerequisite failure, never false assignability or successful verification. Preloading is a caller operation before environment construction.

All eight core identities must belong to this union and the explicitly designated bootstrap loader and be public. Their exact names are java/lang/Object, java/lang/Cloneable, java/io/Serializable, java/lang/Throwable, java/lang/String, java/lang/Class, java/lang/invoke/MethodType and java/lang/invoke/MethodHandle. Object is a noninterface and the unique retained root. Cloneable/Serializable are abstract interfaces whose metadata superclass is the designated Object and whose direct interface list is empty. The other roles are ordinary noninterface classes in the Object-rooted graph; do not demand concrete/final flags (MethodHandle is abstract), a particular constructor, or direct Object ancestry for Throwable. This does not supply core-library implementations, bypass their own future verification, or treat a matching name in another loader as a builtin.

Symbolic types preserve a class spelling and initiating loader until a rule needs loaded facts. Identical symbols, null and generic verification categories must not eagerly resolve unrelated descriptor classes. Necessary resolution consults only frozen bindings. Array spellings remain structural types; they never require registry array ClassIds. A proof cannot be reused after constructing a different environment with other bindings.

## Entry and output API

Use ClassId plus declaration index, avoiding an unbudgeted declared-method name search merely to manufacture MethodId. The selected ClassFile comes canonically from the environment registry, not from an unrelated same-name ClassFile argument.

```rust
pub struct TypeVerificationOptions {
    pub code: CodeValidationOptions,
    pub max_type_symbols: u64,
    pub max_state_cells: u64,
    pub max_edge_checks: u64,
    pub max_utf16_units: u64,
    pub max_work_units: u64,
}
pub struct TypeVerificationUsage {
    pub code: CodeValidationUsage,
    pub type_symbols: u64,
    pub state_cells: u64,
    pub edge_checks: u64,
    pub utf16_units: u64,
    pub work_units: u64,
}
pub struct MethodTypeVerification<'e, 'a> { /* private */ }
impl<'e, 'a> MethodTypeVerification<'e, 'a> {
    pub fn environment(&self) -> &'e VerificationEnvironment<'a>;
    pub fn class_id(&self) -> ClassId;
    pub fn method_index(&self) -> usize;
    pub fn code_validation(&self) -> &MethodCodeValidation<'a>;
    pub fn usage(&self) -> TypeVerificationUsage;
}
pub fn verify_method_types<'e, 'a>(
    environment: &'e VerificationEnvironment<'a>,
    class: ClassId,
    method_index: usize,
    options: TypeVerificationOptions,
) -> Result<MethodTypeVerification<'e, 'a>, Box<TypeVerificationError>>;
```

Run accepted whole-Code validation internally on the canonical class before proof work. First check registry identity and retained class membership, then invoke that validator once. Preserve its exact error and usage if it fails. Only after a successful partial result reject major below 51 as UnsupportedHistory, with zero type-phase work. Thus malformed historical Code can fail representation before the new history policy; this is intentional. Do not accept caller-supplied decoded instructions or trust a separately supplied partial result as belonging to this environment. Legal ordinary abstract/native declarations have absent Code but still undergo the selected final-override check. No executable frame, heap reference, expanded proof history or mutable signature cache escapes in the output. Temporary symbols/maps/states are dropped after success; the result owns only the partial validation result, environment borrow, identity/index and usage. Make the implied lifetime relationship `'a: 'e` explicit where Rust requires it.

Errors distinguish representation failure, actual type-rule rejection, unavailable class facts, unsupported history, policy limits, accounting overflow and allocation failure. Preserve the selected class/index, structural location and completed usage. No nested failure is reclassified as a Java execution outcome. Use these public shapes; diagnostics do not copy nominal names:

```rust
pub enum VerificationSymbolSource {
    Utf8 { class: ClassId, index: u16, start: u16, end: u16 },
    Definition(ClassId),
    Core(VerificationCoreRole),
}
pub enum TypeVerificationResource {
    TypeSymbols, StateCells, EdgeChecks, Utf16Units, WorkUnits,
}
pub enum TypeVerificationErrorKind {
    Registry(RegistryError),
    Code(Box<CodeValidationError>),
    UnretainedClass { class: ClassId },
    UnsupportedHistory { major: u16 },
    UnavailableFact { initiating_loader: LoaderId, symbol: VerificationSymbolSource },
    TypeRule { reason: &'static str },
    FinalOverride { ancestor: ClassId, ancestor_method_index: usize },
    ResourceLimit { resource: TypeVerificationResource, limit: u64, required: u64 },
    AccountingOverflow { resource: TypeVerificationResource },
    AllocationFailure { resource: &'static str },
}
pub struct TypeVerificationError {
    pub class: ClassId,
    pub method_index: usize,
    pub location: CodeValidationLocation,
    pub usage: TypeVerificationUsage,
    pub kind: TypeVerificationErrorKind,
}
```

Options, usage, bindings, core, scalar locations/resources/symbol sources implement Debug, Clone, Copy, Eq and PartialEq where their fields permit; usage has zero Default and options have the defaults below. Errors/kinds implement Debug, Display and Error; wrapped Registry/Code errors remain their direct source. Environment/result have Debug but private fields, no Clone, mutation or public issuer. A Code failure copies its location/usage exactly into the outer diagnostic with all type counters zero. Registry/membership/history errors use Method location. Later errors use the current instruction, handler or map location; final-override errors use Method plus the ancestor/index payload. Both new public constructors/functions return boxed errors to keep these rich fixed diagnostics out of large Result error layouts; no broad Clippy exemption is needed. Boxed diagnostic allocation follows the existing crate's error-allocation convention, not a recoverable Java heap policy.

After successful representation and modern-version checks, scan the selected signature and perform the selected no-final-override check. If bodyless, return there. Otherwise build the initial frame and prescribed maps in file order, check handler catch-type/target-map legality in table order, then perform the merged instruction/map proof in offset order. Initial handler legality checks target-map presence, catch assignability to Throwable, a one-slot prescribed stack, and assignability from the incoming catch type to that prescribed slot. Object and the unusable verification type Top are legal destinations. A handler that prescribes Top receives an unusable slot. A normal return may leave it untouched. Trying to consume it as a category-one value or a reference fails the corresponding instruction rule. Local/flag compatibility is checked against actual instruction inputs later. Thus an invalid handler stack can precede an ill-typed instruction zero. A frame-shape error discovered during expansion precedes later instruction errors. At each instruction, fund its rule, check its normal transfer and explicit targets first, then visit handlers in original table order against its input locals and exceptional constructor views. Preserve that input-local view using deferred edits or a bounded overlay; never let a store's post-state replace the exception source and never clone all locals per instruction. Missing facts or exhausted resources stop immediately; do not convert them to a type mismatch or continue to a later successful rule.

## Algorithm and state representation

Use the modern stack-map-guided sequential proof, not an execution trace or reachable-only traversal. Merge instruction offsets with declared map offsets. At a map, validate the incoming state when one exists, then use the prescribed map state. After unconditional transfer, a subsequent instruction needs its required map; end-of-Code requires the appropriate terminal state. Check branch targets and every applicable exception edge, including instructions whose runtime implementation could not throw. Do not infer arbitrary join states or silently synthesize absent maps.

Keep a private verifier type representation distinct from public runtime Value. Preserve narrow primitive array kinds, structural ranks, symbolic nominal identities, null, unusable Top, category widths and exact uninitialized instruction offsets. Constructor initialization flags are independent of operand/local aliases. An overwrite may remove the last visible unfinished-this alias without authorizing return. Explicit Top entries and category-two continuation slots must not be confused by load/store, stack manipulation or map expansion.

Retain immutable map states only at supplied map offsets, plus an initial state and current working frame. Share unchanged local buffers across Same and SameLocalsOneStack maps by arena indices; do not require infallible Arc allocation. Store only explicit local prefixes with an implicit Top suffix, rather than padding every map to max_locals. Append/Chop/Full may create a funded new prefix; each copy is charged. No per-instruction frame snapshot or unbounded alias list is permitted. Current-state storage may reuse actual-demand capacity. Every comparison, copy and constructor substitution scan must be funded; handler processing should use an input-local view plus the required exceptional transformation rather than cloning the full frame at every instruction.

Descriptor/name parsing uses borrowed UTF-16 ranges and per-invocation scalar caches. The old representation pass's cache is private and temporary, so the new phase funds its own semantic scans; it must not claim those allocations/work were already paid. Structural type interning may share repeated symbols, but must charge first hashing/comparison of their complete name and preserve initiating-loader identity. Runtime `is_assignable`, field access and method-selection services must not be reused as verifier predicates.

Protected-member checks use immutable class/member facts and the verifier's specific symbolic-name and superclass rules. First inspect the selected class's superclass names to determine whether the operand is in the relevant superclass case; do not eagerly resolve an unrelated owner just to perform this test. Where member facts are needed, retain the actual declaring class, flags and kind. Field lookup is own declaration, direct superinterfaces recursively in declaration order, then superclass; method lookup is nearest exact declaration in the class chain. Private/static/nonprotected matches hide older declarations for this fact query and must not be skipped. Valid interface members cannot introduce a protected declaration, so absence after the class chain needs no default-method selection. An absent member is left for linkage; missing required class facts is UnavailableFact. This query never reports runtime nest/access/staticness/loading failures.

Apply the protected receiver restriction only when the inherited fact is protected and its actual declaring runtime package differs from the selected class's package. Runtime package equality includes defining LoaderId. The receiver must satisfy the selected-class restriction in addition to the ordinary owner/type condition. No receiver restriction applies to static operations. Array owners remain structural; array clone verification does not demand a registry array identity. Preserve the specified array-clone exception for an array receiver of protected Object.clone rather than rejecting it as an unrelated receiver. These choices are pinned by the original p.Base -> q.Middle -> p/q.Child field/method probes: package equality with Middle does not substitute for equality with the actual Base declarer. No general member resolution or execution authorization is certified.

The selected no-final-override check follows actual overriding, including the runtime-package/transitive cases of [JVMS 5.4.5](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.5). Selected private/static methods and initializer names are exempt. Visit superclass declarations nearest first for the exact name and descriptor. Ignore ancestor private/static declarations without stopping. Maintain the set of reachable runtime packages, initially the selected owner's package. A matching instance declaration is overridden when public/protected, or when its package is already reachable; reject if that declaration is final, otherwise add its package. An inaccessible package declaration neither rejects nor extends the set. Continue to farther declarations. This implements package bridges without a per-ancestor replay of the entire chain. Interface default selection is not involved. Signature matching here is exact descriptor text, not runtime reference-value assignment; loading constraints remain a separate obligation. The coordinator's 13 original probes show both the legal private/static exemptions and package-final cases. Their invalid overrides fail class definition with IncompatibleClassChangeError on JDK21; the checker returns structured FinalOverride and makes no claim that every rejected method corresponds to a lookup-stage VerifyError.

Constructor rules must use the selected class type after successful this/super chaining, not blindly substitute the called superclass type. Ordinary helper exceptions caught before chaining preserve the pending constructor proof state. The designated Object constructor has its specified initial receiver state. For a new-site token the named constructor must match that allocation's class; for unfinished this it must name the current class or direct superclass. new rejects a live same-site token on the operand stack and replaces same-site local aliases with Top; it does not reject ordinary repeated executions of a completed allocation site.

For every handler covering a constructor invocation, compare the handler map against both the incoming local aliases and those aliases substituted to the initialized class, retaining the original constructor flag in both exceptional comparisons. The handler stack contains only its catch type. This forces allocation-target local aliases to become unusable and prevents a failed this/super call from clearing the unfinished-this flag. Normal completion instead substitutes aliases and clears that flag only for unfinished this. Do not copy the SE21 pseudocode's inconsistent ExceptionStackFrame expression or superclass rewrite literally. Its prose, [OpenJDK clarification JDK-8302497](https://cr.openjdk.org/~dlsmith/8302497/8302497-20231218/specs/invokespecial-verification-jvms.html), and the original JDK21 probes support the intended distinction. That linked document is a later draft clarification, not an assertion that this increment implements Java 23. The coordinator's allocation-handler matrix distinguishes all three local maps: initialized/uninitialized aliases reject even for rethrow; Top permits rethrow or swallowing the exception, but loading/retrying the discarded alias rejects. The failed this/super matrix rejects Top, uninitializedThis and current-class maps; ordinary caught-helper and post-super alias cases remain valid.

All modern opcode families require direct transfer coverage, including all legal dup/pop forms, narrow arrays, null array operations, typed field/method signatures, invokedynamic, dynamic constants, monitor operations and athrow. In particular, generic reference predicates can include unfinished values; actual ordinary-reference demands must remain distinct. Interface destination relaxation does not authorize primitive arrays to arbitrary interfaces, and aastore verification does not perform runtime component-store checks.

## Complete instruction and relation inventory

Implement the applicable instruction predicates of JVMS 4.10.1.9, with the explicit reconciliations above. The following is an obligation matrix, not an execution-support whitelist:

| Family | Required proof |
| --- | --- |
| Constants, ldc forms | Exact computational category; String/Class/MethodType/MethodHandle use the designated bootstrap identities; Dynamic uses its declared field type without running a bootstrap. |
| Local loads/stores/iinc, including wide | Correct initialized value or permitted unfinished reference; local bounds already represented, but type/category and overlapping category-two invalidation must be checked. WideTail differs from explicit Top. |
| pop/dup/swap | All legal category-one/category-two forms, including unfinished reference values; no accidental promotion. |
| Numeric arithmetic/conversions/comparisons | Exact operand/result types and widths, including shift count Int and narrow conversions returning Int. |
| Branches/switches | Required Int or generic reference operands; every explicit target has a supplied assignable map, including target zero; duplicate switch targets still checked/funded. |
| Returns | Opcode and value agree with selected descriptor; reference assignment uses verification relation; constructor return also requires cleared pending-this flag. Do not invent an empty-stack-after-return requirement. |
| Primitive/reference array operations | Exact primitive array kind (byte/boolean shared where specified), Int index, reference-array shape for aaload/aastore; null follows the verification rules. aastore accepts ordinary reference values without proving runtime component compatibility. |
| new/newarray/anewarray/multianewarray/arraylength | Correct class/array structural types, dimensions and Int counts; new-site replacement and constructor flags as above. No Java allocation or class initialization. |
| Fields | Descriptor transfer, ordinary owner receiver and protected restriction; special uninitializedThis putfield exception requires the current class as symbolic owner and an exact own field declaration. No other unfinished receiver/value escape. Runtime staticness/finality/access/link failures are not simulated. |
| invokevirtual/interface/static/dynamic | Reverse argument type/slot matching and declared result; receiver requirements where present. Do not resolve a target body, initialize, enforce runtime component assignment, or require successful dynamic linking. |
| invokespecial | Ordinary special receiver/owner rules, including structural owner legality; constructor exact allocation owner or current/direct-super chain, normal alias substitution and both exceptional views. Do not reuse runtime ACC_SUPER selection. |
| checkcast/instanceof/athrow/monitors | Required initialized-reference or generic-reference predicate exactly as specified; athrow value is assignable to designated Throwable (null permitted). Monitor operations permit generic unfinished references and do not prove balanced locking. |

Verification assignment is a private relation. Exact scalar/symbol identity and null/category shortcuts precede necessary environment lookup. Distinct initiating aliases can become equal only through frozen bindings. Nominal class-to-interface assignment is relaxed by the verifier; array-to-arbitrary-interface assignment is not. Arrays assign to designated Object/Cloneable/Serializable, and component comparison is recursive with primitive leaves exact and reference components using the verification relation. Interface metadata Object edges are consumed according to that relation, not runtime class-only protected-subclass code. Missing necessary bindings remain unavailable, even if an execution profile would reject the operation first.

Prescribed frames preserve logical local entries as well as expanded slot widths: Full [Long, Top] occupies three slots, and Chop(1) removes only the explicit Top. Initial locals contain receiver and parameters, with an implicit unused Top suffix. Stack-map constructor flags derive from unfinished-this locals, not an arbitrary stack occurrence. Source flags must be compatible with destination flags even when aliases disappear. Catch-all uses designated Throwable; typed catches must be assignable to it. The exceptional source stack contains exactly the catch type. Its prescribed map has one assignable destination slot, which may be Top. All handlers undergo the initial legality check, and covered instructions subsequently check the exceptional local and flag views.

## Resource policy

Both environment and proof counters are checked u64, inclusive, with atomic multi-counter charges and no policy-sized reservations. Environment options separately bound physical class rows, edges, bindings, scanned UTF-16 and work. Proof options bound symbols/cache records, cumulatively materialized state cells, edge examinations, new UTF-16 work and total work; Code options retain their independent accepted charges.

```rust
pub struct VerificationEnvironmentOptions {
    pub max_class_rows: u64,     // default 65_536
    pub max_edges: u64,          // default 262_144
    pub max_bindings: u64,       // default 262_144
    pub max_utf16_units: u64,    // default 8_388_608
    pub max_work_units: u64,     // default 16_777_216
}
pub struct VerificationEnvironmentUsage {
    pub class_rows: u64,
    pub edges: u64,
    pub bindings: u64,
    pub utf16_units: u64,
    pub work_units: u64,
}
pub enum VerificationEnvironmentResource {
    ClassRows, Edges, Bindings, Utf16Units, WorkUnits,
}
pub enum VerificationEnvironmentLocation {
    Plan(usize),
    ClassRow { plan: usize, row: usize },
    Class(ClassId),
    Binding(usize),
    Core(VerificationCoreRole),
}
pub enum VerificationCoreRole {
    Object, Cloneable, Serializable, Throwable, String, Class, MethodType, MethodHandle,
}
pub struct VerificationEnvironmentError {
    pub location: VerificationEnvironmentLocation,
    pub usage: VerificationEnvironmentUsage,
    pub kind: VerificationEnvironmentErrorKind,
}
pub enum VerificationEnvironmentErrorKind {
    Registry(RegistryError),
    MissingClass { class: ClassId },
    ConflictingHierarchy { class: ClassId },
    MissingDependency { class: ClassId, dependency: ClassId },
    BindingConflict { initiating_loader: LoaderId, existing: ClassId, incoming: ClassId },
    InvalidCore { role: VerificationCoreRole, class: ClassId, reason: &'static str },
    InvalidMetadata { class: ClassId, reason: &'static str },
    ResourceLimit { resource: VerificationEnvironmentResource, limit: u64, required: u64 },
    AccountingOverflow { resource: VerificationEnvironmentResource },
    AllocationFailure { resource: &'static str },
}
```

Environment construction order: funded physical plan/class/edge inventory and consistent merge; metadata name/kind/closure/root/cycle validation; implicit self/edge bindings; explicit bindings in caller order; core-role validation. Count one work unit per plan header, physical class row, physical edge and attempted binding, in addition to funded name scans and additional validation traversals. A physical class row and edge each atomically add their resource and base work. Every ordered-edge comparison during deduplication pays work. Graph validation pays each entered node and examined edge, with iterative bounded storage. Class names/package facts are scanned once per retained ClassId under full UTF-16 funding; subsequent scalar identities are reused. Every binding attempt pays one Binding and Work before validation/reservation, even if duplicate. With merge-then-seed, the binding total is unique retained classes plus their direct edges plus explicit binding count, not duplicated physical rows. Check registry.lookup_class for each initiating-loader/target-name binding and reject any different local definition even outside the retained union; fund the full queried spelling before that lookup. All allocations are fallible and precede publication of the environment. Failure publishes none of it and changes no borrowed registry/plan.

Preserve first-seen unique class order from physical plan/class rows. Use that order for graph root visitation and implicit bindings; for each retained class seed its self binding, superclass edge if present, then direct interfaces in metadata order. Traverse retained edges and member declarations in their original order; explicit bindings retain caller order; core roles follow the struct order above. Hash-map iteration must not choose observable validation/error order. Repeating identical inputs/options yields the same first error and usage.

Type options default to 65,536 symbols/cache records, 4,194,304 state cells, 4,194,304 edge checks, 4,194,304 UTF-16 units and 67,108,864 work units, plus default CodeValidationOptions. Primitive categories/offset tokens are inline scalars; dynamically retained symbol, type, descriptor-summary and resolution-cache records each consume the symbol budget. Compact signature type-ID lists also consume state cells; nominal names remain borrowed.

StateCells is cumulative logical materialization with no refunds: initial/prescribed local prefixes, prescribed stacks, copied prefixes, new current cells and compact signature lists count. Reusing Vec capacity does not erase the charge for rematerializing a copy. An in-place overwrite of an existing cell pays work but does not create another cell; sharing an immutable Same prefix creates no cells. Keep current locals borrowed from its arena buffer until a write requires a funded private copy; a map reset to an already retained prefix must not copy it gratuitously. Compare only explicit prefixes when their common implicit Top suffix can be established in constant time. Fund a whole copy/expansion atomically to StateCells and Work before allocation and traversal; do not charge those same copied cells again. A pure compare/substitution scan pays its full visited cell count to Work before scanning, without pretending that unchanged slots were newly allocated.

For exact counting, expanded Full [Long, Top] creates three cells: Long, WideTail and explicit Top. A compact parameter type-ID list creates one cell for a Long parameter; expanding that parameter into initial locals creates two additional slot cells. These are different retained representations and both are charged when actually materialized.

EdgeChecks counts each instruction-times-handler-row examination, including uncovered rows; an instruction with an empty handler table has no such edge charge. Each actual handler-state comparison adds another edge check (two exceptional comparisons for a constructor call). It also counts every branch/switch target, including duplicates, and each incoming-state comparison at a declared map. A sequential instruction-to-instruction fallthrough without a map has no separate edge event. Handler legality's initial target-map/catch check is one additional edge per handler. Target lookup work is separate. Branch target zero requires a declared map; the implicit method-entry state is not an invented table row. All map/target lookups are checked and funded before lookup.

Each new UTF-16 semantic scan atomically funds its full span to Utf16Units and WorkUnits before reading it. Cache by canonical owner plus Utf8 index, span and semantic role; repeated CP aliases to that same source reuse its summary, while each checked CP lookup costs one Work. Exact symbol interning across different sources funds their full hashing/comparison work before deduplication. A source-key cache lookup costs one Work even on a hit; new retained cache records also pay TypeSymbols. Binding queries and member name/descriptor equality cannot hide repeated string scans behind scalar keys: reuse already funded interned spelling identities, or precharge the complete compared spans. Do not use unbudgeted registry find_declared operations.

Count one Work per instruction, map header, handler row examination, relation request, entered hierarchy node, examined hierarchy edge, member candidate, checked CP lookup and scalar cache/target lookup. Every EdgeChecks increment also atomically pays one Work; where a handler-row examination already is that edge event, do not charge its base Work twice. Binary target lookups use a fixed upper bound of 16 comparisons because represented Code has at most 65,535 instruction boundaries; fund 16 Work for each such lookup instead of data-dependent uncharged comparison loops. Scalar hash lookups retain ordinary hash-table amortization; all long keys have their separate funded scans. Relation scans use iterative traversal with every node/edge charged before examination/enqueue, including duplicates; no recursion proportional to class depth or descriptor rank.

For avoidance of double billing, a binary target lookup pays 16 Work total, replacing the scalar lookup charge of one, not adding to it. An already indexed direct scalar target lookup pays one. The environment's class-name/package semantic summary scan happens once per retained class, but every registry name-binding lookup additionally funds the queried name to both Utf16Units and WorkUnits, including repeated binding attempts; that cost represents the actual string-key lookup, not another retained summary. A proof's already resolved scalar cache hit does not repeat that string-key lookup.

An instruction's stack operations fund every visited/popped/pushed logical slot, including WideTail accounting, before mutation. New pushed/materialized slots additionally pay StateCells; overwrites of existing locals pay Work only, except a required COW copy. Full prefix copying and comparisons use the cell formulas above, not an additional per-element base charge. Changes to temporary proof state after an earlier paid phase need not roll back when a later check fails, because no proof is published; failed multi-counter charges themselves change none of their counters. Allocation reserves actual required additions fallibly; zero/MAX policy limits must not induce policy-sized allocations. Any deferred large input list is funded before traversal/allocation, not after materializing it.

Environment and proof budget accounting are independent. A proof reports the accepted whole-Code counters plus its own counters; constructing an environment does not consume a later proof's allowance. No output history or cache grows across verify calls. Tests must hand-count small empty/bodyless, straight-line, map, handler and constructor cases, then test each inclusive boundary, zero and MAX, cumulative failures and unchanged borrowed input.

## Authorization and review gates

The coordinator and both read-only workers completed consistent readbacks of these public APIs, semantic choices, lifetimes, budget model and acceptance scope. The coordinator's deterministic-order clarification is included. No new runtime accessor or identity issuer is required or authorized. Implementation may now begin in separate mutable worktrees from the published Code-validation/hosted-evidence baseline. Any semantic or public API deviation requires director review and a contract amendment before being treated as authorized scope. This contract is not an implementation review or a whole-JVM conformance claim.

## Planned ownership and acceptance

Production worker owns verifier environment/type-checker code and private invariant tests in one isolated worktree. Independent test worker owns public synthetic tests and an original builder in another. Coordinator owns workspace integration, genuine/type-state fixtures, stage-aware reference tools, provenance, CI and the final fresh-target gate. No execution-engine changes are part of this contract.

The production worker must publish an immutable compiling API snapshot before the independent worker refreshes dependencies; subsequently publish exact frozen sources and hashes. Source files may be split into environment, symbols/relations, frames/maps and instruction checking modules. No public frame mutation, host truth predicate or unsafe metadata issuer is permitted. Private tests cover deterministic reservation/counter overflow, corrupt internal indices/tails/flags, transactional environment construction and failure without borrowed-input mutation. Independent tests must not call private helpers or merely restate implementation loops.

Required independent acceptance groups are:

1. Environment registry/plan identity, closed unions, duplicate physical rows/edges, conflicting ordered edges and aliases, explicit binding collisions with local definitions outside the union, core lookalikes/wrong loaders/kinds, missing facts and historical dependency classes. Prove two environments over the same registry do not share proof authority.
2. Every modern opcode's positive and negative transfer, all legal and illegal category-two stack forms, narrow array kinds, null/array/interface rules, dynamic descriptors without bootstrap execution, reference monitors, malformed flow and wrong returns. All opcodes outside current interpreter support still belong here.
3. Initial frames, absent/empty maps, explicit branch-to-zero maps, dead mapped blocks, terminal/fallthrough boundaries, duplicate switch targets, every normalized map form, three-slot Full [Long, Top] followed by Chop/Append, overlap stores and max_stack/max_locals. Preserve the partial API's acceptance of the two deferred type negatives while proving the new API rejects them.
4. Constructor initial/base state, post-super aliases typed as current class, early exact-own putfield, repeated new-site local invalidation/stack rejection, normal this/super substitution, lost aliases with a live flag, ordinary caught helper before chain, failed separate allocation before chain, and both handler views for failed this/super/allocation calls. Include the original actual-execution probes, not only synthetically accepted dead code.
5. Loader-aware nominal/array relations, necessary versus unnecessary resolution, protected inherited declaring packages and array clone, nearest nonprotected hiding, and the full selected final-override matrix including bodyless methods, private/static exemptions and package bridges.
6. Hand-counted inclusive counters and error precedence; zero/MAX; 10,000-class iterative ancestry; many duplicate/interface edges; 65,535-unit shared descriptors; large locals with 10,000 Same maps under a bound that rules out locals-times-maps copying; late failure with correct accepted usage; immutable result borrows and repeated independent calls.

Genuine target Code remains the original checked-in javac or documented original emitted bytes. Truthful external facts may be supplied through **explicitly synthetic metadata projections** generated by JDK-only tooling: capture exact class names, defining-loader designation, kind/flags, direct hierarchy and declared field/method name-descriptor-access facts, with JDK version, capture inputs and output hashes. Retain the required hierarchy closure. Any inert body required solely to admit a concrete projected declaration is labeled synthetic, is never executed, and is never selected or counted as a genuine method proof. No invented successful constructor, Object body, absent declaration or bootstrap implementation is credited. These are test environment facts relative to the captured library, not a production class loader or a proof of those library bodies. Missing uncaptured necessary facts remain unavailable; coverage reports disclose rejected/unavailable selections rather than silently omit them.

The JDK differential driver must distinguish definition, reflective lookup and invocation. Accepted verification can still end in invocation linkage/type exceptions, such as interface dispatch ICCE or aastore ArrayStoreException; those are positive verifier cases. Invalid final overrides can fail during definition. Expected outcomes are probe-specific stages/categories, never a blanket VerifyError label. Record immutable original probe sources/artifacts and provenance before promoting them into the accepted corpus.

The complete opcode matrix, resource/corruption tests, loader-sensitive facts, declared-map transitions, constructor exceptional paths and independent JDK-stage cases must pass. Preserve the old partial API's two intentionally accepted type negatives; the new checker rejects them. Genuine coverage must say which required dependency facts were supplied and which unavailable bodies could not be checked; do not fabricate successful external-library facts to inflate coverage. Frozen source review, all old regressions, exact final hashes and a fresh-target debug/release gate remain mandatory. Full-class aggregation, historical inference, full loading constraints and execution-gate integration remain named subsequent obligations.

## Handler stack amendment of 2026-11-05

The original dispatch contract (SHA256 A3485554B807791FFBD31E678599C367D29904DAD7F248D77BD0BDE25D01B930) imposed an extra reference-kind condition on the prescribed handler stack. The director authorizes the narrower predicate above. Exception transfer supplies the catch type and pointwise frame assignment permits discarding it to Top. Operations that consume an operand still require their specified input type. See [verification types and frame assignment](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10.1.4) and [handler legality](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10.1.6).

Four original major-65 classes reproduce the distinction under Zulu JDK 21.0.7 with full verification. The Top handler with a void return and the Throwable handler with pop then return both invoke successfully. Top followed by pop or astore_0 fails at reflective lookup with VerifyError. Include all four in stage-aware acceptance and retain the initial-handler error priority for an incompatible prescribed Int slot.

# Modern class type verification

Director-approved implementation contract, 2026-11-05. This document describes a standalone class-level proof against one immutable verification environment. The baseline is the published modern selected-method checker fb585a2.

## Decision and guarantee

Implement a class aggregate in crabspot-verifier. The input is one ordinary canonical ClassId belonging to the supplied environment. Type check every declared method in classfile order, including constructors, class initializers and abstract/native declarations. A successful result establishes the SE 21 modern class type-check condition for that loaded definition relative to the environment's validated hierarchy and frozen initiating-loader facts. The check covers majors 51 through 65 and uses the accepted classfile minor/preview policy. The selected-method signature, Code and type rules are the already reviewed rules. [JVMS 4.10.1](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10.1).

The selected class's methods are the unit of aggregation. Dependency definitions supply facts. A dependency may have an ill-typed method outside the selected class. A subsequent JVM linkage or reflective lookup can separately fail on such a dependency. The prerequisite runtime build_hierarchy rejects a final superclass before returning a hierarchy plan. The verification environment also checks finality when validating the retained edges. The designated bootstrap Object is the unique root. Interface classfiles carry an Object superclass metadata edge. These are established inputs, so the aggregate can consume that evidence in constant time. [JVMS 5.4.1](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.1).

The input boundary is the existing DefinitionRegistry and immutable VerificationEnvironment. Registry admission has already parsed the complete classfile, including CP and attribute placement, declared method uniqueness, bootstrap-table index bounds and class/field/method shapes presently implemented by the parser. The aggregate obtains canonical metadata from that registry and only accepts retained identities. The verifier environment already checks hierarchy names/kinds/closure, consistent aliases, cycles, superclass finality and core identities. Reuse those facts. Extension data, attributes with partial parser support, complete loader/module derivation, preparation-time constraints, runtime resolution, initialization, failure stickiness and execution gating retain their own future obligations. Registry admission rejects module-info/ACC_MODULE before issuing an ordinary ClassId. Test that actual entry boundary and avoid introducing an alternate metadata issuer.

Historical method inference and version-50 fallback require a separate contract. A zero-method major-50 class is still outside this API even though a reference JVM can accept it. Preview eligibility must be checked at the class entry even if it has zero declarations.

## Public API

Add and reexport these public shapes from crabspot-verifier. Names and observable ordering are part of the contract.

```rust
pub struct ClassTypeVerificationOptions {
    pub method: TypeVerificationOptions,
    pub total: ClassTypeVerificationLimits,
}
pub struct ClassTypeVerificationLimits {
    pub max_methods: u64,
    pub max_code_bytes: u64,
    pub max_code_metadata_entries: u64,
    pub max_code_utf16_units: u64,
    pub max_code_work_units: u64,
    pub max_type_symbols: u64,
    pub max_state_cells: u64,
    pub max_edge_checks: u64,
    pub max_type_utf16_units: u64,
    pub max_type_work_units: u64,
}
pub struct ClassTypeVerificationUsage {
    pub methods_started: u64,
    pub code_bodies: u64,
    pub bodyless: u64,
    pub verification: TypeVerificationUsage,
}
pub enum ClassTypeVerificationResource {
    Methods,
    Code(CodeValidationResource),
    Type(TypeVerificationResource),
}
pub struct ClassTypeVerification<'e, 'a> { /* private */ }
impl<'e, 'a: 'e> ClassTypeVerification<'e, 'a> {
    pub fn environment(&self) -> &'e VerificationEnvironment<'a>;
    pub fn class_id(&self) -> ClassId;
    pub fn class(&self) -> &'a ClassFile;
    pub fn usage(&self) -> ClassTypeVerificationUsage;
}
pub fn verify_class_types<'e, 'a: 'e>(
    environment: &'e VerificationEnvironment<'a>,
    class: ClassId,
    options: ClassTypeVerificationOptions,
) -> Result<ClassTypeVerification<'e, 'a>, Box<ClassTypeVerificationError>>;

pub struct ClassTypeVerificationError {
    pub class: ClassId,
    pub method_index: Option<usize>,
    pub usage: ClassTypeVerificationUsage,
    pub kind: ClassTypeVerificationErrorKind,
}
pub enum ClassTypeVerificationErrorKind {
    Registry(RegistryError),
    UnretainedClass { class: ClassId },
    UnsupportedHistory { major: u16 },
    UnsupportedVersion { major: u16, minor: u16 },
    PreviewDisabled,
    Method(Box<TypeVerificationError>),
    AggregateLimit {
        resource: ClassTypeVerificationResource,
        limit: u64,
        required: u64,
        source: Option<Box<TypeVerificationError>>,
    },
    AccountingOverflow {
        resource: ClassTypeVerificationResource,
        source: Option<Box<TypeVerificationError>>,
    },
}
```

Options/limits/usage and resources are Debug, Clone, Copy, Eq, PartialEq. Options and limits implement Default. Usage defaults to zero. Proof is Debug with private fields and construction. Errors have Debug, Display and Error. Expose Registry and Method as direct error sources, and the original clamped method diagnostic as the direct source of a translated aggregate error. Diagnostics use numeric identities, method index and the existing nested structural location. A header failure has method_index None. A declaration limit uses Some(next index) even though the method was never entered. No fictitious PC or method is introduced.

Use default method options. Default total limits in the struct field order above are 65,535 methods, 8,388,608 Code bytes, 4,194,304 Code metadata entries, 16,777,216 Code UTF-16 units, 67,108,864 Code work units, 1,048,576 type symbols, 16,777,216 state cells, 16,777,216 edges, 16,777,216 type UTF-16 units and 268,435,456 type work units. These are host policies. Every aggregate counter and every limit is an inclusive checked u64. Zero and u64::MAX are meaningful. The sole preview switch is options.method.code.allow_preview. The return object retains only the environment borrow, canonical class borrow/identity and usage. Drop each method result, decoded Code and temporary proof state before the next declaration. No per-class vector of results, hashes or expanded frames is needed.

## Deterministic phases and quota arithmetic

1. Validate ClassId through environment.registry.class. Then check retained membership. Return a class-level zero-usage error on either failure. Identity, the environment's already validated superclass condition and header/version checks are fixed constant-time prerequisites. They consume zero declaration or method counters. No second hierarchy traversal, classfile reparse, resolver callback or dependency body proof.
2. Validate the selected header with the same accepted version/minor/preview envelope as whole-Code checking. Preserve UnsupportedVersion for malformed/unsupported version encodings, PreviewDisabled for disabled eligible preview, then UnsupportedHistory for an otherwise accepted major below 51. This class API checks its header before any method even when a historical method body would fail its standalone structural validator. The public registry rejects malformed version encodings at admission. Exercise that public boundary and test the defensive UnsupportedVersion mapping with a private scalar header helper. Use an explicitly preview-enabled registry to obtain a canonical preview class, then exercise both checker options. On a valid empty modern class or interface, succeed with zero usage under all-zero resource limits. A nonempty class visits declarations by ascending original index.
3. At each index, request exactly one Methods unit before invoking the method checker. If the next count exceeds the class limit, do not enter or fund the method. Return AggregateLimit(Methods) with absolute required count, no nested source and the completed prefix usage. Count a funded method as started even if it subsequently fails. No policy-sized reservation.
4. Capture the successful prefix's nine counters. For each Code/type resource independently compute remaining = total_limit minus accepted_prefix using checked subtraction. Pass min(configured_per_method_limit, remaining) to the existing method checker for that resource. Forward allow_preview as configured. Make one checker call per entered declaration. Preserve its phase order and atomic multi-counter charges. Do not run with unrestricted per-method options and discover overuse afterward. Do not retry the method to distinguish errors or pre-scan sibling bodies.
5. On success, checked-add the nine accepted method counters to prefix totals, then increment exactly one of code_bodies/bodyless according to the actual returned Code presence. Reuse only fixed-size scalar data when counting. After the last method the sum of the two completed counts equals methods_started. On failure the completed counts describe only successful declarations. The nine counters include accepted work of the failed method, even where its last requested charge failed atomically. The failed charge is excluded. Any checked arithmetic failure in the wrapper has a structured resource and first failing index. Compute totals in a temporary copy before publishing them.
6. To classify a failure, examine only the direct TypeVerificationErrorKind::ResourceLimit, or the direct CodeValidationErrorKind::ResourceLimit inside TypeVerificationErrorKind::Code. Let P be the configured per-method cap for the reported resource, R the remaining class cap at method entry, U the corresponding prefix before this method, L the reported local limit, and Q the local required cumulative amount from the child error. If R < P and L == R, return AggregateLimit for that resource, with the original configured class limit and absolute required U + Q checked. This formula includes the attempted but unpaid charge exactly once. On U + Q overflow return AccountingOverflow for that class resource, retaining the method error as source. The usage still records paid work. If R >= P, including ties, preserve the full error as Method. It exposes the existing local cap/required/location/usage. For any non-limit failure return Method with the original boxed diagnostic, including nested Code decode errors, final overrides, type-rule failure, missing facts, allocation failures and child counter overflow. Do not scan arbitrary source chains for resource-shaped failures.
7. Class limits cannot turn failure of a later method into successful partial class evidence. Calling again on the same immutable environment is independent and deterministic. Host policy exhaustion can be retried under larger options. JVM linkage error caching belongs to future lifecycle integration.

The ordinary valid path cannot overflow aggregate counters because per-method work is bounded by the checked remaining allowance. Internal overflow/inconsistent-accounting guards must have private tests with scalar fixtures, including absolute U+Q overflow, without constructing enormous classes. Use the declared resource order for checked remainder and checked accepted-sum operations: Code bytes, Code metadata entries, Code UTF-16 units, Code work, type symbols, state cells, edges, type UTF-16 units, type work. For any impossible remainder, accepted-sum or completed-count inconsistency, keep the last representable pre-child nine-counter prefix and both prior completed counts. Keep the funded methods_started for the current index. A checked Methods increment failing before entry leaves the pre-entry count intact. Classify a method, body or bodyless count overflow/inconsistency as Methods. Preserve the actual child error as source when one exists. A pre-call or success-path guard has source None. This transaction precedes classification of an actual child limit. In the separate absolute U+Q translation overflow, the accepted-sum transaction has succeeded, so retain the paid current-method work. Counts of completed bodies/bodyless cannot exceed methods_started.

## Acceptance and ownership

Production owns the verifier-only aggregate API, its bounded accounting, any small private factoring and private scalar corruption/overflow coverage. Existing selected-method and whole-Code APIs and counter behavior are baseline regressions. Independent public testing uses the previously admitted class environment and original classfile builders in its own worktree. Coordinator owns genuine original JDK class-stage rows, augmented corpus tests, source provenance and fresh integration gating. No changes to runtime identity issuance, parser or execution paths are required.

Independent tests cover zero-method modern ordinary/interface and designated Object, zero-method historical rejection, preview enabled/disabled including empty classes, foreign/unretained identity precedence, module descriptor rejection at registry admission, all-good mixed ordinary/constructor/initializer/bodyless declarations, a bad unused sibling before/after the valid method, bodyless final override, and a retained child whose parent contains ill-typed Code. Recheck the child and parent separately. Pin the first failing declaration and nested verifier location. Distinguish a reference JVM's broader reflective lookup on ChildValid from the selected-child guarantee.

Quota tests pin exact method cap and cap+1, every Code/type cumulative resource across multiple bodies, equal-cap ties, a tighter per-method limit, late aggregate failure after earlier and current accepted work, simultaneous limit precedence inherited from the existing checker, zero/MAX and retry determinism. Verify that aggregate successful usage equals the checked sum of independent method usages for identical generous per-method limits. In a many-tiny-method class use a low total allowance to stop at a known declaration. No allocations based on total policy maxima. Review implementation for immediate dropping of earlier decoded bodies.

The genuine corpus gate selects every ordinary genuine definition in the accepted fixture set and compares aggregate body/bodyless totals with the existing independent method loop. Include all 16 stage outcomes from the 15 original planning classes. GoodThenBad, BadThenGood, BadParent, ChildValid and InvalidSecondAfterNative fail getDeclaredMethods with VerifyError. NativeOnly, AbstractOnly and AbstractAndNative return 1, 1 and 2 declarations. EmptyModern, EmptyHistory, EmptyInterface and FinalParent return zero. NativeWithBody fails definition with ClassFormatError and ChildOfFinal fails definition with IncompatibleClassChangeError. EmptyPreview fails definition with UnsupportedClassVersionError without preview and returns zero declarations with preview enabled. Exercise registry, hierarchy, verifier and reflection at their proper stages. The original stage oracle is independent of the class aggregate. Record JDK stages independently with full verification enabled. Report successes by selected definition, never credit synthetic projected JDK library bodies as genuine class proofs, and preserve stage distinctions among registry/hierarchy rejection, method type failure and reflective linkage that also visits a failing parent. Preserve all prior reference sets and the bounded fuzz smoke. Final approval requires immutable worker/readback manifests, actual fresh-target workspace and optimized tests, strict formatting/Clippy, full fixture/source reproduction and hosted evidence after publication.

The existing corpus study covers 173 ordinary genuine classes and 966 declarations. Maximum observed usage by class in the ten limit-field positions is 75, 4,400, 937, 9,524, 18,882, 842, 3,665, 4,037, 59,710 and 73,059. The maxima come from different classes. They establish compatibility headroom for these defaults on this corpus. Preserve all 935 Code and 31 bodyless declarations when expanding the fixture set.

## Dispatch gate

Coordinator, production and independent-test readbacks completed. The director authorizes bounded implementation and independent test work from these exact contract bytes once the coordinator dispatches the isolated worktrees. Publication requires a later frozen source audit and an executed combined gate. Whole-JVM linking, historical verification and M2 completion remain open.

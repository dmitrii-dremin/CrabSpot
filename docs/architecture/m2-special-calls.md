# Ordinary nonconstructor special calls

Status: implemented and approved by the technical director on 2026-11-04; see `m2-special-calls-review.md` for frozen-source review, the accounting correction and final evidence. This follows the approved interface/default increment and supersedes `m2-special-calls-next.md`. The contract below defines the accepted bounded scope. Full M2 remains open.

## Scope and API

Enable invokespecial (b7) for numeric/void ordinary method bodies in the existing runtime-aware engine. Support explicit superclass calls, direct-superinterface defaults, ordinary current-type calls and legal private current-type calls. Preserve the host-selected ordinary class root and explicitly supplied instance storage. Constructors, class initialization, new, reference parameters/results, native binding, monitors and exception dispatch remain excluded.

Add:

```rust
impl FieldResolutionContext<'_> {
    pub fn select_special(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
    ) -> Result<MethodId, MethodAccessError>;
}

// Add to MethodAccessError:
InvalidSpecialOwner { origin: ClassId, owner: ClassId }
```

The new error is a host structural-check failure, not a Java linkage outcome. It needs Display but no artificial source. Reuse existing Java categories, Context errors and UnsupportedSpecialMethod for initialization names. Keep existing public options and execution APIs unchanged.

The runtime worker owns runtime production/services and synthetic runtime tests. The interpreter worker owns its production and synthetic call tests. The coordinator owns genuine fixtures/oracles/integration and final evidence. Use separate worktrees and output directories. No NanoSpec, extra engine, copied hierarchy, second resolver, global cache or reference-JDK application execution is permitted.

## Symbol and resolution phase

Check unchanged context, registered/admitted origin and its immutable class-file version. Methodref is supported throughout the existing version range. InterfaceMethodref is supported for b7 only when the origin's major version is at least 52. Wrong tag, index or version is InvalidConstant { origin, index }. Correctly tagged owners with the wrong class/interface kind retain Java IncompatibleClassChange after symbolic-owner accessibility checking. Array owners remain UnsupportedArrayOwner. Reject `<init>` and `<clinit>` explicitly; do not resolve a constructor and then execute it as an ordinary instance body.

Parse the numeric/void descriptor and reuse class/interface method resolution, signature-polymorphic boundaries, exact loader/name bindings and member access from the preceding contracts. Public inherited declarations do not acquire a second declaring-type accessibility check. Same-class/interface private access works; other private access stays UnsupportedNestAccess. Named modules, nest resolution and reference-type loading constraints remain outside this profile. A resolved static method produces Java IncompatibleClassChange before null.

Preserve the existing class-only subclass predicate for protected access: an interface's structural Object dependency does not make that interface a subclass of Object. In particular, an interface origin outside Object's runtime package cannot gain protected Object-method access through that edge. The coordinator's transient original classfile probe on Zulu OpenJDK 21.0.7 observed IllegalAccessError for a default interface body invoking Object.finalize()V through b7, while the public Object.hashCode()I control executed successfully. This supports the existing access boundary; it does not authorize reference-JDK execution in production or comparison of unspecified identity hash values. Include a synthetic access-order regression using explicit Object metadata.

The normative method resolution/access contracts remain [JVMS 5.4.3–5.4.4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.3). Do not implement special calls by calling select_virtual/select_interface: those services combine resolution with a different receiver-directed selection procedure. Refactor internal resolution helpers where needed while preserving old behavior and exact class-path budget regressions.

## Owner and receiver checks

After successful nonstatic resolution, validate the symbolic owner's relationship to the origin. The supported structural relationships are:

- The owner is exactly the current origin type.
- Both are classes and the owner is a strict superclass of the origin.
- The owner is an interface in the origin's direct-interface list, by exact ClassId.
- The origin is an interface and the owner is its exact admitted Object dependency.

An indirect superinterface alone is insufficient, as is an unrelated/sibling type or another loader's identically named type. Report InvalidSpecialOwner with the checked origin and symbolic owner. Treat this as a defensive implementation of the required structural boundary, not a claim of bytecode verification or complete malformed-program error precedence. Own/direct-interface matching must not be replaced by arbitrary subtype reachability. Constructor exceptions to these rules are deliberately absent.

Next, null yields Java NullPointer. For non-null references, check heap identity/generation/kind, obtaining an ordinary instance's actual ClassId. It must satisfy both the origin type and symbolic owner type: use class ancestry for a class, and complete interface membership for an interface. Failure is Context(InvalidReceiver { expected, actual }), checking origin first. Never accept an arbitrary instance merely because it inherits the selected superclass body. These checks implement the receiver-at-or-below-caller constraint, which is stronger than the usual protected receiver restriction. A structural interface Object edge is not an interface inheritance edge.

The relevant format/structural constraints are [JVMS 4.9.1–4.9.2](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.9.1); [the invokespecial verification explanation](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10.1.9.invokespecial) requires compatibility with both current and symbolic types. Exact failure order for invalid owners is this host policy: resolution/access/staticness first, owner relation next, then null/receiver checks. Supported Java failure ordering is preserved independently.

## Special selection

Maintain three distinct values internally: symbolic owner T, resolved declaration R, and selection start C. Selection must follow [JVMS invokespecial](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.invokespecial), using the name/descriptor already resolved.

For a class origin D and a class symbolic owner T that is a strict ancestor of D, C is D's direct superclass. Otherwise C is T. Java SE 21 treats ACC_SUPER as set regardless of its stored bit and class-file version; do not resurrect pre-Java-8 behavior for raw-bit-clear old files. See [JVMS 4.1](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.1). An interface-origin Object reference starts at its explicit Object dependency.

Search C's own exact instance declaration, then its class superchain if C is a class. When C is an interface and lacks an own instance declaration, consider its explicit Object dependency's own public instance declaration. If none is selected, use the shared maximally-specific candidate analysis over C's superinterfaces, including inherited interfaces from its class superchain. Exactly one concrete maximal candidate selects that body; multiple produce Java IncompatibleClassChange; none produces Java AbstractMethod. An abstract class declaration found earlier masks defaults and yields Java AbstractMethod. Null has already won before these selection failures.

This lookup tests exact instance declarations, not whether they override R. Skip static declarations during selection, while retaining the earlier rejection of static R. Do not skip private instance declarations solely because virtual selection would skip them, and do not impose invokeinterface's public/private selected-method visibility rule on special calls. Access belongs to resolution; selection must not re-resolve a different CP reference or reapply access as if the selected declaration were the symbolic target. In malformed hierarchies, these limited checks do not establish full verifier validity.

The actual receiver class must not choose a different override or contribute unrelated interfaces to special selection. A child overriding m cannot intercept its parent's explicit super.m. A direct-superinterface call can select that interface's inherited default even when another interface on the receiver has a competing default. Conversely, conflicting candidates within C's own applicable closure remain an error.

Before returning the target, require its declaring class/interface to be compatible with the actual receiver using the same checked identity graph. This catches inconsistent supplied Object identities without fabricating a Java linkage result. Native/synchronized targets are returned for the interpreter's existing unsupported preparation checks; no UnsatisfiedLinkError or monitor behavior is invented. Existing virtual/interface final-override defenses remain unchanged; special lookup does not claim a new whole-class verifier or use virtual target selection as a substitute.

## Budgets and execution

Use the existing context's symbol/unit/work budgets and invalidation rules. Charge each new owner-relation node/edge examination and selection declaration scan before work. Direct-interface scans and the interface-origin Object dependency check charge before comparing each edge; interface membership/dominance retain the approved pre-enqueue and candidate-pair rules. Reuse fallible temporary reservations and iterative traversal. No new permanent cache or all-pairs graph matrix is needed. Failed checks may consume work and retain successful symbol bindings as before, but may not mutate object values.

Add CallKind::Special to the existing interpreter action. Runtime mode enables b7; standalone and heap-only entry points continue rejecting it. Extend the shared CP/descriptor extraction to admit the appropriate method-reference kinds for special calls, with the same version rule as runtime. Name/reference-descriptor rejection precedes typed stack extraction as in other instance calls. b7 has no count operand. Malformed stack values remain structured host errors.

Invoke select_special with the current frame's declaring ClassId, not the root or actual receiver class. Prepare the selected MethodId using ResolvedInstance mode and transfer receiver/arguments through the same frame path. Special/virtual/interface hits on one MethodId share one prepared-code charge; every hit still checks static-versus-instance eligibility. Direct interface roots remain rejected. A selected body uses its own CP, fields, descriptors and diagnostic owner; its later virtual/interface calls perform normal receiver dispatch, and its own static helpers retain the existing narrow policy.

Fuel, active frame/slot limits, prepared bytes, cumulative lookup work and heap effects remain shared. Preserve return narrowing/FP behavior, remaining caller operands, transition-site diagnostics and all prior effects after subsequent Java/host failures. No constructor state, uninitialized-reference substitution, initialization mark or general reference assignability claim is added.

## Acceptance evidence

Runtime tests must cover current-class/private/current-interface cases; direct and older superclass references; raw ACC_SUPER clear under old/current majors; same-name overloads; direct-interface inherited defaults; class-super fallback to defaults; selection confined to C rather than actual receiver; static resolution versus static selection candidates; private selection candidates where resolution differs; abstract masking; default conflicts/absence and null ordering; Object lookup priority; legal owner relations and rejected indirect/sibling/foreign-loader identities; receiver compatibility with origin and owner; access/private-nest boundaries; native/synchronized targets; exact budgets, bindings/invalidation, deep iterative traversal and private malformed-handle guards.

Interpreter tests must exercise genuine super/default-super bodies, all numeric/void returns and mixed-width arguments, hidden-field/provenance correctness, special calls followed by virtual/interface/static calls, MethodId/cache reuse across call kinds, typed malformed stacks, version/tag/initialization-name rejection, unsupported old entry points and direct interface roots, eager callee decoding, exact shared limits, deep frames, failure provenance and persistent effects. Preserve existing regression cases rather than simply deleting old b7 rejection checks; update only the runtime-aware expectation with a concrete new result/error.

The coordinator supplies original Java 21 fixtures and independent JDK outputs. Synthetic inputs cover encodings javac does not emit; distinguish normative executable cases from malformed host-defense tests. Preserve the accepted 10,669 execution comparisons, both 151-case service oracles and 59-class corpus. Final gate requires frozen source review, formatting, strict workspace/all-target Clippy, full debug tests, optimized runtime/interpreter tests, all fixture/reference reproduction and expanded-corpus parser smoke. Hosted Linux/Windows validation is recorded separately. No full-M2, complete-conformance or performance claim follows.

# Checked non-null instance-reference fields

Status: bounded implementation approved by the technical director on 2026-11-04 after frozen review and the passing integrated gate recorded in `m2-reference-fields-review.md`. Implementation used separate runtime/interpreter worktrees based on 5257f99. Fresh hosted validation remains pending publication. Full M2 remains open.

## Scope and public boundary

Extend FieldResolutionContext::write_field and existing runtime-aware putfield (b5) to store assignment-compatible non-null ordinary-instance and primitive-array references. Existing read_field/getfield, reference locals, casts and method receivers then consume these values. Keep all public method signatures, execution entry points, heap options and opcode gates. This increment adds behavior to existing instructions, not new opcodes.

Keep ManagedHeap::put_field conservative: every non-null reference remains rejected with InstanceError::UnsupportedReferenceStore for a reference field, even if a universe is bound and the value would be assignable. Its numeric/null behavior and error order remain unchanged. Callers needing checked reference writes must use the context. Do not expose a public unchecked setter, typed-store token constructor or mutable payload/location API.

Export one diagnostic struct and add two FieldAccessError variants:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceFieldMismatch {
    pub field: FieldId,
    pub source: ReferenceType,
    pub target: ReferenceType,
}

// Add to FieldAccessError:
ReferenceType(Box<TypeAccessError>),
IncompatibleReference(Box<ReferenceFieldMismatch>),
```

The boxed reverse TypeAccessError wrapper avoids recursive enum sizing with TypeAccessError::Context(FieldAccessError). Preserve Display and Error::source for ReferenceType. IncompatibleReference is a host validation failure with useful source/target identities and no artificial error source. It is neither ClassCastException nor ArrayStoreException. FieldAccessError's existing Java variants and interpreter mapping remain unchanged; never reinterpret a nested TypeAccessError::Java as an instruction-level Java outcome.

No public new write method is needed. Raw read and children APIs stay unchanged. Reference-array allocation, reference parameters/results, constructors, initialization, final-field writes, static storage writes, verifier, loading-constraint enforcement, handlers, native binding, monitors, collection and object movement remain outside this increment. No NanoSpec.

## Exact checked-write order

Preserve the existing prefix of write_field for every value:

1. Check context validity and origin/Fieldref structure, resolve the symbolic owner and exact field through the existing binding graph, perform owner/member access and the existing loading-constraint boundary.
2. Reject a static selected field with Java IncompatibleClassChange.
3. Reject a final selected field with Java IllegalAccess. This includes null stores and stores from the declaring class, because constructor execution remains unsupported.
4. Check the receiver: null gives Java NullPointer; validate its heap identity, instance kind and admitted type, symbolic owner compatibility and protected receiver restriction as before.
5. Check field storage kind against the Value computational kind. Wrong kind retains Instance(Heap(ValueKindMismatch)); it must not become MissingUniverse or a source-handle error. Numeric values and null references follow the old raw-store path, including numeric narrowing, with no new universe requirement, descriptor resolution, callback, symbol insertion or work charge.

Only a non-null Reference for a reference field enters the new branch:

6. Use reference_type(value) to require the bound universe and validate the source handle and admitted nominal/primitive-array type. A foreign/stale/missing value is rejected before resolving the field descriptor type.
7. Resolve the selected field's reference descriptor identity using its declaring ClassId and defining loader, as specified below. A missing/unadmitted target remains a checked host error; no implicit class creation occurs.
8. Use the shared is_assignable(source, target) relation. False produces IncompatibleReference with the exact selected FieldId and types. Failure or exhausted policy leaves the field unchanged.
9. Commit the exact Reference to the selected logical slot through the private boundary below. No value substitution or allocation occurs.

Preserve all existing outcome ordering for resolution, access, static/final and receiver errors. The extra source/type ordering is an explicit host policy for a kernel without verification. [JVMS putfield](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.putfield) requires assignment-compatible reference values and restricts final writes to initialization methods; it specifies static/final failures before the null receiver failure. This increment does not manufacture a Java type exception for an unverified incompatible store.

A useful implementation is to inspect the selected immutable descriptor's first unit to identify reference storage (L or '['), then preserve the existing path unless both it and the Value select the non-null reference case. Do not call the raw setter speculatively and depend on an error as the successful checked branch. The final private commit must still defend the actual shape kind.

## Descriptor identity and loading boundaries

Read the descriptor from the selected FieldId's declaring class, not merely the initiating Fieldref's owner or the receiver's actual class. For a nominal descriptor Lname;, resolve name using that declaring loader. For an array descriptor, preserve rank and primitive/nominal leaf identity and defining loader using the accepted structural type representation. Reuse immutable UTF-16 slices and the existing resolver/binding cache; nominal leaf targets require pre-admitted layouts. No descriptor tree or second type cache is retained.

Separate type identity resolution from cast target accessibility. The field's caller need not be able to name the field type, and this store must not perform a new public/same-package or nest access test on that descriptor type. An accessible public field can legally have an inaccessible nominal type. Refactor the type parser with an internal explicit access policy if helpful, retaining cast resolution's existing checks and exact work counts. Do not weaken public resolve_reference_type or check_cast to accommodate field stores.

The existing cross-loader boundary remains exact: if a field descriptor has a nominal leaf and the initiating origin and selected declaring type have different defining loaders, select still returns UnsupportedLoadingConstraint before static/final/null handling, for reads, numeric-kind mistakes, null and non-null writes alike. Equal descriptor spelling or currently equal resolver answers does not remove that boundary. Primitive-leaf array descriptors retain their existing behavior and derive array identity from the designated bootstrap loader. The value's actual defining loader need not equal the declaring loader; assignment uses actual resolved ClassIds and the admitted graph.

This preserves rather than implements the broader obligations of [JVMS field resolution and loading constraints](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.3.2). Removing the restriction requires a later concrete constraint/lifecycle design.

## Private commit and edge enumeration

Add only a crate-private heap operation, named commit_reference_field or equivalently explicit, invoked by the checked field service after successful assignment. Recheck receiver ownership/index/generation, instance kind, owner-qualified FieldId location, nonfinal reference storage, source handle ownership/index/generation and destination slot bounds before the final mutation. Null may be supported internally, but must not be used to bypass the old public path. The helper does not independently resolve descriptors or assert constructor state; its caller owns the assignment proof. Never route public put_field through this helper in a way that allows non-null values.

There are no intervening external callbacks between successful relation validation and commit, and the context holds exclusive mutable access to the heap. The commit changes exactly one Value::Reference slot. Allocation counts, layout counts, payload/instance-slot totals and heap identity do not change. Every failure leaves old contents and child enumeration intact; consumed work and successful symbol bindings may remain, as in existing services.

ManagedHeap::children already returns borrowed HeapRef values. Preserve one result for every non-null reference-valued logical location in stored slot order. Duplicate aliases appear repeatedly; self-links and cycles are ordinary edges. Null locations and numeric fields are omitted. Primitive arrays have no children. Validate the parent handle when creating the iterator; production stores establish child-handle validity. Do not recurse, deduplicate, allocate, mutate references or consume heap quotas while enumerating. The iterator is not a mutable root-location visitor and does not implement tracing or reclamation.

## Work accounting and engine behavior

Reuse the context's cumulative symbol/unit/work policy. Non-null descriptor identity resolution charges one LookupSteps unit for each UTF-16 descriptor unit before scanning or hashing it, including L/semicolon delimiters or array brackets. Charge the full descriptor once per checked store, including binding hits; do not add another full-name charge through a refactored helper. Null/numeric stores retain their previous counts. Shared source-type validation and is_assignable keep their accepted validation, type-pair, node and pre-enqueue edge charges.

The symbol cache records the nominal name/leaf slice, not an extra descriptor entry; symbol and UTF-16-unit limits are checked before resolver callbacks. No successful assignment cache is introduced: receiver, field and source are checked on each write. Temporary traversal reservations stay fallible and iterative. Existing allocation bounds continue to bound metadata and payload; storing an edge allocates no new heap object or payload.

The existing interpreter b5 path should need no new production algorithm: it already pops value/receiver, supplies the current frame's origin and calls write_field. New host diagnostics stay in ExecutionErrorKind::FieldAccess with offset, method and transition-site provenance. Existing b4 can push non-null references subsequently used by locals, equality/null branches, casts, primitive-array operations and accepted instance call forms. Earlier entry points still reject b4/b5; method descriptors remain numeric/void. Preserve fuel, frame/slot/code limits, and earlier writes or allocations after later failures.

## Implementation ownership and gate

The runtime worker owns runtime production and synthetic service tests. The interpreter worker owns new public execution tests, diagnostic-mapping regressions and documentation; change production only if necessary to preserve the stated boundary, without a new engine or entry point. The coordinator owns original Java fixtures, independent JDK outputs, genuine integration, final checks and publication. Use separate worktrees and output paths.

Runtime acceptance must cover:

- Self-links, distinct objects, aliases, overwrite, null clearing and cycles; inherited and hidden FieldIds; exact child slot order with repeated handles; no recursive enumeration or quota changes.
- Nominal/interface/Object assignment, all eight primitive arrays to exact arrays and selected core supertypes, incompatible nominal/array/rank targets, and loader-separated identities.
- A public accessible field whose nominal type is inaccessible to the initiating caller, without introducing cast-style type access. Correct declaring-owner descriptor identity, including inherited fields and UTF-16 names.
- Bound/unbound universes; old null/numeric behavior including missing descriptor targets; wrong computational kinds; foreign/stale/missing value and receiver handles; exact failure precedence and unchanged prior field contents/children.
- Continued raw public non-null rejection even for assignable values; private commit guards for final/wrong-kind/unrelated/foreign fields and corrupt slots; no public bypass.
- Preserved nominal cross-loader UnsupportedLoadingConstraint for reads and writes, and legal primitive-array cross-loader cases with explicit core delegation.
- Exact descriptor/cache-hit and cumulative work limits; symbol/unit exhaustion before callbacks; late assignment-budget failure atomicity; deep graph assignment and long borrowed descriptors without recursion.

Interpreter/genuine coverage must include store/get/use of this and primitive arrays; owner-qualified inherited/hidden references; aliases, clearing and field cycles; subsequent virtual/interface/special dispatch and casts through retrieved values; current-frame origin; final/static/access/null ordering; malformed incompatible stores as host errors; persistent earlier effects and preserved provenance after Java/host failures. JDK fixtures must use default instance state and selected numeric wrappers, not execute constructors in CrabSpot. Distinguish independently recorded state from source-derived assertions.

Preserve 12,395 prior execution comparisons, both 151-case service oracles and the 74-class corpus. Acceptance requires frozen technical review, fmt, strict workspace/all-target Clippy, full debug tests, optimized runtime/interpreter tests, genuine fixture/reference reproduction and expanded parser smoke. Hosted Linux/Windows evidence is recorded separately after publication. Full M2, full JVM conformance and performance superiority remain unapproved.

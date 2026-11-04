# Bounded interface and default-method dispatch

Status: bounded implementation approved by the technical director on 2026-11-04; see `m2-interface-dispatch-review.md` for source review and independent evidence. This supersedes the design-only `m2-interface-dispatch-next.md` and extends the approved virtual-call contract. Full M2 remains open.

## Scope and ownership

Enable invokeinterface (b9) in the existing runtime-aware instance-body engine and complete interface fallback for invokevirtual (b6). Support numeric explicit parameters, numeric/void results and an implicit ordinary-instance receiver. Concrete default bodies and same-interface private helpers may execute after runtime selection. The existing host API still selects only ordinary class roots and receives explicitly supplied storage. No new construction, initialization, verification or application-startup claim is made.

The runtime worker owns runtime method services and runtime tests. The interpreter worker owns interpreter production and synthetic interface-call tests. The coordinator owns genuine Java fixtures, JDK outputs, independent integration and final checks. Workers use separate mutable worktrees/output directories. Preserve all accepted virtual/class behavior and earlier numeric/heap/field APIs. No NanoSpec is used.

## Public API and shared context

Add this method to the existing context, with the same lifetime and ownership model:

```rust
pub fn select_interface(
    &mut self,
    origin: ClassId,
    index: u16,
    receiver: Reference,
) -> Result<MethodId, MethodAccessError>;
```

Keep `select_virtual` unchanged publicly. Both services may now return a MethodId declared by an interface. Keep existing MethodJavaError and MethodAccessError categories; no new Java outcomes are required. Retain the public UnsupportedInterfaceMethods variant for compatibility, but supported interface fallback must no longer emit it. Malformed CP tags/indices remain InvalidConstant, and reference descriptors, special names, signature-polymorphic execution and cross-class/interface private nest access retain their explicit host boundaries.

Use FieldResolutionContext's admitted graph, loader/name bindings, heap identity checks and cumulative budgets. Origins and resolved owners must be registered/admitted. No second resolver, copied hierarchy, implicit loading, dispatch cache or global candidate cache is introduced. Namespace identity is ClassId, never a displayed name. `select_interface` requires InterfaceMethodref; `select_virtual` still requires Methodref. Wrong CP kind is a host InvalidConstant, whereas a correctly tagged symbolic owner of the wrong class/interface kind is Java IncompatibleClassChange after symbolic-owner access checking.

## Resolution and selection contract

Implement the normative algorithms in [JVMS 5.4.3.3 and 5.4.3.4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.3.3), with access/overriding/selection from [5.4.4–5.4.6](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.4). In particular:

- Class lookup retains its existing priority; interface lookup first checks its own declaration, then Object's public nonstatic declaration, then superinterfaces.
- Eligible inherited interface declarations exclude private/static methods. Maximal candidates exclude declarations shadowed by a matching declaration in a strict subinterface, including abstract redeclarations.
- Resolution prefers the unique nonabstract maximal declaration. Otherwise it may choose an eligible declaration; this implementation chooses the first in deterministic graph traversal order. An empty set yields NoSuchMethod.
- Receiver selection is distinct: private resolved declarations retain their target; eligible class declarations precede interfaces. Without a class target, one nonabstract maximal candidate succeeds, multiple yield IncompatibleClassChange, and zero yield AbstractMethod.

These rules must be implemented once as reusable internal candidate analysis rather than diverging virtual/interface approximations. Determinism is an implementation choice within the specification's permitted resolution variation; tests must not make a selected runtime body depend on an arbitrary abstract resolution witness.

### Concrete graph policy

For class lookup fallback, collect interfaces from the symbolic class and every superclass. For interface lookup fallback, begin with that interface's direct superinterfaces, excluding its own declaration. For receiver selection, collect interfaces from the entire actual class chain, including unrelated interface branches. Deduplicate diamond paths by ClassId. Traverse direct interface lists in class-file order and retain first discovery order; arbitrary-resolution choice uses the first eligible declaration in that order.

For each exact name/descriptor candidate, retain its declaring interface and MethodId. A candidate is dominated if another eligible matching candidate's declaring interface is a strict subinterface of its owner. Private/static declarations do not dominate. Unrelated abstract peers do not suppress a concrete default; a more-specific abstract redeclaration does. Duplicate paths to one declaration never create a conflict. Do not prefer the interface named by the call site when the actual receiver has two competing unrelated defaults.

Object fallback reads the explicit admitted Object dependency of the symbolic interface and only its own declared matching method. Do not fabricate Object metadata or implementation, consult a host JDK, follow Object as a superinterface, or search Object superclasses. Require the existing graph/metadata invariants. The synthetic test environment may provide explicit original minimal Object metadata. If a deliberately inconsistent multi-loader test graph causes a resolved Object declaration not to belong to the actual receiver's class ancestry, return checked host InvalidReceiver/InvalidState rather than forging method ownership or a Java linkage result.

Retain existing package-aware class overriding and full-chain defensive final guards. For an interface-declared public resolved method, the nearest matching nonprivate instance class declaration is eligible; its declaring class need not itself list that interface. This permits inherited class implementations. Static/private class candidates are skipped. A package/protected candidate may be selected; invokeinterface's later visibility rule decides its legality. Do not reuse the old requirement that every resolved declaration must occur in the receiver class chain for an interface-declared method. Private interface resolution selects that exact declaration and never a same-name class method.

### Failure order and boundaries

Follow [invokeinterface](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.invokeinterface) and [invokevirtual](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.invokevirtual): resolution/access and resolved staticness precede null; null precedes receiver membership and selection. For invokeinterface, a non-null ordinary receiver that does not implement the symbolic interface yields Java IncompatibleClassChange. Selected methods must be public or private before abstractness is tested. Missing concrete selections and default conflicts remain distinct AbstractMethod/IncompatibleClassChange outcomes. Native and synchronized support is checked by preparation, not invented as a successful invocation.

Foreign/stale/wrong-kind handles, malformed metadata, policy limits and unsupported profiles remain host failures. Implement interface membership by identity through the admitted graph, excluding interface Object dependencies from interface ancestry. Private access is supported only when origin equals the declaring class/interface; other private access remains UnsupportedNestAccess. Public inherited interface declarations do not require a second declaring-interface accessibility check. Named-module and nest services remain deferred. Primitive/void descriptors introduce no reference-type loading constraints.

A conflict found while examining symbolic superinterfaces must not cause resolution to throw IncompatibleClassChange: resolution can choose a declaration and the subsequent null check may win. Conversely, a class abstract declaration masks defaults and produces AbstractMethod on a non-null receiver. Unsupported/malformed-input ordering outside this profile remains a documented host boundary, not a claim of full verification.

## Resource accounting

Retain current context option types and ordinary class-path accounting so accepted exact-budget regressions remain valid. Interface work uses the same LookupSteps counter. Charge before each examined graph node, traversed interface edge, inspected method declaration, candidate-pair comparison and ancestry/dominance traversal step. Charge an edge before enqueueing its target, including duplicate edges subsequently skipped by visited checks. This prevents high fanout from allocating a whole work list after the last available step.

All traversals and destruction are iterative. Use fallible reservations based on actual discovered graph entries. Temporary candidates/visited/work structures are bounded by admitted graph data; no all-pairs transitive-closure matrix is required. A pairwise dominance algorithm is acceptable initially when every comparison and graph traversal consumes the shared budget, even if worst-case time is quadratic or greater. Do not introduce an uncharged full graph scan in a helper, recursive DFS, or cloned class metadata. Temporary analysis is dropped after each selection; there is no new permanent cache policy. Failure may consume lookup budget and retain successful symbol bindings as before, but cannot mutate heap field values or invent a selected target.

## Interpreter integration

Keep `execute_instance_method_body` and all public execution options unchanged. Runtime-aware mode enables b9; earlier standalone/heap entry points continue rejecting it. The decoder already represents InvokeInterface { index, count } and rejects zero count/nonzero reserved byte eagerly. Preserve those checks, including unreachable malformed encodings.

Before extracting arguments at a reached b9, validate the correct CP tag, method name and supported numeric descriptor, then require `count == 1 + parameter_slots`, with long/double contributing two and every other parameter one. Use checked arithmetic; a required count above 255 cannot match. Mismatch returns the existing ExecutionErrorKind::InvalidOperands at the invocation offset, before runtime resolution or stack extraction. Unsupported reference descriptors keep UnsupportedDescriptor before this count check. See [JVMS 4.9.1](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.9.1) for the static count constraint. This is a reached-instruction defensive check, not a full verifier; do not claim unreachable CP/count validation beyond the decoder's encoding checks.

Replace the internal boolean call action with an explicit static/virtual/interface discriminator. Reuse typed argument extraction, receiver transfer, the shared loop and frame reservation. Call `select_interface` or `select_virtual` with the current frame's declaring ClassId and CP index. Only explicitly selected MethodIds enter the resolved-instance preparation mode. Use a deliberate internal distinction between Static, DirectInstanceRoot and ResolvedInstance preparation. Direct interface roots remain rejected even when supplied a compatible ordinary receiver; resolved concrete interface bodies may execute. Numeric/void descriptor, special-name, native/synchronized/handler and eager code-decoding exclusions remain in force.

Prepared entries stay keyed by complete MethodId. Store/check static-versus-instance eligibility on every hit; a body first reached via b6 and then b9 shares one prepared-code charge. Root-only restrictions are enforced before root execution and cannot be bypassed by cache state. The receiver occupies local zero; numeric args follow at their normal widths. Interface bodies use their own CP, field/method origin and diagnostics. Same-interface static helpers may use the existing current-class invokestatic policy, including its InterfaceMethodref version/kind rules; general cross-class or inherited static resolution remains unsupported.

Calls share fuel, active-frame/slot limits, prepared-code allowance, resolution budgets and heap effects. Keep existing return narrowing/FP rules and transition-site diagnostics. Only MethodAccessError::Java becomes a symbolic Java outcome. Existing writes/allocations persist on every later failure. No second engine, reference-valued API, invokespecial, constructors, initialization, handlers, native binding or monitor support is added.

## Acceptance

Runtime tests must cover own/interface/Object lookup priority; exact matching versus overloads; public/nonstatic-only Object fallback; private/static superinterface exclusion; same-interface private dispatch and unsupported nests; class implementations and abstract masking; inherited implementations whose declarer does not implement the interface; reabstraction; unrelated abstract peers; diamonds; competing defaults and null precedence; selected nonpublic class methods; symbolic wrong kind versus wrong CP tag; wrong receiver membership including same-name/different-loader interfaces; native/synchronized selection; existing final/package class regressions; context invalidation/bindings; exact shared work/symbol limits; deep iterative interface traversal and bounded dominance failure. Include deterministic private reservation/identity guards where public construction cannot exercise them.

Interpreter tests must cover b6 fallback and b9 reaching the same interface MethodId/cache entry; distinct interfaces with equal local method indices; interface-to-interface and default-to-class calls; private interface helpers; own static helpers; every numeric/void result, mixed widths and raw floating values; null/no-such/access/static/abstract/conflict order; count 1, mixed-width count, exact 255 and mismatch/overflow; reserved/zero encoding; malformed CP/stack; host interface-root rejection; old-mode b9 rejection; cache-mode defenses; deep calls and exact resource limits; owner-aware provenance and persistent effects before Java/host failures. Update old UnsupportedInterfaceMethods assertions deliberately to test the newly supported behavior and its errors; preserve their fixtures and relevant guarantees.

The coordinator supplies original javac 21 interface hierarchies and independent JDK outputs for defaults, class overrides, inheritance, private helpers, mixed arguments, recursion, fields, arrays and failures. Use synthetic classfiles for conflicts and malformed encodings that javac correctly refuses. Explicit Object tests must not pretend the kernel supplies the Java library. Preserve all 9,332 prior execution comparisons, both 151-case service oracles and the 43-class baseline corpus. Require formatting, strict workspace/all-target Clippy, full debug tests, optimized runtime/interpreter tests, fixture/reference reproduction, expanded-corpus parser smoke and director review of frozen sources. Record hosted Linux/Windows evidence separately. Passing this increment does not close full M2, conformance or performance goals.

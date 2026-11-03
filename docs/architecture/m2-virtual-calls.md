# Bounded virtual calls across registered class bodies

Status: bounded implementation approved by the technical director on 2026-11-03; see `m2-virtual-calls-review.md` for source review and independent evidence. This supersedes `m2-instance-calls-next.md`. Full M2 remains open.

## Supported increment

Add invokevirtual for ordinary class methods with numeric parameters and numeric/void results to the existing runtime-aware instance-body entry point. Support inherited declarations, runtime overrides, private same-class calls, final methods, package-aware transitive overriding and recursive cross-class body graphs. Continue using explicitly supplied ordinary-instance storage. No constructor, initialization, verification or general Java startup claim follows.

Use the current FieldResolutionContext as the shared execution context for this increment; its public name and existing methods stay compatible. Add reusable method services inside runtime, with no interpreter dependency. Reuse its exact loader/name bindings, admitted graph, identity/invalidation checks and cumulative resource policies. Do not clone the hierarchy or add a second implicit resolver.

The runtime worker owns runtime production and method-resolution tests. The interpreter worker owns interpreter production and synthetic call tests. The coordinator owns genuine Java/JDK fixtures, independent integration and final checks. Separate worktrees remain mandatory.

## Runtime API

```rust
impl FieldResolutionContext<'_> {
    pub fn select_virtual(
        &mut self,
        origin: ClassId,
        index: u16,
        receiver: Reference,
    ) -> Result<MethodId, MethodAccessError>;
}

pub enum MethodJavaError {
    NoSuchMethod, IllegalAccess, IncompatibleClassChange,
    NullPointer, AbstractMethod,
}
pub enum MethodAccessError {
    Java(MethodJavaError),
    Context(FieldAccessError),
    InvalidConstant { origin: ClassId, index: u16 },
    UnsupportedDescriptor,
    UnsupportedSpecialMethod,
    UnsupportedSignaturePolymorphic { method: MethodId },
    UnsupportedInterfaceMethods { owner: ClassId },
    UnsupportedNestAccess { origin: ClassId, owner: ClassId },
    InvalidOverride { method: MethodId, overridden: MethodId },
}
```

Export these types; provide Clone/Debug/Eq/PartialEq and Display/Error consistent with existing errors. Context wraps registry, heap, resolver, policy, allocation, context-change and invalid-receiver errors using the existing FieldAccessError variants. Do not map Context errors to Java outcomes merely because an inner type has a Java variant; the method service translates supported access failures explicitly into MethodJavaError.

`select_virtual` performs distinct internal resolution and selection phases. It returns the selected ordinary class MethodId after abstract-target checking. Selection is not executable-code preparation: native/synchronized targets may be returned, and the interpreter must reject their execution using its existing unsupported-method boundary. It does not bind native code or report an invented UnsatisfiedLinkError. No new context constructor/options are required.

Charge all graph-node visits, compared method declarations and override-relation work to the existing cumulative LookupSteps budget, before work. This includes class lookup, interface-fallback scans, ancestor walks and final/override checks. Existing symbol/unit limits cover new owner bindings. Temporary vectors/sets use checked fallible reservations proportional to actual visited graph data; no recursion or unbounded side cache. Do not introduce a resolution/dispatch cache in this increment: recomputation is bounded by the explicit shared work policy. The interpreter's prepared-code cache remains separately bounded by CallLimits.

## Resolution phase

Origin D and all resolved owners must belong to the unchanged admitted graph. Validate origin registry/membership before reading the constant pool. The input must be a Methodref with valid Class/NameAndType/Utf8 links and a valid method descriptor. InterfaceMethodref passed to this invokevirtual service is an InvalidConstant; it is not interface method resolution. Array owner names use Context(UnsupportedArrayOwner). Special initialization names are UnsupportedSpecialMethod. Explicit reference parameters/results are UnsupportedDescriptor; primitives and void have no reference-type loading constraints.

Resolve the symbolic class C in D's defining-loader context through the already seeded bindings or explicit resolver. Apply the same unnamed-module class accessibility rule as field resolution, then reject interface C with Java IncompatibleClassChange. A nonpublic inaccessible owner fails access before class/interface method lookup. Exact UTF-16 name/descriptor and ClassId/LoaderId identity remain mandatory.

Search C then its class superclasses in order. At each class, honor the signature-polymorphic lookup priority: if it declares exactly one method with the requested name and that method satisfies the JVMS signature-polymorphic definition, return UnsupportedSignaturePolymorphic, even if the call-site descriptor differs. Recognize the special definition only for java/lang/invoke/MethodHandle or java/lang/invoke/VarHandle, one Object[] parameter, and both native/varargs flags. Do not turn it into NoSuchMethod or execute it as an ordinary body. Otherwise choose the first exact name-and-descriptor declaration, including private/static/abstract declarations for the later checks.

If no class declaration matches, inspect the complete superinterface closure of C and every superclass. Visit each interface once, including indirect interfaces; Object dependencies are not interface inheritance. Any exact matching interface declaration that is neither private nor static triggers UnsupportedInterfaceMethods; its owner identifies the encountered interface as a witness, not a selected/maximally-specific target. This explicitly defers maximally-specific/default and abstract-interface resolution rather than skipping a candidate. If the complete scan finds no eligible candidate, return Java NoSuchMethod. Unrelated interface methods and private/static interface declarations do not by themselves prohibit ordinary class resolution. Once a class declaration has been found, no interface fallback is performed.

Apply method access from D to resolved declaration R in class E. Public succeeds. Same-class private succeeds; other private access is UnsupportedNestAccess. Package access requires equal defining loader and exact package text. Protected access succeeds within the same runtime package; otherwise D must be E or a subclass. For non-static protected methods additionally require C to be D, a superclass of D or a subclass of D, and retain the receiver restriction for the next phase. For protected static methods that extra C/D relationship does not apply. Do not separately require accessibility of E when public inherited R is exposed through accessible C.

The supported descriptor is purely primitive/void, so no reference-class loading constraints need to be imposed for this call. After access checks, a static R yields Java IncompatibleClassChange. This must precede null. Resolution, owner/member accessibility and staticness are not deferred until after receiver inspection. Descriptor/unsupported-feature and malformed-input failures are host boundaries, not promises of complete JVM error precedence outside this profile.

## Receiver selection and overriding

After successful non-static resolution, null yields Java NullPointer. For non-null receivers, check heap identity/generation/kind and require an ordinary instance whose class is C or a subclass. If cross-package protected access was used, require its class to be D or a subclass. Unsupported/malformed receiver relationships remain Context(InvalidReceiver), reflecting the missing verifier proof; they are not fabricated Java linkage errors.

If R is private, select R itself. A same-name method in the receiver's class must not intercept a private call. Otherwise select the nearest declaration in the actual receiver's class chain that is an instance method and can override R. Static/private candidate declarations do not override and are skipped during selection, even if their names/descriptors match. Do not confuse that with resolution, which can resolve a static declaration and must then reject it.

Implement the JVMS package-aware transitive rule, rather than a nearest-name or nearest-public heuristic. Public/protected R can be overridden by any matching nonprivate instance declaration on the descendant class chain. For package R, a matching declaration qualifies if it is in R's runtime package, or can override an intervening declaration which itself can override R. Runtime package identity always includes defining LoaderId.

An equivalent iterative procedure for package R is to walk from R's declaring class toward the receiver, initially retaining R's runtime package as reachable. A matching nonprivate instance declaration qualifies when its package is reachable or some already qualifying declaration is public/protected. A qualifying declaration adds its package; a qualifying public/protected declaration makes all subsequent matching nonprivate instance declarations eligible. Preserve every reachable package/public bridge, not only the most recently selected method. The nearest qualifying declaration is selected. This handles a public bridge followed by package declarations in different packages. R itself remains the fallback class declaration, so selection in this supported class-resolved subset does not silently invoke interface defaults.

Final does not create a separate nearest-name dispatch rule. Legal final methods select normally and cannot be overridden. Defensively reject a detected final override in the relevant receiver class chain with host InvalidOverride; do not claim a full verifier or silently invoke an overriding body. Inspect matching nonprivate instance declarations from ancestors to descendants: an earlier public/protected final declaration prohibits any later overriding candidate; a package-final declaration prohibits a later candidate in its runtime package. A transitive override cannot bypass this check, since its first intervening override would already be rejected. Private final declarations do not participate. Check the chain's matching declarations, including ancestors above R, so an already-invalid resolved override is not accepted merely because lookup began below a final declaration. This defensive check also applies when R is private; it does not change the private target selection or treat a descendant as overriding R. Other verifier obligations remain deferred explicitly.

After selection, an abstract selected declaration produces Java AbstractMethod. It wins over no executable Code, but null already won before selection. A concrete class method masks interface defaults even if the class method is abstract. Native/synchronized/code/handler support is checked when the interpreter prepares the selected target, after resolution/null/abstract ordering. No new exception objects or handlers are added.

The normative boundaries are [method resolution](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.3.3), [access](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.4), [overriding/selection](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.5), [signature polymorphism](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-2.html#jvms-2.9.3) and [invokevirtual](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.invokevirtual). This contract describes a bounded supported subset and additional defensive host checks, not complete linkage.

## Interpreter integration

Keep `execute_instance_method_body(context, MethodId, receiver, arguments, options, limits)` unchanged. Enable opcode b6 only in its runtime-aware mode. Earlier static entry points, including the heap entry point, continue to reject b6. Keep one shared interpreter loop and instruction implementation, not a cloned virtual-call engine.

Every registered frame and prepared cache key must identify its MethodId, not just a local declaration index. A tagged internal key for legacy standalone ClassFile versus registered MethodId is permitted. Do not manufacture runtime IDs for legacy APIs or compare cross-class targets only by method index. A frame's constant pool, code, return descriptor and field/method origin belong to its own declaring ClassId. They must not be borrowed from the root or receiver's actual class. Prepared code is counted once per distinct method identity in the current evaluation.

At b6, decode/validate the Methodref call-site descriptor to locate the numeric argument values and receiver on the operand stack. Checked peeking or temporary typed argument extraction is acceptable; malformed operand-type precedence is outside this unverified profile. Invoke the runtime selection service with the current frame's ClassId and receiver. Prepare the returned MethodId in instance mode, place receiver in local zero and arguments after it, and push a new frame using the existing shared frame/slot/code limits and instruction fuel. Arguments are values, not raw slot slices; J/D still occupy two locals. The invoke instruction consumes one fuel unit and each callee instruction consumes its normal unit. No Rust recursion is permitted. Returned numeric/void values are transferred to the actual caller, preserving its remaining stack operands. Existing return narrowing and FP rules remain unchanged.

After crossing into another declaring class, invokestatic helpers retain the earlier narrow policy relative to that current class: its own Methodref/InterfaceMethodref checks, own declaration lookup, numeric/static preparation and no cross-class or inherited static lookup. A static helper in a base-class callee uses the base class's own CP and helper, even if the root has an identically named/indexed helper. All static and virtual call paths must check expected invocation mode even on prepared-cache hits; an instance entry in the cache cannot satisfy invokestatic, nor a static one invokevirtual. Initialization names remain excluded.

Extend `MethodIdentity` with `pub owner: Option<ClassId>`: Some for registry-backed frames and None for legacy standalone APIs. Keep index/name/descriptor. Populate both error.method and error.call_site.method from their own frames, and render owner identity in diagnostics when present. This permits two classes with equal method indices/names/descriptors to remain distinguishable. Preserve existing transition-site diagnostics: when argument extraction or return transfer fails at an invocation, error.method is the affected caller and call_site identifies that same invocation; it does not necessarily identify error.method's parent frame. Return stack overflow uses the receiving caller and its invoke offset, including across classes. Existing field errors retain provenance; new method host failures use `ExecutionErrorKind::MethodAccess(Box<MethodAccessError>)` with an Error source.

Add symbolic `ExecutionOutcome::{NoSuchMethodError, AbstractMethodError}`. Map only MethodAccessError::Java to these or the existing IllegalAccessError, IncompatibleClassChangeError and NullPointerException. Other method failures remain host errors. Eager target decoding and metadata rejection remain in force before target execution. All earlier completed field/array effects persist after Java outcomes, unsupported operations, limit exhaustion and malformed-target errors.

No invokespecial, invokeinterface, new, reference parameters/results, constructor execution, class initialization, generalized cross-class static calls, exception dispatch, monitors or native binding is added. A host-selected root is still a body evaluation on supplied storage, not proof of construction or Java invocation startup.

## Acceptance evidence

Runtime tests cover exact name/descriptor lookup, inherited public methods from inaccessible declarers, owner/member access before null, protected static and non-static distinctions, private nonvirtual selection, final methods and detected final overrides, package-private non-overrides, transitive public bridges across packages, same package text with different loaders, abstract targets versus null, static resolution versus static candidates, signature-polymorphic lookup priority, interface fallback/default/abstract candidates, ignored private/static interface methods and deep iterative traversal. Verify current context binding/invalidation/resource behavior also applies to methods; no alternate resolver may bypass it.

Interpreter tests cover same-index methods in different classes, per-frame constant pools and field origins, hidden fields, base-class static helpers after virtual calls, receiver/local layout for mixed widths, callee return transfer, direct/mutual recursion, every call/cache mode, native/synchronized/handler/ref-descriptor exclusions, malformed CP/stack/receiver cases, host and Java outcome mapping/provenance, and persistent effects before failure. Exercise exact inclusive fuel/frame/slot/prepared-code/shared-resolution limits and a deep call chain without Rust recursion. Old entry points must still reject b6.

Use original genuine Java 21 hierarchies with overrides, inherited/final/private targets, base-selected bodies on child/leaf receivers, numeric recursion, field hiding, mixed-width parameters, null and arithmetic outcomes, arrays and helpers. Preserve all 8,838 existing execution comparisons and both 151-case service oracles. Require debug/release, formatting, strict workspace Clippy, fixture/reference reproduction, appropriate parser smoke and final director review. Record hosted CI separately. No full-M2, complete-conformance or performance claim follows.

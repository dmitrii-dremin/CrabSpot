# Reference arguments and results

Status: implemented and approved as a bounded increment by the technical director on 2026-11-04; see `m2-reference-calls-review.md` for frozen review and integrated evidence. Fresh hosted CI is pending publication. Full M2 remains open. No NanoSpec.

## Scope and compact signatures

Allow reference parameters/results and areturn (b0) only in the existing runtime-aware instance-body graph. All current primitive/void behavior remains. Existing same-current-class static helpers, virtual, interface/default/private and ordinary nonconstructor special calls can carry references. Do not generalize static resolution across classes or execute initialization. Host execute_instance_method_body may receive reference arguments and return a Reference through the existing Value/ExecutionOutcome types. Earlier standalone numeric and primitive-heap entry points continue rejecting reference signatures and b0. Runtime-aware coverage becomes 191 effective opcodes.

Preserve MethodId keys, shared frames/cache, receiver eligibility, opcode modes, eager Code decoding, and all previous unsupported constructor/native/synchronized/handler/nest/signature-polymorphic boundaries. References are existing heap handles, never uninitialized-object values. Returning a handle neither transfers heap ownership nor makes it usable with another heap.

Export these compact runtime types:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceInvocationKind { Virtual, Interface, Special }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodReturnKind {
    Void, Boolean, Byte, Char, Short, Int, Long, Float, Double, Reference,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodValueShape {
    pub parameters: Vec<ValueKind>,
    pub parameter_slots: u16,
    pub result: MethodReturnKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MethodValuePosition { Argument(usize), Return }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MethodReferenceMismatch {
    pub method: MethodId,
    pub position: MethodValuePosition,
    pub source: ReferenceType,
    pub target: ReferenceType,
}
```

Add these public context methods, all returning MethodAccessError:

```rust
pub fn method_value_shape(&mut self, method: MethodId)
    -> Result<MethodValueShape, MethodAccessError>;
pub fn instance_invocation_shape(
    &mut self, origin: ClassId, index: u16, kind: InstanceInvocationKind,
) -> Result<MethodValueShape, MethodAccessError>;
pub fn check_method_reference_profile(
    &mut self, method: MethodId, receiver: Option<Reference>,
) -> Result<(), MethodAccessError>;
pub fn check_method_arguments(&mut self, method: MethodId, arguments: &[Value])
    -> Result<(), MethodAccessError>;
pub fn check_method_return(&mut self, method: MethodId, value: Option<Value>)
    -> Result<(), MethodAccessError>;
```

Keep select_virtual/select_interface/select_special signatures returning the selected MethodId. They must enforce the resolution and selected-profile boundaries internally while the resolved declaration is still available; selected-only argument validation cannot reconstruct it.

Add MethodAccessError variants:

```rust
Type(Box<TypeAccessError>),
UnsupportedLoadingConstraint { origin: ClassId, method: MethodId },
UnsupportedOverrideConstraint { method: MethodId, declaration: MethodId },
ArgumentCount { method: MethodId, expected: usize, actual: usize },
ValueKindMismatch {
    method: MethodId, position: MethodValuePosition,
    expected: Option<ValueKind>, actual: Option<ValueKind>,
},
IncompatibleReference(Box<MethodReferenceMismatch>),
```

Void corresponds to None in ValueKindMismatch. Preserve Display and Error::source for Type. All new variants are host failures; the existing explicit MethodJavaError mapper remains the only source of method-service Java outcomes. Never turn nested TypeJava errors or incompatible arguments/returns into ClassCastException, ArrayStoreException or a fabricated LinkageError. Existing interpreter computational argument/stack errors retain their established categories and precedence.

Prepared bodies retain compact ValueKinds, slot count and MethodReturnKind, not owned Object names, array FieldType trees or resolved signature types. Remove the current owned return FieldType for reference-aware preparation. Old numeric preparation can convert its existing parsed descriptor into the same compact tags. Runtime validation walks immutable borrowed descriptor spans; temporary parsed metadata is bounded by the one classfile descriptor and must be dropped, not cached per body. No new resolved-signature cache, copied class metadata or second engine is allowed.

## Descriptor inspection and accounting

method_value_shape checks context validity, MethodId/owner admission and the declaration's descriptor, then returns compact kinds. It does not resolve nominal types, check actual values, perform caller access or certify body eligibility. instance_invocation_shape checks context/origin admission, the appropriate Methodref/InterfaceMethodref and version rules, owner/name/NAT structure, special-name exclusion and descriptor grammar. It performs no symbolic owner resolution or receiver selection. Preserve special's >=52 InterfaceMethodref rule and the previous invokeinterface version policy.

Shape inspection checks that the owner is a Class constant with a Utf8 name, but does not reject an array-owner spelling. Preserve UnsupportedArrayOwner in the selector's method_symbol phase after typed argument/receiver extraction. This retains the previous underflow/wrong-kind precedence; shared parsing helpers must not accidentally move owner eligibility into structural shape inspection.

A descriptor is reference-bearing if any parameter or result starts with L or '['. Numeric-only descriptors retain zero additional signature/type-work charges so all earlier numeric lookup budgets stay valid. Use a bounded classification prefix: an admitted valid method descriptor reaches its first L/'[' or numeric end within 258 UTF-16 units. Stop at the first reference marker; do not scan a long nominal name before charging. A malformed prefix fails structurally; do not use an unbounded search as classification.

Each public shape/profile/argument/return operation charges the complete UTF-16 descriptor once if it is reference-bearing, before full parsing/scanning or allocating temporary descriptor metadata. Charge even for all-null values and binding/cache hits. Public instance selection separately charges its symbolic descriptor in method_symbol, then the selected descriptor in its profile check. Interpreter extraction-shape inspection and later selection are independent checks, so their symbolic descriptor charges are deliberately both present. Do not hide these charges or add a second full-name charge when resolving a borrowed field-type span.

Within a descriptor operation, parameter/result reference spans use the same identity-only parser as checked fields, resolving through the method declarer's defining loader and shared UTF-16 bindings. Do not apply cast caller-access checks to descriptor types: an accessible method may expose an inaccessible type. Bound nominal/array targets must already be admitted; no implicit loading or name-only bootstrap identity. Primitive-leaf arrays use the explicit universe. All previous symbol/unit limits and pre-callback checks remain.

Runtime-aware prepare_method must obtain its compact shape through method_value_shape at its existing descriptor-processing stage, after existing name/flag/body-mode eligibility checks; it must not first copy a long nominal result into the cache. Cache hits retain static/instance mode checks, while profile/value operations below still run per invocation. Transient allocations for argument vectors or descriptor ranges are bounded by parameter-slot limits and must use fallible reservations when introducing new allocation paths. No policy option is added in this increment.

## Call-site, resolved and selected identities

The call-site symbolic descriptor determines typed argument extraction and invokeinterface's encoded count. The resolved MethodId R determines method access and the symbolic resolution loading boundary. The selected MethodId S determines the actual body, its locals and declared argument/result identities. Existing lookup requires the same exact UTF-16 name and descriptor at these stages; preserve and defensively check that invariant. No Java-source covariant-return conversion is added: an emitted bridge method is an ordinary separate method descriptor.

JVMS method resolution and preparation impose descriptor loading constraints across defining loaders, including overriding and inherited interface selections. A value check against S alone cannot establish those constraints. This kernel still lacks the full initiating-loader constraint/lifecycle machinery. See [JVMS 5.3.4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.3.4), [preparation](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.2) and [method resolution](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.3.3).

Adopt these explicit conservative boundaries for an executed signature containing a nominal reference leaf (Lname; directly or within arrays):

1. During ordinary instance resolution, after resolved-member access checks and before staticness/null handling, require the origin's defining loader to equal R.owner's defining loader. Otherwise return UnsupportedLoadingConstraint { origin, method: R }. This applies even when every argument/result is null and current resolver answers happen to coincide.
2. After existing receiver validation and normal target selection/publicness/abstract checks succeed, invoke check_method_reference_profile(S, Some(receiver)) before returning S. This checks the selected signature and audits the receiver graph as below. It therefore catches a selected declarer or an intermediate matching declaration under another loader even when origin, R and S themselves have the same loader.
3. Host roots and same-current-class static helpers use the profile operation at the engine boundaries below. A static helper needs no overriding-closure audit; its restricted routing already keeps call-site and selected declaration in the same defining class.

For reference-bearing signatures without nominal leaves (only primitive-leaf array references), no nominal loading constraint is introduced. Existing numeric-only selection behavior and work remain unchanged. An actual argument object's loader may differ from the descriptor loader if its actual identity is assignable through the admitted graph.

The profile operation validates method/owner admission and charges its reference-bearing descriptor even for null arguments. For every signature, an instance method requires Some(receiver) and a static method requires None; a mismatched Option shape is a Context InvalidState host error. Only a nominal-bearing instance signature additionally validates the actual ordinary-instance identity and compatibility with the method's declarer using existing class/interface receiver rules, then audits its complete admitted superclass/interface closure, including the method owner's ancestors. Numeric-only and primitive-array-only profiles do not repeat receiver traversal or add a hierarchy audit; existing engine/selector receiver checks remain authoritative. Numeric-only profiles add zero lookup work. Primitive-array-only profiles still pay their complete reference-bearing descriptor charge.

Audit iteratively in deterministic superclass-first, then declared-interface-order traversal, with ClassId deduplication. At every reached type, examine declared methods for the exact target name and descriptor. Any matching declaration whose defining loader differs from S.owner's loader produces UnsupportedOverrideConstraint { method: S, declaration }. Deliberately include matching private/static and otherwise nonselected declarations: this conservative unsupported boundary can reject some legal non-overrides and is not a claim to implement full preparation. In particular, do not inspect only the chosen override path, skip interfaces/defaults, or miss an intermediate L2 declaration between L1 endpoints. Unrelated signatures are not certified; this remains an executed-body profile, not class preparation.

Charge each distinct node before inspection, each declared method before inspection, its complete name units before comparison, and its complete descriptor units before comparison when the name matches. Charge every graph edge before enqueue, including duplicates. Use fallible iterative temporary containers, retain no audit cache, and perform no resolver callback during this admitted-graph audit. These new charges occur only for nominal-reference profiles; preserve every earlier numeric selector budget. The predicate for protected access remains class-only and is not widened by this audit.

Because accepted nominal call-site/R/S signatures and all matching receiver declarations share one defining loader, their exact descriptor spellings use the same binding identities. This establishes the bounded call transfer invariant without an extra caller-descriptor check after return. It does not implement general cross-loader constraints. Primitive-leaf array identity is shared through the explicit bootstrap universe. A later relaxation must implement real constraints rather than equating current resolver answers.

## Argument and result validation

check_method_arguments checks context/method/admission, charges/parses the declared descriptor, then argument count and all computational kinds before resolving any reference target. Z/B/C/S/I require Int; J/D remain two slots; all reference descriptors require Reference. It neither narrows Int arguments nor changes handles. Once computational checks pass, visit reference arguments in declaration order:

- Null requires no universe, source-handle validation, target resolution, binding or assignment traversal. The complete signature charge and caller/profile boundaries still apply.
- For non-null, obtain reference_type(value) first, resolve the selected method's borrowed parameter span using its defining loader with IdentityOnly access, then call is_assignable. A missing/unadmitted target or invalid source stays a wrapped host error. False yields MethodReferenceMismatch with the exact MethodId, Argument index and source/target identities.

Do not pre-resolve later parameters or the result. Preserve already consumed work/bindings on later failure; no callee frame or body effect is created. Calls into methods with a reference result but no reference arguments still receive their full signature charges without resolving the result early.

check_method_return performs the same method/context/signature checks, validates the Option/computational shape against the declared result, and applies the same null or non-null reference rule using Return position. Numeric/void checks do not normalize values. Non-null reference results are checked against the selected declarer's result span, not the runtime receiver type, a field alias or the caller's current class. No fresh accessibility check is made on that type.

The value helpers are not standalone linkage/profile approval: selectors or the explicit host-root/static profile call establish the loading boundary separately. Keep this separation documented. Full signature charges occur even if null then skips resolution; an unbound universe can still execute an otherwise accepted all-null reference transfer. A non-null argument/result requires the bound universe and valid actual heap identity.

## Exact engine order

For a host root, preserve the old prefix: frame-count gate, method eligibility/compact shape/eager decoding, computational argument count/kinds, aggregate frame reservation and receiver validation. Then call check_method_reference_profile(root, Some(receiver)), check_method_arguments(root, args), and only then create locals/frame and start execution. Existing root computational diagnostics precede reference checks. Host-selected interface roots and static roots remain unsupported through this entry point. Null roots retain the old host receiver error; they are not Java call instructions.

For b6/b7/b9, use instance_invocation_shape to replace the interpreter's separate primitive-only descriptor parser. Preserve CP/tag/version and special-name checks, invokeinterface count = 1 + parameter_slots (references count one), then typed reverse extraction of arguments and receiver. Wrong stack kinds/underflow precede resolution. Do not validate reference handles during extraction. Call the existing selector, which enforces access/resolution/static/null/receiver/selection and the loader/profile boundary above. Then preserve target preparation/cache-mode checks, frame/slot/code reservation and computational argument validation, followed by check_method_arguments(S, args), before locals/frame creation.

For b8, preserve the existing same-current-class resolver and its error ordering. Prepare/check mode, reserve, extract and computationally validate arguments as before; then call check_method_reference_profile(target, None) and check_method_arguments before frame creation. The body shape supports references only when Services::Fields is active. Do not silently add general cross-class static calls or initialization.

The runtime b8 resolver may omit its redundant owning parse_method_descriptor call because immutable registry admission already validated that exact CP descriptor. Preserve that parse in standalone modes. Runtime owner/tag/version/NAT/special-name/declared-target checks and borrowed descriptor comparisons remain in their previous order; do not build an owned nominal tree before the target's charged shape operation. No additional symbolic static-shape API or charge is introduced here: unlike b6/b7/b9, this restricted resolver performs no general symbolic method resolution. Target shape/profile/argument charges still apply as specified, including on null values and repeated calls.

At areturn, first require the runtime-aware mode and a Reference result tag in the current prepared body; then pop a typed Reference. Call check_method_return(current MethodId, Some(Value::Reference(value))) before producing Action::Return. A mismatch or policy failure occurs at the callee's return offset with its declaring identity and invocation provenance; do not pop the frame first. Existing primitive return opcodes reject reference-result methods, and areturn rejects primitive/void results. Like earlier returns, unused operand-stack contents are discarded on successful return.

After success, the existing return path releases the callee's slot reservation and pushes the identical one-slot reference to the caller, or returns it to the host. Do not perform a new cast, type-name resolution or current-caller descriptor check at that push: the accepted call-site/R/S identity invariant and callee result validation already establish compatibility. Preserve the established caller-capacity/transition-site diagnostic if the receiving stack cannot hold it. Earlier callee writes and allocations survive every later failure. Each invocation/return instruction still consumes one shared fuel unit.

All new runtime host errors remain boxed under ExecutionErrorKind::MethodAccess. Shape/preparation errors identify the target method; extraction/selection errors retain caller call-site convention; argument-value errors identify the target and attempted call site; return-value errors identify the returning method/return instruction. Preserve numeric error ordering and exact primitive-return narrowing/float-bit behavior.

## Worker ownership and acceptance

The runtime worker owns compact shape/types, descriptor-span/value validation, selector loading/profile integration, host diagnostics, runtime tests and README. A sibling module is appropriate; keep the classfile grammar/API unchanged unless the director approves a concrete necessity. The interpreter worker owns compact prepared return tags, runtime shape/profile/value calls, b0 and argument/result transfer integration, synthetic tests and README. Root owns original Java fixtures/JDK output, integration/state assertions and final evidence. Use isolated worktrees and separate build output.

Runtime tests must cover exact caller/R/S identities; same-loader acceptance and origin/R mismatch before static/null; L1 endpoints with a matching L2 intermediate/default/interface declaration; host-root audit; documented conservative private/static rejection; unrelated-signature allowance; primitive-leaf array signatures across loaders; actual values from other loaders with valid assignment; hidden descriptor types; all-null/unbound/missing-target shortcuts after complete charges; wrong count/kinds, foreign/stale handles, incompatible arguments/results and exact error positions. Pin full signature/cache-hit charges, pre-callback symbol/unit limits, deep iterative audits and late failure preservation. Exercise a near-limit long descriptor shared by multiple small bodies and verify compact prepared metadata does not retain copies of its nominal names. Do not relax previous numeric budget tests.

Interpreter/genuine coverage must include mixed reference/Long/Double slot ordering, null/self/distinct objects, primitive/nominal/multidimensional arrays, aliases/cycles, recursive identity transfer, all four call forms, interface/private/default/super targets, inaccessible result types, areturn root/descendant transfer and caller stack failure, old-mode rejection, malformed values/returns, loader boundaries and failure precedence. Explicitly exercise cached methods reached through multiple call forms and numeric wrappers calling reference helpers. Preserve all 24,421 accepted execution comparisons, both 151-case service oracles and the 88-class corpus.

Fixture recommendation: numeric host wrappers exchange references through ordinary helpers, compare identity and observe field/array mutations. Use default-only host receivers; constructors belong only to the JDK oracle driver. Inspect actual Code/descriptors to confirm b0 and reference signatures plus intended call forms. Separate independent JDK outcomes from source-derived state/handle assertions. Synthetic tests may supply explicit host references and inspect returned handles directly.

Acceptance requires frozen technical review, formatting, strict workspace/all-target Clippy, all debug tests, optimized runtime/interpreter tests, fixture/reference reproduction and expanded parser smoke. Record new hosted Linux/Windows evidence separately after publication. Full M2, complete verification/linkage/loading constraints, JVM conformance and performance superiority remain open.

# ADR 0007: Runtime identity, lifecycle and managed-reference boundaries

Status: design accepted on 2026-11-03. This ADR approves boundaries, not full runtime implementation or M3 completion.

## Context

The numeric execution kernel is growing an explicit call stack. Its same-class method lookup has no defining-loader context and cannot become cross-class resolution merely by accepting another ClassFile. Fields, objects, arrays and exceptions will need stable runtime identities, initialization policy and eventual GC roots. These requirements must precede broader execution support.

## Decision

Introduce a separate `runtime` crate when the first useful metadata-registry implementation lands. It depends on parsed metadata and general infrastructure, never on launcher or an execution backend. The interpreter may depend on runtime services later; runtime must not depend on interpreter-owned execution details. Preserve the pure classfile and bytecode layers.

Class identity is the pair of defining-loader identity and binary class name. Registry-owned opaque LoaderId and ClassId handles must reject accidental use with another registry. Names retain semantic Java UTF-16 units in internal slash form, without lossy display conversion. Initiating loaders and defining loaders are separate concepts; source retrieval without delegation must not be represented as a completed delegation algorithm. This follows [JVMS class creation and loading](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.3).

A registered parsed definition is explicitly unverified and unlinked. It is not a verified runtime class and does not acquire an initialized state simply because the kernel can evaluate one of its methods. Runtime field/method handles carry their owning ClassId; own-member lookup is named accordingly and is not inheritance resolution or dispatch.

Lifecycle responsibilities are explicit: registration, definition/hierarchy checks, verification, preparation, resolution and initialization. Resolution may be lazy and is not a single linear stage. Initialization state distinguishes not started, in progress with an owning execution context, completed and erroneous. Same-context recursive initialization and other-context waiting need different handling. Failures and retry behavior must be modeled according to the failing stage rather than collapsed into an all-purpose boolean. No public setter may manufacture a verified or initialized class. These services must honor [JVMS linking](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4) and [initialization](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.5) before a Java invocation entry point is exposed.

Future heap references use opaque managed handles, with null distinct from an allocated identity, checked owning-heap membership and stale-handle protection if indices are reused. Moving an allocation must not change Java identity. Raw host addresses never become application references. Active locals/stacks, static storage, pending Java exceptions and native root scopes must be enumerable roots before collection is enabled. Per-instruction/frame transition boundaries must permit later safepoint cooperation without embedding JIT-specific behavior.

Java exception propagation remains distinct from host/resource errors. Until exception classes, objects, assignability and handler dispatch exist, a symbolic uncaught arithmetic outcome is explicitly limited to the kernel. Native-method service dispatch remains separate from JNI ABI machinery and fails explicitly when absent.

## Consequences

M2 may require narrow runtime infrastructure before the broader M3/M4 milestones are complete. Reports must distinguish that dependency work from milestone acceptance. A registry or object-layout test does not establish class initialization, verification, library compatibility or application startup. Each real service receives its own bounded implementation contract, tests and review; no placeholder success advances lifecycle state.

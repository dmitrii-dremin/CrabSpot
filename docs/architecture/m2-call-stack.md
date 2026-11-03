# M2 bounded static call-stack increment

Status: implementation authorized by `m2-floating-review.md` on 2026-11-03. This is a method-body kernel extension, not complete Java invocation or M2 acceptance.

## Boundary

Support `invokestatic` only when its symbolic owner is the currently supplied class or interface and its name/descriptor identify an ordinary static method declared there. Resolve names from the actual constant pool and method table; never hardcode fixture names or redirect calls to Rust helper implementations. Cross-class references, inherited lookup, instance calls, native/synchronized methods, special initialization methods, reference descriptors and exception handlers remain explicit unsupported conditions. A missing declared target is not silently searched in an unknown superclass.

The caller supplies parsed metadata to evaluate method bodies. It has not supplied a runtime class, defining loader, initialization state or Java access context. Consequently the API continues to exclude Java loading/linking/initialization, application startup and Java linkage exceptions. This increment implements frame transitions for the supported body graph; it must not claim the full semantics of `invokestatic`. The opcode coverage table must mark this restriction. The [JVMS invocation instruction](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.invokestatic) requires runtime resolution and initialization that remain later responsibilities.

## Engine and limits

Refactor the evaluator to an explicit `Vec<Frame>` execution loop. Each active frame owns its locals, operand stack, selected method identity and next bytecode offset. Prepared per-method metadata contains the descriptor and validated decoded code. A method is prepared on first entry and reused during the evaluation, so recursion does not copy or repeatedly decode its entire body. A complete body is structurally decoded before its first instruction, including unreachable bytes. Do not implement JVM recursion through recursive Rust calls.

Use one shared instruction budget across every frame. Calls and returns each count as instructions; preparation and symbolic lookup do not reset the budget. Add explicit frame-count and aggregate-frame-slot limits. Count the root frame. Reserve `max_locals + max_stack` slots for every active frame with checked arithmetic, even if its logical operand stack is currently sparse; release the reservation on return. Check limits before allocating a new frame. Recommended defaults are 256 frames and 1,000,000 aggregate slots. Zero frame allowance prevents root entry. Resource-limit errors are host policy, not fabricated StackOverflowError or OutOfMemoryError objects.

The prepared-method cache has a separate `max_prepared_code_bytes` allowance, defaulting to 1,048,576 bytes. Charge each distinct method's whole Code byte length once, before decoding it on first preparation. Retain that charge across returns and do not charge repeated use of the same prepared method again. Check cumulative arithmetic and return an explicit host resource error on exhaustion. Borrow the supplied metadata rather than duplicating Code payloads. This measures cached source-code volume, not exact Rust heap allocation; decoded structures and descriptors add bounded overhead. Frame-slot policy covers active frames only and must not be described as an all-allocation memory ceiling.

Preserve the existing `ExecutionOptions { instruction_budget }` entry point for source compatibility, delegating to default call limits. Add a second entry point, `execute_static_method_body_with_limits`, accepting the same class/index/arguments/options plus `CallLimits { max_frames: usize, max_frame_slots: usize, max_prepared_code_bytes: usize }`. Both use the same engine. Hard ceilings may be added if needed to guarantee bounded implementation behavior, but must be documented and tested rather than silently changing Java semantics.

## Calls, returns and diagnostics

Validate the call constant-pool tag, owner kind and method descriptor. Methodref must name a class; InterfaceMethodref must name an interface and obey its supported class-file version. Match the owner by semantic Java name, including constant-pool aliases. Reject initializers and nonstatic targets. Validate supported target declaration, descriptor, Code and handler policy before starting that target.

Arguments come from the caller operand stack in descriptor order, with category-two values occupying two local slots in the callee. Pop in reverse order and populate the callee locals in forward order. There is no receiver. Remaining caller operands stay in place. The caller resumes at the instruction following the call after the callee returns. Void return transfers no value; a numeric return transfers its correctly typed/narrowed value and checks the caller stack's slot capacity. Callee residual operands are discarded. Use the same return semantics at the root.

A symbolic uncaught ArithmeticException propagates out of the body graph; no continuation executes after it. Unsupported targets, invalid frame state, decoding failures and resource exhaustion remain structured host errors. Add selected method identity to diagnostics so an offset in a nested method is unambiguous. Preserve an available call-site offset for errors discovered while resolving or entering a target. This is diagnostic provenance, not a fabricated Java stack trace.

## Acceptance

Use independently compiled Java methods and independently generated JDK outputs for helper chains, recursion, mutual recursion, overload selection by descriptor, zero/multiple/mixed-width arguments, void/narrow/float/double returns, caller operands surviving calls, and arithmetic failure in a descendant. Include interface static calls if the implementation supports a supplied interface body graph.

Synthetic public-API tests exercise malformed references, aliases, wrong owner kind, missing targets, nonstatic and unsupported declarations, stack underflow/type mismatch, invalid callee encoding, overflow on returning into the caller, frame/slot/cache limits and exact shared-budget boundaries. Cache tests include zero allowance, exact thresholds, multiple sequential distinct callees, and repeated use of one cached method. Recursive bytecode must hit an explicit policy limit without overflowing the host stack. A large permitted frame limit with an independently tight slot or instruction budget should remain bounded.

Run formatter, strict Clippy, full workspace tests, fixture/reference reproduction and debug/release call differential tests. Preserve numeric and floating evidence. Director source review and a separate increment disposition are required before extending into real runtime services.

## Next runtime boundary

Before cross-class calls or allocation, introduce a separate runtime representation whose class identity is defining-loader identity plus binary class name, with explicit lifecycle and symbolic-to-runtime reference transitions. The current kernel's per-evaluation method cache is not a runtime constant pool or classloader. Runtime calls must perform resolution/access/initialization obligations instead of reusing a body lookup as if it proved them. Heap references must use managed handles and root ownership; raw host pointers cannot become application references. These boundaries require a concrete reviewed runtime design before its first code lands.

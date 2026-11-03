# Primitive-array execution bridge review

Date: 2026-11-03.

**The primitive-array execution bridge is approved. This is a completed M2 increment; full M2 and later milestones remain open.**

## Review

The director reviewed the shared execution engine and explicit heap entry point, all added instruction branches, array-kind and exception mapping helpers, reference frame operations, private boundary tests, the 17 public array tests, genuine fixture integration, opcode documentation and the runnable method-body example. No blocking finding remains within the bridge contract.

The caller owns the heap, and every frame shares it. Existing numeric entry points create no hidden heap and retain their 148-opcode boundary. The explicit heap entry point adds 31 effective opcodes for 179 total, including all eight primitive kinds through seven load/store families, null/reference locals, identity/null branches, newarray and arraylength. Wide reference load/store forms join the nine numeric wide forms. Parameters and returns remain numeric/void in both roots and callees.

Each array opcode validates its specific permitted kind; an Int computational value does not conflate Boolean, Byte, Char, Short and Int arrays. Narrow conversions and floating-bit storage use the runtime service. Null dereference precedes index bounds, and negative length precedes allocation quotas. The three new exception outcomes are symbolic and uncaught; no Java exception object or exception dispatch is fabricated. Wrong computational types, wrong array kinds, invalid identities and heap resource/reservation failures remain structured host errors with execution context.

Reference locals preserve category-two overlap invalidation. Opaque equality implements identity comparisons; validated heap ownership prevents foreign identities from entering local/branch/dereference operations. The decoder still checks the complete instruction stream before execution, and unsupported method declarations remain rejected eagerly.

Shared fuel counts each instruction once. Frame, frame-slot and prepared-code policies remain active in heap mode. Successful heap effects persist across returns, symbolic exceptions, fuel exhaustion and host errors; no transactional rollback or collection is implied. Public tests check persistent counters and unchanged caller-owned arrays without exposing an unchecked handle issuer merely to inspect otherwise inaccessible internal references.

## Review refinements

Two test gaps were tightened before freeze: persistence now covers null, bounds, negative-size and arithmetic outcomes plus an unsupported-instruction error; old-entry gating now checks every one of the 31 added opcode values using valid encodings. Additional tests check exact call limits/shared fuel and eager reference-descriptor rejection in descendants. No production semantic correction was required by this review.

## Final evidence

The coordinator independently checked frozen source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 256 passed, zero failed |
| Release runtime/interpreter tests | 169 passed: 77 runtime, 92 interpreter |
| Interpreter/JDK differential outcomes | All 8,285 match in both profiles |
| Separate heap service/JDK round trips | All 151 match in both profiles |
| Java fixture reproduction | 30 byte-identical classes |
| Reference reproduction | All five sets reproduce |
| Method-body example | Length 17 returns Int(731); length -1 yields NegativeArraySizeException |

The 8,285 execution cases include all prior 8,005 cases plus 280 array cases over 20 genuine compiled methods. The new cases include 11 null, 36 bounds and 20 negative-size outcomes. Selected-method decoding confirms all eight atype values, all 14 primitive load/store opcodes and both identity/null branch families; synthetic tests cover fixed/indexed/wide reference locals and remaining boundaries. Private tests exercise foreign references, host-error classification and overlap with both halves of Long/Double locals.

The runnable `crates/interpreter/examples/primitive_array_body.rs` parses the checked-in genuine fixture and executes a selected numeric body with explicit heap/fuel limits. It neither invokes the JDK nor starts a Java application. The director reviewed source and coordinator evidence without claiming another independent execution. Linux, hosted CI and Rust 1.85 remain unobserved.

## Continuation boundary

The next proposed direction is ordinary-instance storage and owner-qualified fields, then a reviewed execution bridge. `runtime-instance-storage-next.md` records decisions still needed before a concrete implementation contract, particularly sharing layouts across unrelated classes and rejecting conflicting hierarchy plans. It is not an implementation authorization or a need for user reapproval.

Reference parameters/returns, reference arrays, object instances/constructors, field bytecodes, cross-class/instance invocation, exception objects/handlers, monitors, class initialization, verification, GC and general Java startup remain outside this increment. These remaining M2 and runtime responsibilities must continue through concrete contracts, implementation and evidence; the 179-opcode count alone does not close a milestone.

# ADR 0006: Separate instruction decoding from a numeric execution kernel

Status: accepted; implementation authorized by `docs/architecture/m1-review.md` on 2026-11-03. This ADR does not approve M2 completion.

## Context

The next milestone requires actual bytecode execution. A small interpreter must not acquire fake class loading, ignored initialization, or placeholder native services just to expose a Java-looking launcher command. The first execution increment can establish correct instruction, frame and numeric semantics before runtime class services exist.

## Decision

Introduce two nonempty crates in sequence: `bytecode`, then `interpreter`. The decoder takes code bytes and a class-file major version, returning instructions with offsets, lengths and typed operands or structured errors. It has no execution, classfile, heap or operating-system dependency. The interpreter depends on the decoder and classfile model and initially evaluates selected static method bodies with explicit arguments.

The interpreter API is an execution kernel, not Java method invocation. It does not promise class loading, initialization, access checks, a Java call stack or application startup. Keep the launcher as an inspection tool until a runtime entry point can honor those obligations. Tests may select a parsed method body and evaluate its supported instructions directly, then compare the result with an independently generated JDK reference for a pure numeric method.

Initially support int and long values with explicit category widths. Use a logical-value operand stack with slot accounting, and locals that distinguish empty slots, values and category-two continuation slots. Overwriting either half of a category-two local invalidates the old pair. No raw object pointer or speculative heap reference enters this interface.

Separate successful returns, symbolic uncaught Java exceptions, malformed execution state, unsupported operations, and instruction-budget exhaustion. Arithmetic division by zero produces a Java exception outcome rather than a Rust panic. The eventual runtime maps that outcome into its real exception representation. Instruction limits are explicit host policy, not Java exceptions.

Native/abstract methods, synchronized methods, exception handlers, object/reference operations, calls, fields, floating-point execution and legacy subroutines are unsupported in this initial kernel. Reject unsupported declarations explicitly and report unsupported instructions with their bytecode offset when encountered. Never implement them as no-ops or fabricated values.

## Consequences

Real Java-compiled numeric instruction streams can execute independently in Rust, with deterministic test evidence. The numeric kernel is an M2 increment, not a complete interpreter milestone or a runnable general JVM. The future runtime remains responsible for class identity/lifecycle, calls, exception objects, references, roots, safepoints and native integration. No JIT backend assumption enters the kernel.

The detailed initial interface and acceptance contract is in `docs/architecture/m2-foundation.md`. Normative instruction behavior comes from [JVMS SE 21 chapter 6](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html); frame design follows [JVMS runtime data areas](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-2.html#jvms-2.6).

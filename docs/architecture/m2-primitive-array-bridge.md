# Primitive-array execution bridge

Status: implemented and approved in `m2-primitive-array-bridge-review.md` on 2026-11-03. This extends the method-body kernel, not Java startup or class initialization.

## API and ownership

Add `execute_static_method_body_with_heap(class: &ClassFile, method_index: usize, arguments: &[Value], options: ExecutionOptions, limits: CallLimits, heap: &mut ManagedHeap) -> Result<ExecutionOutcome, ExecutionError>`. Existing entry points retain their documented numeric-only instruction capability and need not create a hidden heap. Share one execution engine with an explicit optional heap service, rather than duplicating numeric semantics.

The caller owns the heap and its policies. All frames of this evaluation use that heap, including existing restricted same-class static calls. Parameters and returns remain numeric or void in every prepared method. Passing references through method descriptors or returning arrays is not included. No heap is created per call or silently reset between frames.

Heap contents and accounting persist after return, symbolic exception, fuel exhaustion or another error. Execution is not a heap transaction: successfully completed prior allocations/stores remain visible. The noncollecting heap retains unreachable arrays until dropped, so repeated evaluations can exhaust its cumulative quotas. Callers can explicitly create a fresh heap for an isolated evaluation.

## Instructions and values

Enable these instructions only for the explicit heap entry point: aconst_null; indexed/fixed/wide aload and astore; if_acmpeq/if_acmpne; ifnull/ifnonnull; newarray; arraylength; the seven primitive array load instructions and seven primitive array store instructions. This adds 31 effective opcode values to the existing 148, plus two wide local forms. aaload, aastore, anewarray, multianewarray and areturn remain unsupported.

Reference values occupy one frame slot and participate in existing category-based stack operations. Reference locals must use checked local access and preserve category-two overlap invalidation. Identity comparisons use opaque Reference equality, including null; they do not compare contents or cast host addresses. astore accepts only Reference in this slice, not obsolete returnAddress values.

Map newarray atype codes 4 through 11 to the eight primitive kinds. Preserve the decoder's validation of all encodings, including unreachable instructions. Each executed instruction consumes the existing shared instruction budget once. An allocation is bounded independently by heap policies; fuel is not a byte-allocation meter.

The bridge checks opcode-specific array kind before accessing elements: iaload/iastore require Int, laload/lastore Long, faload/fastore Float, daload/dastore Double, caload/castore Char, saload/sastore Short, and baload/bastore allow Byte or Boolean. An Int computational result alone does not make these array kinds interchangeable. Use the heap's narrow conversion and floating-bit semantics rather than implementing a second storage representation. These instruction behaviors are specified in [JVMS 6](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.newarray).

## Exceptions, errors and boundaries

Extend ExecutionOutcome with symbolic uncaught `NullPointerException`, `ArrayIndexOutOfBoundsException` and `NegativeArraySizeException`, alongside ArithmeticException. These carry no allocated exception object. A null reference takes the null outcome before any bounds check; a non-null valid array with a signed out-of-range index takes the bounds outcome. Negative newarray length takes the negative-size outcome before heap policy checks. Propagate these outcomes from descendants through the whole evaluation without fabricating exception-table dispatch.

Keep malformed computational values, invalid opcode/array-kind combinations and invalid managed identities as structured ExecutionError values. Add a heap-service error variant preserving the original HeapError and an explicit wrong-array-kind variant. Heap policy exhaustion and allocation reservation failure remain host-policy/service errors; they are not represented as Java OutOfMemoryError. Attach existing method/offset/call-site context consistently.

Retain eager unsupported declaration checks, including exception tables, synchronized/native/abstract bodies and reference descriptors. No verifier proof, reference assignability, reference method invocation, exception handlers, object instances, field access, initialization, monitors, GC, native bridge or application launcher is implied.

## Acceptance

Public synthetic execution tests cover every added opcode family, all fixed/indexed/wide local forms, both outcomes of reference branches, null/alias/distinct identities, all eight newarray kinds, exact narrow conversions, raw NaN storage, reference/category-two overlap, mixed stack forms, wrong kinds, signed index boundaries, negative lengths and null-before-bounds precedence. Verify shared heap use across numeric call frames, retained heap accounting after errors/returns, policy failure versus Java outcomes, and exact fuel behavior.

Add genuine javac array methods with numeric arguments/results and a JDK 21 differential oracle for all eight primitive kinds, aliases, loops, initialization literals, lengths, narrow conversions and uncaught supported exceptions. Keep production execution independent of the JDK. Existing 8,005 outcomes must remain unchanged. Preserve explicit rejection by the old numeric entry points and unsupported reference descriptors by the heap entry point.

Require formatter, strict workspace/all-target Clippy, full debug tests, optimized interpreter/runtime tests, fixture/reference reproduction and director review before further runtime execution expansion.

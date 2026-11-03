# Primitive-array heap review

Date: 2026-11-03.

**The bounded primitive-array heap is approved. The execution bridge in `m2-primitive-array-bridge.md` is authorized for implementation. Full M2 and subsequent milestones remain open.**

## Review

The director reviewed typed payload storage, allocation accounting and publication, identity issuance and validation, public access paths, private guard tests, all 14 public heap tests, reference-boundary regression updates and the independent service oracle. No blocking finding remains within the heap service contract.

The heap owns actual storage for all eight primitive kinds. Boolean masking, signed Byte/Short narrowing, unsigned Char narrowing and computational loads agree with the specified representation. Float/Double stores preserve raw bits, including signed zero and NaN payloads. Checked references and indices precede element access, and failed stores leave existing elements unchanged.

Heap identity allocation cannot wrap; non-null references are issued only through crate-private construction after publication. Each dereference checks owner, arena bounds and generation. Arena movement and moving/swapping the Rust heap value preserve application identity. Dropping a heap does not make its references valid in a later heap. No cloning, entry reuse or manual-free path exists.

Array count, individual length, cumulative elements and typed payload bytes use checked arithmetic before allocation. Fallible payload/arena reservations complete before entries and logical counters are published. Empty arrays still consume distinct entries. Accounting deliberately excludes allocator and arena overhead; allocation failure is a structured host-service error, not a fabricated Java exception. Tests exercise deterministic capacity overflow rather than deliberately exhausting host memory.

The root validator checks identity only. It neither marks nor retains entries, and live allocations are not treated as a substitute root set. Primitive arrays contain no child references. Static storage remains read-only and default-only, so its root iterator remains empty even though the runtime can now issue references.

## Final evidence

The coordinator independently checked frozen source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 235 passed, zero failed |
| Release runtime/interpreter tests | 148 passed: 77 runtime, 71 interpreter |
| Existing execution outcomes | All 8,005 match in both profiles |
| Heap service/JDK round trips | All 151 match in both profiles |
| Java fixture reproduction | 30 byte-identical classes; preceding 29 unchanged |
| Reference reproduction | All five sets reproduce, including 280 array rows |

Only 151 array rows are credited as heap-service differential checks. The complete 280-row array table is prepared for the later bytecode bridge; it does not yet demonstrate interpreter array support. Public tests cover all eight kinds, exact defaults and conversions, aliases, arena growth and moves, empty arrays, foreign/dropped-heap references, wrong types, signed bounds, every quota and failure recovery. Private tests cover forged indices/generations, namespace/accounting limits and reservation failure. Existing numeric entry points reject both null and real allocated references with typed argument errors.

The director reviewed implementation and coordinator evidence without claiming another independent execution. Linux, hosted CI and Rust 1.85 remain unobserved.

## Authorized next increment

The bridge adds an explicit caller-owned heap entry point while preserving numeric/void method descriptors, existing entry-point capability boundaries and shared call/fuel limits. It implements primitive-array bytecodes and symbolic uncaught null/bounds/negative-size outcomes. Prior completed heap effects persist after an evaluation error or exception; the evaluation is not transactional.

Reference arrays, object instances, field access, class mirrors/assignability, initialization, exception dispatch, synchronization, tracing/reclamation and general Java startup remain outside this approval.

# Bounded managed heap for primitive arrays

Status: implemented and approved in `runtime-primitive-array-heap-review.md` on 2026-11-03. A separately gated bridge connects this service to array bytecodes.

## Placement and reference ownership

Implement `runtime::heap` initially and reexport its public types from runtime. HeapRef and Reference already belong to runtime; keeping issuance inside that crate avoids a public unchecked issuer or runtime/heap dependency cycle. Do not create an empty heap crate. A future extraction needs a separate ownership/dependency decision.

`ManagedHeap` owns a unique, nonwrapping heap identity and an arena of primitive-array entries. It is not Clone. Issued references contain owning-heap identity, entry index and generation, never a host address. All dereferences validate ownership, index and generation. Start live generations at a nonzero value and expose no entry reuse or manual-free operation in this noncollecting increment. Dropping one heap and constructing another must not make old handles valid again.

The allocator creates a non-null Reference through crate-private construction only after its entry is ready for publication. Moving the Rust heap value or reallocating its arena cannot change reference identity. Public Reference remains opaque; callers can copy, compare and inspect nullness, but cannot issue a handle for an arbitrary entry. The public heap allocator becomes the first supported non-null issuer.

## Concrete API

Use `PrimitiveArrayKind::{Boolean, Byte, Char, Short, Int, Long, Float, Double}`. Provide `value_kind() -> ValueKind` and `element_bytes() -> usize`; Boolean/Byte occupy one payload byte, Char/Short two, Int/Float four and Long/Double eight. This is the chosen storage/accounting representation, not an exposed object layout.

`HeapOptions` has usize fields `max_arrays`, `max_array_length`, `max_total_elements`, `max_payload_bytes`, with defaults 4,096, 1,048,576, 4,194,304 and 33,554,432 respectively. `ManagedHeap::new(options) -> Result<Self, HeapError>` allocates a fresh heap identity. Its methods are:

- `allocate_primitive_array(kind, length: i32) -> Result<Reference, HeapError>`
- `kind(reference: Reference) -> Result<PrimitiveArrayKind, HeapError>`
- `length(reference: Reference) -> Result<i32, HeapError>`
- `load(reference: Reference, index: i32) -> Result<Value, HeapError>`
- `store(reference: Reference, index: i32, value: Value) -> Result<(), HeapError>`
- `validate_root(handle: HeapRef) -> Result<(), HeapError>`
- `allocated_arrays() -> usize`, `total_elements() -> usize`, `payload_bytes() -> usize`

Generic load/store operate on the array's actual element kind. The future bytecode bridge must additionally enforce the specific opcode's allowed array kinds; this service does not make iaload valid for every array that returns an Int.

HeapError distinguishes null, foreign-heap, missing-index and stale-generation references; negative lengths; index bounds; value-kind mismatch; policy exhaustion with resource/limit/required; identity exhaustion; allocation failure; and invalid internal state if needed. These are structured service errors, not fabricated Java exception objects. The later execution bridge assigns the supported Java exception outcomes explicitly.

## Storage and resource semantics

Allocate actual typed element storage with zero/default values. Preserve Float/Double raw bits across stores and loads, including NaN payloads and signed zeros. Integer computational values store into Boolean using `value & 1`, Byte/Short through signed truncation, Char through unsigned truncation, and Int unchanged. Loads return computational Int with the appropriate sign or zero extension. Long, Float and Double require their corresponding Value kinds. Reference values cannot be stored into primitive arrays. These semantics follow the [JVMS array instructions](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.bastore).

Check null/ownership/generation and signed index bounds before reading or mutating elements. Wrong kinds or indices leave storage unchanged. Length is bounded by the i32 API and explicit per-array policy. Empty arrays still consume an array entry and receive distinct identities, although they consume no element/payload quota.

Before allocation, check array count, per-array length, aggregate elements and `length * element_bytes` plus aggregate payload with checked arithmetic. Do not eagerly allocate the policy maximum. Use fallible vector reservation for arena/payload growth and map reservation failures to a structured error; publish the entry and counters only after successful preparation. Allocation failure must not change logical array count, accounting or existing references. Ordinary allocator capacity overhead may change during a failed attempt and is not part of logical publication.

Payload bytes count typed element contents, not allocator metadata or total process memory. Entry-count limits bound empty-array overhead separately. Generous caller policy does not imply a guarantee that the host can satisfy every permitted allocation. Tests of maximum numeric allowances must allocate small inputs rather than deliberately exhaust host memory.

## Roots and GC boundary

`validate_root` checks a non-null handle obtained from an actual root-value iterator or execution state. It neither marks nor retains anything. Allocated entries are not automatically roots. Primitive arrays contain no child references, and this heap does not maintain a substitute caller root set.

Static storage's `heap_roots` continues to enumerate non-null values without mutable root-location or safepoint semantics; default-only static storage still has none. No static writer is added here. A future collector must coordinate caller roots, static roots, pending exceptions, native scopes and safepoints. Arena indirection permits relocation without exposing addresses, but this increment performs no tracing, reclamation or collection and must not claim those features.

## Acceptance and following bridge

Public tests cover all eight kinds, exact defaults/round trips, signed narrow values and Boolean masking, floating bits, empty arrays, alias identity, arena growth, moved heap values, dropped/foreign heaps, null/bounds/type errors and failed-store atomicity. Exercise every quota at zero and exact boundaries, cumulative byte/element accounting, and recovery after rejected allocations. Private tests cover forged indices/generations and identity/accounting arithmetic limits unreachable through safe public construction.

Use independent Java reference cases for element conversions and observable primitive-array behavior when useful; synthetic tests must cover raw integer stores that javac normally narrows before emitting bytecode. Keep all existing 8,005 numeric/call outcomes unchanged. Require formatter, strict Clippy, workspace tests and optimized runtime/interpreter checks plus director review.

After heap acceptance, the proposed interpreter bridge supports internally allocated primitive arrays while keeping external arguments/results numeric. It can add null/local/reference stack operations, newarray, arraylength and primitive load/store instructions with explicit uncaught null/bounds/negative-size outcomes. It will require a separate contract. Object instances, reference arrays, array Class mirrors/assignability, constructors, initialization, exception dispatch, synchronization, GC and general Java startup remain outside this heap increment.

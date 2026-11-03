# Shared values and default static-storage review

Date: 2026-11-03.

**The shared values and bounded default static-storage increment is approved. The primitive-array heap described in `runtime-primitive-array-heap.md` is authorized for implementation. Full M2 and subsequent milestones remain open.**

## Review

The director reviewed runtime-owned Value/ValueKind, opaque references, the static-storage constructor and lookup paths, interpreter reexports and rejection boundaries, and the public/private regression tests. No blocking finding remains within this increment's contract.

Numeric values retain raw floating bits and category widths. Reference occupies one frame slot; construction is currently null-only. HeapRef contains private identity components and no host address. The relocation adds no reference bytecode support, public non-null issuer or storage writer.

Storage counts actual logical locations with checked arithmetic before allocation, validates retained class/field ownership and static category, and retains one value per declared static field. Field identity preserves hiding and loader/registry scope. Long/Double occupy one storage location despite occupying two frame slots. Descriptor classification caches by `(ClassId, constant-pool index)`, avoiding repeated scans of a long descriptor reused by many fields.

Every location receives its exact default, including positive floating zero and null object/array values. ConstantValue and class initializers remain unapplied. Construction publishes only complete local state and does not mutate registry or hierarchy metadata. Getters reject foreign and absent fields; root enumeration borrows non-null handle values without allocation, mutable root locations or collector claims. It is empty for current default-only storage.

## Final evidence

The coordinator independently checked the frozen source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 217 passed, zero failed |
| Release runtime/interpreter tests | 130 passed: 59 runtime, 71 interpreter |
| Numeric/floating/call differential outcomes | All 8,005 match in both profiles |
| Java fixture reproduction | 29 byte-identical classes; preceding 28 unchanged |
| Reference reproduction | All existing reference sets reproduced |

Genuine compiled metadata exercises 21 static defaults and confirms ConstantValue/class-initializer effects remain unapplied. Synthetic tests cover every primitive type, unresolved object descriptors, primitive/reference/multidimensional arrays through rank 255, interface constants, owner hiding, exact/zero/maximum policies, foreign identities and null root behavior. Private tests cover arithmetic overflow, invalid descriptors and corrupt locations. Interpreter boundary tests preserve typed reference-argument rejection and unsupported reference descriptors/instructions.

The director reviewed implementation and the coordinator's independent results; this record does not claim an additional director execution. Linux, hosted CI and Rust 1.85 remain unobserved.

## Next boundary

The next authorized increment implements `runtime::heap` with real bounded primitive-array allocation and private validated handle issuance. It adds no static mutation, class initialization or application startup. Root values and live allocations remain distinct concepts. A later interpreter bridge requires its own heap acceptance gate; the proposed contract is `m2-primitive-array-bridge.md`.

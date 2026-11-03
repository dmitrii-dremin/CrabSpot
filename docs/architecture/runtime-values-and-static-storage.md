# Runtime values and default static storage

Status: implemented and approved in `runtime-static-storage-review.md` on 2026-11-03. This increment supplies backend-independent values and actual default storage, without executing field bytecodes or advancing Java lifecycle state.

## Value ownership and references

Move the existing numeric Value and ValueKind definitions into `runtime`, preserving Int(i32), Long(i64), Float(u32) and Double(u64), numeric constructors/accessors, representation equality and slot widths. Reexport them from `interpreter` so existing import paths continue to work. The dependency direction becomes interpreter -> runtime; runtime never depends on the execution backend. Update exhaustive matches without changing supported numeric behavior.

Add `Value::Reference(Reference)` and `ValueKind::Reference`, occupying one JVM slot. Reference is an opaque Copy/Eq/Hash wrapper containing an optional opaque HeapRef. It offers `Reference::null()`, `is_null()` and `heap_ref() -> Option<HeapRef>`, with null also as its Default value. It has no public non-null constructor or handle issuer in this increment.

HeapRef is an opaque Copy/Eq/Hash identity with private owning-heap identity, entry index and generation components. It contains no host object pointer or memory address. No code in this increment allocates Java objects, issues a non-null HeapRef or claims handle validity against a real heap. The later allocator must establish heap identity, bounds and generation checks before issuing handles, and relocation must preserve application identity through indirection. These types establish the representation boundary without fabricating heap services.

The numeric interpreter continues to reject reference descriptors and reference instructions. Receiving a Reference value where a numeric argument is expected yields a typed error. Existing frame category operations remain width-based; adding the value kind does not claim reference bytecode coverage or allocation support.

## Default storage

Implement `StaticStorage::new(registry: &DefinitionRegistry, plan: &HierarchyPlan, options: StaticStorageOptions) -> Result<StaticStorage, StorageError>`. The registry must own every retained ClassId/FieldId. Materialize one value location for every declared static field in the plan, including interface fields, ordered by dependency-first classes and each class's declaration order. Inherited static fields are not copied into subclasses. Instance fields receive no static location.

`StaticStorageOptions` contains `max_slots: usize`, default 65,536. Count static logical value locations before allocating them, use checked arithmetic and avoid allocating from the policy maximum itself. Long and Double each consume one typed storage location, unlike their two execution-frame slots. Store compact field kinds or borrow validated metadata rather than copying long descriptors into every location.

Initial values are Int(0) for Boolean/Byte/Char/Short/Int, Long(0), Float(0), Double(0), and Reference(null) for every object/array descriptor. Floating zero bits are positive zero. ConstantValue attributes and explicit field initializers are not applied, including nonzero numeric or String constants and final/interface fields. This implements default storage allocation, not the full preparation procedure or initialization protocol. [JVMS preparation](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.2) distinguishes default values from initializer execution.

Provide `fields() -> &[FieldId]`, `slot_count() -> usize`, and `get(field: FieldId) -> Result<Value, StorageError>`. Unknown, foreign, nonstatic or unretained fields fail explicitly. Keep storage state private. There is no public write/final-field bypass, ConstantValue application method, initialized/prepared flag or Java field-resolution API in this increment.

Structured StorageError distinguishes registry/metadata failures, unsupported/invalid field descriptors, missing static locations and storage resource exhaustion. Build into local state and return the completed storage atomically; failure must not mutate registry or hierarchy state.

## Root enumeration contract

Provide `heap_roots(&self) -> impl Iterator<Item = HeapRef> + '_`. This is an allocation-free borrowed iterator of non-null handle values, not mutable root locations or a stop-the-world snapshot mechanism. Null slots are filtered out. With default-only storage and no handle issuer, the iterator is currently empty by construction. Document that fact rather than claiming a working collector.

Stable managed handles allow a future moving collector to relocate heap entries behind indirection without rewriting these reference values. Future non-null writes, object tracing, native root scopes and safepoint synchronization require an explicit heap/execution contract; this iterator alone provides none of them.

## Acceptance

Run existing numeric/call differential tests unchanged in behavior after the value move; all 8,005 recorded JDK cases must still agree in debug and release. Update public examples/integration exhaustive matches to report unexpected references rather than fabricate numeric results.

Use genuine class metadata plus public synthetic tests to cover every primitive, object and array descriptor, static versus instance ownership, superclass/interface static fields, field hiding, exact positive-zero bits, and nonzero ConstantValue attributes remaining unapplied. Test foreign registry/plan/field handles, absent fields, zero/exact/maximum slot limits, count overflow privately, and atomic failure. Reference API tests establish null identity, kind/width and root filtering without inventing non-null allocations.

Run formatter, strict workspace Clippy, full workspace tests, optimized runtime/interpreter tests and fixture reproduction. A separate director review gates the next storage mutation/initialization or heap increment. No Java startup, verification, initialization, GC, field opcode or non-null reference support is credited here.

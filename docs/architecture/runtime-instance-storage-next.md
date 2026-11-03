# Next object-storage design work

Status: superseded by the authorized concrete contract in `runtime-instance-storage.md` on 2026-11-03. This file preserves the earlier design questions; use the concrete contract for implementation.

The next proposed M2 dependency is actual ordinary-instance storage and owner-qualified fields, followed by a reviewed field-body execution bridge. The registry, hierarchy plan, shared values and managed-reference boundary already supply the necessary identities. This should add usable storage and execution services without labelling allocation as successful construction or initialization.

## Required decisions before implementation

Keep objects and primitive arrays inside the same ManagedHeap ownership namespace. Extend arena entry kinds and reject wrong-kind service access explicitly. Existing primitive-array construction and quota behavior must remain compatible. Instance count, individual/aggregate field slots and retained layout metadata need independent limits; array payload bytes must not silently become a claim to account for all object memory.

Instances carry ClassId and a bounded vector of typed field values. Share immutable layout metadata between instances, retaining each class's own fields and superclass link rather than copying every ancestor field list into every class or object. All inherited instance fields have real value locations, with Long/Double occupying one logical storage location. Hidden fields remain distinct by FieldId owner.

One heap must eventually support unrelated classes and therefore multiple hierarchy plans. Before choosing an API, specify atomic layout admission, registry ownership, deduplication and conflict handling when two plans describe the same ClassId with different resolved ancestors. Never let a later plan reinterpret existing object field locations. A one-root-only prototype must not become an implicit permanent heap restriction.

Default values follow the existing static-storage representation. Instance allocation leaves ConstantValue and Java constructors unapplied and rejects directly allocating interface/abstract definitions. Reference fields begin null. A first mutation service may support numeric values and null, with non-null stores explicitly unsupported until reference assignability exists. It must reject writes to final fields; later constructor execution requires a reviewed privilege/uninitialized-reference protocol, not a public bypass flag. Host field service access is distinct from Java field resolution and access checking.

Preserve exact signed/unsigned subword normalization and floating bits. Before field bytecodes land, compare synthetic raw integer field stores against JDK 21: javac commonly inserts narrowing conversions and cannot alone test all raw-store cases. JVMS SE 21 and later clarifications should be reconciled explicitly in that contract.

Objects eventually add child references to heap tracing. Specify an allocation-free borrowed child-handle iterator separately from caller/static roots. Neither iterator is a safepoint, collector or retention policy. Object allocation must not manufacture verified, prepared, constructed or initialized proof states.

## Execution follow-on

The field-body bridge must name its receiver, MethodId/ClassId context, field resolution scope, access/finality checks, argument/return reference rules and heap ownership. Select genuine methods using instance state for differential tests. Do not hardcode fixture field offsets, skip `<init>`/`<clinit>`, invent Object behavior or call this general Java invocation.

Object/reference arrays, complete assignability, cross-class dispatch, exception objects/handlers, monitors and lifecycle execution remain separate responsibilities until their concrete contracts and evidence are reviewed. Full M2 stays open until its remaining allocation, field, invocation, exception and synchronization requirements are satisfied.

# ADR 0004: Require evidence gates and preserve future runtime invariants

Status: accepted for the project roadmap.

## Context

The charter requires independent technical review and a runtime architecture that can grow toward moving GC, threading, JNI and JIT compilation. An inspection milestone must not accidentally fix runtime identity or reference semantics.

## Decision

The technical director reviews architecture changes and milestone evidence. Foundation parser completion is recorded separately from full M1 approval; M2 begins only after M1 approval. Use the acceptance checklist in `docs/architecture/overview.md` and maintain compatibility and milestone reports with actual executed results. NanoSpec artifacts and workflows are prohibited throughout this project by the user's instruction.

Future runtime class identity includes the defining loader. Class lifecycle and Java exception propagation are explicit. References use a managed abstraction compatible with relocation; runtime APIs identify GC roots and safepoint obligations before allocation/thread features land. Category-one and category-two values are explicit. Interpreter services are independent of a future JIT; VM-native services are independent of the JNI ABI. A future JIT backend choice requires its own ADR.

## Consequences

Missing parser validation cannot be disguised by successful example files. Future components have design constraints without premature empty crates or speculative ABI commitments. Performance work cannot bypass semantics, verification or milestone review.

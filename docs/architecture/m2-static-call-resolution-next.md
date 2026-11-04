# Next: registered static-call resolution and active use

Status: proposed direction on 2026-11-04 after the bounded initialization approval in `m2-initialization-session-review.md`. No production implementation is authorized by this note. An exact contract must precede worker dispatch.

The new initialization scheduler can suspend a checked static call, initialize its actual declaring owner and resume its arguments through the shared frame engine. The remaining same-class invokestatic restriction prevents ordinary helper/factory calls across admitted classes and makes realistic initializer graphs unnecessarily narrow. The next coherent increment should add lawful registered static-method resolution and reuse that continuation, without adding constructors, String fabrication or another execution loop.

The exact contract must settle the following from Java SE21 method/interface resolution and invokestatic rules:

- Methodref versus InterfaceMethodref version rules, symbolic-owner kind, class-hierarchy and interface resolution, and interface static-method noninheritance. Resolution must not silently treat an inherited instance/default candidate as an eligible static declaration.
- Actual MethodId ownership, class/member access, defining-loader runtime packages, protected-static rules and private/nest limitations. Reuse the existing graph, bindings and shared work counters; keep unsupported loading constraints explicit, including reference-bearing signatures across distinct loaders.
- Symbolic/resolved/selected descriptor identity, argument extraction and validation order, static-category failures, special-name exclusion, native/synchronized boundaries and precise caller/callee provenance. Preserve existing numeric/reference call and cache-role guarantees.
- In an initialization session, request initialization of the resolved declaring class/interface, rather than the symbolic owner. Retain the original call PC and typed arguments exactly once. In older host-controlled profiles, define explicitly which opt-in runtime-aware paths gain resolution while retaining their absence of automatic initialization; standalone legacy execution remains separately scoped.
- Checked traversal, descriptor work and cache/frame/fuel accounting, with zero/exact/MAX boundaries, no uncharged name copying, no policy-sized allocation and no silent unsupported-candidate bypass.

Runtime work should expose checked static resolution over existing admitted identities. Interpreter work should replace the restricted registered-method resolver with that service and feed the existing call/preparation/initialization continuations. Root should provide genuine cross-class static helpers called from ordinary bodies and initializers, inherited declaration versus symbolic-subclass cases, interface-owned statics, mixed reference/category-two signatures, recursion, once-only effects and supplied Error propagation. Synthetic tests must cover access/loading/category/order/resource failures, initializer-name exclusion and unchanged legacy behavior.

This direction advances reusable execution toward full M2. String constants, exception construction, constructor uninitialized-reference protocols, synchronization, verification, library bootstrap and application startup remain separate obligations rather than implicit consequences of method resolution.

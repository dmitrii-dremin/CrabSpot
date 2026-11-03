# M2 bounded static-call increment review

Date: 2026-11-03.

**The bounded same-class static-call increment is approved. The definition-registry increment in `runtime-foundation.md` is authorized. Full M2, M3 and M4 remain open.**

## Review outcome

The director reviewed the explicit frame engine, call-reference lookup, prepared-method cache, arguments and returns, resource accounting, diagnostics, all 17 public synthetic call tests, genuine reference integration and final cleanup. No blocking findings remain within the body-graph contract.

Calls use actual symbolic owner/name/descriptor metadata. Semantic aliases work; class/interface reference kinds and the interface-invocation version boundary are checked. Lookup stays within the supplied class or interface and never pretends to search a missing superclass. Unsupported targets, reference descriptors, initializers, native/synchronized declarations and handlers fail explicitly. A new target's entire code is decoded before entry.

The engine uses a vector of frames without recursive Rust invocation. Arguments transfer in descriptor order, category-two values occupy their required local slots, caller operands survive, and returns resume the saved caller offset. Numeric return narrowing and raw floating value transfer are preserved. A descendant's uncaught arithmetic outcome stops the graph.

One instruction budget covers all frames, including calls and returns. Active frames reserve full local-plus-stack slot capacity and release it on return. Prepared method Code volume is charged once per distinct target and remains charged across returns, with limits checked before decoding. Accounting uses checked arithmetic. These policies count specific resources and are not described as exact heap-memory limits.

Errors include selected method identity and an available call-site location. Their optional boxed payloads allocate on failure rather than on every frame transition. Final Clippy cleanup preserved the reviewed behavior.

## Evidence

The coordinator independently validated the frozen source on native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 153 passed, zero failed |
| Release interpreter tests | 66 passed, zero failed |
| JDK differential cases, both profiles | 8,005 agree: 616 integral, 7,263 floating, 126 call cases |
| Fixture reproduction | 24 byte-identical class files |
| Reference reproduction | All four reference sets passed |

The 126 genuine call cases cover class and interface body graphs. Synthetic evidence includes exact shared-fuel boundaries, mixed-width argument/return transfer, caller stack overflow, incorrect references, callee decoding provenance, sparse frame reservations, released frame slots, retained cache charges and repeated cached callees. A 20,000-instruction recursive graph exhausts its shared budget through explicit frames without host-stack recursion.

Execution now supports 148 effective opcode values, with `invokestatic` explicitly restricted to the body-graph subset. The director reviewed source and reported validation evidence, and does not claim another independent execution of the coordinator's commands. Linux, hosted CI and Rust 1.85 remain unobserved.

## Next boundary

The authorized runtime registry establishes scoped loader/class/member identities, immutable definitions, bounded byte sources and atomic admission. It does not manufacture verification, linking or initialization success. Cross-class invocation, fields, objects, arrays, exception dispatch and monitors remain unsupported until their real services are implemented and reviewed. The launcher continues to inspect classfiles; this approval does not create a Java application startup API.

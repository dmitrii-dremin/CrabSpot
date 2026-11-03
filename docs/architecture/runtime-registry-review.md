# Runtime definition-registry review

Date: 2026-11-03.

**The definition-registry increment is approved. The hierarchy/layout increment in `runtime-hierarchy-and-layout.md` is authorized. Full M2, M3 and M4 remain open.**

## Accepted behavior

The director reviewed registry/source/handle/error code, private invariant tests, public API tests, genuine metadata integration and documented boundaries. No blocking findings remain.

Definitions are keyed by exact Java UTF-16 name and defining-loader identity. Equal names in different loaders remain distinct; unpaired surrogates are not collapsed through display conversion. Opaque handles include registry ownership, with checked member and owner indices. The sole global state is a nonwrapping registry-token allocator; definitions and source policy remain local to each registry.

Admission validates names, duplicates, limits, parsed metadata and declared-name agreement before publishing identity or accounting. Failure preserves prior definitions and available capacity. Metadata remains immutable, and own-member lookup does not search an unresolved superclass. Module descriptors and arrays are explicitly outside this ordinary-definition service.

Source loads return existing definitions before reading again. Misses receive the minimum parser/per-definition/remaining-total byte allowance, and returned data is checked even when a source violates its limit. Source absence, retrieval failure, parse failure, mismatch, duplicate, invalid handle and policy exhaustion remain distinguishable. Byte quotas measure source volume, not exact Rust heap allocation.

## Validation

The coordinator independently checked frozen source using native Windows Rust/Cargo 1.98.1:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 178 passed, zero failed |
| Release runtime tests | 25 passed, zero failed |

The runtime tests comprise four private invariant tests, 18 public synthetic tests and three genuine-definition integrations. They cover all foreign-handle kinds, private forged indices, token exhaustion and concurrent uniqueness, checked arithmetic, atomic failures, exact/zero/maximum policies, UTF-16 identity, source caching and returned-length violations. All 23 ordinary classes from the existing 24-class fixture corpus are admitted with unchanged metadata; its genuine module descriptor is rejected without altering registry state.

The preexisting interpreter/parser/decoder suites remain green. Those layers were not changed by this increment. The director reviewed source and reported coordinator evidence, without claiming another independent command execution. Linux, hosted CI and Rust 1.85 remain unobserved.

## Next increment and limits

Registration is explicitly unverified and unlinked. It creates no Java loader object, hierarchy proof, initialized class, exception object or execution service.

The next plan accepts one supplied registered root and an explicit resolver over registered definitions. It validates the listed superclass/interface relationships and computes owner-qualified field locations through parent-linked layouts. Nodes, edges, retained declared fields and per-class instance slots have separate limits. It does not duplicate inherited field lists or synthesize missing Object metadata. Interface layouts have no instance slots. Resolver, registry, source and initialization boundaries remain distinct.

The accepted coordination API and deliberate deferred checks are in `runtime-hierarchy-and-layout.md`; full class derivation, sealed/module/access/override rules, verification, storage initialization and Java startup are not credited by this approval.

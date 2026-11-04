# Reference arguments and results review

Status: approved by the technical director on 2026-11-04 for the bounded reference-argument/result and areturn increment. Frozen technical review and the coordinator's integrated gate satisfy `m2-reference-calls.md`. Full M2 remains open. New hosted Linux/Windows evidence is pending publication.

## Reviewed invariants

Runtime-aware method bodies now exchange actual heap references through the existing static-helper, virtual, interface and ordinary nonconstructor special paths. Prepared methods retain compact computational kinds, slot counts and return tags. Borrowed descriptor spans are temporary; the cache contains no copied nominal names or resolved signature trees. The bounded prefix classifier charges each reference-bearing descriptor before a long parse, including null and cache-hit operations, while numeric-only signatures preserve earlier lookup budgets.

Call-site shapes govern typed extraction and interface slot counts. Resolved declarations govern access and the origin/resolved nominal-loader restriction; selected declarations govern locals and argument/result identities. The selected nominal profile audits the complete admitted receiver superclass/interface graph, including matching intermediate/private/static declarations. This is a documented conservative unsupported boundary, not full JVM loading constraints or class preparation. Graph nodes, declaration comparisons and every enqueued edge consume shared bounded work. Numeric/primitive-array profiles retain their specified no-extra-traversal behavior.

Argument validation checks count and every computational kind before reference resolution. Non-null values validate their actual source identity, resolve the selected declarer's borrowed descriptor with identity-only access, then check assignment. Null skips universe/target resolution only after signature/profile obligations. The same result policy runs at areturn before frame removal. Successful transfer preserves the exact handle; no caller-side recast or name resolution is added. New mismatch/type/loading diagnostics remain host MethodAccess failures, including nested TypeJava variants.

The interpreter preserves one MethodId-keyed engine, existing static/instance cache guards, root/receiver ordering, frame/slot/code/fuel limits and per-frame CP origins. Same-class static helpers use registry-validated borrowed metadata without the earlier redundant owning descriptor parse. Earlier standalone numeric/heap APIs remain restricted. Runtime-aware coverage is 191 effective opcodes.

## Review findings and deliberate changes

Structural invocation-shape inspection initially risked moving array-owner rejection ahead of typed stack extraction by sharing the selector parser. The contract and helper now explicitly defer UnsupportedArrayOwner to selection. Exact empty-stack and wrong-kind regressions establish the old precedence, followed by unsupported-owner rejection after valid extraction.

Runtime shape preparation now checks context validity before later root receiver/field operations. Consequently, invalidated contexts report MethodAccess(ContextChanged) at that required earlier boundary. Two legacy context tests were deliberately updated. Other migrated tests only replace obsolete reference-descriptor exclusions with the newly applicable count, return, lookup or success behavior; unsupported constructors and old entry modes remain covered.

The review also strengthened selected-result coverage: a separately allocated Base may return through a Child receiver when the declared result is Base. The successful child case alone would not distinguish declaration-based validation from an erroneous receiver-based check. Runtime reservation helpers were factored for deterministic capacity-overflow tests without semantic/accounting changes.

The coordinator found a separate fixture-provenance defect: some Windows working sources had CRLF bytes while Git's committed blobs use LF. Recorded raw source hashes therefore differed across checkouts, and CI regeneration before checking concealed the stale provenance. Seven working sources were canonicalized, including the new fixture. All 92 class files and fifteen TSV outputs were byte-identical before and after the repair. Existing 88 class hashes remain unchanged.

Both generators now reject any CR source byte before JDK subprocess calls; accepted source bytes are hashed without normalization. Oracle metadata records the source identity. A pre-regeneration CI check verifies the complete source/hash set and matching JSON/TSV record sets. Review caught the missing-record case, which is now rejected alongside orphan metadata. The director independently ran all six provenance regressions and verified 37 canonical Java sources and fifteen records.

## Coverage

Twenty-two public runtime tests and four private tests cover compact numeric/reference shapes; all-kind-before-reference checks; declared argument/result identities; inaccessible descriptor types; origin/resolved and intermediate-loader restrictions; interface/private/static matches; unrelated signatures; primitive-array cross-loader profiles; actual cross-loader values; exact work/symbol/unit limits; foreign/stale/forged handles; context replacement; and preserved old heap values/edges on failure.

Adversarial cases include the exact 72-step diamond audit, a 128-branch shared-interface graph with pre-enqueue and repeated-operation charges, a 10,000-class iterative audit/drop, and one 65,535-unit descriptor shared by eight methods. Private cases pin prefix/rank/slot boundaries, charge-before-full-parse behavior, borrowed span identity, fallible reservation overflow and structured error sources.

Twenty-one interpreter cases plus one private mapping guard cover host and nested reference transfer, mixed Long/Double/reference slots, all call forms, nominal/primitive/multidimensional arrays, null/self/distinct identities, aliases/cycles, hidden types, exact limits, old modes, cache-mode guards, malformed values/returns, loader boundaries, return/caller-capacity provenance and persistence after late work failure. A 10,001-frame reference recursion remains iterative. Four tiny cached helpers share a 65,004-unit nominal result descriptor without retaining its name per prepared body.

The original ReferenceCall fixtures add 904 independent JDK outcomes over twenty receiver/method pairs: 872 normal results, twelve null, sixteen bounds and four ArrayStoreException outcomes. Supplemental source-derived assertions inspect saved/input/alternate identities, state, every retained array type/content/edge and heap totals. Exact helper descriptors and decoded b0/b6/b7/b8/b9 establish actual reference-transfer coverage. Host receivers still use default storage; constructors execute only in the JDK oracle driver.

## Frozen evidence

The director verified all 29 combined files against `target/coordinator/reference-calls-freeze.json`, after separately reviewing and verifying both nine-file component manifests. Principal SHA-256 values describe native working-copy bytes before Git line-ending normalization:

| File | SHA-256 |
| --- | --- |
| runtime/src/method_values.rs | `504C421B3A98B70D81FF89FA63C0F22DA245A8FD6ADFB18DA300E8684EB0038F` |
| runtime/src/method.rs | `0CA938DE1CF4113BB0C68CFE22A4B076179D5B6BC02A05D275DBBB67D4A1A30A` |
| interpreter/src/execute.rs | `59EF29D8595AEF7E8DB053110DEBC9E820BE9F14ADE741379E72D0E31FC63491` |

The coordinator independently observed on the frozen combined source:

- Formatting and strict workspace/all-target Clippy pass.
- All 680 workspace tests and 593 optimized runtime/interpreter tests pass, with zero failures. The optimized split is 320 runtime and 273 interpreter.
- All 25,325 execution comparisons match in both profiles: 24,421 previous cases plus 904 reference-call cases. Separate 151-case array and 151-case field service oracles pass.
- All 92 class fixtures and fifteen reference-output sets reproduce after the provenance repair; all previous 88 class hashes remain unchanged. The source preflight and six Python regressions pass.
- The identical 92-class corpus passed 10,000 seeded parser mutations: 158 accepted, 9,842 rejected, ten deterministic rechecks, corpus 250 and zero failures; seed 4851011501752864596.

All 29 hashes were reverified after the gate. A mapped-file copy refusal during integration did not change source: inspection established that types.rs already matched its frozen bytes and remained Admin-owned, so the coordinator skipped the identical destination. No ACL change, escalation workaround or process termination was used. Fresh hosted CI remains unobserved for this increment until publication.

## Boundary and next direction

This approves checked reference transfer in the selected-body kernel. It does not approve constructors/uninitialized values, initialization, general static lifecycle, exception dispatch, complete verification/loading constraints, GC, Java startup, full M2, JVM conformance or performance superiority. The next direction is Throwable identity and exception propagation, proposed separately in `m2-exception-dispatch-next.md`; it requires an exact contract before implementation.

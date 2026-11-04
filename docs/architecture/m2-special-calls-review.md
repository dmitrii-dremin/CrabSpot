# Ordinary special-call review

Status: approved by the technical director on 2026-11-04 for the bounded ordinary nonconstructor special-call increment. Both frozen worker snapshots and the coordinator's exact integrated source have satisfied the contract and acceptance gate. Full M2 remains open.

## Reviewed behavior

The runtime's select_special keeps symbolic resolution, legal owner relationships, receiver compatibility and prescribed lookup separate. Class ancestor references start at the current class's direct superclass even when the raw ACC_SUPER bit is clear. The actual receiver cannot redirect a special call to its override or contribute unrelated defaults. Direct-interface calls use the selected interface closure and explicit Object dependency.

Resolution/access/staticness precede the structural owner check and null. Non-null receivers must satisfy current, symbolic and selected declaring identities. Selection skips static declarations, retains eligible private instance declarations, and checks abstractness after null. It reuses bounded interface candidate analysis without importing virtual override/finality rules. The interface Object edge remains excluded from protected class-subclass access.

The interpreter uses the existing frame loop, current-frame ClassId/CP, MethodId cache and ResolvedInstance preparation. Special, virtual and interface calls share prepared-code accounting while static/instance checks remain enforced. Nested calls use their ordinary dispatch rules. Earlier numeric and heap-only entry points continue to reject b7. Constructors and reference signatures remain explicitly unsupported.

## Review correction

The first compiling runtime snapshot charged the origin and owner nodes before comparing an interface origin's Object dependency, but omitted the separate edge comparison charge. Review required a step before that comparison. The correction changes policy accounting, not method selection. The exact regression reaches null at a total of 14 steps, while limit 13 fails before comparison with required 14; the two-direct-interface-edge control reaches null at 15 and fails at 14. Existing b6/b9 budget regressions remain intact.

## Evidence reviewed

The 23 new runtime public tests cover current/private calls, old/current ACC_SUPER behavior, older symbolic ancestors, static/private selection candidates, access and null precedence, Object lookup/access, legal and rejected owners, loader-separated identities, abstract/default conflicts, cumulative budgets, invalidation and a 10,000-class traversal. Extended private tests cover forged origins/helper inputs and structured error-source preservation.

The 20 new interpreter public tests cover shared cache and exact frame/slot/code/fuel limits, special-to-virtual/interface/static transitions, mixed numeric widths, field ownership, decode and return-transfer provenance, 10,000-deep iterative recursion, and retained writes/allocations after Java and host failures. Earlier runtime-aware rejection expectations were updated with concrete special-call results while preserving the old entry-point restrictions.

The coordinator's original Java fixtures provide 450 independently generated JDK outcomes across 36 receiver/method pairs. They cover superclass versus ancestor selection, class-super inherited defaults, interface-super inherited/own defaults, nested interface-super calls, later dispatch, field hiding, mixed numeric/void bodies, recursion and primitive arrays. Two arithmetic and four negative-size outcomes are included. Additional field-state assertions are explicitly source-derived rather than claimed as recorded JDK state observations. The host supplies default storage; Java constructor execution in the oracle does not imply constructor support in CrabSpot.

Frozen runtime production SHA-256: `532D5076B1C45DBE822A1A9427E25FF083C4A62DE69C2BF7964A2687E8B4BD1B` (method.rs). This is the native working-copy byte hash before Git line-ending normalization.

Frozen interpreter production SHA-256: `6EA75E1A2C6F437626F7AE68B21425D119D72E52D4FD4385664F8906433D0DD5` (execute.rs), also for native working-copy bytes before Git line-ending normalization. The director verified both worker fingerprints directly.

An initial Windows mapped-section refusal prevented copying the exact frozen execute.rs bytes. The coordinator recovered through a same-directory atomic replacement, verified exact bytes and retained Admin ownership without ACL changes or process termination, then reran the combined checks. Acceptance uses those post-recovery results, not the earlier runs.

The coordinator independently observed:

- Formatting and strict workspace/all-target Clippy pass.
- 475 workspace tests and 388 optimized runtime/interpreter tests pass, with zero failures. The optimized split is 211 runtime and 177 interpreter tests.
- All 11,119 execution comparisons match the JDK reference outputs in both profiles: 10,669 accepted earlier cases plus 450 special-call cases. Both separate 151-case heap/field service oracles also pass.
- All 65 genuine class fixtures and ten reference-output sets reproduce; the previous 59 class hashes are unchanged.
- The expanded 65-class parser smoke campaign completes 10,000 cases with zero failures: 117 accepted, 9,883 rejected, ten deterministic rechecks, final corpus 182, seed 4851011501752864596.

Both production fingerprints were reverified after the final runs. The coordinator subsequently inspected complete [run 41795065210](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41795065210) for implementation commit 8d2c523: Linux and Windows passed every check with Rust 1.99.1 and Temurin JDK 21.0.13, including checked-in/regenerated fixture tests, all ten reference sets, strict lint, optimized tests and parser smoke. This hosted evidence is recorded separately after source approval.

## Remaining boundaries

This is ordinary numeric/void method-body execution over explicitly registered definitions and supplied heap instances. It does not establish construction, class initialization, verification, general loading constraints, nest/module services, reference arguments/results, native binding, monitors, exception handler dispatch or JVM startup. The bounded [special-call contract](m2-special-calls.md) governs the increment. [Reference types and casts](m2-reference-types-next.md) are a proposed next direction, not part of this acceptance.

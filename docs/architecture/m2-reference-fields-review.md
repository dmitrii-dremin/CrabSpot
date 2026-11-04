# Checked reference-field review

Status: approved by the technical director on 2026-11-04 for the bounded checked non-null instance-reference-field increment. Frozen technical review and the coordinator's integrated gate satisfy `m2-reference-fields.md`. Full M2 remains open.

## Reviewed behavior

The existing checked field service now stores assignment-compatible non-null ordinary-instance and primitive-array references. Resolution/access/loading constraints, staticness, finality, receiver and computational-kind checks preserve their previous order. Numeric and null stores retain the old path without a universe or descriptor-resolution requirement. Non-null stores validate the universe and source identity before resolving the selected declaration's descriptor and applying the shared assignment relation.

Descriptor identity uses the selected field's declaring loader. The internal identity-only policy permits a public field whose nominal type is inaccessible to its caller, while cast resolution retains caller accessibility checks. The existing nominal cross-loader loading-constraint restriction remains explicit. Every non-null store charges the complete UTF-16 descriptor, including cache hits, before scanning or hashing; graph traversal and binding limits remain shared with earlier services.

The public low-level heap setter still rejects non-null reference values. Only the context invokes a crate-private commit after successful assignment. That operation rechecks destination ownership, field relationship, finality, reference storage, source handle and slot bounds before changing one location. Failed stores preserve contents and children. Successful stores preserve exact reference identity and all heap allocation/accounting totals. Child enumeration yields each non-null logical slot in order, retaining aliases and cycles without recursion or allocation.

The interpreter needs no new production execution algorithm, opcode or entry point. Its existing b5 path supplies the actual value, typed receiver and current declaring origin. Runtime-aware coverage remains 186 effective opcodes. Boxed assignment/descriptor failures remain FieldAccess host errors; a nested type-service Java category is not reinterpreted as an instruction-level Java exception. The added private regression pins that distinction.

No blocking implementation defect or semantic contract change was required during this review. The old context-level non-null rejection test was deliberately updated to the new MissingUniverse boundary, with a separate assertion retaining raw-setter rejection. Earlier state remains visible after later Java, host or policy failures.

## Coverage and source review

The runtime adds thirteen public cases and two private guards. They cover self-links and distinct-object cycles, aliases, overwrite/null clearing, inherited and hidden locations, nominal/interface assignments, all eight primitive arrays, inaccessible descriptor types, declaring-loader identity, failure precedence and atomicity, raw refusal and guarded commit failures. Exact first/repeated-store work limits are 22 and 37; the preceding limits fail without mutation. Long descriptor coverage uses 65,535 UTF-16 units and pins cache-hit charging, while a 10,000-class assignment graph exercises iterative traversal. Late resolver, symbol, unit and relation-work failures retain prior contents.

The interpreter adds fifteen public tests and one private mapping regression. They exercise retrieved references through fields, arrays, casts and virtual/interface/ordinary-special calls; current declaring origins; aliases/cycles/clearing; old-mode and reference-signature boundaries; exact shared limits; late assignment-budget failure; category-two local overlap; provenance and effects retained after failures.

The coordinator's original ReferenceField fixtures provide 690 independent JDK outcomes across 42 receiver/method pairs. The 100 exceptional outcomes comprise 14 ClassCastException, 54 NegativeArraySizeException, 28 NullPointerException, two ArithmeticException and two bounds outcomes. Supplemental field, child-order and allocation assertions are source-derived and identified as such. The public-field/hidden-type fixture tests assignment from a different package without adding cast-style type accessibility. Host allocation supplies default storage; these comparisons do not execute constructors in CrabSpot.

The director verified all twelve integrated files against `target/coordinator/reference-fields-freeze.json`. Principal SHA-256 fingerprints are for native working-copy bytes before Git line-ending normalization:

| File | SHA-256 |
| --- | --- |
| runtime/src/field.rs | `B88C32A2CA710FC7B71C1A001964D6BCD06E53E5A8EF8FFC93B44DFDF9C071CE` |
| runtime/src/instance.rs | `5F6AFF3B6A02E95D67740BBCB9FFE19DF89A131AE4700B1E7CB0F2407514D961` |
| runtime/src/types.rs | `297AAF2454810D85030690897FAD651666908074336CAB986A41F630C1DD2D4D` |
| interpreter/src/execute.rs | `2DFA5AB64B26CBCAFD2CA495C0640576FC05178C824583C2B116C0B74409AD7D` |

## Integrated evidence

The coordinator independently observed on the frozen combined source, using local Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

- Formatting and strict workspace/all-target Clippy pass.
- All 551 workspace tests and 464 optimized runtime/interpreter tests pass with zero failures. The optimized split is 253 runtime and 211 interpreter tests.
- All 13,085 execution comparisons match in both profiles: 12,395 earlier cases plus 690 reference-field cases. The separate 151-case array and 151-case field service oracles also pass.
- All 80 genuine class fixtures and twelve reference-output sets reproduce; the previous 74 class hashes remain unchanged.
- The 80-class parser smoke campaign completes 10,000 cases with zero failures: 125 accepted, 9,875 rejected, ten deterministic rechecks, final corpus 205, seed 4851011501752864596.

The coordinator reverified all twelve frozen hashes after these checks. Fresh hosted Linux/Windows validation for this increment remains pending publication; the earlier reference-type increment's passing CI is separate evidence.

## Boundary and next direction

This increment supplies checked ordinary reference-field writes and observable object-graph edges. It does not establish construction or initialization, implement reference method signatures or reference-array allocation, remove loading-constraint restrictions, add static writes, execute handlers, verify bodies, trace/collect objects or start Java applications. Full M2, complete JVM conformance and performance superiority remain unapproved.

The proposed next direction is actual reference-array storage and anewarray/aaload/aastore, described in `m2-reference-arrays-next.md`. That proposal requires an exact API, quota, identity and instruction/error-order contract before implementation.

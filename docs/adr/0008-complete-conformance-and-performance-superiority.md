# ADR 0008: Complete conformance, behavioral compatibility and performance superiority

Status: accepted by the technical director on 2026-11-03, implementing the user's revised long-term objective. This is a goal/acceptance decision, not an implementation or milestone-completion claim.

## Context

The original endpoint was a working Kotlin Spring Boot WebFlux application. The user now requires the complete JVM specification, the required observable behavior of HotSpot and other conforming open JVMs, and better performance. Application compatibility alone cannot establish that broader result.

## Decision

Java SE 21 is the first complete JVM specification target. Maintain an obligation inventory across the selected JVMS and applicable Java Memory Model/native/runtime-library integration contracts. Future releases require an explicit target decision and separate evidence. Existing staged implementation and technical-review gates continue unchanged.

Use HotSpot as the primary behavioral oracle and benchmark baseline, identifying exact builds and configurations. Use other conforming open JVMs for independent checks where available. Specifications govern required behavior and disagreements; permitted variation and nondeterminism are assessed as allowed behaviors, not identical traces. Bug-for-bug equivalence and private HotSpot implementation details are not conformance requirements.

Require demonstrated performance superiority on a predeclared representative suite and supported-platform scope. Review workloads, metrics, comparison conditions, statistical method and suite-level win criteria before collecting acceptance measurements. Publish reproducible results, resource costs, regressions and tradeoffs. Include realistic applications alongside diagnostic microbenchmarks and separate startup/warmup from steady-state behavior. Do not promise every-workload/every-hardware superiority or weaken semantics to produce a win.

Spring Boot becomes an integration milestone. Ultimate success jointly requires complete target-specification conformance, required observable compatibility and measured performance superiority. Any certification claim needs separate evidence.

## Consequences and review

The technical director approves this direction as compatible with the existing independent Rust architecture: differing internal representations, collectors and compilation strategies can pursue speed while preserving required behavior. No backend or optimization is selected by this ADR.

All current approvals and limitations remain valid at their original scope. No new feature implementation, compatibility result or benchmark is claimed. The charter now adds full conformance/performance acceptance beyond the existing application and JIT milestones; implementation continues in reviewed increments. NanoSpec remains prohibited.

# Runtime hierarchy/layout review

Date: 2026-11-03.

**The bounded hierarchy/layout increment is approved. The value relocation and default-static-storage increment in `runtime-values-and-static-storage.md` is authorized. Full M2, M3 and M4 remain open.**

## Review and correction

The director reviewed the iterative graph builder, loader-context name cache, resolver validation, relationship checks, parent-linked layouts, query behavior, public tests and private guard regressions. No blocking findings remain within the listed structural/layout scope.

One specification correction was required: an interface contributes no inherited Object field storage, but its direct Object reference still has to reject a final superclass under [JVMS class derivation](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.3.5). The implementation removed its exemption and a regression checks the final-Object/interface case. The separate zero-instance-storage rule for interfaces remains intact.

The traversal marks only active ancestors gray, allowing shared-interface diamonds while rejecting cycles. Resolver results must belong to the registry and have the requested semantic name; explicit delegation can return a different defining loader. Cached context/name choices remain stable for a build. No missing superclass is replaced with fabricated Object metadata.

Each definition retains only its own field vectors and inherited-slot count. Field locations preserve declaring-owner identity and hiding, static locations stay with the declaring class, and Long/Double fields use one logical typed-value location. Queries follow bounded parent links. Node, edge, retained-field and per-class slot policies use checked arithmetic. Missing internal parent layouts return an error rather than silently inventing zero inherited storage.

## Final evidence

The coordinator independently checked frozen source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 199 passed, zero failed |
| Release runtime tests | 46 passed, zero failed |
| Java fixture reproduction | 28 byte-identical class files |
| Existing differential reference reproduction | All 8,005 outcomes unchanged |
| Expanded-corpus parser smoke campaign | 10,000 cases, zero failures |

The new evidence includes genuine compiled superclass/interface/field-hiding metadata and a 10,000-class synthetic chain whose retained own-field entries remain linear. Build, ancestor-field queries and destruction use no host recursion. Public tests cover shared diamonds, genuine cycles, distinct loader identities, explicit delegation, wrong resolver results and policy boundaries. Private tests cover arithmetic extremes, forged handles and damaged internal query state.

The structural Object test fixture is explicitly supplied test data, not a production fallback or copied runtime implementation. Earlier fixture artifacts retain their hashes. The director reviewed source and reported coordinator evidence, without claiming another independent execution. Linux, hosted CI and Rust 1.85 remain unobserved.

## Deliberate limits

The plan proves only its documented relationship and layout checks. Complete class derivation, sealed/module/access/final-method rules, verification, method selection, storage mutation, initialization, managed allocation and general Java startup remain outside this increment.

The next authorized work moves shared values into runtime and allocates bounded default static locations. It exposes only null reference construction and read-only storage. ConstantValue attributes remain unapplied, no non-null handle is issued, and no prepared/initialized proof state is manufactured. Root enumeration reports non-null handle values rather than mutable locations or a collector/safepoint protocol.

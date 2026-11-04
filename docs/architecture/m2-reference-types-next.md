# Reference types after ordinary special calls

Status: technical direction proposed on 2026-11-04 following the bounded approval in `m2-special-calls-review.md`. This is not implementation authorization. Full M2 remains open. A concrete API and resource/error contract must precede worker dispatch.

## Proposed next executable increment

Introduce a shared, bounded reference-type relation and use it for checkcast and instanceof in the existing runtime-aware method-body engine. Cover ordinary admitted instances and the existing primitive arrays. Keep numeric/void method signatures and the host-supplied receiver. This makes useful reference operations executable without coupling the next step to constructors, initialization, exception objects or a new execution engine.

The same type model should then support non-null reference fields and reference arrays. Do not implement several inconsistent subtype predicates for those consumers. Existing protected-access subclass tests remain class-only: they must not acquire array or interface assignment rules merely because the new service has broader semantics.

## Type identity and boundaries to settle in the exact contract

Use existing registry-scoped ClassId for nominal class/interface identity. Represent an array type by bounded rank and a primitive or nominal leaf, preserving the leaf's defining identity. Descriptors and names are input to resolution, not runtime identity. Preserve exact UTF-16 bindings and explicit loader delegation. A same-named class from another loader must remain distinct.

Require explicit, checked core identities for Object, Cloneable and Serializable before treating an array as their subtype. Do not fabricate their definitions or identify arbitrary user-loader lookalikes by spelling. Primitive-array allocation currently needs no registry: the exact contract must retain that API while defining how a runtime-aware type context attaches its explicit bootstrap universe, rejects contradictory associations, and validates existing handles. It must also specify how target types absent from admitted receiver ancestry can be registered in that context without pretending they have instance layouts.

The first increment needs target array types even though it cannot yet allocate reference arrays: casting an int[] to Object[] must produce a real negative result. A structural array-type representation is not a claim that array Class objects, reflection, loading/initialization state or array method dispatch already exist.

The normative relation distinguishes nominal inheritance from array covariance, including exact primitive components and the special array supertypes. Array defining identity follows component identity. See [JVMS array creation](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.3.3) and [checkcast](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.checkcast). Implement rank reduction and graph walks iteratively, with explicit limits and no recursive descriptor-owned tree.

## Execution and validation

The exact contract must pin instruction-specific null handling, CP validation, target access/resolution and host-handle validation order. Do not copy method-invocation null ordering into casts. Checkcast retains the reference on success and has a symbolic ClassCastException outcome; instanceof produces an int. Neither operation initializes the target. Preserve the single engine, current-frame CP origin, managed-handle validation and existing shared execution limits.

Require synthetic tests for nominal/interface diamonds, loader-separated equal names, primitive arrays versus nominal and array targets, rank boundaries, null, inaccessible/missing targets, foreign handles, exact cumulative work limits and deep nonrecursive traversal. Genuine Java wrappers can cast this and locally allocated primitive arrays, returning numeric results; reference parameters/results are not prerequisites. Keep all earlier dispatch, field and array regressions.

## Subsequent reference storage increment

After the relation is accepted, add bounded reference-array storage and non-null instance-field stores using that relation. Preserve atomic failed stores, final-field restrictions, heap ownership, exact component identity and enumeration of child handles. Reference arrays must share explicit allocation quotas and publish only after complete reservation. Traceable edges do not themselves constitute collection.

For aastore, null array, bounds and incompatible component failures have ordered Java outcomes; null values are compatible with reference components. See [JVMS aastore](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.aastore). Reference-field compatibility failures in malformed unverified bodies require an explicit host-check policy, not an invented ArrayStoreException. Field descriptor resolution across defining loaders also needs a loading-constraint decision before the existing unsupported boundary is removed.

Only after these services are accepted should reference call signatures and returns widen. Constructors still require a separate uninitialized-reference protocol; class initialization needs lifecycle, static writes and failure state; Java exception dispatch needs exception identities and handler matching. None follows automatically from storing a non-null handle.

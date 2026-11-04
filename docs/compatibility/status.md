# Compatibility status

This is an evidence report, not Java certification. Parsing does not establish
bytecode verification, linking, initialization or execution. M1 now has all 30
standard attribute bodies decoded and was approved by the technical director
on 2026-11-03. The sustained exact-source fuzz campaign passed.

The long-term target is complete specification conformance, initially for Java SE
21, with the required observable behavior of conforming JVMs and HotSpot as the
primary reference. Performance superiority must be demonstrated through controlled,
representative comparisons. Kotlin/Spring Boot execution alone will not establish
completion of that goal.

## Implemented services and boundaries

| Area / JVMS SE 21 clauses | Status | Evidence and boundary |
| --- | --- | --- |
| Reader, header and versions (4.1) | Implemented and tested | Checked reads, exact consumption, majors 45-65, historical/preview policy and configured byte limits |
| Constant-pool encodings (4.4) | Implemented and tested | Every tag, wide holes, numeric bits, modified UTF-8, typed indices, version introductions, module/package names and method-handle constraints |
| Names, descriptors and members (4.2-4.6) | Implemented and tested | Descriptor grammar/dimensions/slots, duplicate identities, versioned access flags, class/interface/initializer context and Code presence |
| Standard attributes (4.7) | Implemented and tested for parser stage | All 30 bodies below; later-consumer semantics remain separate |
| Unknown attributes (4.7.1) | Implemented and tested | Payload preservation, including standard-looking names outside applicable version/location |
| Resource bounds | Implemented and tested | Default 16 MiB class input, annotation depth 64, one million annotation/value nodes shared across the whole class; depth hard-capped at 64 even if caller requests more |
| Inspection CLI | Implemented and tested | Escaped metadata, bounded reads, preview opt-in and nonzero argument/input failures; no bytecode execution |
| Genuine Java 21 fixtures | Implemented and tested | 151 classes including cross-class static calls, initialization and primitive/reference/multidimensional-array and instance/reference-field bodies, class/interface/special dispatch and cast hierarchies and raw-field oracle metadata; source/artifact digest manifest and compiler regeneration (M1 acceptance used 17); oracle data alone does not establish execution support |
| Parser fuzzing | Implemented and tested | 108,907,980 seeded cases over 300 seconds, zero failures, exact-source fingerprint; no coverage metrics or proof of panic freedom |
| Linux/Windows hosted CI | Passed for 8ef7f43 | [Linux/Windows run 41845390025](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41845390025) passed every step on both platforms with Rust/Cargo 1.99.1 and Temurin 21.0.13: 151 fixtures/19 reference sets, 882 workspace tests before and after regeneration, 795 optimized tests, provenance, strict lint and parser smoke |
| Rust 1.85 minimum version | Not tested | Manifest declaration; actual local toolchain is Rust 1.98.1 |
| Performance superiority over HotSpot | Not demonstrated | Long-term objective; no comparative performance benchmark results are claimed |
| Bytecode decoder | Implemented and tested | All defined JVM instruction formats, version/operand constraints and static branch destinations; 10 focused tests and malformed-input corpus |
| Numeric execution kernel | Partially implemented and tested | 148 opcode values including restricted invokestatic; 616 integral + 7,263 floating + 126 static-call JDK outcomes match in debug/release; bounded method-body evaluation only |
| Primitive-array execution bridge | Implemented and tested | Explicit heap API adds 31 opcode values (179 total), reference locals/branches and primitive arrays; all 280 JDK cases match in debug/release, including 67 symbolic exceptions; numeric/void method signatures only |
| Runtime definition registry | Implemented and tested | 25 debug/release tests; immutable unverified/unlinked definitions, scoped loader/class/member identities, checked admission limits, declared-member lookup and cached in-memory source reads |
| Hierarchy and logical field layouts | Implemented and tested | 21 added tests including genuine javac graphs and a 10,000-class chain; explicit resolver/delegation, kind/final/cycle checks, bounded parent-linked layouts and owner-qualified fields; full derivation/access/override checks remain deferred |
| Shared runtime values and default static storage | Implemented and tested | All primitive/object/array defaults, bounded owner-qualified static locations and read-only root enumeration; ConstantValue and initializers remain unapplied; no lifecycle advancement or static writer |
| Primitive-array heap service | Implemented and tested | All eight kinds, checked opaque identities, typed stores/loads, bounded fallible allocation, 151 JDK roundtrips; no reuse, reclamation or GC |
| Ordinary-instance memory service | Implemented and tested | Atomic multi-plan admission, shared own-field shapes, six quotas, default allocation, owner-qualified numeric/null stores, final protection and borrowed children; 151 raw-putfield JDK cases; no construction/initialization or field-bytecode claim |
| Runtime field resolution/access | Partially implemented and tested | Explicit unnamed-module context, authoritative loader/name bindings, interface-before-superclass lookup, runtime-package/private/protected checks and three cumulative quotas; named modules, cross-class nest access and cross-loader reference constraints remain unsupported |
| Instance-field execution bridge | Implemented and tested within body-kernel scope | Registry-backed MethodId and explicit receiver; 181 opcode values, shared engine and bounded static helpers, getfield/putfield with symbolic access/missing/static/null failures; 402 genuine + 151 raw-store JDK outcomes; no constructor or initialization claim |
| Ordinary virtual-call execution | Implemented and tested within body-kernel scope | Runtime-aware mode adds invokevirtual (182 opcode values), bounded class resolution and package-aware receiver selection, MethodId-keyed cross-class frames and current-frame static helpers; 494 genuine JDK comparisons pass in debug/release; constructor/signature-polymorphic invocation and lifecycle remain unsupported; reference signatures are covered below |
| Interface/default dispatch | Implemented and tested within body-kernel scope | Runtime-aware mode adds invokeinterface (183 opcode values) and completes class Methodref default fallback; shared maximal-interface analysis, class priority, private helpers, count validation, symbolic conflicts/abstract failures; all 1,337 genuine JDK comparisons pass in debug/release |
| Ordinary special-call execution | Implemented and tested within body-kernel scope | Runtime-aware mode adds nonconstructor invokespecial (184 opcode values), prescribed superclass/direct-interface selection, effective ACC_SUPER, checked owner/receiver/declarer identities and shared MethodId caches; all 450 JDK cases pass in debug/release; constructors remain unsupported |
| Reference types and cast instructions | Implemented and tested within body-kernel scope | Explicit heap Object/Cloneable/Serializable identities; loader-aware nominal/interface and structural-array assignment; checkcast/instanceof (186 runtime-aware opcode values), null target skipping and symbolic ClassCastException; all 1,276 genuine JDK comparisons pass in debug/release; reference-array allocation is covered by the later service below |
| Checked non-null instance-reference fields | Implemented and tested within body-kernel scope | Existing putfield stores compatible ordinary-instance/primitive-array values after declaring-loader descriptor resolution and bounded assignment; public raw writes remain numeric/null only; private atomic commit and per-location child edges; all 690 JDK cases pass in debug/release; checked static writes are covered below; loading-constraint enforcement remains unsupported; reference signatures are covered below |
| Reference-array storage and execution | Implemented and tested within body-kernel scope | Typed retained actual arrays, null defaults, context-only checked allocation/read/store and covariant ArrayStoreException; shared quotas with actual Reference element size, generic length and ordered child edges; anewarray/aaload/aastore bring runtime-aware coverage to 189; all 3,948 JDK cases pass in debug/release; multidimensional allocation is covered below; mirrors, lifecycle and GC remain unsupported |
| Bounded multidimensional allocation | Implemented and tested within body-kernel scope | Runtime-aware c5 brings coverage to 190; complete/partial actual types, all-count sign checks, level-ordered aggregate quotas, atomic work precharge, iterative staging and validated whole-graph publication; 7,388 JDK cases pass in debug/release; host allocation failures remain separate from Java OOM/GC semantics |
| Reference method arguments/results | Implemented and tested within body-kernel scope | Runtime-aware areturn brings coverage to 191; compact charged shapes, selected-declarer identity checks, null shortcuts, all call forms and exact handle transfer; conservative nominal loader/override boundary; all 904 new JDK outcomes pass in debug/release; full loading constraints and lifecycle remain unsupported |
| Supplied-Throwable dispatch | Implemented and tested in opt-in body-kernel profile | Explicit checked Throwable identity, lazy ordered catches, catch-all/finally/rethrow and iterative invocation-PC unwind; athrow gives 192 opt-in opcodes while existing runtime mode stays 191; all 336 JDK outcomes pass in debug/release; all eleven unmaterialized VM categories fail explicitly, existing APIs unchanged |
| Checked static state and body access (5.4.2, 6.5.b2/b3 memory behavior) | Implemented and tested in explicit memory profile | Unique heap-owned locations, defaults, atomic value-preserving extension, typed checked stores, final protection and alias-preserving roots; static-only/composed profiles support 193/194 opcodes; 364 JDK outcomes plus retained-state assertions pass debug/release; ConstantValue, clinit and Java active-use initialization remain unapplied |
| Single-session initialization (5.5, 4.7.2) | Implemented, tested and approved within the single-session profile | Actual clinit, numeric constants, ordered dependencies, once-only state and supplied Error propagation; 145 JDK outcomes and independent state checks pass locally in debug/release; combined gate passes 838 debug and 751 optimized tests. String constants, VM exception production, threads and startup remain unsupported |
| Registered static-call resolution (5.4.3.3/4, 6.5.b8) | Implemented, tested and approved within the registered-call profile | All context-backed profiles resolve cross-class b8 to actual MethodId with access/loading/category checks and unchanged standalone behavior. Session initializes only the declarer. 154 distinct JDK rows (217 profile comparisons before repeats) and full retained-state assertions pass debug/release; native/synchronized execution and full loading constraints remain unsupported |
| Full linkage, bytecode verifier and initialization | Not implemented | Subsequent increments; registration and layout planning are not full linkage |
| Class loading, GC, threads, Java libraries, JNI, Kotlin, Spring Boot, JIT | Not implemented | Subsequent milestones |

No executed test currently remains failing. This statement is limited to the
recorded tests; it is not proof that no defect exists.

Latest independent native Windows checks: 768 workspace tests and 681 optimized
runtime/interpreter tests pass (366 runtime + 315 interpreter), with strict Clippy
and formatting clean. All 26,025 execution comparisons match; separate array and
field services have 151 oracle roundtrips each. All 104 class fixtures and seventeen
reference sets reproduce; all 98 earlier class hashes remain unchanged. Forty-four
new Rust tests cover allocation/extension, identities, access/order, raw stores,
roots, work limits, corrupt retained mappings and persistent effects. Final review
is recorded in docs/architecture/m2-static-state-review.md; full M2 remains open.

Canonical source checks verify 39 Java sources and seventeen named reference
records; six Python regressions pass. Sources retain LF bytes and raw-byte hashes.
Expanded parser smoke completed 10,000 cases with zero failures: 121 accepted,
9,879 rejected, ten rechecks and corpus 225.

Implementation `e29b74f` passed complete
[Linux/Windows run 41828535972](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41828535972).
Inspected logs confirm Rust/Cargo 1.99.1 and Temurin 21.0.13, six Python tests,
39-source/seventeen-record preflight, 104 reproduced classes and all seventeen
reference sets. Both platforms passed 768 workspace tests against checked-in
fixtures and again after regeneration, 681 optimized tests, strict formatting/lint
and parser smoke: 121 accepted, 9,879 rejected, ten rechecks, corpus 225 and zero
failures over 10,000 cases. Earlier evidence retains its named scope.

## Single-session initialization evidence

The frozen combined gate passes formatting, strict workspace/all-target Clippy,
838 debug workspace tests and 751 optimized runtime/interpreter tests. All 26,170
execution outcomes match, including 145 new initialization rows. The new 70 tests
cover numeric constants, superclass/default ordering, recursive partial state,
actual initializer authority, retained active-use operations, exact Error causes,
all eleven VM-exception routes, host panic cleanup, quota boundaries and a
10,000-class iterative scheduler chain. The independent oracle checks complete
static state/status and retained identities, not only returned integers.

All 132 fixture classes and 18 reference sets reproduce with Zulu JDK 21.0.7;
40 source hashes and six provenance regressions pass. All 104 prior class hashes
are unchanged. Parser smoke reports 134 accepted, 9,866 rejected, ten rechecks,
corpus 256 and zero failures over 10,000 cases at seed 4851011501752864596.
All 48 integrated implementation/fixture/config hashes were rechecked after the
gate. Local checks used Windows and Rust/Cargo 1.98.1. Previous hosted records
above retain their original scope.

Implementation `6dbbef4ad399f9d0f5938b3bea2485e08e45eff4` passed complete
[Linux/Windows run 41839995587](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41839995587).
Both job logs were inspected: Rust/Cargo 1.99.1, Temurin JDK 21.0.13, six Python
regressions, 40-source/18-record preflight, 132 reproduced fixtures and all 18
reference sets. Each platform passed 838 checked-in and 838 regenerated workspace
tests plus 751 optimized tests (2,427 Rust test executions), strict formatting/lint,
and parser smoke: 134 accepted, 9,866 rejected, ten rechecks, corpus 256 and zero
failures over 10,000 cases at seed 4851011501752864596.

[Bounded technical approval](../architecture/m2-initialization-session-review.md)
is recorded. This remains a single-session subset. String constants, generated wrappers and
NCDFE, constructors, full loading constraints, concurrent initialization,
loading and Java startup are unsupported. Full M2 and conformance remain open.

## Registered static-call evidence

[Bounded technical approval](../architecture/m2-static-call-resolution-review.md)
is recorded; full M2 remains open.

The frozen combined gate passes formatting, strict workspace/all-target Clippy,
882 debug workspace tests and 795 optimized tests (410 runtime, 385 interpreter).
All 26,324 distinct execution-reference rows match. The new 154 rows consist of
133 initialization-session and 21 ordinary-body cases; the latter run in four
profiles, giving 217 new profile comparisons before successful repeat calls.
Forty-four new Rust tests cover checked resolution, access/category/loading order,
actual declaration identity, per-call work, interface static noninheritance,
prepared-Code aliases, reference/argument checks and initialization continuations.
Seven earlier registered-policy regressions were migrated narrowly; standalone
and heap-only behavior and instance-call resolver budgets remain unchanged.

All 151 class fixtures and 19 output sets reproduce; 41 source records and six
Python regressions pass. All 132 prior class hashes are unchanged. Parser smoke
at seed 4851011501752864596 reports 110 accepted, 9,890 rejected, ten rechecks,
corpus 256 and zero failures over 10,000 cases. All 37 integrated source/fixture/
configuration hashes were reverified after the gate. These are local Windows
results using Rust/Cargo 1.98.1 and Zulu 21.0.7.

Implementation `8ef7f43da98f8a7f7018c9d4e9504e9464752054` passed complete
[Linux/Windows run 41845390025](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41845390025).
Both job logs were inspected: Rust/Cargo 1.99.1, Temurin JDK 21.0.13, six Python
regressions, 41-source/19-record preflight, 151 reproduced fixtures and all 19
reference sets. Each platform passed 882 checked-in and 882 regenerated workspace
tests plus 795 optimized tests (2,559 Rust test executions), strict formatting/lint,
and parser smoke: 110 accepted, 9,890 rejected, ten rechecks, corpus 256 and zero
failures over 10,000 cases at seed 4851011501752864596.

## Standard attribute coverage

"core" refers to crates/classfile/src/tests.rs and its annotation test module;
"metadata" to crates/classfile/tests/metadata.rs; "reference" and "extended" to
the corresponding fixture integration test files. Each row has typed decoding;
truncation and exact-envelope tests supplement the focused assertions listed.

| Attribute | Positive and negative evidence |
| --- | --- |
| ConstantValue | metadata: 55 descriptor/constant-kind cases, invalid indices and lengths |
| Code | core: code length, parameter locals and exception ranges; reference: branches and catches |
| StackMapTable | core: every frame and verification-type form, reserved tags and bounds |
| Exceptions | metadata: order/empty table, reference kinds, counts and lengths |
| InnerClasses | core: historical anonymous-class rule, names and duplicate semantic entries; javac inner class |
| EnclosingMethod | metadata: optional method, NameAndType descriptor kinds, indices and lengths |
| Synthetic | core: empty payload, valid multiplicity and malformed envelope |
| Signature | metadata: typed Utf8 reference and exact body, preserving later-consumer grammar |
| SourceFile | metadata: typed Utf8 reference, invalid tag/index/length |
| SourceDebugExtension | metadata/core: modified UTF-8 including NUL/surrogates, invalid encodings, raw-byte preservation |
| LineNumberTable | metadata: duplicate starts, zero line numbers, range and length failures |
| LocalVariableTable | metadata: names/descriptors/ranges/category-two slots; core: duplicate variable across tables |
| LocalVariableTypeTable | metadata: names/signature preservation/ranges/slots; core: duplicate variable across tables |
| Deprecated | core: empty payload and valid multiplicity; real annotated module |
| RuntimeVisibleAnnotations | core: all value tags/references/descriptors/budgets; extended javac declarations |
| RuntimeInvisibleAnnotations | core: typed envelope and values; extended javac CLASS-retention declarations |
| RuntimeVisibleParameterAnnotations | core: count independence/bounds; extended javac method parameter |
| RuntimeInvisibleParameterAnnotations | core: count independence/bounds; extended javac method parameter |
| RuntimeVisibleTypeAnnotations | core: all 22 targets/all four path kinds, contexts and local bounds; extended javac Code/generic targets |
| RuntimeInvisibleTypeAnnotations | core: discriminants/context/bounds; extended javac field/generic/Code metadata |
| AnnotationDefault | core: permitted context and resource boundaries; extended javac all 13 element-value kinds |
| BootstrapMethods | core: dynamic constants and bootstrap indices/loadable arguments; reference: lambda and concatenation |
| MethodParameters | core: name validation and count preservation for reflection; javac parameter names |
| Module | module unit tests: all tables, names, duplicate aliases, reserved flags, java.base versions, optional references and truncation; reference fixture with all tables |
| ModulePackages | metadata/module: typed table, empty case, duplicates and inventory relationships |
| ModuleMainClass | metadata: typed Class index, wrong tags/indices/lengths |
| NestHost | metadata: typed index and host/member conflict; real inner class |
| NestMembers | metadata: indices/lengths/conflict; real outer class |
| Record | core: nested attributes and descriptors; reference: record component names/types |
| PermittedSubclasses | core: final-class conflict; reference: sealed interface and two subclasses |

## Deliberate later-stage checks

The parser preserves generic signature strings. Generic grammar, erasure,
type-parameter bounds and type-path interpretation belong to the metadata
consumer. MethodParameters/parameter-annotation association and annotation values
versus resolved annotation declarations belong to reflection. Module graph,
service assignability and class existence belong to loading/resolution.

Code/stack-map/type-annotation offset ranges are checked here. Instruction
boundaries, target opcode identity, control flow, stack/local type state and
Throwable relationships belong to the decoder/verifier. Nest access, inheritance
legality and permitted-subclass loading are also deferred. These boundaries are
reviewed in docs/architecture/m1-validation.md; they are not silently credited
as implemented runtime compatibility.

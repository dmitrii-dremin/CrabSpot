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

Rows describe specific services and execution profiles, including earlier explicit
APIs that retain their original restrictions. Later session and launcher profiles
compose additional capabilities. Read the current checkpoint and each linked
contract for the applicable public entry point; these rows are not a countable
specification-conformance score.

| Area / JVMS SE 21 clauses | Status | Evidence and boundary |
| --- | --- | --- |
| Reader, header and versions (4.1) | Implemented and tested | Checked reads, exact consumption, majors 45-65, historical/preview policy and configured byte limits |
| Constant-pool encodings (4.4) | Implemented and tested | Every tag, wide holes, numeric bits, modified UTF-8, typed indices, version introductions, module/package names and method-handle constraints |
| Names, descriptors and members (4.2-4.6) | Implemented and tested | Descriptor grammar/dimensions/slots, duplicate identities, versioned access flags, class/interface/initializer context and Code presence |
| Standard attributes (4.7) | Implemented and tested for parser stage | All 30 bodies below; later-consumer semantics remain separate |
| Unknown attributes (4.7.1) | Implemented and tested | Payload preservation, including standard-looking names outside applicable version/location |
| Resource bounds | Implemented and tested | Default 16 MiB class input, annotation depth 64, one million annotation/value nodes shared across the whole class; depth hard-capped at 64 even if caller requests more |
| Inspection CLI | Implemented and tested | Escaped metadata, bounded reads, preview opt-in and nonzero argument/input failures; no bytecode execution |
| Genuine Java 21 fixtures | Implemented and tested | 168 javac classes, with three new validation-driver definitions; nine construction and eighteen Code-validation original encodings are separately marked synthetic. Source/artifact digest manifests and compiler/encoding regeneration preserve all prior 165 class hashes (M1 acceptance used 17); oracle data alone does not establish execution support |
| Parser fuzzing | Implemented and tested | 108,907,980 seeded cases over 300 seconds, zero failures, exact-source fingerprint; no coverage metrics or proof of panic freedom |
| Linux/Windows hosted CI | Passed for a1f8bd9 | Both runners reproduced the synchronized entry, the 13 monitor CLI/API scenarios and four resource-boundary cases. Linux recorded 4,065 Rust test executions and verified 7,615 ordinary java.base classes. Windows recorded 4,059 and verified 7,589. [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42106349807/job/127522079444) and [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42106349807/job/127522078921) |
| Canonical class objects and Object.getClass (5.4.3.1, 6.5.ldc) | Implemented, tested and approved within the verified single-thread directory profile | Per-heap canonical references for loader-qualified nominal types and arrays. Genuine native selection, typed storage and shared class-monitor identity passed 40 public CLI/API scenarios and eight additional API quota checks. Both JVMs reproduced 78 observations on original and rebuilt inputs. Class instance state, primitive scalar mirrors, general reflection and native binding remain open. [Review](../architecture/m2-class-mirrors-review.md) and [reference results](../architecture/m2-class-mirror-reference.md) |
| Rust 1.85 minimum version | Not tested | Manifest declaration; actual local toolchain is Rust 1.98.1 |
| Performance superiority over HotSpot | Not demonstrated | Long-term objective; no comparative performance benchmark results are claimed |
| Hosted class-object regression suite | Passed for `5394da0` | The Linux and Windows jobs both passed for the published class-object implementation. [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42116584062/job/127557720607) and [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42116584062/job/127557720432) |
| String constants, interning and nonempty application arguments | Implemented, tested and approved in the single-thread directory profile | Fresh argument objects, per-heap canonical literals, two-loader interning, atomic static constant installation and native transport. Raw modified-UTF-8 coverage includes the 65,535-byte limit. Calls to String-declared Java instance methods on the opaque representation still stop with a typed error. Two Java 21 VMs exercised 148 observations. [Reference programs](../architecture/m2-string-reference.md) and [technical review](../architecture/m2-string-constants-review.md) |
| Hosted String reference suite | Passed for `3ab1bbc` | Both hosted Temurin builds reproduced the reference suite. [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42119677756/job/127568472735) and [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42119677756/job/127568441613) |
| Hosted String implementation | Passed for `18fd7a8` | Both runners passed the 27 archived and 14 rebuilt public API scenarios and the physical CLI tests, including invalid UTF-8 on Linux and an isolated UTF-16 unit on Windows. [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42125145176/job/127587291944) and [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42125145176/job/127587292616) |
| Java `String.length`, `String.isEmpty` and private nest access | Implemented, tested and approved for the matched compact image | The directory launcher executes the supplied Zulu Java methods and initializer. Reference runs on both VMs covered 96 length/nest observations plus 12 activation and comparator observations. OpenJ9's different initializer selects the existing opaque String implementation in Auto mode and fails discovery in strict mode. [Reference programs](../architecture/m2-string-java-body-reference.md) and [implementation scope](../architecture/m2-string-java-bodies.md) |
| Bytecode decoder | Implemented and tested | All defined JVM instruction formats, version/operand constraints and static branch destinations; 10 focused tests and malformed-input corpus |
| Distinct String equality and bootstrap assertions | Implemented, tested and approved within the matched compact image | The public launcher executes supplied String.equals, StringLatin1.equals, Class.desiredAssertionStatus and their real initializers. Archived and rebuilt guests returned 511/10 on controlled UTF-16 arguments. Both assertion policies and retained effects of failed Class initialization are tested. All 73 integration checks passed. Application and array mirror queries stop with an explicit unsupported-target error. OpenJ9's different recipe remains an unsupported-shape control. [Technical review](../architecture/m2-string-equality-review.md) and [reference behavior](../architecture/m2-string-equality-reference.md) |
| Throwable construction, capture and VM-produced ArithmeticException | Implemented, tested and approved for the matched single-thread image | Real Java constructors, bounded native capture and catchable failures from all four integer/long zero-divisor operations. The accepted gate includes 65 inherited successful checks and 20 successful continuation checks: 1,679 debug tests, 1,599 optimized tests, 121 Python regressions and public API/CLI replay on two supplied HotSpot libraries. The discovered Unicode replay-input defect was repaired without changing its oracle. Other VM producers and Java-visible stack-element population remain unsupported. The first hosted run failed on canonical fixture provenance; its packaging repair passed all eight targeted checks and received technical approval; replacement CI is pending. [Integration evidence and limits](../architecture/m2-throwable-integration.md) and [technical review](../architecture/m2-vm-exception-construction-review.md) |
| Hosted Throwable reference suite | Passed on Linux and Windows for `5a2924d` | Both Temurin runners reproduced the 20 Throwable and seven filtering cases over archived/rebuilt and normal/interpreted inputs, totaling 108 observations each. Both ran 18 provenance tests with one pinned-Zulu-compiler test skipped; that test passed locally with the exact compiler. Existing Rust suites recorded 4,722 executions on Linux and 4,716 on Windows. This is reference validation, not CrabSpot Throwable execution acceptance. [Linux](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42148910551/job/127667052031) and [Windows](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42148910551/job/127667052363) |
| Hosted String equality implementation | Passed on Linux and Windows for `5e1f673` | Both runners executed the archived/rebuilt equality and assertion guests through the public launcher. Linux recorded 4,722 Rust test executions and Windows 4,716. Both supplied String SHA-256 `831a11d5c76c84b8777a7813440b4570d49934dee05c285c9068a0ec43158412`. [Linux](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42145848368/job/127657006595) and [Windows](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42145848368/job/127657006291) |
| Hosted compact String implementation | Passed on Linux and Windows for `6fd81aa` | Both Temurin images supplied String SHA-256 `831a11d5c76c84b8777a7813440b4570d49934dee05c285c9068a0ec43158412`. The launchers executed the supplied Java bodies on both archived and rebuilt applications, including all nine nest cases. [Linux](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42134504808/job/127619102112) and [Windows](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42134504808/job/127619101474) |
| Hosted equality reference suite | Passed on Linux and Windows for `2a28eb5` | Each hosted Temurin build reproduced 22 public-only observations on archived and rebuilt Java classes. [Linux](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42135268707/job/127621685945) and [Windows](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42135268707/job/127621685486) |
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
| Session allocation and constructors (2.9.1, 4.10.1, 6.5.new/invokespecial/putfield) | Implemented, tested and approved within the session profile | New and real constructor Code, private unfinished aliases, precise chaining/final authority and exception boundaries. All 184 JDK comparisons and complete retained graph/state checks pass debug/release; the fresh combined gate passes 947 workspace and 860 optimized tests. Full verifier, library bootstrap, VM exception construction and GC remain unimplemented; full M2 remains open |
| Whole-method Code validation (4.9.1, selected 4.7.3/4.7.4 representation rules) | Implemented, tested and approved within the partial validation contract | Standalone partial validator checks 890 genuine Code bodies and 31 bodyless declarations across 168 classes. Fifty-one new Rust tests pass debug/release; the final combined gate passes 998 workspace and 911 optimized tests. Eighteen JDK stage-aware cases distinguish twelve rejected static defects, four positives and two deliberately deferred type errors. No flow/type proof, execution permission or implicit runtime gate; full verification remains unimplemented |
| Modern selected-method type verification (4.10.1) | Implemented and approved for the selected-method contract | Stack-map-guided independent Rust proof for majors 51вЂ“65 under explicit loader facts. All 935 genuine bodies and 31 bodyless methods pass locally; 57 stage probes cover constructor, handler, access, override and missing-fact behavior. Whole-class aggregation, legacy inference and execution gates remain open. |
| Modern class type verification (4.10.1) | Implemented and approved for a canonical definition | All declared methods under an immutable loader environment, cumulative quotas, first-failure attribution, historical and preview header checks. All 174 ordinary javac classes and all 7,519 ordinary classes in the tested Zulu 21.0.7 java.base archive passed locally. |
| Initiating-loader records and preparation constraints (5.3.4, selected 5.3.5, 5.4.2, 5.4.5/6) | Implemented and approved in the checked loading domain | Tracks actual initiations separately from required equalities, including both preparation producers and per-obligation rollback. Fresh-loader HotSpot and OpenJ9 runs recorded 130 stage outcomes each. Rust applies the specified rules in the documented disagreement cases. [Results](../architecture/m2-loading-constraints-review.md) |
| Field and ordinary method resolution during execution (5.3.4, 5.4.3.2вЂ“4, 5.4.4) | Implemented, tested and approved in the checked unnamed-module context | Original callers exercise CP-indexed successes and failures across loaders and heaps, descriptor agreement, per-receiver method selection and retained effects. The five reference families produced 119 observations per VM. [Behavior and differences](../architecture/m2-member-resolution-reference.md) |
| Full linkage, verification and initialization | Partial | Directory entry verifies and prepares every class admitted to its executable heap. Historical verification by inference, arbitrary loader behavior, named modules, full nestmate access and VM-created exceptions remain open. |
| Directory class loading and Java startup (5.2, selected 5.3вЂ“5.5) | Implemented, tested and approved for a single-use directory image | The launcher verified and ran a six-class Java application with genuine java.base inputs and an empty bootstrap String array. All 15 cases passed through the CLI and public API, including inherited main, initialization failure and eager rejection of an invalid unused class. [Run the example](../../README.md#run-a-java-application) and [technical review](../architecture/m2-verified-application-entry-review.md). |
| General class-path/JAR loading, GC, threads, JNI and JIT | Unimplemented | Kotlin and Spring Boot application execution and performance relative to HotSpot remain untested. |
| Synchronization in one logical thread (2.11.10; 6.5 monitorenter, monitorexit, calls, returns and athrow) | Implemented, tested and approved for a single logical thread | The physical CLI and public API ran 13 original application cases. Synchronized entry, non-LIFO release across objects, reentry, unfinished-object identity, precise failure PCs and retained effects are covered. The profile enforces both optional structured-locking rules. Threads, blocking, wait/notify and the Java Memory Model remain open. [Behavior and JVM comparisons](../architecture/m2-monitor-reference.md). |

The accepted checkpoint has no recorded outstanding local test failures. The
Throwable integration gate found a corrupted Unicode argument in a replay example;
the reviewed repair passed its explicit regression and the complete archived/rebuilt
String replay. All continuation checks completed successfully. The original failure
and exact inherited-versus-rerun evidence remain in the
[integration report](../architecture/m2-throwable-integration.md). Both first hosted jobs failed on the projection JSON line-ending hash. A canonical
Git export reproduces the failure; a narrow metadata repair and two regression tests
passed all eight targeted checks and received technical approval. Replacement hosted
CI remains pending. General Throwable/JVM support remains partial.

## Modern method verification evidence

The local Windows suite covers every modern effective opcode (198) and all
11 legal wide encodings. It checks each instruction's verification transfer,
including instructions that the runtime cannot execute. The 57 stage probes
distinguish verification failures from later execution failures. In one pair,
a method can return a cast value as Object while the cast's target definition
is missing. HotSpot returns null on the null path and reports NoClassDefFoundError
at invocation on the non-null path. CrabSpot verifies both methods using the
supplied facts. The [current milestone](../milestones/current.md) records the
native gate, counts, corpus and remaining whole-class obligations.

## Whole-method Code validation evidence

The [bounded review](../architecture/m2-code-validation-review.md) approves the increment.
The final fresh-target gate passes 998 workspace tests and 911 optimized tests
(51 verifier, 436 runtime, 424 interpreter), formatting and strict lint. The new
51 tests comprise nine private guards, forty independent public cases and two
genuine/original integration tests. They exercise complete Code rather than a
reachable-only instruction walk, including 890 genuine bodies, 31 bodyless methods,
25,236 instructions and 173 opcode values. Full type-state verification is deferred.

All 168 javac classes, nine prior original construction encodings, eighteen new
original Code-validation encodings and 21 reference sets reproduce. All 165 prior
genuine class hashes remain unchanged. Eight Python regressions and provenance
for 43 Java sources pass. The new JDK oracle records definition/lookup/invocation
stages: four positive cases invoke successfully, twelve static defects fail lookup,
and two deferred type-state errors fail JDK lookup while passing the partial Rust
validator. No Java error object or full-verification certificate is fabricated.

The native Zulu 21.0.7 parser smoke uses 195 inputs and seed 4851011501752864596:
10,000 cases, 138 accepted, 9,862 rejected, ten rechecks, corpus 256, zero failures.
Rust/Cargo is 1.98.1 on Windows x86-64. All 45 frozen source/configuration/fixture
hashes were reverified after the gate. Hosted evidence for this increment also passes.

Implementation `08386b361be4992e4c02ef0931aff46185c809e3` passed complete
[Linux/Windows run 41867714340](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41867714340).
Inspected Linux job 126706186206 and Windows job 126706186887 each passed every
step on Rust/Cargo 1.99.1 and Temurin 21.0.13: 998 checked-in and 998 regenerated
workspace tests, 424 optimized interpreter, 436 runtime and 51 verifier tests
(2,907 Rust executions per platform). Eight Python tests, 43-source/21-record
provenance, 168 javac and 27 original encodings, twenty execution-reference sets
and the eighteen-case stage-aware oracle all pass. Hosted parser smoke on each
platform matches 138 accepted/9,862 rejected over 10,000 cases at the same seed,
with 195 initial inputs, ten rechecks, corpus 256 and zero failures. Earlier hosted
records remain scoped to their named implementation commits.

## Construction validation

The frozen construction implementation passes the independent native Windows gate
in a new build directory: 947 workspace tests and 860 optimized tests (436 runtime,
424 interpreter), strict formatting/lint and all 26,508 distinct execution rows.
Sixty-five Rust tests are added, including 28 public/9 private interpreter tests,
21 public/5 private runtime tests and 2 independent oracle tests. The 184 new rows
cover 112 genuine-source cases and 72 original encodings. All 165 genuine classfiles,
nine original encodings and 20 reference sets reproduce. All 151 prior class hashes
remain unchanged; 42 source records and seven Python regressions pass.

Expanded parser smoke passed 10,000 cases at seed 4851011501752864596 with 174 input
classfiles: 119 accepted, 9,881 rejected, ten rechecks, corpus 256 and zero failures.
The local toolchain is Rust/Cargo 1.98.1 with Zulu 21.0.7 on Windows x86-64.
Construction hosted CI passes on both Linux and Windows; preceding evidence
remains scoped to its named commits. The technical record is [construction review](../architecture/m2-construction-review.md).

Review corrected an observable if_acmp operand-error precedence regression and
hardened constructor authority/one-pass alias accounting. An older debug cache
omitted five new unit tests because snapshot mtimes predated its binary; those
results were discarded. The final fresh-directory runtime unit binary runs all 73
tests, and all 55 frozen path hashes were verified again after the full gate.

Implementation `ea7dc014352d46253fc5123b11dfeec04774eb34` passed complete
[Linux/Windows run 41859897401](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41859897401).
The coordinator inspected Linux job 126678491674 and Windows job 126678492033.
Both used Rust/Cargo 1.99.1 and Temurin 21.0.13 and passed every workflow step:
947 checked-in tests, 947 regenerated workspace tests and 860 optimized tests
(424 interpreter + 436 runtime), totaling 2,754 Rust executions per platform.
Seven Python tests, 42-source/20-record provenance, 165 reproduced javac classes,
nine original encodings and all 20 reference sets also passed.

Hosted parser smoke used 174 input classes and seed 4851011501752864596:
10,000 cases, 122 accepted, 9,878 rejected, ten rechecks, corpus 256 and zero failures.
This is the regenerated Temurin corpus; the native Zulu corpus produced 119/9,881.
Neither result is substituted for the other. No full-JVM or performance acceptance
is implied by the bounded construction approval or these CI results.

## Historical static-memory evidence

At the earlier static-memory checkpoint, independent native Windows checks passed
768 workspace tests and 681 optimized runtime/interpreter tests
(366 runtime + 315 interpreter), with strict Clippy
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

At the single-session initialization checkpoint, String constants, generated exception
wrappers, constructors, full loading constraints, concurrent initialization and Java startup
were still open. Later increments added constructor execution and checked loader agreements.
The current directory-launch status and remaining limits appear in the table above.
[Single-session initialization review](../architecture/m2-initialization-session-review.md).

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

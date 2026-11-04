# CrabSpot architecture

Status: M1, the M2 decoder/numeric, bounded static-call, primitive-array, instance-field, virtual-call, interface/default-dispatch, ordinary nonconstructor special-call, reference-type/cast, checked non-null reference-field, reference-array, multidimensional-allocation, reference-argument/result, opt-in supplied-Throwable dispatch, checked static-memory, single-session initialization and registered cross-class static-call increments, and the runtime registry/hierarchy/shared-values/default-static-storage/primitive-array-heap/ordinary-instance-storage increments are approved in separate review records. See `m2-static-call-resolution-review.md` for the latest bounded approval: lawful registered static resolution and shared execution, including actual-declarer initialization through the existing session. Standalone ClassFile/heap-only calls keep their earlier scope; host-controlled body profiles do not initialize classes. String constants, constructed VM exceptions/wrappers, complete preparation and concurrent initialization remain open. Allocation and constructor reference protocol is proposed in `m2-construction-next.md` and requires an exact implementation contract. Full M2 and later milestones remain open.

`ARCHITECTURE.md` is the project charter. This document refines its initial layout without reducing its roadmap. CrabSpot is an independent Rust JVM project targeting Linux x86-64. The first deliverable reads class files; it does not execute Java programs or claim JVM compatibility. NanoSpec is not used in this project.

## Ultimate target and acceptance

The user expanded the ultimate objective on 2026-11-03: implement the complete JVM specification, match the required observable behavior of HotSpot and other conforming JVMs, and demonstrate superior performance against HotSpot. Java SE 21 is the first complete target; subsequent Java releases need explicit target decisions and evidence. Spring Boot/WebFlux remains M9 integration coverage, not the endpoint. Existing increment approvals, implementation scope and recorded test results are unchanged. The decision is recorded in [ADR 0008](../adr/0008-complete-conformance-and-performance-superiority.md).

Maintain a complete specification-obligation inventory, covering the selected JVMS and applicable Java Memory Model/native/runtime-library contracts. Feature summaries and opcode counts cannot replace that inventory. HotSpot is the primary behavioral oracle; independent conforming open JVMs supply cross-checks when available. Required outputs and failures must agree, while the governing specifications determine allowed nondeterminism and implementation choices. Investigate discrepancies rather than copying bugs or requiring identical unspecified traces. [JVMS implementation freedom](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-2.html#jvms-2.13) permits different internals while preserving required behavior.

The speed objective requires a predeclared representative benchmark suite and acceptance rule against a named, controlled HotSpot baseline. Cover computation, allocation/GC, concurrency and real applications, separating startup/warmup from steady-state throughput and latency and publishing resource costs. Record builds, hardware, software, options, equivalent work, repetitions, variability, raw results and every regression/tradeoff. A suite-level win must be demonstrated; isolated microbenchmark wins or unmeasured aspirations do not count. This is not a claim of superiority on every machine, workload or metric. Correctness cannot be traded for speed.

Complete conformance, required observable compatibility and demonstrated performance superiority are joint ultimate acceptance conditions. Full-target acceptance and official certification require their own evidence; current parser/kernel approvals establish only their documented scope.

## Initial workspace

Only create crates containing implemented behavior:

| Crate | Responsibility | Dependencies allowed initially |
| --- | --- | --- |
| `classfile` | Parse class-file bytes into an inspection representation; expose structured errors and resource limits | Rust standard library; infrastructure dependencies require justification |
| `launcher` | Read an explicitly selected file, invoke the parser, print metadata and errors | `classfile`, standard library |

The implemented M2 foundation additionally contains `bytecode` (independent bounded instruction decoding) and `interpreter` (a selected static method-body execution kernel using `classfile` and `bytecode`). Its scope and evidence are recorded in `m2-foundation-review.md`. The initial parser/launcher description below remains their contract; the launcher still does not start Java applications.

The parser owns `reader`, `error`, `model`, `constant_pool`, `descriptor`, `attribute`, and parser orchestration modules. These are logical boundaries and may be combined while small. A module does not need a separate crate. The CLI executable is `crabspot`; its initial action is `crabspot inspect <path>`. Successful inspection must never be described as verification or execution. The CLI returns a nonzero status on malformed, unsupported, resource-limited, or unreadable input and handles bad arguments without a panic.

The library accepts bytes and options and returns a parsed model or a structured error. It must not open files, spawn processes, resolve classes, initialize globals, or call a reference JVM. The model may own its data initially. Constant-pool indices remain symbolic and use checked lookups. Runtime structures will be separate types.

## Parser contract

The parser is safe Rust. A bounded reader tracks absolute offsets, uses checked arithmetic and slice access, and creates bounded subreaders for attribute payloads. Every recognized payload must consume exactly its declared length; the outer file must be consumed exactly. Allocations must be tied to actual available input and configured limits rather than unchecked declared lengths. Nested records and annotation values need an explicit depth bound. Resource exhaustion limits are implementation policy and must have a distinct error category from malformed format.

Errors include byte offset, a useful structural context, and a stable category. Categories distinguish truncated input, invalid magic/version/tag/encoding/reference/descriptor/structure, unsupported features, configured limits, and trailing data. Diagnostic wording may evolve; callers should not have to parse prose. Error paths must not use `unwrap`, `expect`, unchecked indexing, or integer arithmetic that can panic.

The parser representation preserves all constant-pool alternatives, wide-entry holes, float/double bits, and Java string code units. A Java string is not assumed to be a Rust UTF-8 string. Display conversion must not corrupt the semantic representation. Unknown attributes are length-delimited extension data and are preserved. A known standard attribute retained only as opaque bytes is explicitly tracked as partial support and cannot count as a completed M1 feature.

Descriptors receive a separate reusable parser and typed field/method representation. It enforces complete consumption and applicable dimension and parameter-slot limits. A receiver slot is included where the enclosing method context requires it. Descriptor parsing is not class resolution.

Version support is a declared parser policy: major versions 45 through 65; historical minor values are accepted for majors through 55. For 56 onward, only minor 0 or 65535 is structurally recognized. Older-release preview files are rejected as unsupported. Java 21 preview files require an explicit option and do not imply execution support. Constant kinds respect their introduction versions. Attribute recognition depends on both version and location: a standard-looking name outside its defined context or before introduction is preserved as opaque extension data. It does not trigger recognized-attribute payload or multiplicity checks. Separately specified constraints on recognized attributes still apply.

Within supported constructs, validation covers file-local format rules, typed constant-pool references, names and descriptors, context-sensitive attribute recognition and multiplicities, and local metadata ranges. Method flags require version-aware interpretation, with explicit constructor and class-initializer handling. Validation does not claim to establish control-flow correctness, instruction boundary validity, type safety, linkage, module resolution, or executable semantics. These belong to later bytecode, verifier, loader, and runtime stages. Deferred checks must be documented, not silently credited as implemented.

Metadata checks assigned to later consumers remain there. For example, the parser decodes MethodParameters entries without requiring their count to match the descriptor; future reflection services perform that matching. Generic signature strings are preserved for their later consumer. Neither policy makes opaque standard annotation or module bodies count as decoded support.

The normative format reference is [JVMS SE 21 chapter 4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html), particularly sections 4.1–4.8. The validation boundary follows the distinction from [code constraints](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.9) and [verification](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10). This is a project contract, not a claim that parser acceptance equals conformance.

## Growth boundaries

Crates are introduced when their first coherent implementation exists. The decoder and execution kernel now have that implementation; the remaining runtime services are future work:

- `bytecode`: bounded instruction decoding and instruction offsets, independent of execution. Code bytes remain available in the class-file model until this layer is introduced.
- `verifier`: instruction and type-state checks with explicit inputs, diagnostics, and checked output states. A parsed class is never implicitly a verified class.
- `classloader` and `runtime`: defining-loader-plus-name class identity, runtime constant pools, resolution, and explicit loaded/linked/initializing/initialized/erroneous lifecycle transitions. Initialization synchronization is a runtime concern.
- `interpreter`: JVM values with explicit category-one/category-two semantics, frame/local/operand-stack rules, and Java exception outcomes distinct from host/parser errors. It depends on runtime services, not a future JIT backend.
- `heap`, `gc`, and `threading`: managed reference handles, roots, thread state, and safepoint cooperation. Raw host addresses must not become stable application references. Root ownership and safepoint obligations must be part of runtime API design before allocation or threading lands.
- `native`: separate VM-native service contracts from the JNI ABI and external library loader. Unsupported native methods fail explicitly.
- `jit`: future execution backend using runtime reference/root/safepoint contracts; backend choice needs an ADR.

The dependency direction is launcher -> services -> execution/runtime -> parsed metadata and decoder. `classfile` does not depend back on runtime, interpreter, GC, or launcher. No general-purpose dependency may outsource JVM implementation, and no implementation may delegate application execution to another JVM.

## Environment and reproducibility

The initial host is Windows x86-64, with Rust stable 1.98.1 through the user's Cargo installation and Zulu OpenJDK 21.0.7. The intended deployment target remains Linux x86-64. Native Windows checks cannot be reported as Linux validation. WSL discovery was denied by the host environment. A hosted run for `2617637` subsequently passed checked-in tests and JDK 21.0.13 fixture/reference regeneration on Linux and Windows, but failed strict Clippy on Rust 1.99.1's fetch_update deprecation. After the reviewed CAS correction, [run 41718502938](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41718502938) for implementation commit `a27593f` passed the complete workflow on Linux and Windows, including strict Clippy, optimized tests, regenerated fixtures/references and parser smoke. The coordinator inspected and recorded that hosted evidence. Rust 1.85 execution remains unobserved.

The subsequent field-execution commit `d6ef169` also passed the complete Linux/Windows workflow in [run 41726268353](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41726268353), including all seven reference sets, debug/optimized tests, strict lint and parser smoke. The virtual-call implementation `d6558d8` subsequently passed complete Linux/Windows [run 41730095972](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41730095972) with Rust 1.99.1 and JDK 21.0.13, including all eight reference sets, debug/optimized tests, strict lint and parser smoke.

The interface/default-dispatch implementation `3574484` passed complete Linux/Windows [run 41792109658](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41792109658) with Rust 1.99.1 and JDK 21.0.13, including all nine reference sets, debug/optimized tests, strict lint and parser smoke.

The ordinary-special-call implementation `8d2c523` passed complete Linux/Windows [run 41795065210](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41795065210) with Rust 1.99.1 and JDK 21.0.13, including all ten reference sets, debug/optimized tests, strict lint and parser smoke.

Implementation `6dbbef4ad399f9d0f5938b3bea2485e08e45eff4` passed complete
[Linux/Windows run 41839995587](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41839995587).
Both job logs were inspected: Rust/Cargo 1.99.1, Temurin JDK 21.0.13, six Python
regressions, 40-source/18-record preflight, 132 reproduced fixtures and all 18
reference sets. Each platform passed 838 checked-in and 838 regenerated workspace
tests plus 751 optimized tests (2,427 Rust test executions), strict formatting/lint,
and parser smoke: 134 accepted, 9,866 rejected, ten rechecks, corpus 256 and zero
failures over 10,000 cases at seed 4851011501752864596.

Reference JDK tooling is development-only: compile fixtures, inspect class metadata, and later produce differential reference results. Fixture generation records the JDK version, source inputs, compiler options, and generated artifact provenance. Ordinary Rust tests must be reproducible without quietly executing a Java application or downloading a JDK. Tests using checked-in binary fixtures should validate the expected major version; regeneration uses an explicit Java 21 toolchain.

## Acceptance gates

### M0

M0 requires a buildable workspace, this reviewed architecture, initial ADRs, CI, reproducible formatter/lint/test commands, JDK fixture tooling, compatibility status, and an evidence-based milestone report. Execute `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`. The CI must perform these checks on the Linux target environment. Report CI configuration separately from a remotely observed passing CI run.

### Parser foundation

The first coherent increment includes reader and structured errors, headers and versions, the complete constant-pool encoding surface, member envelopes, descriptors, bounded attributes, CLI inspection, genuine Java 21 examples, and malformed-input tests. It may expose documented partial attribute support. It is useful executable progress, not full M1 acceptance.

### M1

M1 requires all of the following before technical-director approval:

1. All standard Java SE 21 class-file structures have implemented decoding and file-local validation. This includes Code/exception tables, all stack-map frame and verification-type forms, BootstrapMethods and dynamic-constant relationships, annotations and type annotations, module metadata, records and their nested attributes, nests, permitted subclasses, and debug/legacy standard attributes. A compatibility matrix maps each structure and validation rule to tests or an explicit unresolved gap.
2. Positive fixtures are independently compiled with JDK 21 and exercise ordinary classes/interfaces, branches and exceptions, lambdas/string concatenation, annotations/type annotations, records, sealed types, nests, and modules. Synthetic fixtures cover valid encodings that javac does not naturally emit, including dynamic constants and stack-map alternatives.
3. Negative and boundary tests cover truncations, lengths, counts, invalid tags, unusable constant slots, reference kinds, names/descriptors, version boundaries, malformed modified UTF-8, attribute context/multiplicity, nested structures, and configured resource limits. Each discovered bug receives a regression test.
4. Executed property/fuzz testing exercises the public parser on arbitrary and mutated structured bytes with explicit resource limits. Record command, seed/corpus, duration or iterations, result, and regression artifacts. A fuzz target that has never run is not passing fuzz evidence. No finite campaign proves the absence of all panics.
5. Formatter, strict Clippy, and workspace tests pass. Reports contain actual counts, commands, known gaps and platform coverage. The technical director reviews the implementation against this contract and records approval or a precise rejection before M2 begins.

Parser fuzzing and inspection must never execute the input's bytecode. Full verifier semantics, runtime class loading, initialization, and Java compatibility remain outside M1 and must not be inferred from a successful inspection.

# PROJECT CRABSPOT
## Build a Production-Aspiring Java Virtual Machine in Rust — From Scratch

**Project codename:** CrabSpot  
**Technical director:** GPT-6 Astra  
**Primary implementation agent:** GPT-6.1 Sol  
**Language:** Rust  
**Reference implementation:** OpenJDK HotSpot  
**Specification:** Java Virtual Machine Specification (JVMS), Java SE 21  
**Target platform:** Linux x86-64 initially  
**Ultimate objective:** Run real-world Java and Kotlin applications, including Spring Boot, on an independently implemented Rust JVM.

---

# 1. Mission

Build a new Java Virtual Machine implementation from scratch in Rust.

This is not a toy interpreter, a wrapper around an existing JVM, a transpiler, or a research-only proof of concept.

The long-term objective is to create a JVM capable of executing real Java and Kotlin applications with correct semantics, reasonable performance, and an architecture suitable for implementing advanced runtime features.

The JVM must eventually support:

- Java SE 21 bytecode and class-file semantics.
- Java class loading, linking, verification, and initialization.
- Java threads, synchronization, and the Java Memory Model.
- Exception handling and stack unwinding.
- Reflection, method handles, and invokedynamic.
- JNI and native library interoperability.
- Garbage collection.
- JIT compilation and adaptive optimization.
- Java standard library integration.
- Kotlin/JVM applications.
- Spring Boot applications, including WebFlux, Reactor, and Netty.

**This is a long-running engineering project. Do not claim production readiness until demonstrated by evidence.**

# 2. Non-Negotiable Constraints

1. The JVM runtime must be implemented in Rust.
2. Do not fork, embed, or delegate execution to an existing JVM.
3. Existing JVM implementations may be studied as references, but their code must not be copied without explicit licensing review.
4. Existing Rust libraries may be used for general-purpose infrastructure, but not to outsource the core JVM implementation.
5. Do not use an installed Java runtime to execute application bytecode. A reference JDK may be used for compiling test fixtures, generating reference outputs, and differential testing.
6. Avoid unnecessary unsafe Rust. Every unsafe block must have a documented safety invariant.
7. All externally observable behavior must follow the relevant specifications.
8. Every implemented feature must have automated tests.
9. No placeholder implementations may silently return successful results.
10. Never weaken tests to accommodate incorrect runtime behavior.
11. Never fabricate benchmark results, compatibility claims, or test outcomes.
12. Prioritize semantic correctness before performance optimization.
13. The architecture must support incremental development without requiring repeated wholesale rewrites.

# 3. Agent Organization

## GPT-6 Astra — Technical Director

Astra owns:

- Overall architecture and technical direction.
- Specification interpretation.
- Component boundaries and interfaces.
- Milestone planning.
- Implementation task decomposition.
- Review of architectural decisions.
- Compatibility strategy.
- Acceptance criteria.
- Risk management.
- Final milestone approval.

Astra must challenge questionable assumptions and reject implementations that violate the JVM specification.

Astra should delegate bounded implementation tasks to Sol 6.1 whenever the environment supports delegation.

## GPT-6.1 Sol — Implementation Engineer

Sol owns:

- Implementing assigned components.
- Writing unit, integration, and regression tests.
- Running the Rust toolchain.
- Debugging compilation and runtime failures.
- Maintaining code quality.
- Producing implementation evidence.
- Reporting blockers and specification ambiguities.

Sol must not silently change architectural contracts established by Astra.

If direct model-to-model delegation is unavailable, maintain the same responsibilities through explicit task documents and review checkpoints. Do not pretend that delegation or review occurred.

# 4. Architecture

Start with a modular Rust workspace.

Proposed initial structure:

```text
crabspot/
├── Cargo.toml
├── crates/
│   ├── classfile/
│   ├── bytecode/
│   ├── verifier/
│   ├── runtime/
│   ├── interpreter/
│   ├── classloader/
│   ├── heap/
│   ├── native/
│   ├── threading/
│   ├── gc/
│   ├── jit/
│   └── launcher/
├── tests/
│   ├── fixtures/
│   ├── conformance/
│   ├── differential/
│   └── integration/
├── benchmarks/
├── docs/
│   ├── architecture/
│   ├── adr/
│   ├── compatibility/
│   └── milestones/
└── .github/
    └── workflows/
```

This is a proposed architecture, not an instruction to create empty crates.

Astra must review and refine the structure before implementation.

Important architectural requirements:

- Separate class-file parsing from runtime class representation.
- Separate bytecode decoding from execution.
- Model JVM values and operand-stack categories explicitly.
- Represent references through an abstraction compatible with moving garbage collectors.
- Avoid exposing raw object pointers as stable application references.
- Design class identity around the defining class loader and binary class name.
- Model class lifecycle states explicitly.
- Keep interpreter execution independent of future JIT implementation details.
- Define exception propagation as part of execution semantics.
- Account for safepoints, GC roots, and thread coordination from the beginning.
- Keep Java native-method integration separate from JNI ABI implementation.

# 5. Development Roadmap

## M0 — Architecture and Engineering Foundation

Deliver:

- Rust workspace.
- Architecture document.
- Initial ADRs.
- CI pipeline.
- Formatting, linting, and testing configuration.
- Reference JDK test-fixture tooling.
- Compatibility tracking system.
- Milestone acceptance framework.

Acceptance:

- `cargo fmt --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- CI executes the same checks.

## M1 — Class File Parser

Implement the Java SE 21 class-file format, including:

- Magic number and version validation.
- Constant pool parsing.
- Fields, methods, and descriptors.
- Attributes and nested attribute structures.
- Code attributes.
- Exception tables.
- BootstrapMethods.
- StackMapTable.
- Relevant Java 21 class-file constructs.

Acceptance:

- Parse real class files compiled by OpenJDK 21.
- Correctly reject malformed inputs.
- Never panic on malformed external class-file input.
- Pass parser fuzz tests.
- Provide structured, actionable parsing errors.

## M2 — Bytecode Interpreter

Implement a specification-driven bytecode interpreter.

Include:

- Local variables.
- Operand stacks.
- JVM primitive types.
- Arithmetic and conversions.
- Branching.
- Method invocation.
- Field access.
- Object allocation.
- Arrays.
- Exceptions.
- Synchronization instructions.

Acceptance:

- Execute independently compiled Java fixtures.
- Compare observable results against OpenJDK.
- Track implemented opcode coverage.
- Reject unsupported operations explicitly.

## M3 — Runtime and Class Loading

Implement:

- Runtime constant pool.
- Class loading and resolution.
- Class initialization.
- Runtime method and field representations.
- Object layout.
- Type checking and assignability.
- Virtual method dispatch.
- Interface invocation.
- Exception handling.

Acceptance:

- Run nontrivial object-oriented Java programs.
- Pass class initialization and inheritance tests.
- Correctly implement class-loader identity.

## M4 — Verification and Runtime Correctness

Implement bytecode verification consistent with the target JVMS.

Include:

- Type-state validation.
- Control-flow validation.
- Stack map frame handling.
- Access checks.
- Structural constraints.
- Correct linkage and verification errors.

Acceptance:

- Valid class files execute correctly.
- Invalid class files are rejected at the appropriate stages.
- Differential tests cover verification and linkage behavior.

## M5 — Java Standard Library Integration

Support execution against a compatible Java 21 standard library distribution.

Implement the VM-specific native functionality needed by the library.

Do not assume that successfully loading standard-library class files means the library is operational.

Acceptance:

- String manipulation.
- Collections.
- Exceptions.
- Reflection.
- Basic I/O.
- File operations.
- Class loading.
- Selected concurrency utilities.

All acceptance cases must be executable, automated, and compared against OpenJDK.

## M6 — Threads, Synchronization, and Garbage Collection

Implement:

- Java threads.
- Monitor ownership and reentrancy.
- `wait`, `notify`, and `notifyAll`.
- Volatile semantics.
- Java Memory Model requirements.
- GC root enumeration.
- Safe object reclamation.
- Stop-the-world coordination.

Begin with a simple, correct collector.

Acceptance:

- Multithreaded tests.
- GC stress tests.
- Reachability and object-lifetime tests.
- Synchronization correctness tests.
- No known memory unsafety in supported configurations.

## M7 — Advanced JVM Features

Implement:

- Method handles.
- invokedynamic.
- ConstantDynamic.
- Lambda metafactory compatibility.
- Reflection.
- JNI.
- Native library loading.

Acceptance:

- Java lambda expressions.
- Method references.
- Dynamic string concatenation.
- Reflection-heavy applications.
- Representative JNI tests.

## M8 — Kotlin Compatibility

Target Kotlin/JVM applications.

Acceptance:

- Kotlin standard library usage.
- Data classes.
- Collections.
- Lambdas.
- Reflection.
- Suspend functions.
- Coroutine scheduling and cancellation.
- Representative coroutine concurrency tests.

## M9 — Spring Boot Compatibility

Target a minimal Spring Boot application, followed by progressively more complex applications.

Acceptance sequence:

1. Spring Framework dependency injection.
2. Spring Boot application initialization.
3. Configuration properties.
4. Annotation scanning and reflection.
5. Minimal HTTP application.
6. Reactor-based asynchronous execution.
7. Netty integration.
8. Spring Boot WebFlux application serving HTTP requests.

Compare behavior against OpenJDK 21.

## M10 — JIT Compilation and Performance

Implement a JIT compilation pipeline.

Evaluate Cranelift or LLVM as potential backends, but do not select one without an ADR.

Requirements:

- Correct interpreter-to-compiled transitions.
- Runtime metadata.
- GC integration.
- Safepoints.
- Deoptimization strategy.
- Profiling infrastructure.
- Reproducible benchmarks.

Benchmark against OpenJDK using comparable configurations.

Report performance regressions rather than concealing them.

# 6. Testing Philosophy

Use multiple complementary test layers.

**Unit tests:** Individual data structures, parsers, instructions, and runtime operations.

**Integration tests:** JVM components operating together.

**Differential tests:** Execute identical Java programs under CrabSpot and OpenJDK and compare observable behavior.

**Property-based tests:** Validate invariants and generated inputs.

**Fuzz tests:** Especially for class-file parsing, bytecode decoding, and verification.

**Compatibility tests:** Target JVM specification requirements and document the applicable specification clauses.

Do not compare implementation-specific output such as object identity hash codes or nondeterministic thread scheduling without appropriate normalization.

Do not claim Java compatibility certification merely because internal tests pass. Official compatibility certification is a separate process.

# 7. Autonomous Execution Protocol

Operate in iterative engineering cycles.

For every task:

1. Identify the relevant specification requirements.
2. Define observable acceptance criteria.
3. Implement the smallest coherent feature set.
4. Add positive, negative, and boundary tests.
5. Execute the tests.
6. Review failures.
7. Fix defects.
8. Run regression tests.
9. Document limitations.
10. Commit the completed work when authorized.

Do not spend entire iterations writing plans without producing executable code.

Do not generate enormous speculative implementations without incremental compilation and testing.

When a task is blocked, record the blocker, investigate alternatives, and proceed with independent work where possible.

Astra must review architectural changes and milestone completion before the project advances.

# 8. Documentation and Progress Tracking

Maintain:

- `docs/architecture/overview.md`
- `docs/compatibility/status.md`
- `docs/milestones/current.md`
- `docs/adr/`
- `AGENTS.md`

The compatibility report must distinguish:

- Implemented and tested.
- Partially implemented.
- Implemented but failing.
- Not implemented.
- Not tested.

Every milestone report must include:

- Completed features.
- Tests executed.
- Passing and failing counts.
- Known defects.
- Architectural decisions.
- Next tasks.
- Reproducible commands.

Never report a test as passing unless it was actually executed successfully.

# 9. Engineering Quality

Prefer:

- Clear ownership and lifetime boundaries.
- Explicit invariants.
- Strong type modeling.
- Deterministic tests.
- Structured error types.
- Small, reviewable commits.
- Well-defined module interfaces.

Avoid:

- Global mutable state without justification.
- Unchecked indexing on untrusted input.
- `unwrap()` and `expect()` on external input paths.
- Unnecessary cloning.
- Hidden JVM semantic shortcuts.
- Hardcoded behavior for specific test fixtures.
- Fake native-method implementations.
- Ignoring verifier errors.
- Treating parser acceptance as runtime compatibility.

Performance optimizations must preserve observable JVM semantics.

# 10. Immediate Execution Order

Start work immediately.

**Step A — Architecture**

Astra must:

1. Inspect the available development environment.
2. Establish the target Rust toolchain and OpenJDK 21 reference environment.
3. Define the initial module boundaries.
4. Identify the highest-risk architectural decisions.
5. Create the architecture document and initial ADRs.
6. Define M0 and M1 acceptance criteria.

**Step B — Implementation**

Sol 6.1 must:

1. Initialize the Rust workspace.
2. Implement the foundational class-file parser.
3. Add tests using genuine Java 21 class files.
4. Add malformed-input tests.
5. Implement the initial CLI for inspecting class files.
6. Execute all available checks.

**Step C — Review**

Astra must:

1. Review the implementation against the specification.
2. Identify missing test cases.
3. Request fixes for critical issues.
4. Approve or reject M1 based on evidence.

**Step D — Continuation**

Proceed toward M2 only after the required M1 acceptance criteria are satisfied.

Do not stop after producing a plan.

Continue implementing, testing, reviewing, and committing coherent increments within the available execution budget.

If execution must stop, leave the repository in a buildable state and write a precise continuation checkpoint.

# 11. Definition of Success

The ultimate success condition is:

**A real Kotlin Spring Boot WebFlux application executes on CrabSpot, a JVM independently implemented in Rust, without delegating bytecode execution to HotSpot or another JVM.**

Intermediate success must be demonstrated through reproducible tests, not subjective assessments.

The project is allowed to take substantial time, compute, and iteration.

Correctness and architectural integrity take precedence over impressive-looking progress metrics.

---

# Final Instruction

You are not being asked to explain how to build a JVM.

**You are being asked to build one.**

Astra: lead the engineering effort, make architectural decisions, review implementation, and enforce acceptance criteria.

Sol 6.1: write the code, run the tests, fix the failures, and deliver working increments.

Start with M0 and M1.

The crab must eventually run Spring Boot.

🦀

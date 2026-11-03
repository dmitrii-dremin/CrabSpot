# CrabSpot contributor instructions

## Authority and workflow

- Read `ARCHITECTURE.md` and `docs/architecture/overview.md` before changing architectural contracts. The charter and user instructions define the task. Do not use NanoSpec in this project: no NanoSpec commands, files, or process gates.
- The technical director (GPT-6 Astra) owns specification interpretation, component contracts, architectural review, and milestone approval. Delegate bounded implementation work to GPT-6.1 Sol when available, explicitly selecting model and effort. If unavailable, record the limitation rather than pretending a review or delegation occurred.
- Implement coherent increments, compile and test them, document limitations, then request the required technical review. Do not advance to M2 before M1 is approved. Technical execution approval does not replace a required architectural review or human acceptance gate.
- Carry out authorized work without repeated permission requests. Commit only when authorized. Never claim unexecuted tests, remote CI runs, compatibility, benchmarks, or milestone acceptance.
- The user explicitly authorized autonomous continuation on 2026-11-03: proceed through implementation and technical review without asking at each increment; ask only when progress requires user involvement. Preserve milestone review gates and the prohibition on NanoSpec.

## Implementation rules

- Implement the JVM in Rust independently. Do not embed an existing JVM, delegate application execution to it, or outsource core JVM components to a dependency. Reference JDK tools are only for fixtures and explicitly identified reference/differential testing.
- Keep `classfile` independent of filesystem, process execution and runtime state. Keep decoder, verifier, runtime metadata and parsed metadata distinct. Add a crate only when it has real behavior.
- External-input paths use checked reads/references/arithmetic and structured errors. Avoid `unwrap`, `expect`, unsafe code, unbounded recursion and allocations based only on untrusted lengths. Document the invariant for every necessary unsafe block.
- Unknown attributes are extension data; known unimplemented standard attributes are partial support. Unsupported behavior must not silently succeed. Parsing is not verification, linking or execution.
- Every implemented feature needs automated positive, negative and boundary coverage. Fix behavior rather than weakening tests. Preserve regression cases. Use genuine Java 21 fixtures plus synthetic cases for encodings javac does not emit.
- Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`. Run relevant fixture/fuzz checks and record exact commands and outcomes. Document platform-specific limits honestly.
- Update `docs/compatibility/status.md` and `docs/milestones/current.md` with tested, partial, failing, unimplemented and untested states. Milestone reports include counts, defects, decisions, reproducible commands and next work.

## Workspace and Windows safety

- Preserve the interactive user's ownership of repositories and Git metadata. Before Git mutations, verify effective identity and relevant ownership, reusing that verification only while context is unchanged. Use the host's approved interactive-user path if the sandbox identity would create incorrectly owned Git files.
- Prefer `GIT_OPTIONAL_LOCKS=0` for read-only Git inspection. If needed, a verified repository may use a command-scoped `safe.directory` for its exact absolute path; parent repositories and submodules are separate targets. Do not disable the sandbox, rewrite ACLs, take ownership recursively, or add broad global exceptions.
- Unexpected access denial, broken workspace mapping or a hanging tool is an infrastructure incident. Pause affected work, inspect the failure and processes, and involve the user when required. Do not repeat elevated writes per file or cycle workers through a broken environment.
- Do not bypass an explicit auto-review denial through another transport, command or worker. Use a materially safer alternative or explain the denied action and reason.
- Concurrent tasks own separate mutable worktrees, output directories, ports, browser contexts and fixture data where interference is possible. Never stop another task's or the user's process to obtain a resource.
- Prefer project-scoped rules and explicit destinations. Ordinary edits stay in the sandbox; outside writes use the normal approval path. A prefix allowance does not authorize unrelated destinations or operations.

# Parser foundation review

Date: 2026-11-03.

Historical checkpoint: the later `m1-review.md` supersedes this document's full-M1 disposition after the remaining work and evidence were completed.

Reviewer: technical-director agent, separate from the implementation worker. Review was read-only for source code; architectural documents were updated to record resolved contracts.

## Verdict

**Approved as a parser foundation increment. Full M1 is not approved. M2 is not authorized by this review.**

The implementation has a coherent safe-Rust parser boundary and an inspection-only CLI. The foundation can be delivered with its explicit limitations. M0's local engineering checks are complete; the Linux/Windows CI configuration exists, but a remote CI run and Linux execution have not been observed. Windows results must not be represented as Linux results.

## Review corrections

The implementation worker addressed the review findings and added regression coverage:

- Constructor and abstract-method flag checks now account for historical strict floating-point flags. Constructor bridge flags are rejected. Class initializers receive their separate flag, descriptor, Code-presence, and uniqueness rules, including historical descriptors.
- Class/interface context now reaches member validation. Interface field and method flag rules and illegal interface constructors are covered.
- Attribute recognition now depends on version and location. Standard-looking names outside that context are retained as opaque extension data. This also bounds recursive decoding of nested attributes.
- The version-dependent anonymous InnerClasses rule is tested on either side of its boundary. Local metadata names and selected relationships, final/sealed metadata, and the module-info attribute allowlist have additional checks.
- MethodParameters entries are retained without enforcing descriptor-count matching; that association belongs to future reflection services. The architecture records this stage boundary.

These decisions were checked against [JVMS SE 21 methods](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.6), [attributes](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.7), and [class initialization methods](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-2.html#jvms-2.9.2).

The reviewed code uses bounded input reads, exact payload consumption, lossless Java string code units, typed constant-pool alternatives, symbolic references, and structured errors. The CLI limits file reads, escapes metadata for display, exposes the number of opaque recognized standard attributes, and states that bytecode was neither verified nor executed. No runtime execution delegation or unsafe blocks were observed in the implementation reviewed.

## Executed evidence

The coordinator independently executed the final commands below after the implementation worker's successful checks. The technical director inspected source and regression tests and reviewed those reported results; it did not perform another redundant test run.

| Command | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 37 passed, 0 failed: 22 library unit tests, 8 fixture integration tests, 7 CLI integration tests |
| `python tools/compile_fixtures.py --check` | 8 checked-in class files reproduced identically using OpenJDK 21 |

Validation ran natively on Windows using the coordinator's separate `target/coordinator` output directory. Rust was 1.98.1 and the reference JDK was Zulu OpenJDK 21.0.7. The regression suite includes all stack-map frame forms and verification-type encodings, genuine Java 21 fixtures, truncation/mutation cases, and deterministic arbitrary-input panic checks. These bounded smoke tests do not constitute proof of panic freedom or a completed sustained fuzz campaign.

## Full M1 remains open

The following work is required before a new full-M1 acceptance review:

1. Decode and locally validate Module and the standard annotation families, including annotation defaults, parameter annotations, type annotations, and nested values. They are currently explicitly opaque recognized attributes. Add configurable nesting limits before recursive annotation parsing lands.
2. Validate module/package name syntax and finish the systematic audit of file-local metadata/reference rules. Preserve the distinction between format checks and checks assigned to resolution, verification, or reflection. Reviewed foundation tests do not establish complete semantic coverage of every decoded attribute.
3. Extend positive, negative, boundary, and synthetic coverage for the completed structures, including forms not emitted by ordinary javac fixtures. Record each remaining validation gap in the compatibility matrix.
4. Execute and record a sustained fuzz campaign with corpus/seeds, resource bounds, duration or iteration count, and regression artifacts. Keep the existing deterministic tests as repeatable regression coverage.
5. Rerun the required checks and obtain technical-director approval against the full checklist in `overview.md`. Record Linux/remote-CI evidence when available without backdating or inferring successful runs.

This approval applies to the reviewed foundation only. It makes no claim of Java execution, complete class-file conformance, bytecode verification, standard-library compatibility, or production readiness.

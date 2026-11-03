# M1 technical-director acceptance

Date: 2026-11-03.

**M1 is approved. Implementation of the M2 decoder and numeric execution-kernel increment is authorized under ADR 0006 and `m2-foundation.md`.**

This disposition supersedes the full-M1 rejection recorded in the earlier foundation review. That review remains an accurate historical report of its smaller increment. No further user confirmation is needed for the already-authorized continuation.

## Scope accepted

The reviewed Rust classfile library decodes the complete Java SE 21 constant-pool and standard-attribute encoding surface, including all 30 standard attribute bodies. It performs the bounded, file-local validation assigned to the parser in `m1-validation.md`. It exposes typed metadata, lossless Java strings and numeric bits, symbolic indices, actionable errors, and explicit resource-limit failures. Unknown extension attributes remain preserved.

Module and annotation bodies no longer remain opaque recognized attributes. The final review covered the typed module tables and name rules, all annotation element forms, all type-annotation targets/path forms, and shared annotation node/depth budgets. A caller cannot disable the recursive parser's hard depth ceiling by requesting a larger value.

The review also corrected duplicate Synthetic/Deprecated handling, validated SourceDebugExtension encoding, and added checks and regressions for duplicate semantic InnerClasses entries and identical local-variable entries across separate debug-table attributes. Focused metadata tests complement genuine compiler fixtures; the compatibility matrix maps all 30 standard attributes to evidence.

## Final evidence

The coordinator ran independent final checks on the reviewed, frozen source using native Windows Rust 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 77 passed, 0 failed: 45 library unit tests, 4 extended fixture tests, 13 metadata tests, 8 reference tests, 7 CLI tests |
| `python tools/compile_fixtures.py --check` | 17 byte-identical Java 21 class files |
| CLI inspection of the annotated fixture | Passed; zero opaque standard attributes |

The final release-profile fuzz campaign used seed `4851011501752864596`, all 17 genuine class-file seeds, a 64 KiB input limit, default annotation limits, and a bounded feedback corpus of 256 files. It ran for **300.000 seconds** and completed:

- **108,907,980** cases: 3,043,848 accepted and 105,864,132 rejected.
- **106,356** deterministic reparses.
- **0** observed failures.

The source fingerprint was verified unchanged for the final campaign. Evidence is retained in `docs/milestones/evidence/m1-source.json`, `m1-fixtures.json`, and `m1-fuzz.log`. The director read the final log and reviewed the source, tests, coverage matrix and coordinator verification report. It did not claim another independent execution of those tests. An earlier interrupted campaign is excluded from the final evidence.

This is seeded mutation/arbitrary-input fuzzing, not a measured coverage-guided campaign or a proof that every possible malformed input is panic-free.

## Deliberate boundaries

Successful parsing is not bytecode verification, resolution, initialization or execution. Instruction-boundary/opcode/type-state checks, referenced class existence, service and Throwable relationships, nest access and permitted-subclass loading remain assigned to their later stages. Generic signatures and the matching of reflective metadata remain the responsibility of the future metadata/reflection service. Those obligations are recorded explicitly and are not credited as runtime compatibility.

M0's local engineering checks are complete, but remote CI, Linux execution, and the declared Rust 1.85 minimum-version run remain unobserved. M1 acceptance uses the platform-independent parser behavior and native evidence; it does not assert those missing platform results. The Linux target and full roadmap are unchanged.

The next authorized work is actual instruction decoding and a tested numeric method-body evaluator. M2 remains incomplete until its own reviewed scope and acceptance criteria are met. No Java application startup, standard-library compatibility, or production-readiness claim follows from this approval.

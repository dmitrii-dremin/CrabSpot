# M1: Java 21 class-file parser

Status: M1 approved by the technical director on 2026-11-03 after implementation,
independent checks and the final sustained fuzz campaign. See
docs/architecture/m1-review.md. M2 implementation is now authorized.
The user authorized autonomous continuation without per-increment
confirmation on 2026-11-03; no NanoSpec workflow is used.

## Delivered behavior

- Pure safe-Rust class-file library and bounded inspection CLI.
- All constant-pool encodings, lossless modified UTF-8, names, descriptors,
  versioned class/member rules and typed metadata for all 30 standard attributes.
- All annotation value/target/path forms and complete module declaration tables,
  with file-local checks and explicit later-stage semantics.
- Default input bound 16 MiB; class-wide annotation/value budget one million;
  configurable recursion limit with a hard safety ceiling of 64.
- 17 genuine Java 21 fixtures, synthetic positive/negative/boundary coverage,
  a reproducible mutation-fuzz runner, and Linux/Windows CI configuration.

The architecture and implementation were reviewed by GPT-6 Astra. GPT-6.1 Sol
implemented core parsing, a separate Sol worker implemented modules and metadata
tests, and the coordinator added compiler fixtures, integration tests and fuzzing.
Workers used disjoint source ownership and separate output directories.

## Executed checks

Native Windows x86-64, Rust/Cargo 1.98.1, MSVC BuildTools, Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| cargo clippy --workspace --all-targets -- -D warnings | Passed |
| cargo test --workspace | 77 passed, 0 failed, 0 ignored |
| python tools/compile_fixtures.py --check | 17 byte-identical class files |
| crabspot inspect Annotated.class | Passed; version 65.0, zero opaque standard attributes |
| Hosted Linux/Windows CI | Not observed |
| Declared minimum Rust 1.85 | Not executed |

Test counts: 45 library unit tests, 4 extended Java fixture tests, 13 metadata
tests, 8 original fixture/robustness tests, and 7 CLI tests. The coordinator
independently ran the final checks in target/verification-m1 after the workers'
successful checks. No Git repository or commit was created.

## Fuzz evidence

The acceptance campaign runs the release-profile fuzz_parser example for at least
300 seconds with seed 4851011501752864596 and all 17 Java 21 seeds. Inputs are
bounded at 64 KiB; original seeds remain in a corpus capped at 256 entries. Six
mutation families include arbitrary data and structural length/index corruption.
Every 1,024th case is reparsed for deterministic output; failures are saved.

The final campaign passed **108,907,980 cases in 300.000 seconds**: 3,043,848
accepted, 105,864,132 rejected, 106,356 deterministic rechecks, final corpus 256,
zero failures. The input fingerprint was unchanged before and after execution.
The complete log, source fingerprint and fixture manifest are preserved in
docs/milestones/evidence/m1-fuzz.log, m1-source.json and m1-fixtures.json.
This is seeded mutation fuzzing without coverage metrics,
not a claim of coverage-guided fuzzing or proof of panic freedom. An earlier
partial campaign was stopped when review identified final metadata corrections;
it does not count as the final acceptance campaign.

## Review corrections

Reviewed fixes include historical method/initializer flags and descriptors,
interface context, version/location-dependent attribute recognition, legal marker
attribute multiplicity, modified UTF-8 SourceDebugExtension encoding, semantic
InnerClasses duplicates, repeated local variables across debug tables, annotation
budgets/context/ranges, unused module/package constant names and module inventories.
Focused regression tests pass. Source review and test execution are distinguished
in the architecture review reports.

## Remaining scope and limitations

The compatibility matrix in docs/compatibility/status.md maps all 30 standard
attributes to evidence. Generic signature interpretation, reflective association
and resolved annotation types, instruction boundaries/type-state, linkage,
module resolution and class existence remain explicit later-stage responsibilities.
No bytecode execution, class loading, Java library operation or production
compatibility is claimed by M1.

No failing test remains in the executed suite. Linux/remote CI and minimum Rust
version remain untested engineering configurations. M0 has local foundation
evidence but lacks an observed Linux CI result.

## Reproduce

From the project root, execute the formatter, strict Clippy, workspace tests and
fixture command shown above. For the sustained campaign, see tools/FUZZING.md.
The native sandbox requires C:\Users\Admin\.cargo\bin on PATH, a project-local
CARGO_TARGET_DIR/TMP/TEMP, and CARGO_INCREMENTAL=0 to avoid sandbox cache hard-link
warnings. The compiler fixture tool uses target/fixture-tool for scratch space.

Earlier WSL discovery returned access denied; no Linux run was inferred. The
sandbox default temporary directory also denied javac output; project-local
scratch recovered compilation without privilege or ownership changes.

## Continuation

Continue with the approved M2 decoder/execution-kernel design in
docs/architecture/m2-foundation.md.
No user confirmation is needed for routine implementation or technical review.
The M1 gate is satisfied; full M2 completion still needs its own review.

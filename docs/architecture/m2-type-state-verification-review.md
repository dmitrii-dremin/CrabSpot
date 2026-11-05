# Modern method type-state verification review

**Approved for publication, 2026-11-05.** The technical director accepts the standalone Java SE 21 modern method type checker under the reviewed contract and its handler amendment. The coordinator is authorized to publish this increment and begin exact-contract planning for modern class type verification. The next implementation requires its own contract and review. The published source passed hosted Linux and Windows CI.

## Accepted guarantee

For a canonical method declaration in a retained class of major version 51–65, the verifier checks Code representation, final overriding and the modern stack-map type rules against an immutable loader-aware environment. Legal abstract and native declarations receive the applicable bodyless checks. A successful result borrows the canonical method and its exact environment. A failure identifies its phase, location and completed resource usage. The positive instruction audit observed all 198 modern effective opcode values and all 11 legal wide encodings in successfully proved methods.

Full M2 remains open. Further obligations include modern whole-class aggregation, historical inference, loading constraints and verification failure lifecycle, execution integration, remaining runtime behavior and performance evidence. The public method guarantee and its scope are specified in the [modern method contract](m2-type-state-verification.md). The proposed next step is [modern class type verification](m2-class-type-verification-next.md).

## Review decisions

The original contract overconstrained the destination stack of an exception handler. Four original Java 21 cases established that a handler may prescribe Top and return with the exception unused. An attempt to pop or store that Top fails verification. A Throwable-typed handler may pop the value. The amended rule checks assignment from the incoming catch type to one prescribed slot. Constructor exception edges still check both required local views with the original initialization flag. The contract amendment was reviewed before integrating the corrected implementation. [JVMS frame assignment and handler legality](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10.1.6).

Two additional original cases confirm that a nominal source can flow to the designated bootstrap Object without loading the source. Both pass reflective verification. Invoking the null case returns null. Invoking the non-null cast encounters NoClassDefFoundError. The verifier also respects the actual declaring package of protected members and skips an interface's structural Object edge during inherited field lookup. Package-sensitive final overriding includes transitive bridges and private/static exemptions. The verification environment supplies the defining-loader identities for these decisions.

Resource fixes fund verification-type width scans, nested attribute headers and initial ancestry edges before traversal. Frame comparison recognizes identical immutable local prefixes while preserving exceptional substitution, stack and flag checks. The shared-prefix regression covers 30,000 local slots and 10,000 maps with exactly 30,000 materialized cells and less than one million work units. A randomized fingerprint allows spelling tables to grow using scalar hashes. Exact UTF-16 unit equality still distinguishes colliding spellings. Tests cover long descriptors, a 10,000-class hierarchy, transactional limits, corruption guards and deterministic failure locations.

## Independent evidence

The root tests proved 935 genuine Code bodies and 31 legal bodyless declarations across 173 ordinary definitions from 174 javac classfiles including module-info. The Rust integration checks repeat each successful proof and verify its canonical class/method/environment binding. All 168 preceding genuine classfile hashes were preserved. The 122 captured JDK projections supply hierarchy and ordered member facts for the fixture environment, including 614 fields and 3,250 method declarations. An independent Rust reader matched each projected class to its original JDK metadata. Projection bodies serve only fixture admission. The capture explicitly records absent/Missing as unavailable.

The 57 new stage-aware JDK rows use 69 original classfiles. Of these rows, 29 pass verification, including cases with later invocation errors, 23 fail at reflective lookup with VerifyError and five invalid final overrides fail class definition with IncompatibleClassChangeError. The preceding 18-row structural oracle continues to distinguish four positives, twelve representation failures and two type failures. The 26,508 earlier execution comparisons and the separate runtime service oracles remain covered. CI now directly compares the regenerated 57-row TSV with the committed outcomes in addition to checking reproducibility on the current JDK.

## Final gate

I inspected the coordinator's fresh-target evidence and the actual Rust test-result lines, then independently rehashed all 235 files against the final combined manifest. All 35 recorded commands exited successfully. The native toolchains were Rust/Cargo 1.98.1 and Zulu JDK 21.0.7.

| Check | Executed result |
| --- | --- |
| Formatting and strict locked workspace/all-target Clippy | Passed |
| Locked workspace tests in debug | 1,071 passed, zero failed |
| Locked optimized verifier/runtime/interpreter suites | 984 passed, comprising 124 verifier, 436 runtime and 424 interpreter |
| Fixture and provenance gate | 10 Python regressions, 45 canonical Java sources, 22 reference records, 174 genuine classes and 122 independently compared JDK projections |
| Parser harness boundary test | One explicit example test passed at 512 seeds and the first excess seed |
| Seeded parser smoke | 392 initial classfiles, 10,000 cases, 113 accepted, 9,887 rejected, ten deterministic rechecks, 505 retained inputs and zero failures |

The initial 233-file gate completed its Rust suites, then stopped before fuzzing when the expanded seed inventory exceeded its 256-entry harness limit. The coordinator raised the harness cap to 512, preserved the 64 KiB per-input bound, added the explicit boundary test and reran the full gate in a fresh target. I used the resulting 235-file run for this decision. The 73 additional workspace tests comprise 15 private verifier invariants, 54 independent public tests and four coordinator integration tests. The explicit harness boundary test is reported separately.

Reproduction evidence was retained locally under target/verification-type-state-final-549f78608068, with command arguments and exit results in evidence.json and per-command logs alongside it. The final combined manifest is target/coordinator/type-state-final-combined-v2.json, SHA256 3B0A2FFBC72C70467BD4F02578FE28C3B53C8D96651D44DDFBA1C1003FE5BDD9. All 235 working-copy digests matched after the gate. The amended dispatch contract digest is 956F7B94BDE10A78DBC06B42CAF74569F456C8A5A84D3BC8C4C2C3949B1B1492. The coordinator may append actual hosted evidence after inspecting both jobs.

## Hosted confirmation

Implementation `fb585a2f6c07364311ac090efb267eac5c52311a` passed both jobs on
Rust/Cargo 1.99.1 and Temurin 21.0.13. The coordinator inspected both complete
job logs. Each reports 1,071 workspace tests on the committed fixtures, another
1,071 after regeneration, 984 optimized tests and one parser harness boundary
test, totaling **3,127 Rust test executions per platform**. The optimized counts
are 424 interpreter, 436 runtime and 124 verifier.
[Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42067751551/job/127386545824) · [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42067751551/job/127386545408)

Both jobs passed strict lint, formatting, ten Python regressions, the 45-source
and 22-reference provenance checks, and deterministic fixture reproduction.
Each JDK supplied 122 metadata projections with one deliberately unavailable
name. Its regenerated 57-row verification-stage oracle matched the committed
Zulu 21.0.7 outcomes byte for byte. The parser smoke on each platform used
392 initial seeds, ran 10,000 mutations with 113 accepted and 9,887 rejected,
rechecked ten outcomes and finished with 505 retained inputs and zero failures.

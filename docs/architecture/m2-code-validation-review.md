# Whole-method Code validation review

Status: approved by the technical director on 2026-11-04 for the bounded standalone service in `m2-code-validation.md`. Frozen production, independent tests, coordinator fixtures and the fresh-target combined gate pass. Implementation 08386b361be4992e4c02ef0931aff46185c809e3 is published and its Linux/Windows CI passed with inspected coordinator evidence below. Full type-state verification, full M2 and the complete JVM target remain open.

## Reviewed source and result boundary

The director read the production implementation, its nine private tests and README; the independent forty-test suite and original builder; and the coordinator's genuine-corpus and stage-aware JDK oracle tests, emitter and reproduction tool. All seven production and two test snapshot hashes were independently verified, as were all thirty-six root snapshot hashes. The integrated forty-five-file manifest was independently rechecked after the final gate. The contract hash at that gate was `6430F75E380B1CB41344158B3719F03D7139991A409864C1FB452151903EDCCE`.

Evidence manifests are `target/code-validation-runtime-worker/frozen-manifest.json`, `target/code-validation-tests-worker/frozen-manifest-v2.json`, `target/coordinator/code-validation-root-v1-manifest.json` and `target/coordinator/code-validation-final-combined-manifest.json`. Selected hashes identify native working-copy bytes before Git line-ending normalization:

- Instruction/metadata inspection: `C2F15D4A0F279A5208847E20687ACB6A8E27A4CA7330056432CC0D1DBE5A9B8B`.
- Compact descriptor/name scanner: `DE59879A0D68FA3DEE74713534B541C6008A1B003E339F768428D644F01F1CEA`.
- Stack-map representation checks: `84B82EB9DA1ECA5E4D5CC65545771CF3551CEAABBA8039F9898A9CD576A96233`.
- Independent public tests: `695ECB4094F63774C3BFF578406DFC65292BDF51FC79AB9C416BDB694BB02408`.
- Public test builder: `F7E648208D98C5131C958DC0C3329FA0F82796BE8D15E196E72F25FD8C3A078A`.

The sixth workspace crate, `crabspot-verifier`, depends only on classfile and bytecode. Its result borrows the immutable class/member/Code and owns one decoded instruction sequence. Legal ordinary abstract/native declarations have no Code or decoded body. No existing parser, launcher, registry or execution API gains an implicit gate.

Inspection covers every decoded instruction, including dead code and instructions outside the interpreter's executable subset. Used CP tags, links, names, descriptors, version relationships, invocation widths, local ranges and array dimensions are checked. Handler endpoints and normalized stack-map offsets/types are checked, with only current logical local widths retained. Lossless UTF-16 summaries use scalar index/role keys. Full Code funding precedes decode; metadata and UTF funding is atomic; new buffers reserve fallibly from actual demand. The unchanged decoder's input-bounded allocation behavior is documented without inventing recoverable OOM handling.

The result does not certify stack/local flow, return compatibility, fall-off safety, required branch frames, constructor proofs, protected access, bootstrap relations, class identity or linking. Ignored attributes and the enclosing member inventory are not silently certified. Recognition-aware opaque extension semantics remain intact. No parser/decoder behavior or interpreter execution semantics changed.

## Corrections and coverage

The production worker identified and corrected one diagnostic/work-order issue after APIv1: a fixed-category mismatch such as ldc2_w with a Class constant followed its linked name before rejecting. Final code rejects the outer category first; Dynamic still needs its descriptor to determine width. Private and independent public regressions pin Work 20 and UTF 10, including a 60,000-unit Class name that must not be scanned. This is a recorded correction, not a claim that the initial implementation had no issue.

Review clarified that each logical handler start/end/target predicate costs one work unit, including end equal to Code length. It also clarified that whole-class methods_count validation is outside the selected-method result. Neither clarification required a production change. List-count errors now identify their actual metadata phase. Private reserve-helper extraction permits deterministic failure tests without changing the allocation algorithm.

The independent suite covers all CP/local operand families, real wide holes and Dynamic widths, current-version pool transplants, MethodHandle kinds, receiver-inclusive invocation limits, rank limits, historical initializers, body presence, opaque extensions, malformed public metadata, all stack-map forms and width rules, switch boundaries, exact quotas and atomic failures. Review added a twelve-case MethodHandle/MethodType 254/255-slot matrix to exercise its separate receiver policy. It passes without a production correction. Long shared descriptors and 30,000 retained locals followed by 10,000 Same frames pin bounded repeated work. Private tests cover arithmetic/reservation failures, scanner details and 2,048 deterministic malformed-byte mutations, each repeated without input mutation.

The coordinator validates 890 Code bodies and 31 legal bodyless declarations across all 168 genuine classes, covering 25,236 decoded instructions and 173 opcode values. Original emitted files produce eighteen independently recorded JDK outcomes: four accepted, twelve representation negatives, and two type-state negatives deliberately accepted by this partial service. All eighteen define successfully; reflective lookup forces the failing verification stage. Tests distinguish ClassFormatError from VerifyError and never equate definition success with verification. The emitter's exact artifacts, driver classes, source and output hashes are checked; reference-JDK execution remains tooling-only.

## Accepted combined evidence

The accepted build directory is `target/verification-code-validation-final-abde6732caba`, with `evidence.json` and `logs/`. The director inspected the command results, summed the actual Rust test-result lines, checked the fixture/provenance/smoke logs and reverified all forty-five integrated hashes. Local tools were Rust/Cargo 1.98.1 and Zulu JDK 21.0.7.

- `cargo fmt --check`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- `cargo test --workspace --locked`: 998 passed, zero failed.
- Optimized verifier/runtime/interpreter tests: 911 passed, zero failed: 51 verifier, 436 runtime and 424 interpreter.
- Eight Python provenance regressions and canonical checks for 43 Java sources and 21 reference records: pass.
- All 168 javac-produced classes and 27 original encodings reproduce, including the prior nine construction and eighteen new validation files. All 165 prior genuine class hashes are preserved. All 21 reference sets reproduce; the existing 26,508 execution rows remain distinct from the new eighteen stage-aware validation rows.
- Parser smoke over 195 initial files: seed `4851011501752864596`, 10,000 cases, 138 accepted, 9,862 rejected, 10 deterministic rechecks, final corpus 256, zero failures.

The earlier target `verification-code-validation-final-dac35057f160` ran before the final public boundary matrix and is preliminary only (997/910). Approval uses the later fresh target and final test bytes. No stale-cache result is substituted for the accepted gate.

No blocking source finding remains. This approval authorizes publication of the bounded increment, not a claim of full JVM verification, application startup or performance superiority. The next direction is `m2-type-state-verification-next.md`; it authorizes exact-contract planning only.

## Hosted confirmation

The coordinator inspected both complete job logs in run [41867714340](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41867714340) for implementation `08386b361be4992e4c02ef0931aff46185c809e3`: Linux job 126706186206 and Windows job 126706186887. Both passed on Rust/Cargo 1.99.1 and Temurin JDK 21.0.13. Each ran 998 checked-in tests, 998 regenerated-fixture tests, and 911 optimized tests (424 interpreter, 436 runtime, 51 verifier): 2,907 Rust test executions.

Both jobs passed formatting, strict linting, eight Python tests, 43-source/21-record preflight, all 168 genuine and 27 original artifact reproduction checks, twenty execution-reference sets and the eighteen-case stage-aware validation set. Both smoke runs used 195 inputs and seed `4851011501752864596`: 10,000 cases, 138 accepted, 9,862 rejected, ten rechecks, final corpus 256, zero failures. Derived evidence is retained at `target/coordinator/code-validation-hosted-evidence.json`; raw job logs were inspected by the coordinator. This confirms the bounded checkpoint, without enlarging its proof or execution scope.

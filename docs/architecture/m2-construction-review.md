# Construction review

Status: approved by the technical director on 2026-11-04 for the bounded session construction increment in `m2-construction.md`. Frozen runtime, root oracle and revised interpreter review and the independent combined gate in a fresh build directory pass. Publication is authorized under the existing project mandate; fresh hosted CI remains pending publication. Full M2 and complete JVM verification remain open.

## Frozen runtime component

The director verified every entry in the eleven-file immutable runtime manifest against both its snapshot and the integrated main copy. The contract hash is `6BD1444EA7A8D90D80A32E1F826F0FF88A08EA945A22D87CCD8DD92F1E546D6A`. Evidence resides in `target/construction-runtime-worker/frozen-manifest.json` and its `frozen-source-v1` snapshot.

Hashes identify native working-copy bytes before Git line-ending normalization:

- Construction services: `3BCB138717BC9B8E834FDD11039E20A0E72D0FFCF5135382026911B6E6AC69B4`.
- Instance storage and private commit guards: `1395E8119108B8E78413687B03157CD2DE471AACC94B36657179E8BBE85166CC`.
- Runtime public construction tests: `A483562DDC43144BEAE1A224D3A514B791DCDBD924FFEE2A60BEBD041335756F`.

The checked services preserve actual class/member identity, claim and heap fingerprints, access/loading order, ancestor-access-before-constructor-owner failure, lazy designated Object validation, exact Allocation versus This relationships, and own-declaration final authority. The private commit rechecks role, field and constructor fingerprints, owner-qualified slot/kind/finality, source handles and destination bounds before its single write. Raw field setters retain their previous refusal boundaries. Runtime authority tokens do not certify interpreter frame provenance, constructor completion or class initialization.

Allocation funding is incremental before allocation: insufficient work preserves the funded prefix and allocates nothing; a subsequent quota failure retains the complete traversal charge. Reference assignment retains its existing source-first identity-only descriptor rules. Cached operations repeat the required work, including full reference-bearing signatures with all-null arguments. No new successful-construction setter, global initialized-object flag or exception-object materialization is introduced.

The final delta from the initial compiling runtime snapshot adds defensive checks for the actual instance-initializer role and agreement between registry static/final flags and the admitted layout. A forged proof with an ordinary helper MethodId and a matching forged fingerprint now rejects before mutation. This is recorded as private-invariant hardening; no reachable legal-input semantic failure was demonstrated. The regression preserves nonzero fields and non-null child edges across damaged proofs, stale/foreign sources and damaged destination payloads.

Reviewed coverage comprises twenty-one public and five private additions: inclusive and cumulative work limits, quota failure/retry, maximum UTF-16 descriptors, null and nominal-loader boundaries, protected constructor restrictions, exact early-field ownership, other ordinary receivers for own final fields, primitive normalization/raw floating bits, checked reference children, malformed tokens/proofs and iterative 10,000-class traversal. The worker reports formatting, strict workspace/all-target Clippy, 908 workspace tests, 436 optimized runtime tests and whitespace checks passing. These are component results, not substitutes for the pending coordinator gate.

## Frozen integration review

The frozen genuine/original-bytecode oracle component has no blocking review finding. The director verified all thirty-five entries in `target/coordinator/construction-root-manifest.json` against its immutable snapshot and integrated main copy. The Rust oracle hash is `1E6C3F2721BDDB259A881944A64A13D900B45DB2BA2652F7B74CF14D5F1EB313`; the original Java source hash is `FA0AD6E8166C2770F16FB1691542E699EABD3F02DA0843A92096D55344B39E83`.

The model asserts retained array types and payload bytes, aliases, complete fields and child edges, class statuses and heap accounting. It explicitly distinguishes the JDK raw child-loader arrangement from the deliberately single-defining-loader Rust profile. The corpus has 184 rows: 112 genuine cases and 72 original-bytecode cases, including nine additional narrow-store inputs. A genuine completed-allocation loop revisits one new instruction in the same frame; worker synthetic tests separately cover unfinished same-site aliases. The corpus contains 165 javac-produced classes and nine original Java 65 encodings, with all 151 prior class hashes reported unchanged.

The original-bytecode generator verifies the source and compiled emitter hashes, requires JDK 21 and exactly nine major-65 outputs, and compares exact bytes and provenance in check mode. Pre-regeneration checks validate both source provenance and the original artifact set/hashes; the seventh Python regression covers corrupt, missing and extra records/artifacts. CI performs this check before checked-in fixture tests, regenerates and checks the original encodings, runs the JDK construction oracle, and includes the entire fixture tree in parser smoke inputs. The ordinary-definition count update is a corpus-only change. Both oracle tests pass in the final debug and optimized gates.

Interpreter review verified all nine initial frozen component hashes against the immutable snapshot and integrated main copy. The single-funded-scan correction for failed-alias removal and consistent single-activation/MethodId validation at constructor authority-consuming boundaries are present, with mixed-width survivor and damaged-authority regressions. Ordinary helper exceptions and failed separate allocations preserve a pending chain; failed this/super calls invalidate its aliases before caller handler search. Tests also cover actual Object Code and escaping references, same-site reuse, iterative constructor chains, exact limits and panic cleanup.

Final diff review found an additional observable legacy ordering regression: the new private-reference validator in `if_acmpeq`/`if_acmpne` ran on the right operand before extracting the left. Previously both typed operands were extracted before either handle check. A foreign right reference combined with an absent or wrong-kind left operand therefore changed diagnostic precedence. The revised v2 snapshot restores two-operand extraction first and retains private-token validation. Its regression checks both opcodes with a foreign right reference and either an absent or wrong-kind left operand. The director verified all nine v2 immutable snapshot hashes and the exact two-file correction; this finding is resolved.

The revised engine hash is `F0B8629B18D1006DBF682B4A3A9BD24A10DBB75DCD127E6A815F66C330DCE7C3`; frame storage is `65D9AAA49B727A2D2153CE148CE5FD4F694F0AF7B8A27CF22B980940E95508C8`; private construction tests are `1BDED8ADE0608BE6DE7C66CB61DF8517F22C5BA461207A5C90034208A6F8C112`. The v2 worker reports 924 workspace tests and 422 optimized interpreter tests, formatting and strict Clippy passing. Additions comprise twenty-eight public and nine private tests; the sixteen legal stack-manipulation forms also run with private one-slot aliases.

## Verification correction and final combined gate

The coordinator detected stale Cargo debug output after copying immutable snapshots with preserved older modification times. Matching source hashes did not establish that Cargo rebuilt those files: the reused debug cache still listed 68 runtime unit tests where the final source contains 73. The earlier combined debug total is therefore not acceptance evidence. The coordinator integrated v2 with fresh destination timestamps, preserving identical bytes, and ran the complete gate in the new `target/verification-construction-final` Cargo target directory. No cache deletion, ACL change or process termination was used. Final approval uses the fresh-directory results and post-gate source verification only.

The director independently checked all 55 combined manifest hashes after the gate and summed the test-result lines in the final logs. Accepted results are:

- `cargo fmt --check`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- `cargo test --workspace --locked`: 947 passed, zero failed; the rebuilt runtime unit binary includes all 73 tests.
- `cargo test -p crabspot-runtime -p crabspot-interpreter --release --locked`: 860 passed, zero failed (436 runtime and 424 interpreter).
- 65 new Rust tests: 21 runtime public, five runtime private, 28 interpreter public, nine interpreter private and two independent oracle tests. Existing stack-form tests additionally exercise private aliases.
- All 26,508 distinct reference rows across twenty sets pass, including 184 new construction rows; the separate 151 array-service and 151 field-service cases remain covered.
- Reproduction passes for 165 javac classfiles and nine original encodings, all 151 previous javac hashes preserved; 42 canonical sources and seven Python provenance regressions pass.
- Parser smoke over 174 initial classfiles: seed `4851011501752864596`, 10,000 cases, 119 accepted, 9,881 rejected, ten deterministic rechecks, final corpus 256, zero failures. This is seeded mutation evidence, not a coverage-guided campaign or a verifier proof.

Aggregate evidence is `target/coordinator/construction-final-evidence.json`; frozen paths are in `target/coordinator/construction-combined-manifest.json`. Final logs are `construction-final-{fmt,clippy,debug,release,fixtures,fuzz}.log` in the same directory. Hosted CI remains pending publication and must be appended only after the actual run is inspected.

## Bounded disposition

The existing session now executes real allocation and constructor bodies with checked resolution, initialization-before-allocation, private unfinished references, per-frame chaining/promotion/invalidation and scoped own-field final authority. Lawful escaped ordinary references and prior effects survive constructor failure. No global heap completion bit, skipped Object body, constructed VM exception or broad raw-final bypass substitutes for these obligations. Existing body entry points retain their earlier gates.

This approval does not certify the full verifier, all class-loader constraints, GC/root tracing, Java library bootstrap, VM-exception materialization, String constants, concurrent initialization or application startup. The next proposed verification foundation is described in `m2-code-validation-next.md`; it requires a separate exact contract before production dispatch.

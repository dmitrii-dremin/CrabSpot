# Operand-time VM exceptions: final technical review

## Decision

ACCEPTED for the bounded operand-exception increment. Publication is authorized.

This decision binds the final 1,431-input publication manifest `D41CE9C3902649F739958DC85D50D5ABD448451EF303DBC1EBB824CBD0A40A88`, the executed core manifest `4FB24851CB60AF22031D6B0DBAAA510F9F72C361BC87593FB7D3FCAFB66DBE5B`, and the evidence below. It authorizes the coordinator's already-approved commit/push workflow for this increment and its reviewed documentation. It does not approve M2 completion, general Java21 compatibility, performance parity, or the next initialization-failure implementation.

No blocking source, test, provenance or remaining required local validation issue was found. Hosted verification of this operand publication remains unexecuted at the time of this decision; earlier hosted results retain their own commit scope.

## Accepted behavior

The implementation extends the genuine Throwable construction continuation with catchable ArrayIndexOutOfBoundsException, NegativeArraySizeException, ArrayStoreException and ClassCastException at twenty existing array/allocation/cast instructions. Existing ArithmeticException behavior remains supported. Detailed additive service outcomes retain actual scalar/type facts at the failing operation and project back to legacy symbolic results through the shared implementation. Success does not repeat operand extraction, relation checks or instruction fuel.

Real selected Java constructors, ordinary class initialization, checked field writes, native capture and existing dispatch produce the objects and preserve original fault positions, retained guest writes, cause state, identity, refill and rethrow behavior. The new five-kind profile uses a bounded constructor-role plan and the shared engine continuation. Explicit previous Arithmetic-only sessions/wrappers keep their gates; ordinary convenience Auto launchers intentionally advance. Missing or malformed required roles do not silently fall back after the profile's prerequisites select the new path.

Diagnostics use checked UTF-16 work, per-message/cumulative units and temporary-storage limits, followed by existing managed String accounting. Numeric wording follows the declared primary policy; type diagnostics use actual runtime types and avoid invented module/Java-loader names. Independent reference evidence retains vendor differences, including OpenJ9 null ArrayStoreException messages and different negative-dimension selection. The public guest receives independently fixed expected texts for each declared policy, not expectations learned from CrabSpot output.

The additive CLI `--field-lookup-steps N` forwards only the existing budget. The one-million default remains unchanged and its failure is a regression control. The full 248-fault public application explicitly requests 16,777,216, a tested finite allowance rather than an asserted exact minimum. Bounded ASCII parsing, option order/duplication, native argument preservation, zero/MAX and malformed-before-I/O cases are covered; existing input struct literals remain compatible.

## Exact component and source evidence

| Component | Frozen manifest SHA256 |
| --- | --- |
| Runtime13 | `7D261B0F631EB3962C875D06EE42F38C05601080C972231D20C816121E8126CF` |
| Execution18 | `5A69E92253EB100BB7DF9EEBD28D8BEBED71A2C212E354DE75763AF450171EEF` |
| CLI amendment4 | `168D9EB6970F6BAED8CAEA9572C40FB4C8478D1E656B5D60C23FEB718150D7AD` |
| Public reference29 | `2123D5A4F06A4B741A2EED96DAFB8E4067A06C1DB0E7A04511C80564C6BE8224` |
| Explicit-primary amendment4 plus unchanged25 | `1273E4C049F39EC0FAAA6D4878F0A286163CD37BADB53B9CC905BA33BF6DD34B` |
| Product driver/harness3 | `43B99F70D66CCF0AEDDABE36010BDBF55974AA345A2C5B6610DEDC6E6AD146BD` |

All final live and immutable snapshot input hashes were independently rechecked. The publication manifest differs from the executed core manifest only in the CI step explicitly running the new example's Rust unit test; the other 1,430 inputs are unchanged. That exact test passed in the core gate. New reference/product CI steps are separate commands, and primary-only invocation is explicit rather than a silent reduction of the historical dual-VM authority.

Runtime, execution, CLI and harness source reviews covered ordinary/negative/boundary behavior, private authority corruption, complete prerequisite affinity, exact paid prefixes, original/inner error provenance, replacement construction and nested-materialization guards. Earlier immutable snapshots, the String-affinity correction and failed initial observations remain preserved. No test was weakened to bless the default resource failure or a diagnostic mismatch.

## Final local validation

Core evidence `E898C9E3F7E2B7EEB58945EA6F707E64FCD9E1453A3462B6F634A54E74F48F35` contains24 terminal successful commands. I verified every referenced log hash. Actual logs show1,758 workspace debug passes and1,678 optimized passes, with two existing ignores in each; the genuine ignored controls passed separately. Format and strict workspace/all-target Clippy passed. All168 Python regressions passed. Genuine fixture/provenance, metadata and reference checks passed. The supplied Zulu java.base pass proved7,519 ordinary classes with61,708 Code bodies and4,685 bodyless declarations; this is verification evidence, not execution of the full library.

The776-seed sustained parser campaign completed50,390,488 cases in300 seconds, with513,167 accepted,49,877,321 rejected,49,210 deterministic rechecks and zero failures. Parser limits and the separate10,000-case smoke also passed. These results establish the recorded campaign, not exhaustive parser correctness.

Final integrated product evidence `A7FE1581CECDF125C75F623B270C49B21B871D154F32CE97ACD841B4452BB497` completed136 commands on each supplied library, Zulu and Temurin, with archived/rebuilt guests, API and physical CLI checks, exact quotas, wrong-oracle controls and original/refilled/unsupported-null provenance. Reports are:

- Zulu `559CA8147FED0123FA9665DE8BB864A58FE145A2EE953135B76D8A45C02D5AC7`.
- Temurin `6E62C3F1303A69551382009D283D5456A4BFD77D78753D21C389C177669DF122`.

I independently verified both reports/logs, executable/tool/source pins and original/rebuilt eight-class equality, and reran the complete cleared comparison suite against all272 recorded command responses and their exact arguments. This was independent response validation, not another execution. All commands satisfied their expected status, including required negative exits. Final CLI SHA256 is `AD99192B70F6873D6600B57CF73375D49C89BA87F50496388FD30CBA298EA0DB`; product example SHA256 is `622C0BF8462525C86B8AA5FC95521D2617CCE38AE61619CFCD524ADDD6679B15`.

The normal guest independently requires184/42/6/8/8/248/29/1,240 generated diagnostic messages,7,700 message units,9,624 diagnostic work,496 snapshots/backtraces,1,686 frames,7,638 trace work and248 started/produced materializations. Both final supplied-library runs satisfy these values. Zulu/Temurin discovery work is independently computed as4,051/4,269. The worker's separate earlier272-command evidence `6B3D1356A5B6B6B7365F77E2F2144F89A468606CF87896736086A56AC2B250D5` remains component evidence with its actual earlier reference dependency; it is not relabeled as the final integrated run.

The genuine reference component separately preserves88 behavior plus24 codec controls on Zulu/OpenJ9, and44 plus12 in explicit actual-Temurin-primary mode. Local dual acceptance is not inferred from a single-primary report. Reference VMs are used solely for identified reference observations and class compilation; CrabSpot executes the product suites itself.

## Inherited regression and portability evidence

Seven existing public replay families passed with unchanged source guards. Their evidence and limitations remain as reviewed in `operand-legacy-evidence-inheritance-decision.md` SHA256 `7455CA88C578DDC9AAADC98F7E51587DD2814CA7DA1A2555CCCE1E37ABEB995A` and `operand-final-components-and-inheritance-review.md` SHA256 `D986848833EBCF9EFB9AAC40B5C982E225A2AC9505A1503BFA127455CCABA72F`.

The first six families lack measured pre-run executable hashes; later pins are not retrospective measurements. Retained exact-source builds, source guards and the exclusive-cache record support their scoped inheritance. Raw rebuilt executable hashes differ. Independent PE parsing confirmed exactly24 changed bytes per eight executable pairs, confined to validated COFF/debug timestamps and the PDB GUID; every other byte is identical. No executable was edited and no arbitrary normalization policy was introduced. This explained build-metadata difference does not require repeating the seven families, but they remain inherited runs rather than fresh runs on raw-identical final binaries. Throwable also has its own recorded executable pins.

Canonical-export evidence `C1C372F1D9BB36FFA11F734D5A3ACEE023FBBC9090BC7CB2B8A49A9B670B0848`, audit `D4CF0BABDC71077476172574BDEFF289CCC42B925EDBAD1BEB90BB78294916F7`, preserves780 binary files and normalizes66 text CRLF files among1,431 inputs. I checked every exported/captured hash, exact CRLF-only transformation, current attributes hash and independently recomputed every recorded Git clean blob identity. The export's25 reference tests,20 product tests,46-source/23-record provenance and format checks passed with verified log hashes. The initial bare-cargo lookup failure occurred before process creation and is preserved separately from the successful explicit cargo invocation. This is canonical-checkout evidence on Windows, not a Linux execution claim.

## Limits and publication documentation

General NPE production/extended messages, other monitor/catch-resolution VM producers, non-Error initialization wrapping and erroneous-class retry production, Java stack-element population, arbitrary Java loaders/modules, broad native/library behavior, threads/JMM and GC remain open. Host resource and unsupported-operation failures remain typed host failures. The new bodies do not expand the effective opcode count beyond197, and opcode coverage is not a JVM conformance score. No benchmark or faster-than-HotSpot claim is established.

README, compatibility status, current milestone and the integration report were reviewed. The integration report now labels intermediate checks historically and states terminal local results accurately. The coordinator may mechanically replace aggregate-pending statements with this bounded acceptance, update the current-checkpoint title, link this exact review and retain hosted-pending/full-M2-open wording. That disposition update changes no tested source contract. The user-owned untracked parity-audit document is excluded from this authorization.

Independent final audit: target/resolution-constraints-director/operand-aggregate-final-audit.json SHA256 `DA9A76A60517290A06F9A7908CC92EC8F2E8EC2DFF174C6F4C61E02BA1C24FF7`. The audit's pending-document/disposition field records its earlier execution point; this signed-off review supplies the completed disposition. Exact report/source hashes above control the acceptance, not informal shorthand or counts alone.

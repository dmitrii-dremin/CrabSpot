# Throwable construction and VM exceptions: integration evidence

This increment implements the [frozen construction contract](m2-vm-exception-construction.md).
The technical director approved the exact final sources and combined local
verification on 2026-11-06. [Aggregate technical review](m2-vm-exception-construction-review.md).
The first hosted implementation run exposed the packaging defect recorded below.
This report does not close M2.

Source dispositions are available for the [runtime](m2-throwable-runtime-review.md),
[interpreter and launcher](m2-throwable-execution-review.md), and
[public product harness](m2-throwable-public-harness-review.md).

## Behavior under integration

The interpreter executes the supplied Java Throwable, Exception, RuntimeException
and ArithmeticException constructors and their required initializers. The private
`fillInStackTrace(int)` native records bounded managed traces. Integer and long
division and remainder by zero construct actual ArithmeticException objects and
dispatch them at the original failing instruction. Rethrow retains object identity;
refill replaces the current capture. Constructor execution uses private continuations
without fabricated Java frames or constant-pool entries.

Managed trace tokens retain their owning exception and immutable method/PC records.
Discovery, allocation, trace traversal and publication have explicit budgets and
failure boundaries. Uncaught results expose the actual exception class and an
optional current captured head. The Auto, RequireThrowable and Disabled profiles
make image support explicit.

Java stack-trace population remains unsupported. The real `getStackTrace` path
reaches the unbound StackTraceElement population native through `of` at invocation
offset 31; it does not return a fabricated empty trace. Other VM exception producers,
general native bindings, concurrent execution and garbage collection remain open.

## Frozen components

| Component | Files | Manifest SHA-256 | Verification |
| --- | ---: | --- | --- |
| Runtime | 13 | `B4F70C48A5D6B027383CD013002A9087888DC889134EF2BAFFD81925BAB28DEF` | 692 debug and 692 optimized tests; formatting and strict component Clippy passed |
| Interpreter and launcher | 32 | `D8955BCA7DD6A66DF708CA8B19482E6FC81229B3EC60996E7E22621B9C9C42DE` | 743 debug and 743 optimized tests; two existing ignored tests in each run; formatting and strict workspace Clippy passed |
| Public guest and reference harness | 22 | `D3CAC6E29B6B1E0BEF9CE8720DE3918ECEAE4A0261A17259D62E90D8BDF36B6B` | 16 harness tests, 40 Zulu/OpenJ9 observations and 20 alternate-compiler Temurin observations passed |
| Public product harness | 3 | `DB00A40218E34996BD6ABE14F087E6706249BEA3A925CD729DE30420DDD29596` | Source review cleared; 19 independent negative-test groups passed; both worker and coordinator platform replays passed |

The coordinator installed all 45 core files with guarded source-hash checks.
The installed manifest has SHA-256
`184D8E1BE177E414A992D7050264D19BC274F214EDC115891DC73D282EB6B7A2`.
The public reference evidence and its review are recorded
[separately](m2-throwable-entry-reference.md). Reference JVM outcomes alone do not
establish CrabSpot execution support.

## Public execution on two supplied platforms

The frozen public harness completed two Windows runs. On each of Zulu 21.0.7 and
Temurin 21.0.13, CrabSpot passed 48 archived/rebuilt API and resource-limit cases,
14 physical CLI cases and five profile controls. The Zulu run also checked two
explicit OpenJ9-library incompatibility cases. Separate reference executions
produced 40 Zulu/OpenJ9 observations and 20 Temurin observations. Zulu reproduced
the archived compiler bytes exactly; Temurin passed the alternate-compiler
inventory and semantic checks.

Both product platforms returned `65535/8/4/1` for the main guest's fields. Each
normal run proved, prepared and admitted 50 definitions, created one argument
array and used one evaluation. It started and produced 13 VM exceptions, with
23 captures, 51 retained frames and 468 trace-work units. Discovery consumed
18,184 units on Zulu and 18,263 on Temurin, reflecting their actual supplied bytes.
Final fixture, helper, platform and binary checks passed; all three public source
hashes matched their frozen copies afterward. The coordinator independently
verified all six worker log hashes and both report hashes.

```text
python -m unittest discover -s tools -p test_throwable_launch.py -v
python tools/verify_throwable_entry.py
python tools/verify_throwable_entry.py --compare-openj9 PATH_TO_OPENJ9_21_JAVA --openj9-platform PATH_TO_EXTRACTED_OPENJ9_JAVA_BASE
```

Set `JAVA_HOME` to the selected Java 21 HotSpot JDK; `--jdk` and `--platform` allow
explicit compiler/runtime and extracted platform paths. These worker results are
separate from final combined-tree acceptance and hosted CI.

## Regression expectations

The expanded image discovery runs before dependency-closure checks. Existing String
body negative cases now require the same paid discovery work as their successful
peer while retaining zero argument allocations, evaluations, proofs, preparation
witnesses and admitted heap classes. This preserves the pre-execution failure boundary.

AssertionError is now an allowed platform import. Consequently the earlier
application assertion-policy probe reaches its actual Class.desiredAssertionStatus
call at offset 1 through the application initializer at offset 2. It reports the
existing unsupported application-mirror target with its earlier state intact.
The replay checks the actual callee and initializer identities rather than assuming
that an initializer failure must originate in a method declared by that class.

Both expectation migrations received source review. The combined gate's terminal
continuation passed the String body and equality replays on archived and rebuilt
guests. It supplies authoritative successful exit statuses, superseding the earlier
exploratory equality replay whose exit status was not retained.

## Parser robustness

The exact-source campaign used 767 class seeds and ran for 300 seconds with seed
4851011501752864596. It processed 46,210,451 inputs: 469,369 accepted and 45,741,082
rejected, with 45,128 deterministic rechecks, a retained corpus of 1,024, and zero
failures. Source and seed hashes matched after execution. The input fingerprint was
`7f4a95ff4fcbcaac6aafb036ade0a43a4739e72ae3acbcdb2f1ae0a58b8bfd49`.

```text
cargo test -p crabspot-classfile --example fuzz_parser --locked
cargo run -p crabspot-classfile --example fuzz_parser --release --locked -- tests/fixtures --seconds 300 --seed 4851011501752864596 --failures target/coordinator/throwable-fuzz-campaign-v2/failures
```

Both commands exited successfully. These results concern parser robustness, not
coverage of Throwable runtime behavior or proof of full JVM conformance.

## Combined local verification

The initial combined gate passed 65 commands, including 1,679 debug workspace
tests, 1,599 optimized tests and verification of 7,519 ordinary platform classes
with 61,708 Code bodies and 4,685 bodyless declarations. Its 66th command, the
earlier public String replay, failed: the transport fixture returned `9/95/0`
instead of `9/127/0`. The modified Rust example contained a mojibake argument
instead of the original euro sign and rocket. The guest and oracle were correct.

The reviewed amendment restores that argument with ASCII Rust Unicode escapes and
adds a compiled regression asserting `[0x20ac, 0xd83d, 0xde80]`, the eight-argument
total of 37 units, and the nine-argument total of 39 units. A full audit of the 45
changed component files found no other non-ASCII Rust literal. Two README files
also received ASCII punctuation repairs. CI now explicitly executes the example
test before the existing String replay. No Java fixture, expected result, resource
counter or VM library implementation was changed.

The original failed evidence remains intact. The amended manifest has SHA-256
`EC90EBE3547A0C2F74FD0C10B30DAAD329A80B1BB62594C9DBA78038373847A3`:
four changed inputs and 1,385 unchanged inputs. The authorized continuation repeats
formatting, strict workspace Clippy, the explicit example test and release build,
then the full archived/rebuilt String replay and all 15 previously unrun commands.
Prior successful checks are inherited with their source dependencies unchanged;
they must not be reported as fresh debug or optimized runs on the amendment.

All 20 continuation commands passed, including formatting, strict workspace
Clippy, the new example test and its release build. The complete String replay
passed: 27 archived and 14 rebuilt API cases, with physical CLI checks on both
inputs. The original `9/127/0` guest result and 39 retained UTF-16 argument units
were restored without changing the oracle. String body/nest and equality/assertion
replays, Throwable reference checks and both public product platform runs passed.
All executed input hashes matched after the continuation.

The public product harness closed review findings concerning final input-hash
checks, zero-effect image failures and exact failure-location assertions. Its final
immutable sources passed review. The coordinator's Zulu and Temurin runs each
passed 48 archived/rebuilt API cases, 14 physical CLI cases and five profile controls.
The Zulu run also passed two incompatible OpenJ9-library controls. Both reports
record successful same-buffer input rechecks. The 65 inherited successful checks
and 20 continuation checks include 121 Python regression tests. Two existing Rust
tests ignored by the ordinary suites passed when explicitly selected; their
separate execution does not erase the recorded ignores.

The final CI amendment splits three paired-command steps into six single-command
steps without changing their commands or order. This preserves each native command's
failure status on Windows. It changes only CI scheduling after the local gate;
the other 1,388 frozen inputs remain unchanged. The director reviewed that amendment
separately. The first hosted Linux/Windows outcome is recorded below.

| Evidence | SHA-256 |
| --- | --- |
| Initial gate, retaining the discovered failure | `1BCD3235537B1A5CA92B59591FD0A058487CD8E18ADFB8B467B932C1872A2806` |
| Successful terminal continuation | `58B766FD3E7AE14AF1FB8E7945C8CFAF59C79386DC6AFC515FF7173D8FA3763D` |
| Final 1,389-input publication manifest | `E82E03A7CAEE58273DDED99790C06277D777E06F78F5FB9248B97CF2A9607D2B` |

The following commands reproduce the main Rust checks; fixture and public replay
commands are also recorded in CI and the linked reference report:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --release -p crabspot-verifier -p crabspot-runtime -p crabspot-interpreter -p crabspot-launcher
cargo test -p crabspot-launcher --example replay_string_entry --locked
```

The aggregate technical review authorizes publication of these exact inputs. This evidence
establishes the frozen Throwable increment, not full exception coverage, M2
completion, general Java compatibility or performance superiority.

## Hosted checkout packaging defect

Both jobs for published `eca0cc85dbddbb494d5673709fd68b5672fedf1e` failed at
the public Throwable reference tooling step. The exact-byte capture rejected
`projection.json` before the new public reference/product replay. The local
working file had 23 CRLF line endings when its hash was recorded; the repository's
existing `.gitattributes` stores and checks out LF text. The recorded hash therefore
described different bytes from the published artifact. Both JSON documents have
identical contents. [Linux failure](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42164245723/job/127716863265)
and [Windows failure](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42164245723/job/127716863643).

A local Git archive of the published commit reproduced the same failure. The
repair records the already-committed LF projection hash, then updates the enclosing
manifest and harness hash. It does not change any Java source, classfile, reference
result, runtime code or verification rule. Two added regressions check canonical
LF checkout and rejection of byte changes that leave JSON semantics unchanged.
Exact-byte verification remains enforced.

The targeted repair gate runs the reference and product harness tests on both the
working tree and canonical export, dual-VM and alternate-compiler references, and
both complete supplied-platform product sweeps using the unchanged verified Rust
binaries. All six unit/reference commands and both full product sweeps passed.
The unit groups ran 18 reference and 19 product tests on each tree (74 test
executions, including the two new regressions). Each product platform passed
48 API cases, 14 physical CLI cases and five profile controls. Zulu also checked
two incompatible OpenJ9-library cases. Both final reports retained the exact
previously verified Rust binary hashes and passed their same-buffer rechecks.

The first repair runner imposed a 600-second limit on the complete Zulu sweep and
timed out without a product assertion result. That interrupted run remains
preserved. After host process inspection confirmed no remaining matching child,
a separate two-command continuation completed the unfinished sweeps with a longer
limit and a distinct output directory. The six successful checks were inherited
with matching log and input hashes; no interrupted command is counted as passing.

All 1,389 current inputs and the canonical export matched after completion.
Continuation evidence SHA-256 is
`CF33E4DB1C406D0F21C576EE0C8EA35E0536C1451D09A3852CACC1F993E90F60`.
The director approved publication of the exact repair after independently checking
the full evidence chain, source inventories and unchanged Rust binaries.
[Canonical fixture amendment review](m2-throwable-canonical-fixture-review.md).
Replacement hosted CI remains pending. Original local and hosted failures remain
recorded; no hosted success is claimed for this revision.

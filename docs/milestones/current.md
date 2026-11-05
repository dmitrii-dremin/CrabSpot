# Current checkpoint: String literals, interning and arguments

The technical director approved this increment on 2026-11-05. [Technical review](../architecture/m2-string-constants-review.md).

The directory launcher accepts Java arguments after `--`, represents them as fresh Strings and passes a real `String[]` to the program. The runtime canonicalizes String literals and the result of the genuine `String.intern` native across defining loaders in the same heap. Static String constants are installed before the superclass initializer can read them. [Implementation contract](../architecture/m2-string-constants.md) and [Java 21 reference observations](../architecture/m2-string-reference.md).

On the original four-argument Java program, CrabSpot, HotSpot and OpenJ9 produced flags 127, one initializer call, one synchronized-counter increment and argument count four. CrabSpot proved 21 classes and created four argument Strings plus one new literal object. The superclass-initialization fixture returned `41/1/41/3`. Giving it only one literal site left all four observed fields at zero while retaining the first pooled literal. Two distinct constant-pool entries containing the same units shared one object, used two sites and consumed exactly 25 String work units.

The public Rust API reproduced the raw class containing 21,845 isolated low surrogates, distinguished canonically equivalent Unicode spellings and preserved a NUL followed by an isolated surrogate in an argument. A bytecode wrapper checked argument effects through the actual OS launcher, including whitespace, empty arguments, option-looking strings and a supplementary character. On Windows a separate process test passed an isolated surrogate intact. An invocation with an invalid String load category failed verification before argument creation. The broader program retains flags 3 and stops at its real `String.length` call at bytecode offset 68.

Reproduce the public replay with `python tools/verify_string_entry.py` and a Java 21 HotSpot JDK at `JAVA_HOME`. It tests 27 API cases on archived definitions and 14 on freshly rebuilt definitions, with physical CLI tests on both sets. The exact-source parser campaign used 721 seeds and ran for 300 seconds. It accepted 319,142 generated cases, rejected 30,260,714 and completed 29,864 deterministic rechecks with zero failures.

All 61 commands in the combined local gate passed, including formatting, strict workspace Clippy, 1,501 debug tests, 1,421 optimized tests and 50 Python tests. The 32 integrated source hashes matched again after the final public replay. On the Zulu 21.0.7 archive, whole-platform verification covered 7,519 classes and 61,708 method bodies. The two earlier hosted reference jobs for `3ab1bbc` also passed. [Linux](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42119677756/job/127568472735) and [Windows](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42119677756/job/127568441613).

The next proposal targets execution of the Java library's `String.length`, `String.isEmpty` and `coder` bodies against managed byte arrays. The two reference VMs agreed on UTF-16 lengths, while inspection of their supplied class files showed different String initializer dependencies. [Proposed scope](../architecture/m2-string-java-bodies-next.md).

The Java-body reference suite now captures 24 observations on each of two VMs, on archived and regenerated inputs. The program returned lengths `0/3/4/1/2` for empty, Latin-1, mixed, isolated-surrogate and supplementary Strings. A separate program calls only `isEmpty`. Nine encoded nest groups distinguish valid private access from missing, unlisted, wrong-package and different-host cases. The published CrabSpot build still stops on the first `length` at offset 33 and the standalone `isEmpty` at offset 2. [Reference programs and reproduction](../architecture/m2-string-java-body-reference.md).

## Earlier class-object checkpoint

The technical director approved the class-object increment on 2026-11-05. The verified directory launcher executes class literals and the genuine bootstrap `Object.getClass()`. Java code can compare the resulting references, store them in fields and arrays, pass them through methods and use them as locks. Repeated requests for the same represented type return the same object in one heap. Array rank and component type matter, and defining loaders distinguish ordinary classes with the same name. An explicit lock on a nominal class object reenters the same monitor used by a synchronized static method on its declaring class. [Implementation contract](../architecture/m2-class-mirrors.md), [reference observations](../architecture/m2-class-mirror-reference.md) and [technical review](../architecture/m2-class-mirrors-review.md).

The five-class Java application returned `127/3/1` for the identity checks, initializer counter and synchronized call counter. It used eight mirrors and nine distinct constant-pool class sites. It completed with one active monitor and recursion depth three. Limiting the heap to seven mirrors stopped `main` at offset 115 with fields `15/0/0`. Limiting the domain to eight class sites stopped the synchronized method at offset zero with fields `63/3/0`. The storage application returned `63/0` and the descriptor-free literal application returned 1. HotSpot and OpenJ9 produced the same Java effects on the normal programs.

The launcher now requests seven bootstrap imports. On the inspected Zulu 21.0.7 platform their transitive hierarchy comprises 16 executable definitions. An application still needs an allowed import to name a platform type directly. The fixture that attempts to name `GenericDeclaration`, a superinterface of Class, stops in the dependency stage. Calling `Class.getName()` on a mirror stops at the opaque-object boundary. An active static call on Class enters the supplied initializer and reaches the unbound native `registerNatives` at its first invoke. Both checks preserve the application's first field write. Real reflection, arbitrary native methods, catchable VM-created exceptions and concurrent synchronization remain open.

All 57 commands in the combined local gate passed. The fresh build passed formatting, strict all-target Clippy, 1,446 workspace tests, 1,366 optimized tests and 43 Python regressions. Both reference VMs reproduced all 78 observations on archived and fresh classfiles. The public CLI and Rust API each exercised 40 directory cases. Eight additional public API runs checked exact and one-less limits, zero budgets and maximum ceilings. All 36 frozen product inputs matched their SHA-256 values after the gate.

The six-class application from the earlier launcher checkpoint still returned `1/1/37/1/0`. Preparation proved 22 definitions, including 364 Code bodies and 60 bodyless declarations. Preparation used zero argument arrays and zero engine evaluations. Execution added exactly one of each. Independent whole-platform verification of the Zulu archive covered all 7,519 ordinary definitions, 61,708 Code bodies and 4,685 bodyless declarations.

Run the public scenarios and the reference suite with the following commands. The first command accepts an extracted platform through `--platform` or uses the Java 21 JDK at `JAVA_HOME`.

```text
python tools/verify_class_mirror_entry.py
python -m unittest discover -s tools -p test_class_mirror*.py
python tools/class_mirror_reference.py --check --compare-openj9 PATH_TO_OPENJ9_21_JAVA
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p crabspot-verifier -p crabspot-runtime -p crabspot-interpreter -p crabspot-launcher --release --locked
cargo run -p crabspot-classfile --example fuzz_parser --release --locked -- tests/fixtures --seconds 300 --seed 4851011501752864596 --failures target/mirror-fuzz-failures
```

The exact-source parser campaign accepted all 694 initial seeds. Across 31,728,634 generated inputs, it accepted 417,394, rejected 31,311,240, repeated 30,985 deterministic checks and retained 1,024 corpus inputs over 300 seconds. It recorded zero failures. The source and seed hashes matched after the campaign.

The previous reference revision `6d51f69` passed hosted checks on Linux and Windows. Each reproduced 78 HotSpot observations on both archived and regenerated inputs on its own Temurin 21 build. [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42110399479/job/127536110758) and [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42110399479/job/127536111089).

The proposed next increment covers canonical Java String literals, String constant fields, the declared String.intern native, and nonempty application arguments. [String proposal](../architecture/m2-string-constants-next.md).

The published class-object revision `5394da0` passed hosted checks on [Linux](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42116584062/job/127557720607) and [Windows](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42116584062/job/127557720432).

The String reference suite now exercises the frozen contract on Zulu HotSpot and OpenJ9. Original and rebuilt inputs produced 148 observations in four runs. The superclass initializer saw its subclass's two String constants, and separately defined classes shared the same literal. The controlled nine-argument UTF-16 probe returned `9/127/0`. The published CrabSpot baseline still stops on the descriptor-free literal at offset zero. The reference-only workspace passed formatting, strict Clippy and the full Rust test suite. The parser accepted 721 initial seeds and completed 10,000 mutations with zero failures. Seven Python regression tests cover the reference tooling. String execution remains in implementation. [Contract](../architecture/m2-string-constants.md) and [reproduction commands](../architecture/m2-string-reference.md).

## Prior checkpoint: single-thread monitors

The director approved single-thread monitor execution on 2026-11-05. The launcher executes synchronized Java methods and the two monitor bytecodes. A seven-class directory fixture enters a synchronized main, calls an inherited static method and two synchronized instance methods, and uses ordinary objects and arrays as locks. Its four retained fields read `1/2/1/9` on CrabSpot, HotSpot and OpenJ9. The program uses exactly 46 monitor operations. With a ceiling of 45 the engine reports the final return at bytecode offset 112 with all four writes visible. The public launcher and both reference JVMs also exercised raw bytecode cases that distinguish valid reentry, crossed releases, missing ownership and attempted release of another invocation's lock. All 52 local acceptance commands passed and all 98 frozen inputs matched after the gate. [Reference observations](../architecture/m2-monitor-reference.md) and [technical review](../architecture/m2-single-thread-monitors-review.md) and [next class-object proposal](../architecture/m2-class-mirrors-next.md).

The parser campaign on the expanded 592-class corpus ran for 300 seconds with seed 4851011501752864596. It accepted 400,993 of 13,206,553 inputs, rejected 12,805,560, performed 12,898 deterministic rechecks and retained 1,024 inputs with zero recorded failures. An initial corpus discovery run identified the intentionally invalid synchronized-constructor class as a rejected seed. Its bytes now use the project's `.class.bin` convention and the JVM reference runner restores the exact class filename in its own temporary directory.

The fresh local build passed formatting, strict all-target Clippy, 1,379 debug workspace tests, 1,299 optimized verifier/runtime/interpreter/launcher tests, and 34 Python regressions. The 10,000-input parser smoke accepted 82 cases and rejected 9,918 with zero failures. Zulu HotSpot and OpenJ9 each reproduced 62 observations on the original classfiles and 62 on regenerated inputs. A separate export using Git's canonical line endings reproduced all seven tool fingerprints and both JVMs' observations.

```text
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p crabspot-verifier -p crabspot-runtime -p crabspot-interpreter -p crabspot-launcher --release --locked
python -m unittest discover -s tools -p test_monitor_reference.py
python tools/monitor_reference.py --check --compare-openj9 PATH_TO_OPENJ9_21_JAVA
python tools/verify_monitor_entry.py
cargo run -p crabspot-classfile --example fuzz_parser --release --locked -- tests/fixtures --seconds 300 --seed 4851011501752864596 --failures target/monitor-fuzz-failures
```

Hosted checks passed the monitor revision `a1f8bd9` on both operating systems. Linux ran 4,065 Rust test executions and verified 7,615 ordinary platform classes. Windows ran 4,059 and verified 7,589. Both reproduced all 62 HotSpot observations on the archived and regenerated inputs, all 13 public CLI/API monitor scenarios and the four public API limit cases. With a ceiling of 45 monitor operations, both runners stopped at main bytecode offset 112 and retained the expected fields `1/2/1/9`. [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42106349807/job/127522079444) and [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42106349807/job/127522078921).

## Class-object reference checkpoint

The director approved the class-object programs and reference tooling for publication. All 132 frozen inputs matched after the gate. Zulu HotSpot and OpenJ9 each reproduced 78 observations on the archived inputs and 78 on rebuilt inputs. Five Python regression tests, strict Clippy, formatting and 1,379 workspace tests passed. The parser accepted all 694 corpus seeds and processed 10,000 mutated inputs with 77 accepted, 9,923 rejected, ten deterministic rechecks and zero failures.

Running the published pre-mirror executable on all 38 isolated application directories produced twelve failures during dependency checks, 24 during verification and two during main evaluation. This establishes the baseline for the pending implementation review. [Reference results and reproduction](../architecture/m2-class-mirror-reference.md).

## Prior checkpoint: verified application entry

The director approved verified directory application entry on 2026-11-05. The launcher ran a genuine Java 21 `main(String[])` from a class directory.
The six-class application created two implementations of an interface, invoked both and
stored 37. Its five observed static fields matched one call on both reference JVMs.
With an inherited main, launching the child ran the parent and child initializers in order
before the parent's main observed 12. The real-source reference checks ran on HotSpot and
OpenJ9. All 48 commands in the combined local gate passed and all 83 frozen source and fixture hashes matched after the run.
[Run the application](../../README.md#run-a-java-application), [technical review](../architecture/m2-verified-application-entry-review.md) and [next monitor increment](../architecture/m2-monitors-next.md).

The development check launched 15 scenarios through the command line and again through the public Rust API. A failure in the inherited main preserved both writes already made. A failure in the named child's initializer preserved the initialization trace of 14 and left the main-body counter at zero. A malformed unused sibling failed verification before the application ran. The reference controls load each application in a fresh process on Zulu HotSpot 21.0.7 and OpenJ9 21.0.13.0. The directory launcher eagerly checks every supplied application class, so it can reject an unused sibling that a reference launcher never loads.

Two additional explicit tests on the frozen launcher used the extracted Zulu `java.base` and six compiled application classes. The image read 7,519 ordinary platform definitions and one module descriptor. It proved and prepared the 16 classes admitted for execution, checking 222 method bodies and 16 bodyless declarations. Image preparation alone recorded zero argument arrays and zero engine evaluations. The public run recorded one of each and returned the five expected fields.

The parser campaign included all 541 fixture classes and used seed 4851011501752864596. In 300 seconds it processed 11,962,266 inputs, accepted 403,525, rejected 11,558,741 and repeated 11,682 deterministic checks. It retained 1,024 corpus inputs and recorded zero failures.

The fresh Windows build passed formatting, strict workspace Clippy, 1,326 debug workspace tests and 1,246 optimized verifier, runtime, interpreter and launcher tests. The four Python regression suites passed 27 tests. The 10,000-input parser smoke accepted 108 inputs and rejected 9,892 with zero failures. The two example boundary tests and two explicitly selected genuine-directory tests passed separately.

On the first hosted run for `7c85f76`, both operating systems stopped at the launcher reference tool's provenance check. The recorded hash of `fixture_source.py` used CRLF from the original working copy, while Git stored LF. A fresh export reproduced the discrepancy. The repair hashes Python tooling with Git's LF convention and rejects bare carriage returns. All 30 original Java sources, 19 archived class inputs and both JVM result files retain their original hashes. [Hosted run](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42098229949).

Both hosted jobs passed the repaired revision `eb2dd34`. Each ran all 15 launcher cases through the CLI and the public API using its own Temurin 21.0.13 platform. Linux recorded 3,906 Rust test executions and verified 7,615 ordinary platform classes. Windows recorded 3,900 and verified 7,589. Both reproduced the six-class application's five fields, the inherited main result, and all expected early and late failures. [Linux results](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42099581726/job/127498528283) and [Windows results](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42099581726/job/127498528230).

To reproduce the cross-VM observations and exercise both CrabSpot entry points, set `JAVA_HOME` to the primary Java 21 HotSpot JDK and run:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p crabspot-verifier -p crabspot-runtime -p crabspot-interpreter -p crabspot-launcher --release --locked
python -m unittest discover -s tools -p test_application_entry_reference.py
python tools/application_entry_reference.py --check --compare-openj9 PATH_TO_OPENJ9_21_JAVA
python tools/verify_application_entry.py
cargo run -p crabspot-classfile --example fuzz_parser --release --locked -- tests/fixtures --seconds 300 --seed 4851011501752864596 --failures target/application-entry-fuzz-failures
```

## Prior checkpoint: member resolution during execution

The technical director approved checked member resolution on 2026-11-05. The interpreter now resolves ordinary field and method references against the loader records of the referring class. When a field or method is inherited, its declaring loader supplies the other side of the signature agreement. A later call can reuse that result and select a different overriding body for its receiver. [Implementation contract](../architecture/m2-resolution-constraints.md) and [reference observations and limitations](../architecture/m2-member-resolution-reference.md).

- The 7 coordinator execution groups cover original compiled callers, raw mismatched instructions, actual receiver identities, late delegation, missing initiating records and the side effects preceding an interrupted write.
- The full native gate passed 44 commands, including 1,265 debug workspace tests, 1,178 optimized tests, two example boundary tests, 21 Python tool tests and all 93 source hashes. The fresh 10,000-input parser smoke reported 86 accepted, 9,914 rejected and zero failures. [Review and reproduction](../architecture/m2-resolution-constraints-review.md)
- Both reference VMs reproduced their own 119 observations. Comparing complete rows between the two VMs gives 97 identical rows and 22 differences.
- Both hosted jobs passed on implementation commit `0ea9c2a` with 3,710 Rust test executions per platform. Linux checked 7,615 ordinary Temurin 21.0.13 classes and 62,599 Code bodies. Windows checked 7,589 classes and 62,186 Code bodies. Each checked heap opened with one witness per class and zero terminal member entries. [Hosted Linux and Windows run](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42088521610)
- The local Zulu 21.0.7 archive supplied 7,519 ordinary definitions. Verification covered 61,708 Code bodies and 4,685 bodyless declarations. One heap admitted all 7,519 layout graphs and opened with a preparation witness for each definition, zero terminal CP sites and zero allocated Java objects. Its 10,935 distinct graph edges and 11,927 instance fields fit explicit corpus limits of 16,384 classes, 131,072 edges and 262,144 fields. The checked opening consumed 1,343,610 work units, 1,199,394 UTF-16 units and zero scratch entries.
- The expanded parser campaign started with 522 input classes. In 300 seconds at seed 4851011501752864596 it accepted 417,980 of 18,683,667 cases, rejected 18,265,687, repeated 18,246 deterministic checks and retained 1,024 cases with zero recorded failures.

Reproduce from a Java 21 HotSpot JDK and Rust toolchain using the commands below. The two optional OpenJ9 comparisons take the Java executable for the release in the corresponding manifests.

```text
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p crabspot-verifier -p crabspot-runtime -p crabspot-interpreter --release --locked
python -m unittest discover -s tools -p test_member_resolution_reference.py
python tools/member_resolution_reference.py --check
python tools/member_resolution_reference.py --check --compare-openj9 PATH_TO_JAVA
python tools/loading_constraint_reference.py --check --compare-openj9 PATH_TO_JAVA
cargo test -p crabspot-verifier --example verify_java_base --locked
python tools/verify_java_base.py
cargo test -p crabspot-classfile --example fuzz_parser --locked
cargo run -p crabspot-classfile --example fuzz_parser --locked -- tests/fixtures --iterations 10000
```

## Prior checkpoint: initiating loaders and preparation constraints

CrabSpot now records which loaders initiated each ordinary class. When
preparation requires two loaders to agree on a signature type, the runtime
enforces that agreement against both existing definitions and future loads.
A failed insertion preserves earlier successful obligations. The domain admits
a direct definition and its initiating record together. Both declared overrides
and inherited interface implementations use the actual declaring loaders.
[Contract](../architecture/m2-loading-constraints.md) and
[technical review](../architecture/m2-loading-constraints-review.md).

## Verification and reference behavior

- The fresh Windows gate ran 41 commands. It passed 1,181 workspace debug
  tests and 1,094 optimized interpreter, verifier and runtime tests, plus two
  separate example boundary tests. All 123 frozen file hashes matched after the run.
  The runtime increment adds 19 private test groups, 49 independent public
  tests and two corpus replay tests. Eleven provenance tests and five
  loading-reference tool tests passed.
- Against Zulu HotSpot 21.0.7, the main 85 cases cover early and late conflicts,
  delegation after a failed direct definition, transitive equality, selected owners,
  and per-signature rollback. Separate Rust tests enforce the specified outcome
  in all 27 cases across nine additional method-selection shapes. In ten of
  those rows, Zulu permitted a pair of distinct identities that SE 21 requires
  to agree. The 18 additional invocation probes identify which failures occur
  at a later stage. [HotSpot observations](../architecture/m2-loading-selection-reference-disposition.md).
- IBM Semeru OpenJ9 21.0.13.0 ran the same 130 probes. Eight of the 27
  selection rows differed from the required agreement. Both VMs also exposed
  differences in when they report a failure. The committed per-build tables
  preserve the actual stages. [OpenJ9 comparison](../architecture/m2-loading-selection-openj9-addendum.md).
- The installed Zulu java.base archive supplied 7,519 ordinary definitions.
  Each passed checked definition, modern type verification and preparation
  constraints using default per-class limits. The single bootstrap loader
  retained 7,519 initiating records with zero cross-loader unions. The verifier
  covered 61,708 Code bodies and 4,685 bodyless declarations. The module
  descriptor was checked separately.
- The parser smoke started from 498 class files. With seed 4851011501752864596
  it accepted 104 of 10,000 mutations, rejected 9,896, reproduced all ten
  deterministic rechecks and ended with 512 corpus entries and zero failures.

Reproduce with a Java 21 HotSpot JDK selected via JAVA_HOME. The optional
OpenJ9 replay takes the separate Java executable from the reference build
identified in the fixture manifest.

```text
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p crabspot-verifier -p crabspot-runtime -p crabspot-interpreter --release --locked
python -m unittest discover -s tools -p test_loading_constraint_reference.py
python tools/loading_constraint_reference.py --check
python tools/loading_constraint_reference.py --check --compare-openj9 <path-to-pinned-java-executable>
python tools/verify_java_base.py
cargo run -p crabspot-classfile --example fuzz_parser --locked -- tests/fixtures --iterations 10000
```

Full class loading, member-resolution constraints, execution-time proof checks,
and application startup remain open. The [next proposal](../architecture/m2-resolution-constraints-next.md)
connects loader agreements to actual field access and method invocation.

## Hosted preparation and class verification

The published implementation at `fa3f3c811abd3518af6957b14d25d4cc16f7b6c0`
passed both hosted jobs. Each runner completed 1,181 workspace tests before
and after fixture regeneration, 1,094 optimized tests and two example boundary
tests, totaling **3,458 Rust test executions per platform**. The 130 Temurin
21.0.13+9 reference cases reproduced the HotSpot results. Each 10,000-case
fuzz run started with 498 seeds, accepted 101 mutations, rejected 9,899 and
recorded zero failures after ten deterministic rechecks.

| Runner | java.base archive SHA-256 | Prepared and verified classes | Code bodies | Bodyless declarations |
| --- | --- | ---: | ---: | ---: |
| [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42079503453/job/127428520412) | `0f6d16706108b9f6382de8875b8cb1a3c22e559d14d6a4f1742e3d6efcbdb550` | 7,615 | 62,599 | 4,666 |
| [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42079503453/job/127428520137) | `573894d2cf4d7f19421e4696582532716dbfaad247c7427b9e44d233f1df98c2` | 7,589 | 62,186 | 4,694 |

Each archive yielded one bootstrap initiating record per ordinary definition
and zero cross-loader unions. Both jobs checked the module descriptor separately.

## Previous checkpoint: modern class type verification

The new class verifier checks each declaration in file order under a single
cumulative allowance and returns a proof tied to the immutable environment
and canonical definition. It checks version and preview eligibility even for
classes with no methods. A failing declaration identifies its method index,
preserves the original diagnostic, and accounts for work already performed.
[Class verification contract](../architecture/m2-class-type-verification.md).

The technical director approved the increment on 2026-11-05.
[Acceptance review](../architecture/m2-class-type-verification-review.md).
The next planning increment covers [loading constraints during preparation](../architecture/m2-loading-constraints-next.md).

## Class and JDK evidence

- All 174 ordinary definitions among the 175 javac fixtures passed the local
  aggregate check. The nine resource totals for each class matched the sum of
  independent method checks across 937 Code bodies and 31 bodyless declarations.
- Fifteen original class encodings produced sixteen JDK 21 stage outcomes. The
  same preview class was checked twice, with preview disabled and enabled.
  The stage oracle separates definition failure, reflective lookup, and a
  successful proof of a child's own methods when its parent contains bad Code.
  The earlier 57 method-stage cases also agree with their class-wide results.
- A separate reference harness checked every ordinary class in the installed
  Zulu 21.0.7 java.base module. All 7,519 classes passed, with 61,708 Code bodies
  and 4,685 bodyless declarations. The harness read the original archive,
  validated the module descriptor separately and cross-checked the proof counts
  against each class's metadata. The archive SHA-256 was
  `311fa7415ab8e88874b21b1db7f326b7ad94e7c77fd689489dcf21e9919f05c8`.
- Reproduce the staged oracle with `python tools/class_proof_reference.py --check`
  and the real JDK corpus with `python tools/verify_java_base.py` and JAVA_HOME
  set to a JDK 21 installation. Archive contents can vary by build and platform.

Whole-class checking applies to majors 51 through 65. Historical inference,
cross-loader constraints, VM linkage failure caching and consuming verification
evidence at execution remain open. The class checks exercise verification only.

## Fresh native gate

The Windows gate ran 38 commands on the frozen inputs with Rust/Cargo 1.98.1
and Zulu JDK 21.0.7. All passed, and all 47 source hashes matched again after
the run. There were 1,111 workspace debug tests and 1,024 optimized tests
(164 verifier, 436 runtime and 424 interpreter), plus separate example tests
for the java.base input reader and fuzz corpus limit. The 11 Python tests,
46 Java source digests and 23 reference sets passed their checks. The 10,000-case
parser smoke started with 407 valid seeds. It accepted 116 mutations, rejected
9,884 and reproduced all ten rechecked outcomes, with zero recorded failures.
The two raw fixtures that require preview opt-in or fail class-format admission
are kept separately from default-parser fuzz seeds.

## Hosted class verification

Both jobs passed at commit `158262d9e5b36ab2244e8f26391d74d42719b48c`.
Each ran 1,111 workspace tests before and after fixture regeneration, 1,024
optimized package tests and two example boundary tests, for 3,248 Rust test
executions per platform. Each parser smoke recorded 116 accepted and 9,884
rejected mutations with zero failures.

Both runners verified every ordinary definition in their Temurin 21.0.13+9
java.base archive and checked its module descriptor separately.

| Runner | Archive SHA-256 | Ordinary classes | Code bodies | Bodyless | Declarations |
| --- | --- | ---: | ---: | ---: | ---: |
| [Linux job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42072413723/job/127403592291) | `0f6d16706108b9f6382de8875b8cb1a3c22e559d14d6a4f1742e3d6efcbdb550` | 7,615 | 62,599 | 4,666 | 67,265 |
| [Windows job](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42072413723/job/127403592682) | `573894d2cf4d7f19421e4696582532716dbfaad247c7427b9e44d233f1df98c2` | 7,589 | 62,186 | 4,694 | 66,880 |

## Previous checkpoint: modern method verification

Updated 2026-11-05. The new verifier checks a selected method against immutable
loader-scoped hierarchy and member facts. It covers class-file majors 51вЂ“65,
including stack maps, exception edges, constructors and all modern instruction
transfers. The director approved this bounded increment for publication on 2026-11-05.
[Acceptance review](../architecture/m2-type-state-verification-review.md).
The next approved planning task is a [class-level type proof](../architecture/m2-class-type-verification-next.md).

## Corpus and reference outcomes

- All 935 genuine Code bodies and 31 bodyless declarations passed local method
  verification. The 174 javac classes include a module descriptor. The Code
  decoder walked 26,945 instructions across 175 opcode values.
- The original-bytecode suite supplies 69 class encodings and 57 JDK 21 cases.
  Twenty-nine reached invocation, 23 failed at reflective lookup with VerifyError
  and five failed definition for overriding a final method. Rust checks the
  selected declaration, including the overriding method in each definition failure.
- The older 18-case suite now distinguishes 12 structural failures and two type
  failures in the modern checker, with four methods accepted.
- The Zulu 21.0.7 reference supplied 122 JDK metadata projections, 614 fields
  and 3,250 methods. Rust checked ordered metadata against original JDK input.
  Only genuine methods and explicitly selected original test cases count as proofs.
  The probes deliberately omit one class named `absent/Missing`.
- Full class verification, historical inference, loading constraints and VM startup
  remain open. Performance comparisons require a separately reviewed suite.

Reproduction uses the commands in the project README, including
`python tools/type_state_reference.py --check`,
`python tools/type_verification_facts.py --check` and
`cargo test -p crabspot-verifier --release --locked`. The contract records the
JDK evidence and the accepted amendment for handler stack maps containing Top.

## Native gate results

Rust/Cargo 1.98.1 and Zulu JDK 21.0.7 passed all 35 commands from a fresh
target, including formatting, strict all-target Clippy, 1,071 workspace tests,
984 optimized tests (124 verifier, 436 runtime, 424 interpreter), and ten
Python fixture-provenance regressions. All 22 reference sets reproduced.
The input manifests and source bytes matched all 235 recorded hashes before
and after this run.

The first smoke run stopped at the harness's 256-seed limit. The revised harness
retains up to 512 inputs of up to 64 KiB each. A separate example test checks
both the final permitted seed and the first excess seed. The accepted smoke
command used all 392 seeds and seed 4851011501752864596. Of 10,000 mutations,
113 parsed and 9,887 were rejected. All ten deterministic rechecks agreed,
505 inputs remained in the corpus, and the run recorded zero failures.

The published implementation passed hosted Linux and Windows CI. Each runner executed 3,127 Rust tests across both fixture sets, optimized packages and the fuzz harness boundary. The Temurin 21.0.13 stage oracle matched the committed Zulu 21.0.7 outcomes. [Hosted run](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/42067751551).

## Previous checkpoint: whole-method Code validation

Updated 2026-11-04. The [contract](../architecture/m2-code-validation.md) is frozen;
production and independent-test components passed technical review. The final
fresh-build combined gate passes, and [bounded approval](../architecture/m2-code-validation-review.md)
authorizes publication after the director's independent evidence readback. Full M2 and full JVM conformance
remain open. Continue autonomously without NanoSpec.

## Implemented scope

The new standalone `crabspot-verifier` crate checks one selected method's entire
Code without execution or class resolution. It borrows immutable class/member/Code
metadata and owns one decoded instruction sequence with scalar resource usage.
Legal abstract/native declarations return no Code or decoded body. Existing
parser, registry, interpreter, session and launcher APIs gain no implicit gate.

The pass checks version/preview policy, selected name/descriptor and Code presence,
all instruction operands including unreachable instructions, constant-pool links
and loadable categories, versioned invocation tags, receiver-inclusive argument
slots, local ranges and array ranks. It validates exception-handler boundaries,
stack-map offset deltas and normalized forms, uninitialized allocation sites,
current local-entry widths and operand-stack slot capacity. MethodHandle target
kinds and 254/255-slot limits are tested independently from invocation operands.

Four inclusive quotas fund actual Code bytes, metadata entries, UTF-16 scans and
work. Multi-counter charges are atomic; successful compact name/descriptor
summaries are cached by actual Utf8 index and role. The current local-width buffer
is updated without expanding one full frame per bytecode offset or rescanning Same
frames. Opaque extensions retain the parser's location/version recognition policy.

Success remains a partial representation result. Stack/local type flow, joins,
return compatibility, missing required branch frames, reachability/fall-off,
constructor obligations, assignment/protected checks, linkage and execution
permission are not established. Bootstrap-table contents and ignored attribute
bodies are outside this pass. Historical parameterized void class initializers
remain explicitly unsupported. The unchanged decoder's input-bounded allocations
do not gain a new recoverable host-OOM guarantee.

## Independent evidence

All 168 genuine javac classes are traversed: 890 Code bodies, 31 legal bodyless
declarations, 25,236 instructions and 173 opcode values. Repeated validation checks
usage and decoded equality; input metadata remains unchanged. All 165 prior genuine
class hashes are preserved; three new classes belong to the JDK-only fixture driver.

Eighteen original encodings are separately identified and reproduced by a Java
emitter. The JDK driver reads their exact bytes with `-Xverify:all`, defines them,
forces verification by reflective method lookup, then invokes accepted methods.
Four valid cases return one. Twelve static defects fail at lookup and are rejected
by the partial Rust service. Two structurally valid type-state defects also fail
JDK lookup but deliberately pass this Rust stage. Definition succeeds for all 18;
it is not treated as proof of verification. Historical interface-reference cases
use majors 51/52; the other sixteen original files use major 65.

The increment adds 51 Rust tests: nine private guards, forty independent public
synthetic tests and two coordinator corpus/oracle tests. Private mutation coverage
checks 2,048 bounded cases twice. Adversarial tests include a reused 65,004-unit
descriptor, 10,000 Same frames after 30,000 local entries, exact independently
derived accounting, zero/exact/MAX limits, allocation/overflow guards and mutable
metadata transplants. Full reference execution remains 26,508 distinct rows;
the 18 new validation outcomes are recorded separately.

## Final native gate

The accepted increment uses fresh target
`target/verification-code-validation-final-abde6732caba`. Its `evidence.json` and
`logs/` record exact commands/results. All 45 frozen integration paths were hash
verified before and after. Final sources are production frozen-v1 (identical to
APIv2), independent public tests frozen-v2 and coordinator root-v1. The earlier
public-v1 gate (997 debug / 910 optimized) is preliminary and not acceptance evidence.

| Command/check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 998 passed, zero failed |
| `cargo test -p crabspot-verifier -p crabspot-runtime -p crabspot-interpreter --release --locked` | 911 passed: 51 verifier, 436 runtime, 424 interpreter |
| `python -m unittest discover -s tools -p test_fixture_source.py` | Eight passed |
| `python tools/check_fixture_sources.py` | 43 canonical Java sources and 21 reference records |
| `python tools/compile_fixtures.py --check` | 168 genuine classfiles reproduce |
| `python tools/construction_fixtures.py --check` | Nine previous original encodings reproduce |
| `python tools/code_validation_reference.py --check` | 18 original encodings and stage-aware JDK outcomes reproduce |
| Every `tools/*_reference.py --check` | All 21 reference sets reproduce |
| `cargo run -p crabspot-classfile --example fuzz_parser --locked -- tests/fixtures --iterations 10000` | Zero failures |

Local platform: Windows x86-64, Rust/Cargo 1.98.1, Zulu 21.0.7. Parser smoke uses
195 initial classfiles, seed 4851011501752864596 and 10,000 cases: 138 accepted,
9,862 rejected, ten deterministic rechecks and final corpus 256. This is seeded
parser mutation evidence, not a full verifier fuzzing campaign. Hosted CI for this
increment also passes as recorded below; previous records retain their named commit scope.

## Hosted verification

Implementation `08386b361be4992e4c02ef0931aff46185c809e3` passed complete
[Linux/Windows run 41867714340](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41867714340).
The coordinator inspected Linux job 126706186206 and Windows job 126706186887.
Both used Rust/Cargo 1.99.1 and Temurin 21.0.13. Each passed 998 tests against
checked-in fixtures, 998 after regeneration, and 911 optimized tests (424
interpreter, 436 runtime, 51 verifier), totaling 2,907 Rust executions per platform.

Both passed all workflow steps: formatting/strict lint, eight Python regressions,
43-source/21-record provenance, reproduction of 168 javac classes and 27 original
encodings, all twenty execution-reference sets and the new eighteen-case
verification-stage set. Parser smoke on both platforms used 195 inputs and seed
4851011501752864596: 10,000 cases, 138 accepted, 9,862 rejected, ten rechecks,
corpus 256 and zero failures. These hosted results independently match the native
smoke counts; they do not imply full-verifier or JVM acceptance.

## Decisions and remaining work

Review corrected wrong fixed ldc category ordering: reject the category before
following irrelevant Class/String/Handle metadata. Valid-path accounting is
unchanged. The director clarified three logical handler-boundary predicates,
including end equal to Code length, and selected-method scope without certifying
the enclosing class member count. Final independent review added twelve
MethodHandle/MethodType parameter-slot boundary cases. No source finding remains.

The next work is a separately reviewed full type-state verification contract and
implementation, followed by deliberate integration with class derivation and
execution admission. This increment does not complete M2. Full loader constraints,
VM exception construction, GC, Java libraries/startup, threads/JMM, JNI and JIT
remain open. Complete Java SE 21 conformance and demonstrated representative
performance superiority over HotSpot remain the ultimate acceptance conditions.

## Previous accepted checkpoint: session construction

Updated 2026-11-04. The [construction contract](../architecture/m2-construction.md)
is frozen, all components have passed source review, and the fresh-build
independent combined gate passes. [Bounded technical approval](../architecture/m2-construction-review.md)
is recorded; publication is authorized. M1 and the earlier bounded M2/runtime increments remain
approved; full M2 remains open. Continue autonomously without NanoSpec and ask
only when user intervention is necessary.

The ultimate objective remains complete Java SE 21 JVM conformance, required
observable compatibility with conforming JVMs, and demonstrated performance
superiority over HotSpot on a reviewed representative suite. Kotlin/Spring Boot
are integration milestones. No performance or full-conformance result is claimed.

## Implemented scope

InitializationSession executes new and constructor invokespecial using the shared
engine, scheduler, MethodId cache and heap. The effective session surface grows
from 194 to 195 opcode values. Other body profiles retain their construction gates.
The public Value/Reference representations remain unchanged; private locals and
operand values carry unfinished object aliases. Actual bootstrap Object constructor
Code runs, using an explicitly bound root identity rather than a production fallback.

New resolves/access-checks the target, rejects abstract/interface instantiation,
reserves publication capacity, funds an allocation-site scan, initializes the actual
class, then allocates and publishes once. Same-frame repeated-site unfinished stack
aliases fail; successful new invalidates older unfinished locals from that site.
Failed initialization/allocation retains prior effects without publishing a token.
Layout traversal uses incremental context work and existing heap quotas.

Constructor lookup preserves method-resolution/access/loading failures before the
resolved-owner rule; it does not inherit constructors or apply ACC_SUPER dispatch.
Private frame obligations distinguish Pending, Complete and Failed chains. A
successful this/super return promotes only its immediate caller's matching aliases;
escaping constructor exceptions invalidate only those unfinished aliases. Ordinary
helper exceptions caught before chaining leave Pending usable. Ordinary references
already escaped after an inner chain completes survive outer constructor failure.

Actual constructor frames may write final fields declared by their own class,
including through another lawful ordinary receiver. Before chaining, only the
current unfinished this may receive a write to its own declared field through its
own symbolic owner. Other value escapes remain rejected. Reference moves, legal
stack manipulation/discard and all four null/identity branches retain private tokens
without promotion. Separate ConstructionLimits fund alias scans; all existing
frame/slot/Code/fuel/handler/work/heap limits remain shared.

## Independent validation

The final integrated implementation passes two independent Rust oracle tests covering
184 distinct Java 21 rows: 112 genuine-source cases over 16 methods and 72 original
encoding cases over 9 methods, including 9 extra raw narrow-store boundaries. Every
retained field, hidden/final value, object/array type, element, alias/cycle edge,
static slot, admitted-class status and persistent root is checked separately from
JDK return values. Exact instance/slot/array/element/payload counts include failed
and unused allocations. The source loop allocates repeatedly at one new PC in one
frame; the sameSite factory case uses separate frames.

The corpus has 165 genuine javac classfiles (164 ordinary definitions and one module)
and 9 separately marked original class-file version 65 encodings. All 151 previous class hashes are
preserved. Provenance covers 42 Java sources and 20 reference records. Seven Python
regressions pass, including stale/missing/extra/corrupt synthetic artifact checks.
The raw JDK driver uses a child loader while Rust admits one defining loader;
these cases establish construction/value behavior, not cross-loader constraints.
The fixture Error constructors and reflection driver are outside evaluated Code.

All 55 frozen source/configuration/fixture hashes were rechecked after validation.
The final checks used a newly created build directory, not the superseded cache.

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 947 passed, zero failed |
| Optimized runtime/interpreter tests | 860 passed: 436 runtime + 424 interpreter |
| Distinct execution-reference rows | All 26,508 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 field cases match |
| Fixture/reference reproduction | 165 genuine classes, 9 original encodings and 20 output sets |
| Canonical provenance | 42 Java sources and 20 named reference records |
| Python provenance regressions | Seven passed |
| Expanded parser smoke | 10,000 cases, zero failures |

The increment adds 65 Rust tests: 21 public and 5 private runtime tests, 28 public and 9
private interpreter tests, and 2 root oracle tests. All 16 legal stack forms also run
with private aliases inside the existing stack-form test. New coverage includes
1,000 constructor frames, 10,000-class runtime traversal, raw metadata/authority
corruption, source/receiver identity, wrong roles, cache reuse, exact resource
boundaries, lifecycle failures and alias publication/invalidation atomicity.

The parser smoke used seed 4851011501752864596 with 174 initial classfiles:
119 accepted, 9,881 rejected, ten deterministic rechecks, corpus 256 and zero failures.
Local platform: Windows x86-64, Rust/Cargo 1.98.1, Zulu 21.0.7. Hosted construction
validation also passes as recorded below. Full M2 remains open.

## Review corrections and boundaries

The draft review corrected an overrestriction: unfinished references may use
ifnull/ifnonnull and if_acmpeq/if_acmpne, and pop/pop2 may discard them. Original
class-file version 65 probes independently confirmed these rules. Ancestor-constructor access
errors precede the later constructor-owner mismatch; before-super own-field writes
and a caught ordinary helper exception were also checked against the JDK.

Implementation review corrected failed-alias removal to one pass, avoiding an unfunded
second stack scan. Constructor field/return authority is hardened against corrupted
activation/method bindings, with regression coverage before final freeze. The
runtime's private final-store proof also rechecks constructor role and static/final
metadata. Final diff review also found and fixed an observable if_acmp error-order
regression: both typed operands must be extracted before handle validation. New
foreign-handle plus missing/wrong-kind operand cases preserve the old precedence.
No correctness failure was hidden by weakening a test. The genuine
registry corpus count changes from 150 to 164 because fourteen classes were added.

This is executed-path construction protocol, not the full verifier. Stack-map
merges, global control-flow proof and verification rejection timing remain open.
Constructed VM exceptions/wrappers, String constants, full loading constraints,
named modules/nests, native/synchronized execution, concurrent initialization,
threads, libraries, GC/JIT and application startup remain incomplete. Failed
allocations persist in the arena; no active-frame GC root API or reclamation is
claimed. Kotlin/Spring Boot cannot run yet.

GPT-6 Astra/high owns interpretation and review; GPT-6.1 Sol/high workers use
separate mutable worktrees/output directories. Root owns fixtures, integration and
independent evidence. Hash-checked atomic integration preserves Admin ownership.
The user authorized autonomous review and publication. Source review and the
combined gate are approved; inspected Linux/Windows validation also passes.
The next proposed direction is whole-method structural/code constraints followed
by type-state verification. Its exact contract requires a separate review.

A reused debug build directory initially omitted five new runtime unit tests
because copied snapshot timestamps predated the cached binary. Those results are
not acceptance evidence. The final gate uses a brand-new CARGO_TARGET_DIR and
rechecks the frozen source hashes; future snapshot integrations refresh mtimes.

## Hosted construction evidence

Implementation `ea7dc014352d46253fc5123b11dfeec04774eb34` passed complete
[Linux/Windows run 41859897401](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41859897401).
The coordinator inspected Linux job 126678491674 and Windows job 126678492033.
Both used Rust/Cargo 1.99.1 and Temurin 21.0.13 and passed every workflow step:
947 checked-in tests, 947 regenerated workspace tests and 860 optimized tests
(424 interpreter + 436 runtime), totaling 2,754 Rust executions per platform.
Seven Python tests, 42-source/20-record provenance, 165 reproduced javac classes,
nine original encodings and all 20 reference sets also passed.

Hosted parser smoke used 174 input classes and seed 4851011501752864596:
10,000 cases, 122 accepted, 9,878 rejected, ten rechecks, corpus 256 and zero failures.
This is the regenerated Temurin corpus; the native Zulu corpus produced 119/9,881.
Neither result is substituted for the other. No full-JVM or performance acceptance
is implied by the bounded construction approval or these CI results.

## Previous published checkpoint

Registered static-call implementation 8ef7f43 and evidence commit d3ad54f passed
[Linux/Windows run 41845390025](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41845390025).
Both inspected jobs passed 882 checked-in and 882 regenerated workspace tests plus
795 optimized tests (2,559 Rust executions per platform), strict formatting/lint,
151 reproduced fixtures, 19 reference sets and parser smoke. Those results belong
to the preceding static-call checkpoint and are not construction CI evidence.

## Reproduce

```text
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p crabspot-runtime -p crabspot-interpreter --release --locked
python -m unittest discover -s tools -p test_fixture_source.py
python tools/check_fixture_sources.py
python tools/compile_fixtures.py --check
python tools/numeric_reference.py --check
python tools/floating_reference.py --check
python tools/call_reference.py --check
python tools/array_reference.py --check
python tools/field_reference.py --check
python tools/instance_reference.py --check
python tools/virtual_reference.py --check
python tools/interface_dispatch_reference.py --check
python tools/special_reference.py --check
python tools/type_reference.py --check
python tools/reference_field_reference.py --check
python tools/reference_array_reference.py --check
python tools/multidimensional_reference.py --check
python tools/reference_call_reference.py --check
python tools/exception_reference.py --check
python tools/static_reference.py --check
python tools/initialization_reference.py --check
python tools/cross_static_reference.py --check
python tools/construction_fixtures.py --check
python tools/construction_reference.py --check
cargo run -p crabspot-classfile --example fuzz_parser --locked -- tests/fixtures --iterations 10000
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH, set
CARGO_INCREMENTAL=0, and use separate CARGO_TARGET_DIR and TMP/TEMP directories.
Coordinator final output is the fresh target/verification-construction-final; worker outputs are
target/construction-runtime-worker and target/construction-interpreter-worker.

# Throwable reference-only fixtures

This pack records original Java 21 behavior for a prospective exception contract.
It is not CrabSpot runtime execution, a product compatibility claim or permission to
implement that contract. No reference JVM is used by application execution.

`src/` contains the independent guest and `classes/` contains only its four own
class files. `controls/` contains host observers and the platform identity reader;
those compile privately during replay and are never archived as guest classes.
There are no vendor class bytes, private disassemblies or compiled observer classes
in this pack. The private development inventory lives outside the publication tree.

The 20 guest observations cover a user RuntimeException subclass with exact UTF-16
message (including NUL and a lone surrogate), cause attachment, same-object catch
and rethrow, explicit trace refill, supplied constructor replacement, an actual
constructor division fault, two fresh faults each from idiv/irem/ldiv/lrem, a
mismatching earlier catch, retained writes and MIN/-1 controls. The observer calls
real Java getClass/getMessage/getCause/getStackTrace/fillInStackTrace APIs. Its
successful Java stack materialization is reference evidence; the proposed CrabSpot
profile may explicitly stop at a later unsupported Java dependency.

The seven independent filter controls distinguish a lower unrelated Throwable
constructor, an ordinary constructor, a synchronized delegating override, a bypass
override, writable=false, a covariant override with javac bridge, and a same-receiver
int-parameter helper. Both pinned VMs retain unrelated constructors, execute the
overrides and release their monitors. Zulu filters the leading same-receiver
fillInStackTrace name prefix (including covariant/bridge and overload methods), then
its same-object constructors. OpenJ9 retains the override/helper and constructor
frames in those controls. These are separately pinned diagnostic observations, not
a universal Java override or dispatch rule. The bypass and writable=false controls deliberately yield zero traces. The
Throwable API also permits frame omission, including empty traces in some
circumstances: our ordinary normal/-Xint runs declare a nonempty diagnostic envelope,
not a universal Java conformance rule.

`portable.json` separates public class/identity/control-flow/effect gates from the
declared observed nonempty/top-method reference envelope. Neither that envelope nor
our pinned filtering details claim every conforming JVM must expose those frames. `observations/` and `filters/` hold exact
build diagnostics, including lines, trace sizes, messages and filtering. On the
recorded ru_RU Windows host, Zulu produced `/ by zero`; OpenJ9 produced Russian
`деление на ноль`. The language specification does not fix this message. Do not
promote that diagnostic difference into an exception-class/control-flow difference.

Strict replay uses the pinned Zulu 21.0.7 compiler/runtime and Semeru/OpenJ9
21.0.13.0.3 runtime, identified by version plus all nine actual Throwable/library
resource hashes. Compiler flags, original source/guest bytes, rebuilt guest bytes,
collector source, runtime executable/module hashes and commands are recorded.
All exact source, observer and original-class bytes are captured and authenticated
before any subprocess, including version queries. Only fixed captured paths are
compiled or executed. Later live changes abort without supplying subprocess input.
Both original and rebuilt guests run in normal and -Xint modes; their observed
envelope must match within a vendor. Semantic/envelope values agree across vendors.
Shared fixture_source requires canonical LF Java bytes; tool hashing normalizes
CRLF to LF and rejects bare CR, matching existing reference tooling conventions.
Recorded JSON is always written as exact UTF-8/LF bytes.

From the repository root, with JAVA_HOME selecting a Java 21 JDK:

```powershell
python tools/throwable_reference.py --check
python tools/throwable_reference.py --check --java "$env:JAVA_HOME/bin/java.exe" --javac "$env:JAVA_HOME/bin/javac.exe" --compare-openj9 "$env:OPENJ9_JAVA" --work C:/path/to/ignored/throwable-check
python tools/throwable_reference.py --check --portable-primary --java "$env:JAVA_HOME/bin/java.exe" --javac "$env:JAVA_HOME/bin/javac.exe" --work C:/path/to/ignored/throwable-portable
python tools/test_throwable_reference.py -v
```

`--fixtures DIR` permits moving the replay tool into a shared tools directory.
Strict checking preserves the exact recorded diagnostics; `--portable-primary`
checks only the portable requirements on a Java 21 primary and records its actual
identity, without certifying another library's recipes or pinned diagnostics.
Only the pinned compiler version and executable require exact rebuilt byte hashes.
An unpinned Java 21 compiler must reproduce the complete class inventory and the
observed envelope; its different bytes are recorded as different, not relabelled
as identical. Default --check resolves java/javac via JAVA_HOME, uses the shared
root tests/fixtures/throwable path and a unique workspace target/fixture-tool temp
directory. Explicit tool, fixture and retained work paths remain available.

Normal repeated recording validates source/tool pins and all historical observations
and role pins, then leaves the archive unchanged. --reviewed-source-update REASON
is an explicit reviewed source/tool migration; it records the superseded manifest
and reason and still requires every prior observed row and role pin to agree before
writing. A deliberate semantic recording change needs a separate reviewed archive.
The 18 provenance groups cover input/output mutation, original class/observer
separation, line endings, unpinned compiler rules and real pre-Java CLI rejection.
Real-JDK tests mutate live source/classes and add an extra source mid-run, inspect
actual subprocess paths/bytes, verify immutable staged execution and final refusal,
reject replacement of self-consistent changed historical rows, and exercise the
default shared entry with a unique cleaned-up workspace temp directory.

The owned guest disassembly establishes fault BCIs 8 for all four arithmetic helpers
and 18 for VMFailingCtor.<init>; Java StackTraceElement reports source lines rather
than BCIs. The original compiled guest bytes and source pin these locations without
publishing a private disassembly. Constructor-time capture and explicit refill have
distinct real top methods. A thrown replacement preserves its original capture;
previously escaped constructor state and writes remain visible.

The guest deliberately contains AssertionError guard references and an earlier
NullPointerException negative catch. A future launcher contract must explicitly
supply their metadata/import closure or author a separate public entry; these CP
dependencies must not be silently hidden to make this reference guest admissible.
The shared tools/fixture_source.py is an unchanged dependency supplied here only
for isolated replay; integration reuses the repository's existing helper.

Raw four-run diagnostic equality is enforced only for the strict pinned path.
Portable/unpinned runs compare the reduced semantic/observed envelope individually;
deeper frames, source filenames/lines, trace size and VM message variation may
change without being promoted into false failures. Changed effects or required top
method remain failures, and the strict path still rejects raw diagnostic changes.

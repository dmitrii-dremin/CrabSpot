# Public operand-exception fixture

Seven genuine Java21 sources produce eight guest classes; all five operand helper
sources/classes are byte-identical to the retained private reference. The two new
entries use actual catches, class/cause/fresh/rethrow/refill identity and retained
state checks. ObserveOperandMain is external reference code; neither its source
nor compiled class belongs on the CrabSpot guest classpath. No vendor classes are
archived. Sources and every metadata/tool artifact use exact canonical LF.

Expected 17-message vectors in message-oracles.json are independent policies. The
guest receives 34 real String arguments (null/text tags plus values). Reference
launches use strict ASCII N/T+four-hex-unit transport decoded only by the external
observer, preserving null, empty, NUL and unpaired UTF16 units. Four deliberate
wrong-message controls must fail the actual guest guard. The preserved Windows
native-launcher best-fit witness is a diagnostic observation, not an OS argument
fidelity claim.

The Zulu21.0.7 compiler with --release21 -g -encoding UTF-8 must reproduce all
archived guest and observer bytes. Other Java21 compilers require exact output
inventories, fixed javap opcode/descriptor/BCI controls and the full behavioral
matrix; their class bytes may differ. The verifier is read-only: it never updates
recorded expectations. It captures fixed bytes before tools and stages those very
buffers, then checks fixed source/class/control/metadata/tool inventories between
subprocesses and complete runtime/JDK inventory hashes afterward.

The full matrix is archived/rebuilt x HotSpot/OpenJ9 x normal/-Xint, with five main
guards and six uncaught/refill/null controls (88 observations), plus24 independent
ASCII codec boundaries. The explicit locale flags are ru/RU and UTF-8. OpenJ9's
pinned Russian arithmetic wording is a declared runtime/locale oracle; an
unmatched build/locale fails rather than deriving expectations from the caught
value. HotSpot CCE decoration and the CrabSpot fixed sentence remain distinct.
The null case is a reference NPE control; product NPE materialization remains
unsupported.

reference.json is the independently fixed required observation matrix; fresh
actual commands/results are emitted in the replay report. source-projection.json
and provenance/prototype-v4-manifest.json preserve the immutable candidate and
historical transform declarations. Their original status strings are historical.
The check needs only checked-in files plus explicitly supplied Java21 tools.

The run-row label `zulu` identifies the declared primary HotSpot message policy.
The report records the actual VM/compiler versions and binary/tree hashes even
when a supplied tool is Temurin. These labels make no universal diagnostic claim.

CI may explicitly select --single-primary:44 behavior+12 codec checks cover only
the supplied actual HotSpot primary, using the pinned primary message policy.
The secondary runtime is explicitly untested in that report. Local dual-VM
acceptance continues to require --compare-openj9 and88+24 checks. Python run()
keeps its dual default; a single-primary call must opt in and cannot also supply
a secondary. Fresh scope is selected by invocation, never inferred from rows.
validate_report requires an external expected_scope and checks exact runtime
identity keys, declared counts and missing/extra/duplicate/mixed rows. Committed
reference.json always retains its complete dual matrix under capture validation.
Unsupported actual-runtime diagnostic wording fails rather than changing oracle
text or removing checks. prepare/capture/product argument APIs remain unchanged.

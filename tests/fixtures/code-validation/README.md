# Original Code-validation encodings

These 18 files are emitted by `CodeValidationCases.java`, not by javac. Sixteen
use class-file version 65; the interface-reference comparison uses versions 51
and 52. The source and exact bytes are bound by `manifest.json`.

Run `python tools/compile_fixtures.py`, then
`python tools/code_validation_reference.py --check` with a reference JDK 21.
The driver reads these emitted bytes in an isolated child loader with
`-Xverify:all`, then forces verification through reflective method lookup before
invocation. The oracle records the stage and error category, not VM diagnostic
wording. All current cases define successfully; mere definition is not credited
as verification. This driver is test tooling only.

Four cases succeed through invocation. Twelve malformed operands, local ranges,
handler/frame boundaries or uninitialized offsets are rejected at lookup by the
JDK and rejected by CrabSpot's partial validator. The final two cases have
well-formed static structure but invalid type state: the JDK rejects them and the
partial validator deliberately accepts them. Successful static validation is
not type verification or permission to execute.

`DeadInterface52` deliberately names a nonstatic interface method in an
unreachable invokestatic. Its operand tag is legal at version 52; target
staticness belongs to resolution/execution. Version 51 rejects the operand tag.
`UninitValid` describes an unreachable frame containing an unfinished reference;
its recorded allocation site is an actual new instruction. The corresponding
negative points at pop. These cases prevent execution policy or reachability
from replacing whole-method representation validation.

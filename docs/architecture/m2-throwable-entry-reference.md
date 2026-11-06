# Public Throwable application reference

The [public application fixtures](../../tests/fixtures/throwable-entry/README.md)
give the Throwable implementation an independent application-level target.
They contain nine genuine Java 21 guest classes and a separately compiled
reference observer. The observer is never part of the CrabSpot guest directory.

`app.ThrowableMain` must complete with `65535/8/4/1` in its flags, arithmetic-case,
local-catch and completion fields. Its sixteen checks cover message/cause identity,
valid cause initialization, explicit catch/rethrow, trace refresh, retained
constructor effects, eight outer-handler and four same-frame arithmetic faults,
nonexceptional numeric boundaries, four Throwable constructor forms, empty
suppression and reentrant synchronized refill. A separate argument changes the
expected flag mask; the application must then throw the same retained user object
with completion still zero. The final refresh determines that object's current
trace head.

Three further entry points expose an uncaught division fault, a refilled user
exception and a Java stack-trace request. Reference VMs report the original
`idiv` helper for the arithmetic fault and the `refill` helper after explicit
refresh. Their stack-trace request succeeds. The current implementation contract
requires CrabSpot to stop at the actual unsupported `initStackTraceElements`
population native, retaining earlier allocations and captured state; reference
success does not authorize a fabricated successful result.

The original historical `ThrowableGuest` source and four class files remain
unchanged. The public `ThrowableEntryGuest` is a separately named, genuinely
compiled projection with exactly three declared source transformations: the
class rename and two primitive minimum-value substitutions. This removes unused
Integer/Long class-pool references retained by javac despite constant inlining.
The new main similarly uses primitive expressions for floating boundary controls.
Independent disassembly checked that all nine public guest class inventories fit
the frozen public import scope. This is an explicit fixture projection, not a
replacement or relabeling of the historical reference program.

The pinned Zulu and OpenJ9 runs reproduced 40 observations from archived and
freshly compiled guests in normal and interpreted modes. The portable harness
also passed 20 observations with the actual Temurin 21.0.13 compiler/runtime.
The pinned compiler reproduced guest and observer bytes exactly. The alternate
compiler was checked for exact class inventory and required observed behavior.
These results establish the reference target, not acceptance of the VM feature.

All 16 harness regressions passed. They include extra/missing inputs, changed
source/class/observer bytes, altered captured buffers, invalid projection changes,
reference guard and trace mutations, compiler identity, and a real reference run
that leaves the archive untouched. A subprocess mutation regression verifies
that staged input changes during compilation are rejected before any VM runs.
The technical director independently reran the tests and reproduced that rejection.
The source decision is recorded in the [technical review](m2-throwable-entry-reference-review.md).

The replay authenticates the exact captured buffers, verifies staged inputs
before and after compiler subprocesses, and rechecks live and staged inputs after
execution. The recorded archive and historical manifests, reference rows and
tool sources are pinned. The public tool has no recording or update mode.

Set `JAVA_HOME` to a Java 21 JDK and run:

```text
python -m unittest discover -s tools -p test_throwable_entry_reference.py
python tools/throwable_entry_reference.py --check
python tools/throwable_entry_reference.py --check --compare-openj9 PATH_TO_OPENJ9_JAVA
```

The CI workflow includes the portable primary-JDK replay and provenance tests.
Hosted results for this new public suite remain pending publication. Runtime and
interpreter source review and the combined exact-source product gate remain
separate requirements. Full M2 and full JVM conformance remain open.

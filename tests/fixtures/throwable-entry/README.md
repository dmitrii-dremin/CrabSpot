# Public Throwable application

These nine original Java 21 guest classes exercise real Throwable constructors,
message/cause identity, explicit throw/catch/rethrow, refill, escaped constructor
effects, integer division/remainder exceptions and synchronized capture.
`app.ThrowableMain` must return with `flags/arithmeticCases/localCases/completed`
equal to `65535/8/4/1`. Any argument deliberately selects a wrong expectation and
must throw the same retained user exception, with `completed=0`.

`ThrowableUncaught` exposes an uncaught VM arithmetic fault. `ThrowableRefilled`
exposes the current user capture after refill. `ThrowableTraceAccess` invokes the
real Java stack-trace API: reference JVMs return a nonempty trace; this increment
of CrabSpot must stop at the still-unimplemented stack-trace population native.
Reference success does not establish product support for that API.

The public `ThrowableEntryGuest` is a separately named projection of the preserved
historical `throwable/src/probe/ThrowableGuest.java`. Its exact three source
transformations are recorded in `projection.json`: a class rename and primitive
literal replacements for Integer/Long minimum values. Javac otherwise retains
unused wrapper Class constants, which exceed this increment's public import
set. The historical source and class files remain unchanged. The new application
also uses primitive literals for boundary and floating-infinity controls.

`controls/ObserveThrowableMain.java` is compiled separately and never included in
the guest directory. Its printing and stack inspection run only in reference
JVMs. Archived HotSpot and OpenJ9 observations cover both original and rebuilt
inputs in normal and interpreted modes: five cases per run, 40 observations.
The reported trace heads are observations under those conditions, not a claim
that every JVM must retain every frame in every execution mode.

With a Java 21 JDK selected by `JAVA_HOME`, run:

```text
python tools/throwable_entry_reference.py --check
python tools/throwable_entry_reference.py --check --compare-openj9 PATH_TO_OPENJ9_JAVA
python -m unittest discover -s tools -p test_throwable_entry_reference.py
```

The checker captures and authenticates fixed source, observer and class buffers
before spawning tools, rejects extra inputs, and checks every input afterward.
The exact pinned compiler must reproduce guest and observer class bytes. Other
Java 21 compilers must produce the same class inventory and observable results.
The checker is read-only and has no recording/update mode. `harness.json` pins
the replay tool, archive manifest, historical manifest and recorded observations.
No vendor class files or compiled observer classes are distributed here.

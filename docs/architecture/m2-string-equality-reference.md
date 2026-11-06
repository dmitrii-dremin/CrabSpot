# String equality and assertion policy on two Java 21 VMs

The application calls `Object.equals` ten times through one bytecode site, with String receivers. It checks identity, null, a non-String object, distinct equal and unequal Latin-1 and UTF-16 strings, different lengths and coders, and two distinct empty strings. The independent Java observer constructs arguments from numeric UTF-16 units, including an isolated surrogate and a NUL. The pinned HotSpot and OpenJ9 JVMs returned flags 511 and counted all ten calls. HotSpot also returned `511/10` with compact strings disabled. [Java inputs and hashes](../../tests/fixtures/string-equality/manifest.json)

Twenty fresh JVM launches per pinned VM exercise ten assertion settings on archived and rebuilt application classes. Two extra HotSpot runs exercise the noncompact configuration. With system assertions enabled both VMs reported that the bootstrap String and Object classes desired assertions, while application assertions remained disabled. Each observer also loads a fresh application class in two independent class loaders. Changing its class-specific policy after initialization changed the next `desiredAssertionStatus` query. The compiled `assert` statement continued using the decision captured when that class initialized. [HotSpot observations](../../tests/fixtures/string-equality/hotspot.json) and [OpenJ9 observations](../../tests/fixtures/string-equality/openj9.json)

The pinned builds differed on some private or implementation-specific observations. With system assertions enabled, both reported that the StringLatin1 helper desired assertions. Its stored assertions-disabled field was false on HotSpot and true on OpenJ9. The same option yielded different assertion query results for array classes. The harness records both results. On an unpinned HotSpot build it checks the public application effects, class loader identities and per-class policy changes. It consults the actual class bytes supplied by the running VM before enabling private observations.

The preceding CrabSpot launcher stopped at the actual virtual call in `TextEquality.equal@10`. The bootstrap assertion program called `Class.desiredAssertionStatus` and stopped at `main@6`. The application with its own Java `assert` stopped earlier during dependency collection because constant-pool entry 26 names `java/lang/AssertionError`. The implementation proposal covers the genuine Java equality, initialization and bootstrap assertion paths. [Next implementation proposal](m2-string-equality-next.md)

Run the following commands with a Java 21 HotSpot JDK at `JAVA_HOME`. The additional portable replay exercises the public-only observer on a pinned build. The OpenJ9 comparison requires the pinned Java 21 runtime.

```text
python -m unittest discover -s tools -p test_string_equality_reference.py
python tools/string_equality_reference.py --check --compare-openj9 PATH_TO_OPENJ9_21_JAVA
python tools/string_equality_reference.py --check --portable-primary
```

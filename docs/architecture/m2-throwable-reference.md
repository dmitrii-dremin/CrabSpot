# Throwable and VM exception reference behavior

The reference pack records the Java behavior for the proposed Throwable and VM exception increment. Its independent guest has four archived Java 21 classfiles. Separate observers inspect exceptions and runtime class identities. The technical director approved the pack and its integrated checks for publication. [Implementation proposal](m2-vm-exception-construction-next.md), [review](m2-throwable-reference-review.md) and [fixture details](../../tests/fixtures/throwable/README.md).

Twenty observations cover user construction, message and cause identity, catch/rethrow, explicit stack refill, constructor replacement and retained escaped objects. Each of `idiv`, `irem`, `ldiv` and `lrem` produces two fresh ArithmeticExceptions with a zero divisor. Earlier writes survive, the mismatching catch is skipped and the instruction after the fault is not executed. Signed minimum divided by minus one and the corresponding remainder retain their specified normal results.

Seven further controls inspect trace filtering and monitor behavior. They distinguish an unrelated constructor lower in the stack, delegating and bypassing overrides, disabled writable traces, covariant returns with a compiler bridge, and a same-receiver overload. Zulu and OpenJ9 differ in which override and constructor frames they retain. They agree on the tested identity, state and control-flow effects. The recorded normal and interpreted runs define an observed trace envelope. Java permits frame omission, so the exact trace rows have a separate diagnostic role.

The pinned Zulu 21.0.7 and Semeru/OpenJ9 21.0.13 runtimes each ran archived and rebuilt guests in normal and `-Xint` modes. Those eight runs produced 216 observations. The actual Windows Temurin 21.0.13 compiler and runtime also passed all four primary runs, producing 108 observations through the unpinned-compiler path. VM message text is recorded as UTF-16 units. The pinned HotSpot run produced `/ by zero`, while the OpenJ9 run used a localized Russian message.

The replay captures and authenticates source, observer and original class bytes before starting a subprocess. Compilation and execution use those captured bytes. Repeated recording preserves historical rows and performs no archive writes. Canonical text hashing supports Git line-ending conversion. An unpinned compiler must reproduce the guest class inventory and observed semantic envelope. Exact diagnostic comparison applies to the matched compiler/runtime path.

All eight integrated checks passed. Formatting, strict workspace Clippy and 1,598 workspace tests passed using the existing coordinator build cache. The 18 provenance regressions passed, including real-JDK mutation and standard-entry tests. The expanded parser smoke accepted all 758 initial seeds, then processed 10,000 cases with 92 accepted, 9,908 rejected, ten deterministic rechecks and zero failures. All 18 integrated source and CI inputs matched their frozen hashes afterward. [Exact audit](m2-throwable-reference-review.md).

Set `JAVA_HOME` to a Java 21 JDK and run from the repository root.

```text
python tools/throwable_reference.py --check
python tools/throwable_reference.py --check --compare-openj9 PATH_TO_OPENJ9_21_JAVA
python -m unittest discover -s tools -p test_throwable_reference.py
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p crabspot-classfile --example fuzz_parser --locked -- tests/fixtures --iterations 10000
```

CrabSpot's real Throwable construction, capture and VM-produced exception objects remain planned work under the next contract. Public Java stack-trace materialization also needs native StackTraceElement population, loader/module services and array cloning. Full M2 and JVM conformance remain open.

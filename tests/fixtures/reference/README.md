# Numeric reference outcomes

arrays.tsv records 280 primitive-array cases, with provenance in arrays.json.
The heap-service integration uses the 151 roundtrip cases across all eight kinds.
The interpreter integration executes all 280 cases, including aliasing, loops,
lengths, defaults and null/bounds/negative length exceptions. Generate/check with
tools/array_reference.py. Reference arithmetic NaNs are normalized; other floating
values retain exact bits. This table alone does not establish opcode support.

calls.tsv and interface_calls.tsv add 115 class-call and 11 interface-call cases,
including recursion, mixed argument categories, overloads and void return.
Generate/check both with tools/call_reference.py. Their JSON files record hashes
and reference-runtime provenance; Rust evaluates all 126 cases independently.

floating.tsv adds 7,263 independently executed float/double cases, with provenance
in floating.json. F:/D: input and non-NaN result values contain exact hexadecimal
IEEE bits. Arithmetic/conversion NaN results use F:NaN or D:NaN because payloads
are implementation-dependent. All finite/infinite results and signed zeros are
compared exactly. Generate/check with tools/floating_reference.py; run the
floating_reference Rust integration test in both debug and release profiles.

numeric.tsv records 616 independently executed Java 21 cases for the pure methods
in NumericCases.java. The four tab-separated columns are method name, descriptor,
typed input values and expected result. I:/L: distinguish int/long computational
values; V denotes void; E:java/lang/ArithmeticException denotes the expected
uncaught arithmetic exception. Numeric provenance and hashes are in numeric.json.

Generate or check them from the project root:

```text
python tools/compile_fixtures.py
python tools/numeric_reference.py
python tools/numeric_reference.py --check
cargo test -p crabspot-interpreter --test numeric_reference
```

Only the development reference script runs Java. The Rust execution kernel
independently evaluates parsed instruction streams and does not call Java, a
native JVM library, or these reference results at runtime. Ordinary cargo tests
use checked-in results, so they do not need a JDK. CI first tests checked-in
artifacts, then regenerates fixtures/results using its selected JDK and retests.

Cases cover overflow, signed division/remainder (including zero and minimum
values), masked shifts, bit operations, narrowing, conversion, long comparisons,
dense and sparse switches, loops, mixed category-one/two arguments, wide iinc,
constant-pool loads and void return. They establish this execution kernel's
supported subset, not Java class invocation, initialization or full application
compatibility.

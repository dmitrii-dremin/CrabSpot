# M2 floating-point increment review

Date: 2026-11-03.

**The floating-point execution increment is approved. The bounded same-class static-call increment in `m2-call-stack.md` is authorized. Full M2 remains open.**

## Review outcome

The director reviewed the changed value representation, category-two frame handling, all floating execution paths, unit tests, all 18 public synthetic tests, reference integration and opcode documentation. No blocking findings remain within the increment's contract.

Float and Double preserve binary representations during constants, locals, stack manipulation and returns. Java comparisons use numeric ordering rather than Rust Value equality. Arithmetic executes at the specified width; floating divide-by-zero does not produce ArithmeticException. The implementation covers signed-zero negation, truncating remainder, NaN comparison polarity, saturating/truncating integer conversions and direct long-to-float conversion. Double participates in the same local-overlap and stack-slot invariants as Long.

Tests exercise exact non-NaN representations, NaN classification, subnormal boundaries, gradual underflow, overflow, rounding ties, integer conversion thresholds, all fixed/indexed/wide local forms and mixed argument layouts. A long-to-float counterexample distinguishes direct conversion from an incorrect intermediate double conversion. Separated multiply/add tests distinguish instruction-by-instruction rounding from a fused operation. Invalid operand kinds, constant categories, returns, local bounds and stack capacities retain structured errors.

The implementation adds 53 effective opcode values, bringing supported numeric execution to 147, with nine wide forms. The current table is in `crates/interpreter/README.md`. All defined instruction encodings remain decoder-supported independently of this narrower execution set.

## Final evidence

The coordinator independently checked the frozen source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 133 passed, zero failed |
| Release interpreter tests | 46 passed, zero failed: 24 unit, 18 synthetic floating, 2 floating-reference and 2 integer-reference tests |
| Floating differential cases | All 7,263 match OpenJDK in debug and release |
| Existing integer differential cases | All 616 match OpenJDK in debug and release |
| Java fixture reproduction | 21 byte-identical class files |
| Floating and integer reference regeneration | Both checks passed |

Differential results compare exact bits for non-NaN outputs, including signed zero, and classification for NaN outputs whose payload/sign is not portable. Genuine fixture bytecode confirms both arithmetic widths, all numeric conversion families and both NaN comparison directions. The director reviewed source and reported validation evidence; it does not claim another independent execution of these commands.

## Boundaries and continuation

Execution assumes ordinary host rounding and subnormal modes and does not change process floating-point controls. Public documentation records that prerequisite. Linux, hosted CI and Rust 1.85 runs remain unobserved; the Windows evidence is not presented as those results.

This remains a numeric method-body kernel. It does not provide Java application startup, loading, verification, initialization, object/reference semantics, exception dispatch, fields, arrays, monitors or native services. The next increment adds bounded frame transitions and same-class static body lookup while retaining that explicit boundary. Real runtime class identity, lifecycle, resolution and managed-reference services require their own design and review before broader execution support.

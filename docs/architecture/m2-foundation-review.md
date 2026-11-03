# M2 decoder and integer/long foundation review

Date: 2026-11-03.

**The decoder and integer/long execution-kernel increment is approved. The floating-point increment in `m2-floating-point.md` is authorized. Full M2 remains open.**

## Source review

The director reviewed the decoder, checked frame representation, execution loop, errors, public API, synthetic tests and JDK-reference integration. No blocking findings remain within this increment's contract.

The decoder recognizes every defined instruction format, validates bounded operands and branch destinations, and distinguishes invalid encoding from unsupported execution. The reviewed tests cover all opcode encodings, wide instruction boundaries, switch alignment and nonzero padding, signed edge cases, malformed tables, truncation, reserved bytes and version rules. A discovered error-precedence issue for invalid wide followers was corrected and regression tested.

The interpreter evaluates selected ordinary static method bodies. Local category-two overlap invalidates the previous value; stack limits count JVM slots. All 16 legal stack-manipulation forms have order assertions. Integer/long arithmetic wraps correctly, shifts mask their distance, division and remainder handle zero and minimum-value divided by minus one, and integer returns narrow according to the declared type. Return discards remaining operand-stack values. Instruction budgets include returns and prevent execution at zero remaining budget.

Unsupported declarations and exception tables fail before execution. The complete code stream is structurally decoded before evaluation, including unreachable bytes. Valid unsupported constant kinds are reported separately from ordinary invalid constant references/kinds. Dynamic constants are uniformly unsupported; detailed opcode/category matching for Dynamic remains a deferred diagnostic refinement, with no successful execution implied.

## Execution coverage

The foundation executes 94 effective opcode values. Wide is a prefix supporting five implemented forms, not a separate computational operation.

| Opcodes (hex, inclusive ranges) | Implemented behavior |
| --- | --- |
| 00; 02–0a; 10–14 | nop, integer/long constants and supported ldc values |
| 15–16; 1a–21; 36–37; 3b–42 | integer/long local loads and stores |
| 57–5f | all category-correct stack manipulation forms |
| 60–61; 64–65; 68–69; 6c–6d; 70–71; 74–75 | integer/long arithmetic and negation |
| 78–85; 88; 91–94 | shifts, bitwise operations, increment, integer/long/narrow conversions and long comparison |
| 99–a4; a7; aa–ab; c8 | integer conditionals, goto/goto_w and both switches |
| ac–ad; b1 | integer/long/void returns |
| c4 with 15, 16, 36, 37, 84 | wide local indices and signed increment |

All other valid instructions are decoded but fail explicitly if execution reaches them. This table does not claim verification, dynamic constant resolution or Java invocation semantics.

## Final validation evidence

The coordinator independently ran the final checks on the reviewed source using native Windows Rust/Cargo 1.98.1 and Zulu OpenJDK 21.0.7:

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Strict workspace/all-target Clippy | Passed |
| Workspace tests | 106 passed, zero failed: 77 existing tests, 10 decoder, 17 interpreter unit, 2 numeric integrations |
| Java fixture reproduction | 19 byte-identical class files |
| JDK numeric-reference reproduction | 616 identical cases |
| Rust versus JDK numeric results | All 616 agree, including 34 ArithmeticException outcomes |

Genuine compiler output includes wide increment, both switch encodings, loops, conversion and mixed category-two argument layouts. Synthetic tests cover encodings and invalid execution states that ordinary javac output does not produce. Decoder robustness tests also exercise 12,000 deterministic arbitrary cases under two versions and structured mutations; this is not a coverage-guided campaign or proof of universal panic freedom.

The director reviewed source and evidence reported by the coordinator and workers; it does not claim an additional independent execution of their commands. Linux, hosted CI and the declared Rust 1.85 minimum-version execution remain unobserved.

## Continuing M2

The next authorized increment adds floating-point primitives within the same kernel boundary. This approval does not provide class loading, initialization, access checks, verification, Java invocation, exception objects, fields, heap allocation, arrays or synchronization. Those remain required future work for full M2 and the related runtime milestones. Keep the launcher as an inspection tool until a real application entry point can honor its runtime obligations.

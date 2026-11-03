# M2 floating-point execution increment

Status: implementation authorized by `m2-foundation-review.md` on 2026-11-03. This increment does not complete M2.

## Scope and representation

Extend the existing static method-body kernel with Float and Double. Keep its explicit boundary: no class loading, initialization, invocation, heap, references, exception dispatch or synchronization is introduced here. Existing instruction budgets, structured failures and eager declaration checks remain in force.

Use `Value::Float(u32)` and `Value::Double(u64)` with IEEE binary32/binary64 bit payloads. Document that these are bits, not numeric integer values. Provide convenient constructors from f32/f64 and accessors returning the corresponding optional host number. Representation equality is intentionally different from Java floating comparison: signed zeros have different representations, and identical NaN representations compare equal as Rust Value data. Bytecode comparison must use numeric comparison with its prescribed unordered result.

Extend ValueKind and descriptor handling. Float occupies one slot; Double occupies two. Generalize every category-two local check and invalidation from Long-specific matching to width-based behavior. Tests must overwrite both halves of Double locals and mixed Long/Double pairs. Stack manipulation remains category-based and preserves all bits.

## Instructions and semantics

Implement fconst/dconst, Float/Double ldc forms, all float/double local loads and stores including wide, fadd/dadd, fsub/dsub, fmul/dmul, fdiv/ddiv, frem/drem, fneg/dneg, all remaining numeric conversions, fcmpl/fcmpg/dcmpl/dcmpg, and freturn/dreturn. Valid reference or dynamic constants remain explicitly unsupported; wrong constant kinds remain invalid.

Use each operation's declared width and store its rounded result before the next bytecode. No fast-math, algebraic floating intrinsics, fused multiply-add substitution, extended intermediate precision, or flushing subnormals. Division by floating zero yields floating results rather than ArithmeticException. Negation must distinguish signed zeros. Remainder uses truncating-quotient semantics, not IEEE remainder or Euclidean remainder. Compare instructions return their specified result for unordered operands and treat positive and negative zero as equal. These requirements follow [JVMS SE 21 floating-point arithmetic](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-2.html#jvms-2.8) and [individual bytecode instructions](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html).

Use ordinary Rust f32/f64 arithmetic and explicit casts, with tests demonstrating their suitability on the tested target. Rust numeric casts specify truncation and saturation for float-to-integer conversion, NaN-to-zero, and nearest-even rounding for integer-to-float and narrowing float conversion; see the [Rust numeric cast reference](https://doc.rust-lang.org/reference/expressions/operator-expr.html#numeric-cast). Do not use unchecked float-to-integer conversion. Long-to-Float converts directly rather than via Double, avoiding double rounding. The host must use ordinary rounding and subnormal behavior; this kernel does not alter process floating-point control state. Native embedding that changes those modes needs an explicit future boundary policy.

NaN payload/sign after arithmetic or conversion is not a Java portability guarantee. Preserve raw constant and local/stack bits; compare arithmetic NaNs by classification in differential tests. Do not normalize other results or use numeric equality where signed-zero differences matter.

## Evidence and acceptance

The coordinator owns independent Java fixture/reference generation and integration. Use raw hexadecimal IEEE bits as argument encoding, with explicit result kind and either exact non-NaN bits or NaN classification. The Rust kernel must never call Java to execute a case. Reference fixtures should exercise parameterized operations rather than only javac-folded constants.

Require coverage of every added opcode and all four conversion source types, fixed and indexed/wide locals, mixed argument layouts, category-two overlap, wrong types/return kinds, and constant-pool category mismatch. Preserve all integer/long and decoder regressions.

Use a genuine JDK differential corpus covering positive/negative zero, finite values of both signs, infinities, several NaNs, normal/subnormal boundaries, gradual underflow, overflow, rounding ties, integer limits and adjacent conversion thresholds. Include remainder special cases, both comparison NaN directions, signed-zero comparisons, and a separated multiply/add rounding example. Compare exact non-NaN representations, including zeros. Run reference reproduction plus debug and release Rust differential tests, formatting, strict Clippy and the full workspace suite.

Acceptance requires source review, complete opcode evidence, and these checks. Windows execution evidence does not establish Linux execution evidence. Full M2 still requires invocation, fields, allocation, arrays, exceptions and synchronization with the runtime invariants in the project charter.

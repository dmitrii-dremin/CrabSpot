# M2 initial decoder and interpreter contract

Status: implementation authorized by `m1-review.md` on 2026-11-03. Approval of this initial increment will not imply completion of the entire M2 charter.

## Decoder interface

Add a `bytecode` library with a bounded function equivalent to `decode(code: &[u8], major: u16) -> Result<DecodedCode, DecodeError>`. Keep exact API spelling small and consistent between workers. `DecodedCode` exposes its instruction sequence and checked instruction lookup by bytecode offset. Each instruction records the opcode, starting offset, encoded length, and a typed operand variant. Preserve whether an instruction used the wide encoding when useful for diagnostics; wide's embedded opcode is not a separate instruction boundary.

The initial coordination API is:

```rust
decode(code: &[u8], major: u16) -> Result<DecodedCode, DecodeError>
DecodedCode::instructions(&self) -> &[Instruction]
DecodedCode::instruction_at(&self, offset: u32) -> Option<&Instruction>
DecodedCode::code_len(&self) -> u32
```

`Instruction` has `offset: u32`, `length: u32`, `opcode: u8`, `wide: bool` and `operands: Operands`. A wide instruction records the embedded opcode but the offset and length include its prefix. `Operands` variants are `None`, `Byte(i8)`, `Short(i16)`, `Local(u16)`, `Constant(u16)`, `Iinc { index: u16, increment: i16 }`, `Branch(i32)`, `TableSwitch { default: i32, low: i32, offsets: Vec<i32> }`, `LookupSwitch { default: i32, pairs: Vec<(i32, i32)> }`, `InvokeInterface { index: u16, count: u8 }`, `InvokeDynamic { index: u16 }`, `NewArray(u8)`, and `MultiANewArray { index: u16, dimensions: u8 }`. Branch and switch offsets remain relative to the instruction opcode. Decoder results validate every target before exposing successful decoding.

Decode the defined opcode formats, including opcodes the interpreter does not yet execute. This avoids confusing unsupported execution with invalid encoding. Typed operands distinguish signed immediates, local indices, constant-pool indices, branch displacements, increment operands, table/lookup switches, invocation operands, newarray types and multidimensional-array dimensions. The decoder does not resolve constant-pool entries.

Validate truncation, reserved/undefined opcodes, legal wide followers, signed operand interpretation, switch alignment/counts/order, special reserved invocation bytes, nonzero constant-pool indices and instruction-format operand constraints. Enforce applicable version restrictions, including legacy subroutine opcodes. After decoding, validate every branch and switch destination against instruction boundaries. Retain original offsets for diagnostics. A later verifier still checks fallthrough/control flow, operand types, constant-pool compatibility and method-specific constraints.

Switch padding aligns to the beginning of the method's code array, not the host address or the attribute header. Do not invent a requirement that padding values be zero. Count arithmetic uses a wider checked type and remaining-byte checks before allocating table entries. Branch displacement arithmetic must handle negative offsets without unsigned wraparound. Inputs and results remain bounded by the classfile Code size limit.

Decoder tests cover every operand encoding, boundary signed values, all switch alignments, nonzero padding, empty lookup switches, invalid/unsorted keys, impossible counts, all wide forms, truncations, undefined opcodes, version limits, and branches into operands or outside the method. Add malformed/arbitrary-byte panic-resistance tests independent of the class parser.

## Execution-kernel interface

Add an `interpreter` library. Its entry point selects a parsed ordinary static method body, receives typed arguments and an instruction budget, and returns either a normal result, an uncaught Java exception outcome, or a host/execution error. It decodes the selected Code internally or otherwise only accepts a validated decoder result; callers cannot bypass structural decoding with arbitrary constructed instruction objects.

This is method-body evaluation, not a class-loading or invocation API. The distinction must be explicit in names and documentation. Do not add a Java application `run Main` command that bypasses class initialization. Parsed classes and methods are not relabeled as verified runtime classes. The selected body must have Code, a supported descriptor, and no native/abstract/synchronized declaration or exception handlers. Reject unsupported selection conditions before execution.

The first supported value set is `Int(i32)` and `Long(i64)`. Boolean/byte/char/short computational values use Int. Returned values can be absent for void. Expose category width explicitly. Local storage uses empty/value/continuation states; overwriting a long's head or tail invalidates its previous pair. Operand-stack capacity is measured in JVM slots, not the Rust vector length. Every pop, load, store and capacity check returns a structured error on an invalid state.

The initial instruction slice includes integer/long constants, numeric ldc forms, loads/stores including wide, integer/long arithmetic and bitwise operations, shifts, integer increment, integer/long/narrowing conversions, long comparison, integer conditional branches, goto, switches, basic category-correct stack manipulation, and integer/long/void returns. Keep an opcode coverage table. Implement only stack-manipulation forms whose category rules have tests; other forms return an explicit unsupported result until implemented.

Arithmetic uses Java wrapping semantics, including minimum-value divided by minus one. Division and remainder by zero return an uncaught ArithmeticException outcome. Shift distances use the appropriate masked width. Integer returns honor the declared narrow return type. No floating-point or reference operation is replaced by an integer approximation.

Consume one budget unit per executed instruction, including return instructions. Zero remaining budget prevents the next instruction from executing. The budget is deterministic and produces a resource-limit error distinct from a Java exception. No process-global state, real-time sleep, or unbounded host recursion is needed for the first slice.

## Acceptance of this increment

1. Decoder unit and boundary tests pass, including malformed-input robustness and tests of every operand format. Original classfile tests remain green.
2. Frame tests establish category-two slot accounting, overlap invalidation, bounds, underflow, wrong-type behavior and correct argument layout.
3. Independently compiled Java 21 pure numeric fixtures exercise arithmetic edges, conversions, branches, loops and both switch forms. A reference JDK tool records reproducible results and ArithmeticException outcomes. Rust execution is compared against those reference records; the interpreter never delegates execution to Java.
4. Synthetic tests cover encodings and invalid frame states that javac does not naturally emit. Looping code stops at an explicit budget. Unsupported opcodes/features produce actionable errors with offsets.
5. Formatting, strict Clippy, workspace tests and fixture/reference reproducibility checks pass. Reports separate decoder coverage from execution coverage and foundation acceptance from full M2 completion.

After this increment, continue M2 with floating-point semantics, invocation/frame stacks and the minimal real runtime services required by fields, objects, arrays and exceptions. Each expansion must preserve the charter's class identity, lifecycle, managed-reference and future GC/safepoint requirements. Full M2 acceptance still requires the complete interpreter scope and reference comparison described in `ARCHITECTURE.md`.

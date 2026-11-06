# Operand public acceptance: explicit CLI Field work ceiling

Status: FROZEN. Coordinator and execution-owner terminal readbacks of semantic digest 0BFE02B2BD34EFAF221086CE6443C177791C610980C692F5F231136C16E374C8 are complete. Bounded implementation is authorized. It supplements frozen operand contract 607C6D5FCB95C0EDE8797D6D17B0513E2A8000ABEB38EB5660F7B47E1BBE2BE6 without changing its VM semantics or diagnostic formulas.

## Finding and decision

The genuine, unchanged 248-fault OperandMain matrix exhausts the existing default Field LookupSteps ceiling at 1,000,001 required versus 1,000,000 allowed. Evidence: target/operand-public-product-worker/initial-normal.json. With only an explicit API field ceiling of 16,777,216, initial-normal-field.json reports successful fields184/42/6/8/8/248/29/1, Started/Produced248, DiagnosticWork9624/messageUnits7700/messages240, and snapshots496/frames1686/backtraces496/traceWork7638 under unchanged default fuel. This is an execution-policy ceiling, not a demonstrated engine defect. The higher ceiling is a finite tested allowance, not a measured exact minimum.

Expose the existing Field work ceiling through an additive CLI option. Do not change FieldResolutionOptions::default, ApplicationOptions::default, old wrapper semantics, public option struct layouts, instruction fuel, or any other resource ceiling. Do not reduce or split away the guest's full case matrix, automatically retry on quota failure, or silently increase a caller's chosen limit.

## Exact CLI semantics

Syntax: `crabspot run --platform DIR --classes DIR [--field-lookup-steps N] BINARY.MAIN [-- ARG ...]`.

The three named option/value pairs may appear in any order before the entry; --platform and --classes remain required exactly once; --field-lookup-steps is optional exactly once. No option parsing occurs after the entry. Existing optional separator and native guest argument decoding remain unchanged, so after `--`, option-looking tokens remain literal guest arguments. `inspect` is unchanged.

Parse no more than three recognized pairs; reject duplicate options, unknown pre-entry switches, missing option values, missing entry, an early `--`, and a trailing pre-entry option after entry under the existing separator rule with usage exit2. Preserve invalid-native-entry-name failure behavior for an actual entry token. Quota N is nonempty ASCII decimal digits only, parsed with checked arithmetic into usize; zero and representable usize::MAX are valid. Signs, spaces, non-ASCII digits, invalid native Unicode, and overflow are usage errors. Bound scanning to at most20 decimal characters before arithmetic (sufficient for supported32/64-bit usize); oversized numeric tokens are rejected without an unbounded parse or copied input buffer. Reject numeric/grammar errors before directory I/O or engine effects.

Start from ApplicationOptions::default and, only when supplied, set `options.field.max_lookup_steps=N`. Pass that single options value to the existing application entry API. There is no new API profile, new runtime counter, hidden retry, or alternate evaluation. Field work remains the existing cumulative checked lookup/service budget for setup and execution, independent of opcode fuel and other limits.

## Evidence and acceptance

The full original and rebuilt OperandMain physical CLI and equivalent API runs use explicit16,777,216 with identical guest bytes, message-oracle inputs and expected fields. Retain the original default1,000,000 failure as a negative policy control; it must remain a typed Field LookupSteps resource failure, not an uncaught Java exception. The default no-option CLI remains unchanged. Do not claim that the full248 case matrix succeeds with defaults.

Add automatic CLI controls for unchanged old no-option syntax, both required-option orders, quota in each pre-entry position, zero, exact usize maximum, overflow, empty/sign/space/non-ASCII/invalid-native values, duplicates/unknown/missing arguments, entry/separator behavior and option-looking literal guest arguments. At least one actual CLI low-ceiling run must fail at the existing typed quota boundary, and the explicit high-ceiling full genuine matrix must succeed. Existing input struct-literal callers and explicit library resource limits continue compiling unchanged. Unit parser tests should avoid filesystem work; process tests verify usage2 before missing-source errors for malformed inputs.

Update CLI usage and launcher documentation with the option, unchanged default and explicit full-matrix invocation. Resource acceptance continues to distinguish successful finite allowance from exact-cost oracles. Snapshot the narrow source/test/docs amendment, compare all other frozen core files unchanged, run fmt, strict workspace/all-target Clippy and relevant launcher tests, then include amended bytes in the final combined gate. No prior frozen evidence is rewritten.

## Ownership and limits

Execution owner is authorized to implement this isolated CLI adapter/parser/test/docs amendment in its separately assigned operand-public-product worktree and publish a separate immutable amendment manifest. That owner also implements public driver/verification invocation changes under the coordinator's existing delegation; root owns final integration. Runtime has no source change for this policy. Existing source component clearance remains valid for unchanged bytes; new CLI bytes require independent review. Full product/milestone acceptance remains gated on the combined suite and genuine public evidence.

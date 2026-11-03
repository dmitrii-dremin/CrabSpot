# ADR 0001: Begin with a parser library and inspection launcher

Status: accepted for M0/M1 implementation.

## Context

The charter proposes eventual JVM components but prohibits empty crates and execution delegation. A parser is useful before runtime services exist and needs a small, fuzzable trust boundary.

## Decision

Create only `classfile` and `launcher` initially. Keep parsing as a pure bytes-to-model operation in safe Rust. Separate runtime metadata from the parsed representation, and postpone other crates until implemented behavior requires them. The launcher inspects class files and owns filesystem and terminal interactions. It does not execute bytecode.

## Consequences

The parser is independently testable and usable by future loader/verifier code. There is no placeholder JVM executable success path. Future boundaries for decoder, runtime, GC, native integration, and JIT are recorded in the architecture without speculative implementations. No JVM implementation library is used to outsource parsing or execution.

# Parser fuzzing

The std-only Rust example exercises the public parser with deterministic seeded
mutations, arbitrary bytes, truncations, insertions, appended bytes, and corrupted
length/index words. Valid mutations feed a bounded in-memory corpus alongside
the original compiler-produced seeds. It does not measure coverage and is not a
coverage-guided fuzzer. No bytecode is executed.

```text
cargo run -p crabspot-classfile --example fuzz_parser --release -- tests/fixtures/classes --seconds 300 --seed 4851011501752864596 --failures target/fuzz-failures
```

Use `--iterations N` instead of `--seconds N` to reproduce the exact reported
number of cases with the same seed, source revision and corpus. Both limits can
be supplied to stop at whichever comes first. Zero seeds/counts/durations and
invalid inputs fail explicitly. CI executes a short 10,000-case smoke run; full
campaigns and their observed counts are recorded in the milestone evidence.

The corpus is sorted by path, capped at 256 class files, and validated before the
campaign. Inputs are capped at 64 KiB and parsed with the default annotation
depth/node budgets. Original seeds are never displaced. Every 1,024th generated
input is parsed again and compared for deterministic output. Panics or differing
results cause a nonzero exit and save the exact input as
`seed-<seed>-case-<iteration>.class` in the selected failure directory. Process
termination and allocator/stack aborts cannot be recovered by `catch_unwind`;
the parser's resource policies and separate boundary tests remain necessary.

Progress reports print every 15 seconds. The final line records seed, executed
cases, accepted/rejected counts, repeated cases, final corpus size, elapsed time
and failures. Preserve that output with the fixture manifest digest and source
fingerprint. A finite successful campaign cannot prove panic freedom.

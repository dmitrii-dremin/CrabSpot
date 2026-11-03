# Accepted checkpoint: primitive-array execution

Updated 2026-11-03. M1 and the decoder, numeric/call kernel, runtime registry,
hierarchy/layout, shared values/default static storage, primitive-array heap and
array execution bridge increments are approved. Full M2 and later milestones
remain open. Continue autonomously without NanoSpec; ask the user only when their
intervention is unavoidable. The next object-storage design is still a draft.

The user clarified the long-term objective: complete JVM specification support,
required observable compatibility with conforming JVMs (HotSpot is the primary
reference), and demonstrated performance superiority on representative workloads.
Java SE 21 remains the first complete conformance target. Kotlin/Spring Boot are
integration milestones, not the endpoint. This expands the success criteria;
existing partial implementation boundaries and milestone approvals are unchanged.

## Implemented and independently checked

- Five real crates: classfile, bytecode, interpreter, runtime and launcher.
- Java 21 class-file encodings and all 30 standard attributes, lossless UTF-16,
  structured errors and explicit parser limits; metadata inspection CLI.
- Full instruction-format decoder and 179 effective execution opcode values with
  an explicit caller-owned heap. Original numeric APIs retain 148 opcode values.
- Numeric/void static body graphs, shared fuel, explicit frames, frame/slot/code
  limits, same-class symbolic static targets and typed diagnostics.
- Immutable definitions keyed by defining loader and exact name; checked opaque
  class/member identities, bounded sources, atomic admission and source caching.
- Explicit hierarchy resolution with kind/final/cycle checks, parent-linked
  layouts, field hiding and declaring-owner static locations; four graph/layout limits.
- Shared numeric/reference values and default static storage, with zero/+0/null
  values, no ConstantValue assignment, no initializer execution or lifecycle claims.
- Actual eight-kind primitive-array heap: typed payloads, checked opaque
  heap/index/generation identities, bounded fallible allocation and atomic publication.
- Array allocation/access/length, reference locals and identity/null branches;
  symbolic null/bounds/negative-size outcomes separate from host service failures.
  Successful heap effects persist through returns, exceptions, fuel exhaustion
  and host errors. The heap does not collect or reuse entries.

## Validation

Final frozen-code checks passed on native Windows with Rust/Cargo 1.98.1:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 256 passed, zero failed |
| Optimized runtime/interpreter tests | 169 passed: 77 runtime + 92 interpreter |
| Execution differential outcomes | 8,285 match in debug and release |
| Separate heap-service roundtrips | 151 match in debug and release |
| Java 21 fixture reproduction | 30 class files reproduced |
| Reference-result reproduction | All five sets reproduced |
| Runnable array-body demo | 17 -> Int731; -1 -> symbolic negative-size outcome |

Execution outcomes comprise 616 integral, 7,263 floating, 126 static-call and 280
primitive-array cases. Array coverage includes 11 null, 36 bounds and 20 negative
size outcomes. The 151 heap-service cases reuse a subset of the array oracle;
they are a separate service check, not additional unique execution cases.

The native compiler/reference runtime is Zulu OpenJDK 21.0.7. Rust tests use
checked-in fixtures and results; production execution does not invoke Java.
All preexisting fixture hashes were preserved when new sources were added.

M1's exact-source sustained fuzz evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds with zero failures. The later 28-class corpus
also passed a 10,000-case parser smoke run. No finite test proves panic freedom.

No known failing executed tests remain. Linux, hosted CI and the declared Rust
1.85 minimum are not yet tested. CI is configured for Linux/Windows and both
checked-in and regenerated fixtures; a configuration is not a passing remote run.

## Reviews and decisions

Review records in docs/architecture include m1-review.md, m2-foundation-review.md,
m2-floating-review.md, m2-call-stack-review.md, runtime-registry-review.md,
runtime-hierarchy-review.md, runtime-static-storage-review.md,
runtime-primitive-array-heap-review.md and m2-primitive-array-bridge-review.md.
The final bridge review approves this increment only, not all of M2.

Review corrections have regression coverage: interface Object finality checking,
missing internal parent layouts, checked resource arithmetic, foreign references,
and reference stores invalidating either half of category-two locals. Deep graph
and call-stack tests use explicit iteration rather than recursive Rust execution.

## Precise continuation

The next direction is ordinary-instance storage and owner-qualified field access;
see docs/architecture/runtime-instance-storage-next.md. Before implementation,
the director must finalize admission of multiple compatible hierarchy plans into
one heap and the concrete object API. Avoid per-object copies of ancestor metadata,
conflicting shapes for one ClassId, non-null reference stores without assignability,
public final-field bypasses and uninitialized-object execution claims.

Then delegate runtime implementation and independent public tests to Sol, supply
genuine field metadata/oracles, run the standard checks, and obtain director review.
A separately reviewed body-kernel field bridge can follow. Preserve explicit
constructor, class-initialization, verification and method-resolution boundaries.
This requires technical design/review, not another request for user permission.

General Java application startup, full class loading/linkage, verification,
initialization, instance/reference-array execution, exception objects/handlers,
monitors, native methods, threads and GC remain incomplete. Kotlin/Spring Boot is
not yet executable. Synthetic Object roots are explicit metadata test inputs only.

All agents finished this increment. Production and tests are stable; no pending
execution sessions or infrastructure incident remain. The user initialized Git
and authorized publication to https://github.com/dmitrii-dremin/CrabSpot.git.
Local main is based on the remote's initial commit, preserving its Apache 2.0
license. Git metadata ownership was verified as the interactive Windows user.

## Reproduce

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --quiet
cargo test -p crabspot-runtime -p crabspot-interpreter --release --quiet
python tools/compile_fixtures.py --check
python tools/numeric_reference.py --check
python tools/floating_reference.py --check
python tools/call_reference.py --check
python tools/array_reference.py --check
cargo run -p crabspot-interpreter --example primitive_array_body -- 17
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
runtime-worker and registry-test-worker output directories.

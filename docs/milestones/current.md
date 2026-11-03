# Accepted checkpoint: ordinary-instance storage

Updated 2026-11-03. M1 and the decoder, numeric/call kernel, runtime registry,
hierarchy/layout, shared values/default static storage, primitive-array heap and
array execution bridge increments are approved. Ordinary-instance storage is
approved in its final source review. Full M2 and later milestones
remain open. Continue autonomously without NanoSpec; ask the user only when their
intervention is unavoidable. See `docs/architecture/runtime-instance-storage-review.md`
for the ordinary-instance acceptance record.

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
- Ordinary instances share the managed arena with arrays. Atomic admission merges
  multiple compatible hierarchy plans, retains each class's own fields once and
  rejects conflicting resolved ancestors without changing existing objects.
- Six independent instance/layout quotas; default zero/+0/null values, hidden
  fields qualified by owner, numeric narrowing/raw floating bits, final-field
  protection, null-only reference writes and borrowed child iteration. This
  memory service does not execute constructors, initialization or field bytecodes.

## Validation

Final frozen-code checks passed on native Windows with Rust/Cargo 1.98.1:

| Check | Result |
| --- | --- |
| cargo fmt --check | Passed |
| Strict workspace/all-target Clippy | Passed |
| Debug workspace tests | 289 passed, zero failed |
| Optimized runtime/interpreter tests | 202 passed: 110 runtime + 92 interpreter |
| Execution differential outcomes | 8,285 match in debug and release |
| Separate memory-service roundtrips | 151 array + 151 raw field cases match in debug and release |
| Java 21 fixture reproduction | 34 class files reproduced |
| Reference-result reproduction | All six sets reproduced |
| Runnable array-body demo | 17 -> Int731; -1 -> symbolic negative-size outcome |

Execution outcomes comprise 616 integral, 7,263 floating, 126 static-call and 280
primitive-array cases. Array coverage includes 11 null, 36 bounds and 20 negative
size outcomes. The 151 heap-service cases reuse a subset of the array oracle;
they are a separate service check, not additional unique execution cases.
The new field oracle generates direct putfield/getfield instructions in the JDK
without pre-store narrowing; Rust checks the memory service using genuine field
metadata. There are 33 added tests: 22 public, two genuine and nine private
instance/admission/CAS regressions. Both hierarchy and object allocation/query/drop
are tested with 10,000-class chains.

The native compiler/reference runtime is Zulu OpenJDK 21.0.7. Rust tests use
checked-in fixtures and results; production execution does not invoke Java.
All preexisting fixture hashes were preserved when new sources were added.

M1's exact-source sustained fuzz evidence remains in docs/milestones/evidence:
108,907,980 cases over 300 seconds with zero failures. The later 28-class corpus
also passed a 10,000-case parser smoke run. This increment additionally passed
10,000 cases using the expanded 34-class corpus (same seed, zero failures). No finite test proves panic freedom.

Hosted [run 41715154759](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41715154759)
for 2617637 was inspected: Linux and Windows checked-in tests, fixture/reference
regeneration and formatting passed with Rust 1.99.1 and Temurin JDK 21.0.13.
Strict linting failed because AtomicU64::fetch_update is deprecated in that Rust
release; later checks were skipped. A compatible checked CAS-loop correction is
implemented and locally validated in this increment. No complete passing hosted
run is claimed yet; fresh CI will validate the publication.
The declared Rust 1.85 minimum remains untested.

## Reviews and decisions

Review records in docs/architecture include m1-review.md, m2-foundation-review.md,
m2-floating-review.md, m2-call-stack-review.md, runtime-registry-review.md,
runtime-hierarchy-review.md, runtime-static-storage-review.md,
runtime-primitive-array-heap-review.md, m2-primitive-array-bridge-review.md and
runtime-instance-storage-review.md. These approve bounded increments, not all of M2.

Review corrections have regression coverage: interface Object finality checking,
missing internal parent layouts, checked resource arithmetic, foreign references,
and reference stores invalidating either half of category-two locals. Deep graph
and call-stack tests use explicit iteration rather than recursive Rust execution.

## Precise continuation

The next dependency is a separately reviewed field-body execution bridge; see
`docs/architecture/m2-instance-field-bridge-next.md`. Its
contract must define receiver and MethodId/ClassId context, symbolic field
resolution scope, access/finality checks, reference argument/return rules and
heap ownership. Preserve explicit constructor/class-initialization, verifier and
method-resolution boundaries; never treat memory allocation as construction.
Reference arrays and non-null field assignment need assignability first.

General Java application startup, full class loading/linkage, verification,
initialization, instance/reference-array execution, exception objects/handlers,
monitors, native methods, threads and GC remain incomplete. Kotlin/Spring Boot is
not yet executable. Synthetic Object roots are explicit metadata test inputs only.

The implementation and tests are frozen and reviewed; no infrastructure incident
remains. Workers used isolated detached worktrees under target/agent-worktrees. The user initialized Git
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
python tools/field_reference.py --check
cargo run -p crabspot-interpreter --example primitive_array_body -- 17
```

On this Windows host, add C:/Users/Admin/.cargo/bin to PATH and set
CARGO_INCREMENTAL=0. Use isolated CARGO_TARGET_DIR and TMP/TEMP directories inside
the project. The coordinator used target/verification-m2; workers used separate
instance-worker and instance-test-worker output directories.

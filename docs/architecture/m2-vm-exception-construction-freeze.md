# Throwable construction implementation freeze

The technical director froze the [implementation contract](m2-vm-exception-construction.md)
on 2026-11-06 after complete runtime-owner, execution-owner and coordinator readbacks.
The common semantic edition had SHA-256
`0FB1C97EA06EA485B852DB1E729402AAF46C4DFC520BA585F99407053F30F128`.
The frozen edition has raw SHA-256
`1597DEBF5A7ADA0330C10BCF156C2DAFB4EEC18F430B0A7B71D01ABCF367F0D9`.
The coordinator inspected the six changed regions: only review-status wording and
headings changed between those editions.

Implementation starts from published revision
`5a2924d8a6101f43448a2eae94a0f048d7959907`. It contains the reviewed independent
[Throwable reference suite](m2-throwable-reference.md). The coordinator created two
separate worktrees and verified that their Git metadata remained owned by the
interactive Windows user before dispatching implementation.

GPT-6.1 Sol with high reasoning effort owns each bounded component. The runtime
owner implements canonical ABI discovery, native-field protection, managed
backtrace storage, charged snapshots and the construction/authentication bridge.
The execution owner implements native and VM continuations in the shared engine,
original instruction positions, constructor/initializer boundaries, and directory
launcher integration. GPT-6 Astra retains architectural review. The coordinator
owns independent public-application evidence, integration and publication.

This authorizes implementation, not acceptance. Immutable component source review,
the complete integrated verification gate and post-gate source hashes remain
required. Other VM exception producers, Java stack-trace element population,
concurrent execution and full M2 remain open.

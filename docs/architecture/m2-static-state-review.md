# Checked static state review

Status: approved by the technical director on 2026-11-04 for heap-owned checked static memory and the two opt-in body profiles specified in `m2-static-state.md`. Frozen source review and the coordinator's integrated gate pass. Full M2, complete preparation and initialization remain open. New hosted CI is pending publication.

## Reviewed behavior

Each heap owns one static container with explicit membership for every represented class, including zero-field classes. Declared FieldIds own unique logical slots; inherited symbolic owners share the selected declaration's location. Initial ordering follows registry declaration order and field order. Atomic extension appends new classes without resetting or reordering old values. Repeated allocation with unchanged options/membership consumes no lookup work and preserves state. Different options are rejected; graph growth requires a fresh context and profile.

Profiles bind registry/heap identity and admitted class/slot/payload counts. They deliberately do not bind an optional reference universe: default allocation needs none, and first binding later requires only a fresh context under existing rules. Static allocation/writes do not invalidate existing exception/type/method snapshots. Graph growth continues to invalidate those snapshots under their original rules. Same-count heap replacement cannot reuse a profile.

Allocation charges inventory before collecting it, each newly examined declaration before testing staticness, and each distinct descriptor entry's complete UTF-16 length before classification. Compact scalar descriptor keys prevent copying or repeatedly classifying shared long names. Class, logical-slot and actual Value-byte quotas are independent of instance/array quotas. Fallible staging and all destination reservations precede publication; failure preserves the prior container. Payload accounting explicitly excludes metadata and spare capacity.

The runtime reuses existing lawful field selection, loader-scoped packages, protected-static access and conservative nominal loading constraints. Access and category checks precede final rejection and value validation. All final writes remain forbidden. Numeric normalization preserves established subword and raw floating-bit behavior. Non-null assignments validate the source, resolve the declaring descriptor without caller type-access restrictions, then check assignment before a guarded private commit. Null needs no universe or target resolution. Reads validate retained values; roots preserve one handle per non-null location, including aliases, without child traversal or allocation.

The interpreter adds an optional static capability to the existing shared engine. Static-only and static-plus-supplied-Throwable entry points check capabilities before the unchanged root pipeline. Every nested frame uses its current declaring origin. putstatic extracts one Value without a receiver before service selection; service access/category/final checks precede kind/assignment checks. Only direct StaticJava categories map to symbolic outcomes or, in the composed profile, UnsupportedVmException before handler search. Nested diagnostics remain host failures. Actual supplied throws still use ordered handlers and iterative unwind, preserving earlier static writes.

Older execution entry points still reject b2/b3 even when the heap contains static memory. The old standalone StaticStorage remains independent and read-only. The old runtime-aware/exception-only profiles remain at 191/192 effective opcodes; the new static-only/composed profiles support 193/194. No profile applies ConstantValue, runs clinit, marks a class prepared/initialized or grants final-write authority.

## Review findings and corrections

The coordinator identified repeated try_reserve_exact(1) on staging vectors, which could cause quadratic reallocation/copy work. New static vectors now use fallible amortized try_reserve with the same actual bounded additions and logical accounting. Existing unrelated allocation helpers were not changed.

The director found that retained-prefix validation checked map/vector counts but did not prove each enumerated field mapped to its own slot. Duplicate fields plus a phantom map key could satisfy those counts in privately corrupted state. Validation now checks positions[field] == enumeration_index, and a direct duplicate/phantom regression rejects extension without mutation. This was defensive internal-invariant hardening; public APIs cannot construct that corruption.

Retained-prefix validation may precede staging rather than run again immediately before append: there is no resolver callback or old-state mutation between validation and publication. The source documents that invariant. Tests preserve a non-null old prefix through late field quota, descriptor-work and post-membership-reservation failures. The final private reservation helper extraction changes no semantic accounting.

## Coverage

Eighteen public runtime tests and seven private tests cover defaults; every primitive kind and raw narrowing; hidden/inherited/interface locations; package/private/protected and loader boundaries; inaccessible descriptor types; reference assignment, arrays, aliases and cycles; exact roots; universe binding; snapshot replacement; zero/exact/MAX policies; idempotence and extension retry; and unchanged exception/instance state. A 65,535-unit descriptor shared by 100 fields is charged once per immutable CP entry. A 10,000-class chain allocates each declared static once and drops iteratively. Private checks cover arithmetic/reservation overflow, forged profiles, corrupt positions/kinds/finality/handles, precommit atomicity and direct-versus-nested error sources.

Seventeen public interpreter tests and one private mapper test cover both entry points, every call family/current origin, raw B/C/S/Z putstatic without conversion bytecodes, category-two widths, exact limits and provenance, retrieved references used by fields/casts/calls/multidimensional arrays, alias roots, first universe binding, old-mode rejection, actual catch/uncaught persistence and VM outcomes that must remain unsupported before catch-all search. The unchanged engine algorithm is shared across capabilities; only narrow wrapper/result helpers were factored.

The original StaticBody fixture adds 364 JDK outcomes over 34 receiver/method pairs: 282 Int, twelve Long, twenty Float, twenty Double, six symbolic VM failures and 24 supplied uncaught identities. The JDK driver explicitly resets mutable statics for each case, matching fresh host-controlled default storage. Source-derived assertions inspect all values, actual reference/array types, aliases, root order, array contents and independent heap totals. Each row then grows from ten represented classes/twenty slots to eleven classes/24 slots and verifies the old values/roots plus four new defaults. Separate metadata assertions prove that genuine ConstantValue and clinit remain unapplied; these are not differential initialization claims. Actual decoded Code covers b2/b3 and all existing call/throw families. Genuine integral code narrows before some stores, so raw b3 semantics are established separately by the explicit synthetic cases and shared tested normalization.

## Frozen evidence

The director independently verified the seven runtime files in worker/main, the five frozen interpreter files, and all eighteen combined files in `target/coordinator/static-freeze.json`. Principal SHA-256 values describe native working-copy bytes before Git line-ending normalization:

| File | SHA-256 |
| --- | --- |
| runtime/src/static_state.rs | `0AFEB074308A567014884E281AE66DCBEF8B7C74A18D280732A1D774EDA37710` |
| runtime/tests/static_state.rs | `0BFF77E55482B9EDD68E4C2F533D3B9351284A0CE99C15D2DE27F8D4F7663789` |
| interpreter/src/execute.rs | `D1353BA479F438D3F746A65DBE63B79262C3C29A2575EF47FC49E2D0AD667003` |
| interpreter/tests/static_state.rs | `D2397AA6CCE5522AD597F00A1F7E4293E17420D5B01D3CFFA43EE95012896A53` |

The coordinator independently observed on the frozen combined source:

- Formatting and strict workspace/all-target Clippy pass.
- All 768 workspace tests and 681 optimized runtime/interpreter tests pass, with zero failures. The optimized split is 366 runtime and 315 interpreter.
- All 26,025 execution comparisons pass in both profiles: 25,661 previous cases plus 364 static-body cases. Separate 151-case array and 151-case field service oracles pass.
- All 104 genuine classes and seventeen reference-output sets reproduce; all previous 98 class hashes remain unchanged. The 39-source/seventeen-record preflight and six Python regressions pass.
- The expanded corpus passed 10,000 parser mutations: 121 accepted, 9,879 rejected, ten deterministic rechecks, corpus 225 and zero failures; seed 4851011501752864596.

All eighteen source hashes were reverified after the integrated gate. Local validation used Rust/Cargo 1.98.1 and Zulu JDK 21.0.7. New hosted Linux/Windows evidence remains pending publication.

## Boundary and next direction

This approves checked static memory and composed selected-body access. It does not approve active-use initialization, ConstantValue/String materialization, clinit/final-write authority, complete linking/loading constraints, VM exception construction, constructors, synchronization, Java startup, GC, full M2 or the ultimate conformance/performance target. Initialization scheduling and its explicit failure protocol are proposed in `m2-initialization-engine-next.md`; an exact bounded contract must precede implementation.

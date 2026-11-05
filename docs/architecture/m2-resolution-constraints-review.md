# Checked member resolution during execution

**Approved for publication, 5 November 2026.** Genuine Java callers used a domain-backed execution context to store and retrieve references, call across defining loaders and dispatch through a shared constant-pool entry to different receiver implementations. Resolution constrains the actual referring and declaring loaders after access checks. Category, receiver, initialization and writable-field checks remain specific to each use. The frozen contract is `13D9A13ACEAB431BBD3163583DA3E8CD5F4685C7AC60E3BC835742F7EFC1ACB3`. Hosted CI for this change is pending.

## Findings closed

- Heap opening distinguishes a missing initiating record from a real record to a different same-name definition. Previously the latter state reported a recoverable missing binding. Tests use a separately admitted and fully witnessed heap to exercise the incompatibility.
- In the checked profile, distinct runtime packages make private access permanently illegal, including identical textual packages under different defining loaders. The runtime caches that result before imposing signature constraints. Distinct owners in one runtime package retain the unsupported-nest boundary. Field and method regressions also check the legacy behavior.
- Warm symbolic failures replay in a smaller heap after checking the origin. Successful resolutions require the actual symbolic owner and declaring member to be available. Protected-receiver qualifiers, keys, failure kinds, counters and publication tickets have private integrity tests. A selected method under a different loader needs an existing declaration agreement or two actual bindings to one identity. Checking that fact never adds an equality.
- Initial tests covered ordinary body failure. Three added cases preserve the resolved method through abstract-method selection, competing defaults and a failed target initializer. The latter retains a prior write and leaves the target body unentered. Tests also distinguish an unmaterialized VM linkage failure from an actual thrown object.
- The reference report distinguishes the four refused-Class-load rows, eight conflicting-override schedules and the ordinary Rust two-call/late-repair schedule. A separate test now observes an actual call, rejected late definition, successful delegation and reuse of the site in a second compatible heap. Exact CP evidence covers all five families, including distinct member references sharing a Class entry. [Reference results and differences](m2-member-resolution-reference.md)

## Accepted gate

All 93 files matched the combined manifest after the final run, SHA-256 `709DC672BBC58E6477874A39B1D2A3ADB1FC7D0AA3B87C8E833D3426890D2CD2`. The 14 frozen runtime files matched reviewed APIv3. The nine execution/test files matched their reviewed candidate. The remaining 70 files are coordinator-owned. The first concurrent build exited without a diagnostic. Its affected target passed in isolation. A clean serial run completed all 44 checks. The review uses that completed run.

| Check | Observed result |
| --- | --- |
| Format, strict workspace/all-target Clippy, locked Rust tests | 1,265 workspace debug and 1,178 optimized passed. The optimized run covered runtime, interpreter and verifier. |
| Reference/source checks | 46 canonical Java sources and 23 reference records. Eleven provenance, five loading-reference and five member-reference Python tests passed. |
| Member-resolution reference replay | 119 original rows per tested VM, Zulu HotSpot 21.0.7 and pinned Semeru OpenJ9 21.0.13.0. Across builds 97 rows match and 22 differ in observable stage, error or retained state. The strict resolver is tested independently where schedules differ. |
| Real bootstrap corpus | 7,519 original java.base classes, 61,708 Code bodies and 4,685 bodyless methods. All 7,519 got preparation witnesses. Opening their checked heap validated 10,935 layout edges and 11,927 declared instance fields, with 1,343,610 Work, 1,199,394 UTF units and zero Scratch. The image had zero terminal member sites and zero cross-loader unions. This is a metadata/scale test using explicit corpus layout caps. |
| Parser smoke | Seed 4851011501752864596, 522 initial inputs, 10,000 cases, 86 accepted, 9,914 rejected, ten deterministic rechecks, 608 retained inputs and zero failures. |
| Sustained parser campaign | 300 seconds on the rechecked input digest `d49960615f6fe900caad0f38710e4c8e29c6e8c58615673e3f79dcf2f9bb68a5`. 18,683,667 cases, 417,980 accepted, 18,265,687 rejected, 18,246 rechecks, 1,024 retained inputs and zero failures. |

The scope is ordinary checked member resolution in the existing unnamed-module, supplied-metadata execution profiles. Java class-load failure caching, on-demand user loaders, nestmate resolution, named modules and general exception materialization remain open. The next authorized step is **exact-contract planning** for a verified application entry from class-file sources. Production dispatch requires its own feasibility readback and frozen contract. [Next proposal](m2-verified-application-entry-next.md)

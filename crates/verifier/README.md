# Standalone method Code validation

`crabspot-verifier` checks the file-local representation of one selected method's
entire Code, including unreachable instructions. This representation API uses the
class-file model and independent bytecode decoder. Existing parsing, runtime, execution,
initialization and launcher APIs do not acquire an implicit validation gate.

```rust
use crabspot_verifier::{CodeValidationOptions, validate_method_code};
# fn inspect(class: &crabspot_classfile::ClassFile) -> Result<(), Box<dyn std::error::Error>> {
let result = validate_method_code(class, 0, CodeValidationOptions::default())?;
if let Some(decoded) = result.decoded() {
    println!("{} instructions", decoded.instructions().len());
}
# Ok(()) }
```

The result borrows its ClassFile, selected Member and optional Code, and owns one
DecodedCode. A legal ordinary abstract/native declaration has no Code/decoded
body. Private result construction keeps those optional getters consistent. This
is a partial representation result, never a verified-method certificate, execution
permission, class identity or linkage result.

Checks run in a fixed order: version and selected declaration; method attribute
inventory/body presence; Code length and parameter-local capacity; funded decode;
every instruction in offset order; handlers; then nested attributes and stack maps.
Errors retain the requested method index, phase location, accepted usage and a
structured kind. Decode errors remain available through Error::source. Input is
never mutated. Java exception objects are not synthesized.

The instruction pass validates CP tags and followed links, name/descriptor grammar,
version-sensitive loadable constants and method-reference tags, real wide holes,
invocation argument/count slots, explicit/implicit/wide local widths and class/array
operand ranks. It preserves method-reference array-owner spellings rather than
copying execution-kernel restrictions. No bootstrap table, method existence,
accessibility, staticness, assignment or Java class resolution is performed.

Handler endpoints must be actual instruction boundaries (or Code end for the end
point). Stack-map offsets are recomputed from offset_delta and compared with the
public stored value. Uninitialized offsets must name new; UninitializedThis is
limited to a selected constructor. Only current logical local-entry widths and a
cached slot total are maintained: Long/Double are one encoded entry and two slots,
and an explicit Top is a separate one-slot entry. Same frames do not rescan locals.
Missing required branch frames, type equality, constructor flags and flow remain
outside this service. Discarded compact/extended frame tag aliases cannot be
reconstructed from the parsed model.

Code/StackMapTable typed bodies must match their recognized names, locations and
versions. Opaque names outside those recognized contexts retain extension semantics.
Other attribute bodies, including annotation/debug trees, are ignored after their
funded Utf8 header inspection. Historical parameterized void clinit is explicitly
unsupported; semantic static `()V` initializers retain the parser's ignored-flag
policy. Version policy matches the parser's Java 21 envelope and preview opt-in.

Four inclusive u64 policies bound Code bytes, metadata entries, UTF-16 units and
work. Multi-counter charges are atomic. Code's actual length (at most 65535) is
funded before calling the unchanged decoder; its allocations are bounded by that
input, with no new recoverable host-OOM guarantee. Each semantic instruction,
followed CP lookup and boundary predicate pays work; list/header scans fund actual
entries, not configured maxima. Successful UTF-16 summaries are cached by actual
Utf8 CP index and role, while each use still pays link lookups. Caches and the one
current-width buffer reserve fallibly on actual demand. No nominal-name trees,
copied graphs, expanded per-offset frames or persistent resolution cache is kept.

Positive validation does not establish reachability, stack/local flow, merges,
return-opcode compatibility, fall-off safety, constructor legality, protected
receivers, verification, linking or JVM compatibility. Those obligations require a
separate type-state contract. Reference-JDK tools are used only by independent
fixture/oracle tooling, never by this library.
## Modern method type checking

The separate `verify_method_types` entry point proves one canonical registered
method against a particular immutable `VerificationEnvironment`. It first runs
all representation checks above, then supports Java class-file majors 51–65.
Older methods return `UnsupportedHistory` after representation validation; no
historical inference, whole-class proof, linking or execution gate is introduced.

The environment borrows a `DefinitionRegistry` and checked `HierarchyPlan`s. It
checks the closed union, ordered edges, unique runtime packages, initiating-loader
bindings and eight explicitly designated bootstrap core identities. Bindings seed
self, superclass and direct interfaces in first-seen class order, followed by
explicit caller bindings. Conflicting local definitions outside the retained union
also reject. No heap, callback, alternative identity issuer or host-provided
assignment answer is involved. The result borrows this exact environment and
retains its selected ClassId, method index, Code validation and accepted usage.
Independent calls retain no shared proof cache or output history.

```rust
use crabspot_verifier::{TypeVerificationOptions, verify_method_types};
# fn check(env: &crabspot_verifier::VerificationEnvironment<'_>,
#          class: crabspot_runtime::ClassId) -> Result<(), Box<dyn std::error::Error>> {
let proof = verify_method_types(env, class, 0, TypeVerificationOptions::default())?;
assert_eq!(proof.class_id(), class);
# Ok(()) }
```

The checker covers every modern decoded opcode, category-two slots and stack
forms, prescribed maps, all explicit branch targets, mapped dead blocks,
constructor aliases/independent pending-this flags, both constructor exceptional
views, inherited protected receiver facts and selected final overrides. All
handler rows are checked even for instructions without a reachable throw. Handler
maps have one slot assignable from their catch type; Top can discard that value,
but a later pop or astore cannot consume an unusable Top. Structural interface
Object dependencies do not participate in inherited field lookup.

Verification assignment preserves symbolic initiating-loader identities until a
rule needs a binding. Null, exact identities, designated Object destinations and
the specified nominal interface relaxation avoid unnecessary source resolution.
Array covariance and primitive leaves follow verification rules; aastore does not
perform runtime component-store checking. Missing necessary facts return an
explicit unavailable-fact error. Method existence, runtime member staticness,
normal access/linkage, initialization, bootstrap execution and full loading
constraints remain separate obligations. Protected restrictions and final
inheritance rules here are the specific verification checks in this contract.

Environment policies independently bound physical rows/edges, attempted bindings,
UTF-16 scans and work. Per-proof policies bound symbols/cache records, cumulative
materialized cells, edge checks, UTF-16 scans and work, alongside the independent
Code counters. Charges are inclusive and checked; multi-counter funding fails
atomically. Fallible reserves request actual demand rather than policy maxima.
Immutable local prefixes are shared across Same maps, copied only for writes;
identical-prefix comparison uses arena identity. Long spelling hashes are funded
when created/queried and retained as randomized scalar fingerprints, so map growth
does not rescan unrelated retained names. Collisions retain exact name equality.
Errors report class, selected index, phase location and accepted counters without
mutating registry, plans or parsed metadata. Diagnostic allocation follows the
existing host-error convention; the unchanged bounded decoder has no new
recoverable host-OOM guarantee.

External library facts used by tests must be truthful, provenance-bound synthetic
metadata projections of a stated JDK, with complete required hierarchy closure.
Inert admission bodies in such projections are never executed, selected or counted
as genuine verified methods. This library calls no reference JVM. Genuine target
Code, synthetic facts, unavailable selections and reference-JDK stage outcomes
must remain separately reported; success is relative to the supplied immutable
facts and does not claim full JVM verification compatibility.

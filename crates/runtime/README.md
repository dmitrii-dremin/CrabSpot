# Runtime foundations

`DefinitionRegistry` owns immutable parsed class files and assigns opaque,
registry-scoped loader, class, method and field identities. A class identity uses
the exact defining-loader identity and Java UTF-16 binary name in internal slash
form. Names retain unpaired surrogates; display conversion never creates keys.

`define` admits supplied bytes atomically after ownership, name, duplicate,
resource and parser checks. It retains parsed metadata without a second raw-byte
copy. A successful definition remains unverified and unlinked. Declared-member
lookups match exact names and descriptors and do not search superclasses.

`load_from_source` first returns a cached definition when available. On a miss it
asks an explicit source for bytes, with the minimum remaining aggregate,
per-definition and parser byte allowance. It rechecks the returned length even
when the source violates that limit. Absence, source failures, parser rejection,
name mismatch, duplicates, invalid handles and resource policy have separate
structured errors. No failure is cached as an initialized or erroneous Java class.

Default policy permits 256 loaders, 1,024 definitions, 16 MiB per definition and
64 MiB aggregate admitted source bytes. Byte accounting measures input volume,
not exact heap consumption. Ordinary name keys have at most 65,535 UTF-16 units,
the upper bound permitted by a class-file Utf8 entry. Handles have private fields,
checked ownership and bounds, and no removal/reuse path. Only a checked atomic
registry-token allocator is global; the class metadata and loader namespaces are
owned by each registry. Registry construction fails if that token namespace is
exhausted instead of wrapping or reusing an identity.

Module descriptors and array classes are outside ordinary byte definitions.
Filesystem/archive/module-image sources and parent delegation require later
runtime services. Registry admission alone does not validate a hierarchy; the
explicit planner below performs a bounded structural subset. Complete class
derivation, verification, preparation, resolution, initialization and execution
remain later services. The registry neither opens platform paths nor manufactures
Java loader objects or lifecycle proof states.

## Hierarchy and field-layout plans

`build_hierarchy` uses an explicit `HierarchyResolver` to select already registered
definitions in each declaring definition's loader context. `SameLoaderResolver`
performs only same-loader lookup. Delegation requires an explicit resolver policy.
Returned handles and exact UTF-16 names are checked, and each context/name result
is cached consistently using borrowed registry name slices during the build.

Traversal uses bounded, nonrecursive DFS with deterministic dependency-first
completion order. It checks class/interface roles, final direct superclasses and
cycles. Interface Object references remain dependencies subject to superclass
checks, but contribute no inherited instance storage. Missing roots or dependencies
fail instead of synthesizing Object. A failure publishes no plan and leaves the
registry unchanged.

Default hierarchy policy allows 1,024 unique definitions, 8,192 direct graph edges,
65,536 aggregate declared field handles, and 65,536 instance locations per class.
Each definition and its edges/fields are charged once before expansion. A layout
stores its own instance/static FieldIds once, plus a superclass link and inherited
prefix count. It never copies complete ancestor field vectors into every class.
Queries walk a bounded parent chain. Hidden fields retain separate owner-qualified
locations, and statics belong only to their declaring definition.

Locations are logical typed-value slots, not byte addresses or JVM frame slots:
Long and Double fields each occupy one location. Field descriptors remain available
through the registry. No storage/default/reference values are allocated, and
ConstantValue is not assigned by layout computation. Actual preparation and the
initialization protocol remain separate; ConstantValue assignment belongs to
initialization before the corresponding class initializer.

A successful plan proves only these structural/layout checks. Sealed/module/package
derivation rules, final-method override checks, bytecode verification, access checks,
method/interface resolution, dispatch and initialization remain later services.

## Shared values and default static storage

Runtime owns `Value` and `ValueKind`; interpreter reexports preserve the earlier
numeric API. Int/Long values retain their signed widths, Float/Double hold raw IEEE
bits, and representation equality preserves signed-zero and NaN-payload differences.
`Value::Reference` holds an opaque `Reference`. Null has a public constructor;
non-null references are issued only by the managed heap after publication.
`HeapRef` has private heap-identity, index and generation components, with no host
pointer or public unchecked constructor. References occupy one JVM frame slot;
Long and Double occupy two.

`StaticStorage::new` validates the registry ownership of every retained plan class
and field, counts declared static locations before allocation, and materializes
defaults in dependency-first class and declaration order. Instance fields receive
no location, and inherited statics are not copied. The default limit is 65,536
logical locations; Long and Double each consume one storage location. A temporary
scalar-key cache classifies each immutable constant-pool descriptor entry once,
without retaining long descriptor copies per field.

Integral defaults are Int(0) or Long(0); floating defaults have positive-zero bits;
object and array defaults are Reference(null). ConstantValue attributes, including
nonzero numbers and String constants, and explicit initializers remain unapplied.
This allocates default storage, not the full preparation procedure or initialization
protocol. Failed construction does not mutate registry or hierarchy state.

Storage exposes checked reads and field identities, with no public writes or
final-field bypass. `heap_roots` is an allocation-free borrowed iterator of non-null
managed handles. It is empty by construction because default-only static slots
remain null. It does not expose mutable root locations, create a synchronized
snapshot, trace objects, establish safepoints or implement a collector.
This standalone container remains read-only. The separate heap-owned static memory
profile below supplies checked mutation. Initialization scheduling uses a separate
interpreter-owned session and does not change this standalone container.

## Managed primitive-array heap

`ManagedHeap` allocates real zero-filled storage for Boolean, Byte, Char, Short,
Int, Long, Float and Double arrays. Primitive kinds have explicit computational
value kinds and payload widths; Float/Double elements retain raw bits. Boolean
stores use `value & 1`, Byte/Short use signed truncation and Char uses unsigned
truncation. Wrong value kinds and signed indices fail before mutation.

Opaque non-null references contain a unique owning-heap token, arena index and
nonzero generation. Every dereference checks these components. References survive
arena growth and moving the Rust heap value. Heap identity allocation is checked
and never wraps or reuses tokens; dropping one heap cannot validate its references
against a later heap. There is no free, entry-reuse or heap-cloning API.

Default policy permits 4,096 arrays, 1,048,576 elements per array, 4,194,304 total
elements and 33,554,432 payload bytes. Empty arrays still consume distinct entries.
Quota addition/multiplication is checked before fallible typed-payload and arena
reservation. Logical entries and counters are published only after preparation
succeeds. Host reservation failures are structured errors; generous policy is not
a promise that every permitted allocation can be satisfied. Payload accounting
counts element contents, excluding allocator/arena overhead.

The generic service loads/stores the actual array kind. The interpreter's explicit
heap entry point enforces each supported primitive-array opcode's allowed kinds
and maps its supported symbolic Java exception outcomes. `validate_root` checks a supplied
handle without marking or retaining it. Allocated entries are not automatically
roots. Primitive arrays contain no child references. The heap performs no tracing,
collection, reclamation or safepoint synchronization, and has no substitute caller
root set. The raw primitive API does not allocate reference arrays; checked services
are described below. Array class mirrors and general Java startup
remain outside this service. Ordinary instances are an explicitly enabled memory
service described below.


## Ordinary-instance memory service

`ManagedHeap::new_with_instances(HeapOptions, InstanceOptions)` enables ordinary
instances in the same opaque-reference arena as primitive arrays. The original
constructor leaves instance operations disabled. Array counters and all four
array quotas count primitive and reference arrays together. Instance quotas separately
bound retained class shapes, direct dependency edges, own instance fields, object
entries, locations per object and aggregate object locations. Their defaults are
1,024 / 8,192 / 65,536 / 4,096 / 65,536 / 1,048,576 respectively. These are logical
resource limits, not a measurement or reservation of all host memory.

`admit_instance_layouts(registry, &[&plan, ...])` validates the union of supplied
plans against immutable definitions and retained relationships. The first
successful nonempty admission binds one registry. Empty admission is an
unconditional no-op when enabled. Shared ancestors are retained once; later
admission must preserve exact superclass/interface identities and order, fields,
flags and location counts. Metadata stores each class's own fields once, with
parent links and inherited prefixes. Equal names or equal field counts never
substitute for class identity. Interfaces retain Object dependencies without
inheriting Object storage. Admission uses an iterative cycle check and scalar
constant-pool keys for descriptor classification.

Batches stage metadata and check conflicts and quotas before reserving destination
capacity and publishing. Failure changes neither binding, logical counters nor
live-object interpretation. Metadata, object payloads and arena growth use
fallible reservations proportional to actual admitted resources. Generous limits
do not cause eager allocation or guarantee that host allocation will succeed.

`allocate_instance` accepts a retained concrete class and allocates inherited
fields followed by own fields. Long/Double use one logical location each. Every
field receives zero, positive floating zero or null; ConstantValue metadata and
constructors/class initializers remain unapplied. Empty objects still receive
distinct entries. Allocation and superclass walks do not recurse.

`get_field` and `put_field` use owner-qualified FieldIds, preserving inherited
field hiding. They check receiver identity/kind and field relationship before
write finality and value validation. All final writes fail. Nonfinal stores
require exact computational kinds; Z masks with one, B/S truncate and sign extend,
C truncates and zero extends, and floating values preserve raw bits. Reference
fields accept null only through public `put_field`; every raw non-null store is unsupported without
dereferencing its value. A failed write preserves the previous value. Field
services perform no symbolic Fieldref resolution or Java access checks.

`children` is a validated, borrowed iterator over non-null reference-field values;
primitive arrays yield none. Checked context stores can populate these locations;
aliases and cycles are returned as immediate edges in slot order. This iterator is neither caller
roots nor a collector. The raw instance memory API does not perform assignment
checks, field-bytecode execution, constructors or lifecycle transitions; the checked
context described below supplies resolution and assignment. Reclamation remains
unimplemented.

## Field resolution and access context

`FieldResolutionContext::new_unnamed_modules` borrows an immutable registry, a
heap with admitted instance layouts, and an explicit hierarchy resolver. It
selects an unnamed-module profile. The constructor seeds exact loader/name
bindings from the admitted graph, including delegated superclass and interface
identities, and rejects contradictory bindings. New successful owner bindings
are cached; failures remain distinct and may be retried. Names borrow registry
UTF-16 data rather than copying descriptor or ancestor graphs.

The context bounds unique symbols, retained name units, and cumulative lookup
work. Defaults are 16,384 symbols, 1,048,576 UTF-16 units and 1,000,000 steps.
Seeding charges every class and edge; lookup charges each visited node and
compared field declaration. Traversal and ancestry checks are iterative.
Fallible reservations are proportional to actual work. A new context starts
fresh budgets. Heap identity or admitted-class count changes invalidate an
existing context, including replacement through `heap_mut()`; ordinary heap
allocation and field mutation remain permitted.

`read_field` and `write_field` consume a checked Fieldref from an admitted origin
class. Lookup visits own declarations, direct interfaces recursively in their
class-file order, then the superclass. Interfaces do not inherit Object fields.
Access checks preserve defining-loader package identity, same-class private
access, and the specified protected owner/receiver restrictions. Protected
static fields do not impose the additional nonstatic owner/receiver restriction.
Public inherited fields do not require separate declarer accessibility.

Resolution/access/loading-constraint failures precede staticness, write
finality and null checks. Java-shaped failures are explicitly distinguished
from host metadata, resolver, heap and policy failures. Every final write is
rejected; memory-service narrowing and floating bits remain in effect. Null and
numeric stores retain their original universe-free behavior. `check_receiver` validates host-selected
ordinary instance identity without claiming method invocation access checks.

Named modules, cross-loader reference loading constraints, cross-class private
nest access, constructors, verification, class initialization and complete Java
linking remain outside this profile. The runtime has no interpreter dependency.

## Class and interface method selection

`FieldResolutionContext::select_virtual` resolves a checked Methodref, while
`select_interface` requires an InterfaceMethodref. Both select a registered class
or interface MethodId through the same admitted graph, loader/name bindings
and cumulative work policy as field access. There is no dispatch cache or second
resolver. The service returns native/synchronized declarations without claiming
their executable support; a consumer must still prepare and validate Code.

Resolution searches the symbolic class and its superclasses before the complete
interface closure. Signature-polymorphic names in MethodHandle/VarHandle have
their specified priority and remain explicitly unsupported. Interface resolution
checks its own exact declaration, its explicit Object dependency's declared public
nonstatic method, then its superinterfaces. One shared candidate analysis excludes
private/static inherited declarations and removes candidates dominated by stricter
subinterfaces, including abstract redeclarations. Resolution chooses a unique
concrete maximal declaration when present, otherwise the first eligible declaration
in deterministic discovery order. That witness does not determine receiver dispatch.

Access and staticness checks precede null. Private same-class calls retain their
resolved declaration. Other calls select matching nonprivate instance overrides
with package identity including the defining loader. Transitive public/protected
bridges remain reachable through later package declarations. Defensive final
checks include matching ancestors above the resolved method, while private final
declarations do not participate. Abstract selection follows null checking.

Interface-declared targets use the nearest matching nonprivate instance class
implementation before considering defaults, even when that declarer does not list
the interface. Without a class implementation, the full actual receiver interface
closure must contain exactly one concrete maximal default: multiple defaults yield
IncompatibleClassChange, and none yields AbstractMethod. Diamond paths are deduplicated
by identity; private interface calls retain their exact declaration. Interface calls
check receiver membership by ClassId after null and require a selected public/private
method before abstractness. Public inherited members do not require the declaring
interface itself to be accessible.

All scans, receiver ancestry and override-relation work consume the shared lookup
budget before work. Interface edges are charged before enqueueing even duplicate
targets; candidate comparisons include self pairs and charge bounded pairwise
dominance traversals. Temporary storage is iterative and fallibly reserved; no
transitive-closure matrix or permanent candidate cache is retained.
The service does not implement constructors, native binding,
class initialization, verification, named-module/nest services or general Java startup.

## Registered static-call resolution

`FieldResolutionContext::resolve_static` returns the actual declaring static
MethodId through the shared class/interface resolver. Methodref is accepted at
every supported version; InterfaceMethodref requires an origin of major 52 or
later. Symbolic-owner access, full method resolution, member access and the nominal
origin/declarer loader boundary precede the static-category check. A nearer
instance declaration, Object fallback, default or abstract interface candidate
therefore produces IncompatibleClassChange rather than a second static-only search.
Parent-interface static/private declarations are not inherited. Public members of
an inaccessible declaring parent remain usable through a lawful public owner.

Protected access follows the actual member's static flag: a protected static has
no symbolic-owner comparability requirement, while a protected instance result
retains that access check before its category failure. No receiver, override/final
audit, Code preparation, object allocation or class initialization occurs here.
Native/synchronized statics resolve without a claim of executable support.

Every resolution uses the same cumulative signature, node, declaration, interface
edge and candidate work, and the same successful loader/name bindings. Prepared
Code caching cannot replace origin-sensitive authorization. There is no persistent
method-resolution cache. Registered consumers use this path even for same-class
static calls; standalone ClassFile execution retains its earlier same-class service.
Reference-bearing signatures keep their full per-inspection charge and conservative
origin/actual-declarer nominal loading constraint; numeric and primitive-leaf-array
signatures retain their existing policy. Full linking, native binding, nest access
and general JVM startup remain outside this bounded service.

## Ordinary special-call selection

`FieldResolutionContext::select_special` resolves a checked Methodref, or an
InterfaceMethodref from an origin of major 52 or later, for an ordinary nonconstructor
call. Initialization names remain explicitly unsupported. Resolution, access and
resolved staticness precede the defensive symbolic-owner check, then null. Legal
owners are the current type, a class ancestor, a direct superinterface, or the
current interface's exact Object dependency. Other owners produce the host
InvalidSpecialOwner error; this service does not establish bytecode verification.

Superclass calls start lookup at the current class's direct superclass regardless
of the raw ACC_SUPER bit or file version. Other calls start at the symbolic owner.
Lookup selects exact instance declarations (including private ones), skips static
declarations, and searches the prescribed class superchain or interface Object
dependency before shared maximal-default analysis. Access is checked during
resolution, not reapplied to a different selected declaration. The actual receiver's
overrides and additional interface branches cannot alter this lookup; no virtual
receiver-side final guard is introduced here.

The ordinary receiver must satisfy both the current and symbolic types, and the
selected declaring type must also be compatible, by admitted ClassId relationships.
An interface's Object dependency does not make it a class subclass for protected
access. All new node/edge/declaration scans use the existing cumulative work budget;
selection retains successful symbol bindings without introducing another cache.
Native/synchronized methods remain preparation responsibilities. This does not
construct, initialize or mark an instance as having completed a constructor.

## Reference method arguments and results

Runtime-aware consumers obtain compact computational kinds, JVM parameter slots
and a `MethodReturnKind` through `method_value_shape` and
`instance_invocation_shape`. Numeric and reference parameters/results share these
shapes. The latter checks constant-pool structure without resolving the owner;
array-owner eligibility remains a selector check after typed operand extraction.
Shapes neither certify body eligibility nor perform linkage or value assignment.
Prepared consumers retain no nominal names or array descriptor trees per method.

Signature parsing uses temporary borrowed spans and a fixed maximum of 255
parameters. A bounded prefix distinguishes numeric-only descriptors before any
long nominal scan. Each reference-bearing shape, profile, argument or return
operation charges its full UTF-16 descriptor once, including null values and
cache hits. Numeric-only operations add no signature work. Resolving a borrowed
parameter/result span does not charge its name a second time, and shares the
existing fallible binding and cumulative resource policy.

Selectors conservatively reject nominal signatures when the origin and resolved
declarer have different defining loaders. After selection, the reference profile
audits the complete actual receiver superclass/interface graph iteratively. Every
exact matching declaration, including private/static and intermediate declarations,
must share the selected declarer's loader. This is an explicit unsupported boundary,
not general JVM loading constraints or complete preparation. Primitive-leaf array
signatures impose no nominal constraint. Profile receiver Option shape always
matches staticness; only nominal instance profiles add receiver traversal/audit.

`check_method_arguments` validates count and all computational kinds before any
reference resolution. `check_method_return` validates the declared result's Option
and kind. Non-null values validate their actual heap identity first, then resolve
the selected declarer's parameter/result using identity-only access and check
assignment. Inaccessible descriptor types are permitted when the method itself is
accessible. Actual values from other loaders may be assignable. Null skips universe,
target resolution and callbacks after signature/profile charges and constraints.
The helpers do not narrow numeric values or change reference identity, allocate
objects, initialize classes, or independently approve the reference profile.

Incompatible values report a boxed `MethodReferenceMismatch` with the selected
MethodId, argument index or return position, and actual source/declared target
identities. Wrapped type failures preserve their error source; these host failures
are not fabricated Java exceptions. Later failure retains consumed work, successful
bindings and all prior heap effects. Standalone numeric interpreter entry points
continue rejecting reference signatures. Constructors, native binding, full initialization,
full verification, cross-loader loading constraints and collection remain open.

## Reference types and cast services

`ManagedHeap::bind_reference_types` attaches caller-designated bootstrap Object,
Cloneable and Serializable identities to a heap with admitted layouts. It checks
their loader, exact names, public class/interface roles and dependencies, and
requires every admitted graph root to be the selected Object. Publication is
atomic; identical rebinding succeeds and conflicting rebinding fails. Later
incompatible layout batches are rejected before commit. Existing primitive arrays
retain their handles, contents and accounting, and registry-free heap APIs remain
available without a universe.

`ReferenceType` identifies nominal classes/interfaces or structural arrays with a
rank, primitive/nominal leaf and defining loader. `is_assignable` validates both
publicly constructed values before identity shortcuts and iteratively implements
nominal inheritance, interface membership, the three array supertypes and component
covariance through rank 255. Interface-to-Object assignment leaves protected-access
class-only subclass checks unchanged. No array ClassId or mirror is fabricated.

The shared context resolves Class constants using the current origin's loader and
existing borrowed UTF-16 bindings, including array-leaf slices. Targets require
pre-admitted layouts, public or same-runtime-package access, and no initialization.
`check_cast` preserves reference identity on success; `instance_of` returns a bool.
Both validate origin/CP shape before null. Null skips universe, target resolution,
access, callbacks and name scans. Non-null operations validate universe and heap
handles before target resolution and produce explicit ClassCast/IllegalAccess Java
categories; other failures remain checked host errors.

Binding a previously unbound heap invalidates an existing context, as do heap
replacement and additional admission. New target-name scans charge each UTF-16
unit even on cache hits; nominal validation, type reductions, graph nodes and every
edge enqueue share the existing cumulative work limits. Diamond traversal is
deduplicated and fallibly reserved without recursion or permanent type caches.
Constructors, initialization, verification and collection remain outside this service.

## Checked non-null instance-reference fields

`write_field` permits non-null ordinary-instance and primitive-array references
after the established resolution/access/loading-constraint, static/final, receiver
and computational-kind checks. It validates the source universe/handle, resolves
the selected declaration's descriptor using its declaring loader, then applies the
shared assignment relation. Descriptor identity resolution skips cast-style caller
access checks: a public field may expose a type that the caller cannot name. The
existing cross-loader nominal loading-constraint restriction remains in force;
primitive-leaf arrays use the designated bootstrap identity.

Descriptor scans charge every UTF-16 unit, including delimiters, exactly once per
non-null store even on cache hits. Nominal leaves borrow the shared name bindings.
Source/type/resolver/policy failures preserve the old field and immediate children.
An incompatible assignment reports a boxed `ReferenceFieldMismatch` containing
the selected FieldId and exact source/target types; this is a host failure, not a
ClassCastException or ArrayStoreException. Other type failures retain their nested
`TypeAccessError` source and are not reclassified as field Java outcomes.

The context commits through a crate-private operation that rechecks receiver and
source handles, instance/field relationship, nonfinal reference kind and slot bounds
before one mutation. Public `put_field` continues rejecting all non-null references.
Stores allocate no heap entries or payload, do not change quotas or identity, and
do not claim construction or initialization. `children` borrows one handle per
non-null slot, preserving repeated aliases and cycles without recursion, tracing,
deduplication or allocation. The explicit static profile below supplies static
writes; lifecycle protocols and collection remain outside this increment.

## Checked reference arrays

The context's `allocate_reference_array` validates a resolved component identity;
`new_reference_array` first resolves and access-checks a current-origin Class
constant. Both allocate one null-filled vector and retain the complete actual
array type. A nominal component creates rank one; an array component adds one
rank without allocating inner arrays. Rank 254 can produce rank 255; rank 255
components fail before length or quota checks. Typed allocation has no caller
access check and permits abstract/interface components.

`read_reference_array` and `write_reference_array` use the retained component
identity and shared assignment relation. Stores check null, destination kind and
signed bounds before value/type validation. Incompatible values produce the
explicit ArrayStore Java category; field mismatches retain their host diagnostic.
Only destination null/bounds checks and allocation negative length receive their
local Java mapping. Wrapped type errors and corrupt stored/source handles stay
host errors. Reads and null stores do not consume lookup work. Non-null stores
validate source, actual array type and assignment before guarded commit.

Reference arrays share all existing array count, length, element and payload
quotas with primitive arrays. `REFERENCE_ARRAY_ELEMENT_BYTES` is the actual
`size_of::<Reference>()`; checked length multiplication counts the shallow typed
Vec payload. Headers, capacity slack, retained type metadata, arena/allocator
overhead and instance payloads are excluded. It is not total process memory.
Fallible payload/arena reservation completes before handle publication and
counter changes. Failed allocation/store preserves existing state; stores do not
allocate entries or invalidate the context.

Heap `reference_array_type` and `load_reference` expose read-only inspection with
local structural/handle checks. There is no public raw reference-array allocator,
setter or mutable payload. Primitive `kind`/`load`/`store` reject reference arrays;
generic `length` accepts both payloads and reports expected Array for instances.
`children` borrows non-null elements in ascending index order, preserving aliases,
self-edges and cycles without allocation, recursion or tracing. Existing checked
fields and cast/type services accept actual arrays through the same relation.
Mirrors, constructors, initialization, verification and
collection remain outside scope.

## Atomic multidimensional arrays

`allocate_multidimensional_array` accepts a complete checked array type and an
outer-to-inner count slice. `new_multidimensional_array` first resolves/access-checks
a current-origin Class constant, then shares the typed path. Dimension shape is
validated before signs; every requested count is charged and checked, including
counts after zero. A zero stops only deeper allocation. Partial-rank allocation
leaves null components; full primitive leaves receive their existing zero defaults.
Rank 255 is supported without rank increment or host recursion.

Preflight records at most 255 levels and checks each reachable level in order:
cumulative arrays, per-node length, checked aggregate elements, then checked payload
bytes. Primitive rank-one nodes use their actual element width; other nodes use
the reference element width. Existing primitive/reference allocations share the
same limits. Complete node/link work is charged atomically before staging; failure
keeps the precharge counter, arena, counters, old elements and child edges intact.

All payloads are constructed once in a fallibly reserved breadth-first staging
vector. Private future handles connect the exact next-level children and cannot
validate against the live heap before commit. Publication rebuilds the retained
geometry and checks the original heap snapshot, node generations/types/lengths and
every expected child or null slot. Arena capacity for the whole tree is reserved
before moving any entry, then payloads and precomputed totals are published without
further fallible allocation. Private injected-failure and corrupted-staging tests
exercise these boundaries; no public transaction or future-reference issuer exists.

The extra staged entry descriptors and fixed level metadata are bounded by array
count/rank limits. They may coexist with reserved arena capacity and are excluded
from the shallow public payload-byte total. Dropping failed staging or live trees
is iterative because edges are opaque handles. No components are constructed or
initialized, no mirrors are fabricated, and no verification, collection or Java
OutOfMemoryError behavior is claimed.

## Explicit Throwable validation and catch matching

`prepare_exception_profile` validates a caller-designated bootstrap
`java/lang/Throwable` ClassId against the bound reference universe and admitted
graph. The anchor must be a public ordinary nonabstract class directly extending
the designated Object. Validation charges one node, the complete actual UTF-16
name and one direct-superclass edge. It performs no resolution callback, heap
allocation, construction or initialization.

The opaque Copy `ExceptionProfile` retains only the anchor and registry/heap/layout/
universe fingerprint. `check_exception_profile` rechecks that snapshot without
lookup work. Fresh contexts over the unchanged heap may reuse it; allocations do
not invalidate it. A different heap, additional admission, changed universe or
same-count heap replacement cannot reuse an earlier snapshot.

`prepare_throw` first checks the profile, then validates a non-null ordinary
instance and its class-only ancestry to the exact Throwable identity. Arrays,
unrelated instances and invalid handles remain structured host errors. Subclasses
may use another defining loader when their actual admitted superclass chain
reaches this anchor. The opaque `ThrownException` preserves the identical handle
and actual ClassId; it does not certify initialized Throwable library state.

`exception_handler_matches` checks the current declaring origin, token/profile
match and actual instance again. Catch-all requires no type work or resolution.
Typed catches require a non-array Class constant, resolve with the declaring
origin's loader and caller access, and require an ordinary Throwable subclass.
Matching uses the iterative class-only relation. Complete catch-name units are
charged on every examination, including binding hits. Symbol/unit limits apply
before callbacks; consumed work and successful bindings survive later failure.

The interpreter owns ordered range search, lazy examination, frame unwind and
separate dispatch/handler policies. Only reached in-range typed rows resolve;
catch-all, uncovered and later rows cannot cause type resolution. The runtime
maps only direct catch-type IllegalAccess to `Java(IllegalAccessError)`. Nested
type/context failures keep their host category and error source.

The opt-in engine can catch/rethrow supplied instances or report their uncaught
identity. It does not materialize any of the eleven `VmExceptionKind` categories;
they become explicit unsupported host failures in that profile, including null
athrow. Existing entry points retain their symbolic VM outcomes and reject
handlers/athrow. No fabricated default object stands for a VM-produced exception.
Messages, stack traces, constructors, native/monitor/thread startup, asynchronous
exceptions, full verification, collection and general exception production remain
outside this bounded service.

## Heap-owned checked static memory

`allocate_static_state` creates the heap's unique static container from every
currently admitted class/interface, including abstract and zero-field types.
Each declared static FieldId owns one logical location; inheritance creates no
copies and Long/Double use one location each. Initial classes are ordered by
registry declaration index, then fields by declaration order. Defaults are zero,
positive floating zero or null. ConstantValue and actual clinit bodies remain
unapplied: this is host-controlled memory, not Java preparation/initialization.

`StaticStateOptions` defaults to 16,384 classes, 65,536 locations and 8,388,608
payload bytes. Payload is exactly locations times `STATIC_STATE_SLOT_BYTES`, the
actual size of Value, excluding maps, metadata, capacity slack and allocator
overhead. These limits are independent of array and ordinary-instance quotas.
Arithmetic overflow and allocation failures remain explicit host failures.

The first successful allocation fixes options. Repeating identical options with
unchanged layouts consumes no work and preserves all values. Admitted graph
growth requires a fresh context and an atomic extension with the original options.
Only newly admitted classes append locations; old values, field order and root
aliases remain. Zero-field membership is retained so profile generations cannot
be reset or replaced without graph growth. Failed extension preserves the prior
container and permits a later work-budget retry, without restoring old profiles
against the enlarged graph.

Inventory work is charged atomically before collection. Each new declared field
is charged, including skipped instance fields; each distinct scalar CP descriptor
key is charged and classified once without retaining nominal-name copies. Staging
uses bounded, fallible amortized growth. Retained positions must bijectively match
field order; all validation and destination reservation precede publication, and
no callback or fallible operation follows the first published location.

The opaque Copy `StaticStateProfile` binds registry/heap identity and class/slot/
payload counts. Profile checks and host inspection consume no lookup work.
Profiles exclude the reference universe: binding it later requires a fresh
context under existing rules, while the same static profile remains usable.
Static allocation and writes do not invalidate existing exception/type/method
snapshots. Different heaps and same-count heap replacement cannot reuse a token.

`read_static`/`write_static` share field resolution, defining-loader packages,
protected-static access and the conservative nominal loading constraint. Static
category precedes final rejection; every final write fails. Numeric values use
the existing narrowing/masking and raw IEEE-bit rules. Non-null references validate
source identity before the selected declarer's complete identity-only descriptor
and assignment; hidden descriptor types add no caller-access restriction. Null
requires no universe or target resolution. A guarded private commit rechecks
ownership, field/category/finality/kind/position and source handles before mutation.
This body profile exposes no public raw setter, reset or initializer permission.

`static_fields` and `inspect_static` provide read-only owner-qualified inspection.
`static_roots` validates stored values and borrows each non-null handle in field
order, preserving aliases without allocation or child traversal. Corrupt values
are host failures rather than silently filtered roots. Object `children` remains
separate; neither API supplies mutable GC root locations or a synchronized snapshot.

Only direct selector Java categories become StaticJava errors; nested type/context
failures preserve their sources. Opt-in engine profiles enable static bytecodes
and compose with supplied-Throwable dispatch. Existing entry points keep their
gates. Completed writes survive later symbolic Java outcomes, uncaught supplied
references and host/fuel failures. This body profile performs no initialization or
ConstantValue installation. The separate session services below do not add String
materialization, synchronization, general static calls, verification, collection
or VM exception materialization.

## Initialization metadata and checked operations

The interpreter owns opaque `InitializationSession` records, dependency scheduling,
actual initializer frames, completion, abrupt completion and host-interruption
cleanup. Runtime provides plans and operation tokens; it has no lifecycle-success
setter. Existing body APIs retain their host-controlled behavior.

`claim_initialization` validates static and exception profiles, rejects an already
claimed heap, then checks an explicit bootstrap `java/lang/Error` identity. Error
must be a public ordinary nonabstract class directly extending the designated
Throwable. Its one-node/name/edge validation costs seventeen shared lookup steps.
Publication is the last infallible action. A heap retains its unique claim after
the session/claim is dropped; there is no release, reset or replay API. Claims and
plans are opaque non-Clone tokens tied to the registry/heap/profile fingerprints.
Constant-size claim/class checks add no lookup work or resolution callbacks.

`plan_initialization` validates immutable own fields/attributes, numeric
ConstantValue entries and at most one exact clinit()V. Static constants apply even
without FINAL; instance attributes are ignored. B/S sign truncate, C zero truncates,
Z uses the low bit, I/J remain exact and F/D preserve raw bits. Static String
constants reject the entire plan. Descriptors use scalar CP keys and atomic full
UTF-16 charges, without owned nominal trees. Remaining constant/dependency quotas
are checked before staging entries. Failed preparation retains spent lookup work
but publishes no plan or value change.
Historical parameterized initializer descriptors are explicitly unsupported.
Ignored raw initializer flags do not change its semantic static/no-argument role
or relax the eligibility rules for ordinary methods.

Class dependencies begin with the direct superclass, followed by declared-order
interface DFS postorder with identity deduplication. An interface qualifies if it
declares a nonabstract/nonstatic ordinary method, including private, bridge and
synthetic declarations; static-only, abstract-only and clinit declarations do not
qualify. Predicate inspection may stop on its first match, while ancestor traversal
continues. Interface plans themselves have no dependencies. Traversal is iterative,
fallibly reserved and charged before nodes, examined methods and edge enqueueing,
including duplicates. Runtime plans never consult interpreter completion state.

`install_initialization_constants` charges the class's own static range atomically,
validates every retained attribute/CP index, normalized value and destination, then
publishes the batch without fallible work after its first write. Private per-class
ranges keep work linear in own static storage; zero-field ranges and old ranges
survive extension. A plan without constants requires no slot scan. Installed values
alone never certify successful initialization.

Resolved-static tokens retain the actual declaring FieldId and scalar immutable
CP/member fingerprints. Resolution performs normal access/loading/static checks
and final authorization before initialization scheduling. Resumption rechecks its
claim, operation kind, scalar metadata, location and value without replaying the
selector or long names. Non-null assignment retains source-first, identity-only
descriptor rules. Only the exact supported declaring clinit MethodId can authorize
a final destination; helpers, other initializers and all old write_static paths
continue to reject final writes. Runtime's method-context contract is checked, and
the interpreter creates that origin only for actual scheduler initializer frames.

`is_initialization_error` validates the supplied thrown token and tests class-only
ancestry against the explicit Error anchor. Only deliberately elevated direct Java
errors become top-level initialization Java categories; nested Static/Exception/
Context diagnostics preserve host sources. Numeric constant installation, final
stores and prior heap effects are not rolled back on later failure. Error wrapping,
NCDFE materialization, VM-produced exception objects, String interning, constructors,
concurrent initialization, complete preparation/loading constraints and JVM startup
remain outside these runtime services. The session separately owns exactly-once,
recursive-use and erroneous/aborted behavior; no runtime API marks a class completed.

## Checked construction metadata and memory

`resolve_new` validates the permanent initialization claim and caller's Class
constant, rejects array owners, resolves through the existing loader/name policy,
and checks caller access before concrete-class eligibility. Its opaque scalar token
retains the actual class identity. `allocate_new` revalidates that token without
replaying name resolution, funds each admitted superclass shape and actual own
field entry incrementally, then uses the existing atomic default-storage allocator.
Fully funded work remains spent after quota failure; partial funding allocates
nothing. The engine must initialize the class before allocation and tag the returned
storage identity before bytecode or host outcomes can observe it. These services
have no lifecycle or successful-construction setter.

`constructor_invocation_shape` returns the existing compact computational shape;
array-owner refusal stays after extraction at selection. `resolve_constructor`
uses shared symbolic method resolution, access and conservative nominal loading
checks before enforcing the exact constructor declaration owner. An ancestor access
failure can precede NoSuchMethod, but an ancestor constructor is never executed as
an inherited target. Allocation mode requires an exact allocated class; This mode
requires an actual declaring constructor and this/direct-super owner relation.
There is no ACC_SUPER redirection, receiver override audit or second initialization
trigger. The designated Object base is checked lazily after receiver relations;
the interpreter still executes its supplied real constructor Code.

`write_constructor_field` validates the receiver-bound authority, ordinary symbolic
field access/loading, static category and own-declaration final authority before
receiver/value checks. Ordinary receivers may be other compatible instances.
Pre-chain This additionally requires the original bound receiver and exact symbolic
and declared own-field owners. A private short-lived proof rechecks method/field
fingerprints, role, owner-qualified slot/kind/finality, heap/source handles and bounds
before one mutation. Reference assignment keeps source-first identity-only descriptor
resolution and complete per-store work. No callback occurs between proof and commit.
Raw setters and ordinary field contexts continue to reject final writes.

Public Value and Reference remain ordinary values. The interpreter privately owns
unfinished allocation/activation tags, chain obligations, alias promotion and abrupt
invalidation; runtime tokens do not certify frame provenance or completion. Failed
and discarded allocations retain arena quota, and lawful prior writes/escaped
references persist. Only direct intentional Java access categories are elevated;
nested errors preserve host sources. InstantiationError remains an unmaterialized
VM category. Full verification, constructor-time exception creation, GC and Java
startup remain outside this bounded runtime service.

## Checked loading domain and preparation constraints

`LoadingDomain` is an opt-in owner of a fresh `DefinitionRegistry`. Its shared
`registry()` view can build ordinary hierarchy plans and verification environments;
no mutable getter, registry import, extraction or reset can bypass the checked
history. A `LoadingContext` borrows the domain mutably and tracks evaluation work.
Creating a new context resets only that evaluation allowance. Retained-name, pair,
actual-initiation and union quotas remain fixed for the domain's lifetime.

Each direct definition records its defining loader as an initiating loader.
`record_initiation` records explicit delegation after checking handles and the
canonical name. `lookup_initiated` returns only an actual record. A component may
already require a ClassId while a particular loader still returns None. Likewise,
two independent actual records of the same ClassId do not become `constrained`
until their equality components are explicitly joined. Reflexive equality creates
no retained entry. Names preserve exact UTF-16 units under the ordinary registry
name grammar, including classes literally named I or V.

Definition admission and loading bookkeeping form one fail-before-publication
operation. An existing actual initiation rejects duplicate derivation before
parsing. Otherwise normal registry admission checks run before a late component
identity conflict. The parser's established diagnostics and allocation conventions
are preserved. Added names, indexes, pairs and staging use fallible actual-demand
reservations. Scalar stage stamps, future positions, capacity and root/identity
checks precede publication. No provisional ClassId escapes a rejected derivation;
logical counters, old bindings and the next definition index remain available.
Spare capacity may survive a rejected reserve. The registry publishes only after
all added bookkeeping work can commit without a recoverable failure.

`prepare_constraints(plan)` validates every root/row/direct-edge prerequisite
against actual initiating records before imposing anything. An interface's Object
metadata edge participates in this check. An implied component identity cannot
stand in for a missing actual initiation. It then streams the two JVMS 5.4.2
producers: declared overriding methods constrain the selected class loader against
each overridden declaration loader; inherited interface implementations constrain
the selected declaration loader against the originating interface declaration
loader. Private/static/special exclusions, loader-qualified package bridges,
abstract class selections, abstract default suppression and unique nonabstract
maximal defaults follow those preparation rules. Invocation access, receiver,
category/finality checks are separate. The existing interpreter's conservative
signature guard is unchanged.

Each emitted descriptor is walked with borrowed spans, parameters left to right
then result. Nominal array leaves constrain their ordinary names; primitive leaves
and void do not. No signature type is force-loaded. Every successful individual
leaf stays imposed if a later leaf, selection, budget or allocation fails. The
failing insertion changes no logical state. Replaying preparation pays inspection
work again and issues no prepared-class lifecycle bit. Registry, wrong-return,
duplicate-derivation and constraint conflicts retain distinct structured categories;
no Java exception object is allocated. Preparation errors carry the applicable
root, origin/related MethodIds and parameter/result position without copying long
names. Completed obligations do not leave stale leaf sites on later failures.

Persistent resources are charged atomically for actual additions. Context UTF-16,
work and cumulative scratch batches are also atomic, with inclusive u64 limits.
Physical row/edge/candidate loops, CP reads, string comparisons, roots and graph
work are funded before inspection or enqueue. Owned names occur once in a flat
arena. Randomized fingerprint indexes rehash scalar keys, while collision equality
still checks exact spelling under paid UTF work. The union forest uses size-based
joins and deterministic ties without path compression, so failed queries cannot
change future walk costs. Correct trees have logarithmic root walks; defensive
walks stop within the actual pair inventory or earlier work exhaustion.

Preparation retains flat borrowed row views and a scalar ClassId index. Its
prerequisite scan is linear in supplied rows/edges and their funded names.
Declared-producer work is bounded by root declarations times inspected ancestor
and interface declarations. Inherited selection may additionally compare matching
interface candidates pairwise and walk strict interface ancestry for each pair;
every such pair/node/edge and each pending/visited entry is funded. Thus hostile
repeated/diamond scans exhaust explicit work/scratch before growing unbounded
state. No method graph or whole-obligation list is cached, policy maxima are not
allocation sizes, and graph/array/union depth causes no host recursion.

This service is a checked loading ledger and the preparation-time constraint
subset, not a Java class-loader executor or complete linker. It performs no
classpath/source callbacks, verification caching, module/nest checks, automatic
hierarchy initiation, static allocation, initialization, unloading or concurrency.
Reference-JDK trigger observations are reported separately from the strict
preparation rules, including known stage/timing differences; they are not results
of an application execution service here.

## Checked member resolution

`LoadingDomain::enable_member_resolution` opts into a persistent, domain-owned
CP-member ledger. `LoadingContext::prepare_member_owner` mints a root witness
only after complete actual-initiation preflight and both preparation constraint
producers succeed. Earlier successful leaf constraints survive a later failure;
a failed root gets no witness. Ordinary `prepare_constraints` does not mint one.
Every admitted heap class, including interfaces and zero-slot classes, needs its
own witness before `member_context_unnamed_modules` opens a checked context.
Opening checks the actual registry inventory, ordered heap graph and permanent
initiations. It invokes no resolver callback and does not infer initiation from
an equality component's known definition.

Checked contexts share the existing field, static, method and constructor
services. A terminal key is the current declaration's `ClassId` plus its exact CP
index. Cold resolution reserves a site before searching or imposing descriptor
constraints. It records the actual symbolic owner and declaring member, or a
permanent direct `NoSuchField`, `NoSuchMethod`, `IllegalAccess`, symbolic-owner
`IncompatibleClassChange` or `LinkageError` failure. Host interruptions are
retryable and retain only earlier committed constraints. A warm hit revalidates
the current context and required heap memberships; it does not repeat resolution
or descriptor obligations. Aliases in different CP slots remain distinct sites.
No whole-instruction result is cached: category, receiver/protected restrictions,
selection, final authority, constructor roles, initialization and argument/result
checks still run on each use. A warm success cannot supply a missing current
layout. A terminal failure remains replayable on a smaller compatible heap.

For a selected body under a different defining loader, each nominal signature
leaf must already be constrained between resolved and selected declarers, or have
two actual initiations to the same permanent identity. Otherwise selection fails
with host `UnprovenSelection` while preserving CP success; it inserts no relation.
Direct `MethodId` roots acquire no invented symbolic-resolution obligation.
Checked private access between different runtime packages is permanently denied;
distinct owners within one runtime package retain the unsupported-nest boundary.
Legacy contexts keep their conservative cross-loader and nest restrictions.

Prepared witnesses and terminal sites have inclusive `u64` caps. A cold addition
pays its probe, capacity check and paired Work/Scratch reservation before fallible
allocation. Warm resolution pays one loading Work probe; separately metered
per-use work still applies. Full descriptors and emitted names use the accepted
loading UTF-16/work accounting. The duplicate runtime symbol cache stays empty in
checked mode. Scalar cache keys avoid rehashing borrowed descriptor spellings.
Private publication guards and reservation failpoints preserve nonempty old
state. This is checked resolution of supplied metadata in unnamed modules, not a
class-loader executor, module/nest implementation, verifier or complete linker.
Direct linkage failures have symbolic categories; no exception materializer is
provided, and checked exception execution retains its explicit unsupported VM
exception policy.


## Checked application-main selection

A domain-backed `FieldResolutionContext` can mint a private
`ResolvedApplicationMain` with `resolve_application_main(named)`. It preserves
both the named class and the actual declaring MethodId. Class lookup examines
public exact `main([Ljava/lang/String;)V` declarations nearest first along the
superclass chain. A named interface searches only itself. Static interface methods
are not inherited by implementing classes. Private and nonvoid-only methods are
not entries; an exact public instance/native/abstract/synchronized declaration is
an explicit wrong-flags error, with no fallback past it.
`resolve_application_main_with_monitors` shares the same lookup, charges and token
affinity but permits synchronized Code; native/abstract/nonstatic eligibility is
unchanged. The token grants no monitor capability by itself, so an ordinary
unmonitored interpreter session still rejects that synchronized body. The named class itself
need not be public or instantiable.

Each class, examined member and full compared UTF-16 span consumes the existing
field lookup budget. Selection creates no member CP site, equality, object or
initialization record. `check_application_main` rechecks the opaque token's
checked-mode registry/heap/layout affinity and canonical handles without another
name lookup or additional lookup charge. A new heap, stale admitted snapshot,
foreign handle or legacy context cannot consume it. `ApplicationMainError` keeps
structured eligibility, context and token failures.

This token certifies this bounded lookup, not whole-class verification. The
launcher independently proves its complete executable image before selecting one.
The existing direct reference-array allocator can create a real empty array with
the bootstrap String ClassId without inserting a synthetic CP entry or initializing
String. It provides no Java String materialization or general startup service.


The runtime's additive `VmExceptionKind::IllegalMonitorStateException` identifies
an unmaterialized failure from the optional interpreter monitor profile. Monitor
ownership itself remains interpreter-private; this adds no runtime lock handle,
Class mirror, Java thread, scheduler or exception allocator.

## Canonical Class mirrors

A checked domain can enable a separate Class-CP outcome store with
`enable_class_mirror_sites`; its rows hold canonical `ReferenceType` metadata or
permanent access denial, never heap references. A pristine instance heap with a
bound reference universe may bind the final bootstrap `java/lang/Class` role.
Binding seals its admitted graph against growth and ordinary allocation of that
Class identity. Matching redeclarations are idempotent; reset and replacement are
not supported.

`resolve_class_mirror_type` resolves the current origin's actual Class CP slot,
including ultimate nominal array-component accessibility. Each slot has its own
outcome; aliases do not merge CP costs. Host missing facts and resource failures
are repairable and are not cached Java loader failures. Permanent denial can
replay with only the admitted origin on a smaller compatible heap. Successful
hits revalidate the current universe and canonical leaf identity. Legacy Class
CP consumers keep their earlier contracts.

`class_mirror_for_type` issues one VM payload per loader-qualified nominal or
array type per heap. Its runtime type is the designated Class. The canonical
index and issuance-order root vector retain strong references; `children` has no
Java backing-field edges for this opaque payload. A mirror counts as one managed
instance and zero Java instance slots, with no array/payload-byte debit. Each
miss checks mirror and instance caps, funds provisioning and reserves arena,
index and roots before any publication. Hits allocate nothing, including at a
full cap. `ClassMirrorLimits` independently bounds heap mirrors and domain sites;
all name/type/probe/provisioning work uses the existing checked context budgets.

Mirrors participate in ordinary reference fields, statics, arrays, assignments,
casts and equality. Raw and checked instance-field operations reject their
opaque Class state after the established symbolic access/category/receiver
checks. `check_class_mirror_method_entry` also guards direct preselected Class
instance bodies; the common method profile calls it for numeric signatures too.
Holding a mirror in an application object's field does not restrict that object.

The sole native classifier recognizes the immutable declared bootstrap
`Object.getClass()Ljava/lang/Class;` with the required public final native
instance role. Immutable parsed registry admission already excludes Code on that
native, so classification does not scan arbitrary extension attributes. The
existing selectors handle inherited Object, interface fallback, nominal special
calls and the narrow array-symbolic getClass case. Structural array owners remain
in private domain records and must agree with their original Class CP outcome.
Array covariance and actual marker-interface membership are checked per use;
a cached method never authorizes an incompatible receiver or arbitrary native.
The interpreter owns native invocation and monitor-key normalization; this crate
provides no JNI or general native dispatch.

Mirror allocation and type resolution do not initialize Class or the represented
type. No Class reflection state, primitive/void mirrors, Java loader execution,
exception materialization or privileged initialization completion is supplied.
Legacy unbound heaps retain ordinary low-level allocation behavior; a preexisting
ordinary Class instance prevents a later VM binding rather than being retyped.

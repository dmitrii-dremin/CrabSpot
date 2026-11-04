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
Static-storage mutation, initialization execution and field bytecodes remain later work.

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
root set. Reference arrays, array class mirrors/assignability and general Java startup
remain outside this service. Ordinary instances are an explicitly enabled memory
service described below.


## Ordinary-instance memory service

`ManagedHeap::new_with_instances(HeapOptions, InstanceOptions)` enables ordinary
instances in the same opaque-reference arena as primitive arrays. The original
constructor leaves instance operations disabled. Array counters and all four
array quotas continue to count only primitive arrays. Instance quotas separately
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
fields accept null only; every non-null store is explicitly unsupported without
dereferencing its value. A failed write preserves the previous value. Field
services perform no symbolic Fieldref resolution or Java access checks.

`children` is a validated, borrowed iterator over non-null reference-field values;
primitive arrays yield none. Public instance children currently remain empty
because non-null field stores are unsupported. This iterator is neither caller
roots nor a collector. No reclamation, reference arrays, assignability,
constructor protocol, lifecycle proof, field bytecode or interpreter instance
bridge is implemented by this memory service.

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
rejected; approved memory-service narrowing, floating bits and null-only
reference stores remain in effect. `check_receiver` validates host-selected
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

## Ordinary special-call selection

`FieldResolutionContext::select_special` resolves a checked Methodref, or an
InterfaceMethodref from an origin of major 52 or later, for an ordinary numeric/void
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
Reference-array allocation, non-null reference stores, reference call signatures,
constructors, initialization, verification and collection remain outside this service.

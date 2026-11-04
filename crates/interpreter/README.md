# Method-body interpreter

`execute_static_method_body` evaluates a selected ordinary static method body
with computational `Int`, `Long`, `Float` and `Double` arguments. It returns a normal
result or a symbolic uncaught `ArithmeticException`. An explicit heap entry point
also evaluates primitive arrays with supported symbolic array exceptions. This kernel does not perform Java
invocation, class loading, class initialization, verification or exception-object
allocation.

The selected standalone method must have exactly one Code attribute, a numeric/void
descriptor, and no native, abstract or synchronized declaration or exception
handlers. Initialization methods are excluded. The decoder validates the entire
instruction stream before evaluation. Unsupported instructions fail when reached.

## Executed instruction coverage

The original numeric entry points handle 148 effective opcode values. The explicit
heap entry point adds 31, for 179 total. The runtime-aware instance entry point
adds `getfield`, `putfield`, `invokevirtual`, `invokeinterface` and ordinary
`invokespecial`, plus `checkcast`, `instanceof`, `anewarray`, `aaload` and
`aastore`, `multianewarray` and `areturn`, for 191 total. `wide` is an encoding prefix for indexed
loads/stores and increment; heap mode adds two reference local forms to the nine
numeric wide forms.

| Group | Supported instructions |
| --- | --- |
| Constants | `nop`, `iconst_m1` through `iconst_5`, `lconst_0/1`, `fconst_0/1/2`, `dconst_0/1`, `bipush`, `sipush`, int/float `ldc/ldc_w`, long/double `ldc2_w` |
| Locals | Int/long/float/double loads and stores, their `_0` through `_3` forms, `iinc`; all nine indexed operations also support `wide` |
| Stack | `pop`, `pop2`, `dup`, `dup_x1`, `dup_x2`, `dup2`, `dup2_x1`, `dup2_x2`, `swap`; all 16 legal category forms have direct tests |
| Arithmetic | All four numeric types: add, subtract, multiply, divide, remainder, negate; integer/long and/or/xor |
| Shifts | `ishl`, `ishr`, `iushr`, `lshl`, `lshr`, `lushr` with masked int distances |
| Conversions/comparison | All 12 conversions between int/long/float/double; `i2b`, `i2c`, `i2s`; `lcmp`, `fcmpl/g`, `dcmpl/g` |
| Control flow | Six unary int conditionals, six int comparison conditionals, `goto`, `goto_w`, `tableswitch`, `lookupswitch` |
| Bounded body calls | `invokestatic` only for ordinary static numeric methods declared by the supplied class/interface; no inherited or cross-class lookup or runtime initialization |
| Returns | `ireturn`, `lreturn`, `freturn`, `dreturn`, `return`; int returns narrow declared byte/short/char/boolean results |

`execute_static_method_body_with_heap` and `execute_instance_method_body` additionally enable:

| Group | Added heap-mode instructions |
| --- | --- |
| Reference data | `aconst_null`, `aload`, `astore`, their `_0` through `_3` forms, and both indexed `wide` forms |
| Identity/null branches | `if_acmpeq`, `if_acmpne`, `ifnull`, `ifnonnull` |
| Primitive arrays | `newarray`, `arraylength`, `iaload/laload/faload/daload/baload/caload/saload`, and their seven corresponding stores |

Locals distinguish empty, value and category-two continuation slots. A write to
either half of a long or double invalidates its previous pair. Operand-stack capacity counts
JVM slots. Invalid bounds, value types, categories, returns and selection conditions
produce structured errors with bytecode offsets where applicable.

The instruction budget is shared by every frame and counts each executed
instruction, including calls and returns. A
zero budget prevents execution of the next instruction. Exhaustion is a host
resource-policy error, separate from Java arithmetic exceptions.

Calls use an explicit frame vector with no recursive Rust evaluation. Arguments
are transferred in descriptor order, category-two values consume two local slots,
and remaining caller operands survive. Return values are checked against caller
stack capacity. A descendant's supported uncaught exception stops the entire
body graph. Prepared methods are decoded once per evaluation and cached by their
a tagged standalone declaration index or a full registered MethodId; the cache
is not a runtime constant pool or class loader.
Symbolic owner matching uses semantic Java names, including constant-pool aliases,
and target lookup matches both name and descriptor. Methodref owners must be
classes; InterfaceMethodref owners must be interfaces with class-file version 52
or later. Missing declarations are explicit errors and never trigger inherited
lookup.

`execute_static_method_body_with_limits` adds `CallLimits`. The original entry
point delegates with defaults: 256 active frames, 1,000,000 aggregate frame slots,
and 1,048,576 prepared Code bytes. The root frame counts. Every active frame
reserves its full `max_locals + max_stack` capacity before allocation and releases
that reservation on return. Prepared Code bytes are charged once before decoding
and remain charged after return. These checked policies count specific resources;
they are not a general heap-allocation quota. Diagnostics include selected method
identity and an available caller location. Their boxed optional payloads allocate
only when an error is reported, not on every frame transition.

Float/Double values contain raw IEEE binary32/binary64 bits. Their Rust equality
compares representations: signed zeros differ, and identical NaN payloads compare
equal as data. Java comparison bytecodes use numeric ordering and prescribed NaN
results instead. Constructors/accessors convert explicitly to/from host f32/f64.
Constants, locals, stack operations and returns preserve payload bits.

Arithmetic materializes each instruction's declared width before the next
instruction. There is no fused multiply-add substitution. Floating division by
zero yields floating results, and remainder uses truncating-quotient semantics.
Float-to-integer casts truncate and saturate, with NaN becoming zero; long-to-float
conversion rounds directly without an intermediate double. Arithmetic NaN payload
and sign are not a Java portability guarantee; differential tests classify these
NaNs while comparing all other results by exact representation. Execution assumes
ordinary host rounding and subnormal modes and does not modify process FP controls.

## Explicit primitive-array heap

`execute_static_method_body_with_heap` takes the class, method index, numeric
arguments, instruction options, call limits and a caller-owned `ManagedHeap`.
Every frame shares that heap. The old entry points create no hidden heap and
continue rejecting all added heap-mode instructions. Parameters and returns remain
numeric/void, including callees; array/reference descriptors and `areturn` stay
unsupported.

The bridge checks each load/store opcode's allowed primitive kind before element
access. Byte and Boolean share `baload/bastore`; other kinds retain their distinct
opcodes. Narrow conversion and floating-bit storage use the runtime heap service.
Reference locals preserve category-two overlap invalidation, and identity branches
compare opaque reference identity rather than contents or host addresses.

Null dereference, signed array-index failure and negative allocation length produce
symbolic uncaught `NullPointerException`, `ArrayIndexOutOfBoundsException` and
`NegativeArraySizeException`. Null precedes bounds, and negative length precedes
allocation quotas. These outcomes contain no fabricated Java exception object.
Malformed values, invalid identities, wrong opcode/kind combinations and heap
policy/reservation failures remain structured host errors with method/call-site
context. Heap exhaustion is not reported as a Java OutOfMemoryError.

Each instruction consumes shared fuel once. Heap quotas independently bound byte
allocation. Completed allocations and stores persist after returns, exceptions,
budget exhaustion and host errors; evaluation is not a heap transaction. Unreachable
arrays remain in this noncollecting heap, and repeated evaluations can exhaust its
cumulative quotas. Callers may explicitly provide a fresh heap for isolation.

## Current boundary

`Value` and `ValueKind` are owned by runtime and reexported here. Reference data is
enabled only inside the explicit heap or instance body graph; a reference argument supplied
for a numeric parameter produces a typed error in every entry point.

Static fields, generalized runtime invocation,
generalized cross-class static calls, constructor execution,
exception handlers, monitors and legacy subroutines remain unsupported. Numeric
execution never delegates application bytecode to an installed JVM. The reference
fixture integration compares results against separately generated OpenJDK records.
Dynamic constants remain explicitly unsupported, including their resolution and
full opcode/category classification. This numeric method-body kernel is an M2
increment, not a complete M2 interpreter.


## Explicit instance bodies and fields

`execute_instance_method_body(context, MethodId, receiver, arguments, options,
limits)` selects a registered ordinary non-static class body through a reusable
`FieldResolutionContext`. The host supplies a checked non-null ordinary instance
of the declaring class or a subclass. Receiver local zero precedes the
arguments at their normal widths. Argument diagnostics keep their original
argument indices. Private ordinary bodies may be selected explicitly; this is
not root method access checking, virtual dispatch or Java invocation. Its body
can perform the bounded virtual calls described below.

Instance mode reuses the same decoder, prepared-method cache, explicit frame
stack and instruction loop. Static helpers still start their arguments
at local zero and must be declared by their current caller’s class. Every
frame’s field origin is its declaring ClassId, including when the receiver is a
subclass or an earlier frame belongs to another class. Cached instance preparation cannot make the root a valid invokestatic
target. Root and callee limits/fuel have the same meaning as the earlier APIs.
Interface roots, special initialization names, native/abstract/synchronized
methods and handlers remain excluded.

Only this runtime-aware API enables `getfield` and `putfield`. Earlier APIs reject
them even with a configured instance heap. Field operations delegate symbolic
resolution, access and receiver relationships to the context’s explicit unnamed
module profile. Exact owner-qualified storage preserves inherited hiding, raw
floating bits and the approved Z/B/C/S store conversions. Final writes fail;
reference fields retain null defaults and support the checked stores described
below. Reference locals,
identity branches and all supported primitive-array operations remain available.

Explicit field Java failures become symbolic `NoSuchFieldError`,
`IllegalAccessError`, `IncompatibleClassChangeError` or the existing
`NullPointerException`. Resolution/access, staticness and write finality precede
bytecode null dereferences. Other context failures—including unsupported nest or
loading constraints, invalid receivers, changed layouts and resource exhaustion—
remain boxed `ExecutionErrorKind::FieldAccess` errors with method, instruction and
available caller provenance. These outcomes allocate no Java error objects.

Each field instruction consumes one shared fuel unit. Completed field writes and
array allocations persist after every return, symbolic failure or host error;
failed writes preserve their previous values. This bridge does not implement
`new`, constructors, getstatic/putstatic, class initialization,
exception dispatch, monitors, named modules, full verification or complete linkage.


## Bounded virtual body calls

Runtime-aware instance mode additionally enables `invokevirtual` for supported
ordinary class methods. The unchanged host API selects the root directly; nested
virtual instructions use `FieldResolutionContext::select_virtual` to resolve and
select a checked registered MethodId. Earlier static entry points still reject
this opcode, including when supplied with a configured heap.

Each registered frame and prepared-code cache entry has its complete MethodId.
Equal local method indices in different classes remain different bodies and
consume separate prepared-code allowance. Legacy standalone APIs keep their own
tagged index identities and manufacture no runtime handles. Every frame uses its
own declaring class's constant pool, return descriptor, field origin and static
helper policy. Every invocation path checks static/instance mode on cache hits.

Virtual arguments are extracted as typed values from the call-site descriptor.
The receiver occupies target local zero; arguments follow at their usual
widths. Calls and callee instructions consume the same shared fuel. Frames share
limits, context lookup budgets, heap effects and the existing iterative call loop;
recursive and cross-class graphs do not recurse through the host stack. Numeric
and void returns transfer to the actual caller while preserving other operands.

Runtime selection supplies the bounded class-resolution/access/override policy,
including private nonvirtual selection, package-aware transitive overriding and
abstract-target failure. Signature-polymorphic execution and native binding remain explicit unsupported
boundaries. Interface/default dispatch is supported within the profile below. Selected
native/synchronized/handler bodies are rejected before execution. Resolution and
staticness precede bytecode null; null precedes abstract-target selection.

Explicit method Java failures map to symbolic `NoSuchMethodError`,
`AbstractMethodError`, `IllegalAccessError`, `IncompatibleClassChangeError` and
`NullPointerException`. All other method-service failures remain boxed
`ExecutionErrorKind::MethodAccess` errors. Nested context errors are not remapped
as Java outcomes. Diagnostics include optional registered owner identities for
both method and call-site records; standalone owners are None. A call-site record
identifies the originating invocation, including return-transfer failures, and
need not identify an ancestor of the method reporting the error.

Earlier completed writes and allocations survive every failure. This remains an
explicit body-graph evaluation profile, without constructor execution, new,
generalized cross-class
static calls, lifecycle proof, exception dispatch, complete linkage or JVM startup.


## Interface and default bodies

Runtime-aware mode enables `invokeinterface` through the existing public instance
entry point and shared frame loop. `select_interface` consumes an
InterfaceMethodref; `select_virtual` still consumes a Methodref and can resolve
interface defaults through class fallback. Runtime candidate analysis preserves
class priority, maximally-specific defaults, abstract redeclarations and diamond
identity. Public Object fallback requires explicit admitted metadata; this kernel
supplies no implicit Java library implementation.

Typed arguments and receivers follow the same call path as virtual class calls.
At a reached invokeinterface, the encoded count must equal one plus the
parameters' JVM slot widths. Long/Double contribute two. Correct CP kind, method
name and supported descriptor are checked first; a count mismatch or a
required count above 255 is InvalidOperands at the invoke offset, before argument
extraction or runtime resolution. Reference parameters contribute one slot.
The decoder eagerly rejects zero counts and nonzero reserved bytes, including
unreachable encodings. Unreachable CP/count semantics are not verified.

Preparation explicitly distinguishes static bodies, direct instance roots and
runtime-resolved instances. Direct interface roots stay rejected. Selected
concrete interface defaults and same-interface private helpers may execute;
staticness, descriptors, special names, native/synchronized bodies, handlers and
whole-code decoding retain their earlier guards. Static versus instance mode is
checked on every cache hit. The same MethodId reached through invokevirtual and
invokeinterface consumes one prepared-code charge. Interface bodies use their
own constant pools, field/method origins and owner-aware diagnostics. Their own
static helpers retain InterfaceMethodref version/kind checks.

Interface-to-interface and default-to-class calls share instruction fuel, active
frame/slot/code limits, cumulative resolution budgets and persistent heap effects.
Only explicit MethodJavaError values become symbolic Java outcomes. Resolution,
access and resolved staticness precede null; null precedes interface membership
and selection. Nonimplementing ordinary receivers, conflicting defaults, absent
concrete targets and selected visibility follow the runtime's bounded contract.
Other invalid handles/metadata, unsupported profiles and policy failures remain
structured host errors. Constructors, initialization, native binding, handlers
and monitors remain separate work.


## Ordinary special calls

Runtime-aware instance mode enables nonconstructor `invokespecial` with the same
host entry point, context and frame loop. Calls use `select_special` from the
current frame's declaring class or interface. Methodref is supported throughout
the parser's version range; InterfaceMethodref requires the origin to have major
version 52 or above. Earlier standalone and heap-only entry points still reject
b7. Wrong CP kind/version and initialization names fail
before typed stack extraction. `<init>` and `<clinit>` remain explicitly
unsupported: no constructor execution or uninitialized-reference transition is
performed, and storage is never marked constructed or initialized.

The runtime separates symbolic owner, resolved declaration and prescribed lookup
start. Explicit superclass calls start from the origin's direct superclass when
required, even if the reference names an older ancestor or raw ACC_SUPER is clear
in an old file. Direct-superinterface defaults, current-type and private current
type calls retain exact identity checks. Selection uses that prescribed type,
not an override or unrelated interface on the actual receiver. Receiver checks
require compatibility with both current and symbolic types. Invalid structural
owners remain MethodAccess host errors, while supported resolution/access,
staticness, null, abstract-target and default-conflict failures keep their Java
outcomes and ordering.

Special, virtual and interface paths share one prepared entry per MethodId.
Static-versus-instance eligibility remains checked on every cache hit. Selected
bodies use their own constant pool, field origin and diagnostics; subsequent
virtual/interface calls dispatch normally and current-class static helpers retain
their narrow policy. Receiver local zero, mixed numeric widths, narrow/FP/void
returns, remaining caller operands, fuel and frame/slot/code/work limits keep the
earlier rules. Code preparation remains eager and excludes native, synchronized
and handler bodies. Existing writes and allocations persist after every later
Java or host failure. No new public execution API, constructor/lifecycle proof,
class initialization, general static resolution, native
binding, monitors or exception dispatch is supplied by this increment.


## Reference type tests and casts

Runtime-aware mode enables `checkcast` and `instanceof` through the existing
instance entry point. The caller explicitly binds a checked
`ReferenceTypeUniverse` to the heap before non-null type operations. The universe
identifies admitted Object, Cloneable and Serializable definitions and their
bootstrap loader; names do not substitute for identity. This structural test
profile supplies no Java library, class mirrors or initialization proof.
Ordinary method/field/array execution still works without a bound universe.

Both instructions first pop a typed reference and delegate to the shared context
with the current frame's declaring ClassId and CP index. The runtime validates
context/origin and the Class constant shape even for null. Null checkcast retains
null and null instanceof produces Int(0), without requiring a universe, resolving
or accessing the target, inserting a symbol or calling the resolver. For non-null
values the universe and managed handle are checked before target resolution and
access. Successful checkcast preserves the identical reference and other stack
operands; instanceof produces Int(0/1).

Sources are admitted ordinary instances or allocated primitive/reference arrays. Nominal
class/interface ancestry and structural array-component relations use exact
registry/loader identities. All eight primitive array kinds retain distinct
components; arrays have only the explicitly selected core supertypes. Array
ranks through 255 use the same structural representation as retained reference
arrays. Target classes/interfaces outside receiver ancestry require their
own admitted hierarchy plans before context creation. Rebinding a previously
unbound heap invalidates an existing context; construct a new one afterward.

Only explicit TypeJavaError::ClassCast becomes symbolic `ClassCastException`;
explicit IllegalAccess uses the existing symbolic error. Other failures stay
boxed `ExecutionErrorKind::TypeAccess` host errors with method, instruction and
transition-site provenance and useful Error sources. Wrapped context Java errors
are not inferred as type-instruction Java outcomes. Malformed stack failures
precede runtime CP checks; complete-code decoding stays eager while unreachable
CP semantics remain unverified. Earlier numeric and heap-only entry points still
reject these opcodes.

Each instruction consumes one shared fuel unit. Type resolution and relations
share the existing cumulative symbol/name/work limits and do not introduce a
second type cache. Earlier stores and array allocations persist after cast,
access, resolution, budget or malformed-operand failures. Constructors,
initialization, class mirrors, native binding, monitors, verification and Java
exception-object/handler protocols retain their earlier boundaries.


## Checked reference fields

The existing runtime-aware putfield now accepts assignment-compatible non-null
instance and array references through `FieldResolutionContext::write_field`.
Checked field stores added no opcode or execution API. Current runtime-aware
coverage is 191 effective opcodes, including the reference-array instructions
below. The shared engine still forwards the actual computational Value, typed
receiver and current declaring origin to the field service. Reference arguments
and results use the checked method profile below.

Field resolution/access/loading constraints, staticness, write finality and
receiver checks retain their earlier order. Wrong computational kinds remain
ordinary field-kind host failures. Numeric and null stores still require no
reference universe or descriptor-type resolution. Non-null reference stores require
a bound universe and valid source identity, then resolve the selected declaring
field's immutable descriptor using its defining loader and apply the shared
assignment relation. An accessible field may have a type inaccessible to the
caller; descriptor identity resolution adds no cast-style caller-access check.
The existing cross-loader nominal loading-constraint boundary remains in place.

Successful writes store the exact handle, allowing self-links, aliases, cycles,
overwrite and null clearing. Retrieved values can feed locals, casts, field/array
operations and virtual/interface/ordinary-special receivers. Inherited and hidden
fields keep owner-qualified locations. Every failed check leaves the previous
value and children unchanged; consumed work and successful name bindings may
remain. Shared descriptor/source/relation work is charged on every non-null store,
including cache hits. Fuel and frame/slot/code limits retain their earlier meaning.

Incompatible references are boxed FieldAccess host diagnostics with selected
FieldId and exact source/target types, not ClassCastException or ArrayStoreException.
Descriptor/type failures remain nested under FieldAccess with method, instruction,
transition-site provenance and useful Error sources. A nested TypeJavaError is
never inferred as a field instruction's Java outcome. Existing stores and
allocations persist after later Java errors, malformed states or policy failures.

The public low-level ManagedHeap::put_field continues rejecting every non-null
reference, even when assignable and bound. Only the checked context reaches a
crate-private guarded commit. Borrowed children enumeration yields each non-null
logical location in slot order, including repeated aliases and cycles, without
tracing, recursion, deduplication, allocation or heap quota use. These are graph
edges, not roots or a collector. Constructor/lifecycle proof, static writes,
verification, native binding, monitors and exception
dispatch remain outside this profile.


## Actual reference arrays

Runtime-aware instance bodies additionally execute `anewarray`, `aaload` and
`aastore` through the existing context and shared frame loop. Earlier numeric and
primitive-heap APIs reject these three opcodes even with a configured heap.
The bounded multidimensional operation below can also allocate nested arrays.
Arrays travel through locals, fields, element references, checked method
arguments/results and implicit receivers inside the body graph.

`anewarray` resolves the component from the current declaring frame's constant
pool. Component resolution, access and checked rank increment precede negative
length, followed by shared heap quotas and fallible admission. Abstract classes
and interfaces are valid components. New elements are null, including for nested
primitive-array components; allocation never constructs component objects.
Rank 254 components yield rank 255 arrays, while rank 255 components are rejected.
`arraylength` reads either actual array payload; primitive load/store opcodes
still require a primitive payload.

`aaload` pops the Int index and Reference array, then returns the stored identity
unchanged. `aastore` pops the Reference value, Int index and Reference array.
Typed stack extraction precedes service calls. Destination null and signed bounds
precede source identity or assignment checks. Null stores and reads perform no
new relation traversal. Non-null stores use the retained actual component type,
including defining-loader identities and structural covariance; a wider alias
does not weaken its constraint. Existing inaccessible component identities can
accept compatible values without a new caller-access check.

Only explicit `ArrayJavaError` values map to symbolic `NullPointerException`,
`ArrayIndexOutOfBoundsException`, `NegativeArraySizeException`,
`ArrayStoreException` and `IllegalAccessError`. Wrapped heap/type errors remain
boxed `ExecutionErrorKind::ArrayAccess` host failures, even when a nested source
resembles a Java category. Diagnostics retain current method, instruction and
available invocation provenance.

Primitive and reference arrays share the existing array-count, maximum-length,
total-element and payload-byte quotas. A reference element costs the runtime's
`REFERENCE_ARRAY_ELEMENT_BYTES`, independent of Java primitive width. Instance
quotas remain separate. Reads preserve raw opaque identity; writes preserve aliases,
self-links and cycles. The borrowed child iterator reports non-null elements in
ascending index order, including duplicates. No collection or recursive tracing
is implied.

Each instruction consumes one shared fuel unit. Completed allocations, element
stores and field changes persist after later symbolic outcomes, host failures or
budget exhaustion; failed allocation/store leaves prior contents intact. This
increment adds no constructors, initialization, exception
handlers or generalized JVM invocation.


## Bounded multidimensional arrays

Runtime-aware instance bodies also execute `multianewarray` (`c5`) through the
same frame loop and array-error mapper. Numeric and primitive-heap entry points
continue rejecting this opcode before operand extraction. No new execution API,
frame type or prepared-code cache is introduced.

The decoder checks the encoded nonzero dimension count, including unreachable
instructions. Execution pops the requested Int counts in reverse order into a
fixed 255-element buffer, then passes the outermost-to-innermost slice and the
current declaring frame's ClassId to the context. Underflow and wrong stack kinds
precede CP resolution; the interpreter does not inspect count signs or stop at
zero. Target resolution/access, complete array-type validation and requested-rank
checks precede the runtime's outer-to-inner negative-count scan. A negative after
a zero still gives symbolic `NegativeArraySizeException`; a lookup-work policy
failure can stop that scan first and stays a boxed host `ArrayAccess` error.

The runtime preflights the complete reachable geometry, shared array quotas and
node/link work before staging and atomically publishing the resulting graph.
Every reached array has a distinct stable identity and its actual structural
rank. Only requested dimensions are allocated: partial reference components stay
null, primitive leaves receive their existing typed zero defaults, and a reached
zero prevents deeper allocation. Positive unreachable sizes do not consume
allocation quotas. Rank 255 with 255 requested dimensions uses the same bounded,
iterative service; it creates no recursive host execution or owned object tree.

`multianewarray` consumes one interpreter fuel unit. Type resolution, count/level
inspection and checked bulk staging work consume the existing cumulative context
budget; heap geometry uses the same primitive/reference array-count, length,
element and payload policies. Failed preflight/staging leaves the live heap and
prior elements intact. Once publication succeeds, a later instruction, return,
exception or exhausted fuel does not roll it back. Fields, casts, normal array
operations and receiver use see ordinary actual payloads with no special bridge
representation. Constructor/lifecycle boundaries,
exception-object allocation and handlers remain unchanged.


## Checked reference arguments and results

The runtime-aware `execute_instance_method_body` accepts reference parameters and
results through existing `Value::Reference` values. The same shared engine carries
these values through current-class static helpers, virtual/interface/default/private
and ordinary special calls. Earlier numeric and primitive-heap APIs retain their
numeric/void signature restriction and reject `areturn` when reached. The instance
host API still selects an ordinary class root directly; it does not dispatch that
root or execute constructors, initialization or generalized cross-class static calls.

Prepared bodies retain compact `MethodValueShape` computational kinds, parameter
slots and `MethodReturnKind`. They retain no owned nominal result names or resolved
signature types. Runtime-aware preparation obtains this shape at the existing
descriptor stage after method eligibility; old numeric preparation converts to the
same tags. Registered static helpers use already validated borrowed CP metadata
for routing and then the charged shape helper, avoiding an early owned descriptor
copy. Code caching remains keyed by MethodId and checks static/instance mode on
every invocation, including cache hits.

Call-site descriptors determine typed reverse extraction and interface slot counts.
Runtime selectors retain separate resolved and selected identities, checking method
access/staticness/null/receiver/selection before returning the selected body. The
selected declaration determines locals and reference argument/result types. Host
roots preserve computational checks, frame reservation and receiver checks before
reference profile and argument validation. Static helpers prepare/reserve/extract
before their profile and argument checks. Arguments consume their usual slots:
Long/Double two, references one, receiver local zero only for instance bodies.

The bounded nominal-reference profile requires the origin and resolved declarer to
share a defining loader, then audits the selected signature's receiver closure for
matching declarations under different loaders. This includes intermediate, private
and static declarations and may conservatively reject legal non-overrides. These
are explicit host boundaries, not full JVM loading constraints or class preparation.
Primitive-leaf array signatures do not impose that nominal-loader restriction.
Actual argument objects may have different defining loaders when their identities
are assignable through the admitted graph.

Every reference-bearing public signature operation charges its complete descriptor,
including null values and cache hits. Numeric-only signatures add no signature work.
Non-null values require the bound universe and valid actual identity, followed by
selected-declarer descriptor resolution and assignment. Descriptor identity uses
its defining loader without cast-style caller-access checks; accessible methods
can expose inaccessible types. Null skips universe, target resolution and assignment
after the full signature/profile checks. No resolved-signature cache is introduced.

`areturn` requires runtime mode and a reference result tag, pops a typed Reference,
and calls `check_method_return` before removing its frame. Failure identifies the
returning declaration and return offset, preserving invocation provenance. Successful
return transfers the identical handle to the actual caller or host without a new
caller cast/resolution; the selector's accepted descriptor identity invariant and
callee validation establish this bounded transfer. Remaining operand-stack values
are discarded. Caller-capacity failures retain the established receiving-caller
and originating-call diagnostics.

Incompatible reference arguments/results, unsupported loading/override profiles and
nested type failures remain boxed MethodAccess host errors. Only explicit
MethodJavaError values become Java outcomes; no ClassCastException, ArrayStoreException
or LinkageError is inferred from a malformed call value. Each invocation and return
still consumes one shared fuel unit, and frame/slot/code/context policies remain
shared. Earlier field/element writes and allocations survive later failures.
Returned handles remain owned by the supplied heap; returning them does not enable
use with another heap or imply construction, tracing, collection or initialization.

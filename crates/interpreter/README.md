# Method-body interpreter

`execute_static_method_body` evaluates a selected ordinary static method body
with computational `Int`, `Long`, `Float` and `Double` arguments. It returns a normal
result or a symbolic uncaught `ArithmeticException`. An explicit heap entry point
also evaluates primitive arrays with supported symbolic array exceptions. This kernel does not perform Java
invocation, class loading, class initialization, verification or exception-object
allocation. An explicit `InitializationSession`, described below, separately opts
into a bounded single-owner initialization and construction procedure.
`InitializationSession::new_with_monitors` additionally enables single-thread
monitor bytecodes and selected synchronized Code, described in the final section.

The selected standalone method must have exactly one Code attribute, a numeric/void
descriptor, and no native, abstract or synchronized declaration or exception
handlers. Initialization methods are excluded. The decoder validates the entire
instruction stream before evaluation. Unsupported instructions fail when reached.

## Executed instruction coverage

The original numeric entry points handle 148 effective opcode values. The explicit
heap entry point adds 31, for 179 total. The runtime-aware instance entry point
adds `getfield`, `putfield`, `invokevirtual`, `invokeinterface` and ordinary
`invokespecial`, plus `checkcast`, `instanceof`, `anewarray`, `aaload` and
`aastore`, `multianewarray` and `areturn`, for 191 total. The opt-in exception
profile adds `athrow`, for 192 total. The static-only and composed static/exception
profiles add `getstatic`/`putstatic`, for 193 and 194 respectively. The initialization
session adds `new` and constructor semantics for `invokespecial`, for 195 effective
opcodes. A monitored session adds `monitorenter` and `monitorexit`, for 197.
`wide` is an encoding prefix for indexed
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
| Numeric standalone body calls | `invokestatic` only for ordinary static numeric methods declared by the supplied class/interface; registered runtime profiles use the checked static resolution described below |
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
or later. Standalone and heap-only missing declarations remain explicit errors
without inherited lookup; registered calls use checked runtime resolution.

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

Static memory requires the opt-in host-controlled profiles below. Generalized
runtime invocation, generalized cross-class static calls, constructor execution,
VM exception materialization, monitors and legacy subroutines remain unsupported. Numeric
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
stack and instruction loop. Registered static calls start their arguments
at local zero and resolve their actual declaring MethodId through the context.
Every frame's field origin is its declaring ClassId, including when the receiver is a
subclass or an earlier frame belongs to another class. Cached instance preparation cannot make the root a valid invokestatic
target. Root and callee limits/fuel have the same meaning as the earlier APIs.
Interface roots, special initialization names, native/abstract/synchronized
methods remain excluded. This entry point also rejects Code handlers.

Only this runtime-aware API enables `getfield` and `putfield`. Earlier APIs reject
them even with a configured instance heap. Field operations delegate symbolic
resolution, access and receiver relationships to the contextР В Р’В Р В РІР‚В Р В Р’В Р Р†Р вЂљРЎв„ўР В Р вЂ Р Р†Р вЂљРЎвЂєР РЋРЎвЂєs explicit unnamed
module profile. Exact owner-qualified storage preserves inherited hiding, raw
floating bits and the approved Z/B/C/S store conversions. Final writes fail;
reference fields retain null defaults and support the checked stores described
below. Reference locals,
identity branches and all supported primitive-array operations remain available.

Explicit field Java failures become symbolic `NoSuchFieldError`,
`IllegalAccessError`, `IncompatibleClassChangeError` or the existing
`NullPointerException`. Resolution/access, staticness and write finality precede
bytecode null dereferences. Other context failuresР В Р’В Р В РІР‚В Р В Р’В Р Р†Р вЂљРЎв„ўР В Р вЂ Р В РІР‚С™Р РЋРЎС™including unsupported nest or
loading constraints, invalid receivers, changed layouts and resource exhaustionР В Р’В Р В РІР‚В Р В Р’В Р Р†Р вЂљРЎв„ўР В Р вЂ Р В РІР‚С™Р РЋРЎС™
remain boxed `ExecutionErrorKind::FieldAccess` errors with method, instruction and
available caller provenance. These outcomes allocate no Java error objects.

Each field instruction consumes one shared fuel unit. Completed field writes and
array allocations persist after every return, symbolic failure or host error;
failed writes preserve their previous values. This bridge does not implement
`new`, constructors, automatic class initialization,
VM exception materialization, monitors, named modules, full verification or complete linkage.


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
native/synchronized bodies are rejected before execution. Code handlers require
the opt-in exception profile below. Resolution and
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
lifecycle proof, VM exception materialization, complete linkage or JVM startup.


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
staticness, descriptors, special names, native/synchronized bodies and
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
structured host errors. Constructors, initialization, native binding, VM
exception production and monitors remain separate work.


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
virtual/interface calls dispatch normally and registered static calls retain
their narrow policy. Receiver local zero, mixed numeric widths, narrow/FP/void
returns, remaining caller operands, fuel and frame/slot/code/work limits keep the
earlier rules. Code preparation remains eager and excludes native/synchronized
bodies; handlers require the opt-in profile below. Existing writes and allocations persist after every later
Java or host failure. No new public execution API, constructor/lifecycle proof,
class initialization, general static resolution, native
binding or monitors is supplied by this increment.


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
exception-object materialization retain their earlier boundaries.


## Checked reference fields

The existing runtime-aware putfield now accepts assignment-compatible non-null
instance and array references through `FieldResolutionContext::write_field`.
Checked field stores added no opcode or execution API. Base runtime-aware
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
Legacy contexts retain the conservative cross-loader nominal boundary. A checked
loading-domain context instead enforces the agreements described below.

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
edges, not roots or a collector. Constructor/lifecycle proof, final-write authority,
verification, native binding, monitors and VM exception
materialization remain outside this profile.


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
increment adds no constructors, initialization, VM exception
materialization or generalized JVM invocation.


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
exception-object allocation remains unsupported; handlers require the opt-in
profile below.


## Checked reference arguments and results

The runtime-aware `execute_instance_method_body` accepts reference parameters and
results through existing `Value::Reference` values. The same shared engine carries
these values through registered static calls, virtual/interface/default/private
and ordinary special calls. Earlier numeric and primitive-heap APIs retain their
numeric/void signature restriction and reject `areturn` when reached. The instance
host API still selects an ordinary class root directly; it does not dispatch that
root, execute constructors or automatically initialize classes.

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

Legacy contexts without a checked loading domain require the origin and resolved declarer to
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


## Explicit supplied-Throwable dispatch

`execute_instance_method_body_with_exceptions(context, profile, method, receiver,
arguments, options, call_limits, exception_limits)` opts the same body engine into
`athrow`, ordered handlers and iterative unwind. Its `ExceptionOutcome` is either
`Returned(Option<Value>)` or `Uncaught(Reference)`. Original entry points and
`ExecutionOutcome` are unchanged: they reject Code handlers and `athrow`, and retain
their symbolic VM-produced outcomes. No second execution loop or exception-object
allocator is introduced.

The caller obtains an opaque `ExceptionProfile` from the runtime using an explicit,
admitted bootstrap `java/lang/Throwable` ClassId and bound reference universe.
The wrapper checks that token before even the root frame gate. It identifies an
unchanged registry/heap/layout/universe snapshot, and can be reused through fresh
contexts for that same snapshot. It does not certify constructor execution,
initialized library state, messages, stack traces or the complete Throwable ABI.
The host supplies the ordinary Throwable instance; the runtime checks its actual
identity and superclass relationship before issuing a throw token.

Opt-in preparation eagerly decodes Code, then checks an independent aggregate
handler limit and validates every row's protected range, decoded start/end/target
boundaries, Class/Utf8 catch shape and non-array spelling. Nonempty tables require
at least one operand slot. Prepared entries retain only an index into the original
Code attribute; rows and catch names are not cloned. Cache hits do not recount
handlers or repeat structural preparation. Legacy handler rejection still precedes
its original Code-byte/decode stage. These guards are not StackMap verification,
constructor-state verification or eager catch-class ancestry validation.

`athrow` first pops a typed Reference. A checked non-null supplied Throwable keeps
its exact identity through catches, rethrow, replacement and uncaught completion.
Search follows table order and the end-exclusive range. Typed catches resolve only
when their active row is examined before the first match, using the searched
frame's declaring origin and caller-access policy. Catch-all skips resolution;
inactive and later rows cannot trigger missing/inaccessible-type failures.

On a match the engine checks/reserves one operand slot before changing the stack,
then clears every old operand and slot count and installs only the thrown reference.
Locals and frame reservation remain. The next handler instruction consumes normal
fuel. If no row matches, the engine checks its unwind charge and accounting before
removing the frame, releases its slots, validates its saved caller identity and
searches the caller at the actual invocation PC. The already advanced resume PC
is never used for that search. A fully uncaught completion returns the same opaque
handle to the host, without implying Java thread termination or an uncaught-handler
callback.

`ExceptionLimits` defaults to 65,536 aggregate prepared rows and 1,000,000 dispatch
steps. Each searched frame, every row including inactive rows, and each unwind
including the root costs one dispatch step before acting. The counter is shared by
all throws/rethrows in that evaluation; arithmetic overflow differs from an ordinary
inclusive limit failure. Runtime name/ancestry work also consumes the existing
context policy. Handler installation and search do not consume instruction fuel;
`athrow` consumes one unit, so a final fueled throw can finish uncaught while an
entered handler still requires fuel for its next instruction. Existing frame,
slot and prepared-Code policies retain their meaning.

VM exception production remains unsupported in this profile. All supported
VM-produced categories, including the method-selection routes and a direct typed
catch access failure, become `UnsupportedVmException` host errors. Null `athrow`
likewise requires unsupported NullPointerException production. A catch-all cannot
turn those tags into a fabricated object, and no handler search runs for them.
Nested Java-shaped validation errors remain boxed `ExceptionAccess` host errors.
Host policy, malformed code, unsupported operations and invalid supplied identities
cannot be caught. Search failures identify the searched declaration and search PC
with its invocation provenance; throw-validation failures identify the actual
`athrow` instruction.

Earlier writes and allocations survive caught/uncaught completion and every host
failure. No allocation, constructor, initialization, monitor release, async
exception, exception materialization, full loading/verification, Java startup or
GC guarantee is added. This is bounded dispatch of explicitly supplied instances.


## Composable host-controlled static memory

`execute_instance_method_body_with_static_state(context, statics, method, receiver,
arguments, options, call_limits)` opts the same shared engine into `getstatic` and
`putstatic` and returns the existing `ExecutionOutcome`. The composed
`execute_instance_method_body_with_static_state_and_exceptions` also takes an
ExceptionProfile and ExceptionLimits and returns `ExceptionOutcome`. It checks the
static profile first, the exception profile second, then follows the unchanged
root method/shape/argument/reservation/receiver pipeline. Older entry points keep
rejecting both static opcodes even when their heap contains static memory.

The caller obtains an opaque `StaticStateProfile` by explicitly allocating checked
heap-owned defaults through the context. Each declaring FieldId has one logical
location; inherited symbolic owners do not duplicate it. Primitive defaults have
positive zero bits and references start null. ConstantValue and clinit are not
applied, classes are not marked prepared or initialized, and selecting the profile
does not authorize automatic initialization. All final writes are rejected. The
older standalone default-only StaticStorage remains read-only and independent.

A static profile is shared by every nested static helper and virtual, interface or
ordinary special target. Each instruction uses its current declaring class/CP as
the resolution origin. `getstatic` pushes the actual value at its category width.
`putstatic` pops one Value with pop_any and no receiver, then delegates to the
runtime. Underflow precedes field selection; access, static-category, finality and
location checks precede computational-kind and reference assignment checks.
B/S/C/Z narrowing and Float/Double raw-bit storage use the established field rules.

Non-null references require checked actual identity, then the selected declaring
field's descriptor under its defining loader and identity-only assignment. Null
writes do not require a universe or type resolution. Inaccessible descriptor
types are not caller-access failures; conservative nominal cross-loader boundaries
remain. Reads validate stored handles. Static roots enumerate one identity per
non-null location in field order, including aliases, without following child
edges or allocating. Instance/array children and quotas remain separate, and this
is not a collector root-location or synchronization protocol.

Repeated allocation with unchanged admitted classes/options is idempotent and
preserves values without work/reset. Layout growth invalidates old profiles and
contexts; a fresh context can extend with the original options, preserving old
locations/values/root order and appending only new declarations atomically.
Static allocation/writes do not themselves invalidate exception/type/method
snapshots. A profile allocated before first universe binding can be reused with a
fresh context after binding. There is no raw store/reset API or final-write bypass.

Only direct StaticJava errors become the established NoSuchFieldError,
IllegalAccessError or IncompatibleClassChangeError outcomes. In the composed
profile those same categories become UnsupportedVmException before handler search;
a catch-all cannot fabricate their Throwable. Nested Context/Type/Heap Java-shaped
errors remain boxed `ExecutionErrorKind::StaticAccess` host failures. Actually
supplied throws continue through the same ordered catch/unwind implementation.

Each static instruction consumes one ordinary fuel unit, with the same frame,
slot, Code and shared resolution budgets. Completed static changes persist after
returns, symbolic errors, caught/uncaught supplied throws, exhausted fuel, host
failures and rejected later stores. Diagnostics retain current declaration,
bytecode offset and invocation provenance, including late assignment and
category-two push failures. These APIs add checked mutable memory and composable
body execution; they add no initializer execution, lifecycle-success state,
complete loading/preparation, String materialization, final-write authority,
VM exception production, constructor, synchronization, Java startup or GC claim.


## Explicit single-owner initialization session

`InitializationSession::new(context, statics, exceptions, error_class, limits)`
checks the static profile, then the supplied-Throwable profile and admitted-class
limit, reserves records for the actual admitted classes, and permanently claims
that heap. The opaque session exclusively borrows the context. Its `initialize`
method requests a ClassId; `run_static` prepares an ordinary registered static
root, validates arguments/reservations, initializes its owner, then runs its body.
Both use the same iterative engine, prepared-method cache and exception dispatcher.
There is no instance-root session entry point. Dropping a session does not release
its claim or permit a replacement session to replay initialization.

`initialize` returns Completed or an actual supplied uncaught reference;
`run_static` returns the existing ExceptionOutcome. `status` is a checked,
resolution-free view of Uninitialized, InProgress, Completed, Erroneous with its
original cause, or Aborted. Completed certifies only this profile's supported
single-session procedure, not complete JVM verification, linking, preparation or
concurrent initialization. A returned result, host error, or recovered callback
panic cannot leave an InProgress record: an allocation-free unwind guard marks
all remaining active records Aborted. Completed and Erroneous records, prior
static writes, arrays, objects and root identities persist.

A class first installs its own numeric ConstantValue batch, including attributes
on nonfinal static fields, before initializing its superclass and the runtime's
ordered postorder of interfaces declaring concrete instance methods. The default
predicate includes supported private/bridge/synthetic methods and excludes static,
abstract and initializer declarations. Explicit interface initialization does not
initialize its parents. Already-completed child interfaces do not prune ancestors
from a later class plan. Instance ConstantValue attributes are ignored; any static
String ConstantValue rejects the whole plan before installation or publication.
B/C/S/Z constants use raw descriptor normalization, and F/D retain their raw bits.
Constant installation itself does not materialize a String or execute constructors.

A scheduler-created initializer frame has a distinct preparation/cache role and
executes the actual `<clinit>()V` body. Historical ignored access flags are treated
semantically, including no implicit receiver even without ACC_STATIC; historical
parameterized initializer descriptors remain unsupported. Ordinary roots/helpers
still reject special names and native/abstract/synchronized bodies. Only the exact
actual initializer of the selected declaring class may write its final static
fields. A helper, ordinary body, or another class's initializer has no such authority.
Recursive C-to-D-to-C requests observe the existing partial values and never reset
constants or re-enter an already active initializer.

In this session, getstatic/putstatic and supported registered invokestatic trigger
initialization of the actual declaring owner. Each active-use instruction consumes
fuel once. Static operations retain their checked selector token, original PC,
next PC and optional popped value; b8 retains its checked target and typed mixed
arguments. Resumption neither resolves nor pops them again. A frame fallibly
reserves a reusable one-entry token buffer only when needed; legacy frames allocate
none. Tokens are removed on successful resumption and abrupt transfer; there is no
additional unbounded pending-token cache. Reference store validation occurs after
initialization, and a late store failure or read push overflow preserves a target
that has already completed. Handler searches use the original active-use PC.

An actual supplied Error escaping an initializer becomes that class's Erroneous
cause. Dependency-waiting parents acquire the same cause before caller handlers
are searched; an initializer that catches a dependent Error can still complete.
`retained_roots` enumerates one cause per Erroneous record in insertion order,
including duplicate aliases. An escaping other Throwable produces the host
UnsupportedInitializerWrapping result; no wrapper is fabricated. Later uses of
Erroneous records return UnsupportedNoClassDefFoundError, and Aborted records
return InitializationAborted without retry. All supported VM-produced categories
remain UnsupportedVmException before handler search, including null athrow and
direct selection failures. Only direct InitializationAccessError::Java maps to
that policy; nested Static/Exception/Context errors remain structured host errors.

InitializationLimits default to 16,384 admitted classes, 1,048,576 retained plan
dependencies, 65,536 constant entries, 16,384 pending activations and 1,000,000
scheduler steps. Retained plan quotas apply across session evaluations; steps
reset each evaluation, and failed charges do not advance last_initialization_steps.
Instruction fuel, prepared Code/handler budgets, actual frame/slot reservations and
exception-dispatch work reset per evaluation; context lookup work remains cumulative.
No-body classes need scheduler work but can complete with zero instruction fuel
and zero Java frames. The 10,000-class synthetic chain exercises iterative planning,
scheduling and drop. Every counter and allocation demand is checked; cleanup does
not consume another budget. Diagnostics preserve real methods, PCs and invocation
origins and separately identify a responsible initialization_class, including
root/no-body errors without fabricated bytecode locations.

Earlier body/static/exception entry points retain their original gates and never
automatically run initialization, apply constants or gain final-write authority.
The session adds no threads, wait/notify initialization protocol, VM exception
construction, class-loader creation, complete runtime-constant-pool linkage,
Java startup, native binding, monitors or complete lifecycle guarantee.


## Registered cross-class static calls

Every registered runtime-aware profile resolves each `invokestatic` through
`context.resolve_static(current_declaring_class, cp_index)`. The returned MethodId
names the actual static declaration, including a superclass declaration reached
through a symbolic subclass. All instance-body, supplied-Throwable, static-memory,
composed static/Throwable and InitializationSession paths use this service. No new
public entry point, opcode or execution loop is introduced. The opcode coverage
counts above remain unchanged. Standalone ClassFile and heap-only execution retain
the original same-class resolver, numeric descriptors, errors and budgets.

Methodref and version-52-or-later InterfaceMethodref carry the caller's immutable
CP spelling. The context checks owner identity/kind, ordinary name/descriptor,
access and the context's loading-constraint policy, then static category. Class lookup
can find inherited declarations; an eligible instance/default/abstract result
produces IncompatibleClassChangeError rather than being bypassed for a static
ancestor. Interface static methods must actually be declared by the symbolic
interface: a child interface does not inherit its parent's statics. A private
static interface helper is permitted only from that same declaring interface.
Runtime packages include defining-loader identity; protected static access uses
class ancestry without a receiver restriction. Unsupported nest/loader boundaries,
array owners and special initialization names remain explicit host errors.

Resolution runs before target preparation, frame/slot limits and typed argument
pops. A cache miss then prepares the actual owner's Code as ordinary Static,
retaining native/abstract/synchronized/special-name exclusions and eager decoding.
Every cache hit still checks initializer/static eligibility. The cache key is the
full MethodId, so aliases and inherited symbolic owners share Code preparation,
while distinct defining identities remain distinct. Legacy contexts repeat origin-sensitive resolution/access/loading work on every
call. Checked loading-domain contexts reuse their persistent symbolic resolution;
per-use category, selection and value checks remain. The prepared Code cache is
separate from either policy.

After preparation and reservation, arguments are reverse-popped in the prepared
computational shape, restored to descriptor order and checked against the actual
declaring method. There is no receiver or local-zero displacement. Shapes retain
only kinds and compact return tags. Legacy nominal caller/declarer signatures require one defining loader even for
eventual null arguments; primitive-leaf array signatures do not impose that
conservative boundary. Checked contexts impose actual-declarer agreements instead. Value assignment and areturn
use the selected declarer's loader without extra caller access checks on hidden
descriptor types. Raw float/double representations, reference identities,
category-two locals and subword return narrowing retain their established rules.

Host-controlled profiles activate the target without running class initialization
or applying constants; their field/exception capability gates remain unchanged.
Only InitializationSession requests the actual MethodId owner's initialization.
Its existing pending call retains the target, typed arguments, next PC and original
invoke PC. Resumption does not resolve, pop, validate arguments or debit fuel again.
A symbolic subclass remains Uninitialized when only an inherited static declaration
is actively used. Cross-class C-to-D-to-C requests observe partial state, while
completed calls remain once-only and preserve earlier field effects.

An Error escaping target initialization is caught against the caller's original
b8 PC; a later throw from the ordinary target body uses normal callee dispatch.
Sticky Erroneous/Aborted state and retained roots follow the existing session rules.
Only direct MethodAccessError::Java values become the established symbolic results
or, with the supplied-Throwable/session policy, UnsupportedVmException before
handler search. Nested Java-shaped Context/Type failures remain boxed MethodAccess
host errors. Diagnostics keep caller resolution/pops at the invoke site, target
preparation/return failures at the real target and actual caller transition origins.
Static resolution alone adds no constructor execution. Native binding, monitors,
String materialization, VM exception construction, a general loading lifecycle and
Java startup remain outside that scope. Persistent member resolution is separately
opted into through the checked loading domain below.


## Session allocation and constructors

InitializationSession now executes `new` and constructor `invokespecial` through
the same engine, scheduler and MethodId cache. Existing initialize/run_static use
ConstructionLimits::default(); their `_with_construction_limits` counterparts add
a final ConstructionLimits argument. max_alias_steps defaults to 1,048,576, resets
per evaluation even on root preflight failure, and is reported by
last_construction_steps. Old standalone, heap-only and every explicit body/static/
Throwable entry point retain their original new/constructor rejection. A constructor
cannot be selected as a host root; host default-instance allocation still supplies
storage without claiming that any constructor executed.

Public Value/ValueKind/Reference stay unchanged. Private frame values distinguish
ordinary values from one-slot unfinished aliases. An allocation alias includes
its actual handle and allocation site; each constructor activation has a distinct
checked, nonwrapping This identity. Fixed/indexed/wide reference moves and every
legal category stack form preserve aliases, including pop/pop2 discard and mixed
category-two values. Null/identity branches compare validated actual heap identity
without promotion. Ordinary arguments, field/static/array values, casts/instanceof,
returns, throws and ordinary receivers reject unfinished references. Numeric misuse
retains computational-kind diagnostics. An ordinary frame may return an unrelated
ordinary result while disposing of unused unfinished values; that is not construction
success. No unfinished value appears in public results or protocol diagnostics.

A reached new resolves and checks its target, reserves operand/continuation capacity,
funds the site scan, then requests class initialization before allocating storage.
It retains the resolved token and original PC/fuel, so resumption neither resolves
nor allocates twice. A live unfinished stack alias from the same new site is rejected.
Only successful publication invalidates older same-site unfinished locals; a failed
initialization/allocation preserves them. Ordinary references and other sites are
unaffected. All fallible work precedes heap-to-private-token publication. Allocations
use shared instance/slot quotas and charged layout traversal; orphaned or failed
objects remain allocated and retain quota, with no fake OutOfMemoryError or reclaim.

Constructor resolution follows checked symbolic lookup/access/loading before exact
constructor-owner and receiver relations. It does not execute inherited constructors,
perform ACC_SUPER redirection or audit an override chain. Each call prepares or
reuses a distinct Constructor role, shares Code/frame/slot/handler/fuel limits,
validates ordinary arguments and owns its receiver-bound runtime authority.
Only current Pending This may delegate to its class or direct superclass; a separate
Allocation may be constructed before that chain completes. Abstract superclass
constructors may execute for concrete descendants. The bound bootstrap Object
constructor begins Complete with ordinary local0, but executes actual supplied Code;
its public/concrete/root identity is checked lazily, without breaking old sessions.
There is no production empty-Object fallback.

Pending/Complete/Failed chain state is independent of local0 and alias liveness.
Losing local0 or constructing another object cannot authorize premature return.
Normal constructor return checks Complete, validates its void return, funds the
immediate caller scan and promotes matching aliases without allocating. A successful
this/super call completes the caller's own chain; completing a separate allocation
does not. On an escaping constructor exception, matching caller locals become
unusable before caller handler search at the original b7 PC, and a failed This call
marks Failed. A caught ordinary-helper throw or failed separate allocation before
chaining leaves Pending and permits the real chain later. A caught throw inside
the current constructor does not itself fail its caller. Failed chains cannot be
retried or return successfully.

Promotion/invalidation is per frame, never a global heap initialized/poisoned bit.
Ordinary this published after inner completion, or by the actual Object constructor,
remains usable when an outer constructor fails. Earlier writes, roots, arrays and
object identities persist. A constructor failure after class initialization does
not mark that class erroneous. An Error escaping into a class initializer still
uses the established session erroneous-state protocol. Host errors and recovered
resolver panics drop transient frames/tokens and let the initialization guard abort
only still-active classes; no cleanup budget or uninitialized retained-root value
is introduced.

Only actual constructor frames use the checked constructor field service. It permits
own-declaration final writes, including repeated writes and ordinary receivers other
than local0, under normal receiver/access/assignment checks. A Pending This receiver
has the additional exact symbolic/current declaring-owner restrictions; unfinished
values cannot be stored. Helpers and ordinary/raw setters retain final-write refusal.
A single receiver-bound authority must match the executing MethodId. Private tests
reject mismatched/duplicate authority before field or alias mutation, and preserve
all field/root values on late failure.

Each allocation-site scan, normal promotion or exceptional invalidation atomically
funds `1 + logical max_locals + actual operand-value count`. Long/Double count once
as inspected values while still consuming two Java slots. Failed charges preserve
counters and data; funded work remains consumed after later failures. Scans and
chains are iterative, with actual-demand fallible token reservations and no global
alias registry or policy-sized allocation. Tests exercise 1,000 superclass frames,
1,000 recursive this activations, exact budgets, same-site loops and single-pass
mixed-wide survivor accounting. Only direct ConstructionJavaError categories become
UnsupportedVmException, now including InstantiationError; nested Java-shaped errors
remain boxed ConstructionAccess host failures.

This is dynamic enforcement on executed supported paths, not the complete verifier,
stack-map merge validation or full JVM rejection timing. No VM exception objects,
String materialization, concurrent initialization, native binding, synchronization,
complete loading constraints, Java library bootstrap or startup are added. Heap
children/static roots expose lawful ordinary writes; retained_roots still enumerates
initialization-failure causes. Transient frame tokens are conceptual roots, but no
complete active-frame GC root API or collector is claimed.


## Checked loading-domain member resolution

A context returned by `LoadingDomain::member_context_unnamed_modules` uses the
same engine and entry points. Enable the domain's member store, obtain its
`prepare_member_owner` evidence for every heap-admitted class, then open a checked
context. Preparation evidence is separate from verification, initialization and
ordinary `prepare_constraints`. Opening validates the complete admitted graph and
its actual initiating records before execution; an implied equality is not a
loaded-class lookup. The context holds the exclusive domain borrow. Missing host
bindings/layouts can be supplied only after it is dropped, followed by a fresh
checked context. The single-session permanent initialization claim is unchanged.

A persistent member key is the actual containing ClassId and CP index. It does not
include caller method, instruction, receiver, heap or selected body. Successful
resolution retains the symbolic owner, actual declaration and access qualifier.
Descriptor leaves constrain the containing and actual declaring loaders, in
parameter order then return order. The symbolic subclass's unrelated binding is
not constrained. Null arguments/results still require resolution agreements but do
not force their signature types to load. Runtime value checks use the actual body
and declaring field identities. Preparation supplies ordinary overriding/default
agreements; a selected declaration outside those agreements needs an existing
relation or identical actual bindings, otherwise `UnprovenSelection` remains a
retryable host failure. It never invents another equality.

The same CP success can precede null/category/protected-receiver/final-write,
selection, argument, body-preparation or initialization failure. All these checks
remain per use; warm virtual/interface calls can select different bodies. An
inherited static member initializes its actual declarer only. Constructor lookup
may resolve an ancestor before the instruction rejects its symbolic-owner mismatch.
Legacy standalone/heap APIs and unbacked contexts retain their original gates and
conservative nominal/override restrictions. No new opcode or execution loop is added.

Only direct resolution constraint conflicts emit the additive symbolic
`ExecutionOutcome::LinkageError`. Supplied-Throwable and initialization profiles
report `UnsupportedVmException(VmExceptionKind::LinkageError)` without searching
handlers or fabricating a Java exception. Nested Java-shaped host errors remain
host errors. Missing prerequisites or interrupted resource work leave the site
vacant while preserving earlier agreements and heap effects. Direct permanent
resolution errors are cached by category. Warm Java failures replay even on a
smaller checked heap; warm successes still require the symbolic and declaring
layouts. No Java Throwable identity, Class-CP load-failure persistence, classpath
callback, module/nest implementation or complete lifecycle is claimed.

Member sites and prepared witnesses have inclusive domain-lifetime quotas; the
loading work meter is cumulative per context. A field CP warm hit adds exactly one
loading Work and no new descriptor traversal, UTF span or scratch. Existing
per-use lookup, selected-signature, assignment and interpreter budgets still apply.
The checked path does not populate the legacy local symbol memoizer. Independent
public tests pin cold/warm/duplicate-site behavior, prefix retention and host repair,
protected and private access, read-only E/S repair, smaller heaps, exact opening
and quota limits, a 65,535-unit descriptor, many equal CP slots and a deep graph.
Actual bytecode tests cover normal/supplied-throw/session paths and prior effects;
private mapper tests preserve direct-only error projection.


## Named application main in one session evaluation

`InitializationSession::run_application_main` accepts a checked runtime
`ResolvedApplicationMain`, one reference argument and the existing execution,
call, exception and construction limits. It retains the selected declaring method
M for Code, parameter checks, locals, CP and return behavior, while requesting the
named initial class N for active-use initialization. An inherited parent main
therefore follows parent and child initialization, then executes the actual parent
body. A directly named interface can execute its own static main. The old
`run_static` still initializes only its MethodId owner.

The method reuses the existing engine, RootStatic continuation, scheduler, frame
vector and MethodId cache. It performs normal M Code/frame/slot/argument preflight
before N can write an initializer field. One evaluation funds both named
initialization and main: fuel, prepared Code, handlers, dispatch and construction
work never reset between them. A failed child initializer preserves earlier
parent/child effects and never enters M. Root preflight failures preserve
Uninitialized state; later initializer/body failures retain their genuine class,
method and PC diagnostics. Token failures remain boxed
`ExecutionErrorKind::ApplicationMain` host errors, with no fabricated method or PC.

The low-level method still uses ordinary reference assignment checks; it does not
promise that a caller-supplied array is empty. The launcher creates exactly one
non-null bootstrap String[] of length zero and owns whole-image verification and
sealing. Merely creating that array does not initialize String. This adds no
opcode, VM exception object, native bootstrap, platform trust exemption or second
execution loop. Public tests pin inherited identities, old run_static behavior,
shared exact/short fuel and Code limits, argument/preflight effects and stale-token
error provenance.


## Single-thread monitors

`InitializationSession::new_with_monitors` retains the existing static/exception/
initialization checks and permanent claim, then stores `MonitorLimits`. Its
initialize, static-root and application-main evaluations each start one empty
private ledger. Calls, constructors, initializer bodies, active-use continuations
and supplied-Throwable dispatch share that evaluation's ledger and four inclusive
ceilings: steps, active identities, active claims and recursion depth per identity.
The defaults are 1,048,576 / 16,384 / 65,536 / 65,536. Growth is fallible and based
on actual demand, with no host locks or global ownership table.

`monitorenter` / `monitorexit` accept checked ordinary objects, primitive/reference
arrays and private unfinished construction identities without promoting aliases.
Null and invalid handles are checked before monitor work. Selected synchronized
instance bodies lock their actual receiver; synchronized static bodies lock the
actual declaring ClassId, including its defining loader. Initialization finishes
before a deferred synchronized static/root activation acquires its lock. Incidental
clinit flags still do not synchronize the initializer. Constructors retain their
existing declaration and chain rules.

The profile enforces both optional JVMS structured-locking rules. Each invocation
can exit only its own positive explicit claim. A callee cannot consume its caller's
claim or its own implicit synchronized receipt. This deliberately rejects an
explicit exit of an implicit-only lock immediately at that exit PC, rather than
copying reference VMs' later completion failure. Different monitors need not be
released in LIFO order. An explicit pair costs two monitor steps; a synchronized
invocation costs one at activation and one at completion. Completion with no claims
or receipt is free, as is installing a matching same-frame exception handler.

All six returns first validate their original operand/protocol, then close the
frame before result transfer, constructor alias promotion or initializer completion.
Supplied throws search handlers first; only an escaping frame closes its monitor
state. Unbalanced completion replaces the return/throw with
`UnsupportedVmException(IllegalMonitorStateException)` at its real boundary PC,
without fabricating or catching a Java object. `ExecutionErrorKind::Monitor` retains
structured resource/coherence/allocation errors and a typed source. Nested implicit
entry errors identify the caller/invoke PC; host-root entry errors identify the
selected method without a fabricated PC. All interruption paths, including panic,
drop the entire evaluation-local ledger without further policy charges; committed
Java state and initialization statuses remain inspectable.

Legacy `new`, standalone numeric/heap APIs and instance-body APIs continue rejecting
synchronized Code and these bytecodes. The older profile sections describe those
explicit boundaries, not the monitor-enabled session. Monitor metadata uses a lazy
frame-depth map; legacy Frame layout is unchanged and no monitor entries are allocated
without a claim. No class mirrors, Thread.holdsLock native, VM exception production,
wait/notify, blocking, multiple Java threads or memory-order guarantee is supplied.

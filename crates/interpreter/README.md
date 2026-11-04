# Method-body interpreter

`execute_static_method_body` evaluates a selected ordinary static method body
with computational `Int`, `Long`, `Float` and `Double` arguments. It returns a normal
result or a symbolic uncaught `ArithmeticException`. An explicit heap entry point
also evaluates primitive arrays with supported symbolic array exceptions. This kernel does not perform Java
invocation, class loading, class initialization, verification or exception-object
allocation.

The selected method must have exactly one Code attribute, a numeric/void
descriptor, and no native, abstract or synchronized declaration or exception
handlers. Initialization methods are excluded. The decoder validates the entire
instruction stream before evaluation. Unsupported instructions fail when reached.

## Executed instruction coverage

The original numeric entry points handle 148 effective opcode values. The explicit
heap entry point adds 31, for 179 total. The runtime-aware instance entry point
adds `getfield`, `putfield`, `invokevirtual`, `invokeinterface` and ordinary
`invokespecial`, for 184 total. `wide` is an encoding prefix for indexed
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

Reference parameters/returns and reference arrays, static fields, runtime invocation,
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
of the declaring class or a subclass. Receiver local zero precedes the numeric
arguments at their normal widths. Argument diagnostics keep their original
argument indices. Private ordinary bodies may be selected explicitly; this is
not root method access checking, virtual dispatch or Java invocation. Its body
can perform the bounded virtual calls described below.

Instance mode reuses the same decoder, prepared-method cache, explicit frame
stack and instruction loop. Numeric static helpers still start their arguments
at local zero and must be declared by their current caller’s class. Every
frame’s field origin is its declaring ClassId, including when the receiver is a
subclass or an earlier frame belongs to another class. Cached instance preparation cannot make the root a valid invokestatic
target. Root and callee limits/fuel have the same meaning as the earlier APIs.
Interface roots, special initialization names, native/abstract/synchronized
methods, handlers and reference descriptors remain excluded.

Only this runtime-aware API enables `getfield` and `putfield`. Earlier APIs reject
them even with a configured instance heap. Field operations delegate symbolic
resolution, access and receiver relationships to the context’s explicit unnamed
module profile. Exact owner-qualified storage preserves inherited hiding, raw
floating bits and the approved Z/B/C/S store conversions. Final writes fail;
reference fields support null defaults and null stores only. Reference locals,
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
`new`, constructors, getstatic/putstatic, class initialization, reference parameters/results, non-null field stores,
exception dispatch, monitors, named modules, full verification or complete linkage.


## Bounded virtual body calls

Runtime-aware instance mode additionally enables `invokevirtual` for numeric/void
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
The receiver occupies target local zero; numeric arguments follow at their usual
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
reference arguments/results, generalized cross-class
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
name and supported numeric descriptor are checked first; a count mismatch or a
required count above 255 is InvalidOperands at the invoke offset, before argument
extraction or runtime resolution. Reference descriptors remain unsupported.
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
structured host errors. Constructors, initialization, reference
arguments/results, native binding, handlers and monitors remain separate work.


## Ordinary special calls

Runtime-aware instance mode enables nonconstructor `invokespecial` with the same
host entry point, context and frame loop. Calls use `select_special` from the
current frame's declaring class or interface. Methodref is supported throughout
the parser's version range; InterfaceMethodref requires the origin to have major
version 52 or above. Earlier standalone and heap-only entry points still reject
b7. Wrong CP kind/version, reference descriptors and initialization names fail
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
class initialization, reference-valued API, general static resolution, native
binding, monitors or exception dispatch is supplied by this increment.

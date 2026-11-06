# Verified directory application entry

The library constructs one sealed, single-use execution image from explicit
platform and application directories. The CLI provides:

```
crabspot inspect [--enable-preview] FILE
crabspot run --platform DIR --classes DIR BINARY.MAIN [-- ARG ...]
```

A successful run prints nothing and exits zero. Run syntax errors and unknown
commands exit 2; legacy inspect errors remain exit 1. Image, setup and engine
failures exit 1 with a structured stage diagnostic. Exact UTF-16 application
arguments are supported as described below. JARs, arbitrary loader code,
standard streams and general native
methods are outside this profile. The launcher never invokes Java tools or uses
CLASSPATH, JAVA_HOME or PATH to find a missing definition.

`run_directory_application` takes paths, a binary entry name, optional own-static
`I` field observations and typed limits. It supplies every loader, ClassId,
route, plan, proof, heap and runtime profile. No public success flag, resolver,
chosen method or caller-owned heap can grant execution authority. Observations
include private own fields, keep query order/duplicates, and never initialize a
class. They cannot observe a delegated platform definition. Results contain
scalar state and preserved engine errors; no live heap handle escapes.

Both input inventories are gathered before definition admission. Enumeration is
iterative and bounded across all entries, including directories and ignored
regular files. Relative paths are checked under a portable filename policy,
ASCII-fold collisions are rejected, and ordinary definitions are admitted in
exact UTF-16 order, platform first. Two funded bounded merge passes use actual
inventory sizes; policy maxima never become reservation requests. Symlinks,
Windows reparse points, special files, overlapping roots and detectable changes
during single-handle reads are rejected. Names from classfiles never form host
paths. The caller must provide quiescent trees; this is not a sandbox against
hostile concurrent filesystem mutation. Standard platform I/O/diagnostics keep
Rust's allocator conventions; new input-sized collections grow fallibly.

Platform bytes are supplied by the caller. The launcher makes no vendor
provenance claim. Acceptance uses a genuine extracted Java 21 java.base tree.
An optional root module-info.class must describe java.base and is not an ordinary
definition. Application module descriptors, application java/ definitions,
platform/application name overlaps, previews and class versions outside 51–65
are rejected. Package-info remains ordinary metadata.

The child application namespace imports exactly Object, String, Throwable,
Error, Cloneable, Serializable and Class through checked actual delegation. Each same
record supplies the verifier binding and immutable paid route index. Other
platform classes remain facts under their own bootstrap namespace. Application
Class CP entries are eagerly checked, including dead code. Descriptor-only
symbolic names are resolved only when existing rules need actual facts. This
closed-input policy can reject an unused invalid/missing class earlier than a
lazy reference JVM; that is an image-preflight difference.

Hierarchy plans are built under remaining physical row/edge budgets. Every
application definition, the seven imports and their transitive superclass/interface
closure receive whole-class type proof. Other platform definitions are fact-only.
Proof usage is aggregated across the image with independent inclusive counters,
remaining-child clamps and exact nested error sources. Per-class/per-method
policies retain precedence on ties. Borrowed proofs are consumed before the same
domain is prepared. Every executable root obtains a checked preparation witness,
and only the exact proved union is admitted to the one heap. Fact-only metadata
cannot acquire an execution layout through a callback or late auto-admission.

The execution adapter binds the universe before opening one checked context,
selects an opaque checked main token, allocates checked static state and one real
non-null bootstrap String array with the requested argument count, and runs one monitored InitializationSession. The
array needs no Class CP entry naming String and does not initialize its component.
Inherited main uses one shared engine evaluation for named-child initialization
and the selected declaring body; preflight/fuel/frame/Code limits are not reset
between them. Existing standalone inspect and lower-level runtime APIs retain
their own contracts.

Returned, supplied uncaught Throwable and preserved host execution failure remain
distinct. VM-produced exceptions still report the engine's explicit unsupported
materialization boundary; the launcher does not fabricate exception objects.
Observed static effects survive an engine failure. A failed input/proof/setup
cannot expose or resume a partial executable image. This profile is not complete
Java SE compatibility, a module implementation, a general classpath loader or a
performance claim.

Ordinary Rust tests use bounded synthetic metadata and do not need a JDK. Explicit
private genuine-directory checks accept CRABSPOT_APPLICATION_BOOT and
CRABSPOT_APPLICATION_CLASSES and run with --ignored; reference compilation,
extraction and other-VM observations belong to the separate coordinator tooling.


## Monitor-enabled applications

The ordinary library entry and CLI now enable bounded single-thread monitors and
synchronized selected methods, including main. The same verified image, actual
String array and one named-root initialization/evaluation are used.
`run_directory_application_with_monitor_limits` adds an explicit `MonitorLimits`
argument; the existing entry uses its defaults. ApplicationOptions remains
unchanged; String diagnostics add fields to ApplicationUsage. Class initialization remains distinct from the implicit class lock:
the named/target initializer completes before synchronized main/helper activation.

A monitor entry limit failure after named initialization is `MainEvaluation` and
retains initializer writes; a later return/exit failure retains earlier body writes.
Resource failures have a typed `ExecutionErrorKind::Monitor` source. Null and illegal
monitor state remain explicit unsupported exception materialization, never a fake
Throwable or handler match. The profile enforces both structured-locking rules,
including immediate failure when a callee explicitly exits only an implicit/caller
claim. This authorized phase can differ from raw reference-VM malformed-locking
observations. There are no application-visible monitor inspection natives,
waits, blocking or concurrent Java threads. Existing lower-level unmonitored
APIs retain their old instruction/declaration gates.


## Passive Class objects and getClass

Ordinary and custom-monitor directory entry points now enable canonical mirrors.
`run_directory_application_with_mirror_limits` takes the same image options plus
MonitorLimits and ClassMirrorLimits. Defaults allow 16,384 heap mirrors and 262,144
domain Class sites; existing options remain unchanged. String diagnostics later
add usage fields; legacy inspect behavior remains unchanged.

Class is now the seventh universally requested bootstrap import, even when an
application compares class literals typed as Object and has no Class spelling in
its CP. Its actual superclass/interface closure is proved, witnessed and admitted;
those dependencies do not become additional application-visible imports. This can
increase image proof, plan, slot and orchestration usage. Supplied synthetic Class
stubs must deliberately be public final ordinary subclasses of bootstrap Object.
The Object native declaration is checked lazily, so an unrelated numeric program
need not contain a getClass declaration in its test-only bootstrap metadata.

Setup enables the domain Class-site store, binds the reference universe, then binds
mirror issuance on the pristine heap before opening the one checked context. It
still selects the opaque actual main, allocates static state and one real
String array of the requested length, and runs one initialization evaluation. Class and represented types
are not initialized by a passive literal or getClass. Genuine Class static active
use still runs its actual initializer and can stop at an unsupported registerNatives
operation; this feature does not mark the Java library initialized.

Only the real selected bootstrap Object.getClass declaration can use the one
no-Code native path. Unsupported introspection/native overloads and Class instance
state remain guarded. Mirrors can flow through ordinary application storage and
reference arrays. A nominal mirror's explicit lock and its represented class's
static synchronized lock are identical. The public library test sets exact
recursion limits to verify lock identity. The physical CLI runs the same program
with default limits, without a fake holdsLock native. Array mirrors remain distinct from nominal locks and other array types.

Typed mirror and ordinary instance quotas retain earlier Java writes. Resolution
outcomes and issued objects have separate lifetimes; the closed image can reject
missing definitions before execution rather than fabricate a JVM linkage object.
The profile supplies no scalar primitive/void mirrors, reflective Class services,
Java ClassLoader objects, general native access, a VM-created Throwable allocator
or general String library execution. Genuine reference programs and their vendor-specific failure stages
remain separate coordinator evidence.


## UTF-16 arguments and bounded String constants

All directory entry wrappers now select the String-enabled initialization session.
The old function signatures and ApplicationOptions remain intact. Their default
invocation has zero arguments and default String limits; custom monitor/mirror
wrappers preserve their supplied ceilings. The additive
`run_directory_application_with_arguments(directories, entry, observations, options,
invocation)` accepts `ApplicationInvocation { arguments: &[&[u16]], monitors, mirrors,
strings }`. Its default borrows an empty slice. Values preserve every code unit,
including NUL, isolated surrogates, duplicate empties and whitespace; they never
round-trip through a Rust UTF-8 String.

`ApplicationStringLimits` contains the heap String policy and argument ceilings.
Defaults allow 65,536 arguments, 4,194,304 total argument units and 1,048,576 units
per argument. Count also has the independent Java i32 array-length ceiling. Count,
then each value's individual/cumulative length, are checked before directory
traversal or Java effects. Typed indexed `ArgumentError` failures use Input stage.
No policy maximum is used as an allocation size.

Bind the String role on the pristine mirror-enabled heap before opening its checked
context. After main/observation/static/exception setup, allocate one real String[] of the exact length and
issue/store one fresh object per position. Equal inputs remain distinct, including
empty arguments, and setup does not intern them. Setup failures start no engine
evaluation. `ApplicationUsage.argument_strings` counts each successful issuance,
including one whose subsequent checked store fails; `.strings` reports all accepted
heap String usage on every post-binding exit. `.argument_arrays` records published
array allocation immediately. A failing named initializer still retains its full
argument setup usage and earlier Java writes in the result. No live reference
escapes the one-shot image.

CLI suffix syntax is `BINARY.MAIN -- ARG ...`; absence of a suffix or a lone `--`
means zero arguments. A token before the separator is a syntax error (exit 2).
After it, option-like tokens, empty strings and whitespace are data. Windows uses
native UTF-16, preserving lone surrogates. Unix requires strict UTF-8 and converts
it to UTF-16; malformed bytes are indexed Input failures (exit 1). Width/count
validation precedes retained copies; rejected oversized values stop at a bounded
prefix. No lossy replacement, Unicode normalization or shell interpolation occurs.
OS argv cannot contain NUL; the public library API can. Platform transport tests
and exact-unit language tests establish separate claims.

The heap's initially empty pool is shared across loaders within the image. Literals,
String ConstantValue installation and exact selected native intern share canonical
objects. Literal/argument creation and intern do not initialize java.lang.String.
Real static active use keeps the actual initializer and its unsupported bootstrap
boundaries. Genuine Java String instance bodies, arbitrary natives, constructors,
assertion/Throwable boot dependencies and concatenation remain outside this kernel.
The public library supplies actual String references, not fabricated results for
length, hashing or other library methods. Supported compact images can now run
the genuine length/emptiness Code under the optional binding described below. Synthetic platform role stubs must make
String a public final ordinary direct Object subclass; original genuine application
and platform bytes remain unchanged.


## Verified compact String Java Code

Ordinary directory wrappers and the unchanged CLI now probe the canonical bootstrap
String for the supported compact initializer recipe. A clean recipe mismatch keeps
the existing String kernel and its original proof set; a policy/metadata/allocation
failure or an incompatible role after recipe recognition is an explicit typed error.
The probe never converts an error into an opaque-mode success.

`run_directory_application_with_string_bodies(directories, entry, observations,
options, invocation, profile)` adds a separate Copy `ApplicationStringBodyProfile`.
It contains `mode` (Auto or RequireCompact), `discovery` and `backing`. Existing
ApplicationOptions, ApplicationInvocation and argument limits keep their fields.
Auto/default limits are used by all earlier entry points; RequireCompact is a
library preflight policy and adds no CLI switch. Discovery has a separate inclusive
4,194,304-work default. Backing defaults to 33,554,432 work. Zero and MAX limits
are meaningful and never request policy-sized allocations.

Discovery runs once after named dependency checks/hierarchy collection, before
executable-root selection. It uses the actual designated bootstrap LoaderId and
normalized checked CP operands, accepting shifted indices rather than an asserted
binary hash. Matching supplies exactly three private seeds: the case-insensitive
comparator implementation, java.util.Comparator and java.io.ObjectStreamField.
Their real hierarchy closure must pass the same whole-class proof ceilings,
preparation witnesses and layout admission as application/public-import classes.
An invalid unused body or one-short proof budget in a mandatory helper cannot be
skipped. These bootstrap execution seeds create no application-loader import or
initiation capability: the seven public platform imports and eager application
Class-CP restriction remain unchanged.

After the existing reference/mirror/String binding, the adapter binds the opaque
compact plan on the pristine heap before context/argument setup. It enables bounded
checked nests before member preparation, so the genuine private comparator
constructor executes through the ordinary construction protocol. There is still
one named-main evaluation and one shared fuel/frame/Code/initialization domain.
Neither a host substitute nor an extra evaluation initializes the library.

The matched genuine Java length/isEmpty/coder methods execute their actual
instructions. isEmpty materializes the backing passively; length enters coder,
whose actual COMPACT_STRINGS getstatic runs the real String initializer and its
default-interface/constructor dependencies. Backing allocation before a later
failure remains observable; no root preflight failure creates one. String storage
uses actual stable, read-only managed byte arrays, with the compact Latin-1 or
low-byte-first UTF-16 policy, preserving NUL and isolated halves. Empty Strings
still need one zero-length backing array when read. Existing heap array/length/
element/payload limits remain independent from String and discovery policies.

`ApplicationUsage.string_body_discovery` reports paid probe work on every later
exit, including strict refusal and proof failures. `string_bodies` reports paid
backing work, published `backings` and `backing_bytes` after setup/execution errors
or normal completion; neither counter is folded into StringUsage. A zero-method,
flag-valid String role costs exactly19 discovery work; limit18 rejects the final
16-unit name comparison with accepted prefix3 and required19. Backing work is
1+n+B for a cold read, 1 for a warm read, and an empty cold read costs1 plus one
real array. Early array-limit failure precedes encoding work and publishes nothing.

The initial semantic recipe targets the measured compact Zulu21 image. OpenJ9's
different actual initializer remains an explicit strict unsupported-shape control,
even where its language-level length results agree. Hosted Temurin images require
their own recorded shape/result evidence. Noncompact storage, ordinary String
constructors, charAt/hashCode and equality paths without the separate binding, arbitrary platform access and
VM-produced Throwable objects remain outside this binding. The wider words
application must retain the precise next unsupported site after genuine length;
this profile does not claim complete Java library startup or compatibility.


## Verified compact equality and bootstrap assertions

The ordinary directory wrappers and physical CLI additionally use an Auto equality
probe after successful compact-body discovery. The public six-argument
run_directory_application_with_string_equality(directories, entry, observations,
options, invocation, profile) accepts ApplicationStringEqualityProfile { bodies,
mode, discovery, bootstrap_assertions }. Bodies retains the original compact
profile. StringEqualityMode is Auto or RequireCompactEquality; the immutable
BootstrapAssertionPolicy is Disabled by default or explicitly Enabled. Existing
ApplicationOptions, ApplicationInvocation and ApplicationStringBodyProfile literals
remain source compatible. The CLI adds no assertion switch and uses Auto/Disabled.

The second probe has its own inclusive 4,194,304-work default and typed paid-prefix
errors. Auto clean absence preserves the earlier compact/kernel capability; a
claimed equality recipe with an invalid helper, Class field, full dead branch,
handler or native declaration fails before arguments and execution. Strict equality
requires both probes; unsupported compact layout has zero equality work. Proofs
remain mandatory after matching. Only StringLatin1 is an extra executable private
seed; ClassLoader/Map are metadata facts, not app imports or blanket heap layouts.
All real Java bodies share the same image proof/preparation and single evaluation.

The exact selected String.equals and StringLatin1.equals Java Code perform the
comparison. Class.desiredAssertionStatus and Class initialization also execute their
full genuine Code; the two fixed native sites complete in their actual callers
through the ordinary initialization scheduler. ApplicationUsage reports separate
string_equality_discovery and string_equality { natives_registered,
assertion_queries }. The session snapshot is taken immediately after root execution,
including failures, before status/observation access. Pre-session failure reports
zero session state. A late Class array failure preserves registration and earlier
writes without allowing a failed-class query or retry. Policy changes never alter
the equality result or permit app/array mirror queries.

Independent emitted image tests run ten comparisons through one Object.equals CP
reference on nine ASCII arguments, assert 511/10, then 507/10 when only the second
argument changes. The physical CLI guest checks its own mask and an opposite-mask
control reaches actual division by zero. The public library separately verifies
distinct empties, embedded NUL, isolated surrogates and both compact coders, avoiding
OS transport assumptions. Separate Enabled/Disabled images observe both genuine
bootstrap Class query results. Tests also pin exact probe limits, private proof
seeds, old wrapper fallback, full-body role mutations and retained execution state.
Original vendor image/reference replay remains separately recorded by the coordinator.
No private boot class bytes are shipped and no host equality or synthetic Java
initializer replaces the library Code. OpenJ9's different recipes remain an
explicit strict unsupported-shape control; this binding makes no vendor-wide
compatibility or general assertion/native-loading claim.

# Verified directory application entry

The library constructs one sealed, single-use execution image from explicit
platform and application directories. The CLI provides:

```
crabspot inspect [--enable-preview] FILE
crabspot run --platform DIR --classes DIR BINARY.MAIN
```

A successful run prints nothing and exits zero. Run syntax errors and unknown
commands exit 2; legacy inspect errors remain exit 1. Image, setup and engine
failures exit 1 with a structured stage diagnostic. Application
arguments, JARs, arbitrary loader code, standard streams and general native
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
application definition, the six imports and their transitive superclass/interface
closure receive whole-class type proof. Other platform definitions are fact-only.
Proof usage is aggregated across the image with independent inclusive counters,
remaining-child clamps and exact nested error sources. Per-class/per-method
policies retain precedence on ties. Borrowed proofs are consumed before the same
domain is prepared. Every executable root obtains a checked preparation witness,
and only the exact proved union is admitted to the one heap. Fact-only metadata
cannot acquire an execution layout through a callback or late auto-admission.

The execution adapter binds the universe before opening one checked context,
selects an opaque checked main token, allocates checked static state and one real
non-null empty bootstrap String array, and runs one monitored InitializationSession. The
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
empty String array and one named-root initialization/evaluation are used.
`run_directory_application_with_monitor_limits` adds an explicit `MonitorLimits`
argument; the existing entry uses its defaults. ApplicationOptions and usage layouts
are unchanged. Class initialization remains distinct from the implicit class lock:
the named/target initializer completes before synchronized main/helper activation.

A monitor entry limit failure after named initialization is `MainEvaluation` and
retains initializer writes; a later return/exit failure retains earlier body writes.
Resource failures have a typed `ExecutionErrorKind::Monitor` source. Null and illegal
monitor state remain explicit unsupported exception materialization, never a fake
Throwable or handler match. The profile enforces both structured-locking rules,
including immediate failure when a callee explicitly exits only an implicit/caller
claim. This authorized phase can differ from raw reference-VM malformed-locking
observations. There are no application-visible monitor inspection natives, class
mirrors, waits, blocking or concurrent Java threads. Existing lower-level unmonitored
APIs retain their old instruction/declaration gates.


## Passive Class objects and getClass

Ordinary and custom-monitor directory entry points now enable canonical mirrors.
`run_directory_application_with_mirror_limits` takes the same image options plus
MonitorLimits and ClassMirrorLimits. Defaults allow 16,384 heap mirrors and 262,144
domain Class sites; existing options and usage structure layouts are unchanged.
The CLI syntax and legacy inspect behavior are unchanged.

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
still selects the opaque actual main, allocates static state and one real empty
String array, and runs one initialization evaluation. Class and represented types
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
Java ClassLoader objects, general native access, VM-created Throwable or String
implementation. Genuine reference programs and their vendor-specific failure stages
remain separate coordinator evidence.

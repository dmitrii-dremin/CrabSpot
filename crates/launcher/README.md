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
Error, Cloneable and Serializable through checked actual delegation. Each same
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
non-null empty bootstrap String array, and runs one InitializationSession. The
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

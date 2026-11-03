# Runtime definition registry and lifecycle architecture

Status: the first definition-registry increment is authorized by `m2-call-stack-review.md` on 2026-11-03, using the coordination API below. The broader lifecycle and heap sections are binding architectural boundaries, not authorization to skip their later implementation contracts or claims of implemented services.

## First coherent increment

Add a nonempty `runtime` crate implementing a bounded definition registry. It owns immutable parsed class metadata and assigns opaque registry-scoped identities. It does not execute code, open platform paths, invoke Java, perform inheritance lookup or mark classes verified/initialized. This is usable class-definition/source groundwork for later runtime services, rather than a simulated complete classloader.

Provide LoaderId, ClassId, MethodId and FieldId handles. A loader belongs to one registry. A class identity belongs to that registry and a defining loader; method/field handles additionally identify an actual member of their owner. Every lookup validates ownership and index bounds. The implementation may use a private unique registry token plus checked indices; do not expose forgeable raw integers as sufficient identity. A small uniqueness allocator is allowed, but there must be no process-global class registry or global execution state.

Store definitions by exact defining-loader identity and semantic binary class name, represented by Java UTF-16 units in internal slash form. Equal names in different defining loaders identify different classes. Duplicate definition within one loader fails without replacing the previous class. Constant-pool aliases and display spelling must not affect identity. Classfiles for module descriptors and array-descriptor names are outside this ordinary definition service and fail explicitly.

Define from supplied bytes with an expected name, a defining loader and parser options. Validate registry/loader ownership, input budget, parse result and declared-name match before publishing a new identity. Registration is atomic on failure: callers never observe partially inserted definitions or consumed identity entries for failed publication. Retain the parsed model, not a second unnecessary raw-byte copy. Registration success is named and documented as an unverified, unlinked definition.

Provide direct declared-member lookup by semantic name and descriptor, plus checked metadata access through member handles. Preserve access flags and symbolic references. Do not search an unresolved superclass or label own-member lookup as Java method/field resolution. The kernel can later consume these immutable definitions through an explicit bridge without changing what registration proves.

## Source boundary and resource policy

Define a narrow source trait that receives the requested name and a byte limit and returns definition bytes, absence, or a structured source failure. A source belongs to the explicit defining-loader request; no parent delegation is inferred. The initial test/source implementation is an in-memory map. Filesystem, archive, module-image and user-defined Java loader adapters remain outside the core registry.

The source-loading entry point first checks the registry for an existing matching definition, then reads and defines when absent. Repeated loads return the same ClassId without rereading or redefining; explicit duplicate definition still fails. Distinguish missing data, source failure, parser rejection, wrong declared name, duplicate definition, invalid handle and resource exhaustion. These host-service errors are not yet Java linkage exception objects. Do not cache all failures as equivalent erroneous classes; later loading/linking stages specify their own sticky behavior.

Bound individual source bytes and aggregate registered source-byte volume, definition count and loader count. The byte accounting is a reproducible input-volume policy, not exact heap allocation. Check cumulative arithmetic before insertion. A suggested initial policy is the parser's 16 MiB per definition, 64 MiB aggregate source volume, 1,024 definitions and 256 loaders; expose explicit options and test boundary behavior. Immutable metadata and source keys have their own bounded representation overhead. Sources must honor the supplied read limit before materializing unbounded external data.

## Initial coordination API

Use `DefinitionRegistry`, `RegistryOptions`, `RegistryError`, and private-field, Copy/Eq/Hash handles `LoaderId`, `ClassId`, `MethodId`, `FieldId`. `RegistryOptions` contains `max_loaders`, `max_definitions`, `max_definition_bytes`, and `max_total_definition_bytes`, all usize, with the defaults above. Construction is fallible if the chosen identity allocator can exhaust its namespace.

The agreed method names are `new(options)`, `create_loader()`, `define(loader, expected_name, bytes, parse_options)`, `load_from_source(loader, name, source, parse_options)`, `lookup_class(loader, name)`, `class(class_id)`, `defining_loader(class_id)`, `find_declared_method(class_id, name, descriptor)`, `find_declared_field(class_id, name, descriptor)`, `method(method_id)` and `field(field_id)`. All except simple counters return Result with RegistryError; lookup methods return Option inside Result. Names/descriptors take `&[u16]`, bytes take `&[u8]`, and parsing uses the existing Copy ParseOptions. Metadata access returns immutable `&ClassFile` or `&Member`. Expose loader/definition counts and aggregate source-byte accounting for host policy and tests. Member handles expose their owning ClassId through an accessor, without allowing identity forgery.

`DefinitionSource::read_definition(&mut self, name: &[u16], max_bytes: usize)` returns `Result<Option<Vec<u8>>, SourceError>`. A simple InMemorySource maps semantic names to bytes and checks length before cloning. SourceError carries an actionable message; RegistryError distinguishes it from absence and all definition/handle/policy failures. Source loading passes the minimum of parser, per-definition and remaining aggregate allowances and rechecks returned lengths even if a source violates its contract. Cached definitions return before source reads; parsing options apply to new admissions, not retroactive revalidation of registered metadata.

Do not automatically create a fake Java ClassLoader instance or claim a bootstrap delegation policy. `create_loader` creates only an opaque defining-loader identity for this registry service. Future bootstrap/user-loader mechanisms bind real policies and Java objects to such identities under a separate contract.

## Lifecycle and resolution follow-on

Registration does not imply that superclasses exist, hierarchy rules pass, access is legal or code verifies. Subsequent stages use explicit proof-bearing internal results rather than public booleans that callers can set. Track verification separately from initialization: resolution can remain lazy after preparation, and initializing a class does not make an unverified body safe.

The eventual initialization protocol needs at least NotStarted, InProgress(owner), Initialized and Erroneous(cause). It must distinguish same-execution-context reentry from competing-context waiting, initialize required prerequisites in the specified order, execute actual `<clinit>` code, and preserve specified failure behavior. Until the required services exist, a general Java invocation API and application launcher stay unavailable. Do not skip an initializer, fabricate java/lang/Object/native behavior or count metadata registration as passing initialization tests.

A runtime constant pool resolves symbolic entries in the owning class's loader context and caches typed outcomes according to the appropriate resolution rule. It must distinguish class identity from name, static from instance members, declaration from selection, and access failure from missing definitions. Inheritance, interface selection, arrays, modules and loading constraints require explicit extensions. The existing same-class call cache is not reused as if it proved any of these properties.

## Heap and execution follow-on

Before allocation, agree on managed reference identity, null, heap ownership, stale handles, allocation limits and root enumeration. Numeric Value can move to a backend-independent runtime/value module with interpreter reexports when references are introduced; do not introduce a runtime-to-interpreter dependency. Java references must remain distinguishable from numeric bits and category-two continuations.

A first noncollecting managed heap is permissible with an explicit allocation budget, but it must already expose the roots and reference indirection needed by a moving collector. Arrays carry element kinds and lengths; objects carry runtime class identity and reviewed field layout. Pending exception references and class static storage join frame locals/stacks as roots. No native pointer or Rust object address is the public Java reference.

## Registry increment acceptance

Require public-API tests for same-name/different-loader identity, duplicate preservation, wrong-registry and invalid handles, exact Java-name matching, wrong declared name, parser failures, own-member overload lookup, missing member, and nondefinition/module inputs. Test source caching, absence/error distinctions and all resource limits, including arithmetic boundaries. Registration failures must leave prior definitions and accounting usable.

Use genuine independently compiled Java fixtures as registry inputs and preserve classfile/decoder/interpreter tests. Run formatting, strict Clippy and full workspace tests. Director review must approve the concrete API and implementation before any lifecycle, linking, cross-class execution or heap expansion. Acceptance of this registry does not close M2, M3 or M4.

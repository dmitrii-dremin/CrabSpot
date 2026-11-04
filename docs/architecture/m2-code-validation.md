# Whole-method Code validation foundation

Status: implemented and approved by the technical director on 2026-11-04 after frozen source review and the independent fresh-target gate: 998 workspace tests and 911 optimized verifier/runtime/interpreter tests pass. Implementation 08386b361be4992e4c02ef0931aff46185c809e3 is published; inspected Linux/Windows CI run 41867714340 passed. See `m2-code-validation-review.md` for exact source/evidence and resolved review points. The bounded contract was dispatched from construction evidence commit c23d18d. No NanoSpec. Full M2 and full verification remain open.

## Purpose and scope

Add a standalone `crabspot-verifier` workspace crate depending only on `crabspot-classfile` and `crabspot-bytecode`. It checks one selected method's entire Code, including unreachable instructions, without executing instructions or resolving any Java class. All decoder-supported instruction formats participate, including opcodes not executable by the interpreter. Existing parser, registry, interpreter, session and launcher entry points do not acquire a new implicit gate.

The normative inventory is [JVMS 4.9.1](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.9.1), with the Code/StackMapTable representation rules in [4.7.3](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.7.3) and [4.7.4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.7.4). Instruction layouts, branch destinations, CP operand categories, names, dimensions and local ranges are checked throughout the method. Handlers and stack-map locations must denote appropriate boundaries; uninitialized offsets must name new instructions. The distinction from [4.9.2](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.9.2) and [4.10](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.10) is mandatory: no stack/local flow, reachability, merge, assignment, constructor-chain or protected-receiver proof is provided.

Successful output is a partial method Code validation result, never a VerifiedClass/VerifiedMethod, execution permission, linkage result or JVM certificate. Do not synthesize ClassFormatError or VerifyError objects. The service returns its own structured diagnostics. JDK comparison distinguishes definition, reflective verification and execution stages, and does not demand one universal Java error category for all failures.

## Public API

```rust
pub struct CodeValidationOptions {
    pub allow_preview: bool,                 // default false
    pub max_code_bytes: u64,                 // default 65_535
    pub max_metadata_entries: u64,           // default 262_144
    pub max_utf16_units: u64,                // default 1_048_576
    pub max_work_units: u64,                 // default 4_194_304
}
// Clone + Copy + Debug + Eq + PartialEq + Default as appropriate.
pub struct CodeValidationUsage {
    pub code_bytes: u64,
    pub metadata_entries: u64,
    pub utf16_units: u64,
    pub work_units: u64,
}
pub struct MethodCodeValidation<'a> { /* private */ }
impl<'a> MethodCodeValidation<'a> {
    pub fn class(&self) -> &'a ClassFile;
    pub fn method_index(&self) -> usize;
    pub fn method(&self) -> &'a Member;
    pub fn code(&self) -> Option<&'a Code>;
    pub fn decoded(&self) -> Option<&DecodedCode>;
    pub fn usage(&self) -> CodeValidationUsage;
}
pub fn validate_method_code(
    class: &ClassFile, method_index: usize, options: CodeValidationOptions,
) -> Result<MethodCodeValidation<'_>, CodeValidationError>;
```

The result borrows the entire immutable class and stores the selected method/Code index and one owned DecodedCode. Its private fields ensure code and decoded are either both present or both absent. The borrow prevents mutation of the inspected metadata while the result is used. Do not clone ClassFile, Code bytes, handlers, names or stack maps into the result. Do not add a conversion to a verification certificate. Usage is evaluation-local; no persistent cache or context is exposed.

An ordinary abstract or native declaration with no Code succeeds with both optional getters absent. A concrete declaration without Code, a body on an abstract/native declaration, or multiple Code bodies fails. Class initializers use their semantic static role, ignoring raw nonsemantic flags as the existing parser does. Historical parameterized class initializers are an explicit unsupported case in this first service, not invalidated as a format violation. First reject a malformed descriptor as InvalidDescriptor and a nonvoid initializer as InvalidMetadata; UnsupportedHistoricalInitializer applies only to an otherwise valid historical void initializer with parameters. A historical ordinary `()V` initializer is supported even when its raw static flag is ignored by that class-file version.

## Errors and phase order

```rust
pub enum CodeValidationResource { CodeBytes, MetadataEntries, Utf16Units, WorkUnits }
pub enum CodeValidationLocation {
    Method,
    MethodAttribute(usize),
    Instruction(u32),
    ExceptionHandler(usize),
    CodeAttribute(usize),
    StackMapFrame { attribute: usize, frame: usize },
}
pub struct CodeValidationError {
    pub method_index: usize,
    pub location: CodeValidationLocation,
    pub usage: CodeValidationUsage,
    pub kind: CodeValidationErrorKind,
}
pub enum CodeValidationErrorKind {
    UnsupportedVersion { major: u16, minor: u16 },
    PreviewDisabled,
    UnsupportedHistoricalInitializer,
    MethodIndexOutOfBounds { index: usize, count: usize },
    InvalidMetadata(&'static str),
    InvalidConstant { index: u16, expected: &'static str },
    InvalidDescriptor { index: u16 },
    InvalidName { index: u16 },
    InvalidOperand(&'static str),
    LocalOutOfBounds { index: u16, width: u8, max_locals: u16 },
    InvalidBoundary { offset: u32, expected: &'static str },
    InvalidStackMap(&'static str),
    Decode(DecodeError),
    ResourceLimit { resource: CodeValidationResource, limit: u64, required: u64 },
    AccountingOverflow { resource: CodeValidationResource },
    AllocationFailure { resource: &'static str },
}
```

Provide Debug, Display and Error, with the original DecodeError as source. No lossy Java-name copies in diagnostics. Fixed reason strings explain the failed rule; location supplies a method-relative instruction or metadata index, not a fabricated class-file byte offset. Selected-method bounds errors still report the requested index.

Order: version policy; method-index lookup; selected method name/descriptor and minimum role checks; method-attribute inventory and Code presence; Code length and parameter-local capacity; funding and whole decode; instruction checks in offset order; handler table in original order; nested Code attributes in original order, including stack maps. Within an instruction, confirm the required CP tag before following its indices; check the descriptor before relationships that require its shape. A resource check precedes the operation it funds. The first error stops the invocation; partial internal state is dropped and input is never mutated. This service's order is new and explicit; existing runtime diagnostic order remains unchanged.

## Versions and selected metadata

Accept the existing parser's Java SE 21 version envelope: major 45 through 65; minor unrestricted through 55; minor zero for 56 through 64; major 65 minor zero, or 65.65535 only with allow_preview. Reject other envelopes as unsupported. Preserve decoder version restrictions, including modern jsr/ret refusal. Acceptance of historical file-local constraints is not historical type-inference verification. StackMapTable is supported from major 50; historical parameterized clinit returns UnsupportedHistoricalInitializer before Code processing.

The ClassFile model is publicly mutable, so parse success is not treated as an unforgeable certificate. Check selected indices, relevant vector counts and every metadata relationship this service uses. Method and Code attribute counts, handler counts, frame counts and full verification-type lists must fit their class-file count fields. A selected constructor must be nonstatic, nonabstract, nonnative, class-owned and void; modern clinit must be static and ()V. Other method flag combinations outside the Code-presence/receiver decisions remain parser responsibility. Reserved flags do not become arbitrary new rejection rules.

The constant pool's private representation comes from the parser, but may be transplanted into a different mutable ClassFile. Therefore all used indices/tags, required UTF-16 spellings and version-sensitive operand relationships are checked against the current class. This is not a rescan of unrelated pool entries, all class flags/members, bootstrap method contents, module metadata or arbitrary attribute payloads. The enclosing class's methods_count representation is likewise outside this selected-method result; selected-index bounds are checked without scanning or certifying that member inventory.

Selected ordinary method parameters plus a nonstatic receiver must fit the descriptor's 255-slot limit and Code max_locals. The supported clinit initial state has zero arguments/receiver. A bodyless result still checks the selected name/descriptor and attribute inventory, but makes no max_locals or Code claim. Full class/interface method-flag legality, duplicate member identities and unrelated member declarations are explicitly outside this service.

Every scanned attribute header must reference Utf8. Preserve the parser's extension-attribute policy: a predefined name is recognized only at its defined location/version. Method-level Code is recognized in the supported envelope; nested StackMapTable is recognized from major 50. At those recognized locations/versions the name must have its matching typed body, rather than Opaque or a different body. A typed Code body nested inside Code, or a typed StackMapTable body at method level/before major 50, rejects as inconsistent inspection metadata. However, an Opaque attribute named Code inside Code, or named StackMapTable at method level/before major 50, remains an ignored extension. Its internal `standard` display marker does not grant it semantics. Other attribute bodies remain outside this first partial result, including debug tables, annotations and their semantic/type targets. No traversal into ignored annotation/value trees is performed or claimed.

## Instruction pass

Reuse decode for byte layout, reserved opcodes, wide forms, switches, targets and encoded reserved/count/atype bytes. Do not duplicate its parser. Check all instructions afterward, never only a control-flow walk.

The opcode matrix is implementation data in the verifier and must have direct coverage:

- ldc/ldc_w accept category-one loadable constants; ldc2_w accepts category-two numeric or Dynamic descriptors. A Class literal needs major 49; MethodHandle/MethodType need 51; Dynamic needs 55. Long/Double pool holes are checked for those real wide entries, never required after a category-two Dynamic entry. No bootstrap is invoked.
- Field operations require Fieldref plus a field descriptor. Invocation forms require their specified member-reference tags, version-sensitive interface tags for b7/b8, or InvokeDynamic for ba. Follow Class and NameAndType references and validate their relevant spelling/descriptor. Only b7 may name `<init>`, whose descriptor must be void and tag Methodref; other special method names reject. invokeinterface's count is exactly receiver plus argument slots, including category-two widths. No existence/access/staticness or constructor-owner resolution occurs.
- Class operands have checked class/array spellings. new excludes array names; anewarray's result rank cannot exceed 255; multianewarray's nonzero requested dimensions cannot exceed the referenced array rank. No instantiability or class identity lookup occurs.
- Every explicit, implicit and wide local index is checked with its opcode's width, using checked addition rather than max_locals subtraction. ret remains a width-one local operand only in versions the decoder accepts; no return-address flow claim is made.

Use a compact, allocation-free UTF-16 descriptor scanner for summaries and the selected method's initial local widths. Preserve Java code units and the parser's name grammar; do not call an owning descriptor parser per instruction. Array rank, parameter slots and grammar overflow reject. Method-reference owner arrays remain syntactically valid Class spellings; this pass must not copy the interpreter's unsupported-array-owner execution policy. A dead interface-static operand with a lawful tag can pass without linking its named method.

For a loadable String, only its referenced Utf8 tag is inspected; literal contents need no semantic scan or copy and incur no UTF-16 content charge. MethodType checks its method descriptor. MethodHandle checks its reference-kind/target-tag relationship under the current version: kinds 1..4 require Fieldref, 5/8 Methodref, 6/7 Methodref or (from major 52) InterfaceMethodref, and 9 InterfaceMethodref. Other kinds reject. Field targets retain field-name grammar. Kind 8 requires `<init>` and a void descriptor; other method targets forbid initializer names. Check the target descriptor's 255-slot bound with a receiver for kinds 5/7/8/9, without one for kind 6 or MethodType. Dynamic and InvokeDynamic check their NameAndType shapes; BootstrapMethods attribute existence, bootstrap index bounds, arguments and contents are all deferred. No class-attribute inventory occurs. This keeps transplanted pools version-aware without turning the pass into whole-class validation. Invocation descriptors must also fit 255 parameter slots including the implicit receiver for b6/b7/b9; b8/ba have no receiver contribution. Signature-polymorphic runtime adaptation remains outside this file-local check.

Return-opcode/type consistency, operand-stack depth/types, fall-off reachability, assignment/protected relationships, method-selection constraints and constructor-use legality are deliberately not claimed in this pass. They belong to the next type-state contract; do not invent a reachable-only shortcut or an incomplete verifier under the current result name.

## Handlers and stack-map representation

Handlers: require start < end; start/handler are instruction starts; end is an instruction start or Code length. Nonzero catch_type must reference a syntactically nonarray Class. No Throwable ancestry, class accessibility or resolution is attempted.

For StackMapTable, recompute offsets from offset_delta with checked arithmetic and require equality with each stored bytecode_offset. Each computed offset is an instruction start, strictly within Code. Enforce one table, normalized frame shape lengths (Append/Chop one through three) and version eligibility. The parsed model has already discarded some equivalent compact/extended tag distinctions; do not claim to reconstruct or revalidate discarded wire tags. Missing tables remain absent; do not infer that required branch frames exist.

Validate every explicit verification type: Object references a valid Class spelling; Uninitialized points to an actual new instruction; UninitializedThis is limited to a selected constructor. Maintain only current logical local-entry widths and a cached slot total, seeded from the selected method descriptor and receiver, with no max_locals-sized Top padding. For ordinary static methods omit receiver; for supported clinit use an empty initial frame. Process Chop by removing entries, Append by adding entries, Full by replacing the current widths, and Same without rescanning retained locals. Long/Double each occupy one encoded entry and two slots; an explicitly encoded Top is a separate one-slot entry. Check locals against max_locals and each frame's stack slot sum against max_stack. Underflowing Chop and inconsistent public frame metadata are structured failures.

Do not retain expanded frames per bytecode offset, require type equality across instructions, resolve Object types, or establish constructor flag propagation. This width/offset representation validation is only one input to a later type checker. A full frame may describe unreachable code without making its preceding fall-through executable.

## Work and memory accounting

Options and usage counters are u64, inclusive; usize conversions and every increment are checked. Each invocation starts at zero. Zero/MAX policies with small actual inputs are supported without allocating a policy-sized buffer. On a failed multi-counter charge, none of that charge's counters advance; previously funded work remains in the error's usage.

- Code length is first checked as 1..65535 without decoding. Then charge its entire byte length to CodeBytes and WorkUnits atomically before calling decode. Decoder allocations remain its existing bounded, input-derived allocations; this increment does not claim new recoverable host-OOM handling inside that unchanged API. Its branch binary searches have a fixed maximum depth from the Code-length bound.
- Charge one WorkUnit before each instruction's semantic inspection, each checked CP lookup and each decoded-boundary query. A handler's start, end and handler-target predicates each count as a boundary query, including an end equal to Code length; an ordered row that reaches all three predicates therefore pays three query units. This is logical predicate accounting, not a count of binary-search calls. A verification-type Uninitialized lookup pays one query to obtain its instruction, then inspects that returned opcode without a second lookup/charge. Scalar comparisons and fixed-sized tag handling are included in their enclosing charge.
- Charge one MetadataEntry and one WorkUnit for each method/Code attribute header before inspection. Charge the full handler-list and frame-list entry counts to both MetadataEntries and WorkUnits atomically before their loops. Charge each explicit verification-type list's length to both counters before validation/reservation; a SameLocalsOneStack item counts as a one-entry list. No second per-entry base charge follows a bulk charge; CP/boundary/UTF charges remain additional. The outer attribute/frame entry is separate from its contained type entries. Same/Chop do not charge nonexistent explicit types.
- Charge the full Utf8 length to Utf16Units and WorkUnits atomically before its first semantic scan in a given role. Roles are attribute name, method name, field name, Class spelling, field descriptor and method descriptor. A per-invocation cache keyed by the actual Utf8 CP index and role may reuse a successful compact summary; every use still pays its checked Class/NameAndType/Utf8 link lookups. Outer Class/NameAndType aliases sharing one Utf8 entry therefore share its scan, not their link charges. Never key/hash cached entries by an entire name, copy names, or cache failed validation. Different Utf8 indices with equal text are distinct entries.

New caches/current-local buffers use fallible actual-demand reservation and return AllocationFailure without input mutation. Their entries are bounded by inspected pool indices/roles and funded metadata/UTF-16 input. Keep summaries scalar; only the selected method's current local widths need variable-length state. Do not retain parameter-name trees for every invoked signature. Repeated Same frames must not rescan or copy the current locals. Old capacity may be reused, but no policy maximum is a reservation request. Output owns only DecodedCode plus scalar usage/index fields and borrowed metadata.

## Acceptance and ownership

Runtime worker owns new verifier production/Cargo.toml, private tests and README in one isolated worktree. Interpreter worker owns independent verifier public synthetic tests and their shared synthetic builder in a separate worktree; it does not change interpreter production. Production private tests may own separate private local helpers; neither worker mutates the other's test scaffolding. Coordinator owns workspace manifest/lockfile integration, genuine-corpus integration, original JDK rejection oracle/tooling/fixtures and final evidence. The coordinator may add the workspace member in each worker checkout as build scaffolding before dispatch; workers do not publish those root-manifest/lockfile edits. No decoder or parser behavior changes are authorized by this contract; an identified necessary change must return to the director with exact justification.

Required tests include all CP-operand/local families, dead-code defects, valid unsupported execution opcodes, both widths of Dynamic constants and real wide holes, interface tag/version edges, invocation count 255/overflow, rank 255/256, all stack-map forms and category-two Chop/Append/Full accounting, mid-instruction and wide-embedded targets, uninitialized-to-non-new offsets, malformed transplanted/mutated metadata, bodyless methods and wrong Code presence, preview/historical policy, repeated long descriptors and repeated Same frames, zero/exact/MAX quotas, cumulative atomic charge failures, allocation/overflow private guards, no input mutation, and deterministic repeat validation. Explicit negative examples must distinguish rejected representation from deferred type errors that this partial service intentionally does not certify.

The coordinator validates all Code-bearing methods in the genuine corpus, records legal bodyless declarations, and checks original positive/negative encodings independently on JDK 21. Force the relevant verification stage (for example reflective method lookup after definition) and record stage plus error category; never treat mere defineClass success as verification. Keep reference-JDK execution entirely in tooling. Preserve canonical source/artifact provenance, existing execution comparisons and parser fuzz evidence.

Final gate requires immutable snapshots, source hashes, formatter, strict workspace/all-target Clippy, full workspace debug and optimized verifier/runtime/interpreter tests, reproducible fixtures/oracles and technical review. Use a fresh Cargo target directory or otherwise prove copied snapshots triggered rebuilds; hashes alone do not establish compiled-source identity. Only after acceptance should a separate type-state verifier contract be authorized.

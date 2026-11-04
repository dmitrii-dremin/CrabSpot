# Compatibility status

This is an evidence report, not Java certification. Parsing does not establish
bytecode verification, linking, initialization or execution. M1 now has all 30
standard attribute bodies decoded and was approved by the technical director
on 2026-11-03. The sustained exact-source fuzz campaign passed.

The long-term target is complete specification conformance, initially for Java SE
21, with the required observable behavior of conforming JVMs and HotSpot as the
primary reference. Performance superiority must be demonstrated through controlled,
representative comparisons. Kotlin/Spring Boot execution alone will not establish
completion of that goal.

## Implemented services and boundaries

| Area / JVMS SE 21 clauses | Status | Evidence and boundary |
| --- | --- | --- |
| Reader, header and versions (4.1) | Implemented and tested | Checked reads, exact consumption, majors 45Р В Р’В Р В РІР‚В Р В Р’В Р Р†Р вЂљРЎв„ўР В Р вЂ Р В РІР‚С™Р РЋРЎв„ў65, historical/preview policy and configured byte limits |
| Constant-pool encodings (4.4) | Implemented and tested | Every tag, wide holes, numeric bits, modified UTF-8, typed indices, version introductions, module/package names and method-handle constraints |
| Names, descriptors and members (4.2Р В Р’В Р В РІР‚В Р В Р’В Р Р†Р вЂљРЎв„ўР В Р вЂ Р В РІР‚С™Р РЋРЎв„ў4.6) | Implemented and tested | Descriptor grammar/dimensions/slots, duplicate identities, versioned access flags, class/interface/initializer context and Code presence |
| Standard attributes (4.7) | Implemented and tested for parser stage | All 30 bodies below; later-consumer semantics remain separate |
| Unknown attributes (4.7.1) | Implemented and tested | Payload preservation, including standard-looking names outside applicable version/location |
| Resource bounds | Implemented and tested | Default 16 MiB class input, annotation depth 64, one million annotation/value nodes shared across the whole class; depth hard-capped at 64 even if caller requests more |
| Inspection CLI | Implemented and tested | Escaped metadata, bounded reads, preview opt-in and nonzero argument/input failures; no bytecode execution |
| Genuine Java 21 fixtures | Implemented and tested | 80 classes including array/instance/reference-field bodies, class/interface/special dispatch and cast hierarchies and raw-field oracle metadata; source/artifact digest manifest and compiler regeneration (M1 acceptance used 17); oracle data alone does not establish execution support |
| Parser fuzzing | Implemented and tested | 108,907,980 seeded cases over 300 seconds, zero failures, exact-source fingerprint; no coverage metrics or proof of panic freedom |
| Linux/Windows hosted CI | Passed for f05fa69 | [run 41798076784](https://github.com/dmitrii-dremin/CrabSpot/actions/runs/41798076784) completed all checks on both platforms with Rust 1.99.1 and JDK 21.0.13, including strict lint, 74 regenerated fixtures/eleven oracle sets, debug/optimized tests and fuzz smoke |
| Rust 1.85 minimum version | Not tested | Manifest declaration; actual local toolchain is Rust 1.98.1 |
| Performance superiority over HotSpot | Not demonstrated | Long-term objective; no comparative performance benchmark results are claimed |
| Bytecode decoder | Implemented and tested | All defined JVM instruction formats, version/operand constraints and static branch destinations; 10 focused tests and malformed-input corpus |
| Numeric execution kernel | Partially implemented and tested | 148 opcode values including restricted invokestatic; 616 integral + 7,263 floating + 126 static-call JDK outcomes match in debug/release; bounded method-body evaluation only |
| Primitive-array execution bridge | Implemented and tested | Explicit heap API adds 31 opcode values (179 total), reference locals/branches and primitive arrays; all 280 JDK cases match in debug/release, including 67 symbolic exceptions; numeric/void method signatures only |
| Runtime definition registry | Implemented and tested | 25 debug/release tests; immutable unverified/unlinked definitions, scoped loader/class/member identities, checked admission limits, declared-member lookup and cached in-memory source reads |
| Hierarchy and logical field layouts | Implemented and tested | 21 added tests including genuine javac graphs and a 10,000-class chain; explicit resolver/delegation, kind/final/cycle checks, bounded parent-linked layouts and owner-qualified fields; full derivation/access/override checks remain deferred |
| Shared runtime values and default static storage | Implemented and tested | All primitive/object/array defaults, bounded owner-qualified static locations and read-only root enumeration; ConstantValue and initializers remain unapplied; no lifecycle advancement or static writer |
| Primitive-array heap service | Implemented and tested | All eight kinds, checked opaque identities, typed stores/loads, bounded fallible allocation, 151 JDK roundtrips; no reuse, reclamation or GC |
| Ordinary-instance memory service | Implemented and tested | Atomic multi-plan admission, shared own-field shapes, six quotas, default allocation, owner-qualified numeric/null stores, final protection and borrowed children; 151 raw-putfield JDK cases; no construction/initialization or field-bytecode claim |
| Runtime field resolution/access | Partially implemented and tested | Explicit unnamed-module context, authoritative loader/name bindings, interface-before-superclass lookup, runtime-package/private/protected checks and three cumulative quotas; named modules, cross-class nest access and cross-loader reference constraints remain unsupported |
| Instance-field execution bridge | Implemented and tested within body-kernel scope | Registry-backed MethodId and explicit receiver; 181 opcode values, shared engine and bounded static helpers, getfield/putfield with symbolic access/missing/static/null failures; 402 genuine + 151 raw-store JDK outcomes; no constructor or initialization claim |
| Ordinary virtual-call execution | Implemented and tested within body-kernel scope | Runtime-aware mode adds invokevirtual (182 opcode values), bounded class resolution and package-aware receiver selection, MethodId-keyed cross-class frames and current-frame static helpers; 494 genuine JDK comparisons pass in debug/release; constructor/signature-polymorphic invocation, reference signatures and lifecycle remain unsupported |
| Interface/default dispatch | Implemented and tested within body-kernel scope | Runtime-aware mode adds invokeinterface (183 opcode values) and completes class Methodref default fallback; shared maximal-interface analysis, class priority, private helpers, count validation, symbolic conflicts/abstract failures; all 1,337 genuine JDK comparisons pass in debug/release |
| Ordinary special-call execution | Implemented and tested within body-kernel scope | Runtime-aware mode adds nonconstructor invokespecial (184 opcode values), prescribed superclass/direct-interface selection, effective ACC_SUPER, checked owner/receiver/declarer identities and shared MethodId caches; all 450 JDK cases pass in debug/release; constructors remain unsupported |
| Reference types and cast instructions | Implemented and tested within body-kernel scope | Explicit heap Object/Cloneable/Serializable identities; loader-aware nominal/interface and structural-array assignment; checkcast/instanceof (186 runtime-aware opcode values), null target skipping and symbolic ClassCastException; all 1,276 genuine JDK comparisons pass in debug/release; reference-array allocation remains unsupported |
| Checked non-null instance-reference fields | Implemented and tested within body-kernel scope | Existing putfield stores compatible ordinary-instance/primitive-array values after declaring-loader descriptor resolution and bounded assignment; public raw writes remain numeric/null only; private atomic commit and per-location child edges; all 690 JDK cases pass in debug/release; reference signatures, static writes and loading-constraint enforcement remain unsupported |
| Full linkage, bytecode verifier and initialization | Not implemented | Subsequent increments; registration and layout planning are not full linkage |
| Class loading, reference-array heap, GC, threads, Java libraries, JNI, Kotlin, Spring Boot, JIT | Not implemented | Subsequent milestones |

No executed test currently remains failing. This statement is limited to the
recorded tests; it is not proof that no defect exists.

Latest independent native Windows checks: 551 workspace tests and 464 optimized
runtime/interpreter tests pass, with strict Clippy and formatting clean. All
13,085 execution comparisons match; separate array and field memory services
have 151 oracle roundtrips each. All 80 class fixtures and twelve reference sets
reproduce, with all 74 prior class hashes unchanged. This increment adds 32 tests
covering checked field assignment, aliases/cycles, atomic failures, private commit
defenses, hidden descriptor types, loader/UTF-16 boundaries, deep assignment,
exact shared quotas, current-frame origins and retained effects. The old
non-null rejection regression now checks the missing-universe boundary.
Final frozen review approved the increment in docs/architecture/m2-reference-fields-review.md;
full M2 remains open. Hosted CI for this increment is pending publication;
the preceding run in the table applies only to its named implementation commit.

## Standard attribute coverage

"core" refers to crates/classfile/src/tests.rs and its annotation test module;
"metadata" to crates/classfile/tests/metadata.rs; "reference" and "extended" to
the corresponding fixture integration test files. Each row has typed decoding;
truncation and exact-envelope tests supplement the focused assertions listed.

| Attribute | Positive and negative evidence |
| --- | --- |
| ConstantValue | metadata: 55 descriptor/constant-kind cases, invalid indices and lengths |
| Code | core: code length, parameter locals and exception ranges; reference: branches and catches |
| StackMapTable | core: every frame and verification-type form, reserved tags and bounds |
| Exceptions | metadata: order/empty table, reference kinds, counts and lengths |
| InnerClasses | core: historical anonymous-class rule, names and duplicate semantic entries; javac inner class |
| EnclosingMethod | metadata: optional method, NameAndType descriptor kinds, indices and lengths |
| Synthetic | core: empty payload, valid multiplicity and malformed envelope |
| Signature | metadata: typed Utf8 reference and exact body, preserving later-consumer grammar |
| SourceFile | metadata: typed Utf8 reference, invalid tag/index/length |
| SourceDebugExtension | metadata/core: modified UTF-8 including NUL/surrogates, invalid encodings, raw-byte preservation |
| LineNumberTable | metadata: duplicate starts, zero line numbers, range and length failures |
| LocalVariableTable | metadata: names/descriptors/ranges/category-two slots; core: duplicate variable across tables |
| LocalVariableTypeTable | metadata: names/signature preservation/ranges/slots; core: duplicate variable across tables |
| Deprecated | core: empty payload and valid multiplicity; real annotated module |
| RuntimeVisibleAnnotations | core: all value tags/references/descriptors/budgets; extended javac declarations |
| RuntimeInvisibleAnnotations | core: typed envelope and values; extended javac CLASS-retention declarations |
| RuntimeVisibleParameterAnnotations | core: count independence/bounds; extended javac method parameter |
| RuntimeInvisibleParameterAnnotations | core: count independence/bounds; extended javac method parameter |
| RuntimeVisibleTypeAnnotations | core: all 22 targets/all four path kinds, contexts and local bounds; extended javac Code/generic targets |
| RuntimeInvisibleTypeAnnotations | core: discriminants/context/bounds; extended javac field/generic/Code metadata |
| AnnotationDefault | core: permitted context and resource boundaries; extended javac all 13 element-value kinds |
| BootstrapMethods | core: dynamic constants and bootstrap indices/loadable arguments; reference: lambda and concatenation |
| MethodParameters | core: name validation and count preservation for reflection; javac parameter names |
| Module | module unit tests: all tables, names, duplicate aliases, reserved flags, java.base versions, optional references and truncation; reference fixture with all tables |
| ModulePackages | metadata/module: typed table, empty case, duplicates and inventory relationships |
| ModuleMainClass | metadata: typed Class index, wrong tags/indices/lengths |
| NestHost | metadata: typed index and host/member conflict; real inner class |
| NestMembers | metadata: indices/lengths/conflict; real outer class |
| Record | core: nested attributes and descriptors; reference: record component names/types |
| PermittedSubclasses | core: final-class conflict; reference: sealed interface and two subclasses |

## Deliberate later-stage checks

The parser preserves generic signature strings. Generic grammar, erasure,
type-parameter bounds and type-path interpretation belong to the metadata
consumer. MethodParameters/parameter-annotation association and annotation values
versus resolved annotation declarations belong to reflection. Module graph,
service assignability and class existence belong to loading/resolution.

Code/stack-map/type-annotation offset ranges are checked here. Instruction
boundaries, target opcode identity, control flow, stack/local type state and
Throwable relationships belong to the decoder/verifier. Nest access, inheritance
legality and permitted-subclass loading are also deferred. These boundaries are
reviewed in docs/architecture/m1-validation.md; they are not silently credited
as implemented runtime compatibility.

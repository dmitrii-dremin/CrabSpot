# M1 implementation and review checklist

Status: implementation contract and audit checklist, not an acceptance report. The foundation review remains valid for its original increment. ADR 0005 governs the continued metadata work.

## Ownership and integration

The annotation worker owns the annotation model, parser options/budgets, attribute dispatch and contextual integration. The module worker owns `module.rs` and dedicated module tests. It provides a typed `Module` model with requires/exports/opens/uses/provides tables and a crate-private payload decoder accepting the existing reader, constant pool and major version. The annotation worker integrates its exports and attribute variant. The coordinator owns fixture generation, end-to-end assertions, fuzz tooling and progress reports. The technical director reviews the code and evidence and owns architecture/ADR documents.

## Annotation contract

Implement visible/invisible declaration annotations, parameter annotations, type annotations, and annotation defaults as distinct attribute variants. Store every element tag, index, array, nested annotation, target discriminator and type-path entry without lossy string conversion. String-valued annotation constants reference UTF-8 constants, not String constants. Class literals accept return descriptors including void.

Validate encoding, complete consumption, reference kinds, descriptors and names. Validate type-target location and numeric ranges using enclosing class/member/Code metadata. Parameter annotation list sizes are not required to equal method descriptor arity. Default-value compatibility with an externally resolved declaration is deferred. Generic target bounds and type paths that require interpretation of Signature metadata belong to the later generic metadata service. Instruction-specific annotation offsets belong to the future decoder/verifier, while range checks are performed now.

Budgets apply across all members, Code and record components in the same class. Exercise empty data, all scalar tags, enum/class literals, nested annotation/array values, all target forms and path kinds, invalid constant tags, invalid target contexts, truncation, extra payload bytes, and exact budget boundaries. Attempting to raise the recursion option beyond the safe ceiling must not create a stack-overflow path.

## Module contract

Decode all declaration tables with typed symbolic references and optional version indices. Name validation follows the binary format's Unicode/escape rules, without imposing Java source identifier grammar. Compare duplicate names semantically, including distinct constant-pool entries spelling the same name. Check the open-module restriction, required java.base relationship with its version-specific flag constraints, and nonempty provider lists. Keep service assignability, provider construction, module graph consistency and accessibility outside the parser.

ModulePackages and ModuleMainClass remain integrated attributes. Audit duplicate package names and consistency with the packages locally described by Module. Do not infer the existence of external classes or packages. Test reserved flags, empty tables, all reference kinds, aliases, invalid names and lengths, qualified/unqualified exports and opens, services, and historical version boundaries.

## Format audit

| Area | Parser obligation | Explicit later work |
| --- | --- | --- |
| Reader and envelope | Bounded reads, exact consumption, structured offsets, resource policy, header and version checks | None for byte-envelope integrity |
| Constant pool | Every encoding/tag, wide holes, modified UTF-8, introduction versions, symbolic reference kinds, applicable names/descriptors and method-handle relationships | Resolution, member existence, bootstrap invocation |
| Class and members | Local access-flag rules, duplicate identities, legal names/descriptors, initializer/version rules, Code presence and parameter-local capacity | Inheritance legality, loader identity, initialization, access and linkage |
| Attributes | All standard bodies decoded, length checks, recognition by version/context, explicit multiplicities and local references | No generic assumption that metadata is executable or verified |
| Code, exceptions, stack maps | Encoding, local ranges, frame/verification tags and offset arithmetic | Instruction boundaries/opcodes, control flow, stack/local type-state and Throwable relationships |
| ConstantValue and BootstrapMethods | Type/reference compatibility and required bootstrap-index relationships | Value installation, dynamic resolution and invocation |
| InnerClasses and EnclosingMethod | Envelope, reference kinds, local names and versioned relationships | External enclosing-class consistency and compiler source-origin facts |
| Signature and LocalVariableTypeTable signatures | Preserve referenced modified-UTF8 text and local table bounds | Generic grammar, erasure, bound resolution and association, with errors at the metadata consumer |
| SourceFile, SourceDebugExtension, line/local tables | Required lengths, UTF-8 references, modified-UTF8 debug encoding, available local ranges and names | Debugger interpretation, instruction-boundary matching |
| Synthetic and Deprecated | Zero-length payloads; no invented singleton restriction | Source/compiler-origin interpretation |
| MethodParameters | Names/references and entry preservation | Descriptor association in reflection |
| Records, nests and permitted subclasses | Typed payloads, component descriptors, explicit mutually exclusive/flag rules and symbolic references | Record reflection, nest access and subclass loading constraints |
| Annotations | Typed grammar, constant kinds, budgets, target location and locally available ranges | Referenced annotation declarations, value compatibility, generic target semantics and instruction-specific target matching |
| Modules | Typed grammar, names, duplicate names and local declared relationships | Module resolution, service classes and loading |

The review must distinguish a demonstrated defect from an untested rule or a later-stage check. Add focused regression cases for defects. Do not introduce restrictions absent from the specification merely because javac does not generate a form. In particular, preservation of unknown attribute data and reserved flag bits is intentional behavior.

## Completion evidence

- A compatibility matrix links implemented structures to positive and negative tests; all 30 recognized standard attribute bodies are typed.
- Genuine independently compiled Java 21 fixtures exercise annotations, type annotations, modules, records, nests, sealed classes, lambdas and exception metadata. Synthetic cases cover the remaining encodings and edge cases.
- Formatting, strict Clippy, and workspace tests pass. Fixture regeneration is reproducible.
- A recorded sustained fuzz campaign exercises the public parser, including new recursive and module structures, with bounded resources and no observed panic. Failures are retained and fixed before the gate.
- The technical director reviews the final implementation and evidence and writes a separate M1 acceptance disposition. Until then, M1 remains open and M2 has not been approved.

The checklist draws its normative requirements from [JVMS SE 21 chapter 4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html). Its staged checks are project architecture decisions and do not establish Java compatibility certification.

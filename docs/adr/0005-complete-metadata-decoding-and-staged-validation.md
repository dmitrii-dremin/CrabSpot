# ADR 0005: Complete metadata decoding with bounded and staged validation

Status: accepted for continued M1 implementation, 2026-11-03. This decision does not itself approve M1.

## Context

The approved foundation retains module and annotation bodies as opaque standard attributes. The user has authorized autonomous continuation. Implementing those bodies introduces recursive metadata and more contextual checks, while some checks need services that do not yet exist.

## Decision

Implement typed models for every remaining standard attribute. Annotation elements, arrays, nested annotations, type targets and type paths have explicit variants. Module declarations preserve all tables and symbolic indices. The parser never loads the referenced types, resolves services, invokes a bootstrap method, or executes bytecode.

One annotation budget is shared by an entire parse. Count both annotation nodes and element-value nodes. The default node budget is 1,000,000 and the default nesting limit is 64. A recursive implementation caps the effective nesting limit at 64 even if the caller requests more; the actual enforced limit appears in its resource error. Users may request smaller limits. This is an implementation resource policy rather than a declaration that deeper metadata is invalid JVM syntax.

Module parsing is a separate module within the classfile crate. Its payload decoder receives the existing bounded reader, constant pool, and class-file version. The attribute layer retains responsibility for exact payload consumption. Name validators are shared with constant-pool checks so unused module/package constants receive the same checks as referenced entries. Preserve reserved flag bits unless an explicit constraint applies.

Complete decoding is separate from complete runtime validation. The M1 audit assigns each check to parsing, later instruction decoding/verification, resolution/access checks, or generic/reflection metadata services. Deferred work requires a concrete owner/stage, not an unexplained exception. Generic signature text remains lossless metadata, not an already-validated generic type representation. Annotation values are checked for representation and reference kinds without resolving the declaration they describe.

## Evidence gate

All 30 standard attribute encodings need tests and no recognized standard body may remain opaque. Add positive, negative, context, boundary and resource tests. Reproduce genuine JDK 21 examples and exercise synthetic forms not naturally emitted by javac.

Run a sustained native parser fuzz campaign for at least five minutes with a recorded seed, corpus, input limits, case count and failure artifacts. Use both structured mutations and arbitrary bytes, including nested annotation and module seeds. This establishes executed fuzz evidence; it does not imply measured coverage or proof of panic freedom. Keep existing deterministic campaigns in ordinary tests. Coverage-guided campaigns may supplement this evidence and must be named accurately.

Linux CI and minimum-Rust-version evidence remain separately reported engineering checks. An unavailable Linux runner is not evidence of Linux support and does not prevent review of the parser's platform-independent format behavior using native tests. The charter's eventual Linux target remains unchanged.

## Consequences

The parser exposes actual metadata without runtime dependencies or unbounded recursive descent. Format, resource, and later semantic failures stay distinguishable. Milestone review can use a finite, reproducible acceptance checklist while preserving the full runtime roadmap.

Normative references: [JVMS annotation attributes](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.7.16), [module attributes](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.7.25), and [format checking](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html#jvms-4.8).

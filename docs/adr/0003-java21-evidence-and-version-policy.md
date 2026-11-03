# ADR 0003: Use Java 21 fixtures and explicit version policy

Status: accepted for M0/M1 implementation.

## Context

The project targets Java SE 21 and Linux x86-64 while the initial development host is Windows. Reference tooling must not become an application execution dependency. A single successful javac fixture is insufficient evidence for a complete parser.

## Decision

Use OpenJDK 21-compatible tooling to compile owned fixture sources, record provenance, and inspect metadata. Keep reference Java tools outside production dependencies. Use synthetic binary fixtures for unusual structures and malformed cases, plus executed fuzz/property campaigns. Linux CI runs the same formatter/lint/test checks as native development; distinguish configured CI from observed CI results.

Support class major versions 45–65 in the parser with the documented historical minor policy. Support Java 21 preview parsing only by explicit option; reject other release previews. Check per-structure version constraints. This inspection policy does not claim runtime support for preview or historical Java semantics.

## Consequences

Fixtures remain reproducible and independent of CrabSpot. Reports can distinguish Windows evidence, Linux evidence, synthetic coverage, and unknown coverage. Source references, commands, versions, seeds and counts make acceptance review repeatable. Parsing standard-library metadata alone never establishes standard-library execution compatibility.

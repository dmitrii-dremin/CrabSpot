# ADR 0002: Preserve class-file data behind bounded parsing

Status: accepted for M1 implementation.

## Context

External class files are untrusted. Rust strings, unchecked length-derived allocations, and recursive descent without limits can lose valid Java data or produce resource failures. A format parser is not a verifier.

## Decision

Use bounded readers with absolute offsets, exact attribute/file consumption, checked references, typed errors, and configurable resource/depth limits. Keep Java string code units losslessly, retain numeric bit patterns and unknown attributes, and model unusable constant-pool slots explicitly. Do not decode modified UTF-8 using ordinary UTF-8 or make lossy display text the stored value. Descriptor parsing has its own reusable interface.

Enforce local format rules for implemented constructs. Attribute recognition is version- and location-sensitive: preserve a standard-looking name outside its defined context as opaque extension data. Report unsupported, malformed, and resource-limited inputs distinctly. Track opaque recognized standard attributes as partial support; they do not satisfy the M1 gate. Bytecode verification, class resolution, and execution are later stages with separate states.

## Consequences

Diagnostics can identify the offending structure without panicking. The model can represent Java strings that cannot be represented by a Rust `String`. Limits are visible policy choices, not claims that large valid files violate the format. A successful parse is never described as a verified executable class.

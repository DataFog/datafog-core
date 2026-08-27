# PII Protection Engine Roadmap

## Product framing

DataFog Core is a new PII protection engine. It detects sensitive information,
applies policy-driven privacy transformations, and enforces protection
decisions. It preserves useful capabilities from DataFog Python without
inheriting its API structure, legacy aliases, or weak security semantics.

The target model is:

```text
scan(text)       -> findings
transform(...)   -> transformed text and transformation records
protect(...)     -> allow, transform, warn, or block
restore(...)     -> authorized restoration of reversible tokens
```

## Slice 0: Core contract

**Status: complete**

The accepted contract is recorded in
[ADR 001](adr/001-privacy-core-contract.md). The disposition of existing and
proposed capabilities is recorded in the
[privacy capability matrix](privacy-capability-matrix.md).

The decisions cover:

- the new-engine framing and compatibility stance;
- operation and strategy names;
- explicit offset coordinate systems;
- the finding shape;
- strict validation of supplied findings;
- deterministic duplicate and overlap handling;
- transformation and protection result shapes;
- block semantics; and
- the security meaning of hash, pseudonymize, and tokenize.

## Slice 1: Finding and scan contract

- Introduce the accepted `Finding` representation.
- Preserve UTF-8 byte spans internally.
- Return byte and Unicode code-point ranges publicly.
- Add optional confidence and detector provenance.
- Update serialization and fixtures without changing detector coverage.

**Proof:** existing detection fixtures remain valid; Unicode and binding
round-trip fixtures prove every public range selects the reported matched text.

## Slice 2: Transformation framework and redaction

- Validate caller-supplied findings strictly.
- Deduplicate and resolve overlaps once in shared code.
- Apply replacements without invalidating subsequent source ranges.
- Implement typed, document-local redaction placeholders.
- Return ordered transformation records and output ranges.

**Proof:** normal, repeated, duplicate, overlapping, nested, empty, malformed,
and Unicode cases satisfy ADR 001.

## Slice 3: Mask and remove

- Define full and partial masking direction and length semantics.
- Support a validated masking character.
- Add literal removal as a small transformation if a concrete use case remains.

**Proof:** transformation records identify the exact input and output spans for
full, partial, and zero-length replacements.

## Slice 4: Policy selection

- Add entity-type selection.
- Add exact, case-sensitive allowlists.
- Add full-match regex allowlists with bounded behavior.
- Add locale selection and per-entity strategy overrides.
- Define policy validation and precedence.

**Proof:** the same policy retains the same findings whether it scans internally
or receives precomputed findings.

## Slice 5: Protection enforcement

- Implement typed `allow`, `transform`, `warn`, and `block` outcomes.
- Ensure blocking results omit original text and matched values by default.
- Provide safe finding summaries and policy-rule attribution.
- Keep calling-application enforcement explicit.

**Proof:** a blocking finding prevents transformed or original content from
appearing in the ordinary result; clean input passes unchanged.

## Slice 6: Compatibility hash

- Select and document the compatibility digest and encoding.
- Specify determinism, truncation, and collision behavior.
- Document equality leakage and low-entropy guessing risk.
- Keep hash distinct from secure pseudonymization.

**Proof:** fixed vectors are stable and the API never describes the output as
anonymous or securely tokenized.

## Slice 7: One-way pseudonymization

- Define the key-provider boundary.
- Define required tenant, dataset, purpose, and policy-version scope.
- Use a reviewed keyed construction with domain separation.
- Define token encoding and key rotation behavior.
- Suppress sensitive source mappings by default.

**Proof:** identical values are stable within the same entity type and scope;
changing any scope component, entity type, or key version changes the output.

## Slice 8: Reversible tokenization and restoration

- Define opaque token format and authorization context.
- Introduce a vault or reviewed reversible cryptographic boundary.
- Implement token creation and restoration as one end-to-end slice.
- Reject unknown, expired, wrong-scope, and unauthorized tokens.

**Proof:** authorized round trips succeed and every unauthorized variant fails
closed without revealing the original value.

## Slice 9: Binding rollout

After the Rust contract stabilizes:

1. expose the API through the Python binding;
2. expose it through Node with explicit UTF-16 coordinate support; and
3. expose stateless operations through WASM.

Reversible token storage is not promised in WASM without a separate key-custody
and storage design.

## Acceptance bar

Every completed slice must have:

1. documented behavior;
2. normal, empty, duplicate, overlapping, malformed, and Unicode tests where
   applicable;
3. security tests appropriate to the claimed property;
4. capability-continuity fixtures where Python demonstrates an important user
   workflow; and
5. explicit documentation of intentional compatibility differences.

## Explicitly out of scope

- OCR and image extraction;
- spaCy, GLiNER, or other detector implementations;
- legacy Python APIs and return shapes unless separately approved;
- claims of complete anonymization;
- identity resolution across different identifiers; and
- dataset-level techniques such as k-anonymity, generalization, synthetic data,
  and differential privacy.

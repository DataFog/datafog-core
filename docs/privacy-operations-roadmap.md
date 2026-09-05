# PII Privacy Core Roadmap

## Product framing

DataFog Core detects sensitive information, applies privacy transformations,
and restores explicitly reversible tokens. Governance decisions and payload
enforcement belong to a separate layer.

The target model is:

```text
scan(text)       -> findings
transform(...)   -> transformed text and transformation records
scan_and_transform(...) -> explicit scan-then-transform convenience
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
- the transformation result shape; and
- the security meaning and disposition of hash, pseudonymize, and tokenize.

## Slice 1: Finding and scan contract

**Status: complete**

- Introduce the accepted `Finding` representation.
- Preserve UTF-8 byte spans internally.
- Return byte and Unicode code-point ranges publicly.
- Add optional confidence and detector provenance.
- Update serialization and fixtures without changing detector coverage.

**Proof:** existing detection fixtures remain valid; Unicode and binding
round-trip fixtures prove every public range selects the reported matched text.

## Slice 2: Transformation framework and redaction

**Status: complete**

- Require explicit findings in `transform`; never scan implicitly.
- Provide `scan_and_transform` as the explicit convenience operation.
- Validate caller-supplied findings strictly.
- Deduplicate and resolve overlaps once in shared code.
- Apply replacements without invalidating subsequent source ranges.
- Implement unnumbered `[ENTITY_TYPE]` redaction placeholders.
- Return ordered transformation records with explicit output byte and
  code-point ranges.
- Keep entity types equal by default and use deterministic structural,
  confidence, position, and lexical tie-breaking.

**Proof:** normal, repeated, duplicate, overlapping, nested, empty, malformed,
and Unicode cases satisfy ADR 001.

## Slice 3: Mask and remove

**Status: complete**

- Use a discriminated strategy configuration across public bindings.
- Mask every non-revealed Unicode code point, including punctuation.
- Support `first` and `last` reveal modes with a non-negative code-point count.
- Default to `*` and accept exactly one non-whitespace, non-control Unicode
  code point as a custom masking character.
- Add parameterless exact removal with no implicit whitespace normalization.

**Proof:** transformation records identify the exact input and output spans for
full, partial, multibyte-mask-character, unchanged, and zero-length
replacements. Invalid strategy fields and masking characters are rejected.

## Slice 4: Transformation selection

**Status: complete**

- Replace the temporary single-strategy request with one canonical
  transformation-configuration envelope. Do not retain the old shape as a
  shorthand or add a second transformation operation.
- Add an explicit default strategy with exact, case-sensitive per-entity
  overrides.
- Treat an omitted entity selection as all supplied findings, a non-empty
  selection as an exact case-sensitive filter, and an empty selection as an
  error.
- Add entity-scoped exact, case-sensitive allowlists.
- Add entity-scoped, full-match regex allowlists with case-sensitive matching
  by default. Limit each configuration to 100 deduplicated rules, 1 KiB per
  pattern, 10 KiB aggregate source, and 1 MiB per compiled pattern group;
  reject invalid or over-limit configurations atomically.
- Validate every supplied configuration entry, including entries for entity
  types not selected by the current call. Valid but unselected overrides and
  allowlists remain dormant rather than making the configuration invalid.
- Accept empty structural objects and maps as omission, while rejecting empty
  semantic values, explicit `null`, and unknown fields at every nesting level.
- Apply configuration in this order: validate, select entities, apply
  allowlists, resolve overlaps, choose the entity override or default strategy,
  and transform in document order.
- Keep locale in scanning configuration because it affects detection, not the
  transformation of supplied findings.
- Give `scan_and_transform` one divided request envelope with an optional
  `scan` configuration and required `transform` configuration. Reuse those
  configuration types in the standalone operations.
- Retain equal entity-type priority during overlap resolution; configurable
  entity priorities are deferred beyond Slice 4.
- Standardize atomic structured errors across bindings using stable
  `invalid_configuration`, `invalid_finding`, and `internal_error` codes,
  machine-readable reasons, and RFC 6901 request paths. Never include sensitive
  input values in errors.

**Proof:** selection, exact and regex exemptions, overlap interactions,
override fallback, dormant rules, malformed configuration, and Unicode cases
produce the same result whether findings come directly from `scan` or are
supplied to `transform`.

## Slice 5: Exclude unkeyed hash

**Status: complete**

- Do not expose unkeyed hashing as a canonical privacy transformation.
- Treat brute-force guessing and deterministic linkage as disqualifying for
  predictable PII, regardless of digest length or encoding.
- Do not add salt configuration as a compromise: public salts remain guessable,
  per-value random salts remove stable equality, and secret salts belong to the
  keyed pseudonymization design.
- Permit a non-Core compatibility fingerprint only when a concrete migration
  requirement is separately accepted and documented.

**Proof:** the canonical strategy set contains no unkeyed hash operation and
documentation directs deterministic one-way privacy requirements to scoped,
keyed pseudonymization.

## Slice 6: One-way pseudonymization

**Status: complete**

- Add `pseudonymize` with required `key_ref` and optional `key_version`.
- Use fixed HMAC-SHA-256 over the exact UTF-8 matched value and encode the full
  digest as 44-character standard padded Base64.
- Let the key define linkage scope. Do not add tenant, dataset, purpose,
  entity-type, domain-separation, normalization, or algorithm-selection fields.
- Resolve provider keys asynchronously before entering the synchronous
  transformation kernel. Require exactly 32 random bytes and a concrete
  resolved version.
- Resolve each distinct selector used by selected findings once, freeze all
  versions for the request, and fail atomically if any resolution fails.
- Keep key material out of serialized configuration, logs, debug output,
  errors, and results; retain it for one call only and best-effort zeroize it.
- Permit provider-owned caching and retries without adding either to Core.
- Remove `matched_text` from every transformation record. Preserve source
  ranges and detector metadata, and report `key_ref` plus concrete key version
  only for pseudonymization records.
- Ship the provider-backed manager through Rust, Python, and Node. Defer
  browser WASM pseudonymization and cloud-specific provider adapters.

**Proof:** exact input and the same key produce the same full HMAC token across
Rust, Python, and Node; changing exact input or resolved key material changes
the token; different entity types do not alter it; multiple selectors resolve
once and apply atomically; provider failures, invalid key material, and
providerless or browser-WASM calls fail closed; no transformation record echoes
the original PII.

## Slice 7: Reversible tokenization and restoration

**Status: complete**

- Add `{ strategy: "tokenize", token_ref }` without algorithm, TTL,
  determinism, storage, or caller-pinned version settings.
- Require exact, case-sensitive, non-empty request-level `scope` only for
  selected tokenization and every restoration request.
- Keep the provider boundary asynchronous and batched. Tokenization preserves
  repeated values as separate items; restoration deduplicates identical
  envelopes before the provider call.
- Use the canonical `DFTOKENv1(<body-length>):<ref>.<version>.<payload>`
  envelope with unpadded Base64URL components and strict checked parsing.
- Treat token payloads as opaque. Providers own storage or reversible crypto,
  authentication, authorization, scope/profile binding, lifecycle, retries,
  idempotency, cleanup, and audit logging.
- Resolve every pseudonymization key before stateful token creation, validate
  every provider response before mutation, and make Core output atomic.
- Restore every canonical token in the supplied text or return no result. Do
  not add filtering, partial restoration, ignore-failure, recursive restore,
  or nested tokenization modes.
- Return restoration source/output byte and code-point ranges plus token
  reference and concrete profile version, without returning plaintext
  mappings, scope, credentials, or opaque payloads.
- Ship full Rust, Python, and Node support. Browser WASM parses the same
  configuration and envelope but rejects selected tokenization and every
  restoration call with `unsupported_strategy`.
- Ship no built-in cloud, database, vault, or cryptographic provider.

**Proof:** authorized Unicode and repeated-value round trips preserve exact
ranges while producing independently issued tokens; unauthorized and malformed
variants fail closed without partial Core output or plaintext leakage; nested
tokenization and recursive restoration are absent; Rust, Python, and Node agree
while browser WASM rejects provider-backed work.

## Slice 8: Binding completion and release hardening

**Status: complete.**

Rust, Python, and Node implement all capabilities through Slice 7. Browser
WASM implements the stateless transformations through Slice 4 and explicitly
rejects provider-backed pseudonymization. New stateless operations should
continue to ship through the bindings in the same vertical slice as their Rust
implementation rather than waiting for a separate binding rollout.

Node.js and browser WASM findings expose `utf16Range`; their transformation
records expose `sourceUtf16Range` and `outputUtf16Range`; Node restoration
records expose the same source/output names. All are zero-based, end-exclusive
UTF-16 code-unit ranges. One validated Core helper derives them from canonical
byte ranges, while existing byte and code-point fields remain unchanged.

Installed-package tests prove emoji-prefixed findings and transformation or
restoration records select the exact spans with JavaScript `slice`. Existing
Rust, Python, and Node provider-backed conformance coverage remains in place,
and browser WASM continues to reject key- and token-provider work explicitly.

Pseudonymization and reversible token storage are not promised in browser WASM
without a separately accepted host-managed key-custody boundary.

## Next workstream: PERSON detection with automatic field discovery

**Status: Core and bindings implemented; release and downstream adoption pending.**

The [PERSON detection plan](person-detection-plan.md) extends Core with
conservative automatic name-field discovery for structured JSON data, explicit
mapping overrides, and path-qualified findings. It includes shared tests,
transformation integration, performance measurement, and documentation. Core
and its bindings ship together before downstream MCP adoption.

This workstream adds schema-guided PERSON coverage. It does not change the
completed privacy-operation slices or promise general prose name recognition.

## Slice 10: Finding-selection performance

**Status: implemented; review and combined release pending.**

Replace linear duplicate searches with indexed groups and repeated overlap
scans with ordered interval selection. Preserve validation, filtering,
confidence/provenance preferences, and source order for all labels. Retain a
compatibility fallback where mixed confidence prevents safe sorting.

The [selection measurements](finding-selection-performance.md) document exact
behavior comparisons, reproducible scaling benchmarks, and remaining performance
limits. Prepare one 0.3.0 release containing structured PERSON and this change
after review; individual feature PRs do not bump or publish package versions.

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
- governance decisions and enforcement actions such as `allow`, `warn`, and
  `block`;
- claims of complete anonymization;
- identity resolution across different identifiers; and
- dataset-level techniques such as k-anonymity, generalization, synthetic data,
  and differential privacy.

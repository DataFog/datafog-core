# ADR 001: Privacy Core Contract

- **Status:** Accepted
- **Date:** 2026-08-27

## Context

DataFog Core is a PII detection and transformation engine.

The engine is organized around three operations:

```text
scan(text)       -> findings
transform(...)   -> transformed text and transformation records
scan_and_transform(...) -> explicit scan-then-transform convenience
restore(...)     -> authorized restoration of reversible tokens
```

## Decision

### Operation taxonomy

Canonical core operations are `scan`, `transform`, and `restore`.
`transform` always requires caller-supplied findings and never scans
implicitly. `scan_and_transform` is the explicitly named convenience operation
for callers that want Core to perform both steps.

Canonical transformation strategies are:

```text
remove
redact
mask
hash
pseudonymize
tokenize
```

Strategies use a discriminated configuration. Rust represents the same shape
with typed enum variants; object-oriented bindings serialize it as:

```text
{ strategy: "redact" }
{ strategy: "remove" }
{ strategy: "mask", character: "*" }
{
  strategy: "mask",
  character: "*",
  reveal: { direction: "first" | "last", count: non_negative_integer }
}
```

Fields that do not belong to the selected strategy are rejected rather than
silently ignored.

`allow`, `warn`, and `block` are governance decisions, not Core operations or
transformation strategies. A separate governance layer may consume findings
and transformation results to make those decisions. The calling application,
proxy, or firewall is responsible for enforcing them.

### Text ranges

All ranges are zero-based and end-exclusive.

- The Rust implementation uses UTF-8 byte ranges internally.
- The canonical public default is Unicode code-point ranges.
- Every public range states its coordinate unit explicitly.
- UTF-16 code-unit ranges are supported for JavaScript consumers.
- Findings expose both UTF-8 byte and Unicode code-point ranges. Bindings may
  derive additional native ranges without silently changing the meaning of a
  field.
- Ranges always refer to the exact original input. The engine performs no
  implicit Unicode normalization.

### Finding contract

A finding contains:

```text
Finding {
  entity_type
  matched_text
  byte_range
  codepoint_range
  confidence?
  detector_name
  detector_version?
}
```

Built-in entity types use canonical uppercase names. The public entity-type
field remains extensible rather than being restricted to a closed enum.
Confidence, when present, is in the inclusive range `0.0..=1.0`.

### Supplied-finding validation

Transformations reject the entire request when any externally supplied finding
is malformed. Invalid findings are not silently ignored. Invalid conditions
include:

- an empty, reversed, or out-of-bounds range;
- inconsistent byte and code-point ranges;
- a range that does not fall on a valid boundary;
- `matched_text` that differs from the referenced source substring; or
- confidence outside `0.0..=1.0`.

Findings produced by the core scanner must satisfy these invariants by
construction.

### Duplicates and overlaps

`scan` may report meaningful overlaps. `transform` resolves them before making
changes.

Exact duplicates have the same entity type, source range, and matched text and
collapse into one finding. If both duplicate detector results have confidence,
the result with higher confidence is retained; remaining ties use stable
detector provenance ordering.

Overlapping findings are ranked by:

1. a containing span over a span contained within it;
2. longer Unicode code-point span;
3. higher confidence when both findings provide confidence;
4. earlier source position;
5. lexical entity-type ordering; and
6. lexical detector-name and detector-version ordering.

The selected non-overlapping findings are returned in document order. Entity
types have equal priority by default. A later transformation configuration may
provide explicit domain-specific priorities.

### Transformation result

The canonical result follows the transformed-text-plus-entity-record pattern
used by established PII tools:

```text
TransformResult {
  text
  transformations: [Transformation]
}

Transformation {
  finding
  strategy
  replacement
  output_byte_range
  output_codepoint_range
}
```

Transformation records are ordered by source document position and include
only transformations that were actually applied. Output ranges are zero-based,
end-exclusive, refer to the transformed text, and select exactly `replacement`.
The finding already supplies the original value and source ranges, so the
canonical payload does not contain a second original-to-replacement mapping.

A mapping view may be offered as an explicit convenience API. Sensitive
original values are excluded from default debug and log output.

### Security meaning of strategies

- `redact` replaces every selected finding with the unnumbered placeholder
  `[ENTITY_TYPE]`. Repeated occurrences intentionally receive the same
  type-only placeholder; this makes no identity or equality claim.
- `mask` replaces every non-revealed Unicode code point, including punctuation,
  with one masking code point. The default masking character is `*`. A custom
  masking character must be exactly one non-whitespace, non-control Unicode
  code point. Omitting `reveal`, or setting its count to zero, masks the entire
  finding. `first` preserves the requested number of leading code points;
  `last` preserves the requested number of trailing code points. A reveal count
  equal to or greater than the finding length preserves the whole finding
  without error. Output byte ranges still reflect the encoded byte length of
  the chosen masking character.
- `remove` replaces the exact finding span with the empty string. It accepts no
  configuration, does not consume surrounding whitespace, and does not
  normalize the remaining text. Its transformation record uses an empty output
  range at the deletion position.
- `hash` is a compatibility fingerprint with explicitly documented leakage and
  must not be presented as secure pseudonymization.
- `pseudonymize` is a new keyed, scoped, deterministic, one-way operation. It
  is not based on the Python numbered-placeholder behavior.
- `tokenize` creates opaque reversible or vault-backed tokens.
- `restore` accepts only explicitly reversible tokens and requires
  authorization.

Pseudonymization is value pseudonymization, not identity resolution. The core
does not infer that different identifiers belong to the same person.

## Capability-continuity stance

Python behavior is classified as preserved, redesigned, compatibility-only, or
dropped. Compatibility means preserving valuable user capabilities, not
reproducing every Python output. Exact-output compatibility must be justified
and documented separately.

## Consequences

- Bindings cannot treat unlabeled `start` and `end` fields as universally
  portable.
- Strict finding validation may intentionally differ from permissive legacy
  behavior.
- Consumers receive auditable transformation records without a redundant
  mapping dictionary.
- Secure pseudonymization and tokenization require explicit key and scope
  contracts in later ADRs.
- Pseudonymized data remains sensitive and must not be described as anonymous.
- Governance decisions and payload enforcement remain outside DataFog Core.

## Deferred decisions

This ADR does not choose:

- compatibility hash format;
- HMAC token encoding, key provider, scope fields, or rotation procedure;
- reversible-token storage or cryptographic construction;
- production audit and mapping storage.
- custom literal replacement or whitespace-normalizing removal.

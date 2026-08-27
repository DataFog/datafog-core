# ADR 001: Privacy Core Contract

- **Status:** Accepted
- **Date:** 2026-08-27

## Context

DataFog Core is a new PII protection engine, not a Rust port of the DataFog
Python API. The Python library is evidence of useful capabilities and existing
user expectations, but it is not the design authority. The core should preserve
valuable operations without inheriting ambiguous names, unsafe defaults, or
legacy return shapes.

The engine is organized around four operations:

```text
scan(text)       -> findings
transform(...)   -> transformed text and transformation records
protect(...)     -> allow, transform, warn, or block
restore(...)     -> authorized restoration of reversible tokens
```

## Decision

### Operation taxonomy

Canonical core operations are `scan`, `transform`, `protect`, and `restore`.

Canonical transformation strategies are:

```text
remove
redact
mask
hash
pseudonymize
tokenize
```

Canonical protection actions are:

```text
allow
transform
warn
block
```

`block` and `warn` are enforcement decisions, not transformations. Blocking
rejects the entire payload; it does not delete only the detected span. Legacy
Python names and return shapes may be handled by separate compatibility
wrappers, but they do not shape the core API.

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
collapse into one finding. If duplicate detector results differ, the result
with higher confidence is retained; remaining ties use stable detector-name
ordering.

Overlapping findings are ranked by:

1. longer source span;
2. higher entity-type priority;
3. higher confidence;
4. earlier source position; and
5. stable lexical ordering.

The selected non-overlapping findings are returned in document order. Entity
priority has a core default and may later be made policy-configurable.

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
  output_range
}
```

Transformation records are ordered by source document position and include
only transformations that were actually applied. The finding already supplies
the original value and source range, so the canonical payload does not contain
a second original-to-replacement mapping.

A mapping view may be offered as an explicit convenience API. Sensitive
original values are excluded from default debug and log output. Protection
responses do not expose matched PII unless sensitive diagnostics are explicitly
requested.

### Protection result

Protection produces a typed outcome:

```text
Allowed
Transformed
Warned
Blocked
```

A blocked result does not return the original text or matched values by
default. It may return safe diagnostics such as the policy rule, entity types,
and finding count. The calling application is responsible for honoring a block
by stopping the prompt, response, message, or other payload.

When a future policy assigns different actions to different findings, the
decision precedence is:

```text
block > warn > transform > allow
```

Any blocking finding blocks the entire payload.

### Security meaning of strategies

- `redact` creates typed, document-local placeholders.
- `mask` hides all or a configured part of a value.
- `hash` is a compatibility fingerprint with explicitly documented leakage and
  must not be presented as secure pseudonymization.
- `pseudonymize` is a new keyed, scoped, deterministic, one-way operation. It
  is not based on the Python numbered-placeholder behavior.
- `tokenize` creates opaque reversible or vault-backed tokens.
- `restore` accepts only explicitly reversible tokens and requires
  authorization.

Pseudonymization is value pseudonymization, not identity resolution. The core
does not infer that different identifiers belong to the same person.

## Capability-continuity policy

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

## Deferred decisions

This ADR does not choose:

- mask length and direction options;
- compatibility hash format;
- HMAC token encoding, key provider, scope fields, or rotation procedure;
- reversible-token storage or cryptographic construction;
- streaming enforcement behavior; or
- production audit and mapping storage.

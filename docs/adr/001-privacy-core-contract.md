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
{
  strategy: "pseudonymize",
  key_ref: "provider-specific-key-reference",
  key_version?: "provider-specific-version-or-alias"
}
```

Fields that do not belong to the selected strategy are rejected rather than
silently ignored.

### Transformation selection

Transformation configuration has one required default strategy and may add an
entity selection, per-entity strategy overrides, and entity-scoped exact or
regex allowlists. Entity names are matched exactly and case-sensitively. The
public serialized shape is:

```text
{
  default: { strategy: "redact" },
  entities: ["EMAIL", "PHONE"],
  overrides: {
    PHONE: {
      strategy: "mask",
      reveal: { direction: "last", count: 4 }
    }
  },
  allow: {
    exact: { EMAIL: ["support@example.com"] },
    regex: {
      EMAIL: [
        { pattern: ".*@example\\.com", case_sensitive: true }
      ]
    }
  }
}
```

This envelope replaces the temporary single-strategy input introduced while
the transformation framework was built. The old `{ strategy: ... }` shape is
not retained as a shorthand or exposed through a second operation. A caller
that needs only one strategy supplies it as `default`. Rust accepts the typed
transformation configuration, and Python, Node, and WASM accept the same
serialized envelope. Transformation records continue to report the strategy
actually applied to each finding rather than copying the request envelope.

Omitting `entities` selects all supplied findings. A non-empty list selects
only those entity types. An explicitly empty list is rejected, as are duplicate
entity names. Entity types remain extensible and are not limited to built-in
detectors.

Exact allowlists compare against the complete `matched_text` and are
case-sensitive. Regex allowlists use full-match semantics and are
case-sensitive unless explicitly configured otherwise. Patterns are compiled
once per configuration using a non-backtracking regex engine and are subject to
the following limits:

- at most 100 regex rules per transformation configuration after
  deduplication;
- at most 1 KiB of UTF-8 source per pattern;
- at most 10 KiB of aggregate UTF-8 regex source; and
- at most 1 MiB of compiled representation per compiled pattern group.

Invalid patterns and limit violations reject the complete configuration. No
pattern is silently dropped and transformation never proceeds with a partial
allowlist. A runtime timeout is unnecessary because the selected regex engine
does not use backtracking. Valid broad expressions such as `.*` are accepted as
intentional caller policy rather than rejected based on inferred intent. Empty
per-entity allowlists are rejected; repeated allow values and identical regex
rules are deduplicated.

Every supplied configuration entry is validated even when its entity type is
not selected by the current invocation. Valid overrides and allowlists for
unselected entity types are dormant: they have no effect, but do not make the
configuration invalid. This permits one reusable privacy profile to define
behavior for all relevant entity types while individual calls select a subset.
Configuration is never validated against only the findings present in one
document.

Empty optional structural objects are accepted as equivalent to omission. This
includes empty `overrides` and `allow` objects, empty `exact` and `regex` maps,
and an empty `scan` object. `TransformationConfig` itself is not optional and
still requires `default`. This supports configuration builders and serializers
without giving an empty wrapper object a hidden policy meaning.

Semantic empty values are rejected. These include an empty `entities` list,
empty per-entity allowlist arrays, empty or whitespace-only entity names, empty
exact allowlist values, and empty regex pattern source. Explicit `null` is not
an alias for omission and is rejected for every optional field. Unknown fields
are rejected recursively at every configuration level so misspelled or stale
policy fields cannot be silently ignored.

Processing order is:

1. validate all supplied findings and the complete configuration;
2. filter findings using `entities`;
3. exempt exact and regex allowlist matches;
4. resolve duplicates and overlaps among the remaining findings;
5. choose an entity-specific override when present, otherwise `default`; and
6. apply transformations in source document order.

Allowlisted findings remain unchanged and produce no transformation record.
Findings excluded by entity selection likewise remain unchanged and produce no
record. Locale is scanning configuration because it affects detection; it is
not part of transformation selection for caller-supplied findings.

Configurable entity-type priority for overlap resolution is deferred. Slice 4
retains the equal-priority deterministic ranking defined in this ADR.

### Scan-and-transform configuration

Scanning and transformation use separate reusable configuration types because
they control different layers. `ScanConfig` controls detection concerns such as
locale and future detector settings. `TransformationConfig` controls selection,
exemptions, and replacement of supplied findings.

`scan_and_transform` accepts one explicitly divided envelope:

```text
{
  scan: {
    locale: "en-US"
  },
  transform: {
    default: { strategy: "redact" },
    entities: ["EMAIL"]
  }
}
```

The `transform` member is required. The `scan` member may be omitted to use
scanner defaults. Unknown scanning fields are rejected. Locale and other
detection settings never appear in `TransformationConfig` and cannot affect
the transformation of caller-supplied findings.

The same transformation configuration can therefore be passed directly to
`transform` or nested under `transform` in `scan_and_transform`. Entity
selection remains a transformation rule. `scan_and_transform` may avoid
unnecessary detector work when doing so is provably equivalent, but such an
optimization must not change the findings retained or the transformation
result.

### Error contract

Top-level operations expose one structured error contract across Rust, Python,
Node, and WASM. The stable top-level error codes are:

```text
invalid_configuration
invalid_finding
internal_error
key_provider_required
key_not_found
key_access_denied
key_provider_unavailable
invalid_key_material
key_provider_error
unsupported_strategy
```

Caller-correctable errors also include a stable machine-readable `reason` and
an RFC 6901 JSON Pointer `path` identifying the invalid request location.
Finding errors additionally include `finding_index`. Initial reason values
include:

```text
missing_field
unknown_field
invalid_type
invalid_value
empty_value
duplicate_value
invalid_regex
limit_exceeded
matched_text_mismatch
inconsistent_ranges
out_of_bounds
invalid_boundary
invalid_confidence
```

Codes, reasons, and paths are public API. Human-readable messages are intended
for diagnostics and are not stable or suitable for programmatic parsing. Error
details never contain source text, matched PII, or other sensitive input
values. Validation is atomic and no partial transformation result accompanies
an error.

Rust represents the contract with a typed `PrivacyError`. Python exposes
configuration and finding errors as `ValueError` subclasses and internal
failures as a `RuntimeError` subclass. Node and WASM expose JavaScript
`DataFogError` objects with `code`, `reason`, `path`, and optional
`findingIndex`; WASM never rejects with a bare string. Each binding may use its
native exception hierarchy, but the canonical fields retain the same meaning.
Additional reason values may be introduced as validation expands without
creating new top-level categories for every validation case.

Key-provider errors never contain key bytes, source text, credentials, or the
key reference. Their path identifies the pseudonymization selector that could
not be resolved. `key_provider_required` means a synchronous providerless call
selected pseudonymization. `key_provider_unavailable` indicates that retrying
the entire transformation may succeed; no other provider error is
automatically retryable. `unsupported_strategy` identifies a strategy that a
binding intentionally cannot execute, including pseudonymization in browser
WASM. Core does not add retries or backoff around a provider. Provider
implementations own network timeouts and any SDK-level retry policy.

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
  entity_type
  source_byte_range
  source_codepoint_range
  confidence?
  detector_name
  detector_version?
  strategy
  replacement
  output_byte_range
  output_codepoint_range
  key_ref?                 // pseudonymize only
  resolved_key_version?    // pseudonymize only
}
```

Transformation records are ordered by source document position and include
only transformations that were actually applied. Output ranges are zero-based,
end-exclusive, refer to the transformed text, and select exactly `replacement`.
Source metadata deliberately excludes `matched_text`; Core does not echo the
original PII or offer an include-originals switch. Callers that explicitly need
the source value already possess the input and can use the source ranges.

Pseudonymization records include the configured key reference and the concrete
version returned by the provider. They never include key material or a
plaintext-to-token mapping. Non-pseudonymization records omit both key fields.

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
- Unkeyed `hash` is not a canonical Core transformation. Predictable PII can
  be tested through brute-force or dictionary attacks, while deterministic
  output exposes equality and cross-dataset linkage. Salting does not provide
  a useful middle ground: a public or shared salt remains guessable, a random
  per-value salt removes deterministic matching, and a secret salt is a keyed
  pseudonymization design. A separately scoped compatibility adapter may offer
  a plainly named fingerprint only when a concrete migration requirement
  justifies it.
- `pseudonymize` is a new keyed, deterministic, one-way operation. It is not
  based on the Python numbered-placeholder behavior. It computes HMAC-SHA-256
  over the exact UTF-8 bytes of `matched_text` and encodes the complete 32-byte
  digest as standard padded Base64. The result is always 44 characters. Core
  performs no trimming, case folding, Unicode normalization, semantic
  canonicalization, domain separation, digest truncation, or algorithm
  negotiation.
- `tokenize` creates opaque reversible or vault-backed tokens.
- `restore` accepts only explicitly reversible tokens and requires
  authorization.

Pseudonymization is value pseudonymization, not identity resolution. The core
does not infer that different identifiers belong to the same person.

### Pseudonymization key contract

The key defines the linkage scope. The same exact value and key material
produce the same pseudonym, including across different entity types. Separate
tenants, datasets, or purposes use separate keys or key versions; those
concepts are not additional Core configuration fields.

Serializable pseudonymization configuration contains a required `key_ref` and
an optional `key_version`. A runtime `KeyProvider` receives those identifiers
and returns exactly 32 cryptographically random key bytes plus a non-empty
concrete resolved version. Core rejects every other key length and never pads,
hashes, or derives arbitrary input into a key. Base64 decoding, secret-store
formats, password-based derivation, and provider authentication remain outside
Core.

Key resolution is separated from the synchronous transformation kernel. An
asynchronous `PrivacyManager` owns the provider, resolves every distinct key
selector required by the selected findings exactly once, and freezes all
resolved versions for the request. It resolves no keys for findings removed by
entity selection, allowlists, duplicate handling, or overlap resolution. All
keys must resolve and validate before text mutation begins; one failure returns
no text or transformation records. An explicitly requested version never
falls back to another version. Omitting a version permits provider-defined
latest-version behavior, so a later whole-operation retry may observe a
rotation and produce different pseudonyms.

Default and entity-specific strategies may refer to different keys. Distinct
selectors are deduplicated within one request, and each applied record reports
the key reference and concrete resolved version it used.

Core holds resolved material in a non-clonable, non-serializable,
redacted-debug container for one call, never caches it across calls, and
best-effort zeroizes it on every exit path. A provider may implement explicit
caching but owns its TTL and rotation behavior. Core never logs key material or
places it in an error.

Rust, Python, and Node expose the provider-backed manager. The existing
synchronous operations remain available for `remove`, `redact`, and `mask`;
attempting to apply `pseudonymize` without a manager fails with a structured
provider-required error. Browser WASM pseudonymization is deferred because the
chosen provider model would place raw key bytes in browser-accessible linear
memory. Core ships no cloud-vendor SDK adapter in this slice; AWS, Google, and
other integrations belong in separate packages behind the same provider
contract.

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
- Consumers receive auditable transformation records without echoing original
  PII or returning a mapping dictionary.
- Secure pseudonymization requires an explicit key-provider contract;
  reversible tokenization requires a separately accepted key, authorization,
  and storage boundary.
- Pseudonymized data remains sensitive and must not be described as anonymous.
- Governance decisions and payload enforcement remain outside DataFog Core.

## Deferred decisions

This ADR does not choose:

- reversible-token storage or cryptographic construction;
- production audit storage; or
- custom literal replacement or whitespace-normalizing removal.

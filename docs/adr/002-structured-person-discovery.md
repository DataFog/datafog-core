# ADR 002: Structured PERSON discovery and protection

- **Status:** Implemented; pending release
- **Date:** 2026-09-04
- **Extends:** [ADR 001](001-privacy-core-contract.md)

## Decision

Add structured JSON operations in `datafog_core::structured`. Keep the existing
text operations unchanged. All detection, traversal, mapping rules, selection,
and transformation semantics live in Rust. Bindings convert JSON inputs and
finding/record representations and coordinate application providers.

The input is a parsed JSON object or array. Scan string values; preserve numeric,
boolean, and null values. Do not scan object keys or coerce non-string values.
Binding transport uses JSON serialization with strict conversion checks: reject
cycles, non-JSON objects, non-string object keys, non-finite numbers, and integers
outside JavaScript's safe range, ±(2^53−1). This is a shared portability limit,
not a detector threshold. Python accepts dict/list containers; tuples are rejected.
JavaScript accepts plain objects and dense arrays, without accessors, symbol
keys, undefined values, or custom serialization methods. Repeated references
are serialized as independent JSON subtrees; cycles are rejected.
JavaScript options omit object properties whose value is `undefined`, including
optional finding metadata; document data still rejects undefined values.

Use serde_json's default nesting contract: fewer than 128 nested containers.
Apply the same limit to parsed Rust values before cloning/transformation.
Conversion failures produce value-free `invalid_configuration` errors. Input
serialization preserves JSON values, not original formatting or source bytes.

## Discovery and configuration

The structured scan configuration is:

```json
{
  "locale": "en-US",
  "discover_person": true,
  "mappings": {"/customer/name": "PERSON"},
  "exclude": ["/example/first_name"]
}
```

All members are optional. Automatic discovery defaults to enabled. `locale`
retains the existing scanner behavior; it does not enable other field-label
languages. Mappings currently support PERSON only. Explicit mappings remain
active when `discover_person` is false. Exclusions suppress automatic PERSON
classification only; other detectors continue to run. An explicit mapping and
exclusion on the same path are an error. Repeated exclusions are errors. Empty
maps and arrays are accepted; unknown options and explicit null options are not.

Paths are non-root RFC 6901 JSON Pointers. There is no wildcard, suffix, dotted
path, or cross-record matching. Missing paths and non-string targets have no
mapping or finding. Map a string array element by its concrete index. Do not
inherit an alias from a container into its descendant values.

Built-in canonical aliases are `first_name`, `given_name`, `last_name`,
`family_name`, `full_name`, and `surname`. Snake-case aliases accept ASCII case
variants. The two-word aliases also accept their exact camelCase and PascalCase
spellings. `Surname` is covered by the case-insensitive single-word alias.
There is no substring, separator-removal, fuzzy, or dictionary matching.
`name`, `customer.name`, `package.name`, and ordinary prose do not infer PERSON.

A mapping reports `path`, `entity_type: PERSON`, `source`, and `rule`. Source is
`field_alias` or `explicit_mapping`; rule is the canonical alias or
`explicit_mapping`. It contains no field value, although application field names
can themselves be sensitive. Empty string values can have mappings but do not
have PERSON findings. Null and non-string values have neither.

For a string containing any non-whitespace character, a mapped PERSON finding
selects the entire original string, including surrounding whitespace. It does
not infer which substrings are given/family names. Detector names are
`datafog-core/person/field_alias` and `datafog-core/person/explicit_mapping`.
Confidence remains absent, and detector version is the Core package version.

## Operations and results

Rust exposes `discover_fields`, `scan`, `transform`, `scan_and_transform`, and
provider coordination functions in the `structured` module. Python exposes
`discover_fields`, `scan_structured`, `transform_structured`, and
`scan_and_transform_structured`. JavaScript uses `discoverFields`,
`scanStructured`, `transformStructured`, and `scanAndTransformStructured`.

Scan returns `{ mappings, findings }`. Each finding is `{ path, finding }`,
where the nested finding uses the existing text finding contract. Discovery is
also callable alone; ordinary structured scanning includes discovery automatically.

Every string leaf still runs the seven existing text detectors. Findings from
PERSON and existing detectors can overlap. Existing transformation selection
and overlap rules apply unchanged; selecting only PERSON can choose it over an
otherwise overlapping EMAIL finding. Callers must not interpret an unresolved
field as safe.

Object keys are traversed in lexicographic Rust string order and arrays in
numeric index order, depth first. Findings within a leaf sort by start byte,
then decreasing end byte, then entity type. Mapping order follows leaf order.
No global schema cache or propagation between heterogeneous records is added.

Byte/code-point ranges address the exact decoded string at the finding's path.
JavaScript also exposes UTF-16 ranges. No range addresses serialized JSON bytes.
The transformation result is `{ data, transformations }`; each record is
`{ path, transformation }` with the existing source/output range and provenance
fields. Unrelated values and container structure are preserved. Results have
no plaintext mapping or original matched text in transformation records.

Explicit transformation validates all supplied paths/findings before selection.
Invalid findings carry their original list index and a path such as
`/findings/1/path` or `/findings/1/finding/byte_range`. No partial structured
result accompanies an error and input data is not mutated.

Scan-and-transform uses `{ scan?: StructuredScanConfig, transform:
TransformationConfig }`. Scan and transformation settings stay separate. Its
transformation-stage errors receive the same `/transform` prefix as text calls.

## Providers

Rust, Python, and Node managers expose structured transform, scan-and-transform,
and restore methods. Validate the complete request before provider work. Resolve
each distinct key once, then issue one tokenization batch for the document.
Repeated values remain separate tokenization items. Provider correlation IDs
are opaque and must not be interpreted as field paths or indices.

Restoration validates all token envelopes before calling the provider,
deduplicates identical envelopes across fields, and issues one restoration
batch. Every token occurrence is restored or no structured result is returned.
Provider scope checks, errors, and exact-value semantics remain those of ADR 001.
Result atomicity does not imply rollback of external provider side effects.
Node snapshots structured inputs and options before asynchronous provider calls.

Browser WASM supports stateless structured operations. Selected provider-backed
strategies and structured restoration produce `unsupported_strategy`; input and
configuration validation still applies. Core makes no network calls and adds
no model, dictionary, or runtime dependency for discovery.

## Proof and limits

Shared structured detection and transformation fixtures run through Rust and
installed bindings. Additional tests cover strict conversion, Unicode ranges,
invalid later findings before provider work, key deduplication, document-wide
token batches, scoped restoration, and preservation of special JSON keys.

See [the implementation plan](../person-detection-plan.md) and
[performance measurements](../structured-performance.md). Coverage is
schema-guided PERSON protection. Arbitrary prose recognition, learned schemas,
additional formats, and cross-session known-name matching are separate work.

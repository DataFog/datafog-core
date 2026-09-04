# PERSON detection with automatic field discovery

**Status: Core and binding implementation complete; release and downstream adoption pending.**

The implemented contract is [ADR 002](adr/002-structured-person-discovery.md).
Verification and measurement results are recorded in [structured performance](structured-performance.md).

## Outcome and release proof

A caller submits a parsed JSON object or array. Core discovers documented
person-name fields, returns path-qualified `PERSON` findings, and supports
protecting the selected values through its existing transformation semantics.
Explicit mappings and exclusions handle application-specific schemas. No model,
dictionary download, or network access is required for detection.

The first release is proven by positive and negative shared fixtures across
Rust, Python, Node, and browser WASM; unchanged text-scanner conformance;
structured transformation and error tests; and a measured performance and size
comparison with the baseline. Published coverage must describe field discovery,
not arbitrary recognition of names in prose.

Core and its bindings ship together. The current MCP server and other consumers
adopt the verified package afterward. The historical Claude plugin is not the
architecture or API reference for this work.

## Recommended first-release behavior

| Input context | PERSON behavior |
| --- | --- |
| Documented unambiguous aliases such as `first_name`, `firstName`, `given_name`, `last_name`, `family_name`, and `full_name` | Infer a mapping and select the non-empty string value. |
| Caller-supplied field mapping | Apply the declared classification, with explicit provenance. |
| Caller exclusion from automatic PERSON discovery | Do not infer PERSON for that field; continue existing detectors. |
| `name` or `customer.name` without an explicit mapping | Leave unresolved; customers may be organizations. |
| `package.name`, `file.name`, and name-like words inside ordinary text | Do not infer PERSON. |
| Unknown fields | Run the existing text detectors on string values; lack of a PERSON mapping does not imply safety. |
| Null, empty, or whitespace-only name values | Produce no PERSON span. |
| Objects, arrays, numbers, and booleans | Traverse containers; preserve non-string leaves without coercing them into names. |

Resolve aliases through a documented finite list and explicit naming-convention
normalization. Do not use substring matches, fuzzy spelling, or broad synonyms.
Values are selected because of their schema context, not dictionary membership;
uncommon names and non-Latin names therefore remain eligible. Do not infer a
given/family-name split from the contents of a full-name field. For any value
containing a non-whitespace character, select the entire original string,
including surrounding whitespace; whitespace-only values have no finding.

An inferred mapping records an unambiguous field path and the rule that produced
it. Keep mapping evidence separate from `Finding.confidence`; deterministic
rules continue to omit a numeric confidence. Mapping summaries need not contain
the field's value. Treat paths as potentially sensitive metadata when downstream
applications decide what to log.

Automatic PERSON discovery is enabled in the new structured flow. Existing
`scan(text)`, `transform(text, ...)`, and text scan-and-transform calls retain
their current contracts. A raw JSON string passed to `scan(text)` does not gain
implicit JSON parsing.

## Core boundary and coordinate systems

Put traversal, alias rules, mapping precedence, and PERSON classification in
`crates/core`. Use the existing `serde_json` dependency where appropriate.
Bindings translate supported JSON values and results; they do not duplicate
classification rules. Start with parsed JSON data. CSV, SQL schema discovery,
source-code parsing, and original serialized-JSON byte preservation are outside
this release.

A structured finding consists of a field path plus the existing finding for
that string leaf. Its byte and code-point ranges address the exact decoded
string passed to the detector; JavaScript bindings additionally expose UTF-16
ranges. These are not offsets into the serialized JSON document. Source/output
ranges in structured transformation records use the corresponding leaf strings.

Use concrete RFC 6901 JSON Pointers to address object keys and array indices.
Escape keys containing `/` or `~`; dots in keys are literal. Do not introduce
implicit wildcard or suffix path matching. Discover mappings for each input;
do not add a global cache or automatically generalize one record's mapping to
other records. Explicit missing paths have no effect on that document, while
malformed paths and conflicting declarations are configuration errors. Define
mapping-versus-exclusion conflicts as errors rather than silently choosing one.

Keep discovery inspectable, and compose discovery, scanning, and transformation
in a convenient structured workflow. Public names and signatures are specified
in ADR 002. Preserve explicit-findings transformation and the
separate scan-then-transform convenience pattern from ADR 001.

Structured transformation must validate the entire request before returning
changes, preserve unrelated values and container structure, and return no
partial result on failure. Preserve existing entity selection, allowlists,
overlap resolution, overrides, and provider restrictions. Document and test
overlapping PERSON and existing detector findings rather than changing priority
rules incidentally.

## Implementation sequence

### 1. Contract, fixtures, and baseline

**Status: implemented.**

- Write a proposed ADR extending [ADR 001](adr/001-privacy-core-contract.md)
  with the structured operation signatures, JSON input representation, result
  shapes, mapping provenance, error paths, and deterministic traversal order.
- Finalize the alias list and normalization examples. Document PERSON-only
  automatic-discovery exclusions separately from transformation allowlists.
- Specify how invalid non-JSON inputs, numeric representation, malformed
  mappings, missing paths, and non-string mapped values behave consistently
  across bindings. Avoid silent value loss during binding conversion.
- Add expected structured fixture cases without changing the old text fixtures.
- Capture release-build baselines for existing scanning, binding startup, and
  package size, plus representative structured payloads for the new workflow.

**Exit evidence:** precise examples and expected outcomes cover the contract;
baseline commands and environment are reproducible. Proposed APIs are clearly
distinguished from shipped functionality.

### 2. Automatic discovery and scanning vertical slice

**Status: implemented and verified through installed bindings.**

- Implement JSON traversal, alias discovery, explicit mappings/exclusions, and
  PERSON findings in Core as distinct logical components where needed.
- Scan every string leaf with the existing detectors, including unresolved
  fields. Keep non-string values intact and document their coverage boundary.
- Expose the same capability through Python, Node, and WASM in this slice.
- Run the shared structured fixtures through Rust and installed bindings;
  retain the existing text fixture suite as regression evidence.
- Add discovery/scanning API documentation with examples and limitations.

**Exit evidence:** common name fields work without caller mappings; ambiguous
and unrelated fields do not become PERSON; explicit mappings and exclusions
behave identically across runtimes; every returned path and range selects the
reported value.

### 3. Structured protection and Core release

**Status: implementation and local verification complete. Publishing remains pending.**

- Compose the findings with existing transformation semantics and reconstruct
  the structured result in Core. Support explicit-findings transformation and
  the convenience workflow without duplicating detector logic.
- Prove stateless transformations across all bindings. Integrate provider-backed
  transformations/restoration through the existing manager boundary in Rust,
  Python, and Node; browser WASM retains its unsupported-strategy behavior.
- Define provider collection, batching, and validation for a whole structured
  request before implementation. Core result atomicity does not promise rollback
  of external provider side effects.
- Complete the public guides, binding references, release notes, and benchmarks.
- Publish compatible Core and binding packages after the required checks pass.

**Exit evidence:** the complete discover/scan/protect workflow passes across
supported runtimes, provider behavior preserves the existing contract, and
measured overhead and known limitations are reported with the release.

### 4. Downstream adoption

**Status: pending verified release and identification of the current MCP consumer.**

- Update the current MCP consumer to the verified Core package. It supplies
  structured payloads, application mappings, and transformation policy.
- Keep allow/warn/block decisions and tool interception downstream. Do not copy
  name rules or structured transformation semantics into that integration.
- Test the complete customer-record flow from tool payload to model-visible
  output, including unrelated fields, error responses, and integration-specific
  handling of scanner failure.
- Exercise any text-only tool output path explicitly: the new structured
  capability does not claim name recognition in arbitrary logs or prose.

Downstream development may use the settled API and a prerelease; its public
release depends on a verified Core/binding release. No change to the historical
Claude plugin is implied by this plan.

## Required test matrix

| Area | Proof |
| --- | --- |
| Discovery positives | Every documented alias and naming convention; nested records; arrays; uncommon and non-Latin names. |
| Discovery negatives | Generic `name`; organization/customer ambiguity; package and file names; key substrings; ordinary prose; aliases appearing only in values. |
| Mapping policy | Explicit mapping, exclusion, provenance, conflicting declarations, malformed pointers, absent paths, and non-string targets. |
| Structure | Empty objects/arrays, nulls, mixed arrays, heterogeneous records, nested containers, and keys containing dots, slashes, or tildes. No mapping propagation between unrelated records. |
| Ranges | Apostrophes, hyphens, combining marks, emoji, whitespace, and escaped input examples. Byte/code-point/UTF-16 slices select the exact decoded value. |
| Transformations | Redact, mask, remove, selection, overrides, exact/regex allowlists, overlapping findings, unrelated-value preservation, output ranges, and atomic errors. |
| Providers | Pseudonymization consistency, scoped tokenization/restoration, repeated values across fields, whole-request error behavior, and WASM restrictions. |
| Regression | Existing seven-detector fixtures, text APIs, strict configuration validation, and installed-package behavior remain valid. |

Use shared synthetic fixtures for deterministic conformance. Maintain separate
evaluation examples covering realistic schemas to report false mappings and
missed mappings; do not claim general prose-name recall from field fixtures.
Test fallible paths without silently skipping failed fields or panicking on
user-controlled input. Reuse existing test runners instead of creating a
parallel binding test framework.

Before merging Rust changes, run the repository-required commands:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Run installed Python, Node, and browser/WASM suites using
[Development](development.mdx), plus the documentation validation commands
there when the public documentation changes.

## Performance and documentation

Measure cold-start latency, repeated-scan throughput, memory/allocation behavior,
binding conversion and per-field costs, and native/WASM package size. Include
ordinary code/log strings, Unicode names, nested customer records, and repeated
records with and without discoverable fields. Verify offline runtime operation
without downloaded assets. Do not add a dependency or a mapping cache without
evidence that the current implementation needs it.

No numerical performance budget is established yet. Use the baseline and actual
product latency requirements to establish any acceptance budget before using it
as a release gate; do not invent a percentage or latency promise.

Ship a person-discovery guide, configuration/mapping reference, structured range
examples, all binding API references, README coverage updates, and release notes.
Document built-in aliases, override/exclusion rules, unresolved fields, language
coverage of field labels, non-string limitations, and the difference between
schema-guided protection and names recognized in prose. Update the capability
matrix and roadmap status only as functionality lands.

## Work outside this release

Known-name matching across a session, model-based or dictionary-based prose
recognition, fuzzy schema inference, additional document formats, and credential
detectors are independent follow-on work. None is required to establish this
release's automatic field-discovery contract.

## Draft release note

Add model-free PERSON discovery and protection for parsed JSON records across
Rust, Python, Node, and browser WASM. Common given/family/full-name field labels
work automatically; explicit JSON Pointer mappings and exclusions handle custom
schemas. Findings and transformation records include field paths and local
ranges. Rust, Python, and Node managers support structured provider operations.
Existing text APIs retain their coverage and behavior. See the person-discovery
guide for supported aliases, input limits, and the distinction from prose-name
recognition. Package versions and publication are pending.

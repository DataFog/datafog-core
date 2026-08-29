# Privacy Capability Matrix

DataFog Core preserves useful privacy capabilities without treating the Python
API as a contract. Each capability is classified by the treatment appropriate
for a PII detection and transformation engine.

| Capability | Treatment | Core direction |
| --- | --- | --- |
| PII scanning | Preserve | Return validated findings with explicit byte and code-point ranges, optional confidence, and detector provenance; JavaScript bindings also expose explicit UTF-16 ranges. |
| Typed redaction | Preserve | Replace findings with typed, document-local placeholders after deterministic overlap resolution. |
| Character masking | Preserve | Mask Unicode code points with a validated character and explicit leading- or trailing-reveal semantics. |
| Entity-type selection | Preserve | Select canonical entity types through transformation configuration. |
| Exact allowlists | Preserve | Exempt exact values using documented, case-sensitive matching. |
| Regex allowlists | Preserve | Use full-match semantics, validated patterns, and bounded execution. |
| Locale selection | Preserve | Pass locale constraints to detectors without placing detector implementations in the transformation layer. |
| Precomputed findings | Preserve | Permit transformations over caller-supplied findings after strict validation. |
| Prompt/output guardrails | Out of scope | A governance layer may consume Core findings and results to make and enforce `allow`, `warn`, or `block` decisions. |
| Hash replacement | Compatibility only | Exclude unkeyed hashing from canonical Core transformations; consider a plainly named fingerprint in a separate compatibility layer only for an accepted migration requirement. |
| Pseudonymization | Redesign | Use HMAC-SHA-256 over exact UTF-8 input with a provider-resolved 256-bit key and full padded-Base64 output. The key defines linkage scope; do not copy numbered Python placeholders. |
| Reversible tokenization | New | Issue opaque, versioned tokens through a provider-owned vault or reviewed reversible-crypto boundary, with exact scope/profile binding. |
| Restoration | New | Atomically restore every canonical token through an authorized provider call with exact scope checks and range-only audit metadata. |
| Transformation mappings | Redesign | Return ordered transformation records without `matched_text` or a plaintext-to-token mapping. Preserve source ranges and non-sensitive audit metadata. |
| Duplicate handling | Redesign | Collapse exact duplicates deterministically. |
| Overlap handling | Redesign | Resolve overlaps once in the transformation framework using documented precedence. |
| Malformed finding handling | Redesign | Reject the whole transformation instead of silently leaving possible PII unchanged. |
| Legacy function aliases | Compatibility only | Keep aliases out of the core; add wrappers only when a concrete migration need is accepted. |
| Legacy return shapes | Compatibility only | Reproduce only in a separately scoped compatibility binding, if needed. |
| Random fake-value replacement | Drop from core | Do not imply privacy guarantees for synthetic-looking replacements without a defined security and consistency contract. |
| “Anonymization” claims | Drop | Describe concrete transformations and leakage; pseudonymized data is not anonymous. |
| spaCy, GLiNER, or engine selection | Out of scope | Detector implementations remain separate from the privacy transformation contract. |
| OCR and image extraction | Out of scope | Text extraction is a separate subsystem. |
| Dataset-level privacy | Out of scope | k-anonymity, generalization, synthetic datasets, and differential privacy require different data models and risk analysis. |

## Compatibility evidence

Python tests and outputs may be used in three ways:

1. **Capability continuity:** prove that an important user workflow remains
   possible.
2. **Exact compatibility:** reproduce an output only when a documented migration
   requirement justifies it.
3. **Intentional divergence:** prove that the core behaves differently because
   the new contract is safer or clearer.

No abandoned or proposed Python versioned API is authoritative for the Rust
core.

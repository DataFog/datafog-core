# Protected overlap coverage in 0.5.0-beta.2

This is the reviewed stop-and-review behavior for
[#84](https://github.com/DataFog/datafog-core/issues/84), building on #155's
equal-span SSN attribution. The source/range comparator and duplicate preference
are unchanged. After full validation and explicit entity/allowlist filtering,
Core checks whether the selected ranges cover the union of every eligible input.

Previously a partial pair such as `[0,4)` and `[2,6)` could succeed by replacing
the first span while leaving the second span's suffix visible. It now fails with
`review_conflict`, a sanitized message and the original offending finding index;
there is no transformed output. Structured calls preserve global supplied-index
meaning, including when the conflict is in a later field. The public error path
identifies the finding rather than embedding a source/field value.

This is an intentional compatibility change across Rust, Python, Node and WASM:
callers must handle valid but unresolved findings and request correction/review,
rather than assuming that every valid range set transforms successfully. Python
uses its existing `DataFogFindingError` class with the distinct code. Do not catch
the error and send the original text or a manually shortened winner.

Exact duplicate claims, equal-span native SSN/PERSON handling, whole-cover
containment, explicit entity selection and exact/regex allowlists remain as
documented. Selection and allowlists still apply before the coverage obligation;
malformed findings fail before either filtering or conflict handling. Configured
mask/reveal policies retain their meaning. No synthetic union label, split
marker, global source precedence or model-routing change is introduced.

All seven shared selector consumers check coverage before output, provider key
resolution or token creation. Structured operations preflight the entire document
before any provider call, including conflicts in later fields. Unit tests keep
the old comparator reference and independently verify byte-wise coverage across
Unicode ranges, configurations and input orders. Installed Python/Node/browser
WASM regressions cover original-index errors, unchanged exclusion/allowlist and
containment behavior, validation before filtering, and zero provider calls.

Workspace combined review/policy conflicts are implemented separately by #146.
Legacy workspace calls retain their explicit caller-supplied-detection contract;
omission is never proof that input is free of PII. The extension launch integration
must adopt the new combined scanner/committed-recall/manual review path in #132.

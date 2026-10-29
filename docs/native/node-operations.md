# Native Node operation access — beta.3 draft

Thin native adapter for the committed #162 Core/WASM operation contract, including
its approved additive `operationsPage(offset, limit)` access. This draft does not
change history admission, wire encoding, accounting, replay policy or confirmation
reservation. Those decisions remain Core-owned. It is not a release or API freeze.

| Node API | Shared Core API |
| --- | --- |
| `Workspace.createWithOperation(namespace, operation)` | `create_with_operation` |
| `prepareWithOperation(text, findings, operation)` | single-input `prepare_batch_with_operation` |
| `prepareBatchWithOperation(inputs, operation)` | `prepare_batch_with_operation` |
| `prepareRecoveryWithOperation(operation)` | `prepare_recovery_with_operation` |
| `prepareOutcome(outcome, operation)` | `prepare_outcome` |
| `operations()`, `counts()`, `historyCoverage()` | corresponding read-only Core access |
| `operationsPage(offset, limit)` | borrowed Core `operations_page`, copied into owned JS records |

An operation input is exactly `{id: Uint8Array(16), occurredAtMs: number}`; time is
an integer host assertion in `0..253402300799999`. An outcome is exactly
`{kind, issueCount}`, using the current Core outcome kinds. Getters, extra fields,
non-finite/fractional times and invalid transport reject before staging a plan.
Core validates kind/count combinations, IDs, replay, metadata bounds and admission.

Results preserve the WASM camelCase records/counts/coverage shapes. IDs in returned
records are owned numeric arrays; mutations cannot change Core or saved data.
Counts cover recorded history only, with `preUpgradeHistory: "unknown"` for actual
v1/v2 bundles. Full `operations()` remains available; it is never silently paged.
UI callers should use explicit pages to avoid materializing every retained row.
Pages require limit `1..100000`, offset at most the total, and return
`{records, total, offset, nextOffset}`. An offset at the end is explicitly empty;
invalid offsets/limits fail with `resource_limit`, including values that N-API
could otherwise truncate to uint32.

All mutators use the existing opaque per-workspace prepared-change registry,
32-live-plan ceiling, durable-save-before-activation contract, stale sibling
cleanup and garbage-collection behavior. Reads/export/page traversal/discard do
not save anything or invent history. Actual v1/v2 read/export/legacy-no-op bytes
remain exact; a timed saved mutation can stage v3 and needs the existing old-reader
upgrade warning before approval. Copy confirmation is not a successful send and
never increments `confirmedHandoffs` in the current Core draft.

Installed tests execute the shared Core/WASM operation corpus through the packed
native library, plus native input ownership, accessors, plan-cap/activation,
closed-handle and exact legacy-byte checks. Positive and negative TypeScript
consumers compile against the installed package. This is actual current operation
access, not a placeholder for future conversation or confirmation APIs.

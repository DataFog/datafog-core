# Workspace finding sources — proposed 0.5.0-beta.2 contract

This is the review proposal for [D4 / #140](https://github.com/DataFog/datafog-core/issues/140).
The published `0.5.0-beta.1` package does not accept these input fields or return
this preview metadata. Pin a reviewed build that includes this change before using
them. The existing [beta.1 lifecycle contract](extension-integration-0.5.0-beta.1.md)
continues to govern approval, persistence, activation and restoration.

## Proposed fields

```ts
type WorkspaceFindingSource = "scanner" | "recall" | "user";

interface WorkspaceFinding {
  // Existing entity, matched text, byte range and detector fields remain.
  source?: WorkspaceFindingSource;
  needsReview?: boolean;
}

// Each WorkspacePreview.transformations[] item adds:
interface ReviewMetadata {
  source: WorkspaceFindingSource;
  needsReview: boolean;
}
```

`source` describes the declared origin of an input finding. `scanner` includes
rule-based and model findings; model evidence and detector identity remain separate.
`recall` identifies a workspace-recall finding; this change alone does not implement
the recall operation in [#141](https://github.com/DataFog/datafog-core/issues/141).
`user` identifies an explicit selection/correction supplied by the host.

`needsReview` is display metadata. It does not decide whether to transform a span,
authorize disclosure, prove that a person approved it, or permit handoff before
encrypted persistence and activation. Even `user` / `false` still follows the
normal approval/save/activate flow. Metadata is caller-declared and is not
authenticated or inferred from a detector name.

## Compatibility defaults for review

Omitted `source` defaults to `scanner`; omitted `needsReview` defaults to `true`.
This keeps existing preparation calls usable while conservatively requesting
review. The legacy source is a fallback label, not evidence of actual origin.
Hosts must annotate manual selections with `source: "user"` when accurate source
display is required.

Explicit `undefined` is treated as omission by the JavaScript transport. `null`,
unknown/case-mismatched source names and non-boolean review flags reject with
`invalid_finding`. Optional error indexes remain optional; malformed JSON can fail
before a particular input/finding index is available. Error payloads remain
content-free.

Preview output adds the two required fields; consumers that validate an exact set
of object keys must update their validators. Rust's `WorkspaceFinding::new`
retains its existing call shape and initializes the defaults. Rust callers using
struct literals must supply the new fields. This remains a provisional workspace
API change to review before merging.

## Selection and persistence

Sources and review flags never influence selection. Existing type/range validation,
duplicate preference and overlap resolution choose the retained input, and its
metadata follows it into the transformation. Equal duplicates with otherwise
identical detection fields retain the first input's metadata; reordering such a
tie may therefore change the displayed origin while leaving text and aliases
unchanged. Conflicting source fusion and correction precedence belong to the
separate D8 policy.

Single-input and batch previews expose the same fields, with input-local byte,
codepoint and UTF-16 ranges. Metadata is transient: it is not added to the CBOR
snapshot, alias identity, commit head, recovery bundle or backup format. Changing
only source/review flags does not change candidate bytes. Closing/discarding plans
and stale-plan reclamation retain their existing behavior.

```js
const plan = workspace.prepare(text, [{
  entityType: "PERSON",
  matchedText: selectedText,
  range: selectedByteRange,
  source: "user",
  needsReview: false,
}], crypto.getRandomValues(new Uint8Array(16)));

for (const span of plan.preview().transformations) {
  showReviewOrigin(span.source, span.needsReview, span.sourceUtf16Range);
}
// Explicit approval, encrypted durable save, then activateSaved are still required.
```

## Verification

Core tests cover conservative defaults, all source kinds in Unicode batches,
selected duplicates (including signed-zero confidence), stable ties, overlap
selection, unchanged candidate bytes and commit/reopen behavior. Installed WASM
conformance adds malformed metadata and accessor rejection, typings, ranges and
legacy caller compatibility. The actual unpacked MV3 model proof verifies source
metadata on model/scanner preview spans before the existing encrypted-save gates.

This proposal does not implement remembered choices/type rules, operation or
conversation records, recall matching, or a frozen snapshot contract.

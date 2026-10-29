# D8 combined review — implementation plan

Target `0.5.0-beta.2`, [#146](https://github.com/DataFog/datafog-core/issues/146).
Sid approved stopping handoff on unresolved overlap rather than exposing part of
another protected finding. Exact per-type/value `not_sensitive` exceptions follow
#145; generic keep for protected types remains prohibited. The implementation below is a draft public API for maintainer review; the approved
matching, policy and stop-handoff decisions do not by themselves approve its shape.

## Shared Core pipeline

1. Validate source, markers and every scanner/user/correction finding before any
   policy waiver, duplicate resolution or overlap selection. Bound aggregate
   input/result size and reject malformed or stale correction keys explicitly.
2. Add read-only committed recall. Scanner and user input channels establish
   display origin; caller source flags never grant trusted precedence or waive
   protected coverage. Exact duplicate origins fuse deterministically, with
   logical-OR review flags and stable detector metadata selection.
3. Apply committed exact policy and retain all remaining original protected
   intervals as obligations. A manual correction may expand/correct coverage;
   it cannot erase an old protected interval by shrinking or relabelling it into
   a waived identity. Record a type/value false positive through #145 instead.
4. Resolve exact duplicates and whole-cover containment deterministically.
   Corroborate an eligible equal-span native SSN with the Core rule rather than
   trusting an arbitrary caller source/detector string. Unresolved equal-span
   type claims or lost coverage produce content-free original-coordinate
   conflicts; no sanitized text or usable prepared plan is returned.
5. The new authoritative preparation recomputes review against current source,
   choices and head. Legacy single/batch preparation must use the same coverage
   guard. Preserve approval, full-head durable save, single activation, strict
   restore and existing live-plan limits.

Read-only review returns candidates and ranged conflicts, never handoff text.
Correction keys bind to head, exact source and candidate type/range. They are
ephemeral local identifiers, not saved provenance or disclosure authority.
Conflict ranges use original UTF-8 bytes, codepoints and UTF-16 units; a batch
failure identifies its input and leaves the entire workspace unchanged.

## Proposed runtime API

Rust `Workspace::review(ReviewInput)` and WASM `workspace.review(input)` return
read-only candidates and content-free conflicts, with no transformed text or plan.
Each input has `text`, `scannerFindings`, `userSelections` and optional
`corrections: [{candidateKey, replacement}]`. Source channels override supplied
source flags. Exact same type/range candidates merge origins, OR review flags and
retain deterministic populated detector metadata. Candidates expose an ephemeral
key, original coordinate ranges, committed-policy `eligible` and display-only
`selected` flags; neither flag authorizes disclosure.

`prepare_review(inputs, operation_id)` / `prepareReview(inputs, operationId)`
recomputes every input using current saved policy/head/recall. It rejects the
entire batch with `review_conflict`, original input-local byte/codepoint/UTF-16
ranges and batch index before creating a plan if any ambiguity remains. It then
uses the existing prepare/approval/durable-save/activation contract. A caller
must handle edits and stale approval through that same lifecycle.

Correction keys bind the exact source and committed head, plus type/range. Missing,
stale or duplicate targets reject; replacements validate before any policy waiver.
Shrinking/relabeling a correction cannot remove original protected coverage.
Expanding a correction to cover the whole affected area can resolve a crossing;
removing a false positive requires the explicit saved per-type/value choice.
No candidate key is persisted or accepted as approval.

The fixed maximums remain 1 MiB total source, 10,000 supplied + correction + recall
findings, 16 MiB aggregate finding material/transport and 32 batch members. Failures
are explicit, without truncation. Selection and coverage use ordered groups and
range unions, avoiding pairwise conflict expansion. Recall retains its separately
approved work/index limits.

Exact duplicates merge. A corroborated Core-native SSN supersedes an equal-span
PERSON claim; arbitrary caller detector/source strings do not establish that
exception. Other equal-span outermost type claims require correction. Whole-cover
containment can hide all affected text without a partial-span conflict. Crossing
claims are never split or assigned a synthetic union label.

Legacy `prepare` and `prepareBatch` validate every supplied finding, apply exact
committed policy, then enforce coverage/type conflicts on those supplied findings.
They **do not automatically recall**: omissions are never proof that input is
PII-free. The new combined path is required for the extension launch integration
in #132. The non-shipping reference host routes its manual selections to
`userSelections`, model scans to `scannerFindings`, and renders content-free ranged
conflicts through its existing generation/save/approval lifecycle. This does not
establish integration in Joshua's product.

## Generic transform dependency

[#172](https://github.com/DataFog/datafog-core/pull/172) implements the approved
narrow #84 guard, retaining the existing generic comparator and returning a
content-free `review_conflict` before any provider call if selected ranges lose
eligible coverage. See [compatibility details](../overlap-coverage-0.5.0-beta.2.md).
The workspace path additionally combines recall/manual sources and applies saved
per-value choices; generic transforms do not gain workspace policy or recall.

## Proof

The shared synthetic corpus covers disjoint findings, duplicate origins,
corroborated SSNs, covering containment, crossing spans in either order,
Unicode/conflict offsets, valid exact waivers, invalid-before-waiver failures,
manual correction coverage and model/recall/user coexistence. Additional tests
cover policy/backup reopening, stale keys/plans, batches, failed save and actual
packed-SDK MV3 edit → fresh review → approval → encrypted save → activation.
No snapshot field or operation/conversation provenance is introduced here.

# 0.5.0-beta.2: Chrome extension integration

Pin `@datafog/wasm@0.5.0-beta.2` and its lockfile after the release is published.
This prerelease is for synthetic developer integration. The [Requirements
tracker](https://docs.google.com/spreadsheets/d/1Z5bwpzXiyncgg4nC5tPehSoO3mvg6nTGh8gxI4Q-3y0/edit)
distinguishes the release target from verified delivery. A merged release PR alone
does not establish that registry publication succeeded.

The [0.5.0-beta.1 lifecycle guide](extension-integration-0.5.0-beta.1.md) still
describes session ownership, coordinate conversion, durable save/activation,
restoration and explicit model setup. This page describes the beta.2 changes.
The [launch API reference](contracts/launch-api-0.5.0-beta.2-beta.3.md) separates
implemented calls from future proposals. Neither the SDK nor this guide adds a
vault or review UI to Josh's product; product acceptance remains [#132](https://github.com/DataFog/datafog-core/issues/132).

## Review before handoff

Use `workspace.review({text, scannerFindings, userSelections, corrections?})` to
combine detector findings, explicit selections and committed-value recall. Model
findings need workspace UTF-8 byte ranges, as in the earlier integration guide.
Scanner and user inputs use separate channels; caller metadata does not grant
disclosure permission. Optional `corrections` may be omitted or an array; explicit
null is invalid.

Render the returned candidates and their `sources`, `needsReview`, original byte,
codepoint and UTF-16 ranges. `eligible` means saved policy requires hiding that
candidate. `selected` describes the safe automatic selection; it is not approval
or permission to send text. If there are conflicts, every candidate is unselected.

Unresolved crossing coverage or equal-span type conflicts require local review.
Highlight `conflicts[].utf16Range` against the exact original source. Correct the
source or supply a covering selection/correction, then review again. A correction
uses `{candidateKey, replacement}`; keys are tied to the source and workspace head,
are not saved, and cannot waive another protected interval by shrinking a span
or changing its type. Do not silently omit a detection to clear a conflict.

After review, call `prepareReview([input], operationId)` with the current source
and findings. Core recomputes recall, policy and coverage; do not submit an old
review result as authority. A conflicting member rejects the entire batch with
`review_conflict`, an input index and content-free conflict ranges, without a plan
or state mutation. Exact duplicates, safe containing spans and corroborated
equal-span SSN attribution can resolve without an unnecessary conflict.

Show the plan's exact previews and any format-upgrade warning. On approval,
export its candidate, encrypt and atomically save against the complete expected
head, confirm the durable head, then call `activateSaved`. Release text only after
activation succeeds. Edits or changed heads require fresh review and approval.
Discard rejected plans; an uncertain save requires reopen and fresh review.

Legacy `prepare` and `prepareBatch` validate and guard the findings supplied by
their caller. They do not add recall automatically. Omitted findings and an empty
recall result never establish that the source is free of PII. Generic transform
APIs now also return `review_conflict` when their eligible selected spans would
leave part of another eligible finding exposed, before provider calls or output.

## Recall and exact remembered choices

`recall(text)` reads committed values without changing aliases or the head. It
matches existing case/whitespace identity normalization at word boundaries, with
original-coordinate results. It does not infer typos, abbreviations or hidden
account digits. Restoration keeps the first committed spelling.

`policy()` reads saved rules and choices. `preparePolicyChanges({rules, choices},
operationId)` stages explicit edits through the same approval/save/activation
flow. Protected types hide by default and forbid generic keep or type-wide
disclosure. An explicit `not_sensitive` correction may exempt exactly one
normalized type/value, even before it has an alias. Core cannot verify that the
assertion is truthful: an intentional correction can expose actual PII. Explain
this before approval. Removing a setting uses explicit null; missing required
edit fields reject. Source and review flags never authorize such a correction.

Source/review metadata defaults to `scanner` and `true` for legacy findings.
Recall contributes `recall` and `true`; combined review preserves all duplicate
origins and combines review requirements. Read results remain local sensitive
data, not telemetry.

## Existing vaults and backups

New workspaces use policy snapshot v2. A v1 workspace opens and exports unchanged;
read, exact no-op and discard do not upgrade it. A real saved mutation or recovery
may stage v2. Inspect `plan.snapshotFormat()` and explain **before approval/save**:
**0.5.0-beta.1 cannot open the upgraded vault.** Retaining a pre-upgrade backup does
not make later v2 changes readable by beta.1. Never force an upgrade after a failed
save or activate an unconfirmed candidate.

The reference host's new password backups use Argon2id with 256 MiB, three passes
and one lane, require fresh password verification for every export, and preserve
supported legacy imports. Low-memory failure is explicit; no cheaper fallback
is permitted. Credentials, encryption, key storage and durability remain host
responsibilities. This reference implementation does not qualify an unreviewed
shipping extension or server vault.

## Detection and limits

The added rules cover explicitly labelled account digits, account-labelled masked
values/four-digit endings, and bounded street lines after `street address:`,
`shipping address:` or `billing address:`. A suffix is a literal observed value,
not a full account identity. `STREET_ADDRESS` differs from full `ADDRESS`.
These partial rules retain a configured model's broader coverage; they do not
make every address or account detectable without the optional model.

Recall/review limits fail the whole query with `resource_limit`; never turn an
error into an empty successful finding list. The contract documents the provisional
1 MiB source and 10,000-result ceilings plus index/work/transport bounds. Some
valid large workspaces may reject a short query. The browser Small model retains
its separate 65,536-byte/512-token request bounds and explicit provisioning.
Development hardware measurements are not slower-device guarantees.

## Next version and acceptance

Native Python/Node workspace bindings, operation/conversation records, contextual
and progressive streaming restore, server-host qualification and all-SDK contract
freeze target `0.5.0-beta.3`. Their development drafts are not beta.2 runtime APIs.
The existing native detection/transformation packages remain available.

Test the exact published package in the shipping extension: model/rule findings,
combined review, overlap correction, explicit policy change, approval/save failure,
reopen/restart, strict restore, old-vault upgrade warning and encrypted recovery.
Real-data use still requires the tracker’s shipping detection, provider-marker,
complete backup, contract-freeze and browser/server security gates.

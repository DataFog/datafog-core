# Workspace Aliasing V1: Implementation Sequence

Decisions are in [ADR 004](adr/004-workspace-aliasing-v1.md). Task numbers
(0.1, 1.1, D1, 6.3) refer to the V1 engineering breakdown. Work runs in two
repositories: `datafog-core` (Core, WASM, harness) and the new extension
repository.

## This stack

Each PR targets the one before it:

| PR | Change | V1 task |
|---|---|---|
| PR process | Changelog policy and PR writing guidance | — |
| Decisions | ADR 004 and this plan | Phase 0 |
| Persistent workspace | Core aliasing with `[[LABEL_CODE]]` markers, CBOR snapshot, WASM API, encrypted vault harness, browser-restart test | 0.1, 2.1 |
| Identity | Case/spacing-insensitive identity | supports 1.1, 1.3 |
| Batches | Atomic multi-input prepare and approve | base |
| CSV intake | Harness CSV intake with cell review | 6.4 (harness) |
| PDF intake | Harness PDF text intake with page review | 6.2 (harness) |
| Follow-ups | Committed markers pass through new text | base |
| Restore report | All-or-nothing restore that lists blocking markers | Core side of 6.7 |
| Backup | Encrypted backup export and recovery | 5.1 |

The harness in `examples/workspace-extension` is synthetic-only conformance code.
Its CSV, PDF and backup flows are the starting point for the extension
repository.

## datafog-core: remaining Core work

Each item is one PR stacked on the previous one unless marked parallel. Every
item that adds a snapshot key adds it to the decoder tests and the backup
round-trip.

1. **Finding sources (1.4).** `WorkspaceFinding` gains `source` (`scanner`,
   `recall`, `user`) and `needs_review`. Previews report each transformation's
   source. Size S.
2. **Workspace recall (1.1).** Find every committed value in new text by
   normalized identity, at word boundaries, and return findings marked `recall`.
   This is the cheapest accurate source for statements that repeat monthly.
   Size M.
3. **Rules, dispositions and category aliases (0.3, 1.2).** Snapshot keys 6 and
   7. Category issuance uses the marker grammar with the category label. Recall
   skips values remembered as keep or not sensitive. Size M.
4. **Combine sources (D3).** One finding set from scanner, recall and user
   corrections; remembered negatives removed; existing overlap resolution
   reused. Size S–M.
5. **Statement table input and CSV output (3.1, 3.2).** Rows of date,
   description and amount with per-column rules: description scanned, date and
   amount passed through. Core serializes the cleaned table as CSV so the
   handoff and restore views agree byte for byte. Size M–L. This is on the
   critical path to the review screen.
6. **Merges (1.3).** `prepare_merge` and snapshot key 5; restoration to the
   canonical display value; no undo. Size M.
7. **Provenance, conversations and counts.** Snapshot keys 8, 9 and 10.
   Operations take a host-supplied time, so the WASM calls take
   `{operationId, issuedAtMs}`. A conversation record is surface, the site's
   own ID, workspace and issued codes. Restoration can then also ask whether a
   marker was ever sent into this chat. Size M.
8. **Freeze (2.2, 4.1).** Fixtures for every snapshot key, a version constant
   for the WASM workspace contract, and a changelog entry stating the frozen
   format. Size S. Gated on the model marker tests below.

Parallel tracks that can start now:

- **Statement rules (D1).** Detectors in `crates/core` for account numbers after
  a label, masked forms ("ending in 1234", `XXXX1234`), and transfer payees
  (`ZELLE TO …`, `ONLINE TRANSFER TO …`, `VENMO *…`). They need positive and
  negative tests per AGENTS.md. Size M.
- **Test set and scoring (D4).** Synthetic statements in several bank layouts
  with planted values and a scoring script for the ADR 004 bar. The extension's
  table extractor (6.3) uses the same layouts, so build this early. Size M.
- **Browser model spike (D2).** On its own branch. It does not join the stack
  unless it succeeds, and it never gates the pilot.

## Model marker tests (0.1)

Run before the freeze, on ChatGPT, Claude and Gemini:

1. Use three synthetic statements of about 150 rows, with 40–60 distinct
   markers across `PERSON`, `ACCOUNT_NUMBER` and two categories.
2. Ask five question types: totals by payee, largest transactions, recurring
   charges, a monthly summary table, and a free-form explanation.
3. Count markers kept verbatim, kept apart from case or Crockford substitutions,
   unbracketed, damaged and invented. `restore_report` classifies them.
4. Record tokens per marker with each provider's tokenizer.

Proposed pass: at least 99% of referenced markers restore without user action,
with no invented marker that resolves. If one model fails on a label shape, change
the grammar before the freeze, not after.

## Extension repository

1. **Bootstrap.** Copy the harness broker, vault, storage, backup and RPC code.
   Pin `@datafog/wasm` to a tag or commit. Keep the fault-injection hooks out of
   release builds.
2. **Vault interface.** Open, save-if-head-matches, lock, export, import, with a
   vault holding a list of workspaces. Run CI through it. Do this before any
   real data.
3. **Side panel shell (6.1).**
4. **PDF and CSV intake (6.2, 6.4).** Port from the harness. Apply the 1 MiB
   limit to extracted text, not to the file's bytes; statements with logos often
   exceed 1 MiB.
5. **Table extractor (6.3).** The riskiest v1 task, so start it early against
   the D4 layouts. Show dropped content in the preview and block the batch when
   rows don't parse confidently. Once this gates output, drop the image and
   annotation rejections in PDF intake.
6. **Review screen (6.5).** Needs Core items 1–5. Highlight by source, add or
   undo, "remember" toggle, keep-or-category for merchants, rebuild and
   re-approve. Record content-free counts.
7. **Handoff and restore (6.6, 6.7, 6.8).** Write conversation records on
   handoff. The restore view uses `restoreReport` and highlights issue ranges.
   Real-name export is CSV.
8. **Merge screen, settings guide, cleanup (6.9, 6.10, 6.11).**
9. **End-to-end CI (6.12).** PDF → table → detect → approve → save → restart →
   recall → restore, against the pinned WASM build.

## Pilot gate

Real statements start only when all of these hold:

- The model marker tests pass and the snapshot and WASM contract are frozen
  (0.1, 2.2).
- D4 meets the ADR 004 bar.
- The backup round-trip covers every snapshot key (5.1).
- The vault encryption review is done.

The critical path is Core item 5 and extension items 4 → 5 → 6. The table
extractor and statement table input carry the most schedule risk.

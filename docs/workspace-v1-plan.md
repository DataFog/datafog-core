# Workspace Aliasing V1: Implementation Sequence

Decisions are in [ADR 004](adr/004-workspace-aliasing-v1.md). V1 is free-text
paste: pseudonymize pasted text, send it to ChatGPT, restore the answer. Work runs
in two repositories: `datafog-core` (Core, WASM, detection, harness), driven by
Sid, and the Chrome extension, driven by Josh.

## This stack

Each PR targets the one before it:

| PR | Change |
|---|---|
| PR process | Changelog policy and PR writing guidance |
| Decisions | ADR 004 and this plan |
| Persistent workspace | Core aliasing with `[[LABEL_CODE]]` markers, CBOR snapshot, WASM API, encrypted vault harness, browser-restart test |
| Identity | Case/spacing-insensitive identity |
| Batches | Atomic multi-input prepare and approve |
| Follow-ups | Committed markers pass through new text |
| Restore report | All-or-nothing restore that lists blocking markers |
| Backup | Encrypted backup export and recovery |

The harness in `examples/workspace-extension` is synthetic-only conformance code.
Its vault, broker and backup flows are the starting point for the extension.
Harness CSV and PDF intake is parked on its own branch as a reference for the
file-upload investigation; it is not part of this stack.

## datafog-core: remaining work

Each item is one PR stacked on the previous one. Every item that adds a snapshot
key adds it to the decoder tests and the backup round-trip.

| # | PR | What the extension gets | Size |
|---|---|---|---|
| 0 | Contract draft | TypeScript declarations for every call through PR 5, so the extension builds against stubs now | S |
| 1 | Finding sources | Each finding tagged `scanner`, `recall` or `user`, plus `needs_review`; previews report each transformation's source | S |
| 2 | Workspace recall | Every committed value found again in new text, by normalized identity at word boundaries | M |
| 3 | Rules and remembered choices | Snapshot keys 6 and 7: a rule per type, and keep / not-sensitive choices per value; recall skips values remembered as keep or not sensitive | M |
| 4 | Combine sources | One finding list from scanner, recall and the person's corrections, using existing overlap resolution | S–M |
| 5 | Provenance and conversations | Snapshot keys 8–10: operation times (WASM calls take `{operationId, issuedAtMs}`), conversation records, content-free counts; restore can check whether a marker was sent into this chat | M |
| 6 | Freeze | Fixtures for every snapshot key, a version constant for the WASM contract, a changelog entry stating the frozen format; gated on the model marker tests | S |

If the file-upload investigation keeps CSV in V1, add:

- **CSV columns by header.** Read rows into records keyed by header; extend
  `discover_fields` aliases beyond `PERSON` to email, address, phone and account
  fields; return whole-column suggestions for approval; pseudonymize approved
  columns cell by cell through the workspace. Size M.

## Detection, in parallel

- **Browser model (D2).** The in-browser model for V1 is GLiNER2
  (`fastino/gliner2-privacy-filter-PII-multi`). The DataFog PII model is on hold
  and may return through the planned model-engine interface (ROADMAP.md, step 6).
  GLiNER2 is community-tier: accuracy regressions warn but do not block PRs.
  Owner: open.

  **Open questions (browser feasibility):**
  - There is no existing WASM or browser path for GLiNER2 in this repo. The
    Python worker (`bindings/python/python/datafog_core/_gliner2_worker.py`)
    runs via a local Python interpreter; Node and Rust call that worker.
  - Community ONNX exports exist and can run in ONNX Runtime Web:
    - `okasi/gliner2-privacy-filter-pii-multi-onnx` — same checkpoint we use;
      Q4 479 MB, Q8 533 MB, plus 16 MB tokenizer. Apache-2.0.
    - `onnx-community/gliner2-multi-v1-agent-ONNX` — different model (multi-v1);
      fp16 614 MB, fp32 1228 MB. Apache-2.0.
    - No fp16 export of the privacy-filter checkpoint is published. The PyTorch
      weights we use are 1228 MB.
  - The GLiNER2 count-floor fix (issue #87) wraps `model.count_pred` in a
    `FlooredCountPred` torch.nn.Module. At inference it boosts the count=1 score
    when the predicted count is 0, lifting it to 1. A browser runtime must apply
    the same floor to the count head's output, either in JS postprocessing or
    inside the ONNX graph. No named export is verified to include this fix.
  - Latency, memory and download size on real extension hardware are unmeasured.
  - Language coverage: en, fr, es, de, it, pt, nl. Does not include Arabic,
    Persian or Turkish.

  The fallback, if in-browser inference is too slow or too large, is a small
  local helper reached through native messaging. Size M–L.

- **Free-text rules (D1).** Account numbers after a label, masked forms ("ending
  in 1234", `XXXX1234`), and a baseline street-address rule. Positive and negative
  tests per AGENTS.md. Size M.

- **Test set and scoring (D4).** Synthetic pasted text (emails, notes, message
  threads, forms) with planted values and a scoring script for the ADR 004 bar.

  **Scoring notes:**
  - The D4 detection bar (ADR 004) requires at least 90% of names caught. Measure
    with GLiNER2's `person` label at threshold 0.5.
  - Street-address recall is an open measurement: the fixtures have no labelled
    `STREET_ADDRESS` examples yet.
  - The GLiNER2 eval baseline (`baselines/eval-models-gliner2.json`) is
    provisional and warn-only; every score is currently n/a because there are no
    `STREET_ADDRESS` labels. Until a stable baseline is committed, accuracy
    regressions do not block.

  Size M.

## Model marker tests

Run before the freeze, on ChatGPT, Claude and Gemini:

1. Use three synthetic pasted texts with 40–60 distinct markers across `PERSON`,
   `EMAIL`, `PHONE`, `ADDRESS` and `ACCOUNT_NUMBER`.
2. Ask five task types: summarize, draft a reply, extract into a table, rewrite,
   and a follow-up that reuses earlier markers.
3. Count markers kept verbatim, kept apart from case or Crockford substitutions,
   unbracketed, damaged and invented. `restore_report` classifies them.
4. Record tokens per marker with each provider's tokenizer.

Proposed pass: at least 99% of referenced markers restore without user action,
with no invented marker that resolves. If one model fails on a label shape, change
the grammar before the freeze, not after.

## Extension

The Chrome extension lives in its own repository and is owned by Josh. All items
below are extension-side work.

1. **Bootstrap.** Port the harness broker, vault, storage, backup and RPC code.
   Pin `@datafog/wasm` to a beta. Keep fault-injection hooks out of release builds.
2. **Vault interface.** Open, save-if-head-matches, lock, export, import, over a
   vault that holds a list of workspaces. CI tests through it.
3. **Paste surface.** Where the person pastes and reviews. A text area inside the
   extension keeps raw text out of ChatGPT's page; pasting into ChatGPT's box and
   replacing it in place is quicker but puts raw text in the page first.
4. **Review.** Findings highlighted by source; add or undo; a remember toggle
   for keep and not sensitive; approve. Needs Core PRs 1–4.
5. **Handoff.** Copy, or insert into ChatGPT's message box (the prototype's
   insertion code), and write a conversation record.
6. **Restore.** Paste the answer, restore all-or-nothing, highlight issue ranges
   from `restoreReport`.
7. **Follow-ups.** Later questions in the same workspace keep earlier markers.
8. **Settings guide and cleanup.** Temporary Chat, memory and training settings;
   clear temporary data on lock.
9. **File-upload report.** CSV, Excel and PDF findings, and a clear "not
   supported" message for anything V1 doesn't take.
10. **End-to-end CI.** Paste → detect → approve → save → restart → recall →
    restore, against the pinned WASM build.

## Betas

| Beta | Ships after | Unblocks |
|---|---|---|
| A | This stack | Vault, handoff, restore, follow-ups, backup |
| B | Core PRs 1–2 | Review highlights and recall |
| C | Core PRs 3–4 | Remembered choices and a complete review screen |
| D | Core PR 5 | Conversation records |

## Pilot gate

Real text goes into a workspace only when all of these hold:

- The model marker tests pass and the snapshot and WASM contract are frozen.
- D4 meets the ADR 004 bar.
- The backup round-trip covers every snapshot key.
- The vault encryption review is done. **Open question for the review:** exported backup files carry the password-wrapped root key and can be brute-forced offline at the current Argon2id profile (64 MiB, t=3). Whether exports should use a stronger (slower) KDF profile is an open item.

Before that, real-data testing uses a throwaway browser profile or a build that
saves no mappings. The critical path is the review screen (Core PRs 1–4) and
GLiNER2 in the browser, which decides how many names are caught before review.

## Open risks for GLiNER2-first

1. **Browser feasibility.** No browser path exists today. ONNX exports exist but
   download size (479–533 MB for the privacy-filter checkpoint, 614+ MB for
   multi-v1) may be prohibitive for an extension. Measure before committing.
2. **Count-floor (#87).** The `FlooredCountPred` fix wraps `model.count_pred` in
   the Python worker. A browser runtime must apply the same floor to the count
   head's output. No named export is verified to include it.
3. **Street-address fixtures.** The D4 set has no labelled `STREET_ADDRESS`
   examples, so the 90% bar cannot be measured until fixtures are added.
4. **Accuracy baseline.** The GLiNER2 eval baseline is provisional and warn-only;
   every score is n/a because there are no `STREET_ADDRESS` labels.

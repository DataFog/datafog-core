# Synthetic workspace integration slice

This non-shipping Manifest V3 harness starts from DataFog Core 0.4.4 and proves:

**supplied findings → local preview/approval → encrypted atomic save → activation → browser restart/unlock → identical alias reuse → exact local restoration.**

The second slice adds atomic batches of synthetic text. Mark a text's selections,
choose **Add text to batch**, and repeat. **Prepare local preview** includes queued
inputs and any current draft. A rejected member blocks the entire selection and
identifies its input number. Explicit removal invalidates the old preview; prepare
and approve the remaining selection again. Inputs are kept separate in the Core
preview and broker result; numbered sections are display formatting only.

Use synthetic data only. The workspace API, development CBOR schema and marker format are provisional. Do not put real statements in this harness or treat it as the completed privacy extension.

## Run locally

Use the repository's Rust 1.88 toolchain and Node 24 or later:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127 --locked
npm ci --prefix bindings/wasm
npm ci --prefix examples/workspace-extension
npx --prefix examples/workspace-extension playwright install chromium
npm run test:workspace-integration --prefix examples/workspace-extension
```

The command builds and packs the actual WASM package, builds local extension assets and runs the real browser/storage/crypto suite. Add `DATAFOG_HEADED=1` to inspect the browser while it runs. Every scenario uses an isolated temporary profile; the restart test reuses that profile, and cleanup removes it afterward. The termination scenario verifies the isolated browser's user-data directory and process identity before killing that test process.

To inspect the UI separately, run `npm run build --prefix examples/workspace-extension`, load `examples/workspace-extension/dist` as an unpacked extension in Chrome's extension developer settings, and click its toolbar action. Use a disposable browser profile. Closing the owning extension page drops its runtime-only unlock; restart requires the password. A password lost in this synthetic slice cannot be recovered—backup/recovery remains later work.

## Ownership and contract

- `crates/core/src/workspace.rs` owns exact typed identity, marker allocation, validation, pending-change lifecycle and restoration. `workspace/codec.rs` owns the bounded, canonical CBOR development schema. Existing selection/overlap and offset machinery is reused.
- The WASM package exposes `Workspace` and opaque prepared changes. Byte ranges are UTF-8, zero-based and end-exclusive; `workspaceByteRangeFromUtf16` converts browser selections using Core. Optional detection metadata is never required or persisted.
- `core-worker.js` runs Core/WASM. `broker.js` owns approval state and save/activation sequencing; `vault.js` uses native Web Crypto and a separate transient `kdf-worker.js`. Neither the password nor vault keys enter DataFog WASM.
- `storage.js` uses a real IndexedDB read/compare/write transaction and waits for transaction completion. The expected head contains a lossless uint64 revision and complete commit ID. Each workspace has one encrypted bundle containing both snapshot and companion privacy configuration.
- Original values, whitespace, case and Unicode representations are exact. Custom ASCII entity codes are accepted. No detector development or accounting/Sheets behavior is included.

`Workspace.create(namespace, operationId)` takes host-generated 16-byte public CSPRNG IDs. `prepare` does not change committed state. Preview/export keep a change pending; `activateSaved` or discard ends it, including a no-change preparation. The trusted host must actually save before calling `activateSaved`: Core cannot verify a human approval or disk durability. Closing Core invalidates outstanding plan access.

`Workspace.prepareBatch(inputs, operationId)` prepares one candidate bundle for the
whole ordered selection. `change.previews()` returns one preview per input, with
ranges local to that input. `change.preview()` requires exactly one input. The Rust
equivalents are `prepare_batch` and `previews`. New values share allocation state
across members, and one changed batch advances the workspace head once. Failure
returns no partial plan and leaves committed mappings intact. Errors may carry
`inputIndex` and an input-local `findingIndex`, without matched text. The existing
CBOR schema and previously issued marker bytes are unchanged.

## Crypto and persistence

The host uses AES-256-GCM (fresh 96-bit nonces, 128-bit tags), random vault/workspace keys, and explicit libsodium Argon2id13: 16-byte salt, 32-byte result, 64 MiB and opslimit 3. Only the bounded development KDF profile is accepted; there is no weaker fallback. Password and workspace key wrappers have separate authenticated contexts. The snapshot AAD binds its vault, workspace, key ID and complete commit head.

Passwords and raw key buffers are cleared where possible, and runtime Web Crypto keys are nonextractable. JavaScript/native memory erasure is best effort; this does not provide hardware-bound key storage or protection from malicious extension code. IndexedDB transaction completion and requested strict durability do not establish a guarantee against every OS/power failure. Authenticated snapshots do not prove that no newer history exists.

Initial limits are **development guardrails**, not production capacity claims: 1 MiB source, 10,000 findings, 20,000 entries and 16 MiB bundle/output. Finding transport is also bounded before JSON serialization, including repeated object references. Exceeding limits rejects the whole operation. Full-year capacity, final marker preservation/overhead and production security review remain later qualification.

Core batches accept 1–100,000 text leaves, accommodating CSV cells without a new
pseudonymization path. Source bytes, findings and preview output limits apply to
the aggregate selection. The WASM batch JSON transport is bounded to 16 MiB. The
host limits a selection to 32 source inputs/files. These are development guardrails.

## CSV intake

Choose synthetic CSV files locally, then **Review input**, select a record/column,
and **Load text**. Mark selected sensitive text using the existing entity-type
field. **Clear selections** corrects that cell's supplied findings for this
upload. Both adding and clearing selections invalidate approval. All records,
including metadata and the first row, are available; there is no header inference.

The adapter accepts comma-delimited UTF-8, including a UTF-8 BOM, CRLF/LF/CR record
endings, escaped quotes and quoted multiline fields. It preserves whitespace,
duplicate header values, empty cells, blank records and ragged row widths. Every
cell remains a string: amounts, dates and leading-zero IDs are not coerced. The
local preview and approved output are ordered JSON string tables. Original files
stay unchanged, and filenames are displayed locally without entering the output.

Empty files, malformed quoting, invalid UTF-8/NUL-containing input, unsupported
extensions and exceeded limits produce blocking rejected entries. Remove the
rejected file explicitly and prepare/approve the remaining selection again.
Errors from Core are translated from leaf indexes to file/record/column positions.
Records and columns are 1-based in the UI; Core ranges remain input-local and
zero-based. Textarea selections are mapped back over CRLF/CR normalization before
Core converts UTF-16 offsets into UTF-8 byte ranges.

Raw files are limited to 1 MiB each, with aggregate decoded source limited to
1 MiB and 100,000 cells/leaves. A selection exceeding the 32-input limit is rejected
as a group before reading its new files. Progress reports counts; cancellation
aborts FileReader and terminates the parser worker. Completion/cancellation clears
temporary inputs, decoded cells and review selections. No file contents, filenames
or parser excerpts enter diagnostics or CI artifacts.

The host pins MIT-licensed `csv-parse` 7.0.3 for strict CSV parsing. Its
[browser ESM distribution](https://csv.js.org/parse/distributions/browser_esm/)
is bundled locally in a worker (~109 KB unminified in this build); its license is
copied alongside the bundle and asset hashes are recorded. Casting, record skipping,
header conversion and delimiter guessing are disabled. This dependency provides
quoted-field parsing without a new hand-written CSV parser or a Core dependency.
The synthetic workflow uses manually supplied findings.

## PDF text intake

The file picker also accepts synthetic, unencrypted text PDFs. Choose **Review
input**, enter a 1-based page number and **Load text**. Each page is one reviewable
text block, so a name split across adjacent PDF text runs can be selected as one
finding. Core offsets are local to that page's extracted text. **Clear selections**
and further selections invalidate approval; prepare and approve the updated
selection again. CSV and PDF inputs share the same Core batch and saved
workspace aliases.

The approved JSON has `format: "pdf"` and ordered `pages`. Each page contains
`number`, its native PDF `view` bounds, `rotation`, `userUnit` scale and pseudonymized
`text`. Text follows PDF.js extraction order and whitespace handling, with Unicode
normalization disabled. Dates, amounts and leading-zero identifiers remain
extracted strings. Page positions are retained separately from the text; no table
or accounting interpretation is performed. Filenames, PDF metadata, annotations,
embedded files and original PDF bytes do not enter this output.

Every page must contain extractable text. Image-only and blank pages block the
file. Raster images also block intake, including decorative logos and text layers
over scans: this slice makes no exception that could silently drop statement text
inside an image. Encrypted PDFs are rejected even when an empty user password lets
them open without prompting. Forms/XFA, annotations (including links), scripts,
attachments, optional layers, collections, signatures and invisible-text modes are
unsupported and block intake. Malformed/repaired input, parser warnings and parser
error replies also block the file. Supply a supported text-only PDF or CSV export;
explicitly remove a rejected file before reviewing and approving the remainder.

Admission limits remain 1 MiB per source file and 1 MiB aggregate decoded source.
Each PDF is limited to 256 pages, 100,000 extracted text items and 100,000 operators
per page; its page texts count toward the existing aggregate leaf limit. Each file
has a 30-second parsing deadline. Cancellation and the deadline terminate its
worker. The current page's stream is drained without retaining additional text
when decoded limits are exceeded, avoiding a PDF.js stream cancellation race;
the operation still rejects as a whole. PDF.js exposes no hard heap or decoded
stream-size cap, so these remain development guardrails rather than a parser
memory guarantee. Completion/cancellation drops temporary bytes, page text and
findings, while preserving committed encrypted mappings.

The host pins Apache-2.0 [Mozilla PDF.js](https://mozilla.github.io/pdf.js/) via
`pdfjs-dist` 6.4.299. Its compatibility build runs entirely inside the terminable
local worker, using locally bundled standard fonts and CMaps. It provides PDF font,
encoding and content-stream interpretation without a new hand-written PDF parser
or Core dependency. The script is about 3.4 MB unminified, plus local font/CMap
resources. The bundled core-js 3.50.0 MIT notice and resource licenses accompany
the build. The optional Node canvas package is not used by the browser bundle.

Parser diagnostics are reduced to a boolean occurrence flag inside the worker,
never logged or forwarded as exception text. The adapter also checks PDF.js error
replies because this pinned version can return a partial operator list after a
stream failure. These paths are covered by packaged-worker rejection tests.

## Protected follow-up questions

After unlocking, enter a synthetic follow-up in **Synthetic text or follow-up
question**. Paste full pseudonyms from an approved upload and mark any newly
sensitive text outside those markers. Prepare the local preview, review it, and
approve its local handoff. Questions containing only saved pseudonyms still need
preview and approval. Edits and cancellation invalidate approval; unlocking does
not restore or automatically submit a previous draft. Native ChatGPT input still
bypasses this harness.

Rust `prepare`/`prepare_batch` and WASM `prepare`/`prepareBatch` now preserve exact
committed full markers byte for byte, including repeated or adjacent references.
New findings share the same typed mapping registry as file uploads. Existing
markers stay pseudonyms in the preview and output; only newly supplied findings
produce transformation records. Offsets refer to the supplied input text, with
no intermediate restoration. The API signatures, marker bytes and saved schema
are unchanged.

Every well-formed marker in input text must be committed in the workspace's
snapshot. Unknown, foreign-workspace and uncommitted markers reject the entire
input/batch with `invalid_marker`. Damaged marker-like spans such as
`[[PERSON_7KQ]]` are ordinary text in new input, though they fail restoration. A new marker allocated by an
earlier member of the same pending batch is still uncommitted and cannot be used
as an input reference. Short display labels remain ordinary text, not marker keys.
Any supplied finding overlapping a marker, including a partial or enclosing span,
rejects with `invalid_finding` and an input-local finding index. Findings directly
adjacent to markers are allowed. Validation applies to every supplied finding
before selection/replacement.

Core owns marker validation for every text leaf, across all host input formats;
bindings contain no duplicate marker rules. Rejected preparations change no saved
mappings and expose no partial preview. A question using only existing aliases
leaves the mapping head unchanged but still has a single-use prepared plan. The
browser uses its existing approval, encrypted-save and activation sequence for
both changed and unchanged follow-ups. Detection remains supplied findings in
this synthetic workflow.

## Acceptance coverage

| Check | Evidence |
| --- | --- |
| A1 | Encrypted save, actual browser close/relaunch, explicit unlock, byte-identical alias reuse and exact restoration. |
| A2 | Preview cancellation and edits invalidate approval and commit/release nothing. |
| A3 | Invalid types/ranges/text/units, unregistered reserved-marker collisions and resource bounds reject atomically through installed WASM. |
| A4 | Real IndexedDB abort, lost save acknowledgment and browser termination after commit/before activation. Saved state survives; no automatic release/resend. |
| A5 | Two real writers at the same parent; lock/cancel at explicit pre/post-commit barriers. Only a winning authorized flow releases. |
| A6 | Invalid/unknown markers alongside a valid marker reject the whole restoration. |
| A7 | Single-use activation/discard, no-change consumption, close invalidation and repeatable pending export. |
| A8 | Wrong credentials, malformed KDF/envelope fields, tampered ciphertext/AAD, strict CBOR and shared uint64/overflow fixtures. |
| A9 | Real KDF/Web Crypto/storage, locked restart, rejected runtime-key export, no automatic console logging or external requests, and an Argon2id fixture from an independent native libsodium build. |
| A10 | Same local/CI command and pinned tool/browser dependencies; content-free artifact hashes and environment versions. |
| B1 | Whole-batch rejection, indexed errors, explicit removal, one approval/save and identical shared aliases after browser restart. |
| B2 | Changing batch membership invalidates the prior preview and approval; no partial or stale output is released. |
| B3 | Completing or cancelling a batch clears its temporary source inputs and findings. |
| C1 | Packaged CSV worker preserves decoded records and strings; malformed quoting, encoding errors and limits reject with content-free errors. |
| C2 | Real multi-file selection, local cell findings, preserved string tables, shared aliases and exact decoded-name restoration after browser restart. |
| C3 | Rejected files/cells block the whole selection; removal and cell corrections invalidate approval and require a new preview. |
| C4 | Completed/cancelled intake clears file controls, cells and selections; real FileReader cancellation leaves its source File unchanged. |
| E1 | Packaged PDF worker extracts page text and preserves numeric strings; mixed-format review verifies decoded Unicode, page bounds and rotation. |
| E2 | Scans, hybrid image/text pages, encryption (including empty user passwords), unsupported features, malformed/repaired PDFs and file/page/decoded-text limits reject with content-free errors. |
| E3 | CSV/PDF selections share aliases; PDF corrections invalidate approval. A scan blocks the whole batch until explicit removal and a fresh preview. |
| E4 | Completion/cancellation clears PDF review data; saved aliases restore and are reused after browser restart. Cancelling a pending parser request leaves its original File unchanged. |
| F1 | Approved upload → browser restart/unlock → reviewed Unicode follow-up with old aliases and a new name → exact approved output → restart and restoration. Alias-only questions still require approval without advancing mappings. |
| F2 | Damaged, foreign and cancelled-preview markers block the complete question. Selecting an existing marker is rejected; edits/cancellation invalidate approval and preserve committed aliases. Native/installed tests also cover partial/crossing overlaps and rejection of a marker allocated earlier in the same pending batch. |

`scripts/workspace-conformance.mjs` is reused by the installed-package and extension tests. `fixtures/workspace/counters.json` is also consumed by native Rust tests. These test fixtures contain only public synthetic values. The Argon2id fixture records its native reference provenance; Python/native libsodium is not a test runtime dependency.

The harness exports narrow test fault controls for explicit transaction barriers/aborts. They do not replace the normal encryption or storage path and must not be copied into a shipping extension. No traces, screenshots, source statements, credentials or workspace dumps are uploaded. CI artifacts contain code hashes and environment metadata only.

## CI and remaining qualification

`.github/workflows/workspace-integration.yml` runs both the installed package suite and the extension suite on Linux and macOS Chromium. Existing Rust fmt, Clippy and all-feature tests remain required. Branded Chrome/macOS installation and broader product qualification still require separate evidence; a Chromium pass is not a claim about all Chrome versions.

Once this workflow is on the default branch, configure **Workspace integration (ubuntu-latest)** and **Workspace integration (macos-latest)** as required checks in GitHub's branch rules. YAML alone does not enforce branch protection. Do not enable missing checks against other PRs before the workflow lands. Remote branch rules are not changed by this implementation.

Real ChatGPT handoff, merges/separation, backup/recovery, the fuller session cache, remembered unlock and new Python/Node workspace bindings are deferred as specified in the design. The local sink stands in for an approved downstream handoff; it makes no claim about delivery to an external service.

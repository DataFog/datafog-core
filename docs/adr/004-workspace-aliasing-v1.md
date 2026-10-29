# ADR 004: Workspace Aliasing V1 Decisions

- **Status:** Accepted
- **Date:** 2026-10-27

## Context

Workspace aliasing gives each sensitive value a persistent pseudonym, kept in an
encrypted local vault, so model output can be restored to real names. In V1 a
person pastes free text into the DataFog Chrome extension. The extension
pseudonymizes it, the person sends it to ChatGPT, and the answer is restored to
real names on their machine. Follow-up questions keep the same pseudonyms.

The PRs that follow this one build that base on synthetic data: Core aliasing, the
CBOR snapshot, the WASM workspace API, follow-up text, restoration with an issue
report, an encrypted browser vault, backups and a browser-restart test.

Issued markers become permanent once real text is saved in a workspace. Anything
later stages need from a workspace (file intake, native app migration, sync, more
chat surfaces) has to be recorded from then on. This ADR settles the V1 decisions
and the snapshot contents that must exist before that point.
[workspace-v1-plan.md](../workspace-v1-plan.md) sequences the work.

## Decision

### Scope

Free-text paste is the V1 baseline: text in, pseudonymized text out, answer
restored. File upload is an open question that the extension investigation
answers:

- CSV is the most tractable format and the likely first file type.
- Excel is out, because of formulas and linked cells.
- PDFs split into machine-readable ones, which may follow, and ones that need
  OCR, which are out.
- An unsupported file type shows a clear "not supported" message. It is never
  partially processed.

### Markers

Markers are `[[LABEL_CODE]]`, for example `[[PERSON_7KQ4ZM]]`.

- `LABEL` is the uppercase ASCII entity type, at most 32 bytes.
- `CODE` is six Crockford base32 characters (`0-9A-Z` without `I L O U`),
  unique within the workspace. Codes are derived by HMAC-SHA256 from the
  host-random operation ID and re-drawn on collision.
- The workspace namespace and issuing operation ID stay in the vault and never
  go on the wire.
- Restoration ignores case and accepts Crockford's `I`/`L` → `1` and `O` → `0`
  substitutions in the code. It never ignores a wrong label.

Six characters, not four: a backup restore loses every code issued since that
backup, and new codes are drawn without knowing them. With about 1,000 codes per
type lost and 1,000 issued afterwards, four characters (about one million codes)
make a silent wrong restoration likely. Six characters (about one billion) make it
about 0.1%. The same space makes a code the model invents almost never resolve.
Putting the full workspace and operation IDs in each marker would give the same
protection at about 40 tokens per marker.

Before the snapshot freeze, the marker preservation and token tests run on
ChatGPT, Claude and Gemini, not only ChatGPT, so later surfaces do not need a
second format.

### Restoration

Restoration is all-or-nothing. `restore_report` returns text only when every
marker resolves. Otherwise it lists each blocking span as `unknown`, `damaged` or
`unbracketed`, with byte and UTF-16 ranges. A committed code whose brackets were
dropped blocks restoration rather than passing through as text.

### Identity

A value's identity is its entity type plus the value lowercased, trimmed and with
whitespace runs collapsed. The first spelling seen is stored and restored.
Punctuation and other differences go through merges.

### Merges

A merge is a record from one code to a canonical code. It never rewrites or
deletes an entry, because the merged code may already be in past chats. New
text uses the canonical code; the merged code keeps restoring, to the canonical
display value. Merges are not required for the V1 paste flow. When they ship,
this is their shape, and undoing a merge deletes the record.

### Types, rules and remembered choices

| Type | V1 rule |
|---|---|
| `PERSON`, `FIRST_NAME`, `LAST_NAME` | Hidden by default; generic keep forbidden |
| `ADDRESS`, `STREET_ADDRESS` | Hidden by default; generic keep forbidden |
| `EMAIL`, `PHONE` | Hidden by default; generic keep forbidden |
| `ACCOUNT_NUMBER`, `CREDIT_CARD`, `SSN`, `US_ROUTING_NUMBER` | Hidden by default; generic keep forbidden |
| Credentials (`API_KEY`, `BEARER_TOKEN`, `JWT`, `PRIVATE_KEY`, `CREDENTIAL_URI`) | Hidden by default; generic keep forbidden |

Protected types stay hidden by default. Sid approved one narrow exception on
2026-10-28: an explicit per-value `not_sensitive` false-positive correction may
leave the exact existing normalized `(entity type, value)` visible. Core cannot
verify that the correction is truthful, so the person can deliberately override
actual PII. Generic `keep` remains forbidden for protected types. Finding source
and needs-review flags do not grant an exception; no field/reference disclosure
authorization is introduced. Narrower or differently typed corrections do not
waive a wider protected span. Previously issued aliases stay permanent.

Per-value choices are remembered in the encrypted workspace: permitted keep
(send as is) or explicit not sensitive (a scanner false-positive correction). Type rules, remembered choices and
finding sources (`scanner`, `recall`, `user`, plus a needs-review flag) live in
Core. Hosts display the decisions; every preparation surface applies the same
rules. No sensitive plaintext is kept in extension storage; the encrypted
workspace carries aliases, rules and remembered choices.

### What leaves the device

Only text the person approves leaves the local workflow. Remaining protected
findings are replaced by markers; explicitly permitted exact-value choices can
remain visible, including a protected value marked `not_sensitive`.
Nothing from the vault is sent. Where the person pastes is the extension's design
choice. Text pasted straight into ChatGPT's message box is already in ChatGPT's
page before DataFog replaces it, so a paste target inside the extension is the
safer default.

### CSV columns, if CSV is in V1

A column whose header names a sensitive field (for example `email`, `address`,
`phone`, `name`) is flagged as a whole for the person to approve. Core already
discovers name fields by key in structured JSON (`discover_fields`, ADR 002, for
`PERSON` only). CSV support reads rows into records keyed by header and extends
the aliases to the other types. Approved columns are pseudonymized cell by cell
through the same workspace, so the same value gets the same marker in pasted text
and in a file.

### Detection for V1

Rules catch emails, phones, card numbers, SSNs, routing numbers and credentials
well. Names and street addresses in free text are the gap: rules miss most of
them.

0.5.0-beta.1 provides the optional English GLiNER2.5 Small browser adapter through
`@datafog/wasm/models`, with the pinned ONNX export documented in
[browser-model.md](../browser-model.md). Its model and tokenizer files total
about 297 MB and are provisioned separately from the npm package. The existing
synchronous scanner remains rule-based. The native GLiNER2 PII checkpoint and its
count-head fix are a separate adapter; their accuracy numbers do not qualify Small.

The DataFog PII model remains a native option. The shared engine construction and
registry work is tracked in issues #124 and #125; Decima is a separate contributed
engine proposal. Community model accuracy regressions are warn-only, while model
loading, asset identity, valid offsets and integration behavior remain blocking
contract checks.

The fallback path, when no model is available or when the model misses a value,
is workspace recall plus the person's own selection. Workspace recall catches
every value they approved before; the person's selection catches anything the
scanner and recall both missed. Recall targets `0.5.0-beta.2`; `0.5.0-beta.1` supports the
scanner and explicit user selection without claiming that recall is implemented.

**Detection bar (G2).** The synthetic pasted-text set, once it exists, should
measure:

- No missed emails, phone numbers, card numbers, SSNs or account numbers (rules).
- At least 90% of names caught before review, with no recall, using the shipping
  browser model's `person` prompt at its documented raw-score threshold.
- Address recall is an open measurement. Detection fixtures and evaluation are proposed
  in PR #115 and must be reviewed before pilot qualification. Small's `ADDRESS`
  is a full-address span, distinct from the street-line `STREET_ADDRESS` gold
  labels. Define the scoring alignment and measure the actual browser adapter;
  do not reuse native GLiNER2 figures as Small qualification.
- Every previously approved value caught by recall in later text.

### Workspace snapshot

The beta.2 policy draft (#145) introduces candidate version 2 with actual policy
and choices at keys 6/7; version 1 had only a fixed `synthetic_v1` placeholder at
key 6. Sid approved byte-exact v1 read/export/no-op behavior and an explicit saved
upgrade on a real mutation/recovery. Before approval/save the host must explain
that beta.1 cannot open an upgraded vault. Discard and failed saves do not upgrade;
old backup files stay unchanged. New workspaces bootstrap version 2.

The [policy/choice proposal](../proposals/workspace-policy-choices.md) defines that
candidate layout. The separate #162 operation draft must rebase/version coherently
rather than ship a second meaning of version 2. Keys 8–10 below are future target
contents, not implemented policy-v2 fields. Snapshot freeze remains the pilot gate.
Target contents:

| Key | Contents | Lands with |
|---|---|---|
| 0 | Schema version | Workspace PR |
| 1 | Workspace namespace | Workspace PR |
| 2 | Head: revision, commit ID | Workspace PR |
| 3 | Entries: type, display, code, issuing operation, counter | Workspace PR |
| 4 | Per-label counters | Workspace PR |
| 5 | Merges: from code, to code, operation | Merges (after V1) |
| 6 | Policy profile and explicit type rules | Beta.2 policy draft |
| 7 | Remembered choices: type, display, disposition, operation | Beta.2 policy draft |
| 8 | Operations: ID, kind, time | Provenance |
| 9 | Conversations: surface, the site's own ID, created operation, issued codes | Provenance |
| 10 | Content-free counts | Provenance |

Key 5 is reserved now so that adding merges after V1 needs no migration. The
backup carries the whole snapshot, so a complete snapshot is a complete backup.
Its round-trip test must cover every key before the freeze.

### Repositories and interfaces

- **Extension repo.** The V1 Chrome extension lives in its own repository.
  `examples/workspace-extension` stays in Core as a non-shipping conformance
  harness and gets no new product features.
- **SDK contract.** WASM is the extension boundary. Beta.3 adds native in-process
  Python ([#167](https://github.com/DataFog/datafog-core/issues/167)) and Node
  ([#168](https://github.com/DataFog/datafog-core/issues/168)) workspace APIs;
  fail-closed cross-SDK parity ([#171](https://github.com/DataFog/datafog-core/issues/171))
  and G3 ([#154](https://github.com/DataFog/datafog-core/issues/154)) freeze all
  SDKs together with the supported snapshot versions.
- **Streaming and server hosts.** Beta.3 streaming restoration
  ([#169](https://github.com/DataFog/datafog-core/issues/169)) requires a reviewed
  atomicity contract before implementation: progressive output cannot preserve
  whole-response all-or-nothing behavior after release. Server qualification
  ([#170](https://github.com/DataFog/datafog-core/issues/170)) covers durable
  complete-head CAS, tenant/request ownership and worker retries. Stateless
  workers require shared durable mappings or authenticated client-carried
  encrypted snapshots; chat history alone cannot reconstruct mappings. G4
  ([#153](https://github.com/DataFog/datafog-core/issues/153)) includes server
  keys/storage. These additions do not expand beta.2 scope.
- **Vault interface.** The extension puts IndexedDB and Web Crypto behind one
  interface: open, save-if-head-matches, lock, export, import. CI tests through
  it, and a later native app replaces it with a Keychain-backed module.
- **Workspaces per vault.** A vault holds a list of workspaces, even while the V1
  UI creates one.

### Real data

Real text goes into a workspace only after the vault encryption review. Before
that, testing with real data uses a throwaway browser profile, or a build that
saves no mappings.

### Out of V1

- the statements product (below)
- Excel, OCR and other file types, pending the file-upload investigation
- remembered 7/30-day unlock
- sites other than ChatGPT
- merges and undoing them
- a user-facing event-log product (the beta.3 minimal operation records remain in scope)
- lossless operation-history compression ([#165](https://github.com/DataFog/datafog-core/issues/165)); retain every operation record

### Deferred: the statements product

Statements (PDF bank and credit-card statements → ChatGPT → a real-name
spreadsheet) are a longer-tail product, possibly a dedicated web product rather
than the extension. A three-month test of the extension on real usage will
measure the gap. These decisions are parked with it, not discarded:

- Only a statement's transaction rows are sent; headers and footers are dropped.
- Core takes statement rows (date, description, amount) and scans only the
  description.
- Merchants are kept by default, and one click turns one into a category alias
  such as `[[GROCERY_7KQ4ZM]]`.
- Statement rules: masked account numbers ("ending in 1234") and transfer payees
  ("ZELLE TO …").

Deterministic pseudonyms for the same vendor across uploads already exist: the
workspace gives a value the same marker every time.

## Consequences

- Marker overhead drops from about 40 tokens to a handful. The tests above
  confirm this on each model.
- After a backup restore, a new code repeats a lost one with about a
  one-in-a-billion chance per pair, rather than never. Conversation records add a
  second check.
- Spellings that differ by case or spacing need no merge.
- Name and address detection in prose depends on the model spike more than it
  did for statements.
- The extension depends on a released or pinned `@datafog/wasm`, so Core changes
  reach it through the versioned contract rather than shared source.

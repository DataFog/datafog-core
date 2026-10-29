# Synthetic workspace integration slice

This non-shipping Manifest V3 harness starts from DataFog Core 0.4.4 and proves:

**supplied findings → local preview/approval → encrypted atomic save → activation → browser restart/unlock → identical alias reuse → exact local restoration.**

The second slice adds atomic batches of synthetic text. Mark a text's selections,
choose **Add text to batch**, and repeat. **Prepare local preview** includes queued
inputs and any current draft. A rejected member blocks the entire selection and
identifies its input number. Explicit removal invalidates the old preview; prepare
and approve the remaining selection again. Inputs are kept separate in the Core
preview and broker result; numbered sections are display formatting only.

Use synthetic text only. The workspace API, development CBOR schema and marker format are provisional. Do not put real data in this harness or treat it as the completed privacy extension.

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

To inspect the UI separately, run `npm run build --prefix examples/workspace-extension`, load `examples/workspace-extension/dist` as an unpacked extension in Chrome's extension developer settings, and click its toolbar action. Use a disposable browser profile. Closing the owning extension page drops its runtime-only unlock; restart requires an explicit password or recovery-code unlock. Save the one-time recovery code offline when creating a vault.

## Ownership and contract

- `crates/core/src/workspace.rs` owns exact typed identity, marker allocation, validation, pending-change lifecycle and restoration. `workspace/codec.rs` owns the bounded, canonical CBOR development schema. Existing selection/overlap and offset machinery is reused.
- Markers are `[[LABEL_CODE]]`: the uppercase entity type (at most 32 bytes) and a random six-character Crockford base32 code, unique within the workspace, for example `[[PERSON_7KQ4ZM]]`. Restoration ignores case and accepts Crockford's `I`/`L` → `1` and `O` → `0` substitutions in the code; any other marker-like span fails the whole restoration.
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

## Combined review

The reference host now prepares manual selections through `prepareReview`, with
`userSelections` kept separate from model `scannerFindings`. Core adds committed
recall, validates every supplied claim before policy, merges duplicate display
origins, and blocks unresolved protected overlaps with original ranged
`review_conflict` errors. The source view highlights conflicts; edit and prepare a
fresh review before approval. Failed save cannot hand off or activate a mapping.
Read-only review never returns sanitized handoff text or creates a plan.

Legacy SDK `prepare`/`prepareBatch` still use explicitly supplied findings and now
reject unsafe supplied overlaps. Omitted findings are never a clean-text verdict.
The new reviewed path must also be integrated in the shipping extension (#132);
this synthetic harness is not that integration.

## Crypto and persistence

The host uses AES-256-GCM (fresh 96-bit nonces, 128-bit tags), random vault/workspace keys, and explicit libsodium Argon2id13: 16-byte salt, 32-byte result, 64 MiB and opslimit 3. Only the bounded development KDF profile is accepted; there is no weaker fallback. Password and workspace key wrappers have separate authenticated contexts. The snapshot AAD binds its vault, workspace, key ID and complete commit head.

Passwords and raw key buffers are cleared where possible, and runtime Web Crypto keys are nonextractable. JavaScript/native memory erasure is best effort; this does not provide hardware-bound key storage or protection from malicious extension code. IndexedDB transaction completion and requested strict durability do not establish a guarantee against every OS/power failure. Authenticated snapshots do not prove that no newer history exists.

**Backup export security:** Exported backup files carry the password-wrapped root key and can be brute-forced offline at the current Argon2id profile (64 MiB memory, 3 iterations). This profile is tuned for browser KDF latency and is not designed to resist sustained offline attacks. Whether exports should use a stronger (slower) KDF profile is an open question for the planned encryption review.

Initial limits are **development guardrails**, not production capacity claims: 1 MiB source, 10,000 findings, 20,000 entries and 16 MiB bundle/output. Finding transport is also bounded before JSON serialization, including repeated object references. Exceeding limits rejects the whole operation. Full-year capacity, final marker preservation/overhead and production security review remain later qualification.

Batches accept 1–32 inputs. Source bytes, findings and preview output limits apply
to the aggregate selection. The WASM batch JSON transport is bounded to 16 MiB.
This slice uses manually supplied synthetic text and findings.

## Protected follow-up questions

After unlocking, enter a synthetic follow-up in **Synthetic text or follow-up
question**. Paste pseudonyms from earlier approved text and mark any newly
sensitive text outside those markers. Prepare the local preview, review it, and
approve its local handoff. Questions containing only saved pseudonyms still need
preview and approval. Edits and cancellation invalidate approval; unlocking does
not restore or automatically submit a previous draft. Native ChatGPT input still
bypasses this harness.

Rust `prepare`/`prepare_batch` and WASM `prepare`/`prepareBatch` now preserve exact
committed markers byte for byte, including repeated or adjacent references.
New findings share the same typed mapping registry as earlier inputs. Existing
markers stay pseudonyms in the preview and output; only newly supplied findings
produce transformation records. Offsets refer to the supplied input text, with
no intermediate restoration. The API signatures, marker bytes and saved schema
are unchanged.

Every well-formed marker in input text must be committed in the workspace's
snapshot. Unknown, foreign-workspace and uncommitted markers reject the entire
input/batch with `invalid_marker`. Damaged marker-like spans such as
`[[PERSON_7KQ]]` and bare committed codes are rejected before approval with safe
issue ranges, so an unchanged round trip cannot fail later on those spans. A new marker allocated by an
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

## Encrypted backup and recovery

New synthetic vaults have a version-2 envelope with independent password and
recovery wrappers for the same random vault root key. The password slot retains
the bounded Argon2id profile above. The recovery code is `DFR1.` followed by the
canonical unpadded Base64url encoding of 32 random bytes. Native Web Crypto
HKDF-SHA-256 derives its AES-256-GCM wrapping key using a separate 16-byte salt and
the versioned `DFG2:RECOVERY:HKDF:` context bound to the vault/key IDs. The recovery
wrapper authenticates its version, salt and vault/key identity. No slow KDF or
password fallback is used for the uniformly random recovery secret.

Creation shows the code once in extension-owned UI. Save it offline, then choose
**I saved the code; hide it**. Lock, cancel and credential operations clear the
display. The code is never persisted or retained in the broker's unlocked session.
Either credential can explicitly unlock a new vault. Losing both makes its
ciphertext unrecoverable. Credential rotation/reset is separate later work.
Version-1 synthetic vaults retain their password-unlock path but have no recovery
slot; backup export rejects them with `recovery_unavailable`. This slice does not
upgrade those explicitly disposable development vaults. From the first real-use
pilot, format upgrades must preserve mappings, markers and privacy settings through
supported migrations.

**Export encrypted backup** requires a current unlocked workspace and downloads a
`.dfbackup` file containing only the committed encrypted bundle and export-time
credential/key wrappers. A pending preview is not included. Export needs no
plaintext password or recovery code. Files exclude source documents, extracted
text, drafts, handoff/restoration output, runtime keys and local access generations.
The Core bundle carries the supported synthetic privacy configuration together
with mappings; merges and remembered correction rules remain later slices.

Backup version 1 uses bounded, canonical UTF-8 JSON with unpadded Base64url binary
fields. Versions, random identifiers, the saved head and export timestamp are
public; mappings and privacy settings remain encrypted. An additional AES-GCM tag
over the `DFGB1:AUTH:` context and complete canonical payload authenticates the
metadata and both credential slots, including whichever slot was not used to
unlock. It uses the workspace key and a fresh 96-bit nonce. Parsing is capped at
24 MiB, with the existing 16 MiB plaintext bundle limit and fixed accepted KDF
profile checked before derivation. Unknown versions/fields, duplicate or
noncanonical JSON/Base64url encodings, invalid lengths and failed authentication
block the whole import with content-free errors.

Choose a backup and its export-time credential, then **Review backup locally**.
Core validates the decrypted CBOR and its companion configuration; the broker
checks its namespace and complete head against the authenticated envelope.
The local preview shows the workspace tag, export time and saved revision. Time
describes this copy and does not prove freshness: later mappings/settings may be
absent. Only **Approve restore into this empty profile** permits persistence.
Any existing vault is a conflict, even for another copy of the same workspace;
there is no automatic merge, overwrite or exact-retry exception. A conflict
preserves the stored ciphertext, keys, active session and existing privacy plan.
This single-workspace harness does not yet import missing IDs into an occupied
multi-workspace vault.

Rust `prepare_recovery` and WASM `prepareRecovery` stage a fresh operation ID and
checked revision under the existing immutable-plan contract. Recovery has no text
previews or handoff permission. Issued markers, exact originals, counters and
supported privacy settings retain their saved meanings. The Core CBOR schema and
marker format are unchanged. Missing post-backup markers remain invalid: codes
are drawn from the fresh operation ID, so a code issued after recovery repeats a
lost post-backup code only by chance (about one in a billion per pair).

For an empty destination, retain the backup's vault identity and export-time
credential slots, generate a fresh local access generation, and encrypt the
candidate under a new random workspace key, key ID and nonces. No existing vault
keys are replaced. One real IndexedDB absence-check/write transaction commits the
vault, wrapped workspace key and candidate together. Wait for transaction
completion before consuming the Core plan. Import then closes the runtime and
requires explicit fresh unlock; it restores no session/remember capability,
privacy approval or delivery queue. Edits, cancellation and lock discard staged
imports. Before-commit failures write nothing. After-commit interruption or a
lost acknowledgment leaves the durable workspace locked and sends nothing;
reopen it explicitly instead of automatically importing again. Temporary decoded
buffers are cleared best effort and staged workers are terminated.

Backups retain export-time credentials even if credential-management support
later changes the live vault. The native-crypto test independently verifies the
HKDF/AES-GCM backup authentication and different recovered workspace key. The
browser tests use actual downloads and fresh persistent profiles, with no
plaintext backup or credential artifacts uploaded to CI. This remains a synthetic
workflow proof; production security and real-use format qualification are later
work.

## Acceptance coverage

| Check | Evidence |
| --- | --- |
| A1 | Encrypted save, actual browser close/relaunch, explicit unlock, byte-identical alias reuse and exact restoration. |
| A2 | Preview cancellation and edits invalidate approval and commit/release nothing. |
| A3 | Invalid types/ranges/text/units, well-formed marker collisions and resource bounds reject atomically through installed WASM. |
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
| F1 | Approved input → browser restart/unlock → reviewed Unicode follow-up with old aliases and a new name → exact approved output → restart and restoration. Alias-only questions still require approval without advancing mappings. |
| F2 | Unknown, foreign and cancelled-preview markers block the complete question. Selecting an existing marker is rejected; edits/cancellation invalidate approval and preserve committed aliases. Native/installed tests also cover partial/crossing overlaps and rejection of a marker allocated earlier in the same pending batch. |
| G1 | Actual backup download, independent password/recovery-code imports into fresh profiles, locked restart, exact alias reuse/restoration, one-time code cleanup and absence of plaintext secrets/drafts in persistent records or exported files. |
| G2 | Occupied destinations preserve storage, active plans and sessions; malformed/oversized files, tampered metadata or either credential slot, wrong credentials and authenticated invalid Core snapshots reject without writes or partial previews. |
| G3 | Recovery stages a new complete head and fresh workspace encryption material. Old known aliases stay identical, missing later aliases fail, and reused counters cannot redirect old markers. Native Node HKDF/AES-GCM independently confirms backup authentication and a different destination data key. |
| G4 | Import edits, lock/cancel during decrypted staging and before/after commit, actual IndexedDB rollback/lost acknowledgment and two competing imports preserve atomicity and require fresh unlock without releasing content. |

`scripts/workspace-conformance.mjs` is reused by the installed-package and extension tests. `fixtures/workspace/counters.json` is also consumed by native Rust tests. These test fixtures contain only public synthetic values. The Argon2id fixture records its native reference provenance; Python/native libsodium is not a test runtime dependency.

The harness exports narrow test fault controls for explicit transaction barriers/aborts. They do not replace the normal encryption or storage path and must not be copied into a shipping extension. No traces, screenshots, source statements, credentials or workspace dumps are uploaded. CI artifacts contain code hashes and environment metadata only.

## CI and remaining qualification

`.github/workflows/workspace-integration.yml` runs both the installed package suite and the extension suite on Linux and macOS Chromium. Existing Rust fmt, Clippy and all-feature tests remain required. Branded Chrome/macOS installation and broader product qualification still require separate evidence; a Chromium pass is not a claim about all Chrome versions.

Once this workflow is on the default branch, configure **Workspace integration (ubuntu-latest)** and **Workspace integration (macos-latest)** as required checks in GitHub's branch rules. YAML alone does not enforce branch protection. Do not enable missing checks against other PRs before the workflow lands. Remote branch rules are not changed by this implementation.

File intake, real ChatGPT handoff, merges/separation, credential rotation, multi-workspace import, the fuller session cache, remembered unlock and new Python/Node workspace bindings are deferred as specified in the design. The local sink stands in for an approved downstream handoff; it makes no claim about delivery to an external service.

Source marker preflight blocks preparation before approval when literal marker-like
text would fail an unchanged round trip. The harness highlights source issue ranges;
correct the input and prepare a fresh preview. Remove and re-add a queued input to
correct it. Ordinary text such as `[[my note]]` remains ordinary text; full committed
markers pass through, while bare committed codes require their brackets.

Successful WASM activation releases retained plans based on an older workspace
head. Those handles subsequently return `inactive_plan`. No-op activation preserves
other current-head plans, and failed activation changes neither mappings nor plan
capacity. Hosts must still explicitly discard unused current-head plans.

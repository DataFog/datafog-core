# Synthetic native Python host qualification (W6)

Work in progress; not a reviewed production server or a LiteLLM adapter.
This example qualifies the actual installed native Python SDK against encrypted
SQLite snapshots and competing independent worker processes. It never infers a
mapping store from chat history. The response proof includes strict buffered
restoration and a separately selected progressive text-channel transport seam.

The application must authenticate and authorize a principal before selecting a
16-byte tenant/workspace identity or resolving a host key. The synthetic harness
passes a fresh 32-byte AES key explicitly from its parent process to workers;
keys are never stored beside ciphertext, embedded in a bundle, printed or loaded
from a committed file. This demonstrates ownership, not a production KMS or
process-memory isolation guarantee. G4 must review deployment key/storage policy.

The host-specific envelope binds its domain/version, tenant/workspace/key IDs and
complete head as authenticated associated data. AES-256-GCM uses a fresh random
12-byte nonce for every encryption. Authentication completes before Core sees
plaintext. These choices follow [PyCA's AEAD documentation](https://cryptography.io/en/50.0.2/hazmat/primitives/aead/).

SQLite stores encrypted snapshots and public routing/head metadata only. A
conditional update compares both parent revision and commit ID within a real
transaction before acknowledging the saved candidate; separate processes share
that database, not a Python handle or an in-memory lock. SQLite documents its
[transaction semantics](https://www.sqlite.org/lang_transaction.html) and
[conditional updates](https://www.sqlite.org/lang_update.html).

The bounded qualification pool has two workers, one active request/workspace
handle per worker and Core-binding limits of 32 pending changes per handle.
That bounds retained candidates to 64 in this pool. It is not a Core global
counter or an admission guarantee across additional application replicas. A
production host must impose worker/queue/request limits and cancellation policy
at its own admission boundary.

A successful save is necessary before activation and handoff. Failed saves,
cancelled operations and uncertain acknowledgments produce no released text.
A postcommit failure can leave valid durable mappings; reopen and obtain fresh
review/approval rather than automatically resending text. A stateless worker
reopens the authorized encrypted durable snapshot on each request; missing state
fails rather than guessing unknown markers from resent chat history.

Nonstreaming response restoration uses the server-owned request/tenant/workspace
correlation and only the explicit text field. Synthetic tool envelopes stay
unchanged. Provider translation, full R4 integration, cross-runtime X8 and G4 security
review remain separate gates. All fixtures are invented and the proof makes no
real-data-pilot or production approval claim.

CAS protects concurrent saved-state updates, while AEAD authenticates each stored
record. This example does not detect restoration of an entire older authorized
database file: deployment rollback protection and key rotation require the G4
storage/key review. Python caller copies and process memory are not promised
cryptographic erasure.

## Run the installed proof

```sh
cargo dfv build --sdk python
python3 -m venv target/python-host-venv
target/python-host-venv/bin/python -m pip install --require-hashes -r examples/native-python-host/requirements.txt
target/python-host-venv/bin/python -m pip install target/wheels/*.whl
target/python-host-venv/bin/python examples/native-python-host/test_host.py
```

The receipt at `target/python-host-proof.json` contains the source commit, clean
state, installed native binary hash, input hashes, versions and test count. It
contains no fixture text, keys or plaintext snapshots. Separate SQLite processes
use the actual installed wheel; no Python substitute implements workspace logic.

A real saved v1-to-v2 mutation also requires the application's explicit upgrade
notice acknowledgment: tell the person before approval that `0.5.0-beta.1` cannot
open the upgraded vault. The fixture's `approved` and `acknowledge_upgrade` flags
represent that separate user review; they are not automatic approvals supplied by
Core. The host rejects a missing acknowledgment before any durable change.

Assign a fresh server-generated request ID for each outbound attempt and bind its
expected context to that request's trusted transport callback. Provider text and
client-supplied IDs do not establish authorization. The synthetic retry fixtures
use fresh request contexts and Core operation IDs; deployment registries must also
expire/revoke cancelled request contexts before dispatching late responses.

## Streaming text-channel proof

`restore_text_chunks` defaults to strict buffering and acknowledges one atomic
restored event only after EOF. `progressive=True` must be an actual boolean to
opt into irreversible output. It receives already-separated UTF-8 text bytes;
provider framing, tool channels, SSE parsing and a LiteLLM middleware are outside
this synthetic seam. Trusted server-owned request contexts are checked before
any input iterator is consumed.

The host tracks `accepted` counts only after its transport callback returns.
These are callback acknowledgments, not proof that a remote user received text.
Core's `delivered` counts describe output returned to the native caller. They may
exceed host acceptance if cancellation happens after restoration but before the
transport callback; failures report both separately with `complete=False`.
A failed callback may have written externally before losing its acknowledgment,
so `delivery_uncertain` never retries or silently resumes. Prior acknowledged
chunks cannot be retracted. Only an acknowledged final event reports completion.
A cancellation after that final acknowledgment cannot retract observed completion.

Installed tests cover late invalid markers, atomic strict output, progressive
success, cancellation before restoration/between restoration and acceptance/after
partial acceptance/before final acknowledgment, request isolation and uncertain
transport writes. Response restoration never changes the encrypted snapshot.
These observations qualify this host seam only; they do not approve a production
transport, deployment rollback policy or provider integration.

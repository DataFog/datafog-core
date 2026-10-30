# Optional native Python Core–FogVault owner

`datafog_fogvault` is a separate, unpublished `0.0.0` ABI3 extension for Python
3.10+. It delegates persistence, session authority and save/activation to the
existing Rust `datafog-fogvault-native::Owner`; it does not load browser WASM,
duplicate encryption, or change the existing `datafog_core` Python package.
Its qualification build uses Rust 1.94.1, while Core keeps its Rust 1.88 floor.

## Create, save and reopen

```python
import secrets
from datafog_fogvault import NativeWorkspace

tenant = "private-routing-id"  # Host authentication/authorization is separate.
record_id, namespace = secrets.token_bytes(16), secrets.token_bytes(16)
owner = NativeWorkspace.create_new(
    tenant, protected_control_path, vault_path, password_bytes,
    record_id, namespace, secrets.token_bytes(16), secrets.token_bytes(16),
)
text = "Email alice@example.test"
finding = {"entityType": "EMAIL", "matchedText": "alice@example.test",
           "range": {"unit": "utf8_bytes", "start": 6, "end": len(text)}}
plan = owner.prepare_review(tenant, [{"text": text,
    "scannerFindings": [finding], "userSelections": []}], secrets.token_bytes(16))
show_local_review(plan.previews(tenant))
# The application must obtain approval of this exact still-current review.
saved = owner.commit(tenant, plan, True, secrets.token_bytes(16))
show_approved_copy_surface(saved["previews"][0])
owner.close(tenant)

owner = NativeWorkspace.open_existing(
    tenant, protected_control_path, vault_path, password_bytes,
    record_id, namespace, expected_head=saved["head"],
)
assert owner.restore(tenant, saved["previews"][0]) == text
owner.close(tenant)
```

Review/copy functions and credential input are host application code. Sources,
returned strings and all foreign copies remain the application's cleanup duty.
No method sends a message or proves remote delivery.

The host must establish an independently protected control namespace, separate
from the attacker who may replace vault files. Two different paths alone do not
provide that isolation. `create_new` requires a new control namespace and new
vault; it never silently opens existing files. `open_existing` requires both
existing stores. Failed creation can leave an empty newly created control
namespace; no existing vault is deleted or overwritten to repair it.

This candidate supports **password-only** admission with the existing baseline
KDF, 16 MiB payloads, 88-byte binding metadata and 64 records. Those are explicit
host admission settings, not a hardware guarantee. `profile` defaults to
`password_only`; other profiles reject `unsupported_profile` before creation.
Device/provider and recovery-factor custody are not implemented by this Python
binding and never fall back silently to passwords. The records still use the
shared `DFVBRG01` namespace/record/complete-head binding.

## Values, ownership and failure

- Every instance/plan/stream method requires the exact original opaque tenant
  string, and executes only on its creator OS thread. Invalid routing returns
  `wrong_tenant`; wrong-thread use returns `wrong_thread`, without PyO3's
  unsendable-object panic. Tenant strings are routing assertions, not proof of
  authentication; the host owns tenant-to-path authorization.
- No raw Workspace, FogVault, plaintext bundle export or prepared-plan import is
  exposed. Plans belong to one owner; foreign or consumed plans reject. Lock,
  close and owner garbage collection revoke retained plans/streams. Calls are
  synchronous on the calling Python thread and hold the GIL.
- Approval must be the literal Python `True`, not `1`, truthy objects or strings.
  A known rejected approval does not save. Confirmed save precedes activation;
  only successful `commit` returns approved handoff text.
- IDs are exact, nonzero 16-byte Python `bytes`. `WorkspaceHead.revision` is a
  lossless Python `int`; construction rejects bool, float, string, negative and
  overflowing values. Commit IDs are exact 16-byte bytes, with no lossy numeric
  transport. Heads are immutable. Passwords are bytes, never normalized strings.
- Nested findings/review/operation/conversation dictionaries use the current
  Core camelCase fields and zero-based, end-exclusive byte/code-point/UTF-16
  ranges. The findings transport accepts exact built-in dict/list/scalar values,
  not arbitrary objects/accessors. It bounds depth to 32, dictionary keys to 100,
  list entries to 100,000, total nodes to 1,000,000 and logical strings/nodes to
  16 MiB. Core applies its own semantic and resource bounds afterward. Bytes in
  dictionaries represent only fixed 16-byte IDs; bundle/feed bytes have separate
  bounded transports. These logical bounds are not a peak-memory SLA.
- `FogVaultWorkspaceError` exposes a content-free `code`. Core failures retain
  finding/input indexes, marker ranges and review conflicts. An
  `OutcomeUnknown` or `saved_but_not_activated` error includes its exact
  `request_id` bytes. Error messages do not copy passwords, paths, source text
  or raw provider details.

`reconcile(tenant, request_id)` reconciles the retained owner without reactivating
Core. After process restart, `NativeWorkspace.reconcile_existing(tenant,
control_path, vault_path, request_id)` returns only an exact receipt from the
registered vault; it grants no decrypted Core owner or plaintext. Authenticate
and `open_existing` afterward, then prepare and approve fresh work. Never infer
rollback, automatically replay an old approved mutation or resend transport.

## Conversations, streams and recovery

Use `prepare_conversation_send`, retain `plan.send_intents`, commit its pending
state, then hand off. Only a trusted host observation of the exact attempt
permits `prepare_observed_send`, followed by its own approved save. Timeouts stay
pending; `prepare_definitely_not_sent` needs actual non-delivery evidence.
Scoped restore and its streams delegate Core's pending/wrong-chat checks.

`restore_stream(tenant, encoding="utf8", progressive=False, logical_id=None)`
returns an opaque owned stream. UTF-8 feeds require bytes; UTF-16 feeds require
exact integer lists without bools/floats or out-of-range units. Strict buffering
releases no feed text. Progressive mode is explicit and irreversible; terminal
errors retain actual delivered counters and `complete=False`, with no failed-call
text. Malformed feed transport is rejected through Core's opposite-encoding
failure path so it terminates the response rather than silently skipping data.
Explicit reset starts a fresh response at the same admitted head; it does not
undo output already returned.

`import_authenticated_bundle` requires a separately authenticated/decrypted Core
bundle, its expected namespace/full head, a fresh recovery operation, and literal
approval. It delegates source recovery staging to the shared migration helper,
creates fresh destination stores/keys and publishes through Rust Owner. The
source archive is unchanged. It neither decrypts legacy backup files nor defines
future v1 snapshot bytes. No recovered result grants transport permission.

The public stub lists the current subset: ordinary/batch/combined review,
policy changes, recovery, conversation preparation/confirmation, scoped/unscoped
reports and streams, counts/history/operations, and reconciliation. The original
Core Python SDK remains available for other Core operations; this candidate does
not claim every Rust Owner convenience method is exposed yet.

## Build and installed proof

Use the declared toolchain and build an actual wheel, then install it into a
Core-owned isolated environment:

```sh
RUSTUP_TOOLCHAIN=1.94.1 maturin build --locked \
  --manifest-path integrations/fogvault-python/Cargo.toml \
  --out integrations/fogvault-python/dist
python3 -m venv target/fogvault-python-venv
target/fogvault-python-venv/bin/python -m pip install --no-index integrations/fogvault-python/dist/*.whl
target/fogvault-python-venv/bin/python -I integrations/fogvault-python/tests/test_installed.py
```

On Windows use the venv's `Scripts/python.exe`. The installed tests execute the
actual compiled module and registered SQLite adapter with synthetic data. They
cover persistence/reopen, Unicode coordinates, literal approval, tenant/thread
and foreign-plan isolation, writer conflicts, recovery, conversations, stream
revocation/failures, malformed inputs and receipt-only restart reconciliation.
They do not qualify physical providers, power cuts, every OS/Python version,
real shipping-host authentication, registry publication, or production security.

If Python result conversion fails after a confirmed save, the owner and its
children are revoked. The best-effort error has `code="outcome_unknown"`, the
exact `request_id`, and `requires_reopen=True`; reconcile the request and reopen
instead of retrying the change. Tests cover ordinary confirmed saves and reopen;
they do not force allocation exhaustion or guarantee reporting after process OOM.

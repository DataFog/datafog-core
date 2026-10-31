# Native Core–FogVault adapter

This optional Rust package saves DataFog Core workspace bundles through the
maintained registered FogVault native API. The engines remain separate: the
integration owns the sequencing; Core owns workspace semantics; FogVault owns
keys, authenticated records and persistence. Version `1.0.0` implements the
reviewed Core Workspace contract 1 with FogVault `0.1.0`. Production host and
key-provider qualification remain separate.

```sh
cargo add datafog-fogvault-native@=1.0.0
```

## Build and verify

From the Core repository root:

```sh
cargo +1.94.1 test --manifest-path integrations/fogvault-native/Cargo.toml --locked
cargo +1.94.1 fmt --manifest-path integrations/fogvault-native/Cargo.toml --check
cargo +1.94.1 clippy --manifest-path integrations/fogvault-native/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo +1.94.1 run --manifest-path integrations/fogvault-native/Cargo.toml --profile test --example measure --locked
```

The integration has its own workspace, lockfile and Rust 1.94.1 qualification
toolchain. Its declared minimum is Rust 1.89. The native gate runs ten external
consumer tests and one internal lost-acknowledgment test; the registry archive
consumer runs the ten external tests. Core itself remains at Rust 1.88.
Invoking Cargo with `--manifest-path` from Core's root does not select the nested
toolchain, so the explicit `+1.94.1` matters. Core's Rust 1.88 baseline remains
unchanged. The current Core dependency is a repository-relative path; FogVault is
an exact public Git revision with backup disabled. A registry release needs a
separate dependency/distribution decision. The test-only published Core dependency
is exactly `=0.5.0-beta.3`, checked alongside the current Core source.

## Host integration

1. Establish actual independent OS protection for the FogVault control namespace.
   The synthetic test's separate temporary directory does not establish that.
2. Use the maintained FogVault `Host` to create/open and authenticate a vault.
   Pass its `Vault` and `SessionToken` into `Owner::create` or `Owner::open`.
   Record/workspace IDs and operation/request IDs come from the host CSPRNG.
3. Prefer `Owner::review` and `prepare_review` for user-reviewed inputs. Scanner
   findings and source completeness remain host obligations. Preparation returns
   an opaque `Change` and a local preview; it does not grant handoff permission.
4. Obtain explicit approval of the exact preview. Call `Owner::commit` with that
   change, `true`, and a fresh vault request ID. It saves the exact candidate using
   the retained vault head plus record revision and activates only on confirmed
   persistence. Only the returned `Committed` previews are ready for host handoff.
   This does not send a message or prove any remote delivery.
5. On lock, call `Owner::lock` or drop it. It closes Core before locking FogVault;
   prepared changes and restore streams become invalid. Reopen through the host
   with fresh authentication and `Owner::open`, validating the expected workspace
   namespace and, when available, its complete expected head.

No raw Workspace or mutable Vault escapes the owner. Do not retain or mutate raw
handles/revocation channels behind the adapter. One owner serializes mutations;
complete-head CAS rejects competing writers. Foreign changes reject before any
write. Stale source heads and storage/activation failures close the owner and
return no handoff output. A caller's prior preview/plaintext copies remain its
cleanup responsibility; lock is revocation of future authority, not proof that
all foreign copies were erased.

`Error::Vault(OutcomeUnknown { request_id })` retains the exact request identity.
Keep that ID in trusted host operation state. `Owner::reconcile(request_id)`
reconciles only that operation and keeps Core closed. Drop the old owner, reopen
and authenticate via the host, then prepare and approve fresh work. Never infer
rollback, repeat an old approval, or resend transport automatically.

## Supported Core facade

The owner forwards Core review/recall/policy, strict restore reports (including
conversation-scoped reports), counts, history coverage and paged/full recorded
operations. Operation-aware create, prepare/batch, recovery and reported outcomes
retain their Core timestamps and validation. Plans expose snapshot-version and
policy previews before approval without exposing raw bundle bytes or activation.
Both strict buffered and explicit progressive streams use Core's existing
lifetime rules: same-head no-op policy saves preserve a child, changed Core heads
invalidate it, and owner lock closes it. Progressive output already returned
cannot be retracted. Host-reported outcomes do not establish remote delivery.

## Conversations and recovery

Conversation send plans, observed-send resolutions and definite non-delivery use
Core's staged methods and the same save/activate path. Pending sends survive
reopen and block scoped restoration until a matching trusted observation is saved.
The adapter cannot establish real site identity or successful network delivery.
Unscoped restore retains Core's existing compatibility semantics and is not a
fallback for failed conversation-scoped restoration.

`Owner::import_recovered` accepts the zeroizing `RecoveryCandidate` from the
sibling migration package only after explicit approval. The host must create a
fresh destination vault with fresh keys. The helper audits an empty destination
and fences publication with its exact head so a concurrent write cannot turn it
into an overwrite. Initial record creation uses expected revision `None`, while
the binding records the authenticated source parent and actual recovered Core
head. It constructs a new owner from the exact candidate. It does not transplant
legacy keys, decrypt legacy archives, or activate/handoff an old source plan.

The shared 88-byte `DFVBRG01` binding and fixture contract are in
[the integration contract](../../docs/integrations/fogvault-contract.md).

## Evidence and remaining scope

Tests execute the actual maintained Git-pinned FogVault API and SQLite. They
cover save/activate, false approval, foreign plans, competing writers, exact
binding validation, lock/stream lifetime, pending conversation resolution,
recovery without overwrite, published-beta.3/current-Core bundle interchange,
facade report/history/stream parity and actual payload-admission rejection. The
latter rejects a candidate over the admitted native payload limit without a
handoff result, invalidates the pending plan and reopens the exact prior saved
head with its old markers intact. This is a controlled admission failure, not
qualification of the largest supported Core graph or a peak-memory budget.
The lost-ack test performs an actual native publication then injects loss of the
result at the adapter's private receipt boundary; it is not evidence that a real
power cut occurred or that FogVault itself returned an unknown result.

The advisory `measure` example uses 1, 100, 1,000 and 5,000 synthetic mappings and
reports preparation, save/activation, unlock/reopen and restoration timings plus
logical bundle sizes. It does not measure peak memory, allocator overhead or
foreign copies, set a new latency threshold, or qualify low-memory devices.

This Rust integration does not expose a maintained Python or Node host package.
Those bindings, cross-platform CI, physical/provider and power-loss qualification,
final v1 migrations and registry publication remain separate work.

Only one prepared change may be active per integrated owner. A second preparation
returns `resource_limit` before Core clones its state. Discarding, dropping or
consuming the change releases the slot even if an older wrapper remains retained;
closing the owner revokes all its authority. Read-only review and restore streams
remain available. This is a per-owner plan limit, not a global memory budget; see
[combined resource observations](../../docs/integrations/fogvault-resource-budget.md).

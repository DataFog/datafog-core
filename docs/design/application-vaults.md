# Application vault design (#258)

These decisions precede the envelope implementation. The November 2026 client
handoff surveyed Core 1.1.0; this implementation starts from Core 1.5.0. The
Workspace contract stays at 1, with existing marker and snapshot formats intact.

## Reconciliation and provenance (Q1)

One application has one authoritative workspace. Writers serialize an entire
reload/prepare/compare-and-save/activate transaction, rather than creating
independent workspaces and merging them. A stale writer must reload and prepare
again; it must never send its stale preview. Equal normalized type/value identities
already reuse one code inside a workspace. Independently issued aliases and
conflicting codes cannot be imported into that identity map without an explicit
merge contract. No alias, import, cross-vault reconciliation or cloud-sync API is
introduced here. Such work requires a separately reviewed contract and migration.

The encrypted envelope authenticates the application ID, random vault ID and
Workspace contract version. The snapshot retains operation IDs and the exact
Core head. Core's operation timestamps are optional caller-supplied metadata;
the storage layer does not invent historical timestamps. The high-level helper
records its current local operation time and ID, including for known-value
preparations; its explicit operation-aware Rust method accepts host metadata.
A future synchronization
service must preserve this provenance and cannot treat the envelope as a merge.

## Capacity (Q2)

Current Core admission limits are 20,000 live entries, 16 MiB serialized bundle,
1 MiB source per input, 10,000 findings and 32 inputs per batch. The operation
ledger holds 100,000 rows, reserving 256 for recovery; ordinary operations stop
at 99,744 (and can hit the byte reserve earlier). Policy has 20,000 items and
10,000 edits. Retired codes consume bundle space after forgetting. All limits
fail closed with controlled errors; reaching a cap cannot release a preview.
Raising implementation limits needs resource/reader compatibility review; sharding
needs routing and restore semantics and is outside this change. A near-full vault
must be explicitly maintained, not silently replaced or split.

## Restore (Q3)

Existing strict restore and restore_report already validate exact valid markers,
report unknown/expired/malformed markers and withhold complete text on issues.
Progressive restore already releases validated prefixes and carries strict stream
state; it cannot promise that a later chunk will succeed. The application helper
refreshes the committed snapshot before each restore. Long-lived streaming clients
must refresh before creating their response stream and keep that snapshot for its
lifetime. No fuzzy repair or tolerant whole-answer restore is added. Collect
actual plugin failures before proposing a compatible report-only addition.

## Storage and key decision

Keep the existing password-based FogVault owner available. Its current Python
adapter binds one owner to an OS thread, admits one pending plan, and supports
only the password_only profile. FogVault's admitted record payload is 16 MiB and
its default record count is 64; they do not remove Core's stricter snapshot caps.
It is not a zero-KDF short-lived application opener. The new helper instead uses
a random 256-bit key from the OS credential store (Keychain on macOS, Windows
Credential Manager, Secret Service on Linux) and authenticated encryption of
unchanged Core snapshot bytes. No per-operation password derivation occurs.

Headless machines may explicitly opt into an owner-only local key file. Its
reported protection mode is file_key: ciphertext is encrypted at rest, but a
backup or compromise containing both files reveals the vault. This fallback is
never silently selected. A caller-supplied key reports external_key and lets a
launcher supply its own credential provider. A credential-store outage must not
create a replacement key for an existing vault. Missing or incorrect keys fail
closed. Plaintext workspaces are never persisted.

Use an OS advisory lock with a bounded timeout, authenticated full-head CAS,
same-directory temporary file, file sync, atomic replacement and directory sync.
A crash before replacement leaves the old head; after replacement it leaves the
new head, which a fresh opener activates by reopening its snapshot. A failed
save or activation never returns transformed text. Lost acknowledgments require
the caller to inspect the committed operation ID; automatic resend is unsafe.
This provides local durability, not rollback protection against a malicious owner
restoring an older encrypted backup. Paths and filesystem permissions must be
trusted: the helper rejects symlinks, unsafe app IDs and permissive vault/key/lock
files, and checks ownership on Unix. OS-level same-user malware is outside this
boundary. Windows ACL enforcement and provider qualification require actual
Windows proof, rather than claiming Unix mode bits secure Windows files.

The separately versioned envelope is local persistence, not a Core snapshot
format. New envelopes must reject unsupported versions/contract IDs before
opening a Workspace, authenticate metadata, bound allocations and redact errors.

## Envelope 1

`workspace.enc` is `DFAPP001` (8 bytes), contract version (u32 little-endian),
app-ID length (u8), ASCII app ID, vault ID (16 bytes), Workspace namespace
(16 bytes), revision (u64 little-endian), commit ID (16 bytes), key mode (u8:
1 credential store, 2 file key, 3 external key), random XChaCha nonce (24 bytes),
then XChaCha20-Poly1305 ciphertext/tag of the exact Core bundle. Everything before
the nonce is authenticated associated data. The complete file is bounded by the
Core 16 MiB limit plus 512 bytes; metadata is validated again after decrypt/open.
Unsupported magic/contract/mode fails before opening a Workspace. There is no
plaintext candidate, alternate marker representation or snapshot migration.

The `SnapshotBackend` interface reads encrypted bytes and offers only
compare-and-save with the expected complete Core head plus the exact prior
encrypted generation. The owner authenticates that generation before requesting
CAS. Local writes hold an OS lock across reload, prepare, CAS and activation;
three attempts are allowed only on a known conflict. A remote implementation
would need server-atomic CAS and a separately reviewed threat model.

## Measured observations and qualification

The checked-in [macOS ARM64 observation](evidence/application-vault-bench-macos-arm64.json)
uses 20 samples per operation, a 4 KiB prompt and a synthetic external key.
It excludes interpreter/executable startup and **does not measure OS credential
retrieval**, so it cannot qualify default credential-store latency. Mutations
reuse a known identity and record a fresh operation: they exercise complete
encoding, encryption and durable replacement at a full 20k-entry cap, rather
than admitting a new identity into a full vault.

| Live entries | Open/restore p50 / p95 ms | Open/commit p50 / p95 ms | Four writers p95 ms |
| --- | --- | --- | --- |
| 1,000 | 1.93 / 2.22 | 10.89 / 11.10 | 128.42 |
| 10,000 | 16.47 / 17.68 | 35.79 / 37.33 | 335.63 |
| 20,000 | 34.28 / 36.06 | 67.04 / 69.64 | 800.69 |

The proposed 50/100 ms 1k target is met for uncontended external-key observations;
four-writer contention exceeds its 100 ms p95. Serial whole-snapshot durability
creates a queue; these numbers do not justify weakening locking, fsync or Core
validation. Targets remain advisory, with physical-provider measurements and
broader deployment workloads still required before a production latency promise.

Local executed proof is macOS ARM64, Python 3.14; native Windows ACL tests also
cross-compile for Windows x64, which is not execution proof. The application-vault
CI matrix executes native/crash/installed Python 3.10 tests on Linux x64/ARM64,
macOS Intel/ARM64 and Windows x64. Existing native language gates retain Python
3.14. Green results qualify those exact test environments, not every OS build,
credential-provider policy or a physical power-loss test. Fault injection kills
real child processes around rename and activation; storage-device power cuts
and physical OS credential providers remain deployment qualification work.

The dependencies provide reviewed AEAD (`chacha20poly1305`, already used by
FogVault), cross-platform advisory locks (`fs2`), maintained credential providers
(`keyring`), secure atomic temporary files (`tempfile`), CSPRNG IDs (`getrandom`)
and native Unix/Windows permission APIs. Standard-library mode bits cannot
replace Windows ACLs or a credential store. Linux Secret Service builds its
vendored D-Bus dependency so a system development package is not required.

# Native server qualification for 0.5.0-beta.3

`cargo dfv parity --workspace` requires the actual existing Python and Node
encrypted SQLite hosts in addition to the SDK matrix. This profile has exactly
**two host implementations**. Rust provides deterministic Core candidate bundles;
it does not stand in for a Rust SQL host. Browser WASM remains an SDK/browser
qualification, not a fourth SQL implementation.

| Profile | Actual runtimes | Scenarios | Steps |
| --- | --- | ---: | ---: |
| SDK foundation | Rust, installed Python/Node, Chromium WASM | 17 | 156 |
| SDK buffered/progressive restoration | Same four SDKs | 13 | 80 |
| SDK operations/history | Same four SDKs | 7 | 185 |
| Native encrypted server | Python and Node only | 9 | 28 |

The existing top-level `cases: 37` / `steps: 421` retain their four-SDK meaning.
`profiles.nativeServer` reports its own SDK list and 9/28 counts. Missing or empty
profiles fail; the CLI rejects folding host counts into the four-SDK totals or
labelling the native-server profile with four runtimes.

## Real persistence and Core candidates

`fixtures/workspace/native-server-contract-v1.json` pins namespace, operation IDs,
timestamps, synthetic text and findings. The Rust reference calls actual Core
creation/preparation/export/open/count/history methods to produce the seed,
two competing candidates and a byte-preserving no-op. Each installed native SDK
independently prepares its candidate and must match the complete canonical inner
bundle before saving it through the existing host store.

Python uses `EncryptedStore.compare_and_save` / `open`; Node uses
`EncryptedWorkspaceStore.saveIfHeadMatches` / `load`. Both perform actual SQLite
transactions and encrypted reopen. The profile adds no host runtime API and does
not replace saves with mocks. Exact inner bundles, complete heads, operations,
counts, history coverage and policy are retained. Ciphertexts/nonces are random
and deliberately not compared across different host envelope formats. Within
one host, failed writes must preserve the exact encrypted-row digest as well as
the full Core state.

The named checkpoints cover:

- Acknowledged save, activation and encrypted reopen.
- Separate wrong-revision and wrong-commit-ID CAS failures.
- A rollback after the real SQL update, and lost acknowledgment after commit.
- Two independent native processes racing a changed head: exactly one winner.
- Two no-op writers that can both succeed with the same head. CAS is not a
  delivery lock, and these acknowledgments do not prove exactly-once delivery.
- Real process kills before and after commit. The Python before-commit barrier
  is its existing cancellation callback after UPDATE; its after-commit barrier
  follows the store return and precedes activation. Node uses its existing
  before/after-commit fault callbacks. Reopen distinguishes rollback from a
  committed candidate without pretending both APIs have the same acknowledgment.
- Ciphertext and authenticated-head metadata tampering, wrong-key and other-tenant
  access, followed by exact restoration of the original encrypted row.

For a nondeterministic race, each outcome retains its actual operation ID,
candidate bytes, complete head, acknowledgment and raw host error. The final
stored bundle must equal the actual winning candidate from the Core oracle.
No winner ID is rewritten to make outputs agree. A loser cannot report activation.
The no-op case retains both distinct operation IDs and acknowledgments.

Host error vocabularies remain explicit. For example, Python returns
`save_uncertain` and Node returns `save_ack_unknown` for their respective injected
lost-ack paths. Authentication rejection is `invalid_envelope` versus
`invalid_snapshot`. Raw codes/messages remain in per-runtime outputs; the fixture
validates each host's declared contract instead of reducing them to a generic
normalized status. Core bundles and errors are not weakened to accommodate host
representation differences.

## Existing handoff and streaming proof stays mandatory

The profile also executes the unchanged Python host's 22 installed-SDK tests and
the Node host's existing 17 returned checks. These exercise approval, cancellation,
real competing requests, crash/lost acknowledgment, tenant isolation, bounded
plans and strict/opt-in progressive transport behavior. The lower-level shared
CAS fixture is not substituted for that high-level handoff proof.

The isolated Python consumer installs the already existing hash-pinned host
requirements from `examples/native-python-host/requirements.txt`. No dependency
or runtime cryptography policy is added. Synthetic keys remain owned/resolved by
the trusted test hosts, outside Core and model text. Production KMS/key rotation,
provider framing, global host admission, distributed delivery and host security
approval remain separate. These POSIX crash tests use the existing macOS/Linux
host qualification environment; an unavailable runtime fails rather than skips.

## Reproduce and inspect

Run `cargo dfv doctor`, `cargo dfv build`, then `cargo dfv parity --workspace`.
The normal run retains the source commit/dirty state, package and fixture hashes,
Python dependency-pin hash, full Core oracle, `python-native-server.json`,
`node-native-server.json` and the existing Python-suite receipt beside the four
SDK outputs. Those JSON files contain the fixed synthetic fixture data. Encryption keys are
not written to receipts, and real-user input is never used.

Conversation/send-confirmation behavior and its capacity reservation are outside
this profile. Passing these checks does not publish a release or grant shipping
host security approval.

## Prepared-plan ownership under load

The mandatory [plan-pressure subproof](native-plan-pressure-0.5.0-beta.3.md)
qualifies the existing per-handle 32-plan bound across paired handles/workers,
discard and changed-head cleanup, independently stale CAS, retained wrappers and
worker exit. Its counts are separate from both SDK scenarios and native CAS
checkpoints. Exact shared-Core requested allocations and observed SDK RSS/heaps
are reported separately; the Node resident-memory trend is unresolved and no
aggregate admission budget is selected.

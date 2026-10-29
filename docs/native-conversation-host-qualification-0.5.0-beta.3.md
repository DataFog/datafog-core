# Native conversation host qualification — 0.5.0-beta.3 candidate

This test-only profile qualifies C2 save/transport/confirmation sequencing through
installed Python and Node SDKs and the existing encrypted SQLite stores. It adds
no production host API. The Rust Core reference generates deterministic full
candidate bundles and heads for every branch of the fixture.

The 16 scenarios and 66 checkpoints cover the first saved intent before transport,
matching successful callbacks, definite non-delivery, ambiguous transport,
rollback and lost acknowledgments, whole-conversation pause across reopen/recovery,
full-head CAS, two independent-process races, crashes before and after each save,
and conversation/tenant/workspace association. Race receipts retain the actual
winning candidate IDs and raw host acknowledgment/error differences. No random-ID
normalization or mocked persistence is used.

The controlled synthetic transport records three independent pieces of evidence:
the staged attempt (preparation/member/logical identity and approved output hash),
the callback received from that controlled adapter, and its controlled site
verification witness. An actual durable-store read must show the pending intent
before callback execution. A mismatched callback never invokes resolution;
uncertain transport retains pending state. A successful callback permits only a
metadata confirmation save. Failed or uncertain confirmation never resends.

Real child processes are killed inside the SQLite transaction and after commit
but before local activation/acknowledgment. Reopen checks use real decryption and
Core decoding. The independent CAS workers preserve both expected and candidate
heads, full canonical bundles and real exits. The old plain replies and issued
markers stay paused while a follow-up intent is unresolved; another conversation
remains usable. Recovery does not resolve that uncertainty.

This profile applies to **Python/Node native encrypted hosts only**. It is counted
separately from the four-SDK semantic profiles and the existing native CAS profile.
The existing Python host suite (at least 27 cases) and Node suite (at least 17
checks) remain mandatory. Protocol regression tests mutate the actual retained
outputs to verify that omitted callbacks, forged identities/heads, invented
acknowledgments, hidden resends and missing scenarios fail qualification.

The callbacks and site verification are controlled test fixtures. They do not
establish actual provider acceptance, shipping extension identity acquisition,
production key management/security approval or aggregate host admission policy.
No beta.3 publication is authorized by this qualification.

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
winning candidate IDs and raw host acknowledgment/error differences. Each host
executes the complete 16/66 profile; these counts describe shared scenarios, not
a four-SDK workload. No random-ID
normalization or mocked persistence is used.

The controlled synthetic transport records three independent pieces of evidence:
the staged attempt (preparation/member/logical identity and approved output hash),
the callback received from that controlled adapter, and its controlled site
verification witness. Closed builtin callback/witness records reject booleans, non-integer indices,
wrongly typed nested IDs, hashes and surfaces, and unexpected fields. Python
comparisons distinguish integer zero from `False` and `0.0`. An actual durable-store read must show the pending intent
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
acknowledgments, hidden resends and missing scenarios fail qualification. Seven
named real Core error observations form the error oracle. Every code, null index,
original byte/codepoint/UTF-16 range, diagnostic list and completion/progress field
must match that raw oracle exactly; expected ranges are never synthesized. Ordinary
strict errors represent absent optional stream fields as null in both records.
The wrong-context probe includes an emoji prefix so the three coordinate units
differ. Injected indices/conflicts, omitted ranges and invented completion fail.

The callbacks and site verification are controlled test fixtures. They do not
establish actual provider acceptance, shipping extension identity acquisition,
production key management/security approval or aggregate host admission policy.
No beta.3 publication is authorized by this qualification.

# Ordinary confirmation capacity — decision packet

Review proposal for [#148](https://github.com/DataFog/datafog-core/issues/148),
coordinated with [#147/#162](https://github.com/DataFog/datafog-core/pull/162).
**Unapproved; no capacity reservation is implemented here.** The confirmed-send
sequence and conversation-wide pending pause are already approved. Do not ask for
those decisions again.

## The concrete gap

The approved budget is 100,000 retained operation rows: ordinary activity stops at
99,744, with 256 recovery-only slots and a derived recovery byte reserve. No record
may be evicted. A send uses a preparation save and a later confirmation save.

At 99,743 rows, the preparation can consume ordinary row 99,744. The external send
may succeed, but its confirmation is now an ordinary write with no remaining
capacity. The durable intent correctly remains pending, so scoped restoration is
paused; without a separate capacity design, that pause can be permanent. A similar
gap occurs when the first save consumes the available encoded-byte budget.

## Recommendation: reserve ordinary resolution capacity before sending

Before accepting the first saved intent, allocate one **ordinary** resolution-row
credit per intended send member, plus encoder-derived worst-case bytes to resolve
it. These are reservations, not successful sends or invented operation records.
The confirmation or a proven not-sent resolution later consumes its credit. Other
ordinary writes cannot spend reserved capacity. Failed/discarded candidates leave
credits intact; saved-state reopen and recovery preserve them.

Do not take any of the 256 recovery slots, shrink their byte reserve, raise the
100,000 ceiling, drop records or automatically resend. If ordinary space cannot
fund the intent and its eventual resolution, reject before external handoff.
This protects against resource exhaustion, not storage failure, uncertain
acknowledgments or missing trusted observations; those still pause scoped restore.

The alternative is **no reservation guarantee**: retain today's hard admission
rules and clearly tell the host that a successful send can become permanently
unconfirmable at the lifetime limit. Compatibility reads/export remain available,
but no silent unscoped fallback or manual success assertion may remove the pause.

## Candidate accounting that still needs technical review

Let `T` be retained rows and `P` be outstanding, already-funded ordinary resolution
credits. A new ordinary mutation creating `N` intents must fit its actual new rows
and `P + N` credits inside the 99,744 ordinary budget. Resolving one existing intent
adds one actual row and consumes one credit; it must not create another reservation
or increase `confirmedHandoffs` until observed success is durably saved.

This needs a real admission ledger, not a check only at preparation. In particular,
recovery can append while an intent is pending. The existing physical row-count
threshold alone cannot express an ordinary credit funded before those recoveries.
A candidate logical-credit interpretation would preserve `T + P <= 100,000`, keep
unfunded ordinary activity below its existing ceiling, and protect all remaining
recovery capacity. It is **not an approved exception** to the current code.

| Proposed boundary | Expected behavior if reservations are approved |
| --- | --- |
| `T=99,743`, no credits; one new send needs preparation + resolution | Reject the first save; no external send. |
| `T=99,742`, no credits; one new send | Save preparation at `T=99,743, P=1`; unrelated ordinary work cannot consume its credit. |
| Resolve that intent with no intervening recovery | Reach `T=99,744, P=0`; record only the actual outcome. |
| All 256 recoveries occur while that credit remains pending | Candidate ledger must still permit the funded resolution, reaching exactly 100,000 retained rows; all 256 recovery records remain preserved. |

The last row exposes the accounting question explicitly: a funded ordinary
resolution may be appended later in history than a recovery. Before implementation,
#162 and #148 must agree on and independently validate the distinction between
reserved capacity and append position. Do not silently let arbitrary ordinary
operations use a recovery-only slot or discard a credit during recovery.

## Encoded bytes and required proof

Use the final coordinated v3/key9 encoder to bound the **largest possible positive
resolution growth**, including terminal metadata, newly issued membership, the
operation row and numeric/array/header growth. Current pending bytes are already
part of the candidate; do not promise that deleting a pending row always funds its
confirmation. Overlapping pending/member sets can make a mixed intermediate state
larger than the all-resolved state.

New ordinary admission must fit the complete candidate plus outstanding ordinary
resolution-byte credits **and** the separately derived recovery reserve in 16 MiB.
Recovery must preserve funded ordinary bytes as well as rows. The current 7,699-byte
recovery calculation is not permission to spend those bytes on confirmations.
Derive and test the final bounds; no new magic byte constant is selected here.

Before runtime/freeze, jointly test row and byte edges with nonempty key9 data,
multiple pending intents, mixed successful/not-sent resolutions, unrelated writes,
all 256 recoveries, restart, both encrypted backup credentials, malformed claimed
credits and lost acknowledgments. Counts must recompute from actual retained rows;
reservations must never inflate counts. All SDKs must reject the same exhausted or
malformed cases without partial state changes.

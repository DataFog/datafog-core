# R4 restoration delivery contracts (draft)

Target: 0.5.0-beta.3; [#169](https://github.com/DataFog/datafog-core/issues/169).
This is an active implementation draft. **R4 is not complete from a finalize-only adapter.**
Sid has explicitly approved both delivery and EOF/oversized-prefix decisions in
the thread; #169 records the decisions. The technical API remains under review.
Provider events, SSE framing, tool schemas and network callbacks belong outside Core.

## Implemented draft: atomic buffered responses

`Workspace::buffered_restore(RestoreEncoding::{Utf8,Utf16})` pins one immutable
committed snapshot/head and runtime lifetime. It shares an immutable snapshot
reference rather than copying every saved mapping per response. `feed(bytes)` accepts arbitrary
UTF-8 boundaries; `feed_utf16(units)` accepts arbitrary UTF-16 boundaries including
split surrogate pairs. An instance uses exactly one encoding. No feed returns
text. `finalize()` decodes the complete response and returns the existing strict
R2 `RestoreReport`: text only if every recognized marker resolves, otherwise a
content-free original-coordinate report with no partial text.

The decoded input and restored output retain R2's 16 MiB bounds. Incomplete or
invalid Unicode rejects without replacement. Buffers clear on finish, explicit
reset/cancel/close, allocation/limit/encoding errors and Drop. Reset starts a new
response at the same still-current head; close is permanent/idempotent. Workspace
close or a changed saved head invalidates the adapter. A host must close/drop
retained adapters on lock/cancel: invalidation prevents output immediately, while
retained buffers and snapshot references are released when invalidation is observed
or the adapter is closed/dropped. Native/browser owners must drop retained child
handles on workspace close, as they already do for pending plans. No aliases, plans, durable
state or operation/conversation records are allocated.

This buffers the whole answer and therefore **does not provide realtime output**.
It preserves current R2 syntax exactly: an unfinished marker-like literal at EOF
that R2 does not recognize is not newly reclassified by this adapter. That
compatibility is distinct from the progressive EOF decision below. No streaming
method is currently claimed in a published Python/Node/WASM package.

## Approved separate opt-in progressive delivery (implementation pending)

Use a separate `progressive_restore` operation with explicit host opt-in, rather
than weakening R2 or pretending already emitted text can be retracted. The host
receives completed safe text/marker restorations from feed; it retains a bounded
carry for UTF encodings, a potential `[[...]]` marker, and an ASCII word that could
still become a committed unbracketed marker. This can delay a word even when a
provider delivered a delta. A complete committed marker can be emitted immediately.
Source offsets in terminal issues refer to the complete original provider text,
not chunk-local positions or restored output.

**Approved behavior:** make an unknown/damaged marker, a committed bare code,
invalid encoding, a held marker-like prefix at EOF, a bound violation, cancellation,
workspace close or stale head terminal for that response. Stop returning new text,
clear carry, and return `complete: false`, a content-free cause/ranges and exact
already-delivered byte/codepoint/UTF-16 counts. Previously delivered output stays
visible; never return it again as a successful complete response. A caller must
present a failed/incomplete answer and explicitly restart or fall back to a newly
buffered response. A reset is a new response, not rollback of earlier delivery.

Sid explicitly approved that an unfinished possible pseudonym at EOF or an
oversized held prefix must stop and mark the response incomplete, rather than
flush a possible marker as ordinary text. The accepted tradeoff includes code and
literal false positives. Existing nonstreaming R2 behavior remains unchanged.
The current buffered adapter's exact-R2 EOF compatibility described above is an
implementation baseline, not completion of the newly approved streaming guard.

## Decisions and remaining technical review

1. **Approved:** strict all-or-nothing default plus separate opt-in progressive
   delivery. Earlier delivered chunks are irreversible; a late bad marker is
   terminal/incomplete. Do not automatically retry or claim a complete response.
2. **Approved:** unfinished/oversized possible marker prefixes stop incomplete,
   including the known literal/code false-positive tradeoff. Nonstreaming R2 is
   unchanged. This is no longer a pending product question.
3. Mode/encoding/reset/result shapes remain routine implementation/API review.
   UTF-16 input must not silently replace half-surrogates. Progressive declarations
   must not be published before runtime and cross-SDK parity exist.

## Evidence and remaining work

The shared draft fixture distinguishes buffered and proposed progressive outputs:
plain text, every delimiter/label/code split, adjacent/repeated markers, Unicode
encoding splits, a late damaged marker after a valid restored prefix, bare codes,
EOF prefixes, cancellation and stale/closed heads. Buffered Core tests exercise
R2 equality at every byte/code-unit split and real lifecycle/bounds. Progressive
parser/runtime, approved terminal delivery types, native bindings, full X8 parity
and W6 host failure behavior remain required before R4 can close. No conversation
scoping is claimed until #148 is integrated and reviewed.

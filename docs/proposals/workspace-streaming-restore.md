# R4 restoration delivery contracts (draft)

Target: 0.5.0-beta.3; [#169](https://github.com/DataFog/datafog-core/issues/169).
This is a review proposal. **R4 is not complete from a finalize-only adapter.**
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

## Proposed separate opt-in progressive delivery (not exported)

Use a separate `progressive_restore` operation with explicit host opt-in, rather
than weakening R2 or pretending already emitted text can be retracted. The host
receives completed safe text/marker restorations from feed; it retains a bounded
carry for UTF encodings, a potential `[[...]]` marker, and an ASCII word that could
still become a committed unbracketed marker. This can delay a word even when a
provider delivered a delta. A complete committed marker can be emitted immediately.
Source offsets in terminal issues refer to the complete original provider text,
not chunk-local positions or restored output.

**Review recommendation:** make an unknown/damaged marker, a committed bare code,
invalid encoding, a held marker-like prefix at EOF, a bound violation, cancellation,
workspace close or stale head terminal for that response. Stop returning new text,
clear carry, and return `complete: false`, a content-free cause/ranges and exact
already-delivered byte/codepoint/UTF-16 counts. Previously delivered output stays
visible; never return it again as a successful complete response. A caller must
present a failed/incomplete answer and explicitly restart or fall back to a newly
buffered response. A reset is a new response, not rollback of earlier delivery.

Progressive finalization of an unfinished `[[PERSON_...` prefix is deliberately
proposed to fail rather than flush a possible pseudonym. This is stricter EOF
handling than some current R2 literals and needs explicit review, especially for
meaningful bracket/code syntax. Likewise, oversized potential prefixes should
fail explicitly rather than silently turn into an ordinary literal. These
choices are **not approved runtime behavior** in this draft.

## Concrete human review decisions

1. Approve opt-in irreversible partial delivery for beta.3, or ship only buffered
   responses and leave R4 realtime support open. Recommendation: opt-in progressive
   mode with terminal-failure semantics above, strict buffering remaining default.
2. Approve fail-closed EOF/oversized marker-like prefixes for progressive mode,
   including possible literal/code false positives. Recommendation: do not flush a
   held possible marker as successful text; preserve ordinary R2 nonstreaming syntax.
3. Review the explicit mode/encoding/reset/result surface. UTF-16 code-unit input
   must not silently replace half-surrogates. Do not publish declarations for
   progressive methods until their runtime and cross-SDK parity exist.

## Evidence and remaining work

The shared draft fixture distinguishes buffered and proposed progressive outputs:
plain text, every delimiter/label/code split, adjacent/repeated markers, Unicode
encoding splits, a late damaged marker after a valid restored prefix, bare codes,
EOF prefixes, cancellation and stale/closed heads. Buffered Core tests exercise
R2 equality at every byte/code-unit split and real lifecycle/bounds. Progressive
parser/runtime, approved terminal delivery types, native bindings, full X8 parity
and W6 host failure behavior remain required before R4 can close. No conversation
scoping is claimed until #148 is integrated and reviewed.

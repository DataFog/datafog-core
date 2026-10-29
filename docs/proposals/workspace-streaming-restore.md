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

## Implemented draft: separate opt-in progressive delivery

`Workspace::progressive_restore(RestoreEncoding::{Utf8,Utf16})` requires explicit
host opt-in. The host
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
This guard applies only to the opt-in progressive mode. The strict buffered
default and existing nonstreaming R2 keep their current literal/EOF handling.

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

## Draft Rust API and bounded parser

`ProgressiveRestore::feed(&[u8])` and `feed_utf16(&[u16])` return `RestoreChunk`:
`text`, cumulative `delivered { bytes, codepoints, utf16, restored_markers }`, and
`complete: false`. `finalize()` returns the remaining safe text with `complete:
true`. Failure returns `StreamFailure { error: Box<WorkspaceError>, delivered,
complete: false }`; the error retains content-free original response ranges.
No error contains staged output, source text or a restored value.

Each feed stages its own output atomically. A bad marker late within a feed
releases none of that feed, even if it contained valid earlier text; counts include
only successful previous calls. Chunk boundaries can therefore affect how much
text precedes a terminal error. This does not change success output or error source
ranges. Hosts must deliver each returned chunk once, show incomplete status after
failure, and never treat reset/retry as retraction or automatically append a retry
to an already delivered answer. A feed that returns an empty string is valid.

The parser holds ASCII words until a boundary (up to the existing R2 marker
interior bound), so a committed unbracketed word cannot escape before it is
classified. Longer ordinary words stream without retaining the whole word;
committed-marker substrings inside larger words remain ordinary text. `[` or `[[`
and an ASCII-letter-led `[[...` word remain possible bracketed markers. At EOF
these and a single closing bracket stop incomplete. A possible interior exceeding
R2's existing `MAX_MARKER_INTERIOR` (78 ASCII bytes) stops immediately, even if it
was intended as code/literal text. Broken syntax that can no longer become a
marker resumes ordinary text after checking for an exact committed bare word.
Well-formed committed markers retain case/Crockford matching; unknown/damaged
completed markers and committed bare words stop with the existing classifications.

Cumulative decoded input and returned output each retain R2's 16 MiB bound.
UTF-8 retains at most four pending bytes; UTF-16 at most one high surrogate.
The token carry capacity is at most 82 bytes (existing 78-byte interior plus four
brackets), independent of response length. Each call's output staging is bounded
by the remaining output allowance. The immutable snapshot and a fallibly built,
sorted marker lookup are fixed at construction; each lookup is logarithmic in
committed entries. Private lookup values wipe on close/drop, and both adapters
share allocation growth that wipes the old backing before it is freed. Reset
clears carry/counts for a new response at the same valid head; close is permanent.
No operation mutates workspace state or creates a plan/alias/record.

## Evidence and remaining work

[The executable Core fixture](../../fixtures/workspace/streaming-contract-v1.json)
exercises actual feed/finalize/reset/cancel/head lifecycles for both encodings,
late invalid markers, bare markers, EOF and oversized prefixes. Every byte and
UTF-16 split of valid responses matches R2 output; original-coordinate errors,
partial-delivery counts and unchanged saved bytes are checked. A deterministic
4,000-case grammar corpus checks complete syntax against R2. The strict buffered
Core tests continue to check exact R2 equality, including literal EOF compatibility.
Actual allocator hooks prove growth wipes the previous backing before deallocation
and injected fallible allocation failures clear the response. An 8 MiB debug-build
case retained an 82-byte carry and completed in about 1.15 seconds on the local
machine; this is evidence, not a performance threshold or release benchmark.

## Draft WASM transport

The installed draft WASM package exposes `workspace.bufferedRestore(encoding?)`
and `workspace.progressiveRestore(encoding?)`, where encoding is `"utf8"`
(default) or `"utf16"`. Factory-only interface objects mirror Core as `feed`
(`Uint8Array`), `feedUtf16` (`Uint16Array`), `finalize`, `reset`, `close`.
Atomic feeds return `undefined`; progressive feeds return the chunk/count fields
above, with `restoredMarkers` in camel case. Terminal progressive `DataFogError`
retains `delivered`, `complete: false`, and the usual marker ranges. A malformed
transport type terminates through Core's invalid-encoding path instead of letting
wasm-bindgen coerce arrays or strings. Encoding selection is a closed set.

Native stream objects own Core adapters. A weak parent registry closes surviving
children on changed-head activation or workspace close/drop, releasing private
lookup/snapshot references immediately; a same-head no-op preserves children.
Dropping a child does not retain it in the registry. Native ownership guards are
released before JS value/error conversion, and private text is serialized by
borrowing directly into JS rather than an intermediate reallocating JSON string.
JS output strings remain host-owned and cannot promise secure wiping.

This is a runtime draft. Current published Python/Node/WASM packages do not claim
streaming exports. The shared script runs the executable fixture against the
actual installed WASM package in Chromium, including the MV3 reference extension;
it does not establish Josh's shipping product integration. Full native binding
lifecycle/typing, X8 streaming parity and W6 host failure behavior remain required
before R4 can close. Provider framing/SSE and LiteLLM integration belong outside
this parser.
No conversation scoping is claimed until #148 is integrated and reviewed.

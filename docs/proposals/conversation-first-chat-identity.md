# First-chat identity — later decision packet

For [#148](https://github.com/DataFog/datafog-core/issues/148). **Approved October 29: private immutable logical identity, then one-time
matching site-ID binding. Core/WASM implementation draft; actual extension verification remains outstanding.** Confirmed-send and whole-conversation pending pause
are already approved; this packet does not reopen either decision.

## The integration gap and inspected evidence

The current [C2 proposal](workspace-conversation-context.md) requires an exact site
conversation ID before the first saved send intent. A browser site may assign that
ID only after its first send. That creates a first-message gap: Core cannot bind
the initial intent to an identifier the host does not yet have. This packet does
not claim to have verified current ChatGPT network/DOM behavior.

Read-only inspection used extension tracked commit
[`05fb7a486b8760dba821ee19dff72ef3f1de3a75`](https://github.com/DataFog/datafog-browser-extension/tree/05fb7a486b8760dba821ee19dff72ef3f1de3a75).
No provider was called or extension file edited. This is the retained compatibility
checkout, not evidence about Josh's unpublished work or newer remote changes.

- [`content.ts`](https://github.com/DataFog/datafog-browser-extension/blob/05fb7a486b8760dba821ee19dff72ef3f1de3a75/src/content.ts)
  explicitly never touches send. It scans/redacts composer text and observes
  composer replacement; that observation is not a successful-send receipt.
- [`messages.ts`](https://github.com/DataFog/datafog-browser-extension/blob/05fb7a486b8760dba821ee19dff72ef3f1de3a75/src/messages.ts)
  has only scan/redact/protection requests, no conversation identity or send event.
- [`background.ts`](https://github.com/DataFog/datafog-browser-extension/blob/05fb7a486b8760dba821ee19dff72ef3f1de3a75/src/background.ts)
  uses the sender's tab ID for badge state. It does not establish a persistent
  conversation or correlate a submitted message with a newly assigned site ID.

Therefore the existing extension cannot yet execute either option below. Site-ID
acquisition and a trusted matching send observation need a separate real host
implementation/qualification; neither can be inferred from a changed URL, an
empty composer, a click, a model reply field or a tab ID alone.

## Approved: immutable logical context, one-time site binding

Use a workspace-local host-generated logical context as the immutable identity of
the intent. A new conversation starts with an unbound site ID. After a trusted
matching successful-send observation yields the actual site ID, the **same second
saved confirmation** atomically binds it and records issuance. This supports first
messages without retargeting an already approved intent.

1. The trusted host allocates a fresh opaque logical context using the existing
   cryptographic 16-byte ID contract. Bind approval to the authorized workspace,
   surface, logical context and exact prepared batch member. A private host attempt
   registry pins that member to its originating tab/document/navigation generation
   and approved output. Tab/composer identity is transient host evidence, never
   the persistent conversation identity or proof that sending succeeded.
2. The first save persists that logical context, its unbound site-ID state and the
   immutable pending intent together with mappings. Nothing is issued yet. The
   host saves/activates normally before invoking the actual transport. Do not
   infer an attempt or send from preview, clipboard or composer fill.
3. Only a trusted site integration that correlates an actual successful send with
   **that exact attempt** may report a site ID. A URL appearing after arbitrary
   navigation, equal text in another chat, a provider-supplied field or a later
   tab lookup does not establish the association. Ambiguity leaves the intent
   unresolved; it is not an invitation to guess.
4. Prepare a metadata-only confirmation for the original logical context/member,
   the matching observed site ID and a fresh operation ID. Its saved activation
   binds the previously unbound context exactly once, resolves only that intent
   and adds its derived marker set. These changes are atomic; there is no saved
   bound-but-unconfirmed interval or send before the first save.
5. For future responses, the trusted host resolves exact `(surface, siteId)` to
   this same logical context in current authorized state. No mapping means scoped
   restore rejects; pending means the approved whole-conversation pause. There
   is no automatic fallback to another context or the legacy unscoped API.

Proposed invariants: one `(surface, siteId)` binds to at most one logical context
inside a workspace; an existing binding never changes or moves to another context.
Use exact strings, without normalization/URL interpretation. Existing chats whose
ID is already known may establish the binding in their first saved preparation;
their intent identity remains logical and immutable throughout. A conflicting
binding, changed target during approval or observation for another attempt rejects
without a partial saved change. Host-owned logical IDs used by a LiteLLM/server
integration are a distinct identity source, not proof of a browser site's ID.

The candidate key9 context tuple now contains the logical ID, surface, nullable
exact site ID and immutable host-declared UTF-8 site-ID byte budget. The
[current wire packet](conversation-wire-api.md) records strict binding and capacity
cross-checks in the Core/WASM implementation draft. There is no shipped
conversation format to migrate, but final #162/#148 version coordination remains
mandatory. No competing v3 layout may be published under the same version.

## Crash and uncertainty behavior

| Boundary | Required proposed behavior |
| --- | --- |
| First save fails/discards | No send or new durable context/intent/binding; existing state remains unchanged. |
| First save succeeds; tab navigates before send | Do not retarget the approved intent. Only a proved not-attempted resolution could clear it; missing evidence stays pending. |
| Site accepted first send; crash before its ID/confirmation is durably saved | Logical intent stays unbound and pending. Reopening a site URL cannot bind it by guesswork; scoped lookup for an unknown site ID rejects. |
| Same trusted attempt observation remains available | Retry only metadata binding/confirmation against current saved state, never resend. |
| Binding/confirmation commit succeeds but acknowledgment is lost | Reopen and inspect the original intent/terminal record and binding; do not create a second logical chat or resend. |
| Two attempts race to bind the same site ID | Current-state CAS and Core uniqueness checks reject an incompatible loser; it cannot steal or merge the winner's context. |
| Uncertain unbound first send; another known chat is used | Other contexts remain usable; the unresolved intent is never silently attached to that chat. |

This cannot guarantee that every lost observation is recoverable. Without durable
trustworthy association, the first chat can remain paused/unknown after a crash.
That is the existing fail-closed uncertainty guarantee, not invented remote proof.
Recovery/backup preserves both unbound and bound identity state without binding,
resolving or resending anything. First-save confirmation capacity remains the
separately approved reservation contract; a logical ID does not itself solve row/byte/revision limits.

## Alternative considered: scoped first sends unsupported

Keep the exact-site-ID-before-save contract and support scoped sending only where
the trusted host already has an established conversation ID. A new-chat first
message is unsupported in that mode; present that limitation before approval/save
and do not silently use a different restore mode, guess an ID or manufacture an
empty/manual/copied send to obtain one. This is smaller Core/host work but excludes
the normal first-message workflow until a later reviewed design.

Sid approved the immutable logical-context and one-time binding design,
conditional on actual extension evidence for acquiring a
matching accepted-send/site-ID association. If that evidence cannot be established,
ship the explicit unsupported-first-send boundary rather than claiming support.
No provider was called or existing extension changed to obtain the source evidence
in this packet; the Core/WASM implementation does not qualify a shipping adapter. Required future tests cover every crash boundary above, duplicated
tabs/reload/navigation, same-site-ID races, wrong workspace/surface, partial same-
chat batches, all four SDKs and actual reference-host persistence.

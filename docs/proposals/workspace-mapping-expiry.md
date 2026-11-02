# Workspace mapping expiry

## Need

The browser extension keeps one long-lived Workspace per vault. Without a way
to drop old values, every value ever pseudonymized stays in the snapshot. The
extension plans a host-side time-to-live (x days since the value was last
created or restored). Core needs one primitive for that: forget these entries.

Core keeps no clock and no last-used time. The host decides what is stale and
names the entries by marker code.

## API

```rust
impl Workspace {
    pub fn prepare_forget(&self, codes: &[String], operation: PublicId)
        -> Result<PreparedChange>;
}
```

It uses the existing prepare → save → `activate_saved` flow. The plan has no
previews and no policy preview.

- Each code must name a committed entry, at most once. Empty, duplicate,
  unknown or already retired codes fail with `InvalidMarker`.
- More than `MAX_ENTRIES` codes fail with `ResourceLimit`.
- A reused operation id fails like any other prepare.
- Success removes the entries, advances the head and records an operation of
  kind `Forget` (code 11) with zero input and span counts.

## Retired codes

Codes are unique only among live entries. If forgetting freed a code, a later
value could receive it, and an old marker in chat history would restore to the
wrong value. Forgetting therefore retires the code instead:

- Retired codes stay reserved when new codes are drawn.
- `restore_report` reports a retired marker as the new `MarkerIssueKind::Expired`
  ("expired"), with no text, distinct from `Unknown`.
- Progressive restore classifies it the same way.
- Source text that contains a retired marker still fails preparation with
  `InvalidMarker`, as an unknown marker does today.
- Conversation references to a retired code stay valid.
- A value seen again later gets a new marker.

Each retired row keeps only label, code, issuance id, counter and the forget
operation id. The value and its identity key are dropped.

## Snapshot version 4

A forget writes version 4. Workspaces that never forget keep writing version 3,
byte for byte. Version 4 adds key 11: a non-empty array of
`[label, code, issuance, counter, operation]`.

Decoding validates each row (label, code, positive counter), keeps retired codes
disjoint from live codes, counts retired issuances toward issued totals, and
requires the forget operation to exist with kind `Forget` (or fall in
incomplete history). Label counters are now checked after key 11, so a label
whose entries were all forgotten keeps its counter.

**Core 1.0 readers cannot open a version 4 snapshot.** A host should warn
before the first forget, as was done for the version 3 upgrade.

## Scope

- Retired rows have no separate cap. The 16 MiB snapshot limit bounds them to
  roughly 280,000 rows, so live and retired codes together take about 0.03% of
  the 32⁶ code space and the 32 redraw attempts in `issue_code` are not at
  risk. A collision past those attempts returns `ResourceLimit` and never
  reuses a code.
- Forget records carry no `occurred_at_ms`. The host owns time, and the record
  only needs to show that a forget happened.
- Unknown and retired codes reuse `InvalidMarker` rather than adding an error
  code.
- Bindings follow separately: `forget` on Python, Node, WASM and FogVault, plus
  the `expired` issue kind and `snapshotVersion: 4` in typings.

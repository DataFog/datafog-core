# D6/D7 protected policy and remembered choices

Draft for `0.5.0-beta.2`, tracked by #145. Sid approved the narrow protected-type
exception on 2026-10-28 (America/Los_Angeles): an explicit `not_sensitive`
false-positive correction may leave the exact normalized `(entity type, value)`
visible. Core cannot prove that correction truthful; actual PII can therefore be
overridden deliberately. This does not authorize generic `keep` for protected
types, a field/reference authorization scheme, or any source/needs-review bypass.
Protected values remain hidden by default.

## Shared behavior

All types default to hide. Core must apply the same policy in legacy single-input
preparation, batches and recall. Validate every supplied finding before filtering
waived identities, then run existing overlap selection on the eligible findings.
A correction for a narrower or differently typed value does not waive a wider
protected finding. Source and needs-review flags remain display metadata only.

Choices use the existing Unicode lowercase/whitespace identity, without new
punctuation, transliteration, fuzzy or cross-type matching. Changing/removing a
choice affects future preparation and recall only. Previously issued aliases,
first displays and restore mappings stay permanent; a correction never deletes
or reassigns a code. Restoring a committed marker still returns its saved value.

Policy and choice edits must stage immutable candidates, display the requested
before/after edits, export sensitive bytes only to the trusted encrypting host,
and activate exactly once after the normal atomic save/head comparison. Preview,
discard, failed saves, close and stale-head rejection leave committed policy
unchanged. A reopened vault and encrypted backup must carry the same settings.

## Persistence audit and pending reconciliation

The actual current snapshot has seven keys, 0 through 6. Key 6 is the fixed
placeholder `{0: "synthetic_v1", 1: []}`; key 7 does not exist. ADR 004's table is
a target layout, not implemented policy persistence. The current decoder rejects
unknown keys, shape changes and noncanonical CBOR.

The separate unmerged #162 proposal uses snapshot version 2 but preserves that
placeholder and adds operation keys 8/10. A policy implementation cannot silently
ship another incompatible version-2 meaning. Root is reviewing the shared
version/field direction before implementation; no beta.3 operation code is
imported here. Old version-1 read/export byte preservation, explicit saved
upgrade and marker preservation are required in the reviewed plan.

The remaining material choices are the protected-type catalog, whether a choice
may be created before an alias exists, and the coordinated version-2 layout.
They do not reopen Sid's approved explicit `not_sensitive` exception. The proposed
implementation defaults conservatively to hide and must not promise a global
historical operation-ID ledger that the current snapshot does not store.

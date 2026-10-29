# D8 combined review — implementation plan

Target `0.5.0-beta.2`, [#146](https://github.com/DataFog/datafog-core/issues/146).
Sid approved stopping handoff on unresolved overlap rather than exposing part of
another protected finding. Exact per-type/value `not_sensitive` exceptions follow
#145; generic keep for protected types remains prohibited. This document does
not approve a new API shape or change generic transformation behavior by itself.

## Shared Core pipeline

1. Validate source, markers and every scanner/user/correction finding before any
   policy waiver, duplicate resolution or overlap selection. Bound aggregate
   input/result size and reject malformed or stale correction keys explicitly.
2. Add read-only committed recall. Scanner and user input channels establish
   display origin; caller source flags never grant trusted precedence or waive
   protected coverage. Exact duplicate origins fuse deterministically, with
   logical-OR review flags and stable detector metadata selection.
3. Apply committed exact policy and retain all remaining original protected
   intervals as obligations. A manual correction may expand/correct coverage;
   it cannot erase an old protected interval by shrinking or relabelling it into
   a waived identity. Record a type/value false positive through #145 instead.
4. Resolve exact duplicates and whole-cover containment deterministically.
   Corroborate an eligible equal-span native SSN with the Core rule rather than
   trusting an arbitrary caller source/detector string. Unresolved equal-span
   type claims or lost coverage produce content-free original-coordinate
   conflicts; no sanitized text or usable prepared plan is returned.
5. The new authoritative preparation recomputes review against current source,
   choices and head. Legacy single/batch preparation must use the same coverage
   guard. Preserve approval, full-head durable save, single activation, strict
   restore and existing live-plan limits.

Read-only review returns candidates and ranged conflicts, never handoff text.
Correction keys bind to head, exact source and candidate type/range. They are
ephemeral local identifiers, not saved provenance or disclosure authority.
Conflict ranges use original UTF-8 bytes, codepoints and UTF-16 units; a batch
failure identifies its input and leaves the entire workspace unchanged.

## Generic transform investigation

`select_findings` currently validates all inputs, filters selection/allowlists,
then uses the existing overlap comparator. Seven generic operation paths use
that selector. A crossing pair can drop an eligible suffix. A proposed narrow
#84 follow-up checks the union of eligible ranges after existing selection and
returns a content-free conflict before any provider call when coverage is lost.
This changes previously successful generic calls into errors across SDKs and
needs root review of its error/compatibility contract before implementation.
No comparator rewrite, composite marker, or broader address policy is proposed.

## Proof

The shared synthetic corpus covers disjoint findings, duplicate origins,
corroborated SSNs, covering containment, crossing spans in either order,
Unicode/conflict offsets, valid exact waivers, invalid-before-waiver failures,
manual correction coverage and model/recall/user coexistence. Additional tests
cover policy/backup reopening, stale keys/plans, batches, failed save and actual
packed-SDK MV3 edit → fresh review → approval → encrypted save → activation.
No snapshot field or operation/conversation provenance is introduced here.

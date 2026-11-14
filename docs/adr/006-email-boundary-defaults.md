# ADR 006: Trim label syntax in default text-mode email spans

- **Status:** Accepted
- **Date:** 2026-11-14

## Context

The email matcher applies the RFC 5322 local-part atom alphabet, which admits
`=` and `'`. Those characters are also the punctuation that label-style syntax
puts in front of a value, so text-mode spans routinely include them:

- `{"note":"email=alice@example.com"}` reported `email=alice@example.com`
- ``` email = 'dev@example.com' ``` reported `'dev@example.com`
- `SUPPORT_EMAIL=support@example.com` reported the whole string

`format: "env"` and `format: "sql"` already delimit those shapes, but they are
per-caller settings and env mode only recognizes assignments at the start of a
line, so JSON and log lines never benefit. Log-shaped input is the product
wedge; three recorded divergences in `datafog-python`'s native corpus named the
same gap.

## Decision

Text mode refines each email candidate after matching, without changing the
underlying matcher:

1. Drop a leading quote only when the same quote closes immediately after the
   match (`'dev@example.com'`). An unpaired leading quote is a real local-part
   character and stays.
2. Drop a leading `identifier=` prefix. The identifier follows the env
   assignment rules (alphabetic or `_` first, then letters, digits, `_`, `-`),
   and at least one local-part character must remain before the `@`.
3. Only the first identifier-prefixed `=` is a label; inner `=` stays in the
   local part. Digit-led and punctuation-led local parts (`123-45-6789@`,
   `o'connor@`, `orders/billing@`, `first.last+tag@`) are untouched because
   their prefix is not an identifier.

`env` and `sql` keep their existing slice-based semantics, including the rule
that only a line-start `=` separates key and value, which preserves
`EMAIL=customer=tag@example.com` as `customer=tag@example.com`.

## Consequences

This adds and re-spans EMAIL findings, so it is a MINOR release. Three reviewed
expectations moved deliberately:

- `fixtures/email-context.jsonl`: `plain-equals` and `plain-ambiguous` now
  report the trimmed value, matching what `format: "env"` already produced for
  the same input.
- `fixtures/adjacent-entities.json`: the `intact` case that proved `=` inside a
  local part is not split uses `o'connor=tag@example.com`, because a leading
  `customer=` now reads as a label.

A caller who wants the previous whole-string span for a known assignment
document can keep using `format: "env"`, which is unchanged. Downstream
`datafog` merges rule findings with model findings and needs a contract-delta
entry for the new spans.

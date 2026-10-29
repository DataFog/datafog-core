# Street-address baseline: review decision

Status: **human format review needed for 0.5.0-beta.2; no detector enabled**.
This is the remaining street-address slice of
[D2 / #144](https://github.com/DataFog/datafog-core/issues/144), independent of
the masked-account slice. The proposal is based on #159's partial-rule/model
compatibility foundation. It changes no runtime outputs or model ownership.

## Recommended decision

Approve **A: explicit labels first**, using the bounded number-first English
street-line format below. Keep unlabelled addresses with the configured model
and user review. This adds useful deterministic coverage without deciding that
every number/name/suffix phrase in prose is a private address.

**B: unlabelled baseline** is the exact alternative: apply the same value grammar
anywhere in text, without a label. It catches `Send to 12 Willow Road` but also
matches duration/navigation prose such as `We need a 10 minute drive`. That
tradeoff needs human approval and a launch precision target before activation.
Neither option is universal address validation or complete UK/US coverage.

| Input | A: explicit labels | B: unlabelled |
| --- | --- | --- |
| `Street address: 12 Willow Road` | `12 Willow Road` | `12 Willow Road` |
| `Shipping address: 221B Example Street, London` | `221B Example Street` | `221B Example Street` |
| `Send to 12 Willow Road` | No rule finding; retain model coverage | `12 Willow Road` |
| `We need a 10 minute drive` | No rule finding | `10 minute drive` (false-positive tradeoff) |
| `The road to success is long` | No rule finding | No rule finding |
| `Street address: The Rectory` | Outside supported format | Outside supported format |

The proposed corpus `fixtures/proposals/street-address.jsonl` makes both outcomes
explicit. It is acceptance/design data, **not current detector output or a model
qualification set**. `false_positive_tradeoff` rows describe B's cost rather than
claiming the phrase is an address. The human decision is whether to approve A,
approve B's broader tradeoff, or request a different geographic format before
implementation.

## Common proposed value contract

- Canonical type: `STREET_ADDRESS`, distinct from Small's full `ADDRESS`.
- A requires exactly `street address:`, `shipping address:` or `billing address:`
  (ASCII case insensitive), followed by horizontal spacing or no spacing after
  the colon. Bare `address:`, JSON keys, labels without a colon and intervening
  prose do not supply context. B needs no label.
- Value begins with 1–6 contiguous ASCII house-number digits, optionally followed
  by one ASCII letter, then horizontal spacing. This lexical bound is not an
  allocated-address or house-number validation rule.
- Street name: 1–5 Unicode letter tokens, permitting apostrophes (`'` or `’`) and
  hyphens inside a token. Embedded periods, numeric street names, leading unit
  lines, number ranges and slashed numbers are outside this first format.
- Suffix vocabulary, ASCII case insensitive: `Street`/`St`, `Road`/`Rd`,
  `Avenue`/`Ave`, `Lane`/`Ln`, `Drive`/`Dr`, `Court`/`Ct`, `Terrace`/`Ter`, `Way`,
  `Close`, `Place`/`Pl`, `Crescent`/`Cres`. Abbreviations may have one final period.
  Optional following direction: `N`, `S`, `E`, `W`, `NE`, `NW`, `SE`, `SW`.
- Optional single trailing unit: `Apt`, `Suite`, `Unit`, `Flat` or `Floor` plus
  a 1–8 ASCII alphanumeric identifier, with an optional intervening comma and
  horizontal spacing. No leading-unit or multiple-unit grammar is proposed.
- Match the entire supported street line, including a supported trailing unit
  and direction. Stop before a city/region/postcode/country. A comma, semicolon,
  line boundary or end of input terminates the line; sentence-final punctuation
  is outside the span except a period that belongs to a suffix abbreviation.
  Do not accept a shorter prefix of an unsupported leading-unit, numeric range
  or continued value. Unsupported unit continuations need rejection, not silent
  truncation. Fixtures make the intended boundaries concrete.
- Horizontal spacing is space, tab, NBSP or narrow NBSP. A label and its street
  line stay on one line; no multiline capture. Unicode letter/digit/underscore
  adjacency cannot be used to find a candidate inside another token.

B uses the value and termination grammar even where A's label cannot apply (for
example, after a newline). Neither option introduces a global code exemption: a
supported literal phrase in a fenced code block still matches. A closing quote
is not a supported terminator in this initial format.

This is an English-suffix display format that includes some UK/US-style examples,
not a country-wide postal parser. Non-English number-last formats, named premises,
PO boxes, intersections and international component orders remain model/user
review work. A stricter geographic deliverable requires a separate reviewed
corpus; do not infer one from the UK-market discussion alone.

## Existing corpus reconciliation

Reviewed source: [PR #115 fixture file at 54dad9d](https://github.com/DataFog/datafog-core/blob/54dad9d9af4d73e25659b5a766b5c6176fa79409/fixtures/d4-names-addresses.jsonl).
It has 85 records and 38 `STREET_ADDRESS` spans. All annotated codepoint ranges
select their exact declared text. Its stated street-line convention is not
consistent with 12 spans in 11 rows that include city/postcode/country or multiple
lines:

`d4-addr-0007`, `0008`, `0010`, `0027`, `0028`, `0030`, `0034`, `0035`, and
`d4-mixed-0002`, `0005` (two spans), `0010`.

For example, `d4-addr-0007` includes `10 Downing Street\nLondon\nSW1A 2AA`;
`d4-addr-0030` includes the street plus city/postcode and Canada. Keep their offset
validity separate from the boundary-policy mismatch. #115 also includes PO boxes,
intersections and leading-unit forms outside the proposed baseline. Do not
silently relabel its gold data or reuse its reported native GLiNER2 figures as
Small qualification. Its model-quality work remains separate from D2's narrow
rule coverage.

Before pilot scoring, approve street-only vs full-address annotations, reconcile
these records, and score actual Small `ADDRESS` output under its own agreed
alignment. The rule proposal does not solve the separate `ADDRESS` to
`STREET_ADDRESS` scoring contract.

## Compatibility requirements for implementation

Register a future partial street rule without replacing configured native/custom
model `STREET_ADDRESS` coverage, using #159's internal distinction. Keep model
`ADDRESS` labels unchanged. Default scans may combine a narrow street rule with
a model's wider span; existing Core selection and evidence composition apply,
with no blanket source precedence. Do not rename or truncate model full addresses
to make a street-only score pass.

Runtime acceptance must cover positive/hard-negative formats, selected and
default scans, all offset coordinate systems, structured field boundaries,
allowlists, provider round trips and model routing. Retain existing precision
and recall thresholds, installed SDK smoke and live parity. Add the user-facing
changelog and public examples when code is enabled. This proposal keeps #144
open and cannot by itself mark D2 implemented or beta.2 ready.

# Labelled account-number rule contract

Status: **0.5.0-beta.2 draft; not released**.
This is the account-number slice of [D2 / #144](https://github.com/DataFog/datafog-core/issues/144).
[Masked account values](masked-account-number-rule-contract.md) have a separate
format contract; street addresses remain separately scoped. The synthetic
cases in `fixtures/account-number.jsonl` are rule acceptance cases and reusable
launch fixtures, not model qualification results.

## Supported text

The canonical type is `ACCOUNT_NUMBER`, with value-only spans and
`datafog-core/account-number` rule provenance. It runs in all locales by
default and honors ordinary scan selection, transformation selection and allowlists.

Recognize ASCII-case-insensitive `account number`, `account no.`, `acct number`
and `acct no.` labels. The period on `no.` is required in this first slice.
A preceding `bank` is ordinary surrounding text, not a distinct required label.
Words within the label require horizontal spacing. Between label and value,
require horizontal spacing or a single colon/hash with optional horizontal
spacing. Horizontal spacing means space, tab, nonbreaking space or narrow
nonbreaking space; it never crosses a newline.

The value is **6–17 contiguous ASCII digits**, including leading zeros. This is
a deliberately bounded initial lexical format, not bank validation or a claim
that all account numbers have this length. It covers an eight-digit UK-style
synthetic example and longer synthetic identifiers while avoiding short numeric
references. Four/five-digit, longer, alphanumeric, international, hyphenated and
space-grouped accounts are outside this first rule; a configured model may still
find them. Checksums, account existence and institution ownership are not checked.
No real account numbers appear in these fixtures.

Use Unicode word boundaries around the label and value, rejecting adjacent
letters, digits, underscores or combining marks. Do not accept a prefix of a
longer run, decimal, date or split numeric value: a digit after a following
period/comma/slash/hyphen or horizontal spacing disqualifies the whole candidate.
Terminal punctuation or punctuation followed by prose is permitted.

## Exclusions and rationale

- Bare numbers, `reference`, `order`, `invoice`, `customer ID`, generic `account:`
  and `acct:` have insufficient context for this slice.
- Intervening prose (`account number is ...`) and line breaks separate the label
  from its value and are rejected.
- Negative numbers, date/decimal fragments and partially matched digit groups are
  rejected; digits already matched by PHONE, SSN or CREDIT_CARD are not globally
  suppressed or reclassified. A labelled valid card still yields its CREDIT_CARD
  finding. Existing equal-span selection chooses ACCOUNT_NUMBER for a default
  transformation; selecting CREDIT_CARD keeps card attribution. This intentional
  new overlap is covered by a regression, not a new source-precedence rule.
- Masked accounts follow the separate masked-value contract; bare endings or
  masked tokens still lack account context.
- JSON field keys do not supply text context; scan each decoded string as today.

Each positive has a value, exact codepoint offsets and transformation
expectations. Each negative names its exclusion. Conformance tests
check the fixture coordinates and, separately, preserve current model routing.
The implementation proves all cases through Rust and installed Python,
Node and browser WASM, including byte/codepoint/UTF-16 offsets, redact/remove/mask,
entity selection, allowlists and existing provider-backed transformation policy.

## Model compatibility

`ACCOUNT_NUMBER` is explicitly partial in the internal detector registry. It
adds rule findings without replacing configured model coverage, for omitted and
explicit entity selection alike. Native and custom models still execute for
accounts, including formats this lexical rule excludes. Existing entity routing
for EMAIL, PHONE and the other rule types is unchanged.

Equal-span, equal-type findings use current duplicate selection and keep both
sources of evidence. Different spans remain available to the existing
transformation selection policy; neither source gets blanket precedence. Raw
`scan_model` behavior is unchanged. This internal compatibility distinction does
not add the caller-visible engine routing controls tracked in
[#126](https://github.com/DataFog/datafog-core/issues/126). Contract regressions
protect model admission and wider coverage without treating synthetic backend
spans as real model qualification.

## Verification

Run doctor, the rule precision/recall checks, installed SDK smoke and live parity;
this corpus runs in the Core and installed SDK conformance paths. Keep the
existing corpus unchanged unless an intentional coverage change is reviewed. Measure binary and
latency impact before making performance claims. This slice leaves
#144 open; street-address coverage remains separately scoped.


## Local impact measurement

On macOS ARM64 with Node 24.18.0, 49 short synthetic fixture texts scanned 1,000
times each after warmup averaged 0.68 µs before and 1.06 µs after this addition.
This includes returning the new account findings and is an uncontrolled local
measurement, not a stable benchmark claim. The release WASM grew from 2,452,055
to 2,458,647 bytes (+6,592 bytes, about 0.27%). The ordinary `cargo dfv bench`
report remains the separate performance gate.

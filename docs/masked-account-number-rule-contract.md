# Masked account-number rule contract

Status: **0.5.0-beta.2 draft; not released**. This is the masked-account slice of
[D2 / #144](https://github.com/DataFog/datafog-core/issues/144), stacked on the
labelled account rule in [#159](https://github.com/DataFog/datafog-core/pull/159).
Street addresses and masked non-account entity types remain outside this slice.

## Supported formats and ranges

Use the existing canonical `ACCOUNT_NUMBER` type and
`datafog-core/account-number` provenance for two explicit text forms:

- `account ending in 1234`, `acct ending in 1234`, or one of the existing full
  account-number labels (`account number`, `account no.`, `acct number`, `acct
  no.`) followed by `ending in 1234`. Match exactly four ASCII suffix digits;
  retain `ending in` outside the value span.
- An existing full account-number label and delimiter followed by 4–12 copies
  of ASCII X (case insensitive) or 4–12 asterisks, immediately followed by four
  ASCII digits: `account number: XXXX1234` or `acct no.#****1234`. Match the whole
  visible masked token, including its mask characters. Mixed X/asterisk masks,
  grouped masks, other mask symbols and longer/shorter tokens are excluded.

The 4–12 mask bound is a deliberately supported display format, not an account
length standard or an estimate of how many digits are hidden. ASCII case
insensitivity applies to labels and X masks. As in #159, horizontal spacing is
space, tab, NBSP or narrow NBSP; labels and values never span a line break.
Suffix phrases require horizontal spacing between every word. Masked-token
labels use the existing mandatory whitespace or single colon/hash separator.
Unicode word boundaries apply around labels and after the digit suffix; the
existing numeric-continuation guard rejects partial decimal/date/group matches.

## Exclusions

Bare `ending in 1234`, `XXXX1234` and `****1234` have insufficient entity context.
Credit-card, phone, SSN, invoice and reference labels do not activate this account
rule. Intervening prose, unsupported labels such as `account ends in`, Unicode
suffix digits, mixed masks and code-style forms such as `account_number =
XXXX1234` are excluded. This is a lexical rule, not a general code/Markdown
exemption: a supported literal account phrase inside a code block still matches.
Structured field keys do not supply text context or cross field boundaries.

Three negatives from #159 intentionally become positives in this slice:
`account-number-suffix-only` (`account ending in 1234`, value `1234`),
`account-number-x-masked` (`account number: XXXX1234`, value `XXXX1234`) and
`account-number-star-masked` (`account number: ****1234`, value `****1234`).
Their acceptance data is updated explicitly; the original contiguous-number
formats and every other earlier negative keep their behavior.

## Literal identity and model compatibility

A suffix is not a full account identity. Do not reconstruct digits, equate it
with a full number, or promise account-level deduplication or recall. Workspace
identity continues to use the observed entity type plus its existing normalized
literal text. Equal literal suffixes can reuse an alias even if they came from
different real accounts; `1234`, `XXXX1234` and `00121234` remain different
literal identities. X-case normalization follows existing workspace behavior.
Restoration returns the observed literal, not hidden information.

No workspace identity, recall or source-precedence policy changes here. The
partial account rule from #159 still runs alongside a configured account model;
it does not take ownership of the model's broader account coverage. Normal
selection, exact/regex allowlists and all existing transformation strategies
apply to the observed value span.

## Proof

`fixtures/masked-account-number.jsonl` contains synthetic supported examples and
hard negatives with exact offsets and transformation expectations. Core and
installed Python, Node and real browser WASM consume the same cases. Verify
byte/codepoint/UTF-16 offsets, selection, allowlists, structured boundaries,
provider-backed round trips, literal workspace identity and retained model
routing. Run the required rule checks, SDK smoke, live parity and regression
evaluation without weakening existing thresholds. These fixtures do not qualify
model inference. #144 remains open for the separately scoped street-address rule.


## Local impact measurement

On macOS ARM64 with Node 24.18.0, all 76 short masked fixture texts scanned 1,000
times each after warmup averaged 0.54 µs on #159 and 1.12 µs with the new coverage.
The latter also returns the new findings; this uncontrolled local measurement is
not a stable benchmark claim. Release WASM size changed from 2,458,647 to
2,457,641 bytes. The ordinary `cargo dfv bench` report is the separate performance
gate.

# Python Regex Baseline

Reference implementation for comparison only. This document does not constrain
the Rust implementation.

## Pin

- Repository: `datafog/datafog-python`
- Version: `4.8.0a6`
- Commit: `75e414b23a4c9be1938263f509354e2cb4d886e2`
- Invocation: `scan(text, engine="regex")`

## Scan Path

`scan` creates a `RegexAnnotator`, runs one Python `re` pattern per active
label, maps internal `DOB` to `DATE` and `ZIP` to `ZIP_CODE`, suppresses
overlapping entities, then sorts by start offset.

Patterns are compiled when the annotator is created, so they are compiled on
each public `scan` call. No NER model is loaded in this path.

## Detection Behavior

| Returned label | Baseline behavior |
| --- | --- |
| `EMAIL` | Permissive ASCII RFC-5322 subset. Requires `@`, a dotted domain, and an alphabetic TLD of at least two characters. Excludes certain embedded and assignment-like forms. |
| `PHONE` | Matches North American 3-3-4 forms with optional `+1`, plus `+`-prefixed international examples. Allows spaces, hyphens, dots, and parentheses. Does not validate numbering plans or extensions. |
| `SSN` | Matches dashed or undashed nine-digit values. Rejects area `000`/`666`, group `00`, and serial `0000`; does not reject `900`–`999` area numbers. |
| `CREDIT_CARD` | Matches Visa (13 or 16 digits), Mastercard (`51`–`55`), and American Express (`34`/`37`) in selected continuous or separated forms. Does not use Luhn validation and does not support Discover or the newer Mastercard range. The unformatted American Express alternative is end-of-string only. |
| `IP_ADDRESS` | Matches strict dotted IPv4 octets in the `0`–`255` range. Does not match IPv6. |
| `DATE` | Matches US numeric month-first dates with two- or four-digit years, `YYYY-MM-DD`, month-name dates, and `year YYYY`. It checks month/day ranges but not actual calendar validity, so invalid calendar dates can match. |
| `ZIP_CODE` | Matches any word-boundary-delimited five-digit value, with an optional `-dddd` extension. It does not validate assignment to a real location. |

## Differences from Rust Design

| Label | Rust difference | Rust action |
| --- | --- | --- |
| `EMAIL` | No intentional functional difference is defined yet. | No change. |
| `PHONE` | Rust validates normalized digit counts: 10 for North American numbers and 7–15 for `+`-prefixed international numbers. Python accepts 6–19 digits through broader digit-group rules. | No change; retain Rust's narrower range. |
| `SSN` | Rust rejects area numbers `900`–`999`; Python does not. | No change; retain the stricter validation. |
| `CREDIT_CARD` | Rust requires a valid Luhn checksum and supports Discover plus the newer Mastercard range; Python does neither. | No change; retain validation and coverage. |
| `IP_ADDRESS` | Rust supports IPv4 and IPv6; Python supports IPv4 only. | No change; retain IPv6 support. |
| `DATE` | Rust requires a four-digit year and a real calendar date; Python accepts two-digit years and invalid calendar dates. Rust excludes `year YYYY`. | No change; retain the narrower date policy. |
| `ZIP_CODE` | No intentional functional difference is defined. | No change. |

## Output Behavior

- Offsets are zero-based Python string positions (Unicode code points).
- Entity text is the exact matched substring.
- Overlaps are suppressed: longer matches win; then priority is `CREDIT_CARD`, `IP_ADDRESS`, `SSN`, `PHONE`; remaining ties use confidence and deterministic fields.
- Returned entities are sorted by start, end, then type.

## Source

- `datafog/engine.py`: public regex scan path, label mapping, overlap handling.
- `datafog/processing/text_processing/regex_annotator/regex_annotator.py`: pattern definitions and span extraction.

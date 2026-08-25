# Rust `scan` Design

## Shared Scanning Framework

Shared code owns:

- accepting input text;
- collecting candidate matches;
- constructing `(label, text, start, end)` entities;
- Unicode offset conversion;
- ordering, duplicate handling, and overlap handling.

Each label owns its detection logic.

## Detection Strategy by Label

### `EMAIL`

- **Strategy:** Compiled ASCII email regex.
- **Candidate:** Local part + `@` + dotted domain + alphabetic top-level domain.
- **Rules:** Match the complete address; exclude surrounding punctuation; require a dotted domain (`user@localhost` is not an email). Do not support quoted local parts, comments, DNS checks, or internationalized email addresses.

### `PHONE`

- **Strategy:** Candidate matcher plus digit normalization and validation.
- **Candidate:** North American 10-digit numbers, with optional `+1`; international numbers beginning with `+`.
- **Rules:** Allow spaces, hyphens, dots, and parentheses. Require 10 digits for North American numbers and 7–15 digits for international numbers. Reject candidates embedded in letters or longer numeric identifiers. Do not support extensions.

### `SSN`

- **Strategy:** Structured parser.
- **Candidate:** `123-45-6789` or `123456789`.
- **Rules:** Remove dashes, then require exactly nine digits. Reject area numbers `000`, `666`, and `900`–`999`; group number `00`; serial number `0000`; and candidates embedded in a larger alphanumeric token or longer number. Do not support spaces or other separators.

### `CREDIT_CARD`

- **Strategy:** Candidate matcher, digit normalization, issuer-prefix validation, and Luhn validation.
- **Candidate:** 13–19 digits with optional spaces or hyphens.
- **Rules:** Remove separators; require a valid Luhn checksum and a known issuer prefix: Visa (`4`), Mastercard (`51`–`55` or `2221`–`2720`), American Express (`34` or `37`), or Discover standard prefixes. Reject candidates embedded in longer numbers or alphanumeric identifiers. Do not detect CVVs, expiry dates, or arbitrary 13–19 digit values. Return the original matched text, including formatting.

### `IP_ADDRESS`

- **Strategy:** Extract an address-shaped candidate, then validate it with Rust's standard IP parser.
- **Candidate:** IPv4 or IPv6 literal.
- **Rules:** Detect private, loopback, and public addresses. Return only the address, excluding surrounding brackets, ports, and CIDR suffixes. Do not resolve hostnames or perform network lookups. Reject invalid forms, such as `999.1.1.1`.

### `DATE`

- **Strategy:** Candidate matcher followed by calendar validation.
- **Candidate:** `YYYY-MM-DD`, `MM/DD/YYYY`, `MM-DD-YYYY`, or a full/abbreviated month name followed by day and four-digit year.
- **Rules:** Require a real calendar date, including leap-year handling, and return the original matched text. Numeric dates are US month-first only. Do not support ambiguous `DD/MM/YYYY`, bare years, compact digit strings, or date-like version numbers.

### `ZIP_CODE`

- **Strategy:** Structured matcher.
- **Candidate:** Five-digit US ZIP code or ZIP+4 (`12345` or `12345-6789`).
- **Rules:** Require boundaries so the candidate is not embedded in a longer number, word, or identifier. Treat any boundary-delimited five-digit value as a `ZIP_CODE`. Do not validate whether the ZIP is assigned to a real location or support international postal codes.

## Offsets and Unicode

- Use UTF-8 byte ranges internally for detection and slicing.
- Convert returned `start` and `end` offsets to zero-based Unicode code-point positions, matching Python.
- Return the original matched text without Unicode normalization or modification.

## Ordering, Duplicates, and Overlaps

- Sort entities by `start` ascending.
- For entities with the same `start`, place the longer match first.
- Remove exact duplicates: same label, start, end, and text.
- Keep non-identical overlaps rather than discarding either entity.

## Performance Considerations

### Initial Implementation Choices

- Compile reusable regex matchers once and reuse them.
- Use direct byte-based parsers for `SSN`, `CREDIT_CARD`, `DATE`, and `ZIP_CODE`.
- Validate `IP_ADDRESS` candidates with Rust's standard IP parser.
- Keep input borrowed; record byte ranges and allocate only returned entities.
- Use an internal `Label` enum and static lookup data for fixed labels, month names, card prefixes, and validation rules.
- Use an ASCII fast path: byte offsets are public offsets when input is ASCII.
- Collect candidates once, then sort, deduplicate, and resolve overlaps once in the shared framework.
- Keep each `scan` call single-threaded and dependency-free: no model loading, network access, or I/O.

### Deferred Optimizations

- Use a trigger-driven, single-pass scanner only if benchmarks show separate detector passes are a bottleneck.
- Parallelize across independent inputs in batch processing, not within one `scan` call.

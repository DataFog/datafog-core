# Rust Entity Detection Strategy

This document describes the control flow used by the Rust core to detect each supported PII label.
Every detector adds byte-range candidates; `finalize` then sorts and deduplicates them, extracts the matched text, and converts offsets to Unicode code-point offsets when necessary.

## Email (`EMAIL`)

```text
scan(text)
   |
   v
detect_email(text, candidates)
   |
   v
EMAIL_RE.find_iter(text)
   |
   +--> no regex match ───────────────> continue looking
   |
   +--> match found
           |
           v
      Add Candidate {
        label: Email,
        start_byte: match.start(),
        end_byte: match.end()
      }
           |
           v
scan() runs other detectors
           |
           v
finalize(text, all candidates)
   |
   +--> sort and remove duplicate candidates
   |
   +--> convert byte offsets to Unicode code-point offsets
   |
   v
Entity {
  label: "EMAIL",
  text: matched substring,
  start,
  end
}
```

The email regex determines whether a substring is an email; each match becomes an `Email` candidate.

## Phone (`PHONE`)

```text
scan(text)
   |
   v
detect_phone(text, candidates)
   |
   v
PHONE_RE.find_iter(text)
   |
   +--> no regex match ───────────────> continue looking
   |
   +--> match found
           |
           v
      has_phone_boundaries(text, start, end)
           |
           +--> adjacent letter/digit ──> reject match
           |
           +--> valid boundaries
                    |
                    v
            is_valid_phone(match)
                    |
                    +--> invalid digit count ──> reject match
                    |
                    +--> valid digit count
                             |
                             v
                     Add Candidate {
                       label: Phone,
                       start_byte: match.start(),
                       end_byte: match.end()
                     }
                             |
                             v
scan() runs other detectors -> finalize(text, all candidates)
```

Phone matches must not be embedded in alphanumeric text. Domestic numbers require 10 digits, or 11 beginning with `1`; `+`-prefixed international numbers require 7–15 digits.

## Social Security Number (`SSN`)

```text
scan(text)
   |
   v
detect_ssn(text, candidates)
   |
   v
Walk through each byte position in text
   |
   +--> not a digit, or preceded by letter/digit ──> advance 1 byte
   |
   +--> possible SSN start
           |
           v
      ssn_parts_at(bytes, start)
           |
           +--> no valid shape
           |      (NNN-NN-NNNN or NNNNNNNNN)
           |        └────────────────────────────> advance 1 byte
           |
           +--> parse end, area, group, serial
                    |
                    v
          Followed by a letter/digit?
                    |
                    +--> yes ────────────────────> advance 1 byte
                    |
                    +--> no
                             |
                             v
              is_valid_ssn(area, group, serial)
                             |
                             +--> invalid ───────> advance 1 byte
                             |
                             +--> valid
                                      |
                                      v
                             Add Candidate { label: Ssn, start_byte, end_byte }
                                      |
                                      v
                             Continue from end of SSN -> finalize(...)
```

The detector supports dashed and undashed SSNs. It rejects reserved values: area `000`, `666`, or `900`–`999`; group `00`; and serial `0000`.

## Credit Card (`CREDIT_CARD`)

```text
scan(text)
   |
   v
detect_credit_card(text, candidates)
   |
   v
Walk through each byte position in text
   |
   +--> not a digit, or preceded by letter/digit ──> advance 1 byte
   |
   +--> possible card-number start
           |
           v
      card_parts_at(bytes, start)
      Collect digits, allowing single spaces or hyphens
           |
           v
      Empty candidate, or followed by letter/digit?
           |
           +--> yes ─────────────────────────────> advance 1 byte
           |
           +--> no
                    |
                    v
      13–19 digits and a supported issuer prefix?
                    |
                    +--> no ─────────────────────> advance 1 byte
                    |
                    +--> yes
                             |
                             v
                     passes_luhn(digits)?
                             |
                             +--> no ─────────────> advance 1 byte
                             |
                             +--> yes
                                      |
                                      v
                     Add Candidate { label: CreditCard, start_byte, end_byte }
                                      |
                                      v
                     Continue from end of card number -> finalize(...)
```

Supported issuer prefixes cover Visa, American Express, Mastercard, and selected Discover ranges. The number must also pass the Luhn checksum.

## Date (`DATE`)

```text
scan(text)
   |
   v
detect_date(text, candidates)
   |
   v
Walk through each byte position in text
   |
   +--> not a digit/letter, or preceded by letter/digit
   |       └─────────────────────────────────────> advance 1 byte
   |
   +--> possible date start
           |
           v
      date_parts_at(bytes, start)
           |
           +--> Try numeric date: YYYY-MM-DD, MM/DD/YYYY, or MM-DD-YYYY
           |
           +--> Otherwise try named date: MonthName Day, YYYY
           |
           +--> no supported form ───────────────> advance 1 byte
           |
           +--> parsed end, year, month, day
                    |
                    v
      Followed by a letter/digit?
                    |
                    +--> yes ────────────────────> advance 1 byte
                    |
                    +--> no
                             |
                             v
      is_valid_date(year, month, day)
      - year >= 1000
      - month is 1–12
      - day exists in that month (including leap years)
                             |
                             +--> invalid ───────> advance 1 byte
                             |
                             +--> valid
                                      |
                                      v
                             Add Candidate { label: Date, start_byte, end_byte }
                                      |
                                      v
                             Continue from end of date -> finalize(...)
```

The parser validates calendar reality after recognizing a supported date shape, so `2024-02-29` succeeds while `2023-02-29` fails.

## ZIP Code (`ZIP_CODE`)

```text
scan(text)
   |
   v
detect_zip_code(text, candidates)
   |
   v
Walk through each byte position in text
   |
   +--> not a digit, or preceded by letter/digit ──> advance 1 byte
   |
   +--> possible ZIP-code start
           |
           v
      zip_end_at(bytes, start)
           |
           +--> 5 digits?
           |      |
           |      +--> no ───────────────────────> advance 1 byte
           |      |
           |      +--> yes
           |               |
           |               +--> next character is '-'?
           |                     |
           |                     +--> no ──> ZIP end = after 5 digits
           |                     |
           |                     +--> yes
           |                              |
           |                              v
           |                    exactly 4 more digits?
           |                              |
           |                              +--> no ─> reject; advance 1 byte
           |                              |
           |                              +--> yes -> ZIP+4 end
           |
           v
      Followed by a letter/digit?
           |
           +--> yes ─────────────────────────────> advance 1 byte
           |
           +--> no
                    |
                    v
           Add Candidate { label: ZipCode, start_byte, end_byte }
                    |
                    v
           Continue from end of ZIP code -> finalize(...)
```

The detector supports five-digit ZIP codes and ZIP+4 values (`12345-6789`), rejecting candidates embedded in larger alphanumeric strings.

## IP Address (`IP_ADDRESS`)

```text
scan(text)
   |
   v
detect_ip_address(text, candidates)
   |
   v
Walk through each byte position in text
   |
   +--> not a hex digit or ':', or preceded by letter/digit
   |       └────────────────────────────────────────> advance 1 byte
   |
   +--> possible IP-address start
           |
           v
      Try IPv4 first: ipv4_end_at(bytes, start)
      Requires four 1–3 digit groups separated by '.'
           |
           +--> possible IPv4
           |       |
           |       v
           |   Valid boundaries?
           |   Not part of a longer dotted number?
           |   Parses as std::net::Ipv4Addr?
           |       |
           |       +--> all yes ──> add IP_ADDRESS candidate
           |       |                    |
           |       |                    v
           |       |              continue from match end
           |       |
           |       +--> any no ──> try IPv6
           |
           +--> not IPv4 ───────────> try IPv6
                                       |
                                       v
                         ipv6_end_at(bytes, start)
                         Collect hex digits and ':'; require a ':'
                                       |
                                       v
                         Valid boundaries?
                         Parses as std::net::Ipv6Addr?
                                       |
                                       +--> both yes ─> add IP_ADDRESS candidate
                                       |                   |
                                       |                   v
                                       |             continue from match end
                                       |
                                       +--> either no ─> advance 1 byte
                                                        |
                                                        v
finalize(text, all candidates)
   |
   v
Entity {
  label: "IP_ADDRESS",
  text: matched substring,
  start,
  end
}
```

The detector attempts IPv4 before IPv6 and uses Rust's standard `Ipv4Addr` and `Ipv6Addr` parsers as the final validity check.

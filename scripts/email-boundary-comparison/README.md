Run from the repository root:

```sh
cargo run --release --manifest-path scripts/email-boundary-comparison/Cargo.toml > target/email-boundary-comparison.json
```

The committed harness compares this checkout with exact published Rust versions
0.4.4 and 1.7.0. It visits every combination of 20 surrounding contexts, 12 email
shapes, eight separators and three source wrappers: 1,382,400 texts containing
6,912,000 planted emails, SSNs, cards and phones. The contexts include ASCII,
Latin accents and combining marks, Hiragana, Han, Hangul, Cyrillic, Arabic,
Hebrew, Devanagari, Bengali, Thai, Khmer, Georgian, Armenian, Greek and emoji.
Wrappers are text, ENV and SQL syntax; all versions use their default scanner.

The run fails if a planted value covered by its own detector in either baseline
loses that coverage. It separately reports incidental coverage lost when a
different detector's over-wide span previously swallowed a value. Each category
includes its first source text and complete findings, so these cases can be
reproduced and judged without treating accidental EMAIL coverage as a PHONE
detection guarantee. This finite matrix is a regression comparison, not a claim
that every Unicode address or context is recognized.

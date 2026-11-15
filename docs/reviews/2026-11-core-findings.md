# Core 1.4 review disposition

This maps the independent November 15, 2026 review to the Core 2.0 candidate.
Detection fixes, transformation safety changes, binding safety and packaging
have regression coverage; supported lexical limits are stated explicitly below.
Core 2.0 is required by the existing version policy because masking and combined
transformation behavior change. No wire contract identifier changes.

| Finding | Resolution | Evidence |
| --- | --- | --- |
| CORE-1 | Complete IPv4-mapped IPv6 candidates are validated and transformed as one span. | `review_regressions::mapped_ipv6_is_one_complete_finding`, shared development fixtures. |
| CORE-2 | Text-mode emails trim field/query delimiters, dotted keys and leading markup/URL slashes. Malformed doubled-dot local parts are rejected; Unicode mailbox letters retain exact offsets. | `review_regressions::email_values_do_not_absorb_fields_or_markup`, shared Unicode/field fixtures. |
| CORE-3 | Version and length parsing are bounded to 20 digits and stop at the first non-digit; no repeated full-tail header searches. | Reserved-token regression cases and canonical token provider suites. |
| CORE-4 | Tokenization rejects malformed reserved prefixes before provider calls, matching strict restore grammar. Literal malformed prefixes require removal before tokenization. | Reserved-token regression cases; tokenization/restoration guide. |
| CORE-5 | Python configuration conversion tracks container ancestors and limits depth to 64; repeated noncyclic references remain valid. | Installed `binding_safety_conformance.py`. |
| CORE-6 | Non-awaitable key/token/restore callback results raise a sanitized provider error promptly. | Installed timeout-bounded key/token/restore cases in `binding_safety_conformance.py`. |
| CORE-7 | Combined operations merge scanner-produced intersecting spans into their union after selection and allowlists. Explicit transforms retain strict conflict review. Applied in Rust/Python, Node provider preparation, WASM and JSON fields. | Scanner coverage regression; existing explicit-conflict conformance; configuration/migration guide. |
| CORE-8 | Reject bare `::`, ambiguous short hex pairs and single-hextet prefixes; retain loopback, full addresses and supported long multi-group forms. | Mapped/ambiguous IPv6 regression, cached-run tests and SDK fixtures. |
| CORE-9 | ISO date prefixes before `T` plus a digit are recognized. Other reported formats remain intentionally unsupported: variable-width numeric, day-first, day-first English, month-first without a comma, `Sept`, dotted dates and short years. Supported exact formats are documented. | ISO timestamp regression; existing date negatives; Core 2 migration guide. |
| CORE-10 | README installation versions are per registry and link actual Python tags; Rust/npm remain at their verified published 1.1.0 versions until separately released. Phone rule and all ten Python wheel targets are accurately described. | Registry API verification; README/install documentation. |
| CORE-11 | Mask reveal counts leave at least one code point masked, even when requested count exceeds length. Count four on a five-character ZIP deliberately still reveals four, preserving partial-mask configuration. | First/last/zero/oversized reveal regressions; updated existing expectation. |
| CORE-12 | Wheel contains MIT, embedded Apache and CC0 notices with `License-File` metadata; sdist includes root MIT and Rust toolchain pin. | Archive-byte checks and extracted-source rebuild; `test-python-distributions.py`. |
| CORE-13 | Explicit platform/source-build policy lists wheel gaps, free-threaded and PyPy source builds, Rust 1.88 requirements and no-toolchain/offline implications. The ten supported targets remain the qualification matrix. | Installation and versioning docs; source archive/rebuild proof. |
| CORE-14 | Python classes expose their module; Finding/TextRange hash, pickle and deepcopy work; repr is Python-style; reversed ranges raise; package version is exported. Deprecated public panic helper becomes a non-panicking stub, omitted from `__all__`; private probe remains for tests. | Installed binding safety and contract inventory checks. |
| CORE-15 | Caller-provided entity labels reject empty strings, whitespace and marker punctuation before transformation. | Invalid-marker regression; stable invalid-finding error path. |
| CORE-16 | Isolated digit groups are skipped before expensive card parsing, preventing the reported short-group slowdown. | 200 KB single-digit regression; optimized candidate benchmark. |
| CORE-17 | Fixed phone decimal suffixes, newline-spanning phones, labelled tax-ID phones, ZIP price/hyphen fragments, phone+ZIP cards, German IBAN card fragments, labelled IMEI cards, URL-userinfo API keys, escaped-newline PEM and OpenPGP private armor. Added JCB/Diners/Maestro and Luhn-valid UnionPay prefixes. | Precision, armor and native credential regressions; existing unchanged corpora. |
| CORE-18 | Caught binding panics use fixed safe messages; a thread-scoped panic-hook guard suppresses caught payloads on stderr while preserving unrelated host panic behavior. | Subprocess panic confidentiality tests and Rust binding unit tests. |
| CORE-19 | Publish workflow validates archives and installs/imports each actual wheel on a matching native or libc/architecture environment before artifact upload; actions use immutable SHAs. | Actionlint, artifact validator positive/negative tests, native wheel/source smoke. Foreign-target execution is performed by CI, not claimed from macOS. |

CORE-17 intentional lexical limits: `2024010112` and `order #2125551234`
remain possible PHONE findings; `order 123456789 shipped` remains a bare-format
SSN candidate. They cannot be disambiguated reliably without context. Card
numbers use 13–19 Luhn-valid digits and supported issuer prefixes; repeated
spaces and period separators remain unsupported. German postal findings retain
the full label-prefixed value (`PLZ 10115`) for existing locale fixtures. These
limits are documented in the Core 2 migration guide rather than presented as
solved detection claims.

The embedded CNN strips trailing punctuation from predicted spans (PY-L10).
Model resource limits remain enforced in Core; the Python adapter chunks requests
with overlap to resolve PY-M15 without lifting native limits. The core phone-plus-
ZIP candidate exclusion resolves the source of PY-M16.

Required validation commands are `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo dfv build`, `cargo dfv doctor`, `cargo dfv check`, `cargo dfv smoke`,
`cargo dfv parity --live`, `cargo dfv eval`, release/contract checks and Python
archive/installed-consumer checks. Execution reports are retained under
`target/dfv/runs/`; CI retains the hosted evidence for the exact candidate commit.

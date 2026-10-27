# D4 Test Set: Names and Addresses

The D4 test set provides labelled evaluation data for person names (`PERSON`) and
street addresses (`STREET_ADDRESS`). It's the detection measurement fixture for
[ADR 004](adr/004-workspace-aliasing-v1.md) and [workspace-v1-plan.md](workspace-v1-plan.md).

## Purpose

Rules detect emails, phones, card numbers, SSNs, routing numbers, and credentials
well. Names and street addresses in free text are the gap — rules miss most of
them. The D4 set measures how well GLiNER2 (and later models) close that gap.

**Detection bar (ADR 004):**
- At least 90% of names caught before review, using GLiNER2's `person` label at
  threshold 0.5.
- Street-address recall is an open measurement until a stable baseline exists.

## Fixture file

`fixtures/d4-names-addresses.jsonl` — 86 records with 53 `PERSON` and 45
`STREET_ADDRESS` entities, plus hard negatives.

## Entity coverage

### PERSON (53 entities across 44 positive fixtures)

- **Multi-part names:** First + Last, First + Middle + Last
- **Hyphenated names:** García-López, Tanaka-Williams, O'Reilly-Chen
- **Non-English names:** Chinese (李明), Japanese (Tanaka Yuki), Vietnamese
  (Nguyễn Văn Minh), Arabic (Ahmed Al-Rashid, Mohammed bin Salman Al-Faisal),
  Icelandic (Björk Guðmundsdóttir)
- **Diacritics:** María, François, Özlem, Søren, Chloé
- **Names near code:** Inside strings, code comments, function calls
- **Names with punctuation:** Titles (Dr.), suffixes (Jr.), colons, parentheses
- **ALL CAPS names:** WONG, DAVID MING-HO

### STREET_ADDRESS (45 entities across 35 positive fixtures)

- **US addresses:** Standard (123 Main Street, City, ST 12345), with ZIP+4
- **Apartment/unit numbers:** Apt 4B, Suite 3400, Unit 7, Floor 12
- **International formats:** UK (W1U 8EW), German (10117 Berlin), Japanese
  (163-8001), French (75001 Paris), Canadian (N2J 2W2)
- **PO Boxes:** PO Box 1234, Small Town, MT 59001
- **Multi-line addresses:** Newline-separated components
- **Directionals:** N., S.W., NW
- **Named streets:** Martin Luther King Jr. Blvd, Massachusetts Ave

### Hard negatives (20 fixtures)

- **Company names:** Apple, Microsoft
- **Publication names:** The Washington Post
- **Street names as places:** Broadway, Madison Avenue, Wall Street
- **Code patterns:** `struct Address {}`, function parameters
- **Generic text:** "Enter your shipping address"

## Synthetic data policy

All names and addresses are synthetic. No real PII appears in the fixtures.

**Names:** Generated to cover naming patterns across cultures. Any resemblance to
real people is coincidental.

**Addresses:** Mix of famous landmark addresses (1600 Pennsylvania Avenue, 10
Downing Street, 1 Infinite Loop), fictional addresses (742 Evergreen Terrace),
and synthetic addresses in realistic formats.

## Offset validation

`scripts/validate-fixture-offsets.py` verifies that every entity's declared
`start`/`end` offsets match the `text` field in the fixture. Run it to catch
offset drift:

```bash
python3 scripts/validate-fixture-offsets.py
```

The script runs during `cargo dfv check` and fails loudly on any mismatch.

## Running evaluation

```bash
# Rule-based eval (PERSON and STREET_ADDRESS have no rule detectors, so 0 TP)
cargo dfv eval

# GLiNER2 eval (requires provisioned model)
cargo dfv eval --model gliner2
```

## Baseline status

- **Rules:** The rule engine has no PERSON or STREET_ADDRESS detectors, so these
  entity types have 0 TP, 0 FP, 0 FN in rule eval — they're not in the scored
  types and don't appear in the baseline.

- **GLiNER2:** The GLiNER2 baseline (`baselines/eval-models-gliner2.json`) now
  scores `PERSON` and `STREET_ADDRESS`. The baseline is provisional until someone
  provisions the model and runs `cargo dfv eval --model gliner2 --update-baseline`.

## Extending the set

Add new fixtures to `fixtures/d4-names-addresses.jsonl`, one JSON object per
line:

```json
{"id":"d4-person-NNNN","text":"...","entities":[{"label":"PERSON","text":"Name Here","start":0,"end":9}],"category":"single_positive"}
```

Run the offset validator after adding fixtures. Categories: `single_positive`,
`multi_positive`, `hard_negative`.

# D4 Test Set

D4 is a labelled evaluation fixture set for measuring name and address detection against the ADR 004 bar.

## Entity types

| Type | Description | Source |
|------|-------------|--------|
| PERSON | Full person names | GLiNER2 model |
| STREET_ADDRESS | Street line only (number, street name, apt/suite/unit) | GLiNER2 model |
| ZIP_CODE | US postal codes (5-digit or ZIP+4) | Rule detector |

## Labelling convention

### STREET_ADDRESS

**STREET_ADDRESS covers the street line only.** For example:

- `123 Main Street` — yes
- `Apt 4B, 456 Oak Lane` — yes (apartment stays with street)
- `Suite 3400, 350 Fifth Avenue` — yes
- `123 Main Street, Anytown, CA 90210` — no (includes city/state/zip)

**Rationale:** Each address component gets its own marker so city and postal code don't reach the LLM in plain text next to a street marker. City + zip together can narrow identity significantly, so they are detected as separate findings rather than grouped with the street.

### ZIP_CODE

ZIP_CODE uses the existing rule detector and covers:
- 5-digit codes: `90210`
- ZIP+4 codes: `95014-2084`

### State/region

State/region is currently **unlabelled**. Whether to add a STATE entity type is an open question—it's unclear if state abbreviations alone carry enough PII risk to warrant detection.

### City

City names are currently **unlabelled**. There is no CITY entity type in datafog-core. Adding city detection is a product decision for future work.

## ADR 004 thresholds

- **PERSON recall:** ≥90% required (ADR 004)
- **STREET_ADDRESS recall:** No acceptance threshold defined yet — this is an open question for the maintainer.

## Fixture categories

- `single_positive`: One entity of the target type
- `multi_positive`: Multiple entities (same or different types)
- `hard_negative`: Similar text that should NOT match (organization names, street directions, code snippets)

## Entity counts

- 42 PERSON entities
- 38 STREET_ADDRESS entities
- 26 ZIP_CODE entities
- 22 hard negatives (empty entity lists)

## Known misses

### PERSON (1 exact miss, 0 lenient miss)

- `d4-person-0006`: "Müller, Hans" — model predicts "Müller" and "Hans" separately instead of the inverted format

### STREET_ADDRESS (1 genuine miss)

- `d4-addr-0014`: "1234 Martin Luther King Jr. Blvd, Suite 100" — model produces no prediction (long street name with "Jr.")

## Precision notes

When scoring GLiNER2 across all fixtures (D4 + development + final = 1185 fixtures), the model predictions on non-D4 fixtures count as false positives because those fixtures have EMAIL/PHONE/SSN labels but no PERSON/STREET_ADDRESS gold labels.

D4-only precision is higher than the combined precision:
- Combined: ~18% PERSON precision (177 FPs on non-D4 fixtures)
- D4-only: ~64% PERSON precision

The combined precision reflects real over-prediction behavior: the model sometimes predicts email addresses as person names.

## Usage

The D4 fixtures are loaded by `cargo dfv eval` alongside development.jsonl and final.jsonl.

To evaluate GLiNER2 against D4:

```bash
cargo dfv eval --model gliner2
```

The eval reports both exact and lenient (overlap) matching:
- **Exact match:** prediction boundaries must match gold span exactly
- **Lenient match:** prediction must overlap gold span (same label)

Both use one-to-one assignment: each prediction matches at most one gold span.

## Label prompt sensitivity

GLiNER2 results vary with the label prompt used:

| Label | PERSON Exact Recall |
|-------|---------------------|
| `person` | 97.6% (41/42) |
| `name` | ~88% |

The Rust core uses `"person"` as the label prompt. Using `"name"` gives lower recall.

This explains the difference between this PR's 97.6% recall and PR #116's 88.1% recall—they use different label prompts.

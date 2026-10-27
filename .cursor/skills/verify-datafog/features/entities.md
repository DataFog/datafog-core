# Entities

DataFog Core detects PII and credentials using rule-based detectors and optional model inference. Detectors produce findings with entity type, matched text, byte/codepoint/UTF-16 ranges, optional confidence, and detector name/version.

## Sub-features

### Default entities (always enabled)

| Entity | Detector | Notes |
|--------|----------|-------|
| EMAIL | Rule | Format-aware boundaries for `env` and `sql` contexts |
| PHONE | Rule | North American and international formats |
| SSN | Rule | US Social Security Number patterns |
| CREDIT_CARD | Rule | Luhn-validated card numbers |
| DATE | Rule | Common date formats |
| ZIP_CODE | Rule | US postal codes |
| IP_ADDRESS | Rule | IPv4 and IPv6 addresses |

### Credentials (always enabled)

| Entity | Detector | Notes |
|--------|----------|-------|
| API_KEY | Rule | Common API key patterns |
| BEARER_TOKEN | Rule | Bearer token patterns |
| JWT | Rule | JSON Web Token structure |
| PRIVATE_KEY | Rule | PEM-encoded private keys |
| CREDENTIAL_URI | Rule | URIs with embedded credentials |

### Labeled US identifiers (always enabled, require text label)

| Entity | Detector | Notes |
|--------|----------|-------|
| US_ROUTING_NUMBER | Rule | ABA routing numbers with preceding label |
| NPI | Rule | National Provider Identifier with preceding label |

### German locale entities (require `locale: "de"`)

| Entity | Detector | Notes |
|--------|----------|-------|
| GERMAN_IBAN | Rule | German IBAN format |
| GERMAN_VAT_ID | Rule | German VAT ID |
| GERMAN_TAX_ID | Rule | German tax ID |
| GERMAN_SOCIAL_SECURITY | Rule | German SSN |
| GERMAN_POSTAL_CODE | Rule | German postal codes |
| GERMAN_PASSPORT | Rule | German passport numbers |
| GERMAN_RESIDENCE_PERMIT | Rule | German residence permit numbers |

### Optional entities (require explicit config)

| Entity | Activation | Notes |
|--------|------------|-------|
| UUID | `detect_uuid: true` | UUID v1-v5 patterns |
| PERSON | Structured scan only | Name-field discovery in JSON |

### Model-only entities (require ModelSession)

| Entity | Backend | Notes |
|--------|---------|-------|
| FIRST_NAME | DataFog | Model inference |
| LAST_NAME | DataFog | Model inference |
| STREET_ADDRESS | DataFog/GLiNER2 | Model inference |
| CUSTOMER_ID | DataFog | Model inference |
| ACCOUNT_NUMBER | DataFog | Model inference |

## How to get to it (user POV)

### Python

```python
from datafog_core import scan, capabilities

# Default entities
findings = scan("Email jane@example.com")

# Select specific entities
findings = scan("Email jane@example.com", {"entities": ["EMAIL"]})

# German locale
findings = scan("IBAN DE89370400440532013000", {"locale": "de"})

# UUID detection
findings = scan("ID 550e8400-e29b-41d4-a716-446655440000", {"detect_uuid": True})

# Query supported entities
caps = capabilities()
print(caps["supported_entities"])
print(caps["default_entities"])
```

### Node.js

```javascript
import { scan } from "@datafog/node";

const findings = scan("Email jane@example.com");
const german = scan("IBAN DE89370400440532013000", { locale: "de" });
```

### Rust

```rust
use datafog_core::{scan, scan_with_config, ScanConfig, capabilities};

let findings = scan("Email jane@example.com");

let config = ScanConfig::new()
    .with_entities(vec!["EMAIL".into()])
    .unwrap();
let selected = scan_with_config("Email jane@example.com", &config);

let caps = capabilities();
```

### Browser/WASM

```javascript
import { init, scan } from "@datafog/wasm";

await init();
const findings = scan("Email jane@example.com");
```

## Driving it with cargo dfv

### Rule-based detection

```bash
cargo dfv check              # Runs all rule-based tests
cargo dfv check --json       # Machine-readable output
cargo dfv parity             # Cross-SDK offset verification
```

### Model-based detection

```bash
cargo dfv check --models     # Include model detectors
cargo dfv eval --models      # Per-entity metrics with model detectors
```

### Specific test files

Tests live in:
- `crates/core/tests/`: Rust unit tests per detector
- `bindings/python/tests/`: Python conformance tests
- `bindings/node/scripts/`: Node.js package tests
- `bindings/wasm/scripts/`: WASM package tests

## Gotchas

### Email boundary handling

The `format` option affects email detection boundaries:
- Default: standard email extraction
- `format: "env"`: handles `EMAIL=jane@example.com` assignments
- `format: "sql"`: handles quoted strings in SQL

### Phone number validation

- Numbers without `+` use North American rules (10 digits, area/exchange 2-9)
- Numbers with `+` use permissive 7-15 digit check
- Country codes and assigned numbers are not validated

### Labeled identifiers

US_ROUTING_NUMBER and NPI require a text label before the number. The label provides context that the number is an identifier rather than random digits.

### German locale aliases

All of these enable German detection: `de`, `de-DE`, `de_DE`. The locale does not validate account existence or perform IBAN checksums.

### UUID activation

Setting `detect_uuid: true` enables UUID detection. Selecting UUID in entities without this setting does not activate it.

### Decimal and CSV handling

Contiguous card digits after digit+comma or digit+period are suppressed to avoid false positives in decimal numbers. Parse CSV/TSV into cells before scanning.

### Structured PERSON

PERSON detection only works in structured scans via `scan_structured()` or `discover_fields()`. It uses name-field aliases or explicit JSON Pointer mappings.

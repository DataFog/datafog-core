# Workspace

DataFog Core provides a Workspace API for privacy-preserving session management. Workspaces track PII transformations, issue marker codes for restored values, and maintain operation history. The forget API allows retiring committed marker codes after host-selected expiry.

## Sub-features

### Core operations

| Function | Purpose | All SDKs |
|----------|---------|----------|
| `Workspace.create` | Create new workspace | Yes |
| `Workspace.openBundle` | Open from serialized bundle | Yes |
| `prepare` / `prepareBatch` | Stage text transformations | Yes |
| `restore` / `restoreReport` | Restore marker codes to values | Yes |
| `activateSaved` | Commit prepared changes | Yes |
| `exportBundle` | Serialize workspace state | Yes |

### Forget / mapping expiry (Node and WASM only)

| Function | Purpose | Python | Node | WASM |
|----------|---------|--------|------|------|
| `prepareForget` | Retire committed marker codes | No | Yes | Yes |

The forget API retires committed marker codes after host-selected expiry. Core keeps no clock—the host decides which codes to forget. Retired codes:
- Are never reissued
- Report as `expired` in restore operations
- Remain valid for conversation references
- A value seen again later gets a new marker

Saving a forget plan writes snapshot version 4. Core 1.0 cannot open v4 snapshots; earlier betas cannot open v3.

### Marker issue kinds

Restore operations can report marker issues:

| Kind | Meaning |
|------|---------|
| `unknown` | Well-formed but never committed here |
| `damaged` | Unparseable marker |
| `unbracketed` | Committed code without brackets |
| `not_issued_in_conversation` | Not issued in the scoped conversation |
| `expired` | A retired mapping (from `prepareForget`) |

### Operation history

| Kind | Meaning |
|------|---------|
| `create` | Workspace creation |
| `prepare` | Text transformation staged |
| `forget` | Marker codes retired |
| `recovery` | Recovery head staged |
| `policy_change` | Policy modification |
| `conversation_prepare` | Conversation send staged |
| `send_confirmed` | Send outcome recorded |
| `send_not_sent` | Non-send outcome recorded |

### Snapshot versions

| Version | Introduced | Notes |
|---------|------------|-------|
| 1 | beta.1 | Initial format |
| 2 | beta.1 | beta.1 cannot open |
| 3 | beta.2 | beta.1/beta.2 cannot open |
| 4 | 1.0 | Core 1.0 cannot open; supports retired mappings |

## How to get to it (user POV)

### Node.js

```javascript
import { Workspace, DataFogError } from "@datafog/node";

const id = (val) => new Uint8Array(16).fill(val);
const ws = Workspace.create(id(1), id(2));

// Prepare a value
const plan = ws.prepare("Jane Doe", [{
  entityType: "PERSON",
  matchedText: "Jane Doe",
  range: { unit: "utf8_bytes", start: 0, end: 8 }
}], id(3));

const marker = plan.preview().text;  // [[PERSON_ABC123]]

// Activate
const { expectedHead, candidateHead } = plan.heads();
const bundle = plan.exportCandidateBundle();
ws.activateSaved(plan, expectedHead, candidateHead);

// Restore
const restored = ws.restore(marker);  // "Jane Doe"

// Forget after expiry
const code = marker.slice(2, -2).split("_").at(-1);  // "ABC123"
const forgetPlan = ws.prepareForget([code], id(4));
console.log(forgetPlan.snapshotFormat());  // {currentVersion: 3, candidateVersion: 4}

const { expectedHead: fh, candidateHead: fc } = forgetPlan.heads();
ws.activateSaved(forgetPlan, fh, fc);

// Marker is now expired
const report = ws.restoreReport(marker);
console.log(report.issues[0].kind);  // "expired"

ws.close();
```

### Browser/WASM

```javascript
import { init, Workspace } from "@datafog/wasm";

await init();

const id = (val) => new Uint8Array(16).fill(val);
const ws = Workspace.create(id(1), id(2));

// Same API as Node.js
const plan = ws.prepare("Jane Doe", [{
  entityType: "PERSON",
  matchedText: "Jane Doe",
  range: { unit: "utf8_bytes", start: 0, end: 8 }
}], id(3));

// prepareForget available in WASM
const forgetPlan = ws.prepareForget([code], id(4));

ws.close();
```

### Python

```python
from datafog_core import Workspace

# Python has core workspace APIs but not prepareForget
ws = Workspace.create(namespace_id, operation_id)
plan = ws.prepare("Jane Doe", findings, operation_id)
# No ws.prepare_forget() in Python
ws.close()
```

## Driving it with cargo dfv

```bash
cargo dfv parity --workspace     # Workspace contract matrix across SDKs
cargo dfv check                  # Includes workspace conformance tests
```

The parity command with `--workspace` verifies workspace behavior across Rust, Python, Node, and WASM including the forget API where available.

## Gotchas

### Python API availability

The `prepareForget` API is not available in Python. Use Node.js or WASM for mapping expiry workflows.

### Snapshot version compatibility

Saving a forget plan upgrades the snapshot to version 4. Earlier versions of Core cannot open v4 snapshots:
- Core 1.0 cannot open v4
- beta.1/beta.2 cannot open v3
- beta.1 cannot open v2

### Marker code format

Marker codes follow the pattern `[[ENTITY_CODE]]` where CODE is a 6-character alphanumeric string. Extract the code for forget operations:

```javascript
const code = marker.slice(2, -2).split("_").at(-1);
```

### Concurrent operations

Workspace operations are not thread-safe. Concurrent prepare/activate calls may produce conflicts. The workspace reports conflicts through error `marker_issues` and `review_conflicts`.

### Bundle sensitivity

Exported bundles contain sensitive PII. Zero-fill bundle arrays after use and handle them with appropriate security controls.

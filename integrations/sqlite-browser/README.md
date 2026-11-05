# SQLite3 Multiple Ciphers browser Workspace owner

Optional encrypted browser persistence alongside FogVault. Core owns reversible mappings and review; the adapter saves the exact bundle before activation. This package uses SQLite3MC 2.6.1, authenticated ChaCha20-Poly1305, dedicated Worker, OPFS synchronous-access-handle pool, DELETE journal and FULL synchronization. It requires OPFS in a dedicated Worker; no memory or plaintext fallback is offered. Cross-origin isolation is not required by this profile.

```js
import {init, Workspace} from '@datafog/wasm';
import {SQLiteVault, SQLiteWorkspace} from '@datafog/core-sqlite-browser';
await init();
const vault = await SQLiteVault.create({id: crypto.randomUUID().replaceAll('-', ''), password});
const owner = await SQLiteWorkspace.create({Workspace, vault, recordId, namespace});
const plan = owner.prepareReview(inputs, operationId);
showLocalReview(plan.preview());
// Only after explicit local approval of this exact plan:
const saved = await owner.commit(plan, {approved: true});
await owner.lock();
```

Give the adapter exclusive ownership of the vault handle. Lock revokes Core immediately and terminates the database Worker. Reopen with `SQLiteVault.open`, then `SQLiteWorkspace.open` and trusted expected namespace/head. Passwords must be 12–1024 UTF-8 bytes; key wrapping uses PBKDF2-SHA256 (600,000 iterations) and AES-256-GCM. An optional independent random 32-byte `recoveryKey` can unwrap the database key on open. Store it separately. Browser IndexedDB stores authenticated wrapped keys; OPFS stores encrypted database pages. Neither store contains an unwrapped database key.

The host owns UI approval, credential entry, routing and actual transport. The adapter never sends a message. Conflicts and uncertain saves close the owner and authorize no handoff. Reopen durable state and reconcile the request ID; prepare and approve fresh work. Restoring a complete older valid browser profile is outside the unanchored offline rollback guarantee. Browser eviction and user deletion require separately retained backups.

Bundle `worker.mjs`, its imports and `vendor/sqlite3.mjs`/`.wasm` under the extension's own origin. Configure `workerUrl` if a bundler relocates assets. MV3 CSP needs `script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'`. Keep the owning page alive; service-worker memory is not persistence. The detector's Worker supplies findings and never owns this database.

Provision with `python3 scripts/provision-sqlite-browser.py`, build Core WASM, then run `node integrations/sqlite-browser/test/run-installed.mjs`. The check packs and installs candidates into a clean extension consumer; it does not provision assets. Keep package versions unchanged until release approval.

SQLite3MC upstream: https://github.com/utelle/SQLite3MultipleCiphers . Third-party notices must accompany shipped artifacts. Actual shipping-extension/device qualification remains distinct from synthetic Chromium tests.

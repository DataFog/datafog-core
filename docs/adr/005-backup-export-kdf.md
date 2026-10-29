# ADR 005: Stronger password wrapping for exported workspace backups

Status: accepted for the `0.5.0-beta.2` implementation; shipping-host encryption review remains separate.

Issue: [B3 / #142](https://github.com/DataFog/datafog-core/issues/142).

## Decision

New backup version 2 exports use password profile **2**: Argon2id v1.3,
**256 MiB (268435456 bytes), three passes, one lane**, fresh random 16-byte salt,
32-byte output. Profile identifiers permanently bind these exact values; tuning
requires a new identifier and format policy. The pinned libsodium implementation
uses one lane. Neither files nor callers can supply arbitrary memory, iteration,
algorithm or output-length parameters. Unknown profiles and fields are rejected
before derivation. Allocation/module failure is `kdf_unavailable`; there is no
cheaper retry. Each KDF operation uses one disposable worker, serially within the
broker. Cancellation terminates the worker.

This matches libsodium's current moderate memory/operation cost. It is not an exact
RFC 9106 preset: the RFC's 64 MiB/three-pass and 2 GiB/one-pass recommendations use
four lanes. We choose the existing library's single-lane interface rather
than implementing a second Argon2 library or attempting a 2 GiB browser allocation.

## Why replacing the wrapper matters

A version-1 export copied the live vault's 64 MiB password-wrapped root key. An
attacker with that file can guess passwords offline and unwrap the workspace key.
Adding expensive outer authentication without replacing that readable password
slot would preserve this attack path. New exports replace the password slot entirely: fresh salt,
nonce and 256 MiB wrapping key, retaining only that wrapper in the standalone file.
The original profile-1 salt/nonce/wrapped-root tuple is absent.

The user reenters the current vault password at export. Reauthentication opens the
existing root transiently; the root is rewrapped and zeroed in a finally block.
Runtime CryptoKeys stay non-extractable. Credentials are cleared from UI controls,
not stored in the session or persisted. JavaScript strings cannot be reliably
zeroed; byte buffers under our control are cleared and the KDF worker is terminated.
The immutable password string exists only for the operation's lifetime.

The existing high-entropy recovery slot stays intact: a uniformly random 32-byte
recovery secret uses HKDF-SHA-256 and AES-GCM, independently of the password KDF.
A separate AES-GCM authentication tag binds the complete canonical payload,
including the unused credential slot, format/profile, identifiers and saved head.
The v2 context is `DFGB2:AUTH:`; v1 remains `DFGB1:AUTH:`.

## Compatibility and bounded limitations

- New live vault creation stays profile 1 (64 MiB/three passes). Existing vaults
  are not rewritten on export. Vault version 1 remains profile 1 only.
- Legacy backup v1 imports accept profile 1 only; new backup v2 imports accept
  profile 2 only. New exports always use v2. Versions cannot downgrade one another.
- Restored v2 vaults retain profile 2 for password unlock and subsequent exports.
  Recovery-code imports need no password and preserve that stronger wrapper.
  Destination workspace data keys, nonces and commit heads are freshly generated.
- Export requires the vault password even after recovery-code unlock. A user who
  knows only the recovery code can import existing backups but cannot create a
  new export here. This slice does not implement password reset or credential
  rotation. A recovery-code/new-backup-password flow needs separate design.
- Existing backup copies and profile-1 live vault copies are not retroactively
  strengthened. Reusing a password across such copies still exposes its weaker
  offline guessing path. The new profile increases cost; it cannot make a weak
  password unguessable or revoke old exports.
- This decision does not complete G4: the independent review of the actual
  shipping host, secret lifetime, storage and end-to-end backup behavior remains
  a release gate. No production extension or published beta is changed by this PR.

## Chrome measurements

On 2026-10-28, installed **Chrome 156.0.8078.49**, macOS arm64, Apple M5 Pro,
48 GiB RAM; five fresh dedicated workers per profile, libsodium-wrappers-sumo
0.8.4 / libsodium 1.0.22. All inputs were synthetic. Headless Chrome used an
isolated profile and security defaults; a localhost page enforced a self-only
CSP with `wasm-unsafe-eval`. No model was loaded and no 2 GiB allocation was tried.

| Profile | Median derivation | Median whole request | Observed WASM linear heap |
| --- | ---: | ---: | ---: |
| 64 MiB / t3 / p1 | 96.8 ms | 109.8 ms | 67,371,008 bytes |
| 256 MiB / t3 / p1 | 390.1 ms | 399.6 ms | 269,090,816 bytes |

The page's eight-millisecond heartbeat had a maximum observed gap of 9.4 ms during
the 256 MiB runs. The heap is linear WASM allocation, not whole-browser RSS or a
peak-memory proof. These observations establish feasibility on this workstation;
they do not qualify low-memory devices, model/KDF simultaneous use or a latency
SLA. Existing-vault export additionally pays the old profile's reauthentication
cost; exporting a restored profile-2 vault pays two profile-2 derivations serially.

Raw per-run timings, synthetic key digests and environment metadata are in
[`kdf-chrome-benchmark.json`](../../fixtures/workspace/kdf-chrome-benchmark.json).
The measured worker called `crypto_pwhash(32, passwordBytes, salt16, 3,
profileMiB * 1024 * 1024, crypto_pwhash_ALG_ARGON2ID13)`, measured the synchronous
call with `performance.now()`, then reported `sodium.libsodium.HEAPU8.buffer.byteLength`.
Each request awaited `sodium.ready`, used password `synthetic-kdf-benchmark-only`
and salt sixteen bytes of `0x42`, and terminated its fresh worker afterward.
The native reference fixture independently covers the exact persisted profile;
Python/native libsodium is not a runtime dependency of browser tests.

## Verification

The existing actual-extension regression suite exercises password and recovery
imports, browser restart, wrapper absence, wrong export credentials, profile/field
rejection, metadata/unused-slot tampering, cancellation, fresh destination keys and
legacy imports. Both profiles must match independent native libsodium known-answer
fixtures. The UI/broker do not retain credentials between operations.

## Primary guidance

- [libsodium password hashing/key derivation](https://doc.libsodium.org/password_hashing/default_phf):
  moderate currently uses 256 MiB and three operations; choose affordable memory
  and benchmark the intended device. Persist exact algorithm and cost values.
- [RFC 9106, section 4](https://www.rfc-editor.org/rfc/rfc9106.html#section-4):
  Argon2id's recommended presets and hardware/time/memory selection procedure.
- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html):
  Argon2id minimums are a floor; benchmark work factors for deployment constraints.
- [libsodium Argon2id implementation](https://github.com/jedisct1/libsodium/blob/1.0.22-RELEASE/src/libsodium/crypto_pwhash/argon2/pwhash_argon2id.c):
  `crypto_pwhash` passes one lane to Argon2id. Persistent profiles use explicit
  integers rather than mutable convenience constants.

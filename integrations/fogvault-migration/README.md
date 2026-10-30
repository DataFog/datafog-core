# Core bundle recovery staging

This optional, unpublished Rust 1.88 crate validates an **already authenticated
and decrypted** Core bundle and prepares its existing Core recovery operation.
It does not decrypt legacy backups or publish records into FogVault.

```rust
use datafog_core::workspace::{Head, PublicId};
use datafog_fogvault_migration::stage_authenticated_bundle;

fn stage(bytes: &[u8], namespace: PublicId, head: Head, random_operation: PublicId)
    -> Result<(), Box<dyn std::error::Error>>
{
    let candidate = stage_authenticated_bundle(bytes, namespace, head, random_operation)?;
    // Give candidate.bundle() only to the trusted destination encrypting adapter.
    // It must reject an occupied destination atomically and confirm persistence.
    assert_eq!(candidate.namespace(), namespace);
    Ok(())
}
```

The namespace and complete head must come from the authenticated source envelope.
Passing values decoded from an unauthenticated source does not authenticate it.
The host must supply a fresh CSPRNG operation ID, explicit recovery approval,
fresh destination vault identity/keys and an empty destination. Source workspace
namespace and marker identities remain stable; destination vault identity does
not. Core recovery creates a new commit head and recovery operation while
retaining aliases, policy, history and pending conversation authority.

The helper borrows source bytes without changing them. Candidate bytes are
plaintext, borrowed through `bundle()`, and held in an owned zeroizing buffer.
Foreign copies remain caller-owned. Neither staging nor exported bytes grant
handoff permission or prove destination persistence. The maintained destination
adapter owns the atomic empty-destination check, encryption, exact durable save,
record binding, cancellation, unknown-outcome reconciliation and reopening.

The helper deliberately uses `Workspace::open_bundle` and `prepare_recovery`
instead of another CBOR parser/serializer. Current Core v1/v2 snapshots upgrade
to the current v3 snapshot only through that explicit recovery operation. This
does not define the future public Core 1.0.0 snapshot format. Reading or rejecting
a source leaves its original bytes intact.

Run the focused gates from the repository root:

```sh
cargo fmt --manifest-path integrations/fogvault-migration/Cargo.toml --check
cargo clippy --locked --manifest-path integrations/fogvault-migration/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path integrations/fogvault-migration/Cargo.toml
```

Tests use existing synthetic Core v1/v2/v3 fixtures, including high u64 revisions,
policy/aliases, recorded history and pending/resolved conversations. They also
reject wrong namespace/revision/commit IDs, malformed/future snapshots, exhausted
revision authority and reused recovery IDs. They do not establish legacy archive
decryption, empty-destination publication, process-crash durability, provider
security or readiness for a future frozen v1 schema.

See [compatibility and release boundaries](../../docs/integrations/fogvault-compatibility.md)
and the [shared adapter contract](../../docs/integrations/fogvault-contract.md).

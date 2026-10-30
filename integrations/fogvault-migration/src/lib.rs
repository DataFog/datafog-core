//! Stage an authenticated Core snapshot for recovery into a fresh FogVault destination.
//!
//! This module does not decrypt archives or authenticate their origin. The caller
//! first authenticates the source and supplies the namespace and complete head
//! from that authenticated envelope. Candidate bytes remain plaintext and must
//! only enter a trusted encrypting adapter. Staging grants no handoff permission.

use datafog_core::workspace::{Head, PublicId, SnapshotFormat, Workspace, WorkspaceError};
use zeroize::Zeroizing;

#[derive(Debug, PartialEq, Eq)]
pub enum MigrationError {
    SourceBindingMismatch,
    Core(WorkspaceError),
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceBindingMismatch => f.write_str("source_binding_mismatch"),
            Self::Core(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<WorkspaceError> for MigrationError {
    fn from(error: WorkspaceError) -> Self {
        Self::Core(error)
    }
}

/// An uncommitted recovery candidate. It owns no destination or publication authority.
///
/// The destination adapter must require an empty destination atomically, encrypt
/// under fresh destination keys, and reopen the exact durable candidate before
/// making it available. A lost acknowledgement requires reconciliation; these
/// bytes do not prove either persistence or rollback. Foreign byte copies remain
/// the caller's cleanup responsibility. The owned buffer is zeroized on drop.
pub struct RecoveryCandidate {
    namespace: PublicId,
    source_head: Head,
    candidate_head: Head,
    format: SnapshotFormat,
    bundle: Zeroizing<Vec<u8>>,
}

impl RecoveryCandidate {
    pub fn namespace(&self) -> PublicId {
        self.namespace
    }

    pub fn source_head(&self) -> Head {
        self.source_head
    }

    pub fn candidate_head(&self) -> Head {
        self.candidate_head
    }

    pub fn snapshot_format(&self) -> SnapshotFormat {
        self.format
    }

    /// Sensitive plaintext for an encrypting destination adapter, never a public export.
    pub fn bundle(&self) -> &[u8] {
        &self.bundle
    }
}

/// Validate the authenticated binding and let Core stage its supported recovery.
///
/// The input is borrowed and unchanged on success or error. Unsupported snapshot
/// versions, exhausted revision/history budgets and reused operation IDs reject
/// through Core. The operation ID must come from the host CSPRNG; shape checks
/// cannot establish randomness. This is the current Core decoder/recovery path,
/// not an implementation of an unspecified future v1 schema conversion.
pub fn stage_authenticated_bundle(
    bundle: &[u8],
    expected_namespace: PublicId,
    expected_head: Head,
    recovery_operation_id: PublicId,
) -> Result<RecoveryCandidate, MigrationError> {
    let source = Workspace::open_bundle(bundle)?;
    if source.namespace()? != expected_namespace || source.head()? != expected_head {
        return Err(MigrationError::SourceBindingMismatch);
    }
    let change = source.prepare_recovery(recovery_operation_id)?;
    let candidate = RecoveryCandidate {
        namespace: expected_namespace,
        source_head: change.expected_head()?,
        candidate_head: change.candidate_head()?,
        format: change.snapshot_format()?,
        bundle: Zeroizing::new(change.export_candidate_bundle()?.to_vec()),
    };
    // Reopen using the public decoder rather than inventing a parallel serializer.
    let reopened = Workspace::open_bundle(candidate.bundle())?;
    if reopened.namespace()? != candidate.namespace || reopened.head()? != candidate.candidate_head
    {
        return Err(MigrationError::SourceBindingMismatch);
    }
    Ok(candidate)
}

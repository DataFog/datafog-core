//! Optional Core-owned integration over the maintained registered FogVault API.
//!
//! The host establishes trusted control storage, authenticates the vault, owns
//! approval and network transport, and supplies fresh CSPRNG IDs. This owner
//! never sends text. Returned plaintext copies belong to the caller.

pub use datafog_core::workspace as core;
pub use fogvault_native as vault;

use core::{Head, PreparedChange, PublicId, Workspace, WorkspaceError};
use std::{fmt, sync::Arc};
use vault::{CommitReceipt, SessionToken, Vault};
use zeroize::Zeroizing;

pub const BINDING_BYTES: usize = 88;
const MAGIC: &[u8; 8] = b"DFVBRG01";

/// Authenticated record metadata; revisions here are Core revisions, not vault revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub namespace: PublicId,
    pub record_id: PublicId,
    pub parent: Head,
    pub saved: Head,
}
impl Binding {
    pub fn encode(self) -> Result<[u8; BINDING_BYTES]> {
        if self.namespace == [0; 16] || self.record_id == [0; 16] {
            return Err(Error::BindingMismatch);
        }
        let mut out = [0; BINDING_BYTES];
        out[..8].copy_from_slice(MAGIC);
        out[8..24].copy_from_slice(&self.namespace);
        out[24..40].copy_from_slice(&self.record_id);
        out[40..48].copy_from_slice(&self.parent.revision.to_le_bytes());
        out[48..64].copy_from_slice(&self.parent.commit_id);
        out[64..72].copy_from_slice(&self.saved.revision.to_le_bytes());
        out[72..88].copy_from_slice(&self.saved.commit_id);
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != BINDING_BYTES || &bytes[..8] != MAGIC {
            return Err(Error::BindingMismatch);
        }
        // Length is checked above; conversion failures remain controlled errors.
        let invalid = |_| Error::BindingMismatch;
        let binding = Self {
            namespace: bytes[8..24].try_into().map_err(invalid)?,
            record_id: bytes[24..40].try_into().map_err(invalid)?,
            parent: Head {
                revision: u64::from_le_bytes(bytes[40..48].try_into().map_err(invalid)?),
                commit_id: bytes[48..64].try_into().map_err(invalid)?,
            },
            saved: Head {
                revision: u64::from_le_bytes(bytes[64..72].try_into().map_err(invalid)?),
                commit_id: bytes[72..88].try_into().map_err(invalid)?,
            },
        };
        if binding.namespace == [0; 16] || binding.record_id == [0; 16] {
            return Err(Error::BindingMismatch);
        }
        Ok(binding)
    }
}

#[derive(Debug)]
pub enum Error {
    Core(WorkspaceError),
    Vault(vault::Error),
    BindingMismatch,
    ApprovalRequired,
    DestinationOccupied,
    ForeignChange,
    ReopenRequired,
    RevisionExhausted,
    SavedButNotActivated { request_id: PublicId },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(_) => f.write_str("Core rejected the workspace operation"),
            Self::Vault(_) => f.write_str("vault operation failed; inspect the typed cause"),
            Self::BindingMismatch => f.write_str("workspace record binding does not match"),
            Self::ApprovalRequired => f.write_str("explicit approval is required"),
            Self::DestinationOccupied => f.write_str("destination is occupied"),
            Self::ForeignChange => f.write_str("prepared change belongs to another owner"),
            Self::ReopenRequired => {
                f.write_str("workspace owner is closed; authenticated reopen required")
            }
            Self::RevisionExhausted => f.write_str("vault record revision is exhausted"),
            Self::SavedButNotActivated { .. } => {
                f.write_str("saved candidate was not activated; reopen required")
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<WorkspaceError> for Error {
    fn from(error: WorkspaceError) -> Self {
        Self::Core(error)
    }
}
impl From<vault::Error> for Error {
    fn from(error: vault::Error) -> Self {
        Self::Vault(error)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

/// A local preview owned by exactly one adapter runtime. No raw plan import exists.
pub struct Change {
    owner: Arc<()>,
    plan: PreparedChange,
}
impl Change {
    pub fn previews(&self) -> Result<&[core::Preview]> {
        Ok(self.plan.previews()?)
    }
    pub fn heads(&self) -> Result<(Head, Head)> {
        Ok((self.plan.expected_head()?, self.plan.candidate_head()?))
    }
    /// Logical candidate payload length, excluding allocator/foreign copies.
    pub fn candidate_bytes(&self) -> Result<usize> {
        Ok(self.plan.export_candidate_bundle()?.len())
    }
    pub fn snapshot_format(&self) -> Result<core::SnapshotFormat> {
        Ok(self.plan.snapshot_format()?)
    }
    pub fn policy_preview(&self) -> Result<Option<&core::PolicyPreview>> {
        Ok(self.plan.policy_preview()?)
    }
    pub fn send_intents(&self) -> Result<Vec<core::SendIntentId>> {
        Ok(self.plan.send_intents()?)
    }
    pub fn discard(&mut self) {
        self.plan.discard();
    }
}

/// Proof of local persistence and activation, not proof of network delivery.
pub struct Committed {
    pub head: Head,
    pub receipt: CommitReceipt,
    /// Caller-owned plaintext; erase or release copies when the UI no longer needs them.
    pub previews: Vec<String>,
}

/// One serial native owner. It does not expose its raw mutable handles.
pub struct Owner {
    workspace: Workspace,
    vault: Vault,
    token: SessionToken,
    owner: Arc<()>,
    record_id: PublicId,
    namespace: PublicId,
    record_revision: u64,
    receipt: CommitReceipt,
    active: bool,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.lock();
    }
}
impl Owner {
    /// Persist an initial empty workspace into an unoccupied record of an authenticated vault.
    pub fn create(
        vault: Vault,
        token: SessionToken,
        record_id: PublicId,
        namespace: PublicId,
        operation_id: PublicId,
        request_id: PublicId,
    ) -> Result<Self> {
        Self::create_workspace(
            vault,
            token,
            record_id,
            Workspace::create(namespace, operation_id)?,
            request_id,
        )
    }
    pub fn create_with_operation(
        vault: Vault,
        token: SessionToken,
        record_id: PublicId,
        namespace: PublicId,
        operation: core::OperationContext,
        request_id: PublicId,
    ) -> Result<Self> {
        Self::create_workspace(
            vault,
            token,
            record_id,
            Workspace::create_with_operation(namespace, operation)?,
            request_id,
        )
    }
    fn create_workspace(
        mut vault: Vault,
        token: SessionToken,
        record_id: PublicId,
        workspace: Workspace,
        request_id: PublicId,
    ) -> Result<Self> {
        let namespace = workspace.namespace()?;
        let head = workspace.head()?;
        let bytes = Zeroizing::new(workspace.export_bundle()?);
        let binding = Binding {
            namespace,
            record_id,
            parent: head,
            saved: head,
        };
        let receipt = vault.current_receipt(&token)?;
        match vault.read(&token, record_id) {
            Ok(_) => {
                vault.lock();
                return Err(Error::DestinationOccupied);
            }
            Err(vault::Error::NotFound) => {}
            Err(error) => {
                vault.lock();
                return Err(Error::Vault(error));
            }
        }
        let saved = vault.save_at(
            &token,
            receipt.head_digest,
            record_id,
            None,
            &binding.encode()?,
            &bytes,
            request_id,
        );
        match saved {
            Ok(receipt) => Ok(Self {
                workspace,
                vault,
                token,
                owner: Arc::new(()),
                record_id,
                namespace,
                record_revision: 1,
                receipt,
                active: true,
            }),
            Err(error) => {
                vault.lock();
                Err(Error::Vault(error))
            }
        }
    }
    /// Open authenticated bytes; the host supplies the expected namespace and optionally a head.
    pub fn open(
        mut vault: Vault,
        token: SessionToken,
        record_id: PublicId,
        namespace: PublicId,
        expected_head: Option<Head>,
    ) -> Result<Self> {
        let result = (|| {
            let before = vault.current_receipt(&token)?;
            let record = vault.read(&token, record_id)?;
            let binding = Binding::decode(&record.metadata)?;
            if binding.record_id != record_id || binding.namespace != namespace {
                return Err(Error::BindingMismatch);
            }
            let workspace = Workspace::open_bundle(&record.payload)?;
            let head = workspace.head()?;
            if workspace.namespace()? != namespace
                || head != binding.saved
                || expected_head.is_some_and(|expected| expected != head)
            {
                return Err(Error::BindingMismatch);
            }
            let after = vault.current_receipt(&token)?;
            if before != after {
                return Err(Error::Vault(vault::Error::Conflict));
            }
            Ok((workspace, record.revision, after))
        })();
        match result {
            Ok((workspace, record_revision, receipt)) => Ok(Self {
                workspace,
                vault,
                token,
                owner: Arc::new(()),
                record_id,
                namespace,
                record_revision,
                receipt,
                active: true,
            }),
            Err(error) => {
                vault.lock();
                Err(error)
            }
        }
    }
    /// Publish an explicitly approved, independently authenticated recovery candidate.
    /// The migration layer validates and owns exact candidate bytes. The destination
    /// record must be unoccupied. Source keys/credentials are never transplanted.
    pub fn import_recovered(
        mut vault: Vault,
        token: SessionToken,
        record_id: PublicId,
        candidate: &datafog_fogvault_migration::RecoveryCandidate,
        approved: bool,
        request_id: PublicId,
    ) -> Result<Self> {
        if !approved {
            vault.lock();
            return Err(Error::ApprovalRequired);
        }
        let workspace = Workspace::open_bundle(candidate.bundle())?;
        let namespace = candidate.namespace();
        let head = workspace.head()?;
        if workspace.namespace()? != namespace || head != candidate.candidate_head() {
            vault.lock();
            return Err(Error::BindingMismatch);
        }
        let binding = Binding {
            namespace,
            record_id,
            parent: candidate.source_head(),
            saved: head,
        };
        let receipt = vault.current_receipt(&token)?;
        if vault.audit(&token)?.records_checked != 0 {
            vault.lock();
            return Err(Error::DestinationOccupied);
        }
        if vault.current_receipt(&token)? != receipt {
            vault.lock();
            return Err(Error::Vault(vault::Error::Conflict));
        }
        match vault.save_at(
            &token,
            receipt.head_digest,
            record_id,
            None,
            &binding.encode()?,
            candidate.bundle(),
            request_id,
        ) {
            Ok(receipt) => Ok(Self {
                workspace,
                vault,
                token,
                owner: Arc::new(()),
                record_id,
                namespace,
                record_revision: 1,
                receipt,
                active: true,
            }),
            Err(error) => {
                vault.lock();
                Err(Error::Vault(error))
            }
        }
    }
    fn ready(&self) -> Result<()> {
        if self.active && self.vault.is_unlocked() {
            Ok(())
        } else {
            Err(Error::ReopenRequired)
        }
    }
    fn wrap(&self, plan: PreparedChange) -> Change {
        Change {
            owner: Arc::clone(&self.owner),
            plan,
        }
    }
    pub fn head(&self) -> Result<Head> {
        self.ready()?;
        Ok(self.workspace.head()?)
    }
    pub fn namespace(&self) -> Result<PublicId> {
        self.ready()?;
        Ok(self.namespace)
    }
    pub fn record_revision(&self) -> Result<u64> {
        self.ready()?;
        Ok(self.record_revision)
    }
    pub fn prepare(
        &self,
        text: &str,
        findings: &[core::WorkspaceFinding],
        id: PublicId,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(self.workspace.prepare(text, findings, id)?))
    }
    pub fn prepare_review(&self, inputs: &[core::ReviewInput<'_>], id: PublicId) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(self.workspace.prepare_review(inputs, id)?))
    }
    pub fn prepare_batch(
        &self,
        inputs: &[core::WorkspaceInput<'_>],
        id: PublicId,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(self.workspace.prepare_batch(inputs, id)?))
    }
    pub fn prepare_with_operation(
        &self,
        text: &str,
        findings: &[core::WorkspaceFinding],
        operation: core::OperationContext,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(
            self.workspace
                .prepare_with_operation(text, findings, operation)?,
        ))
    }
    pub fn prepare_batch_with_operation(
        &self,
        inputs: &[core::WorkspaceInput<'_>],
        operation: core::OperationContext,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(
            self.workspace
                .prepare_batch_with_operation(inputs, operation)?,
        ))
    }
    pub fn prepare_recovery_with_operation(
        &self,
        operation: core::OperationContext,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(self.workspace.prepare_recovery_with_operation(operation)?))
    }
    pub fn prepare_outcome(
        &self,
        outcome: core::OperationOutcome,
        operation: core::OperationContext,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(self.workspace.prepare_outcome(outcome, operation)?))
    }
    pub fn prepare_conversation_send(
        &self,
        inputs: &[core::ConversationInput<'_>],
        operation: core::OperationContext,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(
            self.workspace
                .prepare_conversation_send(inputs, operation)?,
        ))
    }
    pub fn prepare_observed_send(
        &self,
        intent: core::SendIntentId,
        logical_id: PublicId,
        surface: &str,
        site_id: &str,
        operation: core::OperationContext,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(
            self.workspace
                .prepare_observed_send(intent, logical_id, surface, site_id, operation)?,
        ))
    }
    pub fn prepare_definitely_not_sent(
        &self,
        intent: core::SendIntentId,
        logical_id: PublicId,
        operation: core::OperationContext,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(
            self.workspace
                .prepare_definitely_not_sent(intent, logical_id, operation)?,
        ))
    }
    pub fn prepare_policy_changes(
        &self,
        edits: &core::PolicyEdits,
        id: PublicId,
    ) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(self.workspace.prepare_policy_changes(edits, id)?))
    }
    pub fn prepare_recovery(&self, id: PublicId) -> Result<Change> {
        self.ready()?;
        Ok(self.wrap(self.workspace.prepare_recovery(id)?))
    }
    /// Approval is for this exact immutable preview; false does not consume the change.
    /// Every storage failure closes both owners conservatively. Never retry an old approval.
    pub fn commit(
        &mut self,
        change: &mut Change,
        approved: bool,
        request_id: PublicId,
    ) -> Result<Committed> {
        self.ready()?;
        if !approved {
            return Err(Error::ApprovalRequired);
        }
        if !Arc::ptr_eq(&self.owner, &change.owner) {
            return Err(Error::ForeignChange);
        }
        let (parent, saved) = change.heads()?;
        if self.workspace.head()? != parent {
            self.lock();
            return Err(Error::Core(WorkspaceError {
                code: core::ErrorCode::WorkspaceConflict,
                finding_index: None,
                input_index: None,
                marker_issues: vec![],
                review_conflicts: vec![],
            }));
        }
        let next_revision = self
            .record_revision
            .checked_add(1)
            .ok_or(Error::RevisionExhausted)?;
        let mut previews = Zeroizing::new(
            change
                .plan
                .previews()?
                .iter()
                .map(|p| p.text.clone())
                .collect::<Vec<_>>(),
        );
        let binding = Binding {
            namespace: self.namespace,
            record_id: self.record_id,
            parent,
            saved,
        };
        let result = self.vault.save_at(
            &self.token,
            self.receipt.head_digest,
            self.record_id,
            Some(self.record_revision),
            &binding.encode()?,
            change.plan.export_candidate_bundle()?,
            request_id,
        );
        let receipt = self.accepted_receipt(result)?;
        if self
            .workspace
            .activate_saved(&mut change.plan, parent, saved)
            .is_err()
        {
            self.lock();
            return Err(Error::SavedButNotActivated { request_id });
        }
        self.receipt = receipt.clone();
        self.record_revision = next_revision;
        Ok(Committed {
            head: saved,
            receipt,
            previews: std::mem::take(&mut *previews),
        })
    }
    fn accepted_receipt(&mut self, result: vault::Result<CommitReceipt>) -> Result<CommitReceipt> {
        match result {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                self.lock();
                Err(Error::Vault(error))
            }
        }
    }
    /// Reconcile only the exact retained request. This never reactivates Core or
    /// grants handoff; authenticate and construct a fresh owner afterward.
    pub fn reconcile(&mut self, request_id: PublicId) -> Result<CommitReceipt> {
        self.lock();
        Ok(self.vault.reconcile(request_id)?)
    }
    pub fn restore(&self, text: &str) -> Result<String> {
        self.ready()?;
        Ok(self.workspace.restore(text)?)
    }
    pub fn restore_in_conversation(&self, text: &str, id: PublicId) -> Result<String> {
        self.ready()?;
        Ok(self.workspace.restore_in_conversation(text, id)?)
    }
    pub fn restore_report(&self, text: &str) -> Result<core::RestoreReport> {
        self.ready()?;
        Ok(self.workspace.restore_report(text)?)
    }
    pub fn restore_report_in_conversation(
        &self,
        text: &str,
        id: PublicId,
    ) -> Result<core::RestoreReport> {
        self.ready()?;
        Ok(self.workspace.restore_report_in_conversation(text, id)?)
    }
    pub fn counts(&self) -> Result<core::OperationCounts> {
        self.ready()?;
        Ok(self.workspace.counts()?)
    }
    pub fn history_coverage(&self) -> Result<core::OperationHistoryCoverage> {
        self.ready()?;
        Ok(self.workspace.history_coverage()?)
    }
    pub fn operations(&self) -> Result<&[core::OperationRecord]> {
        self.ready()?;
        Ok(self.workspace.operations()?)
    }
    pub fn progressive_restore(
        &self,
        encoding: core::RestoreEncoding,
    ) -> Result<core::ProgressiveRestore> {
        self.ready()?;
        Ok(self.workspace.progressive_restore(encoding)?)
    }
    pub fn review(&self, input: core::ReviewInput<'_>) -> Result<core::ReviewResult> {
        self.ready()?;
        Ok(self.workspace.review(input)?)
    }
    pub fn recall(&self, text: &str) -> Result<core::RecallResult> {
        self.ready()?;
        Ok(self.workspace.recall(text)?)
    }
    pub fn policy(&self) -> Result<core::PolicyView> {
        self.ready()?;
        Ok(self.workspace.policy()?)
    }
    pub fn operations_page(&self, offset: usize, limit: usize) -> Result<core::OperationPage<'_>> {
        self.ready()?;
        Ok(self.workspace.operations_page(offset, limit)?)
    }
    pub fn conversation(&self, id: PublicId) -> Result<core::ConversationState> {
        self.ready()?;
        Ok(self.workspace.conversation(id)?)
    }
    pub fn conversation_for_site(&self, surface: &str, site_id: &str) -> Result<PublicId> {
        self.ready()?;
        Ok(self.workspace.conversation_for_site(surface, site_id)?)
    }
    pub fn buffered_restore_in_conversation(
        &self,
        id: PublicId,
        encoding: core::RestoreEncoding,
    ) -> Result<core::BufferedRestore> {
        self.ready()?;
        Ok(self
            .workspace
            .buffered_restore_in_conversation(id, encoding)?)
    }
    pub fn progressive_restore_in_conversation(
        &self,
        id: PublicId,
        encoding: core::RestoreEncoding,
    ) -> Result<core::ProgressiveRestore> {
        self.ready()?;
        Ok(self
            .workspace
            .progressive_restore_in_conversation(id, encoding)?)
    }
    pub fn send_intent_status(&self, intent: core::SendIntentId) -> Result<core::SendStatus> {
        self.ready()?;
        Ok(self.workspace.send_intent_status(intent)?)
    }
    pub fn buffered_restore(
        &self,
        encoding: core::RestoreEncoding,
    ) -> Result<core::BufferedRestore> {
        self.ready()?;
        Ok(self.workspace.buffered_restore(encoding)?)
    }
    pub fn lock(&mut self) {
        self.active = false;
        self.workspace.close();
        self.vault.lock();
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;
    fn id() -> PublicId {
        let mut out = [0; 16];
        getrandom::fill(&mut out).unwrap();
        out
    }
    #[test]
    fn lost_ack_after_actual_publication_requires_reconciliation_and_fresh_owner() {
        let temp = tempfile::tempdir().unwrap();
        let host = vault::Host::create_new(temp.path().join("control")).unwrap();
        let path = temp.path().join("vault.sqlite");
        let password = b"synthetic lost receipt fixture";
        let limits = vault::Limits {
            max_payload_bytes: core::MAX_BUNDLE_BYTES,
            max_metadata_bytes: BINDING_BYTES,
            max_records: 1,
        };
        let kdf = vault::KdfPolicy::baseline();
        let mut vault = host.create(&path, password, limits, kdf.clone()).unwrap();
        let token = vault.unlock(password).unwrap();
        let record = id();
        let namespace = id();
        let mut owner = Owner::create(vault, token, record, namespace, id(), id()).unwrap();
        let text = "alice@example.test";
        let finding = core::WorkspaceFinding::new(
            "EMAIL",
            text,
            datafog_core::TextRange {
                start: 0,
                end: text.len(),
            },
        );
        let change = owner.prepare(text, &[finding], id()).unwrap();
        let (parent, saved) = change.heads().unwrap();
        let marker = change.previews().unwrap()[0].text.clone();
        let request_id = id();
        let binding = Binding {
            namespace,
            record_id: record,
            parent,
            saved,
        };
        // Publish via the actual maintained native SQLite API, then inject loss
        // of that result at the adapter's receipt boundary. This is host-ack
        // fault injection, not a claim that FogVault itself returned unknown.
        owner
            .vault
            .save_at(
                &owner.token,
                owner.receipt.head_digest,
                record,
                Some(1),
                &binding.encode().unwrap(),
                change.plan.export_candidate_bundle().unwrap(),
                request_id,
            )
            .unwrap();
        assert!(
            matches!(owner.accepted_receipt(Err(vault::Error::OutcomeUnknown {request_id})), Err(Error::Vault(vault::Error::OutcomeUnknown {request_id: got})) if got == request_id)
        );
        assert!(change.previews().is_err());
        assert!(matches!(owner.restore(&marker), Err(Error::ReopenRequired)));
        assert_eq!(owner.reconcile(request_id).unwrap().request_id, request_id);
        assert!(matches!(owner.head(), Err(Error::ReopenRequired)));
        drop(owner);
        let mut vault = host.open_vault(&path, limits, kdf).unwrap();
        let token = vault.unlock(password).unwrap();
        let reopened = Owner::open(vault, token, record, namespace, Some(saved)).unwrap();
        assert_eq!(reopened.restore(&marker).unwrap(), text);
        assert!(change.previews().is_err());
    }
}

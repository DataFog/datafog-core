//! Proposed content-free history. Host timestamps and outcomes are assertions, not authority.
use super::*;

/// Approved provisional retained-history ceiling; records are never evicted.
pub const MAX_OPERATIONS: usize = 100_000;
/// Only recovery may consume these final slots. Recovery still has a finite ceiling.
pub const RESERVED_RECOVERY_OPERATIONS: usize = 256;
pub const MAX_ORDINARY_OPERATIONS: usize = MAX_OPERATIONS - RESERVED_RECOVERY_OPERATIONS;
pub const MAX_OPERATION_TIME_MS: u64 = 253_402_300_799_999;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationContext {
    pub id: PublicId,
    pub occurred_at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Create,
    Prepare,
    Recovery,
    CopyConfirmed,
    UserCancelled,
    BlockedInput,
    RestoreBlocked,
    PolicyChange,
}
impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Prepare => "prepare",
            Self::Recovery => "recovery",
            Self::CopyConfirmed => "copy_confirmed",
            Self::UserCancelled => "user_cancelled",
            Self::BlockedInput => "blocked_input",
            Self::RestoreBlocked => "restore_blocked",
            Self::PolicyChange => "policy_change",
        }
    }
    pub(super) fn code(self) -> u8 {
        match self {
            Self::Create => 0,
            Self::Prepare => 1,
            Self::Recovery => 2,
            Self::CopyConfirmed => 3,
            Self::UserCancelled => 4,
            Self::BlockedInput => 5,
            Self::RestoreBlocked => 6,
            Self::PolicyChange => 7,
        }
    }
    pub(super) fn from_code(code: u8) -> Result<Self> {
        match code {
            0 => Ok(Self::Create),
            1 => Ok(Self::Prepare),
            2 => Ok(Self::Recovery),
            3 => Ok(Self::CopyConfirmed),
            4 => Ok(Self::UserCancelled),
            5 => Ok(Self::BlockedInput),
            6 => Ok(Self::RestoreBlocked),
            7 => Ok(Self::PolicyChange),
            _ => Err(error(ErrorCode::InvalidSnapshot)),
        }
    }
}

/// Numeric deltas make aggregate counts independently verifiable without content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRecord {
    pub id: PublicId,
    pub kind: OperationKind,
    pub occurred_at_ms: Option<u64>,
    pub prepared_inputs: u64,
    pub transformed_spans: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationCounts {
    pub operations: u64,
    pub prepared_inputs: u64,
    pub transformed_spans: u64,
    /// Reserved at zero until an explicit observed-handoff transaction is implemented.
    pub confirmed_handoffs: u64,
    pub reported_blocked_attempts: u64,
}
/// Describes prior history without inventing counts or historical timestamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreUpgradeHistory {
    Unknown,
    NotApplicable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationHistoryCoverage {
    pub recorded_history_only: bool,
    pub pre_upgrade_history: PreUpgradeHistory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    CopyConfirmed,
    UserCancelled,
    BlockedInput,
    RestoreBlocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationOutcome {
    pub kind: OutcomeKind,
    /// Validated but not retained: this slice counts blocked attempts, not their content.
    pub issue_count: usize,
}

pub(super) fn summarize(records: &[OperationRecord]) -> Result<OperationCounts> {
    if records.len() > MAX_OPERATIONS {
        return Err(error(ErrorCode::InvalidSnapshot));
    }
    let mut ids = BTreeSet::new();
    let mut counts = OperationCounts::default();
    for (index, record) in records.iter().enumerate() {
        if !ids.insert(record.id)
            || record
                .occurred_at_ms
                .is_some_and(|time| time > MAX_OPERATION_TIME_MS)
            || (record.kind == OperationKind::Create && index != 0)
            || (matches!(
                record.kind,
                OperationKind::CopyConfirmed
                    | OperationKind::UserCancelled
                    | OperationKind::BlockedInput
                    | OperationKind::RestoreBlocked
            ) && record.occurred_at_ms.is_none())
            || (record.kind == OperationKind::Prepare
                && !(1..=MAX_INPUTS as u64).contains(&record.prepared_inputs))
            || (record.kind != OperationKind::Prepare
                && (record.prepared_inputs != 0 || record.transformed_spans != 0))
            || record.transformed_spans > MAX_FINDINGS as u64
        {
            return Err(error(ErrorCode::InvalidSnapshot));
        }
        // Bounds above cap these sums well below JavaScript's exact-integer limit.
        counts.operations += 1;
        counts.prepared_inputs += record.prepared_inputs;
        counts.transformed_spans += record.transformed_spans;
        if matches!(
            record.kind,
            OperationKind::BlockedInput | OperationKind::RestoreBlocked
        ) {
            counts.reported_blocked_attempts += 1;
        }
    }
    Ok(counts)
}
impl State {
    pub(super) fn validate_operation_id(&self, id: PublicId, legacy: bool) -> Result<()> {
        if self.head.commit_id == id
            || self.entries.values().any(|entry| entry.issuance == id)
            || self.rules.values().any(|record| record.operation == id)
            || self.choices.values().any(|record| record.operation == id)
        {
            return Err(error(if legacy {
                ErrorCode::InvalidId
            } else {
                ErrorCode::OperationReused
            }));
        }
        if self.operations.iter().any(|operation| operation.id == id) {
            return Err(error(ErrorCode::OperationReused));
        }
        Ok(())
    }
    pub(super) fn admit_record(&self, recovery: bool) -> Result<()> {
        let limit = if recovery {
            MAX_OPERATIONS
        } else {
            MAX_ORDINARY_OPERATIONS
        };
        if self.operations.len() >= limit {
            return Err(error(ErrorCode::ResourceLimit));
        }
        Ok(())
    }
    pub(super) fn record_operation(&mut self, operation: OperationRecord) -> Result<()> {
        self.admit_record(operation.kind == OperationKind::Recovery)?;
        self.operations
            .try_reserve_exact(1)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        self.operations.push(operation);
        self.operation_counts = summarize(&self.operations)?;
        self.snapshot_version = 3;
        Ok(())
    }
}
fn validate_context(operation: OperationContext) -> Result<()> {
    if operation.occurred_at_ms > MAX_OPERATION_TIME_MS {
        return Err(error(ErrorCode::InvalidOperation));
    }
    Ok(())
}
impl Workspace {
    /// Initial creation is an unsaved bootstrap until the host persists its bundle.
    pub fn create_with_operation(namespace: PublicId, operation: OperationContext) -> Result<Self> {
        validate_context(operation)?;
        let mut workspace = Self::create(namespace, operation.id)?;
        if let Some(state) = workspace.state.as_mut() {
            Arc::make_mut(state).operations[0].occurred_at_ms = Some(operation.occurred_at_ms);
        }
        Ok(workspace)
    }
    pub fn operations(&self) -> Result<&[OperationRecord]> {
        Ok(&self.state()?.operations)
    }
    /// Counts always describe recorded rows; old unrecorded activity is not a lifetime total.
    pub fn history_coverage(&self) -> Result<OperationHistoryCoverage> {
        let state = self.state()?;
        Ok(OperationHistoryCoverage {
            recorded_history_only: true,
            pre_upgrade_history: if state
                .operations
                .first()
                .is_some_and(|record| record.kind == OperationKind::Create)
            {
                PreUpgradeHistory::NotApplicable
            } else {
                PreUpgradeHistory::Unknown
            },
        })
    }
    pub fn counts(&self) -> Result<OperationCounts> {
        Ok(self.state()?.operation_counts)
    }
    pub fn prepare_with_operation(
        &self,
        text: &str,
        findings: &[WorkspaceFinding],
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        self.prepare_batch_with_operation(&[WorkspaceInput { text, findings }], operation)
    }
    /// Explicitly records a successful preparation, even when no mappings change.
    pub fn prepare_batch_with_operation(
        &self,
        inputs: &[WorkspaceInput<'_>],
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        validate_context(operation)?;
        self.prepare_batch_internal(inputs, operation.id, Some(operation.occurred_at_ms), true)
    }
    pub fn prepare_recovery_with_operation(
        &self,
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        validate_context(operation)?;
        self.prepare_recovery_internal(operation.id, Some(operation.occurred_at_ms), false)
    }
    /// Optional host-reported outcome. It grants no delivery or release authority.
    pub fn prepare_outcome(
        &self,
        outcome: OperationOutcome,
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        validate_context(operation)?;
        let state = self.state()?;
        state.validate_operation_id(operation.id, false)?;
        let (kind, blocked) = match outcome.kind {
            OutcomeKind::CopyConfirmed => (OperationKind::CopyConfirmed, false),
            OutcomeKind::UserCancelled => (OperationKind::UserCancelled, false),
            OutcomeKind::BlockedInput => (OperationKind::BlockedInput, true),
            OutcomeKind::RestoreBlocked => (OperationKind::RestoreBlocked, true),
        };
        if outcome.issue_count > MAX_FINDINGS || blocked != (outcome.issue_count > 0) {
            return Err(error(ErrorCode::InvalidOperation));
        }
        state.admit_record(false)?;
        let mut candidate = state.clone();
        candidate.head = Head {
            revision: state
                .head
                .revision
                .checked_add(1)
                .ok_or_else(|| error(ErrorCode::ResourceLimit))?,
            commit_id: operation.id,
        };
        candidate.record_operation(OperationRecord {
            id: operation.id,
            kind,
            occurred_at_ms: Some(operation.occurred_at_ms),
            prepared_inputs: 0,
            transformed_spans: 0,
        })?;
        let bundle = Zeroizing::new(codec::encode_for_record(&candidate, false)?);
        Ok(PreparedChange {
            pending: Some(Pending {
                lifetime: Arc::clone(&self.lifetime),
                parent: state.head,
                parent_snapshot_version: state.snapshot_version,
                candidate,
                bundle,
                previews: Vec::new(),
                policy_preview: None,
            }),
        })
    }
}

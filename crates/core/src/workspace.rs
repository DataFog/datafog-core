//! Provisional, reversible workspace API for the synthetic browser workflow.
//!
//! Callers supply findings and fresh public random IDs. The host owns credentials,
//! encryption, approval and durable storage; activating a candidate trusts that host.
mod codec;

use crate::{Finding, TextIndex, TextRange, TransformationConfig, TransformationStrategy};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_FINDINGS: usize = 10_000;
pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
pub const RESERVED_INTRODUCER: &str = "[[DFP";
pub type PublicId = [u8; 16];

/// Complete optimistic-concurrency identity, not an authorization credential.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Head {
    pub revision: u64,
    pub commit_id: PublicId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    ClosedWorkspace,
    InactivePlan,
    WorkspaceConflict,
    InvalidFinding,
    InvalidMarker,
    InvalidSnapshot,
    InvalidId,
    ResourceLimit,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClosedWorkspace => "closed_workspace",
            Self::InactivePlan => "inactive_plan",
            Self::WorkspaceConflict => "workspace_conflict",
            Self::InvalidFinding => "invalid_finding",
            Self::InvalidMarker => "invalid_marker",
            Self::InvalidSnapshot => "invalid_snapshot",
            Self::InvalidId => "invalid_operation_id",
            Self::ResourceLimit => "resource_limit",
        }
    }
}

/// Content-free machine-readable failure. Indexes refer to supplied findings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceError {
    pub code: ErrorCode,
    pub finding_index: Option<usize>,
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code.as_str())
    }
}
impl std::error::Error for WorkspaceError {}
fn error(code: ErrorCode) -> WorkspaceError {
    WorkspaceError {
        code,
        finding_index: None,
    }
}
type Result<T> = std::result::Result<T, WorkspaceError>;

/// Exact source span in UTF-8 bytes. Metadata is optional and never persisted.
#[derive(Clone)]
pub struct WorkspaceFinding {
    pub entity_type: String,
    pub matched_text: String,
    pub byte_range: TextRange,
    pub confidence: Option<f32>,
    pub detector_name: Option<String>,
    pub detector_version: Option<String>,
}

impl WorkspaceFinding {
    pub fn new(
        entity_type: impl Into<String>,
        matched_text: impl Into<String>,
        byte_range: TextRange,
    ) -> Self {
        Self {
            entity_type: entity_type.into(),
            matched_text: matched_text.into(),
            byte_range,
            confidence: None,
            detector_name: None,
            detector_version: None,
        }
    }
}

/// A transformed span, with explicit zero-based, end-exclusive coordinate systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transformation {
    pub source_byte_range: TextRange,
    pub source_codepoint_range: TextRange,
    pub source_utf16_range: TextRange,
    pub output_byte_range: TextRange,
    pub output_codepoint_range: TextRange,
    pub output_utf16_range: TextRange,
}

pub struct Preview {
    pub text: String,
    pub transformations: Vec<Transformation>,
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Identity {
    entity_type: String,
    original: String,
}
impl Drop for Identity {
    fn drop(&mut self) {
        self.original.zeroize();
    }
}
#[derive(Clone)]
struct Entry {
    marker: String,
    issuance: PublicId,
    counter: u64,
}
#[derive(Clone)]
struct State {
    namespace: PublicId,
    head: Head,
    entries: BTreeMap<Identity, Entry>,
    counters: BTreeMap<String, u64>,
}

struct Pending {
    lifetime: Arc<AtomicBool>,
    parent: Head,
    candidate: State,
    bundle: Zeroizing<Vec<u8>>,
    preview: Preview,
}

/// An immutable pending change. Preview/export do not consume it; activation or discard do.
pub struct PreparedChange {
    pending: Option<Pending>,
}
impl PreparedChange {
    fn pending(&self) -> Result<&Pending> {
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| error(ErrorCode::InactivePlan))?;
        if !pending.lifetime.load(Ordering::Acquire) {
            return Err(error(ErrorCode::ClosedWorkspace));
        }
        Ok(pending)
    }
    pub fn preview(&self) -> Result<&Preview> {
        Ok(&self.pending()?.preview)
    }
    pub fn expected_head(&self) -> Result<Head> {
        Ok(self.pending()?.parent)
    }
    pub fn candidate_head(&self) -> Result<Head> {
        Ok(self.pending()?.candidate.head)
    }
    /// Sensitive plaintext bytes for the trusted encrypting host only.
    pub fn export_candidate_bundle(&self) -> Result<&[u8]> {
        Ok(&self.pending()?.bundle)
    }
    pub fn discard(&mut self) {
        self.pending = None;
    }
}

/// One runtime workspace. Closing it never deletes its saved bundle.
pub struct Workspace {
    state: Option<State>,
    lifetime: Arc<AtomicBool>,
}
impl Drop for Workspace {
    fn drop(&mut self) {
        self.close();
    }
}
impl Workspace {
    fn from_state(state: State) -> Result<Self> {
        Ok(Self {
            state: Some(state),
            lifetime: Arc::new(AtomicBool::new(true)),
        })
    }
    fn state(&self) -> Result<&State> {
        self.state
            .as_ref()
            .ok_or_else(|| error(ErrorCode::ClosedWorkspace))
    }
    /// IDs must come from the host OS CSPRNG. Shape checks cannot prove entropy.
    pub fn create(namespace: PublicId, operation_id: PublicId) -> Result<Self> {
        Self::from_state(State {
            namespace,
            head: Head {
                revision: 0,
                commit_id: operation_id,
            },
            entries: BTreeMap::new(),
            counters: BTreeMap::new(),
        })
    }
    pub fn open_bundle(bytes: &[u8]) -> Result<Self> {
        Self::from_state(codec::decode(bytes)?)
    }
    pub fn export_bundle(&self) -> Result<Vec<u8>> {
        codec::encode(self.state()?)
    }
    pub fn head(&self) -> Result<Head> {
        Ok(self.state()?.head)
    }
    pub fn namespace(&self) -> Result<PublicId> {
        Ok(self.state()?.namespace)
    }
    pub fn close(&mut self) {
        self.lifetime.store(false, Ordering::Release);
        self.state = None;
    }

    pub fn prepare(
        &self,
        text: &str,
        findings: &[WorkspaceFinding],
        operation_id: PublicId,
    ) -> Result<PreparedChange> {
        let state = self.state()?;
        source_limit(text)?;
        if text.contains(RESERVED_INTRODUCER) {
            return Err(error(ErrorCode::InvalidMarker));
        }
        if findings.len() > MAX_FINDINGS {
            return Err(error(ErrorCode::ResourceLimit));
        }
        if state.head.commit_id == operation_id
            || state.entries.values().any(|e| e.issuance == operation_id)
        {
            return Err(error(ErrorCode::InvalidId));
        }
        let mut index = TextIndex::new(text);
        let mut adapted = Vec::with_capacity(findings.len());
        let mut total_bytes = 0usize;
        for (finding_index, finding) in findings.iter().enumerate() {
            let invalid = || WorkspaceError {
                code: ErrorCode::InvalidFinding,
                finding_index: Some(finding_index),
            };
            if !valid_type(&finding.entity_type)
                || finding.byte_range.start >= finding.byte_range.end
                || crate::validate_utf8_range(text, finding.byte_range).is_err()
                || finding
                    .detector_name
                    .as_ref()
                    .is_some_and(|v| v.len() > 128)
                || finding
                    .detector_version
                    .as_ref()
                    .is_some_and(|v| v.len() > 128)
            {
                return Err(invalid());
            }
            total_bytes = total_bytes
                .checked_add(finding.matched_text.len())
                .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
            if total_bytes > MAX_BUNDLE_BYTES {
                return Err(error(ErrorCode::ResourceLimit));
            }
            adapted.push(Finding {
                entity_type: finding.entity_type.clone(),
                matched_text: finding.matched_text.clone(),
                byte_range: finding.byte_range,
                codepoint_range: TextRange {
                    start: index.codepoint_offset(finding.byte_range.start),
                    end: index.codepoint_offset(finding.byte_range.end),
                },
                confidence: finding.confidence,
                detector_name: finding.detector_name.clone().unwrap_or_default(),
                detector_version: finding.detector_version.clone(),
            });
        }
        let config = TransformationConfig::new(TransformationStrategy::Redact);
        let selected =
            crate::select_findings(text, &adapted, &config).map_err(|e| WorkspaceError {
                code: ErrorCode::InvalidFinding,
                finding_index: e.finding_index(),
            })?;
        let mut candidate = state.clone();
        let mut output = String::new();
        let mut transformations = Vec::with_capacity(selected.len());
        let mut cursor = 0;
        let mut changed = false;
        for finding in selected {
            let identity = Identity {
                entity_type: finding.entity_type.clone(),
                original: finding.matched_text.clone(),
            };
            if !candidate.entries.contains_key(&identity) {
                if candidate.entries.len() >= MAX_ENTRIES {
                    return Err(error(ErrorCode::ResourceLimit));
                }
                let counter = candidate
                    .counters
                    .get(&finding.entity_type)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
                let entry = Entry {
                    marker: marker(
                        candidate.namespace,
                        operation_id,
                        &finding.entity_type,
                        counter,
                    ),
                    issuance: operation_id,
                    counter,
                };
                candidate
                    .counters
                    .insert(finding.entity_type.clone(), counter);
                candidate.entries.insert(identity.clone(), entry);
                changed = true;
            }
            let entry = candidate
                .entries
                .get(&identity)
                .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
            append(&mut output, &text[cursor..finding.byte_range.start])?;
            let output_start = output.len();
            append(&mut output, &entry.marker)?;
            transformations.push(Transformation {
                source_byte_range: finding.byte_range,
                source_codepoint_range: finding.codepoint_range,
                source_utf16_range: index
                    .utf16_range(finding.byte_range)
                    .map_err(|_| error(ErrorCode::InvalidFinding))?,
                output_byte_range: TextRange {
                    start: output_start,
                    end: output.len(),
                },
                output_codepoint_range: TextRange { start: 0, end: 0 },
                output_utf16_range: TextRange { start: 0, end: 0 },
            });
            cursor = finding.byte_range.end;
        }
        append(&mut output, &text[cursor..])?;
        let mut output_index = TextIndex::new(&output);
        for transformation in &mut transformations {
            let range = transformation.output_byte_range;
            transformation.output_codepoint_range = TextRange {
                start: output_index.codepoint_offset(range.start),
                end: output_index.codepoint_offset(range.end),
            };
            transformation.output_utf16_range = output_index
                .utf16_range(range)
                .map_err(|_| error(ErrorCode::InvalidFinding))?;
        }
        if changed {
            candidate.head = Head {
                revision: state
                    .head
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| error(ErrorCode::ResourceLimit))?,
                commit_id: operation_id,
            };
        }
        let bundle = Zeroizing::new(codec::encode(&candidate)?);
        Ok(PreparedChange {
            pending: Some(Pending {
                lifetime: Arc::clone(&self.lifetime),
                parent: state.head,
                candidate,
                bundle,
                preview: Preview {
                    text: output,
                    transformations,
                },
            }),
        })
    }

    /// Adopt an exact saved candidate. The host must first confirm atomic persistence.
    pub fn activate_saved(
        &mut self,
        plan: &mut PreparedChange,
        expected_parent: Head,
        saved_head: Head,
    ) -> Result<()> {
        let state = self.state()?;
        let pending = plan.pending()?;
        if !Arc::ptr_eq(&pending.lifetime, &self.lifetime)
            || state.head != expected_parent
            || pending.parent != expected_parent
            || pending.candidate.head != saved_head
        {
            return Err(error(ErrorCode::WorkspaceConflict));
        }
        let pending = plan
            .pending
            .take()
            .ok_or_else(|| error(ErrorCode::InactivePlan))?;
        self.state = Some(pending.candidate);
        Ok(())
    }

    /// Restore only exact, committed full markers; failures expose no partial output.
    pub fn restore(&self, text: &str) -> Result<String> {
        let state = self.state()?;
        if text.len() > MAX_BUNDLE_BYTES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let lookup: BTreeMap<_, _> = state
            .entries
            .iter()
            .map(|(id, e)| (e.marker.as_str(), id.original.as_str()))
            .collect();
        let mut output = String::new();
        let mut cursor = 0;
        while let Some(relative) = text[cursor..].find(RESERVED_INTRODUCER) {
            let start = cursor + relative;
            let tail = &text[start..];
            let end = tail
                .find("]]")
                .filter(|end| *end <= 140)
                .ok_or_else(|| error(ErrorCode::InvalidMarker))?
                + 2;
            let full = &tail[..end];
            let original = lookup
                .get(full)
                .ok_or_else(|| error(ErrorCode::InvalidMarker))?;
            append(&mut output, &text[cursor..start])?;
            append(&mut output, original)?;
            cursor = start + end;
        }
        append(&mut output, &text[cursor..])?;
        Ok(output)
    }
}

fn valid_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn marker(namespace: PublicId, operation: PublicId, label: &str, counter: u64) -> String {
    format!(
        "[[DFP1:{}:{}:{label}:{counter}]]",
        URL_SAFE_NO_PAD.encode(namespace),
        URL_SAFE_NO_PAD.encode(operation)
    )
}
fn source_limit(text: &str) -> Result<()> {
    if text.len() > MAX_SOURCE_BYTES {
        Err(error(ErrorCode::ResourceLimit))
    } else {
        Ok(())
    }
}
fn append(output: &mut String, value: &str) -> Result<()> {
    if output
        .len()
        .checked_add(value.len())
        .is_none_or(|n| n > MAX_BUNDLE_BYTES)
    {
        return Err(error(ErrorCode::ResourceLimit));
    }
    output
        .try_reserve(value.len())
        .map_err(|_| error(ErrorCode::ResourceLimit))?;
    output.push_str(value);
    Ok(())
}

/// Convert a browser selection through Core, rejecting split surrogate-pair boundaries.
pub fn byte_range_from_utf16(text: &str, range: TextRange) -> Result<TextRange> {
    source_limit(text)?;
    if range.start > range.end {
        return Err(error(ErrorCode::InvalidFinding));
    }
    let mut index = TextIndex::new(text);
    index
        .byte_range_from_utf16(range)
        .map_err(|_| error(ErrorCode::InvalidFinding))
}

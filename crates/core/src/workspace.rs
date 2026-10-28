//! Provisional, reversible workspace API for the synthetic browser workflow.
//!
//! Callers supply findings and fresh public random IDs. The host owns credentials,
//! encryption, approval and durable storage; activating a candidate trusts that host.
mod codec;

use crate::{Finding, TextIndex, TextRange, TransformationConfig, TransformationStrategy};
use hmac::{Hmac, Mac};
use sha2::Sha256;
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
/// Entity type labels are uppercase ASCII and appear verbatim inside markers.
pub const MAX_LABEL_BYTES: usize = 32;
/// Markers are `[[LABEL_CODE]]` with a random Crockford base32 code of this length.
pub const MARKER_CODE_LENGTH: usize = 6;
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
/// `code` is canonical uppercase Crockford base32 and unique within the workspace.
/// `issuance` records the operation that created the entry; it never leaves the vault.
#[derive(Clone)]
struct Entry {
    code: String,
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
        if marker_spans(text)?
            .iter()
            .any(|span| span.canonical.is_some())
        {
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
        let mut codes: BTreeSet<String> = state.entries.values().map(|e| e.code.clone()).collect();
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
                let code = issue_code(
                    candidate.namespace,
                    operation_id,
                    &finding.entity_type,
                    counter,
                    &mut codes,
                )?;
                let entry = Entry {
                    code,
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
            append(&mut output, &marker(&identity.entity_type, &entry.code))?;
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

    /// Restore committed markers; any other marker-like span fails the whole call so
    /// no partially restored text is exposed. Marker matching ignores case and accepts
    /// Crockford's `I`/`L` → `1` and `O` → `0` substitutions in the code.
    ///
    /// Any `[[LABEL_CODE]]`-shaped span that does not resolve to a committed entry
    /// blocks restoration, including accidental patterns such as `[[my_note]]` or
    /// `[[TODO_ABC123]]`. These are considered damaged markers; the caller should
    /// remove or escape them before retrying.
    pub fn restore(&self, text: &str) -> Result<String> {
        let state = self.state()?;
        if text.len() > MAX_BUNDLE_BYTES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let lookup = marker_lookup(state);
        let mut output = String::new();
        let mut cursor = 0;
        for span in marker_spans(text)? {
            let original = span
                .canonical
                .as_ref()
                .and_then(|canonical| lookup.get(canonical))
                .ok_or_else(|| error(ErrorCode::InvalidMarker))?;
            append(&mut output, &text[cursor..span.range.start])?;
            append(&mut output, original)?;
            cursor = span.range.end;
        }
        append(&mut output, &text[cursor..])?;
        Ok(output)
    }
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Longest `[[...]]` interior inspected as a possible marker: label, separator and code,
/// with slack so moderately damaged markers are still recognized and rejected.
const MAX_MARKER_INTERIOR: usize = 2 * (MAX_LABEL_BYTES + 1 + MARKER_CODE_LENGTH);
const MAX_CODE_ATTEMPTS: u32 = 32;

/// Canonical marker text → original value, for every committed entry.
fn marker_lookup(state: &State) -> BTreeMap<String, &str> {
    state
        .entries
        .iter()
        .map(|(identity, entry)| {
            (
                marker(&identity.entity_type, &entry.code),
                identity.original.as_str(),
            )
        })
        .collect()
}

/// A `[[...]]` span whose interior looks like `LABEL_CODE`. `canonical` is the
/// normalized marker when the interior is well formed, and `None` for a damaged one.
struct MarkerSpan {
    range: TextRange,
    canonical: Option<String>,
}

fn marker_spans(text: &str) -> Result<Vec<MarkerSpan>> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find("[[") {
        let start = cursor + relative;
        let interior_start = start + 2;
        let interior_end = bytes[interior_start..]
            .iter()
            .take(MAX_MARKER_INTERIOR + 1)
            .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
            .map(|offset| interior_start + offset);
        let Some(interior_end) = interior_end else {
            cursor = start + 1;
            continue;
        };
        let interior = &text[interior_start..interior_end];
        let marker_like = interior_end - interior_start <= MAX_MARKER_INTERIOR
            && interior
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
            && interior.contains('_')
            && text[interior_end..].starts_with("]]");
        if !marker_like {
            cursor = start + 1;
            continue;
        }
        spans
            .try_reserve(1)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        spans.push(MarkerSpan {
            range: TextRange {
                start,
                end: interior_end + 2,
            },
            canonical: canonical_marker(interior),
        });
        cursor = interior_end + 2;
    }
    Ok(spans)
}

fn canonical_marker(interior: &str) -> Option<String> {
    let separator = interior.len().checked_sub(MARKER_CODE_LENGTH + 1)?;
    let (label, code) = interior.split_at(separator);
    let code = code.strip_prefix('_')?;
    let label = label.to_ascii_uppercase();
    if !valid_type(&label) {
        return None;
    }
    let code = code
        .bytes()
        .map(|b| match b.to_ascii_uppercase() {
            b'I' | b'L' => Some(b'1'),
            b'O' => Some(b'0'),
            upper if CROCKFORD.contains(&upper) => Some(upper),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()?;
    Some(marker(&label, std::str::from_utf8(&code).ok()?))
}

fn valid_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LABEL_BYTES
        && value.as_bytes()[0].is_ascii_uppercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn valid_code(code: &str) -> bool {
    code.len() == MARKER_CODE_LENGTH && code.bytes().all(|b| CROCKFORD.contains(&b))
}

fn marker(label: &str, code: &str) -> String {
    format!("[[{label}_{code}]]")
}

/// Derive a fresh code from the host-random operation ID. Codes carry no information
/// about the value; they are unpredictable to a model guessing markers, and are
/// re-drawn on collision so each code is unique within the workspace.
fn issue_code(
    namespace: PublicId,
    operation: PublicId,
    label: &str,
    counter: u64,
    codes: &mut BTreeSet<String>,
) -> Result<String> {
    for attempt in 0..MAX_CODE_ATTEMPTS {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&operation).map_err(|_| error(ErrorCode::InvalidId))?;
        mac.update(b"datafog-workspace-marker-code\0");
        mac.update(&namespace);
        mac.update(label.as_bytes());
        mac.update(&[0]);
        mac.update(&counter.to_be_bytes());
        mac.update(&attempt.to_be_bytes());
        let digest = mac.finalize().into_bytes();
        let bits = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) >> 2;
        let code: String = (0..MARKER_CODE_LENGTH)
            .map(|index| {
                let shift = 5 * (MARKER_CODE_LENGTH - 1 - index);
                char::from(CROCKFORD[((bits >> shift) & 31) as usize])
            })
            .collect();
        if codes.insert(code.clone()) {
            return Ok(code);
        }
    }
    Err(error(ErrorCode::ResourceLimit))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn all_attempt_codes(
        namespace: PublicId,
        operation: PublicId,
        label: &str,
        counter: u64,
    ) -> Vec<String> {
        (0..MAX_CODE_ATTEMPTS)
            .map(|attempt| {
                let mut mac = Hmac::<Sha256>::new_from_slice(&operation).unwrap();
                mac.update(b"datafog-workspace-marker-code\0");
                mac.update(&namespace);
                mac.update(label.as_bytes());
                mac.update(&[0]);
                mac.update(&counter.to_be_bytes());
                mac.update(&attempt.to_be_bytes());
                let digest = mac.finalize().into_bytes();
                let bits = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) >> 2;
                (0..MARKER_CODE_LENGTH)
                    .map(|index| {
                        let shift = 5 * (MARKER_CODE_LENGTH - 1 - index);
                        char::from(CROCKFORD[((bits >> shift) & 31) as usize])
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn issue_code_redraws_on_collision_and_exhausts_after_max_attempts() {
        let namespace = [1u8; 16];
        let operation = [0u8; 16];
        let mut codes = BTreeSet::new();

        let first = issue_code(namespace, operation, "PERSON", 0, &mut codes).unwrap();
        assert!(valid_code(&first));
        assert!(codes.contains(&first));

        let second = issue_code(namespace, operation, "PERSON", 1, &mut codes).unwrap();
        assert!(valid_code(&second));
        assert_ne!(first, second);
        assert!(codes.contains(&second));

        let all_codes: BTreeSet<String> = all_attempt_codes(namespace, operation, "PERSON", 2)
            .into_iter()
            .collect();
        let mut blocking = all_codes.clone();
        let result = issue_code(namespace, operation, "PERSON", 2, &mut blocking);
        assert_eq!(result.err().unwrap().code, ErrorCode::ResourceLimit);

        let mut partial = all_codes.iter().skip(1).cloned().collect();
        let fallback = issue_code(namespace, operation, "PERSON", 2, &mut partial).unwrap();
        assert_eq!(fallback, all_codes.iter().next().unwrap().clone());
    }
}

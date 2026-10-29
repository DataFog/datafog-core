//! Combined review is read-only until a conflict-free result is prepared and saved.
use super::*;
use sha2::Digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewConflictKind {
    ProtectedCoverage,
    TypeConflict,
}

/// Content-free original-input coordinates. Indices refer to scanner, user, then recall inputs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewConflict {
    pub kind: ReviewConflictKind,
    pub finding_index: usize,
    pub byte_range: TextRange,
    pub codepoint_range: TextRange,
    pub utf16_range: TextRange,
}

/// Replace one current candidate. Removing protected coverage requires a saved exact choice.
pub struct ReviewCorrection {
    pub candidate_key: String,
    pub replacement: WorkspaceFinding,
}

#[derive(Clone, Copy)]
pub struct ReviewInput<'a> {
    pub text: &'a str,
    pub scanner_findings: &'a [WorkspaceFinding],
    pub user_selections: &'a [WorkspaceFinding],
    pub corrections: &'a [ReviewCorrection],
}

pub struct ReviewCandidate {
    /// Ephemeral key bound to exact input, current head, type and source range.
    pub key: String,
    pub finding: WorkspaceFinding,
    pub sources: Vec<FindingSource>,
    pub codepoint_range: TextRange,
    pub utf16_range: TextRange,
    /// Committed policy requires this candidate to be hidden.
    pub eligible: bool,
    /// Selection for preparation; never permission to send or skip review.
    pub selected: bool,
}

pub struct ReviewResult {
    pub head: Head,
    pub candidates: Vec<ReviewCandidate>,
    pub conflicts: Vec<ReviewConflict>,
    pub(super) finding_count: usize,
    pub(super) finding_bytes: usize,
}

fn hides(state: &State, finding: &Finding) -> bool {
    state.hides(&Identity {
        entity_type: finding.entity_type.clone(),
        key: identity_key(&finding.matched_text),
    })
}

fn conflict(
    index: &mut TextIndex,
    finding: &Finding,
    finding_index: usize,
    kind: ReviewConflictKind,
) -> Result<ReviewConflict> {
    Ok(ReviewConflict {
        kind,
        finding_index,
        byte_range: finding.byte_range,
        codepoint_range: finding.codepoint_range,
        utf16_range: index
            .utf16_range(finding.byte_range)
            .map_err(|_| error(ErrorCode::InvalidFinding))?,
    })
}

/// Inputs have all validated, including policy-waived findings. Original obligations
/// allow corrections to expand coverage without silently erasing protected suffixes.
pub(super) fn select(
    text: &str,
    findings: &[Finding],
    state: &State,
    obligations: &[(usize, Finding)],
) -> Result<(Vec<Finding>, Vec<ReviewConflict>)> {
    let eligible: Vec<_> = findings
        .iter()
        .enumerate()
        .filter(|(_, finding)| hides(state, finding))
        .collect();
    let mut native = Vec::new();
    crate::detect_ssn(text, &mut native);
    let native_ranges: BTreeSet<_> = native
        .into_iter()
        .map(|f| (f.start_byte, f.end_byte))
        .collect();
    let ssn_ranges: BTreeSet<_> = eligible
        .iter()
        .filter(|(_, f)| {
            f.entity_type == "SSN"
                && native_ranges.contains(&(f.byte_range.start, f.byte_range.end))
        })
        .map(|(_, f)| (f.byte_range.start, f.byte_range.end))
        .collect();
    let mut groups: BTreeMap<(usize, usize), Vec<(usize, &Finding)>> = BTreeMap::new();
    for &(original_index, finding) in &eligible {
        if finding.entity_type == "PERSON"
            && ssn_ranges.contains(&(finding.byte_range.start, finding.byte_range.end))
        {
            continue;
        }
        groups
            .entry((finding.byte_range.start, finding.byte_range.end))
            .or_default()
            .push((original_index, finding));
    }
    let mut index = TextIndex::new(text);
    let mut conflicts = Vec::new();
    // Whole-cover containment is safe regardless of a caller's origin flags.
    // Crossing intervals remain candidates and are checked after the existing selector.
    let mut ordered: Vec<_> = groups.into_iter().collect();
    ordered.sort_by_key(|((start, end), _)| (*start, std::cmp::Reverse(*end)));
    let mut outer = Vec::new();
    let mut outer_end = 0;
    for ((_, end), group) in ordered {
        if end <= outer_end {
            continue;
        }
        outer_end = end;
        if group
            .iter()
            .map(|(_, finding)| finding.entity_type.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
        {
            for &(original_index, finding) in &group {
                conflicts.push(conflict(
                    &mut index,
                    finding,
                    original_index,
                    ReviewConflictKind::TypeConflict,
                )?);
            }
        }
        outer.extend(group.into_iter().map(|(_, finding)| finding.clone()));
    }
    let config = TransformationConfig::new(TransformationStrategy::Redact);
    let selected = crate::select_validated_findings(&outer, &config);
    let selected_ranges: Vec<_> = selected.iter().map(|finding| finding.byte_range).collect();
    let mut required: Vec<_> = eligible.iter().map(|(i, f)| (*i, *f)).collect();
    required.extend(obligations.iter().map(|(i, f)| (*i, f)));
    let required_ranges: Vec<_> = required.iter().map(|(_, f)| f.byte_range).collect();
    for uncovered in crate::coverage::uncovered(&required_ranges, &selected_ranges) {
        let (original_index, finding) = required[uncovered];
        conflicts.push(conflict(
            &mut index,
            finding,
            original_index,
            ReviewConflictKind::ProtectedCoverage,
        )?);
    }
    conflicts.sort_by_key(|c| (c.finding_index, c.kind as u8));
    conflicts.dedup();
    if conflicts.len() > MAX_FINDINGS {
        return Err(error(ErrorCode::ResourceLimit));
    }
    Ok((selected, conflicts))
}

fn key(head: Head, source_digest: &[u8], finding: &WorkspaceFinding) -> String {
    let mut hash = Sha256::new();
    hash.update(b"datafog/workspace-review/v1\0");
    hash.update(head.revision.to_be_bytes());
    hash.update(head.commit_id);
    hash.update(source_digest);
    hash.update((finding.entity_type.len() as u64).to_be_bytes());
    hash.update(finding.entity_type.as_bytes());
    hash.update((finding.byte_range.start as u64).to_be_bytes());
    hash.update((finding.byte_range.end as u64).to_be_bytes());
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn prefer_metadata(incoming: &WorkspaceFinding, existing: &WorkspaceFinding) -> bool {
    (
        incoming.detector_name.is_none(),
        incoming.detector_version.is_none(),
        incoming.confidence.is_none(),
        &incoming.detector_name,
        &incoming.detector_version,
        incoming.confidence.map(f32::to_bits),
    ) < (
        existing.detector_name.is_none(),
        existing.detector_version.is_none(),
        existing.confidence.is_none(),
        &existing.detector_name,
        &existing.detector_version,
        existing.confidence.map(f32::to_bits),
    )
}
fn merge_metadata(existing: &mut WorkspaceFinding, incoming: &WorkspaceFinding) {
    if prefer_metadata(incoming, existing) {
        existing.detector_name = incoming.detector_name.clone();
        existing.detector_version = incoming.detector_version.clone();
        existing.confidence = incoming.confidence;
    }
    existing.needs_review |= incoming.needs_review;
}

pub(super) fn build(state: &State, input: ReviewInput<'_>) -> Result<ReviewResult> {
    let count = input
        .scanner_findings
        .len()
        .saturating_add(input.user_selections.len())
        .saturating_add(input.corrections.len());
    if count > MAX_FINDINGS {
        return Err(error(ErrorCode::ResourceLimit));
    }
    source_limit(input.text)?;
    let mut supplied_bytes = 0usize;
    for finding in input
        .scanner_findings
        .iter()
        .chain(input.user_selections)
        .chain(input.corrections.iter().map(|c| &c.replacement))
    {
        supplied_bytes = supplied_bytes
            .saturating_add(finding.matched_text.len())
            .saturating_add(finding.entity_type.len())
            .saturating_add(finding.detector_name.as_ref().map_or(0, String::len))
            .saturating_add(finding.detector_version.as_ref().map_or(0, String::len));
        if supplied_bytes > MAX_BUNDLE_BYTES {
            return Err(error(ErrorCode::ResourceLimit));
        }
    }
    if input.corrections.iter().any(|c| {
        c.candidate_key.len() != 64
            || !c
                .candidate_key
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }) {
        return Err(error(ErrorCode::WorkspaceConflict));
    }
    let committed = marker_lookup(state);
    // Validate every supplied claim and correction before policy, grouping or recall.
    let mut supplied = Vec::with_capacity(count);
    supplied.extend(input.scanner_findings.iter().cloned().map(|mut f| {
        f.source = FindingSource::Scanner;
        f
    }));
    supplied.extend(input.user_selections.iter().cloned().map(|mut f| {
        f.source = FindingSource::User;
        f
    }));
    supplied.extend(input.corrections.iter().map(|c| c.replacement.clone()));
    adapt_findings(input.text, &supplied, &committed)?;
    supplied.truncate(count - input.corrections.len());
    let recalled = recall::find(state, input.text)?;
    if count.saturating_add(recalled.findings.len()) > MAX_FINDINGS {
        return Err(error(ErrorCode::ResourceLimit));
    }
    supplied_bytes = supplied_bytes.saturating_add(
        recalled
            .findings
            .iter()
            .map(|found| found.finding.matched_text.len() + found.finding.entity_type.len())
            .sum::<usize>(),
    );
    if supplied_bytes > MAX_BUNDLE_BYTES {
        return Err(error(ErrorCode::ResourceLimit));
    }
    supplied.extend(recalled.findings.into_iter().map(|found| found.finding));
    let finding_count = supplied.len() + input.corrections.len();
    let (original, _) = adapt_findings(input.text, &supplied, &committed)?;
    let obligations: Vec<_> = original
        .into_iter()
        .enumerate()
        .filter(|(_, f)| hides(state, f))
        .collect();
    let source_digest = Sha256::digest(input.text.as_bytes());
    let mut grouped: BTreeMap<(usize, usize, String), ReviewCandidate> = BTreeMap::new();
    let mut index = TextIndex::new(input.text);
    for finding in supplied {
        let id = (
            finding.byte_range.start,
            finding.byte_range.end,
            finding.entity_type.clone(),
        );
        if let Some(candidate) = grouped.get_mut(&id) {
            merge_metadata(&mut candidate.finding, &finding);
            if !candidate.sources.contains(&finding.source) {
                candidate.sources.push(finding.source);
            }
        } else {
            let codepoint_range = TextRange {
                start: index.codepoint_offset(finding.byte_range.start),
                end: index.codepoint_offset(finding.byte_range.end),
            };
            let utf16_range = index
                .utf16_range(finding.byte_range)
                .map_err(|_| error(ErrorCode::InvalidFinding))?;
            grouped.insert(
                id,
                ReviewCandidate {
                    key: key(state.head, &source_digest, &finding),
                    sources: vec![finding.source],
                    finding,
                    codepoint_range,
                    utf16_range,
                    eligible: false,
                    selected: false,
                },
            );
        }
    }
    let mut candidates: Vec<_> = grouped.into_values().collect();
    let positions: BTreeMap<_, _> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (c.key.clone(), i))
        .collect();
    let mut corrected = BTreeSet::new();
    for correction in input.corrections {
        let Some(&position) = positions.get(&correction.candidate_key) else {
            return Err(error(ErrorCode::WorkspaceConflict));
        };
        if !corrected.insert(position) {
            return Err(error(ErrorCode::InvalidFinding));
        }
        let candidate = &mut candidates[position];
        candidate.finding = correction.replacement.clone();
        candidate.finding.source = FindingSource::User;
        candidate.finding.needs_review = true;
        if !candidate.sources.contains(&FindingSource::User) {
            candidate.sources.push(FindingSource::User);
        }
        candidate.key = key(state.head, &source_digest, &candidate.finding);
        candidate.codepoint_range = TextRange {
            start: index.codepoint_offset(candidate.finding.byte_range.start),
            end: index.codepoint_offset(candidate.finding.byte_range.end),
        };
        candidate.utf16_range = index
            .utf16_range(candidate.finding.byte_range)
            .map_err(|_| error(ErrorCode::InvalidFinding))?;
    }
    // Corrections can converge on the same candidate; keep one identity/range and
    // merge all display origins/review requirements without granting precedence.
    let mut merged: BTreeMap<(usize, usize, String), ReviewCandidate> = BTreeMap::new();
    for candidate in candidates {
        let id = (
            candidate.finding.byte_range.start,
            candidate.finding.byte_range.end,
            candidate.finding.entity_type.clone(),
        );
        if let Some(existing) = merged.get_mut(&id) {
            merge_metadata(&mut existing.finding, &candidate.finding);
            for source in candidate.sources {
                if !existing.sources.contains(&source) {
                    existing.sources.push(source);
                }
            }
        } else {
            merged.insert(id, candidate);
        }
    }
    let mut candidates: Vec<_> = merged.into_values().collect();
    for candidate in &mut candidates {
        candidate.sources.sort_by_key(|source| source.as_str());
        candidate.finding.source = if candidate.sources.contains(&FindingSource::User) {
            FindingSource::User
        } else if candidate.sources.contains(&FindingSource::Recall) {
            FindingSource::Recall
        } else {
            FindingSource::Scanner
        };
    }
    let findings: Vec<_> = candidates.iter().map(|c| c.finding.clone()).collect();
    let (adapted, _) = adapt_findings(input.text, &findings, &committed)?;
    let (selected, conflicts) = select(input.text, &adapted, state, &obligations)?;
    let selected_ids: BTreeSet<_> = selected
        .iter()
        .map(|finding| {
            (
                finding.entity_type.as_str(),
                finding.byte_range.start,
                finding.byte_range.end,
            )
        })
        .collect();
    for (candidate, finding) in candidates.iter_mut().zip(&adapted) {
        candidate.eligible = hides(state, finding);
        candidate.selected = conflicts.is_empty()
            && selected_ids.contains(&(
                finding.entity_type.as_str(),
                finding.byte_range.start,
                finding.byte_range.end,
            ));
    }
    // Deterministic source order even when a correction moves a candidate.
    candidates.sort_by(|a, b| {
        (
            a.finding.byte_range.start,
            a.finding.byte_range.end,
            &a.finding.entity_type,
            &a.key,
        )
            .cmp(&(
                b.finding.byte_range.start,
                b.finding.byte_range.end,
                &b.finding.entity_type,
                &b.key,
            ))
    });
    Ok(ReviewResult {
        head: state.head,
        finding_count,
        finding_bytes: supplied_bytes,
        candidates,
        conflicts,
    })
}

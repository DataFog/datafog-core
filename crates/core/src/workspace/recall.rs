use super::{
    ErrorCode, FindingSource, Head, MAX_BUNDLE_BYTES, MAX_FINDINGS, MAX_SOURCE_BYTES, Result,
    State, TextIndex, TextRange, WorkspaceFinding, committed_marker_ranges, error, identity_key,
    marker_lookup,
};
use regex::Regex;
use std::borrow::Borrow;
use std::collections::BTreeMap;
use zeroize::{Zeroize, Zeroizing};

// Provisional recall-specific ceilings, separate from snapshot compatibility.
const MAX_NORMALIZED_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_NORMALIZED_KEY_BYTES: usize = 16 * 1024 * 1024;
const MAX_INDEX_NODES: usize = 262_144;
const MAX_WORK: usize = 32_000_000;

/// One read-only match. `finding` is directly usable as a preparation input.
pub struct RecallFinding {
    pub finding: WorkspaceFinding,
    pub codepoint_range: TextRange,
    pub utf16_range: TextRange,
}

/// All eligible committed matches at this head, including overlaps.
/// An empty result does not establish that the input contains no PII.
pub struct RecallResult {
    pub head: Head,
    pub findings: Vec<RecallFinding>,
}

// BTreeMap may move its keys. Keep the private scalar in stable storage so Drop
// clears that storage, rather than only clearing a copy moved out of a map slot.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct IndexCharacter(Box<char>);
impl IndexCharacter {
    fn new(value: char) -> Self {
        let key = Self(Box::new(value));
        #[cfg(test)]
        cleanup_tests::allocated(key.0.as_ref());
        key
    }
}
impl Borrow<char> for IndexCharacter {
    fn borrow(&self) -> &char {
        self.0.as_ref()
    }
}
impl Drop for IndexCharacter {
    fn drop(&mut self) {
        self.0.as_mut().zeroize();
        #[cfg(test)]
        cleanup_tests::cleared(self.0.as_ref());
    }
}

#[derive(Default)]
struct Node {
    children: BTreeMap<IndexCharacter, usize>,
    identities: Vec<usize>,
}

struct Unit {
    value: char,
    range: TextRange,
    first: bool,
    last: bool,
}
impl Drop for Unit {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

struct Budget(usize);
impl Budget {
    fn charge(&mut self, amount: usize) -> Result<()> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
        Ok(())
    }
}

fn normalize(
    text: &str,
    offset: usize,
    work: &mut Budget,
    normalized_bytes: &mut usize,
) -> Result<Vec<Unit>> {
    let mut units = Vec::new();
    let mut pending_space: Option<TextRange> = None;
    let mut push = |unit: Unit| -> Result<()> {
        *normalized_bytes += unit.value.len_utf8();
        if *normalized_bytes > MAX_NORMALIZED_SOURCE_BYTES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        work.charge(unit.value.len_utf8())?;
        units
            .try_reserve(1)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        units.push(unit);
        Ok(())
    };
    let mut seen_value = false;
    for (start, value) in text.char_indices() {
        let range = TextRange {
            start: offset + start,
            end: offset + start + value.len_utf8(),
        };
        if value.is_whitespace() {
            if seen_value {
                pending_space = Some(TextRange {
                    start: pending_space.map_or(range.start, |space| space.start),
                    end: range.end,
                });
            }
            continue;
        }
        if let Some(space) = pending_space.take() {
            push(Unit {
                value: ' ',
                range: space,
                first: true,
                last: true,
            })?;
        }
        seen_value = true;
        let mut lower = value.to_lowercase().peekable();
        let mut first = true;
        while let Some(value) = lower.next() {
            push(Unit {
                value,
                range,
                first,
                last: lower.peek().is_none(),
            })?;
            first = false;
        }
    }
    Ok(units)
}

fn word_character(regex: &Regex, value: char) -> bool {
    let mut bytes = [0; 4];
    regex.is_match(value.encode_utf8(&mut bytes))
}

pub(super) fn find(state: &State, text: &str) -> Result<RecallResult> {
    if text.len() > MAX_SOURCE_BYTES {
        return Err(error(ErrorCode::ResourceLimit));
    }
    let markers = committed_marker_ranges(text, &marker_lookup(state))?;
    if state.entries.is_empty() || text.trim().is_empty() {
        return Ok(RecallResult {
            head: state.head,
            findings: Vec::new(),
        });
    }
    let word = Regex::new(r"\A[\p{Alphabetic}\p{M}\p{Nd}\p{Pc}\u{200C}\u{200D}]\z")
        .map_err(|_| error(ErrorCode::InvalidFinding))?;
    let identities: Vec<_> = state
        .entries
        .keys()
        .filter(|identity| state.hides(identity))
        .collect();
    let mut nodes = vec![Node::default()];
    let mut work = Budget(MAX_WORK);
    let mut key_bytes = 0usize;
    for (identity_index, identity) in identities.iter().enumerate() {
        key_bytes = key_bytes.saturating_add(identity.key.len());
        if key_bytes > MAX_NORMALIZED_KEY_BYTES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let mut node = 0;
        for value in identity.key.chars() {
            work.charge(value.len_utf8())?;
            node = if let Some(&child) = nodes[node].children.get(&value) {
                child
            } else {
                if nodes.len() >= MAX_INDEX_NODES {
                    return Err(error(ErrorCode::ResourceLimit));
                }
                nodes
                    .try_reserve(1)
                    .map_err(|_| error(ErrorCode::ResourceLimit))?;
                let child = nodes.len();
                nodes.push(Node::default());
                nodes[node]
                    .children
                    .insert(IndexCharacter::new(value), child);
                child
            };
        }
        nodes[node].identities.push(identity_index);
    }
    let mut findings = Vec::new();
    let mut result_bytes = 256usize;
    let mut normalized_bytes = 0;
    let mut offsets = TextIndex::new(text);
    let mut cursor = 0;
    for marker in markers.iter().map(Some).chain(std::iter::once(None)) {
        let end = marker.map_or(text.len(), |range| range.start);
        let units = normalize(&text[cursor..end], cursor, &mut work, &mut normalized_bytes)?;
        for start in 0..units.len() {
            if !units[start].first {
                continue;
            }
            let mut node = 0;
            for unit in &units[start..] {
                work.charge(1)?;
                let Some(&child) = nodes[node].children.get(&unit.value) else {
                    break;
                };
                node = child;
                if !unit.last {
                    continue;
                }
                for &identity_index in &nodes[node].identities {
                    work.charge(1)?;
                    let identity = identities[identity_index];
                    let range = TextRange {
                        start: units[start].range.start,
                        end: unit.range.end,
                    };
                    let starts_word = identity
                        .key
                        .chars()
                        .next()
                        .is_some_and(|c| word_character(&word, c));
                    let ends_word = identity
                        .key
                        .chars()
                        .next_back()
                        .is_some_and(|c| word_character(&word, c));
                    if (starts_word
                        && text[..range.start]
                            .chars()
                            .next_back()
                            .is_some_and(|c| word_character(&word, c)))
                        || (ends_word
                            && text[range.end..]
                                .chars()
                                .next()
                                .is_some_and(|c| word_character(&word, c)))
                    {
                        continue;
                    }
                    let matched = &text[range.start..range.end];
                    // Allow for worst-case JSON escaping as well as ranges and keys.
                    result_bytes = result_bytes
                        .saturating_add(matched.len() * 6 + identity.entity_type.len() + 512);
                    if findings.len() >= MAX_FINDINGS || result_bytes > MAX_BUNDLE_BYTES {
                        return Err(error(ErrorCode::ResourceLimit));
                    }
                    work.charge(matched.len())?;
                    let matched_key = Zeroizing::new(identity_key(matched));
                    if *matched_key != identity.key {
                        return Err(error(ErrorCode::InvalidFinding));
                    }
                    let mut finding = WorkspaceFinding::new(&identity.entity_type, matched, range);
                    finding.source = FindingSource::Recall;
                    findings
                        .try_reserve(1)
                        .map_err(|_| error(ErrorCode::ResourceLimit))?;
                    findings.push(RecallFinding {
                        finding,
                        codepoint_range: TextRange {
                            start: offsets.codepoint_offset(range.start),
                            end: offsets.codepoint_offset(range.end),
                        },
                        utf16_range: offsets
                            .utf16_range(range)
                            .map_err(|_| error(ErrorCode::InvalidFinding))?,
                    });
                }
            }
        }
        cursor = marker.map_or(end, |range| range.end);
    }
    findings.sort_by(|left, right| {
        left.finding
            .byte_range
            .start
            .cmp(&right.finding.byte_range.start)
            .then_with(|| {
                left.finding
                    .byte_range
                    .end
                    .cmp(&right.finding.byte_range.end)
            })
            .then_with(|| left.finding.entity_type.cmp(&right.finding.entity_type))
    });
    Ok(RecallResult {
        head: state.head,
        findings,
    })
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use crate::workspace::Workspace;
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    #[derive(Default)]
    struct Trace {
        allocated: BTreeSet<usize>,
        cleared: BTreeSet<usize>,
        uncleared: usize,
    }
    thread_local! {
        static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) };
    }
    // Observe while the allocation is still alive; never read freed map/key memory.
    pub(super) fn allocated(value: &char) {
        TRACE.with_borrow_mut(|trace| {
            if let Some(trace) = trace {
                assert!(trace.allocated.insert(std::ptr::from_ref(value) as usize));
            }
        });
    }
    pub(super) fn cleared(value: &char) {
        TRACE.with_borrow_mut(|trace| {
            if let Some(trace) = trace {
                trace.uncleared += usize::from(*value != '\0');
                assert!(trace.cleared.insert(std::ptr::from_ref(value) as usize));
            }
        });
    }
    fn observe<T>(query: impl FnOnce() -> T) -> T {
        TRACE.with_borrow_mut(|trace| *trace = Some(Trace::default()));
        let result = query();
        let trace = TRACE.with_borrow_mut(Option::take).unwrap();
        assert!(!trace.allocated.is_empty());
        assert_eq!(
            trace.allocated, trace.cleared,
            "all original character allocations are cleared before deallocation"
        );
        assert_eq!(trace.uncleared, 0);
        result
    }
    fn committed(value: &str) -> Workspace {
        let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
        let finding = WorkspaceFinding::new(
            "CUSTOM",
            value,
            TextRange {
                start: 0,
                end: value.len(),
            },
        );
        let mut change = workspace.prepare(value, &[finding], [3; 16]).unwrap();
        let parent = change.expected_head().unwrap();
        let head = change.candidate_head().unwrap();
        workspace.activate_saved(&mut change, parent, head).unwrap();
        workspace
    }
    #[test]
    fn ordered_borrowed_keys_clear_their_stable_allocations_after_map_rebalancing() {
        observe(|| {
            let mut children = BTreeMap::new();
            for offset in 0..80 {
                let character = char::from_u32(0x400 + offset).unwrap();
                children.insert(IndexCharacter::new(character), offset);
            }
            assert_eq!(children.get(&'\u{405}'), Some(&5));
            assert_eq!(
                children
                    .keys()
                    .map(|key| *key.borrow())
                    .collect::<Vec<char>>(),
                (0..80)
                    .map(|offset| char::from_u32(0x400 + offset).unwrap())
                    .collect::<Vec<_>>()
            );
        });
    }
    #[test]
    fn success_and_result_index_work_limit_exits_clear_every_index_character() {
        let workspace = committed("İ Jane");
        assert_eq!(
            observe(|| workspace.recall("👋 i\u{307} JANE"))
                .unwrap()
                .findings
                .len(),
            1
        );
        let workspace = committed("Ann");
        assert_eq!(
            observe(|| workspace.recall(&"Ann ".repeat(10_001)))
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
        let workspace = committed(&"z".repeat(MAX_INDEX_NODES));
        assert_eq!(
            observe(|| workspace.recall("plain")).err().unwrap().code,
            ErrorCode::ResourceLimit
        );
        let workspace = committed(&format!("{}!", "*".repeat(4096)));
        assert_eq!(
            observe(|| workspace.recall(&"*".repeat(20_000)))
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
    }
}

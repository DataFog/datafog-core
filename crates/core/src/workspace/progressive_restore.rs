//! Explicit irreversible delivery; provider framing and delivery retries belong to the host.
use super::*;

/// Counts only text returned successfully to the caller, never held or failed-call text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RestoreProgress {
    pub bytes: usize,
    pub codepoints: usize,
    pub utf16: usize,
    pub restored_markers: usize,
}

/// A successful feed is still incomplete. Only successful finalization sets `complete`.
pub struct RestoreChunk {
    pub text: String,
    pub delivered: RestoreProgress,
    pub complete: bool,
}
impl Drop for RestoreChunk {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

/// Terminal, content-free failure. Earlier returned chunks remain delivered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamFailure {
    pub error: Box<WorkspaceError>,
    pub delivered: RestoreProgress,
    pub complete: bool,
}
impl std::fmt::Display for StreamFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for StreamFailure {}
type StreamResult<T> = std::result::Result<T, StreamFailure>;

#[derive(Clone, Copy, Default)]
struct Position {
    bytes: usize,
    codepoints: usize,
    utf16: usize,
}
impl Position {
    fn advance(self, value: char) -> Self {
        Self {
            bytes: self.bytes + value.len_utf8(),
            codepoints: self.codepoints + 1,
            utf16: self.utf16 + value.len_utf16(),
        }
    }
    fn issue(self, end: Self, kind: MarkerIssueKind) -> MarkerIssue {
        MarkerIssue {
            kind,
            byte_range: TextRange {
                start: self.bytes,
                end: end.bytes,
            },
            codepoint_range: TextRange {
                start: self.codepoints,
                end: end.codepoints,
            },
            utf16_range: TextRange {
                start: self.utf16,
                end: end.utf16,
            },
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Word,
    LongWord,
    Bracket,
    Interior,
    Closing,
}

/// One response pinned to an immutable committed workspace head. Each call stages
/// output atomically; failure releases none of that call, but cannot retract prior calls.
pub struct ProgressiveRestore {
    state: Option<Arc<State>>,
    lifetime: Arc<AtomicBool>,
    epoch: Arc<AtomicBool>,
    encoding: RestoreEncoding,
    context: Option<usize>,
    // Canonical public markers sorted for logarithmic lookup; private copies wipe on drop.
    lookup: Vec<(String, Zeroizing<String>)>,
    carry: Zeroizing<Vec<u8>>,
    mode: Mode,
    start: Position,
    position: Position,
    utf8: [u8; 4],
    utf8_length: usize,
    utf8_expected: usize,
    high_surrogate: Option<u16>,
    input_bytes: usize,
    delivered: RestoreProgress,
    active: bool,
}

fn word(value: char) -> bool {
    value.is_ascii_alphanumeric() || value == '_'
}
impl ProgressiveRestore {
    pub(super) fn new(
        state: Arc<State>,
        lifetime: Arc<AtomicBool>,
        epoch: Arc<AtomicBool>,
        encoding: RestoreEncoding,
        context: Option<usize>,
    ) -> Result<Self> {
        let mut lookup = Vec::new();
        lookup
            .try_reserve_exact(state.entries.len())
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        for (identity, entry) in &state.entries {
            let mut display = Zeroizing::new(String::new());
            display
                .try_reserve_exact(entry.display.len())
                .map_err(|_| error(ErrorCode::ResourceLimit))?;
            display.push_str(&entry.display);
            lookup.push((marker(&identity.entity_type, &entry.code), display));
        }
        lookup.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        Ok(Self {
            state: Some(state),
            lifetime,
            epoch,
            encoding,
            context,
            lookup,
            carry: Zeroizing::new(Vec::new()),
            mode: Mode::Text,
            start: Position::default(),
            position: Position::default(),
            utf8: [0; 4],
            utf8_length: 0,
            utf8_expected: 0,
            high_surrogate: None,
            input_bytes: 0,
            delivered: RestoreProgress::default(),
            active: true,
        })
    }
    fn failure(&mut self, failure: WorkspaceError) -> StreamFailure {
        self.clear();
        self.active = false;
        StreamFailure {
            error: Box::new(failure),
            delivered: self.delivered,
            complete: false,
        }
    }
    fn valid_head(&mut self) -> StreamResult<()> {
        let code = if !self.lifetime.load(Ordering::Acquire) {
            Some(ErrorCode::ClosedWorkspace)
        } else if !self.epoch.load(Ordering::Acquire) {
            Some(ErrorCode::WorkspaceConflict)
        } else if self.state.is_none() {
            Some(ErrorCode::InactiveStream)
        } else {
            None
        };
        if let Some(code) = code {
            self.close();
            return Err(self.failure(error(code)));
        }
        Ok(())
    }
    fn active(&mut self) -> StreamResult<()> {
        self.valid_head()?;
        if !self.active {
            return Err(self.failure(error(ErrorCode::InactiveStream)));
        }
        Ok(())
    }
    fn clear(&mut self) {
        self.carry.zeroize();
        self.utf8.zeroize();
        self.utf8_length = 0;
        self.utf8_expected = 0;
        self.high_surrogate.zeroize();
        self.mode = Mode::Text;
    }
    fn known(&self, interior: &str) -> Option<usize> {
        let canonical = canonical_marker(interior)?;
        self.lookup
            .binary_search_by(|entry| entry.0.cmp(&canonical))
            .ok()
    }
    fn carry_text(&self) -> &str {
        // Only ASCII grammar bytes enter carry.
        // Text mode adds only ASCII word/bracket bytes; decoded non-ASCII is emitted directly.
        unsafe { std::str::from_utf8_unchecked(&self.carry) }
    }
    fn issue(&self, start: Position, kind: MarkerIssueKind) -> WorkspaceError {
        let mut failure = error(ErrorCode::InvalidMarker);
        failure.marker_issues.push(start.issue(self.position, kind));
        failure
    }
    fn emit(&self, stage: &mut Zeroizing<Vec<u8>>, bytes: &[u8]) -> Result<()> {
        private_bytes::append(
            stage,
            bytes,
            MAX_BUNDLE_BYTES.saturating_sub(self.delivered.bytes),
        )
    }
    fn flush(&mut self, stage: &mut Zeroizing<Vec<u8>>) -> Result<()> {
        self.emit(stage, &self.carry)?;
        self.carry.zeroize();
        self.mode = Mode::Text;
        Ok(())
    }
    fn bare(&self, interior: &str, start: Position, end: Position) -> Result<()> {
        if self.known(interior).is_some() {
            let mut failure = error(ErrorCode::InvalidMarker);
            failure
                .marker_issues
                .push(start.issue(end, MarkerIssueKind::Unbracketed));
            return Err(failure);
        }
        Ok(())
    }
    fn character(
        &mut self,
        value: char,
        stage: &mut Zeroizing<Vec<u8>>,
        markers: &mut usize,
    ) -> Result<()> {
        let before = self.position;
        self.position = self.position.advance(value);
        if self.position.bytes > MAX_BUNDLE_BYTES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        self.character_at(value, before, stage, markers)
    }
    fn character_at(
        &mut self,
        value: char,
        before: Position,
        stage: &mut Zeroizing<Vec<u8>>,
        markers: &mut usize,
    ) -> Result<()> {
        let mut encoded = [0; 4];
        let bytes = value.encode_utf8(&mut encoded).as_bytes();
        match self.mode {
            Mode::Text => {
                if value == '[' || word(value) {
                    self.start = before;
                    self.mode = if value == '[' {
                        Mode::Bracket
                    } else {
                        Mode::Word
                    };
                    private_bytes::append(&mut self.carry, bytes, MAX_MARKER_INTERIOR + 4)?;
                } else {
                    self.emit(stage, bytes)?;
                }
            }
            Mode::Word => {
                if word(value) {
                    if self.carry.len() == MAX_MARKER_INTERIOR {
                        self.flush(stage)?;
                        self.emit(stage, bytes)?;
                        self.mode = Mode::LongWord;
                    } else {
                        private_bytes::append(&mut self.carry, bytes, MAX_MARKER_INTERIOR + 4)?;
                    }
                } else {
                    self.bare(self.carry_text(), self.start, before)?;
                    self.flush(stage)?;
                    self.character_at(value, before, stage, markers)?;
                }
            }
            Mode::LongWord => {
                if word(value) {
                    self.emit(stage, bytes)?;
                } else {
                    self.mode = Mode::Text;
                    self.character_at(value, before, stage, markers)?;
                }
            }
            Mode::Bracket => {
                if self.carry.len() == 1 && value == '[' {
                    private_bytes::append(&mut self.carry, b"[", MAX_MARKER_INTERIOR + 4)?;
                } else if self.carry.len() == 2 && value.is_ascii_alphabetic() {
                    private_bytes::append(&mut self.carry, bytes, MAX_MARKER_INTERIOR + 4)?;
                    self.mode = Mode::Interior;
                } else if self.carry.len() == 2 && value == '[' {
                    self.emit(stage, b"[")?;
                    self.start = Position {
                        bytes: self.start.bytes + 1,
                        codepoints: self.start.codepoints + 1,
                        utf16: self.start.utf16 + 1,
                    };
                } else {
                    self.flush(stage)?;
                    self.character_at(value, before, stage, markers)?;
                }
            }
            Mode::Interior => {
                if word(value) {
                    if self.carry.len() - 2 == MAX_MARKER_INTERIOR {
                        return Err(self.issue(self.start, MarkerIssueKind::Damaged));
                    }
                    private_bytes::append(&mut self.carry, bytes, MAX_MARKER_INTERIOR + 4)?;
                } else if value == ']' && self.carry.contains(&b'_') {
                    private_bytes::append(&mut self.carry, b"]", MAX_MARKER_INTERIOR + 4)?;
                    self.mode = Mode::Closing;
                } else {
                    let inner = Position {
                        bytes: self.start.bytes + 2,
                        codepoints: self.start.codepoints + 2,
                        utf16: self.start.utf16 + 2,
                    };
                    self.bare(&self.carry_text()[2..], inner, before)?;
                    self.flush(stage)?;
                    self.character_at(value, before, stage, markers)?;
                }
            }
            Mode::Closing => {
                if value == ']' {
                    let interior = &self.carry_text()[2..self.carry.len() - 1];
                    let Some(index) = self.known(interior) else {
                        return Err(self.issue(
                            self.start,
                            if canonical_marker(interior).is_some() {
                                MarkerIssueKind::Unknown
                            } else {
                                MarkerIssueKind::Damaged
                            },
                        ));
                    };
                    if let Some(context) = self.context {
                        let state = self
                            .state
                            .as_deref()
                            .ok_or_else(|| error(ErrorCode::InactiveStream))?;
                        if !conversation::eligible(state, context, &self.lookup[index].0) {
                            return Err(
                                self.issue(self.start, MarkerIssueKind::NotIssuedInConversation)
                            );
                        }
                    }
                    self.emit(stage, self.lookup[index].1.as_bytes())?;
                    *markers += 1;
                    self.carry.zeroize();
                    self.mode = Mode::Text;
                } else {
                    let inner = Position {
                        bytes: self.start.bytes + 2,
                        codepoints: self.start.codepoints + 2,
                        utf16: self.start.utf16 + 2,
                    };
                    let end = Position {
                        bytes: before.bytes - 1,
                        codepoints: before.codepoints - 1,
                        utf16: before.utf16 - 1,
                    };
                    self.bare(&self.carry_text()[2..self.carry.len() - 1], inner, end)?;
                    self.flush(stage)?;
                    self.character_at(value, before, stage, markers)?;
                }
            }
        }
        Ok(())
    }
    fn commit(
        &mut self,
        mut stage: Zeroizing<Vec<u8>>,
        markers: usize,
        complete: bool,
    ) -> StreamResult<RestoreChunk> {
        self.valid_head()?;
        let text = match String::from_utf8(std::mem::take(&mut *stage)) {
            Ok(text) => text,
            Err(failure) => {
                let mut bytes = failure.into_bytes();
                bytes.zeroize();
                return Err(self.failure(error(ErrorCode::InvalidEncoding)));
            }
        };
        self.delivered.bytes += text.len();
        self.delivered.codepoints += text.chars().count();
        self.delivered.utf16 += text.encode_utf16().count();
        self.delivered.restored_markers += markers;
        Ok(RestoreChunk {
            text,
            delivered: self.delivered,
            complete,
        })
    }
    /// Input encoding may split scalars between calls; no malformed Unicode is replaced.
    pub fn feed(&mut self, bytes: &[u8]) -> StreamResult<RestoreChunk> {
        self.active()?;
        if self.encoding != RestoreEncoding::Utf8 {
            return Err(self.failure(error(ErrorCode::InvalidEncoding)));
        }
        self.input_bytes = self.input_bytes.saturating_add(bytes.len());
        if self.input_bytes > MAX_BUNDLE_BYTES {
            return Err(self.failure(error(ErrorCode::ResourceLimit)));
        }
        let mut stage = Zeroizing::new(Vec::new());
        let mut markers = 0;
        let result = (|| {
            for &byte in bytes {
                if self.utf8_length == 0 {
                    self.utf8_expected = match byte {
                        0..=0x7f => 1,
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => return Err(error(ErrorCode::InvalidEncoding)),
                    };
                }
                self.utf8[self.utf8_length] = byte;
                self.utf8_length += 1;
                if self.utf8_length == self.utf8_expected {
                    let value = std::str::from_utf8(&self.utf8[..self.utf8_length])
                        .map_err(|_| error(ErrorCode::InvalidEncoding))?
                        .chars()
                        .next()
                        .ok_or_else(|| error(ErrorCode::InvalidEncoding))?;
                    self.utf8.zeroize();
                    self.utf8_length = 0;
                    self.utf8_expected = 0;
                    self.character(value, &mut stage, &mut markers)?;
                }
            }
            Ok(())
        })();
        if let Err(failure) = result {
            return Err(self.failure(failure));
        }
        self.commit(stage, markers, false)
    }
    pub fn feed_utf16(&mut self, units: &[u16]) -> StreamResult<RestoreChunk> {
        self.active()?;
        if self.encoding != RestoreEncoding::Utf16 {
            return Err(self.failure(error(ErrorCode::InvalidEncoding)));
        }
        let mut stage = Zeroizing::new(Vec::new());
        let mut markers = 0;
        let result = (|| {
            for &unit in units {
                let scalar = if let Some(high) = self.high_surrogate.take() {
                    if !(0xdc00..=0xdfff).contains(&unit) {
                        return Err(error(ErrorCode::InvalidEncoding));
                    }
                    0x10000 + (((high as u32) - 0xd800) << 10) + ((unit as u32) - 0xdc00)
                } else if (0xd800..=0xdbff).contains(&unit) {
                    self.high_surrogate = Some(unit);
                    continue;
                } else if (0xdc00..=0xdfff).contains(&unit) {
                    return Err(error(ErrorCode::InvalidEncoding));
                } else {
                    unit as u32
                };
                self.character(
                    char::from_u32(scalar).ok_or_else(|| error(ErrorCode::InvalidEncoding))?,
                    &mut stage,
                    &mut markers,
                )?;
            }
            Ok(())
        })();
        if let Err(failure) = result {
            return Err(self.failure(failure));
        }
        self.commit(stage, markers, false)
    }
    /// EOF is terminal for a possible bracketed marker, including a lone opening bracket.
    pub fn finalize(&mut self) -> StreamResult<RestoreChunk> {
        self.active()?;
        if self.utf8_length != 0 || self.high_surrogate.is_some() {
            return Err(self.failure(error(ErrorCode::InvalidEncoding)));
        }
        let mut stage = Zeroizing::new(Vec::new());
        let result = match self.mode {
            Mode::Bracket => Err(self.issue(self.start, MarkerIssueKind::Damaged)),
            Mode::Interior | Mode::Closing => {
                let inner = Position {
                    bytes: self.start.bytes + 2,
                    codepoints: self.start.codepoints + 2,
                    utf16: self.start.utf16 + 2,
                };
                let trim = usize::from(self.mode == Mode::Closing);
                let end = Position {
                    bytes: self.position.bytes - trim,
                    codepoints: self.position.codepoints - trim,
                    utf16: self.position.utf16 - trim,
                };
                self.bare(&self.carry_text()[2..self.carry.len() - trim], inner, end)
                    .and_then(|()| Err(self.issue(self.start, MarkerIssueKind::Damaged)))
            }
            Mode::Word => self
                .bare(self.carry_text(), self.start, self.position)
                .and_then(|()| self.flush(&mut stage)),
            Mode::Text | Mode::LongWord => Ok(()),
        };
        if let Err(failure) = result {
            return Err(self.failure(failure));
        }
        let result = self.commit(stage, 0, true);
        self.clear();
        self.active = false;
        result
    }
    /// A new response; prior chunks remain delivered to their original response.
    pub fn reset(&mut self) -> StreamResult<()> {
        self.valid_head()?;
        self.clear();
        self.position = Position::default();
        self.input_bytes = 0;
        self.delivered = RestoreProgress::default();
        self.active = true;
        Ok(())
    }
    /// Permanently discard carry and private lookup copies. Idempotent.
    pub fn close(&mut self) {
        self.clear();
        self.lookup.clear();
        self.state = None;
        self.active = false;
    }
}
impl Drop for ProgressiveRestore {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn megabyte_input_is_released_without_retaining_response_or_unbounded_carry() {
        let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
        let mut response = workspace
            .progressive_restore(RestoreEncoding::Utf8)
            .unwrap();
        let source = vec![b'x'; 1024 * 1024];
        let start = std::time::Instant::now();
        for _ in 0..8 {
            let chunk = response.feed(&source).unwrap();
            assert_eq!(chunk.text.len(), source.len());
            assert!(response.carry.len() <= MAX_MARKER_INTERIOR + 4);
            assert!(response.carry.capacity() <= MAX_MARKER_INTERIOR + 4);
        }
        assert!(response.finalize().unwrap().complete);
        println!(
            "8 MiB ASCII progressive feed/finalize: {:?}; retained carry capacity {} bytes",
            start.elapsed(),
            response.carry.capacity()
        );
    }
}

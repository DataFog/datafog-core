//! Strict R2-compatible buffering. Progressive delivery requires a separate reviewed contract.
use super::*;

/// The encoding is fixed for one buffered response; mixed calls reject explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RestoreEncoding {
    #[default]
    Utf8,
    Utf16,
}

/// Owned atomic response adapter pinned to one committed workspace/head.
/// Feed methods never return text. `finalize` returns the same all-or-nothing
/// report as `Workspace::restore_report` for the complete decoded input.
/// This does not provide progressive delivery or provider/SSE parsing.
pub struct BufferedRestore {
    state: Option<Arc<State>>,
    lifetime: Arc<AtomicBool>,
    epoch: Arc<AtomicBool>,
    encoding: RestoreEncoding,
    buffer: Zeroizing<Vec<u8>>,
    high_surrogate: Option<u16>,
    active: bool,
}
impl BufferedRestore {
    pub(super) fn new(
        state: Arc<State>,
        lifetime: Arc<AtomicBool>,
        epoch: Arc<AtomicBool>,
        encoding: RestoreEncoding,
    ) -> Self {
        Self {
            state: Some(state),
            lifetime,
            epoch,
            encoding,
            buffer: Zeroizing::new(Vec::new()),
            high_surrogate: None,
            active: true,
        }
    }
    fn valid_head(&mut self) -> Result<()> {
        if !self.lifetime.load(Ordering::Acquire) {
            self.close();
            return Err(error(ErrorCode::ClosedWorkspace));
        }
        if !self.epoch.load(Ordering::Acquire) {
            self.close();
            return Err(error(ErrorCode::WorkspaceConflict));
        }
        if self.state.is_none() {
            return Err(error(ErrorCode::InactiveStream));
        }
        Ok(())
    }
    fn active(&mut self) -> Result<()> {
        self.valid_head()?;
        if !self.active {
            return Err(error(ErrorCode::InactiveStream));
        }
        Ok(())
    }
    fn fail(&mut self, code: ErrorCode) -> WorkspaceError {
        self.buffer.zeroize();
        self.high_surrogate.zeroize();
        self.active = false;
        error(code)
    }
    fn append_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.buffer.len().saturating_add(bytes.len()) > MAX_BUNDLE_BYTES {
            return Err(self.fail(ErrorCode::ResourceLimit));
        }
        if self.buffer.try_reserve(bytes.len()).is_err() {
            return Err(self.fail(ErrorCode::ResourceLimit));
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }
    /// Accept arbitrary UTF-8 byte boundaries. Invalid/incomplete UTF-8 rejects at finalize.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<()> {
        self.active()?;
        if self.encoding != RestoreEncoding::Utf8 {
            return Err(self.fail(ErrorCode::InvalidEncoding));
        }
        self.append_bytes(bytes)
    }
    /// Accept arbitrary UTF-16 code-unit boundaries, retaining a split high surrogate.
    /// Unpaired surrogates reject without Unicode replacement or partial output.
    pub fn feed_utf16(&mut self, units: &[u16]) -> Result<()> {
        self.active()?;
        if self.encoding != RestoreEncoding::Utf16 {
            return Err(self.fail(ErrorCode::InvalidEncoding));
        }
        for &unit in units {
            let scalar = if let Some(high) = self.high_surrogate.take() {
                if !(0xDC00..=0xDFFF).contains(&unit) {
                    return Err(self.fail(ErrorCode::InvalidEncoding));
                }
                0x10000 + (((high as u32) - 0xD800) << 10) + ((unit as u32) - 0xDC00)
            } else if (0xD800..=0xDBFF).contains(&unit) {
                self.high_surrogate = Some(unit);
                continue;
            } else if (0xDC00..=0xDFFF).contains(&unit) {
                return Err(self.fail(ErrorCode::InvalidEncoding));
            } else {
                unit as u32
            };
            let Some(character) = char::from_u32(scalar) else {
                return Err(self.fail(ErrorCode::InvalidEncoding));
            };
            let mut bytes = [0; 4];
            self.append_bytes(character.encode_utf8(&mut bytes).as_bytes())?;
        }
        Ok(())
    }
    /// Finalize once. Issues return `text: None`; no transformed prefix is ever released.
    /// EOF marker syntax follows R2 exactly, including its ordinary-literal handling.
    pub fn finalize(&mut self) -> Result<RestoreReport> {
        self.active()?;
        if self.high_surrogate.is_some() {
            return Err(self.fail(ErrorCode::InvalidEncoding));
        }
        let result = (|| {
            let text =
                std::str::from_utf8(&self.buffer).map_err(|_| error(ErrorCode::InvalidEncoding))?;
            restore_report(
                self.state
                    .as_deref()
                    .ok_or_else(|| error(ErrorCode::InactiveStream))?,
                text,
            )
        })();
        self.buffer.zeroize();
        self.high_surrogate.zeroize();
        self.active = false;
        // A concurrent close/head change must not return newly restored text.
        self.valid_head()?;
        result
    }
    /// Start a fresh response at the same still-current pinned head. Old output is not retracted.
    pub fn reset(&mut self) -> Result<()> {
        self.valid_head()?;
        self.buffer.zeroize();
        self.high_surrogate.zeroize();
        self.active = true;
        Ok(())
    }
    /// Permanently discard this adapter and its buffered plaintext. Idempotent.
    pub fn close(&mut self) {
        self.buffer.zeroize();
        self.high_surrogate.zeroize();
        self.state = None;
        self.active = false;
    }
}
impl Drop for BufferedRestore {
    fn drop(&mut self) {
        self.close();
    }
}

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
        match private_bytes::append(&mut self.buffer, bytes, MAX_BUNDLE_BYTES) {
            Ok(()) => Ok(()),
            Err(failure) => Err(self.fail(failure.code)),
        }
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

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    #[derive(Clone, Copy)]
    struct Watch {
        pointer: usize,
        length: usize,
        observed: bool,
        wiped: bool,
        reallocated: bool,
        fail_next: bool,
        injected: bool,
    }
    impl Watch {
        const EMPTY: Self = Self {
            pointer: 0,
            length: 0,
            observed: false,
            wiped: false,
            reallocated: false,
            fail_next: false,
            injected: false,
        };
    }
    thread_local! { static WATCH:Cell<Watch>=const {Cell::new(Watch::EMPTY)}; }
    struct WatchingAllocator;
    fn should_fail() -> bool {
        WATCH
            .try_with(|cell| {
                let mut watch = cell.get();
                if watch.fail_next {
                    watch.fail_next = false;
                    watch.injected = true;
                    cell.set(watch);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }
    fn inspect(pointer: *mut u8, layout: Layout, reallocated: bool) {
        // Allocator hooks must also work after a test thread's TLS is torn down.
        WATCH
            .try_with(|cell| {
                let mut watch = cell.get();
                if watch.pointer != 0 && watch.pointer == pointer as usize {
                    watch.pointer = 0;
                    watch.observed = true;
                    watch.reallocated = reallocated;
                    // This hook runs before System frees/reallocates this live backing.
                    let bytes = unsafe {
                        std::slice::from_raw_parts(pointer, watch.length.min(layout.size()))
                    };
                    watch.wiped = bytes.iter().all(|byte| *byte == 0);
                    cell.set(watch);
                }
            })
            .unwrap_or_default();
    }
    unsafe impl GlobalAlloc for WatchingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if should_fail() {
                std::ptr::null_mut()
            } else {
                unsafe { System.alloc(layout) }
            }
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            inspect(pointer, layout, false);
            unsafe { System.dealloc(pointer, layout) }
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            if should_fail() {
                return std::ptr::null_mut();
            }
            inspect(pointer, layout, true);
            unsafe { System.realloc(pointer, layout, size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: WatchingAllocator = WatchingAllocator;

    fn arm(response: &BufferedRestore, fail_next: bool) {
        WATCH.with(|cell| {
            cell.set(Watch {
                pointer: response.buffer.as_ptr() as usize,
                length: response.buffer.len(),
                fail_next,
                ..Watch::EMPTY
            })
        });
    }
    #[test]
    fn actual_capacity_growth_wipes_old_plaintext_before_deallocation() {
        let workspace = Workspace::create([1; 16], [2; 16]).unwrap();
        let mut response = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
        response.feed(b"synthetic private chunk").unwrap();
        let previous = response.buffer.as_ptr();
        let capacity = response.buffer.capacity();
        let next = vec![b'x'; capacity + 1];
        arm(&response, false);
        response.feed(&next).unwrap();
        let observed = WATCH.with(Cell::get);
        assert!(response.buffer.capacity() > capacity);
        assert_ne!(response.buffer.as_ptr(), previous);
        assert!(
            observed.observed && observed.wiped && !observed.reallocated,
            "live plaintext must never be handed to allocator realloc or freed unwiped"
        );
    }
    #[test]
    fn failed_fresh_allocation_discards_the_entire_response_and_clears_its_backing() {
        let workspace = Workspace::create([3; 16], [4; 16]).unwrap();
        let mut response = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
        response.feed(b"synthetic private chunk").unwrap();
        let next = vec![b'x'; response.buffer.capacity() + 1];
        arm(&response, true);
        let failed = response.feed(&next);
        let injected = WATCH.with(|cell| {
            let mut watch = cell.get();
            let injected = watch.injected;
            watch.fail_next = false;
            cell.set(watch);
            injected
        });
        assert!(injected);
        assert_eq!(failed.err().unwrap().code, ErrorCode::ResourceLimit);
        assert_eq!(
            response.finalize().err().unwrap().code,
            ErrorCode::InactiveStream
        );
        drop(response);
        let observed = WATCH.with(Cell::get);
        assert!(observed.observed && observed.wiped);
        let mut fresh = workspace.buffered_restore(RestoreEncoding::Utf8).unwrap();
        fresh.feed(b"fresh response").unwrap();
        assert_eq!(
            fresh.finalize().unwrap().text.as_deref(),
            Some("fresh response")
        );
    }
}

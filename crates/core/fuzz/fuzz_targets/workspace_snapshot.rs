#![no_main]

//! Fuzz the untrusted-snapshot CBOR decoder through its production entry point.
//!
//! Every accepted byte string is held to the codec's own contract: the decoder
//! only succeeds on already-canonical input, so an accepted snapshot must
//! re-encode byte-identically, decode again, and open as a real workspace that
//! exports the same bytes. Rejections are unconstrained; the decoder
//! deliberately rejects far more than it accepts.

use datafog_core::workspace::Workspace;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(canonical) = Workspace::decode_snapshot(data) else {
        return;
    };

    // Accepted input is canonical by definition: the decoder compares its own
    // re-encoding against the input before returning.
    assert_eq!(
        canonical.as_slice(),
        data,
        "accepted input must be canonical"
    );

    let reencoded = Workspace::decode_snapshot(&canonical).expect("canonical bytes must re-decode");
    assert_eq!(reencoded, canonical, "re-encoding must be a fixed point");

    let workspace = Workspace::open_bundle(&canonical).expect("canonical bytes must open");
    assert_eq!(
        workspace
            .export_bundle()
            .expect("opened workspace must export"),
        canonical,
        "export after open must preserve the canonical bytes"
    );
});

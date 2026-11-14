#![no_main]

//! Fuzz the UTF-8 byte range to UTF-16 offset conversion used by JavaScript
//! consumers.
//!
//! Text comes from arbitrary bytes mapped through `String::from_utf8_lossy`, so
//! the corpus freely produces multi-byte and astral characters. Range endpoints
//! are derived from the text length, which yields in-bounds ranges, ranges that
//! overshoot the end, and endpoints that split a multi-byte character. The
//! conversion must accept exactly the ranges with ordered, in-bounds, on-boundary
//! endpoints, and every acceptance must be a faithful prefix count.

use datafog_core::{TextRange, utf16_range};
use libfuzzer_sys::fuzz_target;

/// Map a selector to an offset that is likely to be interesting for `len`:
/// start, end, just past the end, well past the end, and interior byte positions
/// that usually split a multi-byte character.
fn endpoint(selector: u8, len: usize) -> usize {
    let selector = usize::from(selector);
    match selector % 8 {
        0 => 0,
        1 => len,
        2 => len.saturating_add(1),
        3 => len.saturating_add(4),
        4 => len / 2,
        5 => selector % len.max(1),
        6 => len.saturating_sub(1),
        _ => (selector * 7) % (len + 4).max(1),
    }
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn is_invalid(text: &str, range: TextRange) -> bool {
    range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fuzz_target!(|data: &[u8]| {
    let mode = data.first().copied().unwrap_or(0);
    let start_selector = data.get(1).copied().unwrap_or(0);
    let end_selector = data.get(2).copied().unwrap_or(0);
    let text = String::from_utf8_lossy(data.get(3..).unwrap_or_default());

    let text = text.as_ref();
    let len = text.len();
    let mut range = TextRange {
        start: endpoint(start_selector, len),
        end: endpoint(end_selector, len),
    };
    if mode % 2 == 1 {
        std::mem::swap(&mut range.start, &mut range.end);
    }

    match utf16_range(text, range) {
        Ok(mapped) => {
            assert!(range.start <= range.end, "accepted a reversed range");
            assert!(range.end <= len, "accepted an out-of-bounds range");
            assert!(
                text.is_char_boundary(range.start) && text.is_char_boundary(range.end),
                "accepted a range that splits a character"
            );
            assert!(mapped.start <= mapped.end, "mapped range is reversed");
            assert_eq!(mapped.start, utf16_len(&text[..range.start]));
            assert_eq!(mapped.end, utf16_len(&text[..range.end]));
            assert_eq!(
                utf16_len(&text[range.start..range.end]),
                mapped.end - mapped.start,
                "mapped width must match the covered text"
            );
        }
        Err(_) => assert!(
            is_invalid(text, range),
            "rejected valid range {range:?} over {len} bytes"
        ),
    }

    // A range snapped inward to character boundaries is always valid, so every
    // input exercises the success path too.
    let low = floor_char_boundary(text, range.start.min(range.end));
    let high = floor_char_boundary(text, range.start.max(range.end));
    let snapped = TextRange {
        start: low,
        end: high,
    };
    let mapped = utf16_range(text, snapped).expect("snapped range must be accepted");
    assert_eq!(mapped.start, utf16_len(&text[..low]));
    assert_eq!(mapped.end, utf16_len(&text[..high]));
    assert_eq!(mapped.end - mapped.start, utf16_len(&text[low..high]));
    assert_eq!(
        utf16_range(text, TextRange { start: 0, end: 0 }),
        Ok(TextRange { start: 0, end: 0 })
    );
});

#![no_main]

//! Check planted address coverage through mixed-script context and validate
//! every returned range over arbitrary UTF-8 text on either side.
use datafog_core::{scan, utf16_range};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let selectors = [
        data.first().copied().unwrap_or_default() as usize,
        data.get(1).copied().unwrap_or_default() as usize,
        data.get(2).copied().unwrap_or_default() as usize,
    ];
    let contexts = ["メールは", "用户", "почта", "연락처", "البريد", "😀", " "];
    let emails = [
        "alice@example.com",
        "josé@example.com",
        "jörg@müller.de",
        "e\u{301}@example.com",
        "用户@例子.广告",
    ];
    let arbitrary = String::from_utf8_lossy(data.get(3..).unwrap_or_default());
    let context = contexts[selectors[0] % contexts.len()];
    let first = emails[selectors[1] % emails.len()];
    let second = emails[selectors[2] % emails.len()];
    let mut text = format!("{arbitrary}\n{context}");
    let first_start = text.len();
    text.push_str(first);
    let first_end = text.len();
    text.push_str(context);
    let second_start = text.len();
    text.push_str(second);
    let second_end = text.len();
    text.push_str(context);
    text.push('\n');
    text.push_str(&arbitrary);
    let findings = scan(&text);
    for finding in &findings {
        let range = finding.byte_range;
        assert_eq!(&text[range.start..range.end], finding.matched_text);
        let mapped = utf16_range(&text, range).expect("finding range must be valid");
        let units: Vec<_> = text.encode_utf16().collect();
        assert_eq!(
            String::from_utf16(&units[mapped.start..mapped.end]).unwrap(),
            finding.matched_text
        );
    }
    for (start, end) in [(first_start, first_end), (second_start, second_end)] {
        assert!(
            findings.iter().any(|finding| {
                finding.entity_type == "EMAIL"
                    && finding.byte_range.start <= start
                    && finding.byte_range.end >= end
            }),
            "planted email was left partly uncovered: {text:?}"
        );
    }
});

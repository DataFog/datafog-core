//! Regression cases from the 1.4.0 independent review (synthetic values only).
use datafog_core::{
    Finding, PrivacyContext, TextRange, TokenizeConfig, TransformationConfig,
    TransformationStrategy, parse_scan_and_transform_config, parse_transformation_config,
    required_tokenization_items, scan, scan_and_transform, transform,
};
use serde_json::json;

fn values(text: &str, label: &str) -> Vec<String> {
    scan(text)
        .into_iter()
        .filter(|f| f.entity_type == label)
        .map(|f| {
            assert_eq!(&text[f.byte_range.start..f.byte_range.end], f.matched_text);
            assert_eq!(
                text.chars()
                    .skip(f.codepoint_range.start)
                    .take(f.codepoint_range.end - f.codepoint_range.start)
                    .collect::<String>(),
                f.matched_text
            );
            f.matched_text
        })
        .collect()
}

#[test]
fn mapped_ipv6_is_one_complete_finding() {
    for address in ["::ffff:192.168.1.1", "64:ff9b::192.0.2.33"] {
        assert_eq!(values(address, "IP_ADDRESS"), [address]);
        let config = parse_scan_and_transform_config(
            &json!({"transform":{"default":{"strategy":"redact"},"entities":["IP_ADDRESS"]}}),
        )
        .unwrap();
        assert_eq!(
            scan_and_transform(address, &config).unwrap().text,
            "[IP_ADDRESS]"
        );
    }
    for invalid in [
        "x :: y",
        "a::b",
        "12::34",
        "at 10::30",
        "cafe::",
        "::ffff:999.1.2.3",
    ] {
        assert!(values(invalid, "IP_ADDRESS").is_empty(), "{invalid}");
    }
    assert_eq!(
        values("::1 2001:db8::1", "IP_ADDRESS"),
        ["::1", "2001:db8::1"]
    );
}

#[test]
fn email_values_do_not_absorb_fields_or_markup() {
    for text in [
        "phone=415-555-2671&email=bob@example.com",
        "id=42&email=bob@example.com",
        "user.email=bob@example.com",
        "*bob@example.com*",
        "{bob@example.com}",
    ] {
        assert_eq!(values(text, "EMAIL"), ["bob@example.com"], "{text}");
    }
    assert_eq!(
        values("bob@example.com|jane@example.com", "EMAIL"),
        ["bob@example.com", "jane@example.com"]
    );
    assert_eq!(values("bob..x@example.com", "EMAIL"), ["x@example.com"]);
    assert_eq!(values("mail jörg@müller.de", "EMAIL"), ["jörg@müller.de"]);
    assert_eq!(values("a+b@example.com", "EMAIL"), ["a+b@example.com"]);
}

#[test]
fn email_after_dots_is_reported_from_its_first_local_part_character() {
    for text in [
        "Contact (support).alice@example.com for help",
        "See note [1].alice@example.com",
        "Email me...alice@example.com",
        ".alice@example.com",
        "reply-to>.alice@example.com",
        "'.alice@example.com'",
        "Reach '.alice@example.com's",
        "..alice@example.com",
        "x&.alice@example.com",
        ".email=.alice@example.com",
    ] {
        assert_eq!(values(text, "EMAIL"), ["alice@example.com"], "{text}");
    }
    // Dots are not an address on their own, and a dot before the `@` is still invalid.
    for text in [".@example.com", "...@example.com", "alice.@example.com"] {
        assert!(values(text, "EMAIL").is_empty(), "{text}");
    }
    // Label, quote and numeric-neighbor boundaries are unchanged.
    for (text, email) in [
        ("email=alice@example.com", "alice@example.com"),
        ("note=customer=tag@example.com", "customer=tag@example.com"),
        ("\"alice@example.com\"", "alice@example.com"),
        ("94105.jane@example.com", "jane@example.com"),
        ("219-09-9999.jane@example.com", "jane@example.com"),
        ("first.last@example.com", "first.last@example.com"),
    ] {
        assert_eq!(values(text, "EMAIL"), [email], "{text}");
    }
}

#[test]
fn structured_phone_is_found_after_a_number_and_separator() {
    for (text, phone) in [
        ("2,+1-202-555-0143", "+1-202-555-0143"),
        ("id,phone\n2,+1-202-555-0143\n", "+1-202-555-0143"),
        ("2,(202) 555-0143", "(202) 555-0143"),
        ("row 2,202-555-0143,ok", "202-555-0143"),
        ("2,202.555.0143", "202.555.0143"),
        ("2,202 555 0143", "202 555 0143"),
        ("12,+44 20 7946 0958", "+44 20 7946 0958"),
        ("3.202-555-0143", "202-555-0143"),
        ("3.(202) 555-0143", "(202) 555-0143"),
        ("3.+1-202-555-0143", "+1-202-555-0143"),
        ("2024-01-02,202-555-0143", "202-555-0143"),
        ("$12.50,202-555-0143", "202-555-0143"),
    ] {
        assert_eq!(values(text, "PHONE"), [phone], "{text}");
    }
    // After `<digit>.` an unseparated or dotted run is still the tail of that
    // number; after `<digit>,` phone validation decides.
    for text in [
        "3,14159",
        "1,202",
        "12,5 kg",
        "1.234,56",
        "3,1415926536",
        "3.14159265358",
        "0.2025550143",
        "v2.202.555.0143",
        "2.202.555.0143",
        "7.202.555.0143",
        "10.20.202.555.0143",
    ] {
        assert!(values(text, "PHONE").is_empty(), "{text}");
    }
}

#[test]
fn unseparated_phone_is_found_after_a_number_and_comma() {
    for (text, phones) in [
        (
            "2025550143,2025550144,2025550145",
            vec!["2025550143", "2025550144", "2025550145"],
        ),
        (
            "phones: 2025550143,12025550144",
            vec!["2025550143", "12025550144"],
        ),
        (
            "id,age,phone\n1,34,2025550143\n2,41,6465550199\n",
            vec!["2025550143", "6465550199"],
        ),
        ("94105,2025550143", vec!["2025550143"]),
        ("2024-01-02,2025550143", vec!["2025550143"]),
        ("1,2025550143,1", vec!["2025550143"]),
        ("2,2025550143", vec!["2025550143"]),
        ("row 2,12025550143", vec!["12025550143"]),
        ("818,+65585080", vec!["+65585080"]),
        // The integer part of a single-dot run is a phone by itself.
        ("2,+442079460958.5", vec!["+442079460958.5"]),
        // Accepted cost: ten fractional digits that pass phone validation.
        ("2,7182818285", vec!["7182818285"]),
    ] {
        assert_eq!(values(text, "PHONE"), phones, "{text}");
    }
    // A single-dot decimal after a numeric column is a number, not a phone.
    for text in [
        "249.2754083,142.7399518,730.9506397",
        "6,306.2723362",
        "20,+86.528387",
        "2,1.2025550143",
    ] {
        let phones = values(text, "PHONE");
        assert!(
            phones.iter().all(|phone| text.starts_with(phone.as_str())),
            "{text}: {phones:?}"
        );
    }
    assert!(values("6,306.2723362", "PHONE").is_empty());
}

#[test]
fn phone_at_a_line_end_is_not_joined_with_the_next_line() {
    for (text, phones) in [
        (
            "id,phone\n1,+44 20 7946 0958\n2,+44 20 7946 0958\n",
            vec!["+44 20 7946 0958", "+44 20 7946 0958"],
        ),
        (
            "5,+49 30 901820\n6,+49 30 901821\n",
            vec!["+49 30 901820", "+49 30 901821"],
        ),
        ("+49 30 901820\n42 Main St", vec!["+49 30 901820"]),
        ("+91 98765 43210\n1,x", vec!["+91 98765 43210"]),
        ("+49 30 901820\r\n42 units", vec!["+49 30 901820"]),
        ("+49 30 901820\r42 units", vec!["+49 30 901820"]),
        ("+49 30 901820\u{c}42 units", vec!["+49 30 901820"]),
        ("+49 30 901820\u{b}42 units", vec!["+49 30 901820"]),
        ("+49 30 901820\u{2028}42 units", vec!["+49 30 901820"]),
        ("row 1\n202-555-0143", vec!["202-555-0143"]),
        ("é +1\r(202) 555-0143", vec!["(202) 555-0143"]),
    ] {
        assert_eq!(values(text, "PHONE"), phones, "{text}");
    }
    for text in [
        "202-555\n0143",
        "(202)\n555-0143",
        "202 555\r0143",
        "+1 202\n555",
        "202\u{c}555\u{c}0143",
        "+49 30\u{2029}901",
    ] {
        assert!(values(text, "PHONE").is_empty(), "{text}");
    }
}

#[test]
fn scanner_overlap_redaction_preserves_all_coverage() {
    let config =
        parse_scan_and_transform_config(&json!({"transform":{"default":{"strategy":"redact"}}}))
            .unwrap();
    for text in [
        "(415) 555-2671&a@b.co&",
        "415-555-2671::",
        "::a@b.co",
        "123-45-6789::",
        "Jan 2, 1990'#a@b.co",
    ] {
        let result = scan_and_transform(text, &config).unwrap();
        for finding in scan(text) {
            assert!(
                !result.text.contains(&finding.matched_text),
                "{text}: {}",
                result.text
            );
        }
    }
}

#[test]
fn reserved_token_prefixes_are_rejected_before_tokenization() {
    let config = TransformationConfig::new(TransformationStrategy::Tokenize(
        TokenizeConfig::new("test").unwrap(),
    ));
    let context = PrivacyContext::new("test").unwrap();
    for prefix in [
        "DFTOKENv2 docs",
        "DFTOKENv",
        "DFTOKENv1(3):abc",
        "DFTOKENv1(",
    ] {
        let text = format!("see {prefix}, mail jane@example.com");
        assert!(required_tokenization_items(&text, &scan(&text), &config, Some(&context)).is_err());
    }
}

#[test]
fn invalid_entity_markers_and_total_mask_reveals_are_rejected_or_masked() {
    let text = "a@b.com";
    let config = parse_transformation_config(&json!({"default":{"strategy":"redact"}})).unwrap();
    for label in ["", "a b\n[c]"] {
        let finding = Finding {
            entity_type: label.into(),
            matched_text: text.into(),
            byte_range: TextRange { start: 0, end: 7 },
            codepoint_range: TextRange { start: 0, end: 7 },
            detector_name: "synthetic".into(),
            confidence: None,
            detector_version: None,
        };
        assert!(transform(text, &[finding], &config).is_err());
    }
    for direction in ["first", "last"] {
        let config = parse_transformation_config(
            &json!({"default":{"strategy":"mask","reveal":{"direction":direction,"count":400}}}),
        )
        .unwrap();
        let result = transform(text, &scan(text), &config).unwrap();
        assert_ne!(result.text, text);
        assert_eq!(result.text.chars().filter(|c| *c == '*').count(), 1);
    }
}

#[test]
fn precision_and_datetime_review_cases() {
    assert_eq!(values("2024-01-02T03:04:05Z", "DATE"), ["2024-01-02"]);
    assert!(values("2024-01-02Thing", "DATE").is_empty());
    assert!(values("555-123-4567 94105", "CREDIT_CARD").is_empty());
    assert_eq!(values("555-123-4567 94105", "PHONE"), ["555-123-4567"]);
    for text in ["Steuer-ID 12345678995", "3.14159265358", "415\n555\n2671"] {
        assert!(values(text, "PHONE").is_empty(), "{text}");
    }
    for text in ["$12345.67", "ssn 123-45-67890", "2023-12345"] {
        assert!(values(text, "ZIP_CODE").is_empty(), "{text}");
    }
    assert!(values("IMEI 490154203237518", "CREDIT_CARD").is_empty());
    let token = format!("ghp_{}", "a".repeat(36));
    assert_eq!(
        values(&format!("https://{token}@github.com/"), "API_KEY"),
        [token]
    );
    let single_digit_card = "4 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1";
    assert_eq!(
        values(single_digit_card, "CREDIT_CARD"),
        [single_digit_card]
    );
    let single_digits = "4 ".repeat(100_000);
    assert!(values(&single_digits, "CREDIT_CARD").is_empty());
}

#[test]
fn private_keys_in_escaped_env_and_pgp_armor_keep_source_offsets() {
    for key in [
        "-----BEGIN PRIVATE KEY-----\nYWJjZA==\n-----END PRIVATE KEY-----",
        r"-----BEGIN PRIVATE KEY-----\nYWJjZA==\n-----END PRIVATE KEY-----",
        "-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: example\n\nYWJjZA==\n=abcd\n-----END PGP PRIVATE KEY BLOCK-----",
    ] {
        assert_eq!(values(key, "PRIVATE_KEY"), [key]);
    }
    assert!(
        values(
            "-----BEGIN PRIVATE KEY-----\ninvalid!\n-----END PRIVATE KEY-----",
            "PRIVATE_KEY"
        )
        .is_empty()
    );
}

#[test]
fn iban_digit_groups_are_never_relabelled_as_cards() {
    for text in [
        "IBAN DE44 5001 0517 5407 3249 31 ist gueltig.",
        "DE44\t4111111111111111\t11",
    ] {
        assert!(values(text, "CREDIT_CARD").is_empty(), "{text}");
    }
    // Additional card networks remain detectable outside an IBAN.
    for card in ["3530111333300000", "30569309025904", "6759649826438453"] {
        assert_eq!(values(card, "CREDIT_CARD"), [card]);
    }
}

#[test]
fn context_suppression_and_encoded_wrappers_cover_review_edges() {
    assert!(values("IMEI 490154203237518 123", "CREDIT_CARD").is_empty());
    assert_eq!(
        values("mail jo\u{0308}rg@mu\u{0308}ller.de", "EMAIL"),
        ["jo\u{0308}rg@mu\u{0308}ller.de"]
    );
    let key = r"-----BEGIN PRIVATE KEY-----\nYWJjZA==\n-----END PRIVATE KEY-----";
    for wrapped in [
        format!("PRIVATE_KEY=\"{key}\""),
        format!("{{\"key\":\"{key}\"}}"),
    ] {
        assert_eq!(values(&wrapped, "PRIVATE_KEY"), [key]);
    }
}

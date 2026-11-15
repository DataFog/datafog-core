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
    assert!(values("bob..x@example.com", "EMAIL").is_empty());
    assert_eq!(values("mail jörg@müller.de", "EMAIL"), ["jörg@müller.de"]);
    assert_eq!(values("a+b@example.com", "EMAIL"), ["a+b@example.com"]);
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

#[path = "support/synthetic_fixtures.rs"]
mod synthetic_fixtures;
use datafog_core::{
    PrivacyErrorReason, ScanConfig, capabilities, parse_scan_and_transform_config,
    parse_scan_config, scan, scan_with_config, structured,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn capability_contract_is_sorted_owned_and_scope_explicit() {
    let mut first = capabilities();
    let second = capabilities();
    let value = serde_json::to_value(&second).unwrap();
    assert_eq!(value["contract_version"], 1);
    assert_eq!(second.supported_entities.len(), 22);
    assert_eq!(second.default_entities.len(), 13);
    assert!(
        second
            .supported_entities
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
    assert!(
        second
            .default_entities
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
    assert_eq!(
        second.supported_entities,
        second.entities.keys().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        value["entities"]["PERSON"],
        json!({"scopes":["structured"],"activation":{"kind":"structured"}})
    );
    assert_eq!(
        value["entities"]["UUID"],
        json!({"scopes":["structured","text"],"activation":{"kind":"config","scan_config":{"detect_uuid":true}}})
    );
    for label in &second.default_entities {
        assert_eq!(
            value["entities"][label]["activation"],
            json!({"kind":"default"})
        );
    }
    assert!(
        !second
            .default_entities
            .iter()
            .any(|label| label == "PERSON" || label == "UUID" || label.starts_with("DE_"))
    );
    assert_eq!(
        second
            .locales
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["de", "de-DE", "de_DE", "en-US", "fr"]
    );
    for (locale, metadata) in &second.locales {
        assert!(
            metadata
                .enabled_entities
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        if locale.starts_with("de") {
            assert_eq!(metadata.enabled_entities.len(), 7);
            for label in &metadata.enabled_entities {
                assert_eq!(
                    value["entities"][label]["activation"],
                    json!({"kind":"locale","scan_config":{"locale":"de"}})
                );
            }
        } else {
            assert!(metadata.enabled_entities.is_empty());
        }
    }
    first.supported_entities.clear();
    first.default_entities.clear();
    first
        .locales
        .get_mut("de")
        .unwrap()
        .enabled_entities
        .clear();
    first.entities.get_mut("UUID").unwrap().scopes.clear();
    assert_eq!(capabilities(), second);
}

#[test]
fn every_advertised_text_entity_has_an_executable_activation_recipe() {
    let mut examples = BTreeMap::new();
    for corpus in [
        include_str!("../../../fixtures/development.jsonl"),
        include_str!("../../../fixtures/final.jsonl"),
        include_str!("../../../fixtures/german.jsonl"),
        include_str!("../../../fixtures/jwt.jsonl"),
        include_str!("../../../fixtures/bearer-token.jsonl"),
        include_str!("../../../fixtures/private-key.jsonl"),
        include_str!("../../../fixtures/api-key.jsonl"),
        include_str!("../../../fixtures/uuid.jsonl"),
        include_str!("../../../fixtures/us-routing-number.jsonl"),
        include_str!("../../../fixtures/npi.jsonl"),
    ] {
        for line in corpus.lines() {
            let row: Value = synthetic_fixtures::expand(serde_json::from_str(line).unwrap());
            for entity in row
                .get("entities")
                .or_else(|| row.get("matches"))
                .unwrap()
                .as_array()
                .unwrap()
            {
                examples
                    .entry(entity["label"].as_str().unwrap_or("UUID").to_owned())
                    .or_insert_with(|| row["text"].as_str().unwrap().to_owned());
            }
        }
    }
    let metadata = serde_json::to_value(capabilities()).unwrap();
    let mut exercised = BTreeSet::new();
    for (label, entity) in metadata["entities"].as_object().unwrap() {
        if entity["activation"]["kind"] == "structured" {
            let findings = structured::scan(
                &json!({"full_name":"Jane Smith"}),
                &structured::StructuredScanConfig::default(),
            )
            .unwrap()
            .findings;
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.finding.entity_type == *label)
            );
        } else {
            let config = entity["activation"]
                .get("scan_config")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let text = examples
                .get(label)
                .unwrap_or_else(|| panic!("missing fixture for {label}"));
            assert!(
                scan_with_config(text, &parse_scan_config(&config).unwrap())
                    .iter()
                    .any(|finding| finding.entity_type == *label),
                "advertised activation failed for {label}"
            );
            if entity["activation"]["kind"] != "default" {
                assert!(
                    scan(text)
                        .iter()
                        .all(|finding| finding.entity_type != *label),
                    "opt-in entity {label} enabled by default"
                );
            }
        }
        exercised.insert(label.to_owned());
    }
    assert_eq!(
        exercised.into_iter().collect::<Vec<_>>(),
        capabilities().supported_entities
    );
}

#[test]
fn explicit_unknown_locales_fail_consistently_and_known_aliases_preserve_values() {
    for locale in [
        "de", "de-DE", "de_DE", " De-dE ", "DE_de", "en-US", " EN-us ", "fr", "FR",
    ] {
        let config = ScanConfig::new().with_locale(locale).unwrap();
        assert_eq!(config.locale(), Some(locale));
        assert!(parse_scan_config(&json!({"locale":locale})).is_ok());
        assert!(structured::parse_scan_config(&json!({"locale":locale})).is_ok());
    }
    let error = ScanConfig::new().with_locale("zz-ZZ").unwrap_err();
    assert_eq!(error.reason(), Some(PrivacyErrorReason::InvalidValue));
    for locale in ["zz-ZZ", "en", "de-AT", "french"] {
        let config = json!({"locale":locale});
        for error in [
            parse_scan_config(&config).unwrap_err(),
            structured::parse_scan_config(&config).unwrap_err(),
        ] {
            assert_eq!(error.code().as_str(), "invalid_configuration");
            assert_eq!(error.reason(), Some(PrivacyErrorReason::InvalidValue));
            assert_eq!(error.path(), Some("/locale"));
            assert!(!error.to_string().contains(locale));
        }
        let envelope = json!({"scan":config,"transform":{"default":{"strategy":"redact"}}});
        for error in [
            parse_scan_and_transform_config(&envelope).unwrap_err(),
            structured::parse_scan_and_transform_config(&envelope).unwrap_err(),
        ] {
            assert_eq!(error.reason(), Some(PrivacyErrorReason::InvalidValue));
            assert_eq!(error.path(), Some("/scan/locale"));
        }
    }
    assert!(parse_scan_config(&json!({})).is_ok());
    assert_eq!(
        ScanConfig::new().with_locale("  ").unwrap_err().reason(),
        Some(PrivacyErrorReason::EmptyValue)
    );
    let text = "DE44500105175407324931";
    assert_eq!(
        scan(text),
        scan_with_config(text, &ScanConfig::new().with_locale("en-US").unwrap())
    );
    assert_eq!(
        scan(text),
        scan_with_config(text, &ScanConfig::new().with_locale("fr").unwrap())
    );
}

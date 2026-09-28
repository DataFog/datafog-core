//! Versioned detector inventory derived from the same registry used by scans.
use crate::{Candidate, Label, ScanConfig, german, structured};
use serde::Serialize;
use std::{collections::BTreeMap, sync::LazyLock};

/// Core capability contract. Version 1 is stable throughout the 0.4 series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub contract_version: u32,
    pub supported_entities: Vec<String>,
    /// Entities enabled by a default text scan; structured PERSON is separate.
    pub default_entities: Vec<String>,
    pub locales: BTreeMap<String, LocaleCapabilities>,
    pub entities: BTreeMap<String, EntityCapabilities>,
}

/// Additional entities enabled by an accepted locale identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocaleCapabilities {
    pub enabled_entities: Vec<String>,
}

/// Scan scopes and activation recipe for an entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityCapabilities {
    pub scopes: Vec<String>,
    pub activation: EntityActivation,
}

/// Configuration required to activate detection; transformation selection is separate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntityActivation {
    Default,
    Locale { scan_config: ActivationScanConfig },
    Config { scan_config: ActivationScanConfig },
    Structured,
}

/// A minimal scan configuration fragment that enables the entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivationScanConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detect_uuid: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Activation {
    Default,
    German,
    Uuid,
}

impl Activation {
    fn enabled(self, config: &ScanConfig) -> bool {
        match self {
            Self::Default => true,
            Self::German => config.locale().and_then(locale_activation) == Some(Self::German),
            Self::Uuid => config.uuid_detection_enabled(),
        }
    }

    fn metadata(self) -> EntityActivation {
        match self {
            Self::Default => EntityActivation::Default,
            Self::German => EntityActivation::Locale {
                scan_config: ActivationScanConfig {
                    locale: Some("de".into()),
                    detect_uuid: None,
                },
            },
            Self::Uuid => EntityActivation::Config {
                scan_config: ActivationScanConfig {
                    locale: None,
                    detect_uuid: Some(true),
                },
            },
        }
    }
}

struct Detector {
    labels: Vec<Label>,
    activation: Activation,
    detect: fn(&str, &mut Vec<Candidate>),
}

impl Detector {
    fn single(label: Label, detect: fn(&str, &mut Vec<Candidate>), activation: Activation) -> Self {
        Self {
            labels: vec![label],
            activation,
            detect,
        }
    }
}

static REGISTRY: LazyLock<Vec<Detector>> = LazyLock::new(|| {
    use Activation::{Default, Uuid};
    vec![
        Detector::single(Label::BearerToken, crate::bearer_token::detect, Default),
        Detector::single(Label::ApiKey, crate::api_key::detect, Default),
        Detector::single(Label::Jwt, crate::jwt::detect, Default),
        Detector::single(
            Label::UsRoutingNumber,
            crate::us_routing_number::detect,
            Default,
        ),
        Detector::single(Label::Npi, crate::npi::detect, Default),
        Detector::single(Label::Email, crate::detect_email, Default),
        Detector::single(Label::Phone, crate::detect_phone, Default),
        Detector::single(Label::Ssn, crate::detect_ssn, Default),
        Detector::single(Label::CreditCard, crate::detect_credit_card, Default),
        Detector::single(Label::Date, crate::detect_date, Default),
        Detector::single(Label::ZipCode, crate::detect_zip_code, Default),
        Detector::single(Label::IpAddress, crate::detect_ip_address, Default),
        Detector::single(Label::PrivateKey, crate::private_key::detect, Default),
        Detector {
            labels: german::labels(),
            activation: Activation::German,
            detect: german::detect,
        },
        Detector::single(Label::Uuid, crate::uuid::detect, Uuid),
    ]
});

// Aliases and their activation are shared by validation, dispatch, and metadata.
const LOCALES: &[(&str, Activation)] = &[
    ("de", Activation::German),
    ("de-DE", Activation::German),
    ("de_DE", Activation::German),
    ("en-US", Activation::Default),
    ("fr", Activation::Default),
];

fn locale_activation(locale: &str) -> Option<Activation> {
    LOCALES
        .iter()
        .find(|(alias, _)| locale.trim().eq_ignore_ascii_case(alias))
        .map(|(_, activation)| *activation)
}

pub(super) fn valid_locale(locale: &str) -> bool {
    locale_activation(locale).is_some()
}

pub(super) fn detect(text: &str, config: &ScanConfig, candidates: &mut Vec<Candidate>) {
    for detector in REGISTRY
        .iter()
        .filter(|detector| detector.activation.enabled(config))
    {
        (detector.detect)(text, candidates);
    }
}

/// Return a fresh owned capability inventory derived from executable detectors.
/// Adding fields or entities is additive; changing existing field meaning requires
/// a new contract version. Locale inventories list additions to text defaults.
pub fn capabilities() -> Capabilities {
    let mut entities = BTreeMap::new();
    let mut default_entities = Vec::new();
    for detector in REGISTRY.iter() {
        for label in &detector.labels {
            let name = label.as_str().to_owned();
            if detector.activation == Activation::Default {
                default_entities.push(name.clone());
            }
            entities.insert(
                name,
                EntityCapabilities {
                    scopes: vec!["structured".into(), "text".into()],
                    activation: detector.activation.metadata(),
                },
            );
        }
    }
    entities.insert(
        structured::PERSON_ENTITY_TYPE.into(),
        EntityCapabilities {
            scopes: vec!["structured".into()],
            activation: EntityActivation::Structured,
        },
    );
    default_entities.sort();
    default_entities.dedup();
    let locales = LOCALES
        .iter()
        .map(|(alias, activation)| {
            let mut enabled_entities = REGISTRY
                .iter()
                .filter(|detector| {
                    detector.activation != Activation::Default && detector.activation == *activation
                })
                .flat_map(|detector| {
                    detector
                        .labels
                        .iter()
                        .map(|label| label.as_str().to_owned())
                })
                .collect::<Vec<_>>();
            enabled_entities.sort();
            enabled_entities.dedup();
            ((*alias).into(), LocaleCapabilities { enabled_entities })
        })
        .collect();
    Capabilities {
        contract_version: 1,
        supported_entities: entities.keys().cloned().collect(),
        default_entities,
        locales,
        entities,
    }
}

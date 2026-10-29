//! Core-owned type rules and exact normalized per-value choices.
use super::*;
use serde::{Deserialize, Serialize};

pub const MAX_POLICY_ITEMS: usize = 20_000;
pub const MAX_POLICY_EDITS: usize = 10_000;
pub(super) const PROFILE: &str = "protected_v1";

/// Fixed catalog for this profile; custom types still default to hide.
pub const PROTECTED_TYPES: &[&str] = &[
    "ACCOUNT_NUMBER",
    "ADDRESS",
    "API_KEY",
    "BEARER_TOKEN",
    "CREDENTIAL_URI",
    "CREDIT_CARD",
    "CUSTOMER_ID",
    "DATE",
    "DE_IBAN",
    "DE_PASSPORT_NUMBER",
    "DE_POSTAL_CODE",
    "DE_RESIDENCE_PERMIT_NUMBER",
    "DE_SOCIAL_SECURITY_NUMBER",
    "DE_TAX_ID",
    "DE_VAT_ID",
    "EMAIL",
    "FIRST_NAME",
    "IP_ADDRESS",
    "JWT",
    "LAST_NAME",
    "NPI",
    "PERSON",
    "PHONE",
    "PRIVATE_KEY",
    "SSN",
    "STREET_ADDRESS",
    "US_ROUTING_NUMBER",
    "UUID",
    "ZIP_CODE",
];

pub fn protected_type(entity_type: &str) -> bool {
    PROTECTED_TYPES.contains(&entity_type)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeRule {
    Hide,
    Keep,
}
impl TypeRule {
    pub(super) fn code(self) -> u8 {
        match self {
            Self::Hide => 0,
            Self::Keep => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceDisposition {
    Keep,
    NotSensitive,
}
impl ChoiceDisposition {
    pub(super) fn code(self) -> u8 {
        match self {
            Self::Keep => 0,
            Self::NotSensitive => 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TypeRuleEdit {
    pub entity_type: String,
    pub rule: Option<TypeRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ValueChoiceEdit {
    pub entity_type: String,
    pub value: String,
    pub disposition: Option<ChoiceDisposition>,
}
impl Drop for ValueChoiceEdit {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyEdits {
    pub rules: Vec<TypeRuleEdit>,
    pub choices: Vec<ValueChoiceEdit>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleView {
    pub entity_type: String,
    pub rule: TypeRule,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceView {
    pub entity_type: String,
    pub value: String,
    pub disposition: ChoiceDisposition,
}
impl Drop for ChoiceView {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyView {
    pub snapshot_version: u8,
    pub default_rule: TypeRule,
    pub protected_types: Vec<String>,
    pub rules: Vec<RuleView>,
    pub choices: Vec<ChoiceView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEditPreview {
    pub entity_type: String,
    pub before: Option<TypeRule>,
    pub after: Option<TypeRule>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceEditPreview {
    pub entity_type: String,
    pub value: String,
    pub before: Option<ChoiceDisposition>,
    pub after: Option<ChoiceDisposition>,
}
impl Drop for ChoiceEditPreview {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct PolicyPreview {
    pub rules: Vec<RuleEditPreview>,
    pub choices: Vec<ChoiceEditPreview>,
}

#[derive(Clone)]
pub(super) struct RuleRecord {
    pub rule: TypeRule,
    pub operation: PublicId,
}
#[derive(Clone)]
pub(super) struct ChoiceRecord {
    pub display: String,
    pub disposition: ChoiceDisposition,
    pub operation: PublicId,
}
impl Drop for ChoiceRecord {
    fn drop(&mut self) {
        self.display.zeroize();
    }
}

pub(super) fn choice_identity(entity_type: &str, value: &str) -> Result<Identity> {
    if !valid_type(entity_type)
        || value.len() > MAX_SOURCE_BYTES
        || value.trim().is_empty()
        || marker_spans(value)?
            .iter()
            .any(|span| span.canonical.is_some())
    {
        return Err(error(ErrorCode::InvalidPolicy));
    }
    Ok(Identity {
        entity_type: entity_type.to_owned(),
        key: identity_key(value),
    })
}

impl State {
    pub(super) fn hides(&self, identity: &Identity) -> bool {
        if let Some(choice) = self.choices.get(identity) {
            return match choice.disposition {
                ChoiceDisposition::NotSensitive => false,
                ChoiceDisposition::Keep => protected_type(&identity.entity_type),
            };
        }
        protected_type(&identity.entity_type)
            || self
                .rules
                .get(&identity.entity_type)
                .is_none_or(|record| record.rule == TypeRule::Hide)
    }
    pub(super) fn policy_view(&self) -> PolicyView {
        PolicyView {
            snapshot_version: self.snapshot_version,
            default_rule: TypeRule::Hide,
            protected_types: PROTECTED_TYPES
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            rules: self
                .rules
                .iter()
                .map(|(label, record)| RuleView {
                    entity_type: label.clone(),
                    rule: record.rule,
                })
                .collect(),
            choices: self
                .choices
                .iter()
                .map(|(identity, record)| ChoiceView {
                    entity_type: identity.entity_type.clone(),
                    value: record.display.clone(),
                    disposition: record.disposition,
                })
                .collect(),
        }
    }
}

pub(super) fn apply(
    state: &mut State,
    edits: &PolicyEdits,
    operation: PublicId,
) -> Result<(PolicyPreview, bool)> {
    let count = edits
        .rules
        .len()
        .checked_add(edits.choices.len())
        .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
    if count == 0 {
        return Err(error(ErrorCode::InvalidPolicy));
    }
    if count > MAX_POLICY_EDITS {
        return Err(error(ErrorCode::ResourceLimit));
    }
    let mut value_bytes = 0usize;
    for edit in &edits.choices {
        value_bytes = value_bytes
            .checked_add(edit.value.len())
            .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
        if value_bytes > MAX_BUNDLE_BYTES {
            return Err(error(ErrorCode::ResourceLimit));
        }
    }
    let mut preview = PolicyPreview::default();
    let mut changed = false;
    let mut seen_rules = BTreeSet::new();
    let mut seen_choices = BTreeSet::new();
    for edit in &edits.rules {
        if !valid_type(&edit.entity_type)
            || !seen_rules.insert(&edit.entity_type)
            || (protected_type(&edit.entity_type) && edit.rule == Some(TypeRule::Keep))
        {
            return Err(error(ErrorCode::InvalidPolicy));
        }
        let before = state.rules.get(&edit.entity_type).map(|record| record.rule);
        if before != edit.rule {
            changed = true;
            if let Some(rule) = edit.rule {
                state
                    .rules
                    .insert(edit.entity_type.clone(), RuleRecord { rule, operation });
            } else {
                state.rules.remove(&edit.entity_type);
            }
        }
        preview.rules.push(RuleEditPreview {
            entity_type: edit.entity_type.clone(),
            before,
            after: edit.rule,
        });
    }
    for edit in &edits.choices {
        let identity = choice_identity(&edit.entity_type, &edit.value)?;
        if !seen_choices.insert(identity.clone())
            || (protected_type(&edit.entity_type)
                && edit.disposition == Some(ChoiceDisposition::Keep))
        {
            return Err(error(ErrorCode::InvalidPolicy));
        }
        let before = state
            .choices
            .get(&identity)
            .map(|record| record.disposition);
        if before != edit.disposition {
            changed = true;
            if let Some(disposition) = edit.disposition {
                let display = state
                    .choices
                    .get(&identity)
                    .map_or_else(|| edit.value.clone(), |record| record.display.clone());
                state.choices.insert(
                    identity,
                    ChoiceRecord {
                        display,
                        disposition,
                        operation,
                    },
                );
            } else {
                state.choices.remove(&identity);
            }
        }
        preview.choices.push(ChoiceEditPreview {
            entity_type: edit.entity_type.clone(),
            value: edit.value.clone(),
            before,
            after: edit.disposition,
        });
    }
    if state.rules.len() > MAX_POLICY_ITEMS || state.choices.len() > MAX_POLICY_ITEMS {
        return Err(error(ErrorCode::ResourceLimit));
    }
    Ok((preview, changed))
}

impl Workspace {
    pub fn policy(&self) -> Result<PolicyView> {
        Ok(self.state()?.policy_view())
    }

    /// Stage exact settings edits. The trusted host must approve and save before activation.
    pub fn prepare_policy_changes(
        &self,
        edits: &PolicyEdits,
        operation: PublicId,
    ) -> Result<PreparedChange> {
        let state = self.state()?;
        ensure_fresh_operation(state, operation)?;
        let mut candidate = state.clone();
        let (preview, changed) = apply(&mut candidate, edits, operation)?;
        if changed {
            candidate.snapshot_version = 2;
            candidate.head = Head {
                revision: state
                    .head
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| error(ErrorCode::ResourceLimit))?,
                commit_id: operation,
            };
        }
        let bundle = Zeroizing::new(codec::encode(&candidate)?);
        Ok(PreparedChange {
            pending: Some(Pending {
                lifetime: Arc::clone(&self.lifetime),
                parent: state.head,
                parent_snapshot_version: state.snapshot_version,
                candidate,
                bundle,
                previews: Vec::new(),
                policy_preview: Some(preview),
            }),
        })
    }
}

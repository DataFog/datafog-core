//! Schema-guided scanning of JSON string values. Finding offsets are local to
//! the decoded string at the reported RFC 6901 JSON Pointer.
use super::*;
use serde_json::Value;

/// Reusable structured scan policy. Explicit PERSON mappings still apply when
/// automatic discovery is disabled. Exclusions affect PERSON discovery only.
#[derive(Debug, Clone, Default)]
pub struct StructuredScanConfig {
    scan: ScanConfig,
    disable_person_discovery: bool,
    mappings: BTreeSet<String>,
    exclude: BTreeSet<String>,
}

/// Evidence identifying a name field; contains no field value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FieldMapping {
    pub path: String,
    pub entity_type: String,
    /// `field_alias` or `explicit_mapping`.
    pub source: String,
    /// Canonical alias, or `explicit_mapping` for a caller-declared field.
    pub rule: String,
}

/// A text finding located inside a JSON string value.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredFinding {
    pub path: String,
    pub finding: Finding,
}

/// Discovered name fields and all text findings, in deterministic field order.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredScanResult {
    pub mappings: Vec<FieldMapping>,
    pub findings: Vec<StructuredFinding>,
}

/// A structured argument could not be represented as supported JSON data.
/// Bindings use this constructor to avoid leaking values in conversion errors.
pub fn invalid_data() -> PrivacyError {
    PrivacyError::invalid_configuration(
        PrivacyErrorReason::InvalidType,
        "/data",
        "data must be a JSON object or array with finite numbers and safe integers",
    )
}

/// Decode JSON using serde_json's default nesting limit. Conversion failures
/// deliberately omit parser excerpts, which may contain sensitive values.
pub fn parse_document_json(text: &str) -> Result<Value, PrivacyError> {
    let data: Value = serde_json::from_str(text).map_err(|_| invalid_data())?;
    // Reject unsupported numeric representations before any findings are exposed.
    leaves(&data)?;
    Ok(data)
}

fn validate_pointer(pointer: &str, path: &str) -> Result<(), PrivacyError> {
    let mut bytes = pointer.bytes();
    let valid_prefix = bytes.next() == Some(b'/');
    let mut valid_escapes = true;
    while let Some(byte) = bytes.next() {
        if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
            valid_escapes = false;
            break;
        }
    }
    if !valid_prefix || !valid_escapes {
        return Err(PrivacyError::invalid_configuration(
            PrivacyErrorReason::InvalidValue,
            path,
            "field path must be a non-root RFC 6901 JSON Pointer",
        ));
    }
    Ok(())
}

/// Parse `{ locale?, discover_person?, mappings?: {pointer: "PERSON"}, exclude?: [pointer] }`.
pub fn parse_scan_config(value: &Value) -> Result<StructuredScanConfig, PrivacyError> {
    let object = require_object(value, "", "structured scan configuration must be an object")?;
    reject_unknown_fields(
        object,
        &["locale", "discover_person", "mappings", "exclude"],
        "",
    )?;
    let mut config = StructuredScanConfig::default();
    if let Some(locale) = object.get("locale") {
        config.scan = config.scan.with_locale(require_string(
            locale,
            "/locale",
            "locale must be a string",
        )?)?;
    }
    if let Some(enabled) = object.get("discover_person") {
        config.disable_person_discovery = !enabled.as_bool().ok_or_else(|| {
            PrivacyError::invalid_configuration(
                PrivacyErrorReason::InvalidType,
                "/discover_person",
                "discover_person must be a boolean",
            )
        })?;
    }
    if let Some(mappings) = object.get("mappings") {
        for (pointer, entity) in
            require_object(mappings, "/mappings", "mappings must be an object")?
        {
            let path = format!("/mappings/{}", json_pointer_segment(pointer));
            validate_pointer(pointer, &path)?;
            if require_string(entity, &path, "mapping entity must be a string")? != "PERSON" {
                return Err(PrivacyError::invalid_configuration(
                    PrivacyErrorReason::InvalidValue,
                    path,
                    "structured mappings currently support PERSON only",
                ));
            }
            config.mappings.insert(pointer.clone());
        }
    }
    if let Some(exclude) = object.get("exclude") {
        let values = exclude.as_array().ok_or_else(|| {
            PrivacyError::invalid_configuration(
                PrivacyErrorReason::InvalidType,
                "/exclude",
                "exclude must be an array",
            )
        })?;
        for (index, pointer) in values.iter().enumerate() {
            let path = format!("/exclude/{index}");
            let pointer = require_string(pointer, &path, "excluded path must be a string")?;
            validate_pointer(&pointer, &path)?;
            if config.mappings.contains(&pointer) {
                return Err(PrivacyError::invalid_configuration(
                    PrivacyErrorReason::InvalidValue,
                    path,
                    "a field cannot be both explicitly mapped and excluded",
                ));
            }
            if !config.exclude.insert(pointer) {
                return Err(PrivacyError::invalid_configuration(
                    PrivacyErrorReason::DuplicateValue,
                    path,
                    "excluded field paths must be unique",
                ));
            }
        }
    }
    Ok(config)
}

fn person_alias(key: &str) -> Option<&'static str> {
    // Explicit spelling variants avoid substring/fuzzy matches on arbitrary keys.
    const ALIASES: [(&str, &str, &str); 6] = [
        ("first_name", "firstName", "FirstName"),
        ("given_name", "givenName", "GivenName"),
        ("last_name", "lastName", "LastName"),
        ("family_name", "familyName", "FamilyName"),
        ("full_name", "fullName", "FullName"),
        ("surname", "surname", "Surname"),
    ];
    ALIASES.iter().find_map(|(snake, camel, pascal)| {
        (key.eq_ignore_ascii_case(snake) || key == *camel || key == *pascal).then_some(*snake)
    })
}

struct Leaf<'a> {
    path: String,
    key: Option<&'a str>,
    text: &'a str,
}

fn leaves(data: &Value) -> Result<Vec<Leaf<'_>>, PrivacyError> {
    if !data.is_object() && !data.is_array() {
        return Err(invalid_data());
    }
    let mut pending = vec![(String::new(), None, data, 1usize)];
    let mut leaves = Vec::new();
    while let Some((path, key, value, depth)) = pending.pop() {
        // Match serde_json's default parser: at most 127 nested containers.
        if depth >= 128 && (value.is_object() || value.is_array()) {
            return Err(invalid_data());
        }
        match value {
            Value::Object(object) => {
                let mut entries: Vec<_> = object.iter().collect();
                entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
                for (key, value) in entries.into_iter().rev() {
                    pending.push((
                        format!("{path}/{}", json_pointer_segment(key)),
                        Some(key.as_str()),
                        value,
                        depth + 1,
                    ));
                }
            }
            Value::Array(array) => {
                for (index, value) in array.iter().enumerate().rev() {
                    pending.push((format!("{path}/{index}"), None, value, depth + 1));
                }
            }
            Value::String(text) => leaves.push(Leaf { path, key, text }),
            Value::Number(number) => {
                // JavaScript Number.MAX_SAFE_INTEGER is the portable integer
                // boundary shared by the native and browser bindings.
                let number = number.as_f64().ok_or_else(invalid_data)?;
                if !number.is_finite()
                    || (number.fract() == 0.0 && number.abs() > 9_007_199_254_740_991.0)
                {
                    return Err(invalid_data());
                }
            }
            Value::Null | Value::Bool(_) => {}
        }
    }
    Ok(leaves)
}

fn field_mapping(leaf: &Leaf<'_>, config: &StructuredScanConfig) -> Option<FieldMapping> {
    let explicit = config.mappings.contains(&leaf.path);
    let (source, rule) = if explicit {
        ("explicit_mapping", "explicit_mapping")
    } else {
        if config.disable_person_discovery || config.exclude.contains(&leaf.path) {
            return None;
        }
        ("field_alias", person_alias(leaf.key?)?)
    };
    Some(FieldMapping {
        path: leaf.path.clone(),
        entity_type: "PERSON".into(),
        source: source.into(),
        rule: rule.into(),
    })
}

/// Discover name fields without scanning their contents for other entities.
/// Empty string fields can have a mapping while producing no PERSON finding.
pub fn discover_fields(
    data: &Value,
    config: &StructuredScanConfig,
) -> Result<Vec<FieldMapping>, PrivacyError> {
    Ok(leaves(data)?
        .iter()
        .filter_map(|leaf| field_mapping(leaf, config))
        .collect())
}

/// Scan every JSON string leaf and use name-field evidence to emit PERSON.
pub fn scan(
    data: &Value,
    config: &StructuredScanConfig,
) -> Result<StructuredScanResult, PrivacyError> {
    let mut result = StructuredScanResult {
        mappings: Vec::new(),
        findings: Vec::new(),
    };
    for leaf in leaves(data)? {
        let mut findings = scan_with_config(leaf.text, &config.scan);
        if let Some(mapping) = field_mapping(&leaf, config) {
            if !leaf.text.trim().is_empty() {
                findings.push(Finding {
                    entity_type: "PERSON".into(),
                    matched_text: leaf.text.into(),
                    byte_range: TextRange {
                        start: 0,
                        end: leaf.text.len(),
                    },
                    codepoint_range: TextRange {
                        start: 0,
                        end: leaf.text.chars().count(),
                    },
                    confidence: None,
                    detector_name: format!("datafog-core/person/{}", mapping.source),
                    detector_version: Some(env!("CARGO_PKG_VERSION").into()),
                });
            }
            result.mappings.push(mapping);
        }
        findings.sort_by(|left, right| {
            left.byte_range
                .start
                .cmp(&right.byte_range.start)
                .then_with(|| right.byte_range.end.cmp(&left.byte_range.end))
                .then_with(|| left.entity_type.cmp(&right.entity_type))
        });
        result
            .findings
            .extend(findings.into_iter().map(|finding| StructuredFinding {
                path: leaf.path.clone(),
                finding,
            }));
    }
    Ok(result)
}

/// A transformation inside a JSON string value.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredTransformation {
    pub path: String,
    pub transformation: Transformation,
}

/// A transformed JSON document and field-relative replacement records.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredTransformResult {
    pub data: Value,
    pub transformations: Vec<StructuredTransformation>,
}

struct SelectedLeaf<'a> {
    path: String,
    text: &'a str,
    findings: Vec<Finding>,
}

fn selected_leaves<'a>(
    data: &'a Value,
    findings: &[StructuredFinding],
    config: &TransformationConfig,
) -> Result<Vec<SelectedLeaf<'a>>, PrivacyError> {
    let all = leaves(data)?;
    let mut grouped: BTreeMap<&str, Vec<Finding>> = BTreeMap::new();
    for (index, located) in findings.iter().enumerate() {
        if validate_pointer(&located.path, "").is_err()
            || data
                .pointer(&located.path)
                .and_then(Value::as_str)
                .is_none()
        {
            return Err(PrivacyError {
                code: PrivacyErrorCode::InvalidFinding,
                reason: Some(PrivacyErrorReason::InvalidValue),
                path: Some(format!("/findings/{index}/path")),
                finding_index: Some(index),
                message: "finding path must select an existing string value".into(),
            });
        }
        let text = data
            .pointer(&located.path)
            .and_then(Value::as_str)
            .ok_or_else(invalid_data)?;
        validate_finding(text, &located.finding).map_err(|kind| {
            let mut error = PrivacyError::invalid_finding(index, kind);
            error.path = error.path.map(|path| {
                path.replacen(
                    &format!("/findings/{index}/"),
                    &format!("/findings/{index}/finding/"),
                    1,
                )
            });
            error
        })?;
        grouped
            .entry(&located.path)
            .or_default()
            .push(located.finding.clone());
    }
    all.into_iter()
        .filter_map(|leaf| {
            grouped.remove(leaf.path.as_str()).map(|findings| {
                select_findings(leaf.text, &findings, config).map(|findings| SelectedLeaf {
                    path: leaf.path,
                    text: leaf.text,
                    findings,
                })
            })
        })
        .collect()
}

fn leaf_selectors(leaves: &[SelectedLeaf<'_>], config: &TransformationConfig) -> Vec<KeySelector> {
    let all: Vec<_> = leaves
        .iter()
        .flat_map(|leaf| leaf.findings.iter().cloned())
        .collect();
    key_selectors(config, &all)
}

fn leaf_token_items(
    leaves: &[SelectedLeaf<'_>],
    config: &TransformationConfig,
    context: Option<&PrivacyContext>,
) -> Result<Vec<TokenizeItem>, PrivacyError> {
    let mut items = Vec::new();
    for (index, leaf) in leaves.iter().enumerate() {
        for mut item in
            super::required_tokenization_items(leaf.text, &leaf.findings, config, context)?
        {
            item.id = format!("{index}:{}", item.id);
            items.push(item);
        }
    }
    Ok(items)
}

/// Resolve each distinct key once across the whole document.
pub fn required_key_selectors(
    data: &Value,
    findings: &[StructuredFinding],
    config: &TransformationConfig,
) -> Result<Vec<KeySelector>, PrivacyError> {
    Ok(leaf_selectors(
        &selected_leaves(data, findings, config)?,
        config,
    ))
}

/// Collect one request-wide tokenization batch after validating all findings.
pub fn required_tokenization_items(
    data: &Value,
    findings: &[StructuredFinding],
    config: &TransformationConfig,
    context: Option<&PrivacyContext>,
) -> Result<Vec<TokenizeItem>, PrivacyError> {
    leaf_token_items(&selected_leaves(data, findings, config)?, config, context)
}

/// Transform all selected fields using complete provider results. Input data is
/// never mutated; failure returns no partial structured result.
pub fn transform_with_provider_results(
    data: &Value,
    findings: &[StructuredFinding],
    config: &TransformationConfig,
    context: Option<&PrivacyContext>,
    keys: Vec<ResolvedKeyBinding>,
    token_results: Vec<TokenizeResult>,
) -> Result<StructuredTransformResult, PrivacyError> {
    let leaves = selected_leaves(data, findings, config)?;
    let keys = validate_resolved_keys(leaf_selectors(&leaves, config), keys)?;
    let tokens =
        validate_tokenize_results(&leaf_token_items(&leaves, config, context)?, token_results)?;
    let mut output = data.clone();
    let mut transformations = Vec::new();
    for (index, leaf) in leaves.iter().enumerate() {
        let prefix = format!("{index}:");
        let local_tokens = tokens
            .iter()
            .filter_map(|(id, token)| {
                id.strip_prefix(&prefix)
                    .map(|id| (id.to_owned(), token.clone()))
            })
            .collect();
        let result =
            apply_transformations(leaf.text, &leaf.findings, config, &keys, &local_tokens)?;
        *output
            .pointer_mut(&leaf.path)
            .ok_or_else(|| PrivacyError::internal("structured field disappeared"))? =
            Value::String(result.text);
        transformations.extend(result.transformations.into_iter().map(|transformation| {
            StructuredTransformation {
                path: leaf.path.clone(),
                transformation,
            }
        }));
    }
    Ok(StructuredTransformResult {
        data: output,
        transformations,
    })
}

/// Transform explicit structured findings without invoking detection.
pub fn transform(
    data: &Value,
    findings: &[StructuredFinding],
    config: &TransformationConfig,
) -> Result<StructuredTransformResult, PrivacyError> {
    if let Some(selector) = required_key_selectors(data, findings, config)?
        .into_iter()
        .next()
    {
        return Err(PrivacyError::provider_required(selector.path));
    }
    transform_with_provider_results(data, findings, config, None, Vec::new(), Vec::new())
}

/// Reusable detection and transformation configuration for structured data.
#[derive(Debug, Clone)]
pub struct StructuredScanAndTransformConfig {
    pub scan: StructuredScanConfig,
    pub transform: TransformationConfig,
}

pub fn parse_scan_and_transform_config(
    value: &Value,
) -> Result<StructuredScanAndTransformConfig, PrivacyError> {
    let object = require_object(
        value,
        "",
        "scan-and-transform configuration must be an object",
    )?;
    reject_unknown_fields(object, &["scan", "transform"], "")?;
    let scan = object
        .get("scan")
        .map(parse_scan_config)
        .transpose()
        .map_err(|error| error.prefixed("/scan"))?
        .unwrap_or_default();
    let transform = object.get("transform").ok_or_else(|| {
        PrivacyError::invalid_configuration(
            PrivacyErrorReason::MissingField,
            "/transform",
            "transformation configuration is required",
        )
    })?;
    let transform =
        parse_transformation_config(transform).map_err(|error| error.prefixed("/transform"))?;
    Ok(StructuredScanAndTransformConfig { scan, transform })
}

pub fn scan_and_transform(
    data: &Value,
    config: &StructuredScanAndTransformConfig,
) -> Result<StructuredTransformResult, PrivacyError> {
    let findings = scan(data, &config.scan)?.findings;
    transform(data, &findings, &config.transform).map_err(|error| error.prefixed("/transform"))
}

impl<P: KeyProvider, T: TokenProvider> PrivacyManager<P, T> {
    /// Validate the whole document, resolve keys once, and issue one token batch.
    pub async fn transform_structured(
        &self,
        data: &Value,
        findings: &[StructuredFinding],
        config: &TransformationConfig,
        context: Option<&PrivacyContext>,
    ) -> Result<StructuredTransformResult, PrivacyError> {
        let leaves = selected_leaves(data, findings, config)?;
        let items = leaf_token_items(&leaves, config, context)?;
        let mut keys = Vec::new();
        for selector in leaf_selectors(&leaves, config) {
            if !self.provider.is_configured() {
                return Err(PrivacyError::provider_required(selector.path));
            }
            let key = self
                .provider
                .resolve_key(selector.clone())
                .await
                .map_err(|error| PrivacyError::from_provider_error(selector.path.clone(), error))?;
            validate_resolved_key(&selector, &key)?;
            keys.push(ResolvedKeyBinding::new(selector, key));
        }
        let tokens = if items.is_empty() {
            Vec::new()
        } else {
            if !self.token_provider.is_configured() {
                return Err(PrivacyError::token_provider_required("/context/scope"));
            }
            let context =
                context.ok_or_else(|| PrivacyError::token_provider_required("/context/scope"))?;
            self.token_provider
                .tokenize_batch(context.scope(), items)
                .await
                .map_err(PrivacyError::from_token_provider_error)?
        };
        transform_with_provider_results(data, findings, config, context, keys, tokens)
    }

    pub async fn scan_and_transform_structured(
        &self,
        data: &Value,
        config: &StructuredScanAndTransformConfig,
        context: Option<&PrivacyContext>,
    ) -> Result<StructuredTransformResult, PrivacyError> {
        let findings = scan(data, &config.scan)?.findings;
        self.transform_structured(data, &findings, &config.transform, context)
            .await
            .map_err(|error| error.prefixed("/transform"))
    }
}

/// A restoration record local to one JSON string value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredRestoration {
    pub path: String,
    pub restoration: Restoration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredRestoreResult {
    pub data: Value,
    pub restorations: Vec<StructuredRestoration>,
}

fn restore_inventory(data: &Value) -> Result<BTreeMap<String, RestoreItem>, PrivacyError> {
    let mut inventory = BTreeMap::new();
    for leaf in leaves(data)? {
        for token in parse_tokens(leaf.text)? {
            inventory.entry(token.envelope).or_insert(RestoreItem {
                id: String::new(),
                token_ref: token.token_ref,
                resolved_version: token.resolved_version,
                payload: token.payload,
            });
        }
    }
    for (index, item) in inventory.values_mut().enumerate() {
        item.id = index.to_string();
    }
    Ok(inventory)
}

pub fn required_restore_items(
    data: &Value,
    _context: &PrivacyContext,
) -> Result<Vec<RestoreItem>, PrivacyError> {
    Ok(restore_inventory(data)?.into_values().collect())
}

pub fn restore_with_results(
    data: &Value,
    context: &PrivacyContext,
    results: Vec<RestoredValue>,
) -> Result<StructuredRestoreResult, PrivacyError> {
    let inventory = restore_inventory(data)?;
    let expected: BTreeSet<_> = inventory.values().map(|item| item.id.as_str()).collect();
    let mut values = BTreeMap::new();
    for result in results {
        if !expected.contains(result.id.as_str())
            || values.insert(result.id, result.value).is_some()
        {
            return Err(PrivacyError::invalid_token_material());
        }
    }
    if values.len() != expected.len() {
        return Err(PrivacyError::invalid_token_material());
    }
    let mut output = data.clone();
    let mut restorations = Vec::new();
    for leaf in leaves(data)? {
        let mut local = BTreeMap::new();
        let mut local_results = Vec::new();
        for token in parse_tokens(leaf.text)? {
            if local.contains_key(&token.envelope) {
                continue;
            }
            let id = local.len().to_string();
            let item = inventory
                .get(&token.envelope)
                .ok_or_else(PrivacyError::invalid_token_material)?;
            let value = values
                .get(&item.id)
                .ok_or_else(PrivacyError::invalid_token_material)?;
            local.insert(token.envelope, id.clone());
            local_results.push(RestoredValue::new(id, value.clone()));
        }
        let result = super::restore_with_results(leaf.text, context, local_results)?;
        *output
            .pointer_mut(&leaf.path)
            .ok_or_else(|| PrivacyError::internal("structured field disappeared"))? =
            Value::String(result.text);
        restorations.extend(result.restorations.into_iter().map(|restoration| {
            StructuredRestoration {
                path: leaf.path.clone(),
                restoration,
            }
        }));
    }
    Ok(StructuredRestoreResult {
        data: output,
        restorations,
    })
}

impl<P: KeyProvider, T: TokenProvider> PrivacyManager<P, T> {
    pub async fn restore_structured(
        &self,
        data: &Value,
        context: &PrivacyContext,
    ) -> Result<StructuredRestoreResult, PrivacyError> {
        let items = required_restore_items(data, context)?;
        let results = if items.is_empty() {
            Vec::new()
        } else {
            if !self.token_provider.is_configured() {
                return Err(PrivacyError::token_provider_required("/restore"));
            }
            self.token_provider
                .restore_batch(context.scope(), items)
                .await
                .map_err(PrivacyError::from_token_provider_error)?
        };
        restore_with_results(data, context, results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_structured_fixtures() {
        for line in include_str!("../../../fixtures/structured.jsonl").lines() {
            let case: Value = serde_json::from_str(line).unwrap();
            let config = parse_scan_config(case.get("config").unwrap_or(&json!({}))).unwrap();
            let result = scan(&case["data"], &config).unwrap();
            assert_eq!(
                serde_json::to_value(&result.mappings).unwrap(),
                case["mappings"],
                "{}",
                case["id"]
            );
            assert_eq!(
                discover_fields(&case["data"], &config).unwrap(),
                result.mappings
            );
            let projected: Vec<_> = result.findings.iter().map(|located| {
                let finding = &located.finding;
                let text = case["data"].pointer(&located.path).unwrap().as_str().unwrap();
                assert_eq!(&text[finding.byte_range.start..finding.byte_range.end], finding.matched_text);
                assert_eq!(text.chars().skip(finding.codepoint_range.start).take(finding.codepoint_range.end - finding.codepoint_range.start).collect::<String>(), finding.matched_text);
                assert_eq!(finding.confidence, None);
                json!({"path": located.path, "label": finding.entity_type, "text": finding.matched_text, "start": finding.codepoint_range.start, "end": finding.codepoint_range.end})
            }).collect();
            assert_eq!(json!(projected), case["findings"], "{}", case["id"]);
        }
    }

    #[test]
    fn aliases_are_exact_and_do_not_classify_prose() {
        for alias in [
            "first_name",
            "firstName",
            "FirstName",
            "FIRST_NAME",
            "given_name",
            "givenName",
            "GivenName",
            "last_name",
            "lastName",
            "LastName",
            "family_name",
            "familyName",
            "FamilyName",
            "full_name",
            "fullName",
            "FullName",
            "surname",
            "Surname",
            "SURNAME",
        ] {
            assert_eq!(
                scan(&json!({alias: "May"}), &StructuredScanConfig::default())
                    .unwrap()
                    .findings
                    .len(),
                1,
                "{alias}"
            );
        }
        for alias in [
            "firstname",
            "name",
            "first-name",
            "name_of_customer",
            "first_name_backup",
            "customer.firstName",
        ] {
            assert!(
                scan(&json!({alias: "May"}), &StructuredScanConfig::default())
                    .unwrap()
                    .findings
                    .is_empty(),
                "{alias}"
            );
        }
        assert!(crate::scan(r#"{"first_name":"May"}"#).is_empty());
    }

    #[test]
    fn configuration_errors_are_strict_and_value_free() {
        for (value, path) in [
            (json!({"discover_person":null}), "/discover_person"),
            (json!({"unknown":true}), "/unknown"),
            (json!({"mappings":{"name":"PERSON"}}), "/mappings/name"),
            (
                json!({"mappings":{"/name~2":"PERSON"}}),
                "/mappings/~1name~02",
            ),
            (json!({"mappings":{"/name":"EMAIL"}}), "/mappings/~1name"),
            (
                json!({"mappings":{"/name":"PERSON"},"exclude":["/name"]}),
                "/exclude/0",
            ),
            (json!({"exclude":["/name","/name"]}), "/exclude/1"),
            (json!({"locale":null}), "/locale"),
        ] {
            let error = parse_scan_config(&value).unwrap_err();
            assert_eq!(error.code(), PrivacyErrorCode::InvalidConfiguration);
            assert_eq!(error.path(), Some(path));
        }
        for value in [
            json!(null),
            json!("secret-value"),
            json!(4),
            json!({"n":9007199254740992u64}),
        ] {
            let error = scan(&value, &StructuredScanConfig::default()).unwrap_err();
            assert_eq!(error.path(), Some("/data"));
            assert!(!error.to_string().contains("secret-value"));
        }
    }
    #[test]
    fn shared_structured_transform_fixtures() {
        for line in include_str!("../../../fixtures/structured-transform.jsonl").lines() {
            let case: Value = serde_json::from_str(line).unwrap();
            let config = parse_scan_and_transform_config(&case["config"]).unwrap();
            let result = scan_and_transform(&case["data"], &config).unwrap();
            assert_eq!(result.data, case["expected_data"], "{}", case["id"]);
            let findings = scan(&case["data"], &config.scan).unwrap().findings;
            assert_eq!(
                transform(&case["data"], &findings, &config.transform).unwrap(),
                result
            );
        }
    }

    #[test]
    fn structured_transformations_preserve_structure_and_local_ranges() {
        let data = json!({"a/b": {"full_name": "👋 José", "note": "mail a@example.test"}, "count": 3, "name": "Acme", "empty": null});
        let findings = scan(&data, &StructuredScanConfig::default())
            .unwrap()
            .findings;
        let config =
            parse_transformation_config(&json!({"default":{"strategy":"redact"}})).unwrap();
        let result = transform(&data, &findings, &config).unwrap();
        assert_eq!(
            result.data,
            json!({"a/b": {"full_name":"[PERSON]", "note":"mail [EMAIL]"}, "count":3, "name":"Acme", "empty":null})
        );
        for record in &result.transformations {
            let source = data.pointer(&record.path).unwrap().as_str().unwrap();
            let output = result.data.pointer(&record.path).unwrap().as_str().unwrap();
            let record = &record.transformation;
            assert!(
                !source[record.source_byte_range.start..record.source_byte_range.end].is_empty()
            );
            assert_eq!(
                &output[record.output_byte_range.start..record.output_byte_range.end],
                record.replacement
            );
        }
        let mask = parse_transformation_config(
            &json!({"default":{"strategy":"mask"}, "entities":["PERSON"]}),
        )
        .unwrap();
        assert_eq!(
            transform(&data, &findings, &mask).unwrap().data["a/b"]["full_name"],
            "******"
        );
        let remove = parse_transformation_config(
            &json!({"default":{"strategy":"remove"}, "entities":["PERSON"]}),
        )
        .unwrap();
        assert_eq!(
            transform(&data, &findings, &remove).unwrap().data["a/b"]["full_name"],
            ""
        );
        let allow = parse_transformation_config(&json!({"default":{"strategy":"redact"}, "allow":{"exact":{"PERSON":["👋 José"]}, "regex":{"EMAIL":[{"pattern":".+@example\\.test"}]}}})).unwrap();
        assert_eq!(transform(&data, &findings, &allow).unwrap().data, data);
    }

    #[test]
    fn malformed_later_finding_fails_before_provider_work() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Keys(AtomicUsize);
        impl KeyProvider for Keys {
            fn resolve_key(&self, _selector: KeySelector) -> KeyProviderFuture<'_> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(ResolvedKey::new(vec![7; 32], "v1")) })
            }
        }
        let data = json!({"first_name":"May", "last_name":"Chen"});
        let mut findings = scan(&data, &StructuredScanConfig::default())
            .unwrap()
            .findings;
        findings[1].finding.byte_range.end = 999;
        let config = parse_transformation_config(
            &json!({"default":{"strategy":"pseudonymize","key_ref":"names"}}),
        )
        .unwrap();
        let manager = PrivacyManager::new(Keys(AtomicUsize::new(0)));
        let error = futures::executor::block_on(
            manager.transform_structured(&data, &findings, &config, None),
        )
        .unwrap_err();
        assert_eq!(error.finding_index(), Some(1));
        assert_eq!(error.path(), Some("/findings/1/finding/byte_range"));
        assert_eq!(manager.provider().0.load(Ordering::SeqCst), 0);
        findings[1].path = "/absent".into();
        assert_eq!(
            transform(&data, &findings, &config).unwrap_err().path(),
            Some("/findings/1/path")
        );
    }

    #[test]
    fn structured_keys_are_deduplicated_across_fields() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Keys(AtomicUsize);
        impl KeyProvider for Keys {
            fn resolve_key(&self, _: KeySelector) -> KeyProviderFuture<'_> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(ResolvedKey::new(vec![3; 32], "v1")) })
            }
        }
        let data = json!({"first_name":"May", "last_name":"May"});
        let config = parse_scan_and_transform_config(
            &json!({"transform":{"default":{"strategy":"pseudonymize","key_ref":"names"}}}),
        )
        .unwrap();
        let manager = PrivacyManager::new(Keys(AtomicUsize::new(0)));
        let result = futures::executor::block_on(
            manager.scan_and_transform_structured(&data, &config, None),
        )
        .unwrap();
        assert_eq!(result.data["first_name"], result.data["last_name"]);
        assert_ne!(result.data["first_name"], "May");
        assert_eq!(manager.provider().0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn token_batches_are_document_wide_and_restore_is_deduplicated() {
        use std::sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        #[derive(Default)]
        struct Vault {
            stored: Mutex<BTreeMap<Vec<u8>, String>>,
            calls: AtomicUsize,
            restore_items: AtomicUsize,
        }
        impl TokenProvider for Vault {
            fn tokenize_batch(
                &self,
                scope: &str,
                items: Vec<TokenizeItem>,
            ) -> TokenizeProviderFuture<'_> {
                let scope = scope.to_owned();
                Box::pin(async move {
                    assert_eq!(scope, "test-scope");
                    self.calls.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(items.len(), 2);
                    let mut stored = self.stored.lock().unwrap();
                    Ok(items
                        .into_iter()
                        .enumerate()
                        .map(|(index, item)| {
                            let payload = index.to_string().into_bytes();
                            stored.insert(payload.clone(), item.exact_value.clone());
                            TokenizeResult::new(item.id, payload, "v1")
                        })
                        .collect())
                })
            }
            fn restore_batch(
                &self,
                scope: &str,
                items: Vec<RestoreItem>,
            ) -> RestoreProviderFuture<'_> {
                let scope = scope.to_owned();
                Box::pin(async move {
                    assert_eq!(scope, "test-scope");
                    self.restore_items.store(items.len(), Ordering::SeqCst);
                    let stored = self.stored.lock().unwrap();
                    Ok(items
                        .into_iter()
                        .map(|item| RestoredValue::new(item.id, stored[&item.payload].clone()))
                        .collect())
                })
            }
        }
        let data = json!({"first_name":"May", "last_name":"May"});
        let context = PrivacyContext::new("test-scope").unwrap();
        let manager = PrivacyManager::token_provider_only(Vault::default());
        let config = parse_scan_and_transform_config(
            &json!({"transform":{"default":{"strategy":"tokenize","token_ref":"names"}}}),
        )
        .unwrap();
        let result = futures::executor::block_on(manager.scan_and_transform_structured(
            &data,
            &config,
            Some(&context),
        ))
        .unwrap();
        assert_eq!(manager.token_provider().calls.load(Ordering::SeqCst), 1);
        assert_ne!(result.data["first_name"], result.data["last_name"]);
        let mut repeated = result.data.clone();
        repeated["copy"] = result.data["first_name"].clone();
        let restored =
            futures::executor::block_on(manager.restore_structured(&repeated, &context)).unwrap();
        assert_eq!(
            restored.data,
            json!({"first_name":"May", "last_name":"May", "copy":"May"})
        );
        assert_eq!(
            manager
                .token_provider()
                .restore_items
                .load(Ordering::SeqCst),
            2
        );
        assert_eq!(restored.restorations.len(), 3);
        assert!(restore_with_results(&repeated, &context, vec![]).is_err());
        assert_eq!(
            scan_and_transform(&data, &config).unwrap_err().code(),
            PrivacyErrorCode::TokenProviderRequired
        );
    }

    #[test]
    fn structured_limits_follow_json_transport_and_invalid_values_are_atomic() {
        let mut data = json!({"first_name":"May"});
        for _ in 0..127 {
            data = json!([data]);
        }
        assert!(scan(&data, &StructuredScanConfig::default()).is_err());
        assert!(parse_document_json(r#"{"first_name":"May","n":1e999}"#).is_err());
        let data = json!({"first_name":"a@example.test"});
        let findings = scan(&data, &StructuredScanConfig::default())
            .unwrap()
            .findings;
        let config = parse_transformation_config(
            &json!({"default":{"strategy":"redact"},"entities":["PERSON"]}),
        )
        .unwrap();
        assert_eq!(
            transform(&data, &findings, &config).unwrap().data["first_name"],
            "[PERSON]"
        );
    }
}

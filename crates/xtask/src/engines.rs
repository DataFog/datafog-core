//! Model engine registry - add new engines by extending ENGINE_REGISTRY.
//!
//! To add a new engine:
//! 1. Add an entry to ENGINE_REGISTRY (~15 lines)
//! 2. Create baselines/eval-models-<name>.json via `cargo dfv eval --model <name> --update-baseline`
//! 3. Add provisioning/caching steps to .github/workflows/dfv-models.yml matrix
//!
//! Engine tiers:
//! - FirstParty: F1 regression > 1pt fails, <= 1pt warns. Owned by DataFog.
//! - Community: F1 regression is warn-only, never fails PRs. Requires named owner in CODEOWNERS.
//!
//! Contract checks (blocking for all tiers):
//! - Pinned revision + sha256 verified by doctor
//! - Engine loads behind its feature/config
//! - Spans have valid offsets matching across SDKs
//! - Disabling the engine leaves core untouched

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Engine ownership tier determining accuracy gate behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineTier {
    /// First-party engine owned by DataFog. F1 drop > 1pt fails, <= 1pt warns.
    FirstParty,
    /// Community engine (external model). F1 regression is warn-only, never fails PRs.
    /// Requires a named owner in CODEOWNERS.
    Community,
}

/// Definition of a model engine for eval, check, and provisioning.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Short name used in CLI flags and baseline filenames (e.g., "datafog", "gliner2")
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Ownership tier determining accuracy gate behavior
    pub tier: EngineTier,
    /// Path to provisioning script relative to workspace root
    pub provision_script: &'static str,
    /// Entity types this engine is scored on (others are excluded from metrics)
    pub scored_entity_types: &'static [&'static str],
    /// Function to build the ModelSession config dict for Python
    pub build_config: fn(bundle_path: &Path, manifest_sha256: &str, python_bin: &Path) -> String,
}

fn datafog_config(bundle_path: &Path, manifest_sha256: &str, _python_bin: &Path) -> String {
    let bundle_str = bundle_path.to_string_lossy().replace('\\', "\\\\");
    format!(
        r#"{{"bundle": "{bundle_str}", "manifest_sha256": "{manifest_sha256}", "limits": {{"max_input_bytes": 65536, "max_response_bytes": 1048576, "max_findings": 1024, "timeout_ms": 30000, "cleanup_timeout_ms": 1000}}}}"#
    )
}

fn gliner2_config(bundle_path: &Path, manifest_sha256: &str, python_bin: &Path) -> String {
    let bundle_str = bundle_path.to_string_lossy().replace('\\', "\\\\");
    let python_str = python_bin.to_string_lossy().replace('\\', "\\\\");
    format!(
        r#"{{"bundle": "{bundle_str}", "manifest_sha256": "{manifest_sha256}", "limits": {{"max_input_bytes": 65536, "max_response_bytes": 16777216, "max_findings": 65536, "timeout_ms": 120000, "cleanup_timeout_ms": 5000}}, "gliner2": {{"python": "{python_str}", "labels": [{{"label": "street_address", "threshold": 0.5}}], "intra_threads": 1, "chunk_size": 384, "chunk_overlap": 64}}}}"#
    )
}

/// Registry of all model engines. Add new engines here.
pub static ENGINE_REGISTRY: &[EngineConfig] = &[
    // -------------------------------------------------------------------------
    // DataFog native model - first-party, F1 regression > 1pt fails PR
    // -------------------------------------------------------------------------
    EngineConfig {
        name: "datafog",
        description: "DataFog native PII model (v0.2.0)",
        tier: EngineTier::FirstParty,
        provision_script: "scripts/provision-datafog-model.py",
        scored_entity_types: &[
            "EMAIL",
            "FIRST_NAME",
            "LAST_NAME",
            "PHONE",
            "STREET_ADDRESS",
        ],
        build_config: datafog_config,
    },
    // -------------------------------------------------------------------------
    // GLiNER2 - community tier, F1 regression is warn-only (never fails PR)
    // Integration code (e.g. entity-count gating in crates/core) is still ours
    // and regression tests for it block.
    // -------------------------------------------------------------------------
    EngineConfig {
        name: "gliner2",
        description: "GLiNER2 privacy-filter PII model (community)",
        tier: EngineTier::Community,
        provision_script: "scripts/provision-gliner2-model.py",
        scored_entity_types: &["STREET_ADDRESS"],
        build_config: gliner2_config,
    },
];

impl EngineConfig {
    /// Get engine by name
    pub fn get(name: &str) -> Option<&'static EngineConfig> {
        ENGINE_REGISTRY.iter().find(|e| e.name == name)
    }

    /// List all engine names
    pub fn names() -> Vec<&'static str> {
        ENGINE_REGISTRY.iter().map(|e| e.name).collect()
    }

    /// Path to model bundle directory
    pub fn bundle_path(&self, root: &Path) -> std::path::PathBuf {
        root.join("models").join(self.name)
    }

    /// Path to baseline file
    pub fn baseline_path(&self, root: &Path) -> std::path::PathBuf {
        root.join(format!("baselines/eval-models-{}.json", self.name))
    }

    /// Check if model is provisioned
    pub fn is_provisioned(&self, root: &Path) -> bool {
        self.bundle_path(root).join("bundle-manifest.json").exists()
    }

    /// Get scored entity types as HashSet
    pub fn scored_types_set(&self) -> HashSet<String> {
        self.scored_entity_types
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Get manifest info (sha256, model_id, revision)
    pub fn manifest_info(&self, root: &Path) -> Option<ManifestInfo> {
        let bundle_path = self.bundle_path(root);
        let manifest_path = bundle_path.join("bundle-manifest.json");
        let content = std::fs::read_to_string(&manifest_path).ok()?;

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let sha256 = format!("{:x}", hasher.finalize());

        let manifest: serde_json::Value = serde_json::from_str(&content).ok()?;
        let from_manifest = manifest.get("model").map(|model| {
            (
                model.get("id").and_then(|v| v.as_str()).map(String::from),
                model
                    .get("revision")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            )
        });
        let (model_id, revision) = if let Some((id, rev)) = from_manifest {
            (id, rev)
        } else if self.name == "gliner2" {
            (
                Some("fastino/gliner2-privacy-filter-PII-multi".to_string()),
                Some("1cb4166094dc58fa8d836429f060d6c95f62b495".to_string()),
            )
        } else {
            let config_path = bundle_path.join("config.json");
            let config_content = std::fs::read_to_string(&config_path).ok();
            let config: Option<serde_json::Value> =
                config_content.and_then(|c| serde_json::from_str(&c).ok());
            let rev = config
                .as_ref()
                .and_then(|c| c.get("model_revision"))
                .and_then(|v| v.as_str())
                .map(String::from);
            (Some("DataFog/pii-en-65m".to_string()), rev)
        };

        Some(ManifestInfo {
            sha256,
            model_id,
            revision,
        })
    }

    /// Scan many texts with one ModelSession so the worker loads once.
    pub fn scan_many(
        &self,
        texts: &[String],
        python_bin: &Path,
        root: &Path,
    ) -> Option<Vec<Vec<DetectedEntity>>> {
        let manifest_info = self.manifest_info(root)?;
        let bundle_path = self.bundle_path(root);
        let config = (self.build_config)(&bundle_path, &manifest_info.sha256, python_bin);
        let payload = serde_json::to_string(texts).ok()?;

        let code = format!(
            r#"
import json, sys
from datafog_core import ModelSession
config = {config}
texts = json.load(sys.stdin)
with ModelSession.open(config) as session:
    out = []
    for text in texts:
        result = session.scan_model(text)
        out.append([dict(entity_type=f.entity_type, matched_text=f.matched_text,
            codepoint_start=f.codepoint_range.start, codepoint_end=f.codepoint_range.end) for f in result.findings])
    print(json.dumps(out))
"#
        );

        let output = Command::new(python_bin)
            .args(["-c", &code])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(payload.as_bytes())?;
                }
                child.wait_with_output()
            })
            .ok()?;

        if !output.status.success() {
            eprintln!(
                "{} scan error: {}",
                self.name,
                String::from_utf8_lossy(&output.stderr)
            );
            return None;
        }

        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).ok()
    }
}

#[derive(Debug, Clone)]
pub struct ManifestInfo {
    pub sha256: String,
    pub model_id: Option<String>,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedEntity {
    pub entity_type: String,
    pub matched_text: String,
    pub codepoint_start: usize,
    pub codepoint_end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_engines() {
        assert!(!ENGINE_REGISTRY.is_empty());
        assert!(EngineConfig::get("datafog").is_some());
        assert!(EngineConfig::get("gliner2").is_some());
    }

    #[test]
    fn engine_names_unique() {
        let names = EngineConfig::names();
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len());
    }
}

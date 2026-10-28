use crate::report::workspace_root;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ToolchainConfig {
    pub node_version: String,
    pub rust_version: String,
    pub wasm_bindgen_version: String,
    pub playwright_version: String,
}

impl ToolchainConfig {
    pub fn from_dockerfile(dockerfile_path: &Path) -> Option<Self> {
        let content = fs::read_to_string(dockerfile_path).ok()?;
        let mut args = parse_dockerfile_args(&content);

        Some(ToolchainConfig {
            node_version: args.remove("NODE_VERSION")?,
            rust_version: args.remove("RUST_VERSION")?,
            wasm_bindgen_version: args.remove("WASM_BINDGEN_VERSION")?,
            playwright_version: args.remove("PLAYWRIGHT_VERSION")?,
        })
    }

    pub fn from_workspace() -> Option<Self> {
        let root = workspace_root();
        let dockerfile = root.join(".cursor").join("Dockerfile");
        Self::from_dockerfile(&dockerfile)
    }
}

fn parse_dockerfile_args(content: &str) -> HashMap<String, String> {
    let mut args = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("ARG ") {
            if let Some((key, value)) = rest.split_once('=') {
                args.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    args
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionMatch {
    Exact,
    Mismatch,
}

pub fn parse_version(version: &str) -> (u32, u32, u32) {
    let version = version.trim_start_matches('v');
    let parts: Vec<&str> = version.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn compare_node_version(found: &str, required: &str) -> VersionMatch {
    let (found_major, _, _) = parse_version(found);
    let (required_major, _, _) = parse_version(required);

    if found_major == required_major {
        VersionMatch::Exact
    } else {
        VersionMatch::Mismatch
    }
}

pub fn compare_rust_version(found: &str, required: &str) -> VersionMatch {
    let (found_major, found_minor, _) = parse_version(found);
    let (req_major, req_minor, _) = parse_version(required);

    if found_major == req_major && found_minor == req_minor {
        VersionMatch::Exact
    } else {
        VersionMatch::Mismatch
    }
}

pub fn compare_exact_version(found: &str, required: &str) -> VersionMatch {
    let found = found.split_whitespace().last().unwrap_or(found);
    let required = required.split_whitespace().last().unwrap_or(required);

    if found.trim() == required.trim() {
        VersionMatch::Exact
    } else {
        VersionMatch::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("24"), (24, 0, 0));
        assert_eq!(parse_version("v22.23.4"), (22, 23, 4));
        assert_eq!(parse_version("1.88.0"), (1, 88, 0));
        assert_eq!(parse_version("0.2.127"), (0, 2, 127));
    }

    #[test]
    fn test_compare_node_version() {
        assert_eq!(compare_node_version("v24.1.0", "24"), VersionMatch::Exact);
        assert_eq!(
            compare_node_version("v22.23.4", "24"),
            VersionMatch::Mismatch
        );
        assert_eq!(compare_node_version("24.0.0", "24"), VersionMatch::Exact);
    }

    #[test]
    fn test_compare_rust_version() {
        assert_eq!(
            compare_rust_version("1.88.0", "1.88.0"),
            VersionMatch::Exact
        );
        assert_eq!(
            compare_rust_version("1.88.1", "1.88.0"),
            VersionMatch::Exact
        );
        assert_eq!(
            compare_rust_version("1.87.0", "1.88.0"),
            VersionMatch::Mismatch
        );
    }

    #[test]
    fn test_compare_exact_version() {
        assert_eq!(
            compare_exact_version("wasm-bindgen 0.2.127", "0.2.127"),
            VersionMatch::Exact
        );
        assert_eq!(
            compare_exact_version("wasm-bindgen 0.2.126", "0.2.127"),
            VersionMatch::Mismatch
        );
    }

    #[test]
    fn test_parse_dockerfile_args() {
        let content = r#"FROM ubuntu:24.04

ARG RUST_VERSION=1.88.0
ARG NODE_VERSION=24
ARG WASM_BINDGEN_VERSION=0.2.127
ARG PLAYWRIGHT_VERSION=1.62.1

RUN apt-get update
"#;
        let args = parse_dockerfile_args(content);
        assert_eq!(args.get("RUST_VERSION"), Some(&"1.88.0".to_string()));
        assert_eq!(args.get("NODE_VERSION"), Some(&"24".to_string()));
        assert_eq!(
            args.get("WASM_BINDGEN_VERSION"),
            Some(&"0.2.127".to_string())
        );
        assert_eq!(args.get("PLAYWRIGHT_VERSION"), Some(&"1.62.1".to_string()));
    }

    #[test]
    fn test_toolchain_config_from_dockerfile() {
        let content = r#"ARG RUST_VERSION=1.88.0
ARG NODE_VERSION=24
ARG WASM_BINDGEN_VERSION=0.2.127
ARG PLAYWRIGHT_VERSION=1.62.1
"#;
        let temp = std::env::temp_dir().join("test_dockerfile");
        std::fs::write(&temp, content).unwrap();
        let config = ToolchainConfig::from_dockerfile(&temp).unwrap();
        assert_eq!(config.node_version, "24");
        assert_eq!(config.rust_version, "1.88.0");
        assert_eq!(config.wasm_bindgen_version, "0.2.127");
        assert_eq!(config.playwright_version, "1.62.1");
        std::fs::remove_file(&temp).ok();
    }
}

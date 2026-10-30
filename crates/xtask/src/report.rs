use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub timestamp_utc: String,
    pub git_commit_sha: Option<String>,
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub tool_versions: ToolVersions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolVersions {
    pub rustc: Option<String>,
    pub cargo: Option<String>,
    pub python: Option<String>,
    pub node: Option<String>,
    pub maturin: Option<String>,
    pub wasm_bindgen: Option<String>,
}

impl RunMetadata {
    pub fn collect() -> Self {
        Self {
            timestamp_utc: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            git_commit_sha: get_git_sha(),
            git_branch: get_git_branch(),
            git_dirty: is_git_dirty(),
            tool_versions: ToolVersions::collect(),
        }
    }
}

impl ToolVersions {
    pub fn collect() -> Self {
        Self {
            rustc: get_version("rustc", &["--version"]),
            cargo: get_version("cargo", &["--version"]),
            python: get_version("python3", &["--version"]),
            node: get_version("node", &["--version"]),
            maturin: get_version("python3", &["-m", "maturin", "--version"])
                .or_else(|| get_version("maturin", &["--version"])),
            wasm_bindgen: get_version("wasm-bindgen", &["--version"]),
        }
    }
}

fn get_version(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn get_git_sha() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn get_git_branch() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn is_git_dirty() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

pub fn workspace_root() -> PathBuf {
    // A shared Cargo target can reuse a binary compiled in another worktree.
    // Resolve the caller's checkout first so source metadata and tested inputs
    // cannot silently refer to different checkouts.
    if let Ok(current) = std::env::current_dir() {
        if let Some(root) = discover_workspace_root(&current) {
            return root;
        }
    }
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn discover_workspace_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|root| {
            root.join("Cargo.toml").is_file()
                && root.join("crates/core/Cargo.toml").is_file()
                && root.join("crates/xtask/Cargo.toml").is_file()
        })
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod workspace_tests {
    use super::discover_workspace_root;
    use std::fs;

    #[test]
    fn current_nested_worktree_wins_over_the_outer_checkout() {
        let path = std::env::temp_dir().join(format!(
            "dfv-worktree-root-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let nested = path.join(".worktrees/feature");
        for root in [&path, &nested] {
            for relative in [
                "Cargo.toml",
                "crates/core/Cargo.toml",
                "crates/xtask/Cargo.toml",
            ] {
                let file = root.join(relative);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, "synthetic workspace sentinel").unwrap();
            }
        }
        let caller = nested.join("integrations/example");
        fs::create_dir_all(&caller).unwrap();
        assert_eq!(discover_workspace_root(&caller), Some(nested));
        assert_eq!(discover_workspace_root(&path), Some(path.clone()));
        fs::remove_dir_all(path).unwrap();
    }
}

pub fn create_run_directory() -> std::io::Result<PathBuf> {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let run_dir = workspace_root()
        .join("target")
        .join("dfv")
        .join("runs")
        .join(&timestamp);
    fs::create_dir_all(&run_dir)?;

    let metadata = RunMetadata::collect();
    let metadata_path = run_dir.join("metadata.json");
    if let Ok(json) = serde_json::to_string_pretty(&metadata) {
        let _ = fs::write(&metadata_path, json);
    }

    Ok(run_dir)
}

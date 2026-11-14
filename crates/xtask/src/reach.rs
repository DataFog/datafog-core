//! Reachability guard: every new crate or Python test module must be executed
//! by `cargo dfv check`, either directly or through an explicit exemption.
//!
//! Without this, a nested workspace or a new test file compiles and passes in
//! isolation while the aggregate verification stays green and never runs it.
use std::fs;
use std::path::Path;

/// Crate directories that build outside the root workspace on purpose. Each
/// one must still be reachable through a dedicated `cargo dfv` subcommand.
const NESTED_WORKSPACES: &[&str] = &["crates/core/fuzz"];

/// Python test modules that `test_installed.py` does not import on purpose.
const PY_TEST_EXEMPTIONS: &[&str] = &[
    // The aggregate runner itself.
    "test_installed",
    // Type-level checks that need mypy and pyright; CI runs them separately.
    "test_typing",
    // Opt-in 0.4.1 gate that requires a real `datafog` 4.9 adapter install.
    "test_python49_integration",
];

fn collect_crate_manifests(root: &Path) -> Vec<String> {
    let mut manifests = Vec::new();
    for top in ["crates", "bindings"] {
        let Ok(entries) = fs::read_dir(root.join(top)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.join("Cargo.toml").is_file() {
                manifests.push(format!("{top}/{}", entry.file_name().to_string_lossy()));
            }
            // Bindings and crates can nest deeper (for example crates/core).
            if let Ok(nested) = fs::read_dir(&path) {
                for inner in nested.flatten() {
                    let inner_path = inner.path();
                    if inner_path.is_dir() && inner_path.join("Cargo.toml").is_file() {
                        manifests.push(format!(
                            "{top}/{}/{}",
                            entry.file_name().to_string_lossy(),
                            inner.file_name().to_string_lossy()
                        ));
                    }
                }
            }
        }
    }
    manifests.sort();
    manifests.dedup();
    manifests
}

fn workspace_members(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let mut members = Vec::new();
    let mut in_members = false;
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if line.trim() == "members = [" {
            in_members = true;
            continue;
        }
        if in_members {
            if line.trim_start() == "]" {
                break;
            }
            let entry = line
                .trim()
                .trim_end_matches(',')
                .trim()
                .trim_matches('"')
                .trim_end_matches("/*");
            if !entry.is_empty() {
                members.push(entry.to_string());
            }
        }
    }
    members
}

fn python_test_modules(root: &Path) -> Vec<String> {
    let tests_dir = root.join("bindings/python/tests");
    let Ok(entries) = fs::read_dir(tests_dir) else {
        return Vec::new();
    };
    let mut modules = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("test_") && name.ends_with(".py") {
            modules.push(name.trim_end_matches(".py").to_string());
        }
    }
    modules.sort();
    modules
}

fn imported_test_modules(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("bindings/python/tests/test_installed.py")) else {
        return Vec::new();
    };
    let mut imported = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line
            .strip_prefix("import ")
            .or_else(|| line.strip_prefix("from "))
        {
            let name = name.split_whitespace().next().unwrap_or(name);
            if name.starts_with("test_") {
                imported.push(name.to_string());
            }
        }
    }
    imported
}

/// Returns one human-readable failure per unreachable unit, empty when
/// everything is covered.
pub fn check_reachability(root: &Path) -> Vec<String> {
    let mut failures = Vec::new();

    let members = workspace_members(root);
    for manifest in collect_crate_manifests(root) {
        // Exact matching: a nested directory such as crates/core/fuzz is not
        // built by workspace commands even though its parent is a member.
        let covered = members.contains(&manifest);
        if !covered && !NESTED_WORKSPACES.contains(&manifest.as_str()) {
            failures.push(format!(
                "crate `{manifest}` is not a workspace member and is not in the nested-workspace \
                 exemption list; add it to the root workspace or to crates/xtask/src/reach.rs so \
                 a dfv subcommand executes it"
            ));
        }
    }

    let imported = imported_test_modules(root);
    for module in python_test_modules(root) {
        if !imported.contains(&module) && !PY_TEST_EXEMPTIONS.contains(&module.as_str()) {
            failures.push(format!(
                "python test module `{module}` is not imported by \
                 bindings/python/tests/test_installed.py, so `cargo dfv check` never runs it; \
                 import it there or add it to the exemption list in crates/xtask/src/reach.rs"
            ));
        }
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::workspace_root;

    #[test]
    fn the_fuzz_crate_is_the_only_nested_workspace() {
        let root = workspace_root();
        for manifest in collect_crate_manifests(&root) {
            let covered = workspace_members(&root).contains(&manifest);
            assert!(
                covered || NESTED_WORKSPACES.contains(&manifest.as_str()),
                "unexpected uncovered crate manifest: {manifest}"
            );
        }
    }
}

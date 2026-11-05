//! Managed Python environments for installed-package verification.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn python_bin(venv_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        venv_dir.join("Scripts/python.exe")
    } else {
        venv_dir.join("bin/python")
    }
}

fn ready(python: &Path, venv_dir: &Path) -> Result<(), String> {
    checked(Command::new(python)
        .args(["-I", "-c", "import pathlib, sys, pip; assert sys.prefix != sys.base_prefix; assert pathlib.Path(sys.prefix).resolve() == pathlib.Path(sys.argv[1]).resolve()"])
        .arg(venv_dir), "Python environment readiness")
}

fn checked(command: &mut Command, stage: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("{stage}: could not start process: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{stage}: process failed ({})\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn ensure(venv_dir: &Path) -> Result<PathBuf, String> {
    let python = python_bin(venv_dir);
    let metadata = fs::symlink_metadata(venv_dir);
    let linked_directory = metadata
        .as_ref()
        .is_ok_and(|entry| entry.file_type().is_symlink());
    if linked_directory || ready(&python, venv_dir).is_err() {
        // Cached links may reference a removed runner toolchain. Recreating over
        // those links is insufficient: clear only this managed environment first.
        match metadata {
            Ok(metadata) => {
                let result = if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    fs::remove_file(venv_dir)
                } else {
                    fs::remove_dir_all(venv_dir)
                };
                result.map_err(|error| format!("Python environment cleanup: {error}"))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Python environment inspection: {error}")),
        }
        checked(
            Command::new("python3").args(["-m", "venv"]).arg(venv_dir),
            "Python environment creation",
        )?;
        ready(&python, venv_dir)?;
    }
    Ok(python)
}

/// Ambiguous cached wheels cannot establish which installed candidate was checked.
pub fn built_wheel(root: &Path) -> Result<PathBuf, String> {
    let directory = root.join("target/wheels");
    let mut wheels = fs::read_dir(&directory)
        .map_err(|error| format!("Python wheel selection: built wheel directory is missing or unreadable: {error}; run cargo dfv build --sdk python"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Python wheel selection: cannot read candidate directory: {error}"))?;
    wheels.retain(|path| path.extension().is_some_and(|extension| extension == "whl"));
    if wheels.len() != 1 {
        return Err(format!(
            "Python wheel selection: expected exactly one built wheel, found {}; clean target/wheels and run cargo dfv build --sdk python",
            wheels.len()
        ));
    }
    Ok(wheels.remove(0))
}

/// Reuse only a runnable managed environment and always refresh the built wheel.
pub fn prepare(venv_dir: &Path, wheel: &Path) -> Result<PathBuf, String> {
    let python = ensure(venv_dir)?;
    checked(
        Command::new(&python)
            .args([
                "-m",
                "pip",
                "--disable-pip-version-check",
                "install",
                "--force-reinstall",
                "-q",
            ])
            .arg(wheel),
        "Python wheel installation",
    )?;
    Ok(python)
}

/// Resolve optional requirements without forcing a second copy of healthy dependencies.
pub fn install_extras(python: &Path, wheel: &Path, extras: &[&str]) -> Result<(), String> {
    for extra in extras {
        checked(
            Command::new(python)
                .args(["-m", "pip", "--disable-pip-version-check", "install", "-q"])
                .arg(format!("{}[{extra}]", wheel.display())),
            "Python optional dependency installation",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    fn eval_test_root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dfv-python-env-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    #[cfg(unix)]
    fn invalid_cached_venv_recreates_removed_interpreter_links() {
        use std::os::unix::fs::symlink;
        let root = eval_test_root("broken-interpreter");
        let venv = root.join("venv");
        fs::create_dir_all(venv.join("bin")).unwrap();
        let executable = Command::new("python3")
            .args([
                "-I",
                "-c",
                "import os,sys; print(os.path.basename(sys.executable))",
            ])
            .output()
            .unwrap();
        assert!(executable.status.success());
        let basename = String::from_utf8(executable.stdout).unwrap();
        let names: HashSet<_> = ["python", "python3", basename.trim()].into_iter().collect();
        for name in names {
            symlink(
                root.join("removed-toolchain-python"),
                venv.join("bin").join(name),
            )
            .unwrap();
        }
        fs::write(venv.join("stale-marker"), "cached").unwrap();
        // This is the CI failure: creating over the cache retains the broken
        // primary interpreter link and ensurepip cannot start that interpreter.
        let old_setup = Command::new("python3")
            .args(["-m", "venv"])
            .arg(&venv)
            .output()
            .unwrap();
        assert!(
            !old_setup.status.success(),
            "invalid cache must reproduce setup failure"
        );
        let python = ensure(&venv).expect("invalid managed cache should be rebuilt");
        assert!(ready(&python, &venv).is_ok());
        assert!(!venv.join("stale-marker").exists());
        // A healthy cache is retained rather than reinstalling its runtime.
        fs::write(venv.join("healthy-marker"), "keep").unwrap();
        assert_eq!(ensure(&venv).unwrap(), python);
        assert!(venv.join("healthy-marker").exists());
        // Existence alone is also insufficient when the cached executable can
        // start but cannot run Python (for example, a relocated tool wrapper).
        use std::os::unix::fs::PermissionsExt;
        fs::remove_file(&python).unwrap();
        fs::write(&python, "#!/bin/sh\nexit 19\n").unwrap();
        fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(python.exists());
        assert!(ready(&python, &venv).is_err());
        let rebuilt = ensure(&venv).unwrap();
        assert!(ready(&rebuilt, &venv).is_ok());
        assert!(!venv.join("healthy-marker").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wheel_selection_rejects_missing_and_ambiguous_candidates() {
        let root = eval_test_root("wheel-selection");
        assert!(
            built_wheel(&root)
                .unwrap_err()
                .contains("missing or unreadable")
        );
        let directory = root.join("target/wheels");
        fs::create_dir_all(&directory).unwrap();
        assert!(built_wheel(&root).unwrap_err().contains("found 0"));
        fs::write(directory.join("README"), "not a candidate").unwrap();
        let first = directory.join("first.whl");
        fs::write(&first, "first").unwrap();
        assert_eq!(built_wheel(&root).unwrap(), first);
        fs::write(directory.join("second.whl"), "second").unwrap();
        assert!(built_wheel(&root).unwrap_err().contains("found 2"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wheel_installation_failure_retains_pip_diagnostics() {
        let root = eval_test_root("install-error");
        let wheel = root.join("missing_candidate-1.0-py3-none-any.whl");
        let error = prepare(&root.join("venv"), &wheel).unwrap_err();
        assert!(error.contains("Python wheel installation"));
        assert!(error.contains("stderr:"));
        assert!(error.contains("ERROR:"));
        assert!(error.contains("missing_candidate"));
        fs::remove_dir_all(root).unwrap();
    }
}

use std::process::Command;

#[test]
fn dry_run_requires_no_runtime_and_never_claims_qualification() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["check", "--core-sdk", "--json", "--dry-run"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(output.status.success());
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["pass"], false);
    assert_eq!(summary["executed"], false);
    assert_eq!(summary["dry_run"], true);
}

#[test]
fn qualification_cannot_be_reduced_or_mixed_with_another_gate() {
    for flag in [
        "--sdk",
        "--models",
        "--fogvault",
        "--fogvault-languages",
        "--sample",
        "--unknown",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["check", "--core-sdk", "--json", "--dry-run", flag])
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{flag}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("accepts"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn missing_runtime_cannot_report_a_passing_execution() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["check", "--core-sdk", "--json"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Environment is not ready"));
}

use std::process::Command;
#[test]
fn missing_runtime_cannot_qualify_workspace_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["parity", "--workspace", "--json"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["pass"], false);
}
#[test]
fn workspace_matrix_cannot_be_reduced_to_partial_sdk_or_fixture_sample() {
    for extra in [vec!["--sdk", "wasm"], vec!["--sample", "1"], vec!["--live"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["parity", "--workspace", "--json"])
            .args(extra)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["pass"], false);
    }
}
#[test]
fn explicit_dry_run_never_claims_parity_pass() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["parity", "--workspace", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["pass"], false);
    assert_eq!(report["executed"], false);
}

use std::process::Command;

#[test]
fn live_cli_exits_nonzero_when_sdk_runtimes_cannot_start() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["parity", "--live", "--sample", "1", "--json"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "unavailable runtimes must fail the CLI"
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["pass"], false);
    assert_eq!(report["sdks_compared"], serde_json::json!([]));
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["sdk"] == "node")
    );
}

#[test]
fn parity_cli_rejects_invalid_or_empty_samples() {
    for value in [None, Some("0"), Some("-1"), Some("invalid")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command.args(["parity", "--live", "--json", "--sample"]);
        if let Some(value) = value {
            command.arg(value);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["pass"], false);
        assert_eq!(report["error"], "sample must be a positive integer");
    }
}

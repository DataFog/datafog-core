//! Optional cross-product checks reuse Core's environment gate and report owner.
use crate::doctor::run_doctor_internal;
use crate::report::workspace_root;
use std::process::{Command, ExitCode};

pub fn run(args: &[String], prepare: bool) -> ExitCode {
    if !prepare && !run_doctor_internal().pass {
        eprintln!("Environment is not ready; run cargo dfv doctor and apply its repairs.");
        return ExitCode::FAILURE;
    }
    let script = if prepare {
        "scripts/prepare-fogvault-integration.py"
    } else {
        "scripts/verify-fogvault-integration.py"
    };
    let mut command = Command::new("python3");
    command.arg(script).current_dir(workspace_root());
    for arg in args.iter().filter(|arg| arg.as_str() != "--fogvault") {
        command.arg(arg);
    }
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Cannot execute FogVault integration verification: {error}");
            ExitCode::FAILURE
        }
    }
}

//! Browser-first detector/storage qualification through installed packages.
use crate::doctor::run_doctor_internal;
use crate::report::{create_run_directory, workspace_root};
use std::fs;
use std::process::{Command, ExitCode};

pub fn selected(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--cnn" | "--sqlite-browser" | "--browser-v1-1"
        )
    })
}

pub fn run(args: &[String], prepare: bool) -> ExitCode {
    let scopes: Vec<_> = args
        .iter()
        .filter(|arg| {
            matches!(
                arg.as_str(),
                "--cnn" | "--sqlite-browser" | "--browser-v1-1"
            )
        })
        .collect();
    if scopes.len() != 1
        || args.iter().any(|arg| {
            !matches!(
                arg.as_str(),
                "--cnn" | "--sqlite-browser" | "--browser-v1-1" | "--json"
            )
        })
    {
        eprintln!(
            "Select exactly one browser scope: --cnn, --sqlite-browser or --browser-v1-1, with optional --json."
        );
        return ExitCode::FAILURE;
    }
    let scope = scopes[0].as_str();
    if !prepare && !run_doctor_internal().pass {
        eprintln!("Environment is not ready; run cargo dfv doctor and apply its repairs.");
        return ExitCode::FAILURE;
    }
    let mut commands: Vec<(&str, Vec<&str>)> = Vec::new();
    if prepare {
        commands.push(("cargo", vec!["dfv", "build", "--sdk", "wasm"]));
        if scope != "--cnn" {
            commands.push(("python3", vec!["scripts/provision-sqlite-browser.py"]));
        }
    } else {
        if scope != "--sqlite-browser" {
            commands.push((
                "node",
                vec!["--test", "scripts/test-browser-workspace-coordinator.mjs"],
            ));
            commands.push(("node", vec!["scripts/test-cnn-extension.mjs"]));
        }
        if scope != "--cnn" {
            commands.push((
                "node",
                vec!["integrations/sqlite-browser/test/run-installed.mjs"],
            ));
        }
        if scope == "--browser-v1-1" {
            commands.push(("node", vec!["scripts/test-v1-1-browser.mjs"]));
        }
    }
    let directory = match create_run_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Cannot retain browser evidence: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut results = Vec::new();
    let mut pass = true;
    for (index, (program, arguments)) in commands.iter().enumerate() {
        let output = match Command::new(program)
            .args(arguments)
            .current_dir(workspace_root())
            .output()
        {
            Ok(output) => output,
            Err(error) => {
                eprintln!("Cannot execute browser command: {error}");
                return ExitCode::FAILURE;
            }
        };
        let mut log = output.stdout.clone();
        log.extend_from_slice(&output.stderr);
        let log_path = directory.join(format!("browser-{index}.log"));
        if let Err(error) = fs::write(&log_path, log) {
            eprintln!("Cannot retain browser log: {error}");
            return ExitCode::FAILURE;
        }
        let success = output.status.success();
        results.push(serde_json::json!({"program": program, "args": arguments, "pass": success, "log": log_path}));
        if !success {
            pass = false;
            eprintln!("Browser command failed; see {}", log_path.display());
            break;
        }
    }
    let summary = serde_json::json!({"scope": scope, "prepare": prepare, "executed": true, "pass": pass, "commands": results});
    match serde_json::to_vec_pretty(&summary)
        .map_err(std::io::Error::other)
        .and_then(|bytes| fs::write(directory.join("browser.json"), bytes))
    {
        Ok(()) => println!("{summary}"),
        Err(error) => {
            eprintln!("Cannot retain browser summary: {error}");
            return ExitCode::FAILURE;
        }
    }
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

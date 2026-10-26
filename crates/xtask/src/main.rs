mod bench;
mod build;
mod check;
mod doctor;
mod engines;
mod eval;
mod models;
mod parity;
mod report;
mod smoke;

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let json_output = args.iter().any(|a| a == "--json");
    let cmd = &args[0];

    match cmd.as_str() {
        "doctor" => doctor::run_doctor(&args[1..], json_output),
        "build" => build::run_build(&args[1..], json_output),
        "check" => {
            if args[1..].iter().any(|a| a == "--models") {
                models::run_models_check(&args[1..], json_output)
            } else {
                check::run_check(&args[1..], json_output)
            }
        }
        "eval" => eval::run_eval(&args[1..], json_output),
        "bench" => bench::run_bench(&args[1..], json_output),
        "smoke" => smoke::run_smoke(&args[1..], json_output),
        "parity" => parity::run_parity(&args[1..], json_output),
        "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Unknown command: {cmd}");
            eprintln!("Run `cargo dfv --help` for usage.");
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    println!(
        r#"datafog-core verification CLI

USAGE:
    cargo dfv <COMMAND> [OPTIONS]

COMMANDS:
    doctor    Check environment readiness (toolchains, bindings, models)
    build     Build SDK bindings
    check     Run rule-based detector tests across all SDKs
    eval      Compute precision/recall/F1 against committed baselines
    bench     Measure median latency against committed baselines
    smoke     Install and test built packages as a user would
    parity    Cross-SDK span/offset comparison

OPTIONS:
    --json           Machine-readable JSON output
    --dry-run        Print commands without executing

CHECK OPTIONS:
    --models         Run model-based detector tier (GLiNER2/DataFog)
                     Wraps: scripts/provision-*.py, scripts/model-*.py

EVAL OPTIONS:
    --models           Evaluate model detectors (F1 drop > 1pt fails, < 1pt warns)
    --update-baseline  Write current metrics as the new baseline

BENCH OPTIONS:
    --runs N           Number of benchmark runs (default: 5)
    --iterations N     Iterations per run (default: 100)
    --update-baseline  Write current median as the new baseline

SMOKE OPTIONS:
    --sdk <name>       Test only one SDK (python, node, wasm)

PARITY OPTIONS:
    --live             Run actual scans through installed SDKs and diff outputs

EXAMPLES:
    cargo dfv doctor                   # Check if environment is ready
    cargo dfv doctor --json            # JSON output for automation
    cargo dfv build                    # Build all bindings
    cargo dfv build --sdk python       # Build only Python binding
    cargo dfv check                    # Run all rule-based tests
    cargo dfv check --models           # Run model tier checks
    cargo dfv eval                     # Precision/recall/F1 for rules
    cargo dfv eval --models            # Precision/recall/F1 for models
    cargo dfv bench                    # Latency benchmark
    cargo dfv bench --update-baseline  # Commit new baseline
    cargo dfv smoke                    # Install and test all packages
    cargo dfv parity                   # Fixture offset consistency
    cargo dfv parity --live            # Cross-SDK actual output diff

Reports are written to target/dfv/runs/<timestamp>/ and never deleted by this CLI.
Each run directory includes metadata.json with git commit SHA, branch, and tool versions."#
    );
}

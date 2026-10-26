mod build;
mod check;
mod doctor;
mod parity;
mod report;

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
        "check" => check::run_check(&args[1..], json_output),
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
    parity    Cross-SDK span/offset comparison

OPTIONS:
    --json           Machine-readable JSON output
    --dry-run        Print commands without executing (build, check, parity)
    --models         (check only) Include model-based detectors [NOT YET IMPLEMENTED]

EXAMPLES:
    cargo dfv doctor              # Check if environment is ready
    cargo dfv doctor --json       # JSON output for automation
    cargo dfv build               # Build all bindings
    cargo dfv build --sdk python  # Build only Python binding
    cargo dfv build --dry-run     # Show build commands without running
    cargo dfv check               # Run all rule-based tests
    cargo dfv check --json        # Machine-readable test results
    cargo dfv check --dry-run     # Show test commands without running
    cargo dfv parity              # Compare spans across SDKs
    cargo dfv parity --dry-run    # Show parity check steps

Reports are written to target/dfv/runs/<timestamp>/ and never deleted by this CLI.
Each run directory includes metadata.json with git commit SHA, branch, and tool versions."#
    );
}

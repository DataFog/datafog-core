//! `cargo dfv reports`: inspect or prune the verification run history.
//!
//! Runs are still written to `target/dfv/runs/<timestamp>/` on every command
//! and are never deleted automatically; pruning is opt-in.
use crate::report::workspace_root;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Deserialize)]
struct RunMetadata {
    timestamp_utc: String,
    #[serde(default)]
    git_branch: Option<String>,
    #[serde(default)]
    git_dirty: bool,
}

struct RunDirectory {
    path: PathBuf,
    summary: String,
}

fn collect_runs(root: &std::path::Path) -> Vec<RunDirectory> {
    let runs_dir = root.join("target/dfv/runs");
    let Ok(entries) = fs::read_dir(&runs_dir) else {
        return Vec::new();
    };
    let mut runs: Vec<RunDirectory> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            let path = entry.path();
            let summary = fs::read_to_string(path.join("metadata.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<RunMetadata>(&text).ok())
                .map(|metadata| {
                    format!(
                        "{} {}{}",
                        metadata.timestamp_utc,
                        metadata.git_branch.as_deref().unwrap_or("?"),
                        if metadata.git_dirty { " (dirty)" } else { "" }
                    )
                })
                .unwrap_or_else(|| "no metadata".to_owned());
            RunDirectory { path, summary }
        })
        .collect();
    // Directory names are UTC timestamps, so a name sort is a time sort.
    runs.sort_by(|left, right| left.path.cmp(&right.path));
    runs
}

pub fn run_reports(args: &[String], json_output: bool) -> ExitCode {
    let prune_keep = args
        .iter()
        .position(|a| a == "--prune-keep")
        .and_then(|position| args.get(position + 1))
        .and_then(|value| value.parse::<usize>().ok());

    let root = workspace_root();
    let runs = collect_runs(&root);

    if let Some(keep) = prune_keep {
        if runs.len() <= keep {
            if json_output {
                println!("{{\"pruned\": 0, \"remaining\": {}}}", runs.len());
            } else {
                println!(
                    "{} run(s) on disk; nothing to prune at --prune-keep {keep}",
                    runs.len()
                );
            }
            return ExitCode::SUCCESS;
        }
        let remove_count = runs.len() - keep;
        for run in &runs[..remove_count] {
            if let Err(error) = fs::remove_dir_all(&run.path) {
                eprintln!("could not remove {}: {error}", run.path.display());
                return ExitCode::FAILURE;
            }
            if !json_output {
                println!("removed {}", run.path.display());
            }
        }
        if json_output {
            println!("{{\"pruned\": {remove_count}, \"remaining\": {keep}}}");
        } else {
            println!("kept the newest {keep} run(s)");
        }
        return ExitCode::SUCCESS;
    }

    if json_output {
        let items: Vec<String> = runs
            .iter()
            .map(|run| {
                format!(
                    "{{\"path\": {:?}, \"summary\": {:?}}}",
                    run.path.display().to_string(),
                    run.summary
                )
            })
            .collect();
        println!("[{}]", items.join(", "));
    } else if runs.is_empty() {
        println!("no runs recorded under target/dfv/runs");
    } else {
        for run in &runs {
            println!("{}  {}", run.path.display(), run.summary);
        }
        println!(
            "\n{} run(s); `cargo dfv reports --prune-keep <N>` removes all but the newest N",
            runs.len()
        );
    }
    ExitCode::SUCCESS
}

use datafog_scan_core::scan;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, BufRead, Write};
use std::time::Instant;

#[derive(Deserialize)]
struct InputRecord {
    id: String,
    text: String,
}

#[derive(Serialize)]
struct OutputEntity {
    label: String,
    text: String,
    start: usize,
    end: usize,
}

#[derive(Serialize)]
struct OutputRecord {
    id: String,
    entities: Vec<OutputEntity>,
    durations_ns: Vec<u128>,
}

fn parse_usize_argument(name: &str) -> usize {
    let mut arguments = env::args().skip(1);

    while let Some(argument) = arguments.next() {
        if argument == name {
            return arguments
                .next()
                .unwrap_or_else(|| panic!("{name} requires a value"))
                .parse()
                .unwrap_or_else(|_| panic!("{name} must be an unsigned integer"));
        }
    }

    0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let warmups = parse_usize_argument("--warmups");
    let runs = parse_usize_argument("--runs").max(1);
    let mut records: Vec<InputRecord> = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if !line.trim().is_empty() {
            records.push(serde_json::from_str(&line)?);
        }
    }

    for _ in 0..warmups {
        for record in &records {
            scan(&record.text);
        }
    }

    let stdout = io::stdout();
    let mut output = stdout.lock();

    for record in records {
        let mut entities = Vec::new();
        let mut durations_ns = Vec::with_capacity(runs);

        for run in 0..runs {
            let started = Instant::now();
            let findings = scan(&record.text);
            durations_ns.push(started.elapsed().as_nanos());

            if run == 0 {
                entities = findings
                    .into_iter()
                    .map(|entity| OutputEntity {
                        label: entity.label,
                        text: entity.text,
                        start: entity.start,
                        end: entity.end,
                    })
                    .collect();
            }
        }

        serde_json::to_writer(
            &mut output,
            &OutputRecord {
                id: record.id,
                entities,
                durations_ns,
            },
        )?;
        writeln!(output)?;
    }

    Ok(())
}

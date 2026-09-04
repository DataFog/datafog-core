//! Reproducible text-scanner baseline. Iterations are a measurement parameter,
//! not a performance acceptance threshold.
use std::hint::black_box;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations: usize = std::env::args()
        .nth(1)
        .ok_or("provide iterations")?
        .parse()?;
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    if std::env::args().nth(2).as_deref() == Some("structured") {
        let corpus: Vec<serde_json::Value> = include_str!("../../../fixtures/structured.jsonl")
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        let config = datafog_core::structured::StructuredScanConfig::default();
        let transform = datafog_core::structured::parse_scan_and_transform_config(
            &serde_json::json!({"transform":{"default":{"strategy":"redact"}}}),
        )?;
        for operation in ["discover", "scan", "protect"] {
            let start = Instant::now();
            let mut records = 0;
            for _ in 0..iterations {
                for row in &corpus {
                    let data = black_box(&row["data"]);
                    records += match operation {
                        "discover" => {
                            black_box(datafog_core::structured::discover_fields(data, &config)?)
                                .len()
                        }
                        "scan" => black_box(datafog_core::structured::scan(data, &config)?)
                            .findings
                            .len(),
                        _ => black_box(datafog_core::structured::scan_and_transform(
                            data, &transform,
                        )?)
                        .transformations
                        .len(),
                    };
                }
            }
            let seconds = start.elapsed().as_secs_f64();
            println!(
                "operation={} documents={} iterations={} records={} elapsed_s={:.6} us_per_document={:.3}",
                operation,
                corpus.len(),
                iterations,
                records,
                seconds,
                seconds * 1e6 / (corpus.len() * iterations) as f64
            );
        }
        return Ok(());
    }
    let corpus: Vec<serde_json::Value> = include_str!("../../../fixtures/development.jsonl")
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let texts: Vec<&str> = corpus
        .iter()
        .map(|row| row["text"].as_str().ok_or("missing text"))
        .collect::<Result<_, _>>()?;
    let cold = Instant::now();
    black_box(datafog_core::scan("Email jane@example.test"));
    println!("cold_scan_us={:.3}", cold.elapsed().as_secs_f64() * 1e6);
    let start = Instant::now();
    let mut findings = 0;
    for _ in 0..iterations {
        for text in &texts {
            findings += black_box(datafog_core::scan(black_box(text))).len();
        }
    }
    let seconds = start.elapsed().as_secs_f64();
    let bytes: usize = texts.iter().map(|text| text.len()).sum();
    println!(
        "documents={} iterations={} bytes_per_iteration={} findings={} elapsed_s={:.6} mib_s={:.3}",
        texts.len(),
        iterations,
        bytes,
        findings,
        seconds,
        bytes as f64 * iterations as f64 / seconds / 1048576.0
    );
    Ok(())
}

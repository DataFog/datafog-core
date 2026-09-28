//! Compare base and locale scanning with identical synthetic workloads.
use datafog_core::{ScanConfig, scan_with_config};
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "200".into())
        .parse()?;
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let mixed = "👋 中文 é mail a@example.test; DE44 5001 0517 5407 3249 31; Steuer-ID 12345678901; SVNR 65150804A123; DE123456789; PLZ10115; Passport C12345678; eAT AT1234567. ";
    let workloads = [
        ("short", "Email a@example.test".to_owned()),
        ("mixed", mixed.to_owned()),
        ("long", mixed.repeat(100)),
        (
            "adversarial",
            format!(
                "{}{}{}",
                "DE4450010517540732493X;".repeat(100),
                "Steuer-ID ".repeat(100),
                "9".repeat(2000)
            ),
        ),
    ];
    for (name, text) in workloads {
        for (locale, config) in [
            ("none", ScanConfig::default()),
            ("de", ScanConfig::default().with_locale("de")?),
        ] {
            black_box(scan_with_config(&text, &config));
            let start = Instant::now();
            let mut findings = 0;
            for _ in 0..iterations {
                findings += black_box(scan_with_config(black_box(&text), &config)).len();
            }
            println!(
                "{name},{locale},{},{iterations},{:.3},{findings}",
                text.len(),
                start.elapsed().as_secs_f64() * 1e6 / iterations as f64
            );
        }
    }
    Ok(())
}

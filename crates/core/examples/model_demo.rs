//! Run with --features models and an explicitly provisioned session configuration.
#[cfg(feature = "models")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use datafog_core::models::{Cancellation, ModelSession, ModelSessionConfig, ScanMode};
    let config = std::env::args()
        .nth(1)
        .ok_or("provide a session configuration JSON file")?;
    let config: ModelSessionConfig = serde_json::from_reader(std::fs::File::open(config)?)?;
    let session = ModelSession::open(config)?;
    let text = "👋 John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.";
    let scan = session.scan(text, ScanMode::Combined, false, &Cancellation::default())?;
    let config =
        datafog_core::TransformationConfig::new(datafog_core::TransformationStrategy::Redact);
    let transformed = datafog_core::transform(text, &scan.findings, &config)?;
    println!("{}", serde_json::to_string_pretty(&session.inspect())?);
    println!("{}", transformed.text);
    session.close()?;
    Ok(())
}

#[cfg(not(feature = "models"))]
fn main() {
    eprintln!("This example requires --features models.");
}

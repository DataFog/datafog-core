//! Test-only transport reference for installed SDK streaming parity, using shared Core.
use datafog_core::{
    TextRange,
    workspace::{self as w, RestoreEncoding, Workspace, WorkspaceFinding},
};
use serde_json::{Value, json};
type Run<T> = Result<T, ()>;
fn issue(issue: &w::MarkerIssue) -> Value {
    json!({"kind":issue.kind.as_str(),"byteRange":issue.byte_range,"codepointRange":issue.codepoint_range,"utf16Range":issue.utf16_range})
}
fn progress(value: w::RestoreProgress) -> Value {
    json!({"bytes":value.bytes,"codepoints":value.codepoints,"utf16":value.utf16,"restoredMarkers":value.restored_markers})
}
fn error(error: &w::WorkspaceError, delivered: Option<w::RestoreProgress>) -> Value {
    json!({"code":error.code.as_str(),"findingIndex":error.finding_index,"inputIndex":error.input_index,"markerIssues":error.marker_issues.iter().map(issue).collect::<Vec<_>>(),"reviewConflicts":error.review_conflicts,"delivered":delivered.map(progress),"complete":delivered.map(|_|false)})
}
fn chunk(chunk: w::RestoreChunk) -> Value {
    json!({"text":chunk.text,"delivered":progress(chunk.delivered),"complete":chunk.complete})
}
fn report(report: w::RestoreReport) -> Value {
    json!({"text":report.text,"restoredMarkers":report.restored_markers,"issues":report.issues.iter().map(issue).collect::<Vec<_>>()})
}
fn field<'a>(value: &'a Value, key: &str) -> Run<&'a str> {
    value[key].as_str().ok_or(())
}
fn seed() -> Run<(Workspace, String)> {
    let mut workspace = Workspace::create([1; 16], [2; 16]).map_err(|_| ())?;
    let mut plan = workspace
        .prepare(
            "Jane",
            &[WorkspaceFinding::new(
                "PERSON",
                "Jane",
                TextRange { start: 0, end: 4 },
            )],
            [3; 16],
        )
        .map_err(|_| ())?;
    let marker = plan.preview().map_err(|_| ())?.text.clone();
    let parent = plan.expected_head().map_err(|_| ())?;
    let saved = plan.candidate_head().map_err(|_| ())?;
    workspace
        .activate_saved(&mut plan, parent, saved)
        .map_err(|_| ())?;
    Ok((workspace, marker))
}
fn bytes(step: &Value, marker: &str) -> Run<Vec<u8>> {
    if let Some(bytes) = step.get("bytes") {
        serde_json::from_value(bytes.clone()).map_err(|_| ())
    } else {
        Ok(field(step, "text")?
            .replace("{marker}", marker)
            .replace("{bareMarker}", &marker[2..marker.len() - 2])
            .replace(
                "{oversizedPrefix}",
                &format!("[[PERSON_{}", "A".repeat(100)),
            )
            .into_bytes())
    }
}
enum Stream {
    Buffered(w::BufferedRestore),
    Progressive(w::ProgressiveRestore),
}
impl Stream {
    fn call(&mut self, step: &Value, marker: &str) -> Run<Value> {
        let op = field(step, "op")?;
        let (result, delivered) = match self {
            Self::Buffered(stream) => {
                let result = match op {
                    "feed" => stream.feed(&bytes(step, marker)?).map(|()| Value::Null),
                    "feedUtf16" => stream
                        .feed_utf16(
                            &serde_json::from_value::<Vec<u16>>(step["units"].clone())
                                .map_err(|_| ())?,
                        )
                        .map(|()| Value::Null),
                    "finalize" => stream.finalize().map(report),
                    "reset" => stream.reset().map(|()| Value::Null),
                    "closeStream" => {
                        stream.close();
                        Ok(Value::Null)
                    }
                    _ => return Err(()),
                };
                (result, None)
            }
            Self::Progressive(stream) => {
                let result = match op {
                    "feed" => stream.feed(&bytes(step, marker)?).map(chunk),
                    "feedUtf16" => stream
                        .feed_utf16(
                            &serde_json::from_value::<Vec<u16>>(step["units"].clone())
                                .map_err(|_| ())?,
                        )
                        .map(chunk),
                    "finalize" => stream.finalize().map(chunk),
                    "reset" => stream.reset().map(|()| Value::Null),
                    "closeStream" => {
                        stream.close();
                        Ok(Value::Null)
                    }
                    _ => return Err(()),
                };
                match result {
                    Ok(value) => (Ok(value), None),
                    Err(failure) => (Err(*failure.error), Some(failure.delivered)),
                }
            }
        };
        Ok(match result {
            Ok(value) => json!({"op":op,"ok":true,"value":value}),
            Err(failure) => json!({"op":op,"ok":false,"error":error(&failure,delivered)}),
        })
    }
    fn close(&mut self) {
        match self {
            Self::Buffered(stream) => stream.close(),
            Self::Progressive(stream) => stream.close(),
        }
    }
}
fn run(fixture: Value) -> Run<Value> {
    if fixture["schemaVersion"] != 1 || fixture["profile"] != "workspace-streaming-v1" {
        return Err(());
    }
    let mut cases = Vec::new();
    for scenario in fixture["cases"]
        .as_array()
        .filter(|cases| !cases.is_empty())
        .ok_or(())?
    {
        let mut modes = serde_json::Map::new();
        for mode in ["buffered", "progressive"] {
            let (mut workspace, marker) = seed()?;
            let encoding = match field(scenario, "encoding")? {
                "utf8" => RestoreEncoding::Utf8,
                "utf16" => RestoreEncoding::Utf16,
                _ => return Err(()),
            };
            let mut stream = match mode {
                "buffered" => {
                    Stream::Buffered(workspace.buffered_restore(encoding).map_err(|_| ())?)
                }
                _ => Stream::Progressive(workspace.progressive_restore(encoding).map_err(|_| ())?),
            };
            let mut steps = Vec::new();
            for step in scenario["steps"]
                .as_array()
                .filter(|steps| !steps.is_empty())
                .ok_or(())?
            {
                let op = field(step, "op")?;
                let value = match op {
                    "advanceWorkspace" => {
                        let mut plan = workspace
                            .prepare(
                                "Bob",
                                &[WorkspaceFinding::new(
                                    "PERSON",
                                    "Bob",
                                    TextRange { start: 0, end: 3 },
                                )],
                                [4; 16],
                            )
                            .map_err(|_| ())?;
                        let parent = plan.expected_head().map_err(|_| ())?;
                        let saved = plan.candidate_head().map_err(|_| ())?;
                        workspace
                            .activate_saved(&mut plan, parent, saved)
                            .map_err(|_| ())?;
                        json!({"op":op,"ok":true,"value":null})
                    }
                    "closeWorkspace" => {
                        workspace.close();
                        json!({"op":op,"ok":true,"value":null})
                    }
                    _ => stream.call(step, &marker)?,
                };
                steps.push(value);
            }
            stream.close();
            workspace.close();
            modes.insert(mode.into(), json!({"steps":steps}));
        }
        cases.push(json!({"id":field(scenario,"id")?,"modes":modes}));
    }
    Ok(json!({"schemaVersion":1,"profile":fixture["profile"],"cases":cases}))
}
fn main() {
    let result = std::env::args()
        .nth(1)
        .ok_or(())
        .and_then(|path| std::fs::read(path).map_err(|_| ()))
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|_| ()))
        .and_then(run);
    match result {
        Ok(value) => {
            if serde_json::to_writer(std::io::stdout().lock(), &value).is_err() {
                std::process::exit(1);
            }
        }
        Err(()) => {
            eprintln!("Rust streaming parity reference failed");
            std::process::exit(1);
        }
    }
}

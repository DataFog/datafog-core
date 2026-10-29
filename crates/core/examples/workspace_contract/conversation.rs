//! Test-only command transport. All conversation and stream decisions call public Core.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Context {
    id: w::PublicId,
    surface: String,
    #[serde(deserialize_with = "nullable")]
    site_id: Option<String>,
    site_id_byte_budget: usize,
}
fn nullable<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(input)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    input: Value,
    conversation: Context,
}
impl From<Context> for w::Conversation {
    fn from(value: Context) -> Self {
        Self {
            id: value.id,
            surface: value.surface,
            site_id: value.site_id,
            site_id_byte_budget: value.site_id_byte_budget,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Intent {
    preparation_id: w::PublicId,
    input_index: usize,
}

pub(super) enum Stream {
    Buffered(w::BufferedRestore),
    Progressive(w::ProgressiveRestore),
}
fn progress(value: w::RestoreProgress) -> Value {
    json!({"bytes":value.bytes,"codepoints":value.codepoints,"utf16":value.utf16,"restoredMarkers":value.restored_markers})
}
pub(super) fn stream_failure(value: w::StreamFailure) -> Value {
    let mut result = failure(*value.error);
    result["delivered"] = progress(value.delivered);
    result["complete"] = json!(false);
    result
}
fn chunk(value: w::RestoreChunk) -> Value {
    json!({"text":value.text,"delivered":progress(value.delivered),"complete":value.complete})
}
impl Stream {
    fn call(&mut self, call: &str, bytes: &[u8], units: &[u16]) -> Run<Value> {
        match self {
            Self::Buffered(stream) => Ok(match call {
                "streamFeed" => {
                    stream.feed(bytes)?;
                    Value::Null
                }
                "streamFeedUtf16" => {
                    stream.feed_utf16(units)?;
                    Value::Null
                }
                "streamFinalize" => report(stream.finalize()?),
                "streamReset" => {
                    stream.reset()?;
                    Value::Null
                }
                "streamClose" => {
                    stream.close();
                    Value::Null
                }
                _ => return Err(Failure::Fixture),
            }),
            Self::Progressive(stream) => {
                let value = match call {
                    "streamFeed" => stream.feed(bytes).map(chunk),
                    "streamFeedUtf16" => stream.feed_utf16(units).map(chunk),
                    "streamFinalize" => stream.finalize().map(chunk),
                    "streamReset" => stream.reset().map(|()| Value::Null),
                    "streamClose" => {
                        stream.close();
                        return Ok(Value::Null);
                    }
                    _ => return Err(Failure::Fixture),
                };
                value.map_err(Failure::Stream)
            }
        }
    }
}
pub(super) fn supported(call: &str) -> bool {
    matches!(
        call,
        "prepareConversationSend"
            | "prepareObservedSend"
            | "prepareDefinitelyNotSent"
            | "conversation"
            | "conversationForSite"
            | "sendIntents"
            | "sendIntentStatus"
            | "restoreInConversation"
            | "restoreReportInConversation"
            | "bufferedRestoreInConversation"
            | "progressiveRestoreInConversation"
            | "streamFeed"
            | "streamFeedUtf16"
            | "streamFinalize"
            | "streamReset"
            | "streamClose"
    )
}
impl Runtime {
    pub(super) fn resolved_text(&self, command: &Value) -> Run<String> {
        let text = if let Some(selector) = command.get("textFrom") {
            self.plan_texts
                .get(name(selector, "plan")?)
                .and_then(|items| items.get(field::<usize>(selector, "inputIndex").ok()?))
                .cloned()
                .ok_or(Failure::Fixture)?
        } else {
            text_value(command)?
        };
        let prefix = command
            .get("prefix")
            .map(|value| value.as_str().ok_or(Failure::Fixture))
            .transpose()?
            .unwrap_or("");
        let suffix = command
            .get("suffix")
            .map(|value| value.as_str().ok_or(Failure::Fixture))
            .transpose()?
            .unwrap_or("");
        Ok(format!("{prefix}{text}{suffix}"))
    }
    fn intent(&self, command: &Value) -> Run<w::SendIntentId> {
        if let Some(selector) = command.get("intentFrom") {
            return self
                .plan_intents
                .get(name(selector, "plan")?)
                .and_then(|items| items.get(field::<usize>(selector, "inputIndex").ok()?))
                .copied()
                .ok_or(Failure::Fixture);
        }
        let value: Intent = field(command, "intent")?;
        Ok(w::SendIntentId {
            preparation_id: value.preparation_id,
            input_index: value.input_index,
        })
    }
    pub(super) fn remember_plan(&mut self, command: &Value, plan: PreparedChange) -> Run<Value> {
        let mut value = inspect(&plan)?;
        let intents = plan.send_intents()?;
        if self.conversation_profile {
            value["sendIntents"] = json!(intents);
        }
        let texts = plan
            .previews()?
            .iter()
            .map(|preview| preview.text.clone())
            .collect();
        let key: String = field(command, "plan")?;
        self.plan_owners
            .insert(key.clone(), field(command, "workspace")?);
        self.plan_texts.insert(key.clone(), texts);
        self.plan_intents.insert(key.clone(), intents);
        self.plans.insert(key, plan);
        Ok(value)
    }
    pub(super) fn execute_conversation(&mut self, command: &Value) -> Run<Value> {
        let call = name(command, "call")?;
        match call {
            "prepareConversationSend" => {
                let values: Vec<Input> = field(command, "inputs")?;
                let owned = values
                    .into_iter()
                    .map(|value| {
                        Ok((
                            self.review_input(value.input)?,
                            w::Conversation::from(value.conversation),
                        ))
                    })
                    .collect::<Run<Vec<_>>>()?;
                let inputs: Vec<_> = owned
                    .iter()
                    .map(|(input, context)| w::ConversationInput {
                        input: input.input(),
                        conversation: context.clone(),
                    })
                    .collect();
                let plan = self
                    .workspace(command)?
                    .prepare_conversation_send(&inputs, crate_operation(command)?)?;
                self.remember_plan(command, plan)
            }
            "prepareObservedSend" | "prepareDefinitelyNotSent" => {
                let intent = self.intent(command)?;
                let logical = id(command, "logicalIdByte")?;
                let operation = crate_operation(command)?;
                let workspace = self.workspace(command)?;
                let plan = if call == "prepareObservedSend" {
                    workspace.prepare_observed_send(
                        intent,
                        logical,
                        name(command, "surface")?,
                        name(command, "siteId")?,
                        operation,
                    )?
                } else {
                    workspace.prepare_definitely_not_sent(intent, logical, operation)?
                };
                self.remember_plan(command, plan)
            }
            "sendIntents" => Ok(json!(
                self.plans
                    .get(name(command, "plan")?)
                    .ok_or(Failure::Fixture)?
                    .send_intents()?
            )),
            "sendIntentStatus" => Ok(json!(
                self.workspace(command)?
                    .send_intent_status(self.intent(command)?)?
            )),
            "conversation" => Ok(json!(
                self.workspace(command)?
                    .conversation(id(command, "logicalIdByte")?)?
            )),
            "conversationForSite" => Ok(json!(
                self.workspace(command)?
                    .conversation_for_site(name(command, "surface")?, name(command, "siteId")?)?
            )),
            "restoreInConversation" => {
                Ok(json!(self.workspace(command)?.restore_in_conversation(
                    &self.resolved_text(command)?,
                    id(command, "logicalIdByte")?
                )?))
            }
            "restoreReportInConversation" => Ok(report(
                self.workspace(command)?.restore_report_in_conversation(
                    &self.resolved_text(command)?,
                    id(command, "logicalIdByte")?,
                )?,
            )),
            "bufferedRestoreInConversation" | "progressiveRestoreInConversation" => {
                let encoding = match command
                    .get("encoding")
                    .and_then(Value::as_str)
                    .unwrap_or("utf8")
                {
                    "utf8" => w::RestoreEncoding::Utf8,
                    "utf16" => w::RestoreEncoding::Utf16,
                    _ => return Err(Failure::Fixture),
                };
                let workspace = self.workspace(command)?;
                let logical = id(command, "logicalIdByte")?;
                let stream = if call == "bufferedRestoreInConversation" {
                    Stream::Buffered(workspace.buffered_restore_in_conversation(logical, encoding)?)
                } else {
                    Stream::Progressive(
                        workspace.progressive_restore_in_conversation(logical, encoding)?,
                    )
                };
                self.streams.insert(field(command, "stream")?, stream);
                Ok(Value::Null)
            }
            "streamFeed" | "streamFeedUtf16" | "streamFinalize" | "streamReset" | "streamClose" => {
                let mut bytes = Vec::new();
                let mut units = Vec::new();
                if call == "streamFeed" {
                    bytes = if command.get("bytes").is_some() {
                        field(command, "bytes")?
                    } else {
                        self.resolved_text(command)?.into_bytes()
                    };
                    if let Some(range) = command.get("byteSlice") {
                        let [start, end]: [usize; 2] = parse(range.clone())?;
                        bytes = bytes.get(start..end).ok_or(Failure::Fixture)?.to_vec();
                    }
                } else if call == "streamFeedUtf16" {
                    units = if command.get("units").is_some() {
                        field(command, "units")?
                    } else {
                        self.resolved_text(command)?.encode_utf16().collect()
                    };
                    if let Some(range) = command.get("unitSlice") {
                        let [start, end]: [usize; 2] = parse(range.clone())?;
                        units = units.get(start..end).ok_or(Failure::Fixture)?.to_vec();
                    }
                }
                self.streams
                    .get_mut(name(command, "stream")?)
                    .ok_or(Failure::Fixture)?
                    .call(call, &bytes, &units)
            }
            _ => Err(Failure::Fixture),
        }
    }
}

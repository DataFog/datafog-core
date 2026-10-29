//! Thin transport over shared Core conversation decisions; no site/provider interpretation.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ContextInput {
    id: [u8; 16],
    surface: String,
    #[serde(deserialize_with = "nullable_string")]
    site_id: Option<String>,
    site_id_byte_budget: usize,
}
fn nullable_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
impl From<ContextInput> for workspace::Conversation {
    fn from(value: ContextInput) -> Self {
        Self {
            id: value.id,
            surface: value.surface,
            site_id: value.site_id,
            site_id_byte_budget: value.site_id_byte_budget,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SendInput {
    input: ReviewInputData,
    conversation: ContextInput,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct IntentInput {
    preparation_id: [u8; 16],
    input_index: usize,
}
fn intent(json: &str) -> Result<workspace::SendIntentId, Error> {
    if json.len() > 256 {
        return Err(failure(ErrorCode::InvalidSendIntent));
    }
    let input: IntentInput =
        serde_json::from_str(json).map_err(|_| failure(ErrorCode::InvalidSendIntent))?;
    if input.input_index >= workspace::MAX_INPUTS {
        return Err(failure(ErrorCode::InvalidSendIntent));
    }
    Ok(workspace::SendIntentId {
        preparation_id: input.preparation_id,
        input_index: input.input_index,
    })
}
fn json_value<T: serde::Serialize>(value: &T) -> Result<String, Error> {
    let value = serde_json::to_string(value).map_err(|_| failure(ErrorCode::ResourceLimit))?;
    if value.len() > workspace::MAX_BUNDLE_BYTES {
        return Err(failure(ErrorCode::ResourceLimit));
    }
    Ok(value)
}
impl NativeWorkspace {
    fn stage_conversation(&mut self, plan: PreparedChange) -> Result<u32, Error> {
        if self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let next = self
            .next_plan
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::ResourceLimit))?;
        let handle = self.next_plan;
        self.plans.insert(handle, plan);
        self.next_plan = next;
        Ok(handle)
    }
}
#[napi]
impl NativeWorkspace {
    #[napi(js_name = "prepare_conversation_send")]
    pub fn prepare_conversation_send(
        &mut self,
        inputs_json: String,
        operation_json: String,
    ) -> Result<u32, Error> {
        if inputs_json.len() > workspace::MAX_BUNDLE_BYTES || self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let data: Vec<SendInput> = serde_json::from_str(&inputs_json)
            .map_err(|_| failure(ErrorCode::InvalidConversation))?;
        if data.len() > workspace::MAX_INPUTS {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let mut inputs = Vec::with_capacity(data.len());
        for (index, value) in data.into_iter().enumerate() {
            let review = adapt_review(value.input).map_err(|mut error| {
                error.input_index = Some(index);
                from_error(error)
            })?;
            inputs.push((review, workspace::Conversation::from(value.conversation)));
        }
        let borrowed: Vec<_> = inputs
            .iter()
            .map(|(review, context)| workspace::ConversationInput {
                input: review.as_input(),
                conversation: context.clone(),
            })
            .collect();
        let plan = self
            .inner
            .prepare_conversation_send(&borrowed, operation_context(&operation_json)?)
            .map_err(from_error)?;
        self.stage_conversation(plan)
    }
    #[napi(js_name = "send_intents")]
    pub fn send_intents(&self, handle: u32) -> Result<String, Error> {
        json_value(&self.plan(handle)?.send_intents().map_err(from_error)?)
    }
    #[napi(js_name = "conversation")]
    pub fn conversation(&self, logical_id: Buffer) -> Result<String, Error> {
        json_value(
            &self
                .inner
                .conversation(id(&logical_id)?)
                .map_err(from_error)?,
        )
    }
    #[napi(js_name = "conversation_for_site")]
    pub fn conversation_for_site(&self, surface: String, site_id: String) -> Result<Buffer, Error> {
        if surface.len().saturating_add(site_id.len()) > workspace::MAX_BUNDLE_BYTES {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        Ok(self
            .inner
            .conversation_for_site(&surface, &site_id)
            .map_err(from_error)?
            .to_vec()
            .into())
    }
    #[napi(js_name = "send_intent_status")]
    pub fn send_intent_status(&self, intent_json: String) -> Result<String, Error> {
        json_value(
            &self
                .inner
                .send_intent_status(intent(&intent_json)?)
                .map_err(from_error)?,
        )
    }
    #[napi(js_name = "prepare_observed_send")]
    pub fn prepare_observed_send(
        &mut self,
        intent_json: String,
        logical_id: Buffer,
        surface: String,
        site_id: String,
        operation_json: String,
    ) -> Result<u32, Error> {
        if self.plans.len() >= 32
            || surface.len().saturating_add(site_id.len()) > workspace::MAX_BUNDLE_BYTES
        {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let plan = self
            .inner
            .prepare_observed_send(
                intent(&intent_json)?,
                id(&logical_id)?,
                &surface,
                &site_id,
                operation_context(&operation_json)?,
            )
            .map_err(from_error)?;
        self.stage_conversation(plan)
    }
    #[napi(js_name = "prepare_definitely_not_sent")]
    pub fn prepare_definitely_not_sent(
        &mut self,
        intent_json: String,
        logical_id: Buffer,
        operation_json: String,
    ) -> Result<u32, Error> {
        if self.plans.len() >= 32 {
            return Err(failure(ErrorCode::ResourceLimit));
        }
        let plan = self
            .inner
            .prepare_definitely_not_sent(
                intent(&intent_json)?,
                id(&logical_id)?,
                operation_context(&operation_json)?,
            )
            .map_err(from_error)?;
        self.stage_conversation(plan)
    }
    #[napi(js_name = "restore_in_conversation")]
    pub fn restore_in_conversation(
        &self,
        text: String,
        logical_id: Buffer,
    ) -> Result<String, Error> {
        self.inner
            .restore_in_conversation(&text, id(&logical_id)?)
            .map_err(from_error)
    }
    #[napi(js_name = "restore_report_in_conversation")]
    pub fn restore_report_in_conversation(
        &self,
        text: String,
        logical_id: Buffer,
    ) -> Result<String, Error> {
        let report = self
            .inner
            .restore_report_in_conversation(&text, id(&logical_id)?)
            .map_err(from_error)?;
        let issues: Vec<_> = report.issues.iter().map(marker_issue_value).collect();
        json_value(
            &json!({"text":report.text,"restoredMarkers":report.restored_markers,"issues":issues}),
        )
    }
    #[napi(js_name = "buffered_restore_in_conversation")]
    pub fn buffered_restore_in_conversation(
        &mut self,
        logical_id: Buffer,
        encoding: String,
    ) -> Result<stream::NativeRestoreStream, Error> {
        self.streams
            .scoped(&self.inner, id(&logical_id)?, &encoding, false)
    }
    #[napi(js_name = "progressive_restore_in_conversation")]
    pub fn progressive_restore_in_conversation(
        &mut self,
        logical_id: Buffer,
        encoding: String,
    ) -> Result<stream::NativeRestoreStream, Error> {
        self.streams
            .scoped(&self.inner, id(&logical_id)?, &encoding, true)
    }
}

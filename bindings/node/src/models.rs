use super::*;
use datafog_core::models as core;
use napi::Task;
use napi::bindgen_prelude::AsyncTask;
use std::sync::Arc;

fn error(error: core::ModelError) -> Error {
    let mut value = serde_json::to_value(&error)
        .unwrap_or_else(|_| serde_json::json!({"code":"internal_error"}));
    if let Some(object) = value.as_object_mut() {
        if let Some(index) = object.remove("finding_index") {
            object.insert("findingIndex".into(), index);
        }
    }
    value["message"] = error.to_string().into();
    Error::new(Status::GenericFailure, value.to_string())
}
fn parse_mode(mode: &str) -> napi::Result<core::ScanMode> {
    match mode {
        "model_only" => Ok(core::ScanMode::ModelOnly),
        "combined" => Ok(core::ScanMode::Combined),
        _ => Err(Error::new(
            Status::InvalidArg,
            r#"{"code":"invalid_configuration","reason":"invalid_value","path":"/mode","message":"invalid model scan mode"}"#,
        )),
    }
}

#[napi(object)]
pub struct ModelScanResult {
    pub contract_version: u32,
    pub configuration_id: String,
    pub mode: String,
    pub findings: Vec<Finding>,
    #[napi(ts_type = "ModelFindingEvidence[]")]
    pub evidence: serde_json::Value,
    pub diagnostics: Option<Vec<Finding>>,
}
fn result(text: &str, value: core::ModelScanResult) -> napi::Result<ModelScanResult> {
    let mut index = datafog_core::TextIndex::new(text);
    let findings = value
        .findings
        .into_iter()
        .map(|f| js_finding(&mut index, f))
        .collect::<napi::Result<_>>()?;
    let diagnostics = value
        .diagnostics
        .map(|v| {
            v.into_iter()
                .map(|f| js_finding(&mut index, f))
                .collect::<napi::Result<_>>()
        })
        .transpose()?;
    Ok(ModelScanResult {
        contract_version: value.contract_version,
        configuration_id: value.configuration_id,
        mode: if value.mode == core::ScanMode::ModelOnly {
            "model_only"
        } else {
            "combined"
        }
        .into(),
        findings,
        evidence: serde_json::to_value(value.evidence).map_err(|_| {
            Error::new(
                Status::GenericFailure,
                "model evidence serialization failed",
            )
        })?,
        diagnostics,
    })
}

#[napi(object, object_from_js = false)]
pub struct ModelStructuredScanResult {
    pub contract_version: u32,
    pub configuration_id: String,
    pub mode: String,
    pub findings: Vec<StructuredFinding>,
    #[napi(ts_type = "ModelFindingEvidence[]")]
    pub evidence: serde_json::Value,
    pub mappings: serde_json::Value,
    pub diagnostics: Option<Vec<StructuredFinding>>,
}

#[napi]
pub struct ModelSessionNative {
    session: Arc<core::ModelSession>,
}
pub struct OpenTask {
    config: serde_json::Value,
}
impl Task for OpenTask {
    type Output = core::ModelSession;
    type JsValue = ModelSessionNative;
    fn compute(&mut self) -> napi::Result<Self::Output> {
        let config = core::parse_config(&self.config).map_err(error)?;
        core::ModelSession::open(config).map_err(error)
    }
    fn resolve(&mut self, _: Env, session: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(ModelSessionNative {
            session: Arc::new(session),
        })
    }
}
pub struct ScanTask {
    session: Arc<core::ModelSession>,
    text: String,
    mode: core::ScanMode,
    diagnostics: bool,
}
impl Task for ScanTask {
    type Output = core::ModelScanResult;
    type JsValue = ModelScanResult;
    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.session
            .scan(
                &self.text,
                self.mode,
                self.diagnostics,
                &core::Cancellation::default(),
            )
            .map_err(error)
    }
    fn resolve(&mut self, _: Env, value: Self::Output) -> napi::Result<Self::JsValue> {
        result(&self.text, value)
    }
}
pub struct StructuredTask {
    session: Arc<core::ModelSession>,
    data: serde_json::Value,
    mode: core::ScanMode,
    diagnostics: bool,
}
impl Task for StructuredTask {
    type Output = core::ModelStructuredScanResult;
    type JsValue = ModelStructuredScanResult;
    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.session
            .scan_structured(
                &self.data,
                self.mode,
                self.diagnostics,
                &core::Cancellation::default(),
            )
            .map_err(error)
    }
    fn resolve(&mut self, _: Env, value: Self::Output) -> napi::Result<Self::JsValue> {
        let convert = |located: datafog_core::structured::StructuredFinding| -> napi::Result<StructuredFinding> {
            let text = self.data.pointer(&located.path).and_then(|v|v.as_str())
                .ok_or_else(|| Error::new(Status::GenericFailure,"invalid structured model path"))?;
            let mut index = datafog_core::TextIndex::new(text);
            Ok(StructuredFinding { path: located.path, finding: js_finding(&mut index,located.finding)? })
        };
        let findings = value
            .findings
            .into_iter()
            .map(convert)
            .collect::<napi::Result<_>>()?;
        let diagnostics = value
            .diagnostics
            .map(|v| v.into_iter().map(convert).collect::<napi::Result<_>>())
            .transpose()?;
        Ok(ModelStructuredScanResult {
            contract_version: value.contract_version,
            configuration_id: value.configuration_id,
            mode: if value.mode == core::ScanMode::ModelOnly {
                "model_only"
            } else {
                "combined"
            }
            .into(),
            findings,
            diagnostics,
            evidence: serde_json::to_value(value.evidence).map_err(|_| {
                Error::new(
                    Status::GenericFailure,
                    "model evidence serialization failed",
                )
            })?,
            mappings: serde_json::to_value(value.mappings).map_err(|_| {
                Error::new(
                    Status::GenericFailure,
                    "model mappings serialization failed",
                )
            })?,
        })
    }
}
pub struct LifecycleTask {
    session: Arc<core::ModelSession>,
    close: bool,
}
impl Task for LifecycleTask {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> napi::Result<()> {
        if self.close {
            self.session.close()
        } else {
            self.session.warm_up()
        }
        .map_err(error)
    }
    fn resolve(&mut self, _: Env, _: ()) -> napi::Result<()> {
        Ok(())
    }
}
#[napi]
impl ModelSessionNative {
    #[napi]
    pub fn inspect(&self) -> napi::Result<serde_json::Value> {
        serde_json::to_value(self.session.inspect())
            .map_err(|_| Error::new(Status::GenericFailure, "model inspection failed"))
    }
    #[napi]
    pub fn scan(
        &self,
        text: String,
        mode: String,
        diagnostics: Option<bool>,
    ) -> napi::Result<AsyncTask<ScanTask>> {
        Ok(AsyncTask::new(ScanTask {
            session: Arc::clone(&self.session),
            text,
            mode: parse_mode(&mode)?,
            diagnostics: diagnostics.unwrap_or(false),
        }))
    }
    #[napi]
    pub fn scan_structured(
        &self,
        data_json: String,
        mode: String,
        diagnostics: Option<bool>,
    ) -> napi::Result<AsyncTask<StructuredTask>> {
        let data = datafog_core::structured::parse_document_json(&data_json)
            .map_err(|e| error(e.into()))?;
        Ok(AsyncTask::new(StructuredTask {
            session: Arc::clone(&self.session),
            data,
            mode: parse_mode(&mode)?,
            diagnostics: diagnostics.unwrap_or(false),
        }))
    }
    #[napi]
    pub fn warm_up(&self) -> AsyncTask<LifecycleTask> {
        AsyncTask::new(LifecycleTask {
            session: Arc::clone(&self.session),
            close: false,
        })
    }
    #[napi]
    pub fn health(&self) -> AsyncTask<LifecycleTask> {
        self.warm_up()
    }
    #[napi]
    pub fn close(&self) -> AsyncTask<LifecycleTask> {
        AsyncTask::new(LifecycleTask {
            session: Arc::clone(&self.session),
            close: true,
        })
    }
}

#[napi]
pub fn open_model_session(config: serde_json::Value) -> AsyncTask<OpenTask> {
    AsyncTask::new(OpenTask { config })
}

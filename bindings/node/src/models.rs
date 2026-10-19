use super::*;
use datafog_core::models as core;
use napi::Task;
use napi::bindgen_prelude::Object;
use std::sync::Arc;
use std::time::Instant;

type Resolver<Value> = Box<dyn FnOnce(Env) -> napi::Result<Value> + Send>;

/// Model operations use an admitted thread, avoiding unrelated libuv work queues.
/// Core starts deadlines before dispatch and still owns cancellation and cleanup.
fn dispatch<'env, T: Task + 'static>(env: &'env Env, mut task: T) -> napi::Result<Object<'env>>
where
    T::JsValue: 'static,
{
    let (send, receive) =
        std::sync::mpsc::sync_channel::<napi::JsDeferred<T::JsValue, Resolver<T::JsValue>>>(1);
    std::thread::Builder::new()
        .name("datafog-model-call".into())
        .spawn(move || {
            let Ok(deferred) = receive.recv() else {
                return;
            };
            let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| task.compute()))
                .unwrap_or_else(|_| {
                    Err(error(core::ModelError {
                        code: "model_session_failed".into(),
                        reason: "failed".into(),
                        path: "/backend".into(),
                        finding_index: None,
                    }))
                });
            deferred.resolve(Box::new(move |env| match value {
                Ok(value) => task.resolve(env, value),
                Err(value) => task.reject(env, value),
            }));
        })
        .map_err(|_| {
            error(core::ModelError {
                code: "model_resource_exhausted".into(),
                reason: "thread_unavailable".into(),
                path: "/backend".into(),
                finding_index: None,
            })
        })?;
    let (deferred, promise) = env.create_deferred::<T::JsValue, Resolver<T::JsValue>>()?;
    if let Err(failure) = send.send(deferred) {
        failure.0.reject(error(core::ModelError {
            code: "model_session_failed".into(),
            reason: "failed".into(),
            path: "/backend".into(),
            finding_index: None,
        }));
    }
    Ok(promise)
}

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
#[napi(object)]
pub struct ModelScanResult {
    pub contract_version: u32,
    pub model: String,
    pub findings: Vec<Finding>,
    #[napi(ts_type = "ModelFindingEvidence[]")]
    pub evidence: serde_json::Value,
}
fn result(text: &str, value: core::ModelScanResult) -> napi::Result<ModelScanResult> {
    let mut index = datafog_core::TextIndex::new(text);
    let findings = value
        .findings
        .into_iter()
        .map(|f| js_finding(&mut index, f))
        .collect::<napi::Result<_>>()?;
    Ok(ModelScanResult {
        contract_version: value.contract_version,
        model: value.model,
        findings,
        evidence: serde_json::to_value(value.evidence).map_err(|_| {
            Error::new(
                Status::GenericFailure,
                "model evidence serialization failed",
            )
        })?,
    })
}

#[napi]
#[derive(Default)]
pub struct ModelCancellation {
    cancellation: Arc<core::Cancellation>,
}
#[napi]
impl ModelCancellation {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    #[napi]
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

#[napi]
pub struct ModelSessionNative {
    session: Arc<core::ModelSession>,
}
pub struct OpenTask {
    config: serde_json::Value,
    started: Instant,
}
impl Task for OpenTask {
    type Output = core::ModelSession;
    type JsValue = ModelSessionNative;
    fn compute(&mut self) -> napi::Result<Self::Output> {
        let config = core::parse_config(&self.config).map_err(error)?;
        core::ModelSession::open_started(config, self.started).map_err(error)
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
    request: core::ModelRequest,
    cancellation: Arc<core::Cancellation>,
}
impl Task for ScanTask {
    type Output = core::ModelScanResult;
    type JsValue = ModelScanResult;
    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.session
            .scan_request(&self.request, &self.text, &self.cancellation)
            .map_err(error)
    }
    fn resolve(&mut self, _: Env, value: Self::Output) -> napi::Result<Self::JsValue> {
        result(&self.text, value)
    }
}
pub struct LifecycleTask {
    session: Arc<core::ModelSession>,
}
impl Task for LifecycleTask {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> napi::Result<()> {
        self.session.close().map_err(error)
    }
    fn resolve(&mut self, _: Env, _: ()) -> napi::Result<()> {
        Ok(())
    }
}
#[napi]
impl ModelSessionNative {
    #[napi(ts_return_type = "Promise<ModelScanResult>")]
    pub fn scan<'env>(
        &self,
        env: &'env Env,
        text: String,
        cancellation: Option<&ModelCancellation>,
    ) -> napi::Result<Object<'env>> {
        let request = self.session.prepare_request().map_err(error)?;
        dispatch(
            env,
            ScanTask {
                session: Arc::clone(&self.session),
                text,
                request,
                cancellation: cancellation
                    .map(|v| Arc::clone(&v.cancellation))
                    .unwrap_or_default(),
            },
        )
    }
    #[napi(ts_return_type = "Promise<void>")]
    pub fn close<'env>(&self, env: &'env Env) -> napi::Result<Object<'env>> {
        self.session.request_close();
        let result = dispatch(
            env,
            LifecycleTask {
                session: Arc::clone(&self.session),
            },
        );
        if result.is_err() {
            self.session.close().map_err(error)?;
        }
        result
    }
}

#[napi(ts_return_type = "Promise<ModelSessionNative>")]
pub fn open_model_session<'env>(
    env: &'env Env,
    config: serde_json::Value,
) -> napi::Result<Object<'env>> {
    dispatch(
        env,
        OpenTask {
            config,
            started: Instant::now(),
        },
    )
}

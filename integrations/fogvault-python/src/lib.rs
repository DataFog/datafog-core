//! Optional password-only Python transport over the existing Rust persistence owner.
use datafog_fogvault_native::{Change, Error, Owner, core, vault};
use pyo3::{
    create_exception,
    exceptions::PyValueError,
    prelude::*,
    types::{PyBytes, PyDict, PyInt},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread::{self, ThreadId};
use zeroize::Zeroizing;
mod stream;
mod transport;

create_exception!(datafog_fogvault, FogVaultWorkspaceError, PyValueError);
fn fail(py: Python<'_>, code: &str) -> PyErr {
    let e = FogVaultWorkspaceError::new_err(code.to_owned());
    if let Err(attach) = e.value(py).setattr("code", code) {
        return attach;
    }
    e
}
fn unknown_outcome(py: Python<'_>, request_id: [u8; 16]) -> PyErr {
    let exception = fail(py, "outcome_unknown");
    let attach = || -> PyResult<()> {
        exception
            .value(py)
            .setattr("request_id", PyBytes::new(py, &request_id))?;
        exception.value(py).setattr("requires_reopen", true)?;
        Ok(())
    };
    match attach() {
        Ok(()) => exception,
        Err(error) => error,
    }
}
fn native_error(py: Python<'_>, error: Error) -> PyErr {
    let (code, request) = match &error {
        Error::Core(e) => (e.code.as_str(), None),
        Error::Vault(e) => (
            e.code(),
            match e {
                vault::Error::OutcomeUnknown { request_id } => Some(*request_id),
                _ => None,
            },
        ),
        Error::BindingMismatch => ("binding_mismatch", None),
        Error::ApprovalRequired => ("approval_required", None),
        Error::DestinationOccupied => ("occupied_destination", None),
        Error::ForeignChange => ("foreign_plan", None),
        Error::ReopenRequired => ("owner_closed", None),
        Error::RevisionExhausted | Error::PreparationLimit => ("resource_limit", None),
        Error::SavedButNotActivated { request_id } => {
            ("saved_but_not_activated", Some(*request_id))
        }
    };
    let exception = fail(py, code);
    if let Some(request) = request
        && let Err(e) = exception
            .value(py)
            .setattr("request_id", PyBytes::new(py, &request))
    {
        return e;
    }
    if let Error::Core(e) = error {
        let attach = || -> PyResult<()> {
            exception
                .value(py)
                .setattr("finding_index", e.finding_index)?;
            exception.value(py).setattr("input_index", e.input_index)?;
            exception.value(py).setattr("marker_issues",transport::value(py,Value::Array(e.marker_issues.iter().map(|i|json!({"kind":i.kind.as_str(),"byteRange":i.byte_range,"codepointRange":i.codepoint_range,"utf16Range":i.utf16_range})).collect()))?)?;
            exception.value(py).setattr(
                "review_conflicts",
                transport::object(py, &e.review_conflicts)?,
            )?;
            Ok(())
        };
        if let Err(e) = attach() {
            return e;
        }
    }
    exception
}
fn lock<'a, T>(py: Python<'_>, cell: &'a Mutex<T>) -> PyResult<MutexGuard<'a, T>> {
    cell.try_lock()
        .map_err(|_| fail(py, "operation_in_progress"))
}
fn tenant_value(py: Python<'_>, tenant: &str) -> PyResult<String> {
    if tenant.is_empty() || tenant.len() > 128 || tenant.chars().any(char::is_control) {
        return Err(fail(py, "invalid_tenant"));
    }
    Ok(tenant.to_owned())
}
struct Shared {
    thread: ThreadId,
    tenant: String,
    owner: Mutex<Option<Owner>>,
    plans: Mutex<Vec<Weak<Mutex<Change>>>>,
    streams: Mutex<Vec<Weak<Mutex<stream::Implementation>>>>,
}
impl Shared {
    fn check(&self, py: Python<'_>, tenant: &str) -> PyResult<()> {
        if thread::current().id() != self.thread {
            return Err(fail(py, "wrong_thread"));
        }
        if tenant != self.tenant {
            return Err(fail(py, "wrong_tenant"));
        }
        Ok(())
    }
    fn owner<'a>(
        &'a self,
        py: Python<'_>,
        tenant: &str,
    ) -> PyResult<MutexGuard<'a, Option<Owner>>> {
        self.check(py, tenant)?;
        let guard = lock(py, &self.owner)?;
        if guard.is_none() {
            return Err(fail(py, "owner_closed"));
        }
        Ok(guard)
    }
    fn revoke(&self) {
        if let Ok(mut owner) = self.owner.lock() {
            owner.take();
        }
        if let Ok(mut plans) = self.plans.lock() {
            for p in plans.drain(..).filter_map(|p| p.upgrade()) {
                if let Ok(mut p) = p.lock() {
                    p.discard();
                }
            }
        }
        if let Ok(mut streams) = self.streams.lock() {
            for s in streams.drain(..).filter_map(|s| s.upgrade()) {
                if let Ok(mut s) = s.lock() {
                    s.close();
                }
            }
        }
    }
}
#[pyclass(frozen, skip_from_py_object, name = "WorkspaceHead")]
#[derive(Clone)]
struct Head {
    inner: core::Head,
}
#[pymethods]
impl Head {
    #[new]
    fn new(
        py: Python<'_>,
        revision: &Bound<'_, PyAny>,
        commit_id: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        if !revision.is_exact_instance_of::<PyInt>() {
            return Err(fail(py, "invalid_head"));
        }
        Ok(Self {
            inner: core::Head {
                revision: revision
                    .extract::<u64>()
                    .map_err(|_| fail(py, "invalid_head"))?,
                commit_id: transport::bytes(py, commit_id, 16)?
                    .try_into()
                    .map_err(|_| fail(py, "invalid_head"))?,
            },
        })
    }
    #[getter]
    fn revision(&self) -> u64 {
        self.inner.revision
    }
    #[getter]
    fn commit_id<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.commit_id)
    }
    fn __eq__(&self, other: PyRef<'_, Self>) -> bool {
        self.inner == other.inner
    }
    fn __repr__(&self) -> String {
        format!("WorkspaceHead(revision={})", self.inner.revision)
    }
}

#[pyclass(name = "NativeWorkspace")]
struct NativeWorkspace {
    shared: Arc<Shared>,
}
impl Drop for NativeWorkspace {
    fn drop(&mut self) {
        self.shared.revoke();
    }
}
impl NativeWorkspace {
    fn new_owner(owner: Owner, tenant: String) -> Self {
        Self {
            shared: Arc::new(Shared {
                thread: thread::current().id(),
                tenant,
                owner: Mutex::new(Some(owner)),
                plans: Mutex::new(Vec::new()),
                streams: Mutex::new(Vec::new()),
            }),
        }
    }
    fn wrap(&self, py: Python<'_>, change: Change) -> PyResult<PreparedChange> {
        let inner = Arc::new(Mutex::new(change));
        let mut registry = lock(py, &self.shared.plans)?;
        registry.retain(|p| p.strong_count() > 0);
        registry.push(Arc::downgrade(&inner));
        Ok(PreparedChange {
            shared: Arc::clone(&self.shared),
            inner,
        })
    }
}
fn limits() -> vault::Limits {
    vault::Limits {
        max_payload_bytes: core::MAX_BUNDLE_BYTES,
        max_metadata_bytes: 88,
        max_records: 64,
    }
}
fn profile(py: Python<'_>, value: &str) -> PyResult<()> {
    if value != "password_only" {
        Err(fail(py, "unsupported_profile"))
    } else {
        Ok(())
    }
}
fn open_vault(
    py: Python<'_>,
    control_path: &str,
    vault_path: &str,
    password: &Bound<'_, PyAny>,
    create: bool,
) -> PyResult<(vault::Vault, vault::SessionToken)> {
    let password = Zeroizing::new(transport::bytes(py, password, 1024)?.to_vec());
    let host = if create {
        vault::Host::create_new(control_path)
    } else {
        vault::Host::open(control_path)
    }
    .map_err(|e| native_error(py, Error::Vault(e)))?;
    let mut vault = if create {
        host.create(
            vault_path,
            &password,
            limits(),
            vault::KdfPolicy::baseline(),
        )
    } else {
        host.open_vault(vault_path, limits(), vault::KdfPolicy::baseline())
    }
    .map_err(|e| native_error(py, Error::Vault(e)))?;
    let token = vault
        .unlock(&password)
        .map_err(|e| native_error(py, Error::Vault(e)))?;
    Ok((vault, token))
}
#[pymethods]
impl NativeWorkspace {
    #[staticmethod]
    #[pyo3(signature=(tenant,control_path,vault_path,password,record_id,namespace,operation_id,request_id,*,profile="password_only"))]
    #[allow(clippy::too_many_arguments)]
    fn create_new(
        py: Python<'_>,
        tenant: &str,
        control_path: &str,
        vault_path: &str,
        password: &Bound<'_, PyAny>,
        record_id: &Bound<'_, PyAny>,
        namespace: &Bound<'_, PyAny>,
        operation_id: &Bound<'_, PyAny>,
        request_id: &Bound<'_, PyAny>,
        profile: &str,
    ) -> PyResult<Self> {
        crate::profile(py, profile)?;
        let tenant = tenant_value(py, tenant)?;
        let (record, namespace, operation, request) = (
            transport::id(py, record_id)?,
            transport::id(py, namespace)?,
            transport::id(py, operation_id)?,
            transport::id(py, request_id)?,
        );
        let (vault, token) = open_vault(py, control_path, vault_path, password, true)?;
        Ok(Self::new_owner(
            Owner::create(vault, token, record, namespace, operation, request)
                .map_err(|e| native_error(py, e))?,
            tenant,
        ))
    }
    #[staticmethod]
    #[pyo3(signature=(tenant,control_path,vault_path,password,record_id,namespace,*,expected_head=None,profile="password_only"))]
    #[allow(clippy::too_many_arguments)]
    fn open_existing(
        py: Python<'_>,
        tenant: &str,
        control_path: &str,
        vault_path: &str,
        password: &Bound<'_, PyAny>,
        record_id: &Bound<'_, PyAny>,
        namespace: &Bound<'_, PyAny>,
        expected_head: Option<PyRef<'_, Head>>,
        profile: &str,
    ) -> PyResult<Self> {
        crate::profile(py, profile)?;
        let tenant = tenant_value(py, tenant)?;
        let (record, namespace) = (transport::id(py, record_id)?, transport::id(py, namespace)?);
        let (vault, token) = open_vault(py, control_path, vault_path, password, false)?;
        Ok(Self::new_owner(
            Owner::open(
                vault,
                token,
                record,
                namespace,
                expected_head.map(|h| h.inner),
            )
            .map_err(|e| native_error(py, e))?,
            tenant,
        ))
    }
    /// Receipt-only restart reconciliation. It never opens Core or grants plaintext authority.
    #[staticmethod]
    fn reconcile_existing(
        py: Python<'_>,
        tenant: &str,
        control_path: &str,
        vault_path: &str,
        request_id: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        tenant_value(py, tenant)?;
        let request = transport::id(py, request_id)?;
        let host =
            vault::Host::open(control_path).map_err(|e| native_error(py, Error::Vault(e)))?;
        let mut vault = host
            .open_vault(vault_path, limits(), vault::KdfPolicy::baseline())
            .map_err(|e| native_error(py, Error::Vault(e)))?;
        let receipt = vault
            .reconcile(request)
            .map_err(|e| native_error(py, Error::Vault(e)))?;
        transport::value(
            py,
            json!({"generation":receipt.generation,"requestId":receipt.request_id,"headDigest":receipt.head_digest}),
        )
    }
    #[staticmethod]
    #[pyo3(signature=(tenant,control_path,vault_path,password,record_id,bundle,source_namespace,source_head,operation_id,request_id,approved,*,profile="password_only"))]
    #[allow(clippy::too_many_arguments)]
    fn import_authenticated_bundle(
        py: Python<'_>,
        tenant: &str,
        control_path: &str,
        vault_path: &str,
        password: &Bound<'_, PyAny>,
        record_id: &Bound<'_, PyAny>,
        bundle: &Bound<'_, PyAny>,
        source_namespace: &Bound<'_, PyAny>,
        source_head: PyRef<'_, Head>,
        operation_id: &Bound<'_, PyAny>,
        request_id: &Bound<'_, PyAny>,
        approved: &Bound<'_, PyAny>,
        profile: &str,
    ) -> PyResult<Self> {
        crate::profile(py, profile)?;
        transport::approved(py, approved)?;
        let tenant = tenant_value(py, tenant)?;
        let (record, namespace, operation, request) = (
            transport::id(py, record_id)?,
            transport::id(py, source_namespace)?,
            transport::id(py, operation_id)?,
            transport::id(py, request_id)?,
        );
        let candidate = datafog_fogvault_migration::stage_authenticated_bundle(
            transport::bytes(py, bundle, core::MAX_BUNDLE_BYTES)?,
            namespace,
            source_head.inner,
            operation,
        )
        .map_err(|e| match e {
            datafog_fogvault_migration::MigrationError::Core(e) => native_error(py, Error::Core(e)),
            _ => fail(py, "binding_mismatch"),
        })?;
        let (vault, token) = open_vault(py, control_path, vault_path, password, true)?;
        Ok(Self::new_owner(
            Owner::import_recovered(vault, token, record, &candidate, true, request)
                .map_err(|e| native_error(py, e))?,
            tenant,
        ))
    }
    fn head(&self, py: Python<'_>, tenant: &str) -> PyResult<Head> {
        let mut g = self.shared.owner(py, tenant)?;
        Ok(Head {
            inner: g
                .as_mut()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .head()
                .map_err(|e| native_error(py, e))?,
        })
    }
    fn namespace<'py>(&self, py: Python<'py>, tenant: &str) -> PyResult<Bound<'py, PyBytes>> {
        let g = self.shared.owner(py, tenant)?;
        Ok(PyBytes::new(
            py,
            &g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .namespace()
                .map_err(|e| native_error(py, e))?,
        ))
    }
    fn record_revision(&self, py: Python<'_>, tenant: &str) -> PyResult<u64> {
        let g = self.shared.owner(py, tenant)?;
        g.as_ref()
            .ok_or_else(|| fail(py, "owner_closed"))?
            .record_revision()
            .map_err(|e| native_error(py, e))
    }
    fn prepare(
        &self,
        py: Python<'_>,
        tenant: &str,
        text: &Bound<'_, PyAny>,
        findings: &Bound<'_, PyAny>,
        operation_id: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        self.shared.check(py, tenant)?;
        let text = transport::text(py, text)?;
        let findings: Vec<transport::Finding> = transport::decode(py, findings)?;
        let findings = findings
            .into_iter()
            .map(|f| f.into_core(py))
            .collect::<PyResult<Vec<_>>>()?;
        let id = transport::id(py, operation_id)?;
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare(&text, &findings, id)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn prepare_review(
        &self,
        py: Python<'_>,
        tenant: &str,
        inputs: &Bound<'_, PyAny>,
        operation_id: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        self.shared.check(py, tenant)?;
        let inputs: Vec<transport::Review> = transport::decode(py, inputs)?;
        let inputs = inputs
            .into_iter()
            .map(|i| i.convert(py))
            .collect::<PyResult<Vec<_>>>()?;
        let borrowed = inputs.iter().map(|i| i.borrowed()).collect::<Vec<_>>();
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare_review(&borrowed, transport::id(py, operation_id)?)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn prepare_batch(
        &self,
        py: Python<'_>,
        tenant: &str,
        inputs: &Bound<'_, PyAny>,
        operation_id: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        self.shared.check(py, tenant)?;
        let inputs: Vec<transport::BatchInput> = transport::decode(py, inputs)?;
        let owned = inputs
            .into_iter()
            .map(|i| {
                Ok((
                    i.text,
                    i.findings
                        .into_iter()
                        .map(|f| f.into_core(py))
                        .collect::<PyResult<Vec<_>>>()?,
                ))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let inputs = owned
            .iter()
            .map(|(text, findings)| core::WorkspaceInput { text, findings })
            .collect::<Vec<_>>();
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare_batch(&inputs, transport::id(py, operation_id)?)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn review(
        &self,
        py: Python<'_>,
        tenant: &str,
        input: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        self.shared.check(py, tenant)?;
        let input: transport::Review = transport::decode(py, input)?;
        let input = input.convert(py)?;
        let g = self.shared.owner(py, tenant)?;
        transport::review(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .review(input.borrowed())
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn recall(&self, py: Python<'_>, tenant: &str, text: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        transport::recall(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .recall(&transport::text(py, text)?)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn reconcile(
        &self,
        py: Python<'_>,
        tenant: &str,
        request_id: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let mut g = self.shared.owner(py, tenant)?;
        let r = g
            .as_mut()
            .ok_or_else(|| fail(py, "owner_closed"))?
            .reconcile(transport::id(py, request_id)?)
            .map_err(|e| native_error(py, e))?;
        transport::value(
            py,
            json!({"generation":r.generation,"requestId":r.request_id,"headDigest":r.head_digest}),
        )
    }
    fn prepare_conversation_send(
        &self,
        py: Python<'_>,
        tenant: &str,
        inputs: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        self.shared.check(py, tenant)?;
        let inputs: Vec<transport::ConversationInput> = transport::decode(py, inputs)?;
        let mut reviews = Vec::new();
        let mut conversations = Vec::new();
        for i in inputs {
            reviews.push(i.input.convert(py)?);
            conversations.push(i.conversation.into_core());
        }
        let borrowed = reviews
            .iter()
            .zip(conversations)
            .map(|(r, c)| core::ConversationInput {
                input: r.borrowed(),
                conversation: c,
            })
            .collect::<Vec<_>>();
        let operation: transport::Operation = transport::decode(py, operation)?;
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare_conversation_send(&borrowed, operation.into_core())
                .map_err(|e| native_error(py, e))?,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn prepare_observed_send(
        &self,
        py: Python<'_>,
        tenant: &str,
        intent: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
        surface: &Bound<'_, PyAny>,
        site_id: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        self.shared.check(py, tenant)?;
        let intent: transport::Intent = transport::decode(py, intent)?;
        let operation: transport::Operation = transport::decode(py, operation)?;
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare_observed_send(
                    intent.into_core(),
                    transport::id(py, logical_id)?,
                    &transport::text(py, surface)?,
                    &transport::text(py, site_id)?,
                    operation.into_core(),
                )
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn prepare_definitely_not_sent(
        &self,
        py: Python<'_>,
        tenant: &str,
        intent: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        self.shared.check(py, tenant)?;
        let intent: transport::Intent = transport::decode(py, intent)?;
        let operation: transport::Operation = transport::decode(py, operation)?;
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare_definitely_not_sent(
                    intent.into_core(),
                    transport::id(py, logical_id)?,
                    operation.into_core(),
                )
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn prepare_policy_changes(
        &self,
        py: Python<'_>,
        tenant: &str,
        edits: &Bound<'_, PyAny>,
        operation_id: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        self.shared.check(py, tenant)?;
        let edits: core::PolicyEdits = transport::decode(py, edits)?;
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare_policy_changes(&edits, transport::id(py, operation_id)?)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn prepare_recovery(
        &self,
        py: Python<'_>,
        tenant: &str,
        operation_id: &Bound<'_, PyAny>,
    ) -> PyResult<PreparedChange> {
        let g = self.shared.owner(py, tenant)?;
        self.wrap(
            py,
            g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .prepare_recovery(transport::id(py, operation_id)?)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn commit(
        &self,
        py: Python<'_>,
        tenant: &str,
        change: PyRef<'_, PreparedChange>,
        approved: &Bound<'_, PyAny>,
        request_id: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        self.shared.check(py, tenant)?;
        transport::approved(py, approved)?;
        if !Arc::ptr_eq(&self.shared, &change.shared) {
            return Err(fail(py, "foreign_plan"));
        }
        let request_id = transport::id(py, request_id)?;
        let mut g = self.shared.owner(py, tenant)?;
        let mut plan = lock(py, &change.inner)?;
        let result = g
            .as_mut()
            .ok_or_else(|| fail(py, "owner_closed"))?
            .commit(&mut plan, true, request_id)
            .map_err(|e| native_error(py, e))?;
        let projected = (|| -> PyResult<Py<PyAny>> {
            let out = PyDict::new(py);
            out.set_item("head", Head { inner: result.head }.into_pyobject(py)?)?;
            out.set_item("previews", result.previews)?;
            out.set_item(
                "recordRevision",
                g.as_ref()
                    .ok_or_else(|| fail(py, "owner_closed"))?
                    .record_revision()
                    .map_err(|e| native_error(py, e))?,
            )?;
            out.set_item("receipt",transport::value(py,json!({"generation":result.receipt.generation,"requestId":result.receipt.request_id,"headDigest":result.receipt.head_digest}))?)?;
            Ok(out.into_any().unbind())
        })();
        if projected.is_err() {
            // A confirmed save must never look like an ordinary retryable
            // transport failure. Release guards before revoking child handles.
            g.take();
            drop(plan);
            drop(g);
            self.shared.revoke();
            return Err(unknown_outcome(py, request_id));
        }
        projected
    }
    fn restore(&self, py: Python<'_>, tenant: &str, text: &Bound<'_, PyAny>) -> PyResult<String> {
        let g = self.shared.owner(py, tenant)?;
        g.as_ref()
            .ok_or_else(|| fail(py, "owner_closed"))?
            .restore(&transport::text(py, text)?)
            .map_err(|e| native_error(py, e))
    }
    fn restore_in_conversation(
        &self,
        py: Python<'_>,
        tenant: &str,
        text: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
    ) -> PyResult<String> {
        let g = self.shared.owner(py, tenant)?;
        g.as_ref()
            .ok_or_else(|| fail(py, "owner_closed"))?
            .restore_in_conversation(&transport::text(py, text)?, transport::id(py, logical_id)?)
            .map_err(|e| native_error(py, e))
    }
    fn restore_report(
        &self,
        py: Python<'_>,
        tenant: &str,
        text: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        transport::report(
            py,
            &g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .restore_report(&transport::text(py, text)?)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn restore_report_in_conversation(
        &self,
        py: Python<'_>,
        tenant: &str,
        text: &Bound<'_, PyAny>,
        logical_id: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        transport::report(
            py,
            &g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .restore_report_in_conversation(
                    &transport::text(py, text)?,
                    transport::id(py, logical_id)?,
                )
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn policy(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        transport::object(
            py,
            &g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .policy()
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn counts(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        let c = g
            .as_ref()
            .ok_or_else(|| fail(py, "owner_closed"))?
            .counts()
            .map_err(|e| native_error(py, e))?;
        transport::value(
            py,
            json!({"operations":c.operations,"preparedInputs":c.prepared_inputs,"transformedSpans":c.transformed_spans,"confirmedHandoffs":c.confirmed_handoffs,"reportedBlockedAttempts":c.reported_blocked_attempts}),
        )
    }
    fn history_coverage(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        transport::object(
            py,
            &g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .history_coverage()
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn operations(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        transport::value(py,Value::Array(g.as_ref().ok_or_else(||fail(py,"owner_closed"))?.operations().map_err(|e|native_error(py,e))?.iter().map(|o|json!({"id":o.id,"kind":o.kind.as_str(),"occurredAtMs":o.occurred_at_ms,"preparedInputs":o.prepared_inputs,"transformedSpans":o.transformed_spans})).collect()))
    }
    fn conversation(
        &self,
        py: Python<'_>,
        tenant: &str,
        logical_id: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        transport::object(
            py,
            &g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .conversation(transport::id(py, logical_id)?)
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn send_intent_status(
        &self,
        py: Python<'_>,
        tenant: &str,
        intent: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let g = self.shared.owner(py, tenant)?;
        let intent: transport::Intent = transport::decode(py, intent)?;
        transport::object(
            py,
            &g.as_ref()
                .ok_or_else(|| fail(py, "owner_closed"))?
                .send_intent_status(intent.into_core())
                .map_err(|e| native_error(py, e))?,
        )
    }
    #[pyo3(signature=(tenant,encoding="utf8",*,progressive=false,logical_id=None))]
    fn restore_stream(
        &self,
        py: Python<'_>,
        tenant: &str,
        encoding: &str,
        progressive: bool,
        logical_id: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<stream::RestoreStream> {
        stream::make(
            py,
            &self.shared,
            tenant,
            encoding,
            progressive,
            logical_id.map(|id| transport::id(py, id)).transpose()?,
        )
    }
    fn lock(&self, py: Python<'_>, tenant: &str) -> PyResult<()> {
        self.shared.check(py, tenant)?;
        self.shared.revoke();
        Ok(())
    }
    fn close(&self, py: Python<'_>, tenant: &str) -> PyResult<()> {
        self.lock(py, tenant)
    }
}

#[pyclass(name = "PreparedChange")]
struct PreparedChange {
    shared: Arc<Shared>,
    inner: Arc<Mutex<Change>>,
}
#[pymethods]
impl PreparedChange {
    fn previews(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let _g = self.shared.owner(py, tenant)?;
        transport::previews(
            py,
            lock(py, &self.inner)?
                .previews()
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn heads(&self, py: Python<'_>, tenant: &str) -> PyResult<(Head, Head)> {
        let _g = self.shared.owner(py, tenant)?;
        let (a, b) = lock(py, &self.inner)?
            .heads()
            .map_err(|e| native_error(py, e))?;
        Ok((Head { inner: a }, Head { inner: b }))
    }
    fn policy_preview(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let _g = self.shared.owner(py, tenant)?;
        transport::object(
            py,
            &lock(py, &self.inner)?
                .policy_preview()
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn send_intents(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let _g = self.shared.owner(py, tenant)?;
        transport::object(
            py,
            &lock(py, &self.inner)?
                .send_intents()
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn snapshot_format(&self, py: Python<'_>, tenant: &str) -> PyResult<Py<PyAny>> {
        let _g = self.shared.owner(py, tenant)?;
        transport::object(
            py,
            &lock(py, &self.inner)?
                .snapshot_format()
                .map_err(|e| native_error(py, e))?,
        )
    }
    fn discard(&self, py: Python<'_>, tenant: &str) -> PyResult<()> {
        let _g = self.shared.owner(py, tenant)?;
        lock(py, &self.inner)?.discard();
        Ok(())
    }
}

#[pymodule(gil_used = true)]
fn datafog_fogvault(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", "0.0.0")?;
    m.add(
        "FogVaultWorkspaceError",
        m.py().get_type::<FogVaultWorkspaceError>(),
    )?;
    m.add_class::<NativeWorkspace>()?;
    m.add_class::<PreparedChange>()?;
    m.add_class::<Head>()?;
    m.add_class::<stream::RestoreStream>()?;
    Ok(())
}

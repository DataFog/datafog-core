//! Thin N-API ownership/transport layer; all save and Core authority stays in Owner.
use datafog_fogvault_native::{self as bridge, Change, Owner, core, vault};
use napi::{Error, Status, bindgen_prelude::Buffer};
use napi_derive::napi;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    thread::{self, ThreadId},
};
use zeroize::Zeroizing;

type Result<T> = napi::Result<T>;
const MAX_TRANSPORT: usize = core::MAX_BUNDLE_BYTES;
fn fail(code: &str) -> Error {
    Error::new(
        Status::GenericFailure,
        json!({"code":code,"message":code}).to_string(),
    )
}
fn mapped(error: bridge::Error) -> Error {
    let value = match error {
        bridge::Error::Core(e) => {
            json!({"code":e.code.as_str(),"findingIndex":e.finding_index,"inputIndex":e.input_index,"markerIssues":e.marker_issues.iter().map(issue).collect::<Vec<_>>(),"reviewConflicts":e.review_conflicts})
        }
        bridge::Error::Vault(vault::Error::OutcomeUnknown { request_id }) => {
            json!({"code":"outcome_unknown","requestId":request_id,"requiresReopen":true})
        }
        bridge::Error::SavedButNotActivated { request_id } => {
            json!({"code":"saved_but_not_activated","requestId":request_id,"requiresReopen":true})
        }
        bridge::Error::Vault(e) => {
            json!({"code":match e { vault::Error::Conflict=>"vault_conflict", vault::Error::NotFound=>"not_found", vault::Error::ResourceLimit=>"resource_limit", vault::Error::Capacity=>"capacity", vault::Error::Locked|vault::Error::StaleSession=>"vault_locked", vault::Error::OperationIdReuse=>"operation_id_reuse", vault::Error::Exists=>"destination_occupied", vault::Error::UnsupportedCapability=>"unsupported_capability",_=>"vault_error" },"requiresReopen":true})
        }
        bridge::Error::ApprovalRequired => json!({"code":"approval_required"}),
        bridge::Error::DestinationOccupied => json!({"code":"destination_occupied"}),
        bridge::Error::BindingMismatch => json!({"code":"binding_mismatch"}),
        bridge::Error::ForeignChange => json!({"code":"foreign_change"}),
        bridge::Error::ReopenRequired => json!({"code":"reopen_required","requiresReopen":true}),
        bridge::Error::RevisionExhausted | bridge::Error::PreparationLimit => {
            json!({"code":"resource_limit"})
        }
    };
    Error::new(Status::GenericFailure, value.to_string())
}
fn parse<T: DeserializeOwned>(text: &str) -> Result<T> {
    if text.len() > MAX_TRANSPORT {
        return Err(fail("resource_limit"));
    }
    serde_json::from_str(text).map_err(|_| fail("invalid_transport"))
}
fn output(value: Value) -> Result<String> {
    let out = value.to_string();
    if out.len() > MAX_TRANSPORT * 4 {
        Err(fail("resource_limit"))
    } else {
        Ok(out)
    }
}
fn progress(p: core::RestoreProgress) -> Value {
    json!({"bytes":p.bytes,"codepoints":p.codepoints,"utf16":p.utf16,"restoredMarkers":p.restored_markers})
}
fn stream_error(e: core::StreamFailure) -> Error {
    Error::new(Status::GenericFailure,json!({"code":e.error.code.as_str(),"complete":false,"delivered":progress(e.delivered),"markerIssues":e.error.marker_issues.iter().map(issue).collect::<Vec<_>>(),"reviewConflicts":e.error.review_conflicts}).to_string())
}
fn head(h: core::Head) -> Value {
    json!({"revision":h.revision.to_string(),"commitId":h.commit_id})
}
fn receipt(r: &vault::CommitReceipt) -> Value {
    json!({"generation":r.generation.to_string(),"requestId":r.request_id,"headDigest":r.head_digest})
}
fn issue(i: &core::MarkerIssue) -> Value {
    json!({"kind":i.kind.as_str(),"byteRange":i.byte_range,"codepointRange":i.codepoint_range,"utf16Range":i.utf16_range})
}
fn report(r: core::RestoreReport) -> Value {
    json!({"text":r.text,"restoredMarkers":r.restored_markers,"issues":r.issues.iter().map(issue).collect::<Vec<_>>()})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HeadInput {
    revision: String,
    commit_id: [u8; 16],
}
impl HeadInput {
    fn core(self) -> Result<core::Head> {
        let revision: u64 = self.revision.parse().map_err(|_| fail("invalid_head"))?;
        if revision.to_string() != self.revision {
            return Err(fail("invalid_head"));
        }
        Ok(core::Head {
            revision,
            commit_id: self.commit_id,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Config {
    protected_control_path: String,
    vault_path: String,
    tenant: String,
    record_id: [u8; 16],
    namespace: [u8; 16],
    operation_id: Option<[u8; 16]>,
    request_id: Option<[u8; 16]>,
    expected_head: Option<HeadInput>,
    source_head: Option<HeadInput>,
    approved: Option<bool>,
    #[serde(default)]
    max_payload_bytes: Option<usize>,
    #[serde(default)]
    profile: Option<String>,
}
impl Config {
    fn limits(&self) -> Result<vault::Limits> {
        if self.tenant.is_empty()
            || self.tenant.len() > 128
            || self.tenant.chars().any(char::is_control)
            || self.protected_control_path.is_empty()
            || self.vault_path.is_empty()
            || self.protected_control_path.len() > 4096
            || self.vault_path.len() > 4096
            || self.record_id == [0; 16]
            || self.namespace == [0; 16]
        {
            return Err(fail("invalid_configuration"));
        }
        if self
            .profile
            .as_deref()
            .is_some_and(|x| x != "password_only")
        {
            return Err(fail("unsupported_profile"));
        }
        let max_payload_bytes = self.max_payload_bytes.unwrap_or(core::MAX_BUNDLE_BYTES);
        if !(64..=core::MAX_BUNDLE_BYTES).contains(&max_payload_bytes) {
            return Err(fail("invalid_configuration"));
        }
        Ok(vault::Limits {
            max_payload_bytes,
            max_metadata_bytes: bridge::BINDING_BYTES,
            max_records: 64,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Range {
    unit: String,
    start: usize,
    end: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Finding {
    entity_type: String,
    matched_text: String,
    range: Range,
    confidence: Option<f32>,
    detector_name: Option<String>,
    detector_version: Option<String>,
    #[serde(default)]
    source: core::FindingSource,
    #[serde(default = "needs_review")]
    needs_review: bool,
}
fn needs_review() -> bool {
    true
}
impl Finding {
    fn core(self) -> Result<core::WorkspaceFinding> {
        if self.range.unit != "utf8_bytes" {
            return Err(fail("invalid_finding"));
        }
        Ok(core::WorkspaceFinding {
            entity_type: self.entity_type,
            matched_text: self.matched_text,
            byte_range: datafog_core::TextRange {
                start: self.range.start,
                end: self.range.end,
            },
            confidence: self.confidence,
            detector_name: self.detector_name,
            detector_version: self.detector_version,
            source: self.source,
            needs_review: self.needs_review,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Correction {
    candidate_key: String,
    replacement: Finding,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Review {
    text: String,
    scanner_findings: Vec<Finding>,
    user_selections: Vec<Finding>,
    #[serde(default)]
    corrections: Vec<Correction>,
}
struct BorrowReview {
    text: String,
    scanner: Vec<core::WorkspaceFinding>,
    user: Vec<core::WorkspaceFinding>,
    corrections: Vec<core::ReviewCorrection>,
}
impl Review {
    fn core(self) -> Result<BorrowReview> {
        if self.text.len() > core::MAX_SOURCE_BYTES
            || self
                .scanner_findings
                .len()
                .saturating_add(self.user_selections.len())
                .saturating_add(self.corrections.len())
                > core::MAX_FINDINGS
        {
            return Err(fail("resource_limit"));
        }
        Ok(BorrowReview {
            text: self.text,
            scanner: self
                .scanner_findings
                .into_iter()
                .map(Finding::core)
                .collect::<Result<_>>()?,
            user: self
                .user_selections
                .into_iter()
                .map(Finding::core)
                .collect::<Result<_>>()?,
            corrections: self
                .corrections
                .into_iter()
                .map(|c| {
                    Ok(core::ReviewCorrection {
                        candidate_key: c.candidate_key,
                        replacement: c.replacement.core()?,
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
}
impl BorrowReview {
    fn input(&self) -> core::ReviewInput<'_> {
        core::ReviewInput {
            text: &self.text,
            scanner_findings: &self.scanner,
            user_selections: &self.user,
            corrections: &self.corrections,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Operation {
    id: [u8; 16],
    occurred_at_ms: u64,
}
impl Operation {
    fn core(self) -> core::OperationContext {
        core::OperationContext {
            id: self.id,
            occurred_at_ms: self.occurred_at_ms,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Context {
    id: [u8; 16],
    surface: String,
    #[serde(deserialize_with = "nullable_string")]
    site_id: Option<String>,
    site_id_byte_budget: usize,
}
fn nullable_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}
impl Context {
    fn core(self) -> core::Conversation {
        core::Conversation {
            id: self.id,
            surface: self.surface,
            site_id: self.site_id,
            site_id_byte_budget: self.site_id_byte_budget,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Send {
    input: Review,
    conversation: Context,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Intent {
    preparation_id: [u8; 16],
    input_index: usize,
}
impl Intent {
    fn core(self) -> core::SendIntentId {
        core::SendIntentId {
            preparation_id: self.preparation_id,
            input_index: self.input_index,
        }
    }
}
enum Stream {
    Buffered(core::BufferedRestore),
    Progressive(core::ProgressiveRestore),
}

#[napi]
pub struct NativeOwner {
    owner: Option<Owner>,
    tenant: String,
    thread: ThreadId,
    changes: BTreeMap<u32, Change>,
    streams: BTreeMap<u32, Stream>,
    next: u32,
}
impl NativeOwner {
    fn auth(&self, tenant: &str) -> Result<()> {
        if self.thread != thread::current().id() {
            return Err(fail("wrong_thread"));
        }
        if self.tenant != tenant {
            return Err(fail("wrong_tenant"));
        }
        Ok(())
    }
    fn owner(&self) -> Result<&Owner> {
        self.owner.as_ref().ok_or_else(|| fail("closed_owner"))
    }
    fn admit_plan(&self) -> Result<()> {
        if self.changes.len() >= 32 {
            Err(fail("resource_limit"))
        } else {
            Ok(())
        }
    }
    fn stage(&mut self, change: Change) -> Result<String> {
        if self.changes.len() >= 32 {
            return Err(fail("resource_limit"));
        }
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| fail("resource_limit"))?;
        self.changes.insert(id, change);
        output(json!({"handle":id}))
    }
}
#[napi]
impl NativeOwner {
    #[napi(factory, js_name = "createNew")]
    pub fn create_new(config: String, password: Buffer) -> Result<Self> {
        if password.len() > 1024 {
            return Err(fail("resource_limit"));
        }
        let c: Config = parse(&config)?;
        let limits = c.limits()?;
        if c.expected_head.is_some() || c.source_head.is_some() || c.approved.is_some() {
            return Err(fail("invalid_configuration"));
        }
        let operation = c
            .operation_id
            .ok_or_else(|| fail("invalid_configuration"))?;
        let request = c.request_id.ok_or_else(|| fail("invalid_configuration"))?;
        let password = Zeroizing::new(password.to_vec());
        let host =
            vault::Host::create_new(&c.protected_control_path).map_err(|e| mapped(e.into()))?;
        let mut vault = host
            .create(
                &c.vault_path,
                &password,
                limits,
                vault::KdfPolicy::baseline(),
            )
            .map_err(|e| mapped(e.into()))?;
        let token = vault.unlock(&password).map_err(|e| mapped(e.into()))?;
        let owner = Owner::create(vault, token, c.record_id, c.namespace, operation, request)
            .map_err(mapped)?;
        Ok(Self {
            owner: Some(owner),
            tenant: c.tenant,
            thread: thread::current().id(),
            changes: BTreeMap::new(),
            streams: BTreeMap::new(),
            next: 1,
        })
    }
    #[napi(factory, js_name = "openExisting")]
    pub fn open_existing(config: String, password: Buffer) -> Result<Self> {
        if password.len() > 1024 {
            return Err(fail("resource_limit"));
        }
        let c: Config = parse(&config)?;
        let limits = c.limits()?;
        if c.operation_id.is_some()
            || c.request_id.is_some()
            || c.source_head.is_some()
            || c.approved.is_some()
        {
            return Err(fail("invalid_configuration"));
        }
        let expected = c.expected_head.map(HeadInput::core).transpose()?;
        let password = Zeroizing::new(password.to_vec());
        let host = vault::Host::open(&c.protected_control_path).map_err(|e| mapped(e.into()))?;
        let mut vault = host
            .open_vault(&c.vault_path, limits, vault::KdfPolicy::baseline())
            .map_err(|e| mapped(e.into()))?;
        let token = vault.unlock(&password).map_err(|e| mapped(e.into()))?;
        let owner =
            Owner::open(vault, token, c.record_id, c.namespace, expected).map_err(mapped)?;
        Ok(Self {
            owner: Some(owner),
            tenant: c.tenant,
            thread: thread::current().id(),
            changes: BTreeMap::new(),
            streams: BTreeMap::new(),
            next: 1,
        })
    }
    #[napi(factory, js_name = "importAuthenticatedBundle")]
    pub fn import_authenticated_bundle(
        config: String,
        password: Buffer,
        bundle: Buffer,
    ) -> Result<Self> {
        if password.len() > 1024 || bundle.len() > core::MAX_BUNDLE_BYTES {
            return Err(fail("resource_limit"));
        }
        let mut c: Config = parse(&config)?;
        let limits = c.limits()?;
        if c.approved != Some(true) {
            return Err(fail("approval_required"));
        }
        if c.expected_head.is_some() {
            return Err(fail("invalid_configuration"));
        }
        let source = c
            .source_head
            .take()
            .ok_or_else(|| fail("invalid_head"))?
            .core()?;
        let operation = c
            .operation_id
            .ok_or_else(|| fail("invalid_configuration"))?;
        let request = c.request_id.ok_or_else(|| fail("invalid_configuration"))?;
        let candidate = datafog_fogvault_migration::stage_authenticated_bundle(
            &bundle,
            c.namespace,
            source,
            operation,
        )
        .map_err(|e| match e {
            datafog_fogvault_migration::MigrationError::SourceBindingMismatch => {
                fail("binding_mismatch")
            }
            datafog_fogvault_migration::MigrationError::Core(e) => mapped(e.into()),
        })?;
        let password = Zeroizing::new(password.to_vec());
        let host =
            vault::Host::create_new(&c.protected_control_path).map_err(|e| mapped(e.into()))?;
        let mut vault = host
            .create(
                &c.vault_path,
                &password,
                limits,
                vault::KdfPolicy::baseline(),
            )
            .map_err(|e| mapped(e.into()))?;
        let token = vault.unlock(&password).map_err(|e| mapped(e.into()))?;
        let owner = Owner::import_recovered(vault, token, c.record_id, &candidate, true, request)
            .map_err(mapped)?;
        Ok(Self {
            owner: Some(owner),
            tenant: c.tenant,
            thread: thread::current().id(),
            changes: BTreeMap::new(),
            streams: BTreeMap::new(),
            next: 1,
        })
    }
    #[napi(js_name = "reconcileExisting")]
    pub fn reconcile_existing(config: String, request: Buffer) -> Result<String> {
        let c: Config = parse(&config)?;
        let limits = c.limits()?;
        if request.len() != 16
            || c.operation_id.is_some()
            || c.request_id.is_some()
            || c.expected_head.is_some()
            || c.source_head.is_some()
            || c.approved.is_some()
        {
            return Err(fail("invalid_configuration"));
        }
        let request: [u8; 16] = request
            .as_ref()
            .try_into()
            .map_err(|_| fail("invalid_transport"))?;
        let host = vault::Host::open(&c.protected_control_path).map_err(|e| mapped(e.into()))?;
        let mut vault = host
            .open_vault(&c.vault_path, limits, vault::KdfPolicy::baseline())
            .map_err(|e| mapped(e.into()))?;
        output(receipt(
            &vault.reconcile(request).map_err(|e| mapped(e.into()))?,
        ))
    }
    #[napi]
    pub fn feed(
        &mut self,
        tenant: String,
        handle: u32,
        bytes: Buffer,
        utf16: bool,
    ) -> Result<String> {
        self.auth(&tenant)?;
        if bytes.len() > core::MAX_BUNDLE_BYTES {
            return Err(fail("resource_limit"));
        }
        let stream = self
            .streams
            .get_mut(&handle)
            .ok_or_else(|| fail("inactive_stream"))?;
        if utf16 && !bytes.len().is_multiple_of(2) {
            return Err(fail("invalid_transport"));
        }
        let units = if utf16 {
            bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        match stream {
            Stream::Buffered(s) => {
                if utf16 {
                    s.feed_utf16(&units)
                } else {
                    s.feed(&bytes)
                }
                .map_err(|e| mapped(e.into()))?;
                output(Value::Null)
            }
            Stream::Progressive(s) => {
                let chunk = if utf16 {
                    s.feed_utf16(&units)
                } else {
                    s.feed(&bytes)
                }
                .map_err(stream_error)?;
                output(
                    json!({"text":chunk.text,"complete":chunk.complete,"delivered":progress(chunk.delivered)}),
                )
            }
        }
    }
    #[napi]
    pub fn call(&mut self, tenant: String, method: String, input: String) -> Result<String> {
        self.auth(&tenant)?;
        match method.as_str() {
            "head" => output(head(self.owner()?.head().map_err(mapped)?)),
            "counts" => {
                let c = self.owner()?.counts().map_err(mapped)?;
                output(
                    json!({"operations":c.operations,"preparedInputs":c.prepared_inputs,"transformedSpans":c.transformed_spans,"confirmedHandoffs":c.confirmed_handoffs,"reportedBlockedAttempts":c.reported_blocked_attempts}),
                )
            }
            "historyCoverage" => output(
                serde_json::to_value(self.owner()?.history_coverage().map_err(mapped)?)
                    .map_err(|_| fail("internal_error"))?,
            ),
            "policy" => output(
                serde_json::to_value(self.owner()?.policy().map_err(mapped)?)
                    .map_err(|_| fail("internal_error"))?,
            ),
            "lock" | "close" => {
                if let Some(o) = self.owner.as_mut() {
                    o.lock();
                }
                self.changes.clear();
                self.streams.clear();
                if method == "close" {
                    self.owner = None;
                }
                output(Value::Null)
            }
            "review" => {
                let input: Review = parse(&input)?;
                let owned = input.core()?;
                let result = self.owner()?.review(owned.input()).map_err(mapped)?;
                let candidates=result.candidates.into_iter().map(|c|{let f=c.finding;json!({"key":c.key,"finding":{"entityType":f.entity_type,"matchedText":f.matched_text,"range":{"unit":"utf8_bytes","start":f.byte_range.start,"end":f.byte_range.end},"confidence":f.confidence,"detectorName":f.detector_name,"detectorVersion":f.detector_version,"source":f.source.as_str(),"needsReview":f.needs_review},"sources":c.sources.iter().map(|s|s.as_str()).collect::<Vec<_>>(),"byteRange":f.byte_range,"codepointRange":c.codepoint_range,"utf16Range":c.utf16_range,"eligible":c.eligible,"selected":c.selected})}).collect::<Vec<_>>();
                output(
                    json!({"head":head(result.head),"candidates":candidates,"conflicts":result.conflicts}),
                )
            }
            "prepareReview" => {
                self.admit_plan()?;
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct A {
                    inputs: Vec<Review>,
                    operation_id: [u8; 16],
                }
                let a: A = parse(&input)?;
                if a.inputs.len() > core::MAX_INPUTS {
                    return Err(fail("resource_limit"));
                }
                let owned = a
                    .inputs
                    .into_iter()
                    .map(Review::core)
                    .collect::<Result<Vec<_>>>()?;
                let borrowed = owned.iter().map(BorrowReview::input).collect::<Vec<_>>();
                let change = self
                    .owner()?
                    .prepare_review(&borrowed, a.operation_id)
                    .map_err(mapped)?;
                self.stage(change)
            }
            "prepareConversationSend" => {
                self.admit_plan()?;
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct A {
                    inputs: Vec<Send>,
                    operation: Operation,
                }
                let a: A = parse(&input)?;
                if a.inputs.len() > core::MAX_INPUTS {
                    return Err(fail("resource_limit"));
                }
                let owned = a
                    .inputs
                    .into_iter()
                    .map(|s| Ok((s.input.core()?, s.conversation.core())))
                    .collect::<Result<Vec<_>>>()?;
                let borrowed = owned
                    .iter()
                    .map(|(r, c)| core::ConversationInput {
                        input: r.input(),
                        conversation: c.clone(),
                    })
                    .collect::<Vec<_>>();
                let change = self
                    .owner()?
                    .prepare_conversation_send(&borrowed, a.operation.core())
                    .map_err(mapped)?;
                self.stage(change)
            }
            "prepareObservedSend" => {
                self.admit_plan()?;
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct A {
                    intent: Intent,
                    logical_id: [u8; 16],
                    surface: String,
                    site_id: String,
                    operation: Operation,
                }
                let a: A = parse(&input)?;
                let change = self
                    .owner()?
                    .prepare_observed_send(
                        a.intent.core(),
                        a.logical_id,
                        &a.surface,
                        &a.site_id,
                        a.operation.core(),
                    )
                    .map_err(mapped)?;
                self.stage(change)
            }
            "prepareDefinitelyNotSent" => {
                self.admit_plan()?;
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct A {
                    intent: Intent,
                    logical_id: [u8; 16],
                    operation: Operation,
                }
                let a: A = parse(&input)?;
                let change = self
                    .owner()?
                    .prepare_definitely_not_sent(a.intent.core(), a.logical_id, a.operation.core())
                    .map_err(mapped)?;
                self.stage(change)
            }
            "preparePolicyChanges" => {
                self.admit_plan()?;
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct A {
                    edits: core::PolicyEdits,
                    operation_id: [u8; 16],
                }
                let a: A = parse(&input)?;
                let change = self
                    .owner()?
                    .prepare_policy_changes(&a.edits, a.operation_id)
                    .map_err(mapped)?;
                self.stage(change)
            }
            "prepareRecovery" => {
                self.admit_plan()?;
                let operation: [u8; 16] = parse(&input)?;
                let change = self.owner()?.prepare_recovery(operation).map_err(mapped)?;
                self.stage(change)
            }
            "previews" | "planHeads" | "sendIntents" | "snapshotFormat" | "policyPreview" => {
                let h: u32 = parse(&input)?;
                let c = self.changes.get(&h).ok_or_else(|| fail("inactive_plan"))?;
                match method.as_str(){"previews"=>output(Value::Array(c.previews().map_err(mapped)?.iter().map(|p|json!({"text":p.text,"transformations":p.transformations.iter().map(|t|json!({"source":t.source.as_str(),"needsReview":t.needs_review,"sourceByteRange":t.source_byte_range,"sourceCodepointRange":t.source_codepoint_range,"sourceUtf16Range":t.source_utf16_range,"outputByteRange":t.output_byte_range,"outputCodepointRange":t.output_codepoint_range,"outputUtf16Range":t.output_utf16_range})).collect::<Vec<_>>()})).collect())),"planHeads"=>{let(p,s)=c.heads().map_err(mapped)?;output(json!({"parent":head(p),"candidate":head(s)}))},"sendIntents"=>output(serde_json::to_value(c.send_intents().map_err(mapped)?).map_err(|_|fail("internal_error"))?),"snapshotFormat"=>output(serde_json::to_value(c.snapshot_format().map_err(mapped)?).map_err(|_|fail("internal_error"))?),_=>output(serde_json::to_value(c.policy_preview().map_err(mapped)?).map_err(|_|fail("internal_error"))?)}
            }
            "discard" => {
                let h: u32 = parse(&input)?;
                let mut c = self
                    .changes
                    .remove(&h)
                    .ok_or_else(|| fail("inactive_plan"))?;
                c.discard();
                output(Value::Null)
            }
            "commit" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct A {
                    handle: u32,
                    approved: bool,
                    request_id: [u8; 16],
                }
                let a: A = parse(&input)?;
                let o = self.owner.as_mut().ok_or_else(|| fail("closed_owner"))?;
                let c = self
                    .changes
                    .get_mut(&a.handle)
                    .ok_or_else(|| fail("inactive_plan"))?;
                let result = o.commit(c, a.approved, a.request_id).map_err(mapped)?;
                let encoded = (|| {
                    let record_revision = o.record_revision().map_err(mapped)?.to_string();
                    output(
                        json!({"head":head(result.head),"receipt":receipt(&result.receipt),"recordRevision":record_revision,"previews":result.previews}),
                    )
                })();
                self.changes.remove(&a.handle);
                // Every fallible step after confirmed persistence belongs to
                // the uncertain-reply guard, including receipt projection.
                if encoded.is_err() {
                    if let Some(o) = self.owner.as_mut() {
                        o.lock();
                    }
                    self.changes.clear();
                    self.streams.clear();
                    return Err(mapped(bridge::Error::Vault(vault::Error::OutcomeUnknown {
                        request_id: a.request_id,
                    })));
                }
                encoded
            }
            "reconcile" => {
                let id: [u8; 16] = parse(&input)?;
                let r = self
                    .owner
                    .as_mut()
                    .ok_or_else(|| fail("closed_owner"))?
                    .reconcile(id)
                    .map_err(mapped)?;
                self.changes.clear();
                self.streams.clear();
                output(receipt(&r))
            }
            "restoreReport" | "restore" => {
                let text: String = parse(&input)?;
                if method == "restore" {
                    output(json!(self.owner()?.restore(&text).map_err(mapped)?))
                } else {
                    output(report(self.owner()?.restore_report(&text).map_err(mapped)?))
                }
            }
            "restoreInConversation" | "restoreReportInConversation" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct A {
                    text: String,
                    logical_id: [u8; 16],
                }
                let a: A = parse(&input)?;
                if method == "restoreInConversation" {
                    output(json!(
                        self.owner()?
                            .restore_in_conversation(&a.text, a.logical_id)
                            .map_err(mapped)?
                    ))
                } else {
                    output(report(
                        self.owner()?
                            .restore_report_in_conversation(&a.text, a.logical_id)
                            .map_err(mapped)?,
                    ))
                }
            }
            "operationsPage" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct A {
                    offset: usize,
                    limit: usize,
                }
                let a: A = parse(&input)?;
                let p = self
                    .owner()?
                    .operations_page(a.offset, a.limit)
                    .map_err(mapped)?;
                output(
                    json!({"records":p.records.iter().map(|r|json!({"id":r.id,"kind":r.kind.as_str(),"occurredAtMs":r.occurred_at_ms,"preparedInputs":r.prepared_inputs,"transformedSpans":r.transformed_spans})).collect::<Vec<_>>(),"total":p.total,"offset":p.offset,"nextOffset":p.next_offset}),
                )
            }
            "stream" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct A {
                    progressive: bool,
                    encoding: String,
                    logical_id: Option<[u8; 16]>,
                }
                let a: A = parse(&input)?;
                if self.streams.len() >= 32 {
                    return Err(fail("resource_limit"));
                }
                let encoding = match a.encoding.as_str() {
                    "utf8" => core::RestoreEncoding::Utf8,
                    "utf16" => core::RestoreEncoding::Utf16,
                    _ => return Err(fail("invalid_encoding")),
                };
                let o = self.owner()?;
                let stream = match (a.progressive, a.logical_id) {
                    (false, None) => {
                        Stream::Buffered(o.buffered_restore(encoding).map_err(mapped)?)
                    }
                    (true, None) => {
                        Stream::Progressive(o.progressive_restore(encoding).map_err(mapped)?)
                    }
                    (false, Some(id)) => Stream::Buffered(
                        o.buffered_restore_in_conversation(id, encoding)
                            .map_err(mapped)?,
                    ),
                    (true, Some(id)) => Stream::Progressive(
                        o.progressive_restore_in_conversation(id, encoding)
                            .map_err(mapped)?,
                    ),
                };
                let h = self.next;
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or_else(|| fail("resource_limit"))?;
                self.streams.insert(h, stream);
                output(json!({"handle":h}))
            }
            "streamCheck" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct A {
                    handle: u32,
                }
                let a: A = parse(&input)?;
                if !self.streams.contains_key(&a.handle) {
                    return Err(fail("inactive_stream"));
                }
                output(Value::Null)
            }
            "streamPoison" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct A {
                    handle: u32,
                    utf16: bool,
                }
                let a: A = parse(&input)?;
                let stream = self
                    .streams
                    .get_mut(&a.handle)
                    .ok_or_else(|| fail("inactive_stream"))?;
                match stream {
                    Stream::Buffered(s) => {
                        let result = if a.utf16 {
                            s.feed(&[])
                        } else {
                            s.feed_utf16(&[])
                        };
                        result.map_err(|e| mapped(e.into()))?;
                    }
                    Stream::Progressive(s) => {
                        let result = if a.utf16 {
                            s.feed(&[])
                        } else {
                            s.feed_utf16(&[])
                        };
                        result.map_err(stream_error)?;
                    }
                }
                Err(fail("invalid_encoding"))
            }
            "streamFeed" | "streamFinalize" | "streamReset" | "streamClose" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct A {
                    handle: u32,
                    bytes: Option<Vec<u8>>,
                    units: Option<Vec<u16>>,
                }
                let a: A = parse(&input)?;
                if method == "streamClose" {
                    self.streams
                        .remove(&a.handle)
                        .ok_or_else(|| fail("inactive_stream"))?;
                    return output(Value::Null);
                }
                let stream = self
                    .streams
                    .get_mut(&a.handle)
                    .ok_or_else(|| fail("inactive_stream"))?;
                match stream {
                    Stream::Buffered(s) => {
                        if method == "streamReset" {
                            s.reset().map_err(|e| mapped(e.into()))?;
                            output(Value::Null)
                        } else if method == "streamFinalize" {
                            output(report(s.finalize().map_err(|e| mapped(e.into()))?))
                        } else {
                            match (a.bytes, a.units) {
                                (Some(b), None) => s.feed(&b),
                                (None, Some(u)) => s.feed_utf16(&u),
                                _ => return Err(fail("invalid_transport")),
                            }
                            .map_err(|e| mapped(e.into()))?;
                            output(Value::Null)
                        }
                    }
                    Stream::Progressive(s) => {
                        if method == "streamReset" {
                            s.reset().map_err(stream_error)?;
                            output(Value::Null)
                        } else {
                            let chunk = if method == "streamFinalize" {
                                s.finalize()
                            } else {
                                match (a.bytes, a.units) {
                                    (Some(b), None) => s.feed(&b),
                                    (None, Some(u)) => s.feed_utf16(&u),
                                    _ => return Err(fail("invalid_transport")),
                                }
                            }
                            .map_err(stream_error)?;
                            output(
                                json!({"text":chunk.text,"complete":chunk.complete,"delivered":progress(chunk.delivered)}),
                            )
                        }
                    }
                }
            }
            _ => Err(fail("unsupported_method")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_binding_preserves_u64_and_unknown_request_error() {
        assert_eq!(
            head(core::Head {
                revision: u64::MAX,
                commit_id: [3; 16]
            })["revision"],
            u64::MAX.to_string()
        );
        let request_id = [9; 16];
        let error = mapped(bridge::Error::Vault(vault::Error::OutcomeUnknown {
            request_id,
        }));
        let value: Value = serde_json::from_str(&error.reason).unwrap();
        assert_eq!(value["code"], "outcome_unknown");
        assert_eq!(value["requestId"], json!(request_id));
        assert_eq!(value["requiresReopen"], true);
        assert!(value.get("text").is_none());
    }
    #[test]
    fn native_binding_rejects_wrong_thread_before_owner_access() {
        let other = thread::spawn(|| thread::current().id()).join().unwrap();
        let mut owner = NativeOwner {
            owner: None,
            tenant: "synthetic".into(),
            thread: other,
            changes: BTreeMap::new(),
            streams: BTreeMap::new(),
            next: 1,
        };
        let error = owner
            .call("synthetic".into(), "head".into(), "null".into())
            .unwrap_err();
        let value: Value = serde_json::from_str(&error.reason).unwrap();
        assert_eq!(value["code"], "wrong_thread");
    }
}

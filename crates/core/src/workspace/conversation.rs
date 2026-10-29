//! Saved send intentions, explicit host observations and scoped restoration.
use super::*;
use minicbor::{Decoder, Encoder};

/// Host-owned immutable identity. Core cannot prove site identity or the host's byte bound.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: PublicId,
    pub surface: String,
    pub site_id: Option<String>,
    pub site_id_byte_budget: usize,
}
impl Drop for Conversation {
    fn drop(&mut self) {
        self.surface.zeroize();
        self.site_id.zeroize();
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendIntentId {
    pub preparation_id: PublicId,
    pub input_index: usize,
}
pub struct ConversationInput<'a> {
    pub input: ReviewInput<'a>,
    pub conversation: Conversation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SendStatus {
    Pending,
    ObservedSent,
    DefinitelyNotSent,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationState {
    pub conversation: Conversation,
    pub pending_intents: usize,
    pub issued_markers: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Reference {
    label: String,
    code: String,
}
impl Drop for Reference {
    fn drop(&mut self) {
        self.label.zeroize();
        self.code.zeroize();
    }
}
#[derive(Clone)]
struct Intent {
    context: usize,
    references: Vec<Arc<Reference>>,
}
#[derive(Clone)]
struct Resolution {
    operation: PublicId,
    intent: SendIntentId,
    original: Intent,
    sent: bool,
}
#[derive(Clone, Default)]
pub(super) struct Conversations {
    contexts: Vec<Conversation>,
    intents: BTreeMap<SendIntentId, Intent>,
    resolutions: Vec<Resolution>,
    memberships: BTreeMap<(usize, Arc<Reference>), PublicId>,
}

fn checked(value: Option<usize>) -> Result<usize> {
    value.ok_or_else(|| error(ErrorCode::ResourceLimit))
}
fn width(value: u64) -> usize {
    // Canonical CBOR argument width (shared by unsigned integers/array/text heads).
    match value {
        0..=23 => 1,
        24..=255 => 2,
        256..=65535 => 3,
        65536..=4294967295 => 5,
        _ => 9,
    }
}
fn refs_bytes(references: &[Arc<Reference>]) -> Result<usize> {
    let mut size = width(references.len() as u64);
    for reference in references {
        size = checked(size.checked_add(
            1 + width(reference.label.len() as u64)
                + reference.label.len()
                + width(reference.code.len() as u64)
                + reference.code.len(),
        ))?;
    }
    Ok(size)
}
impl Conversations {
    pub(super) fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }
    fn context_index(&self, id: PublicId) -> Result<usize> {
        self.contexts
            .iter()
            .position(|context| context.id == id)
            .ok_or_else(|| error(ErrorCode::ConversationUnknown))
    }
    fn validate_context(context: &Conversation) -> Result<()> {
        if context.surface.is_empty()
            || context.surface.len() > MAX_BUNDLE_BYTES
            || !(1..=MAX_BUNDLE_BYTES).contains(&context.site_id_byte_budget)
            || context
                .site_id
                .as_ref()
                .is_some_and(|site| site.is_empty() || site.len() > context.site_id_byte_budget)
        {
            return Err(error(ErrorCode::InvalidConversation));
        }
        Ok(())
    }
    fn add_context(&mut self, context: &Conversation) -> Result<usize> {
        Self::validate_context(context)?;
        if let Some(index) = self.contexts.iter().position(|item| item.id == context.id) {
            if self.contexts[index] != *context {
                return Err(error(ErrorCode::InvalidConversation));
            }
            return Ok(index);
        }
        if context.site_id.as_ref().is_some_and(|site| {
            self.contexts
                .iter()
                .any(|item| item.surface == context.surface && item.site_id.as_ref() == Some(site))
        }) {
            return Err(error(ErrorCode::InvalidConversation));
        }
        self.contexts
            .try_reserve_exact(1)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        let index = self.contexts.len();
        self.contexts.push(context.clone());
        Ok(index)
    }
    pub(super) fn pending_count(&self) -> usize {
        self.intents.len()
    }
    fn pending_for(&self, context: usize) -> bool {
        self.intents
            .values()
            .any(|intent| intent.context == context)
    }
    fn eligible(&self, context: usize, canonical: &str) -> bool {
        let Some((label, code)) = canonical
            .strip_prefix("[[")
            .and_then(|value| value.strip_suffix("]]"))
            .and_then(|value| value.rsplit_once('_'))
        else {
            return false;
        };
        self.memberships.contains_key(&(
            context,
            Arc::new(Reference {
                label: label.to_owned(),
                code: code.to_owned(),
            }),
        ))
    }
    fn resolution_credit(&self, intent: &Intent) -> Result<usize> {
        let context = self
            .contexts
            .get(intent.context)
            .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
        // Never subtract a deleted intent. Each independently funded resolution
        // covers its entire terminal row, operation and worst-case memberships.
        let mut size = 1
            + 18
            + 18
            + width(MAX_INPUTS as u64)
            + width(intent.context as u64)
            + 1
            + refs_bytes(&intent.references)?;
        let mut row = Encoder::new(Vec::new());
        codec::encode_operation(
            &OperationRecord {
                id: [0; 16],
                kind: OperationKind::SendConfirmed,
                occurred_at_ms: Some(MAX_OPERATION_TIME_MS),
                prepared_inputs: 0,
                transformed_spans: 0,
            },
            &mut row,
        )
        .map_err(|_| error(ErrorCode::ResourceLimit))?;
        size = checked(size.checked_add(row.writer().len()))?;
        for reference in &intent.references {
            size = checked(size.checked_add(
                1 + width(intent.context as u64)
                    + width(reference.label.len() as u64)
                    + reference.label.len()
                    + width(reference.code.len() as u64)
                    + reference.code.len()
                    + 18,
            ))?;
        }
        if context.site_id.is_none() {
            size = checked(size.checked_add(
                width(context.site_id_byte_budget as u64) + context.site_id_byte_budget,
            ))?;
        }
        // Full maximum headers are a conservative encoder-derived bound on any
        // positive array/head/count-width change, including mixed resolutions.
        let mut headers = Encoder::new(Vec::new());
        headers
            .array(2)
            .and_then(|e| e.u64(u64::MAX))
            .and_then(|e| e.bytes(&[0; 16]))
            .and_then(|e| e.array(5))
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        for _ in 0..5 {
            headers
                .u64(u64::MAX)
                .map_err(|_| error(ErrorCode::ResourceLimit))?;
        }
        for _ in 0..5 {
            headers
                .array(MAX_BUNDLE_BYTES as u64)
                .map_err(|_| error(ErrorCode::ResourceLimit))?;
        }
        checked(size.checked_add(headers.writer().len()))
    }
    fn confirmation_bytes(&self) -> Result<usize> {
        self.intents.values().try_fold(0usize, |sum, intent| {
            checked(sum.checked_add(self.resolution_credit(intent)?))
        })
    }
}

fn encode_references<W: minicbor::encode::Write>(
    references: &[Arc<Reference>],
    e: &mut Encoder<W>,
) -> std::result::Result<(), minicbor::encode::Error<W::Error>> {
    e.array(references.len() as u64)?;
    for reference in references {
        e.array(2)?.str(&reference.label)?.str(&reference.code)?;
    }
    Ok(())
}
fn decode_count(
    d: &mut Decoder<'_>,
    bytes: usize,
    maximum: usize,
    minimum: usize,
) -> Result<usize> {
    let count = d
        .array()?
        .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
    codec::require(
        count <= maximum as u64 && count <= (bytes.saturating_sub(d.position()) / minimum) as u64,
    )?;
    Ok(count as usize)
}
fn decode_refs(
    d: &mut Decoder<'_>,
    bytes: usize,
    pool: &mut BTreeSet<Arc<Reference>>,
) -> Result<Vec<Arc<Reference>>> {
    let count = decode_count(d, bytes, MAX_ENTRIES, 10)?;
    let mut references = Vec::new();
    references
        .try_reserve_exact(count)
        .map_err(|_| error(ErrorCode::ResourceLimit))?;
    for _ in 0..count {
        codec::array(d, 2)?;
        let label = d.str()?;
        let code = d.str()?;
        codec::require(valid_type(label) && valid_code(code))?;
        let reference = Arc::new(Reference {
            label: label.to_owned(),
            code: code.to_owned(),
        });
        let reference = if let Some(existing) = pool.get(&reference) {
            Arc::clone(existing)
        } else {
            codec::require(pool.len() < MAX_ENTRIES)?;
            pool.insert(Arc::clone(&reference));
            reference
        };
        codec::require(
            references
                .last()
                .is_none_or(|previous| previous < &reference),
        )?;
        references.push(reference);
    }
    Ok(references)
}
impl Conversations {
    pub(super) fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut Encoder<W>,
    ) -> std::result::Result<(), minicbor::encode::Error<W::Error>> {
        e.array(4)?.array(self.contexts.len() as u64)?;
        for context in &self.contexts {
            e.array(4)?.bytes(&context.id)?.str(&context.surface)?;
            if let Some(site) = &context.site_id {
                e.str(site)?;
            } else {
                e.null()?;
            }
            e.u64(context.site_id_byte_budget as u64)?;
        }
        e.array(self.intents.len() as u64)?;
        for (id, intent) in &self.intents {
            e.array(4)?
                .bytes(&id.preparation_id)?
                .u64(id.input_index as u64)?
                .u64(intent.context as u64)?;
            encode_references(&intent.references, e)?;
        }
        e.array(self.resolutions.len() as u64)?;
        for resolution in &self.resolutions {
            e.array(6)?
                .bytes(&resolution.operation)?
                .bytes(&resolution.intent.preparation_id)?
                .u64(resolution.intent.input_index as u64)?
                .u64(resolution.original.context as u64)?
                .u8(u8::from(!resolution.sent))?;
            encode_references(&resolution.original.references, e)?;
        }
        e.array(self.memberships.len() as u64)?;
        for ((context, reference), first) in &self.memberships {
            e.array(4)?
                .u64(*context as u64)?
                .str(&reference.label)?
                .str(&reference.code)?
                .bytes(first)?;
        }
        Ok(())
    }
    pub(super) fn decode(d: &mut Decoder<'_>, bytes: usize) -> Result<Self> {
        codec::array(d, 4)?;
        let mut result = Self::default();
        let mut pool = BTreeSet::new();
        let count = decode_count(d, bytes, MAX_OPERATIONS, 22)?;
        result
            .contexts
            .try_reserve_exact(count)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        for _ in 0..count {
            codec::array(d, 4)?;
            let id = codec::id(d)?;
            let surface = d.str()?;
            codec::require(!surface.is_empty() && surface.len() <= MAX_BUNDLE_BYTES)?;
            let site = if d.datatype()? == minicbor::data::Type::Null {
                d.null()?;
                None
            } else {
                Some(d.str()?)
            };
            let budget = d.u64()?;
            codec::require((1..=MAX_BUNDLE_BYTES as u64).contains(&budget))?;
            codec::require(
                site.is_none_or(|value| !value.is_empty() && value.len() <= budget as usize),
            )?;
            result.contexts.push(Conversation {
                id,
                surface: surface.to_owned(),
                site_id: site.map(str::to_owned),
                site_id_byte_budget: budget as usize,
            });
        }
        // A 16-byte CBOR byte string occupies17 bytes, not18. Minimum
        // tuple sizes must never reject dense valid collections.
        let count = decode_count(d, bytes, MAX_OPERATIONS, 21)?;
        for _ in 0..count {
            codec::array(d, 4)?;
            let id = SendIntentId {
                preparation_id: codec::id(d)?,
                input_index: d.u32()? as usize,
            };
            let context = d.u32()? as usize;
            let references = decode_refs(d, bytes, &mut pool)?;
            codec::require(
                result
                    .intents
                    .insert(
                        id,
                        Intent {
                            context,
                            references,
                        },
                    )
                    .is_none(),
            )?;
        }
        let count = decode_count(d, bytes, MAX_OPERATIONS, 39)?;
        result
            .resolutions
            .try_reserve_exact(count)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        for _ in 0..count {
            codec::array(d, 6)?;
            let operation = codec::id(d)?;
            let intent = SendIntentId {
                preparation_id: codec::id(d)?,
                input_index: d.u32()? as usize,
            };
            let context = d.u32()? as usize;
            let outcome = d.u8()?;
            codec::require(outcome <= 1)?;
            let references = decode_refs(d, bytes, &mut pool)?;
            result.resolutions.push(Resolution {
                operation,
                intent,
                original: Intent {
                    context,
                    references,
                },
                sent: outcome == 0,
            });
        }
        let count = decode_count(d, bytes, MAX_BUNDLE_BYTES, 28)?;
        for _ in 0..count {
            codec::array(d, 4)?;
            let context = d.u32()? as usize;
            let label = d.str()?;
            let code = d.str()?;
            codec::require(valid_type(label) && valid_code(code))?;
            let reference = Arc::new(Reference {
                label: label.to_owned(),
                code: code.to_owned(),
            });
            let reference = if let Some(existing) = pool.get(&reference) {
                Arc::clone(existing)
            } else {
                codec::require(pool.len() < MAX_ENTRIES)?;
                pool.insert(Arc::clone(&reference));
                reference
            };
            let first = codec::id(d)?;
            codec::require(
                result
                    .memberships
                    .insert((context, reference), first)
                    .is_none(),
            )?;
        }
        Ok(result)
    }
    pub(super) fn validate(&self, state: &State) -> Result<()> {
        let bad = || error(ErrorCode::InvalidSnapshot);
        let by_id: BTreeMap<_, _> = state
            .operations
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id, (index, row)))
            .collect();
        let aliases: BTreeMap<_, _> = state
            .entries
            .iter()
            .map(|(identity, entry)| {
                (
                    (identity.entity_type.as_str(), entry.code.as_str()),
                    entry.issuance,
                )
            })
            .collect();
        let mut logical = BTreeSet::new();
        let mut bound = BTreeSet::new();
        for context in &self.contexts {
            Self::validate_context(context).map_err(|_| bad())?;
            codec::require(logical.insert(context.id))?;
            if let Some(site) = &context.site_id {
                codec::require(bound.insert((&context.surface, site)))?;
            }
        }
        let mut all = BTreeMap::new();
        for (id, intent) in &self.intents {
            codec::require(all.insert(*id, (intent, None)).is_none())?;
        }
        let mut resolution_ids = BTreeSet::new();
        let mut last_resolution = None;
        for resolution in &self.resolutions {
            codec::require(
                all.insert(resolution.intent, (&resolution.original, Some(resolution)))
                    .is_none()
                    && resolution_ids.insert(resolution.operation),
            )?;
            let (index, row) = by_id.get(&resolution.operation).ok_or_else(bad)?;
            codec::require(
                row.kind
                    == if resolution.sent {
                        OperationKind::SendConfirmed
                    } else {
                        OperationKind::SendNotSent
                    },
            )?;
            codec::require(last_resolution.is_none_or(|previous| previous < *index))?;
            last_resolution = Some(*index);
        }
        codec::require(
            state
                .operations
                .iter()
                .filter(|row| row.kind.is_send_resolution())
                .count()
                == self.resolutions.len(),
        )?;
        let mut creating = vec![None; self.contexts.len()];
        let mut born = vec![0usize; state.operations.len()];
        let mut ended = vec![0usize; state.operations.len()];
        let mut members = BTreeMap::new();
        for (id, (intent, resolution)) in all {
            let (index, row) = by_id.get(&id.preparation_id).ok_or_else(bad)?;
            codec::require(
                row.kind == OperationKind::ConversationPrepare
                    && id.input_index < row.prepared_inputs as usize
                    && intent.context < self.contexts.len(),
            )?;
            born[*index] += 1;
            let origin = (*index, id.input_index);
            if creating[intent.context].is_none_or(|previous| origin < previous) {
                creating[intent.context] = Some(origin);
            }
            for reference in &intent.references {
                let issuance = aliases
                    .get(&(reference.label.as_str(), reference.code.as_str()))
                    .ok_or_else(bad)?;
                if let Some((issued_index, _)) = by_id.get(issuance) {
                    codec::require(issued_index <= index)?;
                }
                // Unknown pre-upgrade issuance remains valid under the existing
                // incomplete-history contract; a known future issuance does not.
            }
            if let Some(resolution) = resolution {
                let (end, _) = by_id.get(&resolution.operation).ok_or_else(bad)?;
                codec::require(end > index)?;
                ended[*end] += 1;
                if resolution.sent {
                    codec::require(self.contexts[intent.context].site_id.is_some())?;
                    for reference in &intent.references {
                        let key = (intent.context, Arc::clone(reference));
                        let replace = members
                            .get(&key)
                            .is_none_or(|previous: &PublicId| by_id[previous].0 > *end);
                        if replace {
                            members.insert(key, resolution.operation);
                        }
                    }
                }
            }
        }
        codec::require(
            creating.iter().all(Option::is_some)
                && creating.windows(2).all(|pair| pair[0] < pair[1])
                && members == self.memberships,
        )?;
        let mut pending = 0usize;
        let mut conversation_started = false;
        for (index, row) in state.operations.iter().enumerate() {
            conversation_started |= row.kind == OperationKind::ConversationPrepare;
            codec::require(if row.kind == OperationKind::ConversationPrepare {
                born[index] == row.prepared_inputs as usize
            } else {
                born[index] == 0
            })?;
            pending = checked(pending.checked_add(born[index])).map_err(|_| bad())?;
            pending = pending.checked_sub(ended[index]).ok_or_else(bad)?;
            let limit = if row.kind == OperationKind::Recovery || row.kind.is_send_resolution() {
                MAX_OPERATIONS
            } else {
                MAX_ORDINARY_OPERATIONS
            };
            // Older imported history was not subject to C2's funded-intent
            // admission. Do not invent stricter legacy read semantics.
            codec::require(!conversation_started || index + 1 + pending <= limit)?;
        }
        codec::require(pending == self.pending_count())?;
        Ok(())
    }
}

/// Derived pools, never trusted caller-supplied credit totals.
pub(super) fn budget(state: &State) -> Result<usize> {
    if state.conversations.contexts.is_empty() {
        return Ok(0);
    }
    let mut remaining = RESERVED_RECOVERY_OPERATIONS;
    let mut bytes = codec::recovery_byte_reserve()?;
    for (index, operation) in state.operations.iter().enumerate() {
        if operation.kind == OperationKind::Recovery {
            if remaining > 0 {
                let after_revision = state
                    .head
                    .revision
                    .checked_sub((state.operations.len() - 1 - index) as u64)
                    .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
                let before_revision = after_revision
                    .checked_sub(1)
                    .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
                let mut encoder = Encoder::new(Vec::new());
                codec::encode_operation(operation, &mut encoder)
                    .map_err(|_| error(ErrorCode::ResourceLimit))?;
                let growth = encoder.writer().len() + width(after_revision)
                    - width(before_revision)
                    + 2 * (width((index + 1) as u64) - width(index as u64));
                bytes = bytes
                    .checked_sub(growth)
                    .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
                remaining -= 1;
                if remaining == 0 {
                    bytes = 0;
                }
            }
        } else if !operation.kind.is_send_resolution() {
            remaining = RESERVED_RECOVERY_OPERATIONS;
            bytes = codec::recovery_byte_reserve()?;
        }
    }
    let pending = state.conversations.pending_count();
    let needed = checked(pending.checked_add(remaining))?;
    if u64::MAX - state.head.revision < needed as u64
        || state
            .operations
            .len()
            .checked_add(pending)
            .is_none_or(|count| count > MAX_OPERATIONS)
    {
        return Err(error(ErrorCode::ResourceLimit));
    }
    checked(bytes.checked_add(state.conversations.confirmation_bytes()?))
}

fn operation_context(operation: OperationContext) -> Result<()> {
    if operation.occurred_at_ms > MAX_OPERATION_TIME_MS {
        return Err(error(ErrorCode::InvalidOperation));
    }
    Ok(())
}
pub(super) fn scoped_context(state: &State, id: PublicId) -> Result<usize> {
    let index = state.conversations.context_index(id)?;
    if state.conversations.pending_for(index) {
        return Err(error(ErrorCode::ConversationPending));
    }
    if state.conversations.contexts[index].site_id.is_none() {
        return Err(error(ErrorCode::ConversationUnknown));
    }
    Ok(index)
}
pub(super) fn eligible(state: &State, context: usize, canonical: &str) -> bool {
    state.conversations.eligible(context, canonical)
}
impl PreparedChange {
    /// Intent IDs are owned metadata, never evidence of a send or permission to release.
    pub fn send_intents(&self) -> Result<Vec<SendIntentId>> {
        let pending = self.pending()?;
        if pending.candidate.head == pending.parent
            || !pending
                .candidate
                .operations
                .last()
                .is_some_and(|row| row.kind == OperationKind::ConversationPrepare)
        {
            return Ok(Vec::new());
        }
        Ok(pending
            .candidate
            .conversations
            .intents
            .keys()
            .filter(|id| id.preparation_id == pending.candidate.head.commit_id)
            .copied()
            .collect())
    }
}
impl Workspace {
    pub fn conversation(&self, id: PublicId) -> Result<ConversationState> {
        let contexts = &self.state()?.conversations;
        let index = contexts.context_index(id)?;
        Ok(ConversationState {
            conversation: contexts.contexts[index].clone(),
            pending_intents: contexts
                .intents
                .values()
                .filter(|intent| intent.context == index)
                .count(),
            issued_markers: contexts
                .memberships
                .keys()
                .filter(|(context, _)| *context == index)
                .count(),
        })
    }
    /// Resolve a trusted exact site identity without normalization or fallback.
    pub fn conversation_for_site(&self, surface: &str, site_id: &str) -> Result<PublicId> {
        self.state()?
            .conversations
            .contexts
            .iter()
            .find(|context| {
                context.surface == surface && context.site_id.as_deref() == Some(site_id)
            })
            .map(|context| context.id)
            .ok_or_else(|| error(ErrorCode::ConversationUnknown))
    }
    pub fn send_intent_status(&self, id: SendIntentId) -> Result<SendStatus> {
        let contexts = &self.state()?.conversations;
        if contexts.intents.contains_key(&id) {
            return Ok(SendStatus::Pending);
        }
        contexts
            .resolutions
            .iter()
            .find(|resolution| resolution.intent == id)
            .map(|resolution| {
                if resolution.sent {
                    SendStatus::ObservedSent
                } else {
                    SendStatus::DefinitelyNotSent
                }
            })
            .ok_or_else(|| error(ErrorCode::InvalidSendIntent))
    }
    /// Save these intentions and mappings before transport. The host must substantiate
    /// the declared site-ID bound and trusted attempt association before first send.
    pub fn prepare_conversation_send(
        &self,
        inputs: &[ConversationInput<'_>],
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        operation_context(operation)?;
        if inputs.is_empty() {
            return Err(error(ErrorCode::InvalidBatch));
        }
        if inputs.len() > MAX_INPUTS {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let mut context_bytes = 0usize;
        for (index, input) in inputs.iter().enumerate() {
            Conversations::validate_context(&input.conversation).map_err(|mut failure| {
                failure.input_index = Some(index);
                failure
            })?;
            context_bytes = checked(
                context_bytes
                    .checked_add(input.conversation.surface.len())
                    .and_then(|count| {
                        count
                            .checked_add(input.conversation.site_id.as_ref().map_or(0, String::len))
                    }),
            )?;
            if context_bytes > MAX_BUNDLE_BYTES {
                return Err(error(ErrorCode::ResourceLimit));
            }
        }
        let reviewed: Vec<_> = inputs.iter().map(|input| input.input).collect();
        let mut plan = self.prepare_review_internal(
            &reviewed,
            operation.id,
            Some(operation.occurred_at_ms),
            true,
        )?;
        let pending = plan
            .pending
            .as_mut()
            .ok_or_else(|| error(ErrorCode::InactivePlan))?;
        let candidate = &mut pending.candidate;
        candidate
            .operations
            .last_mut()
            .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?
            .kind = OperationKind::ConversationPrepare;
        let aliases: BTreeMap<_, _> = candidate
            .entries
            .iter()
            .map(|(identity, entry)| {
                (
                    marker(&identity.entity_type, &entry.code),
                    Arc::new(Reference {
                        label: identity.entity_type.clone(),
                        code: entry.code.clone(),
                    }),
                )
            })
            .collect();
        for (index, input) in inputs.iter().enumerate() {
            let context = candidate
                .conversations
                .add_context(&input.conversation)
                .map_err(|mut failure| {
                    failure.input_index = Some(index);
                    failure
                })?;
            let spans = marker_spans(&pending.previews[index].text)?;
            let mut references = BTreeSet::new();
            for span in spans {
                let canonical = span
                    .canonical
                    .ok_or_else(|| error(ErrorCode::InvalidMarker))?;
                references.insert(Arc::clone(
                    aliases
                        .get(&canonical)
                        .ok_or_else(|| error(ErrorCode::InvalidMarker))?,
                ));
            }
            candidate.conversations.intents.insert(
                SendIntentId {
                    preparation_id: operation.id,
                    input_index: index,
                },
                Intent {
                    context,
                    references: references.into_iter().collect(),
                },
            );
        }
        // First-save admission includes the new intentions' rows, bytes and revisions.
        if candidate
            .operations
            .len()
            .checked_add(candidate.conversations.pending_count())
            .is_none_or(|count| count > MAX_ORDINARY_OPERATIONS)
        {
            return Err(error(ErrorCode::ResourceLimit));
        }
        candidate.conversations.validate(candidate)?;
        pending.bundle = Zeroizing::new(codec::encode_for_record(candidate, false)?);
        Ok(plan)
    }
    /// The host asserts an actual matching successful send, not a click/copy/manual
    /// confirmation. Core cannot prove that assertion. Binding and issuance save together.
    pub fn prepare_observed_send(
        &self,
        intent: SendIntentId,
        logical_id: PublicId,
        surface: &str,
        site_id: &str,
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        self.prepare_send_resolution(intent, logical_id, Some((surface, site_id)), operation)
    }
    /// Trusted definite non-delivery only; timeout/missing acknowledgment is not proof.
    /// This creates no issuance and must never be exposed as manual success correction.
    pub fn prepare_definitely_not_sent(
        &self,
        intent: SendIntentId,
        logical_id: PublicId,
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        self.prepare_send_resolution(intent, logical_id, None, operation)
    }
    fn prepare_send_resolution(
        &self,
        id: SendIntentId,
        logical_id: PublicId,
        observed: Option<(&str, &str)>,
        operation: OperationContext,
    ) -> Result<PreparedChange> {
        operation_context(operation)?;
        let state = self.state()?;
        state.validate_operation_id(operation.id, false)?;
        let original = state
            .conversations
            .intents
            .get(&id)
            .ok_or_else(|| error(ErrorCode::InvalidSendIntent))?;
        let context = &state.conversations.contexts[original.context];
        if context.id != logical_id {
            return Err(error(ErrorCode::InvalidSendIntent));
        }
        if let Some((surface, site)) = observed {
            if context.surface != surface
                || site.is_empty()
                || site.len() > context.site_id_byte_budget
                || context.site_id.as_ref().is_some_and(|bound| bound != site)
                || state
                    .conversations
                    .contexts
                    .iter()
                    .enumerate()
                    .any(|(index, other)| {
                        index != original.context
                            && other.surface == surface
                            && other.site_id.as_deref() == Some(site)
                    })
            {
                return Err(error(ErrorCode::InvalidConversation));
            }
        }
        let mut candidate = state.clone();
        let original = candidate
            .conversations
            .intents
            .remove(&id)
            .ok_or_else(|| error(ErrorCode::InvalidSendIntent))?;
        let sent = observed.is_some();
        if let Some((_, site)) = observed {
            let context = &mut candidate.conversations.contexts[original.context];
            if context.site_id.is_none() {
                context.site_id = Some(site.to_owned());
            }
            for reference in &original.references {
                candidate
                    .conversations
                    .memberships
                    .entry((original.context, Arc::clone(reference)))
                    .or_insert(operation.id);
            }
        }
        candidate.conversations.resolutions.push(Resolution {
            operation: operation.id,
            intent: id,
            original,
            sent,
        });
        candidate.head = Head {
            revision: state
                .head
                .revision
                .checked_add(1)
                .ok_or_else(|| error(ErrorCode::ResourceLimit))?,
            commit_id: operation.id,
        };
        candidate.record_operation(OperationRecord {
            id: operation.id,
            kind: if sent {
                OperationKind::SendConfirmed
            } else {
                OperationKind::SendNotSent
            },
            occurred_at_ms: Some(operation.occurred_at_ms),
            prepared_inputs: 0,
            transformed_spans: 0,
        })?;
        candidate.conversations.validate(&candidate)?;
        let bundle = Zeroizing::new(codec::encode_for_record(&candidate, false)?);
        Ok(PreparedChange {
            pending: Some(Pending {
                lifetime: Arc::clone(&self.lifetime),
                parent: state.head,
                parent_snapshot_version: state.snapshot_version,
                candidate,
                bundle,
                previews: Vec::new(),
                policy_preview: None,
            }),
        })
    }
    /// Explicit scoped restoration never falls back to the compatibility API.
    pub fn restore_report_in_conversation(
        &self,
        text: &str,
        logical_id: PublicId,
    ) -> Result<RestoreReport> {
        let state = self.state()?;
        let context = scoped_context(state, logical_id)?;
        restore_report_scoped(state, text, Some(context))
    }
    pub fn restore_in_conversation(&self, text: &str, logical_id: PublicId) -> Result<String> {
        let mut report = self.restore_report_in_conversation(text, logical_id)?;
        report.text.take().ok_or_else(|| {
            let mut failure = error(ErrorCode::InvalidMarker);
            failure.marker_issues = std::mem::take(&mut report.issues);
            failure
        })
    }
    pub fn buffered_restore_in_conversation(
        &self,
        logical_id: PublicId,
        encoding: RestoreEncoding,
    ) -> Result<BufferedRestore> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| error(ErrorCode::ClosedWorkspace))?;
        let context = scoped_context(state, logical_id)?;
        Ok(BufferedRestore::new(
            Arc::clone(state),
            Arc::clone(&self.lifetime),
            Arc::clone(&self.read_epoch),
            encoding,
            Some(context),
        ))
    }
    pub fn progressive_restore_in_conversation(
        &self,
        logical_id: PublicId,
        encoding: RestoreEncoding,
    ) -> Result<ProgressiveRestore> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| error(ErrorCode::ClosedWorkspace))?;
        let context = scoped_context(state, logical_id)?;
        ProgressiveRestore::new(
            Arc::clone(state),
            Arc::clone(&self.lifetime),
            Arc::clone(&self.read_epoch),
            encoding,
            Some(context),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn operation(index: usize) -> OperationContext {
        OperationContext {
            id: (index as u128).to_be_bytes(),
            occurred_at_ms: MAX_OPERATION_TIME_MS,
        }
    }
    fn context() -> Conversation {
        Conversation {
            id: [10; 16],
            surface: "synthetic_chat".into(),
            site_id: None,
            site_id_byte_budget: 64,
        }
    }
    fn send(workspace: &Workspace, index: usize) -> Result<PreparedChange> {
        workspace.prepare_conversation_send(
            &[ConversationInput {
                input: ReviewInput {
                    text: "plain",
                    scanner_findings: &[],
                    user_selections: &[],
                    corrections: &[],
                },
                conversation: context(),
            }],
            operation(index),
        )
    }
    fn save(workspace: &mut Workspace, plan: &mut PreparedChange) {
        workspace
            .activate_saved(
                plan,
                plan.expected_head().unwrap(),
                plan.candidate_head().unwrap(),
            )
            .unwrap();
    }
    fn history(count: usize) -> Workspace {
        let mut state = Workspace::create([1; 16], [2; 16])
            .unwrap()
            .state()
            .unwrap()
            .clone();
        state.operations = (0..count)
            .map(|index| OperationRecord {
                id: operation(index + 1).id,
                kind: if index == 0 {
                    OperationKind::Create
                } else {
                    OperationKind::Prepare
                },
                occurred_at_ms: None,
                prepared_inputs: u64::from(index > 0),
                transformed_spans: 0,
            })
            .collect();
        state.operation_counts = operations::summarize(&state.operations).unwrap();
        state.head = Head {
            revision: (count - 1) as u64,
            commit_id: operation(count).id,
        };
        Workspace::open_bundle(&codec::encode(&state).unwrap()).unwrap()
    }
    fn legacy(revision: u64) -> Workspace {
        let mut state = Workspace::create([1; 16], [2; 16])
            .unwrap()
            .state()
            .unwrap()
            .clone();
        state.snapshot_version = 2;
        state.head.revision = revision;
        state.operations.clear();
        state.operation_counts = OperationCounts::default();
        Workspace::open_bundle(&codec::encode(&state).unwrap()).unwrap()
    }
    fn padded_legacy(target: usize) -> Workspace {
        let mut state = legacy(5).state().unwrap().clone();
        for index in 0..17 {
            let length = if index == 16 { 100_000 } else { 1_000_000 };
            let display = format!("{index:02}{}", "x".repeat(length - 2));
            state.entries.insert(
                Identity {
                    entity_type: "CUSTOM".into(),
                    key: identity_key(&display),
                },
                Entry {
                    display,
                    code: format!("{:06}", index + 1),
                    issuance: [(index + 10) as u8; 16],
                    counter: (index + 1) as u64,
                },
            );
        }
        state.counters.insert("CUSTOM".into(), 17);
        // These old custom aliases are deliberately excluded from recall; padding
        // proves snapshot admission, not recall's independent index-work budget.
        state.rules.insert(
            "CUSTOM".into(),
            policy::RuleRecord {
                rule: TypeRule::Keep,
                operation: [200; 16],
            },
        );
        let initial = codec::encode(&state).unwrap().len();
        let (_, mut entry) = state.entries.pop_last().unwrap();
        let length = entry.display.len() + target - initial;
        assert!((65_536..=MAX_SOURCE_BYTES).contains(&length));
        entry.display = format!("16{}", "x".repeat(length - 2));
        state.entries.insert(
            Identity {
                entity_type: "CUSTOM".into(),
                key: identity_key(&entry.display),
            },
            entry,
        );
        let bytes = codec::encode(&state).unwrap();
        assert_eq!(bytes.len(), target);
        Workspace::open_bundle(&bytes).unwrap()
    }
    #[test]
    fn cbor_width_matches_actual_encoder_at_every_boundary() {
        for value in [
            0,
            23,
            24,
            255,
            256,
            65535,
            65536,
            u32::MAX as u64,
            u32::MAX as u64 + 1,
            u64::MAX,
        ] {
            let mut e = Encoder::new(Vec::new());
            e.u64(value).unwrap();
            assert_eq!(width(value), e.writer().len());
        }
    }
    #[test]
    fn ordinary_credit_and_all_256_recoveries_fit_without_stealing_a_slot() {
        let full = history(MAX_ORDINARY_OPERATIONS - 1);
        let original = full.export_bundle().unwrap();
        assert_eq!(
            send(&full, 200_000).err().unwrap().code,
            ErrorCode::ResourceLimit
        );
        assert_eq!(full.export_bundle().unwrap(), original);
        let mut workspace = history(MAX_ORDINARY_OPERATIONS - 2);
        let mut first = send(&workspace, 200_001).unwrap();
        let intent = first.send_intents().unwrap()[0];
        save(&mut workspace, &mut first);
        assert_eq!(
            workspace.operations().unwrap().len(),
            MAX_ORDINARY_OPERATIONS - 1
        );
        assert_eq!(
            workspace
                .prepare_outcome(
                    OperationOutcome {
                        kind: OutcomeKind::CopyConfirmed,
                        issue_count: 0
                    },
                    operation(200_002)
                )
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
        for index in 0..RESERVED_RECOVERY_OPERATIONS {
            let mut recovery = workspace
                .prepare_recovery_with_operation(operation(300_000 + index))
                .unwrap();
            save(&mut workspace, &mut recovery);
        }
        assert_eq!(workspace.operations().unwrap().len(), MAX_OPERATIONS - 1);
        assert_eq!(
            workspace
                .prepare_recovery_with_operation(operation(400_000))
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
        let before = workspace.export_bundle().unwrap();
        let mut confirmed = workspace
            .prepare_observed_send(
                intent,
                [10; 16],
                "synthetic_chat",
                "thread_a",
                operation(400_001),
            )
            .unwrap();
        assert_eq!(workspace.export_bundle().unwrap(), before);
        save(&mut workspace, &mut confirmed);
        assert_eq!(workspace.operations().unwrap().len(), MAX_OPERATIONS);
        assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 1);
        assert!(Workspace::open_bundle(&workspace.export_bundle().unwrap()).is_ok());
    }
    #[test]
    fn legacy_revision_credits_preserve_reads_and_fund_both_saves_plus_recovery() {
        for revision in [u64::MAX, u64::MAX - 1, u64::MAX - 256] {
            let workspace = legacy(revision);
            let bytes = workspace.export_bundle().unwrap();
            let mut noop = workspace
                .prepare("plain", &[], operation(500_000).id)
                .unwrap();
            assert_eq!(noop.export_candidate_bundle().unwrap(), bytes);
            noop.discard();
            assert_eq!(
                send(&workspace, 500_001).err().unwrap().code,
                ErrorCode::ResourceLimit
            );
            assert_eq!(workspace.export_bundle().unwrap(), bytes);
        }
        let mut workspace = legacy(u64::MAX - 258);
        let mut first = send(&workspace, 500_002).unwrap();
        let intent = first.send_intents().unwrap()[0];
        save(&mut workspace, &mut first);
        for index in 0..RESERVED_RECOVERY_OPERATIONS {
            let mut recovery = workspace
                .prepare_recovery_with_operation(operation(600_000 + index))
                .unwrap();
            save(&mut workspace, &mut recovery);
        }
        let mut final_save = workspace
            .prepare_observed_send(
                intent,
                [10; 16],
                "synthetic_chat",
                "thread_a",
                operation(700_000),
            )
            .unwrap();
        save(&mut workspace, &mut final_save);
        assert_eq!(workspace.head().unwrap().revision, u64::MAX);
        assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 1);
        assert!(Workspace::open_bundle(&workspace.export_bundle().unwrap()).is_ok());
    }
    #[test]
    fn decoder_recomputes_credits_and_rejects_removed_or_forged_intents() {
        let mut workspace = Workspace::create_with_operation([1; 16], operation(1)).unwrap();
        let mut first = send(&workspace, 2).unwrap();
        save(&mut workspace, &mut first);
        let original = workspace.state().unwrap().clone();
        for mutation in 0..6 {
            let mut state = original.clone();
            match mutation {
                0 => state.conversations.intents.clear(),
                1 => state.conversations.contexts[0].site_id_byte_budget = MAX_BUNDLE_BYTES,
                2 => state.conversations.contexts.push(context()),
                3 => {
                    state
                        .conversations
                        .intents
                        .values_mut()
                        .next()
                        .unwrap()
                        .context = 99
                }
                4 => state.operations.last_mut().unwrap().kind = OperationKind::Prepare,
                _ => {
                    state.conversations.memberships.insert(
                        (
                            0,
                            Arc::new(Reference {
                                label: "PERSON".into(),
                                code: "AAAAAA".into(),
                            }),
                        ),
                        operation(1).id,
                    );
                }
            }
            assert!(
                Workspace::open_bundle(&codec::encode(&state).unwrap()).is_err(),
                "mutation {mutation}"
            );
        }
    }
    #[test]
    fn exact_byte_credit_edge_funds_confirmation_after_all_recovery_allowances() {
        let calibration =
            padded_legacy(MAX_BUNDLE_BYTES - codec::recovery_byte_reserve().unwrap() - 1024);
        let before = calibration.export_bundle().unwrap().len();
        let plan = send(&calibration, 800_000).unwrap();
        let pending = plan.pending().unwrap();
        let growth = pending.bundle.len() - before;
        let target = MAX_BUNDLE_BYTES - growth - budget(&pending.candidate).unwrap();
        drop(plan);
        drop(calibration);
        let blocked = padded_legacy(target + 1);
        let bytes = blocked.export_bundle().unwrap();
        assert_eq!(
            send(&blocked, 800_000).err().unwrap().code,
            ErrorCode::ResourceLimit
        );
        assert_eq!(blocked.export_bundle().unwrap(), bytes);
        drop(bytes);
        drop(blocked);
        let mut workspace = padded_legacy(target);
        let mut first = send(&workspace, 800_000).unwrap();
        assert_eq!(
            first.export_candidate_bundle().unwrap().len()
                + budget(&first.pending().unwrap().candidate).unwrap(),
            MAX_BUNDLE_BYTES
        );
        let intent = first.send_intents().unwrap()[0];
        save(&mut workspace, &mut first);
        assert_eq!(
            workspace
                .prepare_outcome(
                    OperationOutcome {
                        kind: OutcomeKind::CopyConfirmed,
                        issue_count: 0
                    },
                    operation(800_001)
                )
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
        for index in 0..RESERVED_RECOVERY_OPERATIONS {
            let mut recovery = workspace
                .prepare_recovery_with_operation(operation(900_000 + index))
                .unwrap();
            save(&mut workspace, &mut recovery);
        }
        let mut final_save = workspace
            .prepare_observed_send(
                intent,
                [10; 16],
                "synthetic_chat",
                &"t".repeat(64),
                operation(1_000_000),
            )
            .unwrap();
        save(&mut workspace, &mut final_save);
        let bytes = workspace.export_bundle().unwrap();
        assert!(bytes.len() <= MAX_BUNDLE_BYTES);
        assert_eq!(workspace.counts().unwrap().confirmed_handoffs, 1);
        assert!(Workspace::open_bundle(&bytes).is_ok());
    }
    #[test]
    fn a_later_alias_cannot_backfill_an_earlier_confirmed_intent() {
        let mut workspace = Workspace::create_with_operation([1; 16], operation(1)).unwrap();
        let mut seed = workspace
            .prepare_with_operation(
                "Before Name",
                &[WorkspaceFinding::new(
                    "PERSON",
                    "Before Name",
                    TextRange { start: 0, end: 11 },
                )],
                operation(2),
            )
            .unwrap();
        let before_marker = seed.preview().unwrap().text.clone();
        save(&mut workspace, &mut seed);
        let mut first = workspace
            .prepare_conversation_send(
                &[ConversationInput {
                    input: ReviewInput {
                        text: &before_marker,
                        scanner_findings: &[],
                        user_selections: &[],
                        corrections: &[],
                    },
                    conversation: context(),
                }],
                operation(3),
            )
            .unwrap();
        let intent = first.send_intents().unwrap()[0];
        save(&mut workspace, &mut first);
        let mut confirmed = workspace
            .prepare_observed_send(intent, [10; 16], "synthetic_chat", "thread_a", operation(4))
            .unwrap();
        save(&mut workspace, &mut confirmed);
        let mut later = workspace
            .prepare_with_operation(
                "Later Name",
                &[WorkspaceFinding::new(
                    "PERSON",
                    "Later Name",
                    TextRange { start: 0, end: 10 },
                )],
                operation(5),
            )
            .unwrap();
        let marker = later.preview().unwrap().text.clone();
        save(&mut workspace, &mut later);
        assert_eq!(
            workspace
                .restore_in_conversation(&before_marker, [10; 16])
                .unwrap(),
            "Before Name"
        );
        let mut forged = workspace.state().unwrap().clone();
        let (identity, entry) = forged
            .entries
            .iter()
            .find(|(_, entry)| entry.issuance == operation(5).id)
            .unwrap();
        let reference = Arc::new(Reference {
            label: identity.entity_type.clone(),
            code: entry.code.clone(),
        });
        forged.conversations.resolutions[0].original.references = vec![Arc::clone(&reference)];
        forged.conversations.memberships.clear();
        forged
            .conversations
            .memberships
            .insert((0, reference), operation(4).id);
        assert_eq!(
            Workspace::open_bundle(&codec::encode(&forged).unwrap())
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidSnapshot
        );
        assert_eq!(
            workspace
                .restore_in_conversation(&marker, [10; 16])
                .err()
                .unwrap()
                .code,
            ErrorCode::InvalidMarker
        );
    }
    #[test]
    fn unknown_preupgrade_alias_issuance_remains_eligible_for_new_intents() {
        let mut workspace = Workspace::create_with_operation([1; 16], operation(1)).unwrap();
        let mut seed = workspace
            .prepare_with_operation(
                "Legacy Name",
                &[WorkspaceFinding::new(
                    "PERSON",
                    "Legacy Name",
                    TextRange { start: 0, end: 11 },
                )],
                operation(2),
            )
            .unwrap();
        let marker = seed.preview().unwrap().text.clone();
        save(&mut workspace, &mut seed);
        let mut old = workspace.state().unwrap().clone();
        old.snapshot_version = 2;
        old.operations.clear();
        old.operation_counts = OperationCounts::default();
        workspace = Workspace::open_bundle(&codec::encode(&old).unwrap()).unwrap();
        let mut first = workspace
            .prepare_conversation_send(
                &[ConversationInput {
                    input: ReviewInput {
                        text: &marker,
                        scanner_findings: &[],
                        user_selections: &[],
                        corrections: &[],
                    },
                    conversation: context(),
                }],
                operation(3),
            )
            .unwrap();
        let intent = first.send_intents().unwrap()[0];
        save(&mut workspace, &mut first);
        let mut confirmed = workspace
            .prepare_observed_send(intent, [10; 16], "synthetic_chat", "thread_a", operation(4))
            .unwrap();
        save(&mut workspace, &mut confirmed);
        let reopened = Workspace::open_bundle(&workspace.export_bundle().unwrap()).unwrap();
        assert_eq!(
            reopened.restore_in_conversation(&marker, [10; 16]).unwrap(),
            "Legacy Name"
        );
        assert_eq!(
            reopened.history_coverage().unwrap().pre_upgrade_history,
            PreUpgradeHistory::Unknown
        );
    }
}

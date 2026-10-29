//! Fixed development schema, decoded with bounds before allocation and canonical re-encoding.
use super::*;
use minicbor::{Decoder, Encoder};

struct BoundedWriter(Vec<u8>);
impl minicbor::encode::Write for BoundedWriter {
    type Error = ();
    fn write_all(&mut self, bytes: &[u8]) -> std::result::Result<(), ()> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_BUNDLE_BYTES)
        {
            return Err(());
        }
        self.0.try_reserve(bytes.len()).map_err(|_| ())?;
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}

pub(super) fn encode(state: &State) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new(BoundedWriter(Vec::new()));
    encode_into(state, &mut encoder).map_err(|_| error(ErrorCode::ResourceLimit))?;
    Ok(encoder.into_writer().0)
}

fn encode_into(
    state: &State,
    e: &mut Encoder<BoundedWriter>,
) -> std::result::Result<(), minicbor::encode::Error<()>> {
    e.map(if state.snapshot_version == 1 { 7 } else { 9 })?
        .u8(0)?
        .u8(state.snapshot_version)?
        .u8(1)?
        .bytes(&state.namespace)?
        .u8(2)?
        .array(2)?
        .u64(state.head.revision)?
        .bytes(&state.head.commit_id)?
        .u8(3)?
        .array(state.entries.len() as u64)?;
    for (identity, entry) in &state.entries {
        e.array(5)?
            .str(&identity.entity_type)?
            .str(&entry.display)?
            .str(&entry.code)?
            .bytes(&entry.issuance)?
            .u64(entry.counter)?;
    }
    e.u8(4)?.array(state.counters.len() as u64)?;
    for (label, counter) in &state.counters {
        e.array(2)?.str(label)?.u64(*counter)?;
    }
    e.u8(5)?
        .array(0)?
        .u8(6)?
        .map(2)?
        .u8(0)?
        .str("synthetic_v1")?
        .u8(1)?
        .array(0)?;
    if state.snapshot_version == 2 {
        e.u8(8)?.array(state.operations.len() as u64)?;
        for operation in &state.operations {
            e.array(5)?
                .bytes(&operation.id)?
                .u8(operation.kind.code())?;
            if let Some(time) = operation.occurred_at_ms {
                e.u64(time)?;
            } else {
                e.null()?;
            }
            e.u64(operation.prepared_inputs)?
                .u64(operation.transformed_spans)?;
        }
        let counts = state.operation_counts;
        e.u8(10)?
            .array(5)?
            .u64(counts.operations)?
            .u64(counts.prepared_inputs)?
            .u64(counts.transformed_spans)?
            .u64(counts.confirmed_handoffs)?
            .u64(counts.reported_blocked_attempts)?;
    }
    Ok(())
}

impl From<minicbor::decode::Error> for WorkspaceError {
    fn from(_: minicbor::decode::Error) -> Self {
        error(ErrorCode::InvalidSnapshot)
    }
}
fn require(condition: bool) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error(ErrorCode::InvalidSnapshot))
    }
}
fn array(d: &mut Decoder<'_>, size: u64) -> Result<()> {
    require(d.array()? == Some(size))
}
fn key(d: &mut Decoder<'_>, value: u8) -> Result<()> {
    require(d.u8()? == value)
}
fn id(d: &mut Decoder<'_>) -> Result<PublicId> {
    d.bytes()?
        .try_into()
        .map_err(|_| error(ErrorCode::InvalidSnapshot))
}

pub(super) fn decode(bytes: &[u8]) -> Result<State> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(error(ErrorCode::ResourceLimit));
    }
    let mut d = Decoder::new(bytes);
    let field_count = d.map()?;
    key(&mut d, 0)?;
    let snapshot_version = d.u8()?;
    require(matches!(
        (snapshot_version, field_count),
        (1, Some(7)) | (2, Some(9))
    ))?;
    key(&mut d, 1)?;
    let namespace = id(&mut d)?;
    key(&mut d, 2)?;
    array(&mut d, 2)?;
    let head = Head {
        revision: d.u64()?,
        commit_id: id(&mut d)?,
    };
    key(&mut d, 3)?;
    let count = d
        .array()?
        .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
    require(count <= MAX_ENTRIES as u64)?;
    let mut entries = BTreeMap::new();
    let mut codes = BTreeSet::new();
    let mut maximums: BTreeMap<String, u64> = BTreeMap::new();
    for _ in 0..count {
        array(&mut d, 5)?;
        let entity_type = d.str()?;
        require(valid_type(entity_type))?;
        let display = d.str()?;
        let key = identity_key(display);
        require(
            !key.is_empty()
                && display.len() <= MAX_SOURCE_BYTES
                && marker_spans(display)?
                    .iter()
                    .all(|span| span.canonical.is_none()),
        )?;
        let code = d.str()?;
        require(valid_code(code))?;
        let issuance = id(&mut d)?;
        let counter = d.u64()?;
        require(counter > 0)?;
        require(codes.insert(code.to_owned()))?;
        maximums
            .entry(entity_type.to_owned())
            .and_modify(|v| *v = (*v).max(counter))
            .or_insert(counter);
        let identity = Identity {
            entity_type: entity_type.to_owned(),
            key,
        };
        let entry = Entry {
            display: display.to_owned(),
            code: code.to_owned(),
            issuance,
            counter,
        };
        require(entries.insert(identity, entry).is_none())?;
    }
    require(entries.is_empty() || head.revision > 0)?;
    key(&mut d, 4)?;
    let count = d
        .array()?
        .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
    require(count <= MAX_ENTRIES as u64 && count == maximums.len() as u64)?;
    let mut counters = BTreeMap::new();
    for _ in 0..count {
        array(&mut d, 2)?;
        let label = d.str()?;
        require(valid_type(label))?;
        let counter = d.u64()?;
        require(maximums.get(label).is_some_and(|n| counter >= *n))?;
        require(counters.insert(label.to_owned(), counter).is_none())?;
    }
    key(&mut d, 5)?;
    array(&mut d, 0)?;
    key(&mut d, 6)?;
    require(d.map()? == Some(2))?;
    key(&mut d, 0)?;
    require(d.str()? == "synthetic_v1")?;
    key(&mut d, 1)?;
    array(&mut d, 0)?;
    let mut operations = Vec::new();
    let mut operation_counts = OperationCounts::default();
    if snapshot_version == 2 {
        key(&mut d, 8)?;
        let count = d
            .array()?
            .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
        require(count > 0 && count <= MAX_OPERATIONS as u64)?;
        for _ in 0..count {
            array(&mut d, 5)?;
            let operation_id = id(&mut d)?;
            let kind = OperationKind::from_code(d.u8()?)?;
            let occurred_at_ms = if d.datatype()? == minicbor::data::Type::Null {
                d.null()?;
                None
            } else {
                Some(d.u64()?)
            };
            operations.push(OperationRecord {
                id: operation_id,
                kind,
                occurred_at_ms,
                prepared_inputs: d.u64()?,
                transformed_spans: d.u64()?,
            });
        }
        key(&mut d, 10)?;
        array(&mut d, 5)?;
        operation_counts = OperationCounts {
            operations: d.u64()?,
            prepared_inputs: d.u64()?,
            transformed_spans: d.u64()?,
            confirmed_handoffs: d.u64()?,
            reported_blocked_attempts: d.u64()?,
        };
        require(operations::summarize(&operations)? == operation_counts)?;
        require(
            operations
                .last()
                .is_some_and(|operation| operation.id == head.commit_id),
        )?;
        if operations[0].kind == OperationKind::Create {
            require(head.revision.checked_add(1) == Some(operations.len() as u64))?;
        } else {
            require(head.revision >= operations.len() as u64)?;
        }
    }
    require(d.position() == bytes.len())?;
    let state = State {
        snapshot_version,
        operations,
        operation_counts,
        namespace,
        head,
        entries,
        counters,
    };
    let canonical = Zeroizing::new(encode(&state)?);
    require(canonical.as_slice() == bytes)?;
    Ok(state)
}

#[cfg(test)]
mod operation_tests {
    use super::*;
    fn state() -> State {
        Workspace::create([1; 16], [2; 16])
            .unwrap()
            .state
            .clone()
            .unwrap()
    }
    fn invalid(state: State) {
        let bytes = encode(&state).unwrap();
        assert_eq!(
            decode(&bytes).err().unwrap().code,
            ErrorCode::InvalidSnapshot
        );
    }
    #[test]
    fn operation_metadata_counts_and_head_are_strictly_cross_checked() {
        let mut value = state();
        value.operation_counts.operations = 2;
        invalid(value);
        let mut value = state();
        value.operation_counts.confirmed_handoffs = 1;
        invalid(value);
        let mut value = state();
        value.operations[0].occurred_at_ms = Some(MAX_OPERATION_TIME_MS + 1);
        invalid(value);
        let mut value = state();
        value.operations[0].prepared_inputs = 1;
        invalid(value);
        let mut value = state();
        value.head.commit_id = [9; 16];
        invalid(value);
        let mut value = state();
        value.head.revision = 1;
        invalid(value);
        let mut value = state();
        value.operations.clear();
        value.operation_counts = OperationCounts::default();
        invalid(value);
        let mut value = state();
        value.operations.push(value.operations[0].clone());
        invalid(value);
        let mut value = state();
        value.operations[0].kind = OperationKind::Prepare;
        value.operations[0].prepared_inputs = MAX_INPUTS as u64 + 1;
        invalid(value);
        let mut value = state();
        value.operations[0].kind = OperationKind::CopyConfirmed;
        invalid(value);
        let mut value = state();
        value.operations[0].kind = OperationKind::Prepare;
        value.operations[0].prepared_inputs = 1;
        value.operations[0].transformed_spans = MAX_FINDINGS as u64 + 1;
        invalid(value);
        let mut value = state();
        value.operations = vec![value.operations[0].clone(); MAX_OPERATIONS + 1];
        invalid(value);
    }
    #[test]
    fn unknown_kind_and_duplicate_top_level_keys_reject_before_canonical_import() {
        let bytes = encode(&state()).unwrap();
        let mut d = Decoder::new(&bytes);
        d.map().unwrap();
        for _ in 0..7 {
            d.u8().unwrap();
            d.skip().unwrap();
        }
        let key_offset = d.position();
        assert_eq!(d.u8().unwrap(), 8);
        d.array().unwrap();
        d.array().unwrap();
        d.bytes().unwrap();
        let kind_offset = d.position();
        let mut unknown = bytes.clone();
        unknown[kind_offset] = 23;
        assert_eq!(
            decode(&unknown).err().unwrap().code,
            ErrorCode::InvalidSnapshot
        );
        let mut duplicate = bytes;
        duplicate[key_offset] = 6;
        assert_eq!(
            decode(&duplicate).err().unwrap().code,
            ErrorCode::InvalidSnapshot
        );
    }
    #[test]
    fn retention_limit_never_evicts_an_old_id_or_mutates_on_failure() {
        let mut value = state();
        value.operations = (0..MAX_OPERATIONS)
            .map(|n| {
                let mut id = [0; 16];
                id[..8].copy_from_slice(&(n as u64).to_be_bytes());
                OperationRecord {
                    id,
                    kind: OperationKind::UserCancelled,
                    occurred_at_ms: Some(0),
                    prepared_inputs: 0,
                    transformed_spans: 0,
                }
            })
            .collect();
        value.head = Head {
            revision: MAX_OPERATIONS as u64,
            commit_id: value.operations.last().unwrap().id,
        };
        value.operation_counts = operations::summarize(&value.operations).unwrap();
        let saved = encode(&value).unwrap();
        let workspace = Workspace::open_bundle(&saved).unwrap();
        let failure = workspace
            .prepare_outcome(
                OperationOutcome {
                    kind: OutcomeKind::UserCancelled,
                    issue_count: 0,
                },
                OperationContext {
                    id: [9; 16],
                    occurred_at_ms: 0,
                },
            )
            .err()
            .unwrap();
        assert_eq!(failure.code, ErrorCode::ResourceLimit);
        assert_eq!(workspace.export_bundle().unwrap(), saved);
        assert_eq!(
            workspace
                .prepare_with_operation(
                    "",
                    &[],
                    OperationContext {
                        id: [0; 16],
                        occurred_at_ms: 0
                    }
                )
                .err()
                .unwrap()
                .code,
            ErrorCode::OperationReused
        );
    }
}

//! Fixed development schema, decoded with bounds before allocation and canonical re-encoding.
use super::*;
use minicbor::{Decoder, Encoder};

struct BoundedWriter(Zeroizing<Vec<u8>>);
impl minicbor::encode::Write for BoundedWriter {
    type Error = ();
    fn write_all(&mut self, bytes: &[u8]) -> std::result::Result<(), ()> {
        // The same bounded growth used by response adapters wipes replaced
        // backings; ownership also clears partially encoded state on failure.
        private_bytes::append(&mut self.0, bytes, MAX_BUNDLE_BYTES).map_err(|_| ())
    }
}

pub(super) fn encode(state: &State) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new(BoundedWriter(Zeroizing::new(Vec::new())));
    encode_into(state, &mut encoder).map_err(|_| error(ErrorCode::ResourceLimit))?;
    let mut writer = encoder.into_writer();
    // Successful exports deliberately transfer their plaintext bytes to the caller.
    Ok(std::mem::take(&mut *writer.0))
}

fn encode_into(
    state: &State,
    e: &mut Encoder<BoundedWriter>,
) -> std::result::Result<(), minicbor::encode::Error<()>> {
    e.map(if state.snapshot_version == 1 { 7 } else { 8 })?
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
    e.u8(5)?.array(0)?.u8(6)?.map(2)?.u8(0)?;
    if state.snapshot_version == 1 {
        e.str("synthetic_v1")?.u8(1)?.array(0)?;
    } else {
        e.str(policy::PROFILE)?
            .u8(1)?
            .array(state.rules.len() as u64)?;
        for (label, record) in &state.rules {
            e.array(3)?
                .str(label)?
                .u8(record.rule.code())?
                .bytes(&record.operation)?;
        }
        e.u8(7)?.array(state.choices.len() as u64)?;
        for (identity, record) in &state.choices {
            e.array(4)?
                .str(&identity.entity_type)?
                .str(&record.display)?
                .u8(record.disposition.code())?
                .bytes(&record.operation)?;
        }
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
    let fields = d.map()?;
    key(&mut d, 0)?;
    let snapshot_version = d.u8()?;
    require(matches!(
        (snapshot_version, fields),
        (1, Some(7)) | (2, Some(8))
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
        require(display.len() <= MAX_SOURCE_BYTES)?;
        let key = identity_key(display);
        require(
            !key.is_empty()
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
    require(
        d.str()?
            == if snapshot_version == 1 {
                "synthetic_v1"
            } else {
                policy::PROFILE
            },
    )?;
    key(&mut d, 1)?;
    let issuance_ids: BTreeSet<_> = entries.values().map(|entry| entry.issuance).collect();
    let mut rules = BTreeMap::new();
    let mut choices = BTreeMap::new();
    if snapshot_version == 1 {
        array(&mut d, 0)?;
    } else {
        let count = d
            .array()?
            .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
        require(count <= MAX_POLICY_ITEMS as u64)?;
        for _ in 0..count {
            array(&mut d, 3)?;
            let label = d.str()?;
            require(valid_type(label))?;
            let rule = match d.u8()? {
                0 => TypeRule::Hide,
                1 => TypeRule::Keep,
                _ => return Err(error(ErrorCode::InvalidSnapshot)),
            };
            require(!(protected_type(label) && rule == TypeRule::Keep))?;
            let operation = id(&mut d)?;
            require(!issuance_ids.contains(&operation))?;
            require(
                rules
                    .insert(label.to_owned(), policy::RuleRecord { rule, operation })
                    .is_none(),
            )?;
        }
        key(&mut d, 7)?;
        let count = d
            .array()?
            .ok_or_else(|| error(ErrorCode::InvalidSnapshot))?;
        require(count <= MAX_POLICY_ITEMS as u64)?;
        for _ in 0..count {
            array(&mut d, 4)?;
            let label = d.str()?;
            let display = d.str()?;
            let identity = policy::choice_identity(label, display)
                .map_err(|_| error(ErrorCode::InvalidSnapshot))?;
            let disposition = match d.u8()? {
                0 => ChoiceDisposition::Keep,
                1 => ChoiceDisposition::NotSensitive,
                _ => return Err(error(ErrorCode::InvalidSnapshot)),
            };
            require(!(protected_type(label) && disposition == ChoiceDisposition::Keep))?;
            let operation = id(&mut d)?;
            require(!issuance_ids.contains(&operation))?;
            require(
                choices
                    .insert(
                        identity,
                        policy::ChoiceRecord {
                            display: display.to_owned(),
                            disposition,
                            operation,
                        },
                    )
                    .is_none(),
            )?;
        }
        require((rules.is_empty() && choices.is_empty()) || head.revision > 0)?;
    }
    require(d.position() == bytes.len())?;
    let state = State {
        snapshot_version,
        rules,
        choices,
        namespace,
        head,
        entries,
        counters,
    };
    let canonical = Zeroizing::new(encode(&state)?);
    require(canonical.as_slice() == bytes)?;
    Ok(state)
}

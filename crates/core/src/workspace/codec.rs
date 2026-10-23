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
    e.map(7)?
        .u8(0)?
        .u8(1)?
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
            .str(&identity.original)?
            .str(&entry.marker)?
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
    require(d.map()? == Some(7))?;
    key(&mut d, 0)?;
    require(d.u8()? == 1)?;
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
    let mut markers = BTreeSet::new();
    let mut maximums: BTreeMap<String, u64> = BTreeMap::new();
    for _ in 0..count {
        array(&mut d, 5)?;
        let entity_type = d.str()?;
        require(valid_type(entity_type))?;
        let original = d.str()?;
        require(
            !original.is_empty()
                && original.len() <= MAX_SOURCE_BYTES
                && !original.contains(RESERVED_INTRODUCER),
        )?;
        let full = d.str()?;
        require(full.len() <= 140)?;
        let issuance = id(&mut d)?;
        let counter = d.u64()?;
        require(counter > 0 && full == marker(namespace, issuance, entity_type, counter))?;
        require(markers.insert(full.to_owned()))?;
        maximums
            .entry(entity_type.to_owned())
            .and_modify(|v| *v = (*v).max(counter))
            .or_insert(counter);
        let identity = Identity {
            entity_type: entity_type.to_owned(),
            original: original.to_owned(),
        };
        let entry = Entry {
            marker: full.to_owned(),
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
    require(d.position() == bytes.len())?;
    let state = State {
        namespace,
        head,
        entries,
        counters,
    };
    let canonical = Zeroizing::new(encode(&state)?);
    require(canonical.as_slice() == bytes)?;
    Ok(state)
}

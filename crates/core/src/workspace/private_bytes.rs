//! Bounded private storage shared by atomic and progressive response adapters.
use super::*;

// Never let Vec reallocation free an old private backing without wiping it.
pub(super) fn append(buffer: &mut Zeroizing<Vec<u8>>, bytes: &[u8], limit: usize) -> Result<()> {
    let needed = buffer.len().saturating_add(bytes.len());
    if needed > limit {
        return Err(error(ErrorCode::ResourceLimit));
    }
    if needed > buffer.capacity() {
        let capacity = needed.max(buffer.capacity().saturating_mul(2)).min(limit);
        let mut replacement = Zeroizing::new(Vec::new());
        replacement
            .try_reserve_exact(capacity)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        replacement.extend_from_slice(buffer);
        let previous = std::mem::replace(buffer, replacement);
        drop(previous);
    }
    buffer.extend_from_slice(bytes);
    Ok(())
}

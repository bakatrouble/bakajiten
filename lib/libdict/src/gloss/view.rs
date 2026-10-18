//! Gloss artifact readers: header gate, section split, zero-copy
//! index views, postcard entry decoding. Writers live in [`file`].
use super::file::{DictIndex, GLOSS_VERSION, HEADER_LEN};
use super::record::EntryRecord;
use rkyv::Archived;

/// Borrowed artifact: zero-copy index plus the entries blob it
/// frames. Both borrow the same mapped bytes.
pub struct GlossView<'b> {
    pub index: &'b Archived<DictIndex>,
    pub blob: &'b [u8],
}

/// Decode one entry frame (postcard). Corrupt frames are a pack bug —
/// callers log and leave the row empty, never unwind through paint.
pub fn decode_entry(frame: &[u8]) -> Result<EntryRecord, String> {
    postcard::from_bytes(frame).map_err(|e| format!("gloss: entry {e}"))
}

/// Deserialize [`gloss_to_bytes`] output (owned index decode plus all
/// entry frames). Rejects wrong magic and versions, truncation, and
/// trailing garbage.
pub fn gloss_from_bytes(
    bytes: &[u8],
    magic: &[u8; 4],
) -> Result<(DictIndex, Vec<EntryRecord>), String> {
    let (rkyv_bytes, blob) = split_sections(bytes, magic)?;
    // Owned decode tolerates unaligned input (copies to an aligned
    // buffer first); the zero-copy path below stays strict by design.
    let index: DictIndex = if rkyv_bytes.as_ptr().align_offset(16) == 0 {
        rkyv::from_bytes::<DictIndex, rkyv::rancor::Error>(rkyv_bytes)
            .map_err(|e| format!("gloss: rkyv {e}"))?
    } else {
        let mut aligned = rkyv::util::AlignedVec::<16>::new();
        aligned.extend_from_slice(rkyv_bytes);
        rkyv::from_bytes::<DictIndex, rkyv::rancor::Error>(&aligned)
            .map_err(|e| format!("gloss: rkyv {e}"))?
    };
    let mut entries = Vec::with_capacity(index.len());
    for i in 0..index.len() {
        let start = index.entry_offsets[i] as usize;
        let end = index.entry_offsets[i + 1] as usize;
        let frame = blob
            .get(start..end)
            .ok_or_else(|| "gloss: offset out of range".to_string())?;
        entries.push(decode_entry(frame)?);
    }
    Ok((index, entries))
}

/// Zero-copy view of [`gloss_to_bytes`] output: validates header,
/// lengths, and the index archive, then borrows both sections in
/// place. Requires 16-aligned input (mmap pages and
/// [`rkyv::util::AlignedVec`] are); entry frames need no alignment.
/// See [`crate::kanji::kanji_view`] for the alignment contract.
pub fn gloss_view<'b>(bytes: &'b [u8], magic: &[u8; 4]) -> Result<GlossView<'b>, String> {
    let (rkyv_bytes, blob) = split_sections(bytes, magic)?;
    let index = rkyv::access::<Archived<DictIndex>, rkyv::rancor::Error>(rkyv_bytes)
        .map_err(|e| format!("gloss: rkyv {e}"))?;
    Ok(GlossView { index, blob })
}

/// Header-only check (magic, version, exact length): instant,
/// faults at most the first pages. This does NOT validate the
/// archive itself — run [`gloss_view`] once before unchecked access.
pub fn gloss_header(bytes: &[u8], magic: &[u8; 4]) -> Result<(), String> {
    split_sections(bytes, magic).map(|_| ())
}

/// Header + section split shared by all readers.
fn split_sections<'a>(bytes: &'a [u8], magic: &[u8; 4]) -> Result<(&'a [u8], &'a [u8]), String> {
    if bytes.len() < HEADER_LEN {
        return Err("gloss: short header".into());
    }
    if &bytes[..4] != magic {
        return Err("gloss: missing magic".into());
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != GLOSS_VERSION {
        return Err(format!(
            "gloss: unsupported version {version} (this build reads {})",
            GLOSS_VERSION
        ));
    }
    let rkyv_len = u64::from_le_bytes(bytes[8..16].try_into().expect("header checked")) as usize;
    let blob_len = u64::from_le_bytes(bytes[16..24].try_into().expect("header checked")) as usize;
    if bytes.len() != HEADER_LEN + rkyv_len + blob_len {
        if bytes.len() < HEADER_LEN + rkyv_len + blob_len {
            return Err("gloss: truncated".into());
        }
        return Err("gloss: trailing garbage".into());
    }
    Ok((
        &bytes[HEADER_LEN..HEADER_LEN + rkyv_len],
        &bytes[HEADER_LEN + rkyv_len..],
    ))
}

/// rkyv index section of header-checked bytes (`None` on torn
/// input). The exact-length gate at load makes this infallible in
/// practice; the hot path treats `None` as a missing row.
pub fn index_section(bytes: &[u8]) -> Option<&[u8]> {
    sections_unchecked(bytes).map(|(rkyv_bytes, _)| rkyv_bytes)
}

/// Both sections of header-checked bytes (`None` on torn input).
pub fn sections_unchecked(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let rkyv_len = u64::from_le_bytes(bytes.get(8..16)?.try_into().ok()?) as usize;
    let blob_len = u64::from_le_bytes(bytes.get(16..24)?.try_into().ok()?) as usize;
    let both = bytes.get(HEADER_LEN..HEADER_LEN + rkyv_len + blob_len)?;
    Some((&both[..rkyv_len], &both[rkyv_len..]))
}

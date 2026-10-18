use super::record::KanjiRecord;
use serde::{Deserialize, Serialize};

/// Magic prefix of kanji-record files.
pub const KANJI_MAGIC: &[u8; 4] = b"KJD1";
/// Kanji-record format version written (and only accepted) here.
pub const KANJI_VERSION: u32 = 1;

/// Header length: magic + version (u32) + payload length (u64),
/// all little-endian. 16 bytes total, so the payload starts at a
/// 16-aligned offset whenever the file itself is 16-aligned —
/// future-proof against wider archive alignments (see gloss).
const HEADER_LEN: usize = 16;

/// One kanji file: source versions plus every character (strokes for
/// all KanjiVG entries; kanjidic2 metadata empty when absent).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KanjiFile {
    /// KanjiVG source version (`--kanjivg-version`, else "unknown":
    /// checkouts carry no marker).
    pub kanjivg_version: String,
    /// kanjidic2 `<date_of_creation>`.
    pub kanjidic2_version: String,
    pub records: Vec<KanjiRecord>,
}

/// Serialize a kanji file to the file layout (see module docs):
/// header plus an rkyv payload. The payload is position-independent,
/// so the file can be mmapped and read without decoding (see
/// [`kanji_view`]).
pub fn kanji_to_bytes(file: &KanjiFile) -> Vec<u8> {
    let payload = rkyv::to_bytes::<rkyv::rancor::Error>(file).expect("rkyv encode cannot fail");
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(KANJI_MAGIC);
    out.extend_from_slice(&KANJI_VERSION.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

/// Deserialize [`kanji_to_bytes`] output (owned decode). Rejects wrong
/// magic and versions, truncation, and trailing garbage.
pub fn kanji_from_bytes(bytes: &[u8]) -> Result<KanjiFile, String> {
    let payload = split_payload(bytes)?;
    // Owned decode tolerates unaligned input; the zero-copy path
    // below stays strict by design.
    if payload.as_ptr().align_offset(16) != 0 {
        let mut aligned = rkyv::util::AlignedVec::<16>::new();
        aligned.extend_from_slice(payload);
        return rkyv::from_bytes::<KanjiFile, rkyv::rancor::Error>(&aligned)
            .map_err(|e| format!("kanji: rkyv {e}"));
    }
    rkyv::from_bytes::<KanjiFile, rkyv::rancor::Error>(payload)
        .map_err(|e| format!("kanji: rkyv {e}"))
}

/// Zero-copy view of [`kanji_to_bytes`] output: validates header,
/// length, and archive, then borrows the payload in place — no
/// allocation, no per-record decoding. The input may be an mmap
/// region (page-aligned, hence rkyv-compatible); a plain `Vec<u8>`
/// works only when 16-byte aligned (copy into an
/// [`rkyv::util::AlignedVec`] first otherwise).
pub fn kanji_view(bytes: &[u8]) -> Result<&ArchivedKanjiFile, String> {
    let payload = split_payload(bytes)?;
    rkyv::access::<ArchivedKanjiFile, rkyv::rancor::Error>(payload)
        .map_err(|e| format!("kanji: rkyv {e}"))
}

/// Header-only check (magic, version, exact length): instant,
/// faults at most the first page. This does NOT validate the
/// archive itself — same trust model as the gloss side (own pack
/// output, immutable bytes): run [`kanji_view`] for full validation.
pub fn kanji_header(bytes: &[u8]) -> Result<(), String> {
    split_payload(bytes).map(|_| ())
}

/// Header + exact-length check shared by both readers.
fn split_payload(bytes: &[u8]) -> Result<&[u8], String> {
    if bytes.len() < HEADER_LEN {
        return Err("kanji: short header".into());
    }
    if &bytes[..4] != KANJI_MAGIC {
        return Err("kanji: missing KJD1 magic".into());
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != KANJI_VERSION {
        return Err(format!(
            "kanji: unsupported version {version} (this build reads {})",
            KANJI_VERSION
        ));
    }
    let len = u64::from_le_bytes(bytes[8..16].try_into().expect("header checked")) as usize;
    if bytes.len() != HEADER_LEN + len {
        if bytes.len() < HEADER_LEN + len {
            return Err("kanji: truncated".into());
        }
        return Err("kanji: trailing garbage".into());
    }
    Ok(&bytes[HEADER_LEN..])
}

/// Save a kanji file to disk (atomically — see `gloss::file`).
pub fn save_kanji(path: &std::path::Path, file: &KanjiFile) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, kanji_to_bytes(file))?;
    std::fs::rename(&tmp, path)
}

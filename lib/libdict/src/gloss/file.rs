//! Split gloss artifact: an rkyv index section (everything search
//! needs — pool, prefix indexes, per-record key ids and entry
//! offsets) plus a postcard entries blob (compact per-record frames).
//! Search runs zero-copy on the mapped index; `entry_at` decodes one
//! postcard frame per materialized row (microseconds — rows
//! materialize lazily as they scroll into view). Every stored string
//! lives once in `str_blob`, referenced by `StrRef` offset/length
//! (pool entries, gram keys, entry texts alike).
use super::record::{EntryRecord, IndexRecord, StrRef};
use crate::gloss::search::ArchivedDictionaryFile;
use rkyv::Archived;
use serde::{Deserialize, Serialize};

pub const GLOSS_JMDICT_MAGIC: &[u8; 4] = b"GLD1";
pub const GLOSS_JMNEDICT_MAGIC: &[u8; 4] = b"GLN1";
/// Monolingual Japanese dictionary (`kokugo.bin`, packed from
/// `kokugo.jsonl`): same schema as the EDRDG files, separate
/// magic so loaders never confuse it with JMdict.
pub const GLOSS_KOKUGO_MAGIC: &[u8; 4] = b"GLK1";
pub const GLOSS_VERSION: u32 = 1;

/// Header: magic + version (u32) + rkyv length (u64) + blob length
/// (u64) + reserved (u64), all little-endian. 32 bytes total, so the
/// rkyv section starts 16-aligned whenever the file itself is
/// 16-aligned (mmap regions and [`rkyv::util::AlignedVec`] always
/// are) — rkyv access requires it; the blob needs no alignment.
pub(crate) const HEADER_LEN: usize = 32;

/// Search-critical section (rkyv, zero-copy). One per file; files
/// stay separate (`GLD1` vs `GLN1`) on the shared schema.
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
pub struct DictIndex {
    /// Source version stamp (JMdict/JMnedict date or mtime).
    pub version: String,
    /// Deduped headword forms (kanji, readings, keys), first
    /// occurrence order, as blob slices. Key ids index into this.
    pub pool: Vec<StrRef>,
    /// The single string store: every pool string, gram key, and
    /// entry text concatenated once (substring-shared at pack).
    pub str_blob: Vec<u8>,
    pub forward_index: Vec<IndexRecord>,
    pub reverse_index: Vec<IndexRecord>,
    /// Japanese substring postings: unique char-trigrams over
    /// reb/keb keys (blob slices, sorted by content);
    /// `trigram_offsets` (`trigrams + 1`, last = total) slices
    /// `trigram_postings` (record indices, ascending per trigram).
    /// Short keys contribute no trigrams; sub-trigram queries fall
    /// back to a bounded scan (see `search`).
    pub trigrams: Vec<StrRef>,
    pub trigram_offsets: Vec<u32>,
    pub trigram_postings: Vec<u32>,
    /// Meaning substring postings: same shape over normalized
    /// (lowercased) gloss text.
    pub meaning_trigrams: Vec<StrRef>,
    pub meaning_offsets: Vec<u32>,
    pub meaning_postings: Vec<u32>,
    /// Concatenated per-record key pool ids.
    pub key_ids: Vec<u32>,
    /// `records + 1` offsets into `key_ids` (last = total).
    pub key_offsets: Vec<u32>,
    /// Per-record word frequency for the relevance sort (zkanji
    /// points over ke_pri/re_pri, saturated to u16).
    pub freqs: Vec<u16>,
    /// `records + 1` offsets into the entries blob (last = its
    /// length): entry `i` is `blob[offsets[i]..offsets[i+1]]`.
    pub entry_offsets: Vec<u32>,
}

impl DictIndex {
    /// Record count (all three per-record arrays agree by
    /// construction — pack asserts it).
    pub fn len(&self) -> usize {
        self.entry_offsets.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ArchivedDictionaryFile for Archived<DictIndex> {
    fn forward_index(&self) -> &[Archived<IndexRecord>] {
        &self.forward_index
    }
    fn reverse_index(&self) -> &[Archived<IndexRecord>] {
        &self.reverse_index
    }
    fn pool(&self) -> &[Archived<StrRef>] {
        &self.pool
    }
    fn str_blob(&self) -> &[u8] {
        &self.str_blob
    }
    fn record_key_ids(&self, idx: usize) -> &[rkyv::Archived<u32>] {
        let (start, end) = self.key_range(idx);
        &self.key_ids[start..end]
    }
    fn trigrams(&self) -> &[Archived<StrRef>] {
        &self.trigrams
    }
    fn trigram_offsets(&self) -> &[rkyv::Archived<u32>] {
        &self.trigram_offsets
    }
    fn trigram_postings(&self) -> &[rkyv::Archived<u32>] {
        &self.trigram_postings
    }
    fn meaning_trigrams(&self) -> &[Archived<StrRef>] {
        &self.meaning_trigrams
    }
    fn meaning_offsets(&self) -> &[rkyv::Archived<u32>] {
        &self.meaning_offsets
    }
    fn meaning_postings(&self) -> &[rkyv::Archived<u32>] {
        &self.meaning_postings
    }
    fn record_len(&self) -> usize {
        self.len()
    }
    fn entry_range(&self, idx: usize) -> Option<(usize, usize)> {
        let start = self.entry_offsets.get(idx)?.to_native() as usize;
        let end = self.entry_offsets.get(idx + 1)?.to_native() as usize;
        Some((start, end))
    }
}

impl ArchivedDictIndex {
    /// Record count (offsets agree by construction — pack asserts).
    pub fn len(&self) -> usize {
        self.entry_offsets.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn key_range(&self, idx: usize) -> (usize, usize) {
        let start = self.key_offsets[idx].to_native() as usize;
        let end = self.key_offsets[idx + 1].to_native() as usize;
        (start, end)
    }

    /// Raw postcard frame for one entry (bounds-checked; decode with
    /// [`decode_entry`]).
    pub fn entry_frame<'b>(&self, blob: &'b [u8], idx: usize) -> Option<&'b [u8]> {
        let start = self.entry_offsets.get(idx)?.to_native() as usize;
        let end = self.entry_offsets.get(idx + 1)?.to_native() as usize;
        blob.get(start..end)
    }
}

/// Serialize an artifact: header plus rkyv index plus postcard blob.
/// Entries encode in record order; offsets derive from frame lengths.
pub fn gloss_to_bytes(index: &DictIndex, entries: &[EntryRecord], magic: &[u8; 4]) -> Vec<u8> {
    assert_eq!(index.len(), entries.len(), "gloss: index/entries agree");
    assert_eq!(index.freqs.len(), entries.len(), "gloss: freqs agree");
    let payload = rkyv::to_bytes::<rkyv::rancor::Error>(index).expect("rkyv encode cannot fail");
    let mut blob = Vec::new();
    for e in entries {
        let frame = postcard::to_allocvec(e).expect("postcard encode cannot fail");
        blob.extend_from_slice(&frame);
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len() + blob.len());
    out.extend_from_slice(magic);
    out.extend_from_slice(&GLOSS_VERSION.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&(blob.len() as u64).to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&blob);
    out
}

pub fn save_gloss(
    path: &std::path::Path,
    index: &DictIndex,
    entries: &[EntryRecord],
    magic: &[u8; 4],
) -> std::io::Result<()> {
    atomic_write(path, &gloss_to_bytes(index, entries, magic))
}

/// Write bytes atomically (temp + rename on the same filesystem):
/// readers never observe a torn artifact while `pack-data` rewrites
/// multi-hundred-MB files over several minutes.
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

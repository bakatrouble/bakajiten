use super::record::{IndexRecord, StrRef};
use super::view::decode_entry;
use unic::normal::StrNormalForm;

const KATA_HIRA_DELTA: u32 = 'ア' as u32 - 'あ' as u32;
const FULL_HALF_DELTA: u32 = '！' as u32 - '!' as u32;

pub fn normalize_jap(s: &str) -> String {
    s.nfc()
        .map(|c: char| {
            match c {
                // katakana to hiragana
                'ァ'..='ヶ' => char::from_u32(c as u32 - KATA_HIRA_DELTA).unwrap(),
                // fullwidth to ascii
                '！'..='～' => char::from_u32(c as u32 - FULL_HALF_DELTA).unwrap(),
                // ideographic space to 0x20
                '　' => ' ',
                _ => c,
            }
        })
        .collect()
}

pub fn to_key(s: &str) -> [u8; 8] {
    // First 8 bytes, zero-padded: lossy prefix by design, exact
    // matches verify against the real strings. Big-endian byte order
    // means lexicographic array order is key order (partition_point
    // relies on this).
    let mut key: [u8; 8] = [0; 8];
    let bytes = s.as_bytes();
    let n = bytes.len().min(8);
    key[..n].copy_from_slice(&bytes[..n]);
    key
}

fn next_key(key: [u8; 8]) -> [u8; 8] {
    let mut bytes = key;
    for b in bytes.iter_mut().rev() {
        if *b > 0 {
            *b += 1;
            break;
        }
    }
    bytes
}

/// English gloss normalization for meaning search: casefolded
/// (JMdict mixes case — proper nouns, `(surname)` markers), NFC.
/// Same function packs grams and normalizes queries.
pub fn normalize_meaning(s: &str) -> String {
    s.to_lowercase().nfc().collect()
}

#[derive(PartialEq)]
pub enum SearchType {
    Prefix,
    Suffix,
    Middle,
}

pub trait ArchivedDictionaryFile {
    fn forward_index(&self) -> &[rkyv::Archived<IndexRecord>];
    fn reverse_index(&self) -> &[rkyv::Archived<IndexRecord>];
    /// Deduped headword pool (blob slices, see `DictIndex`).
    fn pool(&self) -> &[rkyv::Archived<StrRef>];
    /// The shared string blob every `StrRef` reads.
    fn str_blob(&self) -> &[u8];
    /// Pool indices of one record's normalized search keys.
    fn record_key_ids(&self, idx: usize) -> &[rkyv::Archived<u32>];
    /// Unique Japanese trigram keys, sorted by blob content.
    fn trigrams(&self) -> &[rkyv::Archived<StrRef>];
    /// `trigrams + 1` offsets into `trigram_postings`.
    fn trigram_offsets(&self) -> &[rkyv::Archived<u32>];
    /// Concatenated record indices, ascending per trigram.
    fn trigram_postings(&self) -> &[rkyv::Archived<u32>];
    /// Unique meaning trigram keys, sorted by blob content.
    fn meaning_trigrams(&self) -> &[rkyv::Archived<StrRef>];
    /// `meaning_trigrams + 1` offsets into `meaning_postings`.
    fn meaning_offsets(&self) -> &[rkyv::Archived<u32>];
    /// Concatenated record indices, ascending per trigram.
    fn meaning_postings(&self) -> &[rkyv::Archived<u32>];
    /// Record count (bounds the sub-trigram fallback scan).
    fn record_len(&self) -> usize;
    /// Raw postcard-frame bounds for one entry (meaning
    /// verification decodes through these).
    fn entry_range(&self, idx: usize) -> Option<(usize, usize)>;

    /// Blob bytes for one ref (pack guarantees char boundaries).
    fn blob_slice(&self, at: &rkyv::Archived<StrRef>) -> &[u8] {
        let (o, l) = (at.offset.to_native() as usize, at.len.to_native() as usize);
        &self.str_blob()[o..o + l]
    }

    /// Pool string for one key id, as text.
    fn pool_str(&self, id: u32) -> &str {
        let at = &self.pool()[id as usize];
        std::str::from_utf8(self.blob_slice(at)).expect("pool holds whole strings")
    }

    /// Blob text for one native ref (decoded frames resolve gloss
    /// text through this — the entries blob only carries frames).
    fn str_text(&self, at: &StrRef) -> &str {
        let (o, l) = (at.offset as usize, at.len as usize);
        std::str::from_utf8(&self.str_blob()[o..o + l]).expect("blob holds whole strings")
    }

    /// One record's keys contain the (already normalized) query.
    /// Pool indirection keeps its own helper: the only per-candidate
    /// work besides `contains`.
    fn record_matches(&self, idx: usize, query: &str) -> bool {
        self.record_key_ids(idx)
            .iter()
            .any(|id| self.pool_str(id.to_native()).contains(query))
    }

    /// One record's keys equal the (already normalized) query.
    /// Pool indirection keeps its own helper, mirroring
    /// [`ArchivedDictionaryFile::record_matches`].
    fn record_exact(&self, idx: usize, query: &str) -> bool {
        self.record_key_ids(idx)
            .iter()
            .any(|id| self.pool_str(id.to_native()) == query)
    }

    /// Exact-key lookup behind deinflection verification: normalized
    /// query equality, not prefix containment. Same partition scheme
    /// as [`ArchivedDictionaryFile::search`], verified against the
    /// real strings.
    fn exact(&self, query: &str, cap: usize) -> Vec<usize> {
        let query = normalize_jap(&query);
        let key = to_key(&query);
        let key_hi = next_key(key);
        let index = self.forward_index();
        let start = index.partition_point(|e| e.key < key);
        let end = index.partition_point(|e| e.key < key_hi);
        let mut hits: Vec<usize> = index[start..end]
            .iter()
            .filter_map(|e| {
                let i = e.record_idx.to_native() as usize;
                self.record_exact(i, &query).then_some(i)
            })
            .collect();
        hits.sort_unstable();
        hits.dedup();
        hits.truncate(cap);
        hits
    }

    fn search(&self, type_: SearchType, query: &str, cap: usize) -> Vec<usize> {
        let query = normalize_jap(&query);

        if type_ == SearchType::Middle {
            return self.middle(&query, cap);
        }
        let partition_query: String = match type_ {
            SearchType::Prefix => query.to_string(),
            SearchType::Suffix => query.chars().rev().collect(),
            SearchType::Middle => unreachable!("handled above"),
        };
        let index = match type_ {
            SearchType::Prefix => self.forward_index(),
            SearchType::Suffix => self.reverse_index(),
            SearchType::Middle => unreachable!("handled above"),
        };

        let key = to_key(&partition_query);
        let key_hi = next_key(key);
        let start = index.partition_point(|e| e.key < key);
        let end = index.partition_point(|e| e.key < key_hi);
        let mut hits: Vec<usize> = index[start..end]
            .iter()
            .filter_map(|e| {
                let i = e.record_idx.to_native() as usize;
                self.record_matches(i, &query).then_some(i)
            })
            .collect();

        hits.truncate(cap);
        hits.dedup();
        hits
    }

    /// Substring hits via the trigram postings: distinct query
    /// trigrams intersect (rarest postings first), survivors verify
    /// against the real strings (intersection is a superset filter —
    /// shared trigrams don't imply order). Sub-trigram queries
    /// carry no trigrams: bounded linear scan instead.
    fn middle(&self, query: &str, cap: usize) -> Vec<usize> {
        // Sub-trigram queries carry no trigrams: bounded linear
        // scan instead.
        if query.chars().count() < 3 {
            return (0..self.record_len())
                .filter(|&i| self.record_matches(i, query))
                .take(cap)
                .collect();
        }
        let hits = self.intersect_grams(
            self.trigrams(),
            self.trigram_offsets(),
            self.trigram_postings(),
            query,
        );
        let mut out: Vec<usize> = hits
            .into_iter()
            .filter(|&i| self.record_matches(i, query))
            .collect();
        out.sort_unstable();
        out.dedup();
        out.truncate(cap);
        out
    }

    /// Meaning substring hits over normalized gloss text (same
    /// intersect-then-verify shape as [`ArchivedDictionaryFile::middle`]).
    /// Gloss text lives in postcard frames, so verification decodes
    /// one frame per intersected candidate (microseconds, few
    /// candidates). Needs the entries blob alongside the index.
    fn search_meaning(&self, entry_blob: &[u8], query: &str, cap: usize) -> Vec<usize> {
        let query = normalize_meaning(query);
        // Sub-trigram queries carry no trigrams: bounded linear
        // scan (frame decode per record, capped).
        if query.chars().count() < 3 {
            return (0..self.record_len())
                .filter(|&i| self.record_matches_meaning(entry_blob, i, &query))
                .take(cap)
                .collect();
        }
        let hits = self.intersect_grams(
            self.meaning_trigrams(),
            self.meaning_offsets(),
            self.meaning_postings(),
            &query,
        );
        let mut out: Vec<usize> = hits
            .into_iter()
            .filter(|&i| self.record_matches_meaning(entry_blob, i, &query))
            .collect();
        out.sort_unstable();
        out.dedup();
        out.truncate(cap);
        out
    }

    /// One record's normalized gloss text contains the (already
    /// normalized) query. Decodes the postcard frame — heavier
    /// than the pool check, so only intersected candidates verify
    /// this way.
    fn record_matches_meaning(&self, entry_blob: &[u8], idx: usize, query: &str) -> bool {
        let Some((start, end)) = self.entry_range(idx) else {
            return false;
        };
        let Some(frame) = entry_blob.get(start..end) else {
            return false;
        };
        let Ok(entry) = decode_entry(frame) else {
            return false;
        };
        entry.senses.iter().any(|sense| {
            sense
                .glosses
                .iter()
                .any(|g| normalize_meaning(self.str_text(&g.text)).contains(query))
        })
    }

    /// Shared intersect driver: distinct 3-char grams of `query`
    /// (already normalized by the caller) look up postings, rarest
    /// first. One absent gram empties the hit; short queries carry
    /// no grams (empty intersection — callers fall back to a scan).
    fn intersect_grams(
        &self,
        grams: &[rkyv::Archived<StrRef>],
        offsets: &[rkyv::Archived<u32>],
        postings: &[rkyv::Archived<u32>],
        query: &str,
    ) -> Vec<usize> {
        let chars: Vec<char> = query.chars().collect();
        if chars.len() < 3 {
            return Vec::new();
        }
        let mut keys: Vec<Vec<u8>> = chars
            .windows(3)
            .map(|w| w.iter().collect::<String>().into_bytes())
            .collect();
        keys.sort();
        keys.dedup();
        let mut lists: Vec<&[rkyv::Archived<u32>]> = Vec::new();
        for key in &keys {
            match grams.binary_search_by(|g| self.blob_slice(g).cmp(key)) {
                Ok(pos) => {
                    let start = offsets[pos].to_native() as usize;
                    let end = offsets[pos + 1].to_native() as usize;
                    lists.push(&postings[start..end]);
                }
                Err(_) => return Vec::new(),
            }
        }
        lists.sort_by_key(|l| l.len());
        let mut hits: Vec<u32> = lists[0].iter().map(|i| i.to_native()).collect();
        for rest in &lists[1..] {
            let mut merged = Vec::new();
            let mut a = hits.iter().peekable();
            let mut b = rest.iter().peekable();
            while let (Some(&x), Some(&y)) = (a.peek(), b.peek()) {
                let yn = y.to_native();
                match x.cmp(&yn) {
                    std::cmp::Ordering::Equal => {
                        merged.push(*x);
                        a.next();
                        b.next();
                    }
                    std::cmp::Ordering::Less => {
                        a.next();
                    }
                    std::cmp::Ordering::Greater => {
                        b.next();
                    }
                }
            }
            hits = merged;
            if hits.is_empty() {
                return Vec::new();
            }
        }
        hits.into_iter().map(|i| i as usize).collect()
    }
}

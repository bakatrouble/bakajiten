//! Shared string blob: every stored string interns here once,
//! referenced everywhere by one shape (`StrRef` offset/length —
//! pool entries, gram keys, entry texts alike). Interning is
//! substring-aware: a string already present anywhere in the blob
//! reuses those bytes (exact duplicates are just the trivial
//! case). Short strings skip the substring search — exact-map
//! dedup still applies to all strings, the bounded search only
//! runs where sharing can pay for its scan.
use dict::gloss::record::StrRef;

use std::collections::HashMap;

/// Substring search runs only at this byte length and above: below
/// it a shared ref barely beats inline text, while the scan costs
/// the same. Exact-map dedup has no threshold. Single chars skip
/// the search entirely (no gram to search with — exact-only).
pub const SUBSTRING_MIN_LEN: usize = 6;
/// Candidate blob positions checked per intern: rarest-gram first,
/// so sharing usually proves out in one memcmp; the cap bounds
/// pathological repetition scans.
pub const SUBSTRING_POS_CAP: usize = 32;

/// Pack-time string store (also used by toy builders in tests).
/// Pool ids assign in first-interned order; grams and entry texts
/// take `StrRef`s straight from the shared map without consuming
/// ids.
pub struct StrBlob {
    blob: Vec<u8>,
    /// String bytes to blob slice (the single dedup map for every
    /// stored string).
    exact: HashMap<Vec<u8>, (u32, u32)>,
    /// Pool table in id order (headword forms for `key_ids`).
    table: Vec<StrRef>,
    /// Blob slice to pool id.
    ids: HashMap<(u32, u32), u32>,
    /// Trigram key ([u8;12]-padded, like the query path) to blob
    /// start positions (capped per gram). Bigrams ride a parallel
    /// map: 2-char strings have no trigram to search with.
    occ: HashMap<[u8; 12], Vec<u32>>,
    occ2: HashMap<[u8; 8], Vec<u32>>,
}

impl StrBlob {
    pub fn new() -> Self {
        Self {
            blob: Vec::new(),
            exact: HashMap::new(),
            table: Vec::new(),
            ids: HashMap::new(),
            occ: HashMap::new(),
            occ2: HashMap::new(),
        }
    }

    /// Intern a headword form: pool id, substring-shared when the
    /// bytes already sit in the blob.
    pub fn intern_key(&mut self, s: &str) -> u32 {
        let (offset, len) = self.intern(s);
        if let Some(&id) = self.ids.get(&(offset, len)) {
            return id;
        }
        let id = self.table.len() as u32;
        self.table.push(StrRef { offset, len });
        self.ids.insert((offset, len), id);
        id
    }

    /// Intern an entry text or gram: blob slice, never a pool id.
    pub fn intern_str(&mut self, s: &str) -> StrRef {
        let (offset, len) = self.intern(s);
        StrRef { offset, len }
    }

    /// Pool table in id order plus the blob bytes.
    pub fn finish(self) -> (Vec<StrRef>, Vec<u8>) {
        (self.table, self.blob)
    }

    /// Blob slice for one pool id (pack-time resolution).
    pub fn resolve(&self, id: u32) -> &str {
        let entry = self.table[id as usize];
        self.slice(entry)
    }

    /// Blob slice for one ref (pack-time resolution).
    pub fn slice(&self, at: StrRef) -> &str {
        let (o, l) = (at.offset as usize, at.len as usize);
        std::str::from_utf8(&self.blob[o..o + l]).expect("blob holds whole strings")
    }

    fn intern(&mut self, s: &str) -> (u32, u32) {
        let bytes = s.as_bytes();
        if let Some(&found) = self.exact.get(bytes) {
            return found;
        }
        if bytes.len() >= SUBSTRING_MIN_LEN {
            if let Some(found) = self.find(bytes) {
                self.exact.insert(bytes.to_vec(), (found.offset, found.len));
                return (found.offset, found.len);
            }
        }
        let offset = self.blob.len() as u32;
        self.blob.extend_from_slice(bytes);
        let shared = (offset, bytes.len() as u32);
        self.exact.insert(bytes.to_vec(), shared);
        self.index_grams(offset);
        shared
    }

    /// First blob occurrence of `bytes`, or `None`. Rarest-gram
    /// positions first (a missing gram proves absence outright);
    /// memcmp confirms (hash collisions can't false-hit). Two-char
    /// strings search the bigram map; shorter ones never reach here.
    fn find(&self, bytes: &[u8]) -> Option<StrRef> {
        let text = std::str::from_utf8(bytes).ok()?;
        let chars: Vec<char> = text.chars().collect();
        if chars.len() >= 3 {
            return self.find_grams(bytes, &chars);
        }
        if chars.len() == 2 {
            let key = pad2(&chars);
            let positions = self.occ2.get(&key)?;
            for &at in positions.iter() {
                let (o, l) = (at as usize, bytes.len());
                if self.blob.get(o..o + l) == Some(bytes) {
                    return Some(StrRef {
                        offset: at,
                        len: l as u32,
                    });
                }
            }
        }
        None
    }

    fn find_grams(&self, bytes: &[u8], chars: &[char]) -> Option<StrRef> {
        let mut grams: Vec<([u8; 12], &[u32])> = Vec::new();
        for w in chars.windows(3) {
            let key = pad3(w);
            match self.occ.get(&key) {
                Some(pos) => grams.push((key, pos)),
                None => return None,
            }
        }
        grams.sort_by_key(|(_, pos)| pos.len());
        let (_, positions) = grams.first()?;
        for &at in positions.iter() {
            let (o, l) = (at as usize, bytes.len());
            if self.blob.get(o..o + l) == Some(bytes) {
                return Some(StrRef {
                    offset: at,
                    len: l as u32,
                });
            }
        }
        None
    }

    /// Index gram starts in the appended region plus the overlapping
    /// tail (substrings may start in older bytes and end in new
    /// ones). The blob is always valid UTF-8 (whole strings only),
    /// so the boundary walk below is sound.
    fn index_grams(&mut self, offset: u32) {
        let off = offset as usize;
        let mut start = off.saturating_sub(11);
        while start < off && !is_boundary(&self.blob, start) {
            start += 1;
        }
        let mut at = start;
        while at < self.blob.len() {
            let gram = gram_at(&self.blob, at, 3);
            push_capped(&mut self.occ, gram, at);
            let wide = gram_at(&self.blob, at, 2);
            let mut bigram: [u8; 8] = [0; 8];
            bigram.copy_from_slice(&wide[..8]);
            push_capped8(&mut self.occ2, bigram, at);
            at += char_len(&self.blob, at);
        }
    }
}

impl Default for StrBlob {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounded position push (see `SUBSTRING_POS_CAP`).
fn push_capped(map: &mut HashMap<[u8; 12], Vec<u32>>, key: [u8; 12], pos: usize) {
    let entry = map.entry(key).or_default();
    if entry.len() < SUBSTRING_POS_CAP && !entry.contains(&(pos as u32)) {
        entry.push(pos as u32);
    }
}

fn push_capped8(map: &mut HashMap<[u8; 8], Vec<u32>>, key: [u8; 8], pos: usize) {
    let entry = map.entry(key).or_default();
    if entry.len() < SUBSTRING_POS_CAP && !entry.contains(&(pos as u32)) {
        entry.push(pos as u32);
    }
}

/// Padded trigram key (same shape as the query path).
fn pad3(chars: &[char]) -> [u8; 12] {
    pad_n(chars, 3)
}

/// Padded bigram key for 2-char substring search.
fn pad2(chars: &[char]) -> [u8; 8] {
    let wide = pad_n(chars, 2);
    let mut key: [u8; 8] = [0; 8];
    key.copy_from_slice(&wide[..8]);
    key
}

fn pad_n(chars: &[char], n: usize) -> [u8; 12] {
    let mut key: [u8; 12] = [0; 12];
    let mut at = 0;
    for c in chars.iter().take(n) {
        let m = c.len_utf8();
        c.encode_utf8(&mut key[at..at + m]);
        at += m;
    }
    key
}

fn is_boundary(blob: &[u8], at: usize) -> bool {
    at == 0 || at >= blob.len() || (blob[at] & 0xC0) != 0x80
}

fn char_len(blob: &[u8], at: usize) -> usize {
    let b = blob[at];
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Up to `n` chars from a char boundary, padded (short tail when
/// the blob ends mid-window — only the tail region indexes those;
/// memcmp in `find` keeps them from false-hitting).
fn gram_at(blob: &[u8], at: usize, n: usize) -> [u8; 12] {
    let mut key: [u8; 12] = [0; 12];
    let mut pos = at;
    let mut written = 0;
    for _ in 0..n {
        if pos >= blob.len() {
            break;
        }
        let m = char_len(blob, pos);
        let end = (pos + m).min(blob.len());
        key[written..written + (end - pos)].copy_from_slice(&blob[pos..end]);
        written += end - pos;
        pos = end;
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_duplicates_share_bytes() {
        let mut blob = StrBlob::new();
        let a = blob.intern_key("たなか");
        let b = blob.intern_key("たなか");
        assert_eq!(a, b);
        let (table, bytes) = blob.finish();
        assert_eq!(table.len(), 1);
        assert_eq!(bytes, "たなか".as_bytes());
    }

    #[test]
    fn substrings_reuse_pool_bytes() {
        let mut blob = StrBlob::new();
        blob.intern_key("日本国");
        let id = blob.intern_key("日本");
        // 日本 sits inside 日本国's bytes: no new bytes appended.
        let (table, bytes) = blob.finish();
        assert_eq!(bytes, "日本国".as_bytes());
        assert_eq!(
            table[id as usize],
            StrRef {
                offset: 0,
                len: "日本".len() as u32
            }
        );
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn grams_reuse_key_bytes() {
        let mut blob = StrBlob::new();
        blob.intern_key("にほんご");
        let before = blob.finish().1.len();
        let mut blob = StrBlob::new();
        blob.intern_key("にほんご");
        blob.intern_str("ほんご");
        // The gram sits inside the key bytes: no growth.
        assert_eq!(blob.finish().1.len(), before);
    }

    #[test]
    fn missing_substrings_append() {
        let mut blob = StrBlob::new();
        blob.intern_key("あいう");
        blob.intern_key("えおか");
        let (table, bytes) = blob.finish();
        assert_eq!(table.len(), 2);
        assert_eq!(bytes, "あいうえおか".as_bytes());
    }
}

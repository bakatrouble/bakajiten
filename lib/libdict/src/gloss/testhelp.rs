//! Test-only blob builder: exact-map interning without substring
//! sharing. Read-path tests need well-formed tables (sorted grams,
//! n+1 offsets, refs landing on real bytes), not maximal sharing —
//! the real builder's sharing is covered by its own tests in utils.
//! API mirrors `utils::blob::StrBlob` (`intern_key`, `intern_str`,
//! `slice`, `finish`) so toy code reads the same.

use super::record::StrRef;

use std::collections::HashMap;

pub(crate) struct ToyBlob {
    blob: Vec<u8>,
    exact: HashMap<Vec<u8>, (u32, u32)>,
    table: Vec<StrRef>,
}

impl ToyBlob {
    pub(crate) fn new() -> Self {
        Self {
            blob: Vec::new(),
            exact: HashMap::new(),
            table: Vec::new(),
        }
    }

    pub(crate) fn intern_key(&mut self, s: &str) -> u32 {
        let (offset, len) = self.intern(s);
        if let Some(id) = self
            .table
            .iter()
            .position(|e| e.offset == offset && e.len == len)
        {
            return id as u32;
        }
        let id = self.table.len() as u32;
        self.table.push(StrRef { offset, len });
        id
    }

    pub(crate) fn intern_str(&mut self, s: &str) -> StrRef {
        let (offset, len) = self.intern(s);
        StrRef { offset, len }
    }

    pub(crate) fn gram_postings(
        &mut self,
        key_sets: &[Vec<String>],
        normalize: fn(&str) -> String,
    ) -> (Vec<StrRef>, Vec<u32>, Vec<u32>) {
        let mut pairs: Vec<(String, u32)> = Vec::new();
        for (i, keys) in key_sets.iter().enumerate() {
            let mut grams: Vec<String> = Vec::new();
            for key in keys {
                let normalized = normalize(key);
                let chars: Vec<char> = normalized.chars().collect();
                for w in chars.windows(3) {
                    grams.push(w.iter().collect());
                }
            }
            grams.sort();
            grams.dedup();
            for gram in grams {
                pairs.push((gram, i as u32));
            }
        }
        pairs.sort();
        pairs.dedup();
        let mut trigrams: Vec<StrRef> = Vec::new();
        let mut offsets: Vec<u32> = Vec::new();
        let mut postings: Vec<u32> = Vec::new();
        let mut current: Option<&str> = None;
        for (gram, idx) in &pairs {
            if current != Some(gram.as_str()) {
                current = Some(gram.as_str());
                trigrams.push(self.intern_str(gram));
                offsets.push(postings.len() as u32);
            }
            postings.push(*idx);
        }
        offsets.push(postings.len() as u32);
        (trigrams, offsets, postings)
    }

    pub(crate) fn slice(&self, at: StrRef) -> &str {
        let (o, l) = (at.offset as usize, at.len as usize);
        std::str::from_utf8(&self.blob[o..o + l]).expect("toy blob holds whole strings")
    }

    pub(crate) fn finish(self) -> (Vec<StrRef>, Vec<u8>) {
        (self.table, self.blob)
    }

    fn intern(&mut self, s: &str) -> (u32, u32) {
        let bytes = s.as_bytes();
        if let Some(&found) = self.exact.get(bytes) {
            return found;
        }
        let shared = (self.blob.len() as u32, bytes.len() as u32);
        self.blob.extend_from_slice(bytes);
        self.exact.insert(bytes.to_vec(), shared);
        shared
    }
}

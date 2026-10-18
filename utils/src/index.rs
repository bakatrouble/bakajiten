//! Prefix/suffix indexes over per-record key lists. Keys arrive as
//! plain strings (pack resolves pool indices separately); the index
//! itself stores only prefix bytes plus record positions.
use super::blob::StrBlob;
use dict::gloss::record::{IndexRecord, StrRef};
use dict::gloss::search::{normalize_jap, to_key};

pub fn build_index(key_sets: &[Vec<String>]) -> Vec<IndexRecord> {
    let mut index = Vec::new();
    for (i, keys) in key_sets.iter().enumerate() {
        for key in keys {
            index.push(IndexRecord {
                key: to_key(&normalize_jap(key)),
                record_idx: i as u32,
            });
        }
    }
    index.sort_by(|a, b| a.key.cmp(&b.key));
    index
}

pub fn build_reverse_index(key_sets: &[Vec<String>]) -> Vec<IndexRecord> {
    let mut reverse_index = Vec::new();
    for (i, keys) in key_sets.iter().enumerate() {
        for key in keys {
            reverse_index.push(IndexRecord {
                key: to_key(&normalize_jap(&key.chars().rev().collect::<String>())),
                record_idx: i as u32,
            });
        }
    }
    reverse_index.sort_by(|a, b| a.key.cmp(&b.key));
    reverse_index
}

/// Inverted gram postings over per-record string lists: unique
/// 3-char grams (sorted by content), `records + 1` offsets (last =
/// total postings), record indices ascending per gram. Strings
/// normalize first (caller picks the normalizer — Japanese keys or
/// English glosses — same function the query path applies); grams
/// dedupe per record, then intern into the shared blob (usually
/// reusing key/gloss bytes outright, appending only on a miss).
pub fn build_gram_postings(
    key_sets: &[Vec<String>],
    blob: &mut StrBlob,
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
            trigrams.push(blob.intern_str(gram));
            offsets.push(postings.len() as u32);
        }
        postings.push(*idx);
    }
    offsets.push(postings.len() as u32);
    (trigrams, offsets, postings)
}

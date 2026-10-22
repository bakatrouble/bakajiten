//! Word/name dictionaries (`GLD1`/`GLN1`): JMdict and JMnedict XML
//! converted field-for-field into record structs, sorted by
//! `ent_seq` for deterministic output. Each file is verified with
//! a decode roundtrip before writing.

use super::convert::{convert_kokugo, convert_name, convert_word};
use super::jlpt::JlptIndex;
use super::progress;
use dict::gloss::file::{GLOSS_JMDICT_MAGIC, GLOSS_JMNEDICT_MAGIC, GLOSS_KOKUGO_MAGIC};
use dict::gloss::search::{normalize_jap, normalize_meaning};
use dict::gloss::{gloss_from_bytes, save_gloss, DictIndex, EntryRecord};
use utils::blob::StrBlob;
use utils::index::{build_gram_postings, build_index, build_reverse_index};
use utils::parsers::jmdict::parse_reader;

/// Pack JMdict XML into a `DictFile` artifact.
pub(super) fn pack_jmdict(
    xml: &std::path::Path,
    out: &std::path::Path,
    jlpt: &JlptIndex,
) -> Result<super::Summary, Box<dyn std::error::Error>> {
    let mut blob = StrBlob::new();
    let mut records = Vec::new();
    let mut raw_keys = Vec::new();
    let version = parse_counted(xml, "parse jmdict", &mut records, |e| {
        let (rec, keys) = convert_word(e, &mut blob, jlpt)?;
        raw_keys.push(keys);
        Ok(rec)
    })?;
    finish(
        "jmdict",
        xml,
        version,
        records,
        raw_keys,
        blob,
        out,
        GLOSS_JMDICT_MAGIC,
    )
}

/// Pack JMnedict XML into a `DictFile` artifact (same schema, sibling
/// magic — files stay separate, loaders stay split).
pub(super) fn pack_jmnedict(
    xml: &std::path::Path,
    out: &std::path::Path,
    jlpt: &JlptIndex,
) -> Result<super::Summary, Box<dyn std::error::Error>> {
    let mut blob = StrBlob::new();
    let mut records = Vec::new();
    let mut raw_keys = Vec::new();
    let version = parse_counted(xml, "parse jmnedict", &mut records, |e| {
        let (rec, keys) = convert_name(e, &mut blob, jlpt)?;
        raw_keys.push(keys);
        Ok(rec)
    })?;
    finish(
        "jmnedict",
        xml,
        version,
        records,
        raw_keys,
        blob,
        out,
        GLOSS_JMNEDICT_MAGIC,
    )
}

/// Pack kokugo JSONL (fetch-data kokugo output) into a `DictFile`
/// artifact: same schema as the EDRDG files under a sibling magic
/// (`GLK1`), so the existing search machinery reads it unchanged.
/// Unknown JSON keys (`synonyms`, `hypernyms`, `source`) are
/// ignored here; they ride along for the future related-words
/// strip. Like the XML packs, converters fail loudly on unknown
/// entities and the file is verified with a decode roundtrip
/// before writing.
/// Source-date stamp written by fetch-data next to the kokugo
/// inputs (`--kokugo-versions`): extraction/update ISO dates of
/// the two origins. Unknown sides are null.
#[derive(serde::Deserialize)]
struct KokugoSources {
    kaikki_extracted: Option<String>,
    wnja_updated: Option<String>,
}

/// Latest known source date from a fetch-data stamp file (kaikki
/// extract vs WordNet update). `None` when the file is absent or
/// both sides are unknown — the caller falls back to the content
/// hash.
fn kokugo_source_date(path: Option<&std::path::Path>) -> Option<String> {
    let path = path?;
    let raw = std::fs::read_to_string(path).ok()?;
    let stamp: KokugoSources = serde_json::from_str(&raw).ok()?;
    dict::kanji::version::latest_date(stamp.kaikki_extracted.as_deref(), stamp.wnja_updated.as_deref())
}

pub(super) fn pack_kokugo(
    kaikki: &std::path::Path,
    wordnet: &std::path::Path,
    out: &std::path::Path,
    versions: Option<&std::path::Path>,
    jlpt: &JlptIndex,
    backfill: Option<&std::path::Path>,
) -> Result<super::Summary, Box<dyn std::error::Error>> {
    // Import kaikki + WordNet in-process (deterministic, same audit
    // counters as the retired standalone importer, minus the JSONL
    // file that used to sit between import and pack).
    let lines = super::kokugo::run(kaikki, wordnet)?;
    // Source version: latest of the two origin dates (kaikki
    // extract vs WordNet update). The content-hash fallback keeps
    // unstamped pipelines stable: FNV-1a over the imported lines
    // (stable exactly when the input is; FNV, not SipHash — the std
    // hasher is explicitly unstable across toolchains). The mtime
    // fallback would churn every run (fresh fetch) and defeat
    // identical-output skips.
    let version = match kokugo_source_date(versions) {
        Some(date) => date,
        None => {
            let mut h: u64 = 0xcbf29ce484222325;
            for line in &lines {
                for &b in line.as_bytes() {
                    h ^= b as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
            }
            format!("kokugo-{h:016x}")
        }
    };
    // Reading backfill: reb-less entries take the JMdict reading when
    // exactly one exists across all keb.
    let jmap = match backfill {
        Some(path) => keb_readings(path)?,
        None => std::collections::HashMap::new(),
    };
    let (mut borrowed, mut absent, mut ambiguous) = (0u64, 0u64, 0u64);
    let mut blob = StrBlob::new();
    let mut records = Vec::new();
    let mut raw_keys = Vec::new();
    let mut count = 0u64;
    let bar = progress::count_bar("parse kokugo", lines.len() as u64);
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        let mut e: utils::parsers::jmdict::Entry = serde_json::from_str(line)?;
        if !jmap.is_empty() && e.readings.is_empty() && !e.kanji.is_empty() {
            let mut readings = std::collections::BTreeSet::new();
            for k in &e.kanji {
                if let Some(rs) = jmap.get(&k.keb) {
                    readings.extend(rs.iter().cloned());
                }
            }
            if readings.len() == 1 {
                e.readings.push(utils::parsers::jmdict::ReadingElement {
                    reb: readings.into_iter().next().expect("exactly one"),
                    nokanji: false,
                    restricted_to: Vec::new(),
                    info: Vec::new(),
                    priorities: Vec::new(),
                });
                borrowed += 1;
            } else if readings.is_empty() {
                absent += 1;
            } else {
                ambiguous += 1;
            }
        }
        let (rec, keys) = convert_kokugo(&e, &mut blob, jlpt)?;
        raw_keys.push(keys);
        records.push(rec);
        count += 1;
        if count % 4096 == 0 {
            bar.set_message(format!("parse kokugo ({count} entries)"));
        }
    }
    bar.finish_with_message(format!("parse kokugo ({count} entries)"));
    if !jmap.is_empty() {
        println!("readings borrowed: {borrowed} ambiguous: {ambiguous} absent: {absent}");
    }
    finish(
        "kokugo",
        kaikki,
        Some(version),
        records,
        raw_keys,
        blob,
        out,
        GLOSS_KOKUGO_MAGIC,
    )
}

fn keb_readings(
    path: &std::path::Path,
) -> Result<std::collections::HashMap<String, std::collections::HashSet<String>>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut map = std::collections::HashMap::new();
    parse_reader(std::io::BufReader::new(file), |e| {
        for k in &e.kanji {
            for r in &e.readings {
                map.entry(k.keb.clone()).or_insert_with(std::collections::HashSet::new).insert(r.reb.clone());
            }
        }
    })?;
    Ok(map)
}

/// Shared tail: sort, index, encode, decode roundtrip, save, report.
fn finish(
    label: &str,
    xml: &std::path::Path,
    version: Option<String>,
    records: Vec<EntryRecord>,
    raw_keys: Vec<Vec<String>>,
    mut blob: StrBlob,
    out: &std::path::Path,
    magic: &[u8; 4],
) -> Result<super::Summary, Box<dyn std::error::Error>> {
    let mut pairs: Vec<(EntryRecord, Vec<String>)> = records.into_iter().zip(raw_keys).collect();
    pairs.sort_by_key(|(r, _)| r.ent_seq);
    let (records, raw_keys): (Vec<EntryRecord>, Vec<Vec<String>>) = pairs.into_iter().unzip();
    let spin = progress::spinner(&format!("index {label}"));
    let forward_index = build_index(&raw_keys);
    let reverse_index = build_reverse_index(&raw_keys);
    let (trigrams, trigram_offsets, trigram_postings) =
        build_gram_postings(&raw_keys, &mut blob, normalize_jap);
    // Normalized gloss copies for meaning grams (dedup map shares
    // bytes with already-lowercase texts; mixed-case adds its own).
    // Meaning grams trail everything, so they reuse pool and gloss
    // bytes outright instead of appending.
    let mut meaning_keys: Vec<Vec<String>> = Vec::with_capacity(records.len());
    for r in &records {
        let mut texts = Vec::new();
        for sense in &r.senses {
            for g in &sense.glosses {
                let text = blob.slice(g.text);
                texts.push(text.to_string());
                blob.intern_str(&normalize_meaning(text));
            }
        }
        meaning_keys.push(texts);
    }
    let (meaning_trigrams, meaning_offsets, meaning_postings) =
        build_gram_postings(&meaning_keys, &mut blob, normalize_meaning);
    spin.finish_with_message(format!(
        "{} fwd + {} rev keys + {} trigrams + {} meaning trigrams",
        forward_index.len(),
        reverse_index.len(),
        trigram_postings.len(),
        meaning_postings.len()
    ));
    // Flat key ids + offsets (pack asserts the n+1 shape the
    // readers rely on).
    let mut key_ids = Vec::new();
    let mut key_offsets = Vec::with_capacity(records.len() + 1);
    for keys in &raw_keys {
        key_offsets.push(key_ids.len() as u32);
        for k in keys {
            key_ids.push(blob.intern_key(k));
        }
    }
    key_offsets.push(key_ids.len() as u32);
    // Entry offsets from postcard frame lengths.
    let mut entry_offsets = Vec::with_capacity(records.len() + 1);
    let mut blob_len = 0u32;
    for r in &records {
        entry_offsets.push(blob_len);
        let frame = postcard::to_allocvec(r).expect("postcard encode cannot fail");
        blob_len += frame.len() as u32;
    }
    entry_offsets.push(blob_len);
    // Word frequencies for the relevance sort (zkanji points over
    // ke_pri/re_pri per record).
    let freqs = records
        .iter()
        .map(|r| super::convert::record_freq(&r.kanji, &r.readings))
        .collect();
    let (pool, str_blob) = blob.finish();
    let index = DictIndex {
        version: source_version(xml, version),
        pool,
        str_blob,
        forward_index,
        reverse_index,
        trigrams,
        trigram_offsets,
        trigram_postings,
        meaning_trigrams,
        meaning_offsets,
        meaning_postings,
        key_ids,
        key_offsets,
        freqs,
        entry_offsets,
    };
    verify_save(label, out, &index, &records, magic, records.len())
}

/// Stream-parse with a byte bar (total = file size) plus an entry
/// count in the message, converting each entry on the fly.
/// Converters are fallible (unknown `pos` entities fail the pack);
/// the first conversion error aborts the file.
fn parse_counted<T, F>(
    xml: &std::path::Path,
    label: &str,
    records: &mut Vec<T>,
    mut convert: F,
) -> Result<Option<String>, Box<dyn std::error::Error>>
where
    F: FnMut(&utils::parsers::jmdict::Entry) -> Result<T, Box<dyn std::error::Error>>,
{
    let file = std::fs::File::open(xml)?;
    let total = file.metadata()?.len();
    let bar = progress::bytes_bar(label, total);
    let mut count = 0u64;
    let mut convert_err: Option<Box<dyn std::error::Error>> = None;
    let version = parse_reader(std::io::BufReader::new(bar.wrap_read(file)), |e| {
        if convert_err.is_some() {
            return;
        }
        match convert(&e) {
            Ok(record) => {
                records.push(record);
                count += 1;
                if count % 1024 == 0 {
                    bar.set_message(format!("{label} ({count} entries)"));
                }
            }
            Err(e) => convert_err = Some(e),
        }
    })?;
    if let Some(e) = convert_err {
        return Err(e);
    }
    bar.finish_with_message(format!("{label} ({count} entries)"));
    Ok(version)
}

/// Shared tail: encode, decode roundtrip, save. Reports the pack
/// summary; main prints it (human line or `--json`).
fn verify_save(
    label: &str,
    out: &std::path::Path,
    index: &DictIndex,
    records: &[EntryRecord],
    magic: &[u8; 4],
    count: usize,
) -> Result<super::Summary, Box<dyn std::error::Error>> {
    let spin = progress::spinner(&format!("encode {label}"));
    let bytes = dict::gloss::gloss_to_bytes(index, records, magic);
    let (back_index, back_records) =
        gloss_from_bytes(&bytes, magic).map_err(|e| format!("{label}: {e}"))?;
    assert_eq!(&back_index, index, "{label} roundtrip lost index rows");
    assert_eq!(&back_records, records, "{label} roundtrip lost records");
    assert_eq!(
        dict::gloss::gloss_to_bytes(&back_index, &back_records, magic),
        bytes,
        "{label} roundtrip is not byte-exact"
    );
    save_gloss(out, index, records, magic)?;
    spin.finish_with_message(format!("encode {label}"));
    Ok(super::Summary {
        artifact: label.to_owned(),
        path: out.to_path_buf(),
        records: count,
        bytes: bytes.len(),
        version: index.version.clone(),
    })
}

/// Version stamp: the header version from the parse pass, else
/// the file mtime, else "unknown".
fn source_version(xml: &std::path::Path, parsed: Option<String>) -> String {
    if let Some(date) = parsed {
        return date;
    }
    std::fs::metadata(xml)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::kokugo_source_date;

    fn stamp(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = dir.join(format!("kokugo-sources-{}.json", body.len()));
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn kokugo_stamp_takes_latest_date() {
        let dir = std::env::temp_dir();
        let stamped = |kaikki: &str, wnja: &str| {
            let p = stamp(&dir, &format!(r#"{{"kaikki_extracted":{kaikki},"wnja_updated":{wnja}}}"#));
            kokugo_source_date(Some(&p))
        };
        assert_eq!(
            stamped(r#""2026-10-20""#, r#""2026-09-01""#),
            Some("2026-10-20".to_string())
        );
        assert_eq!(
            stamped(r#""2026-09-01""#, r#""2026-10-20""#),
            Some("2026-10-20".to_string())
        );
        assert_eq!(stamped("null", r#""2026-09-01""#), Some("2026-09-01".to_string()));
        // No stamp, garbage stamp, or no known dates: hash fallback.
        assert_eq!(kokugo_source_date(None), None);
        assert_eq!(
            kokugo_source_date(Some(&stamp(&dir, "not json"))),
            None
        );
        assert_eq!(stamped("null", "null"), None);
        assert_eq!(stamped(r#""garbage""#, "null"), None);
    }
}

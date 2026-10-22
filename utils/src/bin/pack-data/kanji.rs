//! Kanji records (`KJD1`): exactly one drawing per character —
//! the canonical file when the checkout has one (bare id decoding
//! to the character itself), else the first variant seen — joined
//! with kanjidic2 metadata. Kana etc. keep bare records.

use super::progress;
use dict::kanji::{kanji_from_bytes, kanji_to_bytes, save_kanji, KanjiFile, KanjiRecord};
use std::collections::HashMap;
use utils::parsers::kanjivg::KanjiVgEntry;

/// Join strokes with kanjidic2, verify the roundtrip, save.
pub(super) fn pack(
    parsed: &[KanjiVgEntry],
    kanjidic: &std::path::Path,
    kradfile: Option<&std::path::Path>,
    kanjivg_dir: &std::path::Path,
    out: &std::path::Path,
) -> Result<super::Summary, Box<dyn std::error::Error>> {
    use utils::parsers::kanjidic::{creation_date, parse_reader as parse_kanjidic};

    let file = std::fs::File::open(kanjidic)?;
    let bar = progress::bytes_bar("parse kanjidic", file.metadata()?.len());
    let mut dict = HashMap::new();
    let mut dict_count = 0u64;
    parse_kanjidic(std::io::BufReader::new(bar.wrap_read(file)), |c| {
        if let Some(ch) = c.literal.chars().next() {
            dict.entry(ch).or_insert(c);
            dict_count += 1;
            if dict_count % 1024 == 0 {
                bar.set_message(format!("parse kanjidic ({dict_count} chars)"));
            }
        }
    })?;
    bar.finish_with_message(format!("parse kanjidic ({dict_count} chars)"));
    // kradfile parts, keyed by kanji (optional input: absent
    // entries pack with empty parts, like kana without entries).
    let mut parts: HashMap<char, Vec<char>> = HashMap::new();
    if let Some(path) = kradfile {
        let bytes = std::fs::read(path)?;
        let bar = progress::count_bar("parse kradfile", bytes.len() as u64);
        let mut part_count = 0u64;
        utils::parsers::kradfile::parse_bytes(
            &bytes,
            &mut |d: utils::parsers::kradfile::Decomposition| {
                part_count += 1;
                if part_count % 1024 == 0 {
                    bar.set_position(part_count);
                }
                parts.entry(d.kanji).or_insert(d.parts);
            },
        )?;
        bar.finish_with_message(format!("parse kradfile ({part_count} chars)"));
    }
    let bar = progress::count_bar("join kanji", parsed.len() as u64);
    let mut without_dict = 0usize;
    let mut records: Vec<KanjiRecord> = pick_canonical(parsed)
        .values()
        .map(|e| {
            bar.inc(1);
            let ch = e.character.expect("canonical pick is labeled");
            match dict.get(&ch) {
                Some(d) => utils::parsers::kanjidic::to_record(
                    d,
                    e.strokes.clone(),
                    parts.get(&ch).map(Vec::as_slice).unwrap_or(&[]),
                ),
                None => {
                    without_dict += 1;
                    KanjiRecord::bare(ch, e.strokes.clone())
                }
            }
        })
        .collect();
    bar.finish_with_message(format!("join kanji ({} records)", records.len()));
    records.sort_by_key(|r| r.character);
    println!(
        "kanji records: {} ({} without kanjidic2 entry)",
        records.len(),
        without_dict
    );
    let kanjivg = super::kanjivg_version(kanjivg_dir);
    let kanjidic2 = creation_date(kanjidic).unwrap_or_else(|_| "unknown".into());
    // Single date version: the latest of the two sources (both are
    // ISO dates or "unknown" by construction).
    let version = dict::kanji::version::kanji_version(&kanjivg, &kanjidic2);
    let file = KanjiFile {
        kanjivg_version: kanjivg.clone(),
        kanjidic2_version: kanjidic2.clone(),
        records,
    };
    let spin = progress::spinner("encode kanji");
    let bytes = kanji_to_bytes(&file);
    let back = kanji_from_bytes(&bytes)?;
    assert_eq!(
        back.records.len(),
        file.records.len(),
        "kanji roundtrip lost records"
    );
    save_kanji(out, &file)?;
    spin.finish_with_message("encode kanji");
    Ok(super::Summary {
        artifact: "kanji".into(),
        path: out.to_path_buf(),
        records: file.records.len(),
        bytes: bytes.len(),
        version,
    })
}

/// One entry per character, preferring the canonical drawing.
fn pick_canonical<'a>(parsed: &'a [KanjiVgEntry]) -> HashMap<char, &'a KanjiVgEntry> {
    let mut best: HashMap<char, &KanjiVgEntry> = HashMap::new();
    for e in parsed
        .iter()
        .filter(|e| e.character.is_some_and(|_| !e.strokes.is_empty()))
    {
        let ch = e.character.expect("filtered");
        match best.get(&ch) {
            Some(cur) if !cur.is_canonical() && e.is_canonical() => {
                best.insert(ch, e);
            }
            None => {
                best.insert(ch, e);
            }
            _ => {}
        }
    }
    best
}

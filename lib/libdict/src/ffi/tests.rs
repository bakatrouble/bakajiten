//! Exported-API tests over toy artifacts (mirrors `ffi` precedent:
//! exercise the boundary, not the internals).
use super::objects::*;
use super::records::*;
use crate::gloss::field::Field;
use crate::gloss::gloss_type::GlossType;
use crate::gloss::inf::Inf;
use crate::gloss::misc::Misc;
use crate::gloss::name_type::NameType;
use crate::gloss::pos::Pos;
use crate::gloss::priority::Priority;
use crate::gloss::record::{
    EntryRecord, GlossRecord, IndexRecord, KanjiElementRecord, LangSourceRecord,
    ReadingElementRecord, SenseDetail, SenseExtras, SenseRecord, record_freq,
};
use crate::gloss::search::to_key;
use crate::gloss::search::{normalize_jap, normalize_meaning};
use crate::gloss::testhelp::ToyBlob;
use crate::gloss::{DictIndex, GLOSS_JMDICT_MAGIC, GLOSS_JMNEDICT_MAGIC};
use crate::kanji::codes::{Deroo, FourCorner, Kuten, Nelson, Njecd, Oneill, ShDesc, Skip};
use crate::kanji::record::{Point, Stroke};
use crate::kanji::{KanjiFile, KanjiRecord, kanji_to_bytes};
use std::sync::Arc;

/// Test pool interning (mirrors pack-time first-occurrence order).
fn pid(blob: &mut ToyBlob, s: &str) -> u32 {
    blob.intern_key(s)
}

fn ke(blob: &mut ToyBlob, keb: &str) -> KanjiElementRecord {
    KanjiElementRecord {
        keb: pid(blob, keb),
        info: vec![],
        priorities: vec![],
    }
}

fn re(blob: &mut ToyBlob, reb: &str) -> ReadingElementRecord {
    ReadingElementRecord {
        reb: pid(blob, reb),
        nokanji: false,
        restricted_to: vec![],
        info: vec![],
        priorities: vec![],
    }
}

fn sense(blob: &mut ToyBlob, pos: &str, gloss: &str) -> SenseRecord {
    SenseRecord {
        glosses: vec![GlossRecord {
            typ: GlossType::Unspecified,
            text: blob.intern_str(gloss),
        }],
        detail: SenseDetail::packed_word(Pos::from_strs([pos]).0, vec![], vec![]),
        extras: None,
    }
}

fn name_sense(blob: &mut ToyBlob, gloss: &str) -> SenseRecord {
    SenseRecord {
        glosses: vec![GlossRecord {
            typ: GlossType::Unspecified,
            text: blob.intern_str(gloss),
        }],
        detail: SenseDetail::packed_name(NameType::from_strs(["&surname;"]).0),
        extras: None,
    }
}

/// Index rows from raw key strings (pack-time `build_index` input).
fn index_rows(raw: &[Vec<String>], reverse: bool) -> Vec<IndexRecord> {
    let mut index: Vec<IndexRecord> = raw
        .iter()
        .enumerate()
        .flat_map(|(i, keys)| {
            keys.iter().map(move |k| IndexRecord {
                key: if reverse {
                    to_key(&normalize_jap(&k.chars().rev().collect::<String>()))
                } else {
                    to_key(&normalize_jap(k))
                },
                record_idx: i as u32,
            })
        })
        .collect();
    index.sort_by_key(|e| e.key);
    index
}

/// Split artifact from toy entries plus raw keys (pack-time shape:
/// real indexes, flat key ids, entry offsets from frame lengths).
fn assemble(
    mut blob: ToyBlob,
    records: Vec<EntryRecord>,
    raw: Vec<Vec<String>>,
    magic: &[u8; 4],
) -> Vec<u8> {
    let mut key_ids = Vec::new();
    let mut key_offsets = Vec::with_capacity(records.len() + 1);
    for keys in &raw {
        key_offsets.push(key_ids.len() as u32);
        for k in keys {
            key_ids.push(pid(&mut blob, k));
        }
    }
    key_offsets.push(key_ids.len() as u32);
    let freqs = records
        .iter()
        .map(|r| record_freq(&r.kanji, &r.readings))
        .collect();
    let mut entry_offsets = Vec::with_capacity(records.len() + 1);
    let mut blob_len = 0u32;
    for e in &records {
        entry_offsets.push(blob_len);
        blob_len += postcard::to_allocvec(e).expect("encode").len() as u32;
    }
    entry_offsets.push(blob_len);
    let (trigrams, trigram_offsets, trigram_postings) = blob.gram_postings(&raw, normalize_jap);
    let mut meaning_keys: Vec<Vec<String>> = Vec::with_capacity(records.len());
    for e in &records {
        let mut texts = Vec::new();
        for sense in &e.senses {
            for g in &sense.glosses {
                texts.push(blob.slice(g.text).to_string());
            }
        }
        meaning_keys.push(texts);
    }
    let (meaning_trigrams, meaning_offsets, meaning_postings) =
        blob.gram_postings(&meaning_keys, normalize_meaning);
    let (pool, str_blob) = blob.finish();
    let index = DictIndex {
        version: "t".into(),
        pool,
        str_blob,
        forward_index: index_rows(&raw, false),
        reverse_index: index_rows(&raw, true),
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
    crate::gloss::gloss_to_bytes(&index, &records, magic)
}

fn word(
    blob: &mut ToyBlob,
    ent_seq: u32,
    keb: &str,
    reb: &str,
    pos: &str,
    gloss: &str,
) -> EntryRecord {
    EntryRecord {
        ent_seq,
        jlpt: 0,
        kanji: vec![ke(&mut *blob, keb)],
        readings: vec![re(&mut *blob, reb)],
        senses: vec![sense(&mut *blob, pos, gloss)],
    }
}

fn indexed_words() -> Vec<u8> {
    let mut blob = ToyBlob::new();
    let records = vec![
        word(
            &mut blob,
            1000000,
            "日本語",
            "にほんご",
            "&n;",
            "Japanese (language)",
        ),
        word(&mut blob, 1000010, "田中", "たなか", "&n;", "rice field"),
    ];
    let raw = vec![
        vec!["日本語".into(), "にほんご".into()],
        vec!["田中".into(), "たなか".into()],
    ];
    assemble(blob, records, raw, GLOSS_JMDICT_MAGIC)
}

#[test]
fn ffi_search_dedupes_split_key_hits() {
    // Record 0 matches through two keys with record 1's key ordered
    // between them: key order yields [0, 1, 0], and only the
    // post-sort dedup collapses it to one row per record.
    let mut blob = ToyBlob::new();
    let records = vec![
        word(&mut blob, 1, "あa00000", "あz00000", "&n;", "first"),
        word(&mut blob, 2, "あm00000", "あm00000", "&n;", "second"),
    ];
    let raw = vec![
        vec!["あa00000".into(), "あz00000".into()],
        vec!["あm00000".into(), "あm00000".into()],
    ];
    let dict = Dictionary::new(assemble(blob, records, raw, GLOSS_JMDICT_MAGIC)).unwrap();
    let hits = dict.search_indices("あ".into(), SearchMode::Prefix, 0xFFFFFFFF);
    // Relevance order (not file order): equal scores tie-break on
    // the kana (あm… < あz…), so record 1 leads.
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].idx, 1);
    assert_eq!(hits[1].idx, 0);
}

fn indexed_names() -> Vec<u8> {
    let mut blob = ToyBlob::new();
    let records = vec![EntryRecord {
        ent_seq: 5000000,
        jlpt: 0,
        kanji: vec![ke(&mut blob, "田中")],
        readings: vec![re(&mut blob, "たなか")],
        senses: vec![name_sense(&mut blob, "Tanaka")],
    }];
    let raw = vec![vec!["田中".into(), "たなか".into()]];
    assemble(blob, records, raw, GLOSS_JMNEDICT_MAGIC)
}

/// Valid-but-empty artifact (header only, no rows).
fn empty_artifact(magic: &[u8; 4]) -> Vec<u8> {
    crate::gloss::gloss_to_bytes(
        &DictIndex {
            version: "t".into(),
            pool: vec![],
            str_blob: vec![],
            forward_index: vec![],
            reverse_index: vec![],
            meaning_trigrams: vec![],
            meaning_offsets: vec![0],
            meaning_postings: vec![],
            trigrams: vec![],
            trigram_offsets: vec![0],
            trigram_postings: vec![],
            key_ids: vec![],
            key_offsets: vec![0],
            freqs: vec![],
            entry_offsets: vec![0],
        },
        &[],
        magic,
    )
}

fn toy_words() -> Arc<Dictionary> {
    Dictionary::new(indexed_words()).unwrap()
}

fn toy_names() -> Arc<Dictionary> {
    Dictionary::new(indexed_names()).unwrap()
}

/// Same toy words via a temp file (the Qt path-constructor flow).
fn toy_words_path() -> String {
    let dir = std::env::temp_dir();
    let words = dir.join("ffi-test-words.bin");
    std::fs::write(&words, indexed_words()).unwrap();
    words.to_str().unwrap().into()
}

fn toy_kanji() -> Vec<u8> {
    let mut rich = KanjiRecord::bare(
        '日',
        vec![Stroke::new(vec![
            Point::new(10.0, 50.0),
            Point::new(90.0, 50.0),
        ])],
    );
    rich.on = vec!["ニチ".into(), "ジツ".into()];
    rich.kun = vec!["ひ".into()];
    rich.meanings_en = vec!["day".into(), "sun".into()];
    rich.grade = Some(1);
    rich.parts = vec!['口', '一'];
    rich.jis208_code = Some([16, 1]);
    rich.nanori = vec!["あき".into()];
    rich.rad_names = vec!["ひ".into()];
    rich.chinese = vec!["ri4".into()];
    rich.korean = vec!["il".into()];
    rich.vietnamese = vec!["nhật".into()];
    rich.skip = vec![Skip {
        pattern: 4,
        first: 4,
        second: 3,
    }];
    rich.four_corner = vec![FourCorner {
        top_left: 6,
        top_right: 0,
        bottom_left: 1,
        bottom_right: 0,
        extra: 0,
    }];
    rich.deroo = vec![Deroo(1444)];
    rich.sh_desc = vec![ShDesc {
        radical_strokes: 3,
        radical: 'k',
        other_strokes: 11,
        index: 2,
    }];
    rich.jis208 = vec![Kuten {
        plane: 1,
        ku: 48,
        ten: 19,
    }];
    rich.nelson_c = vec![Nelson(123)];
    rich.njecd = vec![Njecd(456)];
    rich.oneill = vec![Oneill {
        num: 2078,
        suffix: Some('A'),
    }];
    let file = KanjiFile {
        kanjivg_version: "t".into(),
        kanjidic2_version: "t".into(),
        records: vec![rich, KanjiRecord::bare('一', vec![])],
    };
    kanji_to_bytes(&file)
}

#[test]
fn ffi_search_finds_prefix_hits_with_content() {
    let d = toy_words();
    assert_eq!(d.count(), 2);
    let hits = d.search("日本".into(), SearchMode::Prefix, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "日本語");
    assert_eq!(hits[0].reading, "にほんご");
    assert!(!hits[0].is_name);
    assert_eq!(hits[0].ent_seq, 1000000);
    assert_eq!(hits[0].senses.len(), 1);
    assert_eq!(hits[0].senses[0].pos, vec![Pos::N as u8]);
    assert_eq!(hits[0].senses[0].glosses, vec!["Japanese (language)"]);
}

#[test]
fn ffi_labels_resolve_indices() {
    assert_eq!(pos_label(Pos::N as u8), "&n;");
    assert_eq!(name_type_label(NameType::Surname as u8), "&surname;");
    assert_eq!(field_label(Field::Buddh as u8), "&Buddh;");
    assert_eq!(misc_label(Misc::Abbr as u8), "&abbr;");
    assert_eq!(pos_label(83), "");
    assert_eq!(name_type_label(25), "");
    assert_eq!(field_label(96), "");
    assert_eq!(misc_label(52), "");
}

#[test]
fn ffi_descriptions_resolve_indices() {
    assert_eq!(pos_description(Pos::V1 as u8), "Ichidan verb");
    assert_eq!(
        name_type_description(NameType::Surname as u8),
        "family or surname"
    );
    assert_eq!(field_description(Field::Comp as u8), "computing");
    assert_eq!(
        misc_description(Misc::Uk as u8),
        "word usually written using kana alone"
    );
    assert_eq!(pos_description(83), "");
    assert_eq!(name_type_description(25), "");
    assert_eq!(field_description(96), "");
    assert_eq!(misc_description(52), "");
}

#[test]
fn ffi_sense_carries_field_and_misc() {
    // Words-side tags ride the boundary as indices; names carry
    // none (empty, never garbage).
    let mut blob = ToyBlob::new();
    let word = SenseRecord {
        glosses: vec![GlossRecord {
            typ: GlossType::Unspecified,
            text: blob.intern_str("temple"),
        }],
        detail: SenseDetail::packed_word(
            Pos::from_strs(["&n;"]).0,
            Field::from_strs(["&Buddh;"]).0,
            Misc::from_strs(["&arch;"]).0,
        ),
        extras: None,
    };
    assert_eq!(word.field_indices(), vec![Field::Buddh as u8]);
    let name = SenseRecord {
        glosses: vec![GlossRecord {
            typ: GlossType::Unspecified,
            text: blob.intern_str("Tanaka"),
        }],
        detail: SenseDetail::packed_name(NameType::from_strs(["&surname;"]).0),
        extras: None,
    };
    assert!(name.field_indices().is_empty());
    assert!(name.misc_indices().is_empty());
}

#[test]
fn ffi_entry_carries_elements_and_extras() {
    let mut blob = ToyBlob::new();
    let keb = blob.intern_key("日本語");
    let reb = blob.intern_key("にほんご");
    let text = |b: &mut ToyBlob, s: &str| b.intern_str(s);
    let entry = EntryRecord {
        ent_seq: 1,
        jlpt: 5,
        kanji: vec![KanjiElementRecord {
            keb,
            info: Inf::from_strs(["&ateji;"]).0,
            priorities: Priority::from_strs(["news1"]).0,
        }],
        readings: vec![ReadingElementRecord {
            reb,
            nokanji: false,
            restricted_to: vec![text(&mut blob, "日本語")],
            info: vec![],
            priorities: vec![],
        }],
        senses: vec![SenseRecord {
            glosses: vec![GlossRecord {
                typ: GlossType::Unspecified,
                text: text(&mut blob, "Japanese"),
            }],
            detail: SenseDetail::packed_word(Pos::from_strs(["&n;"]).0, vec![], vec![]),
            extras: SenseExtras::packed(
                vec![text(&mut blob, "日本語")],
                vec![],
                vec![text(&mut blob, "日本")],
                vec![text(&mut blob, "外国語")],
                vec![text(&mut blob, "note")],
                vec![LangSourceRecord {
                    lang: Some(text(&mut blob, "eng")),
                    text: text(&mut blob, "source"),
                }],
            ),
        }],
    };
    let (pool, str_blob) = blob.finish();
    let pool_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pool).expect("encode");
    let archived = rkyv::access::<
        rkyv::Archived<Vec<crate::gloss::record::StrRef>>,
        rkyv::rancor::Error,
    >(&pool_bytes)
    .expect("view");
    let got = DictEntry::from_native(&entry, archived, &str_blob, false);
    assert_eq!(got.heading, "日本語");
    assert_eq!(got.jlpt, 5);
    assert_eq!(got.kanji.len(), 1);
    assert_eq!(got.kanji[0].keb, "日本語");
    assert_eq!(got.kanji[0].info, vec![Inf::Ateji as u8]);
    assert_eq!(got.kanji[0].priorities, vec!["news1"]);
    assert_eq!(got.readings[0].restricted_to, vec!["日本語"]);
    assert!(!got.readings[0].nokanji);
    let sense = &got.senses[0];
    assert_eq!(sense.xref, vec!["日本"]);
    assert_eq!(sense.ant, vec!["外国語"]);
    assert_eq!(sense.stagk, vec!["日本語"]);
    assert!(sense.stagr.is_empty());
    assert_eq!(sense.info, vec!["note"]);
    assert_eq!(sense.sources.len(), 1);
    assert_eq!(sense.sources[0].lang, Some("eng".into()));
    assert_eq!(sense.sources[0].text, "source");
    assert_eq!(inf_label(Inf::Ateji as u8), "&ateji;");
    assert_eq!(inf_label(11), "");
}

#[test]
fn ffi_search_suffix_and_toggles() {
    let words = toy_words();
    let hits = words.search("語".into(), SearchMode::Suffix, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "日本語");
    // Prefix query is not a suffix hit.
    assert!(
        words
            .search("日本".into(), SearchMode::Suffix, 10)
            .is_empty()
    );
    // The names file carries the surname separately.
    let names = toy_names();
    let hits = names.search("田中".into(), SearchMode::Prefix, 10);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].is_name);
    // ... while the words file holds the rice-field word.
    let hits = words.search("田中".into(), SearchMode::Prefix, 10);
    assert_eq!(hits.len(), 1);
    assert!(!hits[0].is_name);
    // Cap truncates within the file.
    let hits = words.search("田".into(), SearchMode::Prefix, 1);
    assert_eq!(hits.len(), 1);
    assert!(!hits[0].is_name);
    // Empty query and zero cap return empty without scanning.
    assert!(words.search("".into(), SearchMode::Prefix, 10).is_empty());
    assert!(
        words
            .search("日本".into(), SearchMode::Prefix, 0)
            .is_empty()
    );
}

#[test]
fn ffi_search_exact_matches_forms_only() {
    let d = toy_words();
    // Exact key: the record, no partials around it.
    let hits = d.search("日本語".into(), SearchMode::Exact, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "日本語");
    // Prefix of a key is not an exact hit.
    assert!(d.search("日本".into(), SearchMode::Exact, 10).is_empty());
    // Conjugated forms stay out (no deinflection bucket).
    let verbs = verb_dict();
    assert!(
        verbs
            .search("たべた".into(), SearchMode::Exact, 10)
            .is_empty()
    );
    // ... while the dictionary form itself still hits (verb and
    // noun share the key — exact is per key, not per record).
    let hits = verbs.search("たべる".into(), SearchMode::Exact, 10);
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.heading == "食べる"));
}

#[test]
fn ffi_search_middle_finds_substrings() {
    let words = toy_words();
    // Mid-key span (neither prefix nor suffix of any key).
    let hits = words.search("ほんご".into(), SearchMode::Middle, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "日本語");
    // Full-key query matches through the same tier in each file.
    let hits = words.search("たなか".into(), SearchMode::Middle, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "田中");
    let hits = toy_names().search("たなか".into(), SearchMode::Middle, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "田中");
    // Absent trigrams find nothing.
    assert!(
        words
            .search("ほんX".into(), SearchMode::Middle, 10)
            .is_empty()
    );
}

#[test]
fn ffi_search_meaning_finds_gloss_substrings() {
    let words = toy_words();
    // Mid-gloss span, case-insensitive.
    let hits = words.search_meaning_indices("apanes".into(), 0xFFFFFFFF);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].idx, 0);
    assert!(hits[0].chain.is_empty());
    let hits = words.search_meaning_indices("JAPANESE".into(), 0xFFFFFFFF);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].idx, 0);
    // The names file carries its own gloss ("Tanaka" surname).
    let names = toy_names();
    let hits = names.search_meaning_indices("tanaka".into(), 0xFFFFFFFF);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].idx, 0);
    // ... absent from the words file; empty query finds nothing.
    assert!(
        words
            .search_meaning_indices("tanaka".into(), 0xFFFFFFFF)
            .is_empty()
    );
    let hits = words.search_meaning_indices("rice".into(), 0xFFFFFFFF);
    assert_eq!(hits.len(), 1);
    assert!(words.search_meaning_indices("".into(), 10).is_empty());
    assert!(words.search_meaning_indices("zzz".into(), 10).is_empty());
}

#[test]
fn ffi_rejects_garbage_artifacts() {
    assert!(matches!(
        Dictionary::new(b"not a dictionary".to_vec()),
        Err(DictError::InvalidArtifact { .. })
    ));
    let empty_words = empty_artifact(GLOSS_JMDICT_MAGIC);
    let empty_names = empty_artifact(GLOSS_JMNEDICT_MAGIC);
    // Empty-but-valid artifacts construct fine (header only, no
    // validation): searches just come back empty.
    let empty = Dictionary::new(empty_words).unwrap();
    assert_eq!(empty.count(), 0);
    assert!(
        empty
            .search("日本".into(), SearchMode::Prefix, 10)
            .is_empty()
    );
    let empty = Dictionary::new(empty_names).unwrap();
    assert_eq!(empty.count(), 0);
    assert!(matches!(
        KanjiDictionary::new(b"not kanji".to_vec()),
        Err(DictError::InvalidArtifact { .. })
    ));
    assert!(matches!(
        Dictionary::from_path("/no/such/file.bin".into()),
        Err(DictError::InvalidArtifact { .. })
    ));
}

#[test]
fn ffi_from_path_searches_mapped_files() {
    let words = toy_words_path();
    let d = Dictionary::from_path(words).unwrap();
    assert_eq!(d.count(), 2);
    let hits = d.search("日本".into(), SearchMode::Prefix, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "日本語");
}

#[test]
fn ffi_kanji_lookup_finds_metadata() {
    let k = KanjiDictionary::new(toy_kanji()).unwrap();
    assert_eq!(k.kanji_count(), 2);
    let hit = k.lookup("日".into()).unwrap();
    assert_eq!(hit.character, "日");
    assert_eq!(hit.on, vec!["ニチ", "ジツ"]);
    assert_eq!(hit.kun, vec!["ひ"]);
    assert_eq!(hit.meanings, vec!["day", "sun"]);
    assert_eq!(hit.grade, Some(1));
    assert_eq!(hit.jlpt, None);
    assert_eq!(hit.strokes, 1);
    assert_eq!(hit.parts, vec!["口", "一"]);
    let jis = hit.jis208.expect("toy has jis208");
    assert_eq!((jis.ku, jis.ten), (16, 1));
    assert_eq!(hit.nanori, vec!["あき"]);
    assert_eq!(hit.rad_names, vec!["ひ"]);
    assert_eq!(hit.chinese, vec!["ri4"]);
    assert_eq!(hit.korean, vec!["il"]);
    assert_eq!(hit.vietnamese, vec!["nhật"]);
    assert_eq!(hit.skip, vec!["4-4-3"]);
    assert!(!hit.skip_misgrade);
    assert_eq!(hit.four_corner, vec!["6010.0"]);
    assert_eq!(hit.deroo, vec!["1444"]);
    assert_eq!(hit.sh_desc, vec!["3k11.2"]);
    assert_eq!(hit.jis208_refs, vec!["1-48-19"]);
    assert!(hit.jis212.is_empty());
    assert!(hit.jis213.is_empty());
    assert!(hit.deroo_variant.is_empty());
    assert_eq!(hit.nelson_c, vec!["123"]);
    assert_eq!(hit.njecd, vec!["456"]);
    assert_eq!(hit.oneill, vec!["2078A"]);
    assert!(hit.s_h.is_empty());
    // Bare record resolves with empty metadata.
    let bare = k.lookup("一".into()).unwrap();
    assert!(bare.meanings.is_empty());
    assert!(bare.parts.is_empty());
    assert!(bare.jis208.is_none());
    // Missing, empty, and multi-char inputs all miss.
    assert!(k.lookup("本".into()).is_none());
    assert!(k.lookup("".into()).is_none());
    assert!(k.lookup("日本".into()).is_none());
}

#[test]
fn ffi_entry_at_serves_rows_on_demand() {
    let d = toy_words();
    let idx = d.search_indices("日本".into(), SearchMode::Prefix, 10);
    assert_eq!(idx.len(), 1);
    let e = d.entry_at(idx[0].idx).unwrap();
    assert_eq!(e.heading, "日本語");
    assert_eq!(e.reading, "にほんご");
    assert!(!e.is_name);
    let n = toy_names().entry_at(0).unwrap();
    assert!(n.is_name);
    assert_eq!(n.heading, "田中");
    assert!(d.entry_at(9999).is_none());
}

/// Toy verb/adjective dictionary for deinflection tests: ichidan
/// 食べる, godan 飲む, i-adjective 高い, plus a class-less noun
/// sharing the たべる reading (the class filter must reject it for
/// conjugated queries).
fn indexed_verbs() -> Vec<u8> {
    let mut blob = ToyBlob::new();
    let records = vec![
        word(&mut blob, 2000000, "食べる", "たべる", "&v1;", "to eat"),
        word(&mut blob, 2000010, "飲む", "のむ", "&v5m;", "to drink"),
        word(&mut blob, 2000020, "高い", "たかい", "&adj-i;", "high"),
        word(
            &mut blob,
            2000030,
            "食べる",
            "たべる",
            "&n;",
            "eating (noun)",
        ),
    ];
    let raw = vec![
        vec!["食べる".into(), "たべる".into()],
        vec!["飲む".into(), "のむ".into()],
        vec!["高い".into(), "たかい".into()],
        vec!["食べる".into(), "たべる".into()],
    ];
    assemble(blob, records, raw, GLOSS_JMDICT_MAGIC)
}

fn verb_dict() -> Arc<Dictionary> {
    Dictionary::new(indexed_verbs()).unwrap()
}

#[test]
fn ffi_search_deinflects_conjugated_forms() {
    let d = verb_dict();
    // (conjugated query, dictionary-form record index, reason).
    let cases = [
        ("たべた", 0, "past"),
        ("たべない", 0, "neg."),
        ("のんだ", 1, "past"),
        ("のんで", 1, "conjunctive"),
        ("たかかった", 2, "past"),
    ];
    for (query, idx, reason) in cases {
        let hits = d.search_indices(query.into(), SearchMode::Prefix, 10);
        assert_eq!(hits.len(), 1, "{query} finds one hit");
        assert_eq!(hits[0].idx, idx, "{query} finds record {idx}");
        assert_eq!(hits[0].chain, vec![reason.to_string()], "{query} chain");
        // The hit materializes through the normal row path.
        let entry = d.entry_at(hits[0].idx).unwrap();
        assert_eq!(entry.heading, ["食べる", "飲む", "高い"][idx as usize]);
    }
    // Class filter: the noun 食べる shares the たべる reading but
    // carries no verb class, so the conjugated query rejects it.
    let hits = d.search_indices("たべた".into(), SearchMode::Prefix, 10);
    assert!(hits.iter().all(|h| h.idx == 0));
    // Direct hits carry no chain and win over deinflected ones: the
    // dictionary-form query matches both verb and noun directly.
    let hits = d.search_indices("たべる".into(), SearchMode::Prefix, 10);
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.chain.is_empty()));
    // Unknown conjugations stay empty (no over-generation); the
    // passthrough exposes the unverified candidates instead.
    assert!(
        d.search_indices("xyz".into(), SearchMode::Prefix, 10)
            .is_empty()
    );
    let candidates = d.deinflect("たべた".into());
    assert!(
        candidates
            .iter()
            .any(|c| c.surface == "たべる" && c.chain == vec!["past".to_string()])
    );
}

#[test]
fn ffi_search_tiers_exact_deinflected_partial() {
    // たべる (v1 verb), たべた (noun), たべたて (noun): query たべた
    // hits all three tiers — exact, deinflected past, edge-partial.
    let mut blob = ToyBlob::new();
    let records = vec![
        word(&mut blob, 1, "たべる", "たべる", "&v1;", "to eat"),
        word(&mut blob, 2, "たべた", "たべた", "&n;", "ate (noun)"),
        word(&mut blob, 3, "たべたて", "たべたて", "&n;", "freshly eaten"),
    ];
    let raw = vec![
        vec!["たべる".into(), "たべる".into()],
        vec!["たべた".into(), "たべた".into()],
        vec!["たべたて".into(), "たべたて".into()],
    ];
    let dict = Dictionary::new(assemble(blob, records, raw, GLOSS_JMDICT_MAGIC)).unwrap();
    let hits = dict.search_indices("たべた".into(), SearchMode::Prefix, 10);
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].idx, 1);
    assert!(hits[0].chain.is_empty());
    assert!(hits[0].matched.is_empty());
    assert_eq!(hits[1].idx, 0);
    assert_eq!(hits[1].chain, vec!["past".to_string()]);
    assert_eq!(hits[1].matched, "たべる");
    assert_eq!(hits[2].idx, 2);
    assert!(hits[2].chain.is_empty());
    assert!(hits[2].matched.is_empty());
}

#[test]
fn ffi_search_merged_bucket_orders_by_frequency() {
    // Same tier shape as above, but the verb carries frequency
    // (ichi1 on both forms): the single bucket orders by frequency,
    // so the deinflected verb leads the exact noun.
    let mut blob = ToyBlob::new();
    let prio = Priority::from_strs(["ichi1"]).0;
    let verb = EntryRecord {
        ent_seq: 1,
        jlpt: 0,
        kanji: vec![KanjiElementRecord {
            keb: pid(&mut blob, "たべる"),
            info: vec![],
            priorities: prio.clone(),
        }],
        readings: vec![ReadingElementRecord {
            reb: pid(&mut blob, "たべる"),
            nokanji: false,
            restricted_to: vec![],
            info: vec![],
            priorities: prio.clone(),
        }],
        senses: vec![sense(&mut blob, "&v1;", "to eat")],
    };
    let records = vec![
        verb,
        word(&mut blob, 2, "たべた", "たべた", "&n;", "ate (noun)"),
        word(&mut blob, 3, "たべたて", "たべたて", "&n;", "freshly eaten"),
    ];
    let raw = vec![
        vec!["たべる".into(), "たべる".into()],
        vec!["たべた".into(), "たべた".into()],
        vec!["たべたて".into(), "たべたて".into()],
    ];
    let dict = Dictionary::new(assemble(blob, records, raw, GLOSS_JMDICT_MAGIC)).unwrap();
    let hits = dict.search_indices("たべた".into(), SearchMode::Prefix, 10);
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].idx, 0);
    assert_eq!(hits[0].chain, vec!["past".to_string()]);
    assert_eq!(hits[0].matched, "たべる");
    assert_eq!(hits[1].idx, 1);
    assert!(hits[1].chain.is_empty());
    assert_eq!(hits[2].idx, 2);
    assert!(hits[2].chain.is_empty());
}

#[test]
fn ffi_kanji_strokes_serve_polylines() {
    let k = KanjiDictionary::new(toy_kanji()).unwrap();
    let strokes = k.strokes("日".into());
    assert_eq!(strokes.len(), 1);
    assert_eq!(strokes[0].points.len(), 2);
    assert_eq!(
        (strokes[0].points[0].x, strokes[0].points[0].y),
        (10.0, 50.0)
    );
    // Bare and missing characters have no geometry.
    assert!(k.strokes("一".into()).is_empty());
    assert!(k.strokes("本".into()).is_empty());
    assert!(k.strokes("".into()).is_empty());
}

#[test]
fn ffi_kanji_containing_lists_part_users() {
    let k = KanjiDictionary::new(toy_kanji()).unwrap();
    assert_eq!(k.containing("口".into()), vec!["日"]);
    assert_eq!(k.containing("一".into()), vec!["日"]);
    // Bare record has no parts; multi-char and empty miss.
    assert!(k.containing("日".into()).is_empty());
    assert!(k.containing("日本".into()).is_empty());
    assert!(k.containing("".into()).is_empty());
}

#[test]
fn ffi_search_middle_scans_words() {
    let d = toy_words();
    let hits = d.search_middle("日".into(), 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].heading, "日本語");
    assert_eq!(d.search_middle("田".into(), 10)[0].heading, "田中");
    // Shared char across records, capped.
    assert_eq!(d.search_middle("本".into(), 10)[0].heading, "日本語");
    assert!(d.search_middle("本".into(), 0).is_empty());
    // Kana readings count (all keys, unlike the old keb scan);
    // empty and multi-char inputs still miss (kanji scope).
    assert_eq!(d.search_middle("な".into(), 10)[0].heading, "田中");
    assert_eq!(
        toy_names().search_middle("な".into(), 10)[0].heading,
        "田中"
    );
    assert!(d.search_middle("".into(), 10).is_empty());
    assert!(d.search_middle("日本".into(), 10).is_empty());
}

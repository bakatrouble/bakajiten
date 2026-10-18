use super::file::{DictIndex, GLOSS_JMDICT_MAGIC, GLOSS_JMNEDICT_MAGIC, gloss_to_bytes};
use super::gloss_type::GlossType;
use super::name_type::NameType;
use super::pos::Pos;
use super::priority::Priority;
use super::record::{
    EntryRecord, GlossRecord, IndexRecord, KanjiElementRecord, ReadingElementRecord, SenseDetail,
    SenseExtras, SenseRecord, record_freq,
};
use super::search::{ArchivedDictionaryFile, SearchType, normalize_jap, normalize_meaning, to_key};
use super::testhelp::ToyBlob;
use super::view::{GlossView, decode_entry, gloss_from_bytes, gloss_header, gloss_view};

/// Test pool interning (mirrors pack-time first-occurrence order).
fn pid(blob: &mut ToyBlob, s: &str) -> u32 {
    blob.intern_key(s)
}

fn toy_entry(blob: &mut ToyBlob) -> EntryRecord {
    EntryRecord {
        ent_seq: 1000000,
        jlpt: 0,
        kanji: vec![KanjiElementRecord {
            keb: pid(blob, "日本語"),
            info: vec![],
            priorities: vec![Priority::Ichi1],
        }],
        readings: vec![ReadingElementRecord {
            reb: pid(blob, "にほんご"),
            nokanji: false,
            restricted_to: vec![],
            info: vec![],
            priorities: vec![Priority::Ichi1],
        }],
        senses: vec![SenseRecord {
            glosses: vec![GlossRecord {
                typ: GlossType::Unspecified,
                text: blob.intern_str("Japanese (language)"),
            }],
            detail: SenseDetail::packed_word(Pos::from_strs(["&n;"]).0, vec![], vec![]),
            extras: None,
        }],
    }
}

/// Index for `entries` with `raw_keys` (pack-time shape: real key ids
/// plus entry offsets from postcard frame lengths).
fn toy_index(
    mut blob: ToyBlob,
    entries: &[EntryRecord],
    raw_keys: &[Vec<String>],
    reverse: bool,
) -> DictIndex {
    let mut index: Vec<IndexRecord> = raw_keys
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
    let (forward_index, reverse_index) = if reverse {
        (vec![], index)
    } else {
        (index, vec![])
    };
    let (trigrams, trigram_offsets, trigram_postings) = blob.gram_postings(raw_keys, normalize_jap);
    // Meaning grams over the entries' gloss text (pack resolves
    // frame refs through the blob, same as finish()).
    let mut meaning_keys: Vec<Vec<String>> = Vec::with_capacity(entries.len());
    for e in entries {
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
    let mut key_ids = Vec::new();
    let mut key_offsets = Vec::with_capacity(entries.len() + 1);
    for keys in raw_keys {
        key_offsets.push(key_ids.len() as u32);
        for k in keys {
            key_ids.push(pid(&mut blob, k));
        }
    }
    key_offsets.push(key_ids.len() as u32);
    let freqs = entries
        .iter()
        .map(|r| super::record::record_freq(&r.kanji, &r.readings))
        .collect();
    let mut entry_offsets = Vec::with_capacity(entries.len() + 1);
    let mut blob_len = 0u32;
    for e in entries {
        entry_offsets.push(blob_len);
        blob_len += postcard::to_allocvec(e).expect("encode").len() as u32;
    }
    entry_offsets.push(blob_len);
    let (pool, str_blob) = blob.finish();
    DictIndex {
        version: "2026-10-10".into(),
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
    }
}

fn toy_pieces() -> (DictIndex, Vec<EntryRecord>) {
    let mut blob = ToyBlob::new();
    let entries = vec![toy_entry(&mut blob)];
    let raw = vec![vec![]];
    let index = toy_index(blob, &entries, &raw, false);
    (index, entries)
}

fn toy_bytes() -> Vec<u8> {
    let (index, entries) = toy_pieces();
    gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC)
}

#[test]
fn gloss_roundtrip_is_exact() {
    let (index, entries) = toy_pieces();
    let (back_index, back_entries) = gloss_from_bytes(
        &gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC),
        GLOSS_JMDICT_MAGIC,
    )
    .unwrap();
    assert_eq!(back_index, index);
    assert_eq!(back_entries, entries);
}

#[test]
fn gloss_rejects_garbage() {
    assert!(gloss_from_bytes(b"", GLOSS_JMDICT_MAGIC).is_err());
    assert!(gloss_from_bytes(b"GLD1", GLOSS_JMDICT_MAGIC).is_err());
    let full = toy_bytes();
    for len in 0..full.len() {
        assert!(
            gloss_from_bytes(&full[..len], GLOSS_JMDICT_MAGIC).is_err(),
            "len={len}"
        );
    }
    let mut long = full.clone();
    long.extend_from_slice(b"junk");
    assert!(
        gloss_from_bytes(&long, GLOSS_JMDICT_MAGIC)
            .unwrap_err()
            .contains("trailing")
    );
}

#[test]
fn gloss_rejects_sibling_magic() {
    // The point of the split: a JMdict artifact must not decode as
    // JMnedict input and vice versa.
    let words = toy_bytes();
    assert!(gloss_header(&words, GLOSS_JMNEDICT_MAGIC).is_err());
    assert!(
        gloss_from_bytes(&words, GLOSS_JMNEDICT_MAGIC)
            .unwrap_err()
            .contains("magic")
    );
    let mut aligned = rkyv::util::AlignedVec::<16>::new();
    aligned.extend_from_slice(&words);
    assert!(gloss_view(&aligned, GLOSS_JMNEDICT_MAGIC).is_err());
}

#[test]
fn gloss_view_reads_index_without_decoding() {
    let mut aligned = rkyv::util::AlignedVec::<16>::new();
    aligned.extend_from_slice(&toy_bytes());
    let view = gloss_view(&aligned, GLOSS_JMDICT_MAGIC).unwrap();
    assert_eq!(view.index.version, "2026-10-10");
    assert_eq!(view.index.len(), 1);
    assert_eq!(view.index.pool_str(0), "日本語");
    let frame = view.index.entry_frame(view.blob, 0).expect("frame");
    let entry = decode_entry(frame).unwrap();
    assert_eq!(entry.ent_seq, 1000000);
    assert_eq!(view.index.pool_str(entry.kanji[0].keb), "日本語");
    assert_eq!(view.index.pool_str(entry.readings[0].reb), "にほんご");
    assert_eq!(entry.senses[0].tag_entities(), vec!["&n;"]);
    assert_eq!(
        view.index.str_text(&entry.senses[0].glosses[0].text),
        "Japanese (language)"
    );
}

#[test]
fn gloss_view_from_mmap() {
    let path = std::env::temp_dir().join("gloss-view-test.bin");
    std::fs::write(&path, toy_bytes()).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    // SAFETY: the file is not mutated while mapped.
    let map = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let view = gloss_view(&map, GLOSS_JMDICT_MAGIC).unwrap();
    assert_eq!(view.index.len(), 1);
    let frame = view.index.entry_frame(view.blob, 0).expect("frame");
    assert_eq!(decode_entry(frame).unwrap().ent_seq, 1000000);
    drop(map);
    std::fs::remove_file(&path).ok();
}

#[test]
fn entry_frame_rejects_out_of_range() {
    let mut aligned = rkyv::util::AlignedVec::<16>::new();
    aligned.extend_from_slice(&toy_bytes());
    let view = gloss_view(&aligned, GLOSS_JMDICT_MAGIC).unwrap();
    assert!(view.index.entry_frame(view.blob, 1).is_none());
    assert!(decode_entry(b"\xff\xff").is_err());
}

#[test]
fn gloss_header_checks_without_decoding() {
    let bytes = toy_bytes();
    assert!(gloss_header(&bytes, GLOSS_JMDICT_MAGIC).is_ok());
    assert!(gloss_header(&bytes[..bytes.len() - 1], GLOSS_JMDICT_MAGIC).is_err());
    let mut bad = bytes.clone();
    bad[0] = b'X';
    assert!(
        gloss_header(&bad, GLOSS_JMDICT_MAGIC)
            .unwrap_err()
            .contains("magic")
    );
    assert!(gloss_header(b"", GLOSS_JMDICT_MAGIC).is_err());
}

/// Two entries with a real packed-style index, shared by the trait
/// search tests.
fn indexed_pieces() -> (DictIndex, Vec<EntryRecord>) {
    let mut blob = ToyBlob::new();
    let entry = |blob: &mut ToyBlob, ent_seq, keb: &str, reb: &str| EntryRecord {
        ent_seq,
        jlpt: 0,
        kanji: vec![KanjiElementRecord {
            keb: pid(blob, keb),
            info: vec![],
            priorities: vec![],
        }],
        readings: vec![ReadingElementRecord {
            reb: pid(blob, reb),
            nokanji: false,
            restricted_to: vec![],
            info: vec![],
            priorities: vec![],
        }],
        senses: vec![],
    };
    let entries = vec![
        entry(&mut blob, 1000000, "日本語", "にほんご"),
        entry(&mut blob, 1000010, "田中", "たなか"),
    ];
    let raw = vec![
        vec!["日本語".to_string(), "にほんご".to_string()],
        vec!["田中".to_string(), "たなか".to_string()],
    ];
    let index = toy_index(blob, &entries, &raw, false);
    (index, entries)
}

/// Zero-copy view over `buf` (caller keeps `buf` alive).
fn indexed_view(buf: &mut rkyv::util::AlignedVec<16>) -> GlossView<'_> {
    let (index, entries) = indexed_pieces();
    buf.extend_from_slice(&gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC));
    gloss_view(buf, GLOSS_JMDICT_MAGIC).unwrap()
}

#[test]
fn trait_search_finds_long_query() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = indexed_view(&mut buf);
    assert_eq!(
        view.index.search(SearchType::Prefix, "にほんご", 10),
        vec![0]
    );
}

#[test]
fn trait_search_finds_short_prefix() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = indexed_view(&mut buf);
    assert_eq!(view.index.search(SearchType::Prefix, "日本", 10), vec![0]);
}

#[test]
fn trait_search_finds_short_exact_key() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = indexed_view(&mut buf);
    assert_eq!(view.index.search(SearchType::Prefix, "田中", 10), vec![1]);
}

#[test]
fn trait_search_finds_short_kana_prefix() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = indexed_view(&mut buf);
    assert_eq!(view.index.search(SearchType::Prefix, "たな", 10), vec![1]);
}

#[test]
fn freq_points_follow_zkanji_table() {
    use super::record::{KanjiElementRecord, ReadingElementRecord};
    let ke = |pri| KanjiElementRecord {
        keb: 0,
        info: vec![],
        priorities: pri,
    };
    let re = |pri| ReadingElementRecord {
        reb: 0,
        nokanji: false,
        restricted_to: vec![],
        info: vec![],
        priorities: pri,
    };
    // ichi1: 2000 kanji-side, 2000 reading-side (readings halve
    // news/spec only).
    assert_eq!(
        record_freq(&[ke(vec![Priority::Ichi1])], &[re(vec![Priority::Ichi1])]),
        4000
    );
    // news1 kanji 1000 + news2 reading 250.
    assert_eq!(
        record_freq(&[ke(vec![Priority::News1])], &[re(vec![Priority::News2])]),
        1250
    );
    // nf25: (50-25)*25 either side.
    assert_eq!(record_freq(&[ke(vec![Priority::Nf25])], &[]), 625);
    // gai and empty: no points.
    assert_eq!(record_freq(&[ke(vec![Priority::Gai1])], &[]), 0);
    assert_eq!(record_freq(&[], &[]), 0);
}

#[test]
fn relevance_ranks_higher_frequency_first() {
    // Identical shapes (same lengths, no kanji): only the stored
    // frequency differs, so it alone decides the order.
    let mut blob = ToyBlob::new();
    let entry = |blob: &mut ToyBlob, ent_seq, keb: &str, pri| EntryRecord {
        ent_seq,
        jlpt: 0,
        kanji: vec![KanjiElementRecord {
            keb: pid(blob, keb),
            info: vec![],
            priorities: pri,
        }],
        readings: vec![],
        senses: vec![],
    };
    let entries = vec![
        entry(&mut blob, 1, "あき", vec![]),
        entry(&mut blob, 2, "あか", vec![Priority::Ichi1]),
    ];
    let raw = vec![vec!["あき".to_string()], vec!["あか".to_string()]];
    let index = toy_index(blob, &entries, &raw, false);
    let bytes = gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC);
    let mut aligned = rkyv::util::AlignedVec::<16>::new();
    aligned.extend_from_slice(&bytes);
    let view = gloss_view(&aligned, GLOSS_JMDICT_MAGIC).unwrap();
    let mut hits = view.index.search(SearchType::Prefix, "あ", usize::MAX);
    assert_eq!(hits.len(), 2);
    super::rank::sort_hits(view.index, view.blob, &mut hits);
    assert_eq!(hits, vec![1, 0]);
}

#[test]
fn relevance_prefers_shorter_kana_at_equal_scores() {
    // Same (zero) frequency: single kana outranks five kana.
    let mut blob = ToyBlob::new();
    let entry = |blob: &mut ToyBlob, ent_seq, keb: &str| EntryRecord {
        ent_seq,
        jlpt: 0,
        kanji: vec![KanjiElementRecord {
            keb: pid(blob, keb),
            info: vec![],
            priorities: vec![],
        }],
        readings: vec![],
        senses: vec![],
    };
    let entries = vec![entry(&mut blob, 1, "あいうえお"), entry(&mut blob, 2, "あ")];
    let raw = vec![vec!["あいうえお".to_string()], vec!["あ".to_string()]];
    let index = toy_index(blob, &entries, &raw, false);
    let bytes = gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC);
    let mut aligned = rkyv::util::AlignedVec::<16>::new();
    aligned.extend_from_slice(&bytes);
    let view = gloss_view(&aligned, GLOSS_JMDICT_MAGIC).unwrap();
    let mut hits = view.index.search(SearchType::Prefix, "あ", usize::MAX);
    assert_eq!(hits.len(), 2);
    super::rank::sort_hits(view.index, view.blob, &mut hits);
    assert_eq!(hits, vec![1, 0]);
}

#[test]
fn archived_index_stays_compact() {
    // Pins for the size work: byte index keys (no 8-alignment
    // padding) and a pointer-only index header. If rkyv's layout
    // changes, this fails first — re-audit before updating.
    use super::record::IndexRecord;
    use rkyv::Archived;
    assert_eq!(std::mem::size_of::<Archived<IndexRecord>>(), 12);
    // 88 + 4 archived-Vec headers (blob + 3 meaning tables):
    // re-audit before updating.
    assert_eq!(std::mem::size_of::<super::file::ArchivedDictIndex>(), 120);
}

#[test]
fn postcard_entry_is_compact() {
    // The whole point of the split: one entry must encode far below
    // its old 80-byte-sense rkyv shape. Re-audit if this moves.
    let mut blob = ToyBlob::new();
    let frame = postcard::to_allocvec(&toy_entry(&mut blob)).expect("encode");
    assert!(frame.len() < 120, "frame is {} bytes", frame.len());
}

#[test]
fn extras_pack_none_when_empty() {
    assert!(SenseExtras::packed(vec![], vec![], vec![], vec![], vec![], vec![]).is_none());
    let mut blob = ToyBlob::new();
    let xref = blob.intern_str("田中");
    let extras = SenseExtras::packed(vec![], vec![], vec![xref], vec![], vec![], vec![])
        .expect("non-empty xref packs Some");
    assert_eq!(extras.xref, vec![xref]);
}

#[test]
fn detail_packs_none_when_untagged() {
    assert!(SenseDetail::packed_word(vec![], vec![], vec![]).is_none());
    assert!(SenseDetail::packed_name(vec![]).is_none());
    assert!(SenseDetail::packed_name(NameType::from_strs(["&surname;"]).0).is_some());
}

#[test]
fn xref_reads_through_none_and_some() {
    let mut blob = ToyBlob::new();
    let mut entry = toy_entry(&mut blob);
    let xref_ref = blob.intern_str("田中");
    entry.senses[0].extras =
        SenseExtras::packed(vec![], vec![], vec![xref_ref], vec![], vec![], vec![]);
    let entries = vec![entry];
    let raw = vec![vec![]];
    let index = toy_index(blob, &entries, &raw, false);
    let bytes = gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC);
    let (_, back) = gloss_from_bytes(&bytes, GLOSS_JMDICT_MAGIC).unwrap();
    assert_eq!(back[0].senses[0].xref(), &[xref_ref]);
    assert!(toy_pieces().1[0].senses[0].xref().is_empty());
}

#[test]
fn byte_keys_order_like_be_prefixes() {
    // Shorter keys sort before their extensions (zero padding), and
    // same-prefix keys cluster — the range scan relies on both.
    assert!(to_key("日本") < to_key("日本語"));
    assert_eq!(to_key("日本語")[..6], to_key("日本")[..6]);
}

/// Worked acceptance chains: each query
/// must produce the dictionary form with the documented reason
/// chain (engine-level — no dictionary verification here).
#[test]
fn deinflect_walks_doc_chains() {
    use super::deinflect::deinflect;
    // (query, dictionary form, reason chain).
    let chains = [
        ("たべた", "たべる", vec!["past"]),
        ("のんで", "のむ", vec!["conjunctive"]),
        ("きって", "きる", vec!["conjunctive"]),
        ("いって", "いく", vec!["conjunctive"]),
        ("たべない", "たべる", vec!["neg."]),
        ("かわない", "かう", vec!["neg."]),
        ("たべよう", "たべる", vec!["volitional / tentative"]),
        ("かこう", "かく", vec!["volitional / tentative"]),
        ("たべられる", "たべる", vec!["potential / passive"]),
        ("たべさせる", "たべる", vec!["causative"]),
        (
            "たべさせられた",
            "たべる",
            vec!["past", "potential / passive", "causative"],
        ),
        ("かかされる", "かかる", vec!["passive", "causative"]),
        ("きて", "くる", vec!["conjunctive"]),
        ("して", "する", vec!["conjunctive"]),
        (
            "きてください",
            "くる",
            vec!["ください strip", "conjunctive"],
        ),
        ("たべてる", "たべる", vec!["continuous", "conjunctive"]),
        (
            "たべちゃう",
            "たべる",
            vec!["finished / bothersome", "conjunctive"],
        ),
        (
            "いっとく",
            "いく",
            vec!["do in advance / keep ~ing", "conjunctive"],
        ),
        ("しています", "する", vec!["polite progressive suru"]),
        (
            "かかせていただきます",
            "かかる",
            vec!["humble", "conjunctive", "command", "causative"],
        ),
        ("たかかった", "たかい", vec!["past"]),
        ("たかくて", "たかい", vec!["conjunctive"]),
        ("たかければ", "たかい", vec!["-ba conditional"]),
        ("たべなければ", "たべる", vec!["-ba conditional", "neg."]),
        (
            "いかなきゃ",
            "いく",
            vec!["must / neg. -ba cond.", "-ba conditional", "neg."],
        ),
        (
            "いかなければならない",
            "いく",
            vec!["obligation", "-ba conditional", "neg."],
        ),
    ];
    for (query, surface, chain) in chains {
        let hit = deinflect(query).into_iter().find(|c| c.surface == surface);
        assert!(hit.is_some(), "{query} reaches {surface}");
        assert_eq!(hit.expect("found").chain, chain, "{query} chain");
    }
}

/// Same-surface derivations union their claims: たべられる reaches
/// たべる as godan-5r and as ichidan, and the class filter needs
/// the ichidan claim to keep the 食べる hit.
#[test]
fn deinflect_unions_same_surface_claims() {
    use super::deinflect::{InflClass, deinflect};
    let hit = deinflect("たべられる")
        .into_iter()
        .find(|c| c.surface == "たべる")
        .expect("candidate");
    assert!(hit.classes.contains(InflClass::V1));
    assert!(hit.classes.contains(InflClass::V5r));
}

/// Every tag variant carries a non-empty tooltip description
/// (transcribed from the variant docs; the exhaustive matches in
/// `description()` fail to compile on drift).
#[test]
fn tag_descriptions_cover_all_tables() {
    use super::field::Field;
    use super::misc::Misc;
    for i in 0..=u8::MAX {
        if let Some(tag) = Pos::from_index(i) {
            assert!(!tag.description().is_empty(), "pos {i}");
        }
        if let Some(tag) = Misc::from_index(i) {
            assert!(!tag.description().is_empty(), "misc {i}");
        }
        if let Some(tag) = Field::from_index(i) {
            assert!(!tag.description().is_empty(), "field {i}");
        }
        if let Some(tag) = NameType::from_index(i) {
            assert!(!tag.description().is_empty(), "name {i}");
        }
    }
    // Spot checks (doc-transcription fidelity).
    assert_eq!(Pos::from_strs(["&v1;"]).0[0].description(), "Ichidan verb");
    assert_eq!(
        Misc::from_strs(["&uk;"]).0[0].description(),
        "word usually written using kana alone"
    );
    assert_eq!(Field::from_strs(["&comp;"]).0[0].description(), "computing");
    assert_eq!(
        NameType::from_strs(["&surname;"]).0[0].description(),
        "family or surname"
    );
}

#[test]
fn middle_search_finds_substrings() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = indexed_view(&mut buf);
    // Mid-key spans (trigram path): kanji form and reading alike.
    assert_eq!(view.index.search(SearchType::Middle, "本語", 10), vec![0]);
    assert_eq!(view.index.search(SearchType::Middle, "ほんご", 10), vec![0]);
    assert_eq!(view.index.search(SearchType::Middle, "日本語", 10), vec![0]);
    assert_eq!(view.index.search(SearchType::Middle, "たなか", 10), vec![1]);
}

#[test]
fn middle_search_rejects_missing_trigrams() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = indexed_view(&mut buf);
    // Every trigram must post: one absent trigram empties the hit.
    assert!(
        view.index
            .search(SearchType::Middle, "にほんごX", 10)
            .is_empty()
    );
    assert!(
        view.index
            .search(SearchType::Middle, "本語X", 10)
            .is_empty()
    );
}

#[test]
fn middle_search_verifies_shared_trigrams() {
    // All query trigrams post in record 0, but the query itself is
    // absent (order differs): intersection filters, verification
    // rejects — shared trigrams never imply containment.
    let mut blob = ToyBlob::new();
    let entry = |blob: &mut ToyBlob, keb: &str| EntryRecord {
        ent_seq: 1,
        jlpt: 0,
        kanji: vec![KanjiElementRecord {
            keb: pid(blob, keb),
            info: vec![],
            priorities: vec![],
        }],
        readings: vec![],
        senses: vec![],
    };
    let entries = vec![entry(&mut blob, "あいあい"), entry(&mut blob, "おはよう")];
    let raw = vec![vec!["あいあい".to_string()], vec!["おはよう".to_string()]];
    let index = toy_index(blob, &entries, &raw, false);
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    buf.extend_from_slice(&gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC));
    let view = gloss_view(&buf, GLOSS_JMDICT_MAGIC).unwrap();
    assert_eq!(view.index.search(SearchType::Middle, "あいあ", 10), vec![0]);
    assert!(
        view.index
            .search(SearchType::Middle, "いあいあ", 10)
            .is_empty()
    );
}

#[test]
fn middle_search_scans_short_queries() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = indexed_view(&mut buf);
    // Sub-trigram queries carry no trigrams: bounded scan instead.
    assert_eq!(view.index.search(SearchType::Middle, "本", 10), vec![0]);
    assert_eq!(view.index.search(SearchType::Middle, "たな", 10), vec![1]);
    assert_eq!(view.index.search(SearchType::Middle, "本", 1), vec![0]);
    assert!(view.index.search(SearchType::Middle, "zz", 10).is_empty());
}

#[test]
fn trigram_postings_shape_holds() {
    // Unique sorted grams, n+1 offsets, ascending postings.
    let mut blob = ToyBlob::new();
    let (grams, offsets, postings) = blob.gram_postings(
        &[vec!["あいうえお".to_string(), "あい".to_string()]],
        normalize_jap,
    );
    assert_eq!(offsets.len(), grams.len() + 1);
    assert_eq!(*offsets.last().unwrap() as usize, postings.len());
    for w in offsets.windows(2) {
        let (s, e) = (w[0] as usize, w[1] as usize);
        assert!(postings[s..e].windows(2).all(|p| p[0] <= p[1]));
    }
    let (_, bytes) = blob.finish();
    let contents: Vec<&str> = grams
        .iter()
        .map(|g| {
            let (o, l) = (g.offset as usize, g.len as usize);
            std::str::from_utf8(&bytes[o..o + l]).expect("whole grams")
        })
        .collect();
    let mut sorted = contents.clone();
    sorted.sort_unstable();
    assert_eq!(contents, sorted);
    // Short keys contribute nothing.
    let mut blob = ToyBlob::new();
    let (grams, _, _) = blob.gram_postings(&[vec!["あい".to_string()]], normalize_jap);
    assert!(grams.is_empty());
}

/// Gloss-bearing pieces for meaning search (the shared trait
/// toys carry keys only).
fn meaning_pieces() -> (DictIndex, Vec<EntryRecord>) {
    let mut blob = ToyBlob::new();
    let entry = |blob: &mut ToyBlob, ent_seq, keb: &str, gloss: &str| EntryRecord {
        ent_seq,
        jlpt: 0,
        kanji: vec![KanjiElementRecord {
            keb: pid(blob, keb),
            info: vec![],
            priorities: vec![],
        }],
        readings: vec![],
        senses: vec![SenseRecord {
            glosses: vec![GlossRecord {
                typ: GlossType::Unspecified,
                text: blob.intern_str(gloss),
            }],
            detail: None,
            extras: None,
        }],
    };
    let entries = vec![
        entry(&mut blob, 1, "日本語", "Japanese (language)"),
        entry(&mut blob, 2, "田中", "rice field"),
    ];
    let raw = vec![vec!["日本語".to_string()], vec!["田中".to_string()]];
    let index = toy_index(blob, &entries, &raw, false);
    (index, entries)
}

fn meaning_view(buf: &mut rkyv::util::AlignedVec<16>) -> GlossView<'_> {
    let (index, entries) = meaning_pieces();
    buf.extend_from_slice(&gloss_to_bytes(&index, &entries, GLOSS_JMDICT_MAGIC));
    gloss_view(buf, GLOSS_JMDICT_MAGIC).unwrap()
}

#[test]
fn meaning_search_finds_gloss_substrings() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = meaning_view(&mut buf);
    // Mid-gloss spans (meaning trigram path), case-insensitive.
    assert_eq!(view.index.search_meaning(view.blob, "apanes", 10), vec![0]);
    assert_eq!(
        view.index.search_meaning(view.blob, "JAPANESE", 10),
        vec![0]
    );
    assert_eq!(
        view.index.search_meaning(view.blob, "rice field", 10),
        vec![1]
    );
}

#[test]
fn meaning_search_rejects_and_falls_back() {
    let mut buf = rkyv::util::AlignedVec::<16>::new();
    let view = meaning_view(&mut buf);
    assert!(view.index.search_meaning(view.blob, "zzz", 10).is_empty());
    // Sub-trigram queries scan instead.
    assert_eq!(view.index.search_meaning(view.blob, "fi", 10), vec![1]);
}

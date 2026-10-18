use super::*;

fn toy_file() -> KanjiFile {
    KanjiFile {
        kanjivg_version: "test-kanjivg".into(),
        kanjidic2_version: "2026-10-10".into(),
        records: toy_records(),
    }
}

fn toy_records() -> Vec<KanjiRecord> {
    vec![KanjiRecord {
        character: '丁',
        strokes: vec![],
        kun: vec!["ひのと".into()],
        on: vec!["チョウ".into()],
        chinese: vec!["ding1".into()],
        korean: vec!["jeong".into()],
        vietnamese: vec![],
        meanings_en: vec!["street".into()],
        nanori: Vec::new(),
        rad_names: Vec::new(),
        skip: vec![Skip {
            pattern: 1,
            first: 3,
            second: 4,
        }],
        skip_misgrade: false,
        four_corner: vec![FourCorner {
            top_left: 1,
            top_right: 0,
            bottom_left: 1,
            bottom_right: 0,
            extra: 6,
        }],
        deroo: vec![Deroo(3273)],
        sh_desc: vec![ShDesc {
            radical_strokes: 3,
            radical: 'k',
            other_strokes: 11,
            index: 2,
        }],
        jis208: vec![Kuten {
            plane: 1,
            ku: 29,
            ten: 9,
        }],
        jis212: vec![],
        jis213: vec![],
        deroo_variant: vec![Deroo(3675)],
        nelson_c: vec![],
        njecd: vec![],
        oneill: vec![Oneill {
            num: 2078,
            suffix: Some('A'),
        }],
        s_h: vec![],
        grade: Some(3),
        jlpt: Some(2),
        freq: Some(1054),
        radical: Some(9),
        parts: vec!['一', '亅'],
        jis208_code: Some([35, 90]),
    }]
}

#[test]
fn ucs_matches_variant_format() {
    // Uppercase hex, as kanjidic2 carries it.
    assert_eq!(toy_records()[0].ucs(), "4E01");
}

#[test]
fn kuten_parses() {
    assert_eq!(
        Kuten::parse("1-48-19"),
        Some(Kuten {
            plane: 1,
            ku: 48,
            ten: 19
        })
    );
    assert_eq!(Kuten::parse("1-48"), None);
    assert_eq!(Kuten::parse("1-48-19-2"), None);
    assert_eq!(Kuten::parse("3-48-19"), None);
    assert_eq!(Kuten::parse("1-0-19"), None);
    assert_eq!(Kuten::parse("abc"), None);
    assert_eq!(Kuten::parse("1-48-19").unwrap().to_string(), "1-48-19");
}

#[test]
fn oneill_parses() {
    assert_eq!(
        Oneill::parse("2078A"),
        Some(Oneill {
            num: 2078,
            suffix: Some('A')
        })
    );
    assert_eq!(
        Oneill::parse("2078"),
        Some(Oneill {
            num: 2078,
            suffix: None
        })
    );
    assert_eq!(Oneill::parse("A"), None);
    assert_eq!(Oneill::parse(""), None);
    assert_eq!(Oneill::parse("20a8"), None);
    assert_eq!(Oneill::parse("2078A").unwrap().to_string(), "2078A");
    assert_eq!(Oneill::parse("2078").unwrap().to_string(), "2078");
}

#[test]
fn skip_parses() {
    assert_eq!(
        Skip::parse("1-3-4"),
        Some(Skip {
            pattern: 1,
            first: 3,
            second: 4
        })
    );
    assert_eq!(
        Skip::parse("4-7-1"),
        Some(Skip {
            pattern: 4,
            first: 7,
            second: 1
        })
    );
    assert_eq!(Skip::parse("5-3-4"), None);
    assert_eq!(Skip::parse("1-3"), None);
    assert_eq!(Skip::parse("1-3-4-5"), None);
    assert_eq!(Skip::parse("a-b-c"), None);
    assert_eq!(Skip::parse("4-7-1").unwrap().to_string(), "4-7-1");
}

#[test]
fn four_corner_parses() {
    assert_eq!(
        FourCorner::parse("1010.6"),
        Some(FourCorner {
            top_left: 1,
            top_right: 0,
            bottom_left: 1,
            bottom_right: 0,
            extra: 6,
        })
    );
    assert_eq!(FourCorner::parse("1010"), None);
    assert_eq!(FourCorner::parse("1010.16"), None);
    assert_eq!(FourCorner::parse("101a.6"), None);
    assert_eq!(FourCorner::parse("1010.6").unwrap().to_string(), "1010.6");
}

#[test]
fn sh_desc_parses() {
    assert_eq!(
        ShDesc::parse("3k11.2"),
        Some(ShDesc {
            radical_strokes: 3,
            radical: 'k',
            other_strokes: 11,
            index: 2,
        })
    );
    assert_eq!(
        ShDesc::parse("0a10.1"),
        Some(ShDesc {
            radical_strokes: 0,
            radical: 'a',
            other_strokes: 10,
            index: 1,
        })
    );
    assert_eq!(ShDesc::parse("3k11"), None);
    assert_eq!(ShDesc::parse("3K11.2"), None);
    assert_eq!(ShDesc::parse("311.2"), None);
    assert_eq!(ShDesc::parse("3k11.2").unwrap().to_string(), "3k11.2");
}

#[test]
fn deroo_parses() {
    assert_eq!(Deroo::parse("3273"), Some(Deroo(3273)));
    assert_eq!(Deroo::parse(""), None);
    assert_eq!(Deroo::parse("32a3"), None);
    assert_eq!(Deroo::parse("3273").unwrap().to_string(), "3273");
}

#[test]
fn nelson_parses() {
    assert_eq!(Nelson::parse("5008"), Some(Nelson(5008)));
    assert_eq!(Nelson::parse(""), None);
    assert_eq!(Nelson::parse("50a8"), None);
    assert_eq!(Nelson::parse("5008").unwrap().to_string(), "5008");
}

#[test]
fn njecd_parses() {
    assert_eq!(Njecd::parse("1960"), Some(Njecd(1960)));
    assert_eq!(Njecd::parse(""), None);
    assert_eq!(Njecd::parse("19b0"), None);
    assert_eq!(Njecd::parse("1960").unwrap().to_string(), "1960");
}

#[test]
fn kanji_roundtrip_is_exact() {
    let back = kanji_from_bytes(&kanji_to_bytes(&toy_file())).unwrap();
    assert_eq!(back, toy_file());
}

#[test]
fn kanji_rejects_garbage() {
    assert!(kanji_from_bytes(b"").is_err());
    assert!(kanji_from_bytes(b"KJD1").is_err());
    assert!(kanji_from_bytes(b"not a kanji file..............").is_err());
    // Wrong version.
    let mut v = kanji_to_bytes(&toy_file());
    v[4] = 99;
    assert!(kanji_from_bytes(&v).unwrap_err().contains("version"));
    // Truncation must fail, never panic.
    let full = kanji_to_bytes(&toy_file());
    for len in 0..full.len() {
        assert!(kanji_from_bytes(&full[..len]).is_err(), "len={len}");
    }
    // Trailing garbage must fail.
    let mut long = full.clone();
    long.extend_from_slice(b"junk");
    assert!(kanji_from_bytes(&long).unwrap_err().contains("trailing"));
}

#[test]
fn kanji_view_reads_without_decoding() {
    let bytes = kanji_to_bytes(&toy_file());
    // `to_bytes` returns a plain Vec (not necessarily 16-byte
    // aligned): stage through an aligned buffer, like the mmap
    // region below.
    let mut aligned = rkyv::util::AlignedVec::<16>::new();
    aligned.extend_from_slice(&bytes);
    let view = kanji_view(&aligned).unwrap();
    assert_eq!(view.kanjivg_version, "test-kanjivg");
    assert_eq!(view.kanjidic2_version, "2026-10-10");
    assert_eq!(view.records.len(), 1);
    let r = &view.records[0];
    assert_eq!(r.character, '丁');
    assert_eq!(r.on.len(), 1);
    assert_eq!(r.on[0], "チョウ");
    assert_eq!(r.meanings_en.len(), 1);
    assert_eq!(r.meanings_en[0], "street");
    assert!(!r.skip_misgrade);
    assert_eq!(r.skip.len(), 1);
    assert_eq!(r.skip[0].pattern, 1);
    assert_eq!(r.skip[0].first, 3);
    assert!(r.grade.is_some());
    assert!(r.jlpt.is_some());
}

#[test]
fn kanji_view_rejects_garbage() {
    let bytes = kanji_to_bytes(&toy_file());
    let mut aligned = rkyv::util::AlignedVec::<16>::new();
    aligned.extend_from_slice(&bytes);
    // Truncation fails before archive access.
    assert!(kanji_view(&aligned[..aligned.len() - 1]).is_err());
    // Trailing garbage: realign (unaligned tail) and check the error.
    let mut long = aligned.to_vec();
    long.extend_from_slice(b"junk");
    let mut realigned = rkyv::util::AlignedVec::<16>::new();
    realigned.extend_from_slice(&long);
    assert!(
        kanji_view(&realigned)
            .err()
            .expect("must fail")
            .contains("trailing")
    );
    // Wrong magic.
    let mut bad = aligned.to_vec();
    bad[0] = b'X';
    let mut rebad = rkyv::util::AlignedVec::<16>::new();
    rebad.extend_from_slice(&bad);
    assert!(
        kanji_view(&rebad)
            .err()
            .expect("must fail")
            .contains("magic")
    );
}

#[test]
fn kanji_view_from_mmap() {
    let path = std::env::temp_dir().join("kanji-view-test.bin");
    std::fs::write(&path, kanji_to_bytes(&toy_file())).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    // SAFETY: the file is not mutated while mapped.
    let map = unsafe { memmap2::Mmap::map(&file).unwrap() };
    let view = kanji_view(&map).unwrap();
    assert_eq!(view.records.len(), 1);
    assert_eq!(view.records[0].character, '丁');
    assert_eq!(view.records[0].on[0], "チョウ");
    drop(map);
    std::fs::remove_file(&path).ok();
}

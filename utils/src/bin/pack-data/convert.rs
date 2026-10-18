//! `Entry` to record-struct conversion, field-for-field with no
//! interpretation: entity refs stay raw, restrictions stay attached.
//! Every string interns into the shared blob (`StrBlob`); converters
//! take it and return raw keys alongside (index building needs the
//! strings; records keep pool ids for headwords, blob refs for
//! entry texts).

use super::jlpt::JlptIndex;
use dict::gloss::field::Field;
use dict::gloss::gloss_type::GlossType;
use dict::gloss::inf::Inf;
use dict::gloss::misc::Misc;
use dict::gloss::name_type::NameType;
use dict::gloss::pos::Pos;
use dict::gloss::priority::Priority;
use dict::gloss::{
    EntryRecord, GlossRecord, KanjiElementRecord, LangSourceRecord, ReadingElementRecord,
    SenseDetail, SenseExtras, SenseRecord,
};
use utils::blob::StrBlob;
use utils::parsers::jmdict::Entry;

/// Field-for-field `Entry` to [`EntryRecord`]. Unknown `pos`
/// entities are a hard error (new DTD revision): silent loss is
/// worse than a failed pack. Returns raw keys alongside for indexing.
pub(super) fn convert_word(
    e: &Entry,
    blob: &mut StrBlob,
    jlpt: &JlptIndex,
) -> Result<(EntryRecord, Vec<String>), Box<dyn std::error::Error>> {
    convert_entry(e, blob, jlpt, false)
}

/// Kokugo (monolingual Japanese) entries from `kokugo.jsonl`: same
/// shape as [`convert_word`], but keeps `lang == "ja"` glosses
/// (Japanese definitions) instead of the English-only ones.
pub(super) fn convert_kokugo(
    e: &Entry,
    blob: &mut StrBlob,
    jlpt: &JlptIndex,
) -> Result<(EntryRecord, Vec<String>), Box<dyn std::error::Error>> {
    convert_entry(e, blob, jlpt, true)
}

fn convert_entry(
    e: &Entry,
    blob: &mut StrBlob,
    jlpt: &JlptIndex,
    japanese_glosses: bool,
) -> Result<(EntryRecord, Vec<String>), Box<dyn std::error::Error>> {
    let senses: Vec<SenseRecord> = e
        .senses
        .iter()
        .map(|s| {
            let (pos, unknown_pos) = Pos::from_strs(s.pos.iter().map(String::as_str));
            let (field, unknown_field) = Field::from_strs(s.field.iter().map(String::as_str));
            let (misc, unknown_misc) = Misc::from_strs(s.misc.iter().map(String::as_str));
            let mut unknown = unknown_pos;
            unknown.extend(unknown_field);
            unknown.extend(unknown_misc);
            if !unknown.is_empty() {
                return Err(format!(
                    "ent_seq {}: unknown entities: {}",
                    e.ent_seq,
                    unknown.join(", ")
                )
                .into());
            }
            let sources: Vec<LangSourceRecord> = s
                .sources
                .iter()
                .map(|l| LangSourceRecord {
                    lang: l.lang.as_deref().map(|t| blob.intern_str(t)),
                    text: blob.intern_str(&l.text),
                })
                .collect();
            let extras = SenseExtras::packed(
                s.stagk.iter().map(|t| blob.intern_str(t)).collect(),
                s.stagr.iter().map(|t| blob.intern_str(t)).collect(),
                s.xref.iter().map(|t| blob.intern_str(t)).collect(),
                s.ant.iter().map(|t| blob.intern_str(t)).collect(),
                s.info.iter().map(|t| blob.intern_str(t)).collect(),
                sources,
            );
            let mut glosses = Vec::with_capacity(s.glosses.len());
            for g in s.glosses.iter().filter(|g| {
                // JMdict keeps English only (absent `xml:lang`);
                // kokugo keeps Japanese definitions (`lang == "ja"`).
                // Other languages stay in the source either way.
                // `g_gend` is dropped (see `GlossRecord`).
                g.lang.as_deref() == if japanese_glosses { Some("ja") } else { None }
            }) {
                let typ = match &g.typ {
                    None => GlossType::Unspecified,
                    Some(code) => GlossType::from_entity(code).ok_or_else(|| {
                        format!("ent_seq {}: unknown gloss type: {code}", e.ent_seq)
                    })?,
                };
                glosses.push(GlossRecord {
                    typ,
                    text: blob.intern_str(&g.text),
                });
            }
            Ok(SenseRecord {
                glosses,
                detail: SenseDetail::packed_word(pos, field, misc),
                extras,
            })
        })
        .collect::<Result<Vec<SenseRecord>, Box<dyn std::error::Error>>>()?;
    let keys = e.jap_keys();
    let keb: Vec<String> = e.kanji.iter().map(|k| k.keb.clone()).collect();
    let reb: Vec<String> = e.readings.iter().map(|r| r.reb.clone()).collect();
    let level = jlpt.level(&keb, &reb);
    Ok((
        EntryRecord {
            ent_seq: e.ent_seq,
            jlpt: level,
            kanji: convert_kanji(e.ent_seq, &e.kanji, blob)?,
            readings: convert_readings(e.ent_seq, &e.readings, blob)?,
            senses,
        },
        keys,
    ))
}

/// Per-record frequency for the relevance sort (zkanji points
/// over ke_pri/re_pri, saturated to u16).
pub(super) use dict::gloss::record_freq;
/// `ke_inf`/`re_inf` + `ke_pri`/`re_pri` code conversion, shared
/// by both converters. Unknown codes are a hard error (new DTD
/// revision): silent loss is worse than a failed pack.
fn convert_element_codes(
    ent_seq: u32,
    info: &[String],
    priorities: &[String],
) -> Result<(Vec<Inf>, Vec<Priority>), Box<dyn std::error::Error>> {
    let (info, unknown_info) = Inf::from_strs(info.iter().map(String::as_str));
    let (priorities, unknown_pri) = Priority::from_strs(priorities.iter().map(String::as_str));
    let unknown: Vec<&str> = unknown_info.into_iter().chain(unknown_pri).collect();
    if !unknown.is_empty() {
        return Err(format!(
            "ent_seq {ent_seq}: unknown element codes: {}",
            unknown.join(", ")
        )
        .into());
    }
    Ok((info, priorities))
}

fn convert_kanji(
    ent_seq: u32,
    kanji: &[utils::parsers::jmdict::KanjiElement],
    blob: &mut StrBlob,
) -> Result<Vec<KanjiElementRecord>, Box<dyn std::error::Error>> {
    kanji
        .iter()
        .map(|k| {
            let (info, priorities) = convert_element_codes(ent_seq, &k.info, &k.priorities)?;
            Ok(KanjiElementRecord {
                keb: blob.intern_key(&k.keb),
                info,
                priorities,
            })
        })
        .collect()
}

fn convert_readings(
    ent_seq: u32,
    readings: &[utils::parsers::jmdict::ReadingElement],
    blob: &mut StrBlob,
) -> Result<Vec<ReadingElementRecord>, Box<dyn std::error::Error>> {
    readings
        .iter()
        .map(|r| {
            let (info, priorities) = convert_element_codes(ent_seq, &r.info, &r.priorities)?;
            Ok(ReadingElementRecord {
                reb: blob.intern_key(&r.reb),
                nokanji: r.nokanji,
                restricted_to: r.restricted_to.iter().map(|t| blob.intern_str(t)).collect(),
                info,
                priorities,
            })
        })
        .collect()
}

/// Field-for-field `Entry` → [`EntryRecord`] (names side). Unknown
/// `name_type` entities are a hard error, same as `pos` in
/// [`convert_word`]. Name translations become senses with `Name`
/// detail and `Unspecified` gloss types (`xml:lang` is always absent
/// in the data; the field is dropped — see `GlossRecord`).
pub(super) fn convert_name(
    e: &Entry,
    blob: &mut StrBlob,
    jlpt: &JlptIndex,
) -> Result<(EntryRecord, Vec<String>), Box<dyn std::error::Error>> {
    let mut senses = Vec::with_capacity(e.translations.len());
    for t in e.translations.iter() {
        let (name_types, unknown) = NameType::from_strs(t.name_types.iter().map(String::as_str));
        if !unknown.is_empty() {
            return Err(format!(
                "ent_seq {}: unknown name_type entities: {}",
                e.ent_seq,
                unknown.join(", ")
            )
            .into());
        }
        if t.details.iter().any(|d| d.lang.is_some()) {
            return Err(format!("ent_seq {}: unexpected translation lang", e.ent_seq).into());
        }
        senses.push(SenseRecord {
            glosses: t
                .details
                .iter()
                .map(|d| GlossRecord {
                    typ: GlossType::Unspecified,
                    text: blob.intern_str(&d.text),
                })
                .collect(),
            detail: SenseDetail::packed_name(name_types),
            extras: None,
        });
    }
    let keys = e.jap_keys();
    let keb: Vec<String> = e.kanji.iter().map(|k| k.keb.clone()).collect();
    let reb: Vec<String> = e.readings.iter().map(|r| r.reb.clone()).collect();
    let level = jlpt.level(&keb, &reb);
    Ok((
        EntryRecord {
            ent_seq: e.ent_seq,
            jlpt: level,
            kanji: convert_kanji(e.ent_seq, &e.kanji, blob)?,
            readings: convert_readings(e.ent_seq, &e.readings, blob)?,
            senses,
        },
        keys,
    ))
}

#[cfg(test)]
mod convert_tests {
    use super::*;

    fn index_from(csv: &str) -> JlptIndex {
        let path = std::env::temp_dir().join("jlpt-convert-test.csv");
        std::fs::write(&path, csv).unwrap();
        let index = JlptIndex::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        index
    }

    #[test]
    fn jlpt_matches_pairs_kana_and_keb() {
        let index = index_from("level,keb,reb\n5,食べる,たべる\n4,,あの\n5,日,\n4,日,\n");
        // Exact pair.
        assert_eq!(index.level(&["食べる".into()], &["たべる".into()]), 5);
        // Kana form matches by reading.
        assert_eq!(index.level(&[], &["あの".into()]), 4);
        // Keb-only fallback, easiest level wins on conflict.
        assert_eq!(index.level(&["日".into()], &["にち".into()]), 4);
        // Complete miss.
        assert_eq!(index.level(&["日本語".into()], &["にほんご".into()]), 0);
    }

    #[test]
    fn jlpt_rejects_bad_levels() {
        let path = std::env::temp_dir().join("jlpt-convert-bad.csv");
        std::fs::write(&path, "level,keb,reb\n9,食べる,たべる\n").unwrap();
        assert!(JlptIndex::load(&path).is_err());
        std::fs::remove_file(&path).ok();
        assert!(JlptIndex::load(std::path::Path::new("/no/such/file.csv")).is_err());
    }
}

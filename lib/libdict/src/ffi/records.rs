//! FFI record/error types plus native-to-foreign conversion.
use crate::gloss::record::{EntryRecord, StrRef};
use crate::kanji::codes::{
    ArchivedKuten, Deroo, FourCorner, Kuten, Nelson, Njecd, Oneill, ShDesc, Skip,
};
use crate::kanji::record::ArchivedKanjiRecord;

/// Native twin of an archived kuten (archived `u8`s are already
/// native — see the `jis208_code` precedent above).
fn native_kuten(k: &ArchivedKuten) -> Kuten {
    Kuten {
        plane: k.plane,
        ku: k.ku,
        ten: k.ten,
    }
}

/// One sense: tag indices plus its texts. `pos` holds table
/// indices, not strings — words index the `pos` table, names the
/// `name_type` table (see `is_name` on [`DictEntry`); resolve with
/// [`pos_label`] / [`name_type_label`]). `field` and `misc` index
/// their own tables (words only; resolve with [`field_label`] /
/// [`misc_label`]). JMdict senses carry English glosses; JMnedict
/// translations carry every transliteration regardless of language.
/// Restriction and reference lists ride along raw (words only).
#[derive(Debug, Clone, uniffi::Record)]
pub struct DictSense {
    pub pos: Vec<u8>,
    pub field: Vec<u8>,
    pub misc: Vec<u8>,
    pub glosses: Vec<String>,
    /// Cross-references (`see: X`).
    pub xref: Vec<String>,
    /// Antonyms.
    pub ant: Vec<String>,
    /// Kanji forms this sense is restricted to.
    pub stagk: Vec<String>,
    /// Readings this sense is restricted to.
    pub stagr: Vec<String>,
    /// Free-text notes.
    pub info: Vec<String>,
    /// Source-language attestations.
    pub sources: Vec<LangSource>,
}

/// One source-language attestation (`lsource`). `lang` is absent
/// for the rare unattributed ones.
#[derive(Debug, Clone, uniffi::Record)]
pub struct LangSource {
    pub lang: Option<String>,
    pub text: String,
}

/// One kanji form with its annotations. `info` indexes the
/// `ke_inf` table (resolve with [`inf_label`]); `priorities` are
/// raw `ke_pri` codes (`news1`, `ichi1`, …).
#[derive(Debug, Clone, uniffi::Record)]
pub struct DictKanjiElement {
    pub keb: String,
    pub info: Vec<u8>,
    pub priorities: Vec<String>,
}

/// One reading with its annotations. `restricted_to` lists the
/// kanji forms it belongs to (empty = all); `nokanji` marks
/// readings never written with kanji.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DictReadingElement {
    pub reb: String,
    pub nokanji: bool,
    pub restricted_to: Vec<String>,
    pub info: Vec<u8>,
    pub priorities: Vec<String>,
}

/// One index hit: record index for [`Dictionary::entry_at`].
/// The table model holds these without touching entry contents.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchHit {
    pub idx: u32,
    /// Reason chain for deinflected hits (`食べて → 食べる ·
    /// conjunctive`); empty for direct matches.
    pub chain: Vec<String>,
    /// Dictionary form behind a deinflected hit (the candidate
    /// surface, e.g. `たべる` for query `たべた`); empty for direct
    /// hits, which highlight the query itself. The table highlights
    /// its first occurrence per cell.
    pub matched: String,
}

/// One deinflected candidate: dictionary-form surface plus the
/// reason chain that produced it. No dictionary verification here
/// (see [`Dictionary::search_indices`] for verified hits) — this
/// is the "did you mean" path from §8.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Deinflection {
    pub surface: String,
    pub chain: Vec<String>,
}

/// Search edge mode (Qt search-mode buttons: prefix-only,
/// suffix-only, both toggled = substring, neither = exact).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SearchMode {
    Prefix,
    Suffix,
    Middle,
    Exact,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DictEntry {
    /// First kanji form, else first reading (never empty for packed
    /// records).
    pub heading: String,
    /// First reading (may equal the heading).
    pub reading: String,
    pub senses: Vec<DictSense>,
    /// All kanji forms with annotations.
    pub kanji: Vec<DictKanjiElement>,
    /// All readings with annotations.
    pub readings: Vec<DictReadingElement>,
    /// True for JMnedict (names) hits.
    pub is_name: bool,
    pub ent_seq: u32,
    /// JLPT level 1–5, 0 when unknown.
    pub jlpt: u8,
}

/// One kanji record: display metadata plus the stroke count (the
/// strokes themselves stay in the recognizer bank).
#[derive(Debug, Clone, uniffi::Record)]
pub struct KanjiEntry {
    /// The character, as a single-char string (UniFFI has no `char`).
    pub character: String,
    pub on: Vec<String>,
    pub kun: Vec<String>,
    pub meanings: Vec<String>,
    pub grade: Option<u32>,
    pub jlpt: Option<u32>,
    pub freq: Option<u32>,
    /// KanjiVG stroke count.
    pub strokes: u32,
    /// Classical Kangxi radical number 1–214, absent when the
    /// source carries none.
    pub radical: Option<u32>,
    /// Stroke-order parts from kradfile, in stroke order (single-char
    /// strings — UniFFI has no `char`). Empty without an entry.
    pub parts: Vec<String>,
    /// Own JIS X 0208 codepoint, absent without one.
    pub jis208: Option<JisKuten>,
    /// Name readings (`nanori*`, kana).
    pub nanori: Vec<String>,
    /// Radical names (`rad_name*`, usually the character itself).
    pub rad_names: Vec<String>,
    /// Chinese readings (`pinyin`, marked tone form).
    pub chinese: Vec<String>,
    /// Korean readings (`korean_r` + `korean_h`).
    pub korean: Vec<String>,
    /// Vietnamese readings (`vietnam`).
    pub vietnamese: Vec<String>,
    /// SKIP codes (`pattern-first-second`).
    pub skip: Vec<String>,
    /// Any SKIP code flagged as a known miscount.
    pub skip_misgrade: bool,
    /// Four-corner codes (`TLTRBLBR.extra`).
    pub four_corner: Vec<String>,
    /// De Roo codes.
    pub deroo: Vec<String>,
    /// Spahn/Hadamitzky descriptors (`nxnn.n`).
    pub sh_desc: Vec<String>,
    /// JIS X 0208 variant references (`plane-ku-ten`).
    pub jis208_refs: Vec<String>,
    /// JIS X 0212 variant references (`plane-ku-ten`).
    pub jis212: Vec<String>,
    /// JIS X 0213 variant references (`plane-ku-ten`).
    pub jis213: Vec<String>,
    /// De Roo variant references.
    pub deroo_variant: Vec<String>,
    /// Nelson variant references.
    pub nelson_c: Vec<String>,
    /// NJECD variant references.
    pub njecd: Vec<String>,
    /// O'Neill variant references (digits plus suffix letter).
    pub oneill: Vec<String>,
    /// SH variant references (opaque codes).
    pub s_h: Vec<String>,
}

/// Own JIS X 0208 kuten (`ku`, `ten` as in `cp_value`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct JisKuten {
    pub ku: u8,
    pub ten: u8,
}

/// One KanjiVG stroke polyline in pen space (0..255), in stroke
/// order — the stroke-order diagram source.
#[derive(Debug, Clone, uniffi::Record)]
pub struct KanjiStroke {
    pub points: Vec<KanjiPoint>,
}

/// One pen-space point.
#[derive(Debug, Clone, uniffi::Record)]
pub struct KanjiPoint {
    pub x: f32,
    pub y: f32,
}

/// Failures surfaced across the FFI boundary.
///
/// Payloads are named fields (not tuple variants): the TypeScript
/// generator needs field names to emit valid types.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum DictError {
    /// Artifact bytes fail the header check (magic, version, exact
    /// length). The archive itself is never validated — see module
    /// docs for the trust model.
    #[error("invalid dictionary artifact: {message}")]
    InvalidArtifact { message: String },
}

impl DictEntry {
    /// Decoded entry plus its file pool (headword forms) and file
    /// flag. Single conversion for words and names: `pos` holds
    /// `pos`-table indices for words, `name_type`-table indices for
    /// names (see `is_name`; resolve with [`pos_label`] /
    /// [`name_type_label`]).
    pub fn from_native(
        r: &EntryRecord,
        pool: &[rkyv::Archived<StrRef>],
        str_blob: &[u8],
        is_name: bool,
    ) -> Self {
        let text = |id: u32| {
            let at = &pool[id as usize];
            let (o, l) = (at.offset.to_native() as usize, at.len.to_native() as usize);
            std::str::from_utf8(&str_blob[o..o + l])
                .expect("pool holds whole strings")
                .to_string()
        };
        let word = |at: &StrRef| {
            let (o, l) = (at.offset as usize, at.len as usize);
            std::str::from_utf8(&str_blob[o..o + l])
                .expect("blob holds whole strings")
                .to_string()
        };
        let words = |refs: &[StrRef]| refs.iter().map(|at| word(at)).collect();
        Self {
            heading: r
                .kanji
                .first()
                .map(|k| text(k.keb))
                .or_else(|| r.readings.first().map(|x| text(x.reb)))
                .unwrap_or_default(),
            reading: r.readings.first().map(|x| text(x.reb)).unwrap_or_default(),
            senses: r
                .senses
                .iter()
                .map(|s| DictSense {
                    pos: s.tag_indices(),
                    field: s.field_indices(),
                    misc: s.misc_indices(),
                    glosses: s.glosses.iter().map(|g| word(&g.text)).collect(),
                    xref: words(s.xref()),
                    ant: words(s.ant()),
                    stagk: words(s.stagk()),
                    stagr: words(s.stagr()),
                    info: words(s.info_notes()),
                    sources: s
                        .sources()
                        .iter()
                        .map(|l| LangSource {
                            lang: l.lang.as_ref().map(word),
                            text: word(&l.text),
                        })
                        .collect(),
                })
                .collect(),
            kanji: r
                .kanji
                .iter()
                .map(|k| DictKanjiElement {
                    keb: text(k.keb),
                    info: k.info.iter().map(|i| *i as u8).collect(),
                    priorities: k
                        .priorities
                        .iter()
                        .map(|p| p.entity().to_string())
                        .collect(),
                })
                .collect(),
            readings: r
                .readings
                .iter()
                .map(|x| DictReadingElement {
                    reb: text(x.reb),
                    nokanji: x.nokanji,
                    restricted_to: words(&x.restricted_to),
                    info: x.info.iter().map(|i| *i as u8).collect(),
                    priorities: x
                        .priorities
                        .iter()
                        .map(|p| p.entity().to_string())
                        .collect(),
                })
                .collect(),
            is_name,
            ent_seq: r.ent_seq,
            jlpt: r.jlpt,
        }
    }
}

impl From<&ArchivedKanjiRecord> for KanjiEntry {
    fn from(r: &ArchivedKanjiRecord) -> Self {
        Self {
            character: r.character.to_string(),
            on: r.on.iter().map(|s| s.to_string()).collect(),
            kun: r.kun.iter().map(|s| s.to_string()).collect(),
            meanings: r.meanings_en.iter().map(|s| s.to_string()).collect(),
            grade: r.grade.as_ref().map(|g| g.to_native()),
            jlpt: r.jlpt.as_ref().map(|j| j.to_native()),
            freq: r.freq.as_ref().map(|f| f.to_native()),
            strokes: r.strokes.len() as u32,
            radical: r.radical.as_ref().map(|g| g.to_native()),
            parts: r.parts.iter().map(|c| c.to_string()).collect(),
            jis208: r.jis208_code.as_ref().map(|k| JisKuten {
                ku: k[0],
                ten: k[1],
            }),
            nanori: r.nanori.iter().map(|s| s.to_string()).collect(),
            rad_names: r.rad_names.iter().map(|s| s.to_string()).collect(),
            chinese: r.chinese.iter().map(|s| s.to_string()).collect(),
            korean: r.korean.iter().map(|s| s.to_string()).collect(),
            vietnamese: r.vietnamese.iter().map(|s| s.to_string()).collect(),
            // Lookup codes format through their native twins (one
            // Display source of truth — see `kanji::codes`).
            skip: r
                .skip
                .iter()
                .map(|s| {
                    Skip {
                        pattern: s.pattern,
                        first: s.first,
                        second: s.second,
                    }
                    .to_string()
                })
                .collect(),
            skip_misgrade: r.skip_misgrade,
            four_corner: r
                .four_corner
                .iter()
                .map(|c| {
                    FourCorner {
                        top_left: c.top_left,
                        top_right: c.top_right,
                        bottom_left: c.bottom_left,
                        bottom_right: c.bottom_right,
                        extra: c.extra,
                    }
                    .to_string()
                })
                .collect(),
            deroo: r
                .deroo
                .iter()
                .map(|d| Deroo(d.0.to_native()).to_string())
                .collect(),
            sh_desc: r
                .sh_desc
                .iter()
                .map(|d| {
                    ShDesc {
                        radical_strokes: d.radical_strokes.to_native(),
                        radical: d.radical.to_string().chars().next().unwrap_or('?'),
                        other_strokes: d.other_strokes.to_native(),
                        index: d.index.to_native(),
                    }
                    .to_string()
                })
                .collect(),
            jis208_refs: r
                .jis208
                .iter()
                .map(|k| native_kuten(k).to_string())
                .collect(),
            jis212: r
                .jis212
                .iter()
                .map(|k| native_kuten(k).to_string())
                .collect(),
            jis213: r
                .jis213
                .iter()
                .map(|k| native_kuten(k).to_string())
                .collect(),
            deroo_variant: r
                .deroo_variant
                .iter()
                .map(|d| Deroo(d.0.to_native()).to_string())
                .collect(),
            nelson_c: r
                .nelson_c
                .iter()
                .map(|n| Nelson(n.0.to_native()).to_string())
                .collect(),
            njecd: r
                .njecd
                .iter()
                .map(|n| Njecd(n.0.to_native()).to_string())
                .collect(),
            oneill: r
                .oneill
                .iter()
                .map(|o| {
                    Oneill {
                        num: o.num.to_native(),
                        suffix: o.suffix.as_ref().and_then(|c| c.to_string().chars().next()),
                    }
                    .to_string()
                })
                .collect(),
            s_h: r.s_h.iter().map(|s| s.to_string()).collect(),
        }
    }
}

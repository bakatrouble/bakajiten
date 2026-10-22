//! Sense display shaping: the single implementation behind the
//! Android (`domain/Labels.kt`), web (`labels.ts`), and Qt
//! (`DictionaryModel::displaySenses` + `MeaningBuilder`) copies.
//!
//! Unified policy (divergences resolved here):
//! - entity labels abbreviated (`&vt;` -> `vt`), deduplicated per
//!   group, with description tips; names resolve the name-type table
//!   (Android resolved name tags through the pos table — fixed).
//! - `uk` renders as a `kana` badge (description stays); the `uk`
//!   flag marks kana-only entries.
//! - groups stay separate (pos/field/misc) — row order
//!   (pos/field/misc/gloss on Android+web vs pos/misc/gloss/field
//!   on Qt) stays trackside presentation, not shaping.
//! - a sense is content-bearing when it has glosses OR info
//!   (Android/web rule; Qt dropped info-only senses — that branch
//!   now renders); numbering is 1-based over the original list so
//!   gaps stay visible.
//! - heading/reading preference: first form containing the query,
//!   else the entry default (empty query -> default).
//! - `query_for` strips a numeric ・n tail only (Qt split at the
//!   first ・ — narrowed to match Android/web; unpinned by Qt tests).
//! - `match_range` is the first indexOf (empty query -> `None`);
//!   offsets are UTF-16 code units for Kotlin/JS/Qt spans.
use super::{field::Field, misc::Misc, name_type::NameType, pos::Pos};
use crate::ffi::records::{
    DictEntry, MatchRange, ShapedSense, ShapedSource, ShapedTag, TagKind,
};

/// Common-word priorities, verified identical in all three
/// frontends (Qt `ArticleRuns`, web `labels.ts`, Android `Labels`).
const COMMON_PRIOS: [&str; 4] = ["news1", "news2", "ichi1", "ichi2"];

/// One abbreviated label (`&n;` -> `n`; anything else passes through).
fn abbreviate(label: &str) -> &str {
    match label.strip_prefix('&') {
        Some(rest) => rest.strip_suffix(';').unwrap_or(label),
        None => label,
    }
}

fn collect(
    indices: &[u8],
    kind: TagKind,
    resolve: impl Fn(u8) -> Option<(&'static str, &'static str)>,
) -> Vec<ShapedTag> {
    let mut out = Vec::new();
    for &i in indices {
        let Some((label, tip)) = resolve(i) else {
            continue;
        };
        let badge = abbreviate(label);
        if badge.is_empty() || out.iter().any(|t: &ShapedTag| t.label == badge) {
            continue;
        }
        out.push(ShapedTag {
            label: badge.to_string(),
            tip: tip.to_string(),
            kind,
        });
    }
    out
}

/// Every content sense, numbered (gaps stay visible).
pub fn shaped_senses(entry: &DictEntry) -> Vec<ShapedSense> {
    let mut out = Vec::new();
    for (i, s) in entry.senses.iter().enumerate() {
        if s.glosses.is_empty() && s.info.is_empty() {
            continue;
        }
        let pos = collect(&s.pos, TagKind::Pos, |idx| {
            if entry.is_name {
                NameType::from_index(idx).map(|t| (t.entity(), t.description()))
            } else {
                Pos::from_index(idx).map(|t| (t.entity(), t.description()))
            }
        });
        let field = collect(&s.field, TagKind::Field, |idx| {
            Field::from_index(idx).map(|t| (t.entity(), t.description()))
        });
        let misc_raw = collect(&s.misc, TagKind::Misc, |idx| {
            Misc::from_index(idx).map(|t| (t.entity(), t.description()))
        });
        let uk = misc_raw.iter().any(|t| t.label == "uk");
        let misc = misc_raw
            .into_iter()
            .map(|mut t| {
                if t.label == "uk" {
                    t.label = "kana".to_string();
                }
                t
            })
            .collect();
        out.push(ShapedSense {
            n: (i + 1) as u32,
            pos,
            field,
            misc,
            glosses: s.glosses.clone(),
            xref: s.xref.clone(),
            ant: s.ant.clone(),
            info: s.info.clone(),
            sources: s
                .sources
                .iter()
                .map(|l| ShapedSource {
                    lang: l.lang.clone(),
                    text: l.text.clone(),
                })
                .collect(),
            restricted_kanji: s.stagk.clone(),
            restricted_reading: s.stagr.clone(),
            uk,
        });
    }
    out
}

/// Entry is usually written in kana alone (first content sense's
/// `uk` tag).
pub fn is_kana_entry(entry: &DictEntry) -> bool {
    shaped_senses(entry).first().is_some_and(|s| s.uk)
}

/// Common-word badge: news/ichi priorities on any form.
pub fn is_common_word(entry: &DictEntry) -> bool {
    entry
        .kanji
        .iter()
        .flat_map(|k| k.priorities.iter())
        .chain(entry.readings.iter().flat_map(|r| r.priorities.iter()))
        .any(|p| COMMON_PRIOS.contains(&p.as_str()))
}

/// Heading form preferring the kanji spelling that contains the
/// query (desktop heading rule); otherwise the first form.
pub fn prefer_keb(entry: &DictEntry, query: &str) -> String {
    if !query.is_empty()
        && let Some(k) = entry.kanji.iter().find(|k| k.keb.contains(query))
    {
        return k.keb.clone();
    }
    entry.heading.clone()
}

/// Reading preferring the spelling that contains the query.
pub fn prefer_reb(entry: &DictEntry, query: &str) -> String {
    if !query.is_empty()
        && let Some(r) = entry.readings.iter().find(|r| r.reb.contains(query))
    {
        return r.reb.clone();
    }
    entry.reading.clone()
}

/// Byte offset of the query-match span is not what paint needs:
/// UTF-16 code-unit start+length for Kotlin/JS/Qt spans.
pub fn match_range(text: &str, query: &str) -> Option<MatchRange> {
    if query.is_empty() {
        return None;
    }
    let byte_at = text.find(query)?;
    let start = text[..byte_at].encode_utf16().count() as u32;
    let len = query.encode_utf16().count() as u32;
    Some(MatchRange { start, len })
}

/// Drop a numeric ・n tail from cross-reference queries.
pub fn query_for(term: &str) -> String {
    match term.rfind('・') {
        Some(i) if !term[i + '・'.len_utf8()..].is_empty()
            && term[i + '・'.len_utf8()..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            term[..i].to_string()
        }
        _ => term.to_string(),
    }
}

/// First gloss of the first sense (row preview).
pub fn preview_gloss(entry: &DictEntry) -> String {
    entry
        .senses
        .first()
        .and_then(|s| s.glosses.first())
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod display_tests {
    use super::*;
    use crate::ffi::records::{DictKanjiElement, DictReadingElement, DictSense};

    fn tag_index(table: &str, entity: &str) -> u8 {
        let lookup: fn(u8) -> Option<&'static str> = match table {
            "pos" => |i| Pos::from_index(i).map(|t| t.entity()),
            "misc" => |i| Misc::from_index(i).map(|t| t.entity()),
            "field" => |i| Field::from_index(i).map(|t| t.entity()),
            _ => |i| NameType::from_index(i).map(|t| t.entity()),
        };
        (0..=255)
            .find(|&i| lookup(i) == Some(entity))
            .expect("known tag entity")
    }

    fn sense(
        pos: Vec<u8>,
        field: Vec<u8>,
        misc: Vec<u8>,
        glosses: Vec<&str>,
        info: Vec<&str>,
    ) -> DictSense {
        DictSense {
            pos,
            field,
            misc,
            glosses: glosses.into_iter().map(str::to_string).collect(),
            xref: vec![],
            ant: vec![],
            stagk: vec![],
            stagr: vec![],
            info: info.into_iter().map(str::to_string).collect(),
            sources: vec![],
        }
    }

    fn entry(senses: Vec<DictSense>) -> DictEntry {
        DictEntry {
            heading: "食べる".to_string(),
            reading: "たべる".to_string(),
            senses,
            kanji: vec![DictKanjiElement {
                keb: "食べる".to_string(),
                info: vec![],
                priorities: vec!["ichi1".to_string()],
            }],
            readings: vec![DictReadingElement {
                reb: "たべる".to_string(),
                nokanji: false,
                restricted_to: vec![],
                info: vec![],
                priorities: vec![],
            }],
            is_name: false,
            ent_seq: 1_000_100,
            jlpt: 5,
        }
    }

    #[test]
    fn abbreviate_strips_entities() {
        assert_eq!(abbreviate("&vt;"), "vt");
        assert_eq!(abbreviate("kana"), "kana");
        assert_eq!(abbreviate(""), "");
        assert_eq!(abbreviate("&"), "&");
        assert_eq!(abbreviate("&;"), "");
    }

    #[test]
    fn tags_dedup_and_skip_invalid() {
        let vt = tag_index("pos", "&v1;");
        let tags = collect(&[vt, vt, 255], TagKind::Pos, |i| {
            Pos::from_index(i).map(|t| (t.entity(), t.description()))
        });
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].label, "v1");
        assert_eq!(tags[0].kind, TagKind::Pos);
        assert!(!tags[0].tip.is_empty());
    }

    #[test]
    fn uk_becomes_kana_badge() {
        let uk = Misc::Uk as u8;
        let e = entry(vec![sense(vec![], vec![], vec![uk], vec!["to eat"], vec![])]);
        let out = shaped_senses(&e);
        assert_eq!(out.len(), 1);
        assert!(out[0].uk);
        assert_eq!(out[0].misc.len(), 1);
        assert_eq!(out[0].misc[0].label, "kana");
        assert_eq!(out[0].misc[0].tip, Misc::Uk.description());
        assert!(is_kana_entry(&e));
    }

    #[test]
    fn numbering_skips_empty_but_keeps_info_only() {
        let vt = tag_index("pos", "&v1;");
        let e = entry(vec![
            sense(vec![vt], vec![], vec![], vec!["to eat"], vec![]),
            sense(vec![], vec![], vec![], vec![], vec![]),
            sense(vec![], vec![], vec![], vec![], vec!["only a note"]),
        ]);
        let out = shaped_senses(&e);
        assert_eq!(out.iter().map(|s| s.n).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(out[0].glosses, vec!["to eat"]);
        assert_eq!(out[1].info, vec!["only a note"]);
    }

    #[test]
    fn names_resolve_name_type_table() {
        let surname = tag_index("name", "&surname;");
        let mut e = entry(vec![sense(vec![surname], vec![], vec![], vec!["Tanaka"], vec![])]);
        e.is_name = true;
        let out = shaped_senses(&e);
        assert_eq!(out[0].pos.len(), 1);
        assert_eq!(out[0].pos[0].label, "surname");
    }

    #[test]
    fn prefer_forms_follow_query() {
        let e = entry(vec![sense(vec![], vec![], vec![], vec!["to eat"], vec![])]);
        assert_eq!(prefer_keb(&e, ""), "食べる");
        assert_eq!(prefer_reb(&e, ""), "たべる");
        assert_eq!(prefer_keb(&e, "べる"), "食べる");
    }

    #[test]
    fn match_range_utf16_offsets() {
        assert_eq!(match_range("to eat", ""), None);
        assert_eq!(match_range("to eat", "sleep"), None);
        assert_eq!(
            match_range("to eat", "eat"),
            Some(MatchRange { start: 3, len: 3 })
        );
        // CJK: UTF-16 code units, not bytes.
        assert_eq!(
            match_range("あいう", "いう"),
            Some(MatchRange { start: 1, len: 2 })
        );
        // Astral plane: one char, two code units.
        assert_eq!(
            match_range("a𠀋b", "𠀋b"),
            Some(MatchRange { start: 1, len: 3 })
        );
    }

    #[test]
    fn query_for_strips_numeric_tail_only() {
        assert_eq!(query_for("たべる・1"), "たべる");
        assert_eq!(query_for("クロス・チェック"), "クロス・チェック");
        assert_eq!(query_for("abc"), "abc");
        assert_eq!(query_for("a・"), "a・");
    }

    #[test]
    fn common_word_needs_news_or_ichi() {
        let e = entry(vec![sense(vec![], vec![], vec![], vec!["to eat"], vec![])]);
        assert!(is_common_word(&e));
        let mut plain = e.clone();
        plain.kanji[0].priorities.clear();
        assert!(!is_common_word(&plain));
        plain.kanji[0].priorities = vec!["nf01".to_string()];
        assert!(!is_common_word(&plain));
    }

    #[test]
    fn kana_entry_requires_first_sense_uk() {
        let uk = Misc::Uk as u8;
        let e = entry(vec![
            sense(vec![], vec![], vec![], vec!["to eat"], vec![]),
            sense(vec![], vec![], vec![uk], vec!["to live on"], vec![]),
        ]);
        assert!(!is_kana_entry(&e));
        assert_eq!(preview_gloss(&e), "to eat");
        assert_eq!(preview_gloss(&entry(vec![])), "");
    }
}

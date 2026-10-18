//! zkanji-style relevance ranking (`jpSortFunc`): frequency over
//! ke_pri/re_pri points plus short-reading and few-kanji bonuses
//! with staged thresholds. Operates on decoded entries (one decode
//! per hit, not per comparison); corrupt frames score zero and sort
//! last. JLPT modes need an order setting (follow-up — levels are
//! packed since v10).

use std::cmp::Ordering;

use super::file::ArchivedDictIndex;
use super::view::decode_entry;

/// Shorter readings rank higher (zkanji `jpSortFuncKanaLenInc`).
const KANA_LEN_INC: [i32; 9] = [60, 45, 35, 26, 20, 17, 15, 13, 11];
/// Fewer kanji rank higher (zkanji `jpSortFuncKanjiCntInc`).
const KANJI_CNT_INC: [i32; 9] = [60, 45, 35, 26, 20, 17, 15, 13, 11];

/// CJK Unified + Extension A + Compatibility Ideographs: zkanji's
/// `KANJI()` macro checks a validity table on top; the Han property
/// approximates it (pool strings carry no validity flags).
fn is_kanji(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
        || ('\u{3400}'..='\u{4dbf}').contains(&c)
        || ('\u{f900}'..='\u{faff}').contains(&c)
}

/// Non-kana Japanese (々 ・ ー …): zkanji's `VALIDCODE` minus kana.
/// Kana in headwords never counts here.
fn is_extra(c: char) -> bool {
    matches!(c, '々' | '・' | 'ー' | '〜' | '…')
}

/// One hit scored for relevance sorting. Decoded once per search
/// (not per comparison); corrupt frames score zero and sort last.
/// Comparator inputs come precomputed (`kana_len`, `kanji_cnt`,
/// `kanji_len`: counting chars per comparison would redo O(chars)
/// work ~log n times per hit); tiebreak strings borrow the pool
/// (no per-hit allocation — the sort only reorders).
struct ScoredHit<'a> {
    idx: usize,
    freq: u16,
    kana_only: bool,
    /// `kana.chars().count().min(9).saturating_sub(1)` (0–8).
    kana_len: u8,
    /// `kanji_count(kanji).min(8)` (0–8).
    kanji_cnt: u8,
    /// `kanji.chars().count().min(9)` (0–9).
    kanji_len: u8,
    kana: &'a str,
    kanji: &'a str,
}

/// Relevance order for key-order hits: zkanji `jpSortFunc`
/// (frequency/100, non-kana bonus, short-reading and few-kanji
/// bonuses with staged thresholds, kana/kanji tiebreak, record
/// index last for a total order). Inflection is always absent here,
/// so that step is a no-op; JLPT modes need an order setting
/// (follow-up — the levels are packed since v10).
pub fn sort_hits(view: &ArchivedDictIndex, blob: &[u8], hits: &mut Vec<usize>) {
    let pool = &view.pool;
    let text = |id: u32| {
        let at = &pool[id as usize];
        let (o, l) = (at.offset.to_native() as usize, at.len.to_native() as usize);
        std::str::from_utf8(&view.str_blob[o..o + l]).expect("pool holds whole strings")
    };
    let mut scored: Vec<ScoredHit> = hits
        .iter()
        .map(|&idx| {
            let freq = if idx < view.freqs.len() {
                view.freqs[idx].to_native()
            } else {
                0
            };
            let (kana_only, kana_len, kanji_cnt, kanji_len, kana, kanji) = view
                .entry_frame(blob, idx)
                .and_then(|frame| decode_entry(frame).ok())
                .map(|entry| {
                    let kana = entry
                        .readings
                        .first()
                        .map(|r| text(r.reb))
                        .or_else(|| entry.kanji.first().map(|k| text(k.keb)))
                        .unwrap_or_default();
                    let kanji = entry.kanji.first().map(|k| text(k.keb)).unwrap_or_default();
                    (
                        entry.kanji.is_empty(),
                        kana.chars().count().min(9).saturating_sub(1) as u8,
                        kanji_count(kanji).min(8) as u8,
                        kanji.chars().count().min(9) as u8,
                        kana,
                        kanji,
                    )
                })
                .unwrap_or((true, 0, 0, 0, "", ""));
            ScoredHit {
                idx,
                freq,
                kana_only,
                kana_len,
                kanji_cnt,
                kanji_len,
                kana,
                kanji,
            }
        })
        .collect();
    scored.sort_unstable_by(relevance_cmp);
    hits.clear();
    hits.extend(scored.into_iter().map(|s| s.idx));
}

fn relevance_cmp(a: &ScoredHit, b: &ScoredHit) -> Ordering {
    let mut fa = a.freq as i32 / 100;
    let mut fb = b.freq as i32 / 100;
    if !a.kana_only {
        fa += 20;
    }
    if !b.kana_only {
        fb += 20;
    }
    fa += KANA_LEN_INC[a.kana_len as usize];
    fb += KANA_LEN_INC[b.kana_len as usize];
    if (fa - fb).abs() > 70 {
        return fb.cmp(&fa);
    }
    fa += KANJI_CNT_INC[a.kanji_cnt as usize];
    fb += KANJI_CNT_INC[b.kanji_cnt as usize];
    if (fa - fb).abs() > 10 {
        return fb.cmp(&fa);
    }
    fa += 10 - a.kanji_len as i32;
    fb += 10 - b.kanji_len as i32;
    if fa != fb {
        return fb.cmp(&fa);
    }
    a.kana
        .cmp(&b.kana)
        .then_with(|| a.kanji.cmp(&b.kanji))
        .then_with(|| a.idx.cmp(&b.idx))
}

/// `kanji_chars + extra_chars / 2` for the kanji-count bonus.
fn kanji_count(kanji: &str) -> usize {
    let mut kanji_chars = 0;
    let mut extra = 0;
    for c in kanji.chars() {
        if is_kanji(c) {
            kanji_chars += 1;
        } else if is_extra(c) {
            extra += 1;
        }
    }
    kanji_chars + extra / 2
}

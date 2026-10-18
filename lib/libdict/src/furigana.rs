//! Furigana segmentation: split a headword form against its reading
//! into kana-literal runs and kanji ruby groups (`食べる`/`たべる` →
//! `食`/`た` + literal `べる`; `今日`/`きょう` → one group). Greedy
//! outside-in with backtracking (repeated-kana ambiguity like
//! `窺う`/`うかがう` needs more than prefix/suffix stripping) and
//! katakana↔hiragana folding (`ガリ勉`/`がりべん`). Pure raw
//! strings — display transliteration applies to the pieces at render
//! time, never here.

/// One segment: literal kana run (empty ruby) or ruby group.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FuriganaSpan {
    pub text: String,
    pub ruby: String,
}

/// Segments a headword against its reading, or `None` when the pair
/// does not align (ateji, phrases) — and when the head needs no ruby
/// at all (kana-only): both render as the plain heading.
pub fn segment(head: &str, reading: &str) -> Option<Vec<FuriganaSpan>> {
    if head.is_empty() || reading.is_empty() {
        return None;
    }
    let head: Vec<char> = head.chars().collect();
    let reading: Vec<char> = reading.chars().collect();
    let mut spans = Vec::new();
    let mut dead = std::collections::HashSet::new();
    if !align(&head, 0, &reading, 0, &mut spans, &mut dead) {
        return None;
    }
    if spans.iter().all(|s: &FuriganaSpan| s.ruby.is_empty()) {
        return None;
    }
    Some(spans)
}

fn align(
    head: &[char],
    i: usize,
    reading: &[char],
    j: usize,
    out: &mut Vec<FuriganaSpan>,
    dead: &mut std::collections::HashSet<(usize, usize)>,
) -> bool {
    if i == head.len() && j == reading.len() {
        return true;
    }
    if i >= head.len() || j >= reading.len() || dead.contains(&(i, j)) {
        return false;
    }
    // Literal kana run (folded compare): maximal, deterministic.
    if is_kana(head[i]) && fold(head[i]) == fold(reading[j]) {
        let mut k = 0;
        while i + k < head.len()
            && j + k < reading.len()
            && is_kana(head[i + k])
            && fold(head[i + k]) == fold(reading[j + k])
        {
            k += 1;
        }
        out.push(FuriganaSpan {
            text: head[i..i + k].iter().collect(),
            ruby: String::new(),
        });
        if align(head, i + k, reading, j + k, out, dead) {
            return true;
        }
        out.pop();
        return false;
    }
    // Kanji run takes a reading slice: try every split point (the
    // slice ends where the rest still aligns — backtracking covers
    // repeated-kana ambiguity).
    if !is_kana(head[i]) {
        let mut k = 0;
        while i + k < head.len() && !is_kana(head[i + k]) {
            k += 1;
        }
        for m in 1..=reading.len() - j {
            if reading[j..j + m].iter().any(|c| !is_kana(*c)) {
                break;
            }
            out.push(FuriganaSpan {
                text: head[i..i + k].iter().collect(),
                ruby: reading[j..j + m].iter().collect(),
            });
            if align(head, i + k, reading, j + m, out, dead) {
                return true;
            }
            out.pop();
        }
        dead.insert((i, j));
        return false;
    }
    dead.insert((i, j));
    false
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}' | '\u{ff61}'..='\u{ff9f}')
}

/// Full-width katakana reads as hiragana (ガリ勉/がりべん).
/// Halfwidth katakana compares exact (rare in headwords; no
/// fold table worth its wrongness).
fn fold(c: char) -> char {
    if ('\u{30a1}'..='\u{30f6}').contains(&c) {
        char::from_u32(c as u32 - 0x60).unwrap_or(c)
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(head: &str, reading: &str) -> Option<Vec<String>> {
        segment(head, reading).map(|spans| {
            spans
                .iter()
                .map(|s| format!("{}/{}", s.text, s.ruby))
                .collect()
        })
    }

    #[test]
    fn segment_traces() {
        assert_eq!(
            texts("食べる", "たべる"),
            Some(vec!["食/た".into(), "べる/".into()])
        );
        assert_eq!(
            texts("お母さん", "おかあさん"),
            Some(vec!["お/".into(), "母/かあ".into(), "さん/".into()])
        );
        assert_eq!(texts("今日", "きょう"), Some(vec!["今日/きょう".into()]));
        assert_eq!(
            texts("限り限り", "ぎりぎり"),
            Some(vec![
                "限/ぎ".into(),
                "り/".into(),
                "限/ぎ".into(),
                "り/".into()
            ])
        );
        assert_eq!(
            texts("ガリ勉", "がりべん"),
            Some(vec!["ガリ/".into(), "勉/べん".into()])
        );
        assert_eq!(
            texts("掻っ攫う", "かっさらう"),
            Some(vec![
                "掻/か".into(),
                "っ/".into(),
                "攫/さら".into(),
                "う/".into()
            ])
        );
        assert_eq!(
            texts("窺う", "うかがう"),
            Some(vec!["窺/うかが".into(), "う/".into()])
        );
        // Kana-only: nothing to annotate.
        assert_eq!(texts("たべる", "たべる"), None);
        assert_eq!(texts("テーブル", "テーブル"), None);
        // Mismatched pairs: fallback.
        assert_eq!(texts("たべる", "のむ"), None);
        assert_eq!(texts("食べる", "のむ"), None);
        assert_eq!(texts("", "たべる"), None);
        assert_eq!(texts("食べる", ""), None);
    }
}

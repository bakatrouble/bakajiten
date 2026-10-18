//! Kana → romaji for reading display (the query edge converts the
//! other way). Hepburn uses macrons,
//! Kunrei-shiki circumflexes. Long vowels merge (`aa` → ā, `ou` →
//! ō) except word-final `ou` (verb endings like かう stay split —
//! spelling cannot tell them from nouns like きょう, which renders
//! "kyou"); Hepburn additionally keeps `ei` split (せんせい, not
//! sensē). Both fold katakana first and pass non-kana through
//! untouched. を never merges.

/// Romanization school.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RomajiStyle {
    Hepburn,
    Kunrei,
}

/// Convert a kana string (hiragana or katakana) to romaji.
pub fn to_romaji(kana: &str, style: RomajiStyle) -> String {
    let hepburn = style == RomajiStyle::Hepburn;
    let chars: Vec<char> = kana.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut prev_vowel: Option<u8> = None;
    let mut prev_digraph = false;
    let mut i = 0;
    while i < n {
        let french = i == n - 1;
        let c = fold_kana(chars[i]);
        // ヴ + small vowel: va/vi/vu/ve/vo; bare ヴ → vu.
        if c == 'ヴ' {
            if i + 1 < n {
                let tail = match fold_kana(chars[i + 1]) {
                    'ぁ' => Some(("va", b'a')),
                    'ぃ' => Some(("vi", b'i')),
                    'ぇ' => Some(("ve", b'e')),
                    'ぉ' => Some(("vo", b'o')),
                    _ => None,
                };
                if let Some((text, vowel)) = tail {
                    out.push_str(text);
                    prev_vowel = Some(vowel);
                    prev_digraph = false;
                    i += 2;
                    continue;
                }
            }
            out.push_str("vu");
            prev_vowel = Some(b'u');
            prev_digraph = false;
            i += 1;
            continue;
        }
        // Small っ: double the next consonant (っち → cchi via
        // chi's leading c); trailing っ falls back to tsu/tu.
        if c == 'っ' {
            if i + 1 >= n {
                out.push_str(if hepburn { "tsu" } else { "tu" });
                prev_vowel = Some(b'u');
                prev_digraph = false;
                i += 1;
                continue;
            }
            let d = fold_kana(chars[i + 1]);
            let mut next: Option<char> = None;
            if let Some((text, _)) = mora(d, hepburn) {
                next = text.chars().next();
            } else if let Some((tail, _)) = glide_tail(d) {
                next = tail.chars().next();
            }
            if let Some(lead) = next {
                out.push(lead);
                prev_vowel = None; // geminate breaks vowel runs
                prev_digraph = false;
                i += 1; // re-read the next mora normally
                continue;
            }
            i += 1;
            continue;
        }
        // Consonant + small kana digraph (きゃ, しゅ, くゎ …).
        if i + 1 < n {
            let d = fold_kana(chars[i + 1]);
            if let (Some(stem), Some((tail, vowel))) = (digraph_stem(c, hepburn), glide_tail(d)) {
                out.push_str(stem);
                out.push_str(tail);
                prev_vowel = Some(vowel);
                prev_digraph = true;
                i += 2;
                continue;
            }
        }
        // Long mark: replace the previous vowel with its marked
        // form (ラー → rā, シー → shī).
        if c == 'ー' {
            if let Some(prev) = prev_vowel {
                out.pop();
                out.push(long_mark(prev, hepburn));
            } else {
                out.push('-');
            }
            prev_vowel = None;
            prev_digraph = false;
            i += 1;
            continue;
        }
        // ん: n' before vowels and y, m before b/m/p, else n
        // (trailing ん falls through to the plain table entry).
        if c == 'ん' && i + 1 < n {
            let mut next: Option<char> = None;
            let d = fold_kana(chars[i + 1]);
            if i + 2 < n {
                let e2 = fold_kana(chars[i + 2]);
                if let (Some(stem), Some(_)) = (digraph_stem(d, hepburn), glide_tail(e2)) {
                    next = stem.chars().next();
                }
            }
            if next.is_none() {
                if let Some((text, _)) = mora(d, hepburn) {
                    next = text.chars().next();
                } else if d == 'っ' || d == 'ん' {
                    let key = if d == 'ん' { 'ん' } else { 'つ' };
                    next = mora(key, hepburn).and_then(|(text, _)| text.chars().next());
                }
            }
            match next {
                Some(v) if "aiueoy".contains(v) => out.push_str("n'"),
                Some('b' | 'm' | 'p') => out.push('m'),
                _ => out.push('n'),
            }
            prev_vowel = None;
            i += 1;
            continue;
        }
        // Bare vowel mora: merge long vowels, else emit.
        if is_bare_vowel(c) {
            let cur = match c {
                'あ' => b'a',
                'い' => b'i',
                'う' => b'u',
                'え' => b'e',
                _ => b'o',
            };
            if merges_with(prev_vowel, cur, french, prev_digraph, hepburn) {
                // Lengthened previous vowel (tou → tō, sensê):
                // chop it and re-emit marked.
                out.pop();
                out.push(long_mark(prev_vowel.unwrap_or(cur), hepburn));
                prev_vowel = None;
                prev_digraph = false;
                i += 1;
                continue;
            }
            out.push_str(mora(c, hepburn).map(|(text, _)| text).unwrap_or(""));
            prev_vowel = Some(cur);
            prev_digraph = false;
            i += 1;
            continue;
        }
        // Plain mora (を carries no mergeable vowel, ん none).
        if let Some((text, vowel)) = mora(c, hepburn) {
            out.push_str(text);
            prev_vowel = vowel;
            prev_digraph = false;
            i += 1;
            continue;
        }
        // Non-kana (kanji, punctuation): pass through, break runs.
        out.push(chars[i]);
        prev_vowel = None;
        prev_digraph = false;
        i += 1;
    }
    out
}

/// True when the string holds no kanji (a reading in effect —
/// kana-only headings romanize with the readings).
pub fn is_kana(s: &str) -> bool {
    !s.is_empty() && !s.chars().any(is_han)
}

/// CJK unified + compatibility + radical blocks behind Qt's
/// `\p{Han}` for heading purposes (kana, punctuation, and latin
/// pass through as non-Han).
fn is_han(c: char) -> bool {
    matches!(c,
        '\u{3400}'..='\u{4dbf}'
        | '\u{4e00}'..='\u{9fff}'
        | '\u{2e80}'..='\u{2eff}'
        | '\u{2f00}'..='\u{2fd5}'
        | '\u{f900}'..='\u{faff}'
        | '\u{20000}'..='\u{2ebef}'
        | '\u{30000}'..='\u{3134f}')
}

/// Base syllabary entry: romaji plus its vowel (none vowelless).
fn mora(c: char, hepburn: bool) -> Option<(&'static str, Option<u8>)> {
    // Shared readings first; the six school-split morae branch.
    let plain: Option<(&'static str, Option<u8>)> = match c {
        'あ' => Some(("a", Some(b'a'))),
        'い' => Some(("i", Some(b'i'))),
        'う' => Some(("u", Some(b'u'))),
        'え' => Some(("e", Some(b'e'))),
        'お' => Some(("o", Some(b'o'))),
        'か' => Some(("ka", Some(b'a'))),
        'き' => Some(("ki", Some(b'i'))),
        'く' => Some(("ku", Some(b'u'))),
        'け' => Some(("ke", Some(b'e'))),
        'こ' => Some(("ko", Some(b'o'))),
        'さ' => Some(("sa", Some(b'a'))),
        'す' => Some(("su", Some(b'u'))),
        'せ' => Some(("se", Some(b'e'))),
        'そ' => Some(("so", Some(b'o'))),
        'た' => Some(("ta", Some(b'a'))),
        'て' => Some(("te", Some(b'e'))),
        'と' => Some(("to", Some(b'o'))),
        'な' => Some(("na", Some(b'a'))),
        'に' => Some(("ni", Some(b'i'))),
        'ぬ' => Some(("nu", Some(b'u'))),
        'ね' => Some(("ne", Some(b'e'))),
        'の' => Some(("no", Some(b'o'))),
        'は' => Some(("ha", Some(b'a'))),
        'ひ' => Some(("hi", Some(b'i'))),
        'へ' => Some(("he", Some(b'e'))),
        'ほ' => Some(("ho", Some(b'o'))),
        'ま' => Some(("ma", Some(b'a'))),
        'み' => Some(("mi", Some(b'i'))),
        'む' => Some(("mu", Some(b'u'))),
        'め' => Some(("me", Some(b'e'))),
        'も' => Some(("mo", Some(b'o'))),
        'や' => Some(("ya", Some(b'a'))),
        'ゆ' => Some(("yu", Some(b'u'))),
        'よ' => Some(("yo", Some(b'o'))),
        'ら' => Some(("ra", Some(b'a'))),
        'り' => Some(("ri", Some(b'i'))),
        'る' => Some(("ru", Some(b'u'))),
        'れ' => Some(("re", Some(b'e'))),
        'ろ' => Some(("ro", Some(b'o'))),
        'わ' => Some(("wa", Some(b'a'))),
        'ゐ' => Some(("wi", Some(b'i'))),
        'ゑ' => Some(("we", Some(b'e'))),
        'を' => Some(("wo", None)),
        'ん' => Some(("n", None)),
        'が' => Some(("ga", Some(b'a'))),
        'ぎ' => Some(("gi", Some(b'i'))),
        'ぐ' => Some(("gu", Some(b'u'))),
        'げ' => Some(("ge", Some(b'e'))),
        'ご' => Some(("go", Some(b'o'))),
        'ざ' => Some(("za", Some(b'a'))),
        'ず' => Some(("zu", Some(b'u'))),
        'ぜ' => Some(("ze", Some(b'e'))),
        'ぞ' => Some(("zo", Some(b'o'))),
        'だ' => Some(("da", Some(b'a'))),
        'で' => Some(("de", Some(b'e'))),
        'ど' => Some(("do", Some(b'o'))),
        'ば' => Some(("ba", Some(b'a'))),
        'び' => Some(("bi", Some(b'i'))),
        'ぶ' => Some(("bu", Some(b'u'))),
        'べ' => Some(("be", Some(b'e'))),
        'ぼ' => Some(("bo", Some(b'o'))),
        'ぱ' => Some(("pa", Some(b'a'))),
        'ぴ' => Some(("pi", Some(b'i'))),
        'ぷ' => Some(("pu", Some(b'u'))),
        'ぺ' => Some(("pe", Some(b'e'))),
        'ぽ' => Some(("po", Some(b'o'))),
        'ぁ' => Some(("a", Some(b'a'))),
        'ぃ' => Some(("i", Some(b'i'))),
        'ぅ' => Some(("u", Some(b'u'))),
        'ぇ' => Some(("e", Some(b'e'))),
        'ぉ' => Some(("o", Some(b'o'))),
        'ゎ' => Some(("wa", Some(b'a'))),
        _ => None,
    };
    if plain.is_some() {
        return plain;
    }
    // School splits (shi/si, chi/ti, tsu/tu, fu/hu, ji/zi).
    Some(match c {
        'し' => (if hepburn { "shi" } else { "si" }, Some(b'i')),
        'ち' => (if hepburn { "chi" } else { "ti" }, Some(b'i')),
        'つ' => (if hepburn { "tsu" } else { "tu" }, Some(b'u')),
        'ふ' => (if hepburn { "fu" } else { "hu" }, Some(b'u')),
        'じ' | 'ぢ' => (if hepburn { "ji" } else { "zi" }, Some(b'i')),
        'づ' => ("zu", Some(b'u')),
        _ => return None,
    })
}

/// Consonant stem for digraphs (き + small ゃ → きゃ).
fn digraph_stem(c: char, hepburn: bool) -> Option<&'static str> {
    Some(match c {
        'き' => "k",
        'し' => {
            if hepburn {
                "sh"
            } else {
                "s"
            }
        }
        'ち' => {
            if hepburn {
                "ch"
            } else {
                "t"
            }
        }
        'に' => "n",
        'ひ' => "h",
        'み' => "m",
        'り' => "r",
        'ぎ' => "g",
        'じ' | 'ぢ' => {
            if hepburn {
                "j"
            } else {
                "z"
            }
        }
        'び' => "b",
        'ぴ' => "p",
        'く' => "k",
        'う' => "w",
        _ => return None,
    })
}

/// Small-kana glide: romaji tail plus its vowel.
fn glide_tail(c: char) -> Option<(&'static str, u8)> {
    Some(match c {
        'ゃ' => ("ya", b'a'),
        'ゅ' => ("yu", b'u'),
        'ょ' => ("yo", b'o'),
        'ぁ' => ("a", b'a'),
        'ぃ' => ("i", b'i'),
        'ぅ' => ("u", b'u'),
        'ぇ' => ("e", b'e'),
        'ぉ' => ("o", b'o'),
        'ゎ' => ("wa", b'a'),
        _ => return None,
    })
}

/// Long mark for a vowel: macrons (Hepburn) or circumflexes.
fn long_mark(vowel: u8, hepburn: bool) -> char {
    match (vowel, hepburn) {
        (b'a', true) => 'ā',
        (b'a', false) => 'â',
        (b'i', true) => 'ī',
        (b'i', false) => 'î',
        (b'u', true) => 'ū',
        (b'u', false) => 'û',
        (b'e', true) => 'ē',
        (b'e', false) => 'ê',
        (_, true) => 'ō',
        (_, false) => 'ô',
    }
}

/// Whether a bare vowel mora merges with the previous vowel.
/// Identical pairs always merge; `ou` merges unless word-final
/// after a plain mora (verb endings like かう stay split while
/// digraphs merge: きょう → kyō). Hepburn additionally keeps `ei`
/// split (せんせい, not sensē).
fn merges_with(
    prev: Option<u8>,
    cur: u8,
    is_final: bool,
    prev_digraph: bool,
    hepburn: bool,
) -> bool {
    let Some(p) = prev else {
        return false;
    };
    if p == cur {
        return true;
    }
    if p == b'o' && cur == b'u' {
        return !is_final || prev_digraph;
    }
    !hepburn && p == b'e' && cur == b'i'
}

/// Fold katakana (plus small ヵヶ) into hiragana.
fn fold_kana(c: char) -> char {
    match c {
        '\u{30f5}' => 'か',
        '\u{30f6}' => 'け',
        '\u{30a1}'..='\u{30f4}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

fn is_bare_vowel(c: char) -> bool {
    matches!(c, 'あ' | 'い' | 'う' | 'え' | 'お')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hepburn(s: &str) -> String {
        to_romaji(s, RomajiStyle::Hepburn)
    }

    fn kunrei(s: &str) -> String {
        to_romaji(s, RomajiStyle::Kunrei)
    }

    #[test]
    fn converts_both_schools() {
        // Hepburn core (macrons, shi/chi/tsu/fu/ji).
        assert_eq!(hepburn("たべる"), "taberu");
        assert_eq!(hepburn("すし"), "sushi");
        assert_eq!(hepburn("ちず"), "chizu");
        assert_eq!(hepburn("ふじ"), "fuji");
        assert_eq!(hepburn("せんせい"), "sensei");
        assert_eq!(hepburn("とうきょう"), "tōkyō");
        assert_eq!(hepburn("おおさか"), "ōsaka");
        assert_eq!(hepburn("がくせい"), "gakusei");
        // Kunrei core (si/ti/tu/hu/zi, circumflexes, ei merged).
        assert_eq!(kunrei("すし"), "susi");
        assert_eq!(kunrei("ちず"), "tizu");
        assert_eq!(kunrei("ふじ"), "huzi");
        assert_eq!(kunrei("せんせい"), "sensê");
        assert_eq!(kunrei("とうきょう"), "tôkyô");
        // Geminates, ん rules, digraphs, katakana, long mark.
        assert_eq!(hepburn("きって"), "kitte");
        assert_eq!(hepburn("さっか"), "sakka");
        assert_eq!(hepburn("ほん"), "hon");
        assert_eq!(hepburn("さんぽ"), "sampo");
        assert_eq!(hepburn("ほんやく"), "hon'yaku");
        assert_eq!(hepburn("きょう"), "kyō");
        assert_eq!(hepburn("ラーメン"), "rāmen");
        assert_eq!(hepburn("テラスハウス"), "terasuhausu");
        // Verb endings stay split; noun よう is the exception.
        assert_eq!(hepburn("よう"), "you");
        assert_eq!(hepburn("かう"), "kau");
        assert_eq!(hepburn("おもう"), "omou");
    }

    #[test]
    fn kana_detection() {
        assert!(is_kana("たべる"));
        assert!(is_kana("ア"));
        assert!(!is_kana("食べる"));
        assert!(!is_kana(""));
        assert!(!is_kana("日本語"));
    }
}

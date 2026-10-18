//! Numbered pinyin (`ding1`) → tone-marked (`dīng`), for the
//! kanjidic2 Chinese readings. Total and infallible: a missing tone
//! digit (or neutral `5`) yields unmarked text, unknown shapes pass
//! through untouched — never an error path.

/// One numbered syllable to proper pinyin. Lowercases ASCII at
/// entry (data is lowercase; harmless otherwise); `u:` normalizes
/// to `ü` with or without a tone digit.
pub fn to_marked(syllable: &str) -> String {
    let lower = syllable.to_ascii_lowercase();
    let (body, tone) = match lower.as_bytes().last() {
        Some(b'1'..=b'4') => (
            &lower[..lower.len() - 1],
            lower.as_bytes()[lower.len() - 1] - b'0',
        ),
        // Neutral tone: strip the digit, no mark.
        Some(b'5') => (&lower[..lower.len() - 1], 0),
        _ => (lower.as_str(), 0),
    };
    let body = body.replace("u:", "ü");
    if tone == 0 {
        return body;
    }
    let mut chars: Vec<char> = body.chars().collect();
    let Some(at) = nucleus(&chars) else {
        return body;
    };
    chars[at] = marked(chars[at], tone);
    chars.into_iter().collect()
}

/// Nucleus position: `a`, else `o`, else `e`, else the last vowel
/// of `iu`/`ui`, else lone `i`/`u`/`ü` (standard placement).
fn nucleus(chars: &[char]) -> Option<usize> {
    if let Some(at) = chars.iter().position(|&c| c == 'a') {
        return Some(at);
    }
    if let Some(at) = chars.iter().position(|&c| c == 'o') {
        return Some(at);
    }
    if let Some(at) = chars.iter().position(|&c| c == 'e') {
        return Some(at);
    }
    for (i, pair) in chars.windows(2).enumerate() {
        // `iu` marks the `u` (`liù`), `ui` marks the `i` (`guì`).
        if (pair[0] == 'i' && pair[1] == 'u') || (pair[0] == 'u' && pair[1] == 'i') {
            return Some(i + 1);
        }
    }
    chars.iter().position(|&c| c == 'i' || c == 'u' || c == 'ü')
}

/// `tone` is the `1..=4` digit value.
fn marked(base: char, tone: u8) -> char {
    const MARKS: &[(char, [char; 4])] = &[
        ('a', ['ā', 'á', 'ǎ', 'à']),
        ('e', ['ē', 'é', 'ě', 'è']),
        ('i', ['ī', 'í', 'ǐ', 'ì']),
        ('o', ['ō', 'ó', 'ǒ', 'ò']),
        ('u', ['ū', 'ú', 'ǔ', 'ù']),
        ('ü', ['ǖ', 'ǘ', 'ǚ', 'ǜ']),
    ];
    MARKS
        .iter()
        .find(|(plain, _)| *plain == base)
        .map(|(_, marks)| marks[(tone - 1) as usize])
        .unwrap_or(base)
}

#[cfg(test)]
mod tests;

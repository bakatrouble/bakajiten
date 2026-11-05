//! Greedy batch scan plus the incremental IME display. Total and
//! infallible: unknown input falls through untouched, so there is no
//! `Result` and no error path.

use super::table::{GEMINABLE, TABLE};

/// Whole-word exceptions (§1.0): Hepburn transcription diverges from
/// spelling in exactly these common words.
const EXCEPTIONS: &[(&str, &str)] = &[("konnichiwa", "こんにちは"), ("konbanwa", "こんばんは")];

fn lookup(token: &[u8]) -> Option<&'static str> {
    TABLE
        .binary_search_by(|(key, _)| key.as_bytes().cmp(token))
        .ok()
        .map(|i| TABLE[i].1)
}

/// Live IME tail (§2 of the IME doc): a proper prefix of some table
/// token — derivable mechanically, no special cases.
fn is_live_prefix(suffix: &[u8]) -> bool {
    TABLE
        .iter()
        .any(|(key, _)| key.len() > suffix.len() && key.as_bytes().starts_with(suffix))
}

/// Greedy scan of ASCII-lowercased bytes; `lookahead` feeds the final
/// n/geminate decision (display head), `None` at true end-of-input.
/// Byte positions always sit on char boundaries (tokens are ASCII,
/// emissions advance whole chars), so the non-ASCII passthrough
/// below never panics.
fn convert(bytes: &[u8], text: &str, lookahead: Option<u8>, out: &mut String) {
    let at = |i: usize| -> Option<u8> {
        bytes
            .get(i)
            .copied()
            .or(if i == bytes.len() { lookahead } else { None })
    };
    let mut p = 0;
    while p < bytes.len() {
        let byte = bytes[p];
        if byte.is_ascii() {
            let mut matched = false;
            for len in (1..=4).rev() {
                if p + len <= bytes.len() && bytes[p..p + len].is_ascii() {
                    if let Some(kana) = lookup(&bytes[p..p + len]) {
                        out.push_str(kana);
                        p += len;
                        matched = true;
                        break;
                    }
                }
            }
            if matched {
                continue;
            }
            match byte {
                // Moraic n commits always; the apostrophe escape
                // eats two (`kin'en` → きんえん). A doubled n
                // overlaps: only the terminator is consumed when
                // more follows (`nna` → んな), both at true
                // end-of-input (`nn` → ん).
                b'n' => {
                    out.push('ん');
                    p += if at(p + 1) == Some(b'\'') {
                        2
                    } else if at(p + 1) == Some(b'n') && at(p + 2).is_none() {
                        2
                    } else {
                        1
                    };
                }
                // `mm` mirrors `nn` (`emma` → えんま), same
                // end-of-input rule (`mm` → ん).
                b'm' if at(p + 1) == Some(b'm') => {
                    out.push('ん');
                    p += if at(p + 2).is_none() { 2 } else { 1 };
                }
                // Geminate pair (`gakkou` → がっこう).
                _ if at(p + 1) == Some(byte) && GEMINABLE.contains(&byte) => {
                    out.push('っ');
                    p += 1;
                }
                // Unknown char (lone `l`, `x`, …): as-is. Lookup
                // simply misses — garbage in, no result out.
                _ => {
                    out.push(byte as char);
                    p += 1;
                }
            }
        } else {
            // Non-ASCII passes through (mixed queries convert
            // character-class by character-class).
            let ch = text[p..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            p += ch.len_utf8();
        }
    }
}

/// Batch conversion: whole-word exceptions, then the greedy scan.
/// Lowercases ASCII at entry; kana/kanji pass through untouched.
pub fn to_kana(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if let Some((_, kana)) = EXCEPTIONS.iter().find(|(word, _)| *word == lower.as_str()) {
        return kana.to_string();
    }
    let mut out = String::with_capacity(lower.len());
    convert(lower.as_bytes(), &lower, None, &mut out);
    out
}

/// Live head/tail split: longest live tail stays raw (`shin` →
/// しn), converted with one char of lookahead. A trailing n/m
/// that completes an `nn`/`mm` pair commits instead (`nn` → ん):
/// the pair's second half is no longer held out as raw tail.
fn live_split(bytes: &[u8]) -> usize {
    let mut split = bytes.len();
    for i in 0..bytes.len() {
        if is_live_prefix(&bytes[i..]) {
            split = i;
            break;
        }
    }
    if split < bytes.len() && split > 0 {
        let tail = bytes[split];
        if (tail == b'n' && bytes[split - 1] == b'n')
            || (tail == b'm' && bytes[split - 1] == b'm')
        {
            split += 1;
        }
    }
    split
}

/// True when the buffer ends mid-mora (same split as
/// [`display`]: a raw tail shows, so the trackside input owns
/// composition gating behind this — arrows, space, return and
/// friends wait for the mora to settle or backspace; conversion
/// itself never consults it).
pub fn has_pending_tail(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    live_split(lower.as_bytes()) < lower.len()
}

/// IME display: longest live tail stays raw (`shin` → しn), the head
/// converts with one char of lookahead (`katt` → かっt). A
/// completed `nn`/`mm` pair commits whole (`nn` → ん). Empty tail
/// converts whole, exceptions included.
pub fn display(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let split = live_split(bytes);
    if split == bytes.len() {
        return to_kana(raw);
    }
    let mut out = String::new();
    convert(
        &bytes[..split],
        &lower[..split],
        bytes.get(split).copied(),
        &mut out,
    );
    out.push_str(&lower[split..]);
    out
}

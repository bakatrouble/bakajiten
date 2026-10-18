//! Raw streaming parser for EDRDG `radkfile` (multi-radical lookup data).
//!
//! `radkfile` inverts `kradfile`: each section is headed by a `$`
//! line identifying one kanji element (radical), followed by its
//! stroke count and optionally either the JIS X 0212 code of the
//! kanji whose glyph better depicts the element or an image filename
//! (used by the WWWJDIC server). Every other line carrying kanji
//! belongs to the previously identified element:
//!
//! ```text
//! $ 一 1
//! 丁七三上下…
//! ```
//!
//! Each body character is one kanji containing the radical. The
//! optional 4th header field is kept literally.
//!
//! The file is EUC-JP; decoding happens up front (56 KB — no reason
//! to stream bytes). Raw means raw: no normalization, no validation
//! beyond structure. Entry points: [`parse_file`] (callback per
//! radical), [`parse_bytes`] (EUC-JP input, e.g. tests).

use std::path::Path;

use super::error::{Error, Result};

/// One radical section: the radical, its stroke count, and every
/// kanji listed under it, in file order.
#[derive(Debug, Clone)]
pub struct Radical {
    /// The radical itself (single char).
    pub radical: char,
    /// Stroke count from the `$` header.
    pub strokes: u32,
    /// Optional 4th header field: JIS X 0212 code or image filename,
    /// kept raw.
    pub code: Option<String>,
    /// Kanji containing this radical, in file order.
    pub kanji: Vec<char>,
}

/// Stream-parse a file, calling `emit` once per radical section.
/// Constant memory regardless of file size.
pub fn parse_file(path: &Path, emit: impl FnMut(Radical)) -> Result<()> {
    let bytes = std::fs::read(path)?;
    parse_bytes(&bytes, emit)
}

/// Decode EUC-JP bytes and stream-parse them, calling `emit` once per
/// radical section. undecodable bytes fail (no lossy fallback —
/// silent mojibake would poison the converter).
pub fn parse_bytes(bytes: &[u8], mut emit: impl FnMut(Radical)) -> Result<()> {
    let (text, _, had_errors) = encoding_rs::EUC_JP.decode(bytes);
    if had_errors {
        return Err(Error::Bad("radkfile is not valid EUC-JP".into()));
    }
    parse_inner(&text, &mut emit)
}

fn parse_inner(text: &str, emit: &mut dyn FnMut(Radical)) -> Result<()> {
    let mut current: Option<Radical> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        // Blank lines (none in the wild, tolerated) and comments.
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(head) = line.strip_prefix('$') {
            if let Some(prev) = current.take() {
                emit(prev);
            }
            current = Some(parse_header(head, n + 1)?);
        } else {
            let Some(cur) = current.as_mut() else {
                return Err(Error::Bad(format!(
                    "line {}: kanji outside any radical",
                    n + 1
                )));
            };
            // One token per line in the wild; split defensively so
            // multi-run lines still parse, and every char is one kanji.
            for run in line.split_whitespace() {
                cur.kanji.extend(run.chars());
            }
        }
    }
    if let Some(prev) = current.take() {
        emit(prev);
    }
    Ok(())
}

/// One `$` header: `<radical> <strokes> [code?]`.
fn parse_header(head: &str, lineno: usize) -> Result<Radical> {
    let mut parts = head.split_whitespace();
    let err = |m: &str| Error::Bad(format!("line {lineno}: {m}"));
    let rad = parts.next().ok_or_else(|| err("empty radical header"))?;
    if rad.chars().count() != 1 {
        return Err(err(&format!("radical is not one char: {rad:?}")));
    }
    let strokes: u32 = parts
        .next()
        .ok_or_else(|| err("radical header lacks stroke count"))?
        .parse()
        .map_err(|_| err("radical stroke count is not a number"))?;
    let code = parts.next().map(str::to_string);
    if parts.next().is_some() {
        return Err(err("radical header has extra fields"));
    }
    Ok(Radical {
        radical: rad.chars().next().expect("checked single char"),
        strokes,
        code,
        kanji: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EUC-JP bytes of the UTF-8 sample (mirrors real input encoding).
    fn euc(s: &str) -> Vec<u8> {
        let (bytes, _, _) = encoding_rs::EUC_JP.encode(s);
        bytes.into_owned()
    }

    const SAMPLE: &str = "# comment line
$ 一 1
丁七三
$ 化 2 js01

$ 刈 4 3331
foobar
";

    #[test]
    fn sections_comments_blanks_and_codes() {
        let mut out = Vec::new();
        parse_bytes(&euc(SAMPLE), &mut |r| out.push(r)).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].radical, '一');
        assert_eq!(out[0].strokes, 1);
        assert_eq!(out[0].code, None);
        assert_eq!(out[0].kanji, vec!['丁', '七', '三']);
        assert_eq!(out[1].radical, '化');
        assert_eq!(out[1].code.as_deref(), Some("js01"));
        assert!(out[1].kanji.is_empty());
        assert_eq!(out[2].radical, '刈');
        // ASCII body runs parse as chars like anything else.
        assert_eq!(out[2].kanji, vec!['f', 'o', 'o', 'b', 'a', 'r']);
    }

    #[test]
    fn rejects_garbage() {
        // Invalid EUC-JP (lone 0xFF): no lossy fallback.
        assert!(parse_bytes(b"\xff\xff", &mut |_| {}).is_err());
        // Kanji before any header.
        assert!(parse_bytes(&euc("丁丂\n"), &mut |_| {}).is_err());
        // Non-numeric stroke count.
        assert!(parse_bytes(&euc("$ 一 x\n"), &mut |_| {}).is_err());
        // Empty file is fine (zero sections).
        let mut n = 0;
        parse_bytes(&euc("# only comments\n"), &mut |_| n += 1).unwrap();
        assert_eq!(n, 0);
    }
}

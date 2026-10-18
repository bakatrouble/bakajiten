//! Raw streaming parser for EDRDG `kradfile` (kanji → parts data).
//!
//! `kradfile` decomposes each kanji into its stroke-order parts
//! (radicals and other elements), one kanji per line:
//!
//! ```text
//! 亜 : ｜ 一 口
//! ```
//!
//! The kanji, then a ` : ` separator, then its parts in
//! stroke order, whitespace-separated. Each body token is one
//! character; multi-char tokens fail (no silent merging —
//! decomposition boundaries are the whole point).
//!
//! The file is EUC-JP; decoding happens up front (113 KB — no reason
//! to stream bytes). Entry points: [`parse_file`] (callback per
//! kanji), [`parse_bytes`] (EUC-JP input, e.g. tests).

use std::path::Path;

use super::error::{Error, Result};

/// One kanji decomposed into its parts, in stroke order.
#[derive(Debug, Clone)]
pub struct Decomposition {
    /// The kanji itself (single char).
    pub kanji: char,
    /// Its parts, in stroke order (single chars).
    pub parts: Vec<char>,
}

/// Stream-parse a file, calling `emit` once per kanji.
/// Constant memory regardless of file size.
pub fn parse_file(path: &Path, emit: impl FnMut(Decomposition)) -> Result<()> {
    let bytes = std::fs::read(path)?;
    parse_bytes(&bytes, emit)
}

/// Decode EUC-JP bytes and stream-parse them, calling `emit` once per
/// kanji. undecodable bytes fail (no lossy fallback — silent
/// mojibake would poison the decomposition).
pub fn parse_bytes(bytes: &[u8], mut emit: impl FnMut(Decomposition)) -> Result<()> {
    let (text, _, had_errors) = encoding_rs::EUC_JP.decode(bytes);
    if had_errors {
        return Err(Error::Bad("kradfile is not valid EUC-JP".into()));
    }
    parse_inner(&text, &mut emit)
}

fn parse_inner(text: &str, emit: &mut dyn FnMut(Decomposition)) -> Result<()> {
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        // Blank lines (none in the wild, tolerated) and comments.
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        emit(parse_line(line, n + 1)?);
    }
    Ok(())
}

/// One `kanji : part…` line.
fn parse_line(line: &str, lineno: usize) -> Result<Decomposition> {
    let err = |m: &str| Error::Bad(format!("line {lineno}: {m}"));
    let (head, tail) = line
        .split_once(':')
        .ok_or_else(|| err("missing ':' separator"))?;
    let head = head.trim();
    if head.chars().count() != 1 {
        return Err(err(&format!("kanji is not one char: {head:?}")));
    }
    let mut parts = Vec::new();
    for token in tail.split_whitespace() {
        if token.chars().count() != 1 {
            return Err(err(&format!("part is not one char: {token:?}")));
        }
        parts.push(token.chars().next().expect("checked single char"));
    }
    if parts.is_empty() {
        return Err(err("kanji has no parts"));
    }
    Ok(Decomposition {
        kanji: head.chars().next().expect("checked single char"),
        parts,
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
亜 : ｜ 一 口

五 : 一 口
";

    #[test]
    fn lines_comments_and_blanks() {
        let mut out = Vec::new();
        parse_bytes(&euc(SAMPLE), &mut |d| out.push(d)).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kanji, '亜');
        assert_eq!(out[0].parts, vec!['｜', '一', '口']);
        assert_eq!(out[1].kanji, '五');
        assert_eq!(out[1].parts, vec!['一', '口']);
    }

    #[test]
    fn rejects_garbage() {
        // Invalid EUC-JP (lone 0xFF): no lossy fallback.
        assert!(parse_bytes(b"\xff\xff", &mut |_| {}).is_err());
        // Missing separator.
        assert!(parse_bytes(&euc("亜 ｜ 一\n"), &mut |_| {}).is_err());
        // Multi-char kanji head.
        assert!(parse_bytes(&euc("亜亜 : 一\n"), &mut |_| {}).is_err());
        // Multi-char part.
        assert!(parse_bytes(&euc("亜 : 一口\n"), &mut |_| {}).is_err());
        // Empty parts.
        assert!(parse_bytes(&euc("亜 :\n"), &mut |_| {}).is_err());
        // Empty file is fine (zero entries).
        let mut n = 0;
        parse_bytes(&euc("# only comments\n"), &mut |_| n += 1).unwrap();
        assert_eq!(n, 0);
    }
}

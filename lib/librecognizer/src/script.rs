//! First-character script classifier for candidate display:
//! hiragana, katakana (full- and half-width), or other. Pure range
//! checks (mirrors the Qt delegate's `scriptOf`, traces below);
//! tones and colors resolve trackside from the result.

/// First-character script (see the Qt `CandidateListDelegate::Script`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Script {
    Hiragana,
    Katakana,
    Other,
}

/// Script of the first character (empty reads as other).
pub fn script_of(text: &str) -> Script {
    match text.chars().next() {
        None => Script::Other,
        Some(c) => match c {
            '\u{3040}'..='\u{309f}' => Script::Hiragana,
            '\u{30a0}'..='\u{30ff}' | '\u{ff66}'..='\u{ff9f}' => Script::Katakana,
            _ => Script::Other,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_traces() {
        assert_eq!(script_of(""), Script::Other);
        assert_eq!(script_of("あ"), Script::Hiragana);
        assert_eq!(script_of("あいう"), Script::Hiragana);
        assert_eq!(script_of("ア"), Script::Katakana);
        assert_eq!(script_of("ｱ"), Script::Katakana);
        assert_eq!(script_of("食"), Script::Other);
        assert_eq!(script_of("a"), Script::Other);
        // First character decides, whatever follows.
        assert_eq!(script_of("あア"), Script::Hiragana);
        assert_eq!(script_of("アあ"), Script::Katakana);
    }
}

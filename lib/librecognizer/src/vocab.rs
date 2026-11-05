//! Output vocabulary: TFLite class id <-> Unicode character.
//!
//! Ported from tegaki (`google-recog/src/vocab.rs`): class 6759 is the
//! CTC **blank** (blank-last; treating it as a character emits a
//! constant 𠮟 on every input); class `c` (0..6758) emits FST label
//! `c + 2` = `codepoints[c]` (labels 0=`<epsilon>`, 1=`<reserved>`,
//! 2..6760 = codepoints, 6761=`<S>`, 6762=`</S>`).
//!
//! Unlike tegaki, the codepoints are not bundled via `include_str!`:
//! they arrive in the converted bundle's header metadata (see [`crate::convert`]),
//! so this is a plain owned struct parsed at load time.

/// CTC blank class id (last). Doubles as the "no output" symbol.
pub const BLANK: usize = 6759;
/// Number of TFLite output classes (6759 codepoints + blank).
pub const NUM_CLASSES: usize = 6760;

/// FST label id for a TFLite class (`None` for blank).
pub fn label_of_class(class: usize) -> Option<usize> {
    if class == BLANK {
        None
    } else {
        Some(class + 2)
    }
}

/// Owned codepoint table from the converted bundle's header
/// metadata (see [`crate::convert`]).
#[derive(Debug, Clone)]
pub struct Vocab {
    codepoints: Vec<String>,
}

impl Vocab {
    /// Build from the bundle's `codepoints` metadata table.
    pub fn new(codepoints: Vec<String>) -> Result<Self, VocabError> {
        if codepoints.len() != BLANK {
            return Err(VocabError(format!(
                "expected {BLANK} codepoints, got {}",
                codepoints.len()
            )));
        }
        Ok(Self { codepoints })
    }

    /// Character emitted by a non-blank class id.
    pub fn char_of_class(&self, class: usize) -> Option<&str> {
        if class == BLANK {
            return None;
        }
        self.codepoints.get(class).map(String::as_str)
    }

    /// Number of codepoints (excludes blank).
    pub fn num_codepoints(&self) -> usize {
        self.codepoints.len()
    }

    /// Raw table (conversion round-trip checks).
    pub fn codepoints(&self) -> &[String] {
        &self.codepoints
    }
}

/// Malformed bundle vocab metadata.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("invalid vocab: {0}")]
pub struct VocabError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> Vocab {
        // 3-entry table for the mapping test (bypasses the
        // production length check via struct literal).
        Vocab {
            codepoints: vec![String::from(" "), String::from("!"), String::from("あ")],
        }
    }

    #[test]
    fn mapping() {
        assert_eq!(BLANK, 6759);
        assert_eq!(label_of_class(0), Some(2));
        assert_eq!(label_of_class(6759), None);
        let v = tiny();
        assert_eq!(v.char_of_class(6759), None);
        assert_eq!(v.char_of_class(0), Some(" "));
        assert_eq!(v.char_of_class(1), Some("!"));
        assert_eq!(v.char_of_class(2), Some("あ"));
        assert_eq!(v.num_codepoints(), 3);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(Vocab::new(vec![String::from("a")]).is_err());
        assert!(Vocab::new(vec![]).is_err());
    }
}

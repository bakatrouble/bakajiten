//! Japanese dictionary library (JMdict/JMnedict).
//!
//! Storage-optimized binary artifact plus in-memory lookup structures:
//! exact/prefix/suffix/substring reading↔kanji search, gloss full-text
//! index, and Japanese collation-aware ranking.

pub mod ffi;
pub mod furigana;
pub mod gloss;
pub mod kana;
pub mod kanji;
pub mod pinyin;
pub mod romaji;

uniffi::setup_scaffolding!();

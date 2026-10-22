//! Kanji records (`KJD1`): one character with its SVG strokes plus
//! the display subset of kanjidic2 (readings, meanings, grade, JLPT,
//! frequency). The recognizer bank (`KTT1`) stays shape-only; this
//! file feeds future dictionary UI. Layout: header (magic + u32
//! version + u32 payload length) plus an rkyv payload, so the file
//! can be mmapped and read in place without decoding (see
//! [`kanji_view`](file::kanji_view)).

pub mod codes;
pub mod display;
pub mod file;
pub mod record;
pub mod version;
#[cfg(test)]
mod tests;

pub use codes::{Deroo, FourCorner, Kuten, Nelson, Njecd, Oneill, ShDesc, Skip};
pub use file::{
    ArchivedKanjiFile, KANJI_MAGIC, KANJI_VERSION, KanjiFile, kanji_from_bytes, kanji_header,
    kanji_to_bytes, kanji_view, save_kanji,
};
pub use record::{KanjiRecord, Point, Stroke};

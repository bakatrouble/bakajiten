use super::codes::{Deroo, FourCorner, Kuten, Nelson, Njecd, Oneill, ShDesc, Skip};
use serde::{Deserialize, Serialize};

/// A 2D point in pen space (0..255): KanjiVG stroke data as packed.
/// Deliberately NOT `recognizer::geometry::Point` (same shape, own
/// type): a dict→recognizer dependency makes Corrosion's per-crate
/// `cargo rustc` invocations resolve shared-dependency features
/// differently, so the recognizer's fingerprint alternates between
/// invocations and both crates recompile on every build. Strokes
/// never cross the crate boundary (dict only stores/displays them),
/// so the twin cannot drift out of sync functionally.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// One KanjiVG stroke: an ordered polyline (see `Point` for why this
/// is dict-owned rather than shared with the recognizer).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Stroke(pub Vec<Point>);

impl Stroke {
    pub fn new(points: Vec<Point>) -> Self {
        Self(points)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One dictionary character: SVG strokes (pen space) plus display
/// metadata. Variant-tagged values live in dedicated per-type
/// fields (never generic tag arrays): readings by script, meanings
/// by language, lookup codes by system.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KanjiRecord {
    pub character: char,
    pub strokes: Vec<Stroke>,
    /// Kun readings (`ja_kun`, `.` marks okurigana splits).
    pub kun: Vec<String>,
    /// On readings (`ja_on`).
    pub on: Vec<String>,
    /// Chinese readings (`pinyin`).
    pub chinese: Vec<String>,
    /// Korean readings (`korean_r` + `korean_h`).
    pub korean: Vec<String>,
    /// Vietnamese readings (`vietnam`).
    pub vietnamese: Vec<String>,
    /// English glosses (absent `m_lang`); other languages stay in
    /// kanjidic2 itself.
    pub meanings_en: Vec<String>,
    /// Name readings (`nanori*`, kana).
    pub nanori: Vec<String>,
    /// Radical names (`rad_name*`, usually the character itself).
    pub rad_names: Vec<String>,
    /// SKIP codes.
    pub skip: Vec<Skip>,
    /// Any SKIP code flagged as a known miscount.
    pub skip_misgrade: bool,
    /// Four-corner codes.
    pub four_corner: Vec<FourCorner>,
    /// De Roo codes.
    pub deroo: Vec<Deroo>,
    /// SH descriptors.
    pub sh_desc: Vec<ShDesc>,
    /// JIS X 0208 code points.
    pub jis208: Vec<Kuten>,
    /// JIS X 0212 code points.
    pub jis212: Vec<Kuten>,
    /// JIS X 0213 code points.
    pub jis213: Vec<Kuten>,
    /// De Roo variant references.
    pub deroo_variant: Vec<Deroo>,
    /// Nelson variant references.
    pub nelson_c: Vec<Nelson>,
    /// NJECD variant references.
    pub njecd: Vec<Njecd>,
    /// O'Neill variant references (numeric plus optional suffix).
    pub oneill: Vec<Oneill>,
    /// SH variant references (opaque codes).
    pub s_h: Vec<String>,
    pub grade: Option<u32>,
    pub jlpt: Option<u32>,
    pub freq: Option<u32>,
    /// Stroke-order parts from kradfile (`kanji : parts…`), in
    /// stroke order. Empty when kradfile has no entry (kana etc.).
    pub parts: Vec<char>,
    /// The character's own JIS X 0208 codepoint (`cp_value` with
    /// `cp_type="jis208"`, `plane-ku-ten` form like `1-24-62`),
    /// absent when kanjidic2 carries none. Variant references stay in
    /// the `jis208`/`jis212`/`jis213` lists.
    pub jis208_code: Option<[u8; 2]>,
    /// Classical (`rad_type="classical"`) Kangxi radical number
    /// 1–214, absent when kanjidic2 carries none. The glyph itself
    /// is a display mapping (see the Qt side), never stored.
    pub radical: Option<u32>,
}

/// One kanji file: source versions plus every character (strokes for
/// all KanjiVG entries; kanjidic2 metadata empty when absent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KanjiFile {
    /// KanjiVG source version (`--kanjivg-version`, else "unknown":
    /// checkouts carry no marker).
    pub kanjivg_version: String,
    /// kanjidic2 `<date_of_creation>`.
    pub kanjidic2_version: String,
    pub records: Vec<KanjiRecord>,
}

impl KanjiRecord {
    /// Strokes without kanjidic2 metadata (kana etc.): every metadata
    /// field empty.
    pub fn bare(character: char, strokes: Vec<Stroke>) -> Self {
        Self {
            character,
            strokes,
            kun: Vec::new(),
            on: Vec::new(),
            chinese: Vec::new(),
            korean: Vec::new(),
            vietnamese: Vec::new(),
            meanings_en: Vec::new(),
            nanori: Vec::new(),
            rad_names: Vec::new(),
            skip: Vec::new(),
            skip_misgrade: false,
            four_corner: Vec::new(),
            deroo: Vec::new(),
            sh_desc: Vec::new(),
            jis208: Vec::new(),
            jis212: Vec::new(),
            jis213: Vec::new(),
            deroo_variant: Vec::new(),
            nelson_c: Vec::new(),
            njecd: Vec::new(),
            oneill: Vec::new(),
            s_h: Vec::new(),
            grade: None,
            jlpt: None,
            freq: None,
            parts: Vec::new(),
            jis208_code: None,
            radical: None,
        }
    }

    /// UCS codepoint hex, as the `ucs` variant would carry it
    /// (computed, never stored).
    pub fn ucs(&self) -> String {
        format!("{:X}", self.character as u32)
    }
}

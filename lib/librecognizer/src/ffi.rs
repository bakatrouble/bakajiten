//! UniFFI foreign-function interface: recognition for Kotlin, Swift,
//! Python, C++ (Qt, via `uniffi-bindgen-cpp`) and friends.
//!
//! The C++ bindings generate at Qt-build time from the compiled
//! cdylib (see `ui_qt/CMakeLists.txt` + `ui_qt/README.md`), so they
//! can never drift: the generated checksums throw on mismatch.
//!
//! The C++ generator (`v0.10.0+v0.31.0`) targets UniFFI 0.31, so the
//! workspace pins `uniffi = "0.31"` — see the version research in the
//! Qt migration notes. Bumping past 0.31 needs a newer generator
//! first.
//!
//! The exported surface is deliberately narrow: load model bytes once
//! into an opaque [`Recognizer`], then score drawings. All matching
//! numerics live in `rank`/`preprocess`; this module only converts
//! types at the boundary. Panics never cross it — every fallible
//! operation returns [`RecognizerError`], and UniFFI turns Rust
//! panics in exported functions into foreign exceptions rather than
//! undefined behavior.
//!
//! [`Recognizer`] is plain data behind an `Arc` (`Send + Sync` by
//! construction), so foreign callers may share one instance across
//! threads; rayon parallelizes each query internally.

use std::sync::Arc;

use crate::model::templates_from_bytes;
use crate::rank::{find_candidates, KanaFilter, Query, Template};
use crate::script::{script_of as classify, Script};
use crate::{Point, Stroke};

/// Script of the first character (empty reads as other).
#[uniffi::export]
pub fn script_of(text: String) -> Script {
    classify(&text)
}

/// One pen point in 0..255 pen coords (matches `Point`, which UniFFI
/// cannot export directly — tuple structs have no record mapping).
#[derive(Debug, Clone, uniffi::Record)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
}

impl From<StrokePoint> for Point {
    fn from(p: StrokePoint) -> Self {
        Point::new(p.x, p.y)
    }
}

/// One ranked candidate: the character plus its fused distance.
/// Lower is better; absolute values are meaningless across inputs —
/// only ordering matters.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Candidate {
    /// The matched character, as a single-char string (UniFFI has no
    /// `char` type).
    pub character: String,
    pub distance: u64,
    /// Whether the template is hiragana/katakana (same predicate as
    /// [`KanaFilter`]: U+3040–U+30FF). Lets foreign callers style
    /// kana hits without re-deriving script ranges.
    pub is_kana: bool,
}

/// Failures surfaced across the FFI boundary.
///
/// Payloads are named fields (not tuple variants): the TypeScript
/// generator needs field names to emit valid types.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum RecognizerError {
    /// Model bytes fail to decode (see `model::templates_from_bytes`).
    #[error("invalid model: {message}")]
    InvalidModel { message: String },
    /// The model decoded but holds zero templates.
    #[error("model holds no templates")]
    EmptyBank,
}

/// Pack timestamp (`packed_at`, unix seconds) of a `templates.bin`
/// on disk: 16-byte header read only, no model decode. Lets the
/// Android dictionary manager show the saved version.
#[uniffi::export]
pub fn templates_packed_at(path: String) -> Result<u64, RecognizerError> {
    use crate::model::{TEMPLATES_MAGIC, TEMPLATES_VERSION};
    use std::io::Read;
    let mut head = [0u8; 16];
    std::fs::File::open(&path)
        .and_then(|mut f| f.read_exact(&mut head))
        .map_err(|e| RecognizerError::InvalidModel {
            message: format!("{path}: {e}"),
        })?;
    if &head[..4] != TEMPLATES_MAGIC {
        return Err(RecognizerError::InvalidModel {
            message: format!("{path}: missing magic (want KTT1)"),
        });
    }
    let version = u32::from_le_bytes(head[4..8].try_into().expect("4-byte slice"));
    if version != TEMPLATES_VERSION {
        return Err(RecognizerError::InvalidModel {
            message: format!("{path}: unsupported version {version}"),
        });
    }
    Ok(u64::from_le_bytes(head[8..16].try_into().expect("8-byte slice")))
}

/// Fractional stroke reveal for players (arc-length + interpolated
/// tip): constant drawing speed regardless of point density.
#[uniffi::export]
pub fn stroke_slice(points: Vec<StrokePoint>, fraction: f32) -> Vec<StrokePoint> {
    crate::geometry::stroke_slice(
        &points.into_iter().map(Point::from).collect::<Vec<_>>(),
        fraction,
    )
    .into_iter()
    .map(|p| StrokePoint { x: p.x, y: p.y })
    .collect()
}

/// Opaque recognizer: a template bank built once from model bytes.
#[derive(Debug, uniffi::Object)]
pub struct Recognizer {
    bank: Vec<Template>,
    packed_at: u64,
}

#[uniffi::export]
impl Recognizer {
    /// Build a recognizer from `KTT1` template-bank bytes (see
    /// `model`). Share the returned handle across threads and
    /// queries.
    #[uniffi::constructor]
    pub fn new(model_bytes: Vec<u8>) -> Result<Arc<Self>, RecognizerError> {
        let (bank, packed_at) =
            templates_from_bytes(&model_bytes).map_err(|e| RecognizerError::InvalidModel {
                message: e.to_string(),
            })?;
        if bank.is_empty() {
            return Err(RecognizerError::EmptyBank);
        }
        Ok(Arc::new(Self { bank, packed_at }))
    }

    /// Rank candidates for raw pen strokes: one inner vec per stroke,
    /// in 0..255 pen coords. Returns the `top_k` best (lower distance
    /// first); `top_k = 0` returns the whole bank, sorted.
    /// `include_kana` selects kanji+kana (`true`) or kanji-only
    /// (`false`) matching.
    pub fn recognize(
        &self,
        strokes: Vec<Vec<StrokePoint>>,
        top_k: u32,
        include_kana: bool,
    ) -> Vec<Candidate> {
        let raw: Vec<Stroke> = strokes
            .into_iter()
            .map(|s| Stroke::new(s.into_iter().map(Point::from).collect()))
            .collect();
        let query = Query::drawn(&raw);
        let kana = if include_kana {
            KanaFilter::All
        } else {
            KanaFilter::SkipKana
        };
        find_candidates(&query, &self.bank, top_k as usize, kana)
            .into_iter()
            .map(|hit| Candidate {
                character: hit.character.to_string(),
                distance: hit.distance,
                is_kana: hit.is_kana,
            })
            .collect()
    }

    /// Template (bank) count.
    pub fn template_count(&self) -> u64 {
        self.bank.len() as u64
    }

    /// Shape-similar kanji for drawn strokes (the kanji dialog's
    /// Similar strip): kanji-only match like `recognize` with
    /// `include_kana = false`, the `exclude` character dropped,
    /// capped at `top_k` shown (`top_k = 0` shows nothing — unlike
    /// `recognize`, where it means the whole bank). Self exclusion
    /// compares text (not rank — self usually leads, but the filter
    /// holds regardless). Empty without strokes.
    pub fn similar(
        &self,
        strokes: Vec<Vec<StrokePoint>>,
        exclude: String,
        top_k: u32,
    ) -> Vec<Candidate> {
        if strokes.iter().all(|s| s.is_empty()) {
            return Vec::new();
        }
        self.recognize(strokes, top_k.saturating_add(1), false)
            .into_iter()
            .filter(|hit| hit.character != exclude)
            .take(top_k as usize)
            .collect()
    }

    /// Pack timestamp of the loaded bank (unix seconds).
    pub fn model_packed_at(&self) -> u64 {
        self.packed_at
    }
}

#[cfg(test)]
mod ffi_tests {
    use super::*;
    use crate::model::{build_templates, templates_to_bytes, ModelEntry};

    const STAMP: u64 = 1_700_000_000;

    #[test]
    fn ffi_templates_packed_at_reads_header_only() {
        let path = std::env::temp_dir().join("ffi-test-templates-packed-at.bin");
        std::fs::write(&path, toy_model()).unwrap();
        assert_eq!(
            templates_packed_at(path.to_str().unwrap().into()).unwrap(),
            STAMP
        );
        assert!(templates_packed_at("/nonexistent/bakajiten-templates-test.bin".into()).is_err());
    }

    fn toy_model() -> Vec<u8> {
        let h = ModelEntry {
            character: '一',
            strokes: vec![Stroke::new(vec![
                Point::new(10.0, 50.0),
                Point::new(90.0, 50.0),
            ])],
        };
        let v = ModelEntry {
            character: '｜',
            strokes: vec![Stroke::new(vec![
                Point::new(50.0, 10.0),
                Point::new(50.0, 90.0),
            ])],
        };
        templates_to_bytes(&build_templates(&[h, v]), STAMP)
    }

    fn hline() -> Vec<StrokePoint> {
        vec![
            StrokePoint { x: 12.0, y: 52.0 },
            StrokePoint { x: 88.0, y: 48.0 },
        ]
    }

    #[test]
    fn ffi_recognizes_through_the_exported_api() {
        let r = Recognizer::new(toy_model()).unwrap();
        assert_eq!(r.template_count(), 2);
        assert_eq!(r.model_packed_at(), STAMP);
        let hits = r.recognize(vec![hline()], 2, true);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].character, "一");
        assert!(hits[0].distance < hits[1].distance);
        // Whole-bank order matches the native ranking.
        let all = r.recognize(vec![hline()], 0, true);
        assert_eq!(all.len(), 2);
        // Neither toy template is kana, so kanji-only matches all.
        let kanji = r.recognize(vec![hline()], 0, false);
        assert_eq!(kanji.len(), 2);
    }

    #[test]
    fn ffi_marks_kana_candidates() {
        let kana = ModelEntry {
            character: 'あ',
            strokes: vec![Stroke::new(vec![
                Point::new(20.0, 20.0),
                Point::new(80.0, 30.0),
            ])],
        };
        let kanji = ModelEntry {
            character: '一',
            strokes: vec![Stroke::new(vec![
                Point::new(10.0, 50.0),
                Point::new(90.0, 50.0),
            ])],
        };
        let r =
            Recognizer::new(templates_to_bytes(&build_templates(&[kana, kanji]), STAMP)).unwrap();
        let hits = r.recognize(vec![hline()], 0, true);
        assert_eq!(hits.len(), 2);
        let kana_hit = hits.iter().find(|c| c.character == "あ").unwrap();
        assert!(kana_hit.is_kana);
        let kanji_hit = hits.iter().find(|c| c.character == "一").unwrap();
        assert!(!kanji_hit.is_kana);
    }

    #[test]
    fn ffi_similar_excludes_and_caps() {
        let r = Recognizer::new(toy_model()).unwrap();
        // Best match (一) excluded by text; the other template
        // shows, capped at one.
        let hits = r.similar(vec![hline()], "一".into(), 1);
        assert_eq!(hits.len(), 1);
        assert_ne!(hits[0].character, "一");
        assert!(!hits[0].is_kana);
        // Nothing excluded: best first, capped.
        let all = r.similar(vec![hline()], String::new(), 10);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].character, "一");
        // Empty strokes match nothing; zero cap shows nothing.
        assert!(r.similar(vec![], "一".into(), 5).is_empty());
        assert!(r.similar(vec![hline()], String::new(), 0).is_empty());
    }

    #[test]
    fn ffi_rejects_garbage_model() {
        assert!(matches!(
            Recognizer::new(b"not a model".to_vec()),
            Err(RecognizerError::InvalidModel { .. })
        ));
        assert!(matches!(
            Recognizer::new(templates_to_bytes(&[], STAMP)),
            Err(RecognizerError::EmptyBank)
        ));
    }
}

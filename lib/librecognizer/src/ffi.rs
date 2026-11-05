//! UniFFI foreign-function interface over the tegaki engine.
//!
//! The C++ bindings generate at Qt-build time from the compiled
//! cdylib, so they can never drift: the generated checksums throw
//! on mismatch.
//!
//! The exported surface is deliberately narrow: convert the
//! download packs once ([`convert_model`]), load the converted
//! bundle into an opaque [`Recognizer`], then decode pen ink.
//! All numerics live in this crate's ported engine (`ink`,
//! `features`, `nn`, `vocab`, `decode`); this module only converts
//! types at the boundary. Panics never cross it — every fallible
//! operation returns [`RecognizerError`].
//!
//! [`Recognizer`] is plain data behind an `Arc` (`Send + Sync` by
//! construction), so foreign callers may share one instance across
//! threads.

use std::sync::Arc;

use crate::convert::{ConvertedModel, ConvertError};
use crate::decode::{beam_search, greedy, ClassBoost, NoLm};
use crate::ink::Ink;
use crate::nn::{BiLstm, NnError};
use crate::vocab::{Vocab, VocabError};

/// One pen point in 0..255 pen coords. Timestamps are not carried:
/// conversion synthesizes 20ms steps for the whole ink (the JNI
/// fallback the engine was validated against).
#[derive(Debug, Clone, uniffi::Record)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
}

/// One hypothesis: the decoded text plus its score. Beam scores
/// are costs (lower ranks first); the greedy score is a mean
/// frame confidence (higher is surer). Order is what matters —
/// both lists arrive best first.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Candidate {
    pub text: String,
    pub score: f64,
}

/// Converted runtime bundle: the dequantized net plus the JSON
/// vocab the decoder rebuilds its class table from.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConvertedBundle {
    pub safetensors: Vec<u8>,
}

impl From<ConvertedModel> for ConvertedBundle {
    fn from(model: ConvertedModel) -> Self {
        Self {
            safetensors: model.safetensors,
        }
    }
}

/// Failures surfaced across the FFI boundary.
///
/// Payloads are named fields (not tuple variants): the TypeScript
/// generator needs field names to emit valid types.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum RecognizerError {
    /// Pack or bundle bytes fail to decode.
    #[error("invalid model: {message}")]
    InvalidModel { message: String },
    /// Decoding failed.
    #[error("recognition failed: {message}")]
    RecognitionFailed { message: String },
}

impl From<ConvertError> for RecognizerError {
    fn from(e: ConvertError) -> Self {
        Self::InvalidModel { message: e.to_string() }
    }
}

impl From<NnError> for RecognizerError {
    fn from(e: NnError) -> Self {
        Self::InvalidModel { message: e.to_string() }
    }
}

impl From<VocabError> for RecognizerError {
    fn from(e: VocabError) -> Self {
        Self::InvalidModel { message: e.to_string() }
    }
}

/// Convert download packs into the runtime bundle: the `.tflite`
/// net plus the recospec protobuf become one `network.safetensors`.
/// Runs once per pack update; the Qt download path calls it after
/// unzipping, before installing.
#[uniffi::export]
pub fn convert_model(
    tflite_bytes: Vec<u8>,
    recospec_bytes: Vec<u8>,
) -> Result<ConvertedBundle, RecognizerError> {
    crate::convert::convert_model(&tflite_bytes, &recospec_bytes)
        .map(ConvertedBundle::from)
        .map_err(RecognizerError::from)
}

/// Whole ink from pen strokes: one inner vec per stroke, in
/// 0..255 pen coords, timestamps synthesized (20ms steps).
fn ink_from_pen(strokes: Vec<Vec<StrokePoint>>) -> Ink {
    Ink::from_raw(
        strokes
            .into_iter()
            .map(|s| s.into_iter().map(|p| (p.x, p.y, None)).collect())
            .collect(),
    )
}

/// Fractional stroke reveal for players (arc-length + interpolated
/// tip): constant drawing speed regardless of point density.
#[uniffi::export]
pub fn stroke_slice(points: Vec<StrokePoint>, fraction: f32) -> Vec<StrokePoint> {
    if points.is_empty() || fraction <= 0.0 {
        return Vec::new();
    }
    let first = points[0].clone();
    let mut segs = Vec::with_capacity(points.len().saturating_sub(1));
    let mut total = 0.0f64;
    for w in points.windows(2) {
        let len = f64::hypot((w[1].x - w[0].x) as f64, (w[1].y - w[0].y) as f64);
        segs.push(len);
        total += len;
    }
    if total <= 0.0 {
        return vec![first];
    }
    let mut target = f64::min(fraction as f64, 1.0) * total;
    let mut out = vec![first];
    for (i, w) in points.windows(2).enumerate() {
        let seg = segs[i];
        if target > seg {
            target -= seg;
            out.push(w[1].clone());
        } else {
            let t = if seg > 0.0 { target / seg } else { 0.0 };
            out.push(StrokePoint {
                x: (w[0].x as f64 + t * (w[1].x as f64 - w[0].x as f64)) as f32,
                y: (w[0].y as f64 + t * (w[1].y as f64 - w[0].y as f64)) as f32,
            });
            break;
        }
    }
    out
}

/// Opaque recognizer: the converted Japanese bundle (safetensors
/// net with the vocab in its header metadata).
#[derive(uniffi::Object)]
pub struct Recognizer {
    net: BiLstm,
    vocab: Vocab,
    boost: ClassBoost,
}

/// Beam width for interactive recognition: tegaki's GUI passes 8
/// (recospec decoder flag `f12 = 2` is the production beam
/// factor); the beam follows the request below, never dropping
/// under it, so a `topK` strip always has candidates to fill.
const INTERACTIVE_BEAM_WIDTH: usize = 8;

#[uniffi::export]
impl Recognizer {
    /// Build a recognizer from converted bundle contents: the
    /// safetensors net (vocab rides its header metadata). Share
    /// the returned handle across threads and queries.
    #[uniffi::constructor]
    pub fn new(safetensors_bytes: Vec<u8>) -> Result<Arc<Self>, RecognizerError> {
        let net = BiLstm::from_bytes(&safetensors_bytes)?;
        let (codepoints, classes, spec) = crate::convert::read_vocab(&safetensors_bytes)?;
        let vocab = Vocab::new(codepoints)?;
        let classes: Vec<(String, f32)> =
            classes.into_iter().map(|(c, w)| (c, w as f32)).collect();
        let boost = ClassBoost::build(vocab.codepoints(), &classes, &spec);
        Ok(Arc::new(Self { net, vocab, boost }))
    }

    /// Decode pen strokes: one inner vec per stroke, in 0..255 pen
    /// coords. Returns the `nbest` best hypotheses, best first.
    /// Empty ink decodes to no candidates.
    pub fn recognize(
        &self,
        strokes: Vec<Vec<StrokePoint>>,
        nbest: u32,
    ) -> Result<Vec<Candidate>, RecognizerError> {
        let ink = ink_from_pen(strokes);
        if ink.is_empty() {
            return Ok(Vec::new());
        }
        let (feats, _) = crate::features::extract(&ink, None, &crate::features::FeatureConfig::default());
        if feats.is_empty() {
            return Ok(Vec::new());
        }
        let logits = self.net.forward(&feats);
        let width = (nbest.max(1) as usize).max(INTERACTIVE_BEAM_WIDTH);
        let mut hits = beam_search(&logits, width, &self.boost, &self.vocab);
        hits.truncate(nbest.max(1) as usize);
        Ok(hits
            .into_iter()
            .map(|h| Candidate { text: h.text, score: h.score as f64 })
            .collect())
    }

    /// Best-path decode, ignoring the language model entirely.
    pub fn recognize_greedy(
        &self,
        strokes: Vec<Vec<StrokePoint>>,
    ) -> Result<Candidate, RecognizerError> {
        let ink = ink_from_pen(strokes);
        if ink.is_empty() {
            return Err(RecognizerError::RecognitionFailed {
                message: String::from("empty ink"),
            });
        }
        let (feats, _) = crate::features::extract(&ink, None, &crate::features::FeatureConfig::default());
        if feats.is_empty() {
            return Err(RecognizerError::RecognitionFailed {
                message: String::from("empty ink"),
            });
        }
        let logits = self.net.forward(&feats);
        let (text, conf) = greedy(&logits, &self.vocab);
        Ok(Candidate { text, score: conf as f64 })
    }

    /// The engine ships no word-level language model (the compact
    /// FST is a proprietary format): kept so callers can branch on
    /// it, always false.
    pub fn has_language_model(&self) -> bool {
        let _ = NoLm;
        false
    }
}

#[cfg(test)]
mod ffi_tests {
    use super::*;

    fn hline() -> Vec<StrokePoint> {
        vec![
            StrokePoint { x: 12.0, y: 52.0 },
            StrokePoint { x: 88.0, y: 48.0 },
        ]
    }

    #[test]
    fn ffi_rejects_garbage_model() {
        assert!(matches!(
            Recognizer::new(b"not a model".to_vec()),
            Err(RecognizerError::InvalidModel { .. })
        ));
        assert!(matches!(
            convert_model(b"not a net".to_vec(), b"not a spec".to_vec()),
            Err(RecognizerError::InvalidModel { .. })
        ));
    }

    #[test]
    fn ffi_empty_ink_decodes_to_nothing() {
        // Bytes that fail `Recognizer::new` never reach decoding, so
        // empty-ink behavior is asserted at the conversion layer: no
        // samples in, no samples out.
        let ink = ink_from_pen(vec![]);
        assert!(ink.is_empty());
        let one_empty: Vec<Vec<StrokePoint>> = vec![vec![]];
        assert!(ink_from_pen(one_empty).is_empty());
    }

    #[test]
    fn ffi_slice_reveals_by_arc_length() {
        assert!(stroke_slice(vec![], 0.5).is_empty());
        assert!(stroke_slice(hline(), 0.0).is_empty());
        let out = stroke_slice(hline(), 1.0);
        assert_eq!(out.len(), 2);
        let quarter = stroke_slice(hline(), 0.25);
        assert_eq!(quarter.len(), 2);
        assert!((quarter[1].x - 31.0).abs() < 1.0);
    }
}

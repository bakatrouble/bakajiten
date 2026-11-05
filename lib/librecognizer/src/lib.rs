//! Handwriting recognition: tegaki's pure-Rust engine, owned here.
//!
//! The inference engine is ported from tegaki (`~/dev/bakajiten/google-recog`,
//! MIT): ink preprocessing, Bézier features, the 5x144 BiLSTM network,
//! and CTC prefix beam search with character-class scores. What
//! changed in the move:
//!
//! ```text
//! pen strokes (0..255 x/y, no timestamps)
//!   ──► ink (tegaki Ink, synthetic 20ms timestamps)
//!   ──► features (10-dim Bézier curves)
//!   ──► nn::BiLstm forward (safetensors net)
//!   ──► decode (greedy best path or pruned beam + class boost)
//!   ──► candidates (text + score)
//! ```
//!
//! File map: `ink` (pen model + timestamps), `features` (curve
//! fit + 10-dim encode), `nn` (BiLSTM + logits), `vocab` (class
//! id <-> character), `decode` (greedy + beam search), `fst`
//! (text-format FST loader for a future LM), `convert` (one-time
//! TFLite+recospec → bundle), `ffi` (UniFFI boundary:
//! `Recognizer`, `Candidate`, `StrokePoint`, `ConvertedBundle`,
//! `convert_model`, `stroke_slice`).
//!
//! Models are Google's Japanese pack (dl.google.com zips): the
//! frontends download the recospec + tflite, convert once to
//! `network.safetensors` via [`convert::convert_model`], and pass
//! those bytes to [`ffi::Recognizer::new`]. This crate never
//! downloads anything.
//!
//! # Quick start
//!
//! ```ignore
//! use recognizer::ffi::{Recognizer, StrokePoint};
//!
//! let strokes = vec![vec![
//!     StrokePoint { x: 10.0, y: 50.0 },
//!     StrokePoint { x: 90.0, y: 50.0 },
//! ]];
//! let bundle = std::fs::read("network.safetensors").unwrap();
//! let r = Recognizer::new(bundle).expect("model loads");
//! let hits = r.recognize(strokes, 5).expect("ink decodes");
//! println!("{:?}", hits);
//! ```

pub mod convert;
pub mod decode;
pub mod features;
pub mod ffi;
pub mod fst;
pub mod ink;
pub mod nn;
pub mod pack;
pub mod vocab;

uniffi::setup_scaffolding!();

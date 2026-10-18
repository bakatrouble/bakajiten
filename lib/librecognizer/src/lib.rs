//! Classical handwritten-kanji matching: geometry plus a two-stage DTW/layout matcher
//! (per-stroke segment-angle DTW + 16-bit relative-position layout
//! descriptors, fused per candidate).
//!
//! The algorithm is Sólyom Zoltán's handwriting recognizer from
//! [zKanji](https://github.com/z1dev/zkanji), ported here from the
//! algorithm outline in `handwritten-kanji-algorithm`.
//!
//! ```text
//! query strokes ──► simplify ──► dissect ──► per-stroke segment-angle DTW
//!      │                                              │
//!      └────────► 16-bit relative-position layout ────┴──► fused score
//! ```
//!
//! 1. **Shape** — every stroke is denoised (`simplify`, §4.1) and reduced
//!    to a short corner-preserving point list (`dissect`, §4.2); two
//!    strokes compare by DTW over segment angles with brush-hook
//!    suppression (`compare`, §5).
//! 2. **Layout** — each stroke pair is described by a 16-bit descriptor
//!    of relative position (`compute_positions`, §6.1); template vs.
//!    query descriptors compare by band-index distance (`pos_diff`, §6.2).
//! 3. **Fusion** — greedy order-tolerant shape assignment plus layout,
//!    size-prior, and missing-stroke terms (`rank_element`, §7).
//!
//! ## Deliberate deviations from the outline
//!
//! - **No `zksod` / `recdata`.** The original design precomputes
//!   per-template-stroke closest-model indexes with its stroke editor
//!   and loads them verbatim. We have no such file: templates are
//!   KanjiVG strokes, and drawn strokes compare against template
//!   strokes *directly* (the outline's `compareToModels` + `recdata`
//!   lookup collapses into one `compare()` call per drawn/template pair).
//! - **Unit space is pen ÷ 255.** The outline works in draw-area
//!   unit space; pen strokes live in a 255×255 box, so unit coords
//!   are `p / 255.0`. All outline constants transfer unchanged because
//!   they are unit-space fractions.
//! - **No glyph normalization before matching.** Absolute size feeds
//!   the ±5% size prior (§7.3) and relative geometry feeds the layout
//!   descriptors.
//! - **No stroke-count prefilter.** The outline effectively never
//!   prefilters by stroke count (`cntlimit`); every query scores the
//!   full bank (one DTW matrix per drawn/template pair).
//! - **Beam search omitted.** The outline prunes candidates mid-scan at
//!   `1.5×` the running best; we score every candidate and sort, which
//!   yields the same top-k ordering without the scan-order dependence.
//! - **`[RECON]` marks reconstructed details.** The outline leaves a few
//!   formulas implicit (position-zone geometry, hook-region sizing,
//!   dissect scan bookkeeping); those spots implement the most faithful
//!   reading and say so.
//!
//! All `src/kanjistrokes.cpp` line references below point at the
//! outline's numbering.
//!
//! File map: `geometry` (`Point`/`Stroke` polylines in pen space),
//! `geom` (unit-space math, resampling, sections), `preprocess`
//! (`simplify`, `dissect`, comparison-stroke assembly), `shape` (hook
//! suppression, segment-angle DTW, `compare`), `layout` (16-bit position
//! descriptors, `pos_diff`), `rank` (candidate fusion,
//! `find_candidates`), `model` (recognition model file, built from a
//! KanjiVG checkout by the `pack-data` tool), `error` (crate-wide
//! `Error`/`Result`).
//!
//! Pure Rust, dependency-lean: builds anywhere, including WASM,
//! and embeds easily.
//!
//! # Quick start
//!
//! ```no_run
//! use recognizer::{Point, Stroke};
//! use recognizer::preprocess::trajectories_from_strokes;
//! use recognizer::rank::{KanaFilter, Query, Template, find_candidates};
//!
//! // Template: clean KanjiVG strokes; query: hand-drawn strokes.
//! // Both live in pen coords; the matcher converts to unit space.
//! let tmpl_strokes = vec![Stroke::new(vec![Point::new(10.0, 50.0), Point::new(90.0, 50.0)])];
//! let bank = vec![Template::new('一', trajectories_from_strokes(&tmpl_strokes, false))];
//! let query_strokes = vec![Stroke::new(vec![Point::new(12.0, 52.0), Point::new(88.0, 48.0)])];
//! let query = Query::drawn(&query_strokes);
//! let ranked = find_candidates(&query, &bank, 5, KanaFilter::All);
//! println!("{:?}", ranked);
//! ```
//!
//! # Model file
//!
//! ```sh
//! cargo run -p utils --bin pack-data -- --kanjivg dataimports/kanjivg --templates-out data/templates.bin
//! ```
//! loads `templates.bin` at startup via [`model::templates_from_bytes`].

pub mod error;
pub mod ffi;
pub mod geom;
pub mod geometry;
pub mod layout;
pub mod model;
pub mod preprocess;
pub mod rank;
pub mod script;
pub mod shape;

pub use error::{Error, Result};
pub use geometry::{Point, Stroke};

uniffi::setup_scaffolding!();

//! UniFFI foreign-function interface: dictionary search plus kanji
//! records for Kotlin, Swift, Python, C++ (Qt, via
//! `uniffi-bindgen-cpp`) and friends.
//!
//! Same contract as `recognizer::ffi`: the C++ bindings generate
//! at Qt-build time from the compiled cdylib (see
//! `ui_qt/cmake/uniffi.cmake`), so they can never drift. The
//! generator targets UniFFI 0.31, matching the workspace pin.
//!
//! The exported surface is deliberately narrow: load artifact bytes
//! once into opaque handles, then query. All ranking and parsing
//! live in `gloss`/`kanji`; this module only converts types at the
//! boundary. Panics never cross it — every fallible operation
//! returns [`records::DictError`].
//!
//! Memory model: each handle owns its artifact storage — a file
//! mapping on native targets (instant, page-aligned, shared page
//! cache), caller-provided bytes on WASM (no mmap syscalls there).
//! Construction runs a header check only (magic, version, exact
//! length — instant). The archive itself is never validated: these
//! are our own pack outputs, verified with a decode roundtrip at
//! write time, so the header is the only runtime gate (same trust
//! model the old workbench browser used). Queries use unchecked zero-copy
//! views afterwards, which is sound because the storage is owned,
//! immutable, and header-checked before any view existed.
//! Candidates are verified against the real strings: prefix keys are
//! lossy by design.
//!
//! Strokes ride along for display (S.O.D. diagrams) and as match
//! queries, but matching itself lives in the recognizer crate:
//! this crate depends on nothing but parsing and storage, so the
//! two per-crate `cargo rustc` invocations share no units and a
//! build of one never rebuilds the other. Callers join the pieces
//! (strokes out, candidates in) trackside.
pub mod objects;
pub mod records;
#[cfg(test)]
mod tests;

pub use crate::furigana::FuriganaSpan;
pub use crate::kanji::display::GradeKind;
pub use crate::romaji::RomajiStyle;
pub use objects::{Dictionary, KanjiDictionary};
pub use records::{Deinflection, DictEntry, DictError, DictSense, KanjiEntry, SearchHit};

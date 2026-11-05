//! Pack-format readers: TFLite weight parsing and recospec protobuf
//! parsing for Google's handwriting packs.
//!
//! Ported from `rust/crates/mlkit-ink` (`tflite.rs`, `recospec.rs`,
//! `proto.rs`, `settings.rs`, `error.rs`) at pinned revision
//! `bd95dbc`, with only mechanical changes: `alloc` → `std` paths,
//! module paths repointed here, the `std`-feature gate dropped (this
//! crate is always `std`), and two intra-doc links to modules that
//! were not ported flattened to plain text. Parsing behavior is
//! unchanged — `convert`'s determinism test pins the output bytes.

#[macro_use]
mod error;
mod proto;
mod settings;
pub mod recospec;
pub mod tflite;

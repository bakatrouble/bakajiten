//! Build-time source parsers.
//!
//! KanjiVG SVG parsing (`kanjivg`) plus raw dictionary source parsers
//! (`jmdict` for JMdict/JMnedict, `kanjidic` for kanjidic2,
//! `radkfile` for the EDRDG radical index, `kradfile` for the EDRDG
//! kanji-parts index). All parsers are SAX-style
//! and constant-memory; dictionary structs are raw captures (no
//! normalization, no entity expansion — conversion is the
//! consumer's job). Shared [`error`] type for all four.

pub mod error;
pub mod jmdict;
pub mod kanjidic;
pub mod kanjivg;
pub mod kradfile;
pub mod radkfile;

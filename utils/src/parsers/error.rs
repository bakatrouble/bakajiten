//! Shared error type for build-time source parsing.

use thiserror::Error;

/// Failures while reading or parsing dictionary/KanjiVG sources.
#[derive(Debug, Error)]
pub enum Error {
    #[error("xml error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad entry: {0}")]
    Bad(String),
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, Error>;

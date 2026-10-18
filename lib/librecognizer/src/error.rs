use thiserror::Error as ThisError;

/// Crate-wide error type.
#[derive(Debug, ThisError)]
pub enum Error {
    #[error("io error: {0}")]
    Io(String),
    #[error("xml error: {0}")]
    Xml(String),
    #[error("model error: {0}")]
    Model(String),
    #[error("json error: {0}")]
    Json(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e.to_string())
    }
}

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, Error>;

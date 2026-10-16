use serde::Serialize;
use ts_rs::TS;

/// Every fallible core operation returns this error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("database integrity check failed: {0}")]
    Integrity(String),

    #[error("database schema version {found} is newer than this app supports ({supported})")]
    SchemaTooNew { found: usize, supported: usize },

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("{0} not found")]
    NotFound(&'static str),

    #[error("{0}")]
    Conflict(String),

    #[error("nothing to {0}")]
    NothingTo(&'static str),

    #[error("corrupt change log: {0}")]
    Log(String),
}

impl Error {
    /// Stable machine-readable code; the UI switches on this, never on the message.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "sqlite",
            Self::Migration(_) => "migration",
            Self::Integrity(_) => "integrity",
            Self::SchemaTooNew { .. } => "schema_too_new",
            Self::Io(_) => "io",
            Self::InvalidInput(_) => "invalid_input",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::NothingTo(_) => "nothing_to_do",
            Self::Log(_) => "log",
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Log(err.to_string())
    }
}

/// Shorthand for input validation failures.
pub fn invalid(msg: impl Into<String>) -> Error {
    Error::InvalidInput(msg.into())
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Serializable form of [`Error`] that crosses the IPC boundary.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl From<&Error> for ErrorInfo {
    fn from(err: &Error) -> Self {
        Self {
            code: err.code().to_owned(),
            message: err.to_string(),
        }
    }
}

use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    NotFound,
    Io,
    Database,
    Busy,
    Internal,
}

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        tracing::error!(target: "application", error = %error, "Filesystem operation failed");
        Self::new(
            ErrorCode::Io,
            "A local file could not be accessed. Check the folder and permissions.",
        )
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        tracing::error!(target: "application", error = %error, "Database operation failed");
        Self::new(
            ErrorCode::Database,
            "The local job database could not be accessed.",
        )
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        tracing::error!(target: "application", error = %error, "Stored data is invalid");
        Self::new(
            ErrorCode::Database,
            "Some local job data could not be read.",
        )
    }
}

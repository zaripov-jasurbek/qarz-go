use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde_json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("reqwest: {0}")]
    Http(#[from] reqwest::Error),

    #[error("telegram api: {description} (code {error_code:?})")]
    Telegram { error_code: Option<i64>, description: String },

    #[error("not found: {0}")]
    NotFound(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("bad input: {0}")]
    BadInput(String),

    #[error("config: {0}")]
    Config(String),

    #[error("other: {0}")]
    Other(String),
}

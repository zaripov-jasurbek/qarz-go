use crate::error::{AppError, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub bot_token: String,
    pub data_dir: PathBuf,
    /// Если задано — будем поднимать axum-сервер для webhook'а (опционально).
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    /// Адрес axum-сервера, по умолчанию 0.0.0.0:8080.
    pub bind_addr: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bot_token = std::env::var("BOT_TOKEN")
            .map_err(|_| AppError::Config("BOT_TOKEN not set".into()))?;
        let data_dir = std::env::var("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));
        let webhook_url = std::env::var("WEBHOOK_URL").ok().filter(|s| !s.is_empty());
        let webhook_secret = std::env::var("WEBHOOK_SECRET").ok().filter(|s| !s.is_empty());
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
        Ok(Self { bot_token, data_dir, webhook_url, webhook_secret, bind_addr })
    }
}

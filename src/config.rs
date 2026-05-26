use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub bot_token: String,
    /// Connection string MongoDB. Обязателен.
    pub mongodb_uri: String,
    /// Имя базы в MongoDB, по умолчанию "loan_wallet".
    pub mongodb_db: String,
    /// Публичный URL бота — Telegram будет слать сюда апдейты. Обязателен.
    /// Пример: https://my-bot.up.railway.app
    pub webhook_url: String,
    /// Секрет для проверки X-Telegram-Bot-Api-Secret-Token. Необязателен.
    pub webhook_secret: Option<String>,
    /// Адрес axum-сервера, по умолчанию 0.0.0.0:8080.
    pub bind_addr: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bot_token = std::env::var("BOT_TOKEN")
            .map_err(|_| AppError::Config("BOT_TOKEN not set".into()))?;
        let mongodb_uri = std::env::var("MONGODB_URI")
            .map_err(|_| AppError::Config("MONGODB_URI not set".into()))?;
        let mongodb_db = std::env::var("MONGODB_DB")
            .unwrap_or_else(|_| "loan_wallet".into());
        let webhook_url = std::env::var("WEBHOOK_URL")
            .map_err(|_| AppError::Config("WEBHOOK_URL not set".into()))?;
        let webhook_secret = std::env::var("WEBHOOK_SECRET").ok().filter(|s| !s.is_empty());
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
        Ok(Self { bot_token, mongodb_uri, mongodb_db, webhook_url, webhook_secret, bind_addr })
    }
}

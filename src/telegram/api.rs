//! Тонкий клиент к Telegram Bot API. Без сторонних обёрток (teloxide и пр.) —
//! только reqwest + типы из `super::types`.

use crate::error::{AppError, Result};
use crate::telegram::types::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

const BASE: &str = "https://api.telegram.org";

#[derive(Clone)]
pub struct BotApi {
    inner: Arc<Inner>,
}

struct Inner {
    token: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    #[serde(default)]
    error_code: Option<i64>,
    #[serde(default)]
    description: Option<String>,
}

impl BotApi {
    pub fn new(token: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Self { inner: Arc::new(Inner { token: token.into(), http }) }
    }

    pub fn token(&self) -> &str {
        &self.inner.token
    }

    async fn call<T, B>(&self, method: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = format!("{BASE}/bot{}/{}", self.inner.token, method);
        let resp = self.inner.http.post(&url).json(body).send().await?;
        let parsed: TgResponse<T> = resp.json().await?;
        if parsed.ok {
            parsed.result.ok_or_else(|| AppError::Telegram {
                error_code: None,
                description: format!("{method}: ok but no result"),
            })
        } else {
            Err(AppError::Telegram {
                error_code: parsed.error_code,
                description: parsed.description.unwrap_or_default(),
            })
        }
    }

    pub async fn get_me(&self) -> Result<TgUser> {
        self.call("getMe", &json!({})).await
    }

    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_markup: Option<&ReplyMarkup>,
    ) -> Result<Message> {
        let mut body = serde_json::Map::new();
        body.insert("chat_id".into(), json!(chat_id));
        body.insert("text".into(), json!(text));
        body.insert("parse_mode".into(), json!("HTML"));
        body.insert("disable_web_page_preview".into(), json!(true));
        if let Some(rm) = reply_markup {
            body.insert("reply_markup".into(), serde_json::to_value(rm)?);
        }
        self.call("sendMessage", &Value::Object(body)).await
    }

    pub async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<&InlineKeyboardMarkup>,
    ) -> Result<()> {
        let mut body = serde_json::Map::new();
        body.insert("chat_id".into(), json!(chat_id));
        body.insert("message_id".into(), json!(message_id));
        body.insert("text".into(), json!(text));
        body.insert("parse_mode".into(), json!("HTML"));
        if let Some(rm) = reply_markup {
            body.insert("reply_markup".into(), serde_json::to_value(rm)?);
        }
        // result игнорируем — может быть Message или true
        let _: Value = self.call("editMessageText", &Value::Object(body)).await?;
        Ok(())
    }

    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<()> {
        let mut body = serde_json::Map::new();
        body.insert("callback_query_id".into(), json!(callback_query_id));
        if let Some(t) = text {
            body.insert("text".into(), json!(t));
        }
        let _: bool = self.call("answerCallbackQuery", &Value::Object(body)).await?;
        Ok(())
    }

    pub async fn set_webhook(&self, url: &str, secret_token: Option<&str>) -> Result<()> {
        let mut body = serde_json::Map::new();
        body.insert("url".into(), json!(url));
        body.insert("allowed_updates".into(), json!(["message", "callback_query"]));
        if let Some(s) = secret_token {
            body.insert("secret_token".into(), json!(s));
        }
        let _: bool = self.call("setWebhook", &Value::Object(body)).await?;
        Ok(())
    }
}

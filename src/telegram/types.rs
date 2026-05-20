//! Подмножество типов Telegram Bot API, которое нужно боту.
//! Поля, которые не используем, не описываем — serde игнорирует их.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<TgUser>,
    pub chat: Chat,
    pub text: Option<String>,
    pub contact: Option<TgContact>,
}

#[derive(Debug, Deserialize)]
pub struct TgUser {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub language_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Deserialize)]
pub struct TgContact {
    pub phone_number: String,
    pub first_name: String,
    pub last_name: Option<String>,
    /// Telegram user_id владельца контакта (если контакт зарегистрирован в Telegram).
    pub user_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: TgUser,
    pub message: Option<Message>,
    pub data: Option<String>,
}

// ===== Outgoing =====

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ReplyMarkup {
    InlineKeyboard(InlineKeyboardMarkup),
    ReplyKeyboard(ReplyKeyboardMarkup),
    Remove(ReplyKeyboardRemove),
}

#[derive(Debug, Serialize, Clone)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl InlineKeyboardButton {
    pub fn callback(text: impl Into<String>, data: impl Into<String>) -> Self {
        Self { text: text.into(), callback_data: Some(data.into()), url: None }
    }
    pub fn link(text: impl Into<String>, url: impl Into<String>) -> Self {
        Self { text: text.into(), callback_data: None, url: Some(url.into()) }
    }
}

#[derive(Debug, Serialize)]
pub struct ReplyKeyboardMarkup {
    pub keyboard: Vec<Vec<KeyboardButton>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize_keyboard: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_time_keyboard: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct KeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_contact: Option<bool>,
}

impl KeyboardButton {
    pub fn text(t: impl Into<String>) -> Self {
        Self { text: t.into(), request_contact: None }
    }
    pub fn request_contact(t: impl Into<String>) -> Self {
        Self { text: t.into(), request_contact: Some(true) }
    }
}

#[derive(Debug, Serialize)]
pub struct ReplyKeyboardRemove {
    pub remove_keyboard: bool,
}

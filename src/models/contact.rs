use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    /// Владелец адресной книги.
    pub owner_user_id: String,
    /// Как владелец назвал этот контакт.
    pub display_name: String,
    /// Из Telegram shared contact.
    pub phone: String,
    /// Если контакт сам зарегался в боте — линкуем сюда его User.id.
    pub linked_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Contact {
    pub fn new(owner_user_id: String, display_name: String, phone: String) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            owner_user_id,
            display_name,
            phone: normalize_phone(&phone),
            linked_user_id: None,
            created_at: Utc::now(),
        }
    }
}

pub fn normalize_phone(s: &str) -> String {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    // Бывают форматы с 8 в начале (РФ) — нормализуем в +7.
    if digits.len() == 11 && digits.starts_with('8') {
        format!("+7{}", &digits[1..])
    } else if !digits.is_empty() {
        format!("+{}", digits.trim_start_matches('+'))
    } else {
        String::new()
    }
}

use crate::models::money::Currency;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub telegram_id: i64,
    pub telegram_username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
    pub phone: Option<String>,
    pub language: String,
    pub preferred_currency: Currency,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn new(telegram_id: i64, first_name: String) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            telegram_id,
            telegram_username: None,
            first_name,
            last_name: None,
            phone: None,
            language: "ru".to_string(),
            preferred_currency: Currency::Uzs,
            created_at: Utc::now(),
        }
    }

    pub fn display_name(&self) -> String {
        match &self.last_name {
            Some(ln) => format!("{} {}", self.first_name, ln),
            None => self.first_name.clone(),
        }
    }
}

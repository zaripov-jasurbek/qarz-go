use crate::models::money::Currency;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomStatus {
    /// Создатель добавляет позиции и/или участники выбирают.
    Collecting,
    /// Списки заморожены, ждём чтобы все отметились.
    Locked,
    /// Доли посчитаны, долги созданы.
    Settled,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub creator_user_id: String,
    /// Только linked-юзеры (с известным telegram_id).
    pub member_user_ids: Vec<String>,
    pub currency: Currency,
    pub status: RoomStatus,
    pub created_at: DateTime<Utc>,
    pub locked_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
}

impl Room {
    pub fn new(name: String, creator_user_id: String, currency: Currency) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            name,
            creator_user_id: creator_user_id.clone(),
            member_user_ids: vec![creator_user_id],
            currency,
            status: RoomStatus::Collecting,
            created_at: Utc::now(),
            locked_at: None,
            settled_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomItem {
    pub id: String,
    pub room_id: String,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    /// Цена за позицию целиком, в минимальных единицах валюты комнаты.
    pub total_price_minor: i64,
    /// Кто из участников взял эту позицию. Сумма делится между ними.
    pub selected_by: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl RoomItem {
    pub fn new(room_id: String, name: String, total_price_minor: i64) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            room_id,
            name,
            quantity: None,
            unit: None,
            total_price_minor,
            selected_by: Vec::new(),
            created_at: Utc::now(),
        }
    }
}

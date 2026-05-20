use crate::models::money::Currency;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// FSM для многошаговых диалогов. Один Session = одна активная "ветка" на юзера.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub telegram_id: i64,
    pub state: SessionState,
    pub updated_at: DateTime<Utc>,
}

impl Session {
    pub fn new(telegram_id: i64, state: SessionState) -> Self {
        Self { telegram_id, state, updated_at: Utc::now() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum SessionState {
    Idle,

    // Контакты
    AwaitingContactName,                                  // ждём как назвать контакт
    AwaitingContactShare { display_name: String },        // ждём, что юзер нажмёт "поделиться контактом"

    // Комнаты
    AwaitingRoomName,
    AwaitingRoomCurrency { name: String },
    AwaitingItemName { room_id: String },
    AwaitingItemPrice { room_id: String, item_name: String },

    // Долги
    AwaitingDebtorPick,                                   // выбор контакта-должника
    AwaitingDebtAmount { debtor_user_id: String },
    AwaitingDebtCurrency { debtor_user_id: String, amount_minor_or_text: String },
    AwaitingDebtDescription { debtor_user_id: String, amount_minor: i64, currency: Currency },
    AwaitingInstallmentPlan { debt_id: String },          // "3 по 50000 каждые 30 дней"

    // Погашение
    AwaitingPaymentAmount { debt_id: String },
}

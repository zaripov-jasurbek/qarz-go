use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// B заблокировал A, значит A не может его добавить в комнату/повесить долг,
/// а бот не шлёт B уведомлений от действий A.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub blocker_user_id: String,
    pub blocked_user_id: String,
    pub created_at: DateTime<Utc>,
}

impl Block {
    pub fn new(blocker: String, blocked: String) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            blocker_user_id: blocker,
            blocked_user_id: blocked,
            created_at: Utc::now(),
        }
    }
}

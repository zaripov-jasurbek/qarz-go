use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum InvitePurpose {
    /// Приглашение от A: когда B нажмёт /start, связываем B как linked_user
    /// в Contact с этим id.
    AddContact { contact_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    /// Токен в deep-link: t.me/<bot>?start=<token>. Он же _id.
    pub token: String,
    pub created_by_user_id: String,
    pub purpose: InvitePurpose,
    pub used_by_user_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Invite {
    pub fn new(created_by: String, purpose: InvitePurpose) -> Self {
        // Telegram ограничивает start payload до 64 символов [A-Za-z0-9_-].
        let token: String = uuid::Uuid::now_v7()
            .as_simple()
            .to_string();
        Self {
            token,
            created_by_user_id: created_by,
            purpose,
            used_by_user_id: None,
            expires_at: None,
            created_at: Utc::now(),
        }
    }
}

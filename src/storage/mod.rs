use crate::error::Result;
use crate::models::*;
use async_trait::async_trait;

pub mod mongo;

pub use mongo::MongoStorage;

/// Высокоуровневый трейт хранилища. Файловая и Mongo-реализации должны быть
/// взаимозаменяемы. Все методы async, чтобы переход на Mongo был бесшовным.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // ===== Users =====
    async fn get_user_by_telegram_id(&self, telegram_id: i64) -> Result<Option<User>>;
    async fn get_user(&self, id: &str) -> Result<Option<User>>;
    async fn upsert_user(&self, user: &User) -> Result<()>;
    async fn find_users_by_phone(&self, phone: &str) -> Result<Vec<User>>;
    async fn list_all_users(&self) -> Result<Vec<User>>;

    // ===== Contacts =====
    async fn add_contact(&self, contact: &Contact) -> Result<()>;
    async fn list_contacts(&self, owner_user_id: &str) -> Result<Vec<Contact>>;
    async fn get_contact(&self, id: &str) -> Result<Option<Contact>>;
    /// Найти все контакты с заданным телефоном (среди всех владельцев).
    async fn find_contacts_by_phone(&self, phone: &str) -> Result<Vec<Contact>>;
    /// Обновить linked_user_id у контакта.
    async fn link_contact(&self, contact_id: &str, user_id: &str) -> Result<()>;
    async fn delete_contact(&self, id: &str) -> Result<()>;

    // ===== Rooms =====
    async fn create_room(&self, room: &Room) -> Result<()>;
    async fn get_room(&self, id: &str) -> Result<Option<Room>>;
    async fn update_room(&self, room: &Room) -> Result<()>;
    async fn list_rooms_for_user(&self, user_id: &str) -> Result<Vec<Room>>;

    // ===== Room items =====
    async fn add_item(&self, item: &RoomItem) -> Result<()>;
    async fn get_item(&self, id: &str) -> Result<Option<RoomItem>>;
    async fn update_item(&self, item: &RoomItem) -> Result<()>;
    async fn list_items_in_room(&self, room_id: &str) -> Result<Vec<RoomItem>>;

    // ===== Debts =====
    async fn create_debt(&self, debt: &Debt) -> Result<()>;
    async fn get_debt(&self, id: &str) -> Result<Option<Debt>>;
    async fn update_debt(&self, debt: &Debt) -> Result<()>;
    async fn list_debts_for_user(&self, user_id: &str) -> Result<Vec<Debt>>;

    // ===== Blocks =====
    async fn add_block(&self, block: &Block) -> Result<()>;
    async fn remove_block(&self, blocker: &str, blocked: &str) -> Result<()>;
    async fn is_blocked(&self, blocker: &str, blocked: &str) -> Result<bool>;
    async fn list_blocks_by(&self, blocker: &str) -> Result<Vec<Block>>;

    // ===== Invites =====
    async fn create_invite(&self, invite: &Invite) -> Result<()>;
    async fn get_invite(&self, token: &str) -> Result<Option<Invite>>;
    async fn mark_invite_used(&self, token: &str, used_by_user_id: &str) -> Result<()>;

    // ===== Sessions (FSM) =====
    async fn get_session(&self, telegram_id: i64) -> Result<Option<Session>>;
    async fn set_session(&self, session: &Session) -> Result<()>;
    async fn clear_session(&self, telegram_id: i64) -> Result<()>;
}

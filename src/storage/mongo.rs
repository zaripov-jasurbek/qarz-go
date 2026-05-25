//! MongoDB хранилище — реализация Storage trait через mongodb crate.
//!
//! Стратегия _id:
//!  • Сущности с полем `id: String` (User, Contact, Room, RoomItem, Debt, Block)
//!    → используют свой UUID как `_id`. Поле `id` переименовываем при сохранении
//!    (id → _id) и при чтении (_id → id).
//!  • Invite: `_id = token` (String).
//!  • Session: `_id = telegram_id` (i64).
//!
//! DateTime<Utc> хранится как строка RFC3339 — bson пропускает её через
//! стандартный serde, что совместимо с файловым хранилищем без изменений моделей.

use crate::error::{AppError, Result};
use crate::models::*;
use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::bson::{self, doc, Document};
use mongodb::{Client, Collection, Database, IndexModel};
use tracing::info;

pub struct MongoStorage {
    db: Database,
}

impl MongoStorage {
    pub async fn open(uri: &str, db_name: &str) -> Result<Self> {
        info!("connecting to MongoDB: db={db_name}");
        let client = Client::with_uri_str(uri).await.map_err(mongo_err)?;
        let db = client.database(db_name);
        let s = Self { db };
        s.ensure_indexes().await?;
        info!("MongoDB ready");
        Ok(s)
    }

    fn col(&self, name: &str) -> Collection<Document> {
        self.db.collection(name)
    }

    async fn ensure_indexes(&self) -> Result<()> {
        // Создаём индексы при старте (idempotent — повтор не страшен).
        let idx = |keys: Document| IndexModel::builder().keys(keys).build();

        self.col("users").create_index(idx(doc! { "telegram_id": 1 })).await.map_err(mongo_err)?;
        self.col("users").create_index(idx(doc! { "phone": 1 })).await.map_err(mongo_err)?;

        self.col("contacts").create_index(idx(doc! { "owner_user_id": 1 })).await.map_err(mongo_err)?;
        self.col("contacts").create_index(idx(doc! { "phone": 1 })).await.map_err(mongo_err)?;

        self.col("rooms").create_index(idx(doc! { "member_user_ids": 1 })).await.map_err(mongo_err)?;
        self.col("items").create_index(idx(doc! { "room_id": 1 })).await.map_err(mongo_err)?;

        self.col("debts").create_index(idx(doc! { "debtor_user_id": 1 })).await.map_err(mongo_err)?;
        self.col("debts").create_index(idx(doc! { "creditor_user_id": 1 })).await.map_err(mongo_err)?;

        self.col("blocks").create_index(idx(doc! { "blocker_user_id": 1 })).await.map_err(mongo_err)?;

        Ok(())
    }
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn mongo_err(e: mongodb::error::Error) -> AppError {
    AppError::Other(format!("mongo: {e}"))
}
fn bser(e: bson::ser::Error) -> AppError {
    AppError::Other(format!("bson serialize: {e}"))
}
fn bde(e: bson::de::Error) -> AppError {
    AppError::Other(format!("bson deserialize: {e}"))
}

/// Сериализовать сущность у которой есть поле `id` → переименовываем в `_id`.
fn to_doc<T: serde::Serialize>(value: &T) -> Result<Document> {
    let mut d = bson::to_document(value).map_err(bser)?;
    if let Some(id) = d.remove("id") {
        d.insert("_id", id);
    }
    Ok(d)
}

/// Десериализовать сущность с полем `id` ← переименовываем `_id`.
fn from_doc<T: serde::de::DeserializeOwned>(mut d: Document) -> Result<T> {
    if let Some(id) = d.remove("_id") {
        d.insert("id", id);
    }
    bson::from_document(d).map_err(bde)
}

/// Курсор Document → Vec<T> через from_doc.
async fn collect<T: serde::de::DeserializeOwned>(
    mut cursor: mongodb::Cursor<Document>,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    while let Some(d) = cursor.try_next().await.map_err(mongo_err)? {
        out.push(from_doc(d)?);
    }
    Ok(out)
}

// ─── Storage impl ────────────────────────────────────────────────────────────

#[async_trait]
impl crate::storage::Storage for MongoStorage {
    // ===== Users =====

    async fn get_user_by_telegram_id(&self, telegram_id: i64) -> Result<Option<User>> {
        self.col("users")
            .find_one(doc! { "telegram_id": telegram_id })
            .await.map_err(mongo_err)?
            .map(from_doc).transpose()
    }

    async fn get_user(&self, id: &str) -> Result<Option<User>> {
        self.col("users")
            .find_one(doc! { "_id": id })
            .await.map_err(mongo_err)?
            .map(from_doc).transpose()
    }

    async fn upsert_user(&self, user: &User) -> Result<()> {
        let d = to_doc(user)?;
        self.col("users")
            .replace_one(doc! { "_id": &user.id }, d)
            .upsert(true)
            .await.map_err(mongo_err)?;
        Ok(())
    }

    async fn find_users_by_phone(&self, phone: &str) -> Result<Vec<User>> {
        collect(self.col("users").find(doc! { "phone": phone }).await.map_err(mongo_err)?).await
    }

    async fn list_all_users(&self) -> Result<Vec<User>> {
        collect(self.col("users").find(doc! {}).await.map_err(mongo_err)?).await
    }

    // ===== Contacts =====

    async fn add_contact(&self, contact: &Contact) -> Result<()> {
        self.col("contacts").insert_one(to_doc(contact)?).await.map_err(mongo_err)?;
        Ok(())
    }

    async fn list_contacts(&self, owner_user_id: &str) -> Result<Vec<Contact>> {
        collect(
            self.col("contacts")
                .find(doc! { "owner_user_id": owner_user_id })
                .await.map_err(mongo_err)?,
        ).await
    }

    async fn get_contact(&self, id: &str) -> Result<Option<Contact>> {
        self.col("contacts")
            .find_one(doc! { "_id": id })
            .await.map_err(mongo_err)?
            .map(from_doc).transpose()
    }

    async fn find_contacts_by_phone(&self, phone: &str) -> Result<Vec<Contact>> {
        collect(self.col("contacts").find(doc! { "phone": phone }).await.map_err(mongo_err)?).await
    }

    async fn link_contact(&self, contact_id: &str, user_id: &str) -> Result<()> {
        let r = self.col("contacts")
            .update_one(
                doc! { "_id": contact_id },
                doc! { "$set": { "linked_user_id": user_id } },
            )
            .await.map_err(mongo_err)?;
        if r.matched_count == 0 {
            return Err(AppError::NotFound(format!("contact {contact_id}")));
        }
        Ok(())
    }

    async fn delete_contact(&self, id: &str) -> Result<()> {
        let r = self.col("contacts")
            .delete_one(doc! { "_id": id })
            .await.map_err(mongo_err)?;
        if r.deleted_count == 0 {
            return Err(AppError::NotFound(format!("contact {id}")));
        }
        Ok(())
    }

    // ===== Rooms =====

    async fn create_room(&self, room: &Room) -> Result<()> {
        self.col("rooms").insert_one(to_doc(room)?).await.map_err(mongo_err)?;
        Ok(())
    }

    async fn get_room(&self, id: &str) -> Result<Option<Room>> {
        self.col("rooms")
            .find_one(doc! { "_id": id })
            .await.map_err(mongo_err)?
            .map(from_doc).transpose()
    }

    async fn update_room(&self, room: &Room) -> Result<()> {
        let r = self.col("rooms")
            .replace_one(doc! { "_id": &room.id }, to_doc(room)?)
            .await.map_err(mongo_err)?;
        if r.matched_count == 0 {
            return Err(AppError::NotFound(format!("room {}", room.id)));
        }
        Ok(())
    }

    async fn list_rooms_for_user(&self, user_id: &str) -> Result<Vec<Room>> {
        // member_user_ids — массив, MongoDB матчит по элементу.
        collect(
            self.col("rooms")
                .find(doc! { "member_user_ids": user_id })
                .await.map_err(mongo_err)?,
        ).await
    }

    // ===== Room items =====

    async fn add_item(&self, item: &RoomItem) -> Result<()> {
        self.col("items").insert_one(to_doc(item)?).await.map_err(mongo_err)?;
        Ok(())
    }

    async fn get_item(&self, id: &str) -> Result<Option<RoomItem>> {
        self.col("items")
            .find_one(doc! { "_id": id })
            .await.map_err(mongo_err)?
            .map(from_doc).transpose()
    }

    async fn update_item(&self, item: &RoomItem) -> Result<()> {
        let r = self.col("items")
            .replace_one(doc! { "_id": &item.id }, to_doc(item)?)
            .await.map_err(mongo_err)?;
        if r.matched_count == 0 {
            return Err(AppError::NotFound(format!("item {}", item.id)));
        }
        Ok(())
    }

    async fn list_items_in_room(&self, room_id: &str) -> Result<Vec<RoomItem>> {
        collect(
            self.col("items")
                .find(doc! { "room_id": room_id })
                .await.map_err(mongo_err)?,
        ).await
    }

    // ===== Debts =====

    async fn create_debt(&self, debt: &Debt) -> Result<()> {
        self.col("debts").insert_one(to_doc(debt)?).await.map_err(mongo_err)?;
        Ok(())
    }

    async fn get_debt(&self, id: &str) -> Result<Option<Debt>> {
        self.col("debts")
            .find_one(doc! { "_id": id })
            .await.map_err(mongo_err)?
            .map(from_doc).transpose()
    }

    async fn update_debt(&self, debt: &Debt) -> Result<()> {
        let r = self.col("debts")
            .replace_one(doc! { "_id": &debt.id }, to_doc(debt)?)
            .await.map_err(mongo_err)?;
        if r.matched_count == 0 {
            return Err(AppError::NotFound(format!("debt {}", debt.id)));
        }
        Ok(())
    }

    async fn list_debts_for_user(&self, user_id: &str) -> Result<Vec<Debt>> {
        collect(
            self.col("debts")
                .find(doc! { "$or": [
                    { "debtor_user_id": user_id },
                    { "creditor_user_id": user_id },
                ]})
                .await.map_err(mongo_err)?,
        ).await
    }

    // ===== Blocks =====

    async fn add_block(&self, block: &Block) -> Result<()> {
        let filter = doc! {
            "blocker_user_id": &block.blocker_user_id,
            "blocked_user_id": &block.blocked_user_id,
        };
        // Проверяем перед вставкой чтобы не менять _id существующего документа.
        let exists = self.col("blocks").find_one(filter).await.map_err(mongo_err)?.is_some();
        if !exists {
            self.col("blocks").insert_one(to_doc(block)?).await.map_err(mongo_err)?;
        }
        Ok(())
    }

    async fn remove_block(&self, blocker: &str, blocked: &str) -> Result<()> {
        self.col("blocks")
            .delete_one(doc! { "blocker_user_id": blocker, "blocked_user_id": blocked })
            .await.map_err(mongo_err)?;
        Ok(())
    }

    async fn is_blocked(&self, blocker: &str, blocked: &str) -> Result<bool> {
        Ok(self.col("blocks")
            .find_one(doc! { "blocker_user_id": blocker, "blocked_user_id": blocked })
            .await.map_err(mongo_err)?
            .is_some())
    }

    async fn list_blocks_by(&self, blocker: &str) -> Result<Vec<Block>> {
        collect(
            self.col("blocks")
                .find(doc! { "blocker_user_id": blocker })
                .await.map_err(mongo_err)?,
        ).await
    }

    // ===== Invites =====
    // Invite не имеет поля `id` — используем `token` как `_id`.

    async fn create_invite(&self, invite: &Invite) -> Result<()> {
        let mut d = bson::to_document(invite).map_err(bser)?;
        // token → _id
        if let Some(tok) = d.remove("token") {
            d.insert("_id", tok);
        }
        self.col("invites").insert_one(d).await.map_err(mongo_err)?;
        Ok(())
    }

    async fn get_invite(&self, token: &str) -> Result<Option<Invite>> {
        self.col("invites")
            .find_one(doc! { "_id": token })
            .await.map_err(mongo_err)?
            .map(|mut d| {
                // _id → token
                if let Some(t) = d.remove("_id") { d.insert("token", t); }
                bson::from_document::<Invite>(d).map_err(bde)
            })
            .transpose()
    }

    async fn mark_invite_used(&self, token: &str, used_by_user_id: &str) -> Result<()> {
        let r = self.col("invites")
            .update_one(
                doc! { "_id": token },
                doc! { "$set": { "used_by_user_id": used_by_user_id } },
            )
            .await.map_err(mongo_err)?;
        if r.matched_count == 0 {
            return Err(AppError::NotFound(format!("invite {token}")));
        }
        Ok(())
    }

    // ===== Sessions =====
    // Session не имеет поля `id` — используем `telegram_id` (i64) как `_id`.

    async fn get_session(&self, telegram_id: i64) -> Result<Option<Session>> {
        self.col("sessions")
            .find_one(doc! { "_id": telegram_id })
            .await.map_err(mongo_err)?
            .map(|mut d| {
                if let Some(tid) = d.remove("_id") { d.insert("telegram_id", tid); }
                bson::from_document::<Session>(d).map_err(bde)
            })
            .transpose()
    }

    async fn set_session(&self, session: &Session) -> Result<()> {
        let mut d = bson::to_document(session).map_err(bser)?;
        // telegram_id → _id
        if let Some(tid) = d.remove("telegram_id") {
            d.insert("_id", tid);
        }
        self.col("sessions")
            .replace_one(doc! { "_id": session.telegram_id }, d)
            .upsert(true)
            .await.map_err(mongo_err)?;
        Ok(())
    }

    async fn clear_session(&self, telegram_id: i64) -> Result<()> {
        self.col("sessions")
            .delete_one(doc! { "_id": telegram_id })
            .await.map_err(mongo_err)?;
        Ok(())
    }
}

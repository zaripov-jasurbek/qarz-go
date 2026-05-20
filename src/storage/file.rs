//! Файловое хранилище — JSON-файлы под одним Mutex'ом.
//!
//! Каждая сущность кладётся в отдельный `*.json` массив. Чтение/запись идут
//! через in-memory кэш, который грузится при старте. Запись делается атомарно:
//! пишем во временный файл и rename'им поверх — это безопасно даже при сбое.
//!
//! Структура нарочно простая: цель — быстро итерировать и потом без боли
//! перейти на Mongo. Все запросы делают линейный scan по in-memory вектору.

use crate::error::{AppError, Result};
use crate::models::*;
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

#[derive(Default)]
struct State {
    users: Vec<User>,
    contacts: Vec<Contact>,
    rooms: Vec<Room>,
    items: Vec<RoomItem>,
    debts: Vec<Debt>,
    blocks: Vec<Block>,
    invites: Vec<Invite>,
    sessions: HashMap<i64, Session>,
}

pub struct FileStorage {
    dir: PathBuf,
    state: Mutex<State>,
}

impl FileStorage {
    pub async fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        tokio::fs::create_dir_all(&dir).await?;

        let users: Vec<User> = load_or_empty(&dir.join("users.json")).await?;
        let contacts: Vec<Contact> = load_or_empty(&dir.join("contacts.json")).await?;
        let rooms: Vec<Room> = load_or_empty(&dir.join("rooms.json")).await?;
        let items: Vec<RoomItem> = load_or_empty(&dir.join("items.json")).await?;
        let debts: Vec<Debt> = load_or_empty(&dir.join("debts.json")).await?;
        let blocks: Vec<Block> = load_or_empty(&dir.join("blocks.json")).await?;
        let invites: Vec<Invite> = load_or_empty(&dir.join("invites.json")).await?;
        let session_list: Vec<Session> = load_or_empty(&dir.join("sessions.json")).await?;
        let sessions = session_list.into_iter().map(|s| (s.telegram_id, s)).collect();

        Ok(Self {
            dir,
            state: Mutex::new(State {
                users, contacts, rooms, items, debts, blocks, invites, sessions,
            }),
        })
    }

    async fn persist<T: Serialize>(&self, name: &str, data: &T) -> Result<()> {
        save_atomic(&self.dir.join(name), data).await
    }
}

async fn load_or_empty<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match tokio::fs::read(path).await {
        Ok(bytes) if bytes.is_empty() => Ok(T::default()),
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(AppError::Io(e)),
    }
}

async fn save_atomic<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(data)?;
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

#[async_trait]
impl crate::storage::Storage for FileStorage {
    // ===== Users =====
    async fn get_user_by_telegram_id(&self, telegram_id: i64) -> Result<Option<User>> {
        let s = self.state.lock().await;
        Ok(s.users.iter().find(|u| u.telegram_id == telegram_id).cloned())
    }

    async fn get_user(&self, id: &str) -> Result<Option<User>> {
        let s = self.state.lock().await;
        Ok(s.users.iter().find(|u| u.id == id).cloned())
    }

    async fn upsert_user(&self, user: &User) -> Result<()> {
        {
            let mut s = self.state.lock().await;
            if let Some(slot) = s.users.iter_mut().find(|u| u.id == user.id) {
                *slot = user.clone();
            } else {
                s.users.push(user.clone());
            }
            self.persist("users.json", &s.users).await?;
        }
        Ok(())
    }

    async fn find_users_by_phone(&self, phone: &str) -> Result<Vec<User>> {
        let s = self.state.lock().await;
        Ok(s.users.iter().filter(|u| u.phone.as_deref() == Some(phone)).cloned().collect())
    }

    async fn list_all_users(&self) -> Result<Vec<User>> {
        let s = self.state.lock().await;
        Ok(s.users.clone())
    }

    // ===== Contacts =====
    async fn add_contact(&self, contact: &Contact) -> Result<()> {
        let mut s = self.state.lock().await;
        s.contacts.push(contact.clone());
        self.persist("contacts.json", &s.contacts).await
    }

    async fn list_contacts(&self, owner_user_id: &str) -> Result<Vec<Contact>> {
        let s = self.state.lock().await;
        Ok(s.contacts.iter().filter(|c| c.owner_user_id == owner_user_id).cloned().collect())
    }

    async fn get_contact(&self, id: &str) -> Result<Option<Contact>> {
        let s = self.state.lock().await;
        Ok(s.contacts.iter().find(|c| c.id == id).cloned())
    }

    async fn find_contacts_by_phone(&self, phone: &str) -> Result<Vec<Contact>> {
        let s = self.state.lock().await;
        Ok(s.contacts.iter().filter(|c| c.phone == phone).cloned().collect())
    }

    async fn link_contact(&self, contact_id: &str, user_id: &str) -> Result<()> {
        let mut s = self.state.lock().await;
        if let Some(c) = s.contacts.iter_mut().find(|c| c.id == contact_id) {
            c.linked_user_id = Some(user_id.to_string());
            self.persist("contacts.json", &s.contacts).await?;
            Ok(())
        } else {
            Err(AppError::NotFound(format!("contact {contact_id}")))
        }
    }

    async fn delete_contact(&self, id: &str) -> Result<()> {
        let mut s = self.state.lock().await;
        let before = s.contacts.len();
        s.contacts.retain(|c| c.id != id);
        if s.contacts.len() == before {
            return Err(AppError::NotFound(format!("contact {id}")));
        }
        self.persist("contacts.json", &s.contacts).await
    }

    // ===== Rooms =====
    async fn create_room(&self, room: &Room) -> Result<()> {
        let mut s = self.state.lock().await;
        s.rooms.push(room.clone());
        self.persist("rooms.json", &s.rooms).await
    }

    async fn get_room(&self, id: &str) -> Result<Option<Room>> {
        let s = self.state.lock().await;
        Ok(s.rooms.iter().find(|r| r.id == id).cloned())
    }

    async fn update_room(&self, room: &Room) -> Result<()> {
        let mut s = self.state.lock().await;
        let slot = s.rooms.iter_mut().find(|r| r.id == room.id)
            .ok_or_else(|| AppError::NotFound(format!("room {}", room.id)))?;
        *slot = room.clone();
        self.persist("rooms.json", &s.rooms).await
    }

    async fn list_rooms_for_user(&self, user_id: &str) -> Result<Vec<Room>> {
        let s = self.state.lock().await;
        Ok(s.rooms.iter()
            .filter(|r| r.member_user_ids.iter().any(|id| id == user_id))
            .cloned()
            .collect())
    }

    // ===== Room items =====
    async fn add_item(&self, item: &RoomItem) -> Result<()> {
        let mut s = self.state.lock().await;
        s.items.push(item.clone());
        self.persist("items.json", &s.items).await
    }

    async fn get_item(&self, id: &str) -> Result<Option<RoomItem>> {
        let s = self.state.lock().await;
        Ok(s.items.iter().find(|i| i.id == id).cloned())
    }

    async fn update_item(&self, item: &RoomItem) -> Result<()> {
        let mut s = self.state.lock().await;
        let slot = s.items.iter_mut().find(|i| i.id == item.id)
            .ok_or_else(|| AppError::NotFound(format!("item {}", item.id)))?;
        *slot = item.clone();
        self.persist("items.json", &s.items).await
    }

    async fn list_items_in_room(&self, room_id: &str) -> Result<Vec<RoomItem>> {
        let s = self.state.lock().await;
        Ok(s.items.iter().filter(|i| i.room_id == room_id).cloned().collect())
    }

    // ===== Debts =====
    async fn create_debt(&self, debt: &Debt) -> Result<()> {
        let mut s = self.state.lock().await;
        s.debts.push(debt.clone());
        self.persist("debts.json", &s.debts).await
    }

    async fn get_debt(&self, id: &str) -> Result<Option<Debt>> {
        let s = self.state.lock().await;
        Ok(s.debts.iter().find(|d| d.id == id).cloned())
    }

    async fn update_debt(&self, debt: &Debt) -> Result<()> {
        let mut s = self.state.lock().await;
        let slot = s.debts.iter_mut().find(|d| d.id == debt.id)
            .ok_or_else(|| AppError::NotFound(format!("debt {}", debt.id)))?;
        *slot = debt.clone();
        self.persist("debts.json", &s.debts).await
    }

    async fn list_debts_for_user(&self, user_id: &str) -> Result<Vec<Debt>> {
        let s = self.state.lock().await;
        Ok(s.debts.iter()
            .filter(|d| d.debtor_user_id == user_id || d.creditor_user_id == user_id)
            .cloned()
            .collect())
    }

    // ===== Blocks =====
    async fn add_block(&self, block: &Block) -> Result<()> {
        let mut s = self.state.lock().await;
        // не дублируем
        let exists = s.blocks.iter().any(|b|
            b.blocker_user_id == block.blocker_user_id
                && b.blocked_user_id == block.blocked_user_id);
        if !exists {
            s.blocks.push(block.clone());
            self.persist("blocks.json", &s.blocks).await?;
        }
        Ok(())
    }

    async fn remove_block(&self, blocker: &str, blocked: &str) -> Result<()> {
        let mut s = self.state.lock().await;
        let before = s.blocks.len();
        s.blocks.retain(|b| !(b.blocker_user_id == blocker && b.blocked_user_id == blocked));
        if s.blocks.len() != before {
            self.persist("blocks.json", &s.blocks).await?;
        }
        Ok(())
    }

    async fn is_blocked(&self, blocker: &str, blocked: &str) -> Result<bool> {
        let s = self.state.lock().await;
        Ok(s.blocks.iter().any(|b|
            b.blocker_user_id == blocker && b.blocked_user_id == blocked))
    }

    async fn list_blocks_by(&self, blocker: &str) -> Result<Vec<Block>> {
        let s = self.state.lock().await;
        Ok(s.blocks.iter().filter(|b| b.blocker_user_id == blocker).cloned().collect())
    }

    // ===== Invites =====
    async fn create_invite(&self, invite: &Invite) -> Result<()> {
        let mut s = self.state.lock().await;
        s.invites.push(invite.clone());
        self.persist("invites.json", &s.invites).await
    }

    async fn get_invite(&self, token: &str) -> Result<Option<Invite>> {
        let s = self.state.lock().await;
        Ok(s.invites.iter().find(|i| i.token == token).cloned())
    }

    async fn mark_invite_used(&self, token: &str, used_by_user_id: &str) -> Result<()> {
        let mut s = self.state.lock().await;
        let slot = s.invites.iter_mut().find(|i| i.token == token)
            .ok_or_else(|| AppError::NotFound(format!("invite {token}")))?;
        slot.used_by_user_id = Some(used_by_user_id.to_string());
        self.persist("invites.json", &s.invites).await
    }

    // ===== Sessions =====
    async fn get_session(&self, telegram_id: i64) -> Result<Option<Session>> {
        let s = self.state.lock().await;
        Ok(s.sessions.get(&telegram_id).cloned())
    }

    async fn set_session(&self, session: &Session) -> Result<()> {
        let mut s = self.state.lock().await;
        s.sessions.insert(session.telegram_id, session.clone());
        let list: Vec<&Session> = s.sessions.values().collect();
        self.persist("sessions.json", &list).await
    }

    async fn clear_session(&self, telegram_id: i64) -> Result<()> {
        let mut s = self.state.lock().await;
        s.sessions.remove(&telegram_id);
        let list: Vec<&Session> = s.sessions.values().collect();
        self.persist("sessions.json", &list).await
    }
}

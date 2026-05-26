//! Диспетчер: маршрутизирует входящие Update'ы в нужный хэндлер.

pub mod common;
pub mod contacts;
pub mod debts;
pub mod rooms;
pub mod start;

use crate::error::Result;
use crate::handlers::common::{send_main_menu, upsert_from_tg};
use crate::models::SessionState;
use crate::services::Notifier;
use crate::storage::Storage;
use crate::telegram::api::BotApi;
use crate::telegram::types::{CallbackQuery, Message, Update};
use std::sync::Arc;
use tracing::warn;

pub struct Dispatcher<S: Storage> {
    pub storage: Arc<S>,
    pub notifier: Arc<Notifier<S>>,
    pub bot_username: String,
}

impl<S: Storage> Dispatcher<S> {
    pub fn new(storage: Arc<S>, notifier: Arc<Notifier<S>>, bot_username: String) -> Self {
        Self { storage, notifier, bot_username }
    }

    pub async fn handle(&self, api: &BotApi, upd: Update) -> Result<()> {
        if let Some(msg) = upd.message {
            self.handle_message(api, msg).await
        } else if let Some(cq) = upd.callback_query {
            self.handle_callback(api, cq).await
        } else {
            Ok(())
        }
    }

    async fn handle_message(&self, api: &BotApi, msg: Message) -> Result<()> {
        let Some(from) = msg.from.as_ref() else { return Ok(()); };
        if from.is_bot { return Ok(()); }

        let (user, is_new) = upsert_from_tg(&*self.storage, from).await?;
        let chat_id = msg.chat.id;

        // 1) Shared contact (важнее команд, потому что reply-кнопка).
        if let Some(contact) = msg.contact.as_ref() {
            return self.handle_contact(api, &msg, &user, contact).await;
        }

        // 2) Команды.
        if let Some(text) = msg.text.as_deref() {
            let trimmed = text.trim();
            if let Some(rest) = trimmed.strip_prefix("/start") {
                let token = rest.trim();
                let token = if token.is_empty() { None } else { Some(token) };
                return start::handle_start(api, &*self.storage, &msg, &user, is_new, token).await;
            }
            if trimmed == "/help" {
                return start::handle_help(api, &*self.storage, chat_id).await;
            }
            if trimmed == "/menu" {
                self.storage.clear_session(user.telegram_id).await?;
                return send_main_menu(api, &*self.storage, chat_id).await;
            }
            if trimmed == "/cancel" {
                self.storage.clear_session(user.telegram_id).await?;
                api.send_message(chat_id, "Отменено.", None).await?;
                return send_main_menu(api, &*self.storage, chat_id).await;
            }
        }

        // 3) Состояние FSM.
        let session = self.storage.get_session(user.telegram_id).await?;
        let state = session.map(|s| s.state).unwrap_or(SessionState::Idle);
        let text = msg.text.clone().unwrap_or_default();

        match state {
            SessionState::Idle => {
                // Свободный текст без команды — показываем меню.
                send_main_menu(api, &*self.storage, chat_id).await?;
            }
            SessionState::AwaitingContactName => {
                contacts::receive_name(api, &*self.storage, &msg, &user, &text).await?;
            }
            SessionState::AwaitingContactShare { .. } => {
                api.send_message(chat_id, "Нажмите кнопку «📞 Поделиться контактом» внизу.", None).await?;
            }
            SessionState::AwaitingRoomName => {
                rooms::receive_room_name(api, &*self.storage, &msg, &user, &text).await?;
            }
            SessionState::AwaitingRoomCurrency { .. } => {
                api.send_message(chat_id, "Выберите валюту кнопкой выше.", None).await?;
            }
            SessionState::AwaitingItemName { room_id } => {
                rooms::receive_item_name(api, &*self.storage, &msg, &user, &room_id, &text).await?;
            }
            SessionState::AwaitingItemPrice { room_id, item_name } => {
                rooms::receive_item_price(api, &*self.storage, &msg, &user, &room_id, &item_name, &text).await?;
            }
            SessionState::AwaitingDebtorPick => {
                api.send_message(chat_id, "Выберите должника кнопкой выше.", None).await?;
            }
            SessionState::AwaitingDebtAmount { debtor_user_id } => {
                debts::receive_amount(api, &*self.storage, &msg, &user, &debtor_user_id, &text).await?;
            }
            SessionState::AwaitingDebtCurrency { .. } => {
                api.send_message(chat_id, "Выберите валюту кнопкой выше.", None).await?;
            }
            SessionState::AwaitingDebtDescription { debtor_user_id, amount_minor, currency } => {
                debts::receive_description(
                    api, &*self.storage, &*self.notifier, &msg, &user,
                    &debtor_user_id, amount_minor, currency, &text,
                ).await?;
            }
            SessionState::AwaitingInstallmentPlan { debt_id } => {
                debts::receive_installment_plan(
                    api, &*self.storage, &*self.notifier, &msg, &user, &debt_id, &text,
                ).await?;
            }
            SessionState::AwaitingPaymentAmount { debt_id } => {
                debts::receive_payment(api, &*self.storage, &*self.notifier, &msg, &user, &debt_id, &text).await?;
            }
            SessionState::ManagingRoom { .. } => {
                send_main_menu(api, &*self.storage, chat_id).await?;
            }
        }
        Ok(())
    }

    async fn handle_contact(
        &self, api: &BotApi, msg: &Message,
        user: &crate::models::User,
        contact: &crate::telegram::types::TgContact,
    ) -> Result<()> {
        let session = self.storage.get_session(user.telegram_id).await?;
        let state = session.map(|s| s.state).unwrap_or(SessionState::Idle);

        if let SessionState::AwaitingContactShare { display_name } = state {
            return contacts::receive_shared_contact(
                api, &*self.storage, msg, user, &display_name, contact,
            ).await;
        }
        // Иначе считаем, что юзер делится своим телефоном (/start).
        start::handle_shared_contact(api, &*self.storage, msg, user, contact).await
    }

    async fn handle_callback(&self, api: &BotApi, cq: CallbackQuery) -> Result<()> {
        let (user, _) = upsert_from_tg(&*self.storage, &cq.from).await?;
        let chat_id = cq.message.as_ref().map(|m| m.chat.id).unwrap_or(user.telegram_id);
        let data = cq.data.clone().unwrap_or_default();

        // Текст для answer_callback_query — копится по ходу match, в конце один вызов.
        let mut ack_text: Option<String> = None;

        let parts: Vec<&str> = data.split(':').collect();
        match parts.as_slice() {
            ["noop"] => {}

            // ===== menu =====
            ["menu", "main"] => {
                self.storage.clear_session(user.telegram_id).await?;
                send_main_menu(api, &*self.storage, chat_id).await?;
            }
            ["menu", "contacts"] => {
                contacts::show_list(api, &*self.storage, chat_id, &user).await?;
            }
            ["menu", "rooms"] => {
                rooms::show_list(api, &*self.storage, chat_id, &user).await?;
            }
            ["menu", "debts"] => {
                debts::show_list(api, &*self.storage, chat_id, &user).await?;
            }
            ["menu", "help"] => {
                start::handle_help(api, &*self.storage, chat_id).await?;
            }

            // ===== contacts =====
            ["contact", "add"] => {
                contacts::start_add(api, &*self.storage, chat_id, &user).await?;
            }
            ["contact", "open", id] => {
                contacts::show_card(api, &*self.storage, chat_id, &user, id, &self.bot_username).await?;
            }
            ["contact", "del", id] => {
                contacts::delete(api, &*self.storage, chat_id, &user, id).await?;
                ack_text = Some("Удалено".into());
            }
            ["contact", "block", id] => {
                contacts::block(api, &*self.storage, chat_id, &user, id, &self.bot_username).await?;
                ack_text = Some("Заблокирован".into());
            }
            ["contact", "unblock", id] => {
                contacts::unblock(api, &*self.storage, chat_id, &user, id, &self.bot_username).await?;
                ack_text = Some("Разблокирован".into());
            }

            // ===== rooms =====
            ["room", "new"] => {
                rooms::start_new_room(api, &*self.storage, chat_id, &user).await?;
            }
            ["room", "cur", curr] => {
                let session = self.storage.get_session(user.telegram_id).await?;
                if let Some(SessionState::AwaitingRoomCurrency { name }) = session.map(|s| s.state) {
                    rooms::pick_currency(api, &*self.storage, chat_id, &user, &name, curr).await?;
                }
            }
            ["room", "open", id] => {
                rooms::show_room(api, &*self.storage, chat_id, &user, id).await?;
            }
            ["room", "add_item", id] => {
                rooms::start_add_item(api, &*self.storage, chat_id, &user, id).await?;
            }
            ["room", "members", id] => {
                rooms::show_members(api, &*self.storage, chat_id, &user, id).await?;
            }
            ["room", "addm", contact_id] => {
                let session = self.storage.get_session(user.telegram_id).await?;
                if let Some(SessionState::ManagingRoom { room_id }) = session.map(|s| s.state) {
                    rooms::add_member(api, &*self.storage, &*self.notifier, chat_id, &user, &room_id, contact_id).await?;
                    ack_text = Some("Добавлено".into());
                }
            }
            ["room", "settle", id] => {
                rooms::settle(api, &*self.storage, &*self.notifier, chat_id, &user, id).await?;
            }
            ["room", "settle_force", id] => {
                rooms::settle_force(api, &*self.storage, &*self.notifier, chat_id, &user, id).await?;
            }
            ["room", "archive", id] => {
                rooms::archive(api, &*self.storage, chat_id, &user, id).await?;
            }

            // ===== item =====
            ["item", "toggle", id] => {
                rooms::toggle_item(api, &*self.storage, &*self.notifier, chat_id, &user, id).await?;
            }

            // ===== debts =====
            ["debt", "new"] => {
                debts::start_new(api, &*self.storage, chat_id, &user).await?;
            }
            ["debt", "pickdebtor", uid] => {
                debts::pick_debtor(api, &*self.storage, chat_id, &user, uid).await?;
            }
            ["debt", "cur", curr] => {
                let session = self.storage.get_session(user.telegram_id).await?;
                if let Some(SessionState::AwaitingDebtCurrency { debtor_user_id, amount_minor_or_text }) =
                    session.map(|s| s.state)
                {
                    debts::pick_currency(
                        api, &*self.storage, chat_id, &user,
                        &debtor_user_id, &amount_minor_or_text, curr,
                    ).await?;
                }
            }
            ["debt", "open", id] => {
                debts::show_debt(api, &*self.storage, chat_id, &user, id).await?;
            }
            ["debt", "pay", id] => {
                debts::start_payment(api, &*self.storage, chat_id, &user, id).await?;
            }
            ["debt", "installments", id] => {
                debts::start_installments(api, &*self.storage, chat_id, &user, id).await?;
            }
            ["debt", "forgive", id] => {
                debts::forgive(api, &*self.storage, &*self.notifier, chat_id, &user, id).await?;
                ack_text = Some("Прощено".into());
            }

            _ => {
                warn!("неизвестный callback: {data}");
            }
        }

        // Всегда отвечаем на callback, чтобы убрать "часики".
        let _ = api.answer_callback_query(&cq.id, ack_text.as_deref()).await;
        Ok(())
    }
}

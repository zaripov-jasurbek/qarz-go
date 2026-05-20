//! Общие хелперы для хэндлеров: главное меню, форматирование, поиск юзера.

use crate::error::Result;
use crate::models::{Currency, User};
use crate::storage::Storage;
use crate::telegram::api::BotApi;
use crate::telegram::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, ReplyMarkup, TgUser,
};

/// Найти или создать User по telegram-пользователю из апдейта.
/// Возвращает (user, was_newly_created).
pub async fn upsert_from_tg<S: Storage>(
    storage: &S,
    tg: &TgUser,
) -> Result<(User, bool)> {
    if let Some(mut u) = storage.get_user_by_telegram_id(tg.id).await? {
        // Обновим поля, которые могут меняться.
        let mut dirty = false;
        if u.telegram_username != tg.username {
            u.telegram_username = tg.username.clone();
            dirty = true;
        }
        if u.first_name != tg.first_name {
            u.first_name = tg.first_name.clone();
            dirty = true;
        }
        if u.last_name != tg.last_name {
            u.last_name = tg.last_name.clone();
            dirty = true;
        }
        if dirty {
            storage.upsert_user(&u).await?;
        }
        Ok((u, false))
    } else {
        let mut u = User::new(tg.id, tg.first_name.clone());
        u.last_name = tg.last_name.clone();
        u.telegram_username = tg.username.clone();
        if let Some(lc) = &tg.language_code {
            if lc.starts_with("uz") { u.language = "uz".into(); }
            else if lc.starts_with("en") { u.language = "en".into(); }
        }
        storage.upsert_user(&u).await?;
        Ok((u, true))
    }
}

pub fn main_menu_markup() -> ReplyMarkup {
    ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![
                InlineKeyboardButton::callback("👥 Контакты", "menu:contacts"),
                InlineKeyboardButton::callback("🏠 Комнаты", "menu:rooms"),
            ],
            vec![
                InlineKeyboardButton::callback("💸 Долги", "menu:debts"),
                InlineKeyboardButton::callback("ℹ️ Помощь", "menu:help"),
            ],
        ],
    })
}

pub fn back_to_menu_button() -> InlineKeyboardButton {
    InlineKeyboardButton::callback("« Меню", "menu:main")
}

pub async fn send_main_menu<S: Storage>(api: &BotApi, _storage: &S, chat_id: i64) -> Result<()> {
    let text = "<b>Главное меню</b>\n\nВыбери раздел:";
    api.send_message(chat_id, text, Some(&main_menu_markup())).await?;
    Ok(())
}

pub fn currency_keyboard(prefix: &str) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![
            InlineKeyboardButton::callback("UZS", format!("{prefix}:UZS")),
            InlineKeyboardButton::callback("RUB", format!("{prefix}:RUB")),
            InlineKeyboardButton::callback("USD", format!("{prefix}:USD")),
            InlineKeyboardButton::callback("EUR", format!("{prefix}:EUR")),
        ]],
    }
}

pub fn parse_currency(s: &str) -> Option<Currency> {
    Currency::parse(s)
}

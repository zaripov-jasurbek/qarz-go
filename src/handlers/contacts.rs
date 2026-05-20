//! Управление контактами + блокировки.

use crate::error::Result;
use crate::handlers::common::{back_to_menu_button, send_main_menu};
use crate::models::{normalize_phone, Block, Contact, Invite, InvitePurpose, Session, SessionState, User};
use crate::storage::Storage;
use crate::telegram::api::BotApi;
use crate::telegram::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, Message, ReplyKeyboardMarkup,
    ReplyKeyboardRemove, ReplyMarkup, TgContact,
};

/// Показать список контактов в инлайн-меню.
pub async fn show_list<S: Storage>(api: &BotApi, storage: &S, chat_id: i64, user: &User) -> Result<()> {
    let contacts = storage.list_contacts(&user.id).await?;
    if contacts.is_empty() {
        let kb = InlineKeyboardMarkup {
            inline_keyboard: vec![
                vec![InlineKeyboardButton::callback("➕ Добавить", "contact:add")],
                vec![back_to_menu_button()],
            ],
        };
        api.send_message(
            chat_id,
            "<b>Контакты пустые</b>\n\nДобавьте друзей, чтобы потом включать их в комнаты.",
            Some(&ReplyMarkup::InlineKeyboard(kb)),
        ).await?;
        return Ok(());
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for c in &contacts {
        let mark = if c.linked_user_id.is_some() { "✅" } else { "⏳" };
        rows.push(vec![InlineKeyboardButton::callback(
            format!("{mark} {}", c.display_name),
            format!("contact:open:{}", c.id),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback("➕ Добавить", "contact:add")]);
    rows.push(vec![back_to_menu_button()]);

    let text = "<b>Ваши контакты</b>\n\n✅ — уже в боте, можно добавлять в комнаты\n⏳ — ещё не нажал /start";
    api.send_message(
        chat_id,
        text,
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

/// Карточка одного контакта.
pub async fn show_card<S: Storage>(
    api: &BotApi,
    storage: &S,
    chat_id: i64,
    user: &User,
    contact_id: &str,
    bot_username: &str,
) -> Result<()> {
    let Some(c) = storage.get_contact(contact_id).await? else {
        api.send_message(chat_id, "Контакт не найден.", None).await?;
        return Ok(());
    };
    if c.owner_user_id != user.id {
        api.send_message(chat_id, "Этот контакт не ваш.", None).await?;
        return Ok(());
    }

    let mut text = format!("<b>{}</b>\nТелефон: <code>{}</code>\n", c.display_name, c.phone);
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    match &c.linked_user_id {
        Some(linked_id) => {
            text.push_str("Статус: ✅ в боте\n");
            let blocked_by_them = storage.is_blocked(linked_id, &user.id).await?;
            let i_blocked = storage.is_blocked(&user.id, linked_id).await?;
            if blocked_by_them {
                text.push_str("⚠️ Этот человек заблокировал вас — нельзя добавлять в комнаты и долги.\n");
            }
            if i_blocked {
                rows.push(vec![InlineKeyboardButton::callback(
                    "🔓 Разблокировать", format!("contact:unblock:{}", c.id),
                )]);
            } else {
                rows.push(vec![InlineKeyboardButton::callback(
                    "🚫 Заблокировать", format!("contact:block:{}", c.id),
                )]);
            }
        }
        None => {
            text.push_str("Статус: ⏳ ещё не в боте\n\nОтправьте ему ссылку-приглашение:");
            // Создаём (или находим) инвайт для этого контакта.
            let token = ensure_invite(storage, user, &c.id).await?;
            let link = format!("https://t.me/{}?start={}", bot_username, token);
            text.push_str(&format!("\n<code>{}</code>", link));
            rows.push(vec![InlineKeyboardButton::link("📤 Поделиться приглашением", &link)]);
        }
    }
    rows.push(vec![InlineKeyboardButton::callback(
        "🗑 Удалить контакт", format!("contact:del:{}", c.id),
    )]);
    rows.push(vec![InlineKeyboardButton::callback("« К контактам", "menu:contacts")]);

    api.send_message(
        chat_id,
        &text,
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

async fn ensure_invite<S: Storage>(storage: &S, user: &User, contact_id: &str) -> Result<String> {
    // Простой подход: всегда создаём новый, на скорость хранилища пока пофиг.
    // Если файл разрастётся — добавим cache lookup.
    let inv = Invite::new(
        user.id.clone(),
        InvitePurpose::AddContact { contact_id: contact_id.to_string() },
    );
    let token = inv.token.clone();
    storage.create_invite(&inv).await?;
    Ok(token)
}

/// Начать добавление контакта — попросить имя.
pub async fn start_add<S: Storage>(api: &BotApi, storage: &S, msg_chat_id: i64, user: &User) -> Result<()> {
    storage.set_session(&Session::new(user.telegram_id, SessionState::AwaitingContactName)).await?;
    api.send_message(
        msg_chat_id,
        "Как назвать новый контакт? Просто напишите имя (например: <i>Брат</i>).",
        None,
    ).await?;
    Ok(())
}

/// Юзер прислал имя для контакта → просим поделиться.
pub async fn receive_name<S: Storage>(
    api: &BotApi,
    storage: &S,
    msg: &Message,
    user: &User,
    name: &str,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        api.send_message(msg.chat.id, "Имя не должно быть пустым.", None).await?;
        return Ok(());
    }
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingContactShare { display_name: name.to_string() },
    )).await?;

    let kb = ReplyMarkup::ReplyKeyboard(ReplyKeyboardMarkup {
        keyboard: vec![vec![KeyboardButton::request_contact("📞 Поделиться контактом друга")]],
        resize_keyboard: Some(true),
        one_time_keyboard: Some(true),
    });
    api.send_message(
        msg.chat.id,
        &format!("Окей, контакт назвал <b>{name}</b>.\nТеперь поделитесь его телефоном (кнопка ниже)."),
        Some(&kb),
    ).await?;
    Ok(())
}

/// Юзер поделился контактом друга → сохраняем.
pub async fn receive_shared_contact<S: Storage>(
    api: &BotApi,
    storage: &S,
    msg: &Message,
    user: &User,
    display_name: &str,
    tg_contact: &TgContact,
) -> Result<()> {
    let phone = normalize_phone(&tg_contact.phone_number);
    if phone.is_empty() {
        api.send_message(msg.chat.id, "Не удалось прочитать номер.", None).await?;
        return Ok(());
    }
    let mut c = Contact::new(user.id.clone(), display_name.to_string(), phone.clone());

    // Если этот phone уже привязан к существующему юзеру — сразу линкуем.
    if let Some(linked) = storage.find_users_by_phone(&phone).await?.into_iter().next() {
        c.linked_user_id = Some(linked.id);
    }
    storage.add_contact(&c).await?;
    storage.clear_session(user.telegram_id).await?;

    let remove_kb = ReplyMarkup::Remove(ReplyKeyboardRemove { remove_keyboard: true });
    let status = if c.linked_user_id.is_some() {
        "✅ Этот человек уже в боте — можно сразу добавлять в комнаты."
    } else {
        "⏳ Этот человек ещё не в боте. Откройте карточку контакта, чтобы получить ссылку-приглашение."
    };
    api.send_message(
        msg.chat.id,
        &format!("Контакт <b>{display_name}</b> сохранён.\n\n{status}"),
        Some(&remove_kb),
    ).await?;
    send_main_menu(api, storage, msg.chat.id).await?;
    Ok(())
}

pub async fn block<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, contact_id: &str, bot_username: &str,
) -> Result<()> {
    if let Some(c) = storage.get_contact(contact_id).await? {
        if c.owner_user_id == user.id {
            if let Some(linked) = c.linked_user_id.clone() {
                storage.add_block(&Block::new(user.id.clone(), linked)).await?;
            }
        }
    }
    show_card(api, storage, chat_id, user, contact_id, bot_username).await
}

pub async fn unblock<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, contact_id: &str, bot_username: &str,
) -> Result<()> {
    if let Some(c) = storage.get_contact(contact_id).await? {
        if c.owner_user_id == user.id {
            if let Some(linked) = c.linked_user_id.clone() {
                storage.remove_block(&user.id, &linked).await?;
            }
        }
    }
    show_card(api, storage, chat_id, user, contact_id, bot_username).await
}

pub async fn delete<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, contact_id: &str,
) -> Result<()> {
    if let Some(c) = storage.get_contact(contact_id).await? {
        if c.owner_user_id == user.id {
            storage.delete_contact(contact_id).await?;
        }
    }
    show_list(api, storage, chat_id, user).await
}

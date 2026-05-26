//! Комнаты — общие покупки.

use crate::error::Result;
use crate::handlers::common::{back_to_menu_button, currency_keyboard, parse_currency};
use crate::models::{
    Debt, DebtSource, Money, Room, RoomItem, RoomStatus, Session, SessionState, User,
};

use crate::services::{split_item, Notifier};
use crate::storage::Storage;
use crate::telegram::api::BotApi;
use crate::telegram::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, Message, ReplyMarkup,
};
use std::collections::HashMap;

pub async fn show_list<S: Storage>(api: &BotApi, storage: &S, chat_id: i64, user: &User) -> Result<()> {
    let mut rooms = storage.list_rooms_for_user(&user.id).await?;
    rooms.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for r in &rooms {
        let badge = match r.status {
            RoomStatus::Collecting => "🟢",
            RoomStatus::Locked => "🟡",
            RoomStatus::Settled => "✅",
            RoomStatus::Archived => "📦",
        };
        rows.push(vec![InlineKeyboardButton::callback(
            format!("{badge} {}", r.name),
            format!("room:open:{}", r.id),
        )]);
    }
    rows.push(vec![InlineKeyboardButton::callback("➕ Новая комната", "room:new")]);
    rows.push(vec![back_to_menu_button()]);

    let text = if rooms.is_empty() {
        "<b>Комнат пока нет.</b>\n\nКомната — это общая покупка, где участники делят расходы."
    } else {
        "<b>Ваши комнаты</b>"
    };
    api.send_message(
        chat_id, text,
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

pub async fn show_room<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, room_id: &str,
) -> Result<()> {
    let Some(room) = storage.get_room(room_id).await? else {
        api.send_message(chat_id, "Комната не найдена.", None).await?;
        return Ok(());
    };
    if !room.member_user_ids.iter().any(|id| id == &user.id) {
        api.send_message(chat_id, "Вы не участник этой комнаты.", None).await?;
        return Ok(());
    }

    let items = storage.list_items_in_room(&room.id).await?;
    let is_creator = room.creator_user_id == user.id;
    let mut text = format!("<b>🏠 {}</b>\nСтатус: {}\n", room.name, status_text(room.status));

    // Предзагрузим имена всех участников один раз.
    let mut names: HashMap<String, String> = HashMap::new();
    for mid in &room.member_user_ids {
        if let Some(u) = storage.get_user(mid).await? {
            names.insert(mid.clone(), u.first_name);
        }
    }

    // Список позиций — кто что взял
    if items.is_empty() {
        text.push_str("\n<i>Позиций пока нет.</i>");
    } else {
        text.push_str("\n<b>Позиции:</b>");
        for it in &items {
            let price = Money::new(it.total_price_minor, room.currency).format();
            let taken_names: Vec<String> = it.selected_by.iter().take(3)
                .map(|id| names.get(id).cloned().unwrap_or_else(|| id.clone()))
                .collect();
            let mut taken = taken_names.join(", ");
            if it.selected_by.len() > 3 {
                taken.push_str(&format!(" +{}", it.selected_by.len() - 3));
            }
            let taken_part = if taken.is_empty() { "—".to_string() } else { taken };
            text.push_str(&format!("\n• <b>{}</b> · {} · {}", it.name, price, taken_part));
        }
    }

    // Если активная — показываем кнопки выбора позиций для текущего юзера
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    if room.status == RoomStatus::Collecting {
        for it in &items {
            let mark = if it.selected_by.iter().any(|id| id == &user.id) { "✅" } else { "⬜️" };
            rows.push(vec![InlineKeyboardButton::callback(
                format!("{mark} {}", it.name),
                format!("item:toggle:{}", it.id),
            )]);
        }
    }

    // Кнопки управления для создателя
    let mut admin_row: Vec<InlineKeyboardButton> = Vec::new();
    if is_creator {
        match room.status {
            RoomStatus::Collecting => {
                admin_row.push(InlineKeyboardButton::callback("➕ Позиция", format!("room:add_item:{}", room.id)));
                admin_row.push(InlineKeyboardButton::callback("👥 Участники", format!("room:members:{}", room.id)));
                rows.push(admin_row);
                rows.push(vec![InlineKeyboardButton::callback("🔒 Закрыть и посчитать", format!("room:settle:{}", room.id))]);
            }
            RoomStatus::Locked | RoomStatus::Settled => {
                rows.push(vec![InlineKeyboardButton::callback("📦 Архивировать", format!("room:archive:{}", room.id))]);
            }
            RoomStatus::Archived => {}
        }
    }
    rows.push(vec![InlineKeyboardButton::callback("« К комнатам", "menu:rooms")]);

    api.send_message(
        chat_id, &text,
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

fn status_text(s: RoomStatus) -> &'static str {
    match s {
        RoomStatus::Collecting => "🟢 идёт сбор",
        RoomStatus::Locked => "🟡 закрыта",
        RoomStatus::Settled => "✅ долги созданы",
        RoomStatus::Archived => "📦 в архиве",
    }
}

pub async fn start_new_room<S: Storage>(api: &BotApi, storage: &S, chat_id: i64, user: &User) -> Result<()> {
    storage.set_session(&Session::new(user.telegram_id, SessionState::AwaitingRoomName)).await?;
    api.send_message(chat_id, "Как назвать комнату? (например: <i>Магнум 20 мая</i>)", None).await?;
    Ok(())
}

pub async fn receive_room_name<S: Storage>(
    api: &BotApi, storage: &S, msg: &Message, user: &User, name: &str,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        api.send_message(msg.chat.id, "Имя не должно быть пустым.", None).await?;
        return Ok(());
    }
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingRoomCurrency { name: name.to_string() },
    )).await?;
    api.send_message(
        msg.chat.id,
        &format!("Окей, <b>{name}</b>.\nВыберите валюту:"),
        Some(&ReplyMarkup::InlineKeyboard(currency_keyboard("room:cur"))),
    ).await?;
    Ok(())
}

pub async fn pick_currency<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, name: &str, currency_code: &str,
) -> Result<()> {
    let Some(currency) = parse_currency(currency_code) else {
        api.send_message(chat_id, "Неизвестная валюта.", None).await?;
        return Ok(());
    };
    let room = Room::new(name.to_string(), user.id.clone(), currency);
    storage.create_room(&room).await?;
    storage.clear_session(user.telegram_id).await?;
    api.send_message(
        chat_id,
        &format!("Создана комната <b>{}</b> в {}.\nДобавьте позиции и участников.", room.name, currency.symbol()),
        None,
    ).await?;
    show_room(api, storage, chat_id, user, &room.id).await
}

pub async fn start_add_item<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, room_id: &str,
) -> Result<()> {
    let Some(room) = storage.get_room(room_id).await? else { return Ok(()); };
    if room.creator_user_id != user.id || room.status != RoomStatus::Collecting {
        return Ok(());
    }
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingItemName { room_id: room_id.to_string() },
    )).await?;
    api.send_message(chat_id, "Название позиции? (например: <i>Хлеб</i>)", None).await?;
    Ok(())
}

pub async fn receive_item_name<S: Storage>(
    api: &BotApi, storage: &S, msg: &Message, user: &User, room_id: &str, item_name: &str,
) -> Result<()> {
    let item_name = item_name.trim();
    if item_name.is_empty() {
        api.send_message(msg.chat.id, "Название не должно быть пустым.", None).await?;
        return Ok(());
    }
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingItemPrice {
            room_id: room_id.to_string(),
            item_name: item_name.to_string(),
        },
    )).await?;
    api.send_message(msg.chat.id, "Цена за позицию? (например: <code>15000</code>)", None).await?;
    Ok(())
}

pub async fn receive_item_price<S: Storage>(
    api: &BotApi, storage: &S, msg: &Message, user: &User,
    room_id: &str, item_name: &str, price_text: &str,
) -> Result<()> {
    let Some(room) = storage.get_room(room_id).await? else { return Ok(()); };
    let Some(money) = Money::parse(price_text, room.currency) else {
        api.send_message(msg.chat.id, "Не понял цену. Попробуйте ещё раз — например: <code>15000</code>", None).await?;
        return Ok(());
    };
    let item = RoomItem::new(room.id.clone(), item_name.to_string(), money.amount_minor);
    storage.add_item(&item).await?;
    storage.clear_session(user.telegram_id).await?;
    api.send_message(
        msg.chat.id,
        &format!("✅ <b>{}</b> — {} добавлена.", item.name, money.format()),
        None,
    ).await?;
    show_room(api, storage, msg.chat.id, user, &room.id).await
}

/// Юзер тыкнул "взял/не взял" в позиции.
pub async fn toggle_item<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>, chat_id: i64, user: &User, item_id: &str,
) -> Result<()> {
    let Some(mut item) = storage.get_item(item_id).await? else { return Ok(()); };
    let Some(room) = storage.get_room(&item.room_id).await? else { return Ok(()); };
    if room.status != RoomStatus::Collecting { return Ok(()); }
    if !room.member_user_ids.iter().any(|id| id == &user.id) { return Ok(()); }

    if let Some(pos) = item.selected_by.iter().position(|id| id == &user.id) {
        item.selected_by.remove(pos);
    } else {
        item.selected_by.push(user.id.clone());
    }
    storage.update_item(&item).await?;
    let _ = notifier; // на будущее: notify creator о изменении
    show_room(api, storage, chat_id, user, &room.id).await
}

pub async fn show_members<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, room_id: &str,
) -> Result<()> {
    let Some(room) = storage.get_room(room_id).await? else { return Ok(()); };
    if room.creator_user_id != user.id { return Ok(()); }
    // Сохраняем room_id в сессию — add_member кнопки будут передавать только contact_id.
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::ManagingRoom { room_id: room_id.to_string() },
    )).await?;

    // Список контактов с возможностью добавить в комнату
    let contacts = storage.list_contacts(&user.id).await?;
    let mut text = format!("<b>Участники «{}»:</b>\n", room.name);
    for mid in &room.member_user_ids {
        if mid == &user.id {
            text.push_str(&format!("• {} (вы)\n", user.first_name));
            continue;
        }
        if let Some(u) = storage.get_user(mid).await? {
            text.push_str(&format!("• {}\n", u.display_name()));
        }
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    text.push_str("\n<b>Добавить из контактов:</b>");
    let mut any_addable = false;
    for c in &contacts {
        let Some(linked) = &c.linked_user_id else { continue; };
        if room.member_user_ids.iter().any(|id| id == linked) { continue; }
        // Проверим взаимный блок
        let blocked = storage.is_blocked(linked, &user.id).await?;
        if blocked {
            rows.push(vec![InlineKeyboardButton::callback(
                format!("🚫 {} (заблокировал вас)", c.display_name),
                "noop",
            )]);
            any_addable = true;
            continue;
        }
        rows.push(vec![InlineKeyboardButton::callback(
            format!("➕ {}", c.display_name),
            format!("room:addm:{}", c.id),  // room_id хранится в сессии
        )]);
        any_addable = true;
    }
    if !any_addable {
        text.push_str("\n<i>Нет доступных контактов. Сначала добавьте кого-то в /menu → Контакты.</i>");
    }
    rows.push(vec![InlineKeyboardButton::callback("« К комнате", format!("room:open:{}", room.id))]);

    api.send_message(
        chat_id, &text,
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

pub async fn add_member<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>,
    chat_id: i64, user: &User, room_id: &str, contact_id: &str,
) -> Result<()> {
    let Some(mut room) = storage.get_room(room_id).await? else { return Ok(()); };
    if room.creator_user_id != user.id { return Ok(()); }
    let Some(c) = storage.get_contact(contact_id).await? else { return Ok(()); };
    let Some(linked) = c.linked_user_id.clone() else {
        api.send_message(chat_id, "Этот человек ещё не нажал /start у бота — сначала отправьте ему приглашение.", None).await?;
        return Ok(());
    };
    if storage.is_blocked(&linked, &user.id).await? {
        api.send_message(chat_id, "Этот человек заблокировал вас, добавить нельзя.", None).await?;
        return Ok(());
    }
    if !room.member_user_ids.iter().any(|id| id == &linked) {
        room.member_user_ids.push(linked.clone());
        storage.update_room(&room).await?;
    }
    // Уведомим нового участника
    let _ = notifier.send_from(
        &user.id, &linked,
        &format!("{} добавил вас в комнату <b>{}</b>.\nОткройте /menu → Комнаты, чтобы отметить, что вы взяли.", user.display_name(), room.name),
        None,
    ).await;
    show_members(api, storage, chat_id, user, room_id).await
}

/// Закрыть комнату, посчитать доли и создать долги.
pub async fn settle<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>,
    chat_id: i64, user: &User, room_id: &str,
) -> Result<()> {
    let Some(mut room) = storage.get_room(room_id).await? else { return Ok(()); };
    if room.creator_user_id != user.id { return Ok(()); }
    if room.status != RoomStatus::Collecting { return Ok(()); }

    let items = storage.list_items_in_room(&room.id).await?;
    if items.is_empty() {
        api.send_message(chat_id, "Нет позиций — нечего считать.", None).await?;
        return Ok(());
    }
    let unselected: Vec<&RoomItem> = items.iter().filter(|i| i.selected_by.is_empty()).collect();
    if !unselected.is_empty() {
        let mut t = String::from("⚠️ Эти позиции никто не выбрал:\n");
        for it in &unselected {
            t.push_str(&format!("• {}\n", it.name));
        }
        t.push_str("\nЕсли никто не отметится, эти позиции будут на создателе. Закрыть всё равно?");
        let kb = InlineKeyboardMarkup { inline_keyboard: vec![
            vec![InlineKeyboardButton::callback("✅ Да, закрыть", format!("room:settle_force:{}", room.id))],
            vec![InlineKeyboardButton::callback("« Отмена", format!("room:open:{}", room.id))],
        ]};
        api.send_message(chat_id, &t, Some(&ReplyMarkup::InlineKeyboard(kb))).await?;
        return Ok(());
    }
    do_settle(api, storage, notifier, chat_id, user, &mut room, &items).await
}

pub async fn settle_force<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>,
    chat_id: i64, user: &User, room_id: &str,
) -> Result<()> {
    let Some(mut room) = storage.get_room(room_id).await? else { return Ok(()); };
    if room.creator_user_id != user.id { return Ok(()); }
    if room.status != RoomStatus::Collecting { return Ok(()); }
    let mut items = storage.list_items_in_room(&room.id).await?;
    // Невыбранные позиции вешаем на создателя
    for it in items.iter_mut() {
        if it.selected_by.is_empty() {
            it.selected_by.push(user.id.clone());
            storage.update_item(it).await?;
        }
    }
    do_settle(api, storage, notifier, chat_id, user, &mut room, &items).await
}

async fn do_settle<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>,
    chat_id: i64, user: &User, room: &mut Room, items: &[RoomItem],
) -> Result<()> {
    // Платит всегда создатель. Каждый участник должен creator-у свою долю.
    let mut owes: HashMap<String, i64> = HashMap::new();
    for it in items {
        for share in split_item(it) {
            if share.user_id == user.id { continue; }
            *owes.entry(share.user_id).or_insert(0) += share.amount_minor;
        }
    }

    let mut summary = format!("<b>Расчёт по «{}»</b>\n\n", room.name);
    if owes.is_empty() {
        summary.push_str("Никто никому не должен — все позиции взял создатель.\n");
    } else {
        for (debtor_id, amount) in &owes {
            let m = Money::new(*amount, room.currency);
            let name = storage.get_user(debtor_id).await?
                .map(|u| u.display_name())
                .unwrap_or_else(|| debtor_id.clone());
            summary.push_str(&format!("• <b>{name}</b> → вам {}\n", m.format()));

            let debt = Debt::new(
                debtor_id.clone(),
                user.id.clone(),
                *amount,
                room.currency,
                format!("Комната «{}»", room.name),
                DebtSource::FromRoom { room_id: room.id.clone() },
            );
            storage.create_debt(&debt).await?;

            // Уведомляем должника
            let _ = notifier.send_from(
                &user.id, debtor_id,
                &format!("💸 По комнате <b>{}</b> вы должны {} пользователю {}.", room.name, m.format(), user.display_name()),
                None,
            ).await;
        }
    }

    room.status = RoomStatus::Settled;
    room.settled_at = Some(chrono::Utc::now());
    storage.update_room(room).await?;

    api.send_message(chat_id, &summary, None).await?;
    show_room(api, storage, chat_id, user, &room.id).await
}

pub async fn archive<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, room_id: &str,
) -> Result<()> {
    let Some(mut room) = storage.get_room(room_id).await? else { return Ok(()); };
    if room.creator_user_id != user.id { return Ok(()); }
    room.status = RoomStatus::Archived;
    storage.update_room(&room).await?;
    show_list(api, storage, chat_id, user).await
}


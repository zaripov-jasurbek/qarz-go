//! /start, /help, обработка shared contact, deep-link инвайтов.

use crate::error::Result;
use crate::handlers::common::{main_menu_markup, send_main_menu};
use crate::models::{normalize_phone, Contact, InvitePurpose, User};
use crate::storage::Storage;
use crate::telegram::api::BotApi;
use crate::telegram::types::{
    KeyboardButton, Message, ReplyKeyboardMarkup, ReplyKeyboardRemove, ReplyMarkup, TgContact,
};

/// `/start [token]` — приветствие. Если есть `token`, это deep-link инвайт.
pub async fn handle_start<S: Storage>(
    api: &BotApi,
    storage: &S,
    msg: &Message,
    user: &User,
    is_new: bool,
    token: Option<&str>,
) -> Result<()> {
    let chat_id = msg.chat.id;

    if is_new {
        // Просим поделиться контактом, чтобы бот знал phone и мог связать с
        // адресными книгами других юзеров.
        let prompt = "Привет! Я помогаю делить расходы и считать долги между друзьями.\n\n\
                      Поделитесь контактом — это нужно чтобы вас могли добавить в \
                      комнаты по номеру телефона.";
        let kb = ReplyMarkup::ReplyKeyboard(ReplyKeyboardMarkup {
            keyboard: vec![vec![KeyboardButton::request_contact("📞 Поделиться контактом")]],
            resize_keyboard: Some(true),
            one_time_keyboard: Some(true),
        });
        api.send_message(chat_id, prompt, Some(&kb)).await?;
    }

    if let Some(t) = token {
        consume_invite(storage, user, t).await?;
        api.send_message(
            chat_id,
            "✅ Приглашение принято — теперь приглашающий видит вас в своих контактах.",
            None,
        ).await?;
    }

    if !is_new {
        send_main_menu(api, storage, chat_id).await?;
    }
    Ok(())
}

pub async fn handle_help<S: Storage>(api: &BotApi, _storage: &S, chat_id: i64) -> Result<()> {
    let text = "<b>Что я умею:</b>\n\n\
        • 👥 <b>Контакты</b> — личная адресная книга. Добавь друзей, чтобы потом \
        включать их в комнаты и записывать долги.\n\
        • 🏠 <b>Комнаты</b> — общая покупка. Создатель пишет список покупок и цены, \
        участники отмечают что они взяли, бот считает доли.\n\
        • 💸 <b>Долги</b> — учёт кто кому сколько должен. Можно с рассрочкой.\n\
        • 🚫 <b>Блок</b> — если не хочешь иметь дело с кем-то, заблокируй его \
        в карточке контакта.\n\n\
        Команды: /start, /menu, /help";
    api.send_message(chat_id, text, Some(&main_menu_markup())).await?;
    Ok(())
}

/// Юзер нажал "поделиться контактом". Сохраняем phone и линкуем найденные
/// записи в чужих адресных книгах.
pub async fn handle_shared_contact<S: Storage>(
    api: &BotApi,
    storage: &S,
    msg: &Message,
    user: &User,
    contact: &TgContact,
) -> Result<()> {
    // Проверка: юзер должен делиться СВОИМ контактом, а не чужим.
    if contact.user_id != Some(user.telegram_id) {
        api.send_message(
            msg.chat.id,
            "Пожалуйста, поделитесь именно своим контактом (кнопкой ниже).",
            None,
        ).await?;
        return Ok(());
    }

    let phone = normalize_phone(&contact.phone_number);
    let mut u = user.clone();
    u.phone = Some(phone.clone());
    storage.upsert_user(&u).await?;

    // Линкуем все контакты в чужих книгах с этим телефоном,
    // и создаём обратный контакт у каждого владельца (чтобы и они были видны нам).
    let mut linked = 0usize;
    for c in storage.find_contacts_by_phone(&phone).await? {
        if c.linked_user_id.is_none() {
            storage.link_contact(&c.id, &u.id).await?;
            linked += 1;
        }
        // Создаём обратный контакт: владелец книги → у нас в контактах.
        if let Some(owner) = storage.get_user(&c.owner_user_id).await? {
            if let Some(owner_phone) = owner.phone.clone() {
                let already = storage.find_contacts_by_phone(&owner_phone).await?
                    .into_iter().any(|cx| cx.owner_user_id == u.id);
                if !already {
                    let mut rev = Contact::new(
                        u.id.clone(),
                        owner.display_name(),
                        owner_phone,
                    );
                    rev.linked_user_id = Some(owner.id.clone());
                    storage.add_contact(&rev).await?;
                }
            }
        }
    }

    let remove_kb = ReplyMarkup::Remove(ReplyKeyboardRemove { remove_keyboard: true });
    let text = if linked > 0 {
        format!(
            "✅ Готово! Вас уже добавили в адресную книгу у {linked} {}.\n\n\
             Теперь они могут включать вас в комнаты и записывать долги.",
            ru_people(linked)
        )
    } else {
        "✅ Готово! Номер сохранён.".to_string()
    };
    api.send_message(msg.chat.id, &text, Some(&remove_kb)).await?;
    send_main_menu(api, storage, msg.chat.id).await?;
    Ok(())
}

/// Обработать deep-link инвайт.
async fn consume_invite<S: Storage>(storage: &S, user: &User, token: &str) -> Result<()> {
    let Some(invite) = storage.get_invite(token).await? else {
        return Ok(()); // токен невалидный — просто игнорируем, юзер всё равно увидел старт
    };
    if invite.used_by_user_id.is_some() {
        return Ok(());
    }
    if invite.created_by_user_id == user.id {
        return Ok(()); // самоприглашение
    }
    match &invite.purpose {
        InvitePurpose::AddContact { contact_id } => {
            // 1. Линкуем контакт отправителя → теперь он видит друга как ✅
            storage.link_contact(contact_id, &user.id).await?;
            storage.mark_invite_used(token, &user.id).await?;

            // 2. Создаём обратный контакт: друг теперь тоже видит отправителя.
            if let Some(inviter) = storage.get_user(&invite.created_by_user_id).await? {
                if let Some(phone) = inviter.phone.clone() {
                    // Проверяем, нет ли уже такого контакта у друга.
                    let already = storage.find_contacts_by_phone(&phone).await?
                        .into_iter().any(|c| c.owner_user_id == user.id);
                    if !already {
                        let mut rev = Contact::new(
                            user.id.clone(),
                            inviter.display_name(),
                            phone,
                        );
                        rev.linked_user_id = Some(inviter.id.clone());
                        storage.add_contact(&rev).await?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn ru_people(n: usize) -> &'static str {
    let n100 = n % 100;
    let n10 = n % 10;
    // "у 1 человека" vs "у 2 человек" — единственное число только в случае,
    // когда последняя цифра 1, но не 11.
    if n10 == 1 && !(11..=14).contains(&n100) { "человека" } else { "человек" }
}

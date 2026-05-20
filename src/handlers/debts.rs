//! Долги: создание вручную, просмотр, погашение.

use crate::error::Result;
use crate::handlers::common::{back_to_menu_button, currency_keyboard, parse_currency};
use crate::models::{
    Currency, Debt, DebtSource, DebtStatus, Money, Payment, Session, SessionState, User,
};
use crate::services::clock::fmt_date_local;
use crate::services::{build_installments, parse_plan, Notifier};
use crate::storage::Storage;
use crate::telegram::api::BotApi;
use crate::telegram::types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, ReplyMarkup};

pub async fn show_list<S: Storage>(api: &BotApi, storage: &S, chat_id: i64, user: &User) -> Result<()> {
    let mut debts = storage.list_debts_for_user(&user.id).await?;
    debts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let mut text = String::from("<b>💸 Долги</b>\n");
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    let mut owed_to_me: i64 = 0;
    let mut owed_by_me: i64 = 0;
    // Считаем по-валютно — но в UI пока просто список.

    for d in &debts {
        let me_debtor = d.debtor_user_id == user.id;
        let counterpart_id = if me_debtor { &d.creditor_user_id } else { &d.debtor_user_id };
        let counterpart_name = storage.get_user(counterpart_id).await?
            .map(|u| u.display_name())
            .unwrap_or_else(|| counterpart_id.clone());
        let remaining = Money::new(d.remaining_minor(), d.currency).format();
        let arrow = if me_debtor { "→" } else { "←" };
        let status_emoji = match d.status {
            DebtStatus::Confirmed => "🟠",
            DebtStatus::Settled => "✅",
            DebtStatus::Disputed => "⚠️",
            DebtStatus::Forgiven => "🕊",
        };
        let label = format!("{status_emoji} {arrow} {counterpart_name} · {remaining}");
        rows.push(vec![InlineKeyboardButton::callback(label, format!("debt:open:{}", d.id))]);
        if d.status == DebtStatus::Confirmed {
            if me_debtor { owed_by_me += d.remaining_minor(); }
            else { owed_to_me += d.remaining_minor(); }
        }
    }

    if owed_to_me > 0 || owed_by_me > 0 {
        // Чисто информативно, без учёта валюты (упрощение).
        text.push_str(&format!(
            "\n<i>Активных долгов:</i>\nВам должны: {}\nВы должны: {}\n",
            owed_to_me, owed_by_me
        ));
    }
    if debts.is_empty() {
        text.push_str("\n<i>Долгов пока нет.</i>");
    }

    rows.push(vec![InlineKeyboardButton::callback("➕ Новый долг", "debt:new")]);
    rows.push(vec![back_to_menu_button()]);

    api.send_message(
        chat_id, &text,
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

pub async fn show_debt<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, debt_id: &str,
) -> Result<()> {
    let Some(d) = storage.get_debt(debt_id).await? else {
        api.send_message(chat_id, "Долг не найден.", None).await?;
        return Ok(());
    };
    let me_debtor = d.debtor_user_id == user.id;
    let counterpart_id = if me_debtor { &d.creditor_user_id } else { &d.debtor_user_id };
    let counterpart = storage.get_user(counterpart_id).await?
        .map(|u| u.display_name())
        .unwrap_or_else(|| counterpart_id.clone());

    let original = Money::new(d.original_amount_minor, d.currency).format();
    let paid = Money::new(d.total_paid_minor(), d.currency).format();
    let remaining = Money::new(d.remaining_minor(), d.currency).format();

    let mut text = format!(
        "<b>Долг</b>\n\n\
         {direction} <b>{counterpart}</b>\n\
         Сумма: {original}\nОплачено: {paid}\nОстаток: <b>{remaining}</b>\n\
         {desc}\n",
        direction = if me_debtor { "Вы должны" } else { "Вам должен" },
        desc = if d.description.is_empty() { String::new() } else { format!("\n<i>{}</i>", d.description) },
    );

    if !d.installments.is_empty() {
        text.push_str("\n<b>График:</b>\n");
        for (i, inst) in d.installments.iter().enumerate() {
            let m = Money::new(inst.amount_minor, d.currency).format();
            text.push_str(&format!(
                "{} {}. {} — {}\n",
                if inst.paid { "✅" } else { "⏳" },
                i + 1,
                fmt_date_local(inst.due_date),
                m,
            ));
        }
    }

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    if d.status == DebtStatus::Confirmed && me_debtor && d.remaining_minor() > 0 {
        rows.push(vec![InlineKeyboardButton::callback("💵 Я оплатил часть", format!("debt:pay:{}", d.id))]);
    }
    if d.status == DebtStatus::Confirmed && !me_debtor && d.remaining_minor() > 0 {
        // Рассрочку можно завести только пока никто ещё не платил и графика нет.
        if d.installments.is_empty() && d.payments.is_empty() {
            rows.push(vec![InlineKeyboardButton::callback("📅 Сделать рассрочкой", format!("debt:installments:{}", d.id))]);
        }
        rows.push(vec![InlineKeyboardButton::callback("🕊 Простить остаток", format!("debt:forgive:{}", d.id))]);
    }
    rows.push(vec![InlineKeyboardButton::callback("« К долгам", "menu:debts")]);

    api.send_message(
        chat_id, &text,
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

/// Начало создания нового долга — выбрать должника из контактов.
pub async fn start_new<S: Storage>(api: &BotApi, storage: &S, chat_id: i64, user: &User) -> Result<()> {
    let contacts = storage.list_contacts(&user.id).await?;
    let linked: Vec<_> = contacts.into_iter()
        .filter(|c| c.linked_user_id.is_some())
        .collect();

    if linked.is_empty() {
        api.send_message(
            chat_id,
            "Нет контактов, на которых можно повесить долг. Сначала добавьте кого-то в /menu → Контакты.",
            None,
        ).await?;
        return Ok(());
    }

    storage.set_session(&Session::new(user.telegram_id, SessionState::AwaitingDebtorPick)).await?;

    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for c in &linked {
        let linked_id = c.linked_user_id.clone().unwrap();
        let blocked = storage.is_blocked(&linked_id, &user.id).await?;
        let label = if blocked {
            format!("🚫 {} (заблокировал вас)", c.display_name)
        } else {
            format!("👤 {}", c.display_name)
        };
        let data = if blocked { "noop".to_string() } else { format!("debt:pickdebtor:{}", linked_id) };
        rows.push(vec![InlineKeyboardButton::callback(label, data)]);
    }
    rows.push(vec![InlineKeyboardButton::callback("« Отмена", "menu:debts")]);

    api.send_message(
        chat_id,
        "<b>Кто должен?</b> Выберите из контактов:",
        Some(&ReplyMarkup::InlineKeyboard(InlineKeyboardMarkup { inline_keyboard: rows })),
    ).await?;
    Ok(())
}

pub async fn pick_debtor<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, debtor_user_id: &str,
) -> Result<()> {
    if storage.is_blocked(debtor_user_id, &user.id).await? {
        api.send_message(chat_id, "Этот человек заблокировал вас.", None).await?;
        return Ok(());
    }
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingDebtAmount { debtor_user_id: debtor_user_id.to_string() },
    )).await?;
    api.send_message(chat_id, "Введите сумму (например: <code>50000</code>):", None).await?;
    Ok(())
}

pub async fn receive_amount<S: Storage>(
    api: &BotApi, storage: &S, msg: &Message, user: &User,
    debtor_user_id: &str, amount_text: &str,
) -> Result<()> {
    // Сумма парсится позже, когда узнаем валюту. Сохраняем как текст.
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingDebtCurrency {
            debtor_user_id: debtor_user_id.to_string(),
            amount_minor_or_text: amount_text.trim().to_string(),
        },
    )).await?;
    api.send_message(
        msg.chat.id, "Валюта?",
        Some(&ReplyMarkup::InlineKeyboard(currency_keyboard("debt:cur"))),
    ).await?;
    Ok(())
}

pub async fn pick_currency<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User,
    debtor_user_id: &str, amount_text: &str, currency_code: &str,
) -> Result<()> {
    let Some(currency) = parse_currency(currency_code) else {
        api.send_message(chat_id, "Неизвестная валюта.", None).await?;
        return Ok(());
    };
    let Some(money) = Money::parse(amount_text, currency) else {
        api.send_message(chat_id, "Не понял сумму, начнём заново. /menu", None).await?;
        storage.clear_session(user.telegram_id).await?;
        return Ok(());
    };
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingDebtDescription {
            debtor_user_id: debtor_user_id.to_string(),
            amount_minor: money.amount_minor,
            currency,
        },
    )).await?;
    api.send_message(
        chat_id,
        &format!("Сумма: <b>{}</b>. Опишите за что (одной строкой). Или отправьте «-» чтобы оставить пустым.", money.format()),
        None,
    ).await?;
    Ok(())
}

pub async fn receive_description<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>, msg: &Message, user: &User,
    debtor_user_id: &str, amount_minor: i64, currency: Currency, description: &str,
) -> Result<()> {
    let desc = if description.trim() == "-" { String::new() } else { description.trim().to_string() };
    let debt = Debt::new(
        debtor_user_id.to_string(),
        user.id.clone(),
        amount_minor,
        currency,
        desc,
        DebtSource::Manual,
    );
    storage.create_debt(&debt).await?;
    storage.clear_session(user.telegram_id).await?;

    let m = Money::new(amount_minor, currency).format();
    api.send_message(msg.chat.id, &format!("✅ Долг создан: {m}"), None).await?;

    // Уведомляем должника
    let _ = notifier.send_from(
        &user.id, debtor_user_id,
        &format!("💸 {} записал на вас долг {}.\nПодробности: /menu → Долги.", user.display_name(), m),
        None,
    ).await;

    show_list(api, storage, msg.chat.id, user).await
}

/// Должник нажал "оплатил часть" → просим сумму.
pub async fn start_payment<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, debt_id: &str,
) -> Result<()> {
    let Some(d) = storage.get_debt(debt_id).await? else { return Ok(()); };
    if d.debtor_user_id != user.id { return Ok(()); }
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingPaymentAmount { debt_id: debt_id.to_string() },
    )).await?;
    api.send_message(
        chat_id,
        &format!("Сколько вы вернули? Остаток: <b>{}</b>", Money::new(d.remaining_minor(), d.currency).format()),
        None,
    ).await?;
    Ok(())
}

pub async fn receive_payment<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>,
    msg: &Message, user: &User, debt_id: &str, amount_text: &str,
) -> Result<()> {
    let Some(mut d) = storage.get_debt(debt_id).await? else { return Ok(()); };
    if d.debtor_user_id != user.id { return Ok(()); }
    let Some(money) = Money::parse(amount_text, d.currency) else {
        api.send_message(msg.chat.id, "Не понял сумму.", None).await?;
        return Ok(());
    };
    let pay = money.amount_minor.min(d.remaining_minor());
    d.payments.push(Payment {
        amount_minor: pay,
        at: chrono::Utc::now(),
        note: None,
    });
    // Авто-аллокация по рассрочке: помечаем оплаченными ближайшие платежи.
    d.recompute_installment_status();
    if d.remaining_minor() == 0 {
        d.status = DebtStatus::Settled;
        d.settled_at = Some(chrono::Utc::now());
    }
    storage.update_debt(&d).await?;
    storage.clear_session(user.telegram_id).await?;

    let paid = Money::new(pay, d.currency).format();
    let remaining = Money::new(d.remaining_minor(), d.currency).format();
    api.send_message(
        msg.chat.id,
        &format!("✅ Записано: {paid}. Остаток: {remaining}."),
        None,
    ).await?;

    // Уведомляем кредитора
    let creditor_id = d.creditor_user_id.clone();
    let _ = notifier.send_from(
        &user.id, &creditor_id,
        &format!("💵 {} вернул(а) {} по долгу. Остаток: {}.", user.display_name(), paid, remaining),
        None,
    ).await;

    show_debt(api, storage, msg.chat.id, user, debt_id).await
}

/// Кредитор нажал «📅 Сделать рассрочкой» → просим план.
pub async fn start_installments<S: Storage>(
    api: &BotApi, storage: &S, chat_id: i64, user: &User, debt_id: &str,
) -> Result<()> {
    let Some(d) = storage.get_debt(debt_id).await? else { return Ok(()); };
    if d.creditor_user_id != user.id { return Ok(()); }
    if !d.installments.is_empty() || !d.payments.is_empty() {
        api.send_message(chat_id, "По этому долгу уже есть платежи или график.", None).await?;
        return Ok(());
    }
    storage.set_session(&Session::new(
        user.telegram_id,
        SessionState::AwaitingInstallmentPlan { debt_id: debt_id.to_string() },
    )).await?;
    api.send_message(
        chat_id,
        "Введите план в формате <code>N/D</code> или <code>N/D/YYYY-MM-DD</code>:\n\
         • <code>3/30</code> — 3 платежа каждые 30 дней (первый через 30 дней)\n\
         • <code>4/15/2026-07-01</code> — 4 платежа каждые 15 дней, первый 1 июля\n\n\
         Или /cancel чтобы отменить.",
        None,
    ).await?;
    Ok(())
}

pub async fn receive_installment_plan<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>,
    msg: &Message, user: &User, debt_id: &str, text: &str,
) -> Result<()> {
    let Some(mut d) = storage.get_debt(debt_id).await? else { return Ok(()); };
    if d.creditor_user_id != user.id { return Ok(()); }
    if !d.installments.is_empty() || !d.payments.is_empty() {
        storage.clear_session(user.telegram_id).await?;
        return Ok(());
    }
    let Some(plan) = parse_plan(text) else {
        api.send_message(
            msg.chat.id,
            "Не понял формат. Пример: <code>3/30</code> или <code>3/30/2026-06-15</code>.",
            None,
        ).await?;
        return Ok(());
    };
    let installments = build_installments(&plan, d.original_amount_minor);
    if installments.is_empty() {
        api.send_message(msg.chat.id, "Не удалось построить график.", None).await?;
        return Ok(());
    }
    d.installments = installments;
    storage.update_debt(&d).await?;
    storage.clear_session(user.telegram_id).await?;

    // Уведомляем должника
    let debtor_id = d.debtor_user_id.clone();
    let total = Money::new(d.original_amount_minor, d.currency).format();
    let mut summary = format!(
        "📅 По долгу {} установлен график рассрочки ({} платежей):\n",
        total, d.installments.len()
    );
    for (i, inst) in d.installments.iter().enumerate() {
        summary.push_str(&format!(
            "{}. {} — {}\n",
            i + 1,
            fmt_date_local(inst.due_date),
            Money::new(inst.amount_minor, d.currency).format(),
        ));
    }
    api.send_message(msg.chat.id, &summary, None).await?;
    let _ = notifier.send_from(
        &user.id, &debtor_id,
        &format!("📅 {} установил(а) график рассрочки по вашему долгу.\n\n{}", user.display_name(), summary),
        None,
    ).await;

    show_debt(api, storage, msg.chat.id, user, debt_id).await
}

pub async fn forgive<S: Storage>(
    api: &BotApi, storage: &S, notifier: &Notifier<S>,
    chat_id: i64, user: &User, debt_id: &str,
) -> Result<()> {
    let Some(mut d) = storage.get_debt(debt_id).await? else { return Ok(()); };
    if d.creditor_user_id != user.id { return Ok(()); }
    d.status = DebtStatus::Forgiven;
    d.settled_at = Some(chrono::Utc::now());
    storage.update_debt(&d).await?;

    let debtor_id = d.debtor_user_id.clone();
    let _ = notifier.send_from(
        &user.id, &debtor_id,
        &format!("🕊 {} простил(а) ваш долг ({}).", user.display_name(), d.description),
        None,
    ).await;

    show_debt(api, storage, chat_id, user, debt_id).await
}

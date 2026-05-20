//! Фоновый scheduler для напоминаний о платежах по рассрочке.
//!
//! Telegram Bot API не умеет планировать сообщения, поэтому держим свой цикл:
//! каждые 60 секунд проходим по активным долгам с рассрочкой и шлём
//! напоминания:
//!  • в день срока (если ещё не слали),
//!  • через сутки после срока — «просрочено».
//!
//! Учёт `due_notified_at` / `overdue_notified_at` не даёт спамить.

use crate::error::Result;
use crate::models::{DebtStatus, Money};
use crate::services::clock::fmt_date_local;
use crate::services::Notifier;
use crate::storage::Storage;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tracing::{debug, error, info};

const TICK: std::time::Duration = std::time::Duration::from_secs(60);

pub fn spawn<S: Storage>(storage: Arc<S>, notifier: Arc<Notifier<S>>) {
    tokio::spawn(async move {
        info!("scheduler started, tick = {:?}", TICK);
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            if let Err(e) = tick(&*storage, &*notifier).await {
                error!("scheduler tick error: {e:?}");
            }
        }
    });
}

async fn tick<S: Storage>(storage: &S, notifier: &Notifier<S>) -> Result<()> {
    let now = Utc::now();
    // Простой подход: собираем все долги через каждого юзера. Дубликаты
    // отсекаем по id. Когда переедем на Mongo — заменим на индекс по статусу.
    let users = list_active_user_ids(storage).await?;
    let mut seen = std::collections::HashSet::new();
    for uid in users {
        for debt in storage.list_debts_for_user(&uid).await? {
            if !seen.insert(debt.id.clone()) { continue; }
            if debt.status != DebtStatus::Confirmed { continue; }
            if debt.installments.is_empty() { continue; }
            process_debt(storage, notifier, debt, now).await?;
        }
    }
    Ok(())
}

async fn process_debt<S: Storage>(
    storage: &S,
    notifier: &Notifier<S>,
    mut debt: crate::models::Debt,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut dirty = false;
    let creditor_id = debt.creditor_user_id.clone();
    let debtor_id = debt.debtor_user_id.clone();
    let desc = if debt.description.is_empty() {
        "долг".to_string()
    } else {
        format!("«{}»", debt.description)
    };

    for inst in debt.installments.iter_mut() {
        if inst.paid { continue; }
        let amount = Money::new(inst.amount_minor, debt.currency).format();
        let due_str = fmt_date_local(inst.due_date);

        // 1) день срока: now ∈ [due_date, due_date + 24h)
        if inst.due_notified_at.is_none()
            && now >= inst.due_date
            && now < inst.due_date + Duration::hours(24)
        {
            // должнику
            let to_debtor = format!(
                "⏰ Сегодня срок платежа по {desc}: <b>{amount}</b>.\n\
                 Если уже отдали — нажмите в долге «💵 Я оплатил часть»."
            );
            let _ = notifier.send_from(&creditor_id, &debtor_id, &to_debtor, None).await;
            // кредитору
            let to_creditor = format!(
                "⏰ Сегодня срок платежа от должника по {desc}: <b>{amount}</b>."
            );
            let _ = notifier.send_from(&debtor_id, &creditor_id, &to_creditor, None).await;
            inst.due_notified_at = Some(now);
            dirty = true;
            debug!("notified due for installment {due_str}");
        }

        // 2) просрочка: now >= due_date + 24h и ещё не оплачено
        if inst.overdue_notified_at.is_none()
            && now >= inst.due_date + Duration::hours(24)
        {
            let to_debtor = format!(
                "⚠️ Платёж по {desc} от {due_str} ({amount}) <b>просрочен</b>.\n\
                 Свяжитесь с кредитором или отметьте оплату."
            );
            let _ = notifier.send_from(&creditor_id, &debtor_id, &to_debtor, None).await;
            let to_creditor = format!(
                "⚠️ Платёж по {desc} от {due_str} ({amount}) <b>просрочен</b> должником."
            );
            let _ = notifier.send_from(&debtor_id, &creditor_id, &to_creditor, None).await;
            inst.overdue_notified_at = Some(now);
            dirty = true;
        }
    }

    if dirty {
        storage.update_debt(&debt).await?;
    }
    Ok(())
}

/// Список юзеров, которые точно фигурируют в каких-то долгах. Для файлового
/// хранилища делаем компактный обход — все юзеры с phone (т.е. linked).
async fn list_active_user_ids<S: Storage>(storage: &S) -> Result<Vec<String>> {
    // У нас нет метода list_users, поэтому собираем через сессии — недостаточно.
    // На v1 проще: пробуем кеш через linked-контакты + создателей долгов.
    // Чтобы не плодить новый метод, добавим в Storage list_all_users().
    storage.list_all_users().await.map(|us| us.into_iter().map(|u| u.id).collect())
}

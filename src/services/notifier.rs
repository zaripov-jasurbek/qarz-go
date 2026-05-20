//! Высокоуровневые уведомления. Главное отличие от прямых вызовов BotApi —
//! автоматическая проверка блокировки: если получатель заблокировал отправителя,
//! сообщение не уходит.

use crate::error::Result;
use crate::storage::Storage;
use crate::telegram::api::BotApi;
use crate::telegram::types::ReplyMarkup;
use std::sync::Arc;
use tracing::debug;

pub struct Notifier<S: Storage> {
    pub api: BotApi,
    pub storage: Arc<S>,
}

impl<S: Storage> Notifier<S> {
    pub fn new(api: BotApi, storage: Arc<S>) -> Self {
        Self { api, storage }
    }

    /// Отправить юзеру сообщение от имени `from_user_id`. Если получатель
    /// заблокировал отправителя — молча пропускаем.
    pub async fn send_from(
        &self,
        from_user_id: &str,
        to_user_id: &str,
        text: &str,
        markup: Option<&ReplyMarkup>,
    ) -> Result<()> {
        if self.storage.is_blocked(to_user_id, from_user_id).await? {
            debug!("skipping notify: {to_user_id} blocked {from_user_id}");
            return Ok(());
        }
        let Some(recipient) = self.storage.get_user(to_user_id).await? else {
            debug!("no such user: {to_user_id}");
            return Ok(());
        };
        self.api.send_message(recipient.telegram_id, text, markup).await?;
        Ok(())
    }

    /// Системное сообщение (от имени бота). Игнорирует блокировки.
    pub async fn send_system(
        &self,
        to_user_id: &str,
        text: &str,
        markup: Option<&ReplyMarkup>,
    ) -> Result<()> {
        let Some(recipient) = self.storage.get_user(to_user_id).await? else {
            return Ok(());
        };
        self.api.send_message(recipient.telegram_id, text, markup).await?;
        Ok(())
    }
}

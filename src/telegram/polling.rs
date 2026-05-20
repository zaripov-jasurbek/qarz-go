//! Long-polling loop. Получает обновления через getUpdates и отдаёт их
//! диспетчеру хэндлеров.

use crate::error::Result;
use crate::handlers::Dispatcher;
use crate::telegram::api::BotApi;
use std::sync::Arc;
use tracing::{error, info, warn};

pub async fn run<S: crate::storage::Storage>(api: BotApi, dispatcher: Arc<Dispatcher<S>>) -> Result<()> {
    // Если когда-то был установлен webhook — снимаем его, чтобы polling работал.
    if let Err(e) = api.delete_webhook().await {
        warn!("deleteWebhook failed (можно игнорировать): {e}");
    }

    let mut offset: i64 = 0;
    info!("polling started");

    loop {
        match api.get_updates(offset, 25).await {
            Ok(updates) => {
                for upd in updates {
                    offset = upd.update_id + 1;
                    let dispatcher = dispatcher.clone();
                    let api = api.clone();
                    // Каждое обновление обрабатываем в своей задаче, чтобы медленный
                    // хэндлер не задерживал остальные.
                    tokio::spawn(async move {
                        if let Err(e) = dispatcher.handle(&api, upd).await {
                            error!("handler error: {e:?}");
                        }
                    });
                }
            }
            Err(e) => {
                error!("getUpdates error: {e:?}");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

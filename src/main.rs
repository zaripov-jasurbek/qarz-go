use loan_wallet::{
    config::Config,
    error::Result,
    handlers::Dispatcher,
    services::{scheduler, Notifier},
    storage::FileStorage,
    telegram::{api::BotApi, polling},
};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("loan_wallet=info")))
        .init();

    let cfg = Config::from_env()?;
    info!("data dir: {}", cfg.data_dir.display());

    let storage = Arc::new(FileStorage::open(&cfg.data_dir).await?);
    let api = BotApi::new(cfg.bot_token.clone());

    // Узнаём username бота (для deep-link приглашений).
    let me = api.get_me().await?;
    let bot_username = me.username.clone().unwrap_or_default();
    info!("бот: @{} ({})", bot_username, me.first_name);

    let notifier = Arc::new(Notifier::new(api.clone(), storage.clone()));
    let dispatcher = Arc::new(Dispatcher::new(storage.clone(), notifier.clone(), bot_username));

    // Фоновый scheduler напоминаний о платежах. Tokio cancel'нёт его при выходе из main.
    scheduler::spawn(storage.clone(), notifier.clone());

    // Polling. Если позже нужен webhook — собрать axum-app из
    // loan_wallet::telegram::webhook::router(...) и крутить параллельно tokio::join!.
    polling::run(api, dispatcher).await?;
    Ok(())
}

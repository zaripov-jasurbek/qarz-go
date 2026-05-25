use loan_wallet::{
    config::Config,
    error::Result,
    handlers::Dispatcher,
    services::{scheduler, Notifier},
    storage::MongoStorage,
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

    let storage = Arc::new(MongoStorage::open(&cfg.mongodb_uri, &cfg.mongodb_db).await?);
    let api = BotApi::new(cfg.bot_token.clone());

    let me = api.get_me().await?;
    let bot_username = me.username.clone().unwrap_or_default();
    info!("бот: @{} ({})", bot_username, me.first_name);

    let notifier = Arc::new(Notifier::new(api.clone(), storage.clone()));
    let dispatcher = Arc::new(Dispatcher::new(storage.clone(), notifier.clone(), bot_username));

    scheduler::spawn(storage.clone(), notifier.clone());

    polling::run(api, dispatcher).await?;
    Ok(())
}

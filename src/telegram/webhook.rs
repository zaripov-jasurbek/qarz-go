//! Каркас axum-роутера под webhook. Не используется при polling, но готов для
//! подключения, когда захочешь перейти на webhook режим.
//!
//! Использование (когда понадобится):
//! ```ignore
//! let app = webhook::router(api, dispatcher, "my-secret".into());
//! axum::serve(listener, app).await?;
//! ```

use crate::handlers::Dispatcher;
use crate::telegram::api::BotApi;
use crate::telegram::types::Update;
use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use std::sync::Arc;
use tracing::error;

struct WebhookState<S: crate::storage::Storage> {
    api: BotApi,
    dispatcher: Arc<Dispatcher<S>>,
    secret: Option<String>,
}

impl<S: crate::storage::Storage> Clone for WebhookState<S> {
    fn clone(&self) -> Self {
        Self {
            api: self.api.clone(),
            dispatcher: self.dispatcher.clone(),
            secret: self.secret.clone(),
        }
    }
}

pub fn router<S: crate::storage::Storage>(
    api: BotApi,
    dispatcher: Arc<Dispatcher<S>>,
    secret: Option<String>,
) -> Router {
    let state = WebhookState { api, dispatcher, secret };
    Router::new()
        .route("/webhook", post(handle_update::<S>))
        .with_state(state)
}

async fn handle_update<S: crate::storage::Storage>(
    State(st): State<WebhookState<S>>,
    headers: HeaderMap,
    Json(update): Json<Update>,
) -> impl IntoResponse {
    if let Some(expected) = &st.secret {
        let got = headers
            .get("X-Telegram-Bot-Api-Secret-Token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if got != expected {
            return StatusCode::FORBIDDEN;
        }
    }
    let api = st.api.clone();
    let dispatcher = st.dispatcher.clone();
    tokio::spawn(async move {
        if let Err(e) = dispatcher.handle(&api, update).await {
            error!("webhook handler error: {e:?}");
        }
    });
    StatusCode::OK
}

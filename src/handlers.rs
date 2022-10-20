use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};

use crate::state::AppState;

#[derive(Deserialize)]
pub struct UrlBody {
    pub url: String,
}

pub async fn post_url_handler(
    State(app_state): State<AppState>,
    Json(body): Json<UrlBody>,
) -> String {
    // Channel to receive result from the parsing task
    let (response_tx, response_rx) = oneshot::channel();
    app_state.url_sender.send((body.url, response_tx)).unwrap();

    response_rx.await.unwrap()
}

pub async fn task_persist_handler(pool: Arc<PgPool>, urls: Vec<String>, _: mpsc::Sender<()>) {
    // OBS: the connection is returned to the pool on drop.
    // If it was other database vendor, we would use `sqlx::QueryBuilder::push_values` (https://github.com/launchbadge/sqlx/blob/main/FAQ.md#how-can-i-bind-an-array-to-a-values-clause-how-can-i-do-bulk-inserts).
    // We assume everything works as expected for simplicity, error handling would be done in a real case since a lot can go wrong here.
    // This is valid for the whole project
    let mut conn = pool.acquire().await.unwrap();
    let query =
        sqlx::query("INSERT INTO urls (url) SELECT * FROM UNNEST($1::text[])").bind(&urls[..]);
    query.execute(&mut conn).await.unwrap();
    println!("Persisted {} urls", urls.len());
}

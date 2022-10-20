use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{routing::post, Router};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use tokio::{
    signal,
    sync::{mpsc, oneshot},
};
use url::Url;

use crate::handlers::task_persist_handler;

pub mod handlers;
pub mod state;
pub mod url_body;

/**
Starts the server with a max value for the url buffer.
*/
pub async fn start_server(max: u8) {
    let (request_tx, mut request_rx) =
        mpsc::unbounded_channel::<(String, oneshot::Sender<String>)>();
    let (db_tx, mut db_rx) = mpsc::unbounded_channel::<Vec<String>>();
    let db_tx2 = db_tx.clone();

    let buffer = Arc::new(Mutex::new(Vec::with_capacity(max as usize)));
    let buffer2 = Arc::clone(&buffer);
    // Url parsing task
    let url_handle = tokio::spawn(async move {
        let mut total = 0u8;
        while let Some((url, response_tx)) = request_rx.recv().await {
            if Url::parse(&url).is_ok() {
                let mut buffer = buffer.lock().unwrap();
                buffer.push(url);
                total += 1;
                if total == max {
                    // Send the buffer to the db task to be persisted
                    //
                    // Since the buffer is not that big I chose to just clone it, no need for synchronization
                    db_tx.send(buffer.clone()).unwrap();
                    total = 0;
                    buffer.clear();
                }
                response_tx.send("OK".to_string()).unwrap();
            } else {
                println!("Invalid URL: {}", url);
                response_tx.send("Invalid URL".to_string()).unwrap();
            }
        }
    });

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
    let db_shutdown_tx = shutdown_tx.clone();
    // DB task
    tokio::spawn(async move {
        let port = 5432;
        let pool = Arc::new(
            PgPoolOptions::new()
                .max_connections(5)
                .idle_timeout(Some(Duration::from_secs(30)))
                .connect_lazy_with(
                    PgConnectOptions::new()
                        .host("host")
                        .port(port)
                        .username("username")
                        .password("password")
                        .database("database")
                        .ssl_mode(PgSslMode::Disable),
                ),
        );

        while let Some(urls) = db_rx.recv().await {
            let persist_shutdown_tx = db_shutdown_tx.clone();
            // Spawn new task so we dont block the channel from receiving more data
            tokio::spawn(task_persist_handler(
                Arc::clone(&pool),
                urls,
                persist_shutdown_tx,
            ));
        }
    });

    // Drop so we need to wait just for the other tasks to finish
    drop(shutdown_tx);

    let app_state = Arc::new(state::AppState {
        url_sender: request_tx,
    });
    let app =
        Router::with_state_arc(app_state.clone()).route("/url", post(handlers::post_url_handler));

    // Run server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {addr}");
    // Shut it down if CTRL-C is pressed
    tokio::select! {
        _ = axum::Server::bind(&addr).serve(app.into_make_service()) => {},
        _ = signal::ctrl_c() => {},
    }

    println!("Shutting down");
    // For some reason the `select!` doesn't cancel the task if we put the handle there
    // so we manually cancel it so the db task can shutdown
    //
    // It's **probably** fine, since there's no more server, all in flight requests would fail, which means we don't need to respond to the request,
    // which means we don't have to handle the remaining urls in the channel buffer
    url_handle.abort();

    // Send remaining items already processed to db task
    db_tx2.send(buffer2.lock().unwrap().clone()).unwrap();
    // Droping the sender will make the receiver return None, finishing the db task
    drop(db_tx2);

    // Waits for db/persist tasks to finish
    // If there is a persist task still in progress it will block until it finishes
    // If not it will exit since the db task was the last active task
    let _ = shutdown_rx.recv().await;
}

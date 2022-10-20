use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct AppState {
    /// Channel to send urls to the url parsing task
    ///
    /// The task will send a response back to the oneshot channel
    pub url_sender: mpsc::UnboundedSender<(String, oneshot::Sender<String>)>,
}

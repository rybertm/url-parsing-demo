use url_parsing_demo::start_server;

#[tokio::main]
async fn main() {
    start_server(100).await;
}

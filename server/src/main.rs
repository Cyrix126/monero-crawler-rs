use axum::serve;
use monero_crawler_server_lib::{AppState, router};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let state = AppState::new(confy::load("monero-crawler-rs", "config.toml")?).await?;
    let listener =
        tokio::net::TcpListener::bind(format!("127.0.0.1:{}", state.config.listen_port)).await?;
    info!("Listening on port {}", state.config.listen_port);
    serve(listener, router(state)).await?;
    Ok(())
}

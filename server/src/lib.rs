use axum::Router;
use config::Config;
use db::migration::run_migrations;
use deadpool_diesel::sqlite::Pool;
use reqwest::{Client, ClientBuilder};

pub mod config;
pub mod crawler;
pub mod db;
pub mod error;
pub mod handler;

#[derive(Clone)]
pub struct AppState {
    // roadmap could include allowing to use command line args and environments variable.
    // Configuration that the program will run with.
    pub config: Config,
    // Database pool connections
    pub pool: Pool,
    // reqwest client to crawl the monero network
    pub client: Client,
}

impl AppState {
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let pool = Pool::builder(deadpool_diesel::Manager::new(
            config.db_path.to_str().unwrap(),
            deadpool_diesel::Runtime::Tokio1,
        ))
        .build()?;
        run_migrations(&pool).await?;
        let client = ClientBuilder::new()
            .build()
            .expect("value given to builder should be valid");
        Ok(AppState {
            config,
            pool,
            client,
        })
    }
}
pub fn router(state: AppState) -> Router {
    Router::new()
        // .route("/start", ))
        // .route("/stop", ))
        // .route("/refresh", ))
        // .route("/clean", ))
        // .route("/dump", )
        // .route("/filter", ))
        .with_state(state)
}

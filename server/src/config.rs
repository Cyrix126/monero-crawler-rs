use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};
use url::Url;

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    // database path
    pub db_path: PathBuf,
    // port on which the API will listen for incoming connections
    pub listen_port: u16,
    pub seednodes: Vec<Url>,
}

impl Default for Config {
    fn default() -> Self {
        let mut db_path = directories::ProjectDirs::from("", "", "monero-crawler-rs")
            .unwrap()
            .data_dir()
            .to_path_buf();
        db_path.push("db.sqlite");
        Self {
            db_path,
            listen_port: 10200,
            seednodes: vec![Url::from_str("http://127.0.0.1:18081").unwrap()],
        }
    }
}

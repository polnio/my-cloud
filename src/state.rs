use crate::config::Config;
use crate::database::Database;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type StateHandle = Arc<State>;

pub struct State {
    pub creds: Mutex<HashMap<String, (String, Option<String>)>>,
    pub config: Config,
    pub database: Database,
}

impl State {
    pub async fn new(config: Config) -> Result<Self> {
        let creds = Default::default();
        let database = Database::connect(&config.database.url).await?;
        Ok(Self {
            creds,
            config,
            database,
        })
    }
}

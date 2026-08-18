use config::ConfigError;
use std::path::PathBuf;

#[derive(serde::Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    #[cfg(feature = "webdav")]
    pub webdav: WebdavConfig,
}

#[derive(serde::Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[cfg(feature = "webdav")]
#[derive(serde::Deserialize)]
pub struct WebdavConfig {
    pub host: String,
    pub port: u16,
    pub root: PathBuf,
}

impl Config {
    pub fn parse(path: &str) -> Result<Self, ConfigError> {
        config::Config::builder()
            .add_source(config::File::with_name(path))
            .add_source(config::Environment::with_prefix("MY_CLOUD"))
            .build()?
            .try_deserialize()
    }
}

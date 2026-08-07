use config::ConfigError;
#[cfg(feature = "webdav")]
use std::path::PathBuf;

#[derive(serde::Deserialize)]
pub struct Config {
    #[cfg(feature = "webdav")]
    pub webdav: WebdavConfig,
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

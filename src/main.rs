mod args;
mod auth;
mod config;
mod database;
mod services;
mod state;

use crate::args::Args;
use crate::config::Config;
use crate::state::State;
use anyhow::Context as _;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::select;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let result = tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_max_level(Level::WARN)
            .finish(),
    )
    .context("Failed to set global default subscriber");
    if let Err(err) = result {
        eprintln!("{:?}", err);
    }

    let args = Args::parse();
    let config = {
        let path = args
            .config
            .map(PathBuf::from)
            .or_else(|| dirs::config_dir().map(|p| p.join("my-cloud").join("config.toml")))
            .context("Failed to find config path")?;
        Config::parse(&path).context("Failed to parse config")?
    };
    let state = State::new(config).await?;
    let state = Arc::new(state);

    #[cfg(feature = "webdav")]
    let webdav = services::webdav::serve(state);
    #[cfg(not(feature = "webdav"))]
    let webdav = std::future::pending::<anyhow::Result<()>>();

    let ctrl_c = tokio::signal::ctrl_c();

    select! {
        result = webdav => result.context("Failed to serve webdav")?,
        result = ctrl_c => result.context("Failed to listen for ctrl-c")?,
    }

    Ok(())
}

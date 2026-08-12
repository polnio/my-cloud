mod args;
mod auth;
mod config;
mod services;
mod state;

use crate::args::Args;
use crate::config::Config;
use crate::state::StateHandle;
use anyhow::Context as _;
use std::sync::Arc;
use tokio::select;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let result = tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_max_level(Level::DEBUG)
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
            .or_else(|| {
                dirs::config_dir().map(|p| {
                    p.join("my-cloud")
                        .join("config.toml")
                        .to_string_lossy()
                        .into_owned()
                })
            })
            .context("Failed to find config path")?;
        Config::parse(&path).context("Failed to parse config")?
    };
    let config = Arc::new(config);
    let state = StateHandle::default();

    #[cfg(feature = "webdav")]
    let webdav = services::webdav::serve(config, state);
    #[cfg(not(feature = "webdav"))]
    let webdav = std::future::pending::<anyhow::Result<()>>();

    let ctrl_c = tokio::signal::ctrl_c();

    select! {
        result = webdav => result.context("Failed to serve webdav")?,
        result = ctrl_c => result.context("Failed to listen for ctrl-c")?,
    }

    Ok(())
}

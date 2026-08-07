mod args;
mod config;
#[cfg(feature = "webdav")]
mod webdav;

use crate::args::Args;
use crate::config::Config;
use anyhow::Context as _;
use tokio::select;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    #[cfg(feature = "webdav")]
    let webdav = webdav::serve(&config);
    #[cfg(not(feature = "webdav"))]
    let webdav = std::future::pending::<()>();

    let ctrl_c = tokio::signal::ctrl_c();

    select! {
        result = webdav => result.context("Failed to serve webdav")?,
        result = ctrl_c => result.context("Failed to listen for ctrl-c")?,
    }

    Ok(())
}

use tokio::select;

#[cfg(feature = "webdav")]
mod webdav;

#[tokio::main]
async fn main() {
    let cwd = std::env::current_dir().unwrap();

    #[cfg(feature = "webdav")]
    let webdav = webdav::serve(&cwd, ([127, 0, 0, 1], 5000).into());
    #[cfg(not(feature = "webdav"))]
    let webdav = std::future::pending::<()>();

    let ctrl_c = tokio::signal::ctrl_c();

    select! {
        _ = webdav => {}
        _ = ctrl_c => {}
    }
}

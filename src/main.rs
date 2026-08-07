use tokio::select;

#[cfg(feature = "dav")]
mod dav;

#[tokio::main]
async fn main() {
    let cwd = std::env::current_dir().unwrap();

    #[cfg(feature = "dav")]
    let dav = dav::serve(&cwd, ([127, 0, 0, 1], 5000).into());
    #[cfg(not(feature = "dav"))]
    let dav = std::future::pending::<()>();

    let ctrl_c = tokio::signal::ctrl_c();

    select! {
        _ = dav => {}
        _ = ctrl_c => {}
    }
}

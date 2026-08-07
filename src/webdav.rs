use crate::config::Config;
use dav_server::DavHandler;
use dav_server::fakels::FakeLs;
use dav_server::localfs::LocalFs;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

pub fn serve(config: &Config) -> impl Future<Output = std::io::Result<()>> + use<> {
    let mut tasks = JoinSet::new();
    let handler = DavHandler::builder()
        .filesystem(LocalFs::new(&config.webdav.root, false, false, false))
        .locksystem(FakeLs::new())
        .build_handler();

    let host = config.webdav.host.clone();
    let port = config.webdav.port;

    async move {
        let listener = TcpListener::bind((host, port)).await?;
        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let handler = handler.clone();
            tasks.spawn(http1::Builder::new().serve_connection(
                io,
                service_fn(move |req| {
                    let handler = handler.clone();
                    async move { Ok::<_, Infallible>(handler.handle(req).await) }
                }),
            ));
        }
    }
}

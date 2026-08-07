use crate::config::Config;
use crate::state::StateHandle;
use anyhow::Context as _;
use dav_server::DavHandler;
use dav_server::body::Body;
use dav_server::fakels::FakeLs;
use dav_server::localfs::LocalFs;
use headers::authorization::Basic;
use headers::{Authorization, HeaderMapExt as _};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

pub fn serve(
    config: Arc<Config>,
    state: StateHandle,
) -> impl Future<Output = std::io::Result<()>> + use<> {
    let mut tasks = JoinSet::new();

    async move {
        let listener = TcpListener::bind((config.webdav.host.as_str(), config.webdav.port)).await?;
        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let config = config.clone();
            let state = state.clone();
            tasks.spawn(http1::Builder::new().serve_connection(
                io,
                service_fn(move |req| {
                    let config = config.clone();
                    let state = state.clone();
                    async move { Ok::<_, Infallible>(handle(req, config, state).await) }
                }),
            ));
        }
    }
}

async fn handle(
    request: Request<Incoming>,
    config: Arc<Config>,
    state: StateHandle,
) -> Response<Body> {
    /// https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/WWW-Authenticate
    static AUTH_CHALLENGE: &str = "Basic realm=\"Basic auth\"";
    let creds = match Auth::from_request(&request) {
        Ok(creds) => creds,
        Err(err) => {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("WWW-Authenticate", AUTH_CHALLENGE)
                .body(err.to_string().into())
                .unwrap();
        }
    };

    let mut state = state.lock().await;
    let logged = if let Some(password) = state.creds.get(&creds.username) {
        password == &creds.password
    } else if let Some(user) = config.users.get(&creds.username) {
        let result = crate::auth::verify_password(&creds.password, &user.password_file)
            .context("Failed to verify password");
        match result {
            Ok(success) => {
                println!("New hit");
                if success {
                    state
                        .creds
                        .insert(creds.username.clone(), creds.password.clone());
                }
                success
            }
            Err(err) => {
                return Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header("WWW-Authenticate", AUTH_CHALLENGE)
                    .body(err.to_string().into())
                    .unwrap();
            }
        }
    } else {
        false
    };
    drop(state);
    if !logged {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", AUTH_CHALLENGE)
            .body("Invalid credentials".into())
            .unwrap();
    }

    let root = config.webdav.root.join(&creds.username);
    let handler = DavHandler::builder()
        .filesystem(LocalFs::new(&root, false, false, false))
        .locksystem(FakeLs::new())
        .principal(creds.username)
        .build_handler();
    handler.handle(request).await
}

#[derive(Clone, PartialEq, Eq)]
struct Auth {
    username: String,
    password: String,
}

impl Auth {
    fn from_request(request: &Request<Incoming>) -> anyhow::Result<Self> {
        let auth = request
            .headers()
            .typed_get::<Authorization<Basic>>()
            .context("Missing authorization header")?;
        let username = auth.username().to_owned();
        let password = auth.password().to_owned();
        Ok(Self { username, password })
    }
}

use crate::config::Config;
use crate::state::StateHandle;
use anyhow::Context as _;
use dav_server::DavHandler;
use dav_server::body::Body;
use dav_server::fakels::FakeLs;
use dav_server::fs::GuardedFileSystem;
use dav_server::localfs::LocalFs;
use futures_util::StreamExt as _;
use headers::authorization::Basic;
use headers::{Authorization, HeaderMapExt as _};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::os::unix::ffi::OsStrExt as _;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

pub async fn serve(config: Arc<Config>, state: StateHandle) -> std::io::Result<()> {
    let mut tasks = JoinSet::new();

    std::fs::create_dir_all(&config.webdav.root)?;
    for (id, user) in config.users.iter() {
        std::fs::create_dir_all(
            config
                .webdav
                .root
                .join(user.display_name.as_ref().unwrap_or(id)),
        )?;
    }
    for (id, group) in config.groups.iter() {
        std::fs::create_dir_all(
            config
                .webdav
                .root
                .join(group.display_name.as_ref().unwrap_or(id)),
        )?;
    }

    let handler = DavHandler::builder()
        .filesystem(MyCloudFs::new(config.clone()))
        .locksystem(FakeLs::new())
        .build_handler();

    let listener = TcpListener::bind((config.webdav.host.as_str(), config.webdav.port)).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let config = config.clone();
        let state = state.clone();
        let handler = handler.clone();
        tasks.spawn(http1::Builder::new().serve_connection(
            io,
            service_fn(move |req| {
                let config = config.clone();
                let state = state.clone();
                let handler = handler.clone();
                async move { Ok::<_, Infallible>(handle(req, handler, config, state).await) }
            }),
        ));
    }
}

async fn handle(
    request: Request<Incoming>,
    handler: DavHandler<MyCloudFsState>,
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

    let state = MyCloudFsState {
        username: creds.username.clone(),
    };

    handler.handle_guarded(request, creds.username, state).await
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

#[derive(Clone)]
struct MyCloudFs {
    config: Arc<Config>,
    inner: Box<LocalFs>,
}

#[derive(Clone)]
struct MyCloudFsState {
    username: String,
}

impl MyCloudFs {
    fn new(config: Arc<Config>) -> Box<Self> {
        Box::new(Self {
            inner: LocalFs::new(&config.webdav.root, false, false, false),
            config,
        })
    }

    fn is_authorized(
        &self,
        credentials: &MyCloudFsState,
        path: &dav_server::davpath::DavPath,
    ) -> bool {
        let pathbuf = path.as_pathbuf();
        let Some(root) = pathbuf.iter().skip(1).next() else {
            // We are at the root
            return true;
        };

        self.is_related_to_user(credentials, root.as_bytes())
    }

    fn is_related_to_user(&self, credentials: &MyCloudFsState, name: &[u8]) -> bool {
        let user = &self.config.users[&credentials.username];
        let user_name = user.display_name.as_ref().unwrap_or(&credentials.username);
        if name == user_name.as_bytes() {
            return true;
        }

        if self.config.groups.iter().any(|(id, group)| {
            let group_name = group.display_name.as_ref().unwrap_or(id);
            name == group_name.as_bytes() && group.users.contains(&credentials.username)
        }) {
            return true;
        }

        false
    }
}

impl GuardedFileSystem<MyCloudFsState> for MyCloudFs {
    fn open<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        options: dav_server::fs::OpenOptions,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, Box<dyn dav_server::fs::DavFile>> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.open(path, options, &())
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        meta: dav_server::fs::ReadDirMeta,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, dav_server::fs::FsStream<Box<dyn dav_server::fs::DavDirEntry>>>
    {
        Box::pin(async move {
            if !self.is_authorized(credentials, path) {
                return Err(dav_server::fs::FsError::Forbidden);
            }
            // let user = &self.config.users[&credentials.username];
            // let user_name = user.display_name.as_ref().unwrap_or(&credentials.username);
            let mut stream = self.inner.read_dir(path, meta, &()).await?;
            if path.as_bytes() != b"/" {
                return Ok(stream);
            }
            let mut entries = Vec::new();
            while let Some(entry) = stream.next().await {
                let entry = entry?;
                let name = entry.name();
                if !self.is_related_to_user(credentials, &name) {
                    continue;
                }
                entries.push(Ok(entry));
            }
            Ok(Box::pin(futures_util::stream::iter(entries)) as _)
        })
    }

    fn metadata<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, Box<dyn dav_server::fs::DavMetaData>> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.metadata(path, &())
    }

    fn symlink_metadata<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, Box<dyn dav_server::fs::DavMetaData>> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.symlink_metadata(path, &())
    }

    fn create_dir<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, ()> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.create_dir(path, &())
    }

    fn remove_dir<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, ()> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.remove_dir(path, &())
    }

    fn remove_file<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, ()> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.remove_file(path, &())
    }

    fn rename<'a>(
        &'a self,
        from: &'a dav_server::davpath::DavPath,
        to: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, ()> {
        if !self.is_authorized(credentials, from) || !self.is_authorized(credentials, to) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.rename(from, to, &())
    }

    fn copy<'a>(
        &'a self,
        from: &'a dav_server::davpath::DavPath,
        to: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, ()> {
        if !self.is_authorized(credentials, from) || !self.is_authorized(credentials, to) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.copy(from, to, &())
    }

    fn have_props<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        credentials: &'a MyCloudFsState,
    ) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { false });
        }
        self.inner.have_props(path, &())
    }

    fn patch_props<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        patch: Vec<(bool, dav_server::fs::DavProp)>,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, Vec<(StatusCode, dav_server::fs::DavProp)>> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.patch_props(path, patch, &())
    }

    fn get_props<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        do_content: bool,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, Vec<dav_server::fs::DavProp>> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.get_props(path, do_content, &())
    }

    fn get_prop<'a>(
        &'a self,
        path: &'a dav_server::davpath::DavPath,
        prop: dav_server::fs::DavProp,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, Vec<u8>> {
        if !self.is_authorized(credentials, path) {
            return Box::pin(async move { Err(dav_server::fs::FsError::Forbidden) });
        }
        self.inner.get_prop(path, prop, &())
    }

    fn get_quota<'a>(
        &'a self,
        credentials: &'a MyCloudFsState,
    ) -> dav_server::fs::FsFuture<'a, (u64, Option<u64>)> {
        _ = credentials;
        self.inner.get_quota(&())
    }
}

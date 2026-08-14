mod auth;
mod dispatcher;
mod protocol;
mod util;

use crate::config::Config;
use crate::state::StateHandle;
use auth::{Auth, handle_unauthenticated};
use axum::body::{Body, Bytes};
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{Next, from_fn};
use axum::response::{IntoResponse, Response};
use axum::{Router, debug_middleware};
use dispatcher::Dispatcher;
use itertools::Itertools as _;
use std::borrow::Cow;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

const NGX_HTTP_AUTOINDEX_NAME_LEN: usize = 50;

pub async fn serve(config: Arc<Config>, state: StateHandle) -> std::io::Result<()> {
    std::fs::create_dir_all(&config.webdav.root)?;
    for id in config.users.keys() {
        std::fs::create_dir_all(config.webdav.root.join(id))?;
    }
    for id in config.groups.keys() {
        std::fs::create_dir_all(config.webdav.root.join(id))?;
    }

    let state = WebDavState {
        app_state: state,
        config: config.clone(),
    };

    let router = Dispatcher::new()
        .get(handle_get)
        .head(handle_head)
        .put(handle_put)
        .delete(handle_delete)
        .mkcol(handle_mkcol)
        .copy(handle_copy)
        .r#move(handle_move)
        .propfind(handle_propfind)
        .options(handle_options)
        .dispatch();
    let app = Router::new()
        .route("/", router.clone())
        .route("/{*path}", router)
        .layer(from_fn(handle_unauthenticated))
        .layer(from_fn(handle_response_headers))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .with_state(state);

    let listener = TcpListener::bind((config.webdav.host.as_str(), config.webdav.port)).await?;
    axum::serve(listener, app).await
}

async fn handle_get(
    State(state): State<WebDavState>,
    auth: Auth,
    opath: Option<Path<String>>,
) -> Response {
    let path = get_real_path(opath, &state.config);
    let meta = match get_meta(&path) {
        Ok(meta) => meta,
        Err(status) => return status.into_response(),
    };
    if meta.is_file() {
        let props = protocol::propfind::get_props(&path);
        let mut headers = HeaderMap::new();
        let body = Body::from_stream(ReaderStream::new(
            tokio::fs::File::open(path.as_ref()).await.unwrap(),
        ));
        set_headers(&meta, &props, &mut headers);
        (StatusCode::OK, headers, body).into_response()
    } else if meta.is_dir() {
        // https://github.com/nginx/nginx/blob/master/src/http/modules/ngx_http_autoindex_module.c
        let rel_path =
            PathBuf::from("/").join(path.strip_prefix(&state.config.webdav.root).unwrap());
        let is_root = rel_path.parent().is_none();
        let dpath = rel_path.to_string_lossy();
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(err) => {
                tracing::error!("Failed to read directory {dpath}: {err}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        let mut body = format!(
            r#"<html><head><title>Index of {dpath}</title></head><body><h1>Index of {dpath}</h1><hr><pre><a href="../">../</a>"#
        );
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::error!("Failed to read directory entry {dpath}: {err}");
                    continue;
                }
            };
            let mut file_name = entry.file_name();
            let href = util::encode_uri(&file_name.to_string_lossy(), true);
            let rel_path = rel_path.join(&file_name);
            let Some((dn, _)) =
                auth::is_path_related_to_user(&rel_path, auth.username(), &state.config)
            else {
                continue;
            };
            if is_root {
                file_name = dn.unwrap().into();
            }
            let time = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|date| chrono::DateTime::<chrono::Utc>::from(date).to_rfc2822());
            let time = match time {
                Ok(time) => time,
                Err(err) => {
                    tracing::error!("Failed to get modification time for {dpath}: {err}");
                    continue;
                }
            };
            let is_dir = entry.path().is_dir();
            let name = file_name.to_string_lossy();
            body.push_str("\n<a href=\"");
            body.push_str(&href);
            if is_dir {
                body.push('/');
            }
            body.push('"');
            body.push('>');
            // TODO: Escape HTML
            if name.len() > NGX_HTTP_AUTOINDEX_NAME_LEN {
                body.push_str(&name[..NGX_HTTP_AUTOINDEX_NAME_LEN]);
                body.push_str("..&gt;</a>");
            } else {
                body.push_str(&name);
                if is_dir {
                    body.push('/');
                }
                body.push_str("</a>");
                for _ in name.len()..NGX_HTTP_AUTOINDEX_NAME_LEN {
                    body.push(' ');
                }
            }
            body.push(' ');
            body.push_str(&time);
        }
        body.push_str("\n</pre><hr></body></html>");
        (StatusCode::OK, axum::response::Html(body)).into_response()
    } else {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

async fn handle_head(
    State(state): State<WebDavState>,
    _auth: Auth,
    opath: Option<Path<String>>,
) -> Response {
    let path = get_real_path(opath, &state.config);
    let meta = match get_meta(&path) {
        Ok(meta) => meta,
        Err(status) => return status.into_response(),
    };
    if meta.is_file() {
        let props = protocol::propfind::get_props(&path);
        let mut headers = HeaderMap::new();
        set_headers(&meta, &props, &mut headers);
        (StatusCode::OK, headers).into_response()
    } else {
        (StatusCode::OK, [("Content-Type", "text/html")]).into_response()
    }
}

async fn handle_put(
    State(state): State<WebDavState>,
    opath: Option<Path<String>>,
    _auth: Auth,
    body: Bytes,
) -> impl IntoResponse {
    let path = get_real_path(opath, &state.config);
    if path_depth(&path, &state.config) < 2 {
        return StatusCode::FORBIDDEN;
    }
    if path.is_dir() {
        return StatusCode::BAD_REQUEST;
    }
    let is_created = !path.exists();
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => return StatusCode::CONFLICT,
            _ => {
                tracing::error!(
                    "Failed to create/open file {path:?}: {err}",
                    path = path.display()
                );
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        },
    };
    eprintln!("{:?}", String::from_utf8_lossy(&body));
    if let Err(err) = file.write_all(&body) {
        tracing::error!(
            "Failed to write to file {path:?}: {err}",
            path = path.display()
        );
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    if is_created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    }
}

async fn handle_delete(
    State(state): State<WebDavState>,
    _auth: Auth,
    opath: Option<Path<String>>,
) -> impl IntoResponse {
    let path = get_real_path(opath, &state.config);
    if path_depth(&path, &state.config) < 2 {
        return StatusCode::FORBIDDEN;
    }
    let result = if path.is_dir() {
        std::fs::remove_dir_all(&path)
    } else {
        std::fs::remove_file(&path)
    };
    if let Err(err) = result {
        match err.kind() {
            std::io::ErrorKind::NotFound => return StatusCode::NOT_FOUND,
            _ => {
                tracing::error!(
                    "Failed to delete file {path:?}: {err}",
                    path = path.display()
                );
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    }
    StatusCode::NO_CONTENT
}

async fn handle_mkcol(
    State(state): State<WebDavState>,
    _auth: Auth,
    opath: Option<Path<String>>,
) -> impl IntoResponse {
    let path = get_real_path(opath, &state.config);
    if path_depth(&path, &state.config) < 2 {
        return StatusCode::FORBIDDEN;
    }
    if let Err(err) = std::fs::create_dir(&path) {
        match err.kind() {
            std::io::ErrorKind::AlreadyExists => return StatusCode::METHOD_NOT_ALLOWED,
            std::io::ErrorKind::NotFound => return StatusCode::CONFLICT,
            std::io::ErrorKind::StorageFull => return StatusCode::INSUFFICIENT_STORAGE,
            _ => {
                tracing::error!(
                    "Failed to create directory {path:?}: {err}",
                    path = path.display()
                );
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    }
    StatusCode::CREATED
}

async fn handle_copy(
    State(state): State<WebDavState>,
    auth: Auth,
    opath: Option<Path<String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (src_path, dest_path) = match prepare_copy_move(&state, &auth, opath, &headers) {
        Ok(v) => v,
        Err(status) => return status,
    };
    let result = if src_path.is_dir() {
        if headers.get("Depth").is_some_and(|h| h.as_bytes() == b"0") {
            std::fs::create_dir(&dest_path)
        } else {
            let mut result = Ok(());
            for entry in walkdir::WalkDir::new(&src_path) {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        tracing::error!(
                            "Failed to walk directory {src_path:?}: {err}",
                            src_path = src_path.display()
                        );
                        if let Some(err) = err.into_io_error() {
                            result = Err(err);
                        }
                        break;
                    }
                };
                let path = entry.path();
                let new_path = dest_path.join(path.strip_prefix(&src_path).unwrap());
                if path.is_dir() {
                    if let Err(err) = std::fs::create_dir(new_path)
                        && err.kind() != std::io::ErrorKind::NotFound
                    {
                        result = Err(err);
                        break;
                    }
                } else {
                    if let Err(err) = std::fs::copy(path, new_path) {
                        result = Err(err);
                        break;
                    }
                }
            }
            result
        }
    } else {
        std::fs::copy(&src_path, &dest_path).map(|_| ())
    };
    if let Err(err) = result {
        match err.kind() {
            std::io::ErrorKind::StorageFull => return StatusCode::INSUFFICIENT_STORAGE,
            _ => {
                tracing::error!(
                    "Failed to copy file {src_path:?} to {dest_path:?}: {err}",
                    src_path = src_path.display(),
                    dest_path = dest_path.display()
                );
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    }
    StatusCode::CREATED
}

async fn handle_move(
    State(state): State<WebDavState>,
    auth: Auth,
    opath: Option<Path<String>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (src_path, dest_path) = match prepare_copy_move(&state, &auth, opath, &headers) {
        Ok(v) => v,
        Err(status) => return status,
    };

    if let Err(err) = std::fs::rename(&src_path, &dest_path) {
        match err.kind() {
            std::io::ErrorKind::StorageFull => return StatusCode::INSUFFICIENT_STORAGE,
            _ => {
                tracing::error!(
                    "Failed to move file {src_path:?} to {dest_path:?}: {err}",
                    src_path = src_path.display(),
                    dest_path = dest_path.display()
                );
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    }

    StatusCode::CREATED
}

async fn handle_propfind(
    State(state): State<WebDavState>,
    auth: Auth,
    headers: HeaderMap,
    opath: Option<Path<String>>,
    body: Bytes,
) -> Response {
    let path = get_real_path(opath, &state.config);
    let Ok(depth) = headers
        .get("DEPTH")
        .map(|d| {
            d.to_str()
                .map_err(|_| ())
                .and_then(|d| d.parse().map_err(|_| ()))
        })
        .unwrap_or(Ok(usize::MAX))
    else {
        return (StatusCode::BAD_REQUEST, "Invalid depth").into_response();
    };

    let req = if body.is_empty() {
        Default::default()
    } else {
        match protocol::propfind::request::Request::parse(&*body) {
            Ok(req) => req,
            Err(errs) => {
                // TODO: Handle errors
                _ = errs;
                return (StatusCode::BAD_REQUEST, "Invalid body").into_response();
            }
        }
    };

    if !path.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let metas = walkdir::WalkDir::new(path.as_ref())
        .max_depth(depth)
        .into_iter()
        .filter_map(|item| {
            let item = match item {
                Ok(item) => item,
                Err(err) => {
                    // TODO: Handle errors
                    tracing::error!("Failed to walk directory: {err}");
                    return None;
                }
            };
            let path = item.path();
            let rel_path =
                PathBuf::from("/").join(path.strip_prefix(&state.config.webdav.root).unwrap());
            let (dn, rel_path) =
                auth::is_path_related_to_user(&rel_path, auth.username(), &state.config)?;
            let mut meta = protocol::propfind::get_props(path);
            if path == state.config.webdav.root {
                meta.set_is_root();
            }
            if rel_path.parent().is_some_and(|p| p.parent().is_none()) {
                meta.set_is_principal(dn.unwrap().to_owned());
            }
            Some((rel_path.into_owned(), meta))
        })
        .collect_vec();

    let response = protocol::propfind::response::Response::from_metadata(metas, req.props);

    let mut body = Vec::new();
    if let Err(err) = response.to_xml(&mut body) {
        // TODO: Handle errors
        tracing::error!("Failed to serialize response: {err}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize response: {err}"),
        )
            .into_response();
    }

    (
        StatusCode::MULTI_STATUS,
        [("Content-Type", "text/xml; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn handle_options() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("ALLOW", dispatcher::METHODS), ("DAV", "1")],
    )
}

fn get_real_path(path: Option<Path<String>>, config: &Config) -> Cow<'_, std::path::Path> {
    if let Some(Path(p)) = path {
        Cow::Owned(config.webdav.root.join(p))
    } else {
        Cow::Borrowed(config.webdav.root.as_path())
    }
}

fn get_meta(path: &std::path::Path) -> Result<std::fs::Metadata, StatusCode> {
    path.metadata().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            tracing::error!("Failed to get metadata for {}: {}", path.display(), err);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })
}

fn set_headers(
    meta: &std::fs::Metadata,
    props: &protocol::propfind::Metadata,
    headers: &mut HeaderMap,
) {
    if let Some(last_modified) = props.modified_date() {
        headers.insert(
            "Last-Modified",
            chrono::DateTime::<chrono::Utc>::from(last_modified)
                .to_rfc2822()
                .parse()
                .unwrap(),
        );
    }
    headers.insert(
        "Content-Type",
        props
            .mime()
            .unwrap_or("application/octet-stream")
            .parse()
            .unwrap(),
    );
    headers.insert("Content-Length", meta.len().to_string().parse().unwrap());
}

fn path_depth(path: &std::path::Path, config: &Config) -> usize {
    path.strip_prefix(&config.webdav.root)
        .unwrap()
        .iter()
        .count()
}

#[debug_middleware]
async fn handle_response_headers(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("Connection", HeaderValue::from_static("keep-alive"));
    response
}

fn prepare_copy_move<'a>(
    state: &'a WebDavState,
    auth: &Auth,
    opath: Option<Path<String>>,
    headers: &HeaderMap,
) -> Result<(Cow<'a, std::path::Path>, std::path::PathBuf), StatusCode> {
    let Some(mut dest) = headers.get("Destination").and_then(|h| h.to_str().ok()) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let parse = dest.parse::<axum::http::Uri>();
    if let Ok(uri) = &parse {
        dest = uri.path();
    }
    let Some((_, dest)) =
        auth::is_path_related_to_user(dest.as_ref(), auth.username(), &state.config)
    else {
        return Err(StatusCode::FORBIDDEN);
    };
    let dest = dest.strip_prefix("/").unwrap_or(dest.as_ref());
    let overwrite = headers.get("Overwrite").and_then(|h| h.to_str().ok());
    let src_path = get_real_path(opath, &state.config);
    let mut dest_path = state.config.webdav.root.join(dest);
    if src_path.is_file() && dest_path.is_dir() {
        dest_path = dest_path.join(src_path.file_name().unwrap());
    }
    if path_depth(&src_path, &state.config) < 2 {
        return Err(StatusCode::FORBIDDEN);
    }
    if path_depth(&dest_path, &state.config) < 2 {
        return Err(StatusCode::FORBIDDEN);
    }
    if src_path == dest_path {
        return Err(StatusCode::FORBIDDEN);
    }
    if !src_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }
    if !dest_path.parent().is_some_and(|p| p.exists()) {
        return Err(StatusCode::CONFLICT);
    }
    if overwrite == Some("F") && dest_path.exists() {
        return Err(StatusCode::PRECONDITION_FAILED);
    }
    Ok((src_path, dest_path))
}

#[derive(Clone)]
struct WebDavState {
    app_state: StateHandle,
    config: Arc<Config>,
}

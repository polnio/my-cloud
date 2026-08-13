mod auth;
mod dispatcher;
mod protocol;
mod util;

use crate::config::Config;
use crate::state::StateHandle;
use auth::{Auth, handle_unauthenticated};
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::from_fn;
use axum::response::{IntoResponse, Response};
use dispatcher::Dispatcher;
use itertools::Itertools as _;
use std::borrow::Cow;
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
        .propfind(handle_propfind)
        .options(handle_options)
        .dispatch();
    let app = Router::new()
        .route("/", router.clone())
        .route("/{*path}", router)
        .layer(from_fn(handle_unauthenticated))
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
    let path = if let Some(Path(p)) = opath {
        Cow::Owned(state.config.webdav.root.join(p))
    } else {
        Cow::Borrowed(state.config.webdav.root.as_path())
    };
    let meta = match path.metadata() {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(err) => {
            tracing::error!("Failed to get metadata for {}: {}", path.display(), err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if meta.is_file() {
        let mime = mime_guess::from_path(path.as_ref());
        let mime = match mime.count() {
            1 => Some(mime.first().unwrap().to_string()),
            _ => None,
        };
        let file = tokio::fs::File::open(path.as_ref()).await.unwrap();
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);
        (
            StatusCode::OK,
            [
                (
                    "Content-Type",
                    mime.unwrap_or_else(|| "application/octet-stream".to_owned()),
                ),
                ("Content-Length", meta.len().to_string()),
                // (
                //     "Content-Disposition",
                //     format!(
                //         "attachment; filename=\"{}\"",
                //         path.file_name().unwrap().to_string_lossy()
                //     ),
                // ),
            ],
            body,
        )
            .into_response()
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

async fn handle_options() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (
                "ALLOW",
                "GET,HEAD,PUT,DELETE,MKCOL,COPY,MOVE,PROPFIND,OPTIONS,LOCK,UNLOCK",
            ),
            ("DAV", "1"),
        ],
    )
}

async fn handle_propfind(
    State(state): State<WebDavState>,
    auth: Auth,
    headers: HeaderMap,
    opath: Option<Path<String>>,
    body: Bytes,
) -> Response {
    let path = if let Some(Path(p)) = opath {
        Cow::Owned(state.config.webdav.root.join(p))
    } else {
        Cow::Borrowed(state.config.webdav.root.as_path())
    };
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
            let Some((dn, rel_path)) =
                auth::is_path_related_to_user(&rel_path, auth.username(), &state.config)
            else {
                return None;
            };
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

#[derive(Clone)]
struct WebDavState {
    app_state: StateHandle,
    config: Arc<Config>,
}

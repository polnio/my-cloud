use super::WebDavState;
use crate::config::Config;
use crate::state::State;
use axum::debug_middleware;
use axum::extract::{FromRequestParts, Request};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use headers::authorization::Basic;
use headers::{Authorization, HeaderMapExt as _};
use std::borrow::Cow;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

pub struct Auth(Authorization<Basic>);
impl Auth {
    pub fn username(&self) -> &str {
        self.0.username()
    }
}

impl FromRequestParts<WebDavState> for Auth {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &WebDavState,
    ) -> Result<Self, Self::Rejection> {
        let auth = parts
            .headers
            .typed_get::<Authorization<Basic>>()
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let username = auth.username();
        let password = auth.password();

        {
            let mut app_state = state.app_state.lock().await;
            check_auth(&mut app_state, &state.config, username, password)?;
        }

        let path = super::util::decode_uri(parts.uri.path());
        // TODO: Support non-Unix platforms
        let path = Path::new(OsStr::from_bytes(path.as_ref()));
        if is_path_related_to_user(path, username, &state.config).is_none() {
            if parts.method == axum::http::method::Method::PUT {
                return Err(StatusCode::CONFLICT);
            }
            return Err(StatusCode::NOT_FOUND);
        }

        Ok(Self(auth))
    }
}

fn check_auth(
    app_state: &mut State,
    config: &Config,
    username: &str,
    password: &str,
) -> Result<(), StatusCode> {
    if app_state.creds.get(username).map(|s| s.as_str()) == Some(password) {
        return Ok(());
    }
    let Some(password_file) = config.users.get(username).map(|u| &u.password_file) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    match crate::auth::verify_password(password, password_file) {
        Ok(true) => {
            app_state
                .creds
                .insert(username.to_owned(), password.to_owned());
            Ok(())
        }
        Ok(false) => Err(StatusCode::UNAUTHORIZED),
        // TODO: Handle error
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[debug_middleware]
pub async fn handle_unauthenticated(req: Request, next: Next) -> Response {
    /// https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/WWW-Authenticate
    static AUTH_CHALLENGE: &str = "Basic realm=\"Basic authentication\"";
    let mut response = next.run(req).await;
    if response.status() == StatusCode::UNAUTHORIZED {
        response
            .headers_mut()
            .insert("WWW-Authenticate", HeaderValue::from_static(AUTH_CHALLENGE));
    }
    response
}

pub fn is_path_related_to_user<'a, 'b>(
    path: &'a Path,
    username: &'b str,
    config: &'b Config,
) -> Option<(Option<&'b str>, Cow<'a, Path>)> {
    let mut parts = path.iter().skip(1); // Skip the root directory
    let Some(home) = parts.next() else {
        // This is the root directory
        return Some((None, Cow::Borrowed(path)));
    };
    if home == username {
        let name = config.users[username]
            .display_name
            .as_deref()
            .unwrap_or(username);
        return Some((Some(name), Cow::Borrowed(path)));
    }
    if let Some(name) = config.users[username].display_name.as_deref()
        && name == home
    {
        return Some((
            Some(name),
            Cow::Owned(super::util::replace_path_component(path, 1, username)),
        ));
    }

    let result = config.groups.iter().find_map(|(gid, g)| {
        if gid.as_str() == home {
            Some((true, gid, g))
        } else if g.display_name.as_ref().is_some_and(|n| n.as_str() == home) {
            Some((false, gid, g))
        } else {
            None
        }
    });
    if let Some((is_id, gid, group)) = result
        && group.users.iter().any(|u| u == username)
    {
        let name = group.display_name.as_deref().unwrap_or(gid.as_str());
        if is_id {
            return Some((Some(name), Cow::Borrowed(path)));
        } else {
            return Some((
                Some(name),
                Cow::Owned(super::util::replace_path_component(path, 1, gid)),
            ));
        }
    }
    None
}

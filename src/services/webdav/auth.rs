#[cfg(feature = "caldav")]
use super::CALENDARS_PATH;
use crate::state::{State, StateHandle};
use axum::debug_middleware;
use axum::extract::{FromRequestParts, Request};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use headers::authorization::Basic;
use headers::{Authorization, HeaderMapExt as _};
use itertools::Itertools as _;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

pub struct Auth {
    header: Authorization<Basic>,
    display_name: Option<String>,
    groups: Vec<(String, Option<String>)>,
}
impl Auth {
    pub fn username(&self) -> &str {
        self.header.username()
    }
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
    pub fn groups(&self) -> &[(String, Option<String>)] {
        &self.groups
    }
}

impl FromRequestParts<StateHandle> for Auth {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &StateHandle,
    ) -> Result<Self, Self::Rejection> {
        let auth = parts
            .headers
            .typed_get::<Authorization<Basic>>()
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let username = auth.username();
        let password = auth.password();

        let display_name = check_auth(state, username, password).await?;

        let groups = sqlx::query!(
            "SELECT `group_id`, `display_name` FROM `user_group`
            JOIN `group` ON `id` = `group_id`
            WHERE `user_id` = ?",
            username
        )
        .fetch_all(&*state.database)
        .await
        .unwrap_or_else(|err| {
            tracing::error!("Failed to fetch groups: {err}");
            Default::default()
        });
        let groups = groups
            .into_iter()
            .map(|g| (g.group_id, g.display_name))
            .collect_vec();
        let auth = Self {
            header: auth,
            display_name,
            groups,
        };

        let path = super::util::decode_uri(parts.uri.path());
        // TODO: Support non-Unix platforms
        let path = Path::new(OsStr::from_bytes(path.as_ref()));
        if is_path_related_to_user(path, &auth).is_none() {
            if parts.method == axum::http::method::Method::PUT {
                return Err(StatusCode::CONFLICT);
            }
            if parts.method == "MKCOL" && path.iter().count() < 3 {
                return Err(StatusCode::FORBIDDEN);
            }
            return Err(StatusCode::NOT_FOUND);
        }

        Ok(auth)
    }
}

async fn check_auth(
    state: &State,
    username: &str,
    password: &str,
) -> Result<Option<String>, StatusCode> {
    let mut creds = state.creds.lock().await;
    if let Some((pwd, dn)) = creds.get(username)
        && pwd == password
    {
        return Ok(dn.clone());
    }
    let user = sqlx::query!(
        "SELECT `password_hash`, `display_name` FROM `user` WHERE `id` = ?",
        username
    )
    .fetch_optional(&*state.database)
    .await;
    let user = match user {
        Ok(Some(user)) => user,
        Ok(None) => return Err(StatusCode::UNAUTHORIZED),
        Err(err) => {
            tracing::error!("Failed to fetch user: {err}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    match crate::auth::verify_password(password, &user.password_hash) {
        Ok(true) => {
            creds.insert(
                username.to_owned(),
                (password.to_owned(), user.display_name.clone()),
            );
            Ok(user.display_name)
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

pub enum Principal<'a> {
    Root,
    User(&'a str),
    Group(&'a str),
}
impl<'a> Principal<'a> {
    pub fn display_name<'b>(
        &self,
        user: (&'b str, &'b str),
        groups: impl IntoIterator<Item = (&'b str, &'b str)>,
    ) -> Option<&'b str> {
        match self {
            Self::Root => None,
            Self::User(username) => (*username == user.0).then_some(user.1),
            Self::Group(group_id) => groups
                .into_iter()
                .find_map(|(gid, name)| (*group_id == gid).then_some(name)),
        }
    }
}

pub fn is_path_related_to_user<'a>(path: &Path, auth: &'a Auth) -> Option<(bool, Principal<'a>)> {
    let ret = inner_is_path_related_to_user(path, auth, "/".as_ref());
    #[cfg(feature = "caldav")]
    let ret = ret.or_else(|| inner_is_path_related_to_user(path, auth, CALENDARS_PATH.as_ref()));
    ret
}

fn inner_is_path_related_to_user<'a>(
    path: &Path,
    auth: &'a Auth,
    root: &Path,
) -> Option<(bool, Principal<'a>)> {
    let sub_path = path.strip_prefix(root).ok()?;
    let mut parts = sub_path.iter();
    let Some(home) = parts.next() else {
        // This is the root directory
        return Some((false, Principal::Root));
    };
    let is_principal = parts.next().is_none();
    let username = auth.username();
    if home == username {
        return Some((is_principal, Principal::User(username)));
    }

    auth.groups.iter().find_map(|(gid, _)| {
        (gid.as_str() == home).then_some((is_principal, Principal::Group(gid)))
    })
}

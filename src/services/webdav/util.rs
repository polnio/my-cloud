use itertools::Itertools as _;
use std::{
    borrow::Cow,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

pub fn decode_uri(uri: &str) -> Cow<'_, [u8]> {
    let pcts = uri.chars().positions(|c| c == '%').collect_vec();
    if pcts.is_empty() {
        return Cow::Borrowed(uri.as_bytes());
    }
    let mut decoded = Vec::with_capacity(uri.len() - 2 * pcts.len());
    let mut uri = uri;
    let mut last = None;
    for pct in pcts {
        let old = last.replace(pct);
        let pct = if let Some(old) = old {
            pct - old - 3
        } else {
            pct
        };
        let (before, after) = uri.split_at(pct);
        decoded.extend_from_slice(before.as_bytes());
        let part = &after[1..3];
        if let Ok(byte) = u8::from_str_radix(part, 16) {
            decoded.push(byte);
        } else {
            decoded.push(b'%');
            decoded.extend_from_slice(part.as_bytes());
        }
        uri = &after[3..];
    }
    decoded.extend_from_slice(uri.as_bytes());
    Cow::Owned(decoded)
}

pub fn encode_uri(uri: &'_ str, encode_slashes: bool) -> String {
    let mut encoded = String::with_capacity(uri.len());
    for byte in uri.bytes() {
        if byte.is_ascii_alphanumeric()
            || byte == b'-'
            || byte == b'_'
            || byte == b'.'
            || (!encode_slashes && byte == b'/')
        {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{:02x}", byte));
        }
    }
    encoded
}

pub fn replace_path_component(
    path: impl AsRef<Path>,
    index: usize,
    new_component: impl AsRef<OsStr>,
) -> PathBuf {
    path.as_ref()
        .components()
        .enumerate()
        .map(|(i, component)| {
            if i == index {
                Component::Normal(new_component.as_ref())
            } else {
                component
            }
        })
        .collect()
}

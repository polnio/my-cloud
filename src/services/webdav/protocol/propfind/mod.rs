pub mod request;
pub mod response;

use super::{Error, NS_DAV};
use bitflags::bitflags;
use std::ffi::OsStr;
use std::fs::FileType;
use std::hash::{DefaultHasher, Hasher};
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    creation_date: Option<SystemTime>,
    modified_date: Option<SystemTime>,
    size: Option<u64>,
    kind: Option<Kind>,
    display_name: Option<String>,
    mime: Option<String>,
    etag: Option<String>,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Kind: u8 {
        const COLLECTION = 1 << 0;
        const PRINCIPAL  = 1 << 1;
    }
}

impl Kind {
    fn to_xml(self) -> String {
        self.iter()
            .map(|kind| match kind {
                Self::COLLECTION => "<collection/>",
                Self::PRINCIPAL => "<principal/>",
                _ => unreachable!(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InnerMetadata {
    creation_date: Option<SystemTime>,
    modified_date: Option<SystemTime>,
    size: Option<u64>,
    kind: Option<FileType>,
}
impl From<std::fs::Metadata> for InnerMetadata {
    fn from(value: std::fs::Metadata) -> Self {
        Self {
            creation_date: value.created().ok(),
            modified_date: value.modified().ok(),
            size: value.is_file().then_some(value.len()),
            kind: Some(value.file_type()),
        }
    }
}

impl Metadata {
    #[allow(dead_code)]
    pub fn creation_date(&self) -> Option<SystemTime> {
        self.creation_date
    }
    #[allow(dead_code)]
    pub fn modified_date(&self) -> Option<SystemTime> {
        self.modified_date
    }
    #[allow(dead_code)]
    pub fn size(&self) -> Option<u64> {
        self.size
    }
    #[allow(dead_code)]
    pub fn kind(&self) -> Option<Kind> {
        self.kind
    }
    #[allow(dead_code)]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
    #[allow(dead_code)]
    pub fn mime(&self) -> Option<&str> {
        self.mime.as_deref()
    }
    #[allow(dead_code)]
    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }
    pub fn set_is_root(&mut self) {
        self.display_name = None;
    }
    pub fn set_is_principal(&mut self, display_name: String) {
        if let Some(kind) = &mut self.kind {
            *kind |= Kind::PRINCIPAL;
        }
        self.display_name = Some(display_name);
    }
}

pub fn get_props(path: impl AsRef<Path>) -> Metadata {
    let path = path.as_ref();
    let inner_meta = path.metadata().map(InnerMetadata::from).unwrap_or_default();

    let mut kind = Kind::empty();
    if inner_meta.kind.is_some_and(|f| f.is_dir()) {
        kind |= Kind::COLLECTION;
    }

    let mime = mime_guess::from_path(path);
    let mime = if mime.count() <= 1 {
        mime.first().map(|m| m.to_string())
    } else {
        None
    };

    // TODO: Maybe use a better etag generator?
    let etag = inner_meta.modified_date.map(|modified| {
        let date = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let mut hasher = DefaultHasher::new();
        hasher.write_u128(date);
        hasher.write(path.as_os_str().as_encoded_bytes());
        hasher.finish().to_string()
    });

    Metadata {
        creation_date: inner_meta.creation_date,
        modified_date: inner_meta.modified_date,
        size: inner_meta.size,
        kind: Some(kind),
        display_name: path
            .file_name()
            .and_then(OsStr::to_str)
            .map(ToOwned::to_owned),
        mime,
        etag,
    }
}

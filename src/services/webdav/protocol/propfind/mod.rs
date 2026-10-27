pub mod request;
pub mod response;

use super::{Error, NS_DAV};
#[cfg(feature = "caldav")]
use super::{NS_CALDAV, NS_ICAL};
#[cfg(feature = "caldav")]
use crate::services::webdav::CALENDARS_PATH;
use bitflags::bitflags;
use enumset::{EnumSet, EnumSetType};
use std::ffi::OsStr;
use std::fs::FileType;
use std::hash::{DefaultHasher, Hasher};
use std::path::Path;
use std::time::SystemTime;

type Props = EnumSet<Prop>;

#[derive(Debug, EnumSetType)]
pub enum Prop {
    // RFC 4918: WebDAV
    CreationDate,
    DisplayName,
    GetContentLength,
    GetContentType,
    GetETag,
    GetLastModified,
    ResourceType,
    CurrentUserPrincipal,
    // RFC 3744: ACL
    AlternateURISet,
    PrincipalURL,
    GroupMemberSet,
    GroupMembership,
    Owner,
    Group,
    SupportedPrivilegeSet,
    CurrentUserPrivilegeSet,
    // RFC 4791: CalDAV
    #[cfg(feature = "caldav")]
    CalendarHomeSet,
    #[cfg(feature = "caldav")]
    CalendarSupportedComponents,
    #[cfg(feature = "caldav")]
    CalendarSupportedData,
    #[cfg(feature = "caldav")]
    CalendarTimezone,
    // Apple's ICAL
    #[cfg(feature = "caldav")]
    CalendarColor,
}

impl Prop {
    pub fn from_str(ns: &str, tag: &str) -> Option<Self> {
        match (ns, tag) {
            // RFC 4918: WebDAV
            (NS_DAV, "creationdate") => Some(Self::CreationDate),
            (NS_DAV, "displayname") => Some(Self::DisplayName),
            (NS_DAV, "getcontentlength") => Some(Self::GetContentLength),
            (NS_DAV, "getcontenttype") => Some(Self::GetContentType),
            (NS_DAV, "getetag") => Some(Self::GetETag),
            (NS_DAV, "getlastmodified") => Some(Self::GetLastModified),
            (NS_DAV, "resourcetype") => Some(Self::ResourceType),
            (NS_DAV, "current-user-principal") => Some(Self::CurrentUserPrincipal),
            // RFC 3744: ACL
            (NS_DAV, "alternate-uri-set") => Some(Self::AlternateURISet),
            (NS_DAV, "principal-URL") => Some(Self::PrincipalURL),
            (NS_DAV, "group-member-set") => Some(Self::GroupMemberSet),
            (NS_DAV, "group-membership") => Some(Self::GroupMembership),
            (NS_DAV, "owner") => Some(Self::Owner),
            (NS_DAV, "group") => Some(Self::Group),
            (NS_DAV, "supported-privilege-set") => Some(Self::SupportedPrivilegeSet),
            (NS_DAV, "current-user-privilege-set") => Some(Self::CurrentUserPrivilegeSet),
            // RFC 4791: CalDAV
            #[cfg(feature = "caldav")]
            (NS_CALDAV, "calendar-home-set") => Some(Self::CalendarHomeSet),
            #[cfg(feature = "caldav")]
            (NS_CALDAV, "supported-calendar-component-set") => {
                Some(Self::CalendarSupportedComponents)
            }
            #[cfg(feature = "caldav")]
            (NS_CALDAV, "supported-calendar-data") => Some(Self::CalendarSupportedData),
            #[cfg(feature = "caldav")]
            (NS_CALDAV, "calendar-timezone") => Some(Self::CalendarTimezone),
            // Apple's ICAL
            #[cfg(feature = "caldav")]
            (NS_ICAL, "calendar-color") => Some(Self::CalendarColor),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            // RFC 4918: WebDAV
            Self::CreationDate => "D:creationdate",
            Self::DisplayName => "D:displayname",
            Self::GetContentLength => "D:getcontentlength",
            Self::GetContentType => "D:getcontenttype",
            Self::GetETag => "D:getetag",
            Self::GetLastModified => "D:getlastmodified",
            Self::ResourceType => "D:resourcetype",
            Self::CurrentUserPrincipal => "D:current-user-principal",
            // RFC 3744: ACL
            Self::AlternateURISet => "D:alternate-uri-set",
            Self::PrincipalURL => "D:principal-URL",
            Self::GroupMemberSet => "D:group-member-set",
            Self::GroupMembership => "D:group-membership",
            Self::Owner => "D:owner",
            Self::Group => "D:group",
            Self::SupportedPrivilegeSet => "D:supported-privilege-set",
            Self::CurrentUserPrivilegeSet => "D:current-user-privilege-set",
            // RFC 4791: CalDAV
            #[cfg(feature = "caldav")]
            Self::CalendarHomeSet => "C:calendar-home-set",
            #[cfg(feature = "caldav")]
            Self::CalendarSupportedComponents => "C:supported-calendar-component-set",
            #[cfg(feature = "caldav")]
            Self::CalendarSupportedData => "C:supported-calendar-data",
            #[cfg(feature = "caldav")]
            Self::CalendarTimezone => "C:calendar-timezone",
            // Apple's ICAL
            #[cfg(feature = "caldav")]
            Self::CalendarColor => "ICAL:calendar-color",
        }
    }
    fn default_set() -> Props {
        Self::CreationDate
            | Self::DisplayName
            | Self::GetContentLength
            | Self::GetContentType
            | Self::GetETag
            | Self::GetLastModified
            | Self::ResourceType
            | Self::CurrentUserPrincipal
            | Self::CalendarColor
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    creation_date: Option<SystemTime>,
    modified_date: Option<SystemTime>,
    size: Option<u64>,
    kind: Option<Kind>,
    display_name: Option<String>,
    mime: Option<String>,
    etag: Option<String>,
    current_user_principal: Option<String>,
    owner: Option<String>,
    group: Option<String>,
    #[cfg(feature = "caldav")]
    calendar_home_set: Option<String>,
    #[cfg(feature = "caldav")]
    calendar_color: Option<String>,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Kind: u8 {
        const COLLECTION = 1 << 0;
        const PRINCIPAL  = 1 << 1;
        #[cfg(feature = "caldav")]
        const CALENDAR   = 1 << 2;
    }
}

impl Kind {
    fn to_xml(self) -> String {
        self.iter()
            .map(|kind| match kind {
                Self::COLLECTION => "<D:collection/>",
                Self::PRINCIPAL => "<D:principal/>",
                #[cfg(feature = "caldav")]
                Self::CALENDAR => "<C:calendar/>",
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
    #[allow(dead_code)]
    pub fn is_calendar(&self) -> bool {
        self.kind.is_some_and(|kind| kind.contains(Kind::CALENDAR))
    }
    #[allow(dead_code)]
    pub fn is_principal(&self) -> bool {
        self.kind.is_some_and(|kind| kind.contains(Kind::PRINCIPAL))
    }
    pub fn set_is_root(&mut self) {
        self.display_name = None;
    }
    pub fn set_is_principal(&mut self, username: &str, display_name: String) {
        if let Some(kind) = &mut self.kind {
            *kind |= Kind::PRINCIPAL;
        }
        self.display_name = Some(display_name);
        #[cfg(feature = "caldav")]
        {
            self.calendar_home_set = Some(format!("{CALENDARS_PATH}/{username}/"));
        }
        #[cfg(not(feature = "caldav"))]
        {
            _ = username;
        }
    }
    // #[cfg(not(feature = "caldav"))]
    pub fn set_is_owner(&mut self, username: &str) {
        self.owner = Some(format!("/{username}/"));
    }
    pub fn set_is_group(&mut self, group: &str) {
        self.group = Some(format!("/{group}/"));
    }
    // #[cfg(feature = "caldav")]
    // pub fn set_is_owner(&mut self, username: &str, is_calendar: bool) {
    //     if is_calendar {
    //         self.owner = Some(format!("/calendars/{username}/"));
    //     } else {
    //         self.owner = Some(format!("/{username}/"));
    //     }
    // }
    #[cfg(feature = "caldav")]
    pub fn set_is_calendar(&mut self, display_name: String, color: String) {
        if let Some(kind) = &mut self.kind {
            *kind |= Kind::CALENDAR;
        }
        self.display_name = Some(display_name);
        self.mime = Some("text/calendar".to_owned());
        self.calendar_color = Some(color);
    }
}

pub fn get_props(path: impl AsRef<Path>, username: &str) -> Metadata {
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
        current_user_principal: Some(format!("/{username}/")),
        owner: None,
        group: None,
        #[cfg(feature = "caldav")]
        calendar_home_set: None,
        #[cfg(feature = "caldav")]
        calendar_color: None,
    }
}

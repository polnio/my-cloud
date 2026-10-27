pub mod propfind;

use thiserror::Error;

const NS_DAV: &str = "DAV:";
#[cfg(feature = "caldav")]
const NS_CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
#[cfg(feature = "caldav")]
const NS_ICAL: &str = "http://apple.com/ns/ical/";

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("XML error: {0}")]
    Xml(#[from] xml::reader::Error),
    #[error("Expected start element {name}")]
    ExpectedStartElement { name: &'static str },
    #[error("Expected end element {name}")]
    ExpectedEndElement { name: &'static str },
}

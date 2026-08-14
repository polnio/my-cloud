use crate::services::webdav::util::encode_uri;
use chrono::{DateTime, Utc};
use itertools::{Either, Itertools as _};
use std::path::PathBuf;
use xml::writer::XmlEvent;

pub struct Response {
    items: Vec<Item>,
}

pub struct Item {
    href: String,
    props: Vec<Prop>,
}

pub struct Prop {
    kind: PropKind,
    raw_value: Option<String>,
}

#[derive(Clone, Copy)]
pub enum PropKind {
    CreationDate,
    DisplayName,
    GetContentLength,
    GetContentType,
    GetETag,
    GetLastModified,
    ResourceType,
}

impl PropKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CreationDate => "creationdate",
            Self::DisplayName => "displayname",
            Self::GetContentLength => "getcontentlength",
            Self::GetContentType => "getcontenttype",
            Self::GetETag => "getetag",
            Self::GetLastModified => "getlastmodified",
            Self::ResourceType => "resourcetype",
        }
    }
}

impl Prop {
    fn from_metadata(meta: super::Metadata, props: super::request::Props) -> Vec<Self> {
        let mut out = Vec::with_capacity(props.iter().count());
        if props.contains(super::request::Props::CREATION_DATE) {
            out.push(Prop {
                kind: PropKind::CreationDate,
                raw_value: meta
                    .creation_date
                    .map(|d| DateTime::<Utc>::from(d).to_rfc3339()),
            });
        }
        if props.contains(super::request::Props::DISPLAY_NAME) {
            out.push(Prop {
                kind: PropKind::DisplayName,
                raw_value: meta.display_name,
            });
        }
        if props.contains(super::request::Props::GET_CONTENT_LENGTH) {
            out.push(Prop {
                kind: PropKind::GetContentLength,
                raw_value: meta.size.map(|s| s.to_string()),
            });
        }
        if props.contains(super::request::Props::GET_CONTENT_TYPE) {
            out.push(Prop {
                kind: PropKind::GetContentType,
                raw_value: meta.mime,
            });
        }
        if props.contains(super::request::Props::GET_ETAG) {
            out.push(Prop {
                kind: PropKind::GetETag,
                raw_value: meta.etag,
            });
        }
        if props.contains(super::request::Props::GET_LAST_MODIFIED) {
            out.push(Prop {
                kind: PropKind::GetLastModified,
                raw_value: meta
                    .modified_date
                    .map(|d| DateTime::<Utc>::from(d).to_rfc3339()),
            });
        }
        if props.contains(super::request::Props::RESOURCE_TYPE) {
            out.push(Prop {
                kind: PropKind::ResourceType,
                raw_value: meta.kind.map(|kind| kind.to_xml()),
            });
        }
        out
    }
}

impl Response {
    pub fn to_xml(&self, w: impl std::io::Write) -> xml::writer::Result<()> {
        let mut writer = xml::EventWriter::new(w);
        writer.write(XmlEvent::start_element("multistatus").ns("", "DAV:"))?;
        for item in &self.items {
            let (ok, not_found): (Vec<_>, Vec<_>) =
                item.props.iter().partition_map(|p| match &p.raw_value {
                    Some(v) => Either::Left((p.kind, v.as_str())),
                    None => Either::Right(p.kind),
                });

            writer.write(XmlEvent::start_element("response"))?;

            writer.write(XmlEvent::start_element("href"))?;
            writer.write(XmlEvent::characters(&encode_uri(&item.href, false)))?;
            writer.write(XmlEvent::end_element())?;

            if !ok.is_empty() {
                writer.write(XmlEvent::start_element("propstat"))?;

                writer.write(XmlEvent::start_element("prop"))?;
                for (kind, raw_value) in ok {
                    writer.write(XmlEvent::start_element(kind.as_str()))?;
                    writer.write(XmlEvent::raw_characters(raw_value))?;
                    writer.write(XmlEvent::end_element())?;
                }
                writer.write(XmlEvent::end_element())?;

                writer.write(XmlEvent::start_element("status"))?;
                writer.write(XmlEvent::characters("HTTP/1.1 200 OK"))?;
                writer.write(XmlEvent::end_element())?;

                writer.write(XmlEvent::end_element())?;
            }

            if !not_found.is_empty() {
                writer.write(XmlEvent::start_element("propstat"))?;

                writer.write(XmlEvent::start_element("prop"))?;
                for kind in not_found {
                    writer.write(XmlEvent::start_element(kind.as_str()))?;
                    writer.write(XmlEvent::end_element())?;
                }
                writer.write(XmlEvent::end_element())?;

                writer.write(XmlEvent::start_element("status"))?;
                writer.write(XmlEvent::characters("HTTP/1.1 404 Not Found"))?;
                writer.write(XmlEvent::end_element())?;

                writer.write(XmlEvent::end_element())?;
            }

            writer.write(XmlEvent::end_element())?;
        }
        writer.write(XmlEvent::end_element())?;
        Ok(())
    }

    pub fn from_metadata(
        items: Vec<(PathBuf, super::Metadata)>,
        props: super::request::Props,
    ) -> Self {
        Response {
            items: items
                .into_iter()
                .map(|(path, meta)| Item {
                    href: path.to_string_lossy().into_owned(),
                    props: Prop::from_metadata(meta, props),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_propfind_response() {
        let response = Response {
            items: vec![Item {
                href: "/".to_owned(),
                props: vec![
                    Prop {
                        kind: PropKind::ResourceType,
                        raw_value: Some("<principal/><collection/>".to_owned()),
                    },
                    Prop {
                        kind: PropKind::GetContentLength,
                        raw_value: None,
                    },
                ],
            }],
        };
        const EXPECTED: &[u8; 326] = br#"<?xml version="1.0" encoding="UTF-8"?><multistatus xmlns="DAV:"><response><href>/</href><propstat><prop><resourcetype><principal/><collection/></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><getcontentlength /></prop><status>HTTP/1.1 404 Not Found</status></propstat></response></multistatus>"#;
        let mut output = Vec::new();
        response.to_xml(&mut output).unwrap();
        assert_eq!(output, EXPECTED);
    }

    // fn
}

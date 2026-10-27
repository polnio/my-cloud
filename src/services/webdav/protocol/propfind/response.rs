#[cfg(feature = "caldav")]
use crate::services::webdav::protocol::{NS_CALDAV, NS_ICAL};
use crate::services::webdav::{protocol::NS_DAV, util::encode_uri};
use chrono::{DateTime, Utc};
use itertools::{Either, Itertools as _};
use std::borrow::Cow;
use std::path::Path;
use xml::writer::XmlEvent;

pub struct Response<'a, 'b> {
    items: Vec<Item<'a, 'b>>,
    is_props_requested: bool,
}

pub struct Item<'a, 'b> {
    href: Cow<'b, str>,
    props: Vec<Prop<'a>>,
}

pub struct Prop<'a> {
    kind: super::Prop,
    raw_value: Option<Cow<'a, str>>,
}

impl<'a> Prop<'a> {
    fn from_metadata(meta: &'a super::Metadata, props: super::Props) -> Vec<Self> {
        #[cfg(feature = "caldav")]
        let is_calendar = meta.is_calendar();
        props
            .iter()
            .map(|prop| {
                let raw_value = match prop {
                    // RFC 4918: WebDAV
                    super::Prop::CreationDate => meta
                        .creation_date
                        .map(|d| DateTime::<Utc>::from(d).to_rfc3339().into()),
                    super::Prop::DisplayName => meta.display_name.as_deref().map(Cow::Borrowed),
                    super::Prop::GetContentLength => meta.size.map(|s| s.to_string().into()),
                    super::Prop::GetContentType => meta.mime.as_deref().map(Cow::Borrowed),
                    super::Prop::GetETag => meta.etag.as_deref().map(Cow::Borrowed),
                    super::Prop::GetLastModified => meta
                        .modified_date
                        .map(|d| DateTime::<Utc>::from(d).to_rfc3339().into()),
                    super::Prop::ResourceType => meta.kind.map(|kind| kind.to_xml().into()),
                    super::Prop::CurrentUserPrincipal => {
                        Some(meta.current_user_principal.as_deref().map_or(Cow::Borrowed(""), |href| {
                            format!("<D:href>{href}</D:href>").into()
                        }))
                    }
                    // RFC 3744: ACL
                    super::Prop::AlternateURISet => None,
                    super::Prop::PrincipalURL => None,
                    super::Prop::GroupMemberSet => None,
                    super::Prop::GroupMembership => None,
                    super::Prop::Owner => {
                        Some(meta.owner.as_deref().map_or(Cow::Borrowed(""), |o| {
                            format!("<D:href>{o}</D:href>").into()
                        }))
                    }
                    super::Prop::Group => {
                        Some(meta.group.as_deref().map_or(Cow::Borrowed(""), |o| {
                            format!("<D:href>{o}</D:href>").into()
                        }))
                    }
                    super::Prop::SupportedPrivilegeSet => None,
                    super::Prop::CurrentUserPrivilegeSet => Some(Cow::Borrowed(
                        r#"<D:privilege><D:all/></D:privilege><D:privilege><D:all/></D:privilege><D:privilege><D:write/></D:privilege><D:privilege><D:write-properties/></D:privilege><D:privilege><D:write-content/></D:privilege>"#,
                    )),
                    // RFC 7951: CalDAV
                    #[cfg(feature = "caldav")]
                    super::Prop::CalendarHomeSet => {
                        Some(meta.calendar_home_set.as_deref().map_or(Cow::Borrowed(""), |href| {
                            format!("<D:href>{href}</D:href>").into()
                        }))
                    }
                    #[cfg(feature = "caldav")]
                    super::Prop::CalendarSupportedComponents => {
                        is_calendar.then_some(Cow::Borrowed(r#"<C:comp name="VEVENT"/>"#))
                    }
                    #[cfg(feature = "caldav")]
                    super::Prop::CalendarSupportedData => is_calendar.then_some(Cow::Borrowed(
                        r#"<C:calendar-data content-type="text/calendar" version="2.0"/>"#,
                    )),
                    #[cfg(feature = "caldav")]
                    super::Prop::CalendarTimezone => None,
                    // Apple's ICAL
                    #[cfg(feature = "caldav")]
                    super::Prop::CalendarColor => meta
                        .calendar_color
                        .as_ref()
                        .map(|c| format!("{}ff", c).into()),
                };
                Prop {
                    kind: prop,
                    raw_value,
                }
            })
            .collect()
    }
}

impl<'a, 'b> Response<'a, 'b> {
    pub fn to_xml(&self, w: impl std::io::Write) -> xml::writer::Result<()> {
        let mut writer = xml::EventWriter::new(w);
        writer.write(
            XmlEvent::start_element("multistatus")
                .ns("D", NS_DAV)
                .ns("C", NS_CALDAV)
                .ns("ICAL", NS_ICAL),
        )?;
        for item in &self.items {
            let (ok, not_found): (Vec<_>, Vec<_>) =
                item.props.iter().partition_map(|p| match &p.raw_value {
                    Some(v) => Either::Left((p.kind, v.as_ref())),
                    None => Either::Right(p.kind),
                });

            writer.write(XmlEvent::start_element("D:response"))?;

            writer.write(XmlEvent::start_element("D:href"))?;
            writer.write(XmlEvent::characters(&encode_uri(&item.href, false)))?;
            writer.write(XmlEvent::end_element())?;

            if !ok.is_empty() {
                writer.write(XmlEvent::start_element("D:propstat"))?;

                writer.write(XmlEvent::start_element("D:prop"))?;
                for (kind, raw_value) in ok {
                    writer.write(XmlEvent::start_element(kind.as_str()))?;
                    writer.write(XmlEvent::raw_characters(raw_value))?;
                    writer.write(XmlEvent::end_element())?;
                }
                writer.write(XmlEvent::end_element())?;

                writer.write(XmlEvent::start_element("D:status"))?;
                writer.write(XmlEvent::characters("HTTP/1.1 200 OK"))?;
                writer.write(XmlEvent::end_element())?;

                writer.write(XmlEvent::end_element())?;
            }

            if self.is_props_requested && !not_found.is_empty() {
                writer.write(XmlEvent::start_element("D:propstat"))?;

                writer.write(XmlEvent::start_element("D:prop"))?;
                for kind in not_found {
                    writer.write(XmlEvent::start_element(kind.as_str()))?;
                    writer.write(XmlEvent::end_element())?;
                }
                writer.write(XmlEvent::end_element())?;

                writer.write(XmlEvent::start_element("D:status"))?;
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
        // items: Vec<(PathBuf, super::Metadata)>,
        items: impl IntoIterator<Item = (&'b Path, &'a super::Metadata)>,
        req: &super::request::Request,
    ) -> Self {
        Self {
            items: items
                .into_iter()
                .map(|(path, meta)| Item {
                    href: path.to_string_lossy(),
                    props: Prop::from_metadata(meta, req.props),
                })
                .collect(),
            is_props_requested: req.is_props_requested,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::webdav::protocol::propfind::Prop as PropKind;

    #[test]
    fn test_propfind_response() {
        let response = Response {
            items: vec![Item {
                href: "/".into(),
                props: vec![
                    Prop {
                        kind: PropKind::ResourceType,
                        raw_value: Some("<principal/><collection/>".into()),
                    },
                    Prop {
                        kind: PropKind::GetContentLength,
                        raw_value: None,
                    },
                ],
            }],
            is_props_requested: true,
        };
        const EXPECTED: &[u8; 326] = br#"<?xml version="1.0" encoding="UTF-8"?><multistatus xmlns="DAV:"><response><href>/</href><propstat><prop><resourcetype><principal/><collection/></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><getcontentlength /></prop><status>HTTP/1.1 404 Not Found</status></propstat></response></multistatus>"#;
        let mut output = Vec::new();
        response.to_xml(&mut output).unwrap();
        assert_eq!(output, EXPECTED);
    }

    // fn
}

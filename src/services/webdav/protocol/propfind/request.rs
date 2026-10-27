use super::{Error, NS_DAV, Prop, Props};
use std::io::Read;
use xml::{EventReader, reader::XmlEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub props: Props,
    pub is_props_requested: bool,
}
impl Default for Request {
    fn default() -> Self {
        let props = Prop::default_set();
        let is_props_requested = false;
        Self {
            props,
            is_props_requested,
        }
    }
}

#[derive(Default)]
enum ParseState {
    #[default]
    Begin,
    StartDocument,
    StartPropFind,
    StartProp,
    EndProp,
    EndPropFind,
    EndDocument,
}

impl Request {
    pub fn parse(r: impl Read) -> Result<Self, Vec<Error>> {
        let parser = EventReader::new(r);
        let mut errors = Vec::new();
        let mut state = ParseState::default();
        let mut props = Props::empty();
        for event in parser {
            let Some(event) = event.push_error(&mut errors) else {
                continue;
            };
            if matches!(event, XmlEvent::Whitespace(_)) {
                continue;
            }
            match state {
                ParseState::Begin => {
                    expect_start_document(&event).push_error(&mut errors);
                    state = ParseState::StartDocument;
                }
                ParseState::StartDocument => {
                    expect_start_element(&event, "propfind").push_error(&mut errors);
                    state = ParseState::StartPropFind;
                }
                ParseState::StartPropFind => {
                    expect_start_element(&event, "prop").push_error(&mut errors);
                    state = ParseState::StartProp;
                }
                ParseState::StartProp => {
                    if expect_end_element(&event, "prop").is_ok() {
                        state = ParseState::EndProp;
                        continue;
                    }
                    let XmlEvent::StartElement { name, .. } = event else {
                        // TODO: handle error
                        continue;
                    };
                    let Some(ns) = name.namespace.as_deref() else {
                        // todo: handle error
                        continue;
                    };
                    if let Some(prop) = super::Prop::from_str(ns, &name.local_name) {
                        props.insert(prop);
                    }
                }
                ParseState::EndProp => {
                    expect_end_element(&event, "propfind").push_error(&mut errors);
                    state = ParseState::EndPropFind;
                }
                ParseState::EndPropFind => {
                    expect_end_document(&event).push_error(&mut errors);
                    state = ParseState::EndDocument;
                }
                ParseState::EndDocument => {
                    // TODO: this should be an error
                }
            }
        }
        if errors.is_empty() {
            Ok(Self {
                props,
                is_props_requested: true,
            })
        } else {
            Err(errors)
        }
    }
}

fn expect_start_document(event: &XmlEvent) -> Result<(), Error> {
    match event {
        XmlEvent::StartDocument { .. } => Ok(()),
        _ => Err(Error::ExpectedStartElement { name: "Document" }),
    }
}

fn expect_end_document(event: &XmlEvent) -> Result<(), Error> {
    match event {
        XmlEvent::EndDocument => Ok(()),
        _ => Err(Error::ExpectedEndElement { name: "Document" }),
    }
}

fn expect_start_element(event: &XmlEvent, tag_name: &'static str) -> Result<(), Error> {
    match event {
        XmlEvent::StartElement { name, .. }
            if name.local_name == tag_name && name.namespace.as_deref() == Some(NS_DAV) =>
        {
            Ok(())
        }
        _ => Err(Error::ExpectedStartElement { name: tag_name }),
    }
}

fn expect_end_element(event: &XmlEvent, tag_name: &'static str) -> Result<(), Error> {
    match event {
        XmlEvent::EndElement { name, .. }
            if name.local_name == tag_name && name.namespace.as_deref() == Some(NS_DAV) =>
        {
            Ok(())
        }
        _ => Err(Error::ExpectedEndElement { name: tag_name }),
    }
}

trait PushError {
    type Item;
    type Error;
    fn push_error<E>(self, errs: &mut Vec<E>) -> Option<Self::Item>
    where
        E: From<Self::Error>;
}
impl<T, E> PushError for Result<T, E> {
    type Item = T;
    type Error = E;
    fn push_error<U>(self, errs: &mut Vec<U>) -> Option<Self::Item>
    where
        U: From<Self::Error>,
    {
        match self {
            Ok(v) => Some(v),
            Err(err) => {
                errs.push(err.into());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_propfind_request() {
        const XML: &str = r#"
            <?xml version="1.0" encoding="utf-8" ?>
            <D:propfind xmlns:D="DAV:">
              <D:prop>
                    <D:creationdate/>
                    <D:displayname/>
                    <D:getcontentlength/>
                    <D:getcontenttype/>
                    <D:getetag/>
                    <D:getlastmodified/>
                    <D:resourcetype/>
                </D:prop>
            </D:propfind>
        "#;
        let expected = Request {
            props: Props::all(),
            is_props_requested: true,
        };
        let cursor = Cursor::new(XML.trim());
        let result = Request::parse(cursor);
        assert_eq!(result, Ok(expected));
    }
}

//! A small XML element tree on top of quick-xml, used for the XML formats
//! Folitecta reads and writes (XFDF comments and form data, XMP metadata).

use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::error::{Error, Result};

/// An XML element with its attributes and children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct XmlElement {
    /// Qualified name as written in the document (`prefix:local`).
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
}

/// A child of an element.
#[derive(Debug, Clone, PartialEq)]
pub enum XmlNode {
    Element(XmlElement),
    Text(String),
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Escape character data.
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

/// Escape an attribute value (line breaks and tabs survive normalization).
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
    out
}

impl XmlElement {
    pub fn new(name: &str) -> Self {
        XmlElement {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Local part of the name (after any namespace prefix).
    pub fn local_name(&self) -> &str {
        local(&self.name)
    }

    /// Whether the local name matches (ignoring case).
    pub fn is(&self, local_name: &str) -> bool {
        self.local_name().eq_ignore_ascii_case(local_name)
    }

    /// Attribute value by local name (ignoring case and namespace prefix).
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| local(k).eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Set (or replace) an attribute.
    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.attrs.iter_mut().find(|(k, _)| k == name) {
            Some(slot) => slot.1 = value,
            None => self.attrs.push((name.to_string(), value)),
        }
    }

    /// Builder form of [`XmlElement::set`].
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.set(name, value);
        self
    }

    /// Builder: append text.
    pub fn with_text(mut self, text: &str) -> Self {
        self.push_text(text);
        self
    }

    pub fn push(&mut self, child: XmlElement) {
        self.children.push(XmlNode::Element(child));
    }

    pub fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.children.last_mut() {
            Some(XmlNode::Text(t)) => t.push_str(text),
            _ => self.children.push(XmlNode::Text(text.to_string())),
        }
    }

    /// Child elements.
    pub fn elements(&self) -> impl Iterator<Item = &XmlElement> {
        self.children.iter().filter_map(|c| match c {
            XmlNode::Element(e) => Some(e),
            XmlNode::Text(_) => None,
        })
    }

    /// First child element with a local name.
    pub fn child(&self, name: &str) -> Option<&XmlElement> {
        self.elements().find(|e| e.is(name))
    }

    /// Child elements with a local name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlElement> + 'a {
        self.elements().filter(move |e| e.is(name))
    }

    /// First descendant (depth first, including `self`) with a local name.
    pub fn find(&self, name: &str) -> Option<&XmlElement> {
        if self.is(name) {
            return Some(self);
        }
        self.elements().find_map(|e| e.find(name))
    }

    /// Concatenated text of the element and its descendants.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        for c in &self.children {
            match c {
                XmlNode::Text(t) => out.push_str(t),
                XmlNode::Element(e) => e.collect_text(out),
            }
        }
    }

    /// Serialize the element. With `indent`, elements without text content are
    /// laid out one per line.
    pub fn write(&self, out: &mut String, indent: Option<usize>) {
        let pad = |out: &mut String, n: usize| {
            for _ in 0..n {
                out.push_str("  ");
            }
        };
        out.push('<');
        out.push_str(&self.name);
        for (k, v) in &self.attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            out.push_str(&escape_attr(v));
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        let has_text = self.children.iter().any(|c| matches!(c, XmlNode::Text(_)));
        let child_indent = match indent {
            Some(n) if !has_text => Some(n + 1),
            _ => None,
        };
        for c in &self.children {
            match c {
                XmlNode::Text(t) => out.push_str(&escape_text(t)),
                XmlNode::Element(e) => {
                    if let Some(n) = child_indent {
                        out.push('\n');
                        pad(out, n);
                    }
                    e.write(out, child_indent);
                }
            }
        }
        if let (Some(n), Some(_)) = (indent, child_indent) {
            out.push('\n');
            pad(out, n);
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }

    /// Compact serialization.
    pub fn to_xml(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, None);
        s
    }

    /// Indented serialization.
    pub fn to_pretty_xml(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, Some(0));
        s
    }
}

fn bad(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("invalid XML: {e}"))
}

fn element_of(reader: &Reader<&[u8]>, s: &BytesStart) -> Result<XmlElement> {
    let name = String::from_utf8_lossy(s.name().as_ref()).to_string();
    let mut attrs = Vec::new();
    for a in s.attributes().with_checks(false) {
        let a = a.map_err(bad)?;
        let key = String::from_utf8_lossy(a.key.as_ref()).to_string();
        let value = a
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
            .map_err(bad)?
            .to_string();
        attrs.push((key, value));
    }
    Ok(XmlElement {
        name,
        attrs,
        children: Vec::new(),
    })
}

fn attach(stack: &mut [XmlElement], root: &mut Option<XmlElement>, el: XmlElement) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(XmlNode::Element(el)),
        None => {
            if root.is_none() {
                *root = Some(el);
            }
        }
    }
}

/// Parse an XML document and return its root element.
pub fn parse(xml: &str) -> Result<XmlElement> {
    let xml = xml.trim_start_matches('\u{feff}');
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut root: Option<XmlElement> = None;
    loop {
        match reader.read_event().map_err(bad)? {
            Event::Start(s) => {
                let el = element_of(&reader, &s)?;
                stack.push(el);
            }
            Event::Empty(s) => {
                let el = element_of(&reader, &s)?;
                attach(&mut stack, &mut root, el);
            }
            Event::End(_) => {
                let el = stack.pop().ok_or_else(|| bad("unexpected end tag"))?;
                attach(&mut stack, &mut root, el);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.push_text(&t.xml10_content().map_err(bad)?);
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    top.push_text(&String::from_utf8_lossy(&c.into_inner()));
                }
            }
            Event::GeneralRef(r) => {
                let text = match r.resolve_char_ref().map_err(bad)? {
                    Some(ch) => ch.to_string(),
                    None => {
                        let name = r.decode().map_err(bad)?;
                        match resolve_xml_entity(&name) {
                            Some(v) => v.to_string(),
                            None => format!("&{name};"),
                        }
                    }
                };
                if let Some(top) = stack.last_mut() {
                    top.push_text(&text);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(bad("an element is not closed"));
    }
    root.ok_or_else(|| bad("no root element"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_attributes_text_and_entities() {
        let doc = parse(
            "\u{feff}<?xml version=\"1.0\"?><a x='1' p:y=\"a&amp;b&#10;c\"><b>t &lt;1&gt; &#233;</b><c/><![CDATA[<raw>]]></a>",
        )
        .unwrap();
        assert!(doc.is("a"));
        assert_eq!(doc.attr("x"), Some("1"));
        assert_eq!(doc.attr("y"), Some("a&b\nc"));
        assert_eq!(doc.child("b").unwrap().text(), "t <1> é");
        assert!(doc.child("c").is_some());
        assert_eq!(doc.text(), "t <1> é<raw>");
    }

    #[test]
    fn round_trips_through_serialization() {
        let mut root = XmlElement::new("root").with("k", "a\"b\nc");
        root.push(XmlElement::new("t").with_text("x & y\r\nz"));
        root.push(XmlElement::new("empty"));
        let xml = root.to_pretty_xml();
        let back = parse(&xml).unwrap();
        assert_eq!(back.attr("k"), Some("a\"b\nc"));
        assert_eq!(back.child("t").unwrap().text(), "x & y\r\nz");
        assert!(back.child("empty").is_some());
    }

    #[test]
    fn rejects_malformed_documents() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
        assert!(parse("just text").is_err());
    }
}

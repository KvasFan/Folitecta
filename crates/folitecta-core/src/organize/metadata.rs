//! XMP metadata: the packet of the document, its properties as name and
//! value, and a packet made from the document information when there is none.

use lopdf::Object;

use crate::document::Document;
use crate::util::deref;
use crate::xml::{XmlElement, XmlNode, escape_text};

/// Text of an element and of its descendants.
fn text_of(e: &XmlElement) -> String {
    let mut out = String::new();
    for c in &e.children {
        match c {
            XmlNode::Text(t) => out.push_str(t),
            XmlNode::Element(child) => out.push_str(&text_of(child)),
        }
    }
    out
}

/// Value of a property element: its text, or the items of an `rdf:Alt`,
/// `rdf:Seq` or `rdf:Bag` joined with `; `.
fn property_value(e: &XmlElement) -> String {
    let items: Vec<String> = e
        .children
        .iter()
        .filter_map(|c| match c {
            XmlNode::Element(x) if x.is("Alt") || x.is("Seq") || x.is("Bag") => Some(x),
            _ => None,
        })
        .flat_map(|list| {
            list.children.iter().filter_map(|c| match c {
                XmlNode::Element(li) if li.is("li") => Some(text_of(li).trim().to_string()),
                _ => None,
            })
        })
        .collect();
    if items.is_empty() {
        // Nested structures: the attributes of the element, then its text.
        let attrs: Vec<String> = e
            .attrs
            .iter()
            .filter(|(k, _)| !k.starts_with("xmlns") && !k.starts_with("rdf:"))
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        let text = text_of(e).split_whitespace().collect::<Vec<_>>().join(" ");
        if attrs.is_empty() {
            text
        } else if text.is_empty() {
            attrs.join(", ")
        } else {
            format!("{} {}", attrs.join(", "), text)
        }
    } else {
        items.join("; ")
    }
}

fn collect(e: &XmlElement, out: &mut Vec<(String, String)>) {
    if e.is("Description") {
        for (k, v) in &e.attrs {
            if !k.starts_with("xmlns") && !k.starts_with("rdf:") && k != "about" {
                out.push((k.clone(), v.clone()));
            }
        }
        for c in &e.children {
            if let XmlNode::Element(p) = c {
                out.push((p.name.clone(), property_value(p)));
            }
        }
        return;
    }
    for c in &e.children {
        if let XmlNode::Element(x) = c {
            collect(x, out);
        }
    }
}

impl Document {
    /// The XMP packet of the document (`/Metadata` of the catalog).
    pub fn xmp_packet(&self) -> Option<String> {
        let doc = &self.inner;
        let m = doc.catalog().ok()?.get(b"Metadata").ok()?;
        let s = deref(doc, m).as_stream().ok()?;
        let data = if s.dict.has(b"Filter") {
            s.decompressed_content().ok()?
        } else {
            s.content.clone()
        };
        let text = String::from_utf8_lossy(&data).to_string();
        (!text.trim().is_empty()).then_some(text)
    }

    /// Properties of the XMP packet: qualified name and value, in order.
    pub fn xmp_properties(&self) -> Vec<(String, String)> {
        let Some(packet) = self.xmp_packet() else {
            return Vec::new();
        };
        // The packet wrapper (<?xpacket ...?>) is a processing instruction.
        let start = packet
            .find("<x:xmpmeta")
            .or_else(|| packet.find("<rdf:RDF"))
            .unwrap_or(0);
        let body = &packet[start..];
        let end = body
            .rfind("</x:xmpmeta>")
            .map(|e| e + "</x:xmpmeta>".len())
            .or_else(|| body.rfind("</rdf:RDF>").map(|e| e + "</rdf:RDF>".len()))
            .unwrap_or(body.len());
        let Ok(root) = crate::xml::parse(&body[..end]) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        collect(&root, &mut out);
        out
    }

    /// An XMP packet: the document's own, or one made from its information.
    pub fn xmp_or_generated(&self) -> String {
        self.xmp_packet().unwrap_or_else(|| self.generated_xmp())
    }

    /// An XMP packet made from the document information.
    pub fn generated_xmp(&self) -> String {
        let info = self.info();
        let lang = {
            let l = self.language();
            if l.is_empty() { "x-default".to_string() } else { l }
        };
        let mut props = String::new();
        let alt = |name: &str, v: &str| {
            format!(
                "<{name}><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></{name}>",
                escape_text(v)
            )
        };
        if !info.title.is_empty() {
            props.push_str(&alt("dc:title", &info.title));
        }
        if !info.author.is_empty() {
            props.push_str(&format!(
                "<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>",
                escape_text(&info.author)
            ));
        }
        if !info.subject.is_empty() {
            props.push_str(&alt("dc:description", &info.subject));
        }
        if !info.keywords.is_empty() {
            props.push_str(&format!(
                "<pdf:Keywords>{}</pdf:Keywords>",
                escape_text(&info.keywords)
            ));
        }
        if !info.producer.is_empty() {
            props.push_str(&format!(
                "<pdf:Producer>{}</pdf:Producer>",
                escape_text(&info.producer)
            ));
        }
        if !info.creator.is_empty() {
            props.push_str(&format!(
                "<xmp:CreatorTool>{}</xmp:CreatorTool>",
                escape_text(&info.creator)
            ));
        }
        props.push_str("<dc:format>application/pdf</dc:format>");
        format!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
             <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
             <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
             <rdf:Description rdf:about=\"\" xml:lang=\"{}\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">{props}</rdf:Description>\n\
             </rdf:RDF>\n\
             </x:xmpmeta>\n\
             <?xpacket end=\"w\"?>",
            escape_text(&lang)
        )
    }

    /// Write an XMP packet as the document's metadata.
    pub fn set_xmp_packet(&mut self, packet: &str) -> crate::Result<()> {
        self.begin_edit();
        let stream = lopdf::Stream::new(
            lopdf::dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            packet.as_bytes().to_vec(),
        );
        let id = self.inner.add_object(stream);
        let cat = self.inner.trailer.get(b"Root")?.as_reference()?;
        self.inner
            .get_dictionary_mut(cat)?
            .set("Metadata", Object::Reference(id));
        self.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_of_a_packet() {
        let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
        assert!(doc.xmp_packet().is_none());
        doc.set_info("Report & notes", "Ann Smith", "Quarter", "a, b")
            .unwrap();
        let packet = doc.generated_xmp();
        doc.set_xmp_packet(&packet).unwrap();
        let props = doc.xmp_properties();
        let get = |k: &str| props.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("dc:title").as_deref(), Some("Report & notes"));
        assert_eq!(get("dc:creator").as_deref(), Some("Ann Smith"));
        assert_eq!(get("pdf:Keywords").as_deref(), Some("a, b"));
        assert_eq!(get("dc:format").as_deref(), Some("application/pdf"));
        assert!(get("xml:lang").is_some());
        assert_eq!(doc.xmp_or_generated(), packet);
    }
}
